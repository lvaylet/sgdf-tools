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
  - [Audit automatisé avec WAHA (`audit-whatsapp-waha.sh`)](#audit-automatisé-avec-waha-audit-whatsapp-wahash)
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
   - Consultation et listing complet des représentants légaux avec leurs coordonnées téléphoniques (mobile et domicile) et adresses postales.

3. **Synchronisation Google Contacts (via n8n)** :
   - Exposition d'un endpoint API générant des fiches prêtes à l'emploi avec noms, téléphones, courriels, filiation dans le champ note (ex. `Parent de : Lisa (Scouts-Guides), Lucie (Farfadets)`) et labels d'unités (`SGDF`, `SGDF - Scouts-Guides`).

4. **Audit différentiel WhatsApp (via WAHA & n8n)** :
   - Comparaison des membres effectifs d'un ou plusieurs groupes WhatsApp (fournis par WAHA) avec les personnes attendues.
   - Détection des représentants légaux manquants, identification des intrus connus (membres ou parents d'autres unités) et signalement des numéros inconnus.
   - Support d'un audit ciblé par unité ou d'un audit global multi-groupes (`POST /api/v1/audits/whatsapp` et `sgdf audit whatsapp-all`).

---

## 🏛 Architecture

Le système s'appuie sur une séparation stricte des responsabilités (voir [ADR-0001](docs/adr/0001-separation-of-concerns-sgdf-tools-and-n8n.md)) :

- **`sgdf-tools`** est le cœur métier et la source de vérité locale (stockage SQLite, déduplication, calculs d'écarts).
- **`n8n`** gère l'orchestration des flux tiers et l'authentification externe (OAuth2 Google Contacts, requêtes vers la passerelle WAHA).

```text
                           +----------------------------------------+
                           |   Intranet SGDF (ExportIndividus.xls)   |
                           +----------------------------------------+
                                              |
                                              v (Upload / Import)
+--------------------+       +----------------------------------------+
|     sgdf-cli       | HTTP  |                sgdf-api                |
| (Clap, table/json) |<----->|       (Axum, Bearer Token Auth)        |
+--------------------+       +----------------------------------------+
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

### Avec Nix & direnv (Recommandé)

Le projet utilise un [Nix Flake](https://nixos.wiki/wiki/Flakes) couplé à [direnv](https://direnv.net/) et [just](https://github.com/casey/just) pour un environnement de développement sans friction :

1. **Activation automatique avec direnv** :
   Le fichier `.envrc` est configuré avec `use flake`. Dès que vous entrez dans le répertoire, votre shell charge automatiquement tous les outils (`rustc`, `cargo`, `clippy`, `rustfmt`, `rust-analyzer`, `sqlx-cli`, `sqlite`, `just`, etc.) :

   ```bash
   direnv allow
   ```

   *(Ou manuellement sans direnv via `nix develop`).*

2. **Commandes SDLC avec `just`** :
   Un `justfile` regroupe les opérations les plus courantes :

   ```bash
   just                # Affiche l'aide et la liste des recettes disponibles
   just build          # Compile l'ensemble du workspace
   just test           # Lance tous les tests unitaires et d'intégration
   just lint           # Lance clippy avec arrêt sur warning (-D warnings)
   just lint-md        # Vérifie la conformité Markdown sans modifier les fichiers
   just fmt            # Formate l'ensemble du projet (Rust + Markdown)
   just fmt-md         # Formate uniquement les fichiers Markdown avec markdownlint-cli2
   just fmt-check      # Vérifie le formatage sans modifier les fichiers (Rust + Markdown)
   just setup-hooks    # Active les hooks Git locaux (.githooks/) pour pre-commit et pre-push
   just verify         # Exécute la chaîne qualité complète (fmt-check + lint + test)
   just run-api        # Démarre l'API locale avec SQLite
   just cli status     # Exécute la CLI (ex. just cli units list)
   just import         # Raccourci pour importer assets/ExportIndividus.xls
   just audit-whatsapp # Exécute l'audit WhatsApp global en interrogeant WAHA en direct
   ```

### Sans Nix

Assurez-vous de disposer d'une toolchain Rust récente (>= 1.80) ainsi que des bibliothèques système nécessaires :

- `pkg-config`
- `openssl` (headers de développement)
- `sqlite3`
- `just` (optionnel)

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
| `WAHA_URL` | URL de la passerelle WAHA (WhatsApp HTTP API) | `https://waha.homelab.vaylet.fr` |
| `WAHA_API_KEY` | Clé d'API secrète transmise via l'en-tête `X-Api-Key` | *(optionnel en dev, requis pour WAHA)* |

Un fichier local `.env` (ignoré par Git) peut être créé à la racine du dépôt pour définir ces variables sensibles sans risquer de les exposer :

```bash
# Exemple de fichier .env local
WAHA_URL="https://waha.homelab.vaylet.fr"
WAHA_API_KEY="votre_cle_secrete_waha"
SGDF_API_URL="http://127.0.0.1:3000"
```

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

Documentation interactive OpenAPI & Swagger UI :

- **Swagger UI** : Accédez à `http://127.0.0.1:3000/swagger-ui/` dans votre navigateur pour explorer et tester interactivement l'ensemble des endpoints. Le bouton **Authorize** permet de renseigner votre jeton Bearer pour tester les routes sécurisées.
- **Spécification OpenAPI (JSON)** : Disponible publiquement sur `http://127.0.0.1:3000/api-docs/openapi.json` (importable dans Postman, Insomnia ou n8n).

### Ligne de commande (`sgdf-cli`)

La CLI est un client HTTP léger qui interroge l'API (voir [ADR-0002](docs/adr/0002-cli-as-api-client.md)).

```bash
# Vérifier l'accès à l'API
sgdf status

# Importer un export Gest'Act
sgdf import assets/ExportIndividus.xls

# Consulter les unités et configurer un JID WhatsApp
sgdf units list
sgdf units show 1
sgdf units set-jid 1 "120363028374659281@g.us"

# Consulter tous les représentants légaux avec coordonnées téléphoniques et postales
sgdf representants list
sgdf representants list --json

# Exporter les contacts normalisés pour Google Contacts
sgdf export google-contacts --json

# Réaliser un audit différentiel WhatsApp ciblé sur une unité
sgdf audit whatsapp 1 --participants "+33687966975,0609266743"
sgdf audit whatsapp 1 --participants /tmp/waha_participants.json

# Réaliser un audit différentiel WhatsApp global multi-groupes
sgdf audit whatsapp-all --groups /tmp/all_groups.json
sgdf audit whatsapp-all --groups /tmp/all_groups.json --json

# Cibler une instance distante avec authentification Bearer
sgdf --api-url https://sgdf.homelab.vaylet.fr --token "VOTRE_TOKEN" status
```

### Audit automatisé avec WAHA (`audit-whatsapp-waha.sh`)

Le script [`scripts/audit-whatsapp-waha.sh`](scripts/audit-whatsapp-waha.sh) automatise la chaîne complète en interrogeant la passerelle WAHA en direct pour extraire les participants de tous les groupes associés aux unités scoutes :

1. Il récupère en une seule requête la liste des groupes et leurs participants depuis l'API WAHA (`GET /api/default/groups`).
2. Il transmet cette cartographie en temps réel à `sgdf-tools` (`sgdf audit whatsapp-all`).
3. L'outil fait correspondre les JIDs aux unités configurées en base et génère la synthèse globale avec les 3 catégories : **manquants**, **intrus connus** (membres ou parents d'autres unités) et **inconnus**.

```bash
# Lancement via la recette just (recommandé)
just audit-whatsapp

# Format de sortie brut JSON (pour intégration ou traitement jq)
just audit-whatsapp --json

# Ou exécution directe du script bash
./scripts/audit-whatsapp-waha.sh
./scripts/audit-whatsapp-waha.sh --json
```

---

## ❄️ Packaging & Déploiement Nix / NixOS

### Compilation via Nix Flakes

Le `flake.nix` fournit des packages hermétiques compilés via `buildRustPackage` :

```bash
# Compiler l'ensemble des binaires du workspace (sgdf-api et sgdf-cli)
nix build

# Compiler spécifiquement le serveur API ou la CLI
nix build .#sgdf-api
nix build .#sgdf-cli

# Exécuter directement la CLI sans installation
nix run .#sgdf-cli -- status
```

### Module système NixOS

Le `flake.nix` exporte un module système NixOS prêt à l'emploi (`nixosModules.default`).
Il configure automatiquement un service systemd durci (`DynamicUser = true`, persistance dans `StateDirectory = "sgdf-tools"`, et application automatique des migrations SQLite au démarrage).

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
