# Deployment Options

Last checked: 2026-07-08.

## Recommendation

Use Tiger Cloud Performance as the v1 production database for a small hosted
Scargo launch.

Scargo is not a generic PostgreSQL app. Startup creates the `timescaledb`
extension, turns `obd2_metric_reading` into a hypertable, enables TimescaleDB
compression, and creates a continuous aggregate. Any production database must
support PostgreSQL plus the `timescaledb` extension.

Tiger Cloud is the smallest responsible v1 path because Tiger's extension docs
list `timescaledb` for all Tiger Cloud services, and the pricing page gives a
clear low starting point: Performance compute starts at $30/month, storage is
priced per GB-month, and Performance includes point-in-time recovery,
Performance Insights, and basic support. The app can still run on any host that
can reach the database over `SCARGO_DATABASE_URL`; this choice does not commit
Scargo app hosting to one cloud vendor.

Use a small self-hosted TimescaleDB VM or container as the fallback when monthly
managed-database cost is the hard blocker. That path is cheaper on cash spend
but moves backups, restore drills, monitoring, patching, extension upgrades,
failover, and incident response to the Scargo operator.

## Scargo requirements

- PostgreSQL with the `timescaledb` extension available to `CREATE EXTENSION`.
- Hypertables, TimescaleDB compression policy, and continuous aggregates.
- `SCARGO_ENV=production` with an explicit `SCARGO_DATABASE_URL` supplied by the
  environment or an ignored `.env` file.
- No real database URLs or passwords in tracked files.

## Options

| Option | TimescaleDB support | Cost shape | Ownership | Fit |
| --- | --- | --- | --- | --- |
| Tiger Cloud Performance | Yes. Tiger lists `timescaledb` for all Tiger Cloud services. | Starts at $30/month compute plus GB-month storage; HA replicas and higher tiers increase cost. | Provider handles managed database platform, PITR capability, monitoring surfaces, HA mechanics, and service upgrades; Scargo owner still owns sizing, spend review, secrets, backup policy, and restore drills. | Recommended v1. |
| Azure Database for PostgreSQL Flexible Server | Yes. Microsoft lists `timescaledb`; it requires `shared_preload_libraries`. | vCore-hour compute plus provisioned storage and consumed backup storage. HA doubles provisioned primary/secondary resources. | Azure owns managed database platform mechanics; Scargo owner owns extension enablement, server parameters, sizing, cost review, secrets, and restore drills. | Viable managed fallback if Azure hosting/account constraints matter. |
| AWS RDS for PostgreSQL | Not a v1 option. AWS's supported-extension page does not list `timescaledb` as of this check. | Instance-hour compute plus storage, backup, network, and HA/standby costs if used. | AWS would own managed database platform mechanics, but the missing extension blocks Scargo. | Reconsider only after AWS official docs list `timescaledb`. |
| Google Cloud SQL for PostgreSQL | Not a v1 option. Google says Cloud SQL can install only supported extensions, and its supported-extension page does not list `timescaledb` as of this check. | vCPU-hour plus memory-hour, storage, backup, network, and HA costs if used. | Google would own managed database platform mechanics, but the missing extension blocks Scargo. | Reconsider only after Google official docs list `timescaledb`. |
| Small VM/container host | Yes if the operator installs TimescaleDB, for example with the official Docker image path. | VM/container monthly cost plus disk, snapshots, backup storage, monitoring, and operator time. | Scargo owner owns backups, PITR or snapshot strategy, restore drills, monitoring, OS and database patching, TimescaleDB upgrades, failover, and incidents. | Cost fallback, not the default. |

## App hosting options

Keep the production database managed unless there is a deliberate decision to
self-host TimescaleDB. The app host only needs to run the Scargo Rust web server,
serve `dashboard/static`, keep environment config secret, and reach the database
through `SCARGO_DATABASE_URL`.

Recommended app-hosting shape for v1: use a small always-on Linux host when the
Dropbox poller must run inside the app process. Use a serverless container host
only when polling is disabled, split into a scheduled worker, or the platform is
configured with a minimum warm instance.

