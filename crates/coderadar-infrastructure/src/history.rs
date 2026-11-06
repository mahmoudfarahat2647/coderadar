//! SQLite implementation of the [`ScanHistoryRepository`] port.
//!
//! The scan history is the only durable state of the product: every finished
//! scan is stored with its counters and with a JSON snapshot of its findings,
//! so the history panel can be rendered without re-reading the workspace
//! (`CR-SPEC-13 §6`).
//!
//! `rusqlite` is a **blocking** driver. SQLite calls therefore never run on
//! the thread that drives the UI: every statement is handed over to a
//! dedicated worker thread and the result is returned through a one-shot
//! channel, which keeps the `std::future`-based port of Layer 2 free of any
//! async runtime while still guaranteeing that the UI thread is never
//! blocked by disk access.

use std::path::{Path, PathBuf};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};

use coderadar_domain::{
    DomainError, Finding, ScanCoverage, ScanMode, ScanSession, ScanSessionSummary, ScanUsage,
};
use coderadar_usecases::ScanHistoryRepository;
use rusqlite::{params, Connection, OptionalExtension};

/// SQL schema of the history database.
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS scan_sessions (
         id TEXT PRIMARY KEY,
         root_dir TEXT NOT NULL,
         mode TEXT NOT NULL,
         started_at_unix_ms INTEGER NOT NULL,
         finished_at_unix_ms INTEGER NOT NULL,
         files_scanned INTEGER NOT NULL,
         total_findings INTEGER NOT NULL,
         critical INTEGER NOT NULL,
         high INTEGER NOT NULL,
         medium INTEGER NOT NULL,
         low INTEGER NOT NULL,
         compliance_percent INTEGER,
         suppressed_heuristic_findings INTEGER NOT NULL DEFAULT 0,
         engine_mode TEXT,
         coverage_json TEXT,
         findings_json TEXT NOT NULL
     )";

/// Directory (inside the user home) holding the CodeRadar state.
const STATE_DIRECTORY: &str = ".coderadar";

/// Name of the history database inside the state directory.
const DATABASE_FILE: &str = "history.sqlite3";

/// Placeholder path reported for an in-memory database.
const IN_MEMORY_PATH: &str = ":memory:";

/// A serialisable snapshot of a [`Finding`].
///
/// The snapshot is the persistence format of the findings: it keeps the
/// information the history panel needs without depending on the in-memory
/// layout of the domain.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FindingSnapshot {
    /// Stable identifier of the finding.
    pub id: String,
    /// Path of the offending file.
    pub file_path: String,
    /// First offending line (1-based, inclusive).
    pub line_start: u32,
    /// Last offending line (1-based, inclusive).
    pub line_end: u32,
    /// Rule that produced the finding.
    pub rule_id: String,
    /// Review pillar, as its canonical lowercase token.
    pub category: String,
    /// Severity band, as its canonical uppercase token.
    pub severity: String,
    /// Associated CWE identifier, when applicable.
    pub cwe: Option<String>,
    /// Short title.
    pub title: String,
    /// Longer explanation.
    pub description: String,
    /// Engine that produced the finding, or `unknown` for legacy snapshots.
    #[serde(default = "unknown_engine")]
    pub engine: String,
    /// Evaluator confidence, unknown when absent from legacy snapshots.
    #[serde(default)]
    pub confidence: Option<f32>,
    /// Model probability, unknown for legacy snapshots or other evaluators.
    #[serde(default)]
    pub model_probability: Option<f32>,
    /// Heuristic weight, unknown for legacy snapshots or other evaluators.
    #[serde(default)]
    pub heuristic_weight: Option<f32>,
}

/// Deserialization default for the engine marker of legacy snapshots.
fn unknown_engine() -> String {
    "unknown".to_string()
}

impl FindingSnapshot {
    /// Build a snapshot from a domain finding.
    pub fn from_finding(finding: &Finding) -> Self {
        Self {
            id: finding.id.clone(),
            file_path: finding.file_path.clone(),
            line_start: finding.line_start,
            line_end: finding.line_end,
            rule_id: finding.rule_id.clone(),
            category: finding.category.as_str().to_string(),
            severity: finding.severity.as_str().to_string(),
            cwe: finding.cwe.clone(),
            title: finding.title.clone(),
            description: finding.description.clone(),
            engine: finding.engine.as_str().to_string(),
            confidence: Some(finding.confidence),
            model_probability: finding.model_probability,
            heuristic_weight: finding.heuristic_weight,
        }
    }
}

