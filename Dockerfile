FROM rust:1.53-alpine as builder
WORKDIR /usr/src/socks5-forwarder
RUN apk add --no-cache musl-dev libressl-dev
COPY . .
RUN RUSTFLAGS="" cargo build --release

FROM alpine:latest
MAINTAINER ihciah <ihciah@gmail.com>

ENV LISTEN=""
ENV TARGET=""
ENV PROXY=""
ENV USERNAME=""
ENV PASSWORD=""

COPY ./entrypoint.sh /
RUN chmod +x /entrypoint.sh && apk add --no-cache ca-certificates
COPY --from=builder /usr/src/socks5-forwarder/target/release/socks5-forwarder /usr/local/bin/socks5-forwarder
ENTRYPOINT ["/entrypoint.sh"]