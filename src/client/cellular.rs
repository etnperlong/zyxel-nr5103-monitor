use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use super::{
    ZyxelClient,
    dal::{DalOid, DalResponse},
};
use crate::telemetry::zyxel::CellWanStatusObject;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CellWanBandConfig {
    #[serde(rename = "INTF_Supported_Access_Technologies")]
    pub supported_access_technologies: String,
    #[serde(rename = "INTF_Preferred_Access_Technology")]
    pub preferred_access_technology: String,
    #[serde(rename = "INTF_Current_Access_Technology")]
    pub current_access_technology: String,
    #[serde(rename = "INTF_Supported_Bands")]
    pub supported_bands: String,
    #[serde(rename = "INTF_Preferred_Bands")]
    pub preferred_bands: String,
    #[serde(rename = "INTF_Current_Band")]
    pub current_band: String,
}

#[derive(Debug, Deserialize)]
struct DalUpdateResponse {
    result: String,
}

impl ZyxelClient {
    pub async fn get_cellwan_status(&self) -> Result<CellWanStatusObject> {
        self.get_dal::<DalResponse<CellWanStatusObject>>(DalOid::CellWanStatus)
            .await?
            .into_first_object()
            .context("No cellwan_status object returned from DAL endpoint")
    }

    pub async fn get_cellwan_band(&self) -> Result<CellWanBandConfig> {
        self.get_dal::<DalResponse<CellWanBandConfig>>(DalOid::CellWanBand)
            .await?
            .into_first_object()
            .context("No cellwan_band object returned from DAL endpoint")
    }

    pub async fn set_cellwan_band(&self, config: &CellWanBandConfig) -> Result<()> {
        let response = self
            .set_dal::<_, DalUpdateResponse>(DalOid::CellWanBand, config)
            .await?;

        if response.result.trim().eq_ignore_ascii_case("ZCFG_SUCCESS")
            || response.result.trim().eq_ignore_ascii_case("SUCCESS")
        {
            return Ok(());
        }

        bail!("Failed to update cellwan_band: {}", response.result)
    }
}
