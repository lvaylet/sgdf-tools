use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use sgdf_core::domain::{
    GlobalWhatsappAuditResponse, UnitSummary, WhatsappAuditRequest, WhatsappAuditResponse,
};
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
async fn test_whatsapp_audit_end_to_end() {
    let (_pool, app) = setup_app_with_data().await;

    // 1. Récupérer la liste des unités pour trouver l'ID de "1ERE SCOUTS GUIDES DE LA ROCHELLE"
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/units")
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let units: Vec<UnitSummary> = serde_json::from_slice(&body).unwrap();
    let unit = units
        .iter()
        .find(|u| u.nom.contains("SCOUTS GUIDES"))
        .expect("L'unité Scouts-Guides doit exister");
    let unit_id = unit.id;

    // 2. Préparer un payload d'audit avec :
    // - Laurent Vaylet (présent, sous format 0687966975)
    // - Jaina Elléouët (cheftaine de l'unité, sous format WAHA 33661135865@c.us)
    // - Louison Ully (Compagnon, sous format 0782449488 -> intrus connu dans cette unité !)
    // - Deux numéros inconnus / obsolètes (+33699999999 et 0688888888)
    let audit_req = WhatsappAuditRequest {
        participants: vec![
            "0687966975".to_string(),
            "33661135865@c.us".to_string(),
            "0782449488".to_string(),
            "+33699999999".to_string(),
            "0688888888".to_string(),
        ],
    };

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/units/{}/whatsapp-audit", unit_id))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&audit_req).unwrap()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let audit_res: WhatsappAuditResponse = serde_json::from_slice(&body).unwrap();

    // 3. Vérifier les inconnus
    assert_eq!(audit_res.unknown.len(), 2);
    assert!(audit_res.unknown.contains(&"+33699999999".to_string()));
    assert!(audit_res.unknown.contains(&"+33688888888".to_string()));
    assert!(!audit_res.unknown.contains(&"+33687966975".to_string()));
    assert!(!audit_res.unknown.contains(&"+33661135865".to_string()));
    assert!(!audit_res.unknown.contains(&"+33782449488".to_string()));

    // 4. Vérifier les intrus connus (Louison Ully est dans les Compagnons, pas Scouts-Guides)
    assert_eq!(audit_res.unexpected.len(), 1);
    let intruder = &audit_res.unexpected[0];
    assert_eq!(intruder.nom, "ULLY");
    assert_eq!(intruder.prenom, "LOUISON");
    assert_eq!(intruder.telephone, "+33782449488");
    assert!(intruder
        .unites_legitimes
        .iter()
        .any(|u| u.contains("COMPAGNONS")));

    // 4. Vérifier les manquants
    // Laurent Vaylet est présent, donc il ne doit PAS être dans missing
    assert!(
        !audit_res
            .missing
            .iter()
            .any(|m| m.nom == "VAYLET" && m.prenom == "LAURENT"),
        "Laurent Vaylet est participant au groupe, il ne doit pas être manquant"
    );

    // Fanny Vaylet (autre représentante légale de Lisa) n'est pas dans participants, elle doit être manquante
    let fanny = audit_res
        .missing
        .iter()
        .find(|m| m.nom == "VAYLET" && m.prenom == "FANNY")
        .expect("Fanny Vaylet doit être marquée comme manquante");

    assert_eq!(fanny.telephone_mobile.as_deref(), Some("+33676079747"));
    assert!(
        fanny.jeunes.iter().any(|j| j.contains("LISA")),
        "Fanny doit être associée à sa fille Lisa : {:?}",
        fanny.jeunes
    );

    // 5. Vérifier le cas d'une unité introuvable -> 404
    let req_404 = Request::builder()
        .method("POST")
        .uri("/api/v1/units/999999/whatsapp-audit")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&audit_req).unwrap()))
        .unwrap();

    let res_404 = app.clone().oneshot(req_404).await.unwrap();
    assert_eq!(res_404.status(), StatusCode::NOT_FOUND);

    // 6. Vérifier l'audit d'une unité de Compagnons (adultes autonomes sans représentants légaux)
    let compa_unit = units
        .iter()
        .find(|u| u.nom.contains("COMPAGNONS"))
        .expect("L'unité Compagnons doit exister");

    // Louison Ully (+33782449488) est présente dans le groupe, mais pas Marguerite Fauchery (+33766393598)
    let compa_audit_req = WhatsappAuditRequest {
        participants: vec!["0782449488".to_string()],
    };

    let compa_req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/units/{}/whatsapp-audit", compa_unit.id))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&compa_audit_req).unwrap()))
        .unwrap();

    let compa_res = app.oneshot(compa_req).await.unwrap();
    assert_eq!(compa_res.status(), StatusCode::OK);
    let compa_body = compa_res.into_body().collect().await.unwrap().to_bytes();
    let compa_audit: WhatsappAuditResponse = serde_json::from_slice(&compa_body).unwrap();

    // Louison ne doit pas être manquante
    assert!(!compa_audit.missing.iter().any(|m| m.prenom == "LOUISON"));
    // Marguerite doit être manquante
    let marguerite = compa_audit
        .missing
        .iter()
        .find(|m| m.prenom == "MARGUERITE")
        .expect("Marguerite doit être marquée manquante");
    assert_eq!(marguerite.telephone_mobile.as_deref(), Some("+33766393598"));
    assert!(
        marguerite.jeunes.is_empty(),
        "Pour une unité compagnon, jeunes doit être vide"
    );
}

#[tokio::test]
async fn test_whatsapp_global_audit_end_to_end() {
    let (pool, app) = setup_app_with_data().await;

    // 1. Configurer un JID pour l'unité Compagnons
    let compa_unit: (i64,) = sqlx::query_as("SELECT id FROM unites WHERE nom LIKE '%COMPAGNONS%'")
        .fetch_one(&pool)
        .await
        .unwrap();

    let compa_jid = "120363999999999@g.us";
    sgdf_core::store::update_unit_jid(&pool, compa_unit.0, Some(compa_jid))
        .await
        .unwrap();

    let sg_unit: (i64,) = sqlx::query_as("SELECT id FROM unites WHERE nom LIKE '%SCOUTS GUIDES%'")
        .fetch_one(&pool)
        .await
        .unwrap();

    // 2. Préparer une requête globale multi-groupes :
    // - Un groupe par unit_id (Scouts Guides)
    // - Un groupe par JID (Compagnons)
    let payload = serde_json::json!({
        "groups": [
            {
                "unit_id": sg_unit.0,
                "participants": [
                    "0687966975", // Laurent Vaylet (présent)
                    "0782449488", // Louison Ully (intrus connu)
                    "+33699999999" // Inconnu
                ]
            },
            {
                "jid": compa_jid,
                "participants": [
                    "0782449488" // Louison Ully (présente)
                ]
            }
        ]
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/audits/whatsapp")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let global_audit: GlobalWhatsappAuditResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(global_audit.summary.units_auditees, 2);
    assert!(global_audit.summary.total_manquants > 0);
    assert_eq!(global_audit.summary.total_intrus_connus, 1);
    assert_eq!(global_audit.summary.total_inconnus, 1);
    assert_eq!(global_audit.units.len(), 2);
}
