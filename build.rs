use std::process::Command;

fn main() {
    let commit = std::env::var("ATELIER_COMMIT").ok().or_else(|| {
        Command::new("git")
            .args(["rev-parse", "--short=10", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    });
    println!(
        "cargo:rustc-env=ATELIER_COMMIT={}",
        commit.unwrap_or_else(|| "inconnu".into())
    );
    println!(
        "cargo:rustc-env=ATELIER_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
    println!("cargo:rerun-if-env-changed=ATELIER_COMMIT");
    println!("cargo:rerun-if-changed=.git/HEAD");
}
