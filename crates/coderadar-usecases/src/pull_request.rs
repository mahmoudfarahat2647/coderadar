//! Pull Request hygiene use case (`PR-*`, Phase 8).
//!
//! The use case reads the staged changeset through a [`ChangesetGateway`],
//! asks the domain for the hygiene assessment, and attaches the authoritative
//! rule metadata from `rules/manifest.json` to every violation. This is the
//! only place where a `PR-*` violation becomes a domain [`Finding`].

use std::sync::Arc;

use coderadar_domain::{DomainError, Finding, PullRequestHygiene, RiskScore, RuleSpec};

use crate::ports::{BoxFuture, ChangesetGateway};

/// The hygiene scorecard plus the findings produced by its violations.
#[derive(Debug, Clone, PartialEq)]
pub struct PullRequestAssessment {
    /// Metrics and triggers shown by the PR Hygiene HUD.
    pub hygiene: PullRequestHygiene,
    /// One finding per violation, with the severity of its rule.
    pub findings: Vec<Finding>,
}

/// Use case: assess the staged changeset of a repository.
pub struct AssessPullRequest {
    gateway: Arc<dyn ChangesetGateway>,
}

impl AssessPullRequest {
    /// Create the use case on top of a Git changeset gateway.
    pub fn new(gateway: Arc<dyn ChangesetGateway>) -> Self {
        Self { gateway }
    }

    /// Read the staged diff and evaluate the `PR-*` rules.
    ///
    /// Findings are sorted by file, line and rule so two identical changesets
    /// always produce the same report.
    ///
    /// # Errors
    /// Returns [`DomainError`] when the changeset cannot be read.
    pub fn assess<'a>(
        &'a self,
        root: &'a str,
        rules: &'a [RuleSpec],
    ) -> BoxFuture<'a, Result<PullRequestAssessment, DomainError>> {
        Box::pin(async move {
            let changeset = self.gateway.staged_changeset(root).await?;
            let hygiene = changeset.assess();

            let mut findings = Vec::with_capacity(hygiene.violations.len());
            for violation in &hygiene.violations {
                let Some(rule) = rules.iter().find(|rule| rule.id == violation.rule_id) else {
                    continue;
                };
                let mut finding = Finding::new(
                    rule.id.clone(),
                    violation.file_path.clone(),
                    violation.line,
                    violation.line,
                )
                .with_category(rule.category)
                .with_severity(rule.severity)
                .with_title(rule.title.clone())
                .with_description(violation.description.clone())
                .with_confidence(violation.confidence)
                .with_risk_score(RiskScore::from(rule.severity));
                if let Some(cwe) = rule.primary_cwe() {
                    finding = finding.with_cwe(cwe);
                }
                findings.push(finding);
            }

            findings.sort_by(|left, right| {
                (
                    &left.file_path,
                    left.line_start,
                    left.line_end,
                    &left.rule_id,
                )
                    .cmp(&(
                        &right.file_path,
                        right.line_start,
                        right.line_end,
                        &right.rule_id,
                    ))
            });

            Ok(PullRequestAssessment { hygiene, findings })
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use coderadar_domain::{
        Changeset, DomainError, EvaluationEngine, FileChange, FindingCategory, RuleScope, Severity,
    };

    use super::AssessPullRequest;
    use crate::ports::{BoxFuture, ChangesetGateway};

    struct FakeGateway {
        changeset: Changeset,
    }

    impl ChangesetGateway for FakeGateway {
        fn staged_changeset<'a>(
            &'a self,
            _root: &'a str,
        ) -> BoxFuture<'a, Result<Changeset, DomainError>> {
            let changeset = self.changeset.clone();
            Box::pin(async move { Ok(changeset) })
        }
    }

    fn pr_rule(id: &str, title: &str, severity: Severity) -> coderadar_domain::RuleSpec {
        coderadar_domain::RuleSpec {
            id: id.to_string(),
            title: title.to_string(),
            category: FindingCategory::PullRequests,
            severity,
            cwe: Vec::new(),
            scope: RuleScope::GitChangeset,
            engine: EvaluationEngine::Tier0Static,
            assertion: String::new(),
        }
    }

    fn rules() -> Vec<coderadar_domain::RuleSpec> {
        vec![
            pr_rule("PR-SCP-001", "Scope Creep / Bloated PR", Severity::Medium),
            pr_rule("PR-BRK-002", "Breaking API Contract Change", Severity::High),
            pr_rule(
                "PR-TST-003",
                "Missing Test Coverage for New Features",
                Severity::Medium,
            ),
        ]
    }

    fn changeset() -> Changeset {
        Changeset::new(
            "HEAD",
            vec![
                FileChange::new("src/api.ts", 600, 10)
                    .with_removed_public_api(vec!["export function gone() {}".to_string()]),
                FileChange::new("docs/readme.md", 4, 0),
            ],
        )
    }

    #[test]
    fn every_violation_becomes_a_finding_with_manifest_metadata() {
        let gateway = Arc::new(FakeGateway {
            changeset: changeset(),
        });
        let assessment = crate::test_support::block_on(
            AssessPullRequest::new(gateway).assess("/repo", &rules()),
        )
        .expect("assessment succeeds");

        assert!(assessment.hygiene.scope_creep);
        assert!(assessment.hygiene.breaking_change);
        assert!(assessment.hygiene.missing_tests);
        assert_eq!(assessment.findings.len(), 3);

        let breaking = assessment
            .findings
            .iter()
            .find(|finding| finding.rule_id == "PR-BRK-002")
            .expect("breaking finding");
        assert_eq!(breaking.category, FindingCategory::PullRequests);
        assert_eq!(breaking.severity, Severity::High);
        assert_eq!(breaking.title, "Breaking API Contract Change");
        assert_eq!(breaking.file_path, "src/api.ts");
        assert_eq!(breaking.line_start, 1);

        let missing_tests = assessment
            .findings
            .iter()
            .find(|finding| finding.rule_id == "PR-TST-003")
            .expect("missing tests finding");
        assert_eq!(missing_tests.severity, Severity::Medium);
    }

    #[test]
    fn an_empty_changeset_is_clean() {
        let gateway = Arc::new(FakeGateway {
            changeset: Changeset::default(),
        });
        let assessment = crate::test_support::block_on(
            AssessPullRequest::new(gateway).assess("/repo", &rules()),
        )
        .expect("assessment succeeds");

        assert!(assessment.hygiene.is_clean());
        assert!(assessment.findings.is_empty());
    }

    #[test]
    fn unknown_rule_ids_are_ignored_instead_of_panicking() {
        let gateway = Arc::new(FakeGateway {
            changeset: changeset(),
        });
        let only_scope_creep = [pr_rule(
            "PR-SCP-001",
            "Scope Creep / Bloated PR",
            Severity::Medium,
        )];
        let assessment = crate::test_support::block_on(
            AssessPullRequest::new(gateway).assess("/repo", &only_scope_creep),
        )
        .expect("assessment succeeds");

        assert_eq!(assessment.findings.len(), 1);
        assert_eq!(assessment.findings[0].rule_id, "PR-SCP-001");
    }
}
