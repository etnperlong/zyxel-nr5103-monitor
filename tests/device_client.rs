use rsa::{RsaPrivateKey, pkcs8::EncodePublicKey};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use zyxel_nr5103_monitor::{client::ZyxelClient, config::RouterConfig};

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
        body.len(),
        body
    )
}

fn http_empty_response() -> String {
    "HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".to_string()
}

fn test_public_key_pem() -> String {
    RsaPrivateKey::new(&mut rand::thread_rng(), 2048)
        .unwrap()
        .to_public_key()
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .unwrap()
}

#[tokio::test]
async fn get_basic_information_returns_device_details() {
    let rsa_response = http_json_response(format!(
        r#"{{"RSAPublicKey":{}}}"#,
        serde_json::to_string(&test_public_key_pem()).unwrap()
    ));
    let info_response = http_json_response(
        r#"{"ModelName":"NR5103E","SoftwareVersion":"1.00(ABUV.6)C0"}"#.to_string(),
    );
    let host = spawn_http_server(vec![rsa_response, info_response]);

    let client = ZyxelClient::new(&RouterConfig {
        host: host.trim_start_matches("http://").to_string(),
        protocol: "http".to_string(),
        username: "admin".to_string(),
        password: "secret".to_string(),
    })
    .await
    .unwrap();

    let info = client.get_basic_information().await.unwrap();

    assert_eq!(info.model_name, "NR5103E");
    assert_eq!(info.software_version, "1.00(ABUV.6)C0");
}

#[tokio::test]
async fn reboot_accepts_empty_success_response() {
    let rsa_response = http_json_response(format!(
        r#"{{"RSAPublicKey":{}}}"#,
        serde_json::to_string(&test_public_key_pem()).unwrap()
    ));
    let reboot_response = http_empty_response();
    let host = spawn_http_server(vec![rsa_response, reboot_response]);

    let client = ZyxelClient::new(&RouterConfig {
        host: host.trim_start_matches("http://").to_string(),
        protocol: "http".to_string(),
        username: "admin".to_string(),
        password: "secret".to_string(),
    })
    .await
    .unwrap();

    client.reboot().await.unwrap();
}
