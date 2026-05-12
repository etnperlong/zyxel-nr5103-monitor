use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};

use super::{ApiEndpoint, ZyxelClient};

const DAL_EP: ApiEndpoint = ApiEndpoint {
    path: "/cgi-bin/DAL",
    method: "GET",
    requires_auth: true,
    encrypt_request: false,
    include_aes_key: false,
};

const DAL_SET_EP: ApiEndpoint = ApiEndpoint {
    path: "/cgi-bin/DAL",
    method: "PUT",
    requires_auth: true,
    encrypt_request: true,
    include_aes_key: false,
};

#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
pub struct DalResponse<T> {
    pub result: String,
    #[serde(
        rename = "Object",
        default,
        deserialize_with = "deserialize_vec_or_single"
    )]
    pub object: Vec<T>,
}

impl<T> DalResponse<T> {
    pub fn into_first_object(self) -> Option<T> {
        if !is_success_result(&self.result) {
            return None;
        }

        self.object.into_iter().next()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DalOid {
    Status,
    CellWanStatus,
    CellWanBand,
    TrafficStatus,
}

impl DalOid {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::CellWanStatus => "cellwan_status",
            Self::CellWanBand => "cellwan_band",
            Self::TrafficStatus => "Traffic_Status",
        }
    }
}

impl ZyxelClient {
    pub async fn get_dal<T>(&self, oid: DalOid) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let path = format!("{}?oid={}", DAL_EP.path, oid.as_str());

        self.execute_path::<(), T>(&DAL_EP, &path, None)
            .await?
            .context("Empty response from DAL endpoint")
    }

    pub async fn set_dal<Req, Resp>(&self, oid: DalOid, body: &Req) -> Result<Resp>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        let path = format!("{}?oid={}", DAL_SET_EP.path, oid.as_str());

        self.execute_path(&DAL_SET_EP, &path, Some(body))
            .await?
            .context("Empty response from DAL endpoint")
    }
}

fn is_success_result(result: &str) -> bool {
    let trimmed = result.trim();
    trimmed.eq_ignore_ascii_case("ZCFG_SUCCESS") || trimmed.eq_ignore_ascii_case("SUCCESS")
}

fn deserialize_vec_or_single<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let Some(value) = Option::<serde_json::Value>::deserialize(deserializer)? else {
        return Ok(Vec::new());
    };

    match value {
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<T>, _>>()
            .map_err(serde::de::Error::custom),
        serde_json::Value::Object(_) => serde_json::from_value(value)
            .map(|item| vec![item])
            .map_err(serde::de::Error::custom),
        serde_json::Value::Null | serde_json::Value::String(_) => Ok(Vec::new()),
        other => serde_json::from_value(other)
            .map(|item| vec![item])
            .map_err(serde::de::Error::custom),
    }
}

