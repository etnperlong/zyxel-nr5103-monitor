mod client;
mod config;
mod monitor;

use anyhow::Result;
use std::sync::Arc;
use tracing::info;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = config::load_config()?;

    let filter = EnvFilter::try_new(&cfg.log_level).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing::subscriber::set_global_default(
        FmtSubscriber::builder().with_env_filter(filter).finish(),
    )?;

    info!("Starting zyxel-nr5103-monitor");

    let router_client = Arc::new(client::ZyxelClient::new(&cfg.router).await?);

    router_client.login().await?;
    info!(
        session_key = router_client.session_key(),
        "Login successful"
    );

    let device_info = router_client.get_basic_information().await?;
    info!(
        model = %device_info.model_name,
        firmware = %device_info.software_version,
        "Device info"
    );

    info!("Starting monitor loop");
    let monitor = monitor::Monitor::new(Arc::clone(&router_client), cfg.monitor)?;
    monitor.run().await?;

    Ok(())
}
