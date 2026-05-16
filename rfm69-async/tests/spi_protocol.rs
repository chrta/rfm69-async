// SPDX-License-Identifier: AGPL-3.0-only

// The constants block below mirrors the RFM69 register table from the
// datasheet. Some entries aren't referenced by today's tests but are kept
// as documentation; silence the dead-code lint for the whole binary
// rather than deleting them when a test stops using one.
#![allow(dead_code)]

//! Wire-level register-sequence regression tests for [`Rfm69`].
//!
//! `tests/common::MockTrx` mocks at the [`Transceiver`] boundary, which is
//! the right level for arbitration/MAC tests but bypasses every byte that
//! actually goes on the SPI bus. These tests instead drive a real
//! [`Rfm69`] against `embedded-hal-mock` peripherals (SPI / pins /
//! delay), so a typo in an `Operation::Write([addr | 0x80, byte])`, an
//! inverted bitmask, or a wrong scaling constant fails loudly.
//!
//! The `rd` / `wr` / `wr_n` / `rd_n` helpers emit the
//! `transaction_start … transaction_end` framing per
//! `embedded_hal_async::spi::SpiDevice::transaction`; tests build a flat
//! expectation `Vec` by chaining helpers in source order. Mocks for
//! reset / DIO0 / delay carry their own expectation lists; every test
//! ends in `done()` on each mock so any unconsumed expectation panics.

use embedded_hal_mock::eh1::delay::CheckedDelay;
use embedded_hal_mock::eh1::delay::Transaction as DelayTransaction;
use embedded_hal_mock::eh1::digital::Edge as PinEdge;
use embedded_hal_mock::eh1::digital::Mock as PinMock;
use embedded_hal_mock::eh1::digital::State as PinState;
use embedded_hal_mock::eh1::digital::Transaction as PinTransaction;
use embedded_hal_mock::eh1::spi::Mock as SpiMock;
use embedded_hal_mock::eh1::spi::Transaction as SpiTransaction;
use futures::executor::block_on;
use rfm69_async::registers::{
    ContinuousDagc, DataMode, DccCutoff, FifoMode, InterPacketRxDelay, LnaConfig, LnaGain, LnaImpedance, Modulation,
    ModulationShaping, ModulationType, OpMode, PacketConfig, PacketDc, PacketFiltering, PacketFormat, RxBw, RxBwFsk,
};
use rfm69_async::{Address, Error, Flags, Packet, Rfm69};

// Numeric register addresses. Duplicated as constants here rather than
// re-imported from `crate::registers::Register` so the tests double-check
// the address bytes the driver actually puts on the wire — re-importing
// the enum would make the test tautological.
const REG_FIFO: u8 = 0x00;
const REG_OPMODE: u8 = 0x01;
const REG_DATA_MODUL: u8 = 0x02;
const REG_BITRATE_MSB: u8 = 0x03;
const REG_FDEV_MSB: u8 = 0x05;
const REG_FRF_MSB: u8 = 0x07;
const REG_VERSION: u8 = 0x10;
const REG_LNA: u8 = 0x18;
const REG_RX_BW: u8 = 0x19;
const REG_RSSI_VALUE: u8 = 0x24;
const REG_DIO_MAPPING1: u8 = 0x25;
const REG_IRQ_FLAGS1: u8 = 0x27;
const REG_IRQ_FLAGS2: u8 = 0x28;
const REG_RSSI_THRESH: u8 = 0x29;
const REG_PREAMBLE_MSB: u8 = 0x2C;
const REG_SYNC_CONFIG: u8 = 0x2E;
const REG_SYNC_VALUE1: u8 = 0x2F;
const REG_PACKET_CONFIG_1: u8 = 0x37;
const REG_FIFO_THRESH: u8 = 0x3C;
const REG_PACKET_CONFIG_2: u8 = 0x3D;
const REG_TEST_DAGC: u8 = 0x6F;

// OpMode register values.
const OPMODE_SLEEP: u8 = 0x00;
const OPMODE_STANDBY: u8 = 0x04;
const OPMODE_TX: u8 = 0x0c;
const OPMODE_RX: u8 = 0x10;

