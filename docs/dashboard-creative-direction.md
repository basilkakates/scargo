# Scargo Dashboard Creative Direction

The dashboard should feel like a compact vehicle telemetry cockpit: dark,
fast, readable, and operational. It should use the Scargo mascot and logo as a
strong brand signal without turning the dashboard into a marketing page.

## Source References

The working creative references live in the ignored `creative_direction/`
folder. The dashboard serves one shared Scargo logo from
`dashboard/static/assets/scargo-logo.png` and a square favicon at
`dashboard/static/favicon.png`, so the app can render without serving the raw
creative-direction folder.

Primary references:
- `creative_direction/dashboard.png` for layout density, dark panels, and chart
  treatment.
- `creative_direction/scargo-logo.png` and `creative_direction/favicon.png` for
  the served Scargo logo and favicon. The served logo keeps one opaque,
  dark-cockpit background that works in both themes, and the favicon uses the
  same mark in a square high-resolution PNG composition.
- `creative_direction/c3a1252c-28c2-4602-8159-21d77689e745.png` for Scargo
  mascot style. The dashboard asset removes the clashing background and does
  not repeat the tagline.
- `creative_direction/Basil tech small.png` and
  `creative_direction/Basil tech full.png` for Basil Tech identity. The served
  Basil Tech treatment keeps clean live-rendered text and uses a simple
  animated-style basil leaf asset.
- `creative_direction/scargo.jpg` and `creative_direction/scargo2.jpg` for
  mascot energy, color, and motion cues.

## Visual Rules

- Support both dark and light modes with the same dashboard structure.
- Use a dark cockpit base with low-contrast panel borders as the default.
- Use purple as the main action/selection color.
- Use green for connected and healthy states.
- Use yellow for caution and brand energy.
- Use red only for errors or high-severity states.
- Keep charts dense, legible, and information-first.
- Keep cards at 8px radius or less.
- Avoid landing-page hero layouts, decorative blobs, and marketing copy.
- Do not repeat the tagline in page chrome when it already exists in a logo
  reference or campaign asset.
- Use the shared Scargo logo asset as the primary app brand in both themes and
  use `/favicon.png` as the browser-tab mark.
- Use Basil Tech as secondary maker branding in the sidebar/footer, not as the
  dashboard title. Keep the text clean and use only the leaf as the illustrated
  brand accent.

## Dashboard Behavior

The visual refresh must preserve the existing dashboard contract:
- The app stays vanilla HTML, CSS, and JavaScript under `dashboard/static/`.
- Chart.js stays loaded from CDN.
- A dedicated `auth.html` entry page can share the visual system, but the main
  dashboard at `/` should show signed-in account chrome only, not inline
  login/register fields.
- Reads and dashboard CSV uploads use the dashboard session cookie.
- Vehicle, time range, metric, unit, raw, and summary controls continue to use
  the existing API endpoints.
- Theme selection is a front-end preference stored in browser local storage.
- No build-tool change is required for this design direction.
