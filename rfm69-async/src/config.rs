// SPDX-License-Identifier: AGPL-3.0-only

//! Configurations that are available to initialize the rfm69

use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::digital::Wait;
use embedded_hal_async::spi::SpiDevice;

use crate::error::Error;
use crate::registers::*;
use crate::rfm::Rfm69;

/// Configuration compatible with Low Power Lab radio protocol
///
/// See `<https://github.com/LowPowerLab/RFM69>`
///
/// Note: This configuration is not tested/used.
pub async fn low_power_lab_defaults<SPI, RESET, DIO0, DELAY, E>(
    rfm: &mut Rfm69<SPI, RESET, DIO0, DELAY>,
    network_id: u8,
    frequency: u32,
) -> Result<(), Error<E, RESET::Error, DIO0::Error>>
where
    SPI: SpiDevice<u8, Error = E>,
    RESET: OutputPin,
    DIO0: InputPin + Wait,
    DELAY: DelayNs,
{
    // Plain FSK (no shaping), 55555 bps, 200 Ω LNA input.
    apply(
        rfm,
        network_id,
        frequency,
        ModulationShaping::Shaping00,
        55_555,
        LnaImpedance::Ohm200,
    )
    .await
}

/// Custom configuration (gfsk, 100kBit/sec)
///
/// This uses gfsk to reduce the used bandwidth and a 100kBit/sec data rate.
/// Otherwise it is similar to the Low Power Lab configuration.
pub async fn my_defaults<SPI, RESET, DIO0, DELAY, E>(
    rfm: &mut Rfm69<SPI, RESET, DIO0, DELAY>,
    network_id: u8,
    frequency: u32,
) -> Result<(), Error<E, RESET::Error, DIO0::Error>>
where
    SPI: SpiDevice<u8, Error = E>,
    RESET: OutputPin,
    DIO0: InputPin + Wait,
    DELAY: DelayNs,
{
    // GFSK with BT = 0.5, 100 kbps, 50 Ω LNA input.
    apply(
        rfm,
        network_id,
        frequency,
        ModulationShaping::Shaping10,
        100_000,
        LnaImpedance::Ohm50,
    )
    .await
}

/// Shared register recipe behind the public presets. They differ only in
/// modulation `shaping`, `bit_rate`, and the LNA input impedance `lna_zin`;
/// every other register is identical, so the common sequence lives here once.
async fn apply<SPI, RESET, DIO0, DELAY, E>(
    rfm: &mut Rfm69<SPI, RESET, DIO0, DELAY>,
    network_id: u8,
    frequency: u32,
    shaping: ModulationShaping,
    bit_rate: u32,
    lna_zin: LnaImpedance,
) -> Result<(), Error<E, RESET::Error, DIO0::Error>>
where
    SPI: SpiDevice<u8, Error = E>,
    RESET: OutputPin,
    DIO0: InputPin + Wait,
    DELAY: DelayNs,
{
    rfm.reset().await?;
    rfm.set_mode(OpMode::Standby).await?;
    rfm.modulation(Modulation {
        data_mode: DataMode::Packet,
        modulation_type: ModulationType::Fsk,
        shaping,
    })
    .await?;
    rfm.bit_rate(bit_rate).await?;
    rfm.fdev(50_000).await?;
    rfm.rx_bw(RxBw {
        dcc_cutoff: DccCutoff::Percent4,
        rx_bw: RxBwFsk::Khz125dot0,
    })
    .await?;
    rfm.preamble_length(3).await?;
    rfm.sync(&[0x2d, network_id]).await?;
    rfm.packet(PacketConfig {
        format: PacketFormat::Variable(66),
        dc: PacketDc::None,
        filtering: PacketFiltering::None,
        crc: true,
        interpacket_rx_delay: InterPacketRxDelay::Delay2Bits,
        auto_rx_restart: true,
    })
    .await?;
    rfm.fifo_mode(FifoMode::NotEmpty).await?;
    rfm.lna(LnaConfig {
        zin: lna_zin,
        gain_select: LnaGain::AgcLoop,
    })
    .await?;
    rfm.rssi_threshold(220).await?;
    rfm.frequency(frequency).await?;
    rfm.continuous_dagc(ContinuousDagc::ImprovedMarginAfcLowBetaOn0).await?;
    rfm.set_mode(OpMode::Sleep).await?;
    Ok(())
}