/// Everything that is written to one row of the history table.
#[derive(Debug, Clone)]
struct SessionRecord {
    id: String,
    root_dir: String,
    mode: String,
    started_at_unix_ms: u64,
    finished_at_unix_ms: u64,
    files_scanned: usize,
    total_findings: usize,
    critical: usize,
    high: usize,
    medium: usize,
    low: usize,
    compliance_percent: Option<u8>,
    suppressed_heuristic_findings: usize,
    engine_mode: Option<String>,
    coverage_json: Option<String>,
    usage_json: Option<String>,
    analyzed_sources_json: Option<String>,
    consumed_sources_json: Option<String>,
    completion: Option<String>,
    findings_json: String,
}

impl SessionRecord {
    fn from_session(session: &ScanSession) -> Result<Self, DomainError> {
        let summary = session.summary();
        let snapshots: Vec<FindingSnapshot> = session
            .findings
            .iter()
            .map(FindingSnapshot::from_finding)
            .collect();
        let findings_json = serde_json::to_string(&snapshots).map_err(|error| {
            DomainError::External(format!("cannot serialise the findings: {error}"))
        })?;
        Ok(Self {
            id: summary.id,
            root_dir: summary.root_dir,
            mode: summary.mode.as_str().to_string(),
            started_at_unix_ms: session.started_at_unix_ms,
            finished_at_unix_ms: session
                .finished_at_unix_ms
                .unwrap_or(session.started_at_unix_ms),
            files_scanned: summary.files_scanned,
            total_findings: summary.total_findings,
            critical: summary.critical,
            high: summary.high,
            medium: summary.medium,
            low: summary.low,
            compliance_percent: summary.compliance_percent,
            suppressed_heuristic_findings: summary.suppressed_heuristic_findings,
            engine_mode: summary.engine_mode.clone(),
            completion: summary.completion.clone(),
            coverage_json: summary
                .coverage
                .map(|c| serde_json::to_string(&CoverageRecord::from(c)))
                .transpose()
                .map_err(|error| {
                    DomainError::External(format!("cannot serialise coverage: {error}"))
                })?,
            usage_json: summary
                .usage
                .map(|usage| serde_json::to_string(&UsageRecord::from(usage)))
                .transpose()
                .map_err(|error| {
                    DomainError::External(format!("cannot serialise usage: {error}"))
                })?,
            analyzed_sources_json: session
                .analyzed_sources
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    DomainError::External(format!("cannot serialise analyzed sources: {error}"))
                })?,
            consumed_sources_json: session
                .consumed_sources
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|error| {
                    DomainError::External(format!("cannot serialise consumed sources: {error}"))
                })?,
            findings_json,
        })
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct UsageRecord {
    primary_batches: u64,
    fallback_batches: u64,
    fallback_failures: u64,
}

impl From<ScanUsage> for UsageRecord {
    fn from(value: ScanUsage) -> Self {
        Self {
            primary_batches: value.primary_batches,
            fallback_batches: value.fallback_batches,
            fallback_failures: value.fallback_failures,
        }
    }
}

