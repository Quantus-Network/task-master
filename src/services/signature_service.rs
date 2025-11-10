use quantus_cli::qp_dilithium_crypto::{traits::verify as dilithium_verify, types::DilithiumPublic};
use sp_core::crypto::{AccountId32, Ss58Codec};
use std::convert::TryFrom;
use sp_runtime::traits::IdentifyAccount;
use tracing::info;

#[derive(Debug, thiserror::Error)]
pub enum SigServiceError {
    #[error("Invalid SS58 address: {0}")]
    InvalidAddress(String),
    #[error("Hex decode error: {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("Verification failed")]
    VerifyFailed,
}

pub type SigServiceResult<T> = Result<T, SigServiceError>;

pub struct SignatureService;

impl SignatureService {
    pub fn verify_message(message: &[u8], signature_hex: &str, public_key_hex: &str) -> SigServiceResult<bool> {
        let sig_hex = signature_hex.strip_prefix("0x").unwrap_or(signature_hex);
        let pk_hex = public_key_hex.strip_prefix("0x").unwrap_or(public_key_hex);
        let sig = hex::decode(sig_hex)?;
        let pk = hex::decode(pk_hex)?;
        let ok = dilithium_verify(&pk, message, &sig);
        info!(
            message_len = message.len(),
            signature_len = sig.len(),
            public_key_len = pk.len(),
            ok = ok,
            "SignatureService::verify_message"
        );
        Ok(ok)
    }

    pub fn verify_address(public_key_hex: &str, address_ss58: &str) -> SigServiceResult<bool> {
        let pk_hex = public_key_hex.strip_prefix("0x").unwrap_or(public_key_hex);
        let pk = hex::decode(pk_hex)?;
        let expected = AccountId32::from_ss58check(address_ss58)
            .map_err(|e| SigServiceError::InvalidAddress(format!("{:?}", e)))?;
        let dil = DilithiumPublic::try_from(pk.as_slice()).map_err(|_| SigServiceError::VerifyFailed)?;
        let derived = dil.into_account();
        let ok = derived == expected;
        info!(public_key_len = pk.len(), ok = ok, "SignatureService::verify_address");
        Ok(ok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_core::crypto::Ss58Codec;
    use quantus_cli::qp_dilithium_crypto::types::DilithiumPublic;
    use std::convert::TryFrom;

    // Smoke test: verifies API shape. For a full e2e, we'd integrate a signer to produce a real signature.
    #[test]
    fn verify_message_rejects_garbage() {
        let msg = b"hello world";
        let signature_hex = "00".repeat(16);
        let public_key_hex = "11".repeat(32);
        let ok = SignatureService::verify_message(msg, &signature_hex, &public_key_hex).unwrap();
        assert!(!ok);
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let msg = b"quantus-signature-test";
        let entropy = [7u8; 32];
        let hedge = [2u8; 32];
        let kp = qp_rusty_crystals_dilithium::ml_dsa_87::Keypair::generate(&entropy);
        let pk = kp.public.to_bytes();
        let sig = kp.sign(msg, None, Some(hedge));
        let pk_hex = hex::encode(pk);
        let sig_hex = hex::encode(sig);
        assert!(SignatureService::verify_message(msg, &sig_hex, &pk_hex).unwrap());

        let addr = DilithiumPublic::try_from(hex::decode(&pk_hex).unwrap().as_slice()).unwrap().into_account().to_ss58check();
        assert!(SignatureService::verify_address(&pk_hex, &addr).unwrap());
    }
}

