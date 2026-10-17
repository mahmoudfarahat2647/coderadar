//! `ARCH-DEP-001` — The Dependency Rule Violation.
//!
//! File/module scope, executed against [`crate::layer_map::LayerMap`]:
//!
//! 1. a file in `domain`/`usecases` importing a relative module that resolves
//!    to an *outer* layer is a violation (dependencies must point inward);
//! 2. a file in `domain`/`usecases` importing a known framework/ORM/UI
//!    package is a violation regardless of relative structure.
//!
//! Only inner-layer files are checked: an outer layer importing an inner one
//! is legal by definition. Imports are extracted from whole-file
//! ([`ChunkKind::FileModule`]) chunks with line-anchored regexes covering
//! static imports/exports, side-effect imports, CommonJS `require(...)` calls
//! and dynamic `import(...)` calls. Type-only imports (`import type ... from`)
//! are treated as regular imports: even when erased at runtime, they still
//! make an inner layer's source code depend on the outer layer's types.

use std::sync::LazyLock;

use coderadar_domain::{ChunkKind, CodeChunk, Finding, RuleSpec};
use regex::Regex;

use crate::heuristics::build_finding;
use crate::layer_map::{LayerMap, ProjectLayer};

/// Bare package prefixes that inner layers may never import.
pub const FORBIDDEN_FRAMEWORK_PREFIXES: [&str; 24] = [
    "express",
    "fastify",
    "koa",
    "hapi",
    "@nestjs/",
    "typeorm",
    "sequelize",
    "prisma",
    "@prisma/",
    "mongoose",
    "knex",
    "objection",
    "react",
    "react-dom",
    "next",
    "@angular/",
    "vue",
    "axios",
    "node-fetch",
    "ioredis",
    "pg",
    "mysql",
    "mysql2",
    "better-sqlite3",
];

/// Confidence of a relative import that resolves to an outer layer.
const RELATIVE_IMPORT_CONFIDENCE: f32 = 0.9;
/// Confidence of a bare import of a known framework/ORM/UI package.
const FORBIDDEN_PACKAGE_CONFIDENCE: f32 = 0.95;

/// `import ... from 'x'` / `export ... from 'x'`, including multiline clauses.
///
/// Anchored at the start of a line so `import`/`export` inside string
/// literals or comments is never mistaken for a statement; the clause may not
/// contain quotes or `;`, which keeps the match inside a single statement.
static IMPORT_FROM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^[ \t]*(?:import|export)\b[^'";]*?\bfrom\s*['"]([^'"]+)['"]"#)
        .expect("static import-from pattern compiles")
});

/// Side-effect imports: `import 'x'`.
static IMPORT_BARE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^[ \t]*import\s*['"]([^'"]+)['"]"#)
        .expect("static bare-import pattern compiles")
});

/// CommonJS requires and dynamic imports: `require('x')`, `import('x')`.
///
/// The prefix character class rejects quotes so a `require` mentioned inside
/// a string literal does not register as an import.
static IMPORT_CALL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|[^\w'"])(?:require|import)\s*\(\s*['"]([^'"]+)['"]\s*\)"#)
        .expect("static import-call pattern compiles")
});

/// One extracted import: the specifier and the line that carries it.
#[derive(Debug)]
struct ImportSite {
    specifier: String,
    line: u32,
}

/// Evaluate `ARCH-DEP-001` against whole-file ([`ChunkKind::FileModule`])
/// chunks. AST windows are ignored.
pub(crate) fn evaluate(
    rule: &RuleSpec,
    chunks: &[CodeChunk],
    layer_map: &LayerMap,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for chunk in chunks
        .iter()
        .filter(|chunk| chunk.kind == ChunkKind::FileModule)
    {
        let Some(file_layer) = layer_map.layer_of(&chunk.file_path) else {
            continue;
        };
        if !matches!(file_layer, ProjectLayer::Domain | ProjectLayer::UseCases) {
            continue;
        }
        for site in extract_imports(chunk) {
            if let Some(finding) = check_import(rule, chunk, file_layer, &site, layer_map) {
                findings.push(finding);
            }
        }
    }
    findings
}

