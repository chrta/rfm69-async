use embassy_time::{with_timeout, Duration, Timer};

use crate::{Address, Flags, Packet, Transceiver, TrxError};

// Delay sending MAC ACK by this duration, so the original sender could switch
// from TX to RX mode.
const MAC_ACK_TX_DELAY: Duration = Duration::from_millis(10);
const MAC_ACK_TIMEOUT: Duration = Duration::from_millis(50);
const TX_RETRY_DELAY: Duration = Duration::from_millis(200);

#[derive(Debug)]
pub enum TxError {
    AckTimeout,
    TrxError(TrxError),
}

pub struct Stack<TRX> {
    trx: TRX,
    address: Address,
}

impl<TRX: Transceiver> Stack<TRX> {
    pub fn new(trx: TRX, address: Address) -> Self {
        Self { trx, address }
    }

    pub async fn send_packet(&mut self, dst: Address, flags: Flags, data: &[u8]) -> Result<(), TxError> {
        let packet =
            Packet::new(self.address, dst, flags, data).map_err(|_| TxError::TrxError(TrxError::WrongPacketFormat))?;

        match flags {
            Flags::None | Flags::Ack(0) => {
                log::info!("Sending packet");
                self.trx.send(&packet).await.map_err(|e| TxError::TrxError(e))
            }
            Flags::Ack(retries) => {
                for i in 1..=retries {
                    log::info!("Sending packet {i} of {retries} and waiting for ACK");
                    self.trx.send(&packet).await.map_err(|e| TxError::TrxError(e))?;
                    let result = with_timeout(MAC_ACK_TIMEOUT, self.wait_for_mac_ack(dst)).await;
                    match result {
                        Ok(Ok(())) => return Ok(()),
                        Ok(Err(e)) => return Err(TxError::TrxError(e)),
                        Err(_) => Timer::after(TX_RETRY_DELAY).await,
                    }
                }
                Err(TxError::AckTimeout)
            }
        }
    }

    pub async fn receive_packet(&mut self) -> Result<Packet, TrxError> {
        loop {
            let packet = self.trx.recv().await?;
            match packet.dst {
                Address::Unicast(addr) if Address::Unicast(addr) == self.address => {
                    if let Flags::Ack(n) = packet.flags {
                        // Do not send acks to requests with 0 retry count
                        if n > 0 {
                            let ack = Packet::new(self.address, packet.src, Flags::Ack(0), &[])
                                .map_err(|_| TrxError::WrongPacketFormat)?;
                            log::info!("Sending requested ACK as reply");

                            // Add small delay, if the sender is not able to switch into receive mode quick enough
                            Timer::after(MAC_ACK_TX_DELAY).await;

                            self.trx.send(&ack).await?;
                        }
                    }
                    return Ok(packet);
                }
                Address::Broadcast => return Ok(packet),
                _ => (),
            }
        }
    }

    async fn wait_for_mac_ack(&mut self, from: Address) -> Result<(), TrxError> {
        loop {
            // expect an ack from dst
            let rx_packet = self.trx.recv().await?;
            if rx_packet.src == from && rx_packet.dst == self.address && rx_packet.is_ack() {
                log::info!("Received valid ACK");
                return Ok(());
            }
        }
    }
}
