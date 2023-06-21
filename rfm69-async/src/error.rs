use crate::traits::TrxError;

/// Error for rfm69 transceiver
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error<SPI, RESET, DIO0> {
    VersionMismatch(u8),
    Reset(RESET),
    SPI(SPI),
    DIO0(DIO0),
    SyncSize,
    WrongPacketFormat,
}

impl<SPI, RESET, DIO0> From<Error<SPI, RESET, DIO0>> for TrxError {
    fn from(error: Error<SPI, RESET, DIO0>) -> Self {
        match error {
            Error::VersionMismatch(_) => TrxError::TrxNotFound,
            Error::Reset(_) => TrxError::Reset,
            Error::SPI(_) => TrxError::Spi,
            Error::DIO0(_) => TrxError::Gpio,
            Error::SyncSize => TrxError::Config,
            Error::WrongPacketFormat => TrxError::WrongPacketFormat,
        }
    }
}
