//! API keys for analyzers. A manifest declares the environment variable names it may receive
//! (`[permissions] secrets`); the user stores values in the OS keychain; a run of that analyzer
//! — and only that analyzer — gets them in its otherwise cleared environment. Values are never
//! written to holdmap's files, logged, shown or returned to a front end.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use thiserror::Error;

/// Most secret names one manifest may declare.
pub const MAX_SECRETS: usize = 4;
/// Longest value accepted.
pub const MAX_SECRET_LEN: usize = 4096;

/// A secret value. Its `Debug` output is redacted and it has no `Display` or `Serialize`.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wrap a value read from the user or the keychain.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value, for the child process environment only.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// Why a secret could not be stored or read.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SecretError {
    /// This build or system has no keychain.
    #[error("no OS keychain is available: {0}")]
    Unavailable(String),
    /// The name or value was refused.
    #[error("{0}")]
    Invalid(String),
    /// The keychain refused the operation.
    #[error("the OS keychain refused: {0}")]
    Keychain(String),
}

/// Where analyzer secrets live.
pub trait SecretStore: Send + Sync {
    /// The value for `name`, if stored.
    fn get(&self, analyzer: &str, name: &str) -> Result<Option<Secret>, SecretError>;
    /// Store `value` for `name`, replacing any previous value.
    fn set(&self, analyzer: &str, name: &str, value: &Secret) -> Result<(), SecretError>;
    /// Delete the value; `true` when one existed.
    fn delete(&self, analyzer: &str, name: &str) -> Result<bool, SecretError>;
    /// A short label for the permissions sheet, e.g. "macOS Keychain".
    fn label(&self) -> &'static str;
}

/// Whether `name` is an acceptable secret environment variable name.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && name.len() <= 64
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !crate::runner::ENV_NAMES.contains(&name)
        && !matches!(name, "SYSTEMROOT" | "LD_PRELOAD" | "DYLD_INSERT_LIBRARIES")
        && !name.starts_with("LD_")
        && !name.starts_with("DYLD_")
}

/// Check a value before storing it.
pub fn check_value(value: &Secret) -> Result<(), SecretError> {
    let v = value.expose();
    if v.is_empty() {
        return Err(SecretError::Invalid("the key is empty".into()));
    }
    if v.len() > MAX_SECRET_LEN {
        return Err(SecretError::Invalid(format!(
            "the key is longer than {MAX_SECRET_LEN} bytes"
        )));
    }
    if v.chars().any(|c| c.is_control()) {
        return Err(SecretError::Invalid(
            "the key contains control characters or line breaks".into(),
        ));
    }
    Ok(())
}

/// The keychain service name for one analyzer.
pub fn service_name(analyzer: &str) -> String {
    format!("holdmap.analyzer.{analyzer}")
}

/// No keychain: nothing is stored and runs get no secrets.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSecrets;

impl SecretStore for NoSecrets {
    fn get(&self, _: &str, _: &str) -> Result<Option<Secret>, SecretError> {
        Ok(None)
    }
    fn set(&self, _: &str, _: &str, _: &Secret) -> Result<(), SecretError> {
        Err(SecretError::Unavailable(
            "this build of holdmap has no keychain support".into(),
        ))
    }
    fn delete(&self, _: &str, _: &str) -> Result<bool, SecretError> {
        Ok(false)
    }
    fn label(&self) -> &'static str {
        "no keychain"
    }
}

/// In-memory store for tests and previews.
#[derive(Debug, Default)]
pub struct MemorySecrets(Mutex<HashMap<(String, String), Secret>>);

impl SecretStore for MemorySecrets {
    fn get(&self, analyzer: &str, name: &str) -> Result<Option<Secret>, SecretError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| SecretError::Keychain("poisoned".into()))?
            .get(&(analyzer.into(), name.into()))
            .cloned())
    }
    fn set(&self, analyzer: &str, name: &str, value: &Secret) -> Result<(), SecretError> {
        self.0
            .lock()
            .map_err(|_| SecretError::Keychain("poisoned".into()))?
            .insert((analyzer.into(), name.into()), value.clone());
        Ok(())
    }
    fn delete(&self, analyzer: &str, name: &str) -> Result<bool, SecretError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| SecretError::Keychain("poisoned".into()))?
            .remove(&(analyzer.into(), name.into()))
            .is_some())
    }
    fn label(&self) -> &'static str {
        "memory"
    }
}

