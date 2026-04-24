use aes::Aes256;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use cbc::{Decryptor, Encryptor};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, pkcs8::EncodePublicKey};
use serde_json::Value;
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

type Aes256CbcEnc = Encryptor<Aes256>;
type Aes256CbcDec = Decryptor<Aes256>;

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

fn decrypt_request_payload(aes_key: &[u8; 32], content: &str, iv: &str) -> Value {
    let ciphertext = B64.decode(content).unwrap();
    let iv = B64.decode(iv).unwrap();
    let iv_prefix: [u8; 16] = iv[..16].try_into().unwrap();
    let plaintext = Aes256CbcDec::new(&(*aes_key).into(), &iv_prefix.into())
        .decrypt_padded_vec_mut::<Pkcs7>(&ciphertext)
        .unwrap();

    serde_json::from_slice(&plaintext).unwrap()
}

fn encrypt_response_payload(aes_key: &[u8; 32], payload: &str) -> String {
    let iv32 = [9_u8; 32];
    let iv_prefix: [u8; 16] = iv32[..16].try_into().unwrap();
    let ciphertext = Aes256CbcEnc::new(&(*aes_key).into(), &iv_prefix.into())
        .encrypt_padded_vec_mut::<Pkcs7>(payload.as_bytes());

    serde_json::json!({
        "content": B64.encode(ciphertext),
        "iv": B64.encode(iv32),
    })
    .to_string()
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
    let (host, requests) =
        spawn_http_server(vec![rsa_response, connectivity_response, logout_response]);

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

#[tokio::test]
async fn monitor_reauthenticates_and_reboots_after_connectivity_failure() {
    let private_key = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
    let public_key_pem = private_key
        .to_public_key()
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let request_log = Arc::clone(&requests);

    let server = thread::spawn(move || {
        let (mut rsa_stream, _) = listener.accept().unwrap();
        let mut rsa_buffer = [0_u8; 4096];
        let rsa_read = rsa_stream.read(&mut rsa_buffer).unwrap();
        let rsa_request = String::from_utf8_lossy(&rsa_buffer[..rsa_read]).to_string();
        request_log.lock().unwrap().push(rsa_request);
        let rsa_response = http_response(
            "200 OK",
            "application/json",
            &format!(
                r#"{{"RSAPublicKey":{}}}"#,
                serde_json::to_string(&public_key_pem).unwrap()
            ),
        );
        rsa_stream.write_all(rsa_response.as_bytes()).unwrap();
        rsa_stream.flush().unwrap();

        let (mut connectivity_stream, _) = listener.accept().unwrap();
        let mut connectivity_buffer = [0_u8; 4096];
        let connectivity_read = connectivity_stream.read(&mut connectivity_buffer).unwrap();
        let connectivity_request =
            String::from_utf8_lossy(&connectivity_buffer[..connectivity_read]).to_string();
        request_log.lock().unwrap().push(connectivity_request);
        let connectivity_response = http_empty_response("500 Internal Server Error");
        connectivity_stream
            .write_all(connectivity_response.as_bytes())
            .unwrap();
        connectivity_stream.flush().unwrap();

        let (mut auth_check_stream, _) = listener.accept().unwrap();
        let mut auth_check_buffer = [0_u8; 4096];
        let auth_check_read = auth_check_stream.read(&mut auth_check_buffer).unwrap();
        let auth_check_request =
            String::from_utf8_lossy(&auth_check_buffer[..auth_check_read]).to_string();
        request_log.lock().unwrap().push(auth_check_request);
        let auth_check_response = http_empty_response("500 Internal Server Error");
        auth_check_stream
            .write_all(auth_check_response.as_bytes())
            .unwrap();
        auth_check_stream.flush().unwrap();

        let (mut logout_stream, _) = listener.accept().unwrap();
        let mut logout_buffer = [0_u8; 4096];
        let logout_read = logout_stream.read(&mut logout_buffer).unwrap();
        let logout_request = String::from_utf8_lossy(&logout_buffer[..logout_read]).to_string();
        request_log.lock().unwrap().push(logout_request);
        let logout_response = http_response("200 OK", "text/plain", "OK");
        logout_stream.write_all(logout_response.as_bytes()).unwrap();
        logout_stream.flush().unwrap();

        let (mut login_stream, _) = listener.accept().unwrap();
        let mut login_buffer = [0_u8; 8192];
        let login_read = login_stream.read(&mut login_buffer).unwrap();
        let login_request = String::from_utf8_lossy(&login_buffer[..login_read]).to_string();
        request_log.lock().unwrap().push(login_request.clone());

        assert!(
            login_request
                .to_ascii_lowercase()
                .contains("content-type: application/x-www-form-urlencoded")
        );
        let login_body = login_request.split("\r\n\r\n").nth(1).unwrap();
        let payload: Value = serde_json::from_str(login_body).unwrap();
        let encrypted_key = B64.decode(payload["key"].as_str().unwrap()).unwrap();
        let encoded_aes_key = private_key
            .decrypt(Pkcs1v15Encrypt, &encrypted_key)
            .unwrap();
        let aes_key_vec = B64.decode(encoded_aes_key).unwrap();
        let aes_key: [u8; 32] = aes_key_vec.try_into().unwrap();
        let decrypted_request = decrypt_request_payload(
            &aes_key,
            payload["content"].as_str().unwrap(),
            payload["iv"].as_str().unwrap(),
        );
        assert_eq!(decrypted_request["Input_Account"], "admin");

        let login_response = http_response(
            "200 OK",
            "application/json",
            &encrypt_response_payload(&aes_key, r#"{"result":"ZCFG_SUCCESS","sessionkey":77}"#),
        );
        login_stream.write_all(login_response.as_bytes()).unwrap();
        login_stream.flush().unwrap();

        let (mut reboot_stream, _) = listener.accept().unwrap();
        let mut reboot_buffer = [0_u8; 4096];
        let reboot_read = reboot_stream.read(&mut reboot_buffer).unwrap();
        let reboot_request = String::from_utf8_lossy(&reboot_buffer[..reboot_read]).to_string();
        request_log.lock().unwrap().push(reboot_request);
        let reboot_response = http_empty_response("200 OK");
        reboot_stream.write_all(reboot_response.as_bytes()).unwrap();
        reboot_stream.flush().unwrap();

        let (mut final_logout_stream, _) = listener.accept().unwrap();
        let mut final_logout_buffer = [0_u8; 4096];
        let final_logout_read = final_logout_stream.read(&mut final_logout_buffer).unwrap();
        let final_logout_request =
            String::from_utf8_lossy(&final_logout_buffer[..final_logout_read]).to_string();
        request_log.lock().unwrap().push(final_logout_request);
        let final_logout_response = http_response("200 OK", "text/plain", "OK");
        final_logout_stream
            .write_all(final_logout_response.as_bytes())
            .unwrap();
        final_logout_stream.flush().unwrap();
    });

    let host = format!("http://{addr}");
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
        Arc::clone(&client),
        MonitorConfig {
            interval: Duration::from_secs(60),
            url: format!("{host}/generate_204"),
            timeout: Duration::from_secs(2),
            max_retries: 1,
            min_reboot_interval: Duration::from_secs(300),
        },
    )
    .unwrap();

    let monitor_task = tokio::spawn(async move { monitor.run().await });

    tokio::time::sleep(Duration::from_millis(100)).await;
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

    server.join().unwrap();

    let recorded_requests = requests.lock().unwrap().clone();
    assert!(
        recorded_requests
            .iter()
            .any(|request| request.starts_with("GET /generate_204 "))
    );
    assert!(
        recorded_requests
            .iter()
            .any(|request| request.starts_with("GET /getBasicInformation "))
    );
    assert!(
        recorded_requests
            .iter()
            .any(|request| request.starts_with("POST /cgi-bin/UserLogout"))
    );
    assert!(
        recorded_requests
            .iter()
            .any(|request| request.starts_with("POST /UserLogin "))
    );
    assert!(
        recorded_requests
            .iter()
            .any(|request| request.contains("POST /cgi-bin/Reboot?sessionkey=77 "))
    );
}
