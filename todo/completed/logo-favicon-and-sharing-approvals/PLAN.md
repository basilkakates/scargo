# Logo, Favicon, And Sharing Approval Plan Set

This folder breaks the original combined plan into independently claimable
implementation plans.

## Claimable plans

1. `01-branding-assets/PLAN.md`
   - Updates the shared Scargo logo asset, adds a favicon, wires favicon links,
     and updates branding docs.
   - Suggested branch: `feature/logo-favicon-assets`

2. `02-sharing-approval-api/PLAN.md`
   - Adds the auth capability flag, vehicle pending approval counts, and
     dev/test-only approval endpoints.
   - Suggested branch: `feature/sharing-approval-api`

3. `03-vehicles-approval-ui/PLAN.md`
   - Adds vehicles-page controls for pending sharing approvals using the API
     contract from the backend plan.
   - Suggested branch: `feature/vehicles-sharing-approval-ui`

## Coordination notes

- `01-branding-assets` can be implemented independently.
- `02-sharing-approval-api` can be implemented independently.
- `03-vehicles-approval-ui` should start after, or in coordination with,
  `02-sharing-approval-api` because it depends on the new auth and vehicle JSON
  fields plus approval endpoints.
- Each plan includes its own documentation and validation requirements. Do not
  defer docs into a separate cleanup plan.
- Preserve the privacy boundary from the original plan: approval actions must
  never approve uploads outside the current account's linked private uploads.

## Original combined goal

- Make the Scargo logo render well in both dark and light mode without swapping
  assets between themes.
- Add a favicon derived from the same brand mark and serve it on dashboard pages.
- Add owner-visible, dev/test-only controls to approve pending exact-VIN and
  cohort sharing stats from the vehicles page.
- Reuse existing `ingest_upload` approval timestamps instead of adding schema.
- Keep repo docs in sync with API, UI, asset, and policy changes.
