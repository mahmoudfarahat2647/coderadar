//! Desktop AppSec posture of CodeRadar Studio (`NFR-05`, `CR-SPEC-01` §4).
//!
//! These tests are the cross-platform twin of `scripts/check-security.sh`:
//! the shell guard runs in CI on Linux, this test runs on **every** machine
//! that runs `cargo test` — Windows, macOS and Linux alike.
//!
//! Seven invariants are asserted, one per `#[test]`:
//!
//! 1. the product sources never spawn a process (no shell injection surface);
//! 2. neither the backend nor the webview invokes a shell interpreter;
//! 3. the webview CSP is strict and free of any remote origin;
//! 4. the Tauri ACL grants no filesystem / shell / process / http / oci
//!    permission;
//! 5. the webview loads no remote resource;
//! 6. the API key is never persisted in plaintext;
//! 7. every workspace write goes through the workspace guard.
//!
//! The forbidden tokens are assembled at run time from fragments so that this
//! file never contains a complete forbidden token — the shell guard greps the
//! same tree and must not trip on the guard itself.

use std::fs;
use std::path::{Path, PathBuf};

/// Directories that never contain product code.
const SKIPPED_DIRECTORIES: [&str; 6] =
    ["tests", "scripts", "node_modules", "target", "dist", ".git"];

/// The repository root, when the test runs from a normal checkout.
///
/// `CARGO_MANIFEST_DIR` is `<root>/crates/coderadar-infrastructure`, so the
/// root is two levels up.
fn repository_root() -> Option<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.parent()?.parent()?;
    if root.join("Cargo.toml").is_file() && root.join("src-tauri").is_dir() {
        Some(root.to_path_buf())
    } else {
        None
    }
}

/// Announce that a check could not run.
fn skip(reason: &str) {
    eprintln!("[security] {reason}: the check is skipped");
}

/// A handle on the repository under test.
struct Repository {
    root: PathBuf,
}

impl Repository {
    /// Locate the repository, or `None` when it cannot be found.
    fn locate() -> Option<Self> {
        repository_root().map(|root| Self { root })
    }

    /// Absolute path of a repository-relative path.
    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// Read a repository file; an unreadable file reads as empty.
    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path(relative)).unwrap_or_default()
    }

    /// A short, repository-relative, platform-independent path for messages.
    fn display(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// The `src` directory of every crate of the workspace.
    fn crate_source_dirs(&self) -> Vec<PathBuf> {
        [
            "crates/coderadar-domain/src",
            "crates/coderadar-usecases/src",
            "crates/coderadar-adapters/src",
            "crates/coderadar-infrastructure/src",
        ]
        .iter()
        .map(|relative| self.path(relative))
        .collect()
    }

    /// Every source directory that ships in the desktop binary.
    fn product_source_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = self.crate_source_dirs();
        dirs.push(self.path("src-tauri/src"));
        dirs
    }

    /// Every Rust file of the given directories.
    fn rust_files(&self, dirs: &[PathBuf]) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for dir in dirs {
            collect_files(dir, &mut files);
        }
        files
            .into_iter()
            .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
            .collect()
    }

    /// Every frontend source file (`src/`) plus `index.html`.
    fn frontend_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        collect_files(&self.path("src"), &mut files);
        let index = self.path("index.html");
        if index.is_file() {
            files.push(index);
        }
        files
            .into_iter()
            .filter(|path| {
                matches!(
                    path.extension().and_then(|extension| extension.to_str()),
                    Some("ts" | "tsx" | "js" | "jsx" | "css" | "html" | "json")
                )
            })
            .collect()
    }
}

/// Collect every file below `dir`, skipping the well-known non-product
/// directories.
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    for path in paths {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.is_dir() {
            if SKIPPED_DIRECTORIES.contains(&name.as_str()) {
                continue;
            }
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Read a JSON string body, returning the unescaped value and the remainder.
fn read_json_string(text: &str) -> Option<(String, &str)> {
    let mut value = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some((value, chars.as_str())),
            '\\' => {
                let escaped = chars.next()?;
                match escaped {
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    'r' => value.push('\r'),
                    'b' => value.push('\u{8}'),
                    'f' => value.push('\u{c}'),
                    'u' => {
                        let hex: String = chars.by_ref().take(4).collect();
                        let code = u32::from_str_radix(&hex, 16).ok()?;
                        value.push(char::from_u32(code)?);
                    }
                    other => value.push(other),
                }
            }
            other => value.push(other),
        }
    }
    None
}

