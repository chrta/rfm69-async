use heapless::Vec;

use crate::address::Address;
use crate::flags::Flags;

#[derive(Debug)]
pub enum PacketError {
    DataTooLong,
    DataTooShort,
}

/// Packet that can be sent and received
#[derive(Debug)]
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
