//! `.coderadar.json` layer mapping with fallback conventions.
//!
//! Implements `CR-SPEC-02 §4`: CodeRadar learns a project's Clean Architecture
//! boundaries either from an optional `.coderadar.json` at the scan root or
//! from conventional directory heuristics. The map answers two questions the
//! Tier 0 `ARCH-DEP-001` rule needs:
//!
//! * which layer does a (relative) file path belong to?
//! * which layer does a relative import specifier resolve to?

use std::path::Path;

use coderadar_domain::DomainError;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Deserialize;

/// The four Clean Architecture layers, ordered innermost → outermost.
///
/// The ordering is load-bearing: a file in layer *L* violates the Dependency
/// Rule when it imports a module in a layer `> L`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProjectLayer {
    /// Enterprise business rules (innermost).
    Domain,
    /// Application business rules.
    UseCases,
    /// Interface adapters.
    Adapters,
    /// Frameworks & drivers (outermost).
    Infrastructure,
}

impl ProjectLayer {
    /// All layers, innermost first.
    pub const ALL: [Self; 4] = [
        Self::Domain,
        Self::UseCases,
        Self::Adapters,
        Self::Infrastructure,
    ];

    /// Canonical token used by `.coderadar.json` keys.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Domain => "domain",
            Self::UseCases => "usecases",
            Self::Adapters => "adapters",
            Self::Infrastructure => "infrastructure",
        }
    }

    /// Depth of the layer in the onion: `0` = domain, `3` = infrastructure.
    pub const fn depth(self) -> u8 {
        match self {
            Self::Domain => 0,
            Self::UseCases => 1,
            Self::Adapters => 2,
            Self::Infrastructure => 3,
        }
    }
}

/// Where a [`LayerMap`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerMapSource {
    /// Loaded from a `.coderadar.json` file.
    ConfigFile,
    /// Built-in conventional heuristics.
    Fallback,
}

impl LayerMapSource {
    /// Canonical token used in reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigFile => "config_file",
            Self::Fallback => "fallback",
        }
    }
}

/// Conventional layer globs (`CR-SPEC-02 §4`), used when a layer is absent
/// from `.coderadar.json`.
const fn fallback_globs(layer: ProjectLayer) -> &'static [&'static str] {
    match layer {
        ProjectLayer::Domain => &["src/domain/**", "lib/core/**", "domain/**"],
        ProjectLayer::UseCases => &["src/usecases/**", "src/application/**", "usecases/**"],
        ProjectLayer::Adapters => &["src/adapters/**", "src/controllers/**", "src/presenters/**"],
        ProjectLayer::Infrastructure => {
            &["src/infra/**", "src/infrastructure/**", "src/database/**"]
        }
    }
}

