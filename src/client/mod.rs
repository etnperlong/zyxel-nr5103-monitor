pub mod auth;
pub mod crypto;
pub mod device;

use anyhow::{Context, Result, bail};
use reqwest::{
    Client, ClientBuilder,
    header::{self},
};
use serde::{Serialize, de::DeserializeOwned};
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use crate::config::RouterConfig;
use crypto::{CryptoState, EncryptedResponse};

pub struct ZyxelClient {
    base_url: String,
    username: String,
    password: String,
    http: Client,
    crypto: Option<CryptoState>,
    session_key: Arc<AtomicI64>,
    use_https: bool,
}

#[derive(Debug)]
pub struct ApiEndpoint {
    pub path: &'static str,
    pub method: &'static str,
    pub requires_auth: bool,
    pub encrypt_request: bool,
    pub include_aes_key: bool,
}

impl ZyxelClient {
    pub async fn new(cfg: &RouterConfig) -> Result<Self> {
        let use_https = !cfg.host.to_lowercase().starts_with("http://");
        let base_url = if use_https {
            format!("https://{}", cfg.host.trim_start_matches("https://"))
        } else {
            format!("http://{}", cfg.host.trim_start_matches("http://"))
        };

        let http = ClientBuilder::new()
            .danger_accept_invalid_certs(true)
            .cookie_store(true)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client")?;

        let crypto = if use_https {
            None
        } else {
            let pem = Self::fetch_rsa_key_static(&http, &base_url).await?;
            Some(CryptoState::new(&pem)?)
        };

        Ok(Self {
            base_url,
            username: cfg.username.clone(),
            password: cfg.password.clone(),
            http,
            crypto,
            session_key: Arc::new(AtomicI64::new(0)),
            use_https,
        })
    }

    async fn fetch_rsa_key_static(http: &Client, base_url: &str) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct RsaKeyResp {
            #[serde(rename = "RSAPublicKey")]
            rsa_public_key: String,
        }

        let url = format!("{base_url}/getRSAPublickKey");
        let response: RsaKeyResp = http
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .context("Failed to fetch RSA public key")?;

        Ok(response.rsa_public_key)
    }

    pub async fn execute<Req, Resp>(
        &self,
        ep: &ApiEndpoint,
        body: Option<&Req>,
    ) -> Result<Option<Resp>>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        let _credentials = (&self.username, &self.password);

        let mut url = format!("{}{}", self.base_url, ep.path);
        if ep.requires_auth {
            let session_key = self.session_key.load(Ordering::SeqCst);
            if session_key != 0 {
                url.push_str(&format!("?sessionkey={session_key}"));
            }
        }

        let request_builder = match ep.method {
            "GET" => self.http.get(&url),
            "POST" => self.http.post(&url),
            "DELETE" => self.http.delete(&url),
            other => bail!("Unsupported HTTP method: {other}"),
        };

        let request_builder = if let Some(body) = body {
            if !self.use_https && ep.encrypt_request {
                let crypto = self.crypto.as_ref().context("No crypto state in HTTP mode")?;
                let encrypted = crypto.encrypt_json(body, ep.include_aes_key)?;
                request_builder
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .json(&encrypted)
            } else {
                request_builder
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .json(body)
            }
        } else {
            request_builder
        };

        let http_response = request_builder.send().await.context("HTTP request failed")?;

        if !http_response.status().is_success() {
            bail!("HTTP {} for {}", http_response.status(), ep.path);
        }

        let raw_bytes = http_response.bytes().await?;
        if raw_bytes.is_empty() {
            return Ok(None);
        }

        let json_bytes = if !self.use_https && ep.encrypt_request && body.is_some() {
            let encrypted_response: EncryptedResponse = serde_json::from_slice(&raw_bytes)?;
            let crypto = self.crypto.as_ref().context("No crypto state in HTTP mode")?;
            crypto.decrypt_response(&encrypted_response)?
        } else {
            raw_bytes.to_vec()
        };

        self.try_extract_session_key(&json_bytes);

        let parsed = serde_json::from_slice(&json_bytes).context("Failed to deserialize response")?;
        Ok(Some(parsed))
    }

    fn try_extract_session_key(&self, data: &[u8]) {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(data)
            && let Some(session_key) = value.get("sessionkey").and_then(|value| value.as_i64())
            && session_key != 0
        {
            self.session_key.store(session_key, Ordering::SeqCst);
        }
    }

    pub fn session_key(&self) -> i64 {
        self.session_key.load(Ordering::SeqCst)
    }
}
