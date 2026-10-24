# Phase 1 — Socle du CLI

Branche : `feature/socle-cli`

## Objectif

Un binaire `atelier` qui expose toutes les commandes (même si certaines ne sont pas
encore branchées), lit et écrit sa configuration, et respecte les conventions de
sortie et de codes de retour.

## Livrables

- Projet Cargo (`clap`, `tokio`, `bollard`, `serde`, `toml`, `minijinja`, `reqwest`
  avec rustls), profil release optimisé.
- `src/cli.rs` : toutes les commandes et sous-commandes, aide détaillée par commande
  (`atelier help <commande>`, `atelier <commande> --help`).
- Options globales : `--json`, `-y/--yes`, `--no-color` (et `NO_COLOR`).
- `src/config.rs` : structure typée, valeurs par défaut sûres (`listen = 127.0.0.1`),
  lecture/écriture atomique.
- `src/paths.rs` : `/etc/atelier` et `/var/lib/atelier`, surchargeables par variables
  d'environnement.
- `src/error.rs` : codes de retour de l'architecture.
- `atelier version` et `atelier config show` fonctionnels.

## Critères d'acceptation

- `atelier --help` liste toutes les commandes du cahier des charges.
- `atelier version --json` renvoie `{"version": ..., "commit": ..., "target": ...}`.
- `atelier config show` sans installation renvoie le code 3 avec un message clair.
- Une option inconnue renvoie le code 2.
- `cargo test` et `cargo clippy -- -D warnings` passent.
