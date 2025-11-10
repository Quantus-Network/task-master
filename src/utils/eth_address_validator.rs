use tiny_keccak::{Hasher, Keccak};

/// Validates a string to check if it's a valid Ethereum address.
///
/// This function performs the following checks:
/// 1.  Checks for the "0x" prefix.
/// 2.  Validates the length is exactly 42 characters.
/// 3.  Ensures all characters are valid hexadecimal digits.
/// 4.  Validates the EIP-55 mixed-case checksum if present. If the address
///     is all lowercase or all uppercase, it is also considered valid.
///
/// # Arguments
///
/// * `address` - A string slice that holds the Ethereum address to validate.
///
/// # Returns
///
/// * `true` if the address is a valid Ethereum address, `false` otherwise.
///
/// # Examples
///
/// ```
// assert!(is_valid_eth_address("0xfb6916095ca1df60bb79ce92ce3ea74c37c5d359")); // lowercase
// assert!(is_valid_eth_address("0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359")); // checksummed
// assert!(!is_valid_eth_address("0xfb6916095ca1df60bb79ce92ce3ea74c37c5d35")); // invalid length
/// ```
pub fn is_valid_eth_address(address: &str) -> bool {
    if address.len() != 42 {
        return false;
    }

    let prefix = &address[..2];
    if prefix != "0x" && prefix != "0X" {
        return false;
    }

    // Get the address part without the "0x" prefix
    let addr_part = &address[2..];

    if !addr_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }

    // If the address is all lowercase or all uppercase, it's valid (no checksum)
    let is_all_lowercase = addr_part
        .chars()
        .all(|c| c.is_lowercase() || c.is_digit(10));
    let is_all_uppercase = addr_part
        .chars()
        .all(|c| c.is_uppercase() || c.is_digit(10));

    if is_all_lowercase || is_all_uppercase {
        return true;
    }

    // If it's mixed-case, validate the EIP-55 checksum
    validate_checksum(addr_part)
}

/// Validates the EIP-55 checksum for a given address part (without "0x").
fn validate_checksum(address_part: &str) -> bool {
    let lower_addr = address_part.to_lowercase();

    // Compute the Keccak-256 hash of the lowercase address
    let mut hasher = Keccak::v256();
    hasher.update(lower_addr.as_bytes());
    let mut hash_output = [0u8; 32];
    hasher.finalize(&mut hash_output);

    // Iterate through the original address and compare casing based on the hash
    for (i, c) in address_part.chars().enumerate() {
        if c.is_ascii_digit() {
            continue; // Digits are not checksummed
        }

        // Get the i-th nibble (4 bits) of the hash.
        // Each byte of the hash corresponds to two hex characters in the address.
        let hash_nibble = if i % 2 == 0 {
            hash_output[i / 2] >> 4 // Get the high nibble
        } else {
            hash_output[i / 2] & 0x0F // Get the low nibble
        };

        if hash_nibble >= 8 {
            // If the nibble is 8 or greater, the character must be uppercase
            if c.is_lowercase() {
                return false;
            }
        } else {
            // Otherwise, the character must be lowercase
            if c.is_uppercase() {
                return false;
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correctly_validate_eth_address() {
        let addresses_to_test = vec![
            // --- Valid Addresses ---
            // Vitalik Buterin's address (checksummed)
            ("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045", true),
            // Same address, lowercase
            ("0xd8da6bf26964af9d7eed9e03e53415d37aa96045", true),
            // Same address, uppercase
            ("0XD8DA6BF26964AF9D7EED9E03E53415D37AA96045", true),
            // Another valid checksum address
            ("0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359", true),
            // --- Invalid Addresses ---
            // Invalid checksum
            ("0xd8dA6BF26964aF9D7eEd9e03E53415D37aA9604f", false),
            // Invalid length (too short)
            ("0xd8da6bf26964af9d7eed9e03e53415d37aa9604", false),
            // Invalid length (too long)
            ("0xd8da6bf26964af9d7eed9e03e53415d37aa960455", false),
            // Missing "0x" prefix
            ("d8da6bf26964af9d7eed9e03e53415d37aa96045", false),
            // Invalid hex characters
            ("0xd8da6bf26964af9d7eed9e03e53415d37aa9604g", false),
            // Empty string
            ("", false),
        ];

        for (address, expected) in addresses_to_test {
            let is_valid = is_valid_eth_address(address);
            assert_eq!(
                is_valid, expected,
                "Validation failed for address: {}",
                address
            );
        }
    }
}
