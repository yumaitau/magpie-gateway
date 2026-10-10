#!/usr/bin/env bash
# Magpie Gateway engagement tool.
#
#   ./magpie.sh new <client>              answer a few questions
#   ./magpie.sh up <client>               deploy, or apply changes
#   ./magpie.sh connect <client> <person> setup for Claude Code, Codex and Copilot
#   ./magpie.sh key add|remove <client> <person>
#   ./magpie.sh sso <client>              register the portal in Microsoft Entra ID
#   ./magpie.sh check <client>            smoke test the live gateway
#   ./magpie.sh report <client>           download the audit log and register
#   ./magpie.sh down <client>             destroy everything
#
# Everything for one client lives in engagements/<client>/. That folder holds
# secrets and Terraform state. It is git-ignored. Keep it somewhere the client
# has agreed to, and hand it over at the end.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGAGEMENTS="${MAGPIE_ENGAGEMENTS:-$ROOT/engagements}"

# Bedrock AU inference profiles route only to Sydney and Melbourne.
DEFAULT_BEDROCK_MODELS="claude-opus-5-5=au.anthropic.claude-opus-5-5,claude-sonnet-5-5=au.anthropic.claude-sonnet-5-5,claude-haiku-5-5=au.anthropic.claude-haiku-5-5"

say() { printf '%s\n' "$*"; }
step() { printf '\n\033[1m%s\033[0m\n' "$*"; }
die() {
	printf 'magpie: %s\n' "$*" >&2
	exit 1
}

need() {
	for tool in "$@"; do
		command -v "$tool" >/dev/null 2>&1 || die "$tool is not installed."
	done
}

ask() { # ask VAR "Question" [default]
	local answer
	if [[ -n "${3:-}" ]]; then
		read -r -p "$2 [$3]: " answer
		answer="${answer:-$3}"
	else
		read -r -p "$2: " answer
	fi
	printf -v "$1" '%s' "$answer"
}

ask_secret() { # ask_secret VAR "Question"
	local answer
	read -r -s -p "$2: " answer
	printf '\n'
	printf -v "$1" '%s' "$answer"
}

confirm() { # confirm "Question" [Y|N]
	local answer default="${2:-N}"
	read -r -p "$1 [$([[ $default == Y ]] && echo Y/n || echo y/N)]: " answer
	answer="${answer:-$default}"
	[[ $answer =~ ^[Yy] ]]
}

random() { openssl rand -hex "${1:-32}"; }

sha256() { printf '%s' "$1" | openssl dgst -sha256 | awk '{print $NF}'; }

slug() {
	[[ $1 =~ ^[a-z][a-z0-9-]{1,17}$ ]] || die "Use 2 to 18 lower-case letters, digits or hyphens for the client name, starting with a letter."
}

load() {
	slug "${1:-}"
	CLIENT="$1"
	DIR="$ENGAGEMENTS/$CLIENT"
	[[ -f "$DIR/magpie.conf" ]] || die "No engagement called $CLIENT. Run: ./magpie.sh new $CLIENT"
	# shellcheck disable=SC1091
	source "$DIR/magpie.conf"
	# shellcheck disable=SC1091
	source "$DIR/secrets.env"
}

set_conf() { # set_conf FILE KEY VALUE: replace or append KEY="VALUE"
	local file="$1" key="$2" value="$3" tmp
	tmp="$(mktemp)"
	grep -v "^$key=" "$file" >"$tmp" || true
	printf '%s=%q\n' "$key" "$value" >>"$tmp"
	cat "$tmp" >"$file"
	rm -f "$tmp"
}

