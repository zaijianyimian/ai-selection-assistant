use super::*;
use serde::de::DeserializeOwned;
use zeroize::Zeroizing;
pub fn endpoint(input: &str) -> AppResult<String> {
    let mut u = url::Url::parse(input.trim()).map_err(|_| bad_url())?;
    if !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
    {
        return Err(bad_url());
    }
    let local = matches!(
        u.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    if u.scheme() != "https" && !(u.scheme() == "http" && local) {
        return Err(bad_url());
    }
    if u.host_str().is_none() {
        return Err(bad_url());
    }
    let path = u.path().trim_end_matches('/').to_string();
    u.set_path(&format!("{path}/"));
    Ok(u.to_string())
}
fn bad_url() -> AppError {
    AppError::new(
        "sync_url",
        "同步地址须为 HTTPS；仅本机测试允许 HTTP。地址不能含用户名、密码、查询参数或片段。",
    )
}
pub struct Http {
    client: reqwest::Client,
    url: String,
    token: Option<Zeroizing<String>>,
}
pub enum Reply<T> {
    Ok(T),
    Rejected(Failure, u16, u64),
}
impl Http {
    pub fn new(url: &str, token: Option<Zeroizing<String>>) -> AppResult<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(12))
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .map_err(|_| network())?,
            url: endpoint(url)?,
            token,
        })
    }
    pub async fn request<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> AppResult<Reply<T>> {
        let mut r = self.client.request(method, format!("{}{path}", self.url));
        if let Some(token) = &self.token {
            r = r.bearer_auth(token.as_str());
        }
        if let Some(body) = body {
            let b = serde_json::to_vec(&body).map_err(storage::err)?;
            if b.len() > MAX_BYTES {
                return Err(AppError::new("sync_request_size", "同步请求超出字节限制。"));
            }
            r = r.header("content-type", "application/json").body(b);
        }
        let mut response = r.send().await.map_err(|_| network())?;
        let status = response.status().as_u16();
        let retry = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(60u64)
            .clamp(1, 300);
        if (300..400).contains(&status) {
            return Err(AppError::new(
                "sync_redirect",
                "同步服务返回重定向；请填写最终 HTTPS 地址。",
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| network())? {
            if bytes.len() + chunk.len() > MAX_BYTES {
                return Err(AppError::new(
                    "sync_response_size",
                    "同步响应超出字节限制。",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !(200..300).contains(&status) {
            let f: Failure = serde_json::from_slice(&bytes).unwrap_or(Failure {
                code: "invalid_response".into(),
                conflicts: vec![],
                context: None,
            });
            return Ok(Reply::Rejected(f, status, retry));
        }
        if status == 204 {
            bytes = b"null".to_vec();
        }
        let value = serde_json::from_slice(&bytes)
            .map_err(|_| AppError::new("sync_protocol", "同步响应格式无效或协议不兼容。"))?;
        Ok(Reply::Ok(value))
    }
}
pub fn network() -> AppError {
    AppError::new(
        "sync_offline",
        "无法连接同步服务，请检查网络、服务地址和证书。",
    )
}
pub fn rejection(status: u16) -> AppError {
    match status {
        401 => AppError::new("sync_auth", "设备凭据已失效，请重新登录同步服务。"),
        429 => AppError::new("sync_rate", "同步请求过于频繁，稍后自动重试。"),
        _ => AppError::new(
            "sync_server",
            "同步服务拒绝请求，请检查服务状态或存储配额。",
        ),
    }
}
pub fn unwrap<T>(reply: Reply<T>) -> AppResult<T> {
    match reply {
        Reply::Ok(v) => Ok(v),
        Reply::Rejected(_, s, retry) => Err(rejection_after(s, retry)),
    }
}

pub fn rejection_after(status: u16, retry: u64) -> AppError {
    let mut e = rejection(status);
    if status == 429 {
        e.retry_after = Some(retry.clamp(1, 300));
    }
    e
}
