# Scargo Roadmap

Scargo's current direction is a hosted beta for owner-focused OBD2 telemetry.
The product should become easy to deploy, easy to connect to Dropbox, and useful
for a vehicle owner before deeper analytics or commercial data products expand.

## Hosted Beta Goals

- Run as an always-on web app with a managed TimescaleDB-compatible database.
- Let early users create an account, connect Dropbox, ingest CSVs, and inspect
  vehicles without operator help.
- Keep raw telemetry private to the owner while building aggregate cohort value.
- Produce owner-facing health reports from existing telemetry before adding new
  collection burdens.
- Infer maintenance need and likely completed maintenance from measured data
  instead of requiring users to log every event by hand.

## Active Plan Sets

- `todo/deployment-readiness/` chooses the deploy shape, container path, local
  dev loop, and production runbook.
- `todo/docs-roadmap-refresh/` keeps docs organized and current.
- `todo/hosted-beta-onboarding/` improves account, upload, Dropbox, vehicle, and
  guest/production flows for early users.
- `todo/dropbox-ingest-operability/` improves sync visibility and hosted
  diagnostics.
- `todo/owner-health-report-v1/` turns existing owner-scoped telemetry into a
  first useful report.
- `todo/maintenance-inference-v1/` plans maintenance detection from telemetry
  changes, not manual event entry.
- `todo/cohort-coverage-beta-access/` defines data-quality scoring and early
  contributor access rules.

## Supporting Docs

- `README.md` is the user-facing setup and capability overview.
- `AGENTS.md` is the developer/agent source of truth for repo shape and workflow.
- `docs/privacy-model.md` defines ownership, sharing, cohorts, and cost controls.
- `docs/metric-policy.md` defines which metrics can roll up or become public
  aggregate cohort inputs.
- `docs/deployment-options.md` compares production database and app-hosting
  options.
- `docs/dashboard-creative-direction.md` defines the dashboard visual system.
- `docs/monetization-strategy.md` captures longer-term product and revenue
  ideas.

## Deferred Directions

- Mobile preprocessing upload format.
- Explicit vehicle transfer endpoint.
- Fully automated retention enforcement beyond the documented target.
- Shop partner workflows and aggregate reliability products.
- Manual maintenance-event entry, unless inference needs sparse optional labels
  for validation.
