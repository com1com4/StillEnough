pub mod access;
pub mod errors;
pub mod protocol;
pub mod providers;
pub mod request;
pub mod storage;
pub mod transport;
#[cfg(windows)]
pub mod windows;
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");
