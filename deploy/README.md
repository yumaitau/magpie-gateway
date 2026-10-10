# Deployment examples

Compose starts the portal, a notes MCP server, and agentgateway.
The proxy config is `compose/agentgateway.yaml`: the LLM webhook points
at the portal, and the MCP allow list is `example_search`. A localhost
shape of that webhook is `examples/agentgateway.yaml`. The proxy image
is `ghcr.io/agentgateway/agentgateway:v1.6.0`. This tree is pinned a
few commits later, at `docs/UPSTREAM`.

Nothing in the default Compose file or Helm values enables telemetry,
Presidio, break-glass login or a phone-home endpoint.

## Image

```bash
docker build -f deploy/portal.Dockerfile -t magpie-gateway-portal:0.1.0 .
```

The container runs as uid 65532 with a read-only root filesystem. SQLite
lives on `/data`. A `postgres://` URL is refused by the portal. The
Postgres DDL is `POSTGRES_SCHEMA` in the library, for an operator who
applies it with their own client.

Pushes to `main` publish `ghcr.io/yumaitau/magpie-gateway-portal` tagged
with the commit SHA and `main`, then sign that digest with keyless
cosign. No signing key is stored in the repository. GitHub leaves a
new container package private. An owner of the package marks it
public before `cosign verify` works without a login.

```bash
cosign verify ghcr.io/yumaitau/magpie-gateway-portal:main \
  --certificate-identity-regexp '^https://github.com/yumaitau/magpie-gateway/' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

Write a CycloneDX SBOM with syft:

```bash
syft magpie-gateway-portal:0.1.0 -o cyclonedx-json > magpie-portal.cdx.json
```

## Compose

```bash
export MAGPIE_SESSION_SECRET="$(openssl rand -hex 32)"
docker compose -f deploy/compose/docker-compose.yml up --build
```

`deploy/compose/smoke.sh` calls `example_admin` through the proxy and
checks the notes server never saw it, then calls `example_search` and
checks that the notes server did. The proxy listens on
`127.0.0.1:3000`. Set `MAGPIE_PROXY_PORT` if that port is taken, and
`MAGPIE_PROXY_URL` to the same port when you run the smoke test.

Presidio is a profile, using `ghcr.io/data-privacy-stack`. Those
images already run as uid 1001 and listen on port 3000, so Compose
does not override the user. Pin the image digest before you use it.
After it is up, put `http://127.0.0.1:5002/analyze` in the portal
Settings page. Leave it empty and the portal makes no outbound DLP
call.

## Helm

```bash
helm template magpie deploy/helm/magpie-gateway --set sessionSecret="$(openssl rand -hex 32)"
```

The chart refuses an empty `sessionSecret` and the old defaults
`change-me`, `dev-only-change-me`, and `magpie-example-key`. It does
not create a TLS certificate. Terminate TLS at the ingress. The chart
still runs the portal only. Use Compose when you want the proxy beside it.

## Cloud examples

`terraform/aws` is ECS Fargate in `ap-southeast-2`. `terraform/azure`
is a Container App in `australiaeast`. Both expect you to supply the
image and a session secret. They are examples, not a landing zone.
