use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn atelier(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("atelier").unwrap();
    cmd.env("ATELIER_CONFIG_DIR", dir.path().join("etc"))
        .env("ATELIER_DATA_DIR", dir.path().join("data"))
        .env("NO_COLOR", "1");
    cmd
}

fn installed() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("etc")).unwrap();
    std::fs::write(
        dir.path().join("etc/atelier.toml"),
        "[server]\nport = 8080\n",
    )
    .unwrap();
    dir
}

#[test]
fn help_lists_every_command() {
    let dir = TempDir::new().unwrap();
    let out = atelier(&dir).arg("--help").assert().success();
    let text = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    for cmd in [
        "install",
        "uninstall",
        "start",
        "stop",
        "restart",
        "status",
        "logs",
        "update",
        "model",
        "key",
        "config",
        "expose",
        "chat",
        "test",
        "backup",
        "restore",
        "doctor",
        "version",
        "help",
    ] {
        assert!(
            text.contains(&format!("  {cmd} ")),
            "commande absente de l'aide : {cmd}"
        );
    }
}

#[test]
fn help_for_subcommand() {
    let dir = TempDir::new().unwrap();
    atelier(&dir)
        .args(["help", "key", "create"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--rpm"));
}

#[test]
fn version_json() {
    let dir = TempDir::new().unwrap();
    let out = atelier(&dir).args(["version", "--json"]).assert().success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
    assert!(v["commit"].is_string());
    assert!(v["target"].is_string());
}

#[test]
fn unknown_option_is_usage_error() {
    let dir = TempDir::new().unwrap();
    atelier(&dir).arg("--nope").assert().code(2);
}

#[test]
fn not_installed_exit_code() {
    let dir = TempDir::new().unwrap();
    atelier(&dir)
        .args(["config", "show"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("install"));
}

#[test]
fn not_installed_json_error() {
    let dir = TempDir::new().unwrap();
    let out = atelier(&dir)
        .args(["config", "show", "--json"])
        .assert()
        .code(3);
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["code"], 3);
}

#[test]
fn config_set_get_and_validation() {
    let dir = installed();
    atelier(&dir)
        .args(["config", "set", "port", "9000", "--no-apply"])
        .assert()
        .success();
    atelier(&dir)
        .args(["config", "get", "server.port"])
        .assert()
        .success()
        .stdout("9000\n");
    atelier(&dir)
        .args(["config", "set", "server.port", "abc", "--no-apply"])
        .assert()
        .code(2);
    atelier(&dir)
        .args(["config", "set", "inconnue.cle", "1", "--no-apply"])
        .assert()
        .code(2);
    let out = atelier(&dir)
        .args(["config", "show", "--json"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["server"]["port"], 9000);
    assert_eq!(v["server"]["listen"], "127.0.0.1");
}