/// Check one import site against the Dependency Rule.
fn check_import(
    rule: &RuleSpec,
    chunk: &CodeChunk,
    file_layer: ProjectLayer,
    site: &ImportSite,
    layer_map: &LayerMap,
) -> Option<Finding> {
    if site.specifier.starts_with('.') {
        let target = layer_map.layer_of_import(&chunk.file_path, &site.specifier)?;
        if target.depth() <= file_layer.depth() {
            return None;
        }
        return Some(build_finding(
            rule,
            &chunk.file_path,
            site.line,
            site.line,
            format!(
                "relative import \"{}\" resolves to the \"{}\" layer from \"{}\" — dependencies must point inward",
                site.specifier,
                target.as_str(),
                file_layer.as_str()
            ),
            RELATIVE_IMPORT_CONFIDENCE,
        ));
    }
    let package = forbidden_package(&site.specifier)?;
    Some(build_finding(
        rule,
        &chunk.file_path,
        site.line,
        site.line,
        format!(
            "imports framework/ORM/UI package \"{package}\" from the \"{}\" layer",
            file_layer.as_str()
        ),
        FORBIDDEN_PACKAGE_CONFIDENCE,
    ))
}

/// The forbidden package a bare specifier belongs to, if any.
///
/// Scoped prefixes (`@nestjs/`) match any package under the scope; plain
/// prefixes match the package itself or its subpath only, so `express` never
/// swallows `express-session`.
fn forbidden_package(specifier: &str) -> Option<&'static str> {
    FORBIDDEN_FRAMEWORK_PREFIXES
        .iter()
        .find(|prefix| is_forbidden(specifier, prefix))
        .map(|prefix| prefix.strip_suffix('/').unwrap_or(prefix))
}

/// `true` when `specifier` matches the forbidden `prefix`.
fn is_forbidden(specifier: &str, prefix: &str) -> bool {
    if prefix.ends_with('/') {
        specifier.starts_with(prefix)
    } else {
        specifier == prefix
            || specifier
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/'))
    }
}

/// Extract every import specifier from a chunk, in source order.
fn extract_imports(chunk: &CodeChunk) -> Vec<ImportSite> {
    let mut sites = Vec::new();
    for regex in [&IMPORT_FROM, &IMPORT_BARE, &IMPORT_CALL] {
        for captures in regex.captures_iter(&chunk.content) {
            let Some(specifier) = captures.get(1) else {
                continue;
            };
            sites.push(ImportSite {
                specifier: specifier.as_str().to_string(),
                line: line_of_offset(chunk, specifier.start()),
            });
        }
    }
    sites.sort_by_key(|site| site.line);
    sites
}

/// Absolute line number of a byte offset inside the chunk content.
fn line_of_offset(chunk: &CodeChunk, offset: usize) -> u32 {
    let newlines = chunk.content[..offset].matches('\n').count() as u32;
    chunk.start_line + newlines
}

#[cfg(test)]
mod tests {
    use coderadar_domain::{
        ChunkKind, CodeChunk, EvaluationEngine, Finding, FindingCategory, RuleScope, RuleSpec,
        Severity,
    };

    use super::{evaluate, FORBIDDEN_FRAMEWORK_PREFIXES};
    use crate::layer_map::LayerMap;

    fn rule() -> RuleSpec {
        RuleSpec {
            id: "ARCH-DEP-001".to_string(),
            title: "The Dependency Rule Violation".to_string(),
            category: FindingCategory::CleanArchitecture,
            severity: Severity::High,
            cwe: Vec::new(),
            scope: RuleScope::FileModule,
            engine: EvaluationEngine::Tier0Static,
            assertion: "Inner Domain/UseCase module directly imports outer UI/Controller, ORM, or Framework."
                .to_string(),
        }
    }

    fn file(path: &str, content: &str) -> CodeChunk {
        CodeChunk::file_module(path, "typescript", content.lines().count() as u32, content)
    }

    fn evaluate_file(path: &str, content: &str) -> Vec<Finding> {
        evaluate(&rule(), &[file(path, content)], &LayerMap::fallback())
    }

