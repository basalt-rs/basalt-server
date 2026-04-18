FROM rust:1.93 AS basalt-compilation

ENV PATH=/:$PATH
WORKDIR /basalt-server
COPY . .


RUN cargo build --release --no-default-features -p basalt-server

FROM scratch AS base-basalt

COPY --from=basalt-compilation /basalt-server/target/release/basalt-server /usr/local/bin/
