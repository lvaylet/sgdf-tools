# Affiche la liste des recettes disponibles
default:
    @just --list

# Compile l'ensemble des crates du workspace
build:
    cargo build --workspace

# Lance la suite complète de tests unitaires et d'intégration
test:
    cargo test --workspace

# Vérifie rapidement la compilation et les types
check:
    cargo check --workspace

# Analyse statique avec Clippy (warnings traités comme erreurs)
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Formate le code source avec rustfmt
fmt:
    cargo fmt --all

# Vérifie le formatage sans modifier les fichiers
fmt-check:
    cargo fmt --all -- --check

# Exécute l'ensemble des vérifications qualité (formatage, lint, tests)
verify: fmt-check lint test

# Démarre le serveur API en local avec une base de dev SQLite
run-api:
    SGDF_PORT=3000 SGDF_DATABASE_URL="sqlite://sgdf.db?mode=rwc" cargo run -p sgdf-api

# Exécute la CLI sgdf-cli avec les arguments fournis
cli *args:
    cargo run -p sgdf-cli -- {{args}}

# Importe un fichier d'export Gest'Act vers l'instance locale
import file="assets/ExportIndividus.xls":
    cargo run -p sgdf-cli -- import {{file}}
