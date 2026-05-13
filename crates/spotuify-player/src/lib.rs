//! Player backend abstraction for spotuify.
//!
//! Phase 9 will land:
//! - `PlayerBackend` trait
//! - `EmbeddedBackend` (in-process librespot)
//! - `ConnectOnlyBackend` (Web API transfer only)
//!
//! The `spotifyd` backend has moved here from the binary's src/spotifyd.rs
//! as a leaf extraction. The Phase 9 backend trait will subsume it.

pub mod backends;

pub use backends::spotifyd;
