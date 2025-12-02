use serde::{Deserialize, Serialize};

fn default_purpose() -> TokenPurpose {
    TokenPurpose::Auth
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum TokenPurpose {
    Auth,
    Oauth,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenClaims {
    pub sub: String,
    pub iat: usize,
    pub exp: usize,

    #[serde(default = "default_purpose")]
    pub purpose: TokenPurpose,
}

#[derive(Debug, Deserialize)]
pub struct RequestChallengeBody {
    pub address: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RequestChallengeResponse {
    pub temp_session_id: String,
    pub challenge: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyLoginBody {
    pub temp_session_id: String,
    pub address: String,
    pub public_key: String,
    pub signature: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyLoginResponse {
    pub access_token: String,
}

#[derive(Deserialize)]
pub struct OauthTokenQuery {
    pub token: String,
}
