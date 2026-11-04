//! The user's "Start at login" choice, kept as a file in their own LaunchAgents folder.

use std::path::PathBuf;

const AGENT_LABEL: &str = "com.developedbyashu.netspeedgauge";

/// Makes a path safe to place inside the agent file.
fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Returns the agent file's text for a program path.
pub fn agent_text(program_path: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n<dict>\n\
<key>Label</key><string>{AGENT_LABEL}</string>\n\
<key>ProgramArguments</key><array><string>{}</string></array>\n\
<key>RunAtLoad</key><true/>\n\
<key>LimitLoadToSessionType</key><string>Aqua</string>\n\
</dict>\n</plist>\n",
        escape_xml(program_path)
    )
}

fn agent_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/LaunchAgents").join(format!("{AGENT_LABEL}.plist")))
}

/// Returns true when the app starts at login.
pub fn is_enabled() -> bool {
    agent_path().is_some_and(|path| path.exists())
}

/// Turns starting at login on or off.
pub fn set_enabled(enabled: bool) {
    let Some(path) = agent_path() else {
        return;
    };
    if !enabled {
        let _ = std::fs::remove_file(path);
        return;
    }
    let Ok(program) = std::env::current_exe() else {
        return;
    };
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = std::fs::write(path, agent_text(&program.to_string_lossy()));
}

/// Keeps the agent pointing at this copy of the app, in case it was moved.
pub fn refresh_path() {
    if is_enabled() {
        set_enabled(true);
    }
}
