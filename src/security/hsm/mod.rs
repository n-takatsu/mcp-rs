//! Hardware Security Module (HSM) integration via PKCS#11.
//!
//! Talks to any PKCS#11-compliant module (a real HSM's vendor-supplied
//! shared library, a cloud HSM's PKCS#11 bridge, or a software
//! implementation such as SoftHSM2 for local development) so that AES keys
//! can be generated with `CKA_EXTRACTABLE = false`: the raw key bytes never
//! exist in this process's memory, only an opaque object handle does.
//!
//! Session concurrency: a PKCS#11 session's thread-safety under concurrent
//! calls is implementation-defined, so all operations against the shared
//! session are serialized through a [`tokio::sync::Mutex`]. Session pooling
//! for higher throughput is intentionally out of scope here.

use cryptoki::context::{CInitializeArgs, CInitializeFlags, Pkcs11};
use cryptoki::error::Error as Pkcs11Error;
use cryptoki::mechanism::aead::GcmParams;
use cryptoki::mechanism::Mechanism;
use cryptoki::object::{Attribute, KeyType, ObjectClass, ObjectHandle};
use cryptoki::session::{Session, UserType};
use cryptoki::slot::Slot;
use cryptoki::types::{AuthPin, Ulong};
use secrecy::{ExposeSecret, SecretString};
use std::path::Path;
use thiserror::Error;
use tokio::sync::Mutex;

/// Length, in bits, of the AES-GCM authentication tag appended to every
/// ciphertext produced by [`HsmProvider::encrypt_gcm`].
const GCM_TAG_BITS: usize = 128;

#[derive(Debug, Error)]
pub enum HsmError {
    #[error("PKCS#11 operation failed: {0}")]
    Pkcs11(#[from] Pkcs11Error),
    #[error("no PKCS#11 slot with a token present is available")]
    NoSlotAvailable,
    #[error("requested PKCS#11 slot id {0} was not found among available slots")]
    SlotNotFound(u64),
    #[error(
        "found {found} keys labeled '{label}'; refusing to guess which one to use \
         (this should never happen if keys are always created through this module)"
    )]
    AmbiguousKeyLabel { label: String, found: usize },
}

/// Configuration for connecting to a PKCS#11 provider.
#[derive(Clone)]
pub struct Pkcs11Config {
    /// Path to the provider's PKCS#11 shared library (e.g.
    /// `/usr/lib/softhsm/libsofthsm2.so` or a vendor-supplied `.so`/`.dll`).
    pub library_path: String,
    /// Slot to use. When `None`, the first slot that has a token present is
    /// used.
    pub slot_id: Option<u64>,
    /// User PIN to log into the token's session with.
    pub pin: SecretString,
}

impl std::fmt::Debug for Pkcs11Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pkcs11Config")
            .field("library_path", &self.library_path)
            .field("slot_id", &self.slot_id)
            .field("pin", &"***")
            .finish()
    }
}

/// A connected, logged-in PKCS#11 session used to generate and operate on
/// non-extractable AES keys.
pub struct HsmProvider {
    session: Mutex<Session>,
}

impl HsmProvider {
    /// Load the PKCS#11 library, open a read/write session on the
    /// configured (or first available) slot, and log in.
    pub fn connect(config: &Pkcs11Config) -> Result<Self, HsmError> {
        let pkcs11 = Pkcs11::new(Path::new(&config.library_path))?;
        pkcs11.initialize(CInitializeArgs::new(CInitializeFlags::OS_LOCKING_OK))?;

        let slot = Self::select_slot(&pkcs11, config.slot_id)?;
        let session = pkcs11.open_rw_session(slot)?;
        session.login(
            UserType::User,
            Some(&AuthPin::new(config.pin.expose_secret().to_string().into())),
        )?;

        Ok(Self {
            session: Mutex::new(session),
        })
    }

    fn select_slot(pkcs11: &Pkcs11, wanted: Option<u64>) -> Result<Slot, HsmError> {
        let slots = pkcs11.get_slots_with_token()?;
        match wanted {
            Some(id) => slots
                .into_iter()
                .find(|slot| slot.id() == id)
                .ok_or(HsmError::SlotNotFound(id)),
            None => slots.into_iter().next().ok_or(HsmError::NoSlotAvailable),
        }
    }

    /// Return the handle of the AES key labeled `label` if one already
    /// exists in the token, so a key survives across process restarts
    /// instead of being silently regenerated.
    async fn find_aes_key(&self, label: &str) -> Result<Option<ObjectHandle>, HsmError> {
        let session = self.session.lock().await;
        let template = [
            Attribute::Class(ObjectClass::SECRET_KEY),
            Attribute::KeyType(KeyType::AES),
            Attribute::Label(label.as_bytes().to_vec()),
        ];
        let mut found = session.find_objects(&template)?;
        match found.len() {
            0 => Ok(None),
            1 => Ok(Some(found.remove(0))),
            found_count => Err(HsmError::AmbiguousKeyLabel {
                label: label.to_string(),
                found: found_count,
            }),
        }
    }

    /// Return the existing AES key handle for `label`, generating a new
    /// non-extractable 256-bit AES key inside the token if none exists yet.
    ///
    /// The generated key is a token object (`CKA_TOKEN = true`) so it
    /// persists in the HSM's own storage across process restarts, and is
    /// created with `CKA_EXTRACTABLE = false` / `CKA_SENSITIVE = true`: the
    /// raw key material never leaves the token.
    pub async fn get_or_generate_aes_key(&self, label: &str) -> Result<ObjectHandle, HsmError> {
        if let Some(handle) = self.find_aes_key(label).await? {
            return Ok(handle);
        }

        let session = self.session.lock().await;
        let template = [
            Attribute::Class(ObjectClass::SECRET_KEY),
            Attribute::KeyType(KeyType::AES),
            Attribute::ValueLen(Ulong::try_from(32usize).expect("32 fits in CK_ULONG")),
            Attribute::Token(true),
            Attribute::Private(true),
            Attribute::Sensitive(true),
            Attribute::Extractable(false),
            Attribute::Encrypt(true),
            Attribute::Decrypt(true),
            Attribute::Label(label.as_bytes().to_vec()),
        ];
        let handle = session.generate_key(&Mechanism::AesKeyGen, &template)?;
        Ok(handle)
    }

    /// Encrypt `plaintext` under the token-resident AES key `key` using
    /// AES-GCM with the given `nonce`, returning the ciphertext with the
    /// authentication tag appended (PKCS#11's standard AES-GCM output
    /// layout), matching the format the software (`ring`-based) encryption
    /// path already produces.
    pub async fn encrypt_gcm(
        &self,
        key: ObjectHandle,
        nonce: &mut [u8],
        plaintext: &[u8],
    ) -> Result<Vec<u8>, HsmError> {
        let session = self.session.lock().await;
        let params = GcmParams::new(nonce, &[], Ulong::try_from(GCM_TAG_BITS).unwrap())?;
        let ciphertext = session.encrypt(&Mechanism::AesGcm(params), key, plaintext)?;
        Ok(ciphertext)
    }

    /// Decrypt `ciphertext` (tag appended, as produced by
    /// [`Self::encrypt_gcm`]) using the token-resident AES key `key`.
    pub async fn decrypt_gcm(
        &self,
        key: ObjectHandle,
        nonce: &mut [u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, HsmError> {
        let session = self.session.lock().await;
        let params = GcmParams::new(nonce, &[], Ulong::try_from(GCM_TAG_BITS).unwrap())?;
        let plaintext = session.decrypt(&Mechanism::AesGcm(params), key, ciphertext)?;
        Ok(plaintext)
    }
}
