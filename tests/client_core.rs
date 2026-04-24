use aes::Aes256;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use cbc::{Decryptor, Encryptor};
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, pkcs8::EncodePublicKey};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use zyxel_nr5103_monitor::{
    client::{ApiEndpoint, ZyxelClient},
    config::RouterConfig,
};

type Aes256CbcEnc = Encryptor<Aes256>;
type Aes256CbcDec = Decryptor<Aes256>;

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
    let iv32 = [7_u8; 32];
    let iv_prefix: [u8; 16] = iv32[..16].try_into().unwrap();
    let ciphertext = Aes256CbcEnc::new(&(*aes_key).into(), &iv_prefix.into())
        .encrypt_padded_vec_mut::<Pkcs7>(payload.as_bytes());

    serde_json::json!({
        "content": B64.encode(ciphertext),
        "iv": B64.encode(iv32),
    })
    .to_string()
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
        protocol: "https".to_string(),
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
        host: host.trim_start_matches("http://").to_string(),
        protocol: "http".to_string(),
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

#[tokio::test]
async fn execute_uses_form_content_type_and_handles_encrypted_http_flow() {
    let private_key = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap();
    let public_key_pem = private_key
        .to_public_key()
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .unwrap();
    let rsa_response = http_json_response(format!(
        r#"{{"RSAPublicKey":{}}}"#,
        serde_json::to_string(&public_key_pem).unwrap()
    ));

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let server = thread::spawn(move || {
        let (mut rsa_stream, _) = listener.accept().unwrap();
        let mut rsa_buffer = [0_u8; 2048];
        let _ = rsa_stream.read(&mut rsa_buffer).unwrap();
        rsa_stream.write_all(rsa_response.as_bytes()).unwrap();
        rsa_stream.flush().unwrap();

        let (mut api_stream, _) = listener.accept().unwrap();
        let mut api_buffer = [0_u8; 8192];
        let read = api_stream.read(&mut api_buffer).unwrap();
        let request = String::from_utf8(api_buffer[..read].to_vec()).unwrap();

        assert!(
            request
                .to_ascii_lowercase()
                .contains("content-type: application/x-www-form-urlencoded")
        );

        let body = request.split("\r\n\r\n").nth(1).unwrap();
        let payload: Value = serde_json::from_str(body).unwrap();
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
        assert_eq!(decrypted_request, serde_json::json!({"hello": "world"}));

        let encrypted_response =
            encrypt_response_payload(&aes_key, r#"{"sessionkey":99,"ok":true}"#);
        let response = http_json_response(encrypted_response);
        api_stream.write_all(response.as_bytes()).unwrap();
        api_stream.flush().unwrap();
    });

    let client = ZyxelClient::new(&RouterConfig {
        host: addr.to_string(),
        protocol: "http".to_string(),
        username: "admin".to_string(),
        password: "secret".to_string(),
    })
    .await
    .unwrap();

    let response = client
        .execute::<Value, Value>(
            &ApiEndpoint {
                path: "/UserLogin",
                method: "POST",
                requires_auth: false,
                encrypt_request: true,
                include_aes_key: true,
            },
            Some(&serde_json::json!({"hello": "world"})),
        )
        .await
        .unwrap()
        .unwrap();

    server.join().unwrap();

    assert_eq!(response["ok"], true);
    assert_eq!(client.session_key(), 99);
}
