//! Manual smoke test for the Antigravity brain scanner.
//!
//! Run with:
//!   cargo run --manifest-path src-tauri/Cargo.toml --no-default-features \
//!     --example antigravity_demo

use claude_code_trace_lib::commands::antigravity::{
    antigravity_brain_root, scan_antigravity_brain, ANTIGRAVITY_FLAVOR, ENV_GEMINI_HOME,
};

fn main() {
    let gemini_home = std::env::var(ENV_GEMINI_HOME).ok();
    let os_home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned());
    println!("=== Antigravity manual smoke test ===");
    println!("flavor:      {}", ANTIGRAVITY_FLAVOR);
    println!("GEMINI_HOME: {:?}", gemini_home);
    println!("OS home:     {:?}", os_home);

    let root = antigravity_brain_root(None, gemini_home.as_deref(), os_home.as_deref());
    let Some(root) = root else {
        println!("(no resolvable home — nothing to scan)");
        return;
    };
    println!("root:        {}", root.display());
    println!("exists:      {}", root.exists());

    if !root.exists() {
        println!("(root does not exist on disk — picker would return [])");
        return;
    }

    let folders = scan_antigravity_brain(&root);
    println!("found:       {} folder(s)", folders.len());
    println!();
    for f in &folders {
        println!("- uuid:    {}", f.uuid);
        println!("  path:    {}", f.path);
        println!("  title:   {:?}", f.title);
        println!("  mtime:   {} ms", f.mod_time_ms);
        println!("  provider: {}", f.provider_id);
        println!();
    }
}
