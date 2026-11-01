#!/usr/bin/env bash
set -euo pipefail

# Charge le fichier .env local s'il existe
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

if [ -f "${PROJECT_ROOT}/.env" ]; then
  # shellcheck disable=SC1091
  set -a
  source "${PROJECT_ROOT}/.env"
  set +a
fi

WAHA_URL="${WAHA_URL:-https://waha.homelab.vaylet.fr}"
WAHA_API_KEY="${WAHA_API_KEY:-}"
SGDF_API_URL="${SGDF_API_URL:-http://127.0.0.1:3000}"

if [ -z "$WAHA_API_KEY" ]; then
  echo "Erreur : La variable d'environnement WAHA_API_KEY n'est pas définie." >&2
  echo "Définissez-la via 'export WAHA_API_KEY=...' ou dans le fichier .env" >&2
  exit 1
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "Erreur : 'curl' est requis mais introuvable dans le PATH." >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "Erreur : 'jq' est requis mais introuvable dans le PATH." >&2
  exit 1
fi

echo "1. Interrogation de WAHA (${WAHA_URL}) pour extraire les groupes et participants..." >&2

# WAHA renvoie l'ensemble des groupes avec .groupMetadata.participants[]
# On extrait le JID et le numéro de téléphone pour chaque participant
GROUPS_PAYLOAD=$(curl -sf -H "X-Api-Key: ${WAHA_API_KEY}" "${WAHA_URL}/api/default/groups" | jq -c '
  {
    groups: [
      .[]
      | select(.groupMetadata.id._serialized != null and (.groupMetadata.participants | length) > 0)
      | {
          jid: .groupMetadata.id._serialized,
          participants: [
            .groupMetadata.participants[].id.user
            // (.groupMetadata.participants[].id._serialized // .groupMetadata.participants[].id | split("@")[0])
          ]
        }
    ]
  }
')

TOTAL_GROUPS=$(echo "$GROUPS_PAYLOAD" | jq '.groups | length')
echo "   -> ${TOTAL_GROUPS} groupe(s) récupéré(s) depuis WAHA." >&2

echo "2. Lancement de l'audit différentiel global auprès de sgdf-tools..." >&2

# Exécute l'audit via le binaire sgdf-cli compilé ou via cargo
if command -v sgdf >/dev/null 2>&1; then
  sgdf --api-url "${SGDF_API_URL}" audit whatsapp-all --groups "$GROUPS_PAYLOAD" "$@"
else
  cargo run --quiet -p sgdf-cli -- --api-url "${SGDF_API_URL}" audit whatsapp-all --groups "$GROUPS_PAYLOAD" "$@"
fi
