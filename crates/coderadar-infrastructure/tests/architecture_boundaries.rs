//! Architectural boundary enforcement, executable on any platform.
//!
//! These tests read `cargo metadata` and fail the moment the Clean
//! Architecture Dependency Rule is broken by a `Cargo.toml` edit — for example
//! adding `tokio` to `coderadar-usecases`. They complement the shell linter in
//! `scripts/check-architecture.sh`.

use std::process::Command;

use serde_json::Value;

fn workspace_metadata() -> Value {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata should run");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("cargo metadata returns valid JSON")
}

fn dependency_names(metadata: &Value, package: &str) -> Vec<String> {
    let pkg = metadata["packages"]
        .as_array()
        .expect("packages array")
        .iter()
        .find(|pkg| pkg["name"] == package)
        .unwrap_or_else(|| panic!("package `{package}` is a workspace member"));
    pkg["dependencies"]
        .as_array()
        .expect("dependencies array")
        .iter()
        .map(|dep| dep["name"].as_str().expect("dependency name").to_string())
        .collect()
}

#[test]
fn domain_core_has_zero_dependencies() {
    let metadata = workspace_metadata();
    let deps = dependency_names(&metadata, "coderadar-domain");
    assert!(
        deps.is_empty(),
        "coderadar-domain must use `std` only, but found: {deps:?}"
    );
}

#[test]
fn usecases_depends_only_on_domain() {
    let metadata = workspace_metadata();
    let deps = dependency_names(&metadata, "coderadar-usecases");
    assert_eq!(
        deps,
        vec!["coderadar-domain".to_string()],
        "coderadar-usecases may only depend on coderadar-domain, but found: {deps:?}"
    );
}

#[test]
fn usecases_has_no_async_runtime_or_outer_layers() {
    let metadata = workspace_metadata();
    let deps = dependency_names(&metadata, "coderadar-usecases");
    let forbidden = [
        "tokio",
        "async-std",
        "smol",
        "futures",
        "reqwest",
        "sqlx",
        "hyper",
        "coderadar-adapters",
        "coderadar-infrastructure",
        "serde",
    ];
    for name in forbidden {
        assert!(
            !deps.iter().any(|dep| dep == name),
            "forbidden dependency `{name}` in coderadar-usecases"
        );
    }
}

#[test]
fn adapters_never_depend_on_infrastructure() {
    let metadata = workspace_metadata();
    let deps = dependency_names(&metadata, "coderadar-adapters");
    assert!(
        !deps.iter().any(|dep| dep == "coderadar-infrastructure"),
        "coderadar-adapters must not depend on coderadar-infrastructure"
    );
}
