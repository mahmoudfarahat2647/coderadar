//! The synthetic repository used by the Phase 7 performance gates.
//!
//! A flat tree of 50 directories with 100 TypeScript files each — 5,000
//! files in total — is enough to make the walk, the chunker and the rule
//! engine work long enough to be measured, while staying small enough to be
//! written in a couple of seconds on every platform.

#![allow(dead_code)]

use std::path::Path;

/// Files per subdirectory.
pub const FILES_PER_DIR: usize = 100;

/// Number of subdirectories.
pub const DIRS: usize = 50;

/// The total number of synthetic files.
pub const TOTAL_FILES: usize = FILES_PER_DIR * DIRS;

/// Write the synthetic repository into `root`.
pub fn build_repository(root: &Path) {
    for dir in 0..DIRS {
        let dir_path = root.join(format!("d{dir}"));
        std::fs::create_dir_all(&dir_path).expect("subdirectory is creatable");
        for file in 0..FILES_PER_DIR {
            let index = dir * FILES_PER_DIR + file;
            let source = format!(
                "// synthetic file {index} for the memory-budget gate\n\
                 // padding line two\n\
                 // padding line three\n\
                 export function f{index}(): number {{\n  \
                 const v = {index};\n  \
                 return v;\n\
                 }}\n\
                 // trailing comment one\n\
                 // trailing comment two\n\
                 // trailing comment three\n"
            );
            std::fs::write(dir_path.join(format!("f{index}.ts")), source)
                .expect("synthetic file is writable");
        }
    }
}
