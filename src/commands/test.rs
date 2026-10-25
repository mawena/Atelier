use serde_json::json;

use super::Ctx;
use crate::api::Api;
use crate::config::Config;
use crate::error::Fail;
use crate::output::{self, bold, green, kv, red};

const DEFAULT_PROMPT: &str = "Explique en trois phrases ce qu'est un modèle de langage.";

pub async fn run(ctx: &Ctx, model: Option<String>, prompt: Option<String>) -> anyhow::Result<()> {
    let cfg = Config::load(&ctx.paths)?;
    let api = Api::local(&cfg, &ctx.paths)?;
    let model = model.unwrap_or_else(|| "default".into());
    let prompt = prompt.unwrap_or_else(|| DEFAULT_PROMPT.into());

    // Santé (contrat : {status, version}).
    let (code, health) = api.health().await?;
    let health_ok = code == 200 && health.as_ref().is_some_and(|h| h.status == "ok");
    if !output::json_mode() {
        kv(
            "GET /health",
            match &health {
                Some(h) if health_ok => green(&format!("{} (version {})", h.status, h.version)),
                Some(h) => red(&format!("{code} {}", h.status)),
                None => red(&format!("{code} réponse invalide")),
            },
        );
    }
    if !health_ok {
        return Err(Fail::Unhealthy(format!(
            "l'API n'est pas en bonne santé — « {} doctor »",
            crate::APP
        ))
        .into());
    }

    // Authentification obligatoire.
    let unauth = api.models_without_key().await?;
    let models = api.models().await?;
    if !output::json_mode() {
        kv(
            "Sans clé",
            if unauth == 401 {
                green("401 (refusé)")
            } else {
                red(&format!("{unauth} (devrait être 401)"))
            },
        );
        kv(
            "GET /v1/models",
            format!("{} modèle(s) : {}", models.len(), models.join(", ")),
        );
    }
    if !models.contains(&model) {
        return Err(crate::error::not_found(format!(
            "le modèle « {model} » n'est pas servi — « {} model list »",
            crate::APP
        )));
    }

    // Complétion en streaming : latence et débit.
    let bar = crate::stack::spinner(&format!("génération avec {model}…"));
    let stats = api
        .chat_stream(
            &model,
            &[json!({ "role": "user", "content": prompt })],
            |_| {},
        )
        .await;
    bar.finish_and_clear();
    let stats = stats?;

    // Embeddings, si un modèle est configuré.
    let embed = if models.iter().any(|m| m == "default-embedding") {
        Some(
            api.embeddings("default-embedding", "Bonjour le monde")
                .await?,
        )
    } else {
        None
    };

    if output::json_mode() {
        return output::print_json(&json!({
            "ok": true,
            "url": api.base_url,
            "health": health,
            "auth_enforced": unauth == 401,
            "models": models,
            "model": model,
            "ttft_ms": stats.ttft_ms,
            "total_ms": stats.total_ms,
            "prompt_tokens": stats.prompt_tokens,
            "completion_tokens": stats.completion_tokens,
            "tokens_per_second": (stats.tokens_per_second * 10.0).round() / 10.0,
            "stream_done_marker": stats.done_marker,
            "embedding_dimensions": embed,
        }));
    }
    kv(
        "Streaming",
        if stats.done_marker {
            green("terminé par data: [DONE]")
        } else {
            red("marqueur [DONE] absent")
        },
    );
    kv(
        "Premier token",
        stats
            .ttft_ms
            .map(|t| format!("{t} ms"))
            .unwrap_or_else(|| "-".into()),
    );
    kv("Durée totale", format!("{} ms", stats.total_ms));
    kv(
        "Tokens générés",
        format!(
            "{}{}",
            stats.completion_tokens,
            if stats.usage_reported {
                ""
            } else {
                " (estimés)"
            }
        ),
    );
    kv(
        "Débit",
        bold(&format!("{:.1} tokens/s", stats.tokens_per_second)),
    );
    if let Some(d) = embed {
        kv("POST /v1/embeddings", format!("vecteur de {d} dimensions"));
    }
    let preview: String = stats.text.chars().take(160).collect();
    output::line(output::dim(&format!(
        "« {}{} »",
        preview.trim(),
        if stats.text.len() > 160 { "…" } else { "" }
    )));
    if unauth != 401 || !stats.done_marker {
        return Err(
            Fail::Problems("le contrat d'API n'est pas entièrement respecté".into()).into(),
        );
    }
    output::success("test réussi");
    Ok(())
}
