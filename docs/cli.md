# Référence du CLI

```
atelier [OPTIONS] <COMMANDE>
```

Options globales, valables pour toutes les commandes :

| Option | Effet |
|--------|-------|
| `--json` | sortie JSON sur stdout (messages humains sur stderr) |
| `-y`, `--yes` | répond oui à toutes les confirmations |
| `--no-color` | sans couleurs (aussi via la variable `NO_COLOR`) |
| `-h`, `--help` | aide de la commande |

Aide détaillée : `atelier help <commande> [sous-commande]`.

Les commandes qui modifient le système demandent les droits root (`sudo`).
Les commandes de lecture (`status`, `logs`, `model list`…) fonctionnent pour tout
utilisateur ayant accès à Docker ; `chat` et `test` utilisent la clé maître (root)
ou la variable `ATELIER_API_KEY`.

## Codes de retour

| Code | Signification |
|------|---------------|
| 0 | succès |
| 1 | erreur générale |
| 2 | usage invalide (argument, clé de configuration, valeur) |
| 3 | Atelier n'est pas installé |
| 4 | Docker indisponible |
| 5 | droits insuffisants |
| 6 | service injoignable ou en mauvaise santé |
| 7 | ressource introuvable (modèle, clé, archive) |
| 8 | annulé par l'utilisateur |
| 10 | `doctor` a trouvé au moins un problème |

En mode `--json`, une erreur produit `{"ok": false, "error": "…", "code": N}`.

---

## install

Installe Docker si besoin, génère la configuration et démarre la pile. Idempotente.

```sh
sudo atelier install
sudo atelier install --model qwen3:8b --webui --yes
sudo atelier install --engine vllm --model Qwen/Qwen3-8B
```

