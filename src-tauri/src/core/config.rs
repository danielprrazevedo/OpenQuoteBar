//! Provider configuration.
//!
//! Stored as TOML in the app config directory so it can be read and edited by
//! hand. API keys are deliberately absent from this file: they live in the OS
//! credential store (see [`crate::core::secrets`]).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

/// File name of the configuration, resolved inside the app config directory.
pub const CONFIG_FILE: &str = "config.toml";

/// Bundled template. Written verbatim on first run, and its comment block is
/// re-emitted every time the app rewrites the file.
const TEMPLATE: &str = include_str!("../../config.example.toml");

/// Marker that separates the documentation header from the provider entries.
///
/// The leading newline matters: the string `[[providers]]` also appears inside
/// the documentation itself, and only the real table starts on its own line.
const PROVIDERS_MARKER: &str = "\n[[providers]]";

/// How a provider's API key should be interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyType {
    /// Account-wide credential, e.g. OpenRouter's `/credits`. Preferred.
    Management,
    /// Per-key credential, e.g. OpenRouter's `/key`.
    Standard,
}

/// A single provider entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Stable provider identifier.
    pub id: String,
    /// Whether the app should read this provider's balance.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Provider-specific credential kind, when the provider has more than one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_type: Option<KeyType>,
}

fn default_enabled() -> bool {
    true
}

/// The whole configuration file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Providers the app knows about.
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

/// Everything that can go wrong while reading or writing the configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The OS did not give us a config directory to work with.
    #[error("could not resolve the configuration directory")]
    NoConfigDir,
    /// The file exists but could not be read.
    #[error("could not read {path}: {source}")]
    Read {
        /// File that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The file could not be written.
    #[error("could not write {path}: {source}")]
    Write {
        /// File that failed to write.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The file is not valid TOML, or does not match the schema.
    #[error("invalid configuration: {0}")]
    Parse(#[from] toml::de::Error),
    /// The file could not be serialized back to TOML.
    #[error("could not serialize the configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
    /// The same provider id appears twice.
    #[error("provider \"{0}\" is listed more than once")]
    DuplicateProvider(String),
    /// The caller asked for a provider that the file does not list.
    #[error("provider \"{0}\" is not in the configuration")]
    UnknownProvider(String),
}

/// Resolves the configuration file path inside the app config directory.
pub fn path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, ConfigError> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|_| ConfigError::NoConfigDir)?;
    Ok(dir.join(CONFIG_FILE))
}

/// Loads the configuration, writing the documented default on first run.
///
/// A malformed file is reported as [`ConfigError::Parse`] rather than being
/// silently replaced, so a typo never destroys what the user wrote.
pub fn load<R: Runtime>(app: &AppHandle<R>) -> Result<Config, ConfigError> {
    let path = path(app)?;

    if !path.exists() {
        write(&path, TEMPLATE)?;
    }

    let raw = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
        path: path.clone(),
        source,
    })?;

    let config: Config = toml::from_str(&raw)?;
    validate(&config)?;

    Ok(config)
}

/// Sets `enabled` for one provider and persists the file.
///
/// The file is rewritten from the parsed values, so the bundled documentation
/// header survives but comments added inside the provider entries do not.
pub fn set_provider_enabled<R: Runtime>(
    app: &AppHandle<R>,
    provider_id: &str,
    enabled: bool,
) -> Result<Config, ConfigError> {
    let path = path(app)?;
    let mut config = load(app)?;

    match config.providers.iter_mut().find(|it| it.id == provider_id) {
        Some(provider) => provider.enabled = enabled,
        None => return Err(ConfigError::UnknownProvider(provider_id.to_string())),
    }

    let body = toml::to_string_pretty(&config)?;
    write(&path, &format!("{}\n{}", header(), body))?;
    Ok(config)
}

/// The documentation comment block that precedes the provider entries.
fn header() -> &'static str {
    TEMPLATE
        .split_once(PROVIDERS_MARKER)
        .map(|(head, _)| head)
        .unwrap_or("")
}

fn write(path: &Path, contents: &str) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    fs::write(path, contents).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn validate(config: &Config) -> Result<(), ConfigError> {
    let mut seen = HashSet::new();

    for provider in &config.providers {
        if !seen.insert(provider.id.as_str()) {
            return Err(ConfigError::DuplicateProvider(provider.id.clone()));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<Config, ConfigError> {
        let config: Config = toml::from_str(raw)?;
        validate(&config)?;
        Ok(config)
    }

    #[test]
    fn the_bundled_template_is_valid() {
        let config = parse(TEMPLATE).expect("template should parse");
        assert_eq!(config.providers.len(), 2);
        assert_eq!(config.providers[0].id, "openrouter");
        assert_eq!(config.providers[0].key_type, Some(KeyType::Management));
        assert!(config.providers.iter().all(|it| it.enabled));
    }

    #[test]
    fn the_header_keeps_the_documentation() {
        let header = header();
        assert!(header.contains("API keys are NOT stored here"));
        // The documentation mentions `[[providers]]` inline, so a naive split
        // would cut the header short and leave the comment glued to the first
        // table header. These lines come after that mention.
        assert!(header.contains("key_type"));
        assert!(!header.contains("\n[[providers]]"));
    }

    #[test]
    fn enabled_defaults_to_true_when_omitted() {
        let config = parse("[[providers]]\nid = \"deepseek\"\n").expect("should parse");
        assert!(config.providers[0].enabled);
        assert_eq!(config.providers[0].key_type, None);
    }

    #[test]
    fn an_empty_file_is_a_valid_empty_config() {
        let config = parse("").expect("should parse");
        assert!(config.providers.is_empty());
    }

    #[test]
    fn duplicate_providers_are_rejected() {
        let raw = "[[providers]]\nid = \"deepseek\"\n\n[[providers]]\nid = \"deepseek\"\n";
        assert!(matches!(
            parse(raw),
            Err(ConfigError::DuplicateProvider(id)) if id == "deepseek"
        ));
    }

    #[test]
    fn malformed_toml_is_reported() {
        assert!(matches!(
            parse("this is not toml"),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn the_parse_error_message_is_actionable() {
        let error = parse("this is not toml").expect_err("should fail");
        assert!(error.to_string().contains("invalid configuration"));
    }

    #[test]
    fn a_provider_without_an_id_is_rejected() {
        assert!(matches!(
            parse("[[providers]]\nenabled = true\n"),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn round_tripping_keeps_the_values() {
        let config = parse(TEMPLATE).expect("template should parse");
        let body = toml::to_string_pretty(&config).expect("should serialize");
        let reparsed = parse(&format!("{}\n{}", header(), body)).expect("should parse again");

        assert_eq!(reparsed.providers.len(), config.providers.len());
        assert_eq!(reparsed.providers[1].id, "deepseek");
        assert_eq!(reparsed.providers[0].key_type, Some(KeyType::Management));
    }
}
