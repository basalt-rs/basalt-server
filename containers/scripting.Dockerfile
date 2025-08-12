FROM rust:1.84 as basalt-compilation

RUN touch /redocly && chmod +x /redocly
ENV PATH=/:$PATH
WORKDIR /basalt-server
COPY . .


RUN cargo build --release --no-default-features --features scripting

FROM fedora:rawhide as base-basalt

COPY --from=basalt-compilation /basalt-server/target/release/basalt-server /usr/local/bin/
