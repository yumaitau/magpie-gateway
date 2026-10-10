# Deployment examples

These files deploy the Magpie portal. They do not build or brand the
agentgateway proxy. Run the proxy from its own image and point its LLM
webhook and MCP allow rule at the portal. See
`examples/agentgateway.yaml`.

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

Sign the image with cosign when you publish it. This repository does
not push to a public registry.

```bash
cosign sign --yes magpie-gateway-portal:0.1.0
```

Write a CycloneDX SBOM with syft:

```bash
syft magpie-gateway-portal:0.1.0 -o cyclonedx-json > magpie-portal.cdx.json
```

## Compose

```bash
docker compose -f deploy/compose/docker-compose.yml up --build
```

Presidio is a profile, using `ghcr.io/data-privacy-stack`. Those
images already run as uid 1001 and listen on port 3000, so Compose
does not override the user. Pin the image digest before you use it.
After it is up, put `http://127.0.0.1:5002/analyze` in the portal
Settings page. Leave it empty and the portal makes no outbound DLP
call.

## Helm

```bash
helm template magpie deploy/helm/magpie-gateway
```

Replace `sessionSecret` with a real secret before install. The chart
does not create a TLS certificate. Terminate TLS at the ingress.

## Cloud examples

`terraform/aws` is ECS Fargate in `ap-southeast-2`. `terraform/azure`
is a Container App in `australiaeast`. Both expect you to supply the
image and a session secret. They are examples, not a landing zone.
