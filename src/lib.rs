mod access;
mod admin;
pub mod config;
pub mod mailer;
pub mod selection;
pub mod store;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use store::{Mail, Participant, Store};
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct Config {
    pub group_name: String,
    pub site_access_token: String,
    pub admin_access_token: Option<String>,
    pub base_url: String,
    pub resend_key: Option<String>,
    pub email_from: Option<String>,
}
pub struct App {
    pub store: Mutex<Store>,
    pub config: Config,
    pub client: reqwest::Client,
    pub selection_path: std::path::PathBuf,
}
impl App {
    pub fn new(store: Store, config: Config) -> Self {
        Self {
            selection_path: selection::path(),
            store: Mutex::new(store),
            config,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .expect("HTTP client"),
        }
    }
}
pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/admin.html", get(admin::page))
        .route("/admin", get(admin::page))
        .route("/api/admin/status", get(admin::status))
        .route("/api/admin/registration", post(admin::registration))
        .route("/api/admin/shuffle", post(admin::shuffle))
        .route("/api/admin/send-emails", post(admin::send_emails))
        .route("/api/access-check", get(access::check))
        .route("/api/site-config", get(site_config))
        .route("/api/health", get(|| async { Json(json!({"ok": true})) }))
        .route("/api/register", post(register))
        .route("/api/participants/count", get(participant_count))
        .route("/api/verify", post(verify))
        .route("/api/selection", post(reveal_selection))
        .layer(DefaultBodyLimit::max(4096))
        .layer(axum::middleware::map_response(
            |mut response: Response| async move {
                response
                    .headers_mut()
                    .insert("x-robots-tag", "noindex, nofollow".parse().unwrap());
                response
                    .headers_mut()
                    .insert("cache-control", "no-store".parse().unwrap());
                response
                    .headers_mut()
                    .insert("referrer-policy", "no-referrer".parse().unwrap());
                response
            },
        ))
        .with_state(app)
}
struct Error(StatusCode, &'static str);
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, "Persistence failed")
    }
}
type Result<T> = std::result::Result<T, Error>;
async fn site_config(State(app): State<Arc<App>>) -> Json<Value> {
    Json(json!({"group_name": app.config.group_name}))
}
async fn reveal_selection(
    State(app): State<Arc<App>>,
    Json(input): Json<Verification>,
) -> Result<Json<Value>> {
    let id = Uuid::parse_str(&input.token)
        .map_err(|_| Error(StatusCode::UNAUTHORIZED, "Invalid reveal link"))?;
    let selection = selection::Selection::load(&app.selection_path).map_err(|_| {
        Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "The Christmas draw is not ready yet",
        )
    })?;
    let recipient = selection
        .recipient(id)
        .ok_or(Error(StatusCode::UNAUTHORIZED, "Invalid reveal link"))?;
    Ok(Json(json!({"name": recipient.name})))
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
#[derive(Deserialize)]
struct Registration {
    name: String,
    email: String,
}
async fn register(
    State(app): State<Arc<App>>,
    Json(input): Json<Registration>,
) -> Result<Json<Value>> {
    let name = input.name.trim().to_string();
    let email = input.email.trim().to_ascii_lowercase();
    if name.is_empty()
        || name.len() > 100
        || name.chars().any(char::is_control)
        || email.len() > 254
        || !email_address::EmailAddress::is_valid(&email)
    {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "Valid name and email required",
        ));
    }
    let mut store = app.store.lock().await;
    if registration_closed(&app, &store.db) {
        return Err(Error(StatusCode::CONFLICT, "Registration is closed"));
    }
    let existing = store.db.participants.iter().find(|p| p.email == email);
    if existing.is_some_and(|p| p.verified) {
        return Err(Error(StatusCode::CONFLICT, "Already signed up"));
    }
    if existing.is_none() && store.db.participants.len() >= 200 {
        return Err(Error(StatusCode::CONFLICT, "Registration capacity reached"));
    }
    let id = existing.map(|p| p.id).unwrap_or_else(Uuid::new_v4);
    let reply = Json(json!({"message": if existing.is_some() {
        "A new confirmation email has been queued. Please check your inbox and spam folder."
    } else {
        "Confirmation email queued. Please check your inbox and spam folder."
    }}));
    let mut next = store.db.clone();
    // Resubmission rotates the link and replaces any queued or accepted verification job.
    next.participants.retain(|p| p.email != email);
    next.mail.retain(|m| m.to != email);
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    next.participants.push(Participant {
        id,
        name,
        email: email.clone(),
        verified: false,
        token_hash: hash(&token),
        expires_at: now() + 86400,
    });
    next.mail.push(Mail { id: Uuid::new_v4(), to: email, subject: format!("{}: confirm your Secret Santa registration", app.config.group_name), text: format!("Join {} for Secret Santa.\n\nConfirm your registration within 24 hours and before registration closes:\n{}/confirm.html?token={}", app.config.group_name, app.config.base_url, token), accepted: false });
    store.commit(next)?;
    Ok(reply)
}
#[derive(Deserialize)]
struct Verification {
    token: String,
}
async fn verify(
    State(app): State<Arc<App>>,
    Json(input): Json<Verification>,
) -> Result<Json<Value>> {
    let mut store = app.store.lock().await;
    if registration_closed(&app, &store.db) {
        return Err(Error(StatusCode::CONFLICT, "Registration is closed"));
    }
    let mut next = store.db.clone();
    let digest = hash(&input.token);
    let p = next
        .participants
        .iter_mut()
        .find(|p| !p.verified && p.token_hash == digest && p.expires_at > now())
        .ok_or(Error(
            StatusCode::BAD_REQUEST,
            "Invalid or expired verification link",
        ))?;
    p.verified = true;
    p.token_hash.clear();
    let email = p.email.clone();
    next.mail.retain(|m| m.to != email);
    store.commit(next)?;
    let mut names: Vec<_> = store
        .db
        .participants
        .iter()
        .filter(|p| p.verified)
        .map(|p| p.name.clone())
        .collect();
    names.sort_by_cached_key(|name| name.to_lowercase());
    Ok(Json(
        json!({"verified": true, "confirmed_count": names.len(), "confirmed_names": names}),
    ))
}

async fn participant_count(State(app): State<Arc<App>>) -> Json<Value> {
    let store = app.store.lock().await;
    Json(json!({
        "registration_closed": registration_closed(&app, &store.db),
        "signup_count": store.db.participants.len(),
        "confirmed_count": store.db.participants.iter().filter(|p| p.verified).count()
    }))
}

fn registration_closed(app: &App, db: &store::Database) -> bool {
    db.closed || db.drawn || app.selection_path.try_exists().unwrap_or(true)
}
