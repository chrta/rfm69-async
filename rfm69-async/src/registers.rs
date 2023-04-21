//! Register addresses and content helpers for the rfm69

/// RFM69 register addresses
///
/// See datasheet `<https://www.hoperf.com/data/upload/portal/20190307/RFM69CW-V1.1.pdf>`
/// chapter 6 "Configuration and Status Registers"
#[derive(Clone, Copy)]
pub enum Register {
    Fifo = 0x00,
    OpMode = 0x01,
    DataModul = 0x02,
    BitrateMsb = 0x03,
    BitrateLsb = 0x04,
    FdevMsb = 0x05,
    FdevLsb = 0x06,
    FrfMsb = 0x07,
    FrfMid = 0x08,
    FrfLsb = 0x09,
    Version = 0x10,
    Lna = 0x18,
    RxBw = 0x19,
    RssiValue = 0x24,
    DioMapping1 = 0x25,
    DioMapping2 = 0x26,
    IrqFlags1 = 0x27,
    IrqFlags2 = 0x28,
    RssiThresh = 0x29,
    PreambleMsb = 0x2C,
    PreambleLsb = 0x2D,
    SyncConfig = 0x2E,
    SyncValue1 = 0x2F,
    PacketConfig1 = 0x37,
    FifoThresh = 0x3C,
    PacketConfig2 = 0x3D,
    TestDagc = 0x6F,
}

impl Register {
    pub fn addr(self) -> u8 {
        self as u8
    }
}

/// Modes of the radio and their corresponding register values.
#[derive(Clone, Copy)]
pub enum OpMode {
    SequencerOff = 0x80,
    ListenOn = 0x40,
    ListenAbort = 0x20,
    Rx = 0x10,
    Tx = 0x0c,
    FreqSyn = 0x08,
    Standby = 0x04,
    Sleep = 0x00,
}

impl OpMode {
    pub fn value(self) -> u8 {
        self as u8
    }
}

pub struct Modulation {
    pub data_mode: DataMode,
    pub modulation_type: ModulationType,
    pub shaping: ModulationShaping,
}

impl Modulation {
    pub(crate) fn value(&self) -> u8 {
        self.data_mode as u8 | self.modulation_type as u8 | self.shaping as u8
    }
}

#[derive(Copy, Clone)]
pub enum DataMode {
    Packet = 0x00,
    Continuous = 0x40,
    ContinuousBitSync = 0x60,
}

#[derive(Copy, Clone)]
pub enum ModulationType {
    Fsk = 0x00,
    Ook = 0x08,
}

#[derive(Copy, Clone)]
pub enum ModulationShaping {
    Shaping00 = 0x00,
    Shaping01 = 0x01,
    Shaping10 = 0x02,
    Shaping11 = 0x03,
}

pub enum FifoMode {
    NotEmpty,
    Level(u8),
}

pub enum InterPacketRxDelay {
    Delay1Bit = 0x00,
    Delay2Bits = 0x10,
    Delay4Bits = 0x20,
    Delay8Bits = 0x30,
    Delay16Bits = 0x40,
    Delay32Bits = 0x50,
    Delay64Bits = 0x60,
    Delay128Bits = 0x70,
    Delay256Bits = 0x80,
    Delay512Bits = 0x90,
    Delay1024Bits = 0xA0,
    Delay2048Bits = 0xB0,
}

pub enum PacketDc {
    None = 0x00,
    Manchester = 0x20,
    Whitening = 0x40,
}

pub enum PacketFiltering {
    None = 0x00,
    Address = 0x02,
    Broadcast = 0x04,
}

pub enum PacketFormat {
    Variable(u8),
    Fixed(u8),
}

pub struct PacketConfig {
    pub format: PacketFormat,
    pub dc: PacketDc,
    pub crc: bool,
    pub filtering: PacketFiltering,
    pub interpacket_rx_delay: InterPacketRxDelay,
    pub auto_rx_restart: bool,
}

pub struct LnaConfig {
    pub zin: LnaImpedance,
    pub gain_select: LnaGain,
}

pub enum LnaImpedance {
    Ohm50 = 0x00,
    Ohm200 = 0x80,
}

pub enum LnaGain {
    AgcLoop = 0b000,
    G1 = 0b001,
    G2 = 0b010,
    G3 = 0b011,
    G4 = 0b100,
    G5 = 0b101,
    G6 = 0b110,
}

pub enum ContinuousDagc {
    Normal = 0x00,
    ImprovedMarginAfcLowBetaOn1 = 0x20,
    ImprovedMarginAfcLowBetaOn0 = 0x30,
}

pub struct RxBw<T>
where
    T: RxBwFreq,
{
    pub dcc_cutoff: DccCutoff,
    pub rx_bw: T,
}

pub enum DccCutoff {
    Percent16 = 0x00,
    Percent8 = 0x20,
    Percent4 = 0x40,
    Percent2 = 0x60,
    Percent1 = 0x80,
    Percent0dot5 = 0xA0,
    Percent0dot25 = 0xC0,
    Percent0dot125 = 0xE0,
}

pub trait RxBwFreq {
    fn value(&self) -> u8;
}

pub enum RxBwFsk {
    Khz2dot6,
    Khz3dot1,
    Khz3dot9,
    Khz5dot2,
    Khz6dot3,
    Khz7dot8,
    Khz10dot4,
    Khz12dot5,
    Khz15dot6,
    Khz20dot8,
    Khz25dot0,
    Khz31dot3,
    Khz41dot7,
    Khz50dot0,
    Khz62dot5,
    Khz83dot3,
    Khz100dot0,
    Khz125dot0,
    Khz166dot7,
    Khz200dot0,
    Khz250dot0,
    Khz333dot3,
    Khz400dot0,
    Khz500dot0,
}

