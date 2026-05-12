use anyhow::Result;
use std::time::{Duration, Instant};

use crate::config::InternetConfig;

use super::{CheckResult, QualityMonitor};

pub(super) struct InternetMonitor {
    config: InternetConfig,
    interval: Duration,
    max_retries: u32,
    client: reqwest::Client,
}

impl InternetMonitor {
    pub(super) fn new(
        config: InternetConfig,
        interval: Duration,
        max_retries: u32,
    ) -> Result<Self> {
        let client = reqwest::ClientBuilder::new()
            .danger_accept_invalid_certs(true)
            .timeout(config.timeout)
            .build()?;

        Ok(Self {
            config,
            interval,
            max_retries,
            client,
        })
    }
}

impl QualityMonitor for InternetMonitor {
    type Success = Duration;
    type Issue = ConnectivityIssue;

    fn interval(&self) -> Duration {
        self.interval
    }

    fn max_retries(&self) -> u32 {
        self.max_retries
    }

    async fn check(&self) -> Result<CheckResult<Self::Success, Self::Issue>> {
        let start = Instant::now();
        let response = match self.client.get(&self.config.url).send().await {
            Ok(response) => response,
            Err(error) => return Ok(CheckResult::Degraded(ConnectivityIssue::new(error))),
        };
        let status = response.status();

        if !status.is_success() && status.as_u16() != 204 {
            return Ok(CheckResult::Degraded(ConnectivityIssue::from_status(
                status,
            )));
        }

        Ok(CheckResult::Healthy(start.elapsed()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConnectivityIssue {
    summary: String,
}

impl ConnectivityIssue {
    fn new(error: reqwest::Error) -> Self {
        Self {
            summary: error.to_string(),
        }
    }

    fn from_status(status: reqwest::StatusCode) -> Self {
        Self {
            summary: format!("Unexpected status: {status}"),
        }
    }

    pub(super) fn summary(&self) -> &str {
        &self.summary
    }
}
