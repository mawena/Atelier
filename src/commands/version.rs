use crate::output;
use crate::{APP, COMMIT, TARGET, VERSION};

pub fn run() -> anyhow::Result<()> {
    if output::json_mode() {
        return output::print_json(&serde_json::json!({
            "name": APP,
            "version": VERSION,
            "commit": COMMIT,
            "target": TARGET,
        }));
    }
    output::line(format!("{APP} {VERSION} ({COMMIT}, {TARGET})"));
    Ok(())
}
