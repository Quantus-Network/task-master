use quantus_cli::qp_dilithium_crypto::{traits::verify, types::DilithiumPublic};
use serde::{Deserialize, Serialize};
use sp_core::{
    crypto::{AccountId32, Ss58Codec},
    ByteArray,
};
use sp_runtime::traits::IdentifyAccount;

#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("Invalid signature format: {0}")]
    InvalidSignature(String),
    #[error("Signature verification failed")]
    VerificationFailed,
    #[error("Invalid Ethereum address format: {0}")]
    InvalidAddress(String),
    #[error("Invalid Quantus address format: {0}")]
    InvalidQuantusAddress(String),
    #[error("Hex decode error: {0}")]
    HexDecode(#[from] hex::FromHexError),
    #[error("SS58 decode error: {0}")]
    Ss58Decode(String),
}

pub type SignatureResult<T> = Result<T, SignatureError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthAddressAssociation {
    pub quan_address: String,
    pub eth_address: String,
    pub signature: String,
    pub public_key: String,
}

/// Verify that a Dilithium signature was created by signing the eth_address with the private key
/// corresponding to the provided public key, and that the public key derives the quan_address
pub fn verify_dilithium_signature(
    quan_address: &str,
    eth_address: &str,
    signature_hex: &str,
    public_key_hex: &str,
) -> SignatureResult<bool> {
    // Remove 0x prefix from eth_address if present
    let eth_address = eth_address.strip_prefix("0x").unwrap_or(eth_address);

    // Validate eth_address format (should be 40 hex characters)
    if eth_address.len() != 40 {
        return Err(SignatureError::InvalidAddress(format!(
            "Ethereum address must be 40 hex characters, got {}",
            eth_address.len()
        )));
    }

    // Validate quan_address format (should be a valid SS58 address starting with 'qz')
    if !quan_address.starts_with("qz") {
        return Err(SignatureError::InvalidQuantusAddress(format!(
            "Quantus address must start with 'qz', got: {}",
            quan_address
        )));
    }

    // Decode the public key from hex
    let public_key_hex = public_key_hex.strip_prefix("0x").unwrap_or(public_key_hex);
    let public_key_bytes = hex::decode(public_key_hex)?;

    // Verify that the public key corresponds to the quan_address
    let expected_account_id = AccountId32::from_ss58check(quan_address)
        .map_err(|e| SignatureError::Ss58Decode(format!("Invalid SS58 address: {:?}", e)))?;

    // Create AccountId32 from the provided public key using DilithiumPublic
    let dilithium_public = DilithiumPublic::from_slice(&public_key_bytes).map_err(|_| {
        SignatureError::InvalidSignature("Invalid Dilithium public key format".to_string())
    })?;
    let derived_account_id = dilithium_public.into_account();

    if derived_account_id != expected_account_id {
        return Err(SignatureError::VerificationFailed);
    }

    // Remove 0x prefix from signature if present
    let signature_hex = signature_hex.strip_prefix("0x").unwrap_or(signature_hex);

    // Decode the signature from hex
    let signature_bytes = hex::decode(signature_hex)?;

    // The message that was signed is the eth_address (without 0x prefix)
    let message = eth_address.as_bytes();

    // Verify the Dilithium signature
    let is_valid = verify(&public_key_bytes, message, &signature_bytes);

    if is_valid {
        Ok(true)
    } else {
        Err(SignatureError::VerificationFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_signature_format() {
        let result = verify_dilithium_signature(
            "qz5CiMhML4GNdPP3ZFdTGZqQyU7hcU8aKJPXqQq8RgHq1b6a",
            "1234567890123456789012345678901234567890",
            "invalid_signature",
            "0102030405060708",
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SignatureError::HexDecode(_)));
    }

    #[test]
    fn test_invalid_quan_address_format() {
        let result = verify_dilithium_signature(
            "invalid_address",
            "1234567890123456789012345678901234567890",
            &"0".repeat(8704), // Dilithium signature is much longer than ECDSA
            &"0".repeat(64),   // 32 bytes public key
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SignatureError::InvalidQuantusAddress(_)
        ));
    }

    #[test]
    fn test_invalid_eth_address_format() {
        let result = verify_dilithium_signature(
            "qz5CiMhML4GNdPP3ZFdTGZqQyU7hcU8aKJPXqQq8RgHq1b6a",
            "invalid_address",
            &"0".repeat(8704),
            &"0".repeat(64),
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SignatureError::InvalidAddress(_)
        ));
    }

    #[test]
    fn test_hex_prefix_handling() {
        let quan_address = "qz5CiMhML4GNdPP3ZFdTGZqQyU7hcU8aKJPXqQq8RgHq1b6a";
        let eth_address = "1234567890123456789012345678901234567890";
        let signature = "0x" + &"0".repeat(8704);

        let pubkey = &"0".repeat(64);

        // Should work with 0x prefix on eth_address
        let result1 = verify_dilithium_signature(
            quan_address,
            &("0x".to_string() + eth_address),
            signature,
            pubkey,
        );

        // Should work without 0x prefix on eth_address
        let result2 = verify_dilithium_signature(quan_address, eth_address, signature, pubkey);

        // Both should have the same error type (verification failed since we're using dummy data)
        assert!(result1.is_err());
        assert!(result2.is_err());
        assert!(matches!(
            result1.unwrap_err(),
            SignatureError::VerificationFailed
        ));
        assert!(matches!(
            result2.unwrap_err(),
            SignatureError::VerificationFailed
        ));
    }
}
