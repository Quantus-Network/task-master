use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenClaims {
    pub sub: String,
    pub iat: usize,
    pub exp: usize,
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

#[derive(Debug, Serialize)]
pub struct GenerateOAuthLinkResponse {
    pub url: String,
}
