use embedded_hal_1::digital::{InputPin, OutputPin};
use embedded_hal_1::spi::Operation;
use embedded_hal_async::delay::DelayUs;
use embedded_hal_async::digital::Wait;
use embedded_hal_async::spi::SpiDevice;

use crate::error::Error;
use crate::packet::Packet;
use crate::registers::*;
use crate::Transceiver;

/// Expected content of Register::Version
const VERSION_CHECK: u8 = 0x24;

// 1_000_000 larger for better precision.
const F_SCALE: u64 = 1_000_000;
const FOSC: u64 = 32_000_000 * F_SCALE;
const FSTEP: u64 = FOSC / 524_288; // FOSC/2^19

/// The rfm69 transceiver
pub struct Rfm69<SPI, RESET, DIO0, DELAY> {
    spi: SPI,
    reset: RESET,
    dio0: Option<DIO0>,
    delay: DELAY,

    /// Current cached active mode
    pub mode: OpMode,
}

impl<SPI, RESET, DIO0, DELAY, E> Rfm69<SPI, RESET, DIO0, DELAY>
where
    SPI: SpiDevice<u8, Error = E>,
    RESET: OutputPin,
    DIO0: InputPin + Wait,
    DELAY: DelayUs,
{
    /// Returns a Rfm69 instance
    ///
    /// This implementation requires all spi pins to be connected (including cs).
    /// Connection the dio0 signal is optional, but preferred. In case dio0 is not connected,
    /// the interrupt register is polled continuously to detect if a packet was received or of the packet was completely sent.
    /// If dio0 is connected, these events are detected without polling, but with a hardware pin interrupt.
    ///
    /// # Arguments
    ///
    /// * `spi` - The spi bus the device is connected to (including cs)
    /// * `reset` - The mcu pin the rfm69 reset pin is connected to
    /// * `dio0` - The mcu pin the rfm69 reset pin is connected to
    /// * `delay` - The delay implementation
    pub fn new(spi: SPI, reset: RESET, dio0: Option<DIO0>, delay: DELAY) -> Self {
        Self {
            spi,
            reset,
            dio0,
            delay,
            mode: OpMode::Standby,
        }
    }

    /// Resets the rfm69 transceiver
    ///
    /// The transceiver is reset using the pin. Afterwards the version register is read to ensure that the transceiver is usable.
    pub async fn reset(&mut self) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        self.reset.set_high().map_err(Error::Reset)?;
        self.delay.delay_ms(10).await;
        self.reset.set_low().map_err(Error::Reset)?;
        self.delay.delay_ms(10).await;
        log::debug!("Reading version register...");
        let version = self.read_register(Register::Version).await?;
        log::debug!("Version: {version:#x}");
        if version == VERSION_CHECK {
            self.set_mode(OpMode::Sleep).await?;
            Ok(())
        } else {
            Err(Error::VersionMismatch(version))
        }
    }

    /// Sets the state of the radio
    ///
    /// Default mode after initiation is `Standby`.
    pub async fn set_mode(&mut self, mode: OpMode) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        self.write_register(Register::OpMode, mode.value()).await?;

        self.mode = mode;
        Ok(())
    }

    /// Sets the modulation in corresponding register
    pub async fn modulation(&mut self, modulation: Modulation) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        self.write_register(Register::DataModul, modulation.value()).await
    }

    /// Sets the data bitrate in corresponding registers
    ///
    /// There might be a loss of precision, so that the actual data rate is slightly off.
    pub async fn bit_rate(&mut self, bit_rate: u32) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        let reg = (FOSC / (bit_rate as u64 * F_SCALE)) as u16;
        self.write_registers(Register::BitrateMsb, &reg.to_be_bytes()).await
    }

    /// Sets the radio frequency in corresponding registers
    ///
    /// There might be a loss of precision, so that the actual frequency is slightly off.
    pub async fn frequency(&mut self, frequency: u32) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        let reg = ((frequency as u64 * F_SCALE) / FSTEP) as u32;
        self.write_registers(Register::FrfMsb, &reg.to_be_bytes()[1..]).await
    }

    /// Sets the frequency deviation in corresponding registers
    pub async fn fdev(&mut self, fdev: u32) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        let reg = ((fdev as u64 * F_SCALE) / FSTEP) as u16;
        self.write_registers(Register::FdevMsb, &reg.to_be_bytes()).await
    }

    /// Sets the rx bandwidth in corresponding register
    pub async fn rx_bw<RxBwT>(&mut self, rx_bw: RxBw<RxBwT>) -> Result<(), Error<E, RESET::Error, DIO0::Error>>
    where
        RxBwT: RxBwFreq,
    {
        self.write_register(Register::RxBw, rx_bw.dcc_cutoff as u8 | rx_bw.rx_bw.value())
            .await
    }

    /// Sets preamble length in corresponding registers
    pub async fn preamble_length(&mut self, length: u16) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        self.write_registers(Register::PreambleMsb, &length.to_be_bytes()).await
    }

    /// Sets sync words in corresponding registers
    ///
    /// Maximal sync length is 8, pass empty buffer to clear the sync flag.
    pub async fn sync(&mut self, sync: &[u8]) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        let len = sync.len();
        if len == 0 {
            return self.update_register(Register::SyncConfig, |r| r & 0x7f).await;
        } else if len > 8 {
            return Err(Error::SyncSize);
        }
        let reg = 0x80 | ((len - 1) as u8) << 3;
        self.write_register(Register::SyncConfig, reg).await?;
        self.write_registers(Register::SyncValue1, sync).await
    }

    /// Sets packet settings in corresponding registers
    pub async fn packet(&mut self, packet_config: PacketConfig) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        let len: u8;
        let mut reg = 0x00;
        match packet_config.format {
            PacketFormat::Fixed(size) => len = size,
            PacketFormat::Variable(size) => {
                len = size;
                reg |= 0x80;
            }
        }
        reg |= packet_config.dc as u8 | packet_config.filtering as u8 | (packet_config.crc as u8) << 4;
        self.write_registers(Register::PacketConfig1, &[reg, len]).await?;
        reg = packet_config.interpacket_rx_delay as u8 | (packet_config.auto_rx_restart as u8) << 1;
        self.update_register(Register::PacketConfig2, |r| r & 0x0d | reg).await
    }

    /// Sets fifo mode in corresponding register
    pub async fn fifo_mode(&mut self, mode: FifoMode) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        match mode {
            FifoMode::NotEmpty => self.update_register(Register::FifoThresh, |r| r | 0x80).await,
            FifoMode::Level(level) => self.write_register(Register::FifoThresh, level & 0x7f).await,
        }
    }

    /// Configure lna in corresponding register
    pub async fn lna(&mut self, lna: LnaConfig) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        let reg = (lna.zin as u8) | (lna.gain_select as u8);
        self.update_register(Register::Lna, |r| (r & 0x78) | reg).await
    }

    /// Configure rssi threshold in corresponding register
    pub async fn rssi_threshold(&mut self, threshold: u8) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        self.write_register(Register::RssiThresh, threshold).await
    }

    /// Configure continuous dagc in corresponding register
    pub async fn continuous_dagc(&mut self, cdagc: ContinuousDagc) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        self.write_register(Register::TestDagc, cdagc as u8).await
    }

    /// Return if irq flag ModeReady is set
    pub async fn is_mode_ready(&mut self) -> Result<bool, Error<E, RESET::Error, DIO0::Error>> {
        let reg = self.read_register(Register::IrqFlags1).await?;
        Ok((reg & IrqFlags1::ModeReady) != 0)
    }

    /// Return if irq flag PacketSent is set
    pub async fn is_packet_sent(&mut self) -> Result<bool, Error<E, RESET::Error, DIO0::Error>> {
        let reg = self.read_register(Register::IrqFlags2).await?;
        Ok((reg & IrqFlags2::PacketSent) != 0)
    }

    /// Return if irq flag PacketReady is set
    pub async fn is_packet_ready(&mut self) -> Result<bool, Error<E, RESET::Error, DIO0::Error>> {
        let reg = self.read_register(Register::IrqFlags2).await?;
        Ok(reg & IrqFlags2::PayloadReady != 0)
    }

    async fn reset_fifo(&mut self) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        self.write_register(Register::IrqFlags2, IrqFlags2::FifoOverrun as u8)
            .await
    }

    async fn read_rssi(&mut self) -> Result<i16, Error<E, RESET::Error, DIO0::Error>> {
        let reg = self.read_register(Register::RssiValue).await?;
        Ok(-i16::from(reg) >> 1)
    }

    async fn read_register(&mut self, reg: Register) -> Result<u8, Error<E, RESET::Error, DIO0::Error>> {
        let mut buffer = [reg.addr() & 0x7f, 0];
        self.spi
            .transaction(&mut [Operation::Transfer(&mut buffer, &[reg.addr() & 0x7f])])
            .await
            .map_err(Error::SPI)?;
        Ok(buffer[1])
    }

    async fn write_register(&mut self, reg: Register, byte: u8) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        self.spi
            .write_transaction(&[&[reg.addr() | 0x80, byte]])
            .await
            .map_err(Error::SPI)
    }

    async fn update_register<F>(&mut self, reg: Register, f: F) -> Result<(), Error<E, RESET::Error, DIO0::Error>>
    where
        F: FnOnce(u8) -> u8,
    {
        let val = self.read_register(reg).await?;
        self.write_register(reg, f(val)).await
    }

    async fn write_registers(&mut self, reg: Register, data: &[u8]) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        self.spi
            .write_transaction(&[&[reg.addr() | 0x80], data])
            .await
            .map_err(Error::SPI)
    }

    async fn read_registers(
        &mut self,
        reg: Register,
        data: &mut [u8],
    ) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        self.spi
            .transaction(&mut [Operation::Write(&[reg.addr() & 0x7f]), Operation::Read(data)])
            .await
            .map_err(Error::SPI)
    }

    /// Read content of all registers that are available
    pub async fn read_all_regs(&mut self) -> Result<[u8; 0x4f], Error<E, RESET::Error, DIO0::Error>> {
        let mut buffer = [0u8; 0x4f];
        self.read_registers(Register::OpMode, &mut buffer).await?;
        Ok(buffer)
    }

    /// Send data over the radio
    ///
    /// This async function returns when all data is sent.
    pub async fn send(&mut self, packet: &Packet) -> Result<(), Error<E, RESET::Error, DIO0::Error>> {
        if self.dio0.is_some() {
            // configure dio mapping 00, so PacketSent is on it
            self.write_register(Register::DioMapping1, 0).await?;
        }

        let mode = self.read_register(Register::OpMode).await?;
        log::debug!("OpMode 0x{:02x}", mode);

        self.set_mode(OpMode::Standby).await?;
        self.delay.delay_ms(1).await;

        // ModeReady does not seem to work, if already in that mode
        while !self.is_mode_ready().await? {
            self.delay.delay_ms(500).await;
            let mode = self.read_register(Register::OpMode).await?;
            let irq1 = self.read_register(Register::IrqFlags1).await?;
            let irq2 = self.read_register(Register::IrqFlags2).await?;
            log::info!("OM 0x{:02x} - Irq1 0x{:02x} - Irq2 0x{:02x}", mode, irq1, irq2);
        }

        self.reset_fifo().await?;
        self.delay.delay_ms(1).await;

        let mut raw = [0_u8; 65];
        let len = packet.to_slice(&mut raw).map_err(|_| Error::WrongPacketFormat)?;
        self.write_registers(Register::Fifo, &raw[..len as usize]).await?;

        self.set_mode(OpMode::Tx).await?;

        if let Some(dio0) = &mut self.dio0 {
            dio0.wait_for_high().await.map_err(Error::DIO0)?;
        } else {
            while !self.is_packet_sent().await? {
                //Timer::after(Duration::from_micros(500_u64)).await;
            }
        }
        log::debug!("Packet Sent");

        self.set_mode(OpMode::Standby).await
    }

    /// Receive data over the radio
    ///
    /// This async function returns once a complete packet is received.
    pub async fn recv(&mut self) -> Result<Packet, Error<E, RESET::Error, DIO0::Error>> {
        if self.dio0.is_some() {
            // configure dio0 mapping 01, so PayloadReady is on it
            self.write_register(Register::DioMapping1, 0x40).await?;
        }

        self.set_mode(OpMode::Rx).await?;

        if let Some(dio0) = &mut self.dio0 {
            dio0.wait_for_high().await.map_err(Error::DIO0)?;
        } else {
            while !self.is_packet_ready().await? {
                self.delay.delay_us(500).await;
            }
        }

        self.set_mode(OpMode::Standby).await?;

        // First byte in fifo is length, because af variable packet length.
        let len = self.read_register(Register::Fifo).await?;
        let mut buffer = [0; 64];
        self.read_registers(Register::Fifo, &mut buffer[..len as usize]).await?;
        let rssi = self.read_rssi().await?;

        let packet = Packet::from_rx_data(len, &buffer, rssi).map_err(|_| Error::WrongPacketFormat)?;

        log::debug!("Rx: Rssi {}; Len {}", rssi, len);

        Ok(packet)
    }
}

impl<SPI, RESET, DIO0, DELAY, E> Transceiver for Rfm69<SPI, RESET, DIO0, DELAY>
where
    SPI: SpiDevice<u8, Error = E>,
    RESET: OutputPin,
    DIO0: InputPin + Wait,
    DELAY: DelayUs,
{
    async fn send(&mut self, packet: &Packet) -> Result<(), crate::TrxError> {
        Rfm69::send(self, packet).await.map_err(|e| e.into())
    }

    async fn recv(&mut self) -> Result<Packet, crate::TrxError> {
        Rfm69::recv(self).await.map_err(|e| e.into())
    }
}
