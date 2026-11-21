use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use secret_santa::{
    App, Config, router,
    selection::{Entry, Selection},
    store::Store,
};
use std::{fs, sync::Arc};
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn reveal_resolves_cycle_without_disclosing_private_draw() {
    let dir = tempfile::tempdir().unwrap();
    let selection = Selection {
        participants: (0..3)
            .map(|i| Entry {
                id: Uuid::new_v4(),
                name: format!("Person {i}"),
                email: format!("p{i}@example.com"),
            })
            .collect(),
    };
    let path = dir.path().join("selection.json");
    fs::write(&path, serde_json::to_vec(&selection).unwrap()).unwrap();
    let mut app = App::new(
        Store::open(dir.path().join("db.json")).unwrap(),
        Config {
            group_name: "Test Group".into(),
            admin_access_token: Some("test-admin-secret-at-least-32-characters".into()),
            site_access_token: "shared-test-token".into(),
            base_url: "https://example.com".into(),
            resend_key: None,
            email_from: None,
        },
    );
    app.selection_path = path.clone();
    let routes = router(Arc::new(app));
    for i in 0..3 {
        let id = selection.participants[i].id;
        let response = routes
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/selection")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({"token":id}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
            serde_json::json!({"name":format!("Person {}",(i+1)%3)})
        );
        let response = routes
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/access-check")
                    .header("x-forwarded-uri", format!("/reveal.html?token={id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
    for token in ["bad".to_string(), Uuid::new_v4().to_string()] {
        let response = routes
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/selection")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::json!({"token":token}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    fs::write(path, b"broken JSON").unwrap();
    let response = routes
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/selection")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"token":selection.participants[0].id}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn invalid_draws_are_rejected() {
    let p = Entry {
        id: Uuid::new_v4(),
        name: "Test".into(),
        email: "test@example.com".into(),
    };
    assert!(
        Selection {
            participants: vec![p.clone()]
        }
        .validate()
        .is_err()
    );
    assert!(
        Selection {
            participants: vec![p.clone(), p]
        }
        .validate()
        .is_err()
    );
}
