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

async fn set(ctx: &Ctx, key: &str, value: &str, _no_apply: bool) -> anyhow::Result<()> {
    ctx.paths.require_privileges()?;
    let mut cfg = Config::load(&ctx.paths)?;
    cfg.set(key, value)?;
    cfg.save(&ctx.paths)?;
    output::success(format!(
        "{} = {}",
        crate::config::resolve_alias(key),
        cfg.get(key)?
    ));
    Ok(())
}
