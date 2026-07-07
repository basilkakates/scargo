# User Accounts V1

## Goal and success criteria
- Add first-class dashboard accounts with username/password login.
- Let uploads authenticate with generated API tokens.
- Keep guest access available for development and smoke tests.
- Preserve existing telemetry ownership and aggregate behavior.
- Keep raw telemetry account-scoped and global comparisons aggregate-only.

## Implementation instructions
- Implement on branch `feature/user-accounts`.
- Add auth schema to clean bootstrap: account usernames/password hashes, sessions, and API tokens.
- Add an explicit migration script for existing databases instead of startup rewrites.
- Resolve account identity from a session cookie for the dashboard or `Authorization: Bearer` for uploads.
- Keep `X-Scargo-User-Key` as a dev/test fallback only and document it as deprecated.
- Reuse existing owner summary and cohort APIs for aggregates; do not add duplicate aggregate endpoints.

## Tools and commands
- Inspect state with `git status --short --branch`.
- Run `cargo test`.
- Run ignored smoke checks with `cargo test --test smoke_stack -- --ignored --nocapture` when a local TimescaleDB service is available.

## Relevant files and context
- `src/api/privacy.rs` owns account resolution, sessions, API tokens, and guest fallback.
- `src/api/auth.rs` owns register/login/logout/me/token endpoints.
- `src/db/migrate.rs` owns current clean schema bootstrap.
- `dashboard/static/index.html` and `dashboard/static/app.js` own the no-build dashboard login/upload UI.
- `scripts/scan-and-ingest.py` and `src/bulk.rs` own non-browser upload credentials.
- `docs/privacy-model.md`, `README.md`, and `AGENTS.md` must stay aligned with auth behavior.

## Acceptance checks and tests
- Users can register and log in on the dashboard.
- Logged-in browser uploads use the session cookie.
- Scripts and bulk ingest can claim uploads with an API token.
- Dev/test can still use the deterministic guest account.
- Existing owner-scoped reads remain scoped to the authenticated account.
- Public cohort reads remain aggregate-only and do not expose raw peer rows.
