# Phase 5 — Clés, configuration et HTTPS

Branche : `feature/cles-config`

## Objectif

Gérer l'accès à l'API et la configuration sans éditer de fichier à la main.

## Livrables

- `key create <nom> [--rpm N] [--tpm N]` : la clé complète n'est affichée qu'une
  fois ; `key list` montre identifiant court, nom, limites, dates, dépenses ;
  `key revoke <id>`.
- `config set <clé> <valeur>` : clés connues uniquement (port, adresse d'écoute,
  domaine, limites, Web UI…), validation des valeurs, régénération et
  redémarrage des services concernés.
- `expose --domain <domaine> [--email <email>]` : active HTTPS (ports 80/443),
  vérifie la résolution DNS et la disponibilité des ports ; `expose --off` revient
  à l'accès local.

## Critères d'acceptation

- Une requête sans clé ou avec une clé révoquée reçoit 401.
- Les limites RPM d'une clé sont appliquées (429 au-delà).
- `config set server.port abc` est refusé (code 2) sans modifier la configuration.
- Ouvrir sur `0.0.0.0` affiche un avertissement de sécurité.
- Après `expose --domain`, `https://<domaine>/health` répond.
