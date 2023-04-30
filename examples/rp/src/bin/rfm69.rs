#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver;
use embassy_rp::{interrupt, spi};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Delay, Duration, Timer};
use rfm69_async::{config, Rfm69};
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let irq = interrupt::take!(USBCTRL_IRQ);
    let driver = Driver::new(p.USB, irq);
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
    let mut rx_buffer: [u8; 66] = [0; 66];
    loop {
        /* Because of variable packet length config, the first byte in the fifo must be the length of the following data.
         * TODO: Move this into send function and Packet struct
         */
        //let res = rfm.send(&[4, 0xAA, 1, 2, counter as u8]).await;
        //log::info!("Tx Res {:?}", res);
        //Timer::after(Duration::from_secs(1)).await;
        counter += 1;
        log::info!("Tick {}", counter);
        let res = rfm.recv(&mut rx_buffer).await;
        log::info!("Rx Res {:?}", res);
        if let Ok(len) = res {
            log::info!("Rx Data {:?}", &rx_buffer[..len as usize]);
        }
    }
}
