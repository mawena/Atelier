use crate::cli::ConfigCommand;
use crate::config::Config;
use crate::output;

use super::Ctx;

pub async fn run(ctx: &Ctx, cmd: ConfigCommand) -> anyhow::Result<()> {
    match cmd {
        ConfigCommand::Show => show(ctx),
        ConfigCommand::Get { key } => {
            let cfg = Config::load(&ctx.paths)?;
            let value = cfg.get(&key)?;
            if output::json_mode() {
                return output::print_json(
                    &serde_json::json!({ "key": crate::config::resolve_alias(&key), "value": value }),
                );
            }
            output::line(value);
            Ok(())
        }
        ConfigCommand::Set {
            key,
            value,
            no_apply,
        } => set(ctx, &key, &value, no_apply).await,
    }
}

fn show(ctx: &Ctx) -> anyhow::Result<()> {
    let cfg = Config::load(&ctx.paths)?;
    if output::json_mode() {
        return output::print_json(&cfg);
    }
    output::line(output::dim(&format!(
        "# {}",
        ctx.paths.config_file().display()
    )));
    output::line(toml::to_string_pretty(&cfg)?);
    Ok(())
}

async fn set(ctx: &Ctx, key: &str, value: &str, no_apply: bool) -> anyhow::Result<()> {
    ctx.paths.require_privileges()?;
    let mut cfg = Config::load(&ctx.paths)?;
    let before = cfg.clone();
    cfg.set(key, value)?;
    let full = crate::config::resolve_alias(key);
    if cfg == before {
        output::success(format!("{full} = {} (inchangé)", cfg.get(key)?));
        return Ok(());
    }
    cfg.save(&ctx.paths)?;
    if !crate::config::is_loopback(&cfg.server.listen)
        && crate::config::is_loopback(&before.server.listen)
    {
        output::warn(format!(
            "l'API écoute sur {} : elle est joignable depuis le réseau (clé API obligatoire). \
             Pour du HTTPS, préférez « {} expose --domain <domaine> ».",
            cfg.server.listen,
            crate::APP
        ));
    }
    let mut applied = false;
    if !no_apply {
        let docker = crate::docker::Docker::connect().await;
        let running = match &docker {
            Ok(d) => !d.containers().await?.is_empty(),
            Err(_) => false,
        };
        if running {
            let changes = crate::render::regenerate(&cfg, &ctx.paths)?;
            crate::stack::apply(&ctx.paths, &cfg, &changes).await?;
            applied = true;
        }
    }
    if output::json_mode() {
        return output::print_json(&serde_json::json!({
            "ok": true, "key": full, "value": cfg.get(key)?, "applied": applied,
        }));
    }
    output::success(format!("{full} = {}", cfg.get(key)?));
    if applied {
        output::success("configuration appliquée");
    } else if !no_apply {
        output::line(output::dim(&format!(
            "Pile arrêtée : sera appliqué au prochain « {} start ».",
            crate::APP
        )));
    }
    Ok(())
}
