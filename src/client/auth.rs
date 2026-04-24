use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use serde::{Deserialize, Serialize};

use super::{ApiEndpoint, ZyxelClient};

#[derive(Serialize)]
struct LoginRequest<'a> {
    #[serde(rename = "Input_Account")]
    input_account: &'a str,
    #[serde(rename = "Input_Passwd")]
    input_passwd: String,
    #[serde(rename = "currLang")]
    curr_lang: &'static str,
    #[serde(rename = "RememberPassword")]
    remember_password: u8,
    #[serde(rename = "SHA512_password")]
    sha512_password: bool,
}

#[derive(Deserialize)]
pub struct LoginResponse {
    pub result: Option<String>,
    pub sessionkey: Option<serde_json::Number>,
}

const LOGIN_EP: ApiEndpoint = ApiEndpoint {
    path: "/UserLogin",
    method: "POST",
    requires_auth: false,
    encrypt_request: true,
    include_aes_key: true,
};

const LOGOUT_EP: ApiEndpoint = ApiEndpoint {
    path: "/cgi-bin/UserLogout",
    method: "POST",
    requires_auth: true,
    encrypt_request: false,
    include_aes_key: false,
};

impl ZyxelClient {
    pub async fn login(&self) -> Result<()> {
        let req = LoginRequest {
            input_account: &self.username,
            input_passwd: B64.encode(self.password.as_bytes()),
            curr_lang: "en",
            remember_password: 0,
            sha512_password: false,
        };

        let resp: Option<LoginResponse> = self.execute(&LOGIN_EP, Some(&req)).await?;

        if let Some(response) = resp {
            if response.result.as_deref() != Some("ZCFG_SUCCESS") {
                anyhow::bail!("Login failed: result = {:?}", response.result);
            }

            let session_key = response
                .sessionkey
                .as_ref()
                .and_then(serde_json::Number::as_i64)
                .unwrap_or_else(|| self.session_key());

            if session_key == 0 {
                anyhow::bail!("Login succeeded without session key");
            }
        } else if self.session_key() == 0 {
            anyhow::bail!("Login returned empty response without session key");
        }

        Ok(())
    }

    pub async fn logout(&self) -> Result<()> {
        let mut url = format!("{}{}", self.base_url, LOGOUT_EP.path);
        let session_key = self.session_key();
        if session_key != 0 {
            url.push_str(&format!("?sessionkey={session_key}"));
        }

        let response = self
            .http
            .post(&url)
            .send()
            .await
            .map_err(anyhow::Error::from)?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP {} for {}", response.status(), LOGOUT_EP.path);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::LoginRequest;
    use base64::{Engine, engine::general_purpose::STANDARD as B64};
    use reqwest::ClientBuilder;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, atomic::AtomicI64};
    use std::thread;

    use crate::client::ZyxelClient;

    fn spawn_http_server(responses: Vec<String>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer).unwrap();
                stream.write_all(response.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });

        format!("http://{addr}")
    }

    fn http_response(content_type: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn test_client(base_url: String, session_key: i64) -> ZyxelClient {
        ZyxelClient {
            base_url,
            username: "admin".to_string(),
            password: "secret".to_string(),
            http: ClientBuilder::new().build().unwrap(),
            crypto: None,
            session_key: Arc::new(AtomicI64::new(session_key)),
            use_https: true,
        }
    }

    #[test]
    fn login_request_uses_router_expected_fields() {
        let request = LoginRequest {
            input_account: "admin",
            input_passwd: B64.encode("secret".as_bytes()),
            curr_lang: "en",
            remember_password: 0,
            sha512_password: false,
        };

        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({
                "Input_Account": "admin",
                "Input_Passwd": "c2VjcmV0",
                "currLang": "en",
                "RememberPassword": 0,
                "SHA512_password": false,
            })
        );
    }

    #[tokio::test]
    async fn login_accepts_success_response() {
        let host = spawn_http_server(vec![http_response(
            "application/json",
            r#"{"result":"ZCFG_SUCCESS","sessionkey":9}"#,
        )]);
        let client = test_client(host, 0);

        client.login().await.unwrap();

        assert_eq!(client.session_key(), 9);
    }

    #[tokio::test]
    async fn login_rejects_success_response_without_session_key() {
        let host = spawn_http_server(vec![http_response(
            "application/json",
            r#"{"result":"ZCFG_SUCCESS"}"#,
        )]);
        let client = test_client(host, 0);

        let error = client.login().await.unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Login succeeded without session key")
        );
    }

    #[tokio::test]
    async fn logout_ignores_non_json_response_body() {
        let host = spawn_http_server(vec![http_response("text/plain", "OK")]);
        let client = test_client(host, 42);

        client.logout().await.unwrap();
    }

    #[tokio::test]
    async fn logout_propagates_http_failures() {
        let host = spawn_http_server(vec![
            "HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                .to_string(),
        ]);
        let client = test_client(host, 42);

        let error = client.logout().await.unwrap_err();

        assert!(error.to_string().contains("HTTP 500 Internal Server Error"));
    }
}
