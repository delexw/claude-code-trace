//! Regression guard for the Tauri ACL.
//!
//! Every IPC command registered in `tauri::generate_handler![ ... ]` must also be
//! granted by the `[default]` permission set in `permissions/default.toml`.
//! Otherwise the desktop build rejects the call at runtime with
//! `Command <name> not allowed by ACL` — the bug that hit `set_wsl_distros` when
//! the WSL feature added the handler but forgot the matching `commands.allow`
//! entry. The check runs in both directions so a stale ACL grant is caught too.

use std::collections::{BTreeMap, BTreeSet};

const LIB_RS: &str = include_str!("../src/lib.rs");
const DEFAULT_TOML: &str = include_str!("../permissions/default.toml");

/// Contents of every double-quoted string in `s`.
fn quoted_strings(s: &str) -> BTreeSet<String> {
    s.split('"')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, part)| part.to_string())
        .collect()
}

/// Command names registered in `tauri::generate_handler![ ... ]` (last path segment).
fn handler_commands() -> BTreeSet<String> {
    let start = LIB_RS
        .find("generate_handler![")
        .expect("generate_handler! macro present in lib.rs");
    let rest = &LIB_RS[start..];
    let open = rest.find('[').expect("opening [ for generate_handler!");
    let close = rest.find(']').expect("closing ] for generate_handler!");
    rest[open + 1..close]
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| t.rsplit("::").next().unwrap().trim().to_string())
        .collect()
}

/// Identifiers referenced by the `[default]` permission set.
fn default_permission_set() -> BTreeSet<String> {
    let key = "permissions = [";
    let start = DEFAULT_TOML
        .find(key)
        .expect("[default] permissions array present");
    let after = &DEFAULT_TOML[start + key.len()..];
    let end = after.find(']').expect("closing ] for default permissions");
    quoted_strings(&after[..end])
}

/// Map of each `[[permission]]` identifier to the commands it allows.
fn permission_blocks() -> BTreeMap<String, BTreeSet<String>> {
    let mut map = BTreeMap::new();
    for chunk in DEFAULT_TOML.split("[[permission]]").skip(1) {
        let Some(id_line) = chunk
            .lines()
            .find(|l| l.trim_start().starts_with("identifier"))
        else {
            continue;
        };
        let Some(id) = quoted_strings(id_line).into_iter().next() else {
            continue;
        };
        let key = "commands.allow = [";
        let cmds = match chunk.find(key) {
            Some(s) => {
                let after = &chunk[s + key.len()..];
                let end = after.find(']').unwrap_or(after.len());
                quoted_strings(&after[..end])
            }
            None => BTreeSet::new(),
        };
        map.insert(id, cmds);
    }
    map
}

/// Commands granted by the `[default]` permission set.
fn acl_granted_commands() -> BTreeSet<String> {
    let referenced = default_permission_set();
    let blocks = permission_blocks();
    referenced
        .iter()
        .filter_map(|id| blocks.get(id))
        .flat_map(|cmds| cmds.iter().cloned())
        .collect()
}

#[test]
fn handler_and_acl_grant_the_same_commands() {
    let handler = handler_commands();
    let granted = acl_granted_commands();

    let missing_from_acl: Vec<_> = handler.difference(&granted).collect();
    assert!(
        missing_from_acl.is_empty(),
        "commands registered in generate_handler! but missing an ACL `commands.allow` entry \
         (these fail at runtime with `Command <name> not allowed by ACL`): {missing_from_acl:?}"
    );

    let extra_in_acl: Vec<_> = granted.difference(&handler).collect();
    assert!(
        extra_in_acl.is_empty(),
        "ACL grants commands that are not registered in generate_handler!: {extra_in_acl:?}"
    );
}

#[test]
fn wsl_commands_are_acl_granted() {
    let granted = acl_granted_commands();
    assert!(
        granted.contains("list_wsl_distros"),
        "list_wsl_distros must be ACL-granted"
    );
    assert!(
        granted.contains("set_wsl_distros"),
        "set_wsl_distros must be ACL-granted"
    );
}
