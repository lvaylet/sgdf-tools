use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL, Table};
use reqwest::multipart::{Form, Part};
use sgdf_core::domain::ImportSummary;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sgdf")]
#[command(about = "CLI de gestion pour le groupe scout SGDF", long_about = None)]
struct Cli {
    #[arg(long, env = "SGDF_API_URL", default_value = "http://127.0.0.1:3000")]
    api_url: String,

    #[arg(long, env = "SGDF_AUTH_TOKEN")]
    token: Option<String>,

    #[arg(long, global = true, help = "Sortie brute au format JSON")]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Vérifie l'état de santé de l'API
    Status,

    /// Importe un fichier d'export intranet Gest'Act (.xls ou HTML)
    Import {
        /// Chemin du fichier à importer
        file: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Status => {
            let client = reqwest::Client::new();
            let mut req = client.get(format!("{}/health", cli.api_url));
            if let Some(token) = &cli.token {
                req = req.bearer_auth(token);
            }
            let res = req.send().await?;

            if res.status().is_success() {
                if cli.json {
                    println!("{}", serde_json::json!({ "status": "ok", "url": cli.api_url }));
                } else {
                    println!("API disponible sur {} (statut : {})", cli.api_url, res.status());
                }
            } else {
                eprintln!("Erreur lors de l'appel API : {}", res.status());
                std::process::exit(1);
            }
        }
        Commands::Import { file } => {
            if !file.exists() {
                eprintln!("Fichier introuvable : {}", file.display());
                std::process::exit(1);
            }

            let file_bytes = tokio::fs::read(file).await?;
            let filename = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("export.xls")
                .to_string();

            let form = Form::new().part(
                "file",
                Part::bytes(file_bytes)
                    .file_name(filename)
                    .mime_str("application/vnd.ms-excel")?,
            );

            let client = reqwest::Client::new();
            let mut req = client.post(format!("{}/api/v1/imports", cli.api_url)).multipart(form);
            if let Some(token) = &cli.token {
                req = req.bearer_auth(token);
            }

            let res = req.send().await?;
            if !res.status().is_success() {
                let err_text = res.text().await?;
                eprintln!("Échec de l'import : {}", err_text);
                std::process::exit(1);
            }

            let summary: ImportSummary = res.json().await?;

            if cli.json {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                let mut table = Table::new();
                table.load_preset(UTF8_FULL);
                table.set_header(vec!["Métrique", "Quantité"]);
                table.add_row(vec!["Individus (adhérents & parents)", &summary.individus_imported.to_string()]);
                table.add_row(vec!["Unités pédagogiques", &summary.unites_imported.to_string()]);
                table.add_row(vec!["Inscriptions saison", &summary.inscriptions_imported.to_string()]);
                table.add_row(vec!["Liens de parenté enregistrés", &summary.liens_parente_imported.to_string()]);
                println!("{}", table);
            }
        }
    }

    Ok(())
}
