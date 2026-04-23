use aes::Aes256;
use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use cbc::{Decryptor, Encryptor};
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use rand::RngCore;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey, pkcs8::DecodePublicKey};
use serde::{Deserialize, Serialize};

type Aes256CbcEnc = Encryptor<Aes256>;
type Aes256CbcDec = Decryptor<Aes256>;

#[derive(Debug, Serialize)]
pub struct EncryptedRequest {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub iv: String,
}

#[derive(Debug, Deserialize)]
pub struct EncryptedResponse {
    pub content: String,
    pub iv: String,
}

pub struct CryptoState {
    rsa_public_key: RsaPublicKey,
    aes_key: [u8; 32],
}

impl CryptoState {
    pub fn new(pem: &str) -> Result<Self> {
        let rsa_public_key =
            RsaPublicKey::from_public_key_pem(pem).context("Failed to parse RSA public key")?;
        let mut aes_key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut aes_key);

        Ok(Self {
            rsa_public_key,
            aes_key,
        })
    }

    pub fn encrypt_json(
        &self,
        payload: &impl Serialize,
        include_key: bool,
    ) -> Result<EncryptedRequest> {
        let json_bytes = serde_json::to_vec(payload).context("JSON serialization failed")?;

        let mut iv32 = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut iv32);
        let iv16: [u8; 16] = iv32[..16]
            .try_into()
            .map_err(|_| anyhow!("IV prefix length mismatch"))?;

        let ciphertext = Aes256CbcEnc::new(&self.aes_key.into(), &iv16.into())
            .encrypt_padded_vec_mut::<Pkcs7>(&json_bytes);

        let key = if include_key {
            let b64_aes = B64.encode(self.aes_key);
            let encrypted_key = self
                .rsa_public_key
                .encrypt(&mut rand::thread_rng(), Pkcs1v15Encrypt, b64_aes.as_bytes())
                .context("RSA encryption failed")?;

            Some(B64.encode(encrypted_key))
        } else {
            None
        };

        Ok(EncryptedRequest {
            content: B64.encode(ciphertext),
            key,
            iv: B64.encode(iv32),
        })
    }

    pub fn decrypt_response(&self, resp: &EncryptedResponse) -> Result<Vec<u8>> {
        let ciphertext = B64
            .decode(&resp.content)
            .context("Base64 decode content failed")?;
        let iv32 = B64.decode(&resp.iv).context("Base64 decode IV failed")?;

        if iv32.len() < 16 {
            bail!("IV too short");
        }

        let iv16: [u8; 16] = iv32[..16]
            .try_into()
            .map_err(|_| anyhow!("IV prefix length mismatch"))?;

        Aes256CbcDec::new(&self.aes_key.into(), &iv16.into())
            .decrypt_padded_vec_mut::<Pkcs7>(&ciphertext)
            .map_err(|error| anyhow!("AES decryption failed: {error:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::{CryptoState, EncryptedResponse};
    use base64::{Engine, engine::general_purpose::STANDARD as B64};
    use rsa::{Pkcs1v15Encrypt, RsaPrivateKey, pkcs8::EncodePublicKey};
    use serde::Serialize;

    #[derive(Serialize)]
    struct Payload {
        username: &'static str,
        password: &'static str,
    }

    fn test_keypair() -> RsaPrivateKey {
        RsaPrivateKey::new(&mut rand::thread_rng(), 2048).unwrap()
    }

    fn test_public_key_pem(private_key: &RsaPrivateKey) -> String {
        private_key
            .to_public_key()
            .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
            .unwrap()
    }

    #[test]
    fn encrypt_json_round_trips_through_decrypt_response() {
        let private_key = test_keypair();
        let crypto = CryptoState::new(&test_public_key_pem(&private_key)).unwrap();
        let request = crypto
            .encrypt_json(
                &Payload {
                    username: "admin",
                    password: "secret",
                },
                false,
            )
            .unwrap();

        let plaintext = crypto
            .decrypt_response(&EncryptedResponse {
                content: request.content,
                iv: request.iv,
            })
            .unwrap();

        assert_eq!(plaintext, br#"{"username":"admin","password":"secret"}"#);
    }

    #[test]
    fn encrypt_json_includes_rsa_encrypted_aes_key_when_requested() {
        let private_key = test_keypair();
        let crypto = CryptoState::new(&test_public_key_pem(&private_key)).unwrap();
        let request = crypto
            .encrypt_json(
                &Payload {
                    username: "admin",
                    password: "secret",
                },
                true,
            )
            .unwrap();

        let encrypted_key = B64.decode(request.key.unwrap()).unwrap();
        let decrypted_key = private_key
            .decrypt(Pkcs1v15Encrypt, &encrypted_key)
            .unwrap();

        assert_eq!(B64.decode(decrypted_key).unwrap().len(), 32);
    }

    #[test]
    fn decrypt_response_rejects_short_iv() {
        let private_key = test_keypair();
        let crypto = CryptoState::new(&test_public_key_pem(&private_key)).unwrap();
        let error = crypto
            .decrypt_response(&EncryptedResponse {
                content: B64.encode([]),
                iv: B64.encode([1_u8, 2, 3]),
            })
            .unwrap_err();

        assert_eq!(error.to_string(), "IV too short");
    }
}
