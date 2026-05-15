// SPDX-License-Identifier: (AGPL-3.0-only AND (MIT OR Apache-2.0))

//! HIL test bin: receive-and-count server.
//!
//! Paired with `hil_send_client`. Receives `Flags::Ack(_)` packets from
//! `Address::Unicast(2)`, decodes the 16-bit counter in the first two
//! payload bytes, and emits machine-readable lines for the host runner:
//!
//! ```text
//! HIL: ready
//! HIL: recv 0 from Unicast(2)
//! HIL: dup 0
//! ...
//! HIL: PASS received=100
//! ```
//!
//! The Stack's auto-ACK reply (driven by the `Flags::Ack(n)` bit set by
//! the client) closes the round-trip without any explicit `send` from
//! this bin. `HIL: dup` lines are expected — they record retries the
//! client made when our ACK got lost; the server's count of unique
//! packets is what `HIL: PASS received=N` reports.
//!
//! 30 seconds of silence (no new or duplicate packets) before all
//! `N_PACKETS` have arrived is treated as a hung scenario and reported as
//! `HIL: FAIL received=<count>`.

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
use embassy_time::{Delay, Duration, Timer, with_timeout};
use rfm69_async::{Address, MacTiming, Rfm69, Stack, StackResources, config};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
});

const N_PACKETS: u16 = 100;
const OWN_ADDRESS: Address = Address::Unicast(1);
const NO_PROGRESS_TIMEOUT: Duration = Duration::from_secs(30);

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver).unwrap());

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

        // Bitmap of unique counters seen so far. Allocate inline; with
        // N_PACKETS = 100 this is 100 bytes — well within the embassy task
        // stack budget.
        let mut seen = [false; N_PACKETS as usize];
        let mut unique: u16 = 0;
        let mut announced_pass = false;

        loop {
            match with_timeout(NO_PROGRESS_TIMEOUT, stack.recv()).await {
                Ok(packet) => {
                    let data = packet.data.as_slice();
                    if data.len() < 2 {
                        log::warn!("HIL: bad-payload from {:?}", packet.src);
                        continue;
                    }
                    let counter = u16::from_le_bytes([data[0], data[1]]);
                    if counter >= N_PACKETS {
                        log::warn!("HIL: bad-counter {} from {:?}", counter, packet.src);
                        continue;
                    }
                    let idx = counter as usize;
                    if seen[idx] {
                        log::info!("HIL: dup {}", counter);
                    } else {
                        seen[idx] = true;
                        unique += 1;
                        log::info!("HIL: recv {} from {:?}", counter, packet.src);
                    }
                    if unique == N_PACKETS && !announced_pass {
                        log::info!("HIL: PASS received={}", unique);
                        announced_pass = true;
                        // Keep receiving so client retries get ACKed cleanly.
                    }
                }
                Err(_) => {
                    if announced_pass {
                        // We're past the scenario; silence is fine.
                        continue;
                    }
                    log::error!("HIL: FAIL received={}", unique);
                    park_forever().await;
                }
            }
        }
    })
    .await;
}

async fn park_forever() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
