//! Ariel OS is an operating system for secure, memory-safe, low-power Internet of Things (IoT).
//!
//! See the [README](https://github.com/ariel-os/ariel-os) for more details.
//!
//! # Examples
//!
//! Application examples can be found in the [`examples` directory](https://github.com/ariel-os/ariel-os/tree/main/examples).
//!
//! # Cargo features
#![doc = document_features::document_features!(feature_label = r#"<span class="stab portability"><code>{feature}</code></span>"#)]
#![no_std]
#![feature(doc_auto_cfg)]

pub mod buildinfo;

#[cfg(feature = "bench")]
#[doc(inline)]
pub use ariel_os_bench as bench;
#[doc(inline)]
pub use ariel_os_debug as debug;
#[cfg(feature = "random")]
#[doc(inline)]
pub use ariel_os_random as random;
#[doc(inline)]
pub use ariel_os_rt as rt;
#[cfg(feature = "threading")]
#[doc(inline)]
pub use ariel_os_threads as thread;

// Attribute macros
pub use ariel_os_macros::config;
pub use ariel_os_macros::spawner;
pub use ariel_os_macros::task;
#[cfg(any(feature = "threading", doc))]
pub use ariel_os_macros::thread;

// ensure this gets linked
use ariel_os_boards as _;

pub use ariel_os_embassy::api::*;
pub use static_cell::{ConstStaticCell, StaticCell};

/// This module contains all third party crates as used by Ariel OS.
///
/// TODO: The version of this crate (`ariel-os`) will correspond to changes in
/// these dependencies (keeping semver guarantees).
pub mod reexports {
    pub use ariel_os_embassy::reexports::*;
    // These are used by proc-macros we provide
    pub use linkme;
    pub use static_cell;
}

#[cfg(all(feature = "single-core", feature = "multi-core"))]
compile_error!(
    "feature \"single-core\" and feature \"multi-core\" cannot be enabled at the same time"
);
