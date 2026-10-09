use super::{Sig, SignalError};
use crate::model::RawSocket;
use std::io;

/// Socket enumeration is not available on this platform; always returns an error.
pub fn list_sockets() -> io::Result<Vec<RawSocket>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "platform not supported",
    ))
}
/// No process identity on this platform.
pub fn start_token(_pid: u32) -> Option<u64> {
    None
}
/// Always false on this platform.
pub fn is_zombie(_pid: u32) -> bool {
    false
}
/// Signalling is not available on this platform; always returns an error.
pub fn signal(_pid: u32, _t: u64, _s: Sig) -> Result<(), SignalError> {
    Err(SignalError::Other("platform not supported".into()))
}
