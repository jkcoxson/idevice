//! XCTest service client for iOS instruments protocol.
//!
//! This module provides orchestration for running XCTest bundles (including
//! WebDriverAgent) on iOS devices through the instruments and testmanagerd
//! protocols. It handles session setup, test runner launch, and lifecycle
//! event dispatch.
//!
//! Supports iOS 11+ via lockdown and iOS 17+ via RSD tunnel.
//!
//! # Example
//! ```rust,no_run
//! # #[cfg(feature = "xctest")]
//! # {
//! use idevice::services::dvt::xctest::{TestConfig, XCUITestService};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), idevice::IdeviceError> {
//!     // provider setup omitted
//!     Ok(())
//! }
//! # }
//! ```
// Jackson Coxson

pub mod dtx_services;
pub mod listener;
pub mod types;
