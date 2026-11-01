use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn test_swagger_ui_and_openapi_spec_accessible() {
    let pool = sgdf_core::db::create_test_pool().await.unwrap();
    // Test avec token d'authentification pour prouver que Swagger et OpenAPI restent publics
    let auth_token = Some("secret-token-123".to_string());
    let app = sgdf_api::build_app(pool, auth_token);

    // 1. GET /api-docs/openapi.json sans token -> 200 OK
    let req_spec = Request::builder()
        .method("GET")
        .uri("/api-docs/openapi.json")
        .body(Body::empty())
        .unwrap();

    let res_spec = app.clone().oneshot(req_spec).await.unwrap();
    assert_eq!(res_spec.status(), StatusCode::OK);

    let body_spec = res_spec.into_body().collect().await.unwrap().to_bytes();
    let spec: serde_json::Value = serde_json::from_slice(&body_spec).unwrap();

    assert_eq!(
        spec["openapi"].as_str().unwrap().chars().next().unwrap(),
        '3'
    );
    assert!(spec["info"]["title"].as_str().is_some());

    // Vérifier la présence des chemins documentés
    let paths = spec["paths"].as_object().expect("paths doit être un objet");
    assert!(paths.contains_key("/api/v1/units"));
    assert!(paths.contains_key("/api/v1/units/{id}"));
    assert!(paths.contains_key("/api/v1/units/{id}/members"));
    assert!(paths.contains_key("/api/v1/representants-legaux"));
    assert!(paths.contains_key("/api/v1/exports/google-contacts"));
    assert!(paths.contains_key("/api/v1/units/{id}/whatsapp-audit"));
    assert!(paths.contains_key("/api/v1/audits/whatsapp"));
    assert!(paths.contains_key("/health"));

    // 2. GET /swagger-ui/ sans token -> 200 OK (Swagger UI UI HTML)
    let req_ui = Request::builder()
        .method("GET")
        .uri("/swagger-ui/")
        .body(Body::empty())
        .unwrap();

    let res_ui = app.oneshot(req_ui).await.unwrap();
    assert_eq!(res_ui.status(), StatusCode::OK);
}
