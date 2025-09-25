FROM rust:1.86 AS basalt-compilation

ENV PATH=/:$PATH
WORKDIR /basalt-server
COPY . .


RUN cargo build --release --no-default-features

FROM scratch AS base-basalt

COPY --from=basalt-compilation /basalt-server/target/release/basalt-server /usr/local/bin/
