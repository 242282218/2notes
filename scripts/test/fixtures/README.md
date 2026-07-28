# 0.3.0 Upgrade Baseline Fixture

## Purpose

Provide a reproducible 0.3.x content baseline for Gate 0 freeze and later 0.4.x upgrade checks.

## Important limitation

`v0.3.0-upgrade-baseline.sqlite` is a **schema-equivalent 0.3.0 content baseline**:

- created by current app migrations (version 4) and repository APIs
- **not** captured from a real 0.3.0 installer session or user machine

Use it for restore/reopen and future migration reopen tests. Do not claim it as installer-captured production data.

## Contents

Seeded mix:

- capture entry
- knowledge entry (with promote-time alias after rename)
- capture entry containing resolved WikiLink + unresolved WikiLink
- trash entry
- non-empty quick-capture draft

## Regenerate

From repo root on Windows:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test/generate-v030-upgrade-fixture.ps1
```

Or:

```bash
GENERATE_V030_FIXTURE=1 cargo test --manifest-path src-tauri/Cargo.toml --lib v030_upgrade_baseline_fixture_seeds_and_restores -- --nocapture
```

Outputs:

- `scripts/test/fixtures/v0.3.0-upgrade-baseline.sqlite`
- `scripts/test/fixtures/v0.3.0-upgrade-baseline.metrics.json`

The Rust test also restores the generated backup into a live AppState path and re-checks metrics.
