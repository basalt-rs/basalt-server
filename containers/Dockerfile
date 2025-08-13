FROM rust:1.86 as basalt-compilation

ENV PATH=/:$PATH
WORKDIR /basalt-server
COPY . .


RUN cargo build --release --no-default-features

FROM fedora:rawhide as base-basalt

COPY --from=basalt-compilation /basalt-server/target/release/basalt-server /usr/local/bin/