| Option | App fit | Cost shape | Ownership | Fit |
| --- | --- | --- | --- | --- |
| Amazon EC2 small Linux instance | Good. EC2 gives full OS control, persistent long-running processes, security groups, and straightforward Docker or `systemd` deployment. | Instance-hour or per-second compute plus EBS, data transfer, IPv4, and optional load balancer/snapshot costs. | AWS owns hardware and virtualization; Scargo owner owns OS patching, firewall rules, TLS/proxy, deploys, logs, process restarts, and monitoring. | Good AWS box option, especially with Tiger Cloud as the database. |
| Amazon Lightsail VPS | Good for the simplest AWS-owned box. Lightsail bundles easy VPS instances, networking, storage, and predictable monthly plans. | Predictable monthly instance plan plus overage or extra AWS resources. | Similar to EC2, but simpler surface area and fewer knobs. | Good if AWS is preferred but EC2 feels too broad. |
| Google Cloud Run | Good if Scargo can tolerate scale-to-zero or the poller is externalized. Cloud Run runs containers and can host web apps with SQL access. | Request/resource billing with free monthly grants; idle cost can be near zero when scaled to zero. | Provider owns container platform; Scargo owner owns image, env, secrets, min-instance choice, and worker/poller design. | Best low-ops serverless container path. |
| Azure Container Apps | Good managed container alternative. It supports API endpoints, jobs/background processing, scale-to-zero, and a free monthly grant. | Per-second vCPU/RAM/request usage with optional idle minimum replicas. | Provider owns container platform; Scargo owner owns image, env, secrets, scaling, and worker layout. | Strong if Azure account/region alignment matters. |
| DigitalOcean App Platform | Good simple PaaS. It supports Git or container deploys, automatic HTTPS, app metrics, rollback, and paid tiers from about $5/month. | Predictable app-platform tiers; cost grows with instance size, autoscaling, and bandwidth. | Provider owns platform; Scargo owner owns image/source deploy config, env, and service sizing. | Easiest non-hyperscaler small launch path. |
| Render or Heroku | Good Heroku-style PaaS choices with low operational overhead and container/buildpack support. | Monthly service tiers; usually simpler but less flexible than a raw VM. | Provider owns platform; Scargo owner owns app config, env, and scaling choice. | Good if developer experience matters more than lowest cost. |

Do not choose AWS App Runner for a new Scargo deployment. AWS says App Runner no
longer accepts new customers as of 2026-04-30 and recommends ECS Express Mode
for containerized applications.

## Rough monthly cost model

This is an order-of-magnitude model for launch planning, not a bill forecast.
The live database was not available during this check, and the repo contains only
compact CSV parser fixtures, so replace these assumptions after a real
`rollup-retention-report.py` run against uploaded vehicle data.

Assumptions:

- 1.5 trips per user per day, or 45 trips per user per 30-day month.
- One trip is modeled as a 30-minute CSV shaped like the current compact fixture:
  about 83 metrics every 4.3 seconds, rounded to 35,000 metric readings per
  trip after Scargo expands sample rows into per-metric rows.
- 75 bytes per compressed TimescaleDB metric row including rough table and index
  overhead. Plain PostgreSQL without Timescale compression is modeled as 5x
  larger, matching Tiger's published average compression basis.
- 180 days of compressed raw retention is the default. Durable daily rollups are
  small compared with raw rows at these assumptions.
- One dashboard query per user per month. Query cost is negligible compared with
  ingest and storage at these traffic levels.
- Tiger Cloud Performance base compute is modeled at $30/month plus $0.177 per
  GB-month storage. Tiger Scale is modeled at $36/month plus $0.212 per GB-month
  hot storage. Tiger tiered storage is modeled at $0.021 per GB-month after the
  first 30 days of hot data.
- Self-hosted storage-only comparisons use the Amazon EBS gp3 example rate of
  $0.08 per GB-month and exclude instance, backup, snapshot, monitoring, and
  operator time.
- App host cost uses rough Lightsail-style always-on sizing: $12/month at 10
  users, $24/month at 100, $84/month at 1,000, and $164/month at 10,000. Larger
  production deployments may need HA, support, replicas, or a different app tier.

| Users | Trips/month | Metric rows/month | New compressed raw/month | 180-day compressed raw | Tiger Performance DB/month | App + Tiger DB/month |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 10 | 450 | 15.75 million | 1.2 GB | 7.1 GB | $31 | $43 |
| 100 | 4,500 | 157.5 million | 11.8 GB | 70.9 GB | $43 | $67 |
| 1,000 | 45,000 | 1.575 billion | 118.1 GB | 708.8 GB | $155 | $239 |
| 10,000 | 450,000 | 15.75 billion | 1.18 TB | 7.1 TB | $1,284+ | $1,448+ |

At 10,000 users the storage number crosses the simple small-launch envelope.
Expect a higher database tier, explicit retention policy, rollup-first product
behavior, or cold/tiered storage before taking that level of ingest seriously.

### Retention variants

Total estimated monthly cost with the app host plus Tiger Performance DB:

| Raw retention | 10 users | 100 users | 1,000 users | 10,000 users |
| --- | ---: | ---: | ---: | ---: |
| 30 days | $42 | $56 | $135 | $403 |
| 90 days | $43 | $60 | $177 | $821 |
| 180 days | $43 | $67 | $239 | $1,448 |
| 365 days | $45 | $79 | $368 | $2,738 |

Default to 180 days of raw retention for launch, then keep daily and monthly
rollups indefinitely so long-range charts do not require raw-row storage.

### TimescaleDB savings or losses

TimescaleDB is a storage and operations win for Scargo's metric table, but Tiger
Cloud is not the cheapest possible place to store bytes.

