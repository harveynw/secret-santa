use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use secret_santa::{App, Config, router, store::Store};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn request(router: &Router, path: &str, data: Value) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["x-robots-tag"], "noindex, nofollow");
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
#[tokio::test]
async fn signup_verification_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.json");
    let mut app = App::new(
        Store::open(&path).unwrap(),
        Config {
            group_name: "Test Group".into(),
            admin_access_token: Some("test-admin-secret-at-least-32-characters".into()),
            site_access_token: "test-signup-access-token-at-least-32-characters".into(),
            base_url: "https://example.com".into(),
            resend_key: None,
            email_from: None,
        },
    );
    app.selection_path = dir.path().join("selection.json");
    let app = Arc::new(app);
    let routes = router(app.clone());
    let response = routes
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/site-config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let config: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(config, json!({"group_name":"Test Group"}));
    for path in [
        "/api/admin/close",
        "/api/admin/draw",
        "/api/admin/retry-emails",
    ] {
        assert_eq!(
            request(&routes, path, json!({})).await.0,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        request(
            &routes,
            "/api/register",
            json!({"name":"", "email":"invalid"})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let data = json!({"name":" Person ", "email":"PERSON@example.com"});
    assert_eq!(
        request(&routes, "/api/register", data.clone()).await.0,
        StatusCode::OK
    );
    let (old_id, old_mail_id, old_token) = {
        let mut store = app.store.lock().await;
        assert!(store.db.mail[0].subject.contains("Test Group"));
        assert!(store.db.mail[0].text.contains("Test Group"));
        let old_token = store.db.mail[0]
            .text
            .split("/confirm.html?token=")
            .nth(1)
            .unwrap()
            .to_string();
        let ids = (store.db.participants[0].id, store.db.mail[0].id);
        // A previously accepted email must also be replaced when confirmation is requested again.
        let mut next = store.db.clone();
        next.mail[0].accepted = true;
        next.mail[0].text.clear();
        store.commit(next).unwrap();
        (ids.0, ids.1, old_token)
    };
    let (status, response) = request(&routes, "/api/register", data.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        response["message"]
            .as_str()
            .unwrap()
            .contains("new confirmation email")
    );
    assert_eq!(
        request(&routes, "/api/verify", json!({"token":old_token}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let token = {
        let store = app.store.lock().await;
        assert_eq!(store.db.participants.len(), 1);
        assert_eq!(store.db.mail.len(), 1);
        assert_eq!(store.db.participants[0].id, old_id);
        assert_ne!(store.db.mail[0].id, old_mail_id);
        assert!(!store.db.mail[0].accepted);
        assert_eq!(store.db.participants[0].email, "person@example.com");
        store.db.mail[0]
            .text
            .split("/confirm.html?token=")
            .nth(1)
            .unwrap()
            .to_string()
    };
    assert_eq!(
        request(&routes, "/api/verify", json!({"token":"incorrect"}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(&routes, "/api/verify", json!({"token":token}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        request(&routes, "/api/verify", json!({"token":token}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let (status, error) = request(&routes, "/api/register", data).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"], "Already signed up");
    assert!(app.store.lock().await.db.mail.is_empty());
    drop(routes);
    drop(app);
    let store = Store::open(&path).unwrap();
    assert!(store.db.participants[0].verified);
    assert!(store.db.participants[0].token_hash.is_empty());
    assert!(store.db.mail.is_empty());
}
#[test]
fn corrupt_database_fails_closed_and_second_process_is_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.json");
    let first = Store::open(&path).unwrap();
    assert!(Store::open(&path).is_err());
    drop(first);
    std::fs::write(&path, "broken").unwrap();
    assert!(Store::open(&path).is_err());
}

#[tokio::test]
async fn counts_and_verification_names_include_only_confirmed_people() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = App::new(
        Store::open(dir.path().join("db.json")).unwrap(),
        Config {
            group_name: "Test Group".into(),
            admin_access_token: Some("test-admin-secret-at-least-32-characters".into()),
            site_access_token: "test-signup-access-token-at-least-32-characters".into(),
            base_url: "https://example.com".into(),
            resend_key: None,
            email_from: None,
        },
    );
    app.selection_path = dir.path().join("selection.json");
    let app = Arc::new(app);
    let routes = router(app.clone());
    async fn counts(routes: &Router) -> Value {
        let response = routes
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/participants/count")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }
    assert_eq!(
        counts(&routes).await,
        json!({"registration_closed":false,"signup_count":0,"confirmed_count":0})
    );
    let mut tokens = Vec::new();
    for (name, email) in [
        ("Zoe", "zoe@example.com"),
        ("alice", "alice@example.com"),
        ("Pending", "pending@example.com"),
    ] {
        assert_eq!(
            request(&routes, "/api/register", json!({"name":name,"email":email}))
                .await
                .0,
            StatusCode::OK
        );
        let store = app.store.lock().await;
        tokens.push(
            store
                .db
                .mail
                .last()
                .unwrap()
                .text
                .split("/confirm.html?token=")
                .nth(1)
                .unwrap()
                .to_string(),
        );
    }
    assert_eq!(
        counts(&routes).await,
        json!({"registration_closed":false,"signup_count":3,"confirmed_count":0})
    );
    let (_, first) = request(&routes, "/api/verify", json!({"token":tokens[0]})).await;
    assert_eq!(
        first,
        json!({"verified":true,"confirmed_count":1,"confirmed_names":["Zoe"]})
    );
    let (status, second) = request(&routes, "/api/verify", json!({"token":tokens[1]})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        second,
        json!({"verified":true,"confirmed_count":2,"confirmed_names":["alice","Zoe"]})
    );
    assert_eq!(
        counts(&routes).await,
        json!({"registration_closed":false,"signup_count":3,"confirmed_count":2})
    );
    let (status, error) = request(&routes, "/api/verify", json!({"token":"invalid"})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(error.get("confirmed_names").is_none());
}
