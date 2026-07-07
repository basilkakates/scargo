# Dedicated Auth Page

## Goal and success criteria
- Add a dedicated account creation and login page for dashboard users.
- Gate the dashboard behind that entry page for unauthenticated sessions.
- Keep guest access available in dev/test only as an explicit user action.
- Preserve the existing auth API contract and upload-token behavior.
- Leave the dashboard with signed-in account controls only, not login/register inputs.

## Implementation instructions
1. Add a new static `dashboard/static/auth.html` page plus companion script.
2. Reuse the existing auth API endpoints: `/api/auth/register`, `/api/auth/login`, `/api/auth/logout`, `/api/auth/me`, and `/api/auth/tokens`.
3. Build one shared auth form with login and create-account modes using `username` and `password` only.
4. Surface the existing backend validation rules in the page copy:
   - username `3-80` chars
   - allowed chars `a-z`, `0-9`, `.`, `_`, `-`, `@`
   - password minimum `8` chars
5. On auth-page load, call `/api/auth/me`:
   - redirect real signed-in accounts to `/`
   - keep guest fallback on the page and expose an explicit `Continue as guest` action
   - stay on the page for unauthenticated failures
6. On successful register or login, redirect to `/`.
7. If register returns an `upload_token`, carry it through redirect in client storage and display it once on the dashboard.
8. Remove login/register controls from the dashboard page and replace them with a compact signed-in account strip:
   - signed-in users: account label, logout, upload-token generation
   - guest users: guest label, sign-in link/button, no token controls
9. Update dashboard boot logic so unauthenticated users, and guest sessions without explicit guest consent, are redirected to `/auth.html`.
10. Keep guest consent and one-time register token display in browser storage only.
11. Update repo docs to describe the dedicated auth page and explicit guest flow.

## Tools and commands to use
- Inspect status before edits: `git status --short --branch`
- Review app shape with lean-ctx:
  - `ctx_read dashboard/static/index.html`
  - `ctx_read dashboard/static/app.js`
  - `ctx_read src/main.rs`
  - `ctx_read src/api/auth.rs`
  - `ctx_read docs/privacy-model.md`
- Validate after edits:
  - `cargo test`
  - `cargo test --test smoke_stack -- --ignored --nocapture`

## Relevant files, data, and context
- `dashboard/static/index.html` currently contains inline login/register controls.
- `dashboard/static/app.js` already calls `/api/auth/me`, `/api/auth/login`, `/api/auth/register`, `/api/auth/logout`, and `/api/auth/tokens`.
- `src/main.rs` serves static files from `dashboard/static/`, so `auth.html` can be added without new Rust routing.
- `src/api/auth.rs` is the source of truth for request shapes and validation limits.
- `README.md` and `docs/privacy-model.md` need updates for any dashboard auth-flow change.
- Existing dashboard design rules live in `docs/dashboard-creative-direction.md`.

## Acceptance checks and tests
- `GET /auth.html` returns `200`.
- Unauthenticated browser sessions land on `/auth.html` before the dashboard.
- Register creates an account, sets session state, and lands on the dashboard.
- Login lands on the dashboard without showing guest state.
- Dev/test guest flow requires explicit `Continue as guest`.
- Guest sessions cannot generate upload tokens from dashboard UI.
- Logout returns the browser to `/auth.html`.
- `cargo test` passes.
- `cargo test --test smoke_stack -- --ignored --nocapture` passes.

## Suggested branch name
- `feature/dedicated-auth-page`
