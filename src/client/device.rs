use anyhow::{Context, Result};
use serde::Deserialize;

use super::{ApiEndpoint, ZyxelClient};

#[derive(Debug, Deserialize)]
pub struct BasicInformation {
    #[serde(rename = "ModelName")]
    pub model_name: String,
    #[serde(rename = "SoftwareVersion")]
    pub software_version: String,
}

const GET_INFO_EP: ApiEndpoint = ApiEndpoint {
    path: "/getBasicInformation",
    method: "GET",
    requires_auth: false,
    encrypt_request: false,
    include_aes_key: false,
};

const REBOOT_EP: ApiEndpoint = ApiEndpoint {
    path: "/cgi-bin/Reboot",
    method: "POST",
    requires_auth: true,
    encrypt_request: false,
    include_aes_key: false,
};

impl ZyxelClient {
    pub async fn get_basic_information(&self) -> Result<BasicInformation> {
        self.execute::<(), _>(&GET_INFO_EP, None)
            .await?
            .context("Empty response from getBasicInformation")
    }

    pub async fn reboot(&self) -> Result<()> {
        let _ = self
            .execute::<(), serde_json::Value>(&REBOOT_EP, None)
            .await?;
        Ok(())
    }
}
