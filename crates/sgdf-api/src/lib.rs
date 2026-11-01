use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, patch, post},
    Router,
};
use serde_json::json;
use sgdf_core::domain::{
    AdherentDetails, GlobalWhatsappAuditRequest, GlobalWhatsappAuditResponse,
    GlobalWhatsappAuditSummary, GoogleContactEntry, ImportSummary, Individu, JeuneRattache,
    RepresentantLegalDetail, RepresentantLegalSummary, Saison, UnitAdherentsResponse, UnitSummary,
    UnitWhatsappAuditResult, Unite, UpdateUnitPayload, WhatsappAuditIntruder, WhatsappAuditMissing,
    WhatsappAuditRequest, WhatsappAuditResponse, WhatsappGroupParticipantsInput,
};
use sqlx::SqlitePool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_swagger_ui::SwaggerUi;

pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("Secret Token")
                        .description(Some(
                            "Jeton d'authentification Bearer (ex. SGDF_AUTH_TOKEN)",
                        ))
                        .build(),
                ),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health_check,
        import_gestact_file,
        list_units,
        update_unit,
        get_unit_members,
        export_google_contacts,
        list_representants_legaux,
        audit_unit_whatsapp_handler,
        audit_global_whatsapp_handler,
    ),
    components(
        schemas(
            ImportSummary,
            Individu,
            Unite,
            UnitSummary,
            UpdateUnitPayload,
            RepresentantLegalSummary,
            JeuneRattache,
            RepresentantLegalDetail,
            AdherentDetails,
            UnitAdherentsResponse,
            GoogleContactEntry,
            WhatsappAuditRequest,
            WhatsappAuditMissing,
            WhatsappAuditIntruder,
            WhatsappAuditResponse,
            WhatsappGroupParticipantsInput,
            GlobalWhatsappAuditRequest,
            UnitWhatsappAuditResult,
            GlobalWhatsappAuditSummary,
            GlobalWhatsappAuditResponse,
            Saison,
        )
    ),
    modifiers(&SecurityAddon),
    info(
        title = "SGDF Tools API",
        version = "0.1.0",
        description = "API REST de gestion des adhérents, contacts, représentants légaux et audits WhatsApp pour groupe scout SGDF"
    )
)]
pub struct ApiDoc;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub auth_token: Option<String>,
}

pub fn build_app(pool: SqlitePool, auth_token: Option<String>) -> Router {
    let state = Arc::new(AppState { pool, auth_token });

    let api_routes = Router::new()
        .route("/v1/imports", post(import_gestact_file))
        .route("/v1/units", get(list_units))
        .route("/v1/units/{id}", patch(update_unit))
        .route("/v1/units/{id}/members", get(get_unit_members))
        .route("/v1/exports/google-contacts", get(export_google_contacts))
        .route("/v1/representants-legaux", get(list_representants_legaux))
        .route(
            "/v1/units/{id}/whatsapp-audit",
            post(audit_unit_whatsapp_handler),
        )
        .route("/v1/audits/whatsapp", post(audit_global_whatsapp_handler))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .route("/health", get(health_check))
        .nest("/api", api_routes)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() != b_bytes.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a_bytes.iter().zip(b_bytes.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, (StatusCode, Json<serde_json::Value>)> {
    if let Some(expected_token) = state.auth_token.as_ref().filter(|t| !t.trim().is_empty()) {
        let auth_header = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok());

        let is_valid = match auth_header {
            Some(header_val) => {
                if let Some(token) = header_val.strip_prefix("Bearer ") {
                    constant_time_eq(token.trim(), expected_token)
                } else {
                    false
                }
            }
            None => false,
        };

        if !is_valid {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "Authentification requise : header Authorization Bearer invalide ou manquant"
                })),
            ));
        }
    }

    Ok(next.run(req).await)
}

#[utoipa::path(
    get,
    path = "/health",
    tag = "Santé",
    responses(
        (status = 200, description = "Serveur opérationnel", body = String)
    )
)]
async fn health_check() -> &'static str {
    "OK"
}

#[utoipa::path(
    post,
    path = "/api/v1/imports",
    tag = "Imports",
    responses(
        (status = 200, description = "Synthèse de l'import Gest'Act", body = ImportSummary),
        (status = 400, description = "Fichier manquant dans le payload multipart"),
        (status = 422, description = "Format de fichier invalide"),
        (status = 401, description = "Authentification requise")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn import_gestact_file(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let mut file_bytes = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" || name == "export" {
            if let Ok(bytes) = field.bytes().await {
                file_bytes = bytes.to_vec();
                break;
            }
        }
    }

    if file_bytes.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Le champ 'file' est requis dans le payload multipart" })),
        ));
    }

    let summary = sgdf_core::ingestion::import_gestact_export(&state.pool, &file_bytes)
        .await
        .map_err(|e| match e {
            sgdf_core::ingestion::IngestionError::InvalidFormat(err) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Erreur lors du parsing du fichier : {}", err) })),
            ),
            sgdf_core::ingestion::IngestionError::Database(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Erreur de base de données : {}", err) })),
            ),
        })?;

    Ok((StatusCode::OK, Json(summary)))
}

