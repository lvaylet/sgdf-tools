use axum::{routing::get, Router};
use std::net::SocketAddr;
use tracing::info;

async fn health_check() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/health", get(health_check));

    let host = std::env::var("SGDF_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("SGDF_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    info!("Serveur sgdf-api démarré sur http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
