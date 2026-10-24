# Phase 7 — Distribution, CI et documentation

Branche : `feature/distribution`

## Objectif

Rendre le projet installable en une ligne, testé automatiquement, documenté.

## Livrables

- `install.sh` : détecte l'architecture, télécharge le binaire de la dernière
  version (ou `ATELIER_VERSION`), vérifie la somme SHA-256, installe dans
  `/usr/local/bin`, puis lance `atelier install` avec les arguments transmis.
- `atelier update` met aussi à jour le binaire (téléchargement vérifié,
  remplacement atomique).
- `.github/workflows/ci.yml` : `cargo fmt --check`, `clippy -D warnings`, tests,
  `shellcheck`, build musl amd64 et arm64.
- `.github/workflows/release.yml` : sur tag `v*`, binaires statiques et
  `SHA256SUMS` publiés dans la release.
- Documentation : README, `docs/installation.md`, `docs/cli.md`, `docs/api.md`
  (contrat et exemples curl), `docs/models.md` (tableau RAM/VRAM).

## Critères d'acceptation

- `curl -fsSL .../install.sh | sudo sh` installe et lance Atelier.
- Les binaires produits sont statiques (`file` indique « statically linked »).
- La CI passe sur `develop` et `main`.
- Chaque commande est documentée avec un exemple.
