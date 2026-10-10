# Portal image. Build from the repository root:
#   docker build -f deploy/portal.Dockerfile -t magpie-gateway-portal:0.1.0 .
#
# The image does not contain agentgateway. It does not phone home.

FROM rust:1.99.0-bookworm AS build
WORKDIR /src
COPY crates/cel-fork crates/cel-fork
COPY magpie magpie
RUN cargo build --manifest-path magpie/Cargo.toml -p magpie-portal --release --locked

FROM debian:bookworm-slim
RUN apt-get update \
	&& apt-get upgrade -y --no-install-recommends \
	&& apt-get install -y --no-install-recommends ca-certificates \
	&& rm -rf /var/lib/apt/lists/* \
	&& useradd --uid 65532 --user-group --home-dir /data --shell /usr/sbin/nologin magpie \
	&& mkdir -p /data \
	&& chown 65532:65532 /data
COPY --from=build /src/magpie/target/release/magpie-portal /usr/local/bin/magpie-portal
USER 65532:65532
ENV MAGPIE_DATABASE_URL=/data/magpie.sqlite \
	MAGPIE_LISTEN=0.0.0.0:8088 \
	MAGPIE_COOKIE_SECURE=1 \
	HOME=/tmp
EXPOSE 8088
VOLUME /data
ENTRYPOINT ["/usr/local/bin/magpie-portal"]
