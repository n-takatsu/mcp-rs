//! WordPress credentials in the OS credential store (#533).
//!
//! Claude Desktop passes MCP servers only a fixed set of environment
//! variables plus the `env` of its JSON config, while Claude Code and
//! terminals pass the user's environment. Credentials read from environment
//! variables therefore had to be written twice. The OS credential store is
//! readable the same way whichever client starts mcp-rs:
//!
//! - Windows: Credential Manager (generic credential `<name>.mcp-rs`)
//! - macOS: Keychain (service `mcp-rs`, account `<name>`)
//! - Linux and other Unix: Secret Service (GNOME Keyring, KWallet, ...)
//!
//! The config file refers to an entry by name:
//!
//! ```toml
//! [handlers.wordpress]
//! url = "https://example.com"
//! enabled = true
//! keyring = "example.com"
//! ```
//!
//! The entry holds the site URL, the username and the Application Password,
//! as one JSON secret, so a single `mcp-rs wordpress-keyring set <name>` is
//! all that is needed. Secret values never appear in error messages.
//!
//! The site URL ties the entry to one site (#541): mcp-rs uses the entry
//! only when the `url` of the config file has the same origin (scheme, host
//! and port). Otherwise a config file someone else wrote (for example in a
//! repository opened in Claude Code) could name the entry with
//! `url = "https://attacker.example"` and receive the password.

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Service name of every mcp-rs entry in the OS credential store.
pub const KEYRING_SERVICE: &str = "mcp-rs";

/// Why credentials could not be read from or written to the credential store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyringError {
    /// The entry name is empty.
    EmptyName,
    /// No entry with this name.
    NotFound(String),
    /// The OS credential store could not be used (for example no Secret
    /// Service on a headless Linux server or in a container).
    Unavailable(String),
    /// The entry exists but does not hold mcp-rs WordPress credentials.
    Invalid(String),
    /// The site URL is not an http(s) URL.
    InvalidSiteUrl(String),
    /// The entry was stored without a site URL (before #541).
    MissingSiteUrl(String),
    /// The entry belongs to another site than the `url` of the config file.
    SiteMismatch {
        name: String,
        stored: String,
        configured: String,
    },
}

impl fmt::Display for KeyringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(f, "the keyring entry name is empty"),
            Self::NotFound(name) => write!(
                f,
                "no WordPress credentials named '{name}' in the OS credential store. Register them with `mcp-rs wordpress-keyring set {name}`"
            ),
            Self::Unavailable(reason) => write!(
                f,
                "the OS credential store is not available ({reason}). Use environment variables (${{VAR}}) in the config file instead"
            ),
            Self::Invalid(name) => write!(
                f,
                "the keyring entry '{name}' does not hold mcp-rs WordPress credentials. Register it again with `mcp-rs wordpress-keyring set {name}`"
            ),
            Self::InvalidSiteUrl(url) => write!(
                f,
                "'{url}' is not a site URL: use the address of the WordPress site, starting with https:// (or http://)"
            ),
            Self::MissingSiteUrl(name) => write!(
                f,
                "the credentials named '{name}' were stored without the site URL. Register them again with `mcp-rs wordpress-keyring set {name}`, which also asks for the site URL"
            ),
            Self::SiteMismatch {
                name,
                stored,
                configured,
            } => write!(
                f,
                "the credentials named '{name}' are for {stored}, but url in the config file is {configured}. They are not sent to another site. Fix url, or register credentials for {configured} with `mcp-rs wordpress-keyring set <name>`"
            ),
        }
    }
}

impl std::error::Error for KeyringError {}

/// A place that keeps one secret string per entry name. The OS credential
/// store in production, an in-memory map in tests.
pub trait SecretStore {
    /// The secret, or `None` if there is no entry with this name.
    fn get(&self, name: &str) -> Result<Option<Zeroizing<String>>, KeyringError>;
    /// Create or replace the entry.
    fn set(&self, name: &str, secret: &str) -> Result<(), KeyringError>;
    /// Delete the entry. `false` if there was none.
    fn delete(&self, name: &str) -> Result<bool, KeyringError>;
}

/// The OS credential store, under the [`KEYRING_SERVICE`] service.
pub struct OsKeyring;

