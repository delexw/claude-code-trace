fn main() {
    // The Tauri build step (context codegen, etc.) is only needed for the
    // desktop app. Headless-only builds (`--no-default-features`) skip it so
    // they don't require the tauri-build dependency or a frontend bundle.
    #[cfg(feature = "desktop")]
    tauri_build::build();
}
