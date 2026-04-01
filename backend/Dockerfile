FROM rust:1.77-slim AS builder
WORKDIR /app

# Install build deps including OpenSSL and poppler-utils for potential test runs
RUN apt-get update && apt-get install -y \
    libssl-dev \
    pkg-config \
    poppler-utils \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
# Cache dependencies layer
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

COPY src ./src
COPY config ./config
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    ca-certificates \
    poppler-utils \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/nimbus .
COPY --from=builder /app/config ./config
EXPOSE 3000
CMD ["./nimbus"]