#[cfg(feature = "os-keyring")]
impl OsKeyring {
    fn entry(name: &str) -> Result<keyring::Entry, KeyringError> {
        keyring::Entry::new(KEYRING_SERVICE, name).map_err(map_keyring_error)
    }
}

/// `keyring::Error` messages do not contain the secret, but `BadEncoding`
/// carries the raw bytes, so only the kind is reported for it.
#[cfg(feature = "os-keyring")]
fn map_keyring_error(error: keyring::Error) -> KeyringError {
    match error {
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(_, _) => {
            KeyringError::Unavailable("the stored value could not be read".to_string())
        }
        // "No default store has been set" hides why: the store failed to
        // start (for example no D-Bus session on a Linux server). That
        // reason is kept by store_status()
        keyring::Error::NoDefaultStore => match keyring::Entry::store_status() {
            Err(reason) => KeyringError::Unavailable(reason.to_string()),
            Ok(()) => KeyringError::Unavailable(keyring::Error::NoDefaultStore.to_string()),
        },
        other => KeyringError::Unavailable(other.to_string()),
    }
}

#[cfg(feature = "os-keyring")]
impl SecretStore for OsKeyring {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<String>>, KeyringError> {
        match Self::entry(name)?.get_password() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring_error(e)),
        }
    }

    fn set(&self, name: &str, secret: &str) -> Result<(), KeyringError> {
        Self::entry(name)?
            .set_password(secret)
            .map_err(map_keyring_error)
    }

    fn delete(&self, name: &str) -> Result<bool, KeyringError> {
        match Self::entry(name)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(map_keyring_error(e)),
        }
    }
}

/// Built without the `os-keyring` feature (the Docker image): every call
/// fails with the reason.
#[cfg(not(feature = "os-keyring"))]
impl SecretStore for OsKeyring {
    fn get(&self, _name: &str) -> Result<Option<Zeroizing<String>>, KeyringError> {
        Err(not_built())
    }

    fn set(&self, _name: &str, _secret: &str) -> Result<(), KeyringError> {
        Err(not_built())
    }

    fn delete(&self, _name: &str) -> Result<bool, KeyringError> {
        Err(not_built())
    }
}

#[cfg(not(feature = "os-keyring"))]
fn not_built() -> KeyringError {
    KeyringError::Unavailable(
        "this mcp-rs was built without the os-keyring feature, as the Docker image is".to_string(),
    )
}

/// WordPress credentials as kept in one keyring entry.
pub struct WordPressKeyringCredentials {
    /// The site the credentials are for, as an origin
    /// (`https://example.com`, see [`site_origin`]), #541. Empty for an
    /// entry stored before #541.
    pub site_url: String,
    pub username: String,
    pub application_password: Zeroizing<String>,
}

impl fmt::Debug for WordPressKeyringCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WordPressKeyringCredentials")
            .field("site_url", &self.site_url)
            .field("username", &self.username)
            .field("application_password", &"[REDACTED]")
            .finish()
    }
}

/// The JSON layout of the secret. `kind` tells mcp-rs entries apart from
/// other secrets that happen to use the same name.
#[derive(Serialize, Deserialize)]
struct StoredCredentials {
    kind: String,
    /// The site's origin (#541). Missing in entries stored before #541
    #[serde(default)]
    url: String,
    username: String,
    password: String,
}

const STORED_KIND: &str = "wordpress";

fn check_name(name: &str) -> Result<&str, KeyringError> {
    let name = name.trim();
    if name.is_empty() {
        Err(KeyringError::EmptyName)
    } else {
        Ok(name)
    }
}

/// The origin of a site URL (`scheme://host[:port]`, lowercase, without the
/// default port), #541. `None` unless it is an http(s) URL with a host.
pub fn site_origin(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
        return None;
    }
    Some(parsed.origin().ascii_serialization())
}

/// Check that `credentials` (named `name`) are for the site at
/// `configured_url`, the `url` of the config file (#541). The scheme counts:
/// credentials stored for https are not sent over http.
pub fn check_site(
    name: &str,
    credentials: &WordPressKeyringCredentials,
    configured_url: &str,
) -> Result<(), KeyringError> {
    let configured = site_origin(configured_url)
        .ok_or_else(|| KeyringError::InvalidSiteUrl(configured_url.to_string()))?;
    if credentials.site_url.is_empty() {
        return Err(KeyringError::MissingSiteUrl(name.to_string()));
    }
    if credentials.site_url != configured {
        return Err(KeyringError::SiteMismatch {
            name: name.to_string(),
            stored: credentials.site_url.clone(),
            configured,
        });
    }
    Ok(())
}

