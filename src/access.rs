use crate::App;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use subtle::ConstantTimeEq;

// Caddy overwrites the original-URI header before calling this loopback endpoint.
pub async fn check(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    let uri = headers
        .get("x-forwarded-uri")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !uri.starts_with('/') || uri.starts_with("//") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(url) = reqwest::Url::parse(&format!("https://santa.invalid{uri}")) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let tokens: Vec<_> = url
        .query_pairs()
        .filter(|(k, _)| k == "token")
        .map(|(_, v)| v.into_owned())
        .collect();
    if tokens.len() != 1 {
        return (StatusCode::UNAUTHORIZED, "A valid access link is required.").into_response();
    }
    let digest = crate::hash(&tokens[0]);
    let valid =
        match url.path() {
            "/admin" | "/admin.html" => crate::admin::valid_token(&app, &tokens[0]),
            "/reveal.html" => uuid::Uuid::parse_str(&tokens[0]).ok().is_some_and(|id| {
                crate::selection::Selection::load(&app.selection_path)
                    .ok()
                    .is_some_and(|selection| selection.recipient(id).is_some())
            }),
            "/confirm.html" => {
                let store = app.store.lock().await;
                !crate::registration_closed(&app, &store.db)
                    && store.db.participants.iter().any(|p| {
                        !p.verified && p.expires_at > crate::now() && p.token_hash == digest
                    })
            }
            "/" | "/signup.html" | "/api/register" | "/api/participants/count" => {
                let expected = crate::hash(&app.config.site_access_token);
                bool::from(digest.as_bytes().ct_eq(expected.as_bytes()))
            }
            _ => false,
        };
    if valid {
        StatusCode::NO_CONTENT.into_response()
    } else {
        (StatusCode::UNAUTHORIZED, "Invalid or expired access link.").into_response()
    }
}
