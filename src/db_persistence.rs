use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::repositories::DbResult;

#[derive(Debug, Clone)]
pub struct DbPersistence {
    pub pool: PgPool,
}

impl DbPersistence {
    pub async fn new(database_url: &str) -> DbResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;

        Ok(pool)
    }
}
