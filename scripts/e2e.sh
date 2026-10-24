#!/bin/sh
# Test de bout en bout sur une vraie machine avec Docker.
# Installe la pile avec de très petits modèles, vérifie le contrat d'API,
# les clés, la sauvegarde, puis désinstalle avec purge.
#
#   scripts/e2e.sh                         # binaire : target/debug/atelier
#   ATELIER_BIN=/usr/local/bin/atelier scripts/e2e.sh
#
# Variables : E2E_PORT (18080), E2E_MODEL (qwen3:0.6b), E2E_EMBED (all-minilm),
#             E2E_KEEP=1 pour ne pas désinstaller à la fin.
set -eu

BIN="${ATELIER_BIN:-target/debug/atelier}"
PORT="${E2E_PORT:-18080}"
MODEL="${E2E_MODEL:-qwen3:0.6b}"
EMBED="${E2E_EMBED:-all-minilm}"
URL="http://127.0.0.1:$PORT"

if [ -z "${ATELIER_CONFIG_DIR:-}" ] && [ "$(id -u)" -ne 0 ]; then
  WORK=$(mktemp -d)
  export ATELIER_CONFIG_DIR="$WORK/etc" ATELIER_DATA_DIR="$WORK/data"
fi

pass=0
ok() { pass=$((pass + 1)); printf '\033[32m✔\033[0m %s\n' "$*"; }
ko() { printf '\033[31m✘\033[0m %s\n' "$*" >&2; exit 1; }
check() { desc=$1; shift; if "$@"; then ok "$desc"; else ko "$desc"; fi; }

cleanup() {
  if [ -z "${E2E_KEEP:-}" ]; then
    "$BIN" uninstall --purge --yes >/dev/null 2>&1 || true
    # Fichiers créés par les conteneurs (uid différents) en mode non root.
    if [ -n "${WORK:-}" ] && [ -d "$WORK" ]; then
      docker run --rm -v "$WORK:/w" alpine:3 rm -rf /w/data /w/etc >/dev/null 2>&1 || true
      rm -rf "$WORK"
    fi
  fi
}
trap cleanup EXIT

echo "== installation"
"$BIN" install --yes --port "$PORT" --model "$MODEL" --embedding-model "$EMBED" --key-name e2e --json > /tmp/atelier-e2e-install.json
KEY=$(sed -n 's/.*"api_key": *"\([^"]*\)".*/\1/p' /tmp/atelier-e2e-install.json)
[ -n "$KEY" ] || ko "pas de clé dans la sortie de install"
ok "install (clé créée)"

echo "== idempotence"
"$BIN" install --yes --port "$PORT" --json > /tmp/atelier-e2e-install2.json
check "install relancé sans nouvelle clé" grep -q '"api_key": null' /tmp/atelier-e2e-install2.json

echo "== contrat d'API"
check "GET /health → status ok + version" sh -c "curl -fsS $URL/health | grep -q '\"status\":\"ok\",\"version\":'"
code=$(curl -s -o /dev/null -w '%{http_code}' "$URL/v1/models")
check "GET /v1/models sans clé → 401" [ "$code" = 401 ]
code=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer sk-faux" "$URL/v1/models")
check "GET /v1/models clé invalide → 401" [ "$code" = 401 ]
models=$(curl -fsS -H "Authorization: Bearer $KEY" "$URL/v1/models")
check "GET /v1/models liste default" sh -c "echo '$models' | grep -q '\"id\":\"default\"'"
check "GET /v1/models liste $MODEL" sh -c "echo '$models' | grep -q '\"id\":\"$MODEL\"'"
code=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $KEY" "$URL/key/list")
check "routes d'administration non exposées (404)" [ "$code" = 404 ]

