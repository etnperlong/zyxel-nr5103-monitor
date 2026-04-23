use rsa::{RsaPrivateKey, pkcs8::EncodePublicKey};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use zyxel_nr5103_monitor::{
    client::{ApiEndpoint, ZyxelClient},
    config::RouterConfig,
};

fn spawn_http_server(responses: Vec<String>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 2048];
            let _ = stream.read(&mut buffer).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        }
    });

    format!("http://{addr}")
}

fn http_json_response(body: String) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(), body
    )
}

fn test_public_key_pem() -> String {
    RsaPrivateKey::new(&mut rand::thread_rng(), 2048)
        .unwrap()
        .to_public_key()
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .unwrap()
}

#[tokio::test]
async fn new_uses_https_mode_without_fetching_rsa_key() {
    let client = ZyxelClient::new(&RouterConfig {
        host: "router.local".to_string(),
        username: "admin".to_string(),
        password: "secret".to_string(),
    })
    .await
    .unwrap();

    assert_eq!(client.session_key(), 0);
}

#[tokio::test]
async fn execute_updates_session_key_from_json_response() {
    let rsa_response = http_json_response(format!(
        r#"{{"RSAPublicKey":{}}}"#,
        serde_json::to_string(&test_public_key_pem()).unwrap()
    ));
    let api_response = http_json_response(r#"{"sessionkey":42,"ok":true}"#.to_string());
    let host = spawn_http_server(vec![rsa_response, api_response]);

    let client = ZyxelClient::new(&RouterConfig {
        host,
        username: "admin".to_string(),
        password: "secret".to_string(),
    })
    .await
    .unwrap();

    let response = client
        .execute::<(), Value>(
            &ApiEndpoint {
                path: "/status",
                method: "GET",
                requires_auth: false,
                encrypt_request: false,
                include_aes_key: false,
            },
            None,
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(response["ok"], true);
    assert_eq!(client.session_key(), 42);
}