| Users | Tiger Performance with TimescaleDB | Plain PostgreSQL equivalent | Storage saved by compression | Monthly DB savings |
| --- | ---: | ---: | ---: | ---: |
| 10 | $31 | $36 | 28 GB | $5 |
| 100 | $43 | $93 | 284 GB | $50 |
| 1,000 | $155 | $657 | 2.8 TB | $502 |
| 10,000 | $1,284 | $6,302 | 28.4 TB | $5,018 |

### TimescaleDB options

180-day DB-only comparison:

| Option | 10 users | 100 users | 1,000 users | 10,000 users | Use when |
| --- | ---: | ---: | ---: | ---: | --- |
| Tiger Performance, hot storage | $31 | $43 | $155 | $1,284 | Recommended v1 default |
| Tiger Scale, hot storage | $38 | $51 | $186 | $1,539 | Higher limits or features are required |
| Tiger Scale, 30 days hot plus tiered storage | $36 | $40 | $73 | $410 | Long retention matters and old raw data is rarely queried |
| Self-hosted TimescaleDB storage only on gp3 | $1 | $6 | $57 | $567 | Cash cost matters more than operations |
| Plain PostgreSQL, no Timescale compression | $36 | $93 | $657 | $6,302 | Not recommended for Scargo metrics |

Net read:

- At 10 users, Tiger Cloud can cost more than a tiny self-hosted database because
  the managed base compute dominates the bill. The savings are mostly reduced
  operational work, not storage dollars.
- At 100 users, TimescaleDB compression starts to matter, but the main win is
  still lower database maintenance and safer managed backups.
- At 1,000 users and above, using TimescaleDB features is materially cheaper than
  keeping uncompressed raw rows in plain PostgreSQL, even before counting faster
  time-window queries and continuous aggregates.
- Tiger Scale with tiered storage is the cheapest managed retention path in this
  rough model once retained raw data is measured in hundreds of GB.
- A self-hosted TimescaleDB box can be cheaper on infrastructure line items, but
  it moves backup, restore, patching, monitoring, disk growth, and incident work
  back to the operator.

## Switch triggers

- Switch from Tiger Cloud to self-hosted only if the managed bill exceeds the
  early-user cost ceiling and the operator accepts backup and uptime ownership.
- Switch from self-hosted to a managed database when restore drills, patching,
  monitoring, or downtime response becomes operational pain.
- Switch from Tiger Cloud to Azure only if app hosting, account ownership, or
  regional requirements make Azure materially simpler and Azure still officially
  supports `timescaledb` for the target PostgreSQL version.
- Do not use AWS RDS or Google Cloud SQL until their official extension docs
  list `timescaledb`.

## Source links

- Tiger Cloud Postgres extensions: <https://docs.tigerdata.com/use-timescale/latest/extensions/>
- Tiger Cloud pricing: <https://www.tigerdata.com/pricing>
- Tiger self-hosted Docker install: <https://docs.tigerdata.com/self-hosted/latest/install/installation-docker/>
- Azure extension allow-list: <https://learn.microsoft.com/en-us/azure/postgresql/flexible-server/concepts-extensions>
- Azure extension versions by name: <https://learn.microsoft.com/en-us/azure/postgresql/extensions/concepts-extensions-versions>
- Azure Flexible Server pricing: <https://azure.microsoft.com/en-us/pricing/details/postgresql/flexible-server/>
- AWS RDS PostgreSQL extension versions: <https://docs.aws.amazon.com/AmazonRDS/latest/PostgreSQLReleaseNotes/postgresql-extensions.html>
- AWS RDS PostgreSQL pricing: <https://aws.amazon.com/rds/postgresql/pricing/>
- Google Cloud SQL PostgreSQL extensions: <https://cloud.google.com/sql/docs/postgres/extensions>
- Google Cloud SQL pricing: <https://cloud.google.com/sql/pricing>
- Amazon EC2 overview: <https://aws.amazon.com/ec2/>
- Amazon EC2 pricing: <https://aws.amazon.com/ec2/pricing/>
- Amazon EBS pricing: <https://aws.amazon.com/ebs/pricing/>
- Amazon Lightsail overview: <https://aws.amazon.com/lightsail/>
- Amazon Lightsail pricing: <https://aws.amazon.com/lightsail/pricing/>
- AWS App Runner notice: <https://aws.amazon.com/apprunner/>
- Google Cloud Run overview: <https://cloud.google.com/run>
- Google Cloud Run pricing: <https://cloud.google.com/run/pricing>
- Azure Container Apps overview: <https://azure.microsoft.com/en-us/products/container-apps>
- Azure Container Apps pricing: <https://azure.microsoft.com/en-us/pricing/details/container-apps/>
- DigitalOcean App Platform: <https://www.digitalocean.com/products/app-platform>
- Render pricing: <https://render.com/pricing>
- Heroku platform: <https://www.heroku.com/platform/>
