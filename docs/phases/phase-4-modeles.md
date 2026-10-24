# Phase 4 — Modèles

Branche : `feature/modeles`

## Objectif

Gérer les modèles du moteur et garder la passerelle synchronisée.

## Livrables

- `src/catalog.rs` : catalogue de modèles éprouvés (famille, taille, mémoire
  minimale, usage chat/code/raisonnement/embeddings).
- `src/engine.rs` : liste, téléchargement (progression), suppression, modèles
  chargés, capacités (chat ou embeddings).
- Commandes `model list | search <terme> | pull <nom> | remove <nom> |
  default <nom> | recommend`.
- Régénération de `litellm.yaml` et redémarrage de la passerelle après chaque
  changement de modèles.

## Critères d'acceptation

- `model pull` d'un modèle déjà présent est un succès sans téléchargement.
- Après `model pull qwen3:4b`, `GET /v1/models` liste `qwen3:4b` et `default`.
- `model remove` du modèle par défaut est refusé tant qu'un autre défaut n'est pas
  choisi (sauf `--force`).
- `model recommend` propose un modèle de chat et un modèle d'embeddings adaptés à
  la VRAM (GPU) ou à la RAM (CPU).
- Les modèles d'embeddings sont servis par `/v1/embeddings`.
- Avec vLLM, `model pull` change le modèle servi (un seul à la fois).
