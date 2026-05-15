// SPDX-License-Identifier: (AGPL-3.0-only AND (MIT OR Apache-2.0))

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
use rfm69_async::{Address, Flags, MacTiming, Rfm69, Stack, StackResources, config};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    DMA_IRQ_0 => dma::InterruptHandler<DMA_CH0>, dma::InterruptHandler<DMA_CH1>;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Debug, driver);
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver).unwrap());

    // wait a little so usb logger is completely initialized
    Timer::after(Duration::from_secs(4)).await;
    log::error!("--- Staring echo server ---");

    // SPI0 pins of RPi Pico
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
        log::error!("Error: {:?}", e);
        Timer::after(Duration::from_millis(5000)).await;
        panic!();
    }

    let own_address = Address::Unicast(42);
    let mut resources: StackResources = StackResources::new();
    let (stack, mut runner) = Stack::new(rfm, own_address, &mut resources, MacTiming::default());

    join(runner.run(), async {
        log::info!("Own address: {:?}", own_address);
        loop {
            log::debug!("Trying to receive packet for 600 seconds");
            let rx_result = with_timeout(Duration::from_secs(600), stack.recv()).await;
            match rx_result {
                Ok(packet) => {
                    log::info!("Rx Packet {:?}", packet);
                    let to_address = packet.src;
                    let data = packet.data.as_slice();
                    log::info!("Sending packet to {:?}", to_address);
                    let res = stack.send(to_address, Flags::None, data).await;
                    log::debug!("Tx Res {:?}", res);
                }
                Err(e) => log::info!("Rx timeout error {:?}", e),
            }
        }
    })
    .await;
}