/// The value of a JSON string field.
fn json_string_field(text: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let start = text.find(&needle)? + needle.len();
    let rest = text[start..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    read_json_string(rest).map(|(value, _)| value)
}

/// Every quoted string of a JSON fragment.
fn json_strings(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        match read_json_string(after) {
            Some((value, tail)) => {
                values.push(value);
                rest = tail;
            }
            None => break,
        }
    }
    values
}

/// The strings of a JSON array field.
fn json_string_array(text: &str, field: &str) -> Vec<String> {
    let needle = format!("\"{field}\"");
    let Some(start) = text.find(&needle) else {
        return Vec::new();
    };
    let rest = &text[start + needle.len()..];
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let body = &rest[open + 1..];
    let end = body.find(']').unwrap_or(body.len());
    json_strings(&body[..end])
}

/// Every `http://` origin referenced by a CSP policy.
fn http_origins(policy: &str) -> Vec<String> {
    let mut origins = Vec::new();
    let mut offset = 0usize;
    while let Some(index) = policy[offset..].find("http://") {
        let start = offset + index + "http://".len();
        let tail = &policy[start..];
        let end = tail
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '-'))
            .unwrap_or(tail.len());
        origins.push(tail[..end].to_string());
        offset = start + end;
    }
    origins
}

/// `true` when the line *loads* a remote resource: a module import, a
/// `fetch`, a CSS `url()`, a tag `src=`, a CSS `@import` or a `<link>`
/// stylesheet. An inert string (a URL inside a quoted example) is not a
/// resource load.
fn is_remote_resource_line(line: &str) -> bool {
    const MARKERS: [&str; 7] = [
        "from", "import(", "fetch(", "url(", "src=", "@import", "<link",
    ];
    MARKERS.iter().any(|marker| line.contains(marker))
}

