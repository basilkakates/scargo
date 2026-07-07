# Vehicles Sharing Approval UI

## Goal and success criteria

- Add vehicles-page controls for approving pending exact-VIN and cohort public
  stats when the backend capability allows it.
- Keep controls hidden for guests, production sessions, and users without
  pending approvals.
- Preserve the current vehicle-management visual system.

## Dependency

- This plan depends on the API contract from
  `../02-sharing-approval-api/PLAN.md`.
- If implemented before the backend lands, keep changes behind defensive checks
  so the current vehicles page still works without the new fields.

## Implementation instructions

1. Read the auth capability:
   - Update `dashboard/static/vehicles.js` to read
     `capabilities.approve_pending_public_stats`.
   - Treat missing `capabilities` as false for backward compatibility.
2. Render pending approval actions:
   - Show approval actions only when
     `capabilities.approve_pending_public_stats` is true.
   - Show exact-VIN approval only when
     `exact_vin_share_enabled=true` and
     `exact_vin_pending_approval_count > 0`.
   - Show cohort approval only when `cohort_pending_approval_count > 0`.
   - Keep existing `Public` and `Pending approval` pill behavior.
   - Do not add a new `Partial` status.
3. Add action behavior:
   - POST exact-VIN approvals to
     `/api/vehicles/{vehicle_id}/approve-exact-vin-sharing`.
   - POST cohort approvals to
     `/api/vehicles/{vehicle_id}/approve-cohort-sharing`.
   - Reload the vehicle list after each successful approval.
   - Show a page status message containing result counts.
   - Surface backend errors through the existing page status pattern.
4. Add page copy and styles:
   - Update `dashboard/static/vehicles.html` copy so it explains that pending
     public exposure is still approval-gated.
   - Clarify that cohort approval does not override metric-policy restrictions
     or missing metadata requirements.
   - Add only the CSS needed for the approval action group.
   - Keep buttons and spacing consistent with the existing vehicles page.
5. Update docs:
   - Update `README.md` or `AGENTS.md` if the user-visible vehicles-page flow or
     endpoint behavior changed from the backend plan docs.
   - Do not duplicate privacy-policy text if
     `02-sharing-approval-api/PLAN.md` already updated `docs/privacy-model.md`.

## Tools and commands to use

- Inspect status before edits:
  - `git status --short --branch`
- Review frontend files with lean-ctx:
  - `ctx_read dashboard/static/vehicles.html`
  - `ctx_read dashboard/static/vehicles.js`
  - `ctx_read dashboard/static/auth.js`
  - `ctx_read README.md`
  - `ctx_read AGENTS.md`
- Search current sharing UI references:
  - `ctx_search "Pending approval|exact_vin|cohort|sharing" dashboard/static`
- Validate after edits:
  - `cargo test`
  - Manual browser check of `/vehicles.html` in guest and signed-in dev/test
    sessions if feasible.

## Relevant files, data, and context

- `dashboard/static/vehicles.js` already renders vehicle sharing state.
- `dashboard/static/vehicles.html` owns vehicles-page explanatory copy and CSS.
- Backend response fields expected by this plan:
  - `capabilities.approve_pending_public_stats`
  - `exact_vin_pending_approval_count`
  - `cohort_pending_approval_count`
- Backend approval endpoint response fields expected by this plan:
  - `vehicle_id`
  - `approval`
  - `approved_upload_count`
  - `already_approved_upload_count`

## Acceptance checks and tests

- Approval controls are hidden when the auth capability is missing or false.
- Exact-VIN approval appears only for enabled exact-VIN sharing with pending
  exact-VIN approvals.
- Cohort approval appears only when cohort approvals are pending.
- Buttons include remaining counts, for example
  `Approve remaining exact-VIN uploads (2)`.
- Successful approval reloads the vehicle list.
- Success messages include approved and already-approved counts.
- Backend errors display through the existing page status pattern.
- Vehicles with mixed approved and pending uploads still show existing status
  pills correctly.
- UI remains usable in dark and light modes.

## Suggested branch name

- `feature/vehicles-sharing-approval-ui`
