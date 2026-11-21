#!/bin/sh
set -eu

# Materialise onboarded agent settings on startup (mirroring Intune / MDM deployment)
mkdir -p "$HOME/.claude" "$HOME/.codex" "$HOME/.config/magpie"

cat > "$HOME/.claude/managed-settings.json" <<EOF
{
  "env": {
    "OPENAI_BASE_URL": "${OPENAI_BASE_URL:-http://proxy:4000/v1}",
    "ANTHROPIC_BASE_URL": "${OPENAI_BASE_URL:-http://proxy:4000/v1}",
    "MAGPIE_GATEWAY_PROXY": "${OPENAI_BASE_URL:-http://proxy:4000/v1}",
    "MAGPIE_MCP_URL": "${MAGPIE_MCP_URL:-http://proxy:3000/mcp}"
  },
  "magpieProfile": {
    "agentName": "${AGENT_NAME:-agent}",
    "cohort": "${AGENT_COHORT:-unknown}",
    "actor": "${AGENT_ACTOR:-unknown}",
    "onboardedBy": "magpie-gateway-e2e"
  }
}
EOF

cat > "$HOME/.codex/config.toml" <<EOF
model_provider = "magpie"
model = "${MAGPIE_DEFAULT_MODEL:-au-sovereign-llm}"

[model_providers.magpie]
name = "Magpie Sovereign Gateway"
base_url = "${OPENAI_BASE_URL:-http://proxy:4000/v1}"
env_key = "OPENAI_API_KEY"
wire_api = "chat"
EOF

exec sleep infinity
