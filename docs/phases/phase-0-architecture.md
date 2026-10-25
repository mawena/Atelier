# Phase 0 — Architecture et plan

Branche : `feature/architecture`

## Objectif

Fixer l'architecture, l'arborescence du dépôt et le découpage en phases avant
d'écrire du code.

## Livrables

- [docs/architecture.md](../architecture.md) : briques, flux, sécurité, modules du CLI,
  codes de retour, emplacements.
- [FEUILLE_DE_ROUTE.md](../../FEUILLE_DE_ROUTE.md) et un fichier par phase.
- Arborescence cible (ci-dessous).

## Arborescence du dépôt

```
atelier/
├── Cargo.toml / Cargo.lock
├── install.sh                  installation en une ligne
├── FEUILLE_DE_ROUTE.md
├── README.md
├── CHANGELOG.md
├── LICENSE
├── src/
│   ├── main.rs                 point d'entrée, codes de retour
│   ├── cli.rs                  définition clap
│   ├── config.rs               atelier.toml
│   ├── paths.rs                emplacements
│   ├── secrets.rs              secrets.env
│   ├── render.rs               rendu des templates
│   ├── system.rs               détection matérielle et OS
│   ├── docker.rs               bollard + docker compose
│   ├── provision.rs            installation Docker / NVIDIA toolkit
│   ├── catalog.rs              catalogue et recommandations
│   ├── engine.rs               opérations moteur (Ollama / vLLM)
│   ├── gateway.rs              client HTTP de la passerelle
│   ├── sse.rs                  lecture des flux SSE
│   ├── output.rs               couleurs, tableaux, JSON
│   ├── error.rs                erreurs et codes de retour
│   └── commands/               une implémentation par commande
├── templates/
│   ├── compose.yml.j2
│   ├── litellm.yaml.j2
│   └── Caddyfile.j2
├── tests/                      tests d'intégration du CLI
├── scripts/
│   └── e2e.sh                  test de bout en bout sur une vraie machine
├── docs/
│   ├── architecture.md
│   ├── installation.md
│   ├── cli.md
│   ├── api.md
│   ├── models.md
│   └── phases/
└── .github/workflows/
    ├── ci.yml                  fmt, clippy, tests, build musl
    └── release.yml             binaires et sommes de contrôle sur tag
```

## Critères d'acceptation

- Le document d'architecture décrit chaque brique, ce qui est exposé et ce qui ne
  l'est pas, et le rôle de chaque module du CLI.
- Chaque phase a des livrables et des critères d'acceptation vérifiables.
- La feuille de route liste toutes les commandes demandées.
