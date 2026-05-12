use anyhow::Result;
use tracing::warn;

use crate::client::{ZyxelClient, dal::DalOid};

pub(super) async fn ensure_authenticated(client: &ZyxelClient) -> Result<()> {
    if let Err(_err) = client.get_dal::<serde_json::Value>(DalOid::Status).await {
        warn!("Session check failed, re-logging in");
        let _ = client.logout().await;
        client.login().await?;
    }

    Ok(())
}
