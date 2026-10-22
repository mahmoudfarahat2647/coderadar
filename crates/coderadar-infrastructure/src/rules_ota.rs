//! Signed OTA rule definitions (`CR-SPEC-04 §7`, Phase 8).
//!
//! A downloaded `rules/manifest.json` is only activated when its Ed25519
//! signature verifies against the public key compiled into this binary. The
//! signed payload is the **canonical** JSON of the manifest without its
//! `signature` field: object keys sorted recursively, compact separators, no
//! insignificant whitespace. [`scripts/sign-rules.mjs`](https://github.com/mahmoudfarahat2647/coderadar)
//! produces exactly that representation.
//!
//! The public key is a compile-time constant, so a downloaded manifest can
//! never change the trust anchor. An installed manifest that fails to verify is
//! ignored at runtime and the embedded ruleset stays active.

use std::path::{Path, PathBuf};

use coderadar_domain::DomainError;
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::Value;

use crate::patching::atomic_write;
use crate::rules_manifest::RuleManifest;

/// Ed25519 public key of the CodeRadar rules signing authority (hex).
pub const RULES_PUBLIC_KEY_HEX: &str =
    "1f4d2c5f186388bda1d4d53ac04a3731f9cc02b6894da833876b1f03bd3e80e3";

/// Prefix of the `signature` field (`ed25519:<128 hex characters>`).
pub const SIGNATURE_PREFIX: &str = "ed25519:";

/// Directory, inside the data directory, that holds installed rulesets.
pub const RULES_DIRECTORY: &str = "rules";

/// File name of an installed manifest.
pub const ACTIVE_MANIFEST_FILE: &str = "manifest.json";

/// Where the active rules came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesSource {
    /// The manifest embedded in the binary at compile time.
    Embedded,
    /// A signed manifest installed in the user data directory.
    Installed,
}

/// The ruleset a scan should use, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRuleset {
    /// The active, validated manifest.
    pub manifest: RuleManifest,
    /// Whether the manifest is embedded or was installed.
    pub source: RulesSource,
}

/// Verifies and installs signed rulesets, and resolves the active one.
#[derive(Debug, Clone)]
pub struct RulesStore {
    data_dir: PathBuf,
}