impl From<UsageRecord> for ScanUsage {
    fn from(value: UsageRecord) -> Self {
        Self {
            primary_batches: value.primary_batches,
            fallback_batches: value.fallback_batches,
            fallback_failures: value.fallback_failures,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CoverageRecord {
    discovered: usize,
    supported: usize,
    analyzed: usize,
    unsupported: usize,
    excluded: usize,
    #[serde(default)]
    exclusions_complete: Option<bool>,
    unreadable: usize,
    oversized: usize,
    invalid_encoding: usize,
    incomplete: usize,
}

impl From<ScanCoverage> for CoverageRecord {
    fn from(c: ScanCoverage) -> Self {
        Self {
            discovered: c.discovered,
            supported: c.supported,
            analyzed: c.analyzed,
            unsupported: c.unsupported,
            excluded: c.excluded,
            exclusions_complete: c.exclusions_complete,
            unreadable: c.unreadable,
            oversized: c.oversized,
            invalid_encoding: c.invalid_encoding,
            incomplete: c.incomplete,
        }
    }
}

impl From<CoverageRecord> for ScanCoverage {
    fn from(c: CoverageRecord) -> Self {
        Self {
            discovered: c.discovered,
            supported: c.supported,
            analyzed: c.analyzed,
            unsupported: c.unsupported,
            excluded: c.excluded,
            exclusions_complete: c.exclusions_complete,
            unreadable: c.unreadable,
            oversized: c.oversized,
            invalid_encoding: c.invalid_encoding,
            incomplete: c.incomplete,
        }
    }
}

/// Map a SQLite error into a [`DomainError`].
fn sqlite_error(context: &str, error: rusqlite::Error) -> DomainError {
    DomainError::External(format!("{context}: {error}"))
}

/// Run `work` on a dedicated worker thread and return its outcome.
///
/// `rusqlite` is a **blocking** driver: the statement is executed off the
/// calling thread so the UI thread is never blocked by disk access. The
/// result travels back through a one-shot channel; when the channel is
/// broken the failure is reported with the given `context` wording.
fn dispatch<T: Send + 'static>(
    context: &str,
    work: impl FnOnce() -> Result<T, DomainError> + Send + 'static,
) -> Result<T, DomainError> {
    let (sender, receiver) = sync_channel(1);
    std::thread::spawn(move || {
        let result = work();
        let _ = sender.send(result);
    });
    receiver
        .recv()
        .map_err(|error| DomainError::External(format!("{context}: {error}")))?
}

/// Parse a persisted scan mode.
fn parse_mode(value: &str) -> ScanMode {
    match value {
        "staged_diff" => ScanMode::StagedDiff,
        _ => ScanMode::FullScan,
    }
}

/// Upgrade a version-zero database atomically, retaining its table definition
/// and its explicit indexes and triggers. Unexpected definitions fail closed.
fn migrate(connection: &mut Connection) -> Result<(), DomainError> {
    // Dropping the old table must not cascade into tables that reference it.
    let foreign_keys: bool = connection
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .map_err(|e| sqlite_error("cannot inspect history foreign keys", e))?;
    connection
        .pragma_update(None, "foreign_keys", false)
        .map_err(|e| sqlite_error("cannot disable history foreign keys", e))?;
    let result = migrate_transaction(connection);
    connection
        .pragma_update(None, "foreign_keys", foreign_keys)
        .map_err(|e| sqlite_error("cannot restore history foreign keys", e))?;
    result
}

fn migrate_transaction(connection: &mut Connection) -> Result<(), DomainError> {
    let tx = connection
        .transaction()
        .map_err(|e| sqlite_error("cannot start history migration", e))?;
    let version: i64 = tx
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| sqlite_error("cannot read history version", e))?;
    if !(0..=4).contains(&version) {
        return Err(DomainError::External(format!(
            "unsupported history schema version {version}"
        )));
    }
    if version == 0 {
        let schema: Option<String> = tx
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='scan_sessions'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| sqlite_error("cannot inspect history schema", e))?;
        if let Some(schema) = schema {
            let mut statement = tx
                .prepare("PRAGMA table_info(scan_sessions)")
                .map_err(|e| sqlite_error("cannot inspect history columns", e))?;
            let columns: Vec<(String, bool)> = statement
                .query_map([], |r| Ok((r.get(1)?, r.get::<_, i64>(3)? != 0)))
                .map_err(|e| sqlite_error("cannot inspect history columns", e))?
                .collect::<Result<_, _>>()
                .map_err(|e| sqlite_error("cannot inspect history columns", e))?;
            drop(statement);
            let Some((_, score_not_null)) = columns
                .iter()
                .find(|(name, _)| name == "compliance_percent")
            else {
                return Err(DomainError::External(
                    "history schema lacks compliance_percent".into(),
                ));
            };
            let mut new_schema = schema;
            if *score_not_null {
                let upper = new_schema.to_ascii_uppercase();
                let start = upper.find("COMPLIANCE_PERCENT").ok_or_else(|| {
                    DomainError::External("unrecognised history score declaration".into())
                })?;
                let end = upper[start..].find(',').map_or(upper.len(), |n| start + n);
                let clause = &upper[start..end];
                let offset = clause.find("NOT NULL").ok_or_else(|| {
                    DomainError::External("unrecognised history score constraint".into())
                })?;
                new_schema.replace_range(start + offset..start + offset + "NOT NULL".len(), "");
            }
            let mut statement = tx.prepare("SELECT sql FROM sqlite_master WHERE tbl_name='scan_sessions' AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY type, name")
                .map_err(|e| sqlite_error("cannot inspect history indexes", e))?;
            let objects: Vec<String> = statement
                .query_map([], |r| r.get(0))
                .map_err(|e| sqlite_error("cannot inspect history indexes", e))?
                .collect::<Result<_, _>>()
                .map_err(|e| sqlite_error("cannot inspect history indexes", e))?;
            drop(statement);
            let open = new_schema.find('(').ok_or_else(|| {
                DomainError::External("unrecognised history table definition".into())
            })?;
            let staging_schema = format!(
                "CREATE TABLE scan_sessions_migration {}",
                &new_schema[open..]
            );
            tx.execute_batch(&staging_schema)
                .map_err(|e| sqlite_error("cannot rebuild history table", e))?;
            let names = columns
                .iter()
                .map(|(name, _)| format!("\"{}\"", name.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(", ");
            tx.execute_batch(&format!("INSERT INTO scan_sessions_migration ({names}) SELECT {names} FROM scan_sessions; DROP TABLE scan_sessions; ALTER TABLE scan_sessions_migration RENAME TO scan_sessions;"))
                .map_err(|e| sqlite_error("cannot copy history rows", e))?;
            for object in objects {
                tx.execute_batch(&object)
                    .map_err(|e| sqlite_error("cannot restore history index or trigger", e))?;
            }
            for (name, declaration) in [
                (
                    "suppressed_heuristic_findings",
                    "INTEGER NOT NULL DEFAULT 0",
                ),
                ("engine_mode", "TEXT"),
                ("coverage_json", "TEXT"),
            ] {
                if !columns.iter().any(|(column, _)| column == name) {
                    tx.execute_batch(&format!(
                        "ALTER TABLE scan_sessions ADD COLUMN {name} {declaration}"
                    ))
                    .map_err(|e| sqlite_error("cannot add history column", e))?;
                }
            }
            let nullable: bool = tx.query_row("SELECT [notnull] = 0 FROM pragma_table_info('scan_sessions') WHERE name='compliance_percent'", [], |r| r.get(0))
                .map_err(|e| sqlite_error("cannot verify history score", e))?;
            if !nullable {
                return Err(DomainError::External(
                    "history score is not nullable".into(),
                ));
            }
        } else {
            tx.execute_batch(SCHEMA)
                .map_err(|e| sqlite_error("cannot create history schema", e))?;
        }
        tx.pragma_update(None, "user_version", 1)
            .map_err(|e| sqlite_error("cannot update history version", e))?;
    }
    if version < 2 {
        tx.execute_batch("ALTER TABLE scan_sessions ADD COLUMN usage_json TEXT")
            .map_err(|e| sqlite_error("cannot add history usage", e))?;
        tx.pragma_update(None, "user_version", 2)
            .map_err(|e| sqlite_error("cannot update history version", e))?;
    }
    if version < 3 {
        tx.execute_batch("ALTER TABLE scan_sessions ADD COLUMN analyzed_sources_json TEXT; ALTER TABLE scan_sessions ADD COLUMN consumed_sources_json TEXT")
            .map_err(|e| sqlite_error("cannot add history source provenance", e))?;
        tx.pragma_update(None, "user_version", 3)
            .map_err(|e| sqlite_error("cannot update history version", e))?;
    }
    if version < 4 {
        tx.execute_batch("ALTER TABLE scan_sessions ADD COLUMN completion TEXT")
            .map_err(|e| sqlite_error("cannot add history completion", e))?;
        tx.pragma_update(None, "user_version", 4)
            .map_err(|e| sqlite_error("cannot update history version", e))?;
    }
    tx.commit()
        .map_err(|e| sqlite_error("cannot commit history migration", e))
}

/// SQLite-backed scan history repository.
#[derive(Debug, Clone)]
pub struct SqliteScanHistoryRepository {
    connection: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl SqliteScanHistoryRepository {
    /// Open (or create) the history database at `path`.
    ///
    /// The parent directory is created when it does not exist yet.
    ///
    /// # Errors
    /// Returns [`DomainError::Io`] when the directory cannot be created and
    /// [`DomainError::External`] when the database cannot be opened or the
    /// schema cannot be applied.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DomainError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    DomainError::Io(format!(
                        "cannot create the history directory `{}`: {error}",
                        parent.display()
                    ))
                })?;
            }
        }
        let connection = Connection::open(&path)
            .map_err(|error| sqlite_error("cannot open the history database", error))?;
        Self::from_connection(connection, path)
    }

    /// Open an in-memory history database.
    ///
    /// The database lives for the lifetime of the repository only; nothing is
    /// written to the filesystem.
    ///
    /// # Errors
    /// Returns [`DomainError::External`] when the database cannot be opened or
    /// the schema cannot be applied.
    pub fn open_in_memory() -> Result<Self, DomainError> {
        let connection = Connection::open_in_memory()
            .map_err(|error| sqlite_error("cannot open the in-memory history database", error))?;
        Self::from_connection(connection, PathBuf::from(IN_MEMORY_PATH))
    }

    /// Build a repository around an open connection and apply the schema.
    fn from_connection(mut connection: Connection, path: PathBuf) -> Result<Self, DomainError> {
        migrate(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path,
        })
    }

    /// The path of the history database.
    ///
    /// For an in-memory database this is the placeholder `:memory:`: no file
    /// is backing that database.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The default location of the history database:
    /// `<user home>/.coderadar/history.sqlite3`.
    ///
    /// The directory is created lazily by [`SqliteScanHistoryRepository::open`].
    ///
    /// # Errors
    /// Returns [`DomainError::Io`] when the home directory cannot be
    /// determined.
    pub fn default_path() -> Result<PathBuf, DomainError> {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .ok_or_else(|| {
                DomainError::Io(
                    "the home directory of the current user cannot be determined".to_string(),
                )
            })?;
        let mut path = PathBuf::from(home);
        path.push(STATE_DIRECTORY);
        path.push(DATABASE_FILE);
        Ok(path)
    }

    /// Read back the findings snapshot of one session.
    ///
    /// # Errors
    /// Returns [`DomainError::External`] when the statement fails and
    /// [`DomainError::Parse`] when the stored JSON is malformed.
    pub fn load_snapshots(&self, session_id: &str) -> Result<Vec<FindingSnapshot>, DomainError> {
        let connection = Arc::clone(&self.connection);
        let session_id = session_id.to_string();
        dispatch("the history reader thread failed", move || {
            read_snapshots(&connection, &session_id)
        })
    }
}

