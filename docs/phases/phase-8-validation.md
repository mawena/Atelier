# Phase 8 — Validation de bout en bout et version 0.1.0

Branche : `release/0.1.0`

## Objectif

Vérifier l'ensemble sur une vraie machine et publier la première version.

## Livrables

- `scripts/e2e.sh` : installe avec un petit modèle, vérifie le contrat d'API,
  teste clés, logs, status, backup, puis désinstalle avec purge.
- `CHANGELOG.md`, tag `v0.1.0` sur `main`.

## Critères d'acceptation

- `GET /health` → `{"status":"ok","version":"0.1.0"}`.
- `GET /v1/models` sans clé → 401 ; avec clé → liste des modèles.
- `POST /v1/chat/completions` en `stream: false` et `stream: true` (flux terminé
  par `data: [DONE]`).
- `POST /v1/embeddings` renvoie un vecteur.
- Après `uninstall --purge --yes`, plus aucun conteneur, réseau ni répertoire
  Atelier ne subsiste.