/// File extensions probed when resolving a relative import specifier.
const IMPORT_EXTENSIONS: [&str; 6] = [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"];

/// Directory-index suffixes probed when resolving a relative import specifier.
const IMPORT_INDEX_SUFFIXES: [&str; 4] = ["/index.ts", "/index.tsx", "/index.js", "/index.jsx"];

/// The `.coderadar.json` document shape.
///
/// Unknown keys are ignored; `architecture_pattern` is accepted for
/// forward-compatibility but does not influence the mapping in v1.0.
#[derive(Deserialize)]
struct ConfigDocument {
    #[serde(default, rename = "architecture_pattern")]
    _architecture_pattern: Option<String>,
    #[serde(default)]
    layers: ConfigLayers,
}

/// The `layers` object of a `.coderadar.json` document.
///
/// A layer key that is absent (`None`) falls back to its conventional globs;
/// a present-but-empty array disables that layer's globs entirely.
#[derive(Deserialize, Default)]
struct ConfigLayers {
    #[serde(default)]
    domain: Option<Vec<String>>,
    #[serde(default)]
    usecases: Option<Vec<String>>,
    #[serde(default)]
    adapters: Option<Vec<String>>,
    #[serde(default)]
    infrastructure: Option<Vec<String>>,
}

impl ConfigLayers {
    /// Configured globs for `layer`, or `None` when the key is absent.
    fn configured(&self, layer: ProjectLayer) -> Option<&[String]> {
        match layer {
            ProjectLayer::Domain => self.domain.as_deref(),
            ProjectLayer::UseCases => self.usecases.as_deref(),
            ProjectLayer::Adapters => self.adapters.as_deref(),
            ProjectLayer::Infrastructure => self.infrastructure.as_deref(),
        }
    }
}

/// Maps relative file paths and import specifiers to project layers.
#[derive(Debug, Clone)]
pub struct LayerMap {
    matchers: Vec<(ProjectLayer, GlobSet)>,
    source: LayerMapSource,
}

impl LayerMap {
    // Capture the exact configuration bytes used by the router (CR-SPEC-17 §3).
    pub(crate) fn capture_full(root: &Path) -> (Self, Result<Option<String>, ()>) {
        match read_configuration(root) {
            Ok(Some(bytes)) => {
                let fingerprint = crate::hashing::sha256_hex_bytes(&bytes);
                let map = std::str::from_utf8(&bytes)
                    .ok()
                    .and_then(|json| Self::from_json(json).ok())
                    .unwrap_or_else(Self::fallback);
                (map, Ok(Some(fingerprint)))
            }
            Ok(None) => (Self::fallback(), Ok(None)),
            Err(_) => (Self::fallback(), Err(())),
        }
    }

    /// Load configuration from the captured index, never from the working tree.
    /// Malformed configuration falls back to conventions, as in [`Self::load`].
    ///
    /// # Errors
    /// Returns an error if the captured configuration cannot be read or is oversized.
    pub fn load_staged(
        snapshot: &crate::git_changeset::StagedSnapshot,
    ) -> Result<Self, DomainError> {
        if snapshot.blob_id(".coderadar.json").is_none() {
            return Ok(Self::fallback());
        }
        let bytes = snapshot
            .read_blob(".coderadar.json", crate::scanner::MAX_FILE_BYTES)?
            .ok_or_else(|| {
                DomainError::External("staged .coderadar.json exceeds the size limit".to_string())
            })?;
        Ok(std::str::from_utf8(&bytes)
            .ok()
            .and_then(|json| Self::from_json(json).ok())
            .unwrap_or_else(Self::fallback))
    }

    /// Load `root/.coderadar.json`; on any error fall back to conventions.
    ///
    /// A malformed file never aborts a scan — it degrades to the fallback
    /// conventions and is reported through [`LayerMap::source`].
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(root.join(".coderadar.json"))
            .ok()
            .and_then(|json| Self::from_json(&json).ok())
            .unwrap_or_else(Self::fallback)
    }

    /// Parse a `.coderadar.json` document.
    ///
    /// Missing layer keys fall back to that layer's conventional globs;
    /// unknown keys are ignored.
    ///
    /// # Errors
    /// Returns [`DomainError::Parse`] when the JSON is malformed or a glob is
    /// invalid.
    pub fn from_json(json: &str) -> Result<Self, DomainError> {
        let document: ConfigDocument = serde_json::from_str(json)
            .map_err(|error| DomainError::parse(format!("invalid .coderadar.json: {error}")))?;
        let mut matchers = Vec::with_capacity(ProjectLayer::ALL.len());
        for layer in ProjectLayer::ALL {
            let set = match document.layers.configured(layer) {
                Some(patterns) => compile_globs(layer, patterns.iter().map(String::as_str))?,
                None => compile_globs(layer, fallback_globs(layer).iter().copied())
                    .expect("built-in layer globs are valid"),
            };
            matchers.push((layer, set));
        }
        Ok(Self {
            matchers,
            source: LayerMapSource::ConfigFile,
        })
    }

    /// Built-in conventional heuristics (`CR-SPEC-02 §4`).
    pub fn fallback() -> Self {
        let matchers = ProjectLayer::ALL
            .into_iter()
            .map(|layer| {
                let set = compile_globs(layer, fallback_globs(layer).iter().copied())
                    .expect("built-in layer globs are valid");
                (layer, set)
            })
            .collect();
        Self {
            matchers,
            source: LayerMapSource::Fallback,
        }
    }

    /// Where this map came from.
    pub const fn source(&self) -> LayerMapSource {
        self.source
    }

    /// Layer of a slash-separated path relative to the scan root.
    ///
    /// Layers are probed innermost-first, so when globs of several layers
    /// match the same path the innermost matching layer wins.
    pub fn layer_of(&self, relative_path: &str) -> Option<ProjectLayer> {
        self.matchers
            .iter()
            .find(|(_, globs)| globs.is_match(relative_path))
            .map(|(layer, _)| *layer)
    }

    /// Layer that `specifier` resolves to when imported from `importing_file`.
    ///
    /// Only relative specifiers (`./`, `../`) resolve; bare package specifiers
    /// return `None` (the `ARCH-DEP-001` framework list handles those).
    /// Resolution is purely lexical — the filesystem is never consulted — and
    /// a specifier that would climb above the scan root does not resolve.
    /// Candidates are probed in order: the resolved path as-is, then the
    /// TypeScript/JavaScript extensions, then directory `index` files.
    pub fn layer_of_import(&self, importing_file: &str, specifier: &str) -> Option<ProjectLayer> {
        let resolved = resolve_relative_specifier(importing_file, specifier)?;
        if let Some(layer) = self.layer_of(&resolved) {
            return Some(layer);
        }
        let extensions = IMPORT_EXTENSIONS
            .iter()
            .map(|extension| format!("{resolved}{extension}"));
        let index_files = IMPORT_INDEX_SUFFIXES
            .iter()
            .map(|suffix| format!("{resolved}{suffix}"));
        extensions
            .chain(index_files)
            .find_map(|candidate| self.layer_of(&candidate))
    }
}

