# Atelier

Atelier installe et gère, sur vos serveurs Linux et vos machines locales, des modèles
de langage open source existants — Llama, Qwen, Mistral, DeepSeek, Gemma… — derrière
une **API compatible OpenAI**, sécurisée par clés.

Aucun modèle n'est entraîné et aucune brique n'est réécrite : Atelier assemble des
projets open source éprouvés, tout tourne dans Docker, et un seul binaire, `atelier`,
gère tout de l'installation à la désinstallation.

```
curl -fsSL https://raw.githubusercontent.com/mawena/atelier/main/install.sh | sudo sh
```

## Ce qui est installé

| Brique | Rôle |
|--------|------|
| [Ollama](https://ollama.com) | Moteur d'inférence par défaut (GPU NVIDIA ou CPU, modèles GGUF quantifiés) |
| [vLLM](https://docs.vllm.ai) | Moteur optionnel pour les GPU puissants |
| [LiteLLM Proxy](https://docs.litellm.ai) | Passerelle compatible OpenAI : clés API, limites de débit, journaux |
| [PostgreSQL](https://www.postgresql.org) | Stockage des clés et journaux de la passerelle |
| [Caddy](https://caddyserver.com) | Seul point d'entrée réseau, HTTPS Let's Encrypt automatique |
| [Open WebUI](https://openwebui.com) | Interface web de chat (optionnelle) |

Seul Caddy est exposé ; le moteur n'est jamais joignable directement.
Détails : [docs/architecture.md](docs/architecture.md).

## Démarrage rapide

```sh
sudo atelier install                  # installe Docker si besoin, choisit un modèle, crée une clé
atelier status                        # état des services, URL, mémoire, modèles chargés
sudo atelier chat                     # discuter avec le modèle par défaut dans le terminal
sudo atelier test                     # latence et tokens/s
```

L'installation se termine en affichant l'URL de l'API et une **clé affichée une seule
fois**. Premier appel :

```sh
curl http://127.0.0.1:8080/v1/chat/completions \
  -H "Authorization: Bearer $CLE" \
  -H "Content-Type: application/json" \
  -d '{"model":"default","messages":[{"role":"user","content":"Bonjour"}]}'
```

## Commandes

| Commande | Rôle |
|----------|------|
| `install` | Vérifie la machine, installe Docker / NVIDIA Container Toolkit, génère la configuration, démarre la pile, installe un modèle, crée une clé |
| `uninstall [--purge]` | Supprime la pile ; `--purge` efface aussi modèles, données et configuration |
| `start` · `stop` · `restart [service]` | Cycle de vie |
| `status` | Conteneurs, URL, GPU/CPU, RAM/VRAM, modèles chargés |
| `logs [service] [-f]` | Journaux |
| `update` | Met à jour les images et le CLI sans perte de données |
| `model list · search · pull · remove · default · recommend` | Modèles |
| `key create · list · revoke` | Clés API |
| `config show · get · set` | Configuration |
| `expose --domain <domaine>` | HTTPS avec Let's Encrypt |
| `chat` · `test` | Essais rapides |
| `backup` · `restore <fichier>` | Sauvegardes |
| `doctor` | Diagnostic avec suggestions |
| `version` · `help [commande]` | |

Toutes les commandes acceptent `--json`, `--yes` et `--no-color`.
Référence complète : [docs/cli.md](docs/cli.md).

## Documentation

- [Guide d'installation](docs/installation.md)
- [Référence du CLI](docs/cli.md)
- [Référence de l'API](docs/api.md) et exemples curl
- [Modèles recommandés selon la RAM/VRAM](docs/models.md)
- [Architecture](docs/architecture.md)
- [Feuille de route](FEUILLE_DE_ROUTE.md)

## Sécurité par défaut

- L'API écoute sur `127.0.0.1` tant que vous ne l'ouvrez pas explicitement
  (`atelier expose --domain …` ou `atelier config set listen 0.0.0.0`).
- Le moteur, la base et l'interface web ne publient aucun port.
- Toute requête `/v1/*` exige une clé ; les clés ne sont affichées qu'à leur création.
- Les secrets sont dans `/etc/atelier/secrets.env` (lisible par root uniquement).

## Développement

```sh
cargo test                     # tests unitaires et d'intégration
cargo clippy --all-targets -- -D warnings
cargo zigbuild --release --target x86_64-unknown-linux-musl    # binaire statique amd64
cargo zigbuild --release --target aarch64-unknown-linux-musl   # binaire statique arm64
scripts/e2e.sh                 # installation réelle de bout en bout (Docker requis)
```

Renommer l'outil : changer `name` dans `Cargo.toml` (le binaire, les chemins
`/etc/<nom>`, `/var/lib/<nom>` et le projet Docker suivent) ainsi que `APP` dans
`install.sh`.

Versionnage : git flow (`main`, `develop`, `feature/*`, `release/*`, tags `vX.Y.Z`).

## Licence

MIT — voir [LICENSE](LICENSE). Chaque brique et chaque modèle conserve sa propre
licence ; vérifiez celle du modèle choisi avant un usage commercial.
