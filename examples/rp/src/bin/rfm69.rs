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
use embassy_time::{with_timeout, Delay, Duration, Timer};
use embedded_hal_1::digital::{InputPin, OutputPin};
use embedded_hal_async::delay::DelayUs;
use embedded_hal_async::digital::Wait;
use embedded_hal_async::spi::SpiDevice as SpiDeviceTrait;
use rfm69_async::{config, Address, Error, Flags, Packet, Rfm69};
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
    let own_address = Address::Unicast(42);
    loop {
        let to_address = Address::Unicast(70);
        let res = send_packet(&mut rfm, own_address, to_address, Flags::None, &[0xAA, counter as u8]).await;
        log::info!("Tx Res {:?}", res);
        Timer::after(Duration::from_secs(1)).await;

        counter += 1;
        log::info!("Tick {}", counter);
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

#[derive(Debug)]
enum TxError<SPI, RESET, DIO0> {
    AckTimeout,
    Rfm69Error(Error<SPI, RESET, DIO0>),
}

const MAC_ACK_TIMEOUT: Duration = Duration::from_millis(50);
const TX_RETRY_DELAY: Duration = Duration::from_millis(200);

async fn send_packet<SPI, RESET, DIO0, DELAY, E>(
    rfm: &mut Rfm69<SPI, RESET, DIO0, DELAY>,
    src: Address,
    dst: Address,
    flags: Flags,
    data: &[u8],
) -> Result<(), TxError<E, RESET::Error, DIO0::Error>>
where
    SPI: SpiDeviceTrait<u8, Error = E>,
    RESET: OutputPin,
    DIO0: InputPin + Wait,
    DELAY: DelayUs,
{
    let packet = Packet::new(src, dst, flags, data).map_err(|_| TxError::Rfm69Error(Error::WrongPacketFormat))?;

    match flags {
        Flags::None | Flags::Ack(0) => rfm.send(&packet).await.map_err(|e| TxError::Rfm69Error(e)),
        Flags::Ack(retries) => {
            for _ in 1..retries {
                rfm.send(&packet).await.map_err(|e| TxError::Rfm69Error(e))?;
                let result = with_timeout(MAC_ACK_TIMEOUT, wait_for_mac_ack(rfm, src, dst)).await;
                match result {
                    Ok(Ok(())) => return Ok(()),
                    Ok(Err(e)) => return Err(TxError::Rfm69Error(e)),
                    Err(_) => Timer::after(TX_RETRY_DELAY).await,
                }
            }
            Err(TxError::AckTimeout)
        }
    }
}

async fn wait_for_mac_ack<SPI, RESET, DIO0, DELAY, E>(
    rfm: &mut Rfm69<SPI, RESET, DIO0, DELAY>,
    src: Address,
    dst: Address,
) -> Result<(), Error<E, RESET::Error, DIO0::Error>>
where
    SPI: SpiDeviceTrait<u8, Error = E>,
    RESET: OutputPin,
    DIO0: InputPin + Wait,
    DELAY: DelayUs,
{
    loop {
        // expect an ack from dst
        let rx_packet = rfm.recv().await?;
        if rx_packet.src == dst && rx_packet.dst == src && rx_packet.is_ack() {
            return Ok(());
        }
    }
}

async fn receive_packet<SPI, RESET, DIO0, DELAY, E>(
    rfm: &mut Rfm69<SPI, RESET, DIO0, DELAY>,
    dst: Address,
) -> Result<Packet, Error<E, RESET::Error, DIO0::Error>>
where
    SPI: SpiDeviceTrait<u8, Error = E>,
    RESET: OutputPin,
    DIO0: InputPin + Wait,
    DELAY: DelayUs,
{
    loop {
        let packet = rfm.recv().await?;
        match packet.dst {
            Address::Unicast(addr) if Address::Unicast(addr) == dst => {
                if let Flags::Ack(n) = packet.flags {
                    // Do not send acks to requests with 0 retry count
                    if n > 0 {
                        let ack =
                            Packet::new(dst, packet.src, Flags::Ack(0), &[]).map_err(|_| Error::WrongPacketFormat)?;
                        // TODO: There may be a need for a delay here, if the sender is not able to switch into receive mode quick enough
                        rfm.send(&ack).await?;
                    }
                }
                return Ok(packet);
            }
            Address::Broadcast => return Ok(packet),
            _ => (),
        }
    }
}