impl RulesStore {
    /// Create a store rooted at `data_dir` (the platform app-data directory).
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    /// The platform default data directory (`CODERADAR_DATA_DIR` overrides it).
    pub fn default_data_dir() -> PathBuf {
        if let Some(overridden) = std::env::var_os("CODERADAR_DATA_DIR") {
            return PathBuf::from(overridden);
        }

        let home = std::env::var_os("HOME").map(PathBuf::from);
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA")
                .or_else(|| std::env::var_os("LOCALAPPDATA"))
                .map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            home.map(|home| home.join("Library").join("Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| home.map(|home| home.join(".local").join("share")))
        };

        base.unwrap_or_else(|| PathBuf::from(".coderadar"))
            .join("coderadar-studio")
    }

    /// Path of the installed manifest.
    pub fn manifest_path(&self) -> PathBuf {
        self.data_dir
            .join(RULES_DIRECTORY)
            .join(ACTIVE_MANIFEST_FILE)
    }

    /// Verify a manifest **without** installing it.
    ///
    /// # Errors
    /// Returns [`DomainError::Permission`] when the signature is missing or
    /// malformed, [`DomainError::Invariant`] when it does not verify, and
    /// [`DomainError::Parse`] when the manifest itself is invalid.
    pub fn verify(json: &str) -> Result<RuleManifest, DomainError> {
        let mut value: Value =
            serde_json::from_str(json).map_err(|error| DomainError::parse(error.to_string()))?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| DomainError::parse("the manifest must be a JSON object"))?;
        let signature_text = object
            .get("signature")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DomainError::Permission("the manifest carries no signature".to_string())
            })?
            .to_string();
        object.remove("signature");

        let signature = parse_signature(&signature_text)?;
        let payload = canonical_json(&value);
        let key_bytes = decode_hex::<32>(RULES_PUBLIC_KEY_HEX).map_err(|_| {
            DomainError::invariant("the embedded rules public key is not valid hex")
        })?;
        let key = VerifyingKey::from_bytes(&key_bytes).map_err(|error| {
            DomainError::invariant(format!("the embedded rules public key is invalid: {error}"))
        })?;
        key.verify_strict(payload.as_bytes(), &signature)
            .map_err(|_| {
                DomainError::Permission(
                    "the manifest signature does not match the CodeRadar signing key".to_string(),
                )
            })?;

        RuleManifest::from_json(json)
    }

    /// Verify and atomically install a signed manifest.
    ///
    /// # Errors
    /// Returns the same errors as [`Self::verify`] plus [`DomainError::Io`]
    /// when the manifest cannot be written.
    pub fn install(&self, json: &str) -> Result<RuleManifest, DomainError> {
        let manifest = Self::verify(json)?;
        if let Some(directory) = self.manifest_path().parent() {
            std::fs::create_dir_all(directory)?;
        }
        atomic_write(&self.manifest_path(), json)?;
        Ok(manifest)
    }

    /// Load the installed manifest, if it exists and verifies.
    ///
    /// # Errors
    /// Returns [`DomainError::Io`] when the file exists but cannot be read and
    /// [`DomainError::Permission`] when it fails verification.
    pub fn installed(&self) -> Option<RuleManifest> {
        let json = std::fs::read_to_string(self.manifest_path()).ok()?;
        Self::verify(&json).ok()
    }

    /// Resolve the ruleset a scan must use.
    ///
    /// A missing or invalid installed manifest falls back to the embedded one,
    /// so a corrupt download can never stop the product from scanning. Use
    /// [`Self::installed`] or [`Self::verify`] to report why an update is not
    /// active.
    pub fn active_or_embedded(&self) -> ActiveRuleset {
        if let Some(manifest) = self.installed() {
            return ActiveRuleset {
                manifest,
                source: RulesSource::Installed,
            };
        }
        match RuleManifest::embedded() {
            Ok(manifest) => ActiveRuleset {
                manifest,
                source: RulesSource::Embedded,
            },
            // The embedded manifest is validated by the test suite and by CI;
            // this branch only exists so the signature of this method stays
            // infallible for startup paths.
            Err(_) => ActiveRuleset {
                manifest: RuleManifest {
                    ruleset_version: "0.0.0-unavailable".to_string(),
                    generated_at: String::new(),
                    declared_total_rules: 0,
                    categories: Vec::new(),
                    signature: String::new(),
                    rules: Vec::new(),
                },
                source: RulesSource::Embedded,
            },
        }
    }

    /// The raw JSON of the active ruleset (installed and verified, or embedded).
    ///
    /// This is what the UI shows; it is byte-for-byte the signed document, so
    /// the signature can be verified again outside the product.
    pub fn active_json(&self) -> String {
        let installed = std::fs::read_to_string(self.manifest_path()).ok();
        if let Some(json) = installed {
            if Self::verify(&json).is_ok() {
                return json;
            }
        }
        crate::rules_manifest::EMBEDDED_MANIFEST_JSON.to_string()
    }

    /// The data directory this store writes to.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

/// Parse `ed25519:<hex>` into a signature.
fn parse_signature(text: &str) -> Result<Signature, DomainError> {
    let hex = text
        .strip_prefix(SIGNATURE_PREFIX)
        .ok_or_else(|| {
            DomainError::Permission(format!(
                "the signature must start with `{SIGNATURE_PREFIX}`"
            ))
        })?
        .trim();
    let bytes = decode_hex::<64>(hex).map_err(|error| {
        DomainError::Permission(format!("the manifest signature is not valid hex: {error}"))
    })?;
    Ok(Signature::from_bytes(&bytes))
}

/// Decode a lowercase or uppercase hex string of exactly `N` bytes.
fn decode_hex<const N: usize>(hex: &str) -> Result<[u8; N], DomainError> {
    let bytes = hex.as_bytes();
    if bytes.len() != N * 2 {
        return Err(DomainError::invariant(format!(
            "expected {} hex characters, found {}",
            N * 2,
            bytes.len()
        )));
    }
    let mut output = [0u8; N];
    for (index, slot) in output.iter_mut().enumerate() {
        let high = nibble(bytes[index * 2])?;
        let low = nibble(bytes[index * 2 + 1])?;
        *slot = (high << 4) | low;
    }
    Ok(output)
}

fn nibble(byte: u8) -> Result<u8, DomainError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(DomainError::invariant(format!(
            "`{}` is not a hex digit",
            char::from(byte)
        ))),
    }
}

