use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;

#[derive(Debug)]
pub enum AppError {
    ValidationErrors(ValidationErrors),
    DatabaseError(String),
    NotFound(String),
    InternalServerError,
}
impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        #[derive(Serialize)]
        pub struct ErrorResponse<D = String> {
            pub error: D,
        }

        let (status, error) = match self {
            AppError::DatabaseError(err_msg) => (StatusCode::INTERNAL_SERVER_ERROR, err_msg),
            AppError::NotFound(err_msg) => (StatusCode::NOT_FOUND, err_msg),
            AppError::ValidationErrors(err_msg) => (
                StatusCode::BAD_REQUEST,
                serde_json::to_string(&err_msg.errors).unwrap(),
            ),
            AppError::InternalServerError => (
                StatusCode::INTERNAL_SERVER_ERROR,
                String::from("Internal server error."),
            ),
        };

        let body = Json(ErrorResponse { error });

        (status, body).into_response()
    }
}

#[derive(Debug, Serialize)]
pub struct FieldError {
    field: String,
    message: String,
}

// Collection of validation errors
#[derive(Debug, Serialize)]
pub struct ValidationErrors {
    errors: Vec<FieldError>,
}
impl ValidationErrors {
    pub fn new() -> Self {
        ValidationErrors { errors: Vec::new() }
    }

    pub fn add(&mut self, field: &str, error: String) {
        self.errors.push(FieldError {
            field: field.to_string(),
            message: error,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}
