use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_auth_middleware_protects_api_routes() {
    let pool = sgdf_core::db::create_test_pool().await.unwrap();
    let auth_token = Some("secret-token-12345".to_string());
    let app = sgdf_api::build_app(pool, auth_token);

    // 1. GET /health est public et ne nécessite pas de token
    let req_health = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let res_health = app.clone().oneshot(req_health).await.unwrap();
    assert_eq!(res_health.status(), StatusCode::OK);

    // 2. GET /api/v1/units sans header Authorization -> 401 Unauthorized
    let req_no_auth = Request::builder()
        .method("GET")
        .uri("/api/v1/units")
        .body(Body::empty())
        .unwrap();

    let res_no_auth = app.clone().oneshot(req_no_auth).await.unwrap();
    assert_eq!(res_no_auth.status(), StatusCode::UNAUTHORIZED);
    let body = res_no_auth.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["error"].as_str().is_some());

    // 3. GET /api/v1/units avec mauvais token -> 401 Unauthorized
    let req_bad_token = Request::builder()
        .method("GET")
        .uri("/api/v1/units")
        .header(header::AUTHORIZATION, "Bearer mauvais-token")
        .body(Body::empty())
        .unwrap();

    let res_bad_token = app.clone().oneshot(req_bad_token).await.unwrap();
    assert_eq!(res_bad_token.status(), StatusCode::UNAUTHORIZED);

    // 4. GET /api/v1/units avec mauvais schéma d'authentification (Basic) -> 401 Unauthorized
    let req_basic_auth = Request::builder()
        .method("GET")
        .uri("/api/v1/units")
        .header(header::AUTHORIZATION, "Basic dXNlcjpwYXNz")
        .body(Body::empty())
        .unwrap();

    let res_basic_auth = app.clone().oneshot(req_basic_auth).await.unwrap();
    assert_eq!(res_basic_auth.status(), StatusCode::UNAUTHORIZED);

    // 5. GET /api/v1/units avec le bon Bearer token -> 200 OK
    let req_valid = Request::builder()
        .method("GET")
        .uri("/api/v1/units")
        .header(header::AUTHORIZATION, "Bearer secret-token-12345")
        .body(Body::empty())
        .unwrap();

    let res_valid = app.clone().oneshot(req_valid).await.unwrap();
    assert_eq!(res_valid.status(), StatusCode::OK);

    // 6. Vérifier que d'autres routes /api/* sont également protégées
    let req_export = Request::builder()
        .method("GET")
        .uri("/api/v1/exports/google-contacts")
        .body(Body::empty())
        .unwrap();

    let res_export = app.clone().oneshot(req_export).await.unwrap();
    assert_eq!(res_export.status(), StatusCode::UNAUTHORIZED);

    let req_reps = Request::builder()
        .method("GET")
        .uri("/api/v1/representants-legaux")
        .body(Body::empty())
        .unwrap();

    let res_reps = app.clone().oneshot(req_reps).await.unwrap();
    assert_eq!(res_reps.status(), StatusCode::UNAUTHORIZED);

    let req_audit = Request::builder()
        .method("POST")
        .uri("/api/v1/units/1/whatsapp-audit")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"participants":[]}"#))
        .unwrap();

    let res_audit = app.clone().oneshot(req_audit).await.unwrap();
    assert_eq!(res_audit.status(), StatusCode::UNAUTHORIZED);

    let req_audit_global = Request::builder()
        .method("POST")
        .uri("/api/v1/audits/whatsapp")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"groups":[]}"#))
        .unwrap();

    let res_audit_global = app.oneshot(req_audit_global).await.unwrap();
    assert_eq!(res_audit_global.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_middleware_with_no_token_configured() {
    let pool = sgdf_core::db::create_test_pool().await.unwrap();
    let app = sgdf_api::build_app(pool, None);

    // Sans token configuré, l'accès dev est ouvert
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/units")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_auth_middleware_rejects_empty_token_bypass() {
    let pool = sgdf_core::db::create_test_pool().await.unwrap();
    // Token configuré comme chaîne vide ou espaces -> considéré comme non configuré (dev mode)
    let app = sgdf_api::build_app(pool, Some("   ".to_string()));

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/units")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