// IrqFlags1 / IrqFlags2 bits the driver checks.
const IRQ1_MODE_READY: u8 = 0x80;
const IRQ2_FIFO_OVERRUN: u8 = 0x10;
const IRQ2_PACKET_SENT: u8 = 0x08;
const IRQ2_PAYLOAD_READY: u8 = 0x04;

// Expected version byte from a healthy RFM69.
const VERSION_OK: u8 = 0x24;

// DIO0 mappings the driver writes per operation.
const DIO0_PACKET_SENT: u8 = 0x00; // before send
const DIO0_PAYLOAD_READY: u8 = 0x40; // before recv

// --- Helpers: each returns a complete framed SpiDevice transaction. -----

/// Read one register: `Operation::Transfer(write=[addr & 0x7f], read=[?, ret])`.
/// embedded-hal-mock's `transfer(write_vec, read_vec)` matches lengths
/// independently, so the asymmetric Transfer (1-byte write / 2-byte read)
/// the driver issues maps cleanly onto a 1-byte write / 2-byte response
/// expectation.
fn rd(addr: u8, ret: u8) -> Vec<SpiTransaction<u8>> {
    vec![
        SpiTransaction::transaction_start(),
        SpiTransaction::transfer(vec![addr & 0x7f], vec![0, ret]),
        SpiTransaction::transaction_end(),
    ]
}

/// Write one register byte: a single 2-byte `Operation::Write
/// [addr | 0x80, byte]`.
fn wr(addr: u8, byte: u8) -> Vec<SpiTransaction<u8>> {
    vec![
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![addr | 0x80, byte]),
        SpiTransaction::transaction_end(),
    ]
}

/// Write multiple register bytes: two back-to-back `Operation::Write`s
/// inside one transaction — address byte first, then the data buffer.
fn wr_n(addr: u8, bytes: &[u8]) -> Vec<SpiTransaction<u8>> {
    vec![
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![addr | 0x80]),
        SpiTransaction::write_vec(bytes.to_vec()),
        SpiTransaction::transaction_end(),
    ]
}

/// Read multiple register bytes: write the address (high bit clear), then
/// read `returns.len()` bytes filled from `returns`.
fn rd_n(addr: u8, returns: &[u8]) -> Vec<SpiTransaction<u8>> {
    vec![
        SpiTransaction::transaction_start(),
        SpiTransaction::write_vec(vec![addr & 0x7f]),
        SpiTransaction::read_vec(returns.to_vec()),
        SpiTransaction::transaction_end(),
    ]
}

// --- Tests ---------------------------------------------------------------

