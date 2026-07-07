# Shared Logo And Favicon Assets

## Goal and success criteria

- Make the Scargo logo render correctly in dark and light mode without
  theme-specific asset swapping.
- Add a favicon derived from the same Scargo mark.
- Serve the favicon from dashboard, auth, and vehicles pages.
- Keep branding docs aligned with the served assets.

## Implementation instructions

1. Inspect the current shared logo:
   - Review `dashboard/static/assets/scargo-logo.png`.
   - Confirm the transparent edge problem in light mode.
2. Replace the logo asset in place:
   - Keep the same path: `dashboard/static/assets/scargo-logo.png`.
   - Use an opaque background version.
   - Use the same dark cockpit fill across the full image boundary.
   - Keep one shared logo file for all themes and pages.
3. Add the favicon:
   - Create `dashboard/static/favicon.png`.
   - Base it on the same Scargo mark.
   - Use a square crop or composition legible at browser-tab sizes.
   - Keep the visual background aligned with the updated logo.
4. Wire the favicon:
   - Add `<link rel="icon" href="/favicon.png">` to
     `dashboard/static/index.html`.
   - Add the same tag to `dashboard/static/auth.html`.
   - Add the same tag to `dashboard/static/vehicles.html`.
5. Update docs:
   - Update `docs/dashboard-creative-direction.md` so the served logo rule
     reflects the shared opaque asset and favicon usage.
   - Update `README.md` or `AGENTS.md` only if their static-asset notes become
     stale during implementation.

## Tools and commands to use

- Inspect status before edits:
  - `git status --short --branch`
- Review files with lean-ctx:
  - `ctx_read dashboard/static/index.html`
  - `ctx_read dashboard/static/auth.html`
  - `ctx_read dashboard/static/vehicles.html`
  - `ctx_read docs/dashboard-creative-direction.md`
- Search branding references:
  - `ctx_search "scargo-logo|favicon" dashboard docs README.md AGENTS.md`
- Validate after edits:
  - Open `/`, `/auth.html`, and `/vehicles.html` locally if a server is running.
  - Confirm `/favicon.png` loads.
  - Run `cargo test` if HTML/doc edits are bundled with code changes.

## Relevant files, data, and context

- `dashboard/static/assets/scargo-logo.png` is the current shared logo asset.
- `dashboard/static/index.html`, `auth.html`, and `vehicles.html` reference the
  shared logo but do not define a favicon in the original plan state.
- `src/main.rs` serves `dashboard/static/` from both `/static` and `/`, so a
  tracked root-level favicon asset can be served without new routing logic.
- Keep generated binary assets small and inspectable enough for repo use.

## Acceptance checks and tests

- The shared Scargo logo looks correct in both dark and light mode.
- The dashboard does not need theme-specific logo switching.
- `/favicon.png` loads successfully.
- Browser tabs show the favicon for `/`, `/auth.html`, and `/vehicles.html`.
- `docs/dashboard-creative-direction.md` matches the new asset rule.
- `git diff --check` passes.

## Suggested branch name

- `feature/logo-favicon-assets`
