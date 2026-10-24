# Architecture

Atelier installe et pilote, sur une machine Linux (VPS ou poste local), une pile de
briques open source éprouvées qui servent des modèles de langage existants
(Llama, Qwen, Mistral, DeepSeek, Gemma…) derrière une API compatible OpenAI.

Aucun modèle n'est entraîné, aucune brique n'est réécrite : le seul code du projet
est le CLI `atelier`, écrit en Rust, qui génère la configuration et pilote Docker.

## Vue d'ensemble

```
                Internet / LAN (seulement si ouvert explicitement)
                                  │
                                  ▼
 ┌──────────────────────────── hôte Linux ─────────────────────────────┐
 │                                                                     │
 │   127.0.0.1:8080 (défaut)  ou  :80/:443 (atelier expose --domain)   │
 │                                  │                                  │
 │                        ┌─────────▼─────────┐                        │
 │  réseau "edge"         │   caddy (edge)    │  /health, /v1/*        │
 │                        │  HTTPS Let's Enc. │  tout le reste → 404   │
 │                        └───┬───────────┬───┘                        │
 │                            │           │ (port Web UI, optionnel)   │
 │ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│─ ─ ─ ─ ─ ─│─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│
 │  réseau "internal"         ▼           ▼        (internal: true)    │
 │                     ┌──────────┐  ┌──────────┐                      │
 │  127.0.0.1:4000 ───►│ litellm  │◄─┤ open-   │                      │
 │  (admin, CLI only)  │ passerelle│  │ webui   │                      │
 │                     └──┬────┬──┘  └──────────┘                      │
 │                        │    │                                       │
 │                ┌───────▼┐  ┌▼─────────┐                             │
 │                │postgres│  │ ollama   │  ou  vllm (GPU puissant)    │
 │                │ clés,  │  │ moteur   │  GPU NVIDIA si détecté,     │
 │                │ journaux│ │ GGUF     │  repli CPU sinon            │
 │                └────────┘  └──────────┘                             │
 │                                                                     │
 │   /etc/atelier       configuration générée + secrets (0600)         │
 │   /var/lib/atelier   modèles, base, données Web UI, certificats     │
 └─────────────────────────────────────────────────────────────────────┘
```

## Rôle de chaque brique

| Service    | Image (épinglée)                                  | Rôle |
|------------|---------------------------------------------------|------|
| `caddy`    | `caddy:2.11.7-alpine`                             | Seul point d'entrée réseau. Route `/health` et `/v1/*` vers la passerelle, refuse le reste. HTTPS automatique (Let's Encrypt) quand un domaine est configuré. |
| `litellm`  | `ghcr.io/berriai/litellm-database:main-v1.83.14-stable` | Passerelle compatible OpenAI : clés API virtuelles, limites de débit (RPM/TPM), journaux de requêtes. |
| `postgres` | `postgres:16.15-alpine`                           | Stockage des clés, limites et journaux de LiteLLM. |
| `ollama`   | `ollama/ollama:0.40.10`                           | Moteur par défaut, modèles GGUF quantifiés, GPU ou CPU. |
| `vllm`     | `vllm/vllm-openai:v0.32.0`                        | Moteur optionnel pour GPU puissants (un modèle Hugging Face par instance). |
| `open-webui` | `ghcr.io/open-webui/open-webui:v0.11.4`         | Interface web optionnelle ; parle à la passerelle avec sa propre clé. |

Les versions sont épinglées dans la configuration (`[images]`) et mises à jour par
`atelier update`.

## Flux d'une requête

1. Le client (application Android, curl…) appelle `POST /v1/chat/completions` avec
   `Authorization: Bearer <clé>`.
2. Caddy accepte uniquement `/health` et `/v1/*` et transmet à LiteLLM (flux SSE
   transmis sans mise en tampon).
3. LiteLLM vérifie la clé et ses limites dans Postgres, journalise, puis appelle le
   moteur sur le réseau interne (`ollama_chat/<modèle>` ou `hosted_vllm/<modèle>`).
4. La réponse revient au client ; en streaming, elle se termine par `data: [DONE]`.

`GET /health` est servi par Caddy : il interroge `litellm /health/readiness` et
répond `{"status":"ok","version":"<version atelier>"}` (200) ou
`{"status":"degraded",...}` (503). Pas d'authentification sur cette route.

