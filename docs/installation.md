# Guide d'installation

## Prérequis

| | Minimum | Conseillé |
|---|---|---|
| Système | Linux amd64 ou arm64 | Ubuntu 22.04/24.04, Debian 12 |
| RAM | 4 Go (petits modèles) | 16 Go et plus |
| Disque | 10 Go libres | 50 Go et plus (les modèles pèsent 1 à 45 Go) |
| GPU | aucun | NVIDIA avec pilote installé (`nvidia-smi` fonctionne) |
| Accès | root (sudo) | |

Distributions prises en charge : Ubuntu et Debian en priorité ; Fedora, RHEL, Rocky,
AlmaLinux, CentOS, openSUSE et Arch sont gérés pour l'installation de Docker et du
NVIDIA Container Toolkit.

## Installation en une ligne

```sh
curl -fsSL https://raw.githubusercontent.com/mawena/atelier/main/install.sh | sudo sh
```

Le script :

1. détecte l'architecture et télécharge le binaire statique de la dernière version ;
2. vérifie sa somme SHA-256 ;
3. l'installe dans `/usr/local/bin/atelier` ;
4. lance `atelier install`.

Passer des options à `atelier install` :

```sh
curl -fsSL .../install.sh | sudo sh -s -- --model qwen3:8b --webui
```

Variables utiles : `ATELIER_VERSION=0.1.0` (version précise), `ATELIER_NO_SETUP=1`
(binaire seul, sans lancer l'installation).

## Ce que fait `atelier install`

1. Vérifie l'OS, l'architecture, la RAM, le disque et le GPU.
2. Installe Docker Engine et le plugin Compose s'ils sont absents
   (script officiel `get.docker.com`, ou paquets de la distribution).
3. Si un GPU NVIDIA avec pilote est présent, installe le NVIDIA Container Toolkit et
   déclare le runtime `nvidia` dans Docker. Sans GPU, tout fonctionne sur CPU.
4. Écrit la configuration dans `/etc/atelier` et génère les secrets.
5. Propose un modèle de chat adapté à la mémoire disponible (et un modèle
   d'embeddings), télécharge les images Docker, démarre la pile, télécharge les
   modèles.
6. Crée une première clé API et l'affiche **une seule fois**.

La commande est idempotente : relancée, elle conserve configuration, secrets,
modèles et clés.

### Options

| Option | Effet |
|--------|-------|
| `--model <nom>` | modèle de chat (sinon : recommandé, ou choix interactif) |
| `--embedding-model <nom>` | modèle d'embeddings |
| `--no-embeddings` | pas de modèle d'embeddings |
| `--no-model` | aucun modèle |
| `--engine ollama\|vllm` | moteur (vLLM : `--model` = dépôt Hugging Face) |
| `--gpu auto\|nvidia\|cpu` | utilisation du GPU |
| `--webui` | active Open WebUI |
| `--port <n>` | port de l'API (8080 par défaut) |
| `--listen <ip>` | adresse d'écoute (127.0.0.1 par défaut) |
| `--domain <d>` / `--email <e>` | HTTPS direct |
| `--skip-docker-install` | ne pas installer Docker |
| `--key-name <nom>` | nom de la première clé (`default`) |
| `--yes` | aucune question |

## Exemples

VPS sans GPU, API publique en HTTPS :

```sh
sudo atelier install --yes
sudo atelier expose --domain api.exemple.fr --email admin@exemple.fr
```

Poste local avec GPU et interface web :

```sh
sudo atelier install --webui
# Interface : http://127.0.0.1:3000 (le premier compte créé est administrateur)
```

Serveur GPU avec vLLM :

```sh
sudo atelier install --engine vllm --model Qwen/Qwen3-8B
# Modèle protégé : ajoutez HF_TOKEN=... dans /etc/atelier/secrets.env puis « atelier restart vllm »
```

## Ouvrir l'accès

Par défaut l'API n'est joignable que depuis la machine elle-même.

- **HTTPS avec un domaine (recommandé)** : créez un enregistrement DNS A/AAAA vers
  le serveur, ouvrez les ports 80 et 443, puis `sudo atelier expose --domain …`.
- **Réseau local en HTTP** : `sudo atelier config set listen 0.0.0.0` (le trafic,
  clés comprises, circule en clair).
- **Tunnel SSH** depuis votre poste : `ssh -L 8080:127.0.0.1:8080 serveur`.

Pare-feu : avec ufw, `sudo ufw allow 80,443/tcp`. Pensez aussi au pare-feu de
l'hébergeur.

## Mettre à jour

```sh
sudo atelier update
```

Met à jour le binaire (téléchargement vérifié) puis les images Docker, sans toucher
aux modèles, clés et données.

## Désinstaller

```sh
sudo atelier uninstall           # retire les conteneurs, garde modèles/clés/config
sudo atelier uninstall --purge   # efface tout (confirmation demandée)
sudo rm /usr/local/bin/atelier
```

## Dépannage

```sh
sudo atelier doctor        # diagnostic avec suggestions
atelier logs litellm -f    # journaux de la passerelle
atelier logs ollama        # journaux du moteur
```

| Symptôme | Piste |
|----------|-------|
| `Docker indisponible` | `sudo systemctl start docker` |
| Port 8080 occupé | `sudo atelier config set port 9000` |
| Réponses très lentes | modèle trop gros pour la machine : `atelier model recommend` |
| GPU non utilisé | `atelier doctor` (pilote, toolkit, runtime nvidia) |
| HTTPS ne répond pas | DNS, ports 80/443, `atelier logs caddy` |
