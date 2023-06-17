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
use rfm69_async::mac::{receive_packet, send_packet};
use rfm69_async::{config, Address, Flags, Rfm69};
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver)).unwrap();

    // wait a little so usb logger is completely initialized
    Timer::after(Duration::from_secs(5)).await;

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
    let mut rfm = match rfm {
        Ok(r) => r,
        Err(e) => {
            log::error!("Error: {:?}", e);
            Timer::after(Duration::from_millis(5000)).await;
            panic!("PANICCC");
        }
    };

    //for (index, val) in rfm.read_all_regs().await.unwrap().iter().enumerate() {
    //    log::info!("Register 0x{:02x} = 0x{:02x}", index + 1, val);
    //    Timer::after(Duration::from_millis(10)).await;
    // }

    let mut counter = 0;
    let own_address = Address::Unicast(42);

    log::info!("Own address: {:?}", own_address);
    loop {
        let to_address = Address::Unicast(70);
        log::info!("Sending packet to {:?}", to_address);
        let res = send_packet(&mut rfm, own_address, to_address, Flags::Ack(3), &[0xAA, counter as u8]).await;
        log::info!("Tx Res {:?}", res);

        counter += 1;
        log::info!("Trying to receive packet for 10 seconds (# {})", counter);
        let rx_result = with_timeout(Duration::from_secs(10), receive_packet(&mut rfm, own_address)).await;
        match rx_result {
            Ok(Ok(packet)) => {
                log::info!("Rx Packet {:?}", packet);
            }
            Ok(Err(e)) => log::info!("Rx error {:?}", e),
            Err(e) => log::info!("Rx timeout error {:?}", e),
        }
    }
}