#[utoipa::path(
    get,
    path = "/api/v1/units",
    tag = "Unités",
    responses(
        (status = 200, description = "Liste des unités avec effectifs et JID WhatsApp", body = Vec<UnitSummary>),
        (status = 401, description = "Authentification requise")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn list_units(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let units = sgdf_core::store::get_units(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Erreur de base de données : {}", e) })),
            )
        })?;

    Ok((StatusCode::OK, Json(units)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/units/{id}",
    tag = "Unités",
    params(
        ("id" = i64, Path, description = "Identifiant unique de l'unité")
    ),
    request_body = UpdateUnitPayload,
    responses(
        (status = 200, description = "Unité mise à jour", body = Unite),
        (status = 404, description = "Unité introuvable"),
        (status = 401, description = "Authentification requise")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn update_unit(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdateUnitPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let updated =
        sgdf_core::store::update_unit_jid(&state.pool, id, payload.whatsapp_group_jid.as_deref())
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": format!("Erreur de base de données : {}", e) })),
                )
            })?;

    match updated {
        Some(unit) => Ok((StatusCode::OK, Json(unit))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Unité avec l'id {} introuvable", id) })),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/units/{id}/members",
    tag = "Unités",
    params(
        ("id" = i64, Path, description = "Identifiant unique de l'unité")
    ),
    responses(
        (status = 200, description = "Adhérents et représentants légaux rattachés à l'unité", body = UnitAdherentsResponse),
        (status = 404, description = "Unité introuvable"),
        (status = 401, description = "Authentification requise")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn get_unit_members(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let details = sgdf_core::store::get_unit_adherents(&state.pool, id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Erreur de base de données : {}", e) })),
            )
        })?;

    match details {
        Some(resp) => Ok((StatusCode::OK, Json(resp))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Unité avec l'id {} introuvable", id) })),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/exports/google-contacts",
    tag = "Exports",
    responses(
        (status = 200, description = "Export normalisé des contacts pour Google Contacts", body = Vec<GoogleContactEntry>),
        (status = 401, description = "Authentification requise")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn export_google_contacts(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let contacts = sgdf_core::store::get_google_contacts_export(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Erreur de base de données : {}", e) })),
            )
        })?;

    Ok((StatusCode::OK, Json(contacts)))
}

#[utoipa::path(
    post,
    path = "/api/v1/units/{id}/whatsapp-audit",
    tag = "Audits WhatsApp",
    params(
        ("id" = i64, Path, description = "Identifiant unique de l'unité")
    ),
    request_body = WhatsappAuditRequest,
    responses(
        (status = 200, description = "Rapport différentiel d'audit WhatsApp de l'unité", body = WhatsappAuditResponse),
        (status = 404, description = "Unité introuvable"),
        (status = 401, description = "Authentification requise")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn audit_unit_whatsapp_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<WhatsappAuditRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let result = sgdf_core::store::audit_unit_whatsapp(&state.pool, id, &payload.participants)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Erreur de base de données : {}", e) })),
            )
        })?;

    match result {
        Some(resp) => Ok((StatusCode::OK, Json(resp))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Unité avec l'id {} introuvable", id) })),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/representants-legaux",
    tag = "Représentants Légaux",
    responses(
        (status = 200, description = "Listing complet des représentants légaux avec adresses et téléphones", body = Vec<RepresentantLegalDetail>),
        (status = 401, description = "Authentification requise")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn list_representants_legaux(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let reps = sgdf_core::store::get_representants_legaux(&state.pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Erreur de base de données : {}", e) })),
            )
        })?;

    Ok((StatusCode::OK, Json(reps)))
}

#[utoipa::path(
    post,
    path = "/api/v1/audits/whatsapp",
    tag = "Audits WhatsApp",
    request_body = GlobalWhatsappAuditRequest,
    responses(
        (status = 200, description = "Rapport différentiel consolidé multi-groupes", body = GlobalWhatsappAuditResponse),
        (status = 401, description = "Authentification requise")
    ),
    security(
        ("bearer_auth" = [])
    )
)]
async fn audit_global_whatsapp_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GlobalWhatsappAuditRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let result = sgdf_core::store::audit_global_whatsapp(&state.pool, payload.groups())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Erreur de base de données : {}", e) })),
            )
        })?;

    Ok((StatusCode::OK, Json(result)))
}
