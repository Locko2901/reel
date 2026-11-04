//! A paired app's hashed bearer token; no cookie is read, so a site has nothing to forge.
use crate::AppState;
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use reel_store::models::{Device, User as UserRow};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct DeviceAuth {
    pub device: Device,
    pub user: UserRow,
}

pub fn token_hash(token: &str) -> String {
    reel_pipeline::hash::hex(&Sha256::digest(token.as_bytes()))
}

pub fn code_hash(code: &str) -> String {
    reel_pipeline::hash::hex(&Sha256::digest(
        reel_core::auth::normal_code(code).as_bytes(),
    ))
}

fn refuse(status: StatusCode, line: &str) -> Response {
    (status, axum::Json(serde_json::json!({ "error": line }))).into_response()
}

fn seen_recently(d: &Device) -> bool {
    d.last_seen_at
        .as_deref()
        .and_then(reel_core::format::parse)
        .map(|t| jiff::Timestamp::now().as_second() - t.as_second() < 60)
        .unwrap_or(false)
}

pub async fn require_device(State(st): State<AppState>, mut req: Request, next: Next) -> Response {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.trim().to_string());
    let Some(token) = token.filter(|t| reel_core::ids::looks_like_device_token(t)) else {
        return refuse(
            StatusCode::UNAUTHORIZED,
            "this route takes a device token in the Authorization header; pair the app first",
        );
    };
    let device = match st.ctx.store.device_by_token_hash(&token_hash(&token)).await {
        Ok(Some(d)) if d.revoked_at.is_none() => d,
        Ok(Some(_)) => {
            return refuse(
                StatusCode::UNAUTHORIZED,
                "this device was revoked; pair it again",
            )
        }
        Ok(None) => {
            return refuse(
                StatusCode::UNAUTHORIZED,
                "unknown device token; pair the app again",
            )
        }
        Err(e) => {
            tracing::error!(error = %e, "device lookup");
            return refuse(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the device table did not answer",
            );
        }
    };
    let user = match st.ctx.store.user(&device.user_id).await {
        Ok(Some(u)) if u.disabled_at.is_none() => u,
        Ok(_) => {
            return refuse(
                StatusCode::UNAUTHORIZED,
                "the account this device belongs to is disabled",
            )
        }
        Err(e) => {
            tracing::error!(error = %e, "device owner lookup");
            return refuse(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the user table did not answer",
            );
        }
    };
    if !seen_recently(&device) {
        let _ = st.ctx.store.touch_device(&device.id).await;
    }
    req.extensions_mut().insert(DeviceAuth { device, user });
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_code_hash_ignores_case_and_the_dash() {
        assert_eq!(code_hash("ABCD-2345"), code_hash(" abcd2345 "));
        assert_ne!(code_hash("abcd-2345"), code_hash("abcd-2346"));
        assert_eq!(token_hash("x").len(), 64);
    }
}
