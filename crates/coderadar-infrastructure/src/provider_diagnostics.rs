//! Bounded, source-free Jev provider checks (`CR-SPEC-17 §6`).
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::jev::{JevQuestion, JevResponse};

const RESPONSE_LIMIT: u64 = 64 * 1024;

/// A provider capability, not a promise of future availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderState {
    /// No credential was supplied.
    NotConfigured,
    /// Configured, but inference has not been verified.
    ConfiguredUntested,
    /// The synthetic inference returned a valid decision.
    Available,
    /// The credential was rejected.
    Unauthorized,
    /// The credential lacks access.
    Forbidden,
    /// The provider reported a payment or quota block.
    QuotaBlocked,
    /// The provider is rate limiting requests.
    RateLimited,
    /// The service could not be reached or failed server-side.
    Unreachable,
    /// The response did not match the expected protocol.
    ProtocolIncompatible,
}

/// Safe-to-display result, without provider bodies, URLs, or credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderDiagnostic {
    /// Observed capability state.
    pub state: ProviderState,
    /// Time of this check as seconds since the Unix epoch, when available.
    pub checked_at_unix_secs: Option<u64>,
}

/// Explicit, separately invoked Jev metadata and synthetic inference checks.
///
/// Callers must supply the configured provider endpoints and credential. No
/// source file, workspace path, or arbitrary prompt is accepted by this API.
pub struct JevDiagnostics {
    metadata_endpoint: String,
    decisions_endpoint: String,
    model: String,
    key: Option<String>,
}

impl JevDiagnostics {
    /// Configure the two endpoints and model; an empty key is unconfigured.
    pub fn new(
        metadata_endpoint: &str,
        decisions_endpoint: &str,
        model: &str,
        key: Option<&str>,
    ) -> Self {
        Self {
            metadata_endpoint: metadata_endpoint.to_owned(),
            decisions_endpoint: decisions_endpoint.to_owned(),
            model: model.to_owned(),
            key: key.filter(|key| !key.trim().is_empty()).map(str::to_owned),
        }
    }

    /// Report configuration without performing a network request.
    pub fn initial_state(&self) -> ProviderDiagnostic {
        ProviderDiagnostic {
            state: if self.key.is_some() {
                ProviderState::ConfiguredUntested
            } else {
                ProviderState::NotConfigured
            },
            checked_at_unix_secs: None,
        }
    }

    /// Check authentication/metadata without sending workspace content.
    /// Success does not establish model inference availability.
    pub fn check_metadata(&self) -> ProviderDiagnostic {
        self.check(false)
    }

    /// Run one fixed synthetic inference; no workspace content is sent.
    pub fn check_synthetic(&self) -> ProviderDiagnostic {
        self.check(true)
    }

    fn check(&self, synthetic: bool) -> ProviderDiagnostic {
        let state = self.run(synthetic);
        ProviderDiagnostic {
            state,
            checked_at_unix_secs: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|v| v.as_secs()),
        }
    }

    fn run(&self, synthetic: bool) -> ProviderState {
        let Some(key) = self.key.as_deref() else {
            return ProviderState::NotConfigured;
        };
        let endpoint = if synthetic {
            &self.decisions_endpoint
        } else {
            &self.metadata_endpoint
        };
        let Ok(url) = reqwest::Url::parse(endpoint) else {
            return ProviderState::ProtocolIncompatible;
        };
        let loopback = url
            .host_str()
            .and_then(|host| host.parse::<std::net::IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback());
        if !url.username().is_empty()
            || url.password().is_some()
            || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        {
            return ProviderState::ProtocolIncompatible;
        }
        let Ok(client) = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
        else {
            return ProviderState::Unreachable;
        };
        let request = if synthetic {
            client
                .post(&self.decisions_endpoint)
                .json(&serde_json::json!({
                    "model": self.model,
                    "state": "Synthetic health check: the sky is blue.",
                    "questions": {"probe": {"type": "noul", "instructions": "Is the sky blue?"}}
                }))
        } else {
            client.get(&self.metadata_endpoint)
        };
        let Ok(mut response) = request.bearer_auth(key).send() else {
            return ProviderState::Unreachable;
        };
        let status = response.status();
        match status.as_u16() {
            401 => return ProviderState::Unauthorized,
            403 => return ProviderState::Forbidden,
            402 => return ProviderState::QuotaBlocked,
            429 => return ProviderState::RateLimited,
            500..=599 => return ProviderState::Unreachable,
            _ if !status.is_success() => return ProviderState::ProtocolIncompatible,
            _ => {}
        }
        let mut body = Vec::new();
        if response
            .by_ref()
            .take(RESPONSE_LIMIT + 1)
            .read_to_end(&mut body)
            .is_err()
            || body.len() as u64 > RESPONSE_LIMIT
        {
            return ProviderState::ProtocolIncompatible;
        }
        if synthetic {
            let questions = [(
                "probe".to_owned(),
                JevQuestion::Noul {
                    instructions: "Is the sky blue?".to_owned(),
                },
            )]
            .into_iter()
            .collect();
            match serde_json::from_slice::<JevResponse>(&body) {
                Ok(v) if v.matches_questions(&questions) => ProviderState::Available,
                _ => ProviderState::ProtocolIncompatible,
            }
        } else if serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value.get("data").and_then(|data| data.as_object()).cloned())
            .is_some()
        {
            ProviderState::ConfiguredUntested
        } else {
            ProviderState::ProtocolIncompatible
        }
    }
}
