use asa_sync_protocol::{Conflict, Context, Failure};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
#[derive(Debug)]
pub struct Error {
    pub status: StatusCode,
    pub body: Failure,
}
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn new(status: StatusCode, code: &str) -> Self {
        Self {
            status,
            body: Failure {
                code: code.into(),
                conflicts: vec![],
                context: None,
            },
        }
    }
    pub fn bad(code: &str) -> Self {
        Self::new(StatusCode::BAD_REQUEST, code)
    }
    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "authentication_required")
    }
    pub fn storage(_: impl std::fmt::Debug) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "storage_unavailable")
    }
    pub fn conflict(code: &str, context: Context, conflicts: Vec<Conflict>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            body: Failure {
                code: code.into(),
                conflicts,
                context: Some(context),
            },
        }
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let rate = self.status == StatusCode::TOO_MANY_REQUESTS;
        let mut r = (self.status, Json(self.body)).into_response();
        if rate {
            r.headers_mut().insert("retry-after", "60".parse().unwrap());
        }
        r
    }
}
