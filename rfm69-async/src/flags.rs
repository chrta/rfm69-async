// SPDX-License-Identifier: AGPL-3.0-only

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Flags {
    None,
    Ack(u8),
}
impl Flags {
    pub(crate) fn from_u8(flags: u8) -> Flags {
        match flags {
            0 => Self::None,
            1..=4 => Self::Ack(flags - 1),
            _ => Self::None,
        }
    }

    pub(crate) fn as_u8(&self) -> u8 {
        match self {
            Self::None => 0,
            Self::Ack(retries) => *retries + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_u8_zero_is_none() {
        assert_eq!(Flags::from_u8(0), Flags::None);
    }

    #[test]
    fn from_u8_decodes_ack_with_retry_count_offset_by_one() {
        // wire encoding adds 1, so byte 1 means "this is an ACK (no retries)",
        // byte 2 means "want ACK, sender will retry once", up to byte 4.
        assert_eq!(Flags::from_u8(1), Flags::Ack(0));
        assert_eq!(Flags::from_u8(2), Flags::Ack(1));
        assert_eq!(Flags::from_u8(3), Flags::Ack(2));
        assert_eq!(Flags::from_u8(4), Flags::Ack(3));
    }

    #[test]
    fn from_u8_out_of_range_degrades_to_none() {
        // 5 and above are not valid ACK encodings; degrade rather than panic.
        assert_eq!(Flags::from_u8(5), Flags::None);
        assert_eq!(Flags::from_u8(64), Flags::None);
        assert_eq!(Flags::from_u8(255), Flags::None);
    }

    #[test]
    fn as_u8_none_encodes_to_zero() {
        assert_eq!(Flags::None.as_u8(), 0);
    }

    #[test]
    fn as_u8_ack_adds_one_to_retry_count() {
        assert_eq!(Flags::Ack(0).as_u8(), 1);
        assert_eq!(Flags::Ack(1).as_u8(), 2);
        assert_eq!(Flags::Ack(3).as_u8(), 4);
    }

    #[test]
    fn roundtrip_preserves_valid_flags() {
        for byte in 0u8..=4 {
            assert_eq!(Flags::from_u8(byte).as_u8(), byte);
        }
    }

    #[test]
    fn roundtrip_collapses_invalid_bytes() {
        // The wire-protocol byte is normalized to 0 (Flags::None) when it is
        // outside the recognized range, matching what receive_packet sees.
        for byte in 5u8..=255 {
            assert_eq!(Flags::from_u8(byte).as_u8(), 0);
        }
    }
}
