//! Manual smoke test for the Antigravity brain scanner.
//!
//! Run with:
//!   cargo run --manifest-path src-tauri/Cargo.toml --no-default-features \
//!     --example antigravity_demo
//!
//! Prints the resolved candidate roots and the merged, de-duplicated folder
//! list. Pass `--folders N` to also dump the first N folders with their
//! titles, or `--folders 0` for the summary only.

use claude_code_trace_lib::commands::antigravity::{
    brain_root_candidates, merge_brain_roots, ENV_GEMINI_HOME,
};

fn main() {
    let limit: usize = std::env::args()
        .skip_while(|a| a != "--folders")
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    let gemini_home = std::env::var(ENV_GEMINI_HOME).ok();
    let os_home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned());

    println!("=== Antigravity manual smoke test ===");
    println!("GEMINI_HOME: {:?}", gemini_home);
    println!("OS home:     {:?}", os_home);

    let candidates = brain_root_candidates(None, gemini_home.as_deref(), os_home.as_deref());
    if candidates.is_empty() {
        println!("(no resolvable home — nothing to scan)");
        return;
    }

    println!("\ncandidates (priority order):");
    for c in &candidates {
        println!(
            "  [{}] {}  {}",
            c.flavor,
            if c.exists { "found" } else { "absent" },
            c.path
        );
    }

    let folders = merge_brain_roots(&candidates);
    let with_transcript = folders.iter().filter(|f| f.has_transcript).count();
    let mut per_flavor: Vec<(&str, usize)> = Vec::new();
    for f in &folders {
        match per_flavor
            .iter_mut()
            .find(|(name, _)| *name == f.flavor.as_str())
        {
            Some((_, n)) => *n += 1,
            None => per_flavor.push((f.flavor.as_str(), 1)),
        }
    }

    println!("\nmerged:       {} conversation(s)", folders.len());
    println!("with transcript: {with_transcript}");
    println!("without:      {}", folders.len() - with_transcript);
    for (flavor, n) in per_flavor {
        println!("  {flavor}: {n}");
    }

    if limit == 0 {
        return;
    }
    println!();
    for f in folders.iter().take(limit) {
        println!("- {}", f.uuid);
        println!("  flavor:      {}", f.flavor);
        println!(
            "  transcript:  {}",
            if f.has_transcript { "yes" } else { "no" }
        );
        println!("  title:       {:?}", f.title);
        println!("  mtime:       {} ms", f.mod_time_ms);
        println!();
    }
    if folders.len() > limit {
        println!("… and {} more", folders.len() - limit);
    }
}
