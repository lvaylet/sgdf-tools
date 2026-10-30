# SGDF Tools

Suite d'outils écrite en Rust pour faciliter la gestion administrative et opérationnelle d'un groupe scout local des Scouts et Guides de France (SGDF), en particulier pour le Responsable de Groupe (RG).

---

## 📋 Table des matières

- [Fonctionnalités](#-fonctionnalités)
- [Architecture](#-architecture)
- [Structure du dépôt](#-structure-du-dépôt)
- [Installation & Développement](#-installation--développement)
  - [Avec Nix (Recommandé)](#avec-nix-recommandé)
  - [Sans Nix](#sans-nix)
- [Configuration](#-configuration)
- [Utilisation](#-utilisation)
  - [Serveur API (`sgdf-api`)](#serveur-api-sgdf-api)
  - [Ligne de commande (`sgdf-cli`)](#ligne-de-commande-sgdf-cli)
- [Déploiement sur NixOS](#-déploiement-sur-nixos)
- [Documentation & Modèle Métier](#-documentation--modèle-métier)

---

## 🎯 Fonctionnalités

1. **Ingestion des exports Intranet SGDF** :
   - Lecture directe des fichiers d'export de l'intranet (tableaux HTML au format Windows-1252 portant l'extension `.xls`).
   - Ingestion dans une base SQLite locale persistante avec suivi des saisons scoutes (ex. `2026-2027`) et gestion des mises à jour d'état (*soft update*).

2. **Consolidation et déduplication des contacts** :
   - Extraction des jeunes adhérents rattachés à leurs unités pédagogiques (Farfadets, Louveteaux-Jeannettes, Scouts-Guides, Pionniers-Caravelles, Compagnons).
   - Déduplication automatique des représentants légaux (parents) ayant plusieurs enfants dans différentes unités ou occupant des fonctions bénévoles (parents animateurs, cadres).
   - Normalisation systématique des numéros de téléphone au format international **E.164** (ex. `+33612345678`).

3. **Synchronisation Google Contacts (via n8n)** :
   - Exposition d'un endpoint API générant des fiches prêtes à l'emploi avec noms, téléphones, courriels, filiation dans le champ note (ex. `Parent de : Lisa (Scouts-Guides), Lucie (Farfadets)`) et labels d'unités (`SGDF`, `SGDF - Scouts-Guides`).

4. **Audit différentiel WhatsApp (via WAHA & n8n)** :
   - Endpoint d'audit comparant les membres effectifs d'un groupe WhatsApp (fournis par le service WAHA) avec les représentants légaux attendus d'une unité.
   - Détection des parents manquants et des numéros obsolètes ou inconnus.

---

## 🏛 Architecture

Le système s'appuie sur une séparation stricte des responsabilités (voir [ADR-0001](docs/adr/0001-separation-of-concerns-sgdf-tools-and-n8n.md)) :
- **`sgdf-tools`** est le cœur métier et la source de vérité locale (stockage SQLite, déduplication, calculs d'écarts).
- **`n8n`** gère l'orchestration des flux tiers et l'authentification externe (OAuth2 Google Contacts, requêtes vers la passerelle WAHA).

```
                           +----------------------------------------+
                           |   Intranet SGDF (ExportIndividus.xls)   |
                           +----------------------------------------+
                                              |
                                              v (Upload / Import)
+------------------+         +----------------------------------------+
|     sgdf-cli     |  HTTP   |                sgdf-api                |
| (Clap, table/json|<------->|       (Axum, Bearer Token Auth)        |
+------------------+         +----------------------------------------+
                               |                  |
                               | (sgdf-core)      v
                               |          +-----------------+
                               |          |  SQLite (sqlx)  |
                               |          |  /var/lib/...   |
                               |          +-----------------+
                               v
                       Endpoints REST
             +-----------------+-----------------+
             |                                   |
             v                                   v
  GET /exports/google-contacts         POST /units/{id}/whatsapp-audit
             |                                   |
             +-----------------+-----------------+
                               |
                               v
                        +-------------+
                        |     n8n     |
                        +-------------+
                         /           \
                        v             v
             +-----------------+   +-----------------+
             | Google Contacts |   |   WAHA Service  |
             |   (API Google)  |   |   (WhatsApp)    |
             +-----------------+   +-----------------+
```

---

## 📦 Structure du dépôt

Ce dépôt est un **Cargo Workspace** découpé en trois crates :

- **`crates/sgdf-core`** : Bibliothèque centrale contenant les modèles de domaine (`Individu`, `Unite`, `LienParente`), le parseur d'export, la normalisation E.164 et la couche d'accès SQLite (`sqlx`).
- **`crates/sgdf-api`** : Serveur HTTP développé avec [Axum](https://github.com/tokio-rs/axum), exposant l'API REST protégée par Bearer Token.
- **`crates/sgdf-cli`** : Client en ligne de commande développé avec [Clap](https://github.com/clap-rs/clap) pour piloter l'API à distance ou en local.
- **`docs/adr/`** : Décisions d'architecture (Architecture Decision Records).
- **`GLOSSARY.md`** : Glossaire du langage métier du groupe scout.

---

## 🚀 Installation & Développement

### Avec Nix (Recommandé)

Le projet utilise un [Nix Flake](https://nixos.wiki/wiki/Flakes) garantissant un environnement de développement parfaitement reproductible.

1. **Entrer dans le shell de développement** :
   ```bash
   nix develop
   ```
   Ce shell charge automatiquement `rustc`, `cargo`, `clippy`, `rustfmt`, `rust-analyzer`, `sqlx-cli`, `sqlite`, `pkg-config` et `openssl`.

2. **Lancer les tests unitaires** :
   ```bash
   cargo test
   ```

3. **Vérifier le code avec Clippy** :
   ```bash
   cargo clippy
   ```

### Sans Nix

Assurez-vous de disposer d'une toolchain Rust récente (>= 1.80) ainsi que des bibliothèques système nécessaires :
- `pkg-config`
- `openssl` (headers de développement)
- `sqlite3`

Puis :
```bash
cargo build
cargo test
```

---

## ⚙️ Configuration

Les outils se configurent via des variables d'environnement :

| Variable | Rôle | Valeur par défaut |
|---|---|---|
| `SGDF_HOST` | Adresse d'écoute du serveur `sgdf-api` | `127.0.0.1` |
| `SGDF_PORT` | Port d'écoute du serveur `sgdf-api` | `3000` |
| `SGDF_DATABASE_URL` | Chaîne de connexion SQLite | `sqlite://sgdf.db?mode=rwc` |
| `SGDF_AUTH_TOKEN` | Jeton secret Bearer pour sécuriser l'API | *(optionnel en dev, requis en prod)* |
| `SGDF_API_URL` | URL de l'API ciblée par `sgdf-cli` | `http://127.0.0.1:3000` |

---

## 🖥 Utilisation

### Serveur API (`sgdf-api`)

Démarrage du serveur :
```bash
cargo run -p sgdf-api
```

Vérification de l'état de santé :
```bash
curl http://127.0.0.1:3000/health
# Réponse : OK
```

### Ligne de commande (`sgdf-cli`)

La CLI est un client HTTP léger qui interroge l'API (voir [ADR-0002](docs/adr/0002-cli-as-api-client.md)).

```bash
# Vérifier l'accès à l'API
cargo run -p sgdf-cli -- status

# Cibler une instance distante avec authentification
cargo run -p sgdf-cli -- --api-url https://sgdf.homelab.vaylet.fr --token "VOTRE_TOKEN" status
```

---

## ❄️ Déploiement sur NixOS

Le `flake.nix` exporte un module système NixOS prêt à l'emploi (`nixosModules.default`).

Exemple d'intégration dans votre configuration NixOS :

```nix
{ config, pkgs, inputs, ... }:

{
  imports = [
    inputs.sgdf-tools.nixosModules.default
  ];

  services.sgdf-api = {
    enable = true;
    port = 3000;
    host = "127.0.0.1";
    dataDir = "/var/lib/sgdf-tools";
    environmentFile = "/run/secrets/sgdf-tools.env"; # Contient SGDF_AUTH_TOKEN=...
  };

  # Configuration du reverse proxy (ex: Caddy ou Nginx)
  services.caddy.virtualHosts."sgdf.homelab.vaylet.fr".extraConfig = ''
    reverse_proxy 127.0.0.1:3000
  '';
}
```

---

## 📖 Documentation & Modèle Métier

- **[GLOSSARY.md](./GLOSSARY.md)** : Définitions canoniques des termes métier (*Individu*, *Adhérent*, *Jeune*, *Représentant Légal*, *Unité*, *Groupe*, *Saison*).
- **[ADR-0001 : Séparation des responsabilités entre sgdf-tools et n8n](./docs/adr/0001-separation-of-concerns-sgdf-tools-and-n8n.md)**.
- **[ADR-0002 : CLI comme client HTTP de l'API](./docs/adr/0002-cli-as-api-client.md)**.
