//! Source fingerprints survive history persistence without inventing legacy data.

mod common;

use coderadar_domain::{ScanMode, ScanSession};
use coderadar_infrastructure::SqliteScanHistoryRepository;
use coderadar_usecases::ScanHistoryRepository;
use std::collections::BTreeMap;

#[test]
fn source_fingerprints_round_trip_and_unknown_remains_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.sqlite3");
    let repo = SqliteScanHistoryRepository::open(&path).unwrap();
    let legacy = ScanSession::new("unknown", "root", ScanMode::FullScan, 1);
    let mut known = ScanSession::new("known", "root", ScanMode::FullScan, 2);
    known.analyzed_sources = Some(BTreeMap::new());
    known.consumed_sources = Some(BTreeMap::from([("src/a.ts".into(), "abc".into())]));
    common::block_on(repo.save_scan_session(&legacy)).unwrap();
    common::block_on(repo.save_scan_session(&known)).unwrap();
    drop(repo);
    let repo = SqliteScanHistoryRepository::open(&path).unwrap();
    let rows = common::block_on(repo.get_history(10, None)).unwrap();
    let known_row = rows.iter().find(|row| row.id == "known").unwrap();
    assert_eq!(known_row.analyzed_sources, known.analyzed_sources);
    assert_eq!(known_row.consumed_sources, known.consumed_sources);
    let unknown = rows.iter().find(|row| row.id == "unknown").unwrap();
    assert_eq!(unknown.analyzed_sources, None);
    assert_eq!(unknown.consumed_sources, None);
}