/// Read the WordPress credentials named `name`.
pub fn load_wordpress_credentials(
    store: &dyn SecretStore,
    name: &str,
) -> Result<WordPressKeyringCredentials, KeyringError> {
    let name = check_name(name)?;
    let secret = store
        .get(name)?
        .ok_or_else(|| KeyringError::NotFound(name.to_string()))?;
    let stored: StoredCredentials =
        serde_json::from_str(&secret).map_err(|_| KeyringError::Invalid(name.to_string()))?;
    let stored = Zeroizing::new(stored);
    if stored.kind != STORED_KIND || stored.username.is_empty() || stored.password.is_empty() {
        return Err(KeyringError::Invalid(name.to_string()));
    }
    Ok(WordPressKeyringCredentials {
        site_url: stored.url.clone(),
        username: stored.username.clone(),
        application_password: Zeroizing::new(stored.password.clone()),
    })
}

/// Create or replace the WordPress credentials named `name`, then read them
/// back to make sure startup will find the same values.
pub fn store_wordpress_credentials(
    store: &dyn SecretStore,
    name: &str,
    credentials: &WordPressKeyringCredentials,
) -> Result<(), KeyringError> {
    let name = check_name(name)?;
    if credentials.username.is_empty() || credentials.application_password.is_empty() {
        return Err(KeyringError::Invalid(name.to_string()));
    }
    let site_url = site_origin(&credentials.site_url)
        .ok_or_else(|| KeyringError::InvalidSiteUrl(credentials.site_url.clone()))?;
    let secret = Zeroizing::new(
        serde_json::to_string(&StoredCredentials {
            kind: STORED_KIND.to_string(),
            url: site_url.clone(),
            username: credentials.username.clone(),
            password: credentials.application_password.to_string(),
        })
        .map_err(|_| KeyringError::Invalid(name.to_string()))?,
    );
    store.set(name, &secret)?;

    let read_back = load_wordpress_credentials(store, name)?;
    if read_back.site_url != site_url
        || read_back.username != credentials.username
        || *read_back.application_password != *credentials.application_password
    {
        return Err(KeyringError::Unavailable(
            "the stored credentials did not read back unchanged".to_string(),
        ));
    }
    Ok(())
}

/// Delete the WordPress credentials named `name`. `false` if there were none.
pub fn delete_wordpress_credentials(
    store: &dyn SecretStore,
    name: &str,
) -> Result<bool, KeyringError> {
    store.delete(check_name(name)?)
}

impl zeroize::Zeroize for StoredCredentials {
    fn zeroize(&mut self) {
        self.url.zeroize();
        self.username.zeroize();
        self.password.zeroize();
    }
}

/// An in-memory [`SecretStore`] for tests.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryStore {
    entries: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

