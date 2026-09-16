# Project Memory

## Release Validation

- Frontend gate: `pnpm run format:check`, `pnpm run lint`, `pnpm run typecheck`, `pnpm run test:unit`, `pnpm run build`, `pnpm run audit:frontend`.
- Rust gate: `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`, `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`, `cargo test --manifest-path src-tauri/Cargo.toml --all-features`.
- Production validation: `pnpm run tauri:build`, then run `scripts/test/installed-nsis-smoke.ps1` with the generated NSIS installer and an evidence root below `.tmp/`.
- `scripts/test/cargo-audit-if-available.ps1` retries an online RustSec advisory refresh once and then audits the local cached database. A cached audit must still fail on known vulnerabilities; the fallback only handles transient GitHub transport failures.

## Known Limits

- Vite reports a production JavaScript chunk above 500 kB. It is a non-blocking build warning; code splitting is a future performance task.
- Lint has existing test-only `vue/one-component-per-file` warnings, but no lint errors.
