use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::{models::admin::Admin, repositories::DbResult};

#[derive(Clone, Debug)]
pub struct AdminRepository {
    pool: PgPool,
}
impl AdminRepository {
    fn create_select_base_query<'a>() -> QueryBuilder<'a, Postgres> {
        QueryBuilder::new("SELECT * FROM admins")
    }

    pub fn new(pool: &PgPool) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn find_by_id(&self, id: &Uuid) -> DbResult<Option<Admin>> {
        let mut qb = AdminRepository::create_select_base_query();
        qb.push(" WHERE id = ");
        qb.push_bind(id);

        let admin = qb.build_query_as().fetch_optional(&self.pool).await?;

        Ok(admin)
    }

    pub async fn find_by_username(&self, username: &str) -> DbResult<Option<Admin>> {
        let mut qb = AdminRepository::create_select_base_query();
        qb.push(" WHERE username = ");
        qb.push_bind(username);

        let admin = qb.build_query_as().fetch_optional(&self.pool).await?;

        Ok(admin)
    }
}
