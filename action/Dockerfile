# Stage 1: Plan dependencies
FROM rust:1.95-bookworm AS planner
WORKDIR /app
RUN cargo install cargo-chef --locked
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 2: Cache dependencies
FROM rust:1.95-bookworm AS cacher
WORKDIR /app
RUN cargo install cargo-chef --locked
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p boardflow-action-runner

# Stage 3: Build binary
FROM rust:1.95-bookworm AS builder
WORKDIR /app
COPY . .
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
RUN cargo build --release -p boardflow-action-runner

# Stage 4: Runtime
FROM kicad/kicad:9.0

USER root

RUN apt-get update && apt-get install -y --no-install-recommends \
    python3-pip \
    xvfb \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN pip3 install --break-system-packages interactivehtmlbom

COPY --from=builder /app/target/release/boardflow-action-runner /usr/local/bin/boardflow-action-runner

ENTRYPOINT ["/usr/local/bin/boardflow-action-runner"]
