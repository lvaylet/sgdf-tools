use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn setup_app_with_data() -> (sqlx::SqlitePool, axum::Router) {
    let pool = sgdf_core::db::create_test_pool().await.unwrap();
    let file_bytes = tokio::fs::read("../../assets/ExportIndividus.xls")
        .await
        .expect("assets/ExportIndividus.xls doit exister");
    let rows = sgdf_core::parser::parse_gestact_export(&file_bytes).unwrap();
    sgdf_core::store::import_rows(&pool, &rows).await.unwrap();

    let app = sgdf_api::build_app(pool.clone(), None);
    (pool, app)
}

async fn fetch_units(app: &axum::Router) -> Vec<serde_json::Value> {
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/units")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn test_list_units() {
    let (_pool, app) = setup_app_with_data().await;

    let units = fetch_units(&app).await;

    assert_eq!(units.len(), 7, "Il doit y avoir 7 unités");
    let louveteaux = units
        .iter()
        .find(|u| u["nom"].as_str().unwrap().contains("LOUVETEAUX"))
        .expect("L'unité Louveteaux doit être présente");

    assert!(louveteaux["adherents_count"].as_i64().unwrap() > 0);
    assert!(louveteaux["whatsapp_group_jid"].is_null());
}

#[tokio::test]
async fn test_update_unit_whatsapp_jid() {
    let (_pool, app) = setup_app_with_data().await;

    let units = fetch_units(&app).await;
    let unit_id = units[0]["id"].as_i64().unwrap();

    // 1. Mettre à jour avec un JID
    let patch_payload = serde_json::json!({
        "whatsapp_group_jid": "120363000111222@g.us"
    });
    let patch_req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/units/{}", unit_id))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&patch_payload).unwrap()))
        .unwrap();

    let patch_res = app.clone().oneshot(patch_req).await.unwrap();
    assert_eq!(patch_res.status(), StatusCode::OK);

    let patch_body = patch_res.into_body().collect().await.unwrap().to_bytes();
    let updated: serde_json::Value = serde_json::from_slice(&patch_body).unwrap();
    assert_eq!(updated["whatsapp_group_jid"].as_str().unwrap(), "120363000111222@g.us");

    // 2. Mettre à jour avec un JSON vide `{}` sans échouer
    let empty_patch = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/units/{}", unit_id))
        .header("Content-Type", "application/json")
        .body(Body::from(b"{}" as &[u8]))
        .unwrap();

    let empty_res = app.clone().oneshot(empty_patch).await.unwrap();
    assert_eq!(empty_res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_get_unit_members() {
    let (_pool, app) = setup_app_with_data().await;

    let units = fetch_units(&app).await;
    let sg_unit = units
        .iter()
        .find(|u| u["nom"].as_str().unwrap().contains("SCOUTS GUIDES"))
        .unwrap();
    let unit_id = sg_unit["id"].as_i64().unwrap();

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/units/{}/members", unit_id))
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let details: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(details["unit"]["id"].as_i64().unwrap(), unit_id);
    let adherents = details["adherents"].as_array().unwrap();
    assert!(!adherents.is_empty(), "L'unité doit avoir des adhérents");

    let lisa = adherents
        .iter()
        .find(|m| m["nom"].as_str().unwrap() == "VAYLET" && m["prenom"].as_str().unwrap() == "LISA")
        .expect("Lisa Vaylet doit être dans l'unité Scouts-Guides");

    let reps = lisa["representants_legaux"].as_array().unwrap();
    assert_eq!(reps.len(), 2, "Lisa doit avoir 2 représentants légaux");
    let rep_names: Vec<&str> = reps.iter().map(|r| r["prenom"].as_str().unwrap()).collect();
    assert!(rep_names.contains(&"FANNY"));
    assert!(rep_names.contains(&"LAURENT"));
}