    #[test]
    fn domain_importing_a_framework_package_is_flagged() {
        let findings = evaluate_file("src/domain/order.ts", "import express from \"express\";\n");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line_start, 1);
        assert_eq!(findings[0].line_end, 1);
        assert!((findings[0].confidence - 0.95).abs() < f32::EPSILON);
        assert!(
            findings[0]
                .description
                .contains("framework/ORM/UI package \"express\""),
            "{}",
            findings[0].description
        );
        assert!(findings[0].description.contains("\"domain\" layer"));
    }

    #[test]
    fn domain_importing_an_outer_relative_module_is_flagged() {
        let findings = evaluate_file(
            "src/domain/order.ts",
            "import { db } from \"../infrastructure/db\";\n",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line_start, 1);
        assert!((findings[0].confidence - 0.9).abs() < f32::EPSILON);
        assert!(
            findings[0].description.contains(
                "relative import \"../infrastructure/db\" resolves to the \"infrastructure\" layer from \"domain\""
            ),
            "{}",
            findings[0].description
        );
    }

    #[test]
    fn usecases_importing_an_orm_is_flagged() {
        let findings = evaluate_file(
            "src/application/orders/create_order.ts",
            "import { DataSource } from \"typeorm\";\n",
        );
        assert_eq!(findings.len(), 1);
        assert!(findings[0].description.contains("\"usecases\" layer"));
    }

    #[test]
    fn inward_imports_within_inner_layers_are_legal() {
        let findings = evaluate(
            &rule(),
            &[
                file(
                    "src/domain/order.ts",
                    "import { Money } from \"./money\";\n",
                ),
                file(
                    "src/usecases/create_order.ts",
                    "import { Order } from \"../domain/order\";\n",
                ),
            ],
            &LayerMap::fallback(),
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn outer_to_inner_imports_are_legal() {
        let findings = evaluate_file(
            "src/infrastructure/db/client.ts",
            "import { Order } from \"../../domain/order\";\nimport express from \"express\";\n",
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn adapter_files_are_out_of_scope() {
        let findings = evaluate_file(
            "src/adapters/http/router.ts",
            "import { db } from \"../infrastructure/db\";\n",
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn files_without_a_layer_are_ignored() {
        let findings = evaluate_file("scripts/tool.ts", "import express from \"express\";\n");
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn ast_window_chunks_are_ignored() {
        let chunk = CodeChunk::new(
            "src/domain/order.ts",
            "typescript",
            1,
            1,
            "import express from \"express\";\n",
        );
        assert_eq!(chunk.kind, ChunkKind::AstWindow);
        let findings = evaluate(&rule(), &[chunk], &LayerMap::fallback());
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn hyphenated_and_scoped_packages_match_correctly() {
        let findings = evaluate_file(
            "src/domain/order.ts",
            "import { Module } from \"@nestjs/common\";\nimport fetch from \"node-fetch\";\nimport session from \"express-session\";\n",
        );
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(findings[0].description.contains("\"@nestjs\""));
        assert!(findings[1].description.contains("\"node-fetch\""));
    }

    #[test]
    fn subpath_imports_of_forbidden_packages_are_flagged() {
        let findings = evaluate_file(
            "src/domain/order.ts",
            "import { PrismaClient } from \"@prisma/client\";\nimport { Repository } from \"typeorm/repository\";\n",
        );
        assert_eq!(findings.len(), 2, "{findings:?}");
    }

    #[test]
    fn multiline_imports_report_the_specifier_line() {
        let content = "import { Order } from \"./order\";\n\nimport {\n    Database,\n    Connection,\n} from \"../infrastructure/db\";\n\nexport const x = 1;\n";
        let findings = evaluate_file("src/domain/order.ts", content);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(
            findings[0].line_start, 6,
            "specifier sits on the `from` line"
        );
    }

    #[test]
    fn import_type_counts_as_a_dependency() {
        let findings = evaluate_file(
            "src/domain/order.ts",
            "import type { PrismaClient } from \"@prisma/client\";\n",
        );
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].line_start, 1);
    }

    #[test]
    fn require_and_dynamic_imports_are_extracted() {
        let content =
            "const db = require(\"../infrastructure/db\");\nconst ui = await import(\"react\");\n";
        let findings = evaluate_file("src/domain/order.ts", content);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert_eq!(findings[0].line_start, 1);
        assert_eq!(findings[1].line_start, 2);
    }

    #[test]
    fn multiple_violations_produce_multiple_findings() {
        let content =
            "import express from \"express\";\nimport { db } from \"../database/client\";\n";
        let findings = evaluate_file("src/domain/order.ts", content);
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert_eq!(findings[0].line_start, 1);
        assert_eq!(findings[1].line_start, 2);
    }

    #[test]
    fn forbidden_package_list_is_never_emptied() {
        assert!(FORBIDDEN_FRAMEWORK_PREFIXES.len() >= 24);
        assert!(FORBIDDEN_FRAMEWORK_PREFIXES.contains(&"express"));
    }
}
