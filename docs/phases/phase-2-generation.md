# Phase 2 — Détection système et génération de la pile

Branche : `feature/generation`

## Objectif

Produire, à partir de `atelier.toml`, des secrets et du matériel détecté, des
fichiers Compose, LiteLLM et Caddy corrects et sûrs.

## Livrables

- `src/system.rs` : OS (`/etc/os-release`), architecture, RAM totale/disponible,
  disque libre, CPU, GPU NVIDIA (`nvidia-smi`), présence du NVIDIA Container
  Toolkit, ports occupés.
- `src/secrets.rs` : clé maître, mot de passe Postgres, secret et clé Open WebUI ;
  générés une seule fois, fichier en 0600.
- `templates/*.j2` et `src/render.rs` :
  - `compose.yml` : réseaux `edge` et `internal` (`internal: true`), volumes en
    bind mount sous `/var/lib/atelier`, healthchecks, réservation GPU si détecté,
    services optionnels (vLLM, Open WebUI).
  - `litellm.yaml` : un modèle par modèle installé, alias `default`, embeddings,
    clé maître et base lues depuis l'environnement.
  - `Caddyfile` : `/health` au format du contrat, `/v1/*` vers LiteLLM, 404 sinon,
    HTTPS automatique si domaine.

## Critères d'acceptation

- Tests unitaires de rendu pour chaque variante (CPU, GPU, vLLM, Web UI, domaine).
- Aucun port du moteur, de Postgres ou d'Open WebUI n'est publié dans le Compose.
- Sans domaine ni changement de `listen`, tous les ports publiés sont liés à
  `127.0.0.1`.
- `docker compose -f compose.yml config` valide le fichier généré.
- Relancer la génération ne modifie pas les secrets existants.
