use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sqlx::Row;
use tower::ServiceExt;

#[tokio::test]
async fn test_import_gestact_xls_end_to_end() {
    let pool = sgdf_core::db::create_test_pool().await.unwrap();
    let app = sgdf_api::build_app(pool.clone(), None);

    let file_bytes = tokio::fs::read("../../assets/ExportIndividus.xls")
        .await
        .expect("assets/ExportIndividus.xls doit exister");

    let boundary = "------------------------boundary123456789";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"ExportIndividus.xls\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/vnd.ms-excel\r\n\r\n");
    body.extend_from_slice(&file_bytes);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/imports")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res_body = res.into_body().collect().await.unwrap().to_bytes();
    let summary: serde_json::Value = serde_json::from_slice(&res_body).unwrap();

    let total_individus = summary["individus_importes"].as_i64().unwrap();
    let total_unites = summary["unites_importees"].as_i64().unwrap();
    let total_inscriptions = summary["inscriptions_importees"].as_i64().unwrap();
    let total_liens = summary["liens_parente_importes"].as_i64().unwrap();

    assert_eq!(total_unites, 7, "Il doit y avoir exactement 7 unités");
    assert_eq!(
        total_inscriptions, 112,
        "L'export contient 112 lignes d'inscriptions"
    );
    assert!(
        total_individus > 150,
        "Le total d'individus doit inclure adhérents et parents dédupliqués"
    );
    assert!(
        total_liens > 100,
        "Des liens de parenté doivent être enregistrés"
    );

    // Vérifier la déduplication de Laurent Vaylet (présent comme Responsable de Groupe ET comme parent de Lisa et Lucie)
    let laurent_rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT id, code_adherent, telephone_mobile, courriel_personnel FROM individus WHERE UPPER(nom) = 'VAYLET' AND UPPER(prenom) = 'LAURENT'"
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(
        laurent_rows.len(),
        1,
        "Laurent Vaylet doit être dédupliqué en un seul enregistrement Individu"
    );
    let laurent = &laurent_rows[0];
    assert_eq!(
        laurent.get::<Option<String>, _>("code_adherent").as_deref(),
        Some("164620934")
    );
    assert_eq!(
        laurent
            .get::<Option<String>, _>("telephone_mobile")
            .as_deref(),
        Some("+33687966975")
    );

    // Vérifier les liens de parenté pour Lisa Vaylet
    let lisa: sqlx::sqlite::SqliteRow = sqlx::query(
        "SELECT id FROM individus WHERE UPPER(nom) = 'VAYLET' AND UPPER(prenom) = 'LISA'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let lisa_id: i64 = lisa.get("id");

    let parents_lisa: Vec<String> = sqlx::query(
        "SELECT i.prenom FROM liens_parente lp
         JOIN individus i ON i.id = lp.parent_id
         WHERE lp.jeune_id = ?
         ORDER BY lp.ordre",
    )
    .bind(lisa_id)
    .fetch_all(&pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.get("prenom"))
    .collect();

    assert_eq!(parents_lisa, vec!["FANNY", "LAURENT"]);
}
