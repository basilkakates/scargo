# Maintenance Inference V1

## Goal and success criteria

Infer likely maintenance needs and likely completed maintenance from telemetry
changes without requiring users to manually log maintenance events.

Success means Scargo can flag candidate changes for owner review using measured
signals and can treat optional user confirmation as feedback, not required data
entry.

## Implementation instructions

1. Start with read-only analysis over existing rollups and recent raw owner data.
2. Detect candidate shifts in maintenance-relevant metrics such as fuel trim,
   coolant temperature, intake pressure, battery voltage, efficiency, and
   misfire-like patterns when the data exists.
3. Separate two outputs: possible issue developing and possible maintenance
   already occurred.
4. Keep confidence conservative and show supporting metric windows.
5. Do not build a mandatory maintenance-event logging UI.
6. If feedback is added, make it optional confirmation/dismissal of inferred
   events so it can train later heuristics.
7. Update docs for inference limits and privacy boundaries.

## Tools and commands to use

- `cargo test`
- `python3 scripts/analyze-telemetry.py` only for offline exploration
- `git diff --check`

## Relevant files, data, and context

- `scripts/analyze-telemetry.py`
- `src/api/dashboard.rs`
- `src/api/summary.rs`
- `src/api/pairs.rs`
- `docs/metric-policy.md`
- `docs/privacy-model.md`
- `docs/monetization-strategy.md`

## Acceptance checks and tests

- No required manual event entry is introduced.
- Inference uses only owner-visible raw data or allowed aggregate data.
- Low-data cases return no finding or insufficient-data status.
- Any optional feedback path avoids exposing raw telemetry to other users.

## Suggested branch name

- `feature/maintenance-inference-v1`
