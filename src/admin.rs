use crate::{
    App, Error, Result,
    selection::{Entry, Selection},
    store::{Database, Mail},
};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::Html,
};
use rand::seq::SliceRandom;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub(crate) fn valid_token(app: &App, token: &str) -> bool {
    app.config
        .admin_access_token
        .as_ref()
        .is_some_and(|expected| {
            bool::from(
                crate::hash(token)
                    .as_bytes()
                    .ct_eq(crate::hash(expected).as_bytes()),
            )
        })
}
fn authorize(app: &App, headers: &HeaderMap) -> Result<()> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if valid_token(app, token) {
        Ok(())
    } else {
        Err(Error(
            StatusCode::UNAUTHORIZED,
            "A valid admin token is required",
        ))
    }
}
#[derive(Deserialize)]
pub struct PageQuery {
    token: Option<String>,
}
pub async fn page(
    State(app): State<Arc<App>>,
    Query(query): Query<PageQuery>,
) -> Result<Html<&'static str>> {
    if !valid_token(&app, query.token.as_deref().unwrap_or("")) {
        return Err(Error(
            StatusCode::UNAUTHORIZED,
            "A valid admin link is required",
        ));
    }
    Ok(Html(include_str!("admin.html")))
}
fn snapshot(app: &App, db: &Database) -> Result<Value> {
    // A malformed or unreadable selection must never look like an unshuffled draw.
    let selection = if app.selection_path.try_exists()? {
        Some(Selection::load(&app.selection_path)?)
    } else {
        None
    };
    if selection.is_none() && (db.drawn || !db.reveal_mail.is_empty()) {
        return Err(Error(
            StatusCode::CONFLICT,
            "Draw data is missing; restore the selection before continuing",
        ));
    }
    let accepted = db.reveal_mail.iter().filter(|m| m.accepted).count();
    let stage = if !db.reveal_mail.is_empty() {
        if accepted == db.reveal_mail.len() {
            "emails_accepted"
        } else {
            "emails_queued"
        }
    } else if selection.is_some() {
        "shuffled"
    } else if crate::registration_closed(app, db) {
        "registration_closed"
    } else {
        "registration_open"
    };
    Ok(json!({
        "stage": stage,
        "signup_count": db.participants.len(),
        "confirmed_count": db.participants.iter().filter(|p| p.verified).count(),
        "pending_count": db.participants.iter().filter(|p| !p.verified).count(),
        "draw_count": selection.map(|s| s.participants.len()).unwrap_or(0),
        "emails_total": db.reveal_mail.len(), "emails_accepted": accepted,
        "email_configured": app.config.resend_key.is_some() && app.config.email_from.is_some()
    }))
}
pub async fn status(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Json<Value>> {
    authorize(&app, &headers)?;
    let store = app.store.lock().await;
    Ok(Json(snapshot(&app, &store.db)?))
}
#[derive(Deserialize)]
pub struct Registration {
    open: bool,
}
pub async fn registration(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Json(input): Json<Registration>,
) -> Result<Json<Value>> {
    authorize(&app, &headers)?;
    let mut store = app.store.lock().await;
    let state = snapshot(&app, &store.db)?;
    if !matches!(
        state["stage"].as_str(),
        Some("registration_open" | "registration_closed")
    ) {
        return Err(Error(
            StatusCode::CONFLICT,
            "Registration is locked after shuffling",
        ));
    }
    let mut next = store.db.clone();
    next.closed = !input.open;
    store.commit(next)?;
    Ok(Json(snapshot(&app, &store.db)?))
}
pub async fn shuffle(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Json<Value>> {
    authorize(&app, &headers)?;
    let mut store = app.store.lock().await;
    if snapshot(&app, &store.db)?["stage"] != "registration_closed" {
        return Err(Error(
            StatusCode::CONFLICT,
            "Close registration before shuffling; existing draws stay fixed",
        ));
    }
    let mut selection = Selection {
        participants: store
            .db
            .participants
            .iter()
            .filter(|p| p.verified)
            .map(|p| Entry {
                id: p.id,
                name: p.name.clone(),
                email: p.email.clone(),
            })
            .collect(),
    };
    if selection.participants.len() < 2 {
        return Err(Error(
            StatusCode::CONFLICT,
            "At least two confirmed participants are required",
        ));
    }
    selection.validate()?;
    // Persist closure first, so a crash cannot leave the draw open for registration.
    let mut next = store.db.clone();
    next.closed = true;
    store.commit(next)?;
    selection.participants.shuffle(&mut rand::rng());
    selection.save_new(&app.selection_path)?;
    let mut next = store.db.clone();
    next.drawn = true;
    store.commit(next)?;
    Ok(Json(snapshot(&app, &store.db)?))
}
pub async fn send_emails(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Json<Value>> {
    authorize(&app, &headers)?;
    let mut store = app.store.lock().await;
    let state = snapshot(&app, &store.db)?;
    // Repeated clicks never create new jobs or resend accepted messages.
    if matches!(
        state["stage"].as_str(),
        Some("emails_queued" | "emails_accepted")
    ) {
        return Ok(Json(state));
    }
    if state["stage"] != "shuffled" {
        return Err(Error(
            StatusCode::CONFLICT,
            "Shuffle before sending reveal emails",
        ));
    }
    if app.config.resend_key.is_none() || app.config.email_from.is_none() {
        return Err(Error(
            StatusCode::CONFLICT,
            "Configure RESEND_API_KEY and EMAIL_FROM before sending",
        ));
    }
    // Preserve duplicate-send protection for draws with a legacy delivery journal.
    if app
        .selection_path
        .with_extension("sent.json")
        .try_exists()?
    {
        return Err(Error(
            StatusCode::CONFLICT,
            "Legacy email delivery history exists; reconcile it before using the admin sender",
        ));
    }
    let selection = Selection::load(&app.selection_path)?;
    let mut next = store.db.clone();
    next.closed = true;
    next.reveal_mail = selection.participants.iter().map(|p| Mail {
        id: Uuid::new_v4(), to: p.email.clone(), subject: format!("{}: your Secret Santa is ready to reveal", app.config.group_name),
        text: format!("Hello {},\n\nYour {} Secret Santa draw is ready. Reveal who you’re buying for:\n{}/reveal.html?token={}\n\nGift limit: £0-£20\nKeep it between you and Santa.", p.name, app.config.group_name, app.config.base_url, p.id),
        accepted: false,
    }).collect();
    store.commit(next)?;
    Ok(Json(snapshot(&app, &store.db)?))
}