/// Compile one layer's glob patterns into a [`GlobSet`].
fn compile_globs<'a>(
    layer: ProjectLayer,
    patterns: impl IntoIterator<Item = &'a str>,
) -> Result<GlobSet, DomainError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            DomainError::parse(format!(
                "invalid glob `{pattern}` for the `{}` layer: {error}",
                layer.as_str()
            ))
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| {
        DomainError::parse(format!(
            "invalid `{}` layer glob set: {error}",
            layer.as_str()
        ))
    })
}

/// Lexically resolve a relative import specifier against the importing file.
///
/// Returns `None` for bare specifiers and for specifiers that would climb
/// above the scan root; `./` and `../` segments are normalized without ever
/// touching the filesystem.
fn resolve_relative_specifier(importing_file: &str, specifier: &str) -> Option<String> {
    if !specifier.starts_with('.') {
        return None;
    }
    let mut segments: Vec<&str> = importing_file
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    segments.pop();
    for component in specifier.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments.join("/"))
}

fn read_configuration(root: &Path) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read;
    let file = match std::fs::File::open(root.join(".coderadar.json")) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(crate::scanner::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > crate::scanner::MAX_FILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "configuration exceeds size limit",
        ));
    }
    Ok(Some(bytes))
}

pub(crate) fn configuration_fingerprint(root: &Path) -> Result<Option<String>, ()> {
    read_configuration(root)
        .map(|bytes| bytes.map(|bytes| crate::hashing::sha256_hex_bytes(&bytes)))
        .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{LayerMap, LayerMapSource, ProjectLayer};

    fn assert_layer(map: &LayerMap, path: &str, layer: ProjectLayer) {
        assert_eq!(map.layer_of(path), Some(layer), "path `{path}`");
    }

    #[test]
    fn fallback_maps_every_conventional_directory() {
        let map = LayerMap::fallback();
        for path in [
            "src/domain/order.ts",
            "lib/core/entity.ts",
            "domain/order.ts",
        ] {
            assert_layer(&map, path, ProjectLayer::Domain);
        }
        for path in [
            "src/usecases/create_order.ts",
            "src/application/create_order.ts",
            "usecases/create_order.ts",
        ] {
            assert_layer(&map, path, ProjectLayer::UseCases);
        }
        for path in [
            "src/adapters/http/router.ts",
            "src/controllers/orders.ts",
            "src/presenters/order_presenter.ts",
        ] {
            assert_layer(&map, path, ProjectLayer::Adapters);
        }
        for path in [
            "src/infra/db/client.ts",
            "src/infrastructure/db/client.ts",
            "src/database/migrations.ts",
        ] {
            assert_layer(&map, path, ProjectLayer::Infrastructure);
        }
        assert_eq!(map.layer_of("src/utils/format.ts"), None);
    }

    #[test]
    fn fallback_reports_its_source() {
        assert_eq!(LayerMap::fallback().source(), LayerMapSource::Fallback);
    }

    #[test]
    fn config_file_globs_override_and_missing_layers_fall_back() {
        let json = r#"{
            "architecture_pattern": "clean_architecture",
            "layers": { "domain": ["packages/core/**"] }
        }"#;
        let map = LayerMap::from_json(json).expect("valid config");
        assert_eq!(map.source(), LayerMapSource::ConfigFile);
        assert_layer(&map, "packages/core/order.ts", ProjectLayer::Domain);
        assert_eq!(
            map.layer_of("src/domain/order.ts"),
            None,
            "overridden layer globs replace the fallback"
        );
        assert_layer(
            &map,
            "src/infrastructure/db.ts",
            ProjectLayer::Infrastructure,
        );
    }

    #[test]
    fn recursive_globs_match_nested_files() {
        let map = LayerMap::from_json(r#"{"layers":{"domain":["src/domain/**"]}}"#).unwrap();
        assert_layer(&map, "src/domain/order.ts", ProjectLayer::Domain);
        assert_layer(&map, "src/domain/nested/order.ts", ProjectLayer::Domain);
    }

    #[test]
    fn empty_layer_array_disables_that_layer() {
        let map = LayerMap::from_json(r#"{"layers":{"domain":[]}}"#).unwrap();
        assert_eq!(map.layer_of("src/domain/order.ts"), None);
        assert_layer(&map, "src/usecases/create.ts", ProjectLayer::UseCases);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let json = r#"{
            "future_feature": true,
            "layers": { "domain": ["core/**"], "mystery": ["x/**"] }
        }"#;
        let map = LayerMap::from_json(json).expect("unknown keys are ignored");
        assert_layer(&map, "core/entity.ts", ProjectLayer::Domain);
    }

    #[test]
    fn overlapping_globs_resolve_to_the_innermost_layer() {
        let json = r#"{"layers":{"domain":["src/**"],"infrastructure":["src/infra/**"]}}"#;
        let map = LayerMap::from_json(json).unwrap();
        assert_layer(&map, "src/infra/db.ts", ProjectLayer::Domain);
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        let error = LayerMap::from_json("{ not json").expect_err("malformed json");
        assert!(
            matches!(error, coderadar_domain::DomainError::Parse(_)),
            "got {error:?}"
        );
    }

    #[test]
    fn invalid_glob_is_a_parse_error() {
        let error =
            LayerMap::from_json(r#"{"layers":{"domain":["["]}}"#).expect_err("invalid glob");
        assert!(
            matches!(error, coderadar_domain::DomainError::Parse(_)),
            "got {error:?}"
        );
    }

    #[test]
    fn load_uses_valid_config_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".coderadar.json"),
            r#"{"layers":{"domain":["core/**"]}}"#,
        )
        .unwrap();
        let map = LayerMap::load(dir.path());
        assert_eq!(map.source(), LayerMapSource::ConfigFile);
        assert_layer(&map, "core/order.ts", ProjectLayer::Domain);
    }

    #[test]
    fn load_falls_back_when_config_is_malformed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".coderadar.json"), "{ oops").unwrap();
        let map = LayerMap::load(dir.path());
        assert_eq!(map.source(), LayerMapSource::Fallback);
        assert_layer(&map, "src/domain/order.ts", ProjectLayer::Domain);
    }

    #[test]
    fn load_falls_back_when_config_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let map = LayerMap::load(dir.path());
        assert_eq!(map.source(), LayerMapSource::Fallback);
    }

    #[test]
    fn relative_imports_resolve_lexically() {
        let map = LayerMap::fallback();
        let importing = "src/domain/order.ts";
        assert_eq!(
            map.layer_of_import(importing, "./x"),
            Some(ProjectLayer::Domain)
        );
        assert_eq!(
            map.layer_of_import(importing, "../infrastructure/db"),
            Some(ProjectLayer::Infrastructure)
        );
        assert_eq!(
            map.layer_of_import(importing, "./order"),
            Some(ProjectLayer::Domain),
            "extension candidates"
        );
        assert_eq!(
            map.layer_of_import(importing, "../usecases"),
            Some(ProjectLayer::UseCases),
            "index candidates"
        );
    }

    #[test]
    fn extension_and_index_candidates_are_probed() {
        let map = LayerMap::fallback();
        let importing = "src/domain/order.ts";
        assert_eq!(
            map.layer_of_import(importing, "../adapters/rest"),
            Some(ProjectLayer::Adapters)
        );
        assert_eq!(
            map.layer_of_import(importing, "../controllers/http"),
            Some(ProjectLayer::Adapters)
        );
    }

    #[test]
    fn imports_may_not_escape_the_scan_root() {
        let map = LayerMap::fallback();
        assert_eq!(
            map.layer_of_import("src/domain/order.ts", "../../../infrastructure/db"),
            None
        );
        assert_eq!(
            map.layer_of_import("src/domain/order.ts", "./../../../x"),
            None
        );
    }

    #[test]
    fn bare_specifiers_do_not_resolve() {
        let map = LayerMap::fallback();
        assert_eq!(map.layer_of_import("src/domain/order.ts", "express"), None);
        assert_eq!(
            map.layer_of_import("src/domain/order.ts", "@nestjs/common"),
            None
        );
    }

    #[test]
    fn unresolved_relative_specifiers_return_none() {
        let map = LayerMap::fallback();
        assert_eq!(
            map.layer_of_import("src/domain/order.ts", "../shared/util"),
            None,
            "`src/shared/` belongs to no layer"
        );
    }
}
