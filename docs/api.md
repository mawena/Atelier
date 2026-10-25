# Référence de l'API

L'API suit le format OpenAI : tout client OpenAI (SDK officiels, LangChain,
applications mobiles…) fonctionne en changeant l'URL de base et la clé.

- URL de base : `http://127.0.0.1:8080` par défaut, `https://<domaine>` après
  `atelier expose`.
- Authentification : en-tête `Authorization: Bearer <clé>` sur toutes les routes
  `/v1/*`. Créer une clé : `sudo atelier key create <nom>`.
- Seules `/health` et `/v1/*` sont servies ; toute autre route répond 404.

Dans les exemples : `URL=http://127.0.0.1:8080` et `CLE=sk-...`.

## Contrat garanti

| Méthode | Route | Description |
|---------|-------|-------------|
| GET | `/health` | État du service, sans authentification |
| GET | `/v1/models` | Modèles disponibles |
| POST | `/v1/chat/completions` | Complétion de conversation, `stream` true ou false |
| POST | `/v1/embeddings` | Vecteurs d'embeddings |

### GET /health

```sh
curl $URL/health
```

```json
{"status":"ok","version":"0.1.0"}
```

| Code | `status` | Signification |
|------|----------|---------------|
| 200 | `ok` | passerelle et base opérationnelles |
| 503 | `degraded` | passerelle joignable mais pas prête |
| 503 | `down` | passerelle injoignable |

`version` est la version d'Atelier qui a généré la configuration.

### GET /v1/models

```sh
curl $URL/v1/models -H "Authorization: Bearer $CLE"
```

```json
{
  "object": "list",
  "data": [
    {"id": "default", "object": "model", "created": 1677610602, "owned_by": "openai"},
    {"id": "default-embedding", "object": "model", "created": 1677610602, "owned_by": "openai"},
    {"id": "qwen3:8b", "object": "model", "created": 1677610602, "owned_by": "openai"},
    {"id": "bge-m3", "object": "model", "created": 1677610602, "owned_by": "openai"}
  ]
}
```

Alias stables, à privilégier dans les applications :

- `default` → modèle de chat par défaut (`atelier model default <nom>`) ;
- `default-embedding` → modèle d'embeddings par défaut.

Changer de modèle côté serveur ne demande alors aucune modification des clients.

### POST /v1/chat/completions

Sans streaming :

```sh
curl $URL/v1/chat/completions \
  -H "Authorization: Bearer $CLE" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "default",
    "messages": [
      {"role": "system", "content": "Tu réponds en français, brièvement."},
      {"role": "user", "content": "Quelle est la capitale du Bénin ?"}
    ],
    "temperature": 0.7,
    "max_tokens": 256
  }'
```

```json
{
  "id": "chatcmpl-…",
  "object": "chat.completion",
  "created": 1792851078,
  "model": "default",
  "choices": [
    {"index": 0, "finish_reason": "stop",
     "message": {"role": "assistant", "content": "Porto-Novo."}}
  ],
  "usage": {"prompt_tokens": 31, "completion_tokens": 4, "total_tokens": 35}
}
```

Avec streaming (Server-Sent Events) :

```sh
curl -N $URL/v1/chat/completions \
  -H "Authorization: Bearer $CLE" \
  -H "Content-Type: application/json" \
  -d '{"model":"default","stream":true,"messages":[{"role":"user","content":"Compte jusqu à 3"}]}'
```

```
data: {"id":"chatcmpl-…","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":"1"}}]}

data: {"id":"chatcmpl-…","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":", 2"}}]}

data: {"id":"chatcmpl-…","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```

Le flux se termine toujours par `data: [DONE]`. Ajoutez
`"stream_options": {"include_usage": true}` pour recevoir le décompte des tokens
dans le dernier fragment. Les modèles de raisonnement (DeepSeek-R1, Qwen3…) peuvent
envoyer leur réflexion dans `delta.reasoning_content`.

Paramètres usuels acceptés : `temperature`, `top_p`, `max_tokens`, `stop`, `seed`,
`presence_penalty`, `frequency_penalty`, `response_format`, `tools`. Les paramètres
non pris en charge par le moteur sont ignorés.

### POST /v1/embeddings

```sh
curl $URL/v1/embeddings \
  -H "Authorization: Bearer $CLE" \
  -H "Content-Type: application/json" \
  -d '{"model":"default-embedding","input":["Bonjour le monde","Salut"]}'
```

```json
{
  "object": "list",
  "model": "default-embedding",
  "data": [
    {"object": "embedding", "index": 0, "embedding": [0.0123, -0.0456, …]},
    {"object": "embedding", "index": 1, "embedding": [0.0789, 0.0012, …]}
  ],
  "usage": {"prompt_tokens": 6, "total_tokens": 6}
}
```

## Erreurs

Format OpenAI :

```json
{"error": {"message": "…", "type": "…", "param": null, "code": "401"}}
```

| Code | Cause |
|------|-------|
| 400 | requête invalide (modèle inconnu, JSON mal formé…) |
| 401 | clé absente, invalide, révoquée ou expirée |
| 404 | route non servie |
| 429 | limite de débit de la clé atteinte (RPM/TPM) |
| 502 / 503 | passerelle ou moteur indisponible |

## Clients

Python (`pip install openai`) :

```python
from openai import OpenAI

client = OpenAI(base_url="https://api.exemple.fr/v1", api_key="sk-...")
stream = client.chat.completions.create(
    model="default",
    messages=[{"role": "user", "content": "Bonjour"}],
    stream=True,
)
for chunk in stream:
    print(chunk.choices[0].delta.content or "", end="")
```

Android / Kotlin (OkHttp + SSE) : appelez `POST {base}/v1/chat/completions` avec
`"stream": true`, lisez les lignes `data:` et arrêtez-vous sur `data: [DONE]`.
Vérifiez `GET {base}/health` au démarrage de l'application pour afficher l'état du
service et la version du serveur.

## Limites et journaux

- Limites par clé : `--rpm` et `--tpm` à la création (`atelier key create`) ; valeurs
  par défaut dans `limits.rpm` / `limits.tpm`.
- Délai maximal d'une requête : `limits.timeout` (600 s par défaut).
- Journaux des requêtes : `atelier logs litellm` ; l'historique d'usage par clé est
  conservé dans la base (dernière utilisation visible dans `atelier key list`).