Options : `--model`, `--embedding-model`, `--no-embeddings`, `--no-model`,
`--engine ollama|vllm`, `--gpu auto|nvidia|cpu`, `--webui`, `--port`, `--listen`,
`--domain`, `--email`, `--skip-docker-install`, `--key-name`.
Voir le [guide d'installation](installation.md).

## uninstall

```sh
sudo atelier uninstall                  # conteneurs supprimés, données conservées
sudo atelier uninstall --purge          # + modèles, base, configuration (confirmation)
sudo atelier uninstall --purge --images --yes
```

## start · stop · restart

```sh
sudo atelier start                      # régénère la configuration et démarre
sudo atelier stop                       # arrête (données conservées)
sudo atelier restart                    # toute la pile
sudo atelier restart litellm            # un service
```

Services : `caddy`, `litellm`, `postgres`, `ollama`, `vllm`, `open-webui`.

## status

```sh
atelier status
atelier status --json
```

Affiche l'état de chaque service (santé, mémoire, ports publiés), l'URL de l'API et
de l'interface web, l'exposition, l'accélération (GPU/CPU), la RAM et la VRAM
utilisées, le modèle par défaut et les modèles chargés en mémoire.

## logs

```sh
atelier logs                            # tous les services, 100 dernières lignes
atelier logs litellm -f                 # suivi en continu
atelier logs ollama --tail 20
```

## update

```sh
sudo atelier update                     # CLI puis images
sudo atelier update --images-only
sudo atelier update --cli-only --to 0.2.0
```

Le binaire est téléchargé depuis les versions publiées, vérifié (SHA-256) et
remplacé atomiquement. Les images dont le dépôt est celui par défaut passent aux
versions épinglées par le nouveau CLI ; les images personnalisées ne sont pas
modifiées. Modèles, clés et données sont conservés.

## model

```sh
atelier model list                      # modèles installés, alias default / default-embedding
atelier model search qwen               # catalogue (nom, famille, usage : code, embed…)
sudo atelier model pull qwen3:8b        # télécharge et expose via l'API
sudo atelier model pull mistral:7b --default
sudo atelier model remove llama3.2:3b
sudo atelier model default qwen3:8b     # alias « default » (ou « default-embedding »)
atelier model recommend                 # selon la RAM/VRAM
```

- `pull` d'un modèle présent : succès sans téléchargement.
- Le premier modèle de chat installé devient le défaut ; idem pour les embeddings.
- `remove` du modèle par défaut est refusé sans `--force`.
- Avec vLLM, `pull organisation/modèle` change le modèle servi par vLLM.

## key

```sh
sudo atelier key create android                     # limites par défaut
sudo atelier key create batch --rpm 600 --tpm 200000
sudo atelier key create demo --expires 7d
sudo atelier key list
sudo atelier key revoke 88dc28d0f030                # identifiant court ou nom
```

La clé complète n'est affichée qu'à la création. `key list` montre l'identifiant
court, le nom, un aperçu masqué, les limites, les dates de création, d'expiration et
de dernière utilisation. La clé `open-webui` est interne à l'interface web.

## config

```sh
atelier config show
atelier config get port
sudo atelier config set port 9000
sudo atelier config set webui true
sudo atelier config set listen 0.0.0.0
sudo atelier config set limits.rpm 120 --no-apply
```

`set` valide la valeur (type, plage, format), enregistre, régénère les fichiers et
redémarre les services concernés (sauf `--no-apply`).

| Clé | Alias | Défaut | Description |
|-----|-------|--------|-------------|
| `server.listen` | `listen` | `127.0.0.1` | adresse d'écoute de l'API |
| `server.port` | `port` | `8080` | port HTTP de l'API |
| `server.admin_port` | `admin_port` | `8081` | administration de la passerelle (127.0.0.1 seulement) |
| `server.domain` | `domain` | | domaine HTTPS |
| `server.email` | `email` | | e-mail Let's Encrypt |
| `engine.kind` | `engine` | `ollama` | `ollama` ou `vllm` |
| `engine.gpu` | `gpu` | `auto` | `auto`, `nvidia` ou `cpu` |
| `engine.default_model` | | | modèle derrière `default` |
| `engine.embedding_model` | | | modèle derrière `default-embedding` |
| `engine.keep_alive` | | `10m` | durée de maintien en mémoire d'un modèle inactif |
| `engine.num_parallel` | | `2` | requêtes simultanées par modèle |
| `engine.vllm_model` | | | dépôt Hugging Face servi par vLLM |
| `engine.vllm_max_model_len` | | `8192` | contexte maximal vLLM |
| `engine.vllm_gpu_memory_utilization` | | `0.9` | part de VRAM réservée par vLLM |
| `webui.enabled` | `webui` | `false` | interface Open WebUI |
| `webui.port` | | `3000` | port de l'interface |
| `webui.domain` | | | domaine HTTPS de l'interface |
| `limits.rpm` | `rpm` | `60` | requêtes/minute par défaut des nouvelles clés (0 = illimité) |
| `limits.tpm` | `tpm` | `0` | tokens/minute par défaut des nouvelles clés (0 = illimité) |
| `limits.timeout` | `timeout` | `600` | délai maximal d'une requête (s) |
| `images.<service>` | | épinglées | image Docker de chaque service |

## expose

```sh
sudo atelier expose --domain api.exemple.fr --email admin@exemple.fr
sudo atelier expose --off
```

Vérifie la résolution DNS et la disponibilité des ports 80/443, configure Caddy en
HTTPS (certificat Let's Encrypt obtenu et renouvelé automatiquement), puis attend
que `https://<domaine>/health` réponde.

## chat

```sh
sudo atelier chat
sudo atelier chat --model qwen3:8b --system "Réponds en une phrase."
ATELIER_API_KEY=sk-... atelier chat
```

Commandes : `/reset`, `/model <nom>`, `/exit` (ou Ctrl+D). Le raisonnement des
modèles qui en produisent est affiché en grisé.

## test

```sh
sudo atelier test
sudo atelier test --model qwen3:8b --json
```

Vérifie `/health`, le refus sans clé (401), `/v1/models`, une complétion en
streaming (terminée par `data: [DONE]`) et un embedding ; mesure le temps jusqu'au
premier token, la durée totale et les tokens/s.

```json
{"ok": true, "ttft_ms": 182, "total_ms": 2410, "completion_tokens": 96,
 "tokens_per_second": 42.3, "stream_done_marker": true, "embedding_dimensions": 1024}
```

## backup · restore

```sh
sudo atelier backup                                  # /var/lib/atelier/backups/atelier-<date>.tar.gz
sudo atelier backup --output /root/atelier.tar.gz --with-models
sudo atelier restore /root/atelier.tar.gz
```

L'archive (mode 0600) contient la configuration, les secrets, un export de la base
(clés, limites, journaux), les données de l'interface web et, avec `--with-models`,
les modèles. `restore` arrête la pile, restaure, réimporte la base puis redémarre.
Pour migrer vers un nouveau serveur : `install` sur le nouveau serveur, puis
`restore`.

## doctor

```sh
sudo atelier doctor
sudo atelier doctor --json
```

Vérifie : système, espace disque, Docker et Compose, configuration, GPU / pilote /
toolkit / runtime nvidia, mémoire face au modèle par défaut, ports, santé de chaque
service, passerelle, `/health`, authentification, exposition, DNS et pare-feu.
Chaque problème est accompagné d'une suggestion. Code 10 si un problème bloquant est
trouvé.

## version

```sh
atelier version
atelier version --json     # {"name","version","commit","target"}
```