impl RxBwFreq for RxBwFsk {
    #[inline]
    fn value(&self) -> u8 {
        match self {
            RxBwFsk::Khz2dot6 => 0b10 << 3 | 7,
            RxBwFsk::Khz3dot1 => 0b01 << 3 | 7,
            RxBwFsk::Khz3dot9 => 7,
            RxBwFsk::Khz5dot2 => 0b10 << 3 | 6,
            RxBwFsk::Khz6dot3 => 0b01 << 3 | 6,
            RxBwFsk::Khz7dot8 => 6,
            RxBwFsk::Khz10dot4 => 0b10 << 3 | 5,
            RxBwFsk::Khz12dot5 => 0b01 << 3 | 5,
            RxBwFsk::Khz15dot6 => 5,
            RxBwFsk::Khz20dot8 => 0b10 << 3 | 4,
            RxBwFsk::Khz25dot0 => 0b01 << 3 | 4,
            RxBwFsk::Khz31dot3 => 4,
            RxBwFsk::Khz41dot7 => 0b10 << 3 | 3,
            RxBwFsk::Khz50dot0 => 0b01 << 3 | 3,
            RxBwFsk::Khz62dot5 => 3,
            RxBwFsk::Khz83dot3 => 0b10 << 3 | 2,
            RxBwFsk::Khz100dot0 => 0b01 << 3 | 2,
            RxBwFsk::Khz125dot0 => 2,
            RxBwFsk::Khz166dot7 => 0b10 << 3 | 1,
            RxBwFsk::Khz200dot0 => 0b01 << 3 | 1,
            RxBwFsk::Khz250dot0 => 1,
            RxBwFsk::Khz333dot3 => 0b10 << 3,
            RxBwFsk::Khz400dot0 => 0b01 << 3,
            RxBwFsk::Khz500dot0 => 0,
        }
    }
}

pub enum RxBwOok {
    Khz1dot3,
    Khz1dot6,
    Khz2dot0,
    Khz2dot6,
    Khz3dot1,
    Khz3dot9,
    Khz5dot2,
    Khz6dot3,
    Khz7dot8,
    Khz10dot4,
    Khz12dot5,
    Khz15dot6,
    Khz20dot8,
    Khz25dot0,
    Khz31dot3,
    Khz41dot7,
    Khz50dot0,
    Khz62dot5,
    Khz83dot3,
    Khz100dot0,
    Khz125dot0,
    Khz166dot7,
    Khz200dot0,
    Khz250dot0,
}

impl RxBwFreq for RxBwOok {
    #[inline]
    fn value(&self) -> u8 {
        match self {
            RxBwOok::Khz1dot3 => 0b10 << 3 | 7,
            RxBwOok::Khz1dot6 => 0b01 << 3 | 7,
            RxBwOok::Khz2dot0 => 7,
            RxBwOok::Khz2dot6 => 0b10 << 3 | 6,
            RxBwOok::Khz3dot1 => 0b01 << 3 | 6,
            RxBwOok::Khz3dot9 => 6,
            RxBwOok::Khz5dot2 => 0b10 << 3 | 5,
            RxBwOok::Khz6dot3 => 0b01 << 3 | 5,
            RxBwOok::Khz7dot8 => 5,
            RxBwOok::Khz10dot4 => 0b10 << 3 | 4,
            RxBwOok::Khz12dot5 => 0b01 << 3 | 4,
            RxBwOok::Khz15dot6 => 4,
            RxBwOok::Khz20dot8 => 0b10 << 3 | 3,
            RxBwOok::Khz25dot0 => 0b01 << 3 | 3,
            RxBwOok::Khz31dot3 => 3,
            RxBwOok::Khz41dot7 => 0b10 << 3 | 2,
            RxBwOok::Khz50dot0 => 0b01 << 3 | 2,
            RxBwOok::Khz62dot5 => 2,
            RxBwOok::Khz83dot3 => 0b10 << 3 | 1,
            RxBwOok::Khz100dot0 => 0b01 << 3 | 1,
            RxBwOok::Khz125dot0 => 1,
            RxBwOok::Khz166dot7 => 0b10 << 3,
            RxBwOok::Khz200dot0 => 0b01 << 3,
            RxBwOok::Khz250dot0 => 0,
        }
    }
}

#[repr(u8)]
pub enum IrqFlags1 {
    SyncAddressMatch = 0x01,
    AutoMode = 0x02,
    Timeout = 0x04,
    Rssi = 0x08,
    PllLock = 0x10,
    TxReady = 0x20,
    RxReady = 0x40,
    ModeReady = 0x80,
}

impl core::ops::BitAnd<IrqFlags1> for u8 {
    type Output = Self;
    fn bitand(self, rhs: IrqFlags1) -> Self::Output {
        self & rhs as Self
    }
}

#[repr(u8)]
pub enum IrqFlags2 {
    CrcOk = 0x02,
    PayloadReady = 0x04,
    PacketSent = 0x08,
    FifoOverrun = 0x10,
    FifoLevel = 0x20,
    FifoNotEmpty = 0x40,
    FifoFull = 0x80,
}

impl core::ops::BitAnd<IrqFlags2> for u8 {
    type Output = Self;
    fn bitand(self, rhs: IrqFlags2) -> Self::Output {
        self & rhs as Self
    }
}
