//! Sorties : messages colorés pour les humains, JSON pour les machines.
//!
//! En mode `--json`, les messages humains partent sur stderr et seul le
//! document JSON final est écrit sur stdout.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};

use comfy_table::{Cell, ContentArrangement, Table, presets};
use owo_colors::OwoColorize;
use serde::Serialize;

use crate::error::Fail;

static COLOR: AtomicBool = AtomicBool::new(false);
static JSON: AtomicBool = AtomicBool::new(false);
static YES: AtomicBool = AtomicBool::new(false);

pub fn init(json: bool, no_color: bool, yes: bool) {
    let color = !no_color
        && std::env::var_os("NO_COLOR").is_none()
        && std::io::stdout().is_terminal()
        && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(true);
    COLOR.store(color, Ordering::Relaxed);
    JSON.store(json, Ordering::Relaxed);
    YES.store(yes, Ordering::Relaxed);
}

pub fn json_mode() -> bool {
    JSON.load(Ordering::Relaxed)
}

pub fn assume_yes() -> bool {
    YES.load(Ordering::Relaxed)
}

fn color() -> bool {
    COLOR.load(Ordering::Relaxed)
}

pub fn green(s: &str) -> String {
    if color() {
        s.green().to_string()
    } else {
        s.to_string()
    }
}
pub fn red(s: &str) -> String {
    if color() {
        s.red().to_string()
    } else {
        s.to_string()
    }
}
pub fn yellow(s: &str) -> String {
    if color() {
        s.yellow().to_string()
    } else {
        s.to_string()
    }
}
pub fn cyan(s: &str) -> String {
    if color() {
        s.cyan().to_string()
    } else {
        s.to_string()
    }
}
pub fn bold(s: &str) -> String {
    if color() {
        s.bold().to_string()
    } else {
        s.to_string()
    }
}
pub fn dim(s: &str) -> String {
    if color() {
        s.dimmed().to_string()
    } else {
        s.to_string()
    }
}

/// Écrit une ligne destinée à un humain.
pub fn line(msg: impl AsRef<str>) {
    if json_mode() {
        eprintln!("{}", msg.as_ref());
    } else {
        println!("{}", msg.as_ref());
    }
}

pub fn step(msg: impl AsRef<str>) {
    line(format!("{} {}", cyan("→"), msg.as_ref()));
}

pub fn success(msg: impl AsRef<str>) {
    line(format!("{} {}", green("✔"), msg.as_ref()));
}

pub fn warn(msg: impl AsRef<str>) {
    eprintln!("{} {}", yellow("!"), msg.as_ref());
}

pub fn error(msg: impl AsRef<str>) {
    eprintln!("{} {}", red("✘"), msg.as_ref());
}

pub fn title(msg: impl AsRef<str>) {
    line(bold(msg.as_ref()));
}

/// Paire clé / valeur alignée.
pub fn kv(key: &str, value: impl AsRef<str>) {
    line(format!("  {:<18} {}", dim(key), value.as_ref()));
}

/// Écrit un document JSON sur stdout.
pub fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Tableau sans bordures, en-têtes en gras.
pub fn table(headers: &[&str], rows: Vec<Vec<String>>) {
    let mut t = Table::new();
    t.load_style(presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(headers.iter().map(|h| Cell::new(bold(h))));
    for row in rows {
        t.add_row(row);
    }
    line(t.to_string());
}

/// Demande une confirmation. `--yes` répond oui ; sans terminal, refuse.
pub fn confirm(question: &str, default: bool) -> anyhow::Result<bool> {
    if assume_yes() {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        return Err(Fail::Usage(format!(
            "{question} — confirmation impossible sans terminal, relancez avec --yes"
        ))
        .into());
    }
    Ok(dialoguer::Confirm::new()
        .with_prompt(question)
        .default(default)
        .interact()?)
}

/// Formate une taille en octets.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} o")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_format() {
        assert_eq!(bytes(512), "512 o");
        assert_eq!(bytes(1536), "1.5 Ko");
        assert_eq!(bytes(5 * 1024 * 1024 * 1024), "5.0 Go");
    }
}
