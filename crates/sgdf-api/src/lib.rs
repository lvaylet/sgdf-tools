use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, patch, post},
    Router,
};
use serde_json::json;
use sgdf_core::domain::{UpdateUnitPayload, WhatsappAuditRequest};
use sqlx::SqlitePool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

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
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
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

async fn health_check() -> &'static str {
    "OK"
}

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
