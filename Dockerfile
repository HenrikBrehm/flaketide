FROM rust:1.85-slim-bookworm AS chef
WORKDIR /app
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config libssl-dev \
 && apt-get clean \
 && cargo install cargo-chef --version 0.1.71 --locked

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY build.rs ./
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY build.rs ./
RUN cargo build --release --bin flaketide

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && apt-get clean
COPY --from=builder /app/target/release/flaketide /usr/local/bin/flaketide
WORKDIR /repo
ENTRYPOINT ["/usr/local/bin/flaketide"]