#[cfg(test)]
mod tests {
    use reqwest::ClientBuilder;
    use serde_json::Value;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, atomic::AtomicI64};
    use std::thread;

    use crate::client::{ApiEndpoint, ZyxelClient};

    use super::DalOid;

    fn http_response(status_line: &str, headers: &[(&str, &str)], body: &str) -> String {
        let mut response = format!(
            "HTTP/1.1 {status_line}\r\ncontent-length: {}\r\nconnection: close\r\n",
            body.len()
        );

        for (name, value) in headers {
            response.push_str(name);
            response.push_str(": ");
            response.push_str(value);
            response.push_str("\r\n");
        }

        response.push_str("\r\n");
        response.push_str(body);
        response
    }

    fn spawn_http_server<F>(handler: F) -> String
    where
        F: FnOnce(TcpListener) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        thread::spawn(move || handler(listener));

        format!("http://{addr}")
    }

    fn read_request(stream: &mut std::net::TcpStream) -> String {
        let mut buffer = [0_u8; 8192];
        let bytes_read = stream.read(&mut buffer).unwrap();
        String::from_utf8(buffer[..bytes_read].to_vec()).unwrap()
    }

    fn test_client(base_url: String, session_key: i64) -> ZyxelClient {
        ZyxelClient {
            base_url,
            username: "admin".to_string(),
            password: "secret".to_string(),
            http: ClientBuilder::new().cookie_store(true).build().unwrap(),
            crypto: None,
            session_key: Arc::new(AtomicI64::new(session_key)),
            use_https: true,
        }
    }

    #[tokio::test]
    async fn authenticated_request_without_existing_query_uses_question_mark_for_session_key() {
        let host = spawn_http_server(|listener| {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);

            let response = if request.starts_with("POST /cgi-bin/Reboot?sessionkey=42 HTTP/1.1") {
                http_response("200 OK", &[("content-type", "application/json")], "{}")
            } else {
                http_response("400 Bad Request", &[("content-type", "text/plain")], "bad")
            };

            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        });
        let client = test_client(host, 42);

        let response = client
            .execute::<(), Value>(
                &ApiEndpoint {
                    path: "/cgi-bin/Reboot",
                    method: "POST",
                    requires_auth: true,
                    encrypt_request: false,
                    include_aes_key: false,
                },
                None,
            )
            .await;

        assert!(response.is_ok(), "request failed: {response:?}");
    }

    #[tokio::test]
    async fn authenticated_request_with_existing_query_uses_ampersand_for_session_key() {
        let host = spawn_http_server(|listener| {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);

            let response =
                if request.starts_with("GET /cgi-bin/DAL?oid=status&sessionkey=42 HTTP/1.1") {
                    http_response("200 OK", &[("content-type", "application/json")], "{}")
                } else {
                    http_response("400 Bad Request", &[("content-type", "text/plain")], "bad")
                };

            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        });
        let client = test_client(host, 42);

        let response = client.get_dal::<Value>(DalOid::Status).await;

        assert!(response.is_ok(), "request failed: {response:?}");
    }

    #[tokio::test]
    async fn authenticated_dal_request_preserves_login_cookie() {
        let host = spawn_http_server(|listener| {
            let (mut login_stream, _) = listener.accept().unwrap();
            let login_request = read_request(&mut login_stream);
            assert!(login_request.starts_with("POST /UserLogin HTTP/1.1"));

            let login_response = http_response(
                "200 OK",
                &[
                    ("content-type", "application/json"),
                    ("set-cookie", "sessionid=router-cookie; Path=/; HttpOnly"),
                ],
                r#"{"result":"ZCFG_SUCCESS","sessionkey":9}"#,
            );
            login_stream.write_all(login_response.as_bytes()).unwrap();
            login_stream.flush().unwrap();

            let (mut dal_stream, _) = listener.accept().unwrap();
            let dal_request = read_request(&mut dal_stream);

            let has_expected_path =
                dal_request.starts_with("GET /cgi-bin/DAL?oid=status&sessionkey=9 HTTP/1.1");
            let has_cookie = dal_request.lines().any(|line| {
                line == "cookie: sessionid=router-cookie"
                    || line == "Cookie: sessionid=router-cookie"
            });

            let response = if has_expected_path && has_cookie {
                http_response(
                    "200 OK",
                    &[("content-type", "application/json")],
                    r#"{"ok":true}"#,
                )
            } else {
                http_response("400 Bad Request", &[("content-type", "text/plain")], "bad")
            };

            dal_stream.write_all(response.as_bytes()).unwrap();
            dal_stream.flush().unwrap();
        });
        let client = test_client(host, 0);

        client.login().await.unwrap();
        let response = client.get_dal::<Value>(DalOid::Status).await;

        assert!(response.is_ok(), "request failed: {response:?}");
    }

    #[test]
    fn dal_oid_values_match_router_endpoint_names() {
        assert_eq!(DalOid::Status.as_str(), "status");
        assert_eq!(DalOid::CellWanStatus.as_str(), "cellwan_status");
        assert_eq!(DalOid::CellWanBand.as_str(), "cellwan_band");
        assert_eq!(DalOid::TrafficStatus.as_str(), "Traffic_Status");
    }
}
