#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Address {
    Broadcast,
    Unicast(u8),
}
impl Address {
    pub(crate) fn from_u8(addr: u8) -> Address {
        if addr == 255 {
            Self::Broadcast
        } else {
            Self::Unicast(addr)
        }
    }

    pub(crate) fn as_u8(&self) -> u8 {
        match self {
            Self::Broadcast => 255,
            Self::Unicast(addr) => *addr,
        }
    }
}
