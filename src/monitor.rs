use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::client::{ZyxelClient, dal::DalOid};
use crate::config::{MonitorConfig, RecoveryMethod};
use crate::telemetry::metrics::TelemetryCollector;

const ACCESS_TECHNOLOGY_AUTO: &str = "Auto";
const ACCESS_TECHNOLOGY_NR5G_SA: &str = "NR5G-SA";

enum RecoveryOutcome {
    ConnectivityRestored,
    Rebooted { rebooted_at: Instant },
}

pub struct Monitor {
    client: Arc<ZyxelClient>,
    config: MonitorConfig,
    check_client: reqwest::Client,
    telemetry: Option<TelemetryCollector>,
}

impl Monitor {
    pub fn new(client: Arc<ZyxelClient>, config: MonitorConfig) -> Result<Self> {
        Self::build(client, config, None)
    }

    pub fn with_telemetry(mut self, telemetry: TelemetryCollector) -> Self {
        self.telemetry = Some(telemetry);
        self
    }

    fn build(
        client: Arc<ZyxelClient>,
        config: MonitorConfig,
        telemetry: Option<TelemetryCollector>,
    ) -> Result<Self> {
        let check_client = reqwest::ClientBuilder::new()
            .danger_accept_invalid_certs(true)
            .timeout(config.timeout)
            .build()?;

        Ok(Self {
            client,
            config,
            check_client,
            telemetry,
        })
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Monitor started");
        let mut failure_count: u32 = 0;
        let mut last_reboot: Option<Instant> = None;
        let mut interval = tokio::time::interval(self.config.interval);

        loop {
            tokio::select! {
                _ = interval.tick() => {
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
                            failure_count += 1;
                            warn!(
                                failure_count,
                                max_retries = self.config.max_retries,
                                "Connectivity check failed"
                            );

                            if failure_count >= self.config.max_retries {
                                let reboot_allowed = last_reboot
                                    .map(|instant| instant.elapsed() >= self.config.reboot.min_interval)
                                    .unwrap_or(true);

                                info!("Triggering connection recovery after {} failures", failure_count);

                                match self.recovery(reboot_allowed).await {
                                    Ok(RecoveryOutcome::ConnectivityRestored) => {
                                        failure_count = 0;
                                    }
                                    Ok(RecoveryOutcome::Rebooted { rebooted_at }) => {
                                        last_reboot = Some(rebooted_at);
                                        failure_count = 0;
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
        let response = self.check_client.get(&self.config.url).send().await?;
        let status = response.status();

        if !status.is_success() && status.as_u16() != 204 {
            anyhow::bail!("Unexpected status: {status}");
        }

        Ok(start.elapsed())
    }

    async fn recovery(&self, reboot_allowed: bool) -> Result<RecoveryOutcome> {
        self.check_auth().await?;

        match self.config.recovery_method {
            RecoveryMethod::Reload => {
                match self.try_reload_recovery().await {
                    Ok(true) => return Ok(RecoveryOutcome::ConnectivityRestored),
                    Ok(false) => warn!(
                        "Reload recovery did not restore connectivity, falling back to reboot"
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

    async fn try_reload_recovery(&self) -> Result<bool> {
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
        sleep(self.config.reload.switch_wait).await;

        let mut restore_config = original;
        restore_config.preferred_access_technology = original_preferred;

        info!(
            preferred_access_technology = %restore_config.preferred_access_technology,
            "Restoring preferred access technology"
        );
        self.client.set_cellwan_band(&restore_config).await?;
        sleep(self.config.reload.restore_wait).await;

        match self.check_connectivity().await {
            Ok(_) => Ok(true),
            Err(err) => {
                warn!(
                    error = %err,
                    "Connectivity still unavailable after access technology recovery"
                );
                Ok(false)
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

        if !self.config.reboot.wait_after.is_zero() {
            info!(
                wait_after_secs = self.config.reboot.wait_after.as_secs(),
                "Waiting after reboot before resuming connectivity checks"
            );
            sleep(self.config.reboot.wait_after).await;
        }

        Ok(RecoveryOutcome::Rebooted { rebooted_at })
    }
}