/// Insert one row into the history table.
fn write_record(
    connection: &Arc<Mutex<Connection>>,
    record: &SessionRecord,
) -> Result<(), DomainError> {
    let guard = connection
        .lock()
        .map_err(|_| DomainError::External("the history database is poisoned".to_string()))?;
    guard
        .execute(
            "INSERT OR REPLACE INTO scan_sessions (
                  id, root_dir, mode, started_at_unix_ms, finished_at_unix_ms,
                  files_scanned, total_findings, critical, high, medium, low,
                  compliance_percent, suppressed_heuristic_findings, engine_mode,
                  findings_json, coverage_json, usage_json, analyzed_sources_json, consumed_sources_json, completion
              ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                record.id,
                record.root_dir,
                record.mode,
                record.started_at_unix_ms,
                record.finished_at_unix_ms,
                record.files_scanned as i64,
                record.total_findings as i64,
                record.critical as i64,
                record.high as i64,
                record.medium as i64,
                record.low as i64,
                record.compliance_percent,
                record.suppressed_heuristic_findings as i64,
                record.engine_mode,
                record.findings_json,
                record.coverage_json,
                record.usage_json,
                record.analyzed_sources_json,
                record.consumed_sources_json,
                record.completion,
            ],
        )
        .map_err(|error| sqlite_error("cannot write the scan session", error))?;
    Ok(())
}

