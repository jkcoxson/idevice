//! NSKeyedArchive type proxies for the XCTest protocol.
//!
//! These types are exchanged as NSKeyedArchive-encoded plists between the IDE
//! and the on-device testmanagerd / test runner.  They are distinct from the
//! DTX protocol itself and live here because they are XCTest-specific payloads.
//!
//! Types that are only ever *received* from the runner implement only decode
//! logic.  [`XCTestConfiguration`] and [`XCTCapabilities`] must also be
//! encoded because the IDE sends them to the runner.
// Jackson Coxson
