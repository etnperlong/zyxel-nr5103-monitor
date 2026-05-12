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
    config::{
        MonitorConfig, RebootConfig, RecoveryMethod, ReloadConfig, RouterConfig, SignalConfig,
    },
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

fn test_monitor_config(
    url: String,
    max_retries: u32,
    recovery_method: RecoveryMethod,
) -> MonitorConfig {
    MonitorConfig {
        interval: Duration::from_secs(60),
        url,
        timeout: Duration::from_secs(2),
        max_retries,
        recovery_method,
        reboot: RebootConfig {
            min_interval: Duration::from_secs(300),
            wait_after: Duration::from_millis(10),
        },
        reload: ReloadConfig {
            switch_wait: Duration::from_millis(10),
            restore_wait: Duration::from_millis(10),
        },
        signal: SignalConfig::default(),
    }
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
            host: host.trim_start_matches("http://").to_string(),
            protocol: "http".to_string(),
            username: "admin".to_string(),
            password: "secret".to_string(),
        })
        .await
        .unwrap(),
    );

    let monitor = Monitor::new(
        client,
        test_monitor_config(
            "http://www.gstatic.com/generate_204".to_string(),
            1,
            RecoveryMethod::Reload,
        ),
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
            host: host.trim_start_matches("http://").to_string(),
            protocol: "http".to_string(),
            username: "admin".to_string(),
            password: "secret".to_string(),
        })
        .await
        .unwrap(),
    );

    let monitor = Monitor::new(
        client,
        test_monitor_config(format!("{host}/generate_204"), 3, RecoveryMethod::Reload),
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
async fn metrics_disabled_does_not_poll_dal() {
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
            host: host.trim_start_matches("http://").to_string(),
            protocol: "http".to_string(),
            username: "admin".to_string(),
            password: "secret".to_string(),
        })
        .await
        .unwrap(),
    );

    let monitor = Monitor::new(
        client,
        test_monitor_config(format!("{host}/generate_204"), 3, RecoveryMethod::Reload),
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
            .all(|request| !request.contains("/cgi-bin/DAL?oid="))
    );
}

