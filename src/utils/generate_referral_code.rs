use qp_human_checkphrase::{address_to_checksum, load_bip39_list};
use std::sync::OnceLock;
use tokio::task;

use crate::models::ModelError;

// Create a static OnceLock instance for caching bip39 load.
static WORD_LIST: OnceLock<Vec<String>> = OnceLock::new();

pub async fn generate_referral_code(address: String) -> Result<String, ModelError> {
    let result = task::spawn_blocking(move || {
        //    The closure `|| { ... }` is only executed on the very first call.
        //    `expect` is used here because if the word list can't load,
        //    the application is in an unrecoverable state and should panic.
        let words_list = WORD_LIST
            .get_or_init(|| load_bip39_list().expect("CRITICAL: Failed to load BIP39 word list."));

        let checksum = address_to_checksum(&address, words_list);
        Ok(checksum.join("-"))
    })
    .await;

    match result {
        Ok(inner_result) => inner_result,

        Err(join_error) => {
            eprintln!("Blocking task failed to execute: {}", join_error);
            Err(ModelError::FailedGenerateCheckphrase)
        }
    }
}
