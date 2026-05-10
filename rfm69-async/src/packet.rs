// SPDX-License-Identifier: AGPL-3.0-only

use heapless::Vec;

use crate::address::Address;
use crate::flags::Flags;

#[derive(Debug)]
pub enum PacketError {
    DataTooLong,
    DataTooShort,
}

/// Packet that can be sent and received
#[derive(Debug, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Packet {
    pub src: Address,
    pub dst: Address,
    pub flags: Flags,
    pub data: Vec<u8, 61>,
    pub rssi: Option<i16>,
}

impl Packet {
    /// this is without the length byte itself
    const MIN_VALID_PACKET_LEN: u8 = 3;

    const MAX_PAYLOAD_DATA_LENGTH: usize = 61;

    pub fn new(src: Address, dst: Address, flags: Flags, data: &[u8]) -> Result<Packet, PacketError> {
        if data.len() > Self::MAX_PAYLOAD_DATA_LENGTH {
            return Err(PacketError::DataTooLong);
        }
        Ok(Self {
            src,
            dst,
            flags,
            data: Vec::from_slice(data).unwrap(),
            rssi: None,
        })
    }

    pub fn from_rx_data(len: u8, raw: &[u8], rssi: i16) -> Result<Packet, PacketError> {
        if len < Self::MIN_VALID_PACKET_LEN {
            return Err(PacketError::DataTooShort);
        }
        Ok(Self {
            src: Address::from_u8(raw[0]),
            dst: Address::from_u8(raw[1]),
            flags: Flags::from_u8(raw[2]),
            data: Vec::from_slice(&raw[3..len as usize]).unwrap(),
            rssi: Some(rssi),
        })
    }

    /// Converts packet to byte slice for the fifo
    ///
    /// The returned length is the amount of bytes written into the given
    /// array.
    /// # Arguments
    /// * `raw` - This array is filled
    pub(crate) fn to_slice(&self, raw: &mut [u8; 65]) -> Result<u8, PacketError> {
        // Length of the data inside the fifo (excluding the length itself)
        let fifo_len = self.data.len() as u8 + Self::MIN_VALID_PACKET_LEN;

        raw[0] = fifo_len;
        raw[1] = self.src.as_u8();
        raw[2] = self.dst.as_u8();
        raw[3] = self.flags.as_u8();
        raw[4..4 + self.data.len()].copy_from_slice(self.data.as_slice());
        Ok(fifo_len + 1)
    }

    pub fn is_ack(&self) -> bool {
        match self.flags {
            Flags::None => false,
            Flags::Ack(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accepts_empty_payload() {
        let p = Packet::new(Address::Unicast(1), Address::Unicast(2), Flags::None, &[]).unwrap();
        assert_eq!(p.data.len(), 0);
        assert_eq!(p.rssi, None);
    }

    #[test]
    fn new_accepts_max_payload() {
        let payload = [0xAA; 61];
        let p = Packet::new(Address::Unicast(1), Address::Unicast(2), Flags::None, &payload).unwrap();
        assert_eq!(p.data.len(), 61);
        assert_eq!(p.data.as_slice(), &payload);
    }

    #[test]
    fn new_rejects_oversized_payload() {
        let payload = [0; 62];
        assert!(matches!(
            Packet::new(Address::Unicast(1), Address::Unicast(2), Flags::None, &payload),
            Err(PacketError::DataTooLong)
        ));
    }

    #[test]
    fn from_rx_data_rejects_len_below_header() {
        // A wire packet must be at least 3 bytes (src/dst/flags) excluding the
        // length byte, so anything less is malformed.
        for len in 0..3u8 {
            assert!(matches!(
                Packet::from_rx_data(len, &[0; 4], -100),
                Err(PacketError::DataTooShort)
            ));
        }
    }

    #[test]
    fn from_rx_data_parses_header_only() {
        // src=42, dst=70, flags=Ack(0) (wire byte 1), no payload, len=3
        let raw = [42, 70, 1];
        let p = Packet::from_rx_data(3, &raw, -42).unwrap();
        assert_eq!(p.src, Address::Unicast(42));
        assert_eq!(p.dst, Address::Unicast(70));
        assert_eq!(p.flags, Flags::Ack(0));
        assert_eq!(p.data.len(), 0);
        assert_eq!(p.rssi, Some(-42));
    }

    #[test]
    fn from_rx_data_parses_payload_and_captures_rssi() {
        // src=1, dst=255 (broadcast), flags=None, payload=[0xDE, 0xAD]
        let raw = [1, 255, 0, 0xDE, 0xAD];
        let p = Packet::from_rx_data(5, &raw, -91).unwrap();
        assert_eq!(p.src, Address::Unicast(1));
        assert_eq!(p.dst, Address::Broadcast);
        assert_eq!(p.flags, Flags::None);
        assert_eq!(p.data.as_slice(), &[0xDE, 0xAD]);
        assert_eq!(p.rssi, Some(-91));
    }

    #[test]
    fn to_slice_serializes_header_only() {
        let p = Packet::new(Address::Unicast(42), Address::Unicast(70), Flags::None, &[]).unwrap();
        let mut buf = [0u8; 65];
        let total = p.to_slice(&mut buf).unwrap();
        // fifo_len = 3 (src/dst/flags), total written = fifo_len + 1
        assert_eq!(total, 4);
        assert_eq!(&buf[..4], &[3, 42, 70, 0]);
    }

    #[test]
    fn to_slice_serializes_payload_and_returns_correct_length() {
        let payload = [0x01, 0x02, 0x03];
        let p = Packet::new(
            Address::Unicast(42),
            Address::Broadcast,
            Flags::Ack(2), // wire byte 3
            &payload,
        )
        .unwrap();
        let mut buf = [0u8; 65];
        let total = p.to_slice(&mut buf).unwrap();
        // fifo_len excludes itself: 3 header + 3 payload = 6
        assert_eq!(total, 7);
        assert_eq!(&buf[..7], &[6, 42, 255, 3, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn to_slice_then_from_rx_data_roundtrips() {
        let payload = [0xCA, 0xFE, 0xBA, 0xBE];
        let original = Packet::new(Address::Unicast(7), Address::Unicast(11), Flags::Ack(3), &payload).unwrap();
        let mut buf = [0u8; 65];
        let total = original.to_slice(&mut buf).unwrap();

        // to_slice writes [fifo_len, src, dst, flags, payload...]; from_rx_data
        // consumes src/dst/flags/payload and takes fifo_len out-of-band.
        let len_byte = buf[0];
        let parsed = Packet::from_rx_data(len_byte, &buf[1..total as usize], -55).unwrap();
        assert_eq!(parsed.src, original.src);
        assert_eq!(parsed.dst, original.dst);
        assert_eq!(parsed.flags, original.flags);
        assert_eq!(parsed.data.as_slice(), original.data.as_slice());
        assert_eq!(parsed.rssi, Some(-55));
    }

    #[test]
    fn is_ack_distinguishes_flags() {
        let none = Packet::new(Address::Unicast(1), Address::Unicast(2), Flags::None, &[]).unwrap();
        let ack0 = Packet::new(Address::Unicast(1), Address::Unicast(2), Flags::Ack(0), &[]).unwrap();
        let ack3 = Packet::new(Address::Unicast(1), Address::Unicast(2), Flags::Ack(3), &[]).unwrap();
        assert!(!none.is_ack());
        assert!(ack0.is_ack());
        assert!(ack3.is_ack());
    }
}
