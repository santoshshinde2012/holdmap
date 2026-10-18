//! Loading the catalog secret key for the signing tool. Shared with tests/catalog.rs.
//!
//! Accepts keys made by the minisign CLI (`minisign -G`, `-G -W`) and by the minisign crate.
//! The CLI leaves the checksum of an unencrypted (`-W`) key unset, which the crate rejects as
//! "Corrupted key", so unencrypted keys are read directly; the signing tool then checks every
//! signature against `catalog/holdmap-catalog.pub` before writing anything.

use ct_codecs::{Base64, Decoder};

/// Bytes in a decoded minisign secret key.
const SECRET_KEY_BYTES: usize = 158;

/// Parse a minisign secret key file. Encrypted keys prompt for their password on the terminal.
pub fn load(text: &str) -> Result<minisign::SecretKey, String> {
    let line = text
        .lines()
        .nth(1)
        .map(str::trim)
        .ok_or("not a minisign secret key file")?;
    let bytes = Base64::decode_to_vec(line, None).map_err(|_| "the secret key is not base64")?;
    if bytes.len() != SECRET_KEY_BYTES || &bytes[..2] != b"Ed" {
        return Err("not an Ed25519 minisign secret key".into());
    }
    if bytes[2..4] == [0, 0] {
        return minisign::SecretKey::from_bytes(&bytes).map_err(|e| e.to_string());
    }
    minisign::SecretKeyBox::from_string(text)
        .and_then(|b| b.into_secret_key(None))
        .map_err(|e| e.to_string())
}
