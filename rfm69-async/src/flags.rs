#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Flags {
    None,
    Ack(u8),
}
impl Flags {
    pub(crate) fn from_u8(flags: u8) -> Flags {
        match flags {
            0 => Self::None,
            1..=3 => Self::Ack(flags),
            _ => Self::None,
        }
    }

    pub(crate) fn as_u8(&self) -> u8 {
        match self {
            Self::None => 0,
            Self::Ack(retries) => *retries,
        }
    }
}
