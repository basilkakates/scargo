# Hosted Beta Onboarding

## Goal and success criteria

Make the first hosted-beta session clear for a real user.

Success means an early user can create an account, connect Dropbox or upload a
CSV, see vehicles, understand guest limitations, and recover from common setup
errors without operator intervention.

## Implementation instructions

1. Review `/auth.html`, `/`, `/dropbox.html`, and `/vehicles.html` as one flow.
2. Clarify guest versus signed-in behavior without adding a marketing landing
   page.
3. Improve empty states for no vehicles, no Dropbox connection, no synced files,
   and no visible metrics.
4. Keep auth and upload on the existing session-cookie model.
5. Avoid new frontend dependencies or build tooling.
6. Update README and AGENTS only if behavior or required setup changes.

## Tools and commands to use

- `cargo test`
- `scripts/smoke-docker.sh` when Docker is available
- Browser or Playwright verification if UI layout changes are significant
- `git diff --check`

## Relevant files, data, and context

- `dashboard/static/auth.html`
- `dashboard/static/auth.js`
- `dashboard/static/index.html`
- `dashboard/static/app.js`
- `dashboard/static/dropbox.html`
- `dashboard/static/dropbox.js`
- `dashboard/static/vehicles.html`
- `dashboard/static/vehicles.js`
- `src/api/auth.rs`
- `src/api/dropbox.rs`
- `src/api/vehicles.rs`

## Acceptance checks and tests

- A new user can reach the dashboard only after login/register or explicit dev
  guest consent.
- Guests cannot manage Dropbox connections.
- Empty states name the next action without exposing implementation details.
- Existing smoke test still passes.

## Suggested branch name

- `feature/hosted-beta-onboarding`
