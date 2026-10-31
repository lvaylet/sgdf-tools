use sgdf_api::build_app;
use sgdf_core::db::create_pool;
use std::net::SocketAddr;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let db_url = std::env::var("SGDF_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://sgdf.db?mode=rwc".to_string());
    let pool = create_pool(&db_url).await?;

    let auth_token = std::env::var("SGDF_AUTH_TOKEN").ok();

    let app = build_app(pool, auth_token);

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