/// Read the most recent summaries, newest first.
///
/// When `root` is given, only the sessions of that workspace are returned. The
/// workspace is canonicalised exactly like the stored session, so the filter
/// matches whatever spelling the caller used.
fn read_summaries(
    connection: &Arc<Mutex<Connection>>,
    limit: usize,
    root: Option<&str>,
) -> Result<Vec<ScanSessionSummary>, DomainError> {
    let root = root.map(|root| crate::workspace_guard::canonical_root(std::path::Path::new(root)));
    let guard = connection
        .lock()
        .map_err(|_| DomainError::External("the history database is poisoned".to_string()))?;
    let mut statement = guard
        .prepare(
            "SELECT id, root_dir, mode, finished_at_unix_ms, files_scanned,
                    total_findings, critical, high, medium, low, compliance_percent,
                    suppressed_heuristic_findings, engine_mode, coverage_json, usage_json,
                    analyzed_sources_json, consumed_sources_json, completion
             FROM scan_sessions
             WHERE ?2 IS NULL OR root_dir = ?2
             ORDER BY finished_at_unix_ms DESC, id DESC
             LIMIT ?1",
        )
        .map_err(|error| sqlite_error("cannot prepare the history query", error))?;
    let rows = statement
        .query_map(params![limit as i64, root], |row| {
            Ok(ScanSessionSummary {
                id: row.get(0)?,
                root_dir: row.get(1)?,
                mode: parse_mode(&row.get::<_, String>(2)?),
                finished_at_unix_ms: row.get::<_, i64>(3)? as u64,
                files_scanned: row.get::<_, i64>(4)? as usize,
                total_findings: row.get::<_, i64>(5)? as usize,
                critical: row.get::<_, i64>(6)? as usize,
                high: row.get::<_, i64>(7)? as usize,
                medium: row.get::<_, i64>(8)? as usize,
                low: row.get::<_, i64>(9)? as usize,
                compliance_percent: row.get(10)?,
                suppressed_heuristic_findings: row.get::<_, i64>(11)? as usize,
                engine_mode: row.get(12)?,
                coverage: row
                    .get::<_, Option<String>>(13)?
                    .map(|json| {
                        serde_json::from_str::<CoverageRecord>(&json)
                            .map(ScanCoverage::from)
                            .map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    13,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })
                    })
                    .transpose()?,
                usage: row
                    .get::<_, Option<String>>(14)?
                    .map(|json| {
                        serde_json::from_str::<UsageRecord>(&json)
                            .map(ScanUsage::from)
                            .map_err(|e| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    14,
                                    rusqlite::types::Type::Text,
                                    Box::new(e),
                                )
                            })
                    })
                    .transpose()?,
                analyzed_sources: parse_sources(row, 15)?,
                consumed_sources: parse_sources(row, 16)?,
                completion: row.get(17)?,
            })
        })
        .map_err(|error| sqlite_error("cannot read the scan history", error))?;
    let mut summaries = Vec::new();
    for row in rows {
        summaries.push(row.map_err(|error| sqlite_error("cannot read the scan history", error))?);
    }
    Ok(summaries)
}

