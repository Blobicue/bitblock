//! Argon2id + AES-256-GCM encrypted validator keystore (`DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY`).
//!
//! Stores role-separated Ed25519 proposal seed, consensus vote seed, libp2p transport seed,
//! and optional `lab == true` testnet faucet grant relayer seed. Zero BLS12-381 or DKG material.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::Argon2;
use bitblock_research::crypto_d1::CanonicalEd25519PublicKeyV1;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroize;

#[derive(Debug, Error)]
pub enum KeystoreError {
    #[error("keystore encryption or decryption failed")]
    CryptoFailed,
    #[error("invalid hex in validator keystore")]
    BadHex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedBitblockKeystore {
    pub version: u8,
    pub cipher: String,
    pub kdf: String,
    pub custody_label: String,
    pub validator_id_hex: String,
    pub proposal_pubkey_hex: String,
    pub vote_pubkey_hex: String,
    pub salt_hex: String,
    pub nonce_hex: String,
    pub ciphertext_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorSecretMaterial {
    pub validator_id_hex: String,
    pub proposal_seed_hex: String,
    pub vote_seed_hex: String,
    pub p2p_seed_hex: String,
    #[serde(default)]
    pub faucet_relayer_seed_hex: Option<String>,
}

impl Drop for ValidatorSecretMaterial {
    fn drop(&mut self) {
        self.proposal_seed_hex.zeroize();
        self.vote_seed_hex.zeroize();
        self.p2p_seed_hex.zeroize();
        if let Some(ref mut s) = self.faucet_relayer_seed_hex {
            s.zeroize();
        }
    }
}

pub fn decode_seed_32(hex_str: &str) -> Result<[u8; 32], KeystoreError> {
    let raw = hex::decode(hex_str.trim()).map_err(|_| KeystoreError::BadHex)?;
    raw.try_into().map_err(|_| KeystoreError::BadHex)
}

pub fn encrypt_validator_keystore(
    mat: &ValidatorSecretMaterial,
    password: &str,
) -> Result<EncryptedBitblockKeystore, KeystoreError> {
    let prop_seed = decode_seed_32(&mat.proposal_seed_hex)?;
    let vote_seed = decode_seed_32(&mat.vote_seed_hex)?;
    let prop_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&prop_seed)
        .map_err(|_| KeystoreError::CryptoFailed)?;
    let vote_pk = CanonicalEd25519PublicKeyV1::from_secret_seed(&vote_seed)
        .map_err(|_| KeystoreError::CryptoFailed)?;
    if prop_pk == vote_pk {
        return Err(KeystoreError::CryptoFailed);
    }

    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);

    let mut enc_key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), &salt, &mut enc_key)
        .map_err(|_| KeystoreError::CryptoFailed)?;
    let cipher = Aes256Gcm::new_from_slice(&enc_key).map_err(|_| KeystoreError::CryptoFailed)?;
    let mut plain = serde_json::to_vec(mat).map_err(|_| KeystoreError::CryptoFailed)?;
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plain.as_ref())
        .map_err(|_| KeystoreError::CryptoFailed)?;
    plain.zeroize();
    enc_key.zeroize();

    Ok(EncryptedBitblockKeystore {
        version: 1,
        cipher: "aes-256-gcm".into(),
        kdf: "argon2id".into(),
        custody_label: "DEVNET-TEST-KEYS-NOT-PRODUCTION-CUSTODY".into(),
        validator_id_hex: mat.validator_id_hex.clone(),
        proposal_pubkey_hex: hex::encode(prop_pk.as_bytes()),
        vote_pubkey_hex: hex::encode(vote_pk.as_bytes()),
        salt_hex: hex::encode(salt),
        nonce_hex: hex::encode(nonce_bytes),
        ciphertext_hex: hex::encode(ct),
    })
}

pub fn decrypt_validator_keystore(
    ks: &EncryptedBitblockKeystore,
    password: &str,
) -> Result<ValidatorSecretMaterial, KeystoreError> {
    let salt = hex::decode(&ks.salt_hex).map_err(|_| KeystoreError::BadHex)?;
    let nonce = hex::decode(&ks.nonce_hex).map_err(|_| KeystoreError::BadHex)?;
    let ct = hex::decode(&ks.ciphertext_hex).map_err(|_| KeystoreError::BadHex)?;
    if nonce.len() != 12 {
        return Err(KeystoreError::CryptoFailed);
    }
    let mut enc_key = [0u8; 32];
    Argon2::default()
        .hash_password_into(password.as_bytes(), &salt, &mut enc_key)
        .map_err(|_| KeystoreError::CryptoFailed)?;
    let cipher = Aes256Gcm::new_from_slice(&enc_key).map_err(|_| KeystoreError::CryptoFailed)?;
    let mut pt = cipher
        .decrypt(Nonce::from_slice(&nonce), ct.as_ref())
        .map_err(|_| KeystoreError::CryptoFailed)?;
    enc_key.zeroize();
    let mat: ValidatorSecretMaterial =
        serde_json::from_slice(&pt).map_err(|_| KeystoreError::CryptoFailed)?;
    pt.zeroize();
    let prop_pk =
        CanonicalEd25519PublicKeyV1::from_secret_seed(&decode_seed_32(&mat.proposal_seed_hex)?)
            .map_err(|_| KeystoreError::CryptoFailed)?;
    let vote_pk =
        CanonicalEd25519PublicKeyV1::from_secret_seed(&decode_seed_32(&mat.vote_seed_hex)?)
            .map_err(|_| KeystoreError::CryptoFailed)?;
    if hex::encode(prop_pk.as_bytes()) != ks.proposal_pubkey_hex
        || hex::encode(vote_pk.as_bytes()) != ks.vote_pubkey_hex
    {
        return Err(KeystoreError::CryptoFailed);
    }
    Ok(mat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validator_keystore_roundtrip_and_wrong_password_fails() {
        let mat = ValidatorSecretMaterial {
            validator_id_hex: hex::encode([0x11; 32]),
            proposal_seed_hex: hex::encode([0x22; 32]),
            vote_seed_hex: hex::encode([0x33; 32]),
            p2p_seed_hex: hex::encode([0x44; 32]),
            faucet_relayer_seed_hex: Some(hex::encode([0x55; 32])),
        };
        let enc = encrypt_validator_keystore(&mat, "test-pass-123").expect("encrypt");
        let dec = decrypt_validator_keystore(&enc, "test-pass-123").expect("decrypt");
        assert_eq!(dec.proposal_seed_hex, mat.proposal_seed_hex);
        assert_eq!(dec.vote_seed_hex, mat.vote_seed_hex);
        assert!(decrypt_validator_keystore(&enc, "wrong-pass").is_err());
    }
}
