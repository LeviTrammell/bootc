//! Controller profiles — one per supported variant.
//!
//! Exactly one profile feature must be enabled at build time. The active
//! profile is re-exported as [`ActiveProfile`] so `main.rs` is profile-free.

#[cfg(all(feature = "ogu", feature = "htpc"))]
compile_error!("niri-nav: cannot enable both 'ogu' and 'htpc' features simultaneously");

#[cfg(not(any(feature = "ogu", feature = "htpc")))]
compile_error!("niri-nav: must enable exactly one of 'ogu' or 'htpc' features");

#[cfg(feature = "ogu")]
pub mod ogu;
#[cfg(feature = "htpc")]
pub mod htpc;

#[cfg(feature = "ogu")]
pub use ogu::OguProfile as ActiveProfile;

#[cfg(feature = "htpc")]
pub use htpc::HtpcProfile as ActiveProfile;
