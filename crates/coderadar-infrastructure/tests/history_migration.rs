//! Migration tests through the history repository boundary.

mod common;

use coderadar_domain::{Finding, ScanCoverage, ScanMode, ScanSession, ScanUsage};
use coderadar_infrastructure::SqliteScanHistoryRepository;
use coderadar_usecases::ScanHistoryRepository;
use rusqlite::{params, Connection};

const LEGACY_SCHEMA: &str = "CREATE TABLE scan_sessions (
    id TEXT PRIMARY KEY, root_dir TEXT NOT NULL, mode TEXT NOT NULL,
    started_at_unix_ms INTEGER NOT NULL, finished_at_unix_ms INTEGER NOT NULL,
    files_scanned INTEGER NOT NULL, total_findings INTEGER NOT NULL,
    critical INTEGER NOT NULL, high INTEGER NOT NULL, medium INTEGER NOT NULL,
    low INTEGER NOT NULL, compliance_percent INTEGER NOT NULL,
    findings_json TEXT NOT NULL
); CREATE INDEX history_root ON scan_sessions(root_dir);";

const FINDINGS: &str = r#"[{"id":"f","file_path":"a.ts","line_start":1,"line_end":1,"rule_id":"R","category":"security","severity":"HIGH","cwe":null,"title":"t","description":"d"}]"#;

#[test]
fn migrates_legacy_rows_without_inventing_provenance_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(LEGACY_SCHEMA).unwrap();
    db.execute(
        "INSERT INTO scan_sessions VALUES ('a','root','full_scan',1,2,3,1,0,1,0,0,67,?1)",
        params![FINDINGS],
    )
    .unwrap();
    drop(db);
    for _ in 0..2 {
        let repo = SqliteScanHistoryRepository::open(&path).unwrap();
        let history = common::block_on(repo.get_history(10, None)).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(format!("{:?}", history[0].compliance_percent), "Some(67)");
        assert_eq!(history[0].analyzed_sources, None);
        assert_eq!(history[0].consumed_sources, None);
        assert_eq!(history[0].completion, None);
        let snapshot = serde_json::to_value(&repo.load_snapshots("a").unwrap()[0]).unwrap();
        assert_eq!(snapshot["engine"], "unknown");
        assert!(snapshot["confidence"].is_null());
    }
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT findings_json FROM scan_sessions", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        FINDINGS
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='history_root'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn version_one_history_keeps_unknown_usage_and_round_trips_new_usage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(&LEGACY_SCHEMA.replace(
        "compliance_percent INTEGER NOT NULL",
        "compliance_percent INTEGER",
    ))
    .unwrap();
    db.execute_batch("ALTER TABLE scan_sessions ADD COLUMN suppressed_heuristic_findings INTEGER NOT NULL DEFAULT 0; ALTER TABLE scan_sessions ADD COLUMN engine_mode TEXT; ALTER TABLE scan_sessions ADD COLUMN coverage_json TEXT; INSERT INTO scan_sessions VALUES ('old','root','full_scan',1,2,0,0,0,0,0,0,NULL,'[]',0,NULL,NULL); PRAGMA user_version=1;").unwrap();
    drop(db);
    let repo = SqliteScanHistoryRepository::open(&path).unwrap();
    let mut session = ScanSession::new("new", "root", ScanMode::FullScan, 3);
    let usage = ScanUsage {
        primary_batches: 2,
        fallback_batches: 1,
        fallback_failures: 3,
    };
    session.usage = Some(usage);
    assert_eq!(session.summary().usage, Some(usage));
    common::block_on(repo.save_scan_session(&session)).unwrap();
    drop(repo);
    let repo = SqliteScanHistoryRepository::open(&path).unwrap();
    let rows = common::block_on(repo.get_history(10, None)).unwrap();
    assert_eq!(rows.iter().find(|r| r.id == "old").unwrap().usage, None);
    assert_eq!(
        rows.iter().find(|r| r.id == "new").unwrap().usage,
        Some(usage)
    );
}

