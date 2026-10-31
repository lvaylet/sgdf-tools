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
async fn test_export_google_contacts_end_to_end() {
    let (_pool, app) = setup_app_with_data().await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/exports/google-contacts")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let contacts: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

    assert!(
        contacts.len() > 50,
        "Le répertoire consolidé doit contenir des dizaines de contacts"
    );

    // 1. Laurent Vaylet : RG + Parent de Lisa et Lucie
    let laurent = contacts
        .iter()
        .find(|c| {
            c["nom"].as_str().unwrap() == "VAYLET" && c["prenom"].as_str().unwrap() == "LAURENT"
        })
        .expect("Laurent Vaylet doit être présent dans l'export");

    assert_eq!(
        laurent["telephone_mobile"].as_str().unwrap(),
        "+33687966975"
    );
    let notes = laurent["notes"].as_str().unwrap();
    assert!(
        notes.contains("LISA"),
        "Notes doit mentionner LISA : {}",
        notes
    );
    assert!(
        notes.contains("LUCIE"),
        "Notes doit mentionner LUCIE : {}",
        notes
    );

    let labels: Vec<&str> = laurent["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();

    assert!(labels.contains(&"SGDF"));
    assert!(labels.iter().any(|l| l.contains("SCOUTS GUIDES")));
    assert!(labels.iter().any(|l| l.contains("FARFADETS")));
    assert!(labels.iter().any(|l| l.contains("GROUPE AUFREDY")));

    // 2. Vérifier qu'un parent de plusieurs enfants (ex: Prou Marie ou Alexandre Kauffmann) n'apparaît qu'une seule fois
    let kauffmann_parents: Vec<_> = contacts
        .iter()
        .filter(|c| {
            c["nom"].as_str().unwrap() == "KAUFFMANN"
                && c["prenom"].as_str().unwrap() == "ALEXANDRE"
        })
        .collect();
    assert_eq!(
        kauffmann_parents.len(),
        1,
        "Alexandre Kauffmann ne doit avoir qu'une seule fiche contact"
    );
    let k_notes = kauffmann_parents[0]["notes"].as_str().unwrap();
    assert!(k_notes.contains("BASILE"));
    assert!(k_notes.contains("CELESTE"));

    // 3. Vérifier qu'une cheftaine sans enfant (ex: Jaina Elleouet) est bien incluse avec ses coordonnées directes
    let jaina = contacts
        .iter()
        .find(|c| {
            c["nom"].as_str().unwrap() == "ELLÉOUËT" || c["nom"].as_str().unwrap() == "ELLEOUET"
        })
        .expect("La cheftaine Jaina Elleouet doit être incluse");
    assert_eq!(jaina["telephone_mobile"].as_str().unwrap(), "+33661135865");
    let j_labels: Vec<&str> = jaina["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    assert!(j_labels.iter().any(|l| l.contains("SCOUTS GUIDES")));

    // 4. Vérifier qu'un Compagnon (ex: Marguerite Fauchery-Vedrenne) est bien inclus avec ses coordonnées directes
    let marguerite = contacts
        .iter()
        .find(|c| c["prenom"].as_str().unwrap() == "MARGUERITE")
        .expect("La compagnon Marguerite Fauchery doit être incluse");
    assert_eq!(
        marguerite["telephone_mobile"].as_str().unwrap(),
        "+33766393598"
    );
    let c_labels: Vec<&str> = marguerite["labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l.as_str().unwrap())
        .collect();
    assert!(c_labels.iter().any(|l| l.contains("COMPAGNONS")));
}
