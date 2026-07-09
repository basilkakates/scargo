# Cloud Database Choice

## Goal and success criteria

- Choose a production database path for a small hosted Scargo launch.
- Compare managed Timescale-compatible PostgreSQL against self-hosted TimescaleDB.
- Keep the recommendation cost-aware for a few early users and reversible if usage grows.

Success means the repo has a concise deployment-options note with one recommended v1
database path, one fallback path, expected monthly cost shape, backup ownership, and
TimescaleDB support caveats.

## Implementation instructions

1. Create `docs/deployment-options.md`.
2. Use current official docs to verify whether each provider supports TimescaleDB.
3. Compare at least Tiger Cloud, Azure Database for PostgreSQL Flexible Server,
   AWS RDS PostgreSQL, Google Cloud SQL PostgreSQL, and a small VM/container host.
4. Treat Scargo's hard requirement as PostgreSQL with the `timescaledb` extension.
5. Recommend the smallest responsible v1 path for a few users.
6. Document when to switch paths: cost ceiling, backup burden, operational pain, or
   provider extension limits.

## Tools and commands to use

- Official provider docs only for support claims.
- `git diff --check`

## Relevant files, data, and context

- `compose.yaml`
- `README.md`
- `AGENTS.md`
- `src/config/settings.rs`
- Existing production env requirement: `SCARGO_ENV=production` requires
  `SCARGO_DATABASE_URL`.

## Acceptance checks and tests

- The note states which option owns backups, upgrades, monitoring, and restore drills.
- The note does not commit Scargo to one cloud vendor for app hosting.
- The note keeps real secrets out of tracked files.
- The note includes official-source links for every provider claim.

## Suggested branch name

- `research/deployment-cloud-database-choice`