people() {
	local file
	for file in "$DIR"/keys/*.key; do
		[[ -e $file ]] || continue
		basename "$file" .key
	done
}

url() {
	terraform -chdir="$DIR/terraform" output -raw url 2>/dev/null || die "Not deployed yet. Run: ./magpie.sh up $CLIENT"
}

# ---------------------------------------------------------------- new

cmd_new() {
	slug "${1:-}"
	CLIENT="$1"
	DIR="$ENGAGEMENTS/$CLIENT"
	[[ ! -e "$DIR" ]] || die "$DIR already exists."
	need openssl terraform jq

	step "Magpie Gateway for $CLIENT"
	say "A few questions. Every answer can be changed later in $DIR/magpie.conf."

	local cloud aws_profile owner bedrock="" azure_resource="" azure_models="" azure_region="" azure_key="" names
	ask cloud "Cloud to deploy into (aws)" aws
	[[ $cloud == aws ]] || die "Only aws is ready. Azure Container Apps is next."
	ask aws_profile "AWS CLI profile for the client's account" "${AWS_PROFILE:-default}"
	ask owner "Business owner email at the client (recorded on model approvals)"

	step "Models"
	say "Claude through Amazon Bedrock AU inference profiles stays in Sydney and Melbourne."
	say "Claude Code, Codex and Copilot can all use it through the gateway."
	if confirm "Use Claude on Bedrock (Australia)?" Y; then
		bedrock="$DEFAULT_BEDROCK_MODELS"
		say "Check that the client's account has Anthropic model access in the Bedrock console (ap-southeast-2)."
	fi
	if confirm "Also route to an existing Azure OpenAI resource?" N; then
		ask azure_resource "Azure OpenAI resource name (the part before .openai.azure.com)"
		ask azure_models "Models as name=deployment, comma separated" "gpt-5.4=gpt-5.4"
		say "Australia East has no GPT-5 or Codex models on a Standard (in-region) deployment."
		say "Data Zone deployments process in Asia Pacific, Global ones anywhere."
		if confirm "Are these Standard deployments that keep data in Australia?" N; then
			azure_region="australiaeast"
		else
			azure_region="asia-pacific"
			say "Sovereign mode is on by default, so the portal will block these until you approve that in Settings."
		fi
		ask_secret azure_key "Azure OpenAI API key"
	fi
	[[ -n $bedrock || -n $azure_resource ]] || die "Pick at least one model provider."

	step "People"
	say "Each person gets their own gateway key, so the audit log shows who sent what."
	ask names "Names, comma separated (letters, digits, dots, hyphens)" "$(whoami)"

	mkdir -p "$DIR/keys" "$DIR/terraform"
	chmod 700 "$DIR" "$DIR/keys"
	umask 077
	cat >"$DIR/magpie.conf" <<EOF
# Magpie Gateway settings for $CLIENT. Run ./magpie.sh up $CLIENT after a change.
CLOUD=$cloud
AWS_PROFILE=$(printf '%q' "$aws_profile")
AWS_REGION=ap-southeast-2
OWNER_EMAIL=$(printf '%q' "$owner")
# name=bedrock-model-id pairs. Empty turns Bedrock off.
BEDROCK_MODELS=$(printf '%q' "$bedrock")
# Azure OpenAI resource and name=deployment pairs. Region is australiaeast only for Standard deployments.
AZURE_OPENAI_RESOURCE=$(printf '%q' "$azure_resource")
AZURE_OPENAI_MODELS=$(printf '%q' "$azure_models")
AZURE_OPENAI_REGION=$(printf '%q' "$azure_region")
# Filled in by ./magpie.sh sso
ENTRA_TENANT_ID=
ENTRA_CLIENT_ID=
PORTAL_IMAGE=ghcr.io/yumaitau/magpie-gateway-portal:main
PROXY_IMAGE=ghcr.io/agentgateway/agentgateway:v1.6.0
EOF
	cat >"$DIR/secrets.env" <<EOF
MAGPIE_SESSION_SECRET=$(random)
MAGPIE_WEBHOOK_TOKEN=$(random)
MAGPIE_ADMIN_PASSWORD=$(random 16)
AZURE_OPENAI_API_KEY=$(printf '%q' "$azure_key")
MAGPIE_OIDC_CLIENT_SECRET=
EOF
	local person
	IFS=',' read -r -a list <<<"$names"
	for person in ${list[@]+"${list[@]}"}; do
		person="$(echo "$person" | tr -d '[:space:]')"
		[[ -n $person ]] && new_key "$person"
	done
	printf '*\n' >"$DIR/.gitignore"

	step "Ready"
	say "Saved to $DIR"
	say "Next: ./magpie.sh up $CLIENT"
}

new_key() {
	[[ $1 =~ ^[A-Za-z0-9.-]{1,40}$ ]] || die "Use letters, digits, dots or hyphens for a name: $1"
	[[ ! -e "$DIR/keys/$1.key" ]] || die "$1 already has a key."
	(umask 077 && printf 'mgp_%s\n' "$(random 24)" >"$DIR/keys/$1.key")
}

# ---------------------------------------------------------------- config

# One guardrail webhook pair per model, so the portal sees the model, the
# destination and its region. $MAGPIE_* names are expanded by the proxy from
# its environment; the token is never written into this file.
guardrails() { # guardrails NAME DESTINATION REGION
	local phase
	printf '    guardrails:\n'
	for phase in request response; do
		cat <<EOF
      $phase:
      - webhook:
          target:
            host: "\$MAGPIE_PORTAL_HOST"
          headers:
            ":path": "'/guardrails/$phase'"
            x-magpie-webhook-token: "'\$MAGPIE_WEBHOOK_TOKEN'"
            x-magpie-actor: apiKey.name
            x-magpie-agent: apiKey.name
            x-magpie-model: "'$1'"
            x-magpie-destination: "'$2'"
            x-magpie-region: "'$3'"
            x-magpie-marking: 'request.headers["x-magpie-marking"]'
          failureMode: failClosed
EOF
	done
}

render_proxy() {
	local person pair name model
	cat <<EOF
# Generated by ./magpie.sh for $CLIENT. Change magpie.conf or keys/, then run ./magpie.sh up.
config:
  adminAddr: localhost:15000
  readinessAddr: 0.0.0.0:15021
  statsAddr: localhost:15020
gateways:
  default:
    port: 4000
llm:
  policies:
    apiKey:
      mode: strict
      keys:
EOF
	while read -r person; do
		[[ -n $person ]] || continue
		printf '      - keyHash: "sha256:%s"\n        metadata:\n          name: "%s"\n' \
			"$(sha256 "$(<"$DIR/keys/$person.key")")" "$person"
	done < <(people)
	printf '  models:\n'
	IFS=',' read -r -a pairs <<<"${BEDROCK_MODELS:-}"
	for pair in ${pairs[@]+"${pairs[@]}"}; do
		[[ -n $pair ]] || continue
		name="${pair%%=*}" model="${pair#*=}"
		cat <<EOF
  - name: $name
    provider: bedrock
    params:
      awsRegion: $AWS_REGION
      model: $model
      bedrockEndpointPreference: runtimeOnly
EOF
		guardrails "$name" "bedrock:$model" "$AWS_REGION"
	done
	IFS=',' read -r -a pairs <<<"${AZURE_OPENAI_MODELS:-}"
	for pair in ${pairs[@]+"${pairs[@]}"}; do
		[[ -n $pair && -n ${AZURE_OPENAI_RESOURCE:-} ]] || continue
		name="${pair%%=*}" model="${pair#*=}"
		cat <<EOF
  - name: $name
    provider: azure
    params:
      azureResourceName: $AZURE_OPENAI_RESOURCE
      azureResourceType: openAI
      azureApiVersion: v1
      model: $model
      apiKey: "\$AZURE_OPENAI_API_KEY"
EOF
		guardrails "$name" "azure-openai:$AZURE_OPENAI_RESOURCE/$model" "$AZURE_OPENAI_REGION"
	done
}

bootstrap_models() {
	local pair out=""
	IFS=',' read -r -a pairs <<<"${BEDROCK_MODELS:-}"
	for pair in ${pairs[@]+"${pairs[@]}"}; do [[ -n $pair ]] && out+="${pair%%=*}=$AWS_REGION,"; done
	IFS=',' read -r -a pairs <<<"${AZURE_OPENAI_MODELS:-}"
	for pair in ${pairs[@]+"${pairs[@]}"}; do [[ -n $pair ]] && out+="${pair%%=*}=$AZURE_OPENAI_REGION,"; done
	printf '%s' "${out%,}"
}

render_terraform() {
	local tf="$DIR/terraform"
	[[ -n "$(people)" ]] || die "No keys. Add one: ./magpie.sh key add $CLIENT <person>"
	render_proxy >"$DIR/agentgateway.yaml"
	cat >"$tf/main.tf" <<EOF
# Generated by ./magpie.sh. State stays in this folder.
module "magpie" {
  source = "$ROOT/deploy/terraform/$CLOUD"

  name               = var.name
  region             = var.region
  portal_image       = var.portal_image
  proxy_image        = var.proxy_image
  proxy_config       = var.proxy_config
  portal_environment = var.portal_environment
  secrets            = var.secrets
}

variable "name" { type = string }
variable "region" { type = string }
variable "portal_image" { type = string }
variable "proxy_image" { type = string }
variable "proxy_config" { type = string }
variable "portal_environment" { type = map(string) }
variable "secrets" {
  type      = map(string)
  sensitive = true
}

output "url" {
  value = module.magpie.url
}
EOF
	(
		umask 077
		jq -n \
			--arg name "$CLIENT" \
			--arg region "$AWS_REGION" \
			--arg portal_image "$PORTAL_IMAGE" \
			--arg proxy_image "$PROXY_IMAGE" \
			--rawfile proxy_config "$DIR/agentgateway.yaml" \
			--arg models "$(bootstrap_models)" \
			--arg owner "$OWNER_EMAIL" \
			--arg tenant "${ENTRA_TENANT_ID:-}" \
			--arg client_id "${ENTRA_CLIENT_ID:-}" \
			--arg session "$MAGPIE_SESSION_SECRET" \
			--arg webhook "$MAGPIE_WEBHOOK_TOKEN" \
			--arg admin "$MAGPIE_ADMIN_PASSWORD" \
			--arg oidc_secret "${MAGPIE_OIDC_CLIENT_SECRET:-}" \
			--arg azure_key "${AZURE_OPENAI_API_KEY:-}" \
			'{
				name: $name, region: $region,
				portal_image: $portal_image, proxy_image: $proxy_image,
				proxy_config: $proxy_config,
				portal_environment: ({
					MAGPIE_BOOTSTRAP_MODELS: $models,
					MAGPIE_BOOTSTRAP_OWNER: $owner
				} + (if $client_id != "" then {
					MAGPIE_ENTRA_TENANT_ID: $tenant,
					MAGPIE_OIDC_CLIENT_ID: $client_id,
					MAGPIE_ROLE_OWNERS: "Magpie.Owner",
					MAGPIE_ROLE_ADMINS: "Magpie.Admin",
					MAGPIE_ROLE_APPROVERS: "Magpie.Approver",
					MAGPIE_ROLE_AUDITORS: "Magpie.Auditor"
				} else {} end)),
				secrets: ({
					MAGPIE_SESSION_SECRET: $session,
					MAGPIE_WEBHOOK_TOKEN: $webhook,
					MAGPIE_ADMIN_PASSWORD: $admin
				}
				+ (if $oidc_secret != "" then {MAGPIE_OIDC_CLIENT_SECRET: $oidc_secret} else {} end)
				+ (if $azure_key != "" then {AZURE_OPENAI_API_KEY: $azure_key} else {} end))
			}' >"$tf/terraform.tfvars.json"
	)
}

# ---------------------------------------------------------------- up / down

aws_env() {
	export AWS_PROFILE AWS_REGION
	aws sts get-caller-identity --query Account --output text >/dev/null 2>&1 ||
		die "AWS profile $AWS_PROFILE cannot sign in. Log in to the client's account first."
}

cmd_up() {
	load "${1:-}"
	need terraform jq openssl aws curl
	aws_env
	say "Deploying to AWS account $(aws sts get-caller-identity --query Account --output text) in $AWS_REGION."
	render_terraform
	terraform -chdir="$DIR/terraform" init -upgrade -input=false >/dev/null
	terraform -chdir="$DIR/terraform" apply -input=false
	local base
	base="$(url)"
	step "Waiting for the gateway to answer (CloudFront can take 10 to 15 minutes the first time)"
	local tries=0
	until curl -fsS -o /dev/null "$base/healthz"; do
		tries=$((tries + 1))
		((tries < 90)) || die "No answer from $base/healthz after 15 minutes. Check the ECS service and the /magpie/$CLIENT log group."
		sleep 10
	done
	step "Magpie Gateway is up"
	say "Portal:   $base"
	say "Sign in:  admin / see MAGPIE_ADMIN_PASSWORD in $DIR/secrets.env"
	say "Next:     ./magpie.sh connect $CLIENT <person>    then    ./magpie.sh check $CLIENT"
}

cmd_down() {
	load "${1:-}"
	need terraform
	aws_env
	say "This permanently deletes the gateway for $CLIENT, including the portal database and audit log."
	say "Run ./magpie.sh report $CLIENT first if you need the audit log."
	local confirm
	ask confirm "Type the client name to continue"
	[[ $confirm == "$CLIENT" ]] || die "Not confirmed."
	terraform -chdir="$DIR/terraform" destroy -input=false
}

# ---------------------------------------------------------------- keys

cmd_key() {
	local action="${1:-}"
	load "${2:-}"
	local person="${3:-}"
	[[ -n $person ]] || die "Usage: ./magpie.sh key add|remove $CLIENT <person>"
	case "$action" in
	add) new_key "$person" ;;
	remove)
		[[ -e "$DIR/keys/$person.key" ]] || die "$person has no key."
		rm -f "$DIR/keys/$person.key"
		;;
	*) die "Usage: ./magpie.sh key add|remove $CLIENT <person>" ;;
	esac
	say "Keys changed. Run ./magpie.sh up $CLIENT to apply (the gateway restarts, about a minute)."
}

# ---------------------------------------------------------------- connect

first_model() { # first_model PATTERN
	local pair
	IFS=',' read -r -a pairs <<<"${BEDROCK_MODELS:-},${AZURE_OPENAI_MODELS:-}"
	for pair in ${pairs[@]+"${pairs[@]}"}; do
		# shellcheck disable=SC2053 # glob match is intended
		[[ ${pair%%=*} == $1 ]] && {
			printf '%s' "${pair%%=*}"
			return
		}
	done
}

cmd_connect() {
	load "${1:-}"
	local person="${2:-}"
	[[ -n $person && -e "$DIR/keys/$person.key" ]] || die "Usage: ./magpie.sh connect $CLIENT <person>. People: $(people | tr '\n' ' ')"
	local base key out opus sonnet haiku codex
	base="$(url)"
	key="$(<"$DIR/keys/$person.key")"
	out="$DIR/handover/$person"
	mkdir -p "$out"
	opus="$(first_model 'claude-opus*')"
	sonnet="$(first_model 'claude-sonnet*')"
	haiku="$(first_model 'claude-haiku*')"
	codex="$(first_model 'gpt-*')"
	codex="${codex:-$sonnet}"

	(
		umask 077
		jq -n --arg base "$base" --arg key "$key" --arg opus "$opus" --arg sonnet "$sonnet" --arg haiku "$haiku" '{
			env: ({
				ANTHROPIC_BASE_URL: $base,
				ANTHROPIC_AUTH_TOKEN: $key,
				DISABLE_TELEMETRY: "1"
			}
			+ (if $opus != "" then {ANTHROPIC_DEFAULT_OPUS_MODEL: $opus} else {} end)
			+ (if $sonnet != "" then {ANTHROPIC_DEFAULT_SONNET_MODEL: $sonnet, ANTHROPIC_MODEL: $sonnet} else {} end)
			+ (if $haiku != "" then {ANTHROPIC_DEFAULT_HAIKU_MODEL: $haiku} else {} end))
		}' >"$out/claude-code-settings.json"

		cat >"$out/codex-config.toml" <<EOF
# Append to ~/.codex/config.toml, then: export MAGPIE_API_KEY=<key from magpie.key>
model = "$codex"
model_provider = "magpie"

[model_providers.magpie]
name = "Magpie Gateway"
base_url = "$base/v1"
env_key = "MAGPIE_API_KEY"
wire_api = "responses"
EOF

		local models="[]" pair name api
		IFS=',' read -r -a pairs <<<"${BEDROCK_MODELS:-},${AZURE_OPENAI_MODELS:-}"
		for pair in ${pairs[@]+"${pairs[@]}"}; do
			[[ -n $pair ]] || continue
			name="${pair%%=*}"
			[[ $name == claude-* ]] && api="$base/v1/messages" || api="$base/v1/chat/completions"
			models="$(jq --arg id "$name" --arg url "$api" '. + [{id: $id, name: ("Magpie " + $id), url: $url, toolCalling: true, vision: true, maxInputTokens: 200000, maxOutputTokens: 32000}]' <<<"$models")"
		done
		# Copilot custom endpoints: one provider per API type.
		jq -n --argjson models "$models" '[
			{name: "Magpie Gateway (Claude)", vendor: "customendpoint", apiType: "messages", apiKey: "${input:magpieKey}",
			 models: [$models[] | select(.url | endswith("/v1/messages"))]},
			{name: "Magpie Gateway (OpenAI)", vendor: "customendpoint", apiType: "chat-completions", apiKey: "${input:magpieKey}",
			 models: [$models[] | select(.url | endswith("/chat/completions"))]}
		] | map(select(.models | length > 0))' >"$out/copilot-chatLanguageModels.json"

		cat >"$out/copilot-cli.env" <<EOF
# Source this before running copilot: . ./copilot-cli.env
export COPILOT_PROVIDER_BASE_URL="$base"
export COPILOT_PROVIDER_TYPE="anthropic"
export COPILOT_PROVIDER_API_KEY="$key"
export COPILOT_MODEL="${sonnet:-$codex}"
EOF
		printf '%s\n' "$key" >"$out/magpie.key"
	)

	cat >"$out/README.txt" <<EOF
Magpie Gateway for $CLIENT, set up for $person.
Your key is in magpie.key. Treat it like a password. It identifies you in the audit log.

Claude Code
  Merge claude-code-settings.json into ~/.claude/settings.json (or copy it there if you have none).
  Then run: claude

Codex CLI
  Append codex-config.toml to ~/.codex/config.toml, then:
    export MAGPIE_API_KEY="\$(cat magpie.key)"
    codex

GitHub Copilot in VS Code
  Command palette: "Chat: Manage Language Models" > Add Models > Custom Endpoint,
  or paste copilot-chatLanguageModels.json into chatLanguageModels.json.
  VS Code asks for the key the first time. Inline completions still go to GitHub.

GitHub Copilot CLI
  . ./copilot-cli.env && copilot

Requests that contain Australian identifiers (TFN, Medicare, ABN and others) or
OFFICIAL: Sensitive and above markings may be blocked. The error says why.
Start a new conversation after removing the information.
EOF
	step "Setup for $person written to $out"
	sed 's/^/  /' "$out/README.txt"
	say ""
	say "Send that folder to $person over a channel the client trusts."
}

# ---------------------------------------------------------------- sso

cmd_sso() {
	load "${1:-}"
	need az jq
	local base tenant app_id sp_id secret me
	base="$(url)"
	tenant="$(az account show --query tenantId -o tsv)" || die "Run az login against the client's tenant first."
	say "Registering 'Magpie Gateway ($CLIENT)' in Entra tenant $tenant."
	say "Redirect URI: $base/api/auth/callback"
	confirm "Continue?" Y || die "Stopped."
	local roles
	roles="$(jq -n '[
		["Magpie.Owner", "Owner", "Full control of the Magpie portal"],
		["Magpie.Admin", "Admin", "Manage Magpie settings and approvals"],
		["Magpie.Approver", "Approver", "Approve models, tools and MCP servers"],
		["Magpie.Auditor", "Auditor", "Read and export the Magpie audit log"]
	] | map({allowedMemberTypes: ["User"], value: .[0], displayName: .[1], description: .[2], isEnabled: true, id: null})')"
	local ids=()
	for _ in 1 2 3 4; do ids+=("$(uuidgen | tr '[:upper:]' '[:lower:]')"); done
	roles="$(jq --arg a "${ids[0]}" --arg b "${ids[1]}" --arg c "${ids[2]}" --arg d "${ids[3]}" '[.[0] + {id: $a}, .[1] + {id: $b}, .[2] + {id: $c}, .[3] + {id: $d}]' <<<"$roles")"
	local file
	file="$(mktemp)"
	printf '%s' "$roles" >"$file"
	app_id="$(az ad app create --display-name "Magpie Gateway ($CLIENT)" \
		--sign-in-audience AzureADMyOrg \
		--web-redirect-uris "$base/api/auth/callback" \
		--app-roles "@$file" --query appId -o tsv)"
	rm -f "$file"
	sp_id="$(az ad sp create --id "$app_id" --query id -o tsv)"
	az ad sp update --id "$sp_id" --set appRoleAssignmentRequired=true >/dev/null
	secret="$(az ad app credential reset --id "$app_id" --display-name magpie --years 1 --query password -o tsv)"
	me="$(az ad signed-in-user show --query id -o tsv)"
	az rest --method POST \
		--uri "https://graph.microsoft.com/v1.0/servicePrincipals/$sp_id/appRoleAssignedTo" \
		--body "{\"principalId\":\"$me\",\"resourceId\":\"$sp_id\",\"appRoleId\":\"${ids[0]}\"}" >/dev/null
	set_conf "$DIR/magpie.conf" ENTRA_TENANT_ID "$tenant"
	set_conf "$DIR/magpie.conf" ENTRA_CLIENT_ID "$app_id"
	set_conf "$DIR/secrets.env" MAGPIE_OIDC_CLIENT_SECRET "$secret"
	say "Registered. You are assigned Owner. Assign other people in Entra:"
	say "  Enterprise applications > Magpie Gateway ($CLIENT) > Users and groups."
	say "Only assigned users can sign in. The client secret expires in one year."
	say "Applying..."
	cmd_up "$CLIENT"
}

# ---------------------------------------------------------------- check

cmd_check() {
	load "${1:-}"
	need curl jq
	local base key person model failed=0 code body
	base="$(url)"
	person="$(people | head -1)"
	key="$(<"$DIR/keys/$person.key")"
	model="$(first_model 'claude-haiku*')"
	model="${model:-$(first_model '*')}"

	result() { # result NAME OK [DETAIL]
		if [[ $2 == ok ]]; then printf '  ok    %s\n' "$1"; else
			printf '  FAIL  %s  %s\n' "$1" "${3:-}"
			failed=1
		fi
	}
	step "Checking $base as $person with $model"

	code="$(curl -s -o /dev/null -w '%{http_code}' "$base/healthz")"
	[[ $code == 200 ]] && result "portal answers" ok || result "portal answers" fail "HTTP $code"

	code="$(curl -s -o /dev/null -w '%{http_code}' "$base/v1/models")"
	[[ $code == 401 || $code == 403 ]] && result "gateway refuses a request without a key" ok || result "gateway refuses a request without a key" fail "HTTP $code"

	body="$(curl -s -H "Authorization: Bearer $key" "$base/v1/models")"
	jq -e '.data | length > 0' >/dev/null 2>&1 <<<"$body" && result "models listed: $(jq -r '[.data[].id] | join(", ")' <<<"$body")" ok || result "models listed" fail "$body"

	body="$(curl -s -w '\n%{http_code}' "$base/v1/messages" \
		-H "Authorization: Bearer $key" -H 'content-type: application/json' -H 'anthropic-version: 2023-06-01' \
		-d "{\"model\":\"$model\",\"max_tokens\":20,\"messages\":[{\"role\":\"user\",\"content\":\"Reply with the word ready.\"}]}")"
	[[ ${body##*$'\n'} == 200 ]] && result "Anthropic Messages API (Claude Code)" ok || result "Anthropic Messages API (Claude Code)" fail "${body:0:300}"

	body="$(curl -s -w '\n%{http_code}' "$base/v1/chat/completions" \
		-H "Authorization: Bearer $key" -H 'content-type: application/json' \
		-d "{\"model\":\"$model\",\"max_tokens\":20,\"messages\":[{\"role\":\"user\",\"content\":\"Reply with the word ready.\"}]}")"
	[[ ${body##*$'\n'} == 200 ]] && result "OpenAI Chat Completions API (Copilot)" ok || result "OpenAI Chat Completions API (Copilot)" fail "${body:0:300}"

	body="$(curl -s -w '\n%{http_code}' "$base/v1/responses" \
		-H "Authorization: Bearer $key" -H 'content-type: application/json' \
		-d "{\"model\":\"$model\",\"max_output_tokens\":20,\"input\":\"Reply with the word ready.\"}")"
	[[ ${body##*$'\n'} == 200 ]] && result "OpenAI Responses API (Codex)" ok || result "OpenAI Responses API (Codex)" fail "${body:0:300}"

	# A synthetic ABN with a valid checksum. It must not reach the model.
	body="$(curl -s -w '\n%{http_code}' "$base/v1/messages" \
		-H "Authorization: Bearer $key" -H 'content-type: application/json' -H 'anthropic-version: 2023-06-01' \
		-d "{\"model\":\"$model\",\"max_tokens\":20,\"messages\":[{\"role\":\"user\",\"content\":\"Example only. Our ABN is 12 000 000 041.\"}]}")"
	[[ ${body##*$'\n'} == 403 ]] && result "DLP blocks an Australian identifier" ok || result "DLP blocks an Australian identifier" fail "${body:0:300}"

	code="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$base/guardrails/request")"
	[[ $code == 404 ]] && result "portal guardrail routes are not public" ok || result "portal guardrail routes are not public" fail "HTTP $code"

	((failed == 0)) || die "Some checks failed."
	say ""
	say "All checks passed. Each request is in the portal's audit log under $person."
}

# ---------------------------------------------------------------- report

cmd_report() {
	load "${1:-}"
	need curl
	local base out jar code
	base="$(url)"
	out="$DIR/handover/report-$(date +%Y-%m-%d)"
	mkdir -p "$out"
	jar="$(mktemp)"
	trap 'rm -f "$jar"' RETURN
	code="$(curl -s -o /dev/null -w '%{http_code}' -c "$jar" \
		--data-urlencode "username=admin" --data-urlencode "password=$MAGPIE_ADMIN_PASSWORD" \
		"$base/api/auth/break-glass")"
	[[ $code == 303 || $code == 307 || $code == 302 ]] || die "Admin sign-in failed (HTTP $code)."
	curl -fsS -b "$jar" "$base/audit.csv" -o "$out/audit-log.csv"
	curl -fsS -b "$jar" "$base/use-cases.csv" -o "$out/ai-use-case-register.csv"
	curl -fsS -b "$jar" "$base/api/export" -o "$out/policy-export.json"
	cp "$DIR/agentgateway.yaml" "$out/gateway-config.yaml"
	step "Report written to $out"
	ls -1 "$out" | sed 's/^/  /'
}

# ---------------------------------------------------------------- main

usage() {
	sed -n '2,13p' "$0" | sed 's/^# \{0,1\}//'
	exit "${1:-0}"
}

main() {
	local command="${1:-}"
	shift || true
	case "$command" in
	new) cmd_new "$@" ;;
	up) cmd_up "$@" ;;
	down) cmd_down "$@" ;;
	key) cmd_key "$@" ;;
	connect) cmd_connect "$@" ;;
	sso) cmd_sso "$@" ;;
	check) cmd_check "$@" ;;
	report) cmd_report "$@" ;;
	render) # for tests: print the proxy config
		load "${1:-}"
		render_proxy
		;;
	help | -h | --help | "") usage ;;
	*) usage 1 ;;
	esac
}

main "$@"
