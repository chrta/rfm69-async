// SPDX-License-Identifier: (AGPL-3.0-only AND (MIT OR Apache-2.0))

//! HIL test bin: send-with-ACK client.
//!
//! Sends [`N_PACKETS`] short payloads to `Address::Unicast(1)` with
//! `Flags::Ack(3)`, then emits a machine-readable summary line that the
//! host-side `hil-runner` matches:
//!
//! ```text
//! HIL: ready
//! HIL: send 1 ok
//! ...
//! HIL: PASS sent=100 ack_timeouts=0
//! ```
//!
//! The payload is a little-endian 16-bit counter `[lo, hi]` so the paired
//! `hil_recv_server` can dedup duplicates resulting from ACK loss + retry.
//! On unrecoverable error the bin emits `HIL: FAIL ...` and idles instead
//! of panicking — keeps the serial transcript clean for the runner.

#![no_std]
#![no_main]

use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, USB};
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_rp::{bind_interrupts, dma, spi};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Delay, Duration, Timer};
use rfm69_async::{Address, Flags, MacTiming, Rfm69, Stack, StackResources, TxError, config};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
});

const N_PACKETS: u16 = 100;
const PEER: Address = Address::Unicast(1);
const OWN_ADDRESS: Address = Address::Unicast(2);
// Spacing between successful sends. ACK timeouts already insert
// `MacTiming::tx_retry_delay` per retry, so we don't need a long gap here.
const INTER_SEND_DELAY: Duration = Duration::from_millis(20);

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver).unwrap());

    // Settle so the host has time to enumerate before logs start.
    Timer::after(Duration::from_secs(4)).await;

    let miso = p.PIN_16;
    let mosi = p.PIN_19;
    let clk = p.PIN_18;
    let rfm_cs = p.PIN_17;
    let rfm_reset = p.PIN_11;

    let mut rfm_config = spi::Config::default();
    rfm_config.frequency = 10_000_000;
    let spi_periph = spi::Spi::new(p.SPI0, clk, mosi, miso, p.DMA_CH0, p.DMA_CH1, Irqs, rfm_config);
    let spi_bus: Mutex<NoopRawMutex, _> = Mutex::new(spi_periph);

    let cs = Output::new(rfm_cs, Level::Low);
    let reset = Output::new(rfm_reset, Level::High);
    let dio0 = Some(Input::new(p.PIN_15, Pull::None));
    let rfm_spi = SpiDevice::new(&spi_bus, cs);

    let mut rfm = Rfm69::new(rfm_spi, reset, dio0, Delay);
    if let Err(e) = config::my_defaults(&mut rfm, 42, 868_480_000).await {
        log::error!("HIL: FAIL config error: {:?}", e);
        park_forever().await;
    }

    let mut resources: StackResources = StackResources::new();
    let (stack, mut runner) = Stack::new(rfm, OWN_ADDRESS, &mut resources, MacTiming::default());

    join(runner.run(), async {
        log::info!("HIL: ready");

        // Give the server time to come up if it was flashed second. The
        // justfile flashes the server first, but a manual flash in the
        // opposite order shouldn't lose the first few packets.
        Timer::after(Duration::from_secs(8)).await;

        let mut n_ok: u16 = 0;
        let mut n_timeout: u16 = 0;
        for counter in 0..N_PACKETS {
            let payload = [counter as u8, (counter >> 8) as u8];
            match stack.send(PEER, Flags::Ack(3), &payload).await {
                Ok(()) => {
                    n_ok += 1;
                    log::info!("HIL: send {} ok", counter);
                }
                Err(TxError::AckTimeout) => {
                    n_timeout += 1;
                    log::warn!("HIL: send {} timeout", counter);
                }
                Err(e) => {
                    log::error!("HIL: FAIL send {} err {:?}", counter, e);
                    park_forever().await;
                }
            }
            Timer::after(INTER_SEND_DELAY).await;
        }

        log::info!("HIL: PASS sent={} ack_timeouts={}", n_ok, n_timeout);
        park_forever().await;
    })
    .await;
}

/// After PASS or FAIL there's nothing left to do — just sit forever so the
/// USB transport keeps draining and the runner can read the last lines.
async fn park_forever() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
