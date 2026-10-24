# Changelog

Format : [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/), versionnage
[SemVer](https://semver.org/lang/fr/).

## [0.1.0] — non publiée

Première version.

### Ajouté

- CLI `atelier` en Rust, binaire statique musl (amd64, arm64).
- Pile Docker Compose générée : Caddy (seul point d'entrée, HTTPS Let's Encrypt),
  LiteLLM Proxy (API compatible OpenAI, clés, limites, journaux), PostgreSQL,
  Ollama (GPU NVIDIA ou CPU), vLLM optionnel, Open WebUI optionnelle.
- Contrat d'API : `GET /health` (`{status, version}`), `GET /v1/models`,
  `POST /v1/chat/completions` (streaming SSE terminé par `data: [DONE]`),
  `POST /v1/embeddings`, authentification `Bearer`.
- Alias de modèles stables `default` et `default-embedding`.
- Commandes : `install`, `uninstall [--purge]`, `start`, `stop`, `restart`,
  `status`, `logs`, `update`, `model list|search|pull|remove|default|recommend`,
  `key create|list|revoke`, `config show|get|set`, `expose`, `chat`, `test`,
  `backup`, `restore`, `doctor`, `version`, `help`.
- Installation automatique de Docker, Compose et du NVIDIA Container Toolkit
  (Debian/Ubuntu, RHEL/Fedora, openSUSE, Arch).
- Catalogue de modèles et recommandations selon la RAM/VRAM.
- Sorties colorées ou `--json`, codes de retour documentés, commandes idempotentes.
- `install.sh` (installation en une ligne, somme SHA-256 vérifiée).
- CI (format, lint, tests, binaires statiques) et publication sur tag.
- Documentation : installation, référence CLI, référence API, modèles.