// ---------------------------------------------------------------------------
// 1. No process is spawned by the product code.
// ---------------------------------------------------------------------------
#[test]
fn the_product_sources_never_spawn_a_process() {
    let Some(repo) = Repository::locate() else {
        skip("the repository root was not found");
        return;
    };

    let needles = [
        format!("std::process:{}", "Command"),
        format!("{}::new", "Command"),
        format!("process:{}", "Command"),
    ];

    let mut offenders = Vec::new();
    for file in repo.rust_files(&repo.product_source_dirs()) {
        let content = fs::read_to_string(&file).unwrap_or_default();
        for (number, line) in content.lines().enumerate() {
            if needles.iter().any(|needle| line.contains(needle.as_str())) {
                offenders.push(format!(
                    "{}:{}: {}",
                    repo.display(&file),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the product sources must never spawn a process:\n{}",
        offenders.join("\n")
    );
}

// ---------------------------------------------------------------------------
// 2. No shell interpreter is invoked.
// ---------------------------------------------------------------------------
#[test]
fn no_shell_interpreter_is_invoked() {
    let Some(repo) = Repository::locate() else {
        skip("the repository root was not found");
        return;
    };

    let needles = [
        format!("sh {}", "-c"),
        format!("bash {}", "-c"),
        format!("cmd {}", "/C"),
        format!("cmd.exe {}", "/C"),
        format!("powershell {}", "-Command"),
        format!("powershell {}", "-EncodedCommand"),
    ];
    let eval_needle = format!("eval{}", "(");

    let mut offenders = Vec::new();
    for file in repo.rust_files(&repo.product_source_dirs()) {
        let content = fs::read_to_string(&file).unwrap_or_default();
        for (number, line) in content.lines().enumerate() {
            if needles.iter().any(|needle| line.contains(needle.as_str())) {
                offenders.push(format!(
                    "{}:{}: {}",
                    repo.display(&file),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    for file in repo.frontend_files() {
        let content = fs::read_to_string(&file).unwrap_or_default();
        for (number, line) in content.lines().enumerate() {
            if needles.iter().any(|needle| line.contains(needle.as_str())) {
                offenders.push(format!(
                    "{}:{}: {}",
                    repo.display(&file),
                    number + 1,
                    line.trim()
                ));
            }
            if let Some(index) = line.find(&eval_needle) {
                let preceded_by_identifier = line[..index]
                    .chars()
                    .next_back()
                    .is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
                if !preceded_by_identifier {
                    offenders.push(format!(
                        "{}:{}: {}",
                        repo.display(&file),
                        number + 1,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "no shell interpreter may be invoked:\n{}",
        offenders.join("\n")
    );
}

// ---------------------------------------------------------------------------
// 3. The webview CSP is strict and free of any remote origin.
// ---------------------------------------------------------------------------
#[test]
fn the_webview_csp_is_strict_and_free_of_any_remote_origin() {
    let Some(repo) = Repository::locate() else {
        skip("the repository root was not found");
        return;
    };

    let config = repo.read("src-tauri/tauri.conf.json");
    let csp = json_string_field(&config, "csp")
        .expect("app.security.csp must be declared in src-tauri/tauri.conf.json");

    for directive in [
        "default-src 'self'",
        "script-src 'self'",
        "object-src 'none'",
        "base-uri 'none'",
        "frame-ancestors 'none'",
        "form-action 'none'",
    ] {
        assert!(
            csp.contains(directive),
            "the CSP must contain `{directive}`; found: {csp}"
        );
    }

    assert!(
        !csp.contains("unsafe-eval"),
        "the CSP must not allow 'unsafe-eval'; found: {csp}"
    );

    let script_src = csp
        .split(';')
        .map(str::trim)
        .find(|directive| directive.starts_with("script-src"))
        .expect("the CSP must declare a 'script-src' directive");
    assert!(
        !script_src.contains("unsafe-inline"),
        "the CSP must not allow 'unsafe-inline' in 'script-src'; found: {script_src}"
    );

    assert!(
        !csp.contains("https://"),
        "the CSP must not reference a remote origin; found: {csp}"
    );
    for origin in http_origins(&csp) {
        assert_eq!(
            origin, "ipc.localhost",
            "the CSP must not reference a remote origin; found: {origin} in {csp}"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. The Tauri ACL grants no dangerous permission.
// ---------------------------------------------------------------------------
#[test]
fn the_tauri_acl_grants_no_dangerous_permission() {
    let Some(repo) = Repository::locate() else {
        skip("the repository root was not found");
        return;
    };

    let capabilities = repo.read("src-tauri/capabilities/default.json");
    let permissions = json_string_array(&capabilities, "permissions");
    assert!(
        !permissions.is_empty(),
        "src-tauri/capabilities/default.json must declare a 'permissions' array"
    );

    for permission in &permissions {
        for prefix in ["fs:", "shell:", "process:", "http:", "oci:"] {
            assert!(
                !permission.starts_with(prefix),
                "the capability set grants `{permission}`; the desktop app must not hold filesystem, shell, process, http or oci permissions"
            );
        }
    }

    assert!(
        permissions
            .iter()
            .any(|permission| permission == "dialog:default"),
        "the capability set must keep 'dialog:default'; found: {permissions:?}"
    );
}

// ---------------------------------------------------------------------------
// 5. The webview loads no remote resource.
// ---------------------------------------------------------------------------
#[test]
fn the_webview_loads_no_remote_resource() {
    let Some(repo) = Repository::locate() else {
        skip("the repository root was not found");
        return;
    };

    let index = repo.read("index.html");
    assert!(
        !index.contains("http://") && !index.contains("https://"),
        "index.html must not reference a remote URL: {index}"
    );

    let mut offenders = Vec::new();
    for file in repo.frontend_files() {
        let content = fs::read_to_string(&file).unwrap_or_default();
        for (number, line) in content.lines().enumerate() {
            if !line.contains("http://") && !line.contains("https://") {
                continue;
            }
            if is_remote_resource_line(line) {
                offenders.push(format!(
                    "{}:{}: {}",
                    repo.display(&file),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "the webview must not load a remote resource:\n{}",
        offenders.join("\n")
    );
}

// ---------------------------------------------------------------------------
// 6. The API key is never persisted in plaintext.
// ---------------------------------------------------------------------------
#[test]
fn the_api_key_is_never_persisted_in_plaintext() {
    let Some(repo) = Repository::locate() else {
        skip("the repository root was not found");
        return;
    };

    // The Tauri shell never writes to the filesystem.
    let write_needles = [
        "fs::write",
        "write_all(",
        "File::create",
        "OpenOptions",
        "fs::remove_file",
        "fs::remove_dir",
        "fs::rename",
        "fs::copy",
    ];
    let mut shell_offenders = Vec::new();
    for file in repo.rust_files(&[repo.path("src-tauri/src")]) {
        let content = fs::read_to_string(&file).unwrap_or_default();
        for (number, line) in content.lines().enumerate() {
            if write_needles.iter().any(|needle| line.contains(needle)) {
                shell_offenders.push(format!(
                    "{}:{}: {}",
                    repo.display(&file),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        shell_offenders.is_empty(),
        "the Tauri shell must not write to the filesystem; the API key must only reach the OS keychain:\n{}",
        shell_offenders.join("\n")
    );

    // The OS keychain is the one and only persistence point.
    let keychain = repo.read("crates/coderadar-infrastructure/src/keychain.rs");
    assert!(
        keychain.contains("keyring::Entry"),
        "the keychain store must use the OS credential store (keyring::Entry)"
    );
    assert!(
        keychain.contains("set_password"),
        "the keychain store must persist the credential through the OS credential store"
    );

    let mut persistence_offenders = Vec::new();
    for file in repo.rust_files(&repo.crate_source_dirs()) {
        let content = fs::read_to_string(&file).unwrap_or_default();
        if !content.contains("set_password") {
            continue;
        }
        if repo.display(&file).ends_with("keychain.rs") {
            continue;
        }
        persistence_offenders.push(repo.display(&file));
    }
    assert!(
        persistence_offenders.is_empty(),
        "the API key must only be persisted by the keychain store; found: {persistence_offenders:?}"
    );

    // A direct filesystem write is only allowed inside a test module.
    let mut write_offenders = Vec::new();
    for file in repo.rust_files(&repo.crate_source_dirs()) {
        let content = fs::read_to_string(&file).unwrap_or_default();
        if content.contains("fs::write") && !content.contains("#[cfg(test)]") {
            write_offenders.push(repo.display(&file));
        }
    }
    assert!(
        write_offenders.is_empty(),
        "a crate must not write a file with fs::write outside a test module: {write_offenders:?}"
    );

    // The frontend keeps the key in memory only.
    let storage_needles = [
        "localStorage",
        "sessionStorage",
        "indexedDB",
        "IndexedDB",
        "document.cookie",
    ];
    let mut storage_offenders = Vec::new();
    for file in repo.frontend_files() {
        let content = fs::read_to_string(&file).unwrap_or_default();
        for (number, line) in content.lines().enumerate() {
            if storage_needles.iter().any(|needle| line.contains(needle)) {
                storage_offenders.push(format!(
                    "{}:{}: {}",
                    repo.display(&file),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        storage_offenders.is_empty(),
        "the frontend must not persist the API key in the web storage:\n{}",
        storage_offenders.join("\n")
    );
}

// ---------------------------------------------------------------------------
// 7. Every workspace write goes through the workspace guard.
// ---------------------------------------------------------------------------
#[test]
fn every_workspace_write_goes_through_the_workspace_guard() {
    let Some(repo) = Repository::locate() else {
        skip("the repository root was not found");
        return;
    };

    let guard = repo.read("crates/coderadar-infrastructure/src/workspace_guard.rs");
    assert!(
        !guard.is_empty(),
        "crates/coderadar-infrastructure/src/workspace_guard.rs must exist"
    );
    assert!(
        guard.contains("pub struct WorkspaceGuard"),
        "the workspace guard must declare 'pub struct WorkspaceGuard'"
    );
    assert!(
        guard.contains("pub fn resolve"),
        "the workspace guard must expose a 'resolve' method"
    );

    let patching = repo.read("crates/coderadar-infrastructure/src/patching.rs");
    assert!(
        patching.contains("WorkspaceGuard"),
        "the patch applier must use the workspace guard"
    );
    assert!(
        patching.contains(".resolve("),
        "the patch applier must resolve the target through the workspace guard"
    );
    assert!(
        patching.contains("pub(crate) fn atomic_write"),
        "'atomic_write' must stay 'pub(crate)' in patching.rs: it is the guarded write primitive"
    );
    assert!(
        !patching.contains("pub fn atomic_write"),
        "'atomic_write' must not be part of the public API of the crate"
    );

    let lib = repo.read("crates/coderadar-infrastructure/src/lib.rs");
    assert!(
        !lib.lines().any(|line| {
            line.trim_start().starts_with("pub use") && line.contains("atomic_write")
        }),
        "'atomic_write' must not be re-exported by the infrastructure crate"
    );

    // The atomic write helper has a closed set of callers: the patch applier
    // (which resolves the target through the workspace guard) and the
    // Markdown export (whose target is an absolute path chosen by the user).
    let mut offenders = Vec::new();
    for file in repo.rust_files(&repo.crate_source_dirs()) {
        let content = fs::read_to_string(&file).unwrap_or_default();
        if !content.contains("atomic_write") {
            continue;
        }
        let name = repo.display(&file);
        let file_name = name.rsplit('/').next().unwrap_or_default();
        if !matches!(file_name, "patching.rs" | "export.rs") {
            offenders.push(name);
        }
    }
    assert!(
        offenders.is_empty(),
        "only the patch applier and the Markdown export may call 'atomic_write'; found: {offenders:?}"
    );
}