/// The OS keychain via the `keyring` crate: one entry per analyzer and name, under the
/// service `holdmap.analyzer.<id>`.
#[cfg(feature = "keychain")]
#[derive(Debug, Default, Clone, Copy)]
pub struct KeyringSecrets;

#[cfg(feature = "keychain")]
impl KeyringSecrets {
    fn entry(analyzer: &str, name: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&service_name(analyzer), name).map_err(keyring_error)
    }
}

#[cfg(feature = "keychain")]
fn keyring_error(e: keyring::Error) -> SecretError {
    match e {
        keyring::Error::PlatformFailure(_) | keyring::Error::NoStorageAccess(_) => {
            SecretError::Unavailable(e.to_string())
        }
        other => SecretError::Keychain(other.to_string()),
    }
}

#[cfg(feature = "keychain")]
impl SecretStore for KeyringSecrets {
    fn get(&self, analyzer: &str, name: &str) -> Result<Option<Secret>, SecretError> {
        match Self::entry(analyzer, name)?.get_password() {
            Ok(v) => Ok(Some(Secret::new(v))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(e)),
        }
    }
    fn set(&self, analyzer: &str, name: &str, value: &Secret) -> Result<(), SecretError> {
        Self::entry(analyzer, name)?
            .set_password(value.expose())
            .map_err(keyring_error)
    }
    fn delete(&self, analyzer: &str, name: &str) -> Result<bool, SecretError> {
        match Self::entry(analyzer, name)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(keyring_error(e)),
        }
    }
    fn label(&self) -> &'static str {
        if cfg!(target_os = "macos") {
            "macOS Keychain"
        } else if cfg!(windows) {
            "Windows Credential Manager"
        } else {
            "Secret Service keyring"
        }
    }
}

/// The store this build uses by default.
pub fn default_store() -> Box<dyn SecretStore> {
    #[cfg(feature = "keychain")]
    {
        Box::new(KeyringSecrets)
    }
    #[cfg(not(feature = "keychain"))]
    {
        Box::new(NoSecrets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_value() {
        let s = Secret::new("sk-live-1234");
        assert_eq!(format!("{s:?}"), "Secret(<redacted>)");
        assert!(!format!("{:?}", Some(s)).contains("sk-live"));
    }

    #[test]
    fn names_are_plain_upper_case_and_not_reserved() {
        assert!(valid_name("NVIDIA_API_KEY"));
        assert!(valid_name("OPENAI_API_KEY"));
        for bad in [
            "",
            "path",
            "PATH",
            "HOME",
            "LD_PRELOAD",
            "DYLD_X",
            "A-B",
            "1KEY",
            "KEY=1",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
        assert!(!valid_name(&"A".repeat(65)));
    }

    #[test]
    fn values_are_checked() {
        assert!(check_value(&Secret::new("abc")).is_ok());
        assert!(check_value(&Secret::new("")).is_err());
        assert!(check_value(&Secret::new("a\nb")).is_err());
        assert!(check_value(&Secret::new("a".repeat(MAX_SECRET_LEN + 1))).is_err());
    }

    #[test]
    fn memory_store_round_trips_per_analyzer() {
        let m = MemorySecrets::default();
        m.set("a.x", "KEY", &Secret::new("1")).unwrap();
        assert_eq!(m.get("a.x", "KEY").unwrap(), Some(Secret::new("1")));
        assert_eq!(m.get("b.y", "KEY").unwrap(), None);
        assert!(m.delete("a.x", "KEY").unwrap());
        assert!(!m.delete("a.x", "KEY").unwrap());
    }

    #[test]
    fn no_keychain_refuses_to_store() {
        assert!(matches!(
            NoSecrets.set("a", "K", &Secret::new("v")),
            Err(SecretError::Unavailable(_))
        ));
        assert_eq!(NoSecrets.get("a", "K").unwrap(), None);
    }

    /// Touches the real OS keychain; run by hand: `cargo test -p holdmap-analyzers -- --ignored`.
    #[cfg(feature = "keychain")]
    #[test]
    #[ignore = "uses the real OS keychain"]
    fn real_keychain_round_trip() {
        let k = KeyringSecrets;
        let id = "holdmap.test-only";
        k.set(id, "HOLDMAP_TEST_KEY", &Secret::new("value"))
            .unwrap();
        assert_eq!(
            k.get(id, "HOLDMAP_TEST_KEY").unwrap(),
            Some(Secret::new("value"))
        );
        assert!(k.delete(id, "HOLDMAP_TEST_KEY").unwrap());
    }
}
