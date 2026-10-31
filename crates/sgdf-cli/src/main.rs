use clap::{Parser, Subcommand};
use comfy_table::{presets::UTF8_FULL, Table};
use reqwest::multipart::{Form, Part};
use sgdf_core::domain::{
    GoogleContactEntry, ImportSummary, UnitAdherentsResponse, UnitSummary, Unite,
};
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

    /// Consultation des unités scoutes et de leurs adhérents
    Units {
        #[command(subcommand)]
        command: UnitsCommands,
    },

    /// Exportations des données consolidées du groupe
    Export {
        #[command(subcommand)]
        command: ExportCommands,
    },
}

#[derive(Subcommand)]
enum UnitsCommands {
    /// Liste les unités du groupe avec leurs effectifs et JID WhatsApp
    List,

    /// Affiche les adhérents et représentants légaux d'une unité
    Show {
        /// Identifiant unique de l'unité
        id: i64,
    },

    /// Associe un identifiant de groupe WhatsApp (JID) à une unité
    SetJid {
        /// Identifiant de l'unité
        id: i64,
        /// JID du groupe WhatsApp (ex: 120363xxx@g.us)
        jid: String,
    },
}

#[derive(Subcommand)]
enum ExportCommands {
    /// Exporte les fiches contacts consolidées pour Google Contacts
    GoogleContacts,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let client = reqwest::Client::new();

    let send_req = |req: reqwest::RequestBuilder| {
        if let Some(token) = &cli.token {
            req.bearer_auth(token)
        } else {
            req
        }
    };

    match &cli.command {
        Commands::Status => {
            let req = send_req(client.get(format!("{}/health", cli.api_url)));
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

            let req = send_req(client.post(format!("{}/api/v1/imports", cli.api_url)).multipart(form));
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
                table.add_row(vec![
                    "Individus (adhérents & représentants légaux)",
                    &summary.individus_importes.to_string(),
                ]);
                table.add_row(vec!["Unités pédagogiques", &summary.unites_importees.to_string()]);
                table.add_row(vec!["Inscriptions saison", &summary.inscriptions_importees.to_string()]);
                table.add_row(vec![
                    "Liens de parenté enregistrés",
                    &summary.liens_parente_importes.to_string(),
                ]);
                println!("{}", table);
            }
        }
        Commands::Units { command } => match command {
            UnitsCommands::List => {
                let req = send_req(client.get(format!("{}/api/v1/units", cli.api_url)));
                let res = req.send().await?;
                if !res.status().is_success() {
                    eprintln!("Erreur lors de la récupération des unités : {}", res.status());
                    std::process::exit(1);
                }

                let units: Vec<UnitSummary> = res.json().await?;

                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&units)?);
                } else {
                    let mut table = Table::new();
                    table.load_preset(UTF8_FULL);
                    table.set_header(vec!["ID", "Code Structure", "Nom de l'Unité", "Effectif", "JID WhatsApp"]);
                    for u in units {
                        table.add_row(vec![
                            u.id.to_string(),
                            u.code_structure,
                            u.nom,
                            u.adherents_count.to_string(),
                            u.whatsapp_group_jid.unwrap_or_else(|| "-".to_string()),
                        ]);
                    }
                    println!("{}", table);
                }
            }
            UnitsCommands::Show { id } => {
                let req = send_req(client.get(format!("{}/api/v1/units/{}/members", cli.api_url, id)));
                let res = req.send().await?;
                if !res.status().is_success() {
                    eprintln!("Unité introuvable ou erreur : {}", res.status());
                    std::process::exit(1);
                }

                let details: UnitAdherentsResponse = res.json().await?;

                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&details)?);
                } else {
                    println!("=== {} (Saison {}) ===", details.unit.nom, details.saison);
                    println!(
                        "JID WhatsApp : {}",
                        details.unit.whatsapp_group_jid.as_deref().unwrap_or("Non configuré")
                    );
                    println!();

                    let mut table = Table::new();
                    table.load_preset(UTF8_FULL);
                    table.set_header(vec![
                        "Nom & Prénom",
                        "Fonction",
                        "Code Adhérent",
                        "Téléphone",
                        "Représentants Légaux",
                    ]);

                    for m in details.adherents {
                        let reps_str = if m.representants_legaux.is_empty() {
                            "-".to_string()
                        } else {
                            m.representants_legaux
                                .iter()
                                .map(|r| {
                                    format!(
                                        "{} {} ({})",
                                        r.prenom,
                                        r.nom,
                                        r.telephone_mobile.as_deref().unwrap_or("aucun tél")
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        };

                        table.add_row(vec![
                            format!("{} {}", m.nom, m.prenom),
                            m.fonction.unwrap_or_default(),
                            m.code_adherent.unwrap_or_default(),
                            m.telephone_mobile.unwrap_or_default(),
                            reps_str,
                        ]);
                    }
                    println!("{}", table);
                }
            }
            UnitsCommands::SetJid { id, jid } => {
                let payload = serde_json::json!({ "whatsapp_group_jid": jid });
                let req = send_req(
                    client
                        .patch(format!("{}/api/v1/units/{}", cli.api_url, id))
                        .json(&payload),
                );
                let res = req.send().await?;
                if !res.status().is_success() {
                    eprintln!("Échec de la mise à jour du JID : {}", res.status());
                    std::process::exit(1);
                }

                let updated: Unite = res.json().await?;
                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&updated)?);
                } else {
                    println!(
                        "JID WhatsApp configuré pour l'unité '{}' : {}",
                        updated.nom,
                        updated.whatsapp_group_jid.as_deref().unwrap_or("-")
                    );
                }
            }
        },
        Commands::Export { command } => match command {
            ExportCommands::GoogleContacts => {
                let req = send_req(client.get(format!("{}/api/v1/exports/google-contacts", cli.api_url)));
                let res = req.send().await?;
                if !res.status().is_success() {
                    eprintln!("Erreur lors de l'export des contacts : {}", res.status());
                    std::process::exit(1);
                }

                let contacts: Vec<GoogleContactEntry> = res.json().await?;

                if cli.json {
                    println!("{}", serde_json::to_string_pretty(&contacts)?);
                } else {
                    let mut table = Table::new();
                    table.load_preset(UTF8_FULL);
                    table.set_header(vec![
                        "Nom & Prénom",
                        "Mobile",
                        "Courriel",
                        "Notes (Filiation & Fonctions)",
                        "Labels",
                    ]);

                    for c in contacts {
                        table.add_row(vec![
                            format!("{} {}", c.nom, c.prenom),
                            c.telephone_mobile.unwrap_or_default(),
                            c.courriel_personnel.or(c.courriel_sgdf).unwrap_or_default(),
                            c.notes.unwrap_or_default(),
                            c.labels.join(", "),
                        ]);
                    }
                    println!("{}", table);
                }
            }
        },
    }

    Ok(())
}
