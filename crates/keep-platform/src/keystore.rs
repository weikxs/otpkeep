//! Where this device keeps its data key (design/02-data.md §3.2).

use std::collections::HashMap;
use std::sync::Mutex;

use zeroize::Zeroizing;

/// Why the keystore could not be used. Never carries secret bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum KeystoreError {
    /// There is no usable keystore on this system (for example, no Secret Service on Linux).
    #[error("no system keystore is available")]
    Unavailable,
    /// The keystore exists but refused access (for example, it is locked).
    #[error("the system keystore refused access")]
    Denied,
    /// The item does not exist.
    #[error("the keystore has no such item")]
    NotFound,
    /// Any other keystore failure.
    #[error("the system keystore failed")]
    Failed,
}

/// Stores small secrets under an account name.
pub trait Keystore: Send + Sync {
    /// Loads the secret stored under `account`, or `None` when there is none.
    ///
    /// # Errors
    ///
    /// Returns [`KeystoreError`] when the keystore cannot be used.
    fn load(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, KeystoreError>;

    /// Stores `secret` under `account`, replacing any previous secret.
    ///
    /// # Errors
    ///
    /// Returns [`KeystoreError`] when the keystore cannot be used.
    fn store(&self, account: &str, secret: &[u8]) -> Result<(), KeystoreError>;

    /// Deletes the secret under `account`; deleting a missing secret succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`KeystoreError`] when the keystore cannot be used.
    fn delete(&self, account: &str) -> Result<(), KeystoreError>;
}

type Items = HashMap<String, Zeroizing<Vec<u8>>>;

/// A keystore in memory, for tests and as the stand-in when a test must not touch the real one.
#[derive(Debug, Default)]
pub struct MemoryKeystore {
    items: Mutex<Items>,
    unavailable: bool,
}

impl MemoryKeystore {
    /// An empty, working keystore.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A keystore that fails every call with [`KeystoreError::Unavailable`].
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            unavailable: true,
            ..Self::default()
        }
    }

    fn items(&self) -> Result<std::sync::MutexGuard<'_, Items>, KeystoreError> {
        if self.unavailable {
            return Err(KeystoreError::Unavailable);
        }
        self.items.lock().map_err(|_| KeystoreError::Failed)
    }
}

impl Keystore for MemoryKeystore {
    fn load(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, KeystoreError> {
        Ok(self.items()?.get(account).cloned())
    }

    fn store(&self, account: &str, secret: &[u8]) -> Result<(), KeystoreError> {
        self.items()?
            .insert(account.to_owned(), Zeroizing::new(secret.to_vec()));
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), KeystoreError> {
        self.items()?.remove(account);
        Ok(())
    }
}

/// The operating system's keystore: macOS keychain, Windows Credential Manager or Linux Secret Service.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[derive(Debug, Clone)]
pub struct SystemKeystore {
    service: String,
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
impl SystemKeystore {
    /// The service name under which OTPKeep's items are stored.
    pub const SERVICE: &'static str = "io.github.weikxs.otpkeep";

    /// The keystore under OTPKeep's own service name.
    #[must_use]
    pub fn new() -> Self {
        Self::with_service(Self::SERVICE)
    }

    /// The keystore under another service name, for example a test-only one.
    #[must_use]
    pub fn with_service(service: &str) -> Self {
        Self {
            service: service.to_owned(),
        }
    }

    fn entry(&self, account: &str) -> Result<keyring_core::Entry, KeystoreError> {
        (*PLATFORM_STORE)?;
        keyring_core::Entry::new(&self.service, account).map_err(from_platform)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
impl Default for SystemKeystore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
impl Keystore for SystemKeystore {
    fn load(&self, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>, KeystoreError> {
        match self
            .entry(account)?
            .get_secret()
            .map(Zeroizing::new)
            .map_err(from_platform)
        {
            Ok(secret) => Ok(Some(secret)),
            Err(KeystoreError::NotFound) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn store(&self, account: &str, secret: &[u8]) -> Result<(), KeystoreError> {
        self.entry(account)?
            .set_secret(secret)
            .map_err(from_platform)
    }

    fn delete(&self, account: &str) -> Result<(), KeystoreError> {
        match self
            .entry(account)?
            .delete_credential()
            .map_err(from_platform)
        {
            Ok(()) | Err(KeystoreError::NotFound) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

/// Installs this platform's credential store once per process.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
static PLATFORM_STORE: std::sync::LazyLock<Result<(), KeystoreError>> =
    std::sync::LazyLock::new(|| {
        #[cfg(target_os = "macos")]
        let store = apple_native_keyring_store::keychain::Store::new();
        #[cfg(target_os = "windows")]
        let store = windows_native_keyring_store::Store::new();
        #[cfg(target_os = "linux")]
        let store = dbus_secret_service_keyring_store::Store::new();
        keyring_core::set_default_store(store.map_err(from_platform)?);
        Ok(())
    });

/// Maps a platform error without keeping its payload: some variants carry the secret bytes.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[allow(
    clippy::needless_pass_by_value,
    reason = "the error is consumed so its payload is dropped here"
)]
fn from_platform(error: keyring_core::Error) -> KeystoreError {
    match error {
        keyring_core::Error::NoEntry => KeystoreError::NotFound,
        keyring_core::Error::NoStorageAccess(_) => KeystoreError::Denied,
        keyring_core::Error::NoDefaultStore | keyring_core::Error::NotSupportedByStore(_) => {
            KeystoreError::Unavailable
        }
        _ => KeystoreError::Failed,
    }
}

#[cfg(test)]
mod tests;
