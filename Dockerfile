FROM rust:1.96-bookworm AS build

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=build /src/target/release/scargo /usr/local/bin/scargo
COPY dashboard/static ./dashboard/static

ENV SCARGO_HTTP_HOST=0.0.0.0
ENV SCARGO_HTTP_PORT=8080
EXPOSE 8080

CMD ["scargo"]
