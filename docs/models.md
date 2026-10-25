# Modèles recommandés selon la RAM / VRAM

`atelier model recommend` applique ce tableau à votre machine. Tout modèle de la
[bibliothèque Ollama](https://ollama.com/library) peut aussi être installé avec
`atelier model pull <nom>`.

## Comment lire ce tableau

- **Mémoire** : avec un GPU NVIDIA, c'est la **VRAM** qui compte ; sans GPU, la
  **RAM** (la pile elle-même consomme environ 1,5 Go).
- Les tailles correspondent aux versions quantifiées (Q4) servies par défaut par
  Ollama, avec un contexte de quelques milliers de tokens. Un contexte plus long
  demande plus de mémoire.
- **Sans GPU**, les modèles jusqu'à 8B restent agréables (5 à 15 tokens/s sur un CPU
  récent) ; au-delà, ils fonctionnent mais lentement. `recommend` plafonne donc sa
  proposition à ~8B sur CPU.

## Modèles de chat

| Mémoire disponible | Recommandé | Alternatives | Téléchargement |
|--------------------|------------|--------------|----------------|
| 2 – 4 Go | `gemma3:1b` | `qwen3:1.7b` | 0,8 – 1,4 Go |
| 4 – 8 Go | `qwen3:4b` | `llama3.2:3b`, `gemma3:4b` (images), `mistral:7b` | 2 – 4 Go |
| 8 – 12 Go | `qwen3:8b` | `llama3.1:8b`, `deepseek-r1:8b` (raisonnement), `mistral-nemo:12b` | 5 – 7 Go |
| 12 – 16 Go | `qwen3:8b` | `gemma3:12b` (images) | 5 – 8 Go |
| 16 – 24 Go | `qwen3:14b` | `deepseek-r1:14b`, `gpt-oss:20b`, `mistral-small3.2:24b` (très bon en français) | 9 – 15 Go |
| 24 – 32 Go | `qwen3:30b` (MoE, rapide) | `gemma3:27b`, `qwen3:32b`, `deepseek-r1:32b` | 17 – 20 Go |
| 48 – 64 Go | `llama3.3:70b` | `qwen2.5:72b` | 43 – 47 Go |
| 72 Go et plus | `gpt-oss:120b` | `llama3.3:70b` | 65 Go |

## Modèles de code

| Mémoire | Modèle |
|---------|--------|
| 3 Go | `qwen2.5-coder:1.5b` (autocomplétion) |
| 8 Go | `qwen2.5-coder:7b` |
| 14 Go | `qwen2.5-coder:14b` |
| 20 Go | `devstral:24b` (agents de code) |
| 26 Go | `qwen2.5-coder:32b` |

## Modèles d'embeddings

| Modèle | Dimensions | Langues | Taille | Usage |
|--------|-----------|---------|--------|-------|
| `bge-m3` | 1024 | multilingue (français) | 1,2 Go | recommandé dès 6 Go de mémoire |
| `nomic-embed-text` | 768 | surtout anglais | 274 Mo | petites machines |
| `mxbai-embed-large` | 1024 | anglais | 670 Mo | |
| `all-minilm` | 384 | anglais | 46 Mo | tests, très petites machines |

Un modèle d'embeddings tourne en même temps que le modèle de chat ; prévoyez sa
mémoire en plus.

## vLLM (GPU puissants)

Avec `engine.kind = vllm`, le modèle est un dépôt Hugging Face en pleine précision
(ou quantifié AWQ/GPTQ) :

| VRAM | Exemples |
|------|----------|
| 24 Go | `Qwen/Qwen3-8B`, `meta-llama/Llama-3.1-8B-Instruct` |
| 48 Go | `Qwen/Qwen3-14B`, `Qwen/Qwen3-30B-A3B` |
| 80 Go | `Qwen/Qwen3-32B`, `meta-llama/Llama-3.3-70B-Instruct-AWQ` (AWQ) |

vLLM réserve `engine.vllm_gpu_memory_utilization` (90 %) de la VRAM ; Ollama reste
disponible pour les embeddings (sur CPU si la VRAM est prise).

## Licences

Chaque modèle a sa licence (Apache 2.0 pour Qwen3, Mistral 7B ; licences
communautaires spécifiques pour Llama et Gemma). Vérifiez-la avant un usage
commercial.