#[test]
fn failed_migration_rolls_back_schema_and_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(&LEGACY_SCHEMA.replace(
        "files_scanned INTEGER NOT NULL",
        "files_scanned INTEGER NOT NULL CHECK(files_scanned > 0)",
    ))
    .unwrap();
    db.execute_batch("PRAGMA ignore_check_constraints=ON; INSERT INTO scan_sessions VALUES ('bad','root','full_scan',1,2,0,0,0,0,0,0,100,'[]');").unwrap();
    drop(db);
    assert!(SqliteScanHistoryRepository::open(&path).is_err());
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(db.query_row("SELECT count(*) FROM pragma_table_info('scan_sessions') WHERE name='compliance_percent' AND [notnull]=1", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM scan_sessions WHERE id='bad'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='scan_sessions_migration'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn new_null_score_and_coverage_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let repo = SqliteScanHistoryRepository::open(&path).unwrap();
    let mut session = ScanSession::new("new", "root", ScanMode::FullScan, 1);
    let coverage = ScanCoverage {
        discovered: 3,
        supported: 0,
        analyzed: 0,
        unsupported: 2,
        excluded: 1,
        exclusions_complete: Some(false),
        unreadable: 0,
        oversized: 0,
        invalid_encoding: 0,
        incomplete: 0,
    };
    session.coverage = Some(coverage);
    common::block_on(repo.save_scan_session(&session)).unwrap();
    drop(repo);
    let db = Connection::open(&path).unwrap();
    assert!(db
        .query_row(
            "SELECT compliance_percent FROM scan_sessions WHERE id='new'",
            [],
            |r| r.get::<_, Option<u8>>(0)
        )
        .unwrap()
        .is_none());
    drop(db);
    let repo = SqliteScanHistoryRepository::open(&path).unwrap();
    let history = common::block_on(repo.get_history(10, None)).unwrap();
    assert_eq!(history[0].compliance_percent, None);
    assert_eq!(history[0].coverage, Some(coverage));
    drop(repo);
    let db = Connection::open(&path).unwrap();
    db.execute("UPDATE scan_sessions SET coverage_json = ?1 WHERE id = 'new'", [r#"{"discovered":3,"supported":0,"analyzed":0,"unsupported":2,"excluded":1,"unreadable":0,"oversized":0,"invalid_encoding":0,"incomplete":0}"#]).unwrap();
    drop(db);
    let repo = SqliteScanHistoryRepository::open(&path).unwrap();
    let history = common::block_on(repo.get_history(10, None)).unwrap();
    assert_eq!(history[0].coverage.unwrap().exclusions_complete, None);
}

#[test]
fn migrates_recent_legacy_columns_and_preserves_explicit_finding_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(LEGACY_SCHEMA).unwrap();
    db.execute_batch("ALTER TABLE scan_sessions ADD COLUMN suppressed_heuristic_findings INTEGER NOT NULL DEFAULT 0; ALTER TABLE scan_sessions ADD COLUMN engine_mode TEXT;").unwrap();
    let mut snapshot = serde_json::to_value(
        coderadar_infrastructure::FindingSnapshot::from_finding(&Finding::new("R", "a.ts", 1, 1)),
    )
    .unwrap();
    snapshot["engine"] = "mock".into();
    snapshot["confidence"] = 0.75.into();
    let json = serde_json::to_string(&vec![snapshot]).unwrap();
    db.execute(
        "INSERT INTO scan_sessions VALUES ('a','root','full_scan',1,2,3,1,0,1,0,0,67,?1,4,'jev')",
        params![json],
    )
    .unwrap();
    drop(db);
    let repo = SqliteScanHistoryRepository::open(&path).unwrap();
    let history = common::block_on(repo.get_history(10, None)).unwrap();
    assert_eq!(history[0].suppressed_heuristic_findings, 4);
    assert_eq!(history[0].engine_mode.as_deref(), Some("jev"));
    assert_eq!(history[0].coverage, None);
    let snapshots = repo.load_snapshots("a").unwrap();
    assert_eq!(snapshots[0].engine, "mock");
    assert_eq!(snapshots[0].confidence, Some(0.75));
}

#[test]
fn rejects_a_newer_schema_without_modifying_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA user_version=42; CREATE TABLE future (value TEXT);")
        .unwrap();
    drop(db);
    assert!(SqliteScanHistoryRepository::open(&path).is_err());
    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        42
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='future'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn migration_preserves_rows_referencing_legacy_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(LEGACY_SCHEMA).unwrap();
    db.execute_batch("CREATE TABLE annotations (session_id TEXT REFERENCES scan_sessions(id) ON DELETE CASCADE, note TEXT NOT NULL);
        INSERT INTO scan_sessions VALUES ('a','root','full_scan',1,2,3,1,0,1,0,0,67,'[]');
        INSERT INTO annotations VALUES ('a','keep me');").unwrap();
    drop(db);

    let repo = SqliteScanHistoryRepository::open(&path).unwrap();
    let mut session = ScanSession::new("new", "root", ScanMode::FullScan, 3);
    session.coverage = Some(ScanCoverage::default());
    common::block_on(repo.save_scan_session(&session)).unwrap();
    drop(repo);

    let db = Connection::open(&path).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT note FROM annotations WHERE session_id='a'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "keep me"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM scan_sessions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT compliance_percent FROM scan_sessions WHERE id='new'",
            [],
            |r| r.get::<_, Option<u8>>(0)
        )
        .unwrap(),
        None
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
}
