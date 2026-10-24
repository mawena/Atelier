#!/bin/sh
# Installation en une ligne :
#   curl -fsSL https://raw.githubusercontent.com/mawena/atelier/main/install.sh | sudo sh
#   curl -fsSL .../install.sh | sudo sh -s -- --model qwen3:8b --webui --yes
#
# Variables :
#   ATELIER_VERSION   version à installer (défaut : la dernière)
#   ATELIER_REPO      dépôt GitHub des versions (défaut : mawena/atelier)
#   ATELIER_BIN_DIR   dossier d'installation du binaire (défaut : /usr/local/bin)
#   ATELIER_NO_SETUP  si non vide : installe le binaire sans lancer « atelier install »
set -eu

APP="atelier"
REPO="${ATELIER_REPO:-mawena/atelier}"
BIN_DIR="${ATELIER_BIN_DIR:-/usr/local/bin}"
VERSION="${ATELIER_VERSION:-}"

say() { printf '\033[36m→\033[0m %s\n' "$*"; }
die() { printf '\033[31m✘\033[0m %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Linux" ] || die "Linux uniquement."
case "$(uname -m)" in
  x86_64|amd64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) die "architecture non prise en charge : $(uname -m) (amd64 ou arm64 requis)" ;;
esac

if [ "$(id -u)" -ne 0 ]; then
  if command -v sudo >/dev/null 2>&1; then SUDO="sudo"; else die "relancez en root (sudo)."; fi
else
  SUDO=""
fi

if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL --retry 3 -o "$2" "$1"; }
  fetch_stdout() { curl -fsSL --retry 3 "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -q -O "$2" "$1"; }
  fetch_stdout() { wget -q -O - "$1"; }
else
  die "curl ou wget est requis."
fi

if [ -z "$VERSION" ]; then
  say "recherche de la dernière version"
  VERSION=$(fetch_stdout "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' | head -n 1)
  [ -n "$VERSION" ] || die "impossible de déterminer la dernière version (définissez ATELIER_VERSION)."
fi
VERSION="${VERSION#v}"

ASSET="$APP-$ARCH-unknown-linux-musl"
BASE="https://github.com/$REPO/releases/download/v$VERSION"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT INT TERM

say "téléchargement de $APP $VERSION ($ARCH)"
fetch "$BASE/$ASSET" "$TMP/$ASSET" || die "téléchargement impossible : $BASE/$ASSET"
fetch "$BASE/SHA256SUMS" "$TMP/SHA256SUMS" || die "téléchargement impossible : SHA256SUMS"

say "vérification de la somme SHA-256"
EXPECTED=$(grep " \*\{0,1\}$ASSET\$" "$TMP/SHA256SUMS" | awk '{print $1}')
[ -n "$EXPECTED" ] || die "$ASSET absent de SHA256SUMS"
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL=$(sha256sum "$TMP/$ASSET" | awk '{print $1}')
else
  ACTUAL=$(shasum -a 256 "$TMP/$ASSET" | awk '{print $1}')
fi
[ "$EXPECTED" = "$ACTUAL" ] || die "somme de contrôle invalide (attendu $EXPECTED, obtenu $ACTUAL)"

say "installation dans $BIN_DIR/$APP"
$SUDO mkdir -p "$BIN_DIR"
$SUDO install -m 0755 "$TMP/$ASSET" "$BIN_DIR/$APP"
"$BIN_DIR/$APP" version

if [ -n "${ATELIER_NO_SETUP:-}" ]; then
  say "binaire installé. Lancez « sudo $APP install » pour installer la pile."
  exit 0
fi

# Sans terminal (curl | sh), les questions sont impossibles : on répond oui.
if [ -t 0 ]; then
  exec $SUDO "$BIN_DIR/$APP" install "$@"
else
  exec $SUDO "$BIN_DIR/$APP" install --yes "$@"
fi