fn parse_sources(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<Option<std::collections::BTreeMap<String, String>>> {
    row.get::<_, Option<String>>(index)?
        .map(|json| {
            serde_json::from_str(&json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()
}

/// Read and deserialise the findings snapshot of one session.
fn read_snapshots(
    connection: &Arc<Mutex<Connection>>,
    session_id: &str,
) -> Result<Vec<FindingSnapshot>, DomainError> {
    let guard = connection
        .lock()
        .map_err(|_| DomainError::External("the history database is poisoned".to_string()))?;
    let payload: Option<String> = guard
        .query_row(
            "SELECT findings_json FROM scan_sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| sqlite_error("cannot read the findings snapshot", error))?;
    let Some(payload) = payload else {
        return Ok(Vec::new());
    };
    serde_json::from_str(&payload)
        .map_err(|error| DomainError::Parse(format!("the findings snapshot is malformed: {error}")))
}

impl ScanHistoryRepository for SqliteScanHistoryRepository {
    fn save_scan_session<'a>(
        &'a self,
        session: &'a ScanSession,
    ) -> coderadar_usecases::BoxFuture<'a, Result<(), DomainError>> {
        Box::pin(async move {
            // The record is built here (pure, cheap) so that the worker thread
            // only owns plain data and never borrows the caller.
            let record = SessionRecord::from_session(session)?;
            let connection = Arc::clone(&self.connection);
            dispatch("the history writer thread failed", move || {
                write_record(&connection, &record)
            })
        })
    }

    fn get_history<'a>(
        &'a self,
        limit: usize,
        root: Option<&'a str>,
    ) -> coderadar_usecases::BoxFuture<'a, Result<Vec<ScanSessionSummary>, DomainError>> {
        // The workspace filter is owned by the worker thread: it outlives the
        // borrow of the caller.
        let root = root.map(|root| root.to_string());
        Box::pin(async move {
            let connection = Arc::clone(&self.connection);
            dispatch("the history reader thread failed", move || {
                read_summaries(&connection, limit, root.as_deref())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{FindingSnapshot, SqliteScanHistoryRepository};
    use coderadar_domain::{Finding, ScanMode, ScanSession};
    use coderadar_usecases::ScanHistoryRepository;

    fn session(id: &str, finished: u64, files: usize, findings: Vec<Finding>) -> ScanSession {
        let mut session = ScanSession::new(id, "/workspace", ScanMode::FullScan, finished - 10);
        session.finished_at_unix_ms = Some(finished);
        session.files_scanned = files;
        for finding in findings {
            session.push(finding);
        }
        session
    }

    #[test]
    fn an_in_memory_repository_stores_and_reads_sessions() {
        let repository = SqliteScanHistoryRepository::open_in_memory().expect("repository");
        assert_eq!(repository.path().to_string_lossy(), ":memory:");

        let first = session("a", 1_000, 4, Vec::new());
        let mut second = session("b", 2_000, 2, Vec::new());
        second.suppressed_heuristic_findings = 3;
        second.engine_mode = Some("jev".to_string());
        crate::executor::block_on(repository.save_scan_session(&first)).expect("save");
        crate::executor::block_on(repository.save_scan_session(&second)).expect("save");

        let history = crate::executor::block_on(repository.get_history(10, None)).expect("history");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].id, "b");
        assert_eq!(history[1].id, "a");
        assert_eq!(history[0].files_scanned, 2);
        assert_eq!(
            history[0].compliance_percent,
            second.summary().compliance_percent
        );
        assert_eq!(history[0].suppressed_heuristic_findings, 3);
        assert_eq!(history[0].engine_mode.as_deref(), Some("jev"));
        assert_eq!(history[1].suppressed_heuristic_findings, 0);
        assert_eq!(history[1].engine_mode, None);
    }

    #[test]
    fn the_same_session_id_is_replaced_not_duplicated() {
        let repository = SqliteScanHistoryRepository::open_in_memory().expect("repository");
        let first = session("a", 1_000, 4, Vec::new());
        let second = session("a", 2_000, 7, Vec::new());
        crate::executor::block_on(repository.save_scan_session(&first)).expect("save");
        crate::executor::block_on(repository.save_scan_session(&second)).expect("save");
        let history = crate::executor::block_on(repository.get_history(10, None)).expect("history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].files_scanned, 7);
    }

    #[test]
    fn the_findings_snapshot_round_trips() {
        let repository = SqliteScanHistoryRepository::open_in_memory().expect("repository");
        let finding = Finding::new("SEC-001", "src/app.ts", 3, 3)
            .with_title("Hardcoded secret")
            .with_description("A token is embedded in the source.");
        let session = session("a", 1_000, 1, vec![finding]);
        crate::executor::block_on(repository.save_scan_session(&session)).expect("save");

        let snapshots = repository.load_snapshots("a").expect("snapshots");
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].rule_id, "SEC-001");
        assert_eq!(snapshots[0].file_path, "src/app.ts");
        assert_eq!(snapshots[0].line_start, 3);
        assert_eq!(snapshots[0].title, "Hardcoded secret");
        assert_eq!(snapshots[0].severity, "CLEAN");
    }

    #[test]
    fn a_snapshot_of_an_unknown_session_is_empty() {
        let repository = SqliteScanHistoryRepository::open_in_memory().expect("repository");
        assert!(repository
            .load_snapshots("missing")
            .expect("snapshots")
            .is_empty());
    }

    #[test]
    fn the_default_path_lives_in_the_home_directory() {
        let path = SqliteScanHistoryRepository::default_path().expect("path");
        assert!(path.ends_with("history.sqlite3"));
        assert!(
            path.parent()
                .is_some_and(|parent| parent.ends_with(".coderadar")),
            "{path:?}"
        );
    }

    #[test]
    fn a_snapshot_can_be_built_from_a_finding() {
        let snapshot = FindingSnapshot::from_finding(
            &Finding::new("SEC-002", "a.ts", 1, 2)
                .with_cwe("CWE-798")
                .with_severity(coderadar_domain::Severity::High),
        );
        assert_eq!(snapshot.id, "SEC-002:a.ts:1");
        assert_eq!(snapshot.severity, "HIGH");
        assert_eq!(snapshot.category, "security");
        assert_eq!(snapshot.cwe.as_deref(), Some("CWE-798"));
    }
}
