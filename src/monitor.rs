use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::client::{ZyxelClient, dal::DalOid};
use crate::config::{ActionConfig, MonitorConfig, RecoveryMethod, SignalConfig};
use crate::telemetry::{
    metrics::TelemetryCollector,
    zyxel::{CellWanStatusObject, sanitize_access_technology},
};

const ACCESS_TECHNOLOGY_AUTO: &str = "Auto";
const ACCESS_TECHNOLOGY_NR5G_SA: &str = "NR5G-SA";

enum RecoveryOutcome {
    ConnectivityRestored,
    Rebooted { rebooted_at: Instant },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecoveryTrigger {
    Connectivity,
    SignalQuality,
}

#[derive(Clone, Debug, PartialEq)]
enum SignalQualityIssue {
    Missing5g { access_technology: Option<String> },
    Weak5gRsrp { rsrp: f64, threshold: f64 },
}

impl SignalQualityIssue {
    fn metric_reason(&self) -> &'static str {
        match self {
            Self::Missing5g { .. } => "missing_5g",
            Self::Weak5gRsrp { .. } => "weak_5g_rsrp",
        }
    }

    fn summary(&self) -> String {
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

pub struct Monitor {
    client: Arc<ZyxelClient>,
    config: MonitorConfig,
    action: ActionConfig,
    check_client: reqwest::Client,
    telemetry: Option<TelemetryCollector>,
}

impl Monitor {
    pub fn new(
        client: Arc<ZyxelClient>,
        config: MonitorConfig,
        action: ActionConfig,
    ) -> Result<Self> {
        Self::build(client, config, action, None)
    }

    pub fn with_telemetry(mut self, telemetry: TelemetryCollector) -> Self {
        self.telemetry = Some(telemetry);
        self
    }

    fn build(
        client: Arc<ZyxelClient>,
        config: MonitorConfig,
        action: ActionConfig,
        telemetry: Option<TelemetryCollector>,
    ) -> Result<Self> {
        let check_client = reqwest::ClientBuilder::new()
            .danger_accept_invalid_certs(true)
            .timeout(config.internet.timeout)
            .build()?;

        Ok(Self {
            client,
            config,
            action,
            check_client,
            telemetry,
        })
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Monitor started");
        let mut failure_count: u32 = 0;
        let mut signal_failure_count: u32 = 0;
        let mut last_reboot: Option<Instant> = None;
        let mut internet_interval = tokio::time::interval(self.config.internet_interval());
        let mut signal_interval = tokio::time::interval(self.config.signal_interval());

        loop {
            tokio::select! {
                _ = internet_interval.tick() => {
                    match self.check_connectivity().await {
                        Ok(rtt) => {
                            debug!(rtt_ms = rtt.as_millis(), "Network OK");
                            if let Some(telemetry) = self.telemetry.as_ref() {
                                telemetry.record_connectivity_success(rtt);
                            }
                            failure_count = 0;
                        }
                        Err(_err) => {
                            if let Some(telemetry) = self.telemetry.as_ref() {
                                telemetry.record_connectivity_failure();
                            }
                            signal_failure_count = 0;
                            failure_count += 1;
                            warn!(
                                failure_count,
                                max_retries = self.config.internet_max_retries(),
                                "Connectivity check failed"
                            );

                            if failure_count >= self.config.internet_max_retries() {
                                let reboot_allowed = last_reboot
                                    .map(|instant| instant.elapsed() >= self.action.reboot.min_interval)
                                    .unwrap_or(true);

                                info!("Triggering connection recovery after {} failures", failure_count);

                                match self.recovery(RecoveryTrigger::Connectivity, reboot_allowed).await {
                                    Ok(RecoveryOutcome::ConnectivityRestored) => {
                                        failure_count = 0;
                                        signal_failure_count = 0;
                                    }
                                    Ok(RecoveryOutcome::Rebooted { rebooted_at }) => {
                                        last_reboot = Some(rebooted_at);
                                        failure_count = 0;
                                        signal_failure_count = 0;
                                    }
                                    Err(err) => {
                                        error!(error = %err, "Recovery failed");
                                    }
                                }
                            }
                        }
                    }

                    if let Some(telemetry) = self.telemetry.as_mut()
                        && let Err(error) = telemetry.collect().await
                    {
                        warn!(error = %error, "Telemetry collection failed");
                    }
                }
                _ = signal_interval.tick(), if self.config.signal.enabled => {
                    match self.check_signal_quality().await {
                        Ok(None) => {
                            signal_failure_count = 0;
                        }
                        Ok(Some(issue)) => {
                            if let Some(telemetry) = self.telemetry.as_ref() {
                                telemetry.record_signal_degraded(issue.metric_reason());
                            }
                            signal_failure_count += 1;
                            warn!(
                                signal_failure_count,
                                max_retries = self.config.signal_max_retries(),
                                issue = %issue.summary(),
                                "Signal quality check failed"
                            );

                            if signal_failure_count >= self.config.signal_max_retries() {
                                if let Some(telemetry) = self.telemetry.as_ref() {
                                    telemetry.record_signal_recovery_attempt();
                                }
                                let reboot_allowed = last_reboot
                                    .map(|instant| {
                                        instant.elapsed() >= self.action.reboot.min_interval
                                    })
                                    .unwrap_or(true);

                                info!(
                                    issue = %issue.summary(),
                                    "Triggering connection recovery after degraded signal"
                                );

                                match self.recovery(RecoveryTrigger::SignalQuality, reboot_allowed).await {
                                    Ok(RecoveryOutcome::ConnectivityRestored) => {
                                        if let Some(telemetry) = self.telemetry.as_ref() {
                                            telemetry.record_signal_recovery_success();
                                        }
                                        signal_failure_count = 0;
                                        failure_count = 0;
                                    }
                                    Ok(RecoveryOutcome::Rebooted { rebooted_at }) => {
                                        if let Some(telemetry) = self.telemetry.as_ref() {
                                            telemetry.record_signal_recovery_success();
                                        }
                                        last_reboot = Some(rebooted_at);
                                        signal_failure_count = 0;
                                        failure_count = 0;
                                    }
                                    Err(err) => {
                                        if let Some(telemetry) = self.telemetry.as_ref() {
                                            telemetry.record_signal_recovery_failure();
                                        }
                                        error!(error = %err, "Signal recovery failed");
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            warn!(error = %err, "Signal quality check failed");
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Shutdown signal received");
                    break;
                }
            }
        }

        info!("Monitor stopping, logging out...");
        if let Err(_err) = self.client.logout().await {
            error!("Logout failed");
        }
        info!("Monitor stopped");
        Ok(())
    }

    async fn check_connectivity(&self) -> Result<Duration> {
        let start = Instant::now();
        let response = self
            .check_client
            .get(&self.config.internet.url)
            .send()
            .await?;
        let status = response.status();

        if !status.is_success() && status.as_u16() != 204 {
            anyhow::bail!("Unexpected status: {status}");
        }

        Ok(start.elapsed())
    }

    async fn recovery(
        &self,
        trigger: RecoveryTrigger,
        reboot_allowed: bool,
    ) -> Result<RecoveryOutcome> {
        self.check_auth().await?;

        match self.config.recovery_method {
            RecoveryMethod::Reload => {
                match self.try_reload_recovery(trigger).await {
                    Ok(true) => return Ok(RecoveryOutcome::ConnectivityRestored),
                    Ok(false) => warn!(
                        trigger = ?trigger,
                        "Reload recovery did not restore the monitored condition, falling back to reboot"
                    ),
                    Err(err) => {
                        warn!(error = %err, "Reload recovery failed, falling back to reboot")
                    }
                }

                if !reboot_allowed {
                    anyhow::bail!("Reboot cooldown not elapsed for recovery fallback");
                }

                self.reboot_router().await
            }
            RecoveryMethod::Reboot => {
                if !reboot_allowed {
                    anyhow::bail!("Reboot cooldown not elapsed");
                }

                self.reboot_router().await
            }
        }
    }

    async fn check_auth(&self) -> Result<()> {
        if let Err(_err) = self
            .client
            .get_dal::<serde_json::Value>(DalOid::Status)
            .await
        {
            warn!("Session check failed, re-logging in");
            let _ = self.client.logout().await;
            self.client.login().await?;
        }

        Ok(())
    }

    async fn check_signal_quality(&self) -> Result<Option<SignalQualityIssue>> {
        if !self.config.signal.enabled {
            return Ok(None);
        }

        let status = self.fetch_cellwan_status().await?;

        Ok(signal_quality_issue(&status, &self.config.signal))
    }

    async fn fetch_cellwan_status(&self) -> Result<CellWanStatusObject> {
        match self.client.get_cellwan_status().await {
            Ok(status) => Ok(status),
            Err(initial_error) => {
                debug!(error = %initial_error, "Refreshing router session before retrying signal check");
                self.check_auth().await?;
                self.client.get_cellwan_status().await.with_context(|| {
                    format!("Failed to fetch cellwan_status after session refresh: {initial_error}")
                })
            }
        }
    }

    async fn try_reload_recovery(&self, trigger: RecoveryTrigger) -> Result<bool> {
        if let Some(telemetry) = self.telemetry.as_ref() {
            telemetry.record_reload_attempt();
        }
        let start = Instant::now();

        let original = self.client.get_cellwan_band().await?;
        let original_preferred = original.preferred_access_technology.clone();
        let temporary_preferred = if original_preferred == ACCESS_TECHNOLOGY_NR5G_SA {
            ACCESS_TECHNOLOGY_AUTO
        } else {
            ACCESS_TECHNOLOGY_NR5G_SA
        };

        let mut switch_config = original.clone();
        switch_config.preferred_access_technology = temporary_preferred.to_string();

        info!(
            preferred_access_technology = %temporary_preferred,
            "Temporarily switching preferred access technology"
        );
        self.client.set_cellwan_band(&switch_config).await?;
        sleep(self.action.reload.switch_wait).await;

        let mut restore_config = original;
        restore_config.preferred_access_technology = original_preferred;

        info!(
            preferred_access_technology = %restore_config.preferred_access_technology,
            "Restoring preferred access technology"
        );
        self.client.set_cellwan_band(&restore_config).await?;
        sleep(self.action.reload.restore_wait).await;

        let duration = start.elapsed();

        match self.recovery_target_restored(trigger).await {
            Ok(true) => {
                if let Some(telemetry) = self.telemetry.as_ref() {
                    telemetry.record_reload_success(duration);
                }
                Ok(true)
            }
            Ok(false) => {
                if let Some(telemetry) = self.telemetry.as_ref() {
                    telemetry.record_reload_failure(duration);
                }
                warn!(
                    trigger = ?trigger,
                    "Monitored condition still unhealthy after access technology recovery"
                );
                Ok(false)
            }
            Err(err) => {
                if let Some(telemetry) = self.telemetry.as_ref() {
                    telemetry.record_reload_failure(duration);
                }
                Err(err)
            }
        }
    }

    async fn recovery_target_restored(&self, trigger: RecoveryTrigger) -> Result<bool> {
        match trigger {
            RecoveryTrigger::Connectivity => Ok(self.check_connectivity().await.is_ok()),
            RecoveryTrigger::SignalQuality => {
                if self.check_connectivity().await.is_err() {
                    return Ok(false);
                }

                Ok(self.check_signal_quality().await?.is_none())
            }
        }
    }

    async fn reboot_router(&self) -> Result<RecoveryOutcome> {
        info!("Rebooting router as part of recovery");
        if let Some(telemetry) = self.telemetry.as_ref() {
            telemetry.record_reboot_attempt();
        }

        self.client.reboot().await?;
        let rebooted_at = Instant::now();

        if let Some(telemetry) = self.telemetry.as_ref() {
            telemetry.record_reboot_success();
        }

        if !self.action.reboot.wait_after.is_zero() {
            info!(
                wait_after_secs = self.action.reboot.wait_after.as_secs(),
                "Waiting after reboot before resuming connectivity checks"
            );
            sleep(self.action.reboot.wait_after).await;
        }

        Ok(RecoveryOutcome::Rebooted { rebooted_at })
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
