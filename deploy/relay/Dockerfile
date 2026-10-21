# KhataOS relay in a container (optional; see docs/deploy-relay.md, part C).
# Build from the repository root:
#   docker build -f deploy/relay/Dockerfile -t khata-relay .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p khata-server

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --home-dir /data khata && mkdir /data && chown khata /data
COPY --from=build /src/target/release/khata-server /usr/local/bin/khata-server
USER khata
ENV KHATA_DATA_DIR=/data \
    KHATA_BIND=0.0.0.0:8787
VOLUME /data
EXPOSE 8787
CMD ["khata-server"]
