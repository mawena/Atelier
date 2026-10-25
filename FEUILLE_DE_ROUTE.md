# Feuille de route

Avancement du projet Atelier, phase par phase. Chaque phase a son fichier détaillé
(objectifs, livrables, critères d'acceptation) dans [docs/phases/](docs/phases/).
Les cases sont cochées au fil de l'avancement.

Versionnage : git flow (`main` = versions publiées, `develop` = intégration,
une branche `feature/*` par phase, `release/x.y.z` puis tag `vx.y.z`).

## Phase 0 — Architecture et plan · [détail](docs/phases/phase-0-architecture.md)

- [x] Document d'architecture
- [x] Arborescence du dépôt
- [x] Plan par phases avec critères d'acceptation

## Phase 1 — Socle du CLI · [détail](docs/phases/phase-1-socle-cli.md)

- [x] Projet Cargo, dépendances, profil release statique
- [x] Arbre de commandes clap complet avec aide détaillée
- [x] Options globales `--json`, `--yes`, `--no-color`
- [x] Configuration `atelier.toml` (valeurs par défaut, lecture, écriture)
- [x] Chemins surchargeables (`ATELIER_CONFIG_DIR`, `ATELIER_DATA_DIR`)
- [x] Erreurs typées et codes de retour
- [x] `atelier version`, `atelier config show`

## Phase 2 — Détection système et génération de la pile · [détail](docs/phases/phase-2-generation.md)

- [x] Détection OS, architecture, RAM, disque, CPU
- [x] Détection GPU NVIDIA (pilote, VRAM, toolkit)
- [x] Génération des secrets
- [x] Templates `compose.yml`, `litellm.yaml`, `Caddyfile`
- [x] Variantes CPU / GPU, Ollama / vLLM, Web UI, domaine

## Phase 3 — Cycle de vie de la pile · [détail](docs/phases/phase-3-cycle-de-vie.md)

- [x] Client Docker (bollard) et appel de `docker compose`
- [x] Installation de Docker et du NVIDIA Container Toolkit
- [x] `atelier install`
- [x] `atelier uninstall [--purge]`
- [x] `atelier start | stop | restart`
- [x] `atelier status`
- [x] `atelier logs [service] [-f]`
- [x] `atelier update`

## Phase 4 — Modèles · [détail](docs/phases/phase-4-modeles.md)

- [x] Catalogue des modèles recommandés
- [x] `atelier model list | search | pull | remove | default`
- [x] `atelier model recommend` selon RAM/VRAM
- [x] Synchronisation de la liste des modèles avec la passerelle

## Phase 5 — Clés, configuration et HTTPS · [détail](docs/phases/phase-5-cles-config.md)

- [x] `atelier key create | list | revoke`
- [x] `atelier config set` typé et validé, application automatique
- [x] `atelier expose --domain`

## Phase 6 — Outils · [détail](docs/phases/phase-6-outils.md)

- [x] `atelier chat`
- [x] `atelier test` (latence, tokens/s)
- [x] `atelier doctor`
- [x] `atelier backup | restore`

## Phase 7 — Distribution, CI et documentation · [détail](docs/phases/phase-7-distribution.md)

- [x] `install.sh` (installation en une ligne)
- [x] Mise à jour du CLI par `atelier update`
- [x] CI : format, lint, tests, binaires musl amd64/arm64 — validée localement (actionlint, étapes rejouées, compilation croisée zig) ; première exécution GitHub au premier push
- [x] Publication des versions (binaires + sommes de contrôle) — simulée localement (install.sh et `update` vérifiés contre un miroir, somme falsifiée refusée)
- [x] README, guide d'installation, référence CLI, référence API, tableau des modèles

## Phase 8 — Validation de bout en bout et version 0.1.0 · [détail](docs/phases/phase-8-validation.md)

- [x] Script de validation `scripts/e2e.sh`
- [x] Installation réelle de la pile (passerelle, base, proxy) sur Ubuntu 24.04, CPU
- [ ] Installation réelle avec le moteur Ollama et un modèle — bloquée : espace disque insuffisant sur la machine de test (image Ollama ≈ 7 Go)
- [x] Contrat d'API vérifié : `/health`, `/v1/models`, authentification (401), routes d'administration fermées (404)
- [ ] Contrat d'API vérifié : chat (stream et non-stream, `data: [DONE]`) et embeddings — nécessite le moteur
- [x] Clés (création, limite RPM → 429, révocation → 401), configuration, sauvegarde/restauration vérifiées
- [x] Désinstallation et purge vérifiées
- [ ] Version 0.1.0 publiée sur `main`
