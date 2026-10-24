use std::io::Write;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::Ctx;
use crate::api::{Api, Delta};
use crate::config::Config;
use crate::output::{self, bold, cyan, dim};

pub async fn run(ctx: &Ctx, model: Option<String>, system: Option<String>) -> anyhow::Result<()> {
    let cfg = Config::load(&ctx.paths)?;
    let api = Api::local(&cfg, &ctx.paths)?;
    let mut model = model.unwrap_or_else(|| "default".into());
    let mut history: Vec<Value> = Vec::new();
    let reset = |h: &mut Vec<Value>| {
        h.clear();
        if let Some(s) = &system {
            h.push(json!({ "role": "system", "content": s }));
        }
    };
    reset(&mut history);

    output::line(format!(
        "{} {} — /reset, /model <nom>, /exit (Ctrl+D pour quitter)",
        bold("Chat avec"),
        cyan(&model)
    ));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        print!("{} ", bold(">"));
        std::io::stdout().flush().ok();
        let Some(line) = lines.next_line().await? else {
            println!();
            break;
        };
        let input = line.trim();
        if input.is_empty() {
            continue;
        }
        match input {
            "/exit" | "/quit" | "/q" => break,
            "/reset" => {
                reset(&mut history);
                output::line(dim("historique effacé"));
                continue;
            }
            _ => {}
        }
        if let Some(m) = input.strip_prefix("/model") {
            let m = m.trim();
            if m.is_empty() {
                output::line(format!("modèle actuel : {model}"));
            } else {
                model = m.to_string();
                output::line(dim(&format!("modèle : {model}")));
            }
            continue;
        }

        history.push(json!({ "role": "user", "content": input }));
        let mut in_reasoning = false;
        let result = api
            .chat_stream(&model, &history, |d| {
                match d {
                    Delta::Reasoning(t) => {
                        in_reasoning = true;
                        print!("{}", dim(t));
                    }
                    Delta::Content(t) => {
                        if in_reasoning {
                            in_reasoning = false;
                            println!();
                        }
                        print!("{t}");
                    }
                }
                std::io::stdout().flush().ok();
            })
            .await;
        println!();
        match result {
            Ok(stats) => {
                history.push(json!({ "role": "assistant", "content": stats.text }));
                output::line(dim(&format!(
                    "{} tokens · {:.1} tokens/s · {:.1} s",
                    stats.completion_tokens,
                    stats.tokens_per_second,
                    stats.total_ms as f64 / 1000.0
                )));
            }
            Err(e) => {
                history.pop();
                output::error(format!("{e:#}"));
            }
        }
    }
    Ok(())
}
