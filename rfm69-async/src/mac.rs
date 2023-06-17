#[cfg(feature = "embassy")]
use embassy_time::{with_timeout, Duration, Timer};
use embedded_hal_1::digital::{InputPin, OutputPin};
use embedded_hal_async::delay::DelayUs;
use embedded_hal_async::digital::Wait;
use embedded_hal_async::spi::SpiDevice;

use crate::{Address, Error, Flags, Packet, Rfm69};

#[cfg(feature = "embassy")]
#[derive(Debug)]
pub enum TxError<SPI, RESET, DIO0> {
    AckTimeout,
    Rfm69Error(Error<SPI, RESET, DIO0>),
}

// Delay sending MAC ACK by this duration, so the original sender could switch
// from TX to RX mode.
#[cfg(feature = "embassy")]
const MAC_ACK_TX_DELAY: Duration = Duration::from_millis(10);

#[cfg(feature = "embassy")]
const MAC_ACK_TIMEOUT: Duration = Duration::from_millis(50);

#[cfg(feature = "embassy")]
const TX_RETRY_DELAY: Duration = Duration::from_millis(200);

#[cfg(feature = "embassy")]
pub async fn send_packet<SPI, RESET, DIO0, DELAY, E>(
    rfm: &mut Rfm69<SPI, RESET, DIO0, DELAY>,
    src: Address,
    dst: Address,
    flags: Flags,
    data: &[u8],
) -> Result<(), TxError<E, RESET::Error, DIO0::Error>>
where
    SPI: SpiDevice<u8, Error = E>,
    RESET: OutputPin,
    DIO0: InputPin + Wait,
    DELAY: DelayUs,
{
    let packet = Packet::new(src, dst, flags, data).map_err(|_| TxError::Rfm69Error(Error::WrongPacketFormat))?;

    match flags {
        Flags::None | Flags::Ack(0) => {
            log::info!("Sending packet");
            rfm.send(&packet).await.map_err(|e| TxError::Rfm69Error(e))
        }
        Flags::Ack(retries) => {
            for i in 1..=retries {
                log::info!("Sending packet {i} of {retries} and waiting for ACK");
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

pub async fn wait_for_mac_ack<SPI, RESET, DIO0, DELAY, E>(
    rfm: &mut Rfm69<SPI, RESET, DIO0, DELAY>,
    src: Address,
    dst: Address,
) -> Result<(), Error<E, RESET::Error, DIO0::Error>>
where
    SPI: SpiDevice<u8, Error = E>,
    RESET: OutputPin,
    DIO0: InputPin + Wait,
    DELAY: DelayUs,
{
    loop {
        // expect an ack from dst
        let rx_packet = rfm.recv().await?;
        if rx_packet.src == dst && rx_packet.dst == src && rx_packet.is_ack() {
            log::info!("Received valid ACK");
            return Ok(());
        }
    }
}

pub async fn receive_packet<SPI, RESET, DIO0, DELAY, E>(
    rfm: &mut Rfm69<SPI, RESET, DIO0, DELAY>,
    dst: Address,
) -> Result<Packet, Error<E, RESET::Error, DIO0::Error>>
where
    SPI: SpiDevice<u8, Error = E>,
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
                        log::info!("Sending requested ACK as reply");

                        // Add small delay, if the sender is not able to switch into receive mode quick enough
                        #[cfg(feature = "embassy")]
                        Timer::after(MAC_ACK_TX_DELAY).await;

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
