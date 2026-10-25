# Phase 6 — Outils

Branche : `feature/outils`

## Objectif

Tester, diagnostiquer et sauvegarder.

## Livrables

- `chat [--model M] [--system S]` : conversation en streaming dans le terminal,
  historique en mémoire, commandes `/reset`, `/model`, `/exit`.
- `test [--model M] [--prompt P]` : vérifie `/health`, `/v1/models`, une complétion
  en streaming et un embedding ; mesure le temps jusqu'au premier token, la
  latence totale et les tokens/s.
- `doctor` : Docker (démon, version, Compose), droits, ports, pare-feu (ufw,
  firewalld, nftables), GPU et toolkit, espace disque, santé de chaque service,
  avec une suggestion pour chaque problème.
- `backup [--output F] [--with-models]` : archive `tar.gz` de la configuration, des
  secrets, d'un dump Postgres et des données Web UI. `restore <fichier>` : restaure
  et redémarre.

## Critères d'acceptation

- `atelier test --json` donne `ttft_ms`, `total_ms`, `tokens_per_second`.
- `atelier doctor` renvoie 0 sur une installation saine et 10 sinon.
- Une sauvegarde restaurée sur une installation vierge retrouve les clés API.
