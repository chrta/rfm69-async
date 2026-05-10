// SPDX-License-Identifier: AGPL-3.0-only

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_u8_recognizes_broadcast() {
        assert_eq!(Address::from_u8(255), Address::Broadcast);
    }

    #[test]
    fn from_u8_recognizes_unicast() {
        assert_eq!(Address::from_u8(0), Address::Unicast(0));
        assert_eq!(Address::from_u8(42), Address::Unicast(42));
        // 254 is the highest unicast address; 255 is reserved for broadcast.
        assert_eq!(Address::from_u8(254), Address::Unicast(254));
    }

    #[test]
    fn as_u8_encodes_broadcast() {
        assert_eq!(Address::Broadcast.as_u8(), 255);
    }

    #[test]
    fn as_u8_encodes_unicast() {
        assert_eq!(Address::Unicast(0).as_u8(), 0);
        assert_eq!(Address::Unicast(42).as_u8(), 42);
        assert_eq!(Address::Unicast(254).as_u8(), 254);
    }

    #[test]
    fn roundtrip_preserves_address() {
        for byte in 0u8..=255 {
            assert_eq!(Address::from_u8(byte).as_u8(), byte);
        }
    }
}
