use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

use crate::client::ZyxelClient;
use crate::config::SignalConfig;
use crate::telemetry::zyxel::{CellWanStatusObject, sanitize_access_technology};

use super::auth::ensure_authenticated;
use super::{CheckResult, QualityMonitor};

pub(super) struct SignalMonitor {
    client: Arc<ZyxelClient>,
    config: SignalConfig,
    interval: Duration,
    max_retries: u32,
}

impl SignalMonitor {
    pub(super) fn new(
        client: Arc<ZyxelClient>,
        config: SignalConfig,
        interval: Duration,
        max_retries: u32,
    ) -> Self {
        Self {
            client,
            config,
            interval,
            max_retries,
        }
    }

    async fn fetch_cellwan_status(&self) -> Result<CellWanStatusObject> {
        match self.client.get_cellwan_status().await {
            Ok(status) => Ok(status),
            Err(initial_error) => {
                debug!(error = %initial_error, "Refreshing router session before retrying signal check");
                ensure_authenticated(&self.client).await?;
                self.client.get_cellwan_status().await.with_context(|| {
                    format!("Failed to fetch cellwan_status after session refresh: {initial_error}")
                })
            }
        }
    }
}

impl QualityMonitor for SignalMonitor {
    type Success = ();
    type Issue = SignalQualityIssue;

    fn interval(&self) -> Duration {
        self.interval
    }

    fn max_retries(&self) -> u32 {
        self.max_retries
    }

    fn enabled(&self) -> bool {
        self.config.enabled
    }

    async fn check(&self) -> Result<CheckResult<Self::Success, Self::Issue>> {
        if !self.config.enabled {
            return Ok(CheckResult::Healthy(()));
        }

        let status = self.fetch_cellwan_status().await?;

        Ok(match signal_quality_issue(&status, &self.config) {
            Some(issue) => CheckResult::Degraded(issue),
            None => CheckResult::Healthy(()),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum SignalQualityIssue {
    Missing5g { access_technology: Option<String> },
    Weak5gRsrp { rsrp: f64, threshold: f64 },
}

impl SignalQualityIssue {
    pub(super) fn metric_reason(&self) -> &'static str {
        match self {
            Self::Missing5g { .. } => "missing_5g",
            Self::Weak5gRsrp { .. } => "weak_5g_rsrp",
        }
    }

    pub(super) fn summary(&self) -> String {
        match self {
            Self::Missing5g { access_technology } => match access_technology.as_deref() {
                Some(access_technology) => {
                    format!("5G unavailable, current access technology is {access_technology}")
                }
                None => "5G unavailable, current access technology is unknown".to_string(),
            },
            Self::Weak5gRsrp { rsrp, threshold } => {
                format!("5G RSRP {rsrp} dBm is below threshold {threshold} dBm")
            }
        }
    }
}

fn signal_quality_issue(
    status: &CellWanStatusObject,
    config: &SignalConfig,
) -> Option<SignalQualityIssue> {
    if !config.enabled {
        return None;
    }

    let access_technology = status
        .intf_current_access_technology
        .as_deref()
        .and_then(sanitize_access_technology);

    if config.require_5g && !uses_5g(access_technology.as_deref()) {
        return Some(SignalQualityIssue::Missing5g { access_technology });
    }

    let rsrp = current_5g_rsrp(status, access_technology.as_deref());
    if let Some(rsrp) = rsrp
        && rsrp < config.min_5g_rsrp
    {
        return Some(SignalQualityIssue::Weak5gRsrp {
            rsrp,
            threshold: config.min_5g_rsrp,
        });
    }

    None
}

fn uses_5g(access_technology: Option<&str>) -> bool {
    matches!(
        access_technology,
        Some("nr5g") | Some("nr5g_nsa") | Some("nr5g_sa")
    )
}

fn current_5g_rsrp(status: &CellWanStatusObject, access_technology: Option<&str>) -> Option<f64> {
    if !uses_5g(access_technology) {
        return None;
    }

    status.nsa_rsrp.or(status.intf_rsrp)
}

#[cfg(test)]
mod tests {
    use super::{SignalQualityIssue, signal_quality_issue};
    use crate::{config::SignalConfig, telemetry::zyxel::CellWanStatusObject};

    fn signal_config() -> SignalConfig {
        SignalConfig {
            enabled: true,
            interval: None,
            require_5g: true,
            min_5g_rsrp: -110.0,
            max_retries: Some(1),
        }
    }

    #[test]
    fn signal_quality_detects_missing_5g_when_required() {
        let issue = signal_quality_issue(
            &CellWanStatusObject {
                intf_current_access_technology: Some("LTE".to_string()),
                ..Default::default()
            },
            &signal_config(),
        );

        assert_eq!(
            issue,
            Some(SignalQualityIssue::Missing5g {
                access_technology: Some("lte".to_string())
            })
        );
    }

    #[test]
    fn signal_quality_detects_weak_5g_rsrp() {
        let issue = signal_quality_issue(
            &CellWanStatusObject {
                intf_current_access_technology: Some("NR5G-NSA".to_string()),
                nsa_rsrp: Some(-115.0),
                ..Default::default()
            },
            &signal_config(),
        );

        assert_eq!(
            issue,
            Some(SignalQualityIssue::Weak5gRsrp {
                rsrp: -115.0,
                threshold: -110.0,
            })
        );
    }

    #[test]
    fn signal_quality_accepts_healthy_5g_rsrp() {
        let issue = signal_quality_issue(
            &CellWanStatusObject {
                intf_current_access_technology: Some("NR5G-NSA".to_string()),
                nsa_rsrp: Some(-95.0),
                ..Default::default()
            },
            &signal_config(),
        );

        assert_eq!(issue, None);
    }
}
