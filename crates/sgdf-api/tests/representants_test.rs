use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn setup_app_with_data() -> (sqlx::SqlitePool, axum::Router) {
    let pool = sgdf_core::db::create_test_pool().await.unwrap();
    let file_bytes = tokio::fs::read("../../assets/ExportIndividus.xls")
        .await
        .expect("assets/ExportIndividus.xls doit exister");
    sgdf_core::ingestion::import_gestact_export(&pool, &file_bytes)
        .await
        .unwrap();

    let app = sgdf_api::build_app(pool.clone(), None);
    (pool, app)
}

#[tokio::test]
async fn test_get_representants_legaux_end_to_end() {
    let (_pool, app) = setup_app_with_data().await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/representants-legaux")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let reps: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

    assert!(
        reps.len() > 30,
        "La liste doit contenir plusieurs dizaines de représentants légaux"
    );

    // 1. Vérifier la présence et les coordonnées d'un représentant légal (Laurent Vaylet)
    let laurent = reps
        .iter()
        .find(|r| {
            r["nom"].as_str().unwrap() == "VAYLET" && r["prenom"].as_str().unwrap() == "LAURENT"
        })
        .expect("Laurent Vaylet doit être présent dans la liste des représentants légaux");

    assert_eq!(
        laurent["telephone_mobile"].as_str().unwrap(),
        "+33687966975"
    );
    assert!(!laurent["adresse_ligne1"].is_null());
    assert!(!laurent["adresse_code_postal"].is_null());
    assert!(!laurent["adresse_ville"].is_null());

    let jeunes = laurent["jeunes"]
        .as_array()
        .expect("jeunes doit être un tableau");
    assert!(
        jeunes
            .iter()
            .any(|j| j["prenom"].as_str().unwrap() == "LISA"),
        "Lisa doit être rattachée"
    );
    assert!(
        jeunes
            .iter()
            .any(|j| j["prenom"].as_str().unwrap() == "LUCIE"),
        "Lucie doit être rattachée"
    );

    // 2. Vérifier la déduplication : Alexandre Kauffmann (parent de Basile et Céleste) n'apparaît qu'une fois
    let kauffmann_parents: Vec<_> = reps
        .iter()
        .filter(|r| {
            r["nom"].as_str().unwrap() == "KAUFFMANN"
                && r["prenom"].as_str().unwrap() == "ALEXANDRE"
        })
        .collect();
    assert_eq!(
        kauffmann_parents.len(),
        1,
        "Alexandre Kauffmann ne doit apparaître qu'une seule fois"
    );
    let k_jeunes = kauffmann_parents[0]["jeunes"].as_array().unwrap();
    assert!(k_jeunes
        .iter()
        .any(|j| j["prenom"].as_str().unwrap() == "BASILE"));
    assert!(k_jeunes
        .iter()
        .any(|j| j["prenom"].as_str().unwrap() == "CELESTE"));
}
