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
export MAGPIE_ADMIN_PASSWORD="$(openssl rand -hex 16)"
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

The chart runs the portal and agentgateway in one pod. `./magpie.sh up`
installs it through Terraform. By hand:

```bash
./magpie.sh render acme > proxy.yaml
helm upgrade --install magpie deploy/helm/magpie-gateway -n magpie --create-namespace \
  --set secrets.MAGPIE_SESSION_SECRET="$(openssl rand -hex 32)" \
  --set secrets.MAGPIE_WEBHOOK_TOKEN="$(openssl rand -hex 32)" \
  --set secrets.MAGPIE_ADMIN_PASSWORD="$(openssl rand -hex 16)" \
  --set-file proxy.config=proxy.yaml
```

The chart refuses an empty `MAGPIE_SESSION_SECRET` and the old defaults
`change-me`, `dev-only-change-me` and `magpie-example-key`, and it fails
without an admin password or an OIDC client ID. Set `ingress.enabled`,
`ingress.host`, `ingress.className`, and `ingress.certificateArn` to use an
existing shared ALB ingress. DNS and ACM certificates are managed outside
the chart. Alternatively, set `targetGroupBinding.*` to register the pod
with dedicated target groups, as the default Terraform deployment does.

## Cloud examples

`terraform/aws` is the stack `./magpie.sh up` deploys: an EKS Auto Mode
cluster (or an existing cluster), the Helm chart, an internal load
balancer and a CloudFront address in `ap-southeast-2`. Set `shared_ingress`
and disable `dedicated_edge_enabled` to reuse a cluster's shared ALB with
your own hostname and no dedicated CloudFront or ALB. See
`docs/engagement.md`. `terraform/azure` is still a portal-only
Container App example in `australiaeast`.
