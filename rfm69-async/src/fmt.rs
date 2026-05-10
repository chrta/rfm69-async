// SPDX-License-Identifier: AGPL-3.0-only

// Internal logging macros that route to `log` and/or `defmt` based on cargo
// features. If neither feature is enabled, the macros expand to no-ops while
// still consuming their arguments to avoid unused-variable warnings.
//
// Convention follows the embassy-* crates: both backends can be enabled
// simultaneously (each call fires through every enabled backend).

#![allow(unused_macros)]

macro_rules! debug {
    ($s:literal $(, $x:expr_2021)* $(,)?) => {{
        #[cfg(feature = "defmt")]
        ::defmt::debug!($s $(, $x)*);
        #[cfg(feature = "log")]
        ::log::debug!($s $(, $x)*);
        #[cfg(not(any(feature = "defmt", feature = "log")))]
        let _ = ($( & $x ),*);
    }};
}

macro_rules! info {
    ($s:literal $(, $x:expr_2021)* $(,)?) => {{
        #[cfg(feature = "defmt")]
        ::defmt::info!($s $(, $x)*);
        #[cfg(feature = "log")]
        ::log::info!($s $(, $x)*);
        #[cfg(not(any(feature = "defmt", feature = "log")))]
        let _ = ($( & $x ),*);
    }};
}

macro_rules! warn {
    ($s:literal $(, $x:expr_2021)* $(,)?) => {{
        #[cfg(feature = "defmt")]
        ::defmt::warn!($s $(, $x)*);
        #[cfg(feature = "log")]
        ::log::warn!($s $(, $x)*);
        #[cfg(not(any(feature = "defmt", feature = "log")))]
        let _ = ($( & $x ),*);
    }};
}

macro_rules! error {
    ($s:literal $(, $x:expr_2021)* $(,)?) => {{
        #[cfg(feature = "defmt")]
        ::defmt::error!($s $(, $x)*);
        #[cfg(feature = "log")]
        ::log::error!($s $(, $x)*);
        #[cfg(not(any(feature = "defmt", feature = "log")))]
        let _ = ($( & $x ),*);
    }};
}
