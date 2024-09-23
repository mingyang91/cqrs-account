FROM rust:bookworm AS builder
WORKDIR /usr/src/cqrs-demo
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
		--mount=type=cache,target=/target \
		cargo build --release

FROM debian:bookworm-slim AS runtime
COPY --from=builder /usr/src/cqrs-demo/target/release/cqrs-demo /usr/local/bin/cqrs-demo
ENTRYPOINT [ "/usr/local/bin/cqrs-account" ]
