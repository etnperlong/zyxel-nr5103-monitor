use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::client::ZyxelClient;
use crate::config::MonitorConfig;
use crate::telemetry::metrics::TelemetryCollector;

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
                                let cooldown_ok = last_reboot
                                    .map(|instant| instant.elapsed() >= self.config.min_reboot_interval)
                                    .unwrap_or(true);

                                if cooldown_ok {
                                    info!("Triggering router reboot after {} failures", failure_count);
                                    if let Some(telemetry) = self.telemetry.as_ref() {
                                        telemetry.record_reboot_attempt();
                                    }

                                    if let Err(_err) = self.recovery().await {
                                        error!("Recovery failed");
                                    } else {
                                        if let Some(telemetry) = self.telemetry.as_ref() {
                                            telemetry.record_reboot_success();
                                        }
                                        last_reboot = Some(Instant::now());
                                        failure_count = 0;
                                    }
                                } else {
                                    debug!("Reboot cooldown not elapsed, skipping");
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

    async fn recovery(&self) -> Result<()> {
        self.check_auth().await?;
        self.client.reboot().await?;
        Ok(())
    }

    async fn check_auth(&self) -> Result<()> {
        if let Err(_err) = self.client.get_basic_information().await {
            warn!("Session check failed, re-logging in");
            let _ = self.client.logout().await;
            self.client.login().await?;
        }

        Ok(())
    }
}
