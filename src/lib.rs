/// A pure Rust library designed to be equivalent to libimobiledevice.
/// It's built on top of Tokio's runtime for asynchronous I/O.

/// Manages the connection to the muxer
pub mod muxer;

/// Abstraction of iOS's lockdown daemon
pub mod lockdown;

/// Abstraction of a connection to an iOS device
pub mod connection;

pub mod pairing_file;
