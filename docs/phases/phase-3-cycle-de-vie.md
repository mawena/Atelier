# Phase 3 — Cycle de vie de la pile

Branche : `feature/cycle-de-vie`

## Objectif

Installer, démarrer, observer, mettre à jour et retirer la pile.

## Livrables

- `src/docker.rs` : ping, conteneurs du projet (label Compose), stats mémoire,
  logs (suivi en continu), exec ; `docker compose up/down/pull/restart`.
- `src/provision.rs` : installation de Docker (script officiel get.docker.com,
  paquets de la distribution en repli) et du NVIDIA Container Toolkit (apt, dnf,
  zypper), configuration du runtime.
- Commandes `install`, `uninstall [--purge]`, `start`, `stop`, `restart`, `status`,
  `logs [service] [-f]`, `update`.

## Critères d'acceptation

- `atelier install` sur une machine Ubuntu/Debian sans Docker installe Docker,
  génère la configuration, démarre la pile, propose et télécharge un modèle,
  crée une première clé affichée une seule fois, et termine par l'URL d'accès.
- Relancer `atelier install` ne perd ni clés, ni modèles, ni configuration.
- `atelier uninstall` retire les conteneurs mais garde les données ;
  `--purge` demande confirmation (sauf `--yes`) puis supprime données et config.
- `atelier status` affiche l'état de chaque conteneur, l'URL, GPU/CPU, RAM/VRAM
  utilisées et modèles chargés ; `--json` donne la même information.
- `atelier logs litellm -f` suit les journaux en continu.
- `atelier update` met à jour les images sans perte de données.
- Les commandes qui modifient le système exigent root (code 5 sinon).
