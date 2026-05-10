// SPDX-License-Identifier: (AGPL-3.0-only AND (MIT OR Apache-2.0))

//! Concurrent rx + tx from independent tasks, the headline feature of
//! the (Stack, Runner) split.
//!
//! Layout:
//!  - `main` does setup, allocates 'static resources via `StaticCell`,
//!    splits them into a `(Stack, Runner)` pair, spawns three tasks, then
//!    returns.
//!  - `runner_task` drives the radio Runner; it owns the radio.
//!  - `rx_task` awaits incoming packets via `stack.recv()` in a tight loop
//!    -- no `with_timeout` ceremony, no manual rx/tx interleaving.
//!  - `tx_task` broadcasts a heartbeat packet every 5 s.
//!
//! Both `rx_task` and `tx_task` run concurrently on the same Stack handle
//! (`Stack<'static>` is `Copy`); the Runner internally arbitrates the
//! half-duplex radio.
//!
//! Single-board operation: with no peer, `tx_task` broadcasts succeed
//! immediately (broadcasts aren't ACK'd) and `rx_task` sits idle. Pair with
//! a second Pico flashed with this same bin (different `own_address`) to
//! see each board log the other's heartbeats while still emitting its own.

#![no_std]
#![no_main]

use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, SPI0, USB};
use embassy_rp::spi::{Async, Spi};
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_rp::{bind_interrupts, dma, spi};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Delay, Duration, Timer};
use rfm69_async::{config, Address, Flags, MacTiming, Rfm69, Runner, Stack, StackResources};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
});

// Concrete radio type chosen by this bin's wiring. Spelling it out lets us
// declare per-bin embassy task signatures below; embassy tasks aren't
// generic, so each bin needs its own typedef even when the wiring is
// identical to a sibling bin.
type RadioSpi = SpiDevice<'static, NoopRawMutex, Spi<'static, SPI0, Async>, Output<'static>>;
// `Rfm69`'s third type parameter is the DIO0 pin type itself; the constructor
// takes an `Option<DIO0>` so DIO0-absent is a runtime choice, not a type-level
// one. Spelling that out here.
type Radio = Rfm69<RadioSpi, Output<'static>, Input<'static>, Delay>;

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Debug, driver);
}

#[embassy_executor::task]
async fn runner_task(mut runner: Runner<'static, Radio>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn rx_task(stack: Stack<'static>) -> ! {
    loop {
        let packet = stack.recv().await;
        log::info!(
            "rx_task: from {:?} flags {:?} payload {:?} rssi {:?}",
            packet.src,
            packet.flags,
            packet.data.as_slice(),
            packet.rssi,
        );
    }
}

#[embassy_executor::task]
async fn tx_task(stack: Stack<'static>) -> ! {
    let mut counter: u8 = 0;
    loop {
        // Broadcast heartbeat. No ACK requested -- broadcasts can't be ACK'd.
        let payload = [b'H', b'B', counter];
        log::info!("tx_task: broadcasting heartbeat #{}", counter);
        match stack.send(Address::Broadcast, Flags::None, &payload).await {
            Ok(()) => log::info!("tx_task: send ok"),
            Err(e) => log::error!("tx_task: send error {:?}", e),
        }
        counter = counter.wrapping_add(1);
        Timer::after(Duration::from_secs(5)).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver).unwrap());

    // Wait for USB CDC to enumerate so the host doesn't miss the boot logs.
    Timer::after(Duration::from_secs(4)).await;
    log::error!("--- Starting concurrent demo ---");

    // SPI0 pins of RPi Pico
    let miso = p.PIN_16;
    let mosi = p.PIN_19;
    let clk = p.PIN_18;
    let rfm_cs = p.PIN_17;
    let rfm_reset = p.PIN_11;

    let mut rfm_config = spi::Config::default();
    rfm_config.frequency = 10_000_000;

    let spi = spi::Spi::new(p.SPI0, clk, mosi, miso, p.DMA_CH0, p.DMA_CH1, Irqs, rfm_config);

    // The SPI bus must be 'static so SpiDevice<'static, ...> -- and therefore
    // the radio, Runner, and Stack -- can move into a spawned task.
    static SPI_BUS: StaticCell<Mutex<NoopRawMutex, Spi<'static, SPI0, Async>>> = StaticCell::new();
    let spi_bus = SPI_BUS.init(Mutex::new(spi));

    let cs = Output::new(rfm_cs, Level::Low);
    let reset = Output::new(rfm_reset, Level::High);
    let dio0 = Some(Input::new(p.PIN_15, Pull::None));
    let rfm_spi = SpiDevice::new(spi_bus, cs);

    let rfm = match config::my_defaults(Rfm69::new(rfm_spi, reset, dio0, Delay), 42, 868_480_000).await {
        Ok(r) => r,
        Err(e) => {
            log::error!("Radio init error: {:?}", e);
            Timer::after(Duration::from_millis(5000)).await;
            panic!();
        }
    };

    // Address 100 picked so this bin doesn't collide with echo_client (84) or
    // echo_server / rfm69 (42); change it on each board if pairing two of
    // these.
    let own_address = Address::Unicast(100);
    static RESOURCES: StaticCell<StackResources> = StaticCell::new();
    let resources = RESOURCES.init(StackResources::new());
    let (stack, runner) = Stack::new(rfm, own_address, resources, MacTiming::default());

    log::info!("Own address: {:?}", own_address);
    spawner.spawn(runner_task(runner).unwrap());
    spawner.spawn(rx_task(stack).unwrap());
    spawner.spawn(tx_task(stack).unwrap());
    // main returns; the spawned tasks continue running.
}
