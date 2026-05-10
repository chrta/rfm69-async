// SPDX-License-Identifier: AGPL-3.0-only

#![no_std]

#[macro_use]
mod fmt;

mod address;
pub mod config;
mod error;
mod flags;
mod packet;
mod registers;
mod rfm;
mod traits;

#[cfg(feature = "embassy")]
mod stack;

pub use address::Address;
pub use error::Error;
pub use flags::Flags;
pub use packet::Packet;
pub use rfm::Rfm69;
#[cfg(feature = "embassy")]
pub use stack::{LINK_DOWN_STREAK, LinkState, MacTiming, Runner, Stack, StackResources, TxError};
pub use traits::{Transceiver, TrxError};
