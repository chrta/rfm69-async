// SPDX-License-Identifier: (AGPL-3.0-only AND (MIT OR Apache-2.0))

//! Continuous-Rx diagnostic for the RFM69.
//!
//! Drives the radio without Stack/Runner. Lowers `RegRssiThresh` to
//! maximum sensitivity (~-127 dBm) and polls `RegRssiValue`,
//! `RegIrqFlags1`, `RegIrqFlags2` every few ms. Logs:
//!
//! - Noise-floor min/max every 2 s.
//! - `Rssi` flag rising edge — RX chain heard energy above threshold.
//! - `SyncAddressMatch` rising edge — preamble + sync word locked.
//! - `CrcOk` / `PayloadReady` — full packet decoded.
//!
//! Pair against an `echo_client` (or any RFM69 transmitter on the same
//! freq / bit rate / sync) to localize where the RX chain breaks on a
//! suspect board.

#![no_std]
#![no_main]

use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, USB};
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_rp::{bind_interrupts, dma, spi};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Delay, Duration, Instant, Timer};
use rfm69_async::registers::OpMode;
use rfm69_async::{Rfm69, config};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Debug, driver);
}

const IRQ1_RSSI: u8 = 0x08;
const IRQ1_SYNC: u8 = 0x01;
const IRQ2_CRC_OK: u8 = 0x02;
const IRQ2_PAYLOAD_READY: u8 = 0x04;
const IRQ2_FIFO_OVERRUN: u8 = 0x10;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver).unwrap());

    Timer::after(Duration::from_secs(4)).await;
    log::error!("--- RX diagnostic ---");

    let miso = p.PIN_16;
    let mosi = p.PIN_19;
    let clk = p.PIN_18;
    let rfm_cs = p.PIN_17;
    let rfm_reset = p.PIN_11;

    let mut rfm_config = spi::Config::default();
    rfm_config.frequency = 10_000_000;

    let spi = spi::Spi::new(p.SPI0, clk, mosi, miso, p.DMA_CH0, p.DMA_CH1, Irqs, rfm_config);
    let spi_bus: Mutex<NoopRawMutex, _> = Mutex::new(spi);

    let cs = Output::new(rfm_cs, Level::Low);
    let reset = Output::new(rfm_reset, Level::High);
    let dio0 = Some(Input::new(p.PIN_15, Pull::None));

    let rfm_spi = SpiDevice::new(&spi_bus, cs);

    let mut rfm = Rfm69::new(rfm_spi, reset, dio0, Delay);
    if let Err(e) = config::my_defaults(&mut rfm, 42, 868_480_000).await {
        log::error!("config error: {:?}", e);
        loop {
            Timer::after(Duration::from_secs(1)).await;
        }
    }

    // Maximum sensitivity for the chip's RSSI threshold flag (~-127 dBm).
    if let Err(e) = rfm.rssi_threshold(0xFE).await {
        log::error!("rssi_threshold error: {:?}", e);
    }

    if let Err(e) = rfm.set_mode(OpMode::Rx).await {
        log::error!("set_mode Rx error: {:?}", e);
    }
    Timer::after(Duration::from_millis(10)).await;

    log::info!("Rx armed; sampling RSSI + IRQ flags @ 5 ms");

    let mut last_irq1: u8 = 0;
    let mut last_irq2: u8 = 0;
    let mut rssi_max: i16 = i16::MIN;
    let mut rssi_min: i16 = i16::MAX;
    let mut samples: u32 = 0;
    let mut last_report = Instant::now();

    loop {
        Timer::after(Duration::from_millis(5)).await;

        let rssi = match rfm.rssi().await {
            Ok(v) => v,
            Err(e) => {
                log::error!("rssi err {:?}", e);
                continue;
            }
        };
        let irq1 = match rfm.irq_flags1().await {
            Ok(v) => v,
            Err(e) => {
                log::error!("irq1 err {:?}", e);
                continue;
            }
        };
        let irq2 = match rfm.irq_flags2().await {
            Ok(v) => v,
            Err(e) => {
                log::error!("irq2 err {:?}", e);
                continue;
            }
        };

        if rssi > rssi_max {
            rssi_max = rssi;
        }
        if rssi < rssi_min {
            rssi_min = rssi;
        }
        samples += 1;

        let irq1_rose = irq1 & !last_irq1;
        let irq2_rose = irq2 & !last_irq2;

        if irq1_rose & IRQ1_RSSI != 0 {
            log::warn!("Rssi flag set; RSSI={} dBm", rssi);
        }
        if irq1_rose & IRQ1_SYNC != 0 {
            log::warn!("SyncAddressMatch; RSSI={} dBm irq1={:#04x}", rssi, irq1);
        }
        if irq2_rose & IRQ2_CRC_OK != 0 {
            log::info!("CrcOk; RSSI={} dBm", rssi);
        }
        if irq2_rose & IRQ2_PAYLOAD_READY != 0 {
            log::warn!("PayloadReady; RSSI={} dBm irq2={:#04x}", rssi, irq2);
            // Rearm Rx so subsequent packets show up.
            let _ = rfm.set_mode(OpMode::Standby).await;
            Timer::after(Duration::from_millis(1)).await;
            let _ = rfm.set_mode(OpMode::Rx).await;
            // Resetting state — last_irq1/2 will refresh on next loop.
        }
        if irq2_rose & IRQ2_FIFO_OVERRUN != 0 {
            log::error!("FifoOverrun");
            let _ = rfm.set_mode(OpMode::Standby).await;
            Timer::after(Duration::from_millis(1)).await;
            let _ = rfm.set_mode(OpMode::Rx).await;
        }

        last_irq1 = irq1;
        last_irq2 = irq2;

        let now = Instant::now();
        if (now - last_report).as_secs() >= 2 {
            log::info!(
                "noise min={} dBm max={} dBm samples={} irq1={:#04x} irq2={:#04x}",
                rssi_min,
                rssi_max,
                samples,
                irq1,
                irq2
            );
            rssi_min = i16::MAX;
            rssi_max = i16::MIN;
            samples = 0;
            last_report = now;
        }
    }
}
