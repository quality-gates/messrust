# Development image. Build and run it only as AGENTS.md "Development container" describes.
FROM rust:1.85.1-slim
RUN apt-get update && apt-get install -y --no-install-recommends git bash python3 && rm -rf /var/lib/apt/lists/* \
    && git config --system safe.directory '*'
RUN rustup component add clippy rustfmt
# Build output goes to the size-capped target/ tmpfs that AGENTS.md mounts, not to the host.
ENV CARGO_BUILD_JOBS=2
WORKDIR /workspace
COPY . .
RUN cargo fetch --locked
LABEL dev-image=messrust
CMD ["cargo", "test", "--all-targets", "--locked"]
