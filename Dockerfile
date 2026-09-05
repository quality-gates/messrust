# Runtime image: docker build -t messrust . && docker run --rm -v "$PWD":/code messrust /code text codesize,design
FROM rust:1.85-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/messrust /usr/local/bin/messrust
WORKDIR /code
ENTRYPOINT ["messrust"]
CMD ["--help"]
