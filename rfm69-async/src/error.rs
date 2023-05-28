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
