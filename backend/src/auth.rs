use axum::{
    extract::{FromRequestParts, State},
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;
use crate::AppState;

const KOOMPI_OAUTH_BASE: &str = "https://oauth.koompi.org";
const SESSION_TTL_SECS: u64 = 86400; // 24 hours
const STATE_TTL_SECS: u64 = 600; // 10 minutes

/// User info stored in session
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionUser {
    pub id: String,
    pub full_name: String,
    pub email: Option<String>,
    pub username: Option<String>,
    pub avatar: Option<String>,
    pub wallet_address: Option<String>,
}

/// Axum extractor: reads session cookie, validates against Redis, yields user
#[derive(Clone, Debug)]
pub struct AuthUser(pub SessionUser);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        let cookie_header = parts
            .headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let session_id = cookie_header
            .split(';')
            .filter_map(|c| {
                let c = c.trim();
                c.strip_prefix("karuna_session=")
            })
            .next();

        let session_id = match session_id {
            Some(id) if !id.is_empty() => id,
            _ => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({"error": "Not authenticated"})),
                )
                    .into_response());
            }
        };

        let key = format!("karuna:session:{session_id}");
        let mut conn = app_state
            .redis
            .conn()
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Session store error"})),
                )
                    .into_response()
            })?;

        let user_json: Option<String> = conn.get(&key).await.map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Session lookup error"})),
            )
                .into_response()
        })?;

        match user_json {
            Some(json) => {
                let user: SessionUser = serde_json::from_str(&json).map_err(|_| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "Session data corrupt"})),
                    )
                        .into_response()
                })?;
                Ok(AuthUser(user))
            }
            None => Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Session expired"})),
            )
                .into_response()),
        }
    }
}

/// Needed for the extractor to pull AppState from a composed state type.
use axum::extract::FromRef;

// ── Routes ──────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/login", get(login))
        .route("/api/auth/token", post(exchange_token))
        .route("/api/auth/me", get(me))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/dev-seed", post(dev_seed))
}

/// GET /api/auth/login — Returns KOOMPI OAuth URL (with CSRF state)
async fn login(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let cfg = &state.config;
    if cfg.koompi_client_id.is_empty() {
        return Err(AppError::Internal("KOOMPI OAuth not configured".into()));
    }

    // Generate CSRF state token
    let csrf_state = Uuid::new_v4().to_string();
    let state_key = format!("karuna:oauth_state:{csrf_state}");
    let mut conn = state.redis.conn().await?;
    conn.set_ex::<_, _, ()>(&state_key, "1", STATE_TTL_SECS)
        .await
        .map_err(|e| AppError::Internal(format!("Redis set state error: {e}")))?;

    let url = format!(
        "{KOOMPI_OAUTH_BASE}/v2/oauth?client_id={}&redirect_uri={}&scope=profile.basic%20profile.contact&state={csrf_state}",
        cfg.koompi_client_id,
        urlencoding::encode(&cfg.koompi_redirect_uri),
    );

    Ok(Json(serde_json::json!({ "url": url })))
}

/// POST /api/auth/token — Exchange OAuth code for session
#[derive(Deserialize)]
struct TokenRequest {
    code: String,
    state: String,
}

