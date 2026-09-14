//! Regression guard for `null`-tolerant JSONL entry parsing.
//!
//! Serde's `#[serde(default)]` only fires when a field is *absent*. When Claude
//! Code writes `"field": null` for a non-`Option` field, `serde_json` errors and
//! `parse_entry` drops the entire line. The dropped entry leaves a hole in the
//! `parentUuid` chain, and `resolve_live_chain_uuids` stops walking there — so
//! one unrecognised null can blank out an entire session in the UI. This is
//! exactly what `"usage": {"iterations": null}` (Claude Code v2.1.266 synthetic
//! assistant messages) did.
//!
//! Non-`Option` fields must therefore carry
//! `deserialize_with = "null_as_default"`. Nothing in the type system enforces
//! that, so this test reads the struct definitions and checks every field.
//! `Option<T>` fields are exempt — serde already maps `null` to `None`.

use std::collections::BTreeSet;

const ENTRY_RS: &str = include_str!("../src/parser/entry.rs");

/// Structs deserialized straight from a raw JSONL line.
const DESERIALIZED_STRUCTS: &[&str] = &[
    "Entry",
    "EntryMessage",
    "EntryUsage",
    "CacheCreationUsage",
    "IterationUsage",
];

/// Body of `pub struct <name> { ... }`, up to the closing brace in column 0.
fn struct_body<'a>(src: &'a str, name: &str) -> &'a str {
    let header = format!("pub struct {name} {{");
    let start = src
        .find(&header)
        .unwrap_or_else(|| panic!("struct {name} present in entry.rs"))
        + header.len();
    let rest = &src[start..];
    let end = rest
        .find("\n}")
        .unwrap_or_else(|| panic!("closing brace for struct {name}"));
    &rest[..end]
}

/// `(field name, type)` for every field in `body` whose preceding attributes
/// lack `null_as_default`, skipping `Option<..>` fields.
fn fields_missing_null_guard(body: &str) -> Vec<(String, String)> {
    let mut missing = Vec::new();
    let mut attrs = String::new();
    for line in body.lines() {
        let line = line.trim();
        if line.starts_with("//") || line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("pub ") {
            let (name, ty) = rest
                .split_once(": ")
                .unwrap_or_else(|| panic!("field declaration parses: {line}"));
            let ty = ty.trim_end_matches(',');
            if !ty.starts_with("Option<") && !attrs.contains("null_as_default") {
                missing.push((name.to_string(), ty.to_string()));
            }
            attrs.clear();
        } else {
            // Attribute line, possibly one of several for a multi-line `#[serde(..)]`.
            attrs.push_str(line);
        }
    }
    missing
}

#[test]
fn every_non_option_entry_field_tolerates_explicit_null() {
    let mut offenders: Vec<String> = Vec::new();
    for name in DESERIALIZED_STRUCTS {
        for (field, ty) in fields_missing_null_guard(struct_body(ENTRY_RS, name)) {
            offenders.push(format!("{name}.{field}: {ty}"));
        }
    }
    assert!(
        offenders.is_empty(),
        "these non-Option fields would fail the whole line on `\"field\": null` — \
         add `deserialize_with = \"null_as_default\"` to each:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn guard_detects_a_field_without_the_annotation() {
    // The guard is only useful if it actually fails on an unannotated field.
    let body = "\n    #[serde(default)]\n    pub thing: String,\n";
    let found: BTreeSet<String> = fields_missing_null_guard(body)
        .into_iter()
        .map(|(f, _)| f)
        .collect();
    assert!(found.contains("thing"));

    let guarded =
        "\n    #[serde(default, deserialize_with = \"null_as_default\")]\n    pub thing: String,\n";
    assert!(fields_missing_null_guard(guarded).is_empty());

    // Multi-line attribute form.
    let multiline = "\n    #[serde(\n        default,\n        rename = \"thing\",\n        deserialize_with = \"null_as_default\"\n    )]\n    pub thing: String,\n";
    assert!(fields_missing_null_guard(multiline).is_empty());

    // Option fields are exempt.
    let optional = "\n    #[serde(default)]\n    pub thing: Option<String>,\n";
    assert!(fields_missing_null_guard(optional).is_empty());
}
