FROM rust:1.96.1-bookworm@sha256:a339861ae23e9abb272cea45dfafde21760d2ce6577a70f8a926153677902663 AS build

RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake libclang-dev libprotobuf-dev libssl-dev libudev-dev pkg-config protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY primitives/ primitives/
COPY program/ program/
COPY script/ script/
COPY elf/ elf/

# Embed the checked-in ELFs used by the deployed verifier.
RUN SP1_SKIP_PROGRAM_BUILD=true cargo build --locked --release -p sp1-helios-script --bin operator

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171 AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libudev1 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 helios \
    && useradd --uid 10001 --gid 10001 --create-home --shell /usr/sbin/nologin helios

COPY --from=build /app/target/release/operator /usr/local/bin/operator
USER 10001:10001
ENV HOME=/home/helios
WORKDIR /home/helios

# The operator handles SIGINT through tokio::signal::ctrl_c.
STOPSIGNAL SIGINT
RUN ["operator", "--help"]
CMD ["operator"]
