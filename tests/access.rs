use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use secret_santa::{App, Config, router, store::Store};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
async fn signup_and_confirmation_use_separate_query_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let shared = "test-signup-link-secret-with-32-characters";
    let mut app = App::new(
        Store::open(dir.path().join("db.json")).unwrap(),
        Config {
            group_name: "Test Group".into(),
            admin_access_token: Some("test-admin-secret-at-least-32-characters".into()),
            site_access_token: shared.into(),
            base_url: "https://example.com".into(),
            resend_key: None,
            email_from: None,
        },
    );
    app.selection_path = dir.path().join("selection.json");
    let app = Arc::new(app);
    let routes = router(app.clone());
    let check = |uri: String| {
        Request::builder()
            .uri("/api/access-check")
            .header("x-forwarded-uri", uri)
            .header("x-forwarded-method", "GET")
            .body(Body::empty())
            .unwrap()
    };
    for path in ["/signup.html", "/api/register", "/api/participants/count"] {
        assert_eq!(
            routes
                .clone()
                .oneshot(check(path.into()))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let response = routes
            .clone()
            .oneshot(check(format!("{path}?token={shared}")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(response.headers().get("set-cookie").is_none());
    }
    for uri in [
        "//evil.example/",
        "/signup.html?token=wrong",
        "/signup.html?token=wrong&token=another",
        "/confirm.html",
        "/unknown",
    ] {
        assert_eq!(
            routes
                .clone()
                .oneshot(check(uri.into()))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let response = routes
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"name":"Test","email":"test@example.com"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let verification = {
        let store = app.store.lock().await;
        let text = &store.db.mail[0].text;
        assert!(!text.contains(shared));
        text.split("/confirm.html?token=")
            .nth(1)
            .unwrap()
            .to_string()
    };
    assert_eq!(
        routes
            .clone()
            .oneshot(check(format!("/confirm.html?token={shared}")))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        routes
            .clone()
            .oneshot(check(format!("/signup.html?token={verification}")))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        routes
            .clone()
            .oneshot(check(format!("/confirm.html?token={verification}")))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    assert!(!app.store.lock().await.db.participants[0].verified);
    let response = routes
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/verify")
                .header("content-type", "application/json")
                .body(Body::from(json!({"token":verification}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        routes
            .oneshot(check(format!("/confirm.html?token={verification}")))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn json_closure_blocks_signup_and_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("db.json")).unwrap();
    let mut next = store.db.clone();
    next.closed = true;
    store.commit(next).unwrap();
    let mut app = App::new(
        store,
        Config {
            group_name: "Test Group".into(),
            admin_access_token: Some("test-admin-secret-at-least-32-characters".into()),
            site_access_token: "signup-test-token".into(),
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
                .uri("/api/participants/count")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    use http_body_util::BodyExt;
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let data: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(data["registration_closed"], true);
    for (path, body) in [
        (
            "/api/register",
            json!({"name":"Test","email":"test@example.com"}),
        ),
        ("/api/verify", json!({"token":"anything"})),
    ] {
        let response = routes
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }
    assert!(app.store.lock().await.db.participants.is_empty());
    assert!(app.store.lock().await.db.closed);
}
