FROM rust:1.86 as basalt-compilation

RUN touch /redocly && chmod +x /redocly
ENV PATH=/:$PATH
WORKDIR /basalt-server
COPY . .


RUN cargo build --release

FROM fedora:rawhide as base-basalt

COPY --from=basalt-compilation /basalt-server/target/release/basalt-server /usr/local/bin/
