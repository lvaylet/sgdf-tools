use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sgdf")]
#[command(about = "CLI de gestion pour le groupe scout SGDF", long_about = None)]
struct Cli {
    #[arg(long, env = "SGDF_API_URL", default_value = "http://127.0.0.1:3000")]
    api_url: String,

    #[arg(long, env = "SGDF_AUTH_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Vérifie l'état de santé de l'API
    Status,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Status => {
            let client = reqwest::Client::new();
            let res = client
                .get(format!("{}/health", cli.api_url))
                .send()
                .await?;

            if res.status().is_success() {
                println!("API disponible sur {} (statut : {})", cli.api_url, res.status());
            } else {
                eprintln!("Erreur lors de l'appel API : {}", res.status());
            }
        }
    }

    Ok(())
}
