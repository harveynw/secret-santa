//! Persistent delivery of confirmation emails and admin-approved reveal invitations.
use crate::App;
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub async fn run(app: Arc<App>) {
    if app.config.resend_key.is_none() || app.config.email_from.is_none() {
        eprintln!("Email delivery is disabled: configure RESEND_API_KEY and EMAIL_FROM.");
        return;
    }
    loop {
        let delay = match deliver_one(&app, "https://api.resend.com/emails").await {
            Ok(()) => Duration::from_secs(2),
            Err(()) => {
                // Never print provider responses, email bodies or recipient information.
                eprintln!("Email delivery failed; queued for retry.");
                Duration::from_secs(30)
            }
        };
        tokio::time::sleep(delay).await;
    }
}

async fn deliver_one(app: &App, endpoint: &str) -> Result<(), ()> {
    let (Some(key), Some(from)) = (&app.config.resend_key, &app.config.email_from) else {
        return Err(());
    };
    // Serialize with registration and verification so stale links cannot race them.
    let mut store = app.store.lock().await;
    let reveal = store.db.reveal_mail.iter().any(|m| !m.accepted);
    let mail = if reveal {
        store.db.reveal_mail.iter().find(|m| !m.accepted).cloned()
    } else if crate::registration_closed(app, &store.db) {
        None
    } else {
        store
            .db
            .mail
            .iter()
            .find(|m| {
                !m.accepted
                    && store
                        .db
                        .participants
                        .iter()
                        .any(|p| p.email == m.to && !p.verified && p.expires_at > crate::now())
            })
            .cloned()
    };
    let Some(mail) = mail else {
        return Ok(());
    };
    let response = app
        .client
        .post(endpoint)
        .bearer_auth(key)
        .header("Idempotency-Key", format!("santa/{}", mail.id))
        .json(&json!({"from": from, "to": [mail.to], "subject": mail.subject, "text": mail.text}))
        .send()
        .await
        .map_err(|_| ())?;
    if !response.status().is_success() {
        return Err(());
    }
    let mut next = store.db.clone();
    let queue = if reveal {
        &mut next.reveal_mail
    } else {
        &mut next.mail
    };
    let saved = queue.iter_mut().find(|m| m.id == mail.id).ok_or(())?;
    saved.accepted = true;
    saved.text.clear();
    store.commit(next).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Config,
        store::{Mail, Participant, Store},
    };
    use axum::{
        Router,
        http::{HeaderMap, StatusCode},
        routing::post,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    #[tokio::test]
    async fn failed_delivery_retries_same_job_and_success_is_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.json");
        let id = Uuid::new_v4();
        let mut store = Store::open(&path).unwrap();
        let mut db = store.db.clone();
        db.participants.push(Participant {
            id: Uuid::new_v4(),
            name: "Test".into(),
            email: "test@example.com".into(),
            verified: false,
            token_hash: "hash".into(),
            expires_at: crate::now() + 3600,
        });
        db.mail.push(Mail {
            id,
            to: "test@example.com".into(),
            subject: "Confirm".into(),
            text: "Private verification link".into(),
            accepted: false,
        });
        store.commit(db).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        let service = Router::new().route(
            "/emails",
            post(move |headers: HeaderMap| {
                let count = counter.fetch_add(1, Ordering::SeqCst);
                async move {
                    assert_eq!(headers["idempotency-key"], format!("santa/{id}"));
                    if count == 0 {
                        StatusCode::TOO_MANY_REQUESTS
                    } else {
                        StatusCode::OK
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/emails", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, service).await.unwrap();
        });
        let mut app = App::new(
            store,
            Config {
                group_name: "Test Group".into(),
                admin_access_token: Some("test-admin-secret-at-least-32-characters".into()),
                site_access_token: "test-site-token".into(),
                base_url: "https://example.com".into(),
                resend_key: Some("test-key".into()),
                email_from: Some("santa@example.com".into()),
            },
        );
        app.selection_path = dir.path().join("selection.json");
        assert!(deliver_one(&app, &endpoint).await.is_err());
        assert!(!app.store.lock().await.db.mail[0].accepted);
        assert!(deliver_one(&app, &endpoint).await.is_ok());
        assert!(deliver_one(&app, &endpoint).await.is_ok());
        assert_eq!(count.load(Ordering::SeqCst), 2);
        drop(app);
        let saved = Store::open(path).unwrap();
        assert!(saved.db.mail[0].accepted);
        assert!(saved.db.mail[0].text.is_empty());
        server.abort();
    }
}
