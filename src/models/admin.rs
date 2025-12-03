use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgRow, FromRow, Row};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Admin {
    pub id: Uuid,
    pub username: String,
    pub password: String,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
impl<'r> FromRow<'r, PgRow> for Admin {
    fn from_row(row: &'r PgRow) -> Result<Self, sqlx::Error> {
        let id = row.try_get("id")?;
        let username = row.try_get("username")?;
        let password = row.try_get("password")?;
        let updated_at = row.try_get("updated_at")?;
        let created_at = row.try_get("created_at")?;

        Ok(Admin {
            id,
            username,
            password,
            updated_at,
            created_at,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CreateAdmin {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct AdminLoginPayload {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AdminLoginResponse {
    pub access_token: String,
}

#[derive(Serialize)]
pub struct AdminAuthCheckResponse {
    pub id: Uuid,
    pub username: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AdminClaims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
}
