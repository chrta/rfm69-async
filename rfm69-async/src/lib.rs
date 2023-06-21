#![no_std]
#![feature(type_alias_impl_trait)]
#![feature(async_fn_in_trait)]

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
pub use stack::Stack;
pub use traits::{Transceiver, TrxError};
