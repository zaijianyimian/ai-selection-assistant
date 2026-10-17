FROM rust:1.93-slim-trixie AS build
RUN apt-get update && apt-get install -y --no-install-recommends build-essential ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY crates/sync-protocol crates/sync-protocol
COPY server server
RUN cargo build --locked --release --manifest-path server/Cargo.toml

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 asa && useradd --uid 10001 --gid 10001 --no-create-home asa \
    && mkdir /data /backups && chown 10001:10001 /data /backups && chmod 700 /data /backups
COPY --from=build /build/server/target/release/asa-sync-server /usr/local/bin/asa-sync-server
USER 10001:10001
ENV SYNC_DATABASE=/data/sync.sqlite3 SYNC_BIND=0.0.0.0:8080
VOLUME ["/data", "/backups"]
EXPOSE 8080
ENTRYPOINT ["asa-sync-server"]
CMD ["serve"]
