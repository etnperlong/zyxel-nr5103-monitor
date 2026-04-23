use rsa::{RsaPrivateKey, pkcs8::EncodePublicKey};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use zyxel_nr5103_monitor::{
    client::ZyxelClient,
    config::{MonitorConfig, RouterConfig},
    monitor::Monitor,
};

fn spawn_http_server(responses: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let request_log = Arc::clone(&requests);

    thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let bytes_read = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes_read]);

            if let Some(line) = request.lines().next() {
                request_log.lock().unwrap().push(line.to_string());
            }

            stream.write_all(response.as_bytes()).unwrap();
            stream.flush().unwrap();
        }
    });

    (format!("http://{addr}"), requests)
}

fn http_response(status_line: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status_line}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn http_empty_response(status_line: &str) -> String {
    format!("HTTP/1.1 {status_line}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
}

fn test_public_key_pem() -> String {
    RsaPrivateKey::new(&mut rand::thread_rng(), 2048)
        .unwrap()
        .to_public_key()
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .unwrap()
}

#[tokio::test]
async fn monitor_new_builds_with_router_client_and_config() {
    let rsa_response = http_response(
        "200 OK",
        "application/json",
        &format!(
            r#"{{"RSAPublicKey":{}}}"#,
            serde_json::to_string(&test_public_key_pem()).unwrap()
        ),
    );
    let (host, _) = spawn_http_server(vec![rsa_response]);

    let client = Arc::new(
        ZyxelClient::new(&RouterConfig {
            host,
            username: "admin".to_string(),
            password: "secret".to_string(),
        })
        .await
        .unwrap(),
    );

    let monitor = Monitor::new(
        client,
        MonitorConfig {
            interval: Duration::from_secs(60),
            url: "http://www.gstatic.com/generate_204".to_string(),
            timeout: Duration::from_secs(2),
            max_retries: 1,
            min_reboot_interval: Duration::from_secs(300),
        },
    );

    assert!(monitor.is_ok());
}

#[tokio::test]
async fn monitor_run_checks_connectivity_and_logs_out_on_sigint() {
    let rsa_response = http_response(
        "200 OK",
        "application/json",
        &format!(
            r#"{{"RSAPublicKey":{}}}"#,
            serde_json::to_string(&test_public_key_pem()).unwrap()
        ),
    );
    let connectivity_response = http_empty_response("204 No Content");
    let logout_response = http_response("200 OK", "text/plain", "OK");
    let (host, requests) = spawn_http_server(vec![rsa_response, connectivity_response, logout_response]);

    let client = Arc::new(
        ZyxelClient::new(&RouterConfig {
            host: host.clone(),
            username: "admin".to_string(),
            password: "secret".to_string(),
        })
        .await
        .unwrap(),
    );

    let monitor = Monitor::new(
        client,
        MonitorConfig {
            interval: Duration::from_secs(60),
            url: format!("{host}/generate_204"),
            timeout: Duration::from_secs(2),
            max_retries: 3,
            min_reboot_interval: Duration::from_secs(300),
        },
    )
    .unwrap();

    let monitor_task = tokio::spawn(async move { monitor.run().await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    let status = Command::new("kill")
        .args(["-INT", &std::process::id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());

    let run_result: anyhow::Result<()> = tokio::time::timeout(Duration::from_secs(5), monitor_task)
        .await
        .unwrap()
        .unwrap();
    assert!(run_result.is_ok());

    let recorded_requests = requests.lock().unwrap().clone();
    assert!(
        recorded_requests
            .iter()
            .any(|request| request.starts_with("GET /generate_204 "))
    );
    assert!(
        recorded_requests
            .iter()
            .any(|request| request.starts_with("POST /cgi-bin/UserLogout"))
    );
}
