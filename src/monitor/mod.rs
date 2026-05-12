mod auth;
mod internet;
mod signal;

use anyhow::Result;
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::client::ZyxelClient;
use crate::config::{ActionConfig, MonitorConfig, RecoveryMethod};
use crate::telemetry::metrics::TelemetryCollector;

use self::auth::ensure_authenticated;
use self::internet::InternetMonitor;
use self::signal::SignalMonitor;

const ACCESS_TECHNOLOGY_AUTO: &str = "Auto";
const ACCESS_TECHNOLOGY_NR5G_SA: &str = "NR5G-SA";

#[derive(Debug)]
enum CheckResult<T, E> {
    Healthy(T),
    Degraded(E),
}

trait QualityMonitor {
    type Success;
    type Issue;

    fn interval(&self) -> std::time::Duration;

    fn max_retries(&self) -> u32;

    fn enabled(&self) -> bool {
        true
    }

    fn check(&self)
    -> impl Future<Output = Result<CheckResult<Self::Success, Self::Issue>>> + Send;
}

enum RecoveryOutcome {
    ConnectivityRestored,
    Rebooted { rebooted_at: Instant },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecoveryTrigger {
    Connectivity,
    SignalQuality,
}

pub struct Monitor {
    client: Arc<ZyxelClient>,
    recovery_method: RecoveryMethod,
    action: ActionConfig,
    internet: InternetMonitor,
    signal: SignalMonitor,
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
        let internet_interval = config.internet_interval();
        let internet_max_retries = config.internet_max_retries();
        let signal_interval = config.signal_interval();
        let signal_max_retries = config.signal_max_retries();
        let recovery_method = config.recovery_method;

        let internet =
            InternetMonitor::new(config.internet, internet_interval, internet_max_retries)?;
        let signal = SignalMonitor::new(
            Arc::clone(&client),
            config.signal,
            signal_interval,
            signal_max_retries,
        );

        Ok(Self {
            client,
            recovery_method,
            action,
            internet,
            signal,
            telemetry,
        })
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Monitor started");
        let mut failure_count: u32 = 0;
        let mut signal_failure_count: u32 = 0;
        let mut last_reboot: Option<Instant> = None;
        let mut internet_interval = tokio::time::interval(self.internet.interval());
        let mut signal_interval = tokio::time::interval(self.signal.interval());

        loop {
            tokio::select! {
                biased;

                _ = internet_interval.tick() => {
                    match self.internet.check().await {
                        Ok(CheckResult::Healthy(rtt)) => {
                            debug!(rtt_ms = rtt.as_millis(), "Network OK");
                            if let Some(telemetry) = self.telemetry.as_ref() {
                                telemetry.record_connectivity_success(rtt);
                            }
                            failure_count = 0;
                        }
                        Ok(CheckResult::Degraded(issue)) => {
                            if let Some(telemetry) = self.telemetry.as_ref() {
                                telemetry.record_connectivity_failure();
                            }
                            signal_failure_count = 0;
                            failure_count += 1;
                            warn!(
                                failure_count,
                                max_retries = self.internet.max_retries(),
                                issue = %issue.summary(),
                                "Connectivity check failed"
                            );

                            if failure_count >= self.internet.max_retries() {
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
                        Err(err) => {
                            warn!(error = %err, "Connectivity monitor failed");
                        }
                    }

                    if let Some(telemetry) = self.telemetry.as_mut()
                        && let Err(error) = telemetry.collect().await
                    {
                        warn!(error = %error, "Telemetry collection failed");
                    }
                }
                _ = signal_interval.tick(), if self.signal.enabled() => {
                    match self.signal.check().await {
                        Ok(CheckResult::Healthy(())) => {
                            signal_failure_count = 0;
                        }
                        Ok(CheckResult::Degraded(issue)) => {
                            if let Some(telemetry) = self.telemetry.as_ref() {
                                telemetry.record_signal_degraded(issue.metric_reason());
                            }
                            signal_failure_count += 1;
                            warn!(
                                signal_failure_count,
                                max_retries = self.signal.max_retries(),
                                issue = %issue.summary(),
                                "Signal quality check failed"
                            );

                            if signal_failure_count >= self.signal.max_retries() {
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

    async fn recovery(
        &self,
        trigger: RecoveryTrigger,
        reboot_allowed: bool,
    ) -> Result<RecoveryOutcome> {
        ensure_authenticated(&self.client).await?;

        match self.recovery_method {
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
            RecoveryTrigger::Connectivity => Ok(matches!(
                self.internet.check().await?,
                CheckResult::Healthy(_)
            )),
            RecoveryTrigger::SignalQuality => {
                if !matches!(self.internet.check().await?, CheckResult::Healthy(_)) {
                    return Ok(false);
                }

                Ok(matches!(
                    self.signal.check().await?,
                    CheckResult::Healthy(())
                ))
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