#[cfg(test)]
impl SecretStore for MemoryStore {
    fn get(&self, name: &str) -> Result<Option<Zeroizing<String>>, KeyringError> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .get(name)
            .map(|s| Zeroizing::new(s.clone())))
    }

    fn set(&self, name: &str, secret: &str) -> Result<(), KeyringError> {
        self.entries
            .lock()
            .unwrap()
            .insert(name.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<bool, KeyringError> {
        Ok(self.entries.lock().unwrap().remove(name).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials(username: &str, password: &str) -> WordPressKeyringCredentials {
        WordPressKeyringCredentials {
            site_url: "https://example.com".to_string(),
            username: username.to_string(),
            application_password: Zeroizing::new(password.to_string()),
        }
    }

    #[test]
    fn test_store_load_delete() {
        let store = MemoryStore::default();
        store_wordpress_credentials(
            &store,
            "example.com",
            &credentials("editor\"quote", "abcd EFGH 1234"),
        )
        .unwrap();

        let loaded = load_wordpress_credentials(&store, " example.com ").unwrap();
        assert_eq!(loaded.site_url, "https://example.com");
        assert_eq!(loaded.username, "editor\"quote");
        assert_eq!(loaded.application_password.as_str(), "abcd EFGH 1234");
        assert!(!format!("{loaded:?}").contains("abcd"));

        // Replacing keeps one entry with the new values
        store_wordpress_credentials(&store, "example.com", &credentials("editor", "new pw"))
            .unwrap();
        let loaded = load_wordpress_credentials(&store, "example.com").unwrap();
        assert_eq!(loaded.application_password.as_str(), "new pw");

        assert!(delete_wordpress_credentials(&store, "example.com").unwrap());
        assert!(!delete_wordpress_credentials(&store, "example.com").unwrap());
        assert_eq!(
            load_wordpress_credentials(&store, "example.com").unwrap_err(),
            KeyringError::NotFound("example.com".to_string())
        );
    }

    #[test]
    fn test_rejects_empty_and_foreign_entries() {
        let store = MemoryStore::default();
        assert_eq!(
            load_wordpress_credentials(&store, "  ").unwrap_err(),
            KeyringError::EmptyName
        );
        assert!(store_wordpress_credentials(&store, "x", &credentials("", "pw")).is_err());
        assert!(store_wordpress_credentials(&store, "x", &credentials("u", "")).is_err());

        // Another program's secret under the same name is not used as-is
        store.set("other", "plain-password").unwrap();
        assert_eq!(
            load_wordpress_credentials(&store, "other").unwrap_err(),
            KeyringError::Invalid("other".to_string())
        );
        store
            .set(
                "other",
                r#"{"kind":"github","username":"u","password":"p"}"#,
            )
            .unwrap();
        assert_eq!(
            load_wordpress_credentials(&store, "other").unwrap_err(),
            KeyringError::Invalid("other".to_string())
        );
    }

    /// サイトのURLは、オリジン（スキーム・ホスト・ポート）で比べる（#541）
    #[test]
    fn test_site_origin_and_check_site() {
        assert_eq!(
            site_origin(" HTTPS://Example.COM:443/blog/ ").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            site_origin("http://localhost:8080/wp").as_deref(),
            Some("http://localhost:8080")
        );
        for invalid in ["example.com", "ftp://example.com", "", "https://"] {
            assert_eq!(site_origin(invalid), None, "{invalid}");
        }
        // 保存するときに、オリジンにそろえる
        let store = MemoryStore::default();
        let mut stored = credentials("editor", "pw");
        stored.site_url = "https://Example.com/wp-admin/".to_string();
        store_wordpress_credentials(&store, "a", &stored).unwrap();
        let loaded = load_wordpress_credentials(&store, "a").unwrap();
        assert_eq!(loaded.site_url, "https://example.com");
        stored.site_url = "example.com".to_string();
        assert_eq!(
            store_wordpress_credentials(&store, "a", &stored).unwrap_err(),
            KeyringError::InvalidSiteUrl("example.com".to_string())
        );

        // 同じサイト（パスは見ない）なら使える
        check_site("a", &loaded, "https://example.com/blog").unwrap();
        // 別のホスト・スキーム・ポートには使わない
        for other in [
            "https://attacker.example",
            "http://example.com",
            "https://example.com:8443",
            "https://example.com.attacker.example",
        ] {
            let err = check_site("a", &loaded, other).unwrap_err();
            assert!(matches!(err, KeyringError::SiteMismatch { .. }), "{other}");
            let message = err.to_string();
            assert!(message.contains("https://example.com"), "{message}");
            assert!(!message.contains("pw"), "{message}");
        }
        // URLを持たない古い項目は、作り直しを案内する
        store
            .set(
                "old",
                r#"{"kind":"wordpress","username":"u","password":"p"}"#,
            )
            .unwrap();
        let old = load_wordpress_credentials(&store, "old").unwrap();
        let err = check_site("old", &old, "https://example.com").unwrap_err();
        assert_eq!(err, KeyringError::MissingSiteUrl("old".to_string()));
        assert!(err.to_string().contains("mcp-rs wordpress-keyring set old"));
    }

    #[test]
    fn test_error_messages_point_to_the_fix() {
        let not_found = KeyringError::NotFound("example.com".to_string()).to_string();
        assert!(not_found.contains("mcp-rs wordpress-keyring set example.com"));
        let unavailable = KeyringError::Unavailable("no D-Bus".to_string()).to_string();
        assert!(unavailable.contains("${VAR}"));
    }
}
