use crate::{
    error::{Error, Result},
    store::Store,
};
use argon2::{
    password_hash::SaltString, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier,
};
use asa_sync_protocol::*;
use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{delete, get, post},
    Json, Router,
};
use rand_core::{OsRng, RngCore};
use serde::Deserialize;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
pub fn hash_password(password: &str) -> Result<String> {
    if password.len() < 12 || password.len() > 8192 || password.chars().any(char::is_control) {
        return Err(Error::bad("password_length"));
    }
    let salt = SaltString::generate(&mut OsRng);
    argon()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(Error::storage)
}
fn argon() -> Argon2<'static> {
    Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        Params::new(65_536, 3, 1, None).expect("constant password profile"),
    )
}
struct App {
    store: Arc<Store>,
    rates: Mutex<HashMap<String, (Instant, u32)>>,
    slots: Arc<tokio::sync::Semaphore>,
}
impl App {
    async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || f(&store))
            .await
            .map_err(Error::storage)?
    }
    fn rate(&self, key: String, limit: u32) -> Result<()> {
        let mut rates = self.rates.lock().map_err(Error::storage)?;
        rates.retain(|_, (t, _)| t.elapsed() < Duration::from_secs(60));
        if rates.len() >= 10_000 && !rates.contains_key(&key) {
            return Err(Error::new(StatusCode::TOO_MANY_REQUESTS, "rate_limit"));
        }
        let entry = rates.entry(key).or_insert((Instant::now(), 0));
        if entry.1 >= limit {
            return Err(Error::new(StatusCode::TOO_MANY_REQUESTS, "rate_limit"));
        }
        entry.1 += 1;
        Ok(())
    }
    async fn verify(&self, login: Login, ip: SocketAddr) -> Result<(String, String)> {
        if login.protocol != VERSION {
            return Err(Error::bad("protocol_version"));
        }
        self.rate(format!("login:{}", ip.ip()), self.store.limits.logins)?;
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| Error::new(StatusCode::TOO_MANY_REQUESTS, "authentication_busy"))?;
        let hash = self.run(|s| s.password_hash()).await?;
        let compare = hash.clone();
        let password = Zeroizing::new(login.password);
        let valid = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            PasswordHash::new(&compare).is_ok_and(|parsed| {
                argon()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
        })
        .await
        .map_err(Error::storage)?;
        if !valid {
            return Err(Error::unauthorized());
        }
        Ok((hash, login.device_name))
    }
}
fn json<T>(
    value: std::result::Result<Json<T>, axum::extract::rejection::JsonRejection>,
) -> Result<T> {
    value.map(|v| v.0).map_err(|e| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            Error::new(StatusCode::PAYLOAD_TOO_LARGE, "body_limit")
        } else {
            Error::bad("invalid_json")
        }
    })
}
pub fn router(store: Arc<Store>) -> Router {
    let app = Arc::new(App {
        store,
        rates: Mutex::new(HashMap::new()),
        slots: Arc::new(tokio::sync::Semaphore::new(2)),
    });
    let protected = Router::new()
        .route("/v1/devices", get(devices))
        .route("/v1/devices/{id}", delete(revoke))
        .route("/v1/changes", get(changes))
        .route("/v1/push", post(push))
        .route("/v1/memory/reset", post(reset))
        .route_layer(middleware::from_fn_with_state(app.clone(), authorize));
    Router::new()
        .route(
            "/healthz",
            get(|| async { Json(serde_json::json!({"status":"ok"})) }),
        )
        .route("/v1/auth/check", post(check))
        .route("/v1/devices/enroll", post(enroll))
        .merge(protected)
        .layer(DefaultBodyLimit::max(MAX_BYTES))
        .with_state(app)
}
async fn authorize(
    State(app): State<Arc<App>>,
    mut request: axum::extract::Request,
    next: Next,
) -> Result<Response> {
    let token = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .filter(|s| s.len() <= 256)
        .ok_or_else(Error::unauthorized)?
        .to_owned();
    let id = app.run(move |s| s.authenticate(&token)).await?;
    app.rate(format!("device:{id}"), app.store.limits.requests)?;
    request.extensions_mut().insert(id);
    Ok(next.run(request).await)
}
async fn check(
    State(app): State<Arc<App>>,
    ConnectInfo(ip): ConnectInfo<SocketAddr>,
    body: std::result::Result<Json<Login>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Context>> {
    app.verify(json(body)?, ip).await?;
    Ok(Json(app.run(|s| s.context()).await?))
}
async fn enroll(
    State(app): State<Arc<App>>,
    ConnectInfo(ip): ConnectInfo<SocketAddr>,
    body: std::result::Result<Json<Login>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Enrollment>> {
    let (hash, name) = app.verify(json(body)?, ip).await?;
    let mut random = [0u8; 32];
    OsRng.fill_bytes(&mut random);
    let token = Zeroizing::new(digest(&random));
    Ok(Json(
        app.run(move |s| s.enroll(&hash, &name, &token)).await?,
    ))
}
async fn devices(State(app): State<Arc<App>>) -> Result<Json<Vec<Device>>> {
    Ok(Json(app.run(|s| s.devices()).await?))
}
async fn revoke(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<StatusCode> {
    if !valid_id(&id) {
        return Err(Error::bad("device_id"));
    }
    app.run(move |s| s.revoke(&id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pull {
    stream: Stream,
    after: i64,
    until: Option<i64>,
    limit: Option<usize>,
}
async fn changes(
    State(app): State<Arc<App>>,
    query: std::result::Result<Query<Pull>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Page>> {
    let q = query.map_err(|_| Error::bad("invalid_query"))?.0;
    Ok(Json(
        app.run(move |s| s.page(q.stream, q.after, q.until, q.limit.unwrap_or(MAX_PAGE)))
            .await?,
    ))
}
async fn push(
    State(app): State<Arc<App>>,
    body: std::result::Result<Json<Push>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Accepted>> {
    let p = json(body)?;
    Ok(Json(app.run(move |s| s.push(&p)).await?))
}
async fn reset(
    State(app): State<Arc<App>>,
    body: std::result::Result<Json<Reset>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Accepted>> {
    let p = json(body)?;
    Ok(Json(app.run(move |s| s.reset(&p)).await?))
}