async fn exchange_token(
    State(state): State<AppState>,
    Json(req): Json<TokenRequest>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = &state.config;

    // Validate CSRF state
    let state_key = format!("karuna:oauth_state:{}", req.state);
    let mut conn = state.redis.conn().await?;
    let exists: bool = conn
        .get_del(&state_key)
        .await
        .unwrap_or(false);
    if !exists {
        return Err(AppError::BadRequest("Invalid or expired OAuth state".into()));
    }

    // Exchange code for tokens + user at KOOMPI
    let client = reqwest::Client::new();
    let token_res = client
        .post(format!("{KOOMPI_OAUTH_BASE}/v2/oauth/token"))
        .json(&serde_json::json!({
            "client_id": cfg.koompi_client_id,
            "client_secret": cfg.koompi_client_secret,
            "code": req.code,
            "redirect_uri": cfg.koompi_redirect_uri,
        }))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("KOOMPI token request failed: {e}")))?;

    if !token_res.status().is_success() {
        let body = token_res.text().await.unwrap_or_default();
        tracing::error!("KOOMPI token exchange failed: {body}");
        return Err(AppError::BadRequest("OAuth token exchange failed".into()));
    }

    let token_data: serde_json::Value = token_res
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("KOOMPI token parse error: {e}")))?;

    // Extract user info from response (KOOMPI returns user in token response)
    let user_obj = &token_data["user"];
    let access_token = token_data["access_token"].as_str().unwrap_or("");

    // If user not in token response, fetch from userinfo endpoint
    let user = if user_obj.is_object() && user_obj.get("id").is_some() {
        SessionUser {
            id: user_obj["id"].as_str().unwrap_or("").to_string(),
            full_name: user_obj["full_name"]
                .as_str()
                .or(user_obj["name"].as_str())
                .unwrap_or("User")
                .to_string(),
            email: user_obj["email"].as_str().map(String::from),
            username: user_obj["username"].as_str().map(String::from),
            avatar: user_obj["avatar"].as_str().map(String::from),
            wallet_address: user_obj["wallet_address"].as_str().map(String::from),
        }
    } else if !access_token.is_empty() {
        // Fallback: fetch user info from /v2/oauth/userinfo
        let userinfo_res = client
            .get(format!("{KOOMPI_OAUTH_BASE}/v2/oauth/userinfo"))
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("KOOMPI userinfo failed: {e}")))?;

        let info: serde_json::Value = userinfo_res
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Userinfo parse error: {e}")))?;

        SessionUser {
            id: info["id"].as_str().unwrap_or("").to_string(),
            full_name: info["full_name"]
                .as_str()
                .or(info["name"].as_str())
                .unwrap_or("User")
                .to_string(),
            email: info["email"].as_str().map(String::from),
            username: info["username"].as_str().map(String::from),
            avatar: info["avatar"].as_str().map(String::from),
            wallet_address: info["wallet_address"].as_str().map(String::from),
        }
    } else {
        return Err(AppError::Internal("No user info in KOOMPI response".into()));
    };

    // Create session in Redis
    let session_id = Uuid::new_v4().to_string();
    let session_key = format!("karuna:session:{session_id}");
    let user_json = serde_json::to_string(&user)
        .map_err(|e| AppError::Internal(format!("Serialize session: {e}")))?;

    conn.set_ex::<_, _, ()>(&session_key, &user_json, SESSION_TTL_SECS)
        .await
        .map_err(|e| AppError::Internal(format!("Redis session store error: {e}")))?;

    // Also store the KOOMPI access/refresh tokens for the session
    if let Some(refresh) = token_data["refresh_token"].as_str() {
        let refresh_key = format!("karuna:session:{session_id}:refresh");
        let _ = conn
            .set_ex::<_, _, ()>(&refresh_key, refresh, SESSION_TTL_SECS)
            .await;
    }

    // Set session cookie
    let cookie = format!(
        "karuna_session={session_id}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_TTL_SECS}"
    );

    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({ "user": user })),
    ))
}

/// GET /api/auth/me — Return current user from session
async fn me(auth: AuthUser) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "user": auth.0 }))
}

/// POST /api/auth/logout — Clear session
async fn logout(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    // Read session cookie
    let cookie_header = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if let Some(session_id) = cookie_header
        .split(';')
        .filter_map(|c| c.trim().strip_prefix("karuna_session="))
        .next()
    {
        let key = format!("karuna:session:{session_id}");
        let refresh_key = format!("karuna:session:{session_id}:refresh");
        let mut conn = state.redis.conn().await?;
        let _: () = conn.del(&key).await.unwrap_or(());
        let _: () = conn.del(&refresh_key).await.unwrap_or(());
    }

    // Clear cookie
    let cookie = "karuna_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0";

    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({"status": "logged_out"})),
    ))
}

/// POST /api/auth/dev-seed — Create a dev session (non-production only)
#[derive(Deserialize)]
struct DevSeedRequest {
    name: Option<String>,
    email: Option<String>,
}

async fn dev_seed(
    State(state): State<AppState>,
    Json(req): Json<DevSeedRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user = SessionUser {
        id: format!("dev-{}", Uuid::new_v4()),
        full_name: req.name.unwrap_or_else(|| "Rithythul".into()),
        email: Some(req.email.unwrap_or_else(|| "rithythul@gmail.com".into())),
        username: Some("rithythul".into()),
        avatar: None,
        wallet_address: None,
    };

    let session_id = Uuid::new_v4().to_string();
    let session_key = format!("karuna:session:{session_id}");
    let user_json = serde_json::to_string(&user)
        .map_err(|e| AppError::Internal(format!("Serialize session: {e}")))?;

    let mut conn = state.redis.conn().await?;
    conn.set_ex::<_, _, ()>(&session_key, &user_json, SESSION_TTL_SECS)
        .await
        .map_err(|e| AppError::Internal(format!("Redis dev-seed error: {e}")))?;

    let cookie = format!(
        "karuna_session={session_id}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_TTL_SECS}"
    );

    tracing::info!("Dev seed session created for {}", user.full_name);

    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(serde_json::json!({ "user": user })),
    ))
}
