mod client;
mod config;
mod monitor;
mod telemetry;

use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = config::load_config()?;

    let telemetry = telemetry::otel::TelemetryRuntime::init(&cfg.telemetry)?;
    telemetry::otel::install_subscriber(&cfg.log_level, &telemetry)?;

    let monitor_result = async {
        info!("Starting zyxel-nr5103-monitor");

        let router_client = Arc::new(client::ZyxelClient::new(&cfg.router).await?);

        router_client.login().await?;
        info!("Login successful");

        let device_info = router_client.get_basic_information().await?;
        info!(
            model = %device_info.model_name,
            firmware = %device_info.software_version,
            "Device info"
        );

        info!("Starting monitor loop");
        let monitor = monitor::Monitor::new(Arc::clone(&router_client), cfg.monitor)?;
        monitor.run().await
    }
    .await;

    let shutdown_result = telemetry.shutdown();

    if let Err(shutdown_error) = shutdown_result {
        if monitor_result.is_err() {
            warn!(
                error = %shutdown_error,
                "Telemetry shutdown failed while monitor was already failing"
            );
        } else {
            return Err(shutdown_error);
        }
    }

    monitor_result?;

    Ok(())
}
