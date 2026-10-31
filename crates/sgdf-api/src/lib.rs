use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, patch, post},
    Router,
};
use serde_json::json;
use sgdf_core::domain::UpdateUnitPayload;
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

    Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/imports", post(import_gestact_file))
        .route("/api/v1/units", get(list_units))
        .route("/api/v1/units/{id}", patch(update_unit))
        .route("/api/v1/units/{id}/members", get(get_unit_members))
        .route("/api/v1/exports/google-contacts", get(export_google_contacts))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
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

    let parsed_rows = sgdf_core::parser::parse_gestact_export(&file_bytes).map_err(|e| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": format!("Erreur lors du parsing du fichier : {}", e) })),
        )
    })?;

    let summary = sgdf_core::store::import_rows(&state.pool, &parsed_rows)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Erreur de base de données : {}", e) })),
            )
        })?;

    Ok((StatusCode::OK, Json(summary)))
}

async fn list_units(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    let units = sgdf_core::store::get_units(&state.pool).await.map_err(|e| {
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
    let updated = sgdf_core::store::update_unit_jid(
        &state.pool,
        id,
        payload.whatsapp_group_jid.as_deref(),
    )
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
