use crate::Packet;

#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum TrxError {
    TrxNotFound,
    Reset,
    Spi,
    Gpio,
    Config,
    WrongPacketFormat,
}

pub trait Transceiver {
    async fn send(&mut self, packet: &Packet) -> Result<(), TrxError>;
    async fn recv(&mut self) -> Result<Packet, TrxError>;
}
