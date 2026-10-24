use serde_json::json;

use super::Ctx;
use crate::APP;
use crate::cli::KeyCommand;
use crate::config::Config;
use crate::error::{Fail, usage};
use crate::gateway::{Gateway, WEBUI_KEY_ALIAS};
use crate::output::{self, bold, dim, green, yellow};
use crate::secrets::Secrets;

pub async fn run(ctx: &Ctx, cmd: KeyCommand) -> anyhow::Result<()> {
    ctx.paths.require_privileges()?;
    let cfg = Config::load(&ctx.paths)?;
    let secrets = Secrets::load(&ctx.paths)?;
    let gw = Gateway::new(&cfg, &secrets);
    match cmd {
        KeyCommand::Create {
            name,
            rpm,
            tpm,
            expires,
        } => create(&cfg, &gw, &name, rpm, tpm, expires).await,
        KeyCommand::List => list(&gw).await,
        KeyCommand::Revoke { id } => revoke(&gw, &id).await,
    }
}

/// Durée au format LiteLLM : nombre suivi de s, m, h ou d.
pub fn valid_duration(d: &str) -> bool {
    let (num, unit) = d.split_at(d.len().saturating_sub(1));
    !num.is_empty()
        && num.chars().all(|c| c.is_ascii_digit())
        && matches!(unit, "s" | "m" | "h" | "d")
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

async fn create(
    cfg: &Config,
    gw: &Gateway,
    name: &str,
    rpm: Option<u32>,
    tpm: Option<u32>,
    expires: Option<String>,
) -> anyhow::Result<()> {
    if !valid_name(name) {
        return Err(usage(
            "nom de clé invalide : lettres, chiffres, « - », « _ » et « . » uniquement (64 max.)",
        ));
    }
    if name == WEBUI_KEY_ALIAS {
        return Err(usage(format!(
            "le nom « {WEBUI_KEY_ALIAS} » est réservé à l'interface web"
        )));
    }
    if let Some(d) = &expires
        && !valid_duration(d)
    {
        return Err(usage("durée invalide : utilisez par exemple 30d, 12h, 90m"));
    }
    if gw.list_keys().await?.iter().any(|k| k.name == name) {
        return Err(usage(format!("une clé nommée « {name} » existe déjà")));
    }
    let rpm = rpm.or(Some(cfg.limits.rpm));
    let tpm = tpm.or(Some(cfg.limits.tpm));
    let key = gw
        .create_key(name, rpm, tpm, expires.as_deref(), None)
        .await?;
    if output::json_mode() {
        return output::print_json(&key);
    }
    output::success(format!("clé « {name} » créée (identifiant {})", key.id));
    output::line(bold("Clé (affichée une seule fois, conservez-la) :"));
    output::line(format!("  {}", green(&key.key)));
    output::line(dim(&format!(
        "Limites : {} requêtes/min, {} tokens/min{}",
        key.rpm.map(|r| r.to_string()).unwrap_or_else(|| "∞".into()),
        key.tpm.map(|t| t.to_string()).unwrap_or_else(|| "∞".into()),
        key.expires
            .as_ref()
            .map(|e| format!(", expire le {e}"))
            .unwrap_or_default()
    )));
    Ok(())
}

async fn list(gw: &Gateway) -> anyhow::Result<()> {
    let keys = gw.list_keys().await?;
    if output::json_mode() {
        return output::print_json(&keys);
    }
    if keys.is_empty() {
        output::line(format!(
            "Aucune clé. Créez-en une : « sudo {APP} key create <nom> »"
        ));
        return Ok(());
    }
    let short = |d: &Option<String>| {
        d.as_deref()
            .map(|s| s.chars().take(16).collect::<String>().replace('T', " "))
    };
    let rows = keys
        .iter()
        .map(|k| {
            vec![
                k.id.clone(),
                if k.name == WEBUI_KEY_ALIAS {
                    dim(&format!("{} (interne)", k.name))
                } else {
                    k.name.clone()
                },
                k.preview.clone(),
                k.rpm.map(|r| r.to_string()).unwrap_or_else(|| "∞".into()),
                k.tpm.map(|t| t.to_string()).unwrap_or_else(|| "∞".into()),
                short(&k.created_at).unwrap_or_default(),
                short(&k.expires).unwrap_or_else(|| "jamais".into()),
                short(&k.last_used).unwrap_or_else(|| "-".into()),
            ]
        })
        .collect();
    output::table(
        &[
            "ID",
            "NOM",
            "CLÉ",
            "RPM",
            "TPM",
            "CRÉÉE",
            "EXPIRE",
            "DERNIÈRE UTILISATION",
        ],
        rows,
    );
    Ok(())
}

async fn revoke(gw: &Gateway, id: &str) -> anyhow::Result<()> {
    let key = gw.resolve_key(id).await?;
    if key.name == WEBUI_KEY_ALIAS {
        output::warn(
            "cette clé est utilisée par l'interface web, qui ne pourra plus appeler l'API",
        );
        if !output::confirm("Révoquer quand même ?", false)? {
            return Err(Fail::Cancelled.into());
        }
    }
    gw.delete_key(&key.token).await?;
    if output::json_mode() {
        return output::print_json(&json!({ "ok": true, "revoked": key.id, "name": key.name }));
    }
    output::success(format!("clé « {} » ({}) révoquée", key.name, key.id));
    if key.name != WEBUI_KEY_ALIAS {
        output::line(yellow(
            "Les requêtes utilisant cette clé reçoivent désormais 401.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert!(valid_duration("30d"));
        assert!(valid_duration("12h"));
        assert!(!valid_duration("d"));
        assert!(!valid_duration("30j"));
        assert!(!valid_duration("1.5h"));
    }

    #[test]
    fn names() {
        assert!(valid_name("android-app_1.0"));
        assert!(!valid_name(""));
        assert!(!valid_name("avec espace"));
    }
}
