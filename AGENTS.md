# CLAUDE.md

## Commands

```bash
# Dev
npm run dev              # Vite dev server
npm run tauri dev        # Full Tauri app

# Lint
npx oxlint              # JS/TS lint
cargo clippy --manifest-path src-tauri/Cargo.toml  # Rust lint

# Format
npx oxfmt               # JS/TS format
cargo fmt --manifest-path src-tauri/Cargo.toml     # Rust format

# Test
npx vitest run           # Frontend tests
npm run test:e2e         # Playwright e2e: real headless backend + real browser (see e2e/)
cargo test --manifest-path src-tauri/Cargo.toml    # Rust tests

# Type check
npx tsc --noEmit

# All at once
npm run check            # tsc + oxlint + oxfmt --check + clippy + cargo fmt --check + vitest + cargo test
```

## Rule

After every code change (src, tests, config that affects build), always add enough tests for the changes, then run lint, format, and test before committing:

```bash
npx oxfmt && npx oxlint && npx tsc --noEmit && npx vitest run && cargo fmt --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml && cargo test --manifest-path src-tauri/Cargo.toml
```
