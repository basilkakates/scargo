# Dropbox OAuth and Storage

## Goal and success criteria
- Let each Scargo account connect exactly one Dropbox account through OAuth.
- Store Dropbox refresh/access token data encrypted at rest and scoped to the Scargo account.
- Keep the Dropbox ingest root fixed to `/OBD Fusion/CsvLogs` for v1.
- Expose connection status without returning Dropbox tokens to the browser.
- Bootstrap all required database tables from the current schema path.

Success means:
- `POST /api/dropbox/oauth/start` returns a Dropbox authorization URL.
- `GET /api/dropbox/oauth/callback` validates state, stores the connection, and redirects safely.
- `GET /api/dropbox/connection` returns account-owned connection status and fixed path metadata.
- `POST /api/dropbox/connection/pause` pauses or resumes polling for the signed-in account.
- `DELETE /api/dropbox/connection` removes the signed-in account's Dropbox connection and token data.
- Tokens are encrypted at rest and are never serialized in API responses.

## Implementation instructions
1. Add Dropbox config to `src/config/settings.rs`:
   - `DROPBOX_APP_KEY`
   - `DROPBOX_APP_SECRET`
   - `SCARGO_BASE_URL`
   - `SCARGO_TOKEN_ENCRYPTION_KEY`
   - `SCARGO_DROPBOX_POLL_SEC`
   - `SCARGO_DROPBOX_ENABLED`
2. Treat Dropbox as disabled unless `SCARGO_DROPBOX_ENABLED=true`.
3. Require `DROPBOX_APP_KEY`, `DROPBOX_APP_SECRET`, `SCARGO_BASE_URL`, and `SCARGO_TOKEN_ENCRYPTION_KEY` only when Dropbox is enabled.
4. Add dependencies only as needed for HTTP client calls, URL/query handling, OAuth state signing, and authenticated encryption.
5. Add a new API module, likely `src/api/dropbox.rs`, and register it from `src/api/mod.rs` and `src/api/routes.rs`.
6. Use the existing account/session boundary in `src/api/privacy.rs`; Dropbox connect and connection routes must require a real signed-in account, not guest fallback.
7. Implement OAuth start:
   - create a short-lived state value bound to account id and redirect target
   - store only a hash or signed form of state server-side
   - generate the Dropbox authorize URL with offline token access
   - use callback URL `${SCARGO_BASE_URL}/api/dropbox/oauth/callback`
8. Implement OAuth callback:
   - validate state before token exchange
   - exchange code with Dropbox token endpoint
   - encrypt refresh token and any access token retained
   - upsert one connection row per account
   - set path to `/OBD Fusion/CsvLogs`
   - clear previous sync cursor when reconnecting
9. Add schema bootstrap in `src/db/migrate.rs`:
   - `dropbox_connection`
   - `dropbox_oauth_state`, or an equivalent short-lived state store
   - `dropbox_ingest_file` if the worker task has not already added it
10. Suggested `dropbox_connection` columns:
    - `id UUID PRIMARY KEY`
    - `account_id UUID NOT NULL UNIQUE REFERENCES account(id)`
    - `dropbox_account_id TEXT NOT NULL`
    - `root_path TEXT NOT NULL DEFAULT '/OBD Fusion/CsvLogs'`
    - `encrypted_refresh_token BYTEA NOT NULL`
    - `encrypted_access_token BYTEA`
    - `access_token_expires_at TIMESTAMPTZ`
    - `cursor TEXT`
    - `status TEXT NOT NULL DEFAULT 'active'`
    - `last_sync_at TIMESTAMPTZ`
    - `last_success_at TIMESTAMPTZ`
    - `latest_error TEXT`
    - `created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`
    - `updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`
11. Keep encrypted token material out of logs, debug output, JSON responses, and errors.
12. Add unit tests for encryption round trip, wrong-key failure, OAuth state validation, expired state rejection, and account ownership.

## Tools and commands to use
- Inspect status before edits: `git status --short --branch`
- Review current surfaces with lean-ctx:
  - `ctx_read src/config/settings.rs`
  - `ctx_read src/api/auth.rs`
  - `ctx_read src/api/privacy.rs`
  - `ctx_read src/api/routes.rs`
  - `ctx_read src/db/migrate.rs`
  - `ctx_read Cargo.toml`
- Format and validate:
  - `cargo fmt`
  - `cargo test`

## Relevant files, data, and context
- `src/config/settings.rs` owns environment parsing and production/dev validation.
- `src/api/privacy.rs` owns session cookie and bearer-token account resolution.
- `src/api/auth.rs` is the reference for signed-in account API patterns.
- `src/api/routes.rs` wires `/api/*` routes.
- `src/db/migrate.rs` is clean schema bootstrap, not a legacy migration layer.
- `.env.example`, `README.md`, and `AGENTS.md` must be updated by the docs task when config becomes real behavior.

## Acceptance checks and tests
- `cargo test`
- OAuth state cannot be reused across accounts.
- Expired or tampered OAuth state returns an error and stores no token.
- Guest users cannot connect Dropbox.
- Signed-in users can connect, inspect status, pause/resume, and disconnect only their own connection.
- API responses include `connected`, `status`, `root_path`, `last_sync_at`, `last_success_at`, `latest_error`, and no token fields.
- Database bootstrap on an empty database creates Dropbox tables and indexes.

## Suggested branch name
- `feature/dropbox-oauth-storage`
