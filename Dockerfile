FROM --platform=$BUILDPLATFORM rust:1.88-slim-bullseye AS builder
ARG TARGETARCH
RUN apt-get update && apt-get install -y \
    upx build-essential musl-tools gcc-aarch64-linux-gnu &&\
    rm -rf /var/lib/apt/lists/*

ENV RUST_TARGET_amd64=x86_64-unknown-linux-musl
ENV RUST_TARGET_arm64=aarch64-unknown-linux-musl
RUN rustup target add $(eval echo \$RUST_TARGET_${TARGETARCH})

WORKDIR /app
COPY . .

ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=aarch64-linux-gnu-gcc

RUN TARGET=$(eval echo \$RUST_TARGET_${TARGETARCH}) && \
    cargo build --release --locked --target $TARGET && \
    cp target/$TARGET/release/glance-transmission glance-transmission
RUN upx --best --lzma glance-transmission || true

FROM scratch
WORKDIR /app
# no templates to copy for this simple extension
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

EXPOSE 8080
COPY --from=builder /app/glance-transmission ./glance-transmission

ENV RUST_LOG=info
CMD ["./glance-transmission"]