## Sécurité par défaut

- Caddy publie sur `127.0.0.1` tant que `listen` n'est pas changé explicitement
  (`atelier config set server.listen 0.0.0.0`) ou qu'un domaine n'est pas activé.
- Le moteur (Ollama/vLLM), Postgres et Open WebUI sont sur un réseau Docker
  `internal: true` : aucun port publié, aucun accès sortant direct.
- L'API d'administration de LiteLLM n'est publiée que sur `127.0.0.1:<admin_port>`
  (utilisée par le CLI pour les clés) ; Caddy ne la relaie jamais.
- Chaque requête `/v1/*` exige une clé. La clé maître et les mots de passe sont
  générés à l'installation dans `/etc/atelier/secrets.env` (mode 0600).
- Une clé créée n'est affichée qu'une fois ; seule son empreinte est stockée.

## Le CLI `atelier`

Rust, binaire statique unique (musl, amd64 et arm64).

| Module            | Responsabilité |
|-------------------|----------------|
| `cli.rs`          | Définition clap des commandes, options globales `--json`, `--yes`, `--no-color`. |
| `config.rs`       | `atelier.toml` (serde/toml), valeurs par défaut, `config set` typé et validé. |
| `paths.rs`        | Emplacements (`/etc/atelier`, `/var/lib/atelier`), surchargeables par `ATELIER_CONFIG_DIR` / `ATELIER_DATA_DIR`. |
| `secrets.rs`      | Génération et lecture de `secrets.env`. |
| `render.rs`       | Rendu des templates (minijinja) : `compose.yml`, `litellm.yaml`, `Caddyfile`. |
| `system.rs`       | Détection OS, architecture, RAM, disque, CPU, GPU NVIDIA, ports. |
| `docker.rs`       | Accès Docker via bollard (ping, conteneurs, stats, logs, exec) et appel de `docker compose`. |
| `provision.rs`    | Installation de Docker et du NVIDIA Container Toolkit selon la distribution. |
| `catalog.rs`      | Catalogue des modèles recommandés et logique `recommend`. |
| `engine.rs`       | Opérations sur le moteur (liste, pull, suppression, modèles chargés). |
| `gateway.rs`      | Client HTTP de la passerelle (clés, santé, chat, embeddings). |
| `commands/*.rs`   | Une implémentation par commande. |
| `output.rs`       | Sorties colorées ou JSON, tableaux, confirmations. |
| `error.rs`        | Erreurs typées et codes de retour. |

Seuls `docker compose` (orchestration) et les commandes d'installation système sont
appelés en sous-processus ; tout le reste passe par l'API Docker (bollard) et HTTP.

### Codes de retour

| Code | Signification |
|------|---------------|
| 0  | Succès |
| 1  | Erreur générale |
| 2  | Usage invalide (arguments, clé de configuration inconnue) |
| 3  | Atelier n'est pas installé |
| 4  | Docker indisponible |
| 5  | Droits insuffisants (relancer avec `sudo`) |
| 6  | Service injoignable ou en mauvaise santé |
| 7  | Ressource introuvable (modèle, clé) |
| 8  | Opération annulée par l'utilisateur |
| 10 | `doctor` a trouvé au moins un problème |

### Idempotence

- `install` relancé conserve configuration, secrets, modèles et clés ; il ne
  régénère que les fichiers dérivés et redémarre ce qui a changé.
- `start` sur une pile démarrée, `stop` sur une pile arrêtée, `model pull` d'un
  modèle présent, `uninstall` sans installation : succès sans effet.

## Emplacements

```
/etc/atelier/
├── atelier.toml        configuration source (éditée par `atelier config set`)
├── secrets.env         clé maître, mots de passe (0600)
├── compose.yml         généré
├── litellm.yaml        généré (liste des modèles)
└── Caddyfile           généré
/var/lib/atelier/
├── ollama/             modèles Ollama
├── huggingface/        cache vLLM
├── postgres/           base LiteLLM
├── webui/              données Open WebUI
├── caddy/              certificats et état Caddy
└── backups/            archives `atelier backup`
```

Les fichiers générés portent un en-tête « généré par atelier, ne pas modifier ».
