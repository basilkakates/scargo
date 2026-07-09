# Monetization Strategy

Scargo should start as a paid vehicle-health product for owners. The clean
business model is: users pay for private diagnostics, owner-controlled reports,
and aggregate comparisons. Later revenue can come from aggregate mechanical
trend reports.

Scargo also has a data-network effect: each useful upload improves the baseline
for future users. Early telemetry should be valued by the coverage it creates,
not only by immediate subscription revenue.

## Data value model

Value each upload by what it improves:

- Coverage value: fills a missing year/make/model/engine cohort.
- Rarity value: covers a vehicle cohort with few or no examples.
- Quality value: complete CSVs, useful metric coverage, repeated trips, and
  clean timestamps.
- Recency value: fresh data keeps baselines current.
- Longitudinal value: repeated uploads from the same vehicle show drift and
  aging.
- Event value: data near maintenance, repairs, part changes, fuel changes, or
  inspections.
- Comparison value: improves normal ranges and percentiles for other users.

Use a simple internal score while the product is young:

`data_value = coverage_gap + quality + rarity + recency + longitudinal_depth + event_context`

Use that score to decide which early users get free access, which vehicle
cohorts to recruit, and when a cohort is ready for stronger reports.

## Revenue lines

### Consumer subscription

Offer a free tier for CSV upload, basic dashboard use, and recent history.
Charge for longer history, cohort comparisons, anomaly tracking, saved reports,
AI-assisted maintenance summaries, multi-vehicle support, and deeper trend
explanations.

The product promise is simple: help owners understand whether their car is
behaving normally.

### Friends and family contributor access

Give trusted early users free premium access while they help build baselines.
The useful exchange is consistent uploads and feedback in return for better
personal reports, longer history, and early access to cohort features.

Prioritize contributors by cohort coverage and data quality rather than total
upload count.

### AI-assisted maintenance alerts

Use uploaded telemetry to detect meaningful changes in maintenance-relevant
signals such as fuel trim, coolant temperature, intake pressure, battery
voltage, misfire-like patterns, efficiency drift, and related metrics.

Each alert should show:

- What changed.
- How long it has been happening.
- How the vehicle compares with similar vehicles.
- The supporting metric trend.
- A confidence level.
- A practical next inspection step.

### Owner-controlled vehicle reliability report

Let an owner create a paid, time-limited reliability report for selling, buying,
or pre-inspection.

The report should include upload coverage, health trends, anomaly history,
maintenance-relevant signals, and cohort percentile comparisons. The buyer gets
a clean summary rather than raw trip history.

### Mechanic-ready summary

Convert alerts into a short service-note format that an owner can bring to a
shop. Monetize this through premium subscription features, shop referrals, or
shop accounts.

The useful output is: which metrics changed, when they started changing, which
systems are worth inspecting, and the supporting charts.

### Shop partner access

Give a small number of trusted shops partner access once owner reports are useful
enough to explain. Shops can add structured maintenance context and receive
better before/after reports.

Shop value:

- Before/after maintenance comparisons.
- Cleaner customer-facing service notes.
- Evidence that a repair changed relevant metrics.
- Fleet-style view for vehicles they service repeatedly.

Scargo value:

- Maintenance-labeled data.
- Before/after examples.
- More repeated observations per vehicle.
- Stronger alert examples.
- Faster coverage for common vehicles.

Best first version:

- Shop uses a partner account.
- Vehicle owner links a vehicle or report to that shop.
- Shop adds a maintenance event label with service type, date, rough mileage,
  and notes.
- Scargo compares pre/post metrics and produces a mechanic-ready summary.
- Useful event-linked uploads earn free or discounted partner usage.

### Aggregate reliability intelligence

After cohort sizes are strong, package make/model/year/engine trend reports for
manufacturers, parts companies, warranty providers, fleets, used-car
marketplaces, repair networks, and automotive researchers.

Useful products include:

- Common long-term sensor drift by vehicle cohort.
- Reliability outliers by make, model, year, and engine family.
- Maintenance-pattern benchmarks.
- Seasonal performance differences.
- Aggregate regional or roadway trend reports when grouped as broad statistical
  summaries.

## Data product shapes

All monetized products should fit one of these shapes:

- Private owner dashboard.
- Owner-created share report.
- Shop-linked before/after maintenance summary.
- Cohort benchmark inside the app.
- Aggregate make/model/year/engine report.
- Broad aggregate regional or roadway trend report.

The product does not need customer identity, raw trip replay, peer vehicle
identity, or individual driver scoring to create value.

## Launch order

1. Track cohort coverage and data quality internally.
2. Offer free premium access to friends and family contributors.
3. Add maintenance-event labels to owner reports.
4. Invite 1-3 trusted shops as partner accounts.
5. Build before/after maintenance summaries.
6. Use event data to improve AI-assisted maintenance alerts.
7. Convert the best reports into paid consumer and shop offerings.
8. Package aggregate reliability reports for make/model/year/engine cohorts.
9. Add broad regional trend reports after aggregate reporting is proven in-product.

## Product assumptions

- Scargo stays consumer-first.
- Raw telemetry powers the owner's private experience.
- Shared reports are created by the owner.
- Free access is earned by useful contribution, not unlimited by default.
- Shops receive better reports because they add structured maintenance context.
- Cohort value comes from aggregate comparisons.
- Business-to-business value comes from trend summaries.
- Storage cost stays managed through retention, rollups, and contribution
  scoring.
