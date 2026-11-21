use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use secret_santa::{
    App, Config, router,
    selection::Selection,
    store::{Participant, Store},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;
const ADMIN: &str = "test-admin-secret-at-least-32-characters";
const SIGNUP: &str = "test-signup-secret-at-least-32-characters";
fn app(dir: &tempfile::TempDir, closed: bool) -> App {
    let mut store = Store::open(dir.path().join("db.json")).unwrap();
    let mut next = store.db.clone();
    next.closed = closed;
    store.commit(next).unwrap();
    let mut app = App::new(
        store,
        Config {
            group_name: "Test Group".into(),
            site_access_token: SIGNUP.into(),
            admin_access_token: Some(ADMIN.into()),
            base_url: "https://example.com".into(),
            resend_key: Some("fake-key-never-sent".into()),
            email_from: Some("santa@example.com".into()),
        },
    );
    app.selection_path = dir.path().join("selection.json");
    app
}
async fn call(
    routes: &Router,
    path: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut request =
        Request::builder()
            .uri(path)
            .method(if body.is_some() { "POST" } else { "GET" });
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = routes
        .clone()
        .oneshot(
            request
                .header("content-type", "application/json")
                .body(
                    body.map(|b| Body::from(b.to_string()))
                        .unwrap_or(Body::empty()),
                )
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
#[tokio::test]
async fn admin_page_and_every_action_require_separate_token() {
    let dir = tempfile::tempdir().unwrap();
    let routes = router(Arc::new(app(&dir, false)));
    for token in [None, Some("wrong"), Some(SIGNUP)] {
        for (path, data) in [
            ("status", None),
            ("registration", Some(json!({"open":false}))),
            ("shuffle", Some(json!({}))),
            ("send-emails", Some(json!({}))),
        ] {
            assert_eq!(
                call(&routes, &format!("/api/admin/{path}"), data, token)
                    .await
                    .0,
                StatusCode::UNAUTHORIZED
            );
        }
    }
    for path in ["/admin", "/admin.html"] {
        for token in ["", "wrong", SIGNUP] {
            assert_eq!(
                call(&routes, &format!("{path}?token={token}"), None, None)
                    .await
                    .0,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            call(&routes, &format!("{path}?token={ADMIN}"), None, None)
                .await
                .0,
            StatusCode::OK
        );
    }
    // Supplying a valid query token or a forged reverse-proxy header cannot authorize mutations.
    assert_eq!(
        call(
            &routes,
            &format!("/api/admin/shuffle?token={ADMIN}"),
            Some(json!({})),
            None
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}
#[tokio::test]
async fn stages_are_sequential_draw_is_fixed_and_email_queue_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(app(&dir, false));
    let routes = router(app.clone());
    let status = || call(&routes, "/api/admin/status", None, Some(ADMIN));
    assert_eq!(status().await.1["stage"], "registration_open");
    for endpoint in ["shuffle", "send-emails"] {
        assert_eq!(
            call(
                &routes,
                &format!("/api/admin/{endpoint}"),
                Some(json!({})),
                Some(ADMIN)
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
    }
    for open in [false, true, false] {
        assert_eq!(
            call(
                &routes,
                "/api/admin/registration",
                Some(json!({"open":open})),
                Some(ADMIN)
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    assert_eq!(
        call(&routes, "/api/admin/shuffle", Some(json!({})), Some(ADMIN))
            .await
            .0,
        StatusCode::CONFLICT
    );
    {
        let mut store = app.store.lock().await;
        let mut next = store.db.clone();
        next.participants = (0..4)
            .map(|i| Participant {
                id: Uuid::new_v4(),
                name: format!("Person {i}"),
                email: format!("p{i}@example.com"),
                verified: i < 3,
                token_hash: "".into(),
                expires_at: u64::MAX,
            })
            .collect();
        store.commit(next).unwrap();
    }
    assert_eq!(
        call(&routes, "/api/admin/shuffle", Some(json!({})), Some(ADMIN))
            .await
            .1["stage"],
        "shuffled"
    );
    let bytes = std::fs::read(&app.selection_path).unwrap();
    let draw = Selection::load(&app.selection_path).unwrap();
    assert_eq!(draw.participants.len(), 3);
    for p in &draw.participants {
        assert_ne!(draw.recipient(p.id).unwrap().id, p.id);
    }
    for (endpoint, body) in [
        ("shuffle", json!({})),
        ("registration", json!({"open":true})),
    ] {
        assert_eq!(
            call(
                &routes,
                &format!("/api/admin/{endpoint}"),
                Some(body),
                Some(ADMIN)
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
    }
    assert_eq!(std::fs::read(&app.selection_path).unwrap(), bytes);
    for endpoint in ["/api/register", "/api/verify"] {
        let data = if endpoint.ends_with("register") {
            json!({"name":"Late", "email":"late@example.com"})
        } else {
            json!({"token":"anything"})
        };
        assert_eq!(
            call(&routes, endpoint, Some(data), None).await.0,
            StatusCode::CONFLICT
        );
    }
    for _ in 0..2 {
        let (code, data) = call(
            &routes,
            "/api/admin/send-emails",
            Some(json!({})),
            Some(ADMIN),
        )
        .await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(data["stage"], "emails_queued");
        assert_eq!(data["emails_total"], 3);
        assert!(!data.to_string().contains("@example.com"));
    }
    let jobs: Vec<_> = app
        .store
        .lock()
        .await
        .db
        .reveal_mail
        .iter()
        .map(|m| m.id)
        .collect();
    drop(routes);
    drop(app);
    let restarted = Arc::new(self::app(&dir, false));
    assert_eq!(
        restarted
            .store
            .lock()
            .await
            .db
            .reveal_mail
            .iter()
            .map(|m| m.id)
            .collect::<Vec<_>>(),
        jobs
    );
    {
        let mut store = restarted.store.lock().await;
        let mut next = store.db.clone();
        for mail in &mut next.reveal_mail {
            assert!(mail.text.contains("/reveal.html?token="));
            assert!(mail.text.contains("Test Group"));
            assert!(mail.subject.contains("Test Group"));
            mail.accepted = true;
            mail.text.clear();
        }
        store.commit(next).unwrap();
    }
    let routes = router(restarted.clone());
    assert_eq!(
        call(&routes, "/api/admin/status", None, Some(ADMIN))
            .await
            .1["stage"],
        "emails_accepted"
    );
    assert_eq!(
        call(
            &routes,
            "/api/admin/send-emails",
            Some(json!({})),
            Some(ADMIN)
        )
        .await
        .1["stage"],
        "emails_accepted"
    );
    std::fs::remove_file(&restarted.selection_path).unwrap();
    assert_eq!(
        call(
            &routes,
            "/api/admin/registration",
            Some(json!({"open":true})),
            Some(ADMIN)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}
#[tokio::test]
async fn closed_registration_can_reopen_but_corrupt_draw_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let mut instance = app(&dir, true);
    instance.config.resend_key = None;
    let app = Arc::new(instance);
    let routes = router(app.clone());
    assert_eq!(
        call(
            &routes,
            "/api/admin/registration",
            Some(json!({"open":true})),
            Some(ADMIN)
        )
        .await
        .0,
        StatusCode::OK
    );
    std::fs::write(&app.selection_path, b"broken").unwrap();
    for (endpoint, data) in [
        ("status", None),
        ("shuffle", Some(json!({}))),
        ("send-emails", Some(json!({}))),
        ("registration", Some(json!({"open":true}))),
    ] {
        assert_eq!(
            call(
                &routes,
                &format!("/api/admin/{endpoint}"),
                data,
                Some(ADMIN)
            )
            .await
            .0,
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    assert_eq!(std::fs::read(&app.selection_path).unwrap(), b"broken");
}

#[tokio::test]
async fn missing_admin_token_disables_access_and_missing_mail_config_blocks_sending() {
    let dir = tempfile::tempdir().unwrap();
    let mut instance = app(&dir, false);
    instance.config.admin_access_token = None;
    let routes = router(Arc::new(instance));
    assert_eq!(
        call(&routes, &format!("/admin.html?token={ADMIN}"), None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&routes, "/api/admin/status", None, Some(ADMIN))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    drop(routes);

    let mut instance = app(&dir, false);
    instance.config.resend_key = None;
    let draw = Selection {
        participants: (0..2)
            .map(|i| secret_santa::selection::Entry {
                id: Uuid::new_v4(),
                name: format!("Person {i}"),
                email: format!("p{i}@example.com"),
            })
            .collect(),
    };
    draw.save_new(&instance.selection_path).unwrap();
    let app = Arc::new(instance);
    let routes = router(app.clone());
    assert_eq!(
        call(
            &routes,
            "/api/admin/send-emails",
            Some(json!({})),
            Some(ADMIN)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert!(app.store.lock().await.db.reveal_mail.is_empty());
}