#[tokio::test]
async fn monitor_switches_access_technology_when_signal_monitor_requires_5g() {
    let private_key = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
    let public_key_pem = private_key
        .to_public_key()
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let request_log = Arc::clone(&requests);

    thread::spawn(move || {
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

        let (mut login_stream, _) = listener.accept().unwrap();
        let mut login_buffer = [0_u8; 8192];
        let login_read = login_stream.read(&mut login_buffer).unwrap();
        let login_request = String::from_utf8_lossy(&login_buffer[..login_read]).to_string();
        request_log.lock().unwrap().push(login_request.clone());
        let login_body = login_request.split("\r\n\r\n").nth(1).unwrap();
        let login_payload: Value = serde_json::from_str(login_body).unwrap();
        let encrypted_key = B64.decode(login_payload["key"].as_str().unwrap()).unwrap();
        let encoded_aes_key = private_key
            .decrypt(Pkcs1v15Encrypt, &encrypted_key)
            .unwrap();
        let aes_key_vec = B64.decode(encoded_aes_key).unwrap();
        let aes_key: [u8; 32] = aes_key_vec.try_into().unwrap();
        let login_response = http_response(
            "200 OK",
            "application/json",
            &encrypt_response_payload(&aes_key, r#"{"result":"ZCFG_SUCCESS","sessionkey":77}"#),
        );
        login_stream.write_all(login_response.as_bytes()).unwrap();
        login_stream.flush().unwrap();

        let (mut connectivity_stream, _) = listener.accept().unwrap();
        let mut connectivity_buffer = [0_u8; 4096];
        let connectivity_read = connectivity_stream.read(&mut connectivity_buffer).unwrap();
        let connectivity_request =
            String::from_utf8_lossy(&connectivity_buffer[..connectivity_read]).to_string();
        request_log.lock().unwrap().push(connectivity_request);
        let connectivity_response = http_empty_response("204 No Content");
        connectivity_stream
            .write_all(connectivity_response.as_bytes())
            .unwrap();
        connectivity_stream.flush().unwrap();

        let (mut signal_stream, _) = listener.accept().unwrap();
        let mut signal_buffer = [0_u8; 4096];
        let signal_read = signal_stream.read(&mut signal_buffer).unwrap();
        let signal_request = String::from_utf8_lossy(&signal_buffer[..signal_read]).to_string();
        request_log.lock().unwrap().push(signal_request);
        let signal_response = http_response(
            "200 OK",
            "application/json",
            r#"{"result":"ZCFG_SUCCESS","Object":[{"INTF_Current_Access_Technology":"LTE"}]}"#,
        );
        signal_stream.write_all(signal_response.as_bytes()).unwrap();
        signal_stream.flush().unwrap();

        let (mut auth_stream, _) = listener.accept().unwrap();
        let mut auth_buffer = [0_u8; 4096];
        let auth_read = auth_stream.read(&mut auth_buffer).unwrap();
        let auth_request = String::from_utf8_lossy(&auth_buffer[..auth_read]).to_string();
        request_log.lock().unwrap().push(auth_request);
        let auth_response = http_response("200 OK", "application/json", "{}");
        auth_stream.write_all(auth_response.as_bytes()).unwrap();
        auth_stream.flush().unwrap();

        let (mut dal_get_stream, _) = listener.accept().unwrap();
        let mut dal_get_buffer = [0_u8; 8192];
        let dal_get_read = dal_get_stream.read(&mut dal_get_buffer).unwrap();
        let dal_get_request = String::from_utf8_lossy(&dal_get_buffer[..dal_get_read]).to_string();
        request_log.lock().unwrap().push(dal_get_request);
        let dal_get_response = http_response(
            "200 OK",
            "application/json",
            r#"{"result":"ZCFG_SUCCESS","Object":[{"INTF_Supported_Access_Technologies":"Auto,NR5G-SA,NR5G-NSA,LTE","INTF_Preferred_Access_Technology":"Auto","INTF_Current_Access_Technology":"LTE","INTF_Supported_Bands":"B1,B3,n78","INTF_Preferred_Bands":"Auto","INTF_Current_Band":"B1"}]}"#,
        );
        dal_get_stream.write_all(dal_get_response.as_bytes()).unwrap();
        dal_get_stream.flush().unwrap();

        let (mut switch_stream, _) = listener.accept().unwrap();
        let mut switch_buffer = [0_u8; 8192];
        let switch_read = switch_stream.read(&mut switch_buffer).unwrap();
        let switch_request = String::from_utf8_lossy(&switch_buffer[..switch_read]).to_string();
        request_log.lock().unwrap().push(switch_request.clone());
        let switch_body = switch_request.split("\r\n\r\n").nth(1).unwrap();
        let switch_payload: Value = serde_json::from_str(switch_body).unwrap();
        let switch_decrypted = decrypt_request_payload(
            &aes_key,
            switch_payload["content"].as_str().unwrap(),
            switch_payload["iv"].as_str().unwrap(),
        );
        assert_eq!(
            switch_decrypted["INTF_Preferred_Access_Technology"],
            "NR5G-SA"
        );
        let switch_response = http_response(
            "200 OK",
            "application/json",
            &encrypt_response_payload(&aes_key, r#"{"result":"ZCFG_SUCCESS"}"#),
        );
        switch_stream.write_all(switch_response.as_bytes()).unwrap();
        switch_stream.flush().unwrap();

        let (mut restore_stream, _) = listener.accept().unwrap();
        let mut restore_buffer = [0_u8; 8192];
        let restore_read = restore_stream.read(&mut restore_buffer).unwrap();
        let restore_request = String::from_utf8_lossy(&restore_buffer[..restore_read]).to_string();
        request_log.lock().unwrap().push(restore_request.clone());
        let restore_body = restore_request.split("\r\n\r\n").nth(1).unwrap();
        let restore_payload: Value = serde_json::from_str(restore_body).unwrap();
        let restore_decrypted = decrypt_request_payload(
            &aes_key,
            restore_payload["content"].as_str().unwrap(),
            restore_payload["iv"].as_str().unwrap(),
        );
        assert_eq!(
            restore_decrypted["INTF_Preferred_Access_Technology"],
            "Auto"
        );
        let restore_response = http_response(
            "200 OK",
            "application/json",
            &encrypt_response_payload(&aes_key, r#"{"result":"ZCFG_SUCCESS"}"#),
        );
        restore_stream
            .write_all(restore_response.as_bytes())
            .unwrap();
        restore_stream.flush().unwrap();

        let (mut recovery_connectivity_stream, _) = listener.accept().unwrap();
        let mut recovery_connectivity_buffer = [0_u8; 4096];
        let recovery_connectivity_read = recovery_connectivity_stream
            .read(&mut recovery_connectivity_buffer)
            .unwrap();
        let recovery_connectivity_request =
            String::from_utf8_lossy(&recovery_connectivity_buffer[..recovery_connectivity_read])
                .to_string();
        request_log
            .lock()
            .unwrap()
            .push(recovery_connectivity_request);
        let recovery_connectivity_response = http_empty_response("204 No Content");
        recovery_connectivity_stream
            .write_all(recovery_connectivity_response.as_bytes())
            .unwrap();
        recovery_connectivity_stream.flush().unwrap();

        let (mut recovery_signal_stream, _) = listener.accept().unwrap();
        let mut recovery_signal_buffer = [0_u8; 4096];
        let recovery_signal_read = recovery_signal_stream.read(&mut recovery_signal_buffer).unwrap();
        let recovery_signal_request =
            String::from_utf8_lossy(&recovery_signal_buffer[..recovery_signal_read]).to_string();
        request_log.lock().unwrap().push(recovery_signal_request);
        let recovery_signal_response = http_response(
            "200 OK",
            "application/json",
            r#"{"result":"ZCFG_SUCCESS","Object":[{"INTF_Current_Access_Technology":"NR5G-NSA","NSA_RSRP":"-95"}]}"#,
        );
        recovery_signal_stream
            .write_all(recovery_signal_response.as_bytes())
            .unwrap();
        recovery_signal_stream.flush().unwrap();

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

    let client = Arc::new(
        ZyxelClient::new(&RouterConfig {
            host: addr.to_string(),
            protocol: "http".to_string(),
            username: "admin".to_string(),
            password: "secret".to_string(),
        })
        .await
        .unwrap(),
    );
    client.login().await.unwrap();

    let mut config = test_monitor_config(
        format!("http://{addr}/generate_204"),
        1,
        RecoveryMethod::Reload,
    );
    config.signal = SignalConfig {
        enabled: true,
        require_5g: true,
        min_5g_rsrp: -110.0,
        max_retries: 1,
    };

    let monitor = Monitor::new(Arc::clone(&client), config).unwrap();

    let monitor_task = tokio::spawn(async move { monitor.run().await });

    tokio::time::sleep(Duration::from_millis(250)).await;
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
            .any(|request| request.starts_with("GET /cgi-bin/DAL?oid=cellwan_status&sessionkey=77 "))
    );
    assert!(
        recorded_requests
            .iter()
            .any(|request| request.starts_with("PUT /cgi-bin/DAL?oid=cellwan_band&sessionkey=77 "))
    );
    assert!(
        recorded_requests
            .iter()
            .all(|request| !request.contains("/cgi-bin/Reboot"))
    );
}

#[tokio::test]
async fn monitor_reauthenticates_and_reboots_after_connectivity_failure_when_configured() {
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
            host: addr.to_string(),
            protocol: "http".to_string(),
            username: "admin".to_string(),
            password: "secret".to_string(),
        })
        .await
        .unwrap(),
    );

    let monitor = Monitor::new(
        Arc::clone(&client),
        test_monitor_config(format!("{host}/generate_204"), 1, RecoveryMethod::Reboot),
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
            .any(|request| request.starts_with("GET /cgi-bin/DAL?oid=status "))
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

#[tokio::test]
async fn monitor_switches_access_technology_and_skips_reboot_when_connectivity_recovers() {
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

        let login_body = login_request.split("\r\n\r\n").nth(1).unwrap();
        let login_payload: Value = serde_json::from_str(login_body).unwrap();
        let encrypted_key = B64.decode(login_payload["key"].as_str().unwrap()).unwrap();
        let encoded_aes_key = private_key
            .decrypt(Pkcs1v15Encrypt, &encrypted_key)
            .unwrap();
        let aes_key_vec = B64.decode(encoded_aes_key).unwrap();
        let aes_key: [u8; 32] = aes_key_vec.try_into().unwrap();
        let login_response = http_response(
            "200 OK",
            "application/json",
            &encrypt_response_payload(&aes_key, r#"{"result":"ZCFG_SUCCESS","sessionkey":77}"#),
        );
        login_stream.write_all(login_response.as_bytes()).unwrap();
        login_stream.flush().unwrap();

        let (mut dal_get_stream, _) = listener.accept().unwrap();
        let mut dal_get_buffer = [0_u8; 8192];
        let dal_get_read = dal_get_stream.read(&mut dal_get_buffer).unwrap();
        let dal_get_request = String::from_utf8_lossy(&dal_get_buffer[..dal_get_read]).to_string();
        request_log.lock().unwrap().push(dal_get_request);
        let dal_get_response = http_response(
            "200 OK",
            "application/json",
            r#"{"result":"ZCFG_SUCCESS","Object":[{"INTF_Supported_Access_Technologies":"Auto,NR5G-SA,NR5G-NSA,LTE","INTF_Preferred_Access_Technology":"Auto","INTF_Current_Access_Technology":"NR5G-NSA","INTF_Supported_Bands":"B1,B3,B5,B7,B8,B20,B28,B32,B38,B40,B41,B42,B43,n1,n3,n5,n7,n8,n20,n28,n38,n40,n41,n77,n78","INTF_Preferred_Bands":"Auto","INTF_Current_Band":"B1"}]}"#,
        );
        dal_get_stream
            .write_all(dal_get_response.as_bytes())
            .unwrap();
        dal_get_stream.flush().unwrap();

        let (mut switch_stream, _) = listener.accept().unwrap();
        let mut switch_buffer = [0_u8; 8192];
        let switch_read = switch_stream.read(&mut switch_buffer).unwrap();
        let switch_request = String::from_utf8_lossy(&switch_buffer[..switch_read]).to_string();
        request_log.lock().unwrap().push(switch_request.clone());
        let switch_body = switch_request.split("\r\n\r\n").nth(1).unwrap();
        let switch_payload: Value = serde_json::from_str(switch_body).unwrap();
        let switch_decrypted = decrypt_request_payload(
            &aes_key,
            switch_payload["content"].as_str().unwrap(),
            switch_payload["iv"].as_str().unwrap(),
        );
        assert_eq!(
            switch_decrypted["INTF_Preferred_Access_Technology"],
            "NR5G-SA"
        );
        let switch_response = http_response(
            "200 OK",
            "application/json",
            &encrypt_response_payload(&aes_key, r#"{"result":"ZCFG_SUCCESS"}"#),
        );
        switch_stream.write_all(switch_response.as_bytes()).unwrap();
        switch_stream.flush().unwrap();

        let (mut restore_stream, _) = listener.accept().unwrap();
        let mut restore_buffer = [0_u8; 8192];
        let restore_read = restore_stream.read(&mut restore_buffer).unwrap();
        let restore_request = String::from_utf8_lossy(&restore_buffer[..restore_read]).to_string();
        request_log.lock().unwrap().push(restore_request.clone());
        let restore_body = restore_request.split("\r\n\r\n").nth(1).unwrap();
        let restore_payload: Value = serde_json::from_str(restore_body).unwrap();
        let restore_decrypted = decrypt_request_payload(
            &aes_key,
            restore_payload["content"].as_str().unwrap(),
            restore_payload["iv"].as_str().unwrap(),
        );
        assert_eq!(
            restore_decrypted["INTF_Preferred_Access_Technology"],
            "Auto"
        );
        let restore_response = http_response(
            "200 OK",
            "application/json",
            &encrypt_response_payload(&aes_key, r#"{"result":"ZCFG_SUCCESS"}"#),
        );
        restore_stream
            .write_all(restore_response.as_bytes())
            .unwrap();
        restore_stream.flush().unwrap();

        let (mut recovery_connectivity_stream, _) = listener.accept().unwrap();
        let mut recovery_connectivity_buffer = [0_u8; 4096];
        let recovery_connectivity_read = recovery_connectivity_stream
            .read(&mut recovery_connectivity_buffer)
            .unwrap();
        let recovery_connectivity_request =
            String::from_utf8_lossy(&recovery_connectivity_buffer[..recovery_connectivity_read])
                .to_string();
        request_log
            .lock()
            .unwrap()
            .push(recovery_connectivity_request);
        let recovery_connectivity_response = http_empty_response("204 No Content");
        recovery_connectivity_stream
            .write_all(recovery_connectivity_response.as_bytes())
            .unwrap();
        recovery_connectivity_stream.flush().unwrap();

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
            host: addr.to_string(),
            protocol: "http".to_string(),
            username: "admin".to_string(),
            password: "secret".to_string(),
        })
        .await
        .unwrap(),
    );

    let monitor = Monitor::new(
        Arc::clone(&client),
        test_monitor_config(format!("{host}/generate_204"), 1, RecoveryMethod::Reload),
    )
    .unwrap();

    let monitor_task = tokio::spawn(async move { monitor.run().await });

    tokio::time::sleep(Duration::from_millis(200)).await;
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
            .any(|request| request.starts_with("GET /cgi-bin/DAL?oid=cellwan_band&sessionkey=77 "))
    );
    assert!(
        recorded_requests
            .iter()
            .any(|request| request.starts_with("PUT /cgi-bin/DAL?oid=cellwan_band&sessionkey=77 "))
    );
    assert!(
        recorded_requests
            .iter()
            .all(|request| !request.contains("/cgi-bin/Reboot"))
    );
}

#[tokio::test]
async fn monitor_reboots_when_access_technology_recovery_does_not_restore_connectivity() {
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

        let login_body = login_request.split("\r\n\r\n").nth(1).unwrap();
        let login_payload: Value = serde_json::from_str(login_body).unwrap();
        let encrypted_key = B64.decode(login_payload["key"].as_str().unwrap()).unwrap();
        let encoded_aes_key = private_key
            .decrypt(Pkcs1v15Encrypt, &encrypted_key)
            .unwrap();
        let aes_key_vec = B64.decode(encoded_aes_key).unwrap();
        let aes_key: [u8; 32] = aes_key_vec.try_into().unwrap();
        let login_response = http_response(
            "200 OK",
            "application/json",
            &encrypt_response_payload(&aes_key, r#"{"result":"ZCFG_SUCCESS","sessionkey":77}"#),
        );
        login_stream.write_all(login_response.as_bytes()).unwrap();
        login_stream.flush().unwrap();

        let (mut dal_get_stream, _) = listener.accept().unwrap();
        let mut dal_get_buffer = [0_u8; 8192];
        let dal_get_read = dal_get_stream.read(&mut dal_get_buffer).unwrap();
        let dal_get_request = String::from_utf8_lossy(&dal_get_buffer[..dal_get_read]).to_string();
        request_log.lock().unwrap().push(dal_get_request);
        let dal_get_response = http_response(
            "200 OK",
            "application/json",
            r#"{"result":"ZCFG_SUCCESS","Object":[{"INTF_Supported_Access_Technologies":"Auto,NR5G-SA,NR5G-NSA,LTE","INTF_Preferred_Access_Technology":"Auto","INTF_Current_Access_Technology":"NR5G-NSA","INTF_Supported_Bands":"B1,B3,B5,B7,B8,B20,B28,B32,B38,B40,B41,B42,B43,n1,n3,n5,n7,n8,n20,n28,n38,n40,n41,n77,n78","INTF_Preferred_Bands":"Auto","INTF_Current_Band":"B1"}]}"#,
        );
        dal_get_stream
            .write_all(dal_get_response.as_bytes())
            .unwrap();
        dal_get_stream.flush().unwrap();

        let (mut switch_stream, _) = listener.accept().unwrap();
        let mut switch_buffer = [0_u8; 8192];
        let switch_read = switch_stream.read(&mut switch_buffer).unwrap();
        let switch_request = String::from_utf8_lossy(&switch_buffer[..switch_read]).to_string();
        request_log.lock().unwrap().push(switch_request.clone());
        let switch_body = switch_request.split("\r\n\r\n").nth(1).unwrap();
        let switch_payload: Value = serde_json::from_str(switch_body).unwrap();
        let switch_decrypted = decrypt_request_payload(
            &aes_key,
            switch_payload["content"].as_str().unwrap(),
            switch_payload["iv"].as_str().unwrap(),
        );
        assert_eq!(
            switch_decrypted["INTF_Preferred_Access_Technology"],
            "NR5G-SA"
        );
        let switch_response = http_response(
            "200 OK",
            "application/json",
            &encrypt_response_payload(&aes_key, r#"{"result":"ZCFG_SUCCESS"}"#),
        );
        switch_stream.write_all(switch_response.as_bytes()).unwrap();
        switch_stream.flush().unwrap();

        let (mut restore_stream, _) = listener.accept().unwrap();
        let mut restore_buffer = [0_u8; 8192];
        let restore_read = restore_stream.read(&mut restore_buffer).unwrap();
        let restore_request = String::from_utf8_lossy(&restore_buffer[..restore_read]).to_string();
        request_log.lock().unwrap().push(restore_request.clone());
        let restore_body = restore_request.split("\r\n\r\n").nth(1).unwrap();
        let restore_payload: Value = serde_json::from_str(restore_body).unwrap();
        let restore_decrypted = decrypt_request_payload(
            &aes_key,
            restore_payload["content"].as_str().unwrap(),
            restore_payload["iv"].as_str().unwrap(),
        );
        assert_eq!(
            restore_decrypted["INTF_Preferred_Access_Technology"],
            "Auto"
        );
        let restore_response = http_response(
            "200 OK",
            "application/json",
            &encrypt_response_payload(&aes_key, r#"{"result":"ZCFG_SUCCESS"}"#),
        );
        restore_stream
            .write_all(restore_response.as_bytes())
            .unwrap();
        restore_stream.flush().unwrap();

        let (mut recovery_connectivity_stream, _) = listener.accept().unwrap();
        let mut recovery_connectivity_buffer = [0_u8; 4096];
        let recovery_connectivity_read = recovery_connectivity_stream
            .read(&mut recovery_connectivity_buffer)
            .unwrap();
        let recovery_connectivity_request =
            String::from_utf8_lossy(&recovery_connectivity_buffer[..recovery_connectivity_read])
                .to_string();
        request_log
            .lock()
            .unwrap()
            .push(recovery_connectivity_request);
        let recovery_connectivity_response = http_empty_response("500 Internal Server Error");
        recovery_connectivity_stream
            .write_all(recovery_connectivity_response.as_bytes())
            .unwrap();
        recovery_connectivity_stream.flush().unwrap();

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
            host: addr.to_string(),
            protocol: "http".to_string(),
            username: "admin".to_string(),
            password: "secret".to_string(),
        })
        .await
        .unwrap(),
    );

    let monitor = Monitor::new(
        Arc::clone(&client),
        test_monitor_config(format!("{host}/generate_204"), 1, RecoveryMethod::Reload),
    )
    .unwrap();

    let monitor_task = tokio::spawn(async move { monitor.run().await });

    tokio::time::sleep(Duration::from_millis(200)).await;
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
            .any(|request| request.starts_with("GET /cgi-bin/DAL?oid=cellwan_band&sessionkey=77 "))
    );
    assert!(
        recorded_requests
            .iter()
            .any(|request| request.contains("PUT /cgi-bin/DAL?oid=cellwan_band&sessionkey=77 "))
    );
    assert!(
        recorded_requests
            .iter()
            .any(|request| request.contains("POST /cgi-bin/Reboot?sessionkey=77 "))
    );
}
