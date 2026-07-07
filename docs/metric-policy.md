# Metric Policy

Scargo keeps raw CSV ingest broad but keeps long-term and public aggregates
narrow. The policy for each metric key is defined in `src/ingest/canonical.rs`
and returned by `/api/channels`.

## Fields

| Field | Meaning |
|-------|---------|
| `category` | Logger/source class: `sae_pid`, `fuel`, `trip`, `calculated_pid`, `gps`, `sensor`, `adapter`, `system`, or `unknown` |
| `sensitivity` | `public_vehicle` can appear in anonymous vehicle cohorts; `owner_only`, `location`, and `phone_sensor` cannot |
| `rollup` | Metric is eligible for `vehicle_metric_day` daily rollups |
| `public_cohort` | Metric can be returned from `/api/analysis/cohort/{channel}` |
| `derived_preferred` | Prefer deriving this later from smaller raw inputs instead of asking clients to upload it forever |

Unknown future keys default to `owner_only`, no daily rollup, and no public
cohort exposure. This is conservative for EV, hybrid, phone, GPS, and
logger-specific fields Scargo has not classified yet.

## Category Rules

| Category | Examples | Rollup/public rule |
|----------|----------|--------------------|
| SAE PID | `vehicle_speed`, `engine_rpm`, trim, throttle, temperature, voltage | Roll up and allow public cohorts when numeric and non-identifying |
| Fuel | `instant_fuel_economy`, `fuel_rate`, `co2_flow` | Roll up public efficiency/rate values; keep totals and remaining fuel owner-only |
| Trip | distance, duration, idling, hard brake/accel, cost, average/max speed | Owner-only raw; do not roll up into public averages |
| Calculated PID | MAP, MAF, boost, power, torque, A/F | Roll up mechanical values; keep fuel remaining, distance-to-empty, and acceleration owner-only |
| GPS | latitude, longitude, altitude, bearing, GPS speed, accuracy | Owner-only raw; no rollups; no public cohorts |
| Sensor | phone acceleration, gravity, rotation, pitch/roll, magnetometer | Owner-only raw; no rollups; no public cohorts |
| Adapter/system | adapter voltage, PID refresh rate | Owner-only raw; no public cohorts |

## Minimal Measurement Strategy

Measure standardized vehicle signals first: the SAE PIDs each vehicle actually
supports, timestamp, vehicle key, and offline vehicle metadata needed for
cohorts (`year`, `make`, `model`, `engine_family`). Do not require every vehicle
to produce the same PID set; ICE, hybrid, PHEV, and EV data will differ.

Keep logger-derived fuel, CO2, trip, power, torque, acceleration, and cost values
only as raw owner-visible inputs while Scargo learns correlations. When a value
can be derived reliably from a smaller source set, prefer deriving it in analysis
or future client preprocessing instead of treating it as a required upload field.

Do not add gasoline price, displacement, or other external lookup data to ingest
paths until a specific calculator needs it. External values are inputs to future
derivations, not raw telemetry.
