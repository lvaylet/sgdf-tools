use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde_json::json;
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
