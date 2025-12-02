use sqlx::{PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

use crate::{
    db_persistence::DbError,
    models::admin::{Admin, CreateAdmin},
    repositories::DbResult,
};

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

    pub async fn create(&self, new_admin: &CreateAdmin) -> DbResult<String> {
        let created_id = sqlx::query_scalar::<_, String>(
            "
        INSERT INTO admins (username, password) 
        VALUES ($1, $2)
        RETURNING id
        ",
        )
        .bind(new_admin.username.clone())
        .bind(new_admin.password.clone())
        .fetch_optional(&self.pool)
        .await?;

        if let Some(id) = created_id {
            Ok(id)
        } else {
            Err(DbError::RecordNotFound("Record id is generated".to_string()))
        }
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