/// Serialise a JSON value canonically: recursively sorted object keys, compact
/// separators, no trailing newline.
///
/// This is deliberately implemented here instead of relying on `serde_json`'s
/// map order: the `preserve_order` feature may be enabled by another crate in
/// the same build, and the signed bytes must not depend on feature resolution.
fn canonical_json(value: &Value) -> String {
    let mut output = String::new();
    write_canonical(value, &mut output);
    output
}

fn write_canonical(value: &Value, output: &mut String) {
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(boolean) => output.push_str(if *boolean { "true" } else { "false" }),
        Value::Number(number) => output.push_str(&number.to_string()),
        Value::String(string) => output.push_str(&escape_string(string)),
        Value::Array(items) => {
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical(item, output);
            }
            output.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            output.push('{');
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(&escape_string(key));
                output.push(':');
                if let Some(item) = map.get(*key) {
                    write_canonical(item, output);
                }
            }
            output.push('}');
        }
    }
}

/// JSON string escaping that matches `JSON.stringify` for the manifest's
/// character set (quotes, backslashes and control characters).
fn escape_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::{RulesSource, RulesStore, RULES_PUBLIC_KEY_HEX};

    const EMBEDDED: &str = include_str!("../../../rules/manifest.json");

    #[test]
    fn the_embedded_manifest_verifies_against_the_embedded_key() {
        assert_eq!(RULES_PUBLIC_KEY_HEX.len(), 64);
        let manifest = RulesStore::verify(EMBEDDED).expect("the shipped manifest is signed");
        assert_eq!(manifest.ruleset_version, "2026.8.0-pr");
        assert_eq!(manifest.len(), 19);
    }

    #[test]
    fn editing_a_rule_breaks_the_signature() {
        let tampered = EMBEDDED.replacen(
            "\"title\": \"Hardcoded Secrets & API Keys\"",
            "\"title\": \"Hardcoded Secrets\"",
            1,
        );
        assert_ne!(tampered, EMBEDDED, "the fixture must be editable");
        let error = RulesStore::verify(&tampered).expect_err("tampering is detected");
        assert!(error.to_string().contains("signature"), "{error}");
    }

    #[test]
    fn a_missing_or_malformed_signature_is_refused() {
        let without = EMBEDDED.replacen("\"signature\": \"ed25519:", "\"signature\": \"nope:", 1);
        let error = RulesStore::verify(&without).expect_err("prefix is required");
        assert!(error.to_string().contains("ed25519:"), "{error}");

        let short = EMBEDDED.replacen(
            "\"signature\": \"ed25519:",
            "\"signature\": \"ed25519:00",
            1,
        );
        let error = RulesStore::verify(&short).expect_err("length is checked");
        assert!(error.to_string().contains("hex"), "{error}");
    }

    #[test]
    fn the_verification_is_independent_of_whitespace_and_key_order() {
        let value: serde_json::Value = serde_json::from_str(EMBEDDED).expect("json");
        let pretty = serde_json::to_string_pretty(&value).expect("serialise");
        let manifest =
            RulesStore::verify(&pretty).expect("pretty printing does not break the signature");
        assert_eq!(manifest.len(), 19);
    }

    #[test]
    fn an_install_round_trips_through_the_store() {
        let directory = TempDir::new().unwrap();
        let store = RulesStore::new(directory.path());
        store.install(EMBEDDED).expect("install");

        assert!(store.manifest_path().is_file());
        let installed = store.installed().expect("installed manifest loads");
        assert_eq!(installed.ruleset_version, "2026.8.0-pr");
        let active = store.active_or_embedded();
        assert_eq!(active.source, RulesSource::Installed);
    }

    #[test]
    fn a_corrupt_installed_manifest_falls_back_to_the_embedded_ruleset() {
        let directory = TempDir::new().unwrap();
        let store = RulesStore::new(directory.path());
        fs::create_dir_all(store.manifest_path().parent().unwrap()).unwrap();
        fs::write(store.manifest_path(), "{\"rules\": []}").unwrap();

        let active = store.active_or_embedded();
        assert_eq!(active.source, RulesSource::Embedded);
        assert_eq!(active.manifest.len(), 19);
    }

    #[test]
    fn a_missing_install_directory_still_yields_the_embedded_ruleset() {
        let directory = TempDir::new().unwrap();
        let store = RulesStore::new(directory.path().join("missing"));
        assert!(store.installed().is_none());
        assert_eq!(store.active_or_embedded().source, RulesSource::Embedded);
    }
}