body='{"model":"default","messages":[{"role":"user","content":"Dis bonjour."}],"max_tokens":32}'
resp=$(curl -fsS -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" -d "$body" "$URL/v1/chat/completions")
check "POST /v1/chat/completions (stream false)" sh -c "echo '$resp' | grep -q '\"choices\"'"

sbody='{"model":"default","stream":true,"messages":[{"role":"user","content":"Dis bonjour."}],"max_tokens":32}'
curl -fsSN -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" -d "$sbody" "$URL/v1/chat/completions" > /tmp/atelier-e2e-stream.txt
check "POST /v1/chat/completions (stream true) en SSE" grep -q '^data: {' /tmp/atelier-e2e-stream.txt
check "flux terminé par data: [DONE]" sh -c "grep -v '^\s*$' /tmp/atelier-e2e-stream.txt | tail -n 1 | grep -q '^data: \[DONE\]'"

ebody='{"model":"default-embedding","input":"Bonjour le monde"}'
resp=$(curl -fsS -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" -d "$ebody" "$URL/v1/embeddings")
check "POST /v1/embeddings renvoie un vecteur" sh -c "echo '$resp' | grep -q '\"embedding\":\['"

echo "== CLI"
check "status --json" sh -c "'$BIN' status --json | grep -q '\"healthy\": true'"
check "test --json" sh -c "'$BIN' test --json | grep -q '\"stream_done_marker\": true'"
check "doctor" "$BIN" doctor
check "model list" sh -c "'$BIN' model list --json | grep -q '$MODEL'"
check "model pull idempotent" "$BIN" model pull "$EMBED"
check "model remove du défaut refusé (code 2)" sh -c "'$BIN' model remove '$MODEL' >/dev/null 2>&1; [ \$? -eq 2 ]"

echo "== clés"
NEW=$("$BIN" key create e2e-temp --rpm 2 --json | sed -n 's/.*"key": *"\([^"]*\)".*/\1/p')
ID=$("$BIN" key list --json | tr '{' '\n' | grep '"name": "e2e-temp"' | sed -n 's/.*"id": *"\([^"]*\)".*/\1/p')
[ -n "$NEW" ] && [ -n "$ID" ] || ko "création de clé"
ok "key create / list"
code=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $NEW" "$URL/v1/models")
check "nouvelle clé acceptée" [ "$code" = 200 ]
# La limite RPM s'applique aux appels de génération.
last=""
for _ in 1 2 3; do
  last=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $NEW" -H "Content-Type: application/json" \
    -d '{"model":"default","messages":[{"role":"user","content":"ok"}],"max_tokens":1}' "$URL/v1/chat/completions")
done
check "limite RPM appliquée (429)" [ "$last" = 429 ]
"$BIN" key revoke "$ID" >/dev/null
code=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $NEW" "$URL/v1/models")
check "clé révoquée → 401" [ "$code" = 401 ]

echo "== configuration"
check "config set invalide refusé (code 2)" sh -c "'$BIN' config set port abc >/dev/null 2>&1; [ \$? -eq 2 ]"

echo "== sauvegarde"
"$BIN" backup --output /tmp/atelier-e2e-backup.tar.gz >/dev/null
check "backup créé" [ -s /tmp/atelier-e2e-backup.tar.gz ]
"$BIN" key revoke e2e >/dev/null
"$BIN" restore /tmp/atelier-e2e-backup.tar.gz --yes >/dev/null
code=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $KEY" "$URL/v1/models")
check "restore retrouve la clé révoquée après la sauvegarde" [ "$code" = 200 ]
rm -f /tmp/atelier-e2e-backup.tar.gz

if [ -z "${E2E_KEEP:-}" ]; then
  echo "== désinstallation"
  "$BIN" uninstall --purge --yes >/dev/null
  check "plus aucun conteneur" sh -c "[ -z \"\$(docker ps -aq --filter label=com.docker.compose.project=atelier)\" ]"
  check "plus de réseau" sh -c "! docker network ls --format '{{.Name}}' | grep -q '^atelier_'"
  check "configuration supprimée" sh -c "[ ! -e \"\$ATELIER_CONFIG_DIR/atelier.toml\" ] 2>/dev/null || [ ! -e /etc/atelier/atelier.toml ]"
fi

echo
echo "$pass vérifications réussies"
