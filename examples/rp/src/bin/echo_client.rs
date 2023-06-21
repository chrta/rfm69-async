#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_rp::{bind_interrupts, spi};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{with_timeout, Delay, Duration, Timer};
use rfm69_async::{config, Address, Flags, Rfm69, Stack};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Debug, driver);
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver)).unwrap();

    // wait a little so usb logger is completely initialized
    Timer::after(Duration::from_secs(4)).await;
    log::error!("--- Staring echo client ---");

    // SPI0 pins of RPi Pico
    let miso = p.PIN_16;
    let mosi = p.PIN_19;
    let clk = p.PIN_18;
    let rfm_cs = p.PIN_17;

    let rfm_reset = p.PIN_11;

    let mut rfm_config = spi::Config::default();
    rfm_config.frequency = 10_000_000;

    let spi = spi::Spi::new(p.SPI0, clk, mosi, miso, p.DMA_CH0, p.DMA_CH1, rfm_config);
    let spi_bus: Mutex<NoopRawMutex, _> = Mutex::new(spi);

    let cs = Output::new(rfm_cs, Level::Low);
    let reset = Output::new(rfm_reset, Level::High);
    let dio0 = Some(Input::new(p.PIN_15, Pull::None));

    let rfm_spi = SpiDevice::new(&spi_bus, cs);

    let rfm = config::my_defaults(Rfm69::new(rfm_spi, reset, dio0, Delay), 42, 868_480_000).await;
    let rfm = match rfm {
        Ok(r) => r,
        Err(e) => {
            log::error!("Error: {:?}", e);
            Timer::after(Duration::from_millis(5000)).await;
            panic!();
        }
    };

    let own_address = Address::Unicast(84);
    let mut stack = Stack::new(rfm, own_address);

    let mut counter = 0;

    log::info!("Own address: {:?}", own_address);
    loop {
        let to_address = Address::Unicast(42);
        let data_to_send = [0xAA, counter as u8];
        log::info!("Sending packet to {:?}", to_address);
        let res = stack.send_packet(to_address, Flags::Ack(3), &data_to_send).await;
        log::info!("Tx Res {:?}", res);

        // now expect an echo with same counter, but probably other flags
        log::debug!("Expecting echo within 1 second (# {})", counter);
        let rx_result = with_timeout(Duration::from_secs(1), stack.receive_packet()).await;
        match rx_result {
            Ok(Ok(packet)) => {
                log::info!("Rx Packet {:?}", packet);
                if packet.src == to_address && packet.dst == own_address && packet.data.len() == data_to_send.len() {
                    log::info!("Received expected echo");
                } else {
                    log::error!("Received wrong packet");
                }
            }
            Ok(Err(e)) => log::error!("Rx error {:?}", e),
            Err(e) => log::error!("Rx timeout error {:?}", e),
        }

        Timer::after(Duration::from_secs(10)).await;

        counter += 1;
    }
}
