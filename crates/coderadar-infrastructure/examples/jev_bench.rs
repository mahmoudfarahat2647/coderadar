//! Live Jev Tier 1 latency benchmark (`CR-SPEC-12 §7`).
//!
//! Runs a real scan of a target directory through the live OpenRouter
//! decisions endpoint and reports per-request latency against the Phase 0
//! baseline (`CR-SPEC-08 §3.4`).
//!
//! Usage:
//! ```text
//! JEV_API_KEY=<key> cargo run -p coderadar-infrastructure --example jev_bench [-- <target-dir>]
//! ```

use std::path::PathBuf;
use std::time::Duration;

use coderadar_infrastructure::{
    scan_project_streaming_with, JevConfig, SemanticEngine, JEV_API_KEY_ENV,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("jev_bench failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var(JEV_API_KEY_ENV)
        .map_err(|_| format!("set the {JEV_API_KEY_ENV} environment variable first"))?;
    let target = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../src-tauri/src"));

    let engine = SemanticEngine::live(JevConfig {
        api_key,
        ..JevConfig::default()
    })?;
    println!("target: {}", target.display());

    let scan_root = if target.is_absolute() {
        target
    } else {
        std::path::absolute(&target)?
    };
    let started = std::time::Instant::now();
    // The engine mode is read lazily after the scan so a benchmark that fell
    // back to Mock mid-run records `mock_fallback`, not `jev` (`CR-SPEC-12 §4`).
    let engine_status = engine.status();
    let report = scan_project_streaming_with(
        &scan_root,
        engine.evaluator(),
        Duration::ZERO,
        |_| {},
        move || engine_status.mode().as_str().to_string(),
    )?;
    let wall_ms = started.elapsed().as_millis();
    let snapshot = engine.status().snapshot();

    println!("--- scan ---");
    println!("files        : {}", report.scan.files_scanned);
    println!("ast chunks   : {}", report.scan.ast_chunks);
    println!("findings     : {}", report.findings.len());
    println!("wall clock   : {wall_ms} ms");
    println!("--- jev tier 1 ---");
    println!("requests     : {}", snapshot.tier1_requests);
    println!("fallbacks    : {}", snapshot.fallback_events);
    if let Some(reason) = snapshot.reason.as_ref() {
        println!("fallback via : {reason}");
    }
    println!("last latency : {} ms", snapshot.last_latency_ms);
    println!("max latency  : {} ms", snapshot.max_latency_ms);
    let mean = snapshot
        .total_latency_ms
        .checked_div(snapshot.tier1_requests)
        .unwrap_or(0);
    println!("mean latency : {mean} ms");

    Ok(())
}
