#[allow(dead_code)]
#[path = "../src/commands.rs"]
mod commands;

#[cfg(windows)]
// The test executable needs Common Controls v6 before Windows loads its imports.
core::arch::global_asm!(include_str!("fix_safety_manifest.s"));