#[test]
fn reset_completes_with_known_version() {
    let mut spi_x = Vec::new();
    spi_x.extend(rd(REG_VERSION, VERSION_OK));
    spi_x.extend(wr(REG_OPMODE, OPMODE_SLEEP));

    let reset_x = vec![PinTransaction::set(PinState::High), PinTransaction::set(PinState::Low)];
    let delay_x = vec![
        DelayTransaction::async_delay_ms(10),
        DelayTransaction::async_delay_ms(10),
    ];

    let mut spi = SpiMock::new(&spi_x);
    let mut reset = PinMock::new(&reset_x);
    let mut delay = CheckedDelay::new(&delay_x);
    let dio0: Option<PinMock> = None;
    let mut rfm = Rfm69::new(spi.clone(), reset.clone(), dio0, delay.clone());

    block_on(rfm.reset()).unwrap();

    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn reset_returns_version_mismatch_on_unknown_chip() {
    let spi_x = rd(REG_VERSION, 0x12);

    let reset_x = vec![PinTransaction::set(PinState::High), PinTransaction::set(PinState::Low)];
    let delay_x = vec![
        DelayTransaction::async_delay_ms(10),
        DelayTransaction::async_delay_ms(10),
    ];

    let mut spi = SpiMock::new(&spi_x);
    let mut reset = PinMock::new(&reset_x);
    let mut delay = CheckedDelay::new(&delay_x);
    let dio0: Option<PinMock> = None;
    let mut rfm = Rfm69::new(spi.clone(), reset.clone(), dio0, delay.clone());

    let res = block_on(rfm.reset());
    assert!(matches!(res, Err(Error::VersionMismatch(0x12))));

    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn read_register_uses_addr_with_high_bit_clear() {
    // `is_packet_ready()` reads IrqFlags2 and checks the PayloadReady bit;
    // exercise the read-shape via a publicly reachable method.
    let spi_x = rd(REG_IRQ_FLAGS2, IRQ2_PAYLOAD_READY);

    let mut spi = SpiMock::new(&spi_x);
    let mut reset = PinMock::new(&[]);
    let mut delay = CheckedDelay::new(&[]);
    let dio0: Option<PinMock> = None;
    let mut rfm = Rfm69::new(spi.clone(), reset.clone(), dio0, delay.clone());

    assert!(block_on(rfm.is_packet_ready()).unwrap());

    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn frequency_writes_three_scaled_bytes() {
    // FOSC = 32 MHz; FSTEP = FOSC / 2^19 ≈ 61.035 Hz. For 433 MHz exactly,
    // reg = 433_000_000 / FSTEP = 7094272 = 0x6C4000. The driver writes the
    // top 3 bytes of the 4-byte u32 big-endian rep, i.e. [0x6C, 0x40, 0x00].
    let spi_x = wr_n(REG_FRF_MSB, &[0x6C, 0x40, 0x00]);

    let mut spi = SpiMock::new(&spi_x);
    let mut reset = PinMock::new(&[]);
    let mut delay = CheckedDelay::new(&[]);
    let dio0: Option<PinMock> = None;
    let mut rfm = Rfm69::new(spi.clone(), reset.clone(), dio0, delay.clone());

    block_on(rfm.frequency(433_000_000)).unwrap();

    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn send_with_dio0_drives_full_protocol() {
    // Wire-format packet [fifo_len, src, dst, flags, payload...]:
    // src=1, dst=2, flags=None(0), payload=[0xAB, 0xCD]
    // -> fifo_len = 3 header + 2 payload = 5.
    let pkt = Packet::new(Address::Unicast(1), Address::Unicast(2), Flags::None, &[0xAB, 0xCD]).unwrap();
    let fifo_bytes: &[u8] = &[5, 1, 2, 0, 0xAB, 0xCD];

    let mut spi_x = Vec::new();
    // 1. DIO mapping: PacketSent on DIO0.
    spi_x.extend(wr(REG_DIO_MAPPING1, DIO0_PACKET_SENT));
    // 2. Read OpMode (debug log; mock returns Standby — actual value
    //    unimportant, the driver doesn't act on it).
    spi_x.extend(rd(REG_OPMODE, OPMODE_STANDBY));
    // 3. Standby.
    spi_x.extend(wr(REG_OPMODE, OPMODE_STANDBY));
    // 4. ModeReady poll: returns ModeReady on first read so the loop
    //    body runs once.
    spi_x.extend(rd(REG_IRQ_FLAGS1, IRQ1_MODE_READY));
    // 5. Reset FIFO (write FifoOverrun bit clears it).
    spi_x.extend(wr(REG_IRQ_FLAGS2, IRQ2_FIFO_OVERRUN));
    // 6. FIFO write — length byte + src/dst/flags + payload.
    spi_x.extend(wr_n(REG_FIFO, fifo_bytes));
    // 7. Tx mode.
    spi_x.extend(wr(REG_OPMODE, OPMODE_TX));
    // 8. (DIO0 wait_for_high — pin mock, not SPI.)
    // 9. Standby on completion.
    spi_x.extend(wr(REG_OPMODE, OPMODE_STANDBY));

    let dio0_x = vec![PinTransaction::wait_for_state(PinState::High)];
    let delay_x = vec![
        DelayTransaction::async_delay_ms(1), // post-Standby settle
        DelayTransaction::async_delay_ms(1), // post-FIFO-reset settle
    ];

    let mut spi = SpiMock::new(&spi_x);
    let mut reset = PinMock::new(&[]);
    let mut dio0 = PinMock::new(&dio0_x);
    let mut delay = CheckedDelay::new(&delay_x);
    let mut rfm = Rfm69::new(spi.clone(), reset.clone(), Some(dio0.clone()), delay.clone());

    block_on(rfm.send(&pkt)).unwrap();

    spi.done();
    reset.done();
    dio0.done();
    delay.done();
}

#[test]
fn recv_with_dio0_reads_fifo_and_rssi() {
    // The FIFO content the driver will read out: [src=2, dst=1, flags=None,
    // payload=0xDE,0xAD]. fifo_len excludes the length byte and is 5
    // (3 header + 2 payload).
    let fifo_len: u8 = 5;
    let fifo_payload = [2u8, 1, 0, 0xDE, 0xAD];
    // Driver buffer for FIFO data is 64 bytes — read_registers fills
    // exactly fifo_len bytes via the slice, so the mock response Vec is
    // fifo_len long.
    let rssi_raw = 100u8; // dBm = -50

    let mut spi_x = Vec::new();
    // 1. DIO mapping: PayloadReady on DIO0.
    spi_x.extend(wr(REG_DIO_MAPPING1, DIO0_PAYLOAD_READY));
    // 2. Rx mode.
    spi_x.extend(wr(REG_OPMODE, OPMODE_RX));
    // 3. (DIO0 wait_for_rising_edge.)
    // 4. Standby.
    spi_x.extend(wr(REG_OPMODE, OPMODE_STANDBY));
    // 5. Read FIFO length byte.
    spi_x.extend(rd(REG_FIFO, fifo_len));
    // 6. Read FIFO payload (fifo_len bytes from the FIFO addr).
    spi_x.extend(rd_n(REG_FIFO, &fifo_payload));
    // 7. Read RSSI register.
    spi_x.extend(rd(REG_RSSI_VALUE, rssi_raw));

    let dio0_x = vec![PinTransaction::wait_for_edge(PinEdge::Rising)];

    let mut spi = SpiMock::new(&spi_x);
    let mut reset = PinMock::new(&[]);
    let mut dio0 = PinMock::new(&dio0_x);
    let mut delay = CheckedDelay::new(&[]);
    let mut rfm = Rfm69::new(spi.clone(), reset.clone(), Some(dio0.clone()), delay.clone());

    let pkt = block_on(rfm.recv()).unwrap();
    assert_eq!(pkt.src, Address::Unicast(2));
    assert_eq!(pkt.dst, Address::Unicast(1));
    assert!(matches!(pkt.flags, Flags::None));
    assert_eq!(pkt.data.as_slice(), &[0xDE, 0xAD]);
    assert_eq!(pkt.rssi, Some(-(i16::from(rssi_raw)) >> 1));

    spi.done();
    reset.done();
    dio0.done();
    delay.done();
}

// --- Tier 2: per-setter wire-format coverage ----------------------------
//
// Each test below locks the exact byte sequence one of the public setters
// puts on the SPI bus. The expected values are derived by hand from the
// datasheet and the type definitions in `rfm69_async::registers` — not
// re-imported from those types — so a wrong bit composition in the driver
// surfaces as a mismatch rather than passing tautologically.
//
// Shared helper for "construct a barebones Rfm69 wired to mocks, run the
// closure, drain everything". Each test still spells out its own
// expectation vec so the matching call is local and obvious.

type SetterFixture = (
    Rfm69<SpiMock<u8>, PinMock, PinMock, CheckedDelay>,
    SpiMock<u8>,
    PinMock,
    CheckedDelay,
);

/// Returns `(rfm, spi, reset, delay)` — the caller must `.done()` all three
/// mocks at the end of the test. (DIO0 is `None` since these setters don't
/// touch the pin.) reset and delay come back with empty expectation lists,
/// but still need `done()` so an unexpected GPIO toggle or delay would be
/// caught by the SpiMock's strict matcher behaviour.
fn rfm_with_spi(spi_x: &[SpiTransaction<u8>]) -> SetterFixture {
    let spi = SpiMock::new(spi_x);
    let reset = PinMock::new(&[]);
    let delay = CheckedDelay::new(&[]);
    let dio0: Option<PinMock> = None;
    let rfm = Rfm69::new(spi.clone(), reset.clone(), dio0, delay.clone());
    (rfm, spi, reset, delay)
}

#[test]
fn set_mode_writes_opmode_register() {
    // Standby = 0x04; the setter is a single-byte write.
    let spi_x = wr(REG_OPMODE, 0x04);

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.set_mode(OpMode::Standby)).unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn modulation_composes_three_fields() {
    // Packet (0x00) | Fsk (0x00) | Shaping10 BT=0.5 (0x02) = 0x02.
    let spi_x = wr(REG_DATA_MODUL, 0x02);

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.modulation(Modulation {
        data_mode: DataMode::Packet,
        modulation_type: ModulationType::Fsk,
        shaping: ModulationShaping::Shaping10,
    }))
    .unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn bit_rate_writes_two_scaled_bytes() {
    // FOSC = 32 MHz * F_SCALE; reg = FOSC / (bit_rate * F_SCALE).
    // For 100_000 bps: 32_000_000_000_000 / 100_000_000_000 = 320 = 0x0140.
    let spi_x = wr_n(REG_BITRATE_MSB, &[0x01, 0x40]);

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.bit_rate(100_000)).unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn fdev_writes_two_scaled_bytes() {
    // reg = (fdev * F_SCALE) / FSTEP. FSTEP = FOSC / 2^19, integer
    // division: FSTEP = 61_035_156. For 50_000 Hz:
    // 50_000_000_000 / 61_035_156 = 819 (truncating) = 0x0333.
    let spi_x = wr_n(REG_FDEV_MSB, &[0x03, 0x33]);

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.fdev(50_000)).unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn rx_bw_composes_dcc_cutoff_and_filter() {
    // Percent4 (0x40) | Khz125dot0 (encoded as 2) = 0x42.
    let spi_x = wr(REG_RX_BW, 0x42);

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.rx_bw(RxBw {
        dcc_cutoff: DccCutoff::Percent4,
        rx_bw: RxBwFsk::Khz125dot0,
    }))
    .unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn preamble_length_writes_two_be_bytes() {
    // u16 big-endian: 3 → [0x00, 0x03].
    let spi_x = wr_n(REG_PREAMBLE_MSB, &[0x00, 0x03]);

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.preamble_length(3)).unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn sync_normal_writes_config_byte_plus_payload() {
    // len=2 → config byte = 0x80 | ((2-1) << 3) = 0x88; payload at SyncValue1.
    let mut spi_x = Vec::new();
    spi_x.extend(wr(REG_SYNC_CONFIG, 0x88));
    spi_x.extend(wr_n(REG_SYNC_VALUE1, &[0x2D, 0x42]));

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.sync(&[0x2D, 0x42])).unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn sync_empty_clears_config_bit() {
    // Empty slice → update_register(SyncConfig, r & 0x7F). Mock returns
    // 0xFF; expect 0x7F written back.
    let mut spi_x = Vec::new();
    spi_x.extend(rd(REG_SYNC_CONFIG, 0xFF));
    spi_x.extend(wr(REG_SYNC_CONFIG, 0x7F));

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.sync(&[])).unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn sync_too_long_returns_error_without_spi() {
    // 9 bytes > max sync length 8. The setter rejects before any SPI op.
    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&[]);
    let res = block_on(rfm.sync(&[0u8; 9]));
    assert!(matches!(res, Err(Error::SyncSize)));
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn packet_writes_config1_then_updates_config2() {
    // PacketConfig1 byte:
    //   format=Variable(66) -> reg |= 0x80, len=66
    //   crc=true            -> reg |= 0x10
    //   dc=None, filtering=None -> +0
    //   total = 0x90; len byte follows.
    // PacketConfig2 (update_register): mock returns 0x00; new value =
    //   (0x00 & 0x0D) | (Delay2Bits=0x10 | (auto_rx_restart=true as u8) << 1)
    //   = 0 | 0x12 = 0x12.
    let mut spi_x = Vec::new();
    spi_x.extend(wr_n(REG_PACKET_CONFIG_1, &[0x90, 66]));
    spi_x.extend(rd(REG_PACKET_CONFIG_2, 0x00));
    spi_x.extend(wr(REG_PACKET_CONFIG_2, 0x12));

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.packet(PacketConfig {
        format: PacketFormat::Variable(66),
        dc: PacketDc::None,
        filtering: PacketFiltering::None,
        crc: true,
        interpacket_rx_delay: InterPacketRxDelay::Delay2Bits,
        auto_rx_restart: true,
    }))
    .unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn fifo_mode_not_empty_sets_high_bit() {
    // update_register(FifoThresh, r | 0x80). Mock returns 0x0F; expect
    // 0x8F written back.
    let mut spi_x = Vec::new();
    spi_x.extend(rd(REG_FIFO_THRESH, 0x0F));
    spi_x.extend(wr(REG_FIFO_THRESH, 0x8F));

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.fifo_mode(FifoMode::NotEmpty)).unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn fifo_mode_level_writes_masked_threshold() {
    // FifoMode::Level(level) -> write_register(FifoThresh, level & 0x7F).
    // 0xFF & 0x7F = 0x7F: the high bit is unconditionally cleared.
    let spi_x = wr(REG_FIFO_THRESH, 0x7F);

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.fifo_mode(FifoMode::Level(0xFF))).unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn lna_preserves_reserved_bits() {
    // update_register(Lna, (r & 0x78) | reg). Reg = Ohm200 (0x80) | AgcLoop
    // (0). Mock returns 0xFF; expect (0xFF & 0x78) | 0x80 = 0x78 | 0x80
    // = 0xF8 written back, proving the mask preserves bits 6:3.
    let mut spi_x = Vec::new();
    spi_x.extend(rd(REG_LNA, 0xFF));
    spi_x.extend(wr(REG_LNA, 0xF8));

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.lna(LnaConfig {
        zin: LnaImpedance::Ohm200,
        gain_select: LnaGain::AgcLoop,
    }))
    .unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn rssi_threshold_writes_value_directly() {
    // 220 = 0xDC, single-byte write, no scaling.
    let spi_x = wr(REG_RSSI_THRESH, 0xDC);

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.rssi_threshold(220)).unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn continuous_dagc_writes_test_dagc_register() {
    // ImprovedMarginAfcLowBetaOn0 = 0x30, single-byte write to TestDagc (0x6F).
    let spi_x = wr(REG_TEST_DAGC, 0x30);

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    block_on(rfm.continuous_dagc(ContinuousDagc::ImprovedMarginAfcLowBetaOn0)).unwrap();
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn is_mode_ready_reads_irq_flags1_mode_ready_bit() {
    // Mock returns 0x80 (ModeReady set); the helper should report true.
    let spi_x = rd(REG_IRQ_FLAGS1, IRQ1_MODE_READY);

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    assert!(block_on(rfm.is_mode_ready()).unwrap());
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn is_packet_sent_reads_irq_flags2_packet_sent_bit() {
    // Mock returns IRQ2_PACKET_SENT; the helper should report true.
    let spi_x = rd(REG_IRQ_FLAGS2, IRQ2_PACKET_SENT);

    let (mut rfm, mut spi, mut reset, mut delay) = rfm_with_spi(&spi_x);
    assert!(block_on(rfm.is_packet_sent()).unwrap());
    spi.done();
    reset.done();
    delay.done();
}

#[test]
fn recv_without_dio0_polls_irq_flags2() {
    // No DIO0 -> the driver loops on `is_packet_ready()` reads. Make the
    // first IrqFlags2 read return PayloadReady so the loop exits in one
    // iteration with no delay consumed.
    let fifo_len: u8 = 3;
    let fifo_payload = [3u8, 1, 0]; // header-only packet src=3 dst=1
    let rssi_raw = 60u8;

    let mut spi_x = Vec::new();
    // No DIO mapping write (the driver only does that when DIO0 is Some).
    spi_x.extend(wr(REG_OPMODE, OPMODE_RX));
    spi_x.extend(rd(REG_IRQ_FLAGS2, IRQ2_PAYLOAD_READY));
    spi_x.extend(wr(REG_OPMODE, OPMODE_STANDBY));
    spi_x.extend(rd(REG_FIFO, fifo_len));
    spi_x.extend(rd_n(REG_FIFO, &fifo_payload));
    spi_x.extend(rd(REG_RSSI_VALUE, rssi_raw));

    let mut spi = SpiMock::new(&spi_x);
    let mut reset = PinMock::new(&[]);
    let mut delay = CheckedDelay::new(&[]);
    let dio0: Option<PinMock> = None;
    let mut rfm = Rfm69::new(spi.clone(), reset.clone(), dio0, delay.clone());

    let pkt = block_on(rfm.recv()).unwrap();
    assert_eq!(pkt.src, Address::Unicast(3));
    assert_eq!(pkt.dst, Address::Unicast(1));
    assert_eq!(pkt.data.len(), 0);
    assert_eq!(pkt.rssi, Some(-(i16::from(rssi_raw)) >> 1));

    spi.done();
    reset.done();
    delay.done();
}
