#!/usr/bin/env bash
# Magpie Gateway engagement tool.
#
#   ./magpie.sh new <client>              answer a few questions
#   ./magpie.sh up <client>               deploy, or apply changes
#   ./magpie.sh connect <client> <person> setup for Claude Code, Codex and Copilot
#   ./magpie.sh key add|remove <client> <person>
#   ./magpie.sh sso <client>              register the portal in Microsoft Entra ID
#   ./magpie.sh check <client>            smoke test the live gateway
#   ./magpie.sh uitest <client>           browser test of the live portal
#   ./magpie.sh report <client>           download the audit log and register
#   ./magpie.sh kubectl <client> <args>   kubectl in the magpie namespace
#   ./magpie.sh use <client> [person]     point this machine's coding agents at the gateway
#   ./magpie.sh unuse                     undo use
#   ./magpie.sh down <client>             destroy everything
#
# Everything for one client lives in engagements/<client>/. That folder holds
# secrets and Terraform state. It is git-ignored. Keep it somewhere the client
# has agreed to, and hand it over at the end.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGAGEMENTS="${MAGPIE_ENGAGEMENTS:-$ROOT/engagements}"

# Bedrock AU inference profiles route only to Sydney and Melbourne.
# Newest first. `new` keeps the first one per family the account can call.
BEDROCK_CANDIDATES="
opus au.anthropic.claude-opus-5-5 au.anthropic.claude-opus-5 au.anthropic.claude-opus-4-8 au.anthropic.claude-opus-4-7 au.anthropic.claude-opus-4-6-v1
sonnet au.anthropic.claude-sonnet-5-5 au.anthropic.claude-sonnet-5 au.anthropic.claude-sonnet-4-6 au.anthropic.claude-sonnet-4-5-20250929-v1:0
haiku au.anthropic.claude-haiku-5-5 au.anthropic.claude-haiku-4-5-20251001-v1:0
"

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
	GUARDRAIL_MODE="${GUARDRAIL_MODE:-enforce}"
	case "$GUARDRAIL_MODE" in
		enforce|log-only) ;;
		*) die "GUARDRAIL_MODE must be enforce or log-only." ;;
	esac
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

first_person() {
	local person
	for person in $(people); do
		printf '%s' "$person"
		return
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

	local cloud aws_profile cluster="" auto_mode=false lb_subnets="" storage_class="" owner bedrock="" azure_resource="" azure_models="" azure_region="" azure_key="" names
	ask cloud "Cloud to deploy into (aws)" aws
	[[ $cloud == aws ]] || die "Only aws is ready. Azure Container Apps is next."
	ask aws_profile "AWS CLI profile for the client's account" "${AWS_PROFILE:-default}"

	step "Kubernetes"
	say "Magpie runs on EKS. Leave this blank to create a VPC and an EKS Auto Mode cluster"
	say "(about US\$110 a month before nodes, for the control plane and one NAT gateway)."
	ask cluster "Existing EKS cluster to install into, in ap-southeast-2" ""
	if [[ -n $cluster ]]; then
		local found
		found="$(AWS_PROFILE="$aws_profile" inspect_cluster "$cluster")" || exit 1
		read -r auto_mode lb_subnets storage_class <<<"$found"
	fi
	ask owner "Business owner email at the client (recorded on model approvals)"

	step "Models"
	say "Claude through Amazon Bedrock AU inference profiles stays in Sydney and Melbourne."
	say "Claude Code, Codex and Copilot can all use it through the gateway."
	if confirm "Use Claude on Bedrock (Australia)?" Y; then
		bedrock="$(AWS_PROFILE="$aws_profile" detect_bedrock)"
		[[ -n $bedrock ]] || die "The account cannot call any Claude AU inference profile. Turn on Anthropic model access in the Bedrock console (ap-southeast-2), then run new again."
		say "Using: $bedrock"
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
# Empty creates an EKS Auto Mode cluster. Otherwise Magpie installs into this one.
EKS_CLUSTER=$(printf '%q' "$cluster")
EKS_AUTO_MODE=$auto_mode
# Private subnets for the internal load balancer, comma separated. Empty uses the created ones.
EKS_LB_SUBNETS=$(printf '%q' "$lb_subnets")
# Storage class for the portal volume on a cluster without Auto Mode. Empty uses the default.
EKS_STORAGE_CLASS=$(printf '%q' "$storage_class")
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
# Initial mode only. After first start, change it in portal Settings.
GUARDRAIL_MODE=log-only
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

# Subnets in a VPC whose default route does not go to an internet gateway.
# CloudFront VPC origins need the load balancer in private subnets.
private_subnets() { # private_subnets VPC_ID
	local tables
	tables="$(aws ec2 describe-route-tables --region ap-southeast-2 --filters "Name=vpc-id,Values=$1" --output json)"
	aws ec2 describe-subnets --region ap-southeast-2 --filters "Name=vpc-id,Values=$1" --output json |
		jq -r --argjson tables "$tables" '
			($tables.RouteTables | map(select(any(.Associations[]?; .Main == true))) | first) as $main
			| [.Subnets[]
				| .SubnetId as $id
				| (($tables.RouteTables | map(select(any(.Associations[]?; .SubnetId == $id))) | first) // $main) as $table
				| select(any($table.Routes[]?; .DestinationCidrBlock == "0.0.0.0/0" and ((.GatewayId // "") | startswith("igw-"))) | not)
				| $id] | join(",")'
}

# Prints "<auto mode true|false> <load balancer subnets> <storage class>" and
# stops when the cluster lacks what the chart needs.
inspect_cluster() {
	local name="$1" auto vpc subnets class=""
	need aws jq
	auto="$(aws eks describe-cluster --region ap-southeast-2 --name "$name" \
		--query 'cluster.computeConfig.enabled' --output text 2>/dev/null)" ||
		die "Cannot read EKS cluster $name in ap-southeast-2 with this profile."
	vpc="$(aws eks describe-cluster --region ap-southeast-2 --name "$name" --query 'cluster.resourcesVpcConfig.vpcId' --output text)"
	subnets="$(private_subnets "$vpc")"
	[[ $(tr ',' '\n' <<<"$subnets" | grep -c .) -ge 2 ]] ||
		die "VPC $vpc needs at least two private subnets for the internal load balancer."
	say "Load balancer goes in private subnets $subnets." >&2
	if [[ $auto == True ]]; then
		say "$name runs EKS Auto Mode. Nothing else is needed." >&2
		printf 'true %s \n' "$subnets"
		return
	fi
	aws eks describe-addon --region ap-southeast-2 --cluster-name "$name" --addon-name eks-pod-identity-agent >/dev/null 2>&1 ||
		die "$name has no eks-pod-identity-agent add-on. Add it (aws eks create-addon --addon-name eks-pod-identity-agent), then run new again."
	say "$name is not in Auto Mode. It needs the AWS Load Balancer Controller (for TargetGroupBinding)" >&2
	say "and a storage class backed by the EBS CSI driver." >&2
	read -r -p "Storage class for the portal volume (blank uses the cluster default): " class
	printf 'false %s %s\n' "$subnets" "$class"
}

# Gateway name for a Bedrock ID: au.anthropic.claude-sonnet-4-5-20250929-v1:0 -> claude-sonnet-4-5
model_alias() {
	local id="${1#au.anthropic.}"
	id="${id%-v1:0}"
	id="${id%-v1}"
	printf '%s' "$id" | sed -E 's/-[0-9]{8}$//'
}

# One tiny Converse call per candidate, newest first, until one per family answers.
detect_bedrock() {
	local family id out=""
	need aws
	say "Checking which Claude models this account can call in ap-southeast-2..." >&2
	while read -r family ids; do
		[[ -n $family ]] || continue
		for id in $ids; do
			if aws bedrock-runtime converse --region ap-southeast-2 --model-id "$id" \
				--messages '[{"role":"user","content":[{"text":"hi"}]}]' \
				--inference-config maxTokens=1 >/dev/null 2>&1; then
				out+="$(model_alias "$id")=$id,"
				break
			fi
		done
	done <<<"$BEDROCK_CANDIDATES"
	printf '%s' "${out%,}"
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
  logging:
    database:
      url: /data/gateway/requests.sqlite
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

# Destination strings the guardrail compares for PROTECTED and above.
approved_protected() {
	local pair out=""
	IFS=',' read -r -a pairs <<<"${BEDROCK_MODELS:-}"
	for pair in ${pairs[@]+"${pairs[@]}"}; do
		[[ -n $pair ]] && out+="bedrock:${pair#*=},"
	done
	IFS=',' read -r -a pairs <<<"${AZURE_OPENAI_MODELS:-}"
	for pair in ${pairs[@]+"${pairs[@]}"}; do
		[[ -n $pair && -n ${AZURE_OPENAI_RESOURCE:-} ]] && out+="azure-openai:${AZURE_OPENAI_RESOURCE}/${pair#*=},"
	done
	printf '%s' "${out%,}"
}

# ghcr.io/owner/name:tag -> ghcr.io/owner/name@sha256:..., so each up deploys
# an exact image and a new :main build actually rolls out. Other registries,
# and images already pinned, are left alone.
pin_image() {
	local image="$1" repo tag token digest
	[[ $image == ghcr.io/*:* && $image != *@sha256:* ]] || {
		printf '%s' "$image"
		return
	}
	repo="${image#ghcr.io/}"
	tag="${repo##*:}"
	repo="${repo%:*}"
	token="$(curl -fsS "https://ghcr.io/token?scope=repository:$repo:pull" | jq -r .token)" || die "Cannot read $image from GHCR."
	digest="$(curl -fsSI -H "Authorization: Bearer $token" \
		-H 'Accept: application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json, application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json' \
		"https://ghcr.io/v2/$repo/manifests/$tag" | tr -d '\r' | awk -F': ' 'tolower($1)=="docker-content-digest" {print $2}')"
	[[ $digest == sha256:* ]] || die "Cannot resolve $image to a digest."
	printf 'ghcr.io/%s@%s' "$repo" "$digest"
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

  existing_cluster_name                = var.existing_cluster_name
  existing_cluster_auto_mode           = var.existing_cluster_auto_mode
  existing_cluster_storage_class       = var.existing_cluster_storage_class
  load_balancer_subnet_ids             = var.load_balancer_subnet_ids
  cluster_endpoint_public_access_cidrs = var.cluster_endpoint_public_access_cidrs
  dedicated_edge_enabled               = var.dedicated_edge_enabled
  shared_ingress                       = var.shared_ingress
  chart_path                           = "$ROOT/deploy/helm/magpie-gateway"
}

variable "name" { type = string }
variable "region" { type = string }
variable "portal_image" { type = string }
variable "proxy_image" { type = string }
variable "proxy_config" { type = string }
variable "portal_environment" { type = map(string) }
variable "existing_cluster_name" { type = string }
variable "existing_cluster_auto_mode" { type = bool }
variable "existing_cluster_storage_class" { type = string }
variable "load_balancer_subnet_ids" { type = list(string) }
variable "dedicated_edge_enabled" { type = bool }
variable "shared_ingress" {
  type = object({ enabled = bool, host = string, class_name = string, certificate_arn = string, annotations = map(string) })
}
variable "cluster_endpoint_public_access_cidrs" { type = list(string) }
variable "secrets" {
  type      = map(string)
  sensitive = true
}

output "url" {
  value = module.magpie.url
}

output "ingress_certificate_validation" {
  value = module.magpie.ingress_certificate_validation
}

output "cluster_name" {
  value = module.magpie.cluster_name
}
EOF
	# A created cluster's Kubernetes API only answers this machine.
	local operator
	operator="$(curl -fsS https://checkip.amazonaws.com | tr -d '[:space:]')" || die "Cannot work out this machine's public address."
	(
		umask 077
		jq -n \
			--arg name "$CLIENT" \
			--arg region "$AWS_REGION" \
			--arg cluster "${EKS_CLUSTER:-}" \
			--argjson auto_mode "${EKS_AUTO_MODE:-false}" \
			--arg storage_class "${EKS_STORAGE_CLASS:-}" \
			--arg lb_subnets "${EKS_LB_SUBNETS:-}" \
			--argjson dedicated_edge "${DEDICATED_EDGE_ENABLED:-true}" \
			--arg ingress_host "${INGRESS_HOST:-}" \
			--arg ingress_class "${INGRESS_CLASS:-alb-shared}" \
			--arg ingress_certificate "${INGRESS_CERTIFICATE_ARN:-}" \
			--argjson ingress_annotations "$(
				if [[ -n ${INGRESS_ANNOTATIONS_FILE:-} ]]; then
					cat "$DIR/$INGRESS_ANNOTATIONS_FILE"
				else
					printf '{}'
				fi
			)" \
			--arg operator "$operator/32" \
			--arg portal_image "$(pin_image "$PORTAL_IMAGE")" \
			--arg proxy_image "$(pin_image "$PROXY_IMAGE")" \
			--rawfile proxy_config "$DIR/agentgateway.yaml" \
			--arg models "$(bootstrap_models)" \
			--arg protected "$(approved_protected)" \
			--arg owner "$OWNER_EMAIL" \
			--arg guardrail_mode "$GUARDRAIL_MODE" \
			--arg tenant "${ENTRA_TENANT_ID:-}" \
			--arg client_id "${ENTRA_CLIENT_ID:-}" \
			--arg session "$MAGPIE_SESSION_SECRET" \
			--arg webhook "$MAGPIE_WEBHOOK_TOKEN" \
			--arg admin "$MAGPIE_ADMIN_PASSWORD" \
			--arg oidc_secret "${MAGPIE_OIDC_CLIENT_SECRET:-}" \
			--arg azure_key "${AZURE_OPENAI_API_KEY:-}" \
			'{
				name: $name, region: $region,
				existing_cluster_name: $cluster,
				dedicated_edge_enabled: $dedicated_edge,
				shared_ingress: {enabled: ($ingress_host != ""), host: $ingress_host, class_name: $ingress_class, certificate_arn: $ingress_certificate, annotations: $ingress_annotations},
				existing_cluster_auto_mode: $auto_mode,
				existing_cluster_storage_class: $storage_class,
				load_balancer_subnet_ids: ($lb_subnets | split(",") | map(select(. != ""))),
				cluster_endpoint_public_access_cidrs: [$operator],
				portal_image: $portal_image, proxy_image: $proxy_image,
				proxy_config: $proxy_config,
				portal_environment: ({
					MAGPIE_GUARDRAIL_MODE: $guardrail_mode,
					MAGPIE_BOOTSTRAP_MODELS: $models,
					MAGPIE_BOOTSTRAP_OWNER: $owner,
					MAGPIE_APPROVED_PROTECTED: $protected
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

# Terraform shows the plan and asks. MAGPIE_AUTO_APPROVE=1 skips that for scripts.
tf() { # tf apply|destroy
	if [[ ${MAGPIE_AUTO_APPROVE:-} == 1 ]]; then
		terraform -chdir="$DIR/terraform" "$1" -auto-approve
	else
		terraform -chdir="$DIR/terraform" "$1"
	fi
}

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
	tf apply
	local base
	base="$(url)"
	step "Waiting for the gateway to answer (CloudFront can take 10 to 15 minutes the first time)"
	local tries=0
	until curl -fs -o /dev/null "$base/healthz"; do
		tries=$((tries + 1))
		((tries < 90)) || die "No answer from $base/healthz after 15 minutes. Check the ECS service and the /magpie/$CLIENT log group."
		sleep 10
	done
	step "Magpie Gateway is up"
	say "Portal:   $base"
	say "Sign in:  admin / see MAGPIE_ADMIN_PASSWORD in $DIR/secrets.env"
	say "Logs:     ./magpie.sh kubectl $CLIENT logs deploy/magpie -c proxy"
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
	tf destroy
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
	return 0
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

# Read the saved portal choice; magpie.conf supplies only the initial default.
live_guardrail_mode() (
	local base="$1" jar code mode
	jar="$(mktemp)"
	trap 'rm -f "$jar"' EXIT
	code="$(curl -s -o /dev/null -w '%{http_code}' -c "$jar" \
		--data-urlencode "username=admin" --data-urlencode "password=$MAGPIE_ADMIN_PASSWORD" \
		"$base/api/auth/break-glass")"
	[[ $code == 303 || $code == 307 || $code == 302 ]] || die "Admin sign-in failed (HTTP $code)."
	mode="$(curl -fsS -b "$jar" "$base/settings/guardrail-mode" | jq -er '.mode | select(. == "enforce" or . == "log-only")')" || die "Cannot read the live guardrail mode."
	printf '%s' "$mode"
)

cmd_check() {
	load "${1:-}"
	need curl jq
	local base key person model failed=0 code body mode
	base="$(url)"
	person="$(first_person)"
	key="$(<"$DIR/keys/$person.key")"
	model="$(first_model 'claude-haiku*')"
	model="${model:-$(first_model '*')}"

	result() { # result NAME OK [DETAIL]
		if [[ $2 == ok ]]; then printf '  ok    %s\n' "$1"; else
			printf '  FAIL  %s  %s\n' "$1" "${3:-}"
			failed=1
		fi
	}
	mode="$(live_guardrail_mode "$base")"
	step "Checking $base as $person with $model ($mode)"

	code="$(curl -s -o /dev/null -w '%{http_code}' "$base/healthz")"
	[[ $code == 200 ]] && result "portal answers" ok || result "portal answers" fail "HTTP $code"

	code="$(curl -sL --max-redirs 5 -o /dev/null -w '%{http_code}' "$base/")"
	[[ $code == 200 ]] && result "bare address opens the portal" ok || result "bare address opens the portal" fail "HTTP $code"

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

	# A synthetic ABN with a valid checksum tests the configured rollout mode.
	body="$(curl -s -w '\n%{http_code}' "$base/v1/messages" \
		-H "Authorization: Bearer $key" -H 'content-type: application/json' -H 'anthropic-version: 2023-06-01' \
		-d "{\"model\":\"$model\",\"max_tokens\":20,\"messages\":[{\"role\":\"user\",\"content\":\"Example only. Our ABN is 12 000 000 041.\"}]}")"
	if [[ $mode == log-only ]]; then
		[[ ${body##*$'\n'} == 200 ]] && result "log-only allows an Australian identifier" ok || result "log-only allows an Australian identifier" fail "${body:0:300}"
	else
		[[ ${body##*$'\n'} == 400 && $body == *"Magpie Gateway blocked"* ]] && result "DLP blocks an Australian identifier" ok || result "DLP blocks an Australian identifier" fail "${body:0:300}"
	fi

	local private_path
	for private_path in /guardrails/request /guardrails/response /mcp/check; do
		code="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$base$private_path")"
		[[ $code == 404 ]] && result "$private_path is not public" ok || result "$private_path is not public" fail "HTTP $code"
	done

	((failed == 0)) || die "Some checks failed."
	say ""
	say "All checks passed. Each request is in the portal's audit log under $person."
}

# ---------------------------------------------------------------- uitest

cmd_uitest() {
	load "${1:-}"
	need npm npx
	local person model
	person="$(first_person)"
	model="$(first_model 'claude-haiku*')"
	model="${model:-$(first_model '*')}"
	(
		cd "$ROOT/magpie/axe"
		npm ci --silent
		npx playwright install chromium >/dev/null
		MAGPIE_LIVE_URL="$(url)" MAGPIE_ADMIN_PASSWORD="$MAGPIE_ADMIN_PASSWORD" \
			MAGPIE_KEY="$(<"$DIR/keys/$person.key")" MAGPIE_PERSON="$person" MAGPIE_MODEL="$model" \
			npx playwright test live/ --reporter=list
	)
}

# ---------------------------------------------------------------- kubectl

# Runs kubectl against the deployment's cluster with a kubeconfig kept in
# the engagement folder, so the operator's own config is not touched.
cmd_kubectl() {
	load "${1:-}"
	shift
	need aws kubectl
	aws_env
	local cluster
	cluster="$(terraform -chdir="$DIR/terraform" output -raw cluster_name 2>/dev/null)" || die "Not deployed yet."
	aws eks update-kubeconfig --region "$AWS_REGION" --name "$cluster" --kubeconfig "$DIR/kubeconfig" >/dev/null
	kubectl --kubeconfig "$DIR/kubeconfig" --namespace magpie "$@"
}

# ---------------------------------------------------------------- use / unuse

# Point the coding agents on this machine at a deployed gateway: Claude Code,
# Codex, opencode and Copilot in VS Code. The key is stored once in
# ~/.config/magpie/<client>.key and each tool reads it from there. Every file
# is backed up once to <file>.before-magpie, and unuse removes only what use
# added.
MAGPIE_HOME="${MAGPIE_HOME:-$HOME/.config/magpie}"
CLAUDE_SETTINGS="$HOME/.claude/settings.json"
CODEX_CONFIG="${CODEX_HOME:-$HOME/.codex}/config.toml"
OPENCODE_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/opencode"
VSCODE_MODELS="$HOME/Library/Application Support/Code/User/chatLanguageModels.json"

# opencode reads opencode.jsonc after opencode.json, so a model set in the
# .jsonc wins. Edit that one when it exists and holds plain JSON.
opencode_config() {
	if [[ -f "$OPENCODE_DIR/opencode.jsonc" ]] && jq empty "$OPENCODE_DIR/opencode.jsonc" >/dev/null 2>&1; then
		printf '%s' "$OPENCODE_DIR/opencode.jsonc"
	else
		printf '%s' "$OPENCODE_DIR/opencode.json"
	fi
}

backup_once() {
	[[ -f $1 && ! -e "$1.before-magpie" ]] && cp -p "$1" "$1.before-magpie"
	return 0
}

json_edit() { # json_edit FILE JQ_ARGS...
	local file="$1" tmp
	shift
	[[ -f $file ]] || printf '{}\n' >"$file"
	tmp="$(mktemp)"
	jq "$@" "$file" >"$tmp" && cat "$tmp" >"$file"
	rm -f "$tmp"
}

codex_strip() { # remove our blocks and restore the lines use commented out
	[[ -f $CODEX_CONFIG ]] || return 0
	local tmp
	tmp="$(mktemp)"
	awk '
		/^# >>> magpie/ { skip = 1; next }
		/^# <<< magpie/ { skip = 0; next }
		skip { next }
		/^# magpie-was: / { sub(/^# magpie-was: /, ""); print; next }
		{ print }
	' "$CODEX_CONFIG" >"$tmp" && cat "$tmp" >"$CODEX_CONFIG"
	rm -f "$tmp"
}

cmd_use() {
	load "${1:-}"
	local person="${2:-$(first_person)}"
	[[ -e "$DIR/keys/$person.key" ]] || die "Usage: ./magpie.sh use $CLIENT <person>. People: $(people | tr '\n' ' ')"
	need jq curl
	local base keyfile opus sonnet haiku
	base="$(url)"
	curl -fs -o /dev/null "$base/healthz" || die "$base is not answering. Fix the gateway before pointing agents at it."
	opus="$(first_model 'claude-opus*')"
	sonnet="$(first_model 'claude-sonnet*')"
	haiku="$(first_model 'claude-haiku*')"
	[[ -n $sonnet ]] || die "use needs a claude-sonnet model on the gateway."
	mkdir -p "$MAGPIE_HOME"
	chmod 700 "$MAGPIE_HOME"
	keyfile="$MAGPIE_HOME/$CLIENT.key"
	(umask 077 && cp "$DIR/keys/$person.key" "$keyfile")

	step "Claude Code ($CLAUDE_SETTINGS)"
	mkdir -p "$(dirname "$CLAUDE_SETTINGS")"
	backup_once "$CLAUDE_SETTINGS"
	json_edit "$CLAUDE_SETTINGS" --arg base "$base" --arg helper "cat '$keyfile'" --arg client "$CLIENT" \
		--arg opus "$opus" --arg sonnet "$sonnet" --arg haiku "$haiku" '
		.env = ((.env // {}) + {ANTHROPIC_BASE_URL: $base, MAGPIE_GATEWAY: $client}
			+ (if $opus != "" then {ANTHROPIC_DEFAULT_OPUS_MODEL: $opus} else {} end)
			+ {ANTHROPIC_DEFAULT_SONNET_MODEL: $sonnet}
			+ (if $haiku != "" then {ANTHROPIC_DEFAULT_HAIKU_MODEL: $haiku} else {} end))
		| .apiKeyHelper = $helper'
	say "  base URL $base, key from $keyfile"

	if [[ -f $CODEX_CONFIG ]] || command -v codex >/dev/null 2>&1; then
		step "Codex ($CODEX_CONFIG)"
		mkdir -p "$(dirname "$CODEX_CONFIG")"
		touch "$CODEX_CONFIG"
		backup_once "$CODEX_CONFIG"
		codex_strip
		local tmp
		tmp="$(mktemp)"
		{
			printf '# >>> magpie (./magpie.sh unuse removes this block)\nmodel = "%s"\nmodel_provider = "magpie"\n# <<< magpie\n' "$sonnet"
			# Top-level model keys must come before any table, so the old ones are parked.
			awk 'BEGIN { top = 1 }
				/^\[/ { top = 0 }
				top && /^(model|model_provider)[[:space:]]*=/ { print "# magpie-was: " $0; next }
				{ print }' "$CODEX_CONFIG"
			printf '# >>> magpie providers\n[model_providers.magpie]\nname = "Magpie Gateway (%s)"\nbase_url = "%s/v1"\nwire_api = "responses"\n\n[model_providers.magpie.auth]\ncommand = "cat"\nargs = ["%s"]\n# <<< magpie providers\n' \
				"$CLIENT" "$base" "$keyfile"
		} >"$tmp" && cat "$tmp" >"$CODEX_CONFIG"
		rm -f "$tmp"
		say "  model $sonnet through provider magpie"
	fi

	if [[ -d $OPENCODE_DIR ]] || command -v opencode >/dev/null 2>&1; then
		mkdir -p "$OPENCODE_DIR"
		local OPENCODE_CONFIG
		OPENCODE_CONFIG="$(opencode_config)"
		step "opencode ($OPENCODE_CONFIG)"
		[[ -f "$OPENCODE_DIR/opencode.jsonc" && $OPENCODE_CONFIG != *.jsonc ]] &&
			say "  opencode.jsonc has comments, so it was left alone. If it sets a model, change it to magpie/$sonnet."
		backup_once "$OPENCODE_CONFIG"
		local previous
		previous="$(jq -r '.model // ""' "$OPENCODE_CONFIG" 2>/dev/null || true)"
		[[ $previous == magpie/* ]] || printf '%s\n' "$previous" >"$MAGPIE_HOME/opencode-previous-model"
		local models
		models="$(jq -n --arg o "$opus" --arg s "$sonnet" --arg h "$haiku" \
			'[$o, $s, $h] | map(select(. != "")) | map({key: ., value: {name: ("Magpie " + .)}}) | from_entries')"
		json_edit "$OPENCODE_CONFIG" --arg base "$base/v1" --arg key "{file:$keyfile}" --arg model "magpie/$sonnet" \
			--argjson models "$models" --arg client "$CLIENT" '
			.provider = ((.provider // {}) + {magpie: {
				npm: "@ai-sdk/anthropic", name: ("Magpie Gateway (" + $client + ")"),
				# The Anthropic SDK sends apiKey as x-api-key; the gateway reads the bearer header.
				options: {baseURL: $base, apiKey: $key, headers: {Authorization: ("Bearer " + $key)}},
				models: $models}})
			| .model = $model'
		say "  model magpie/$sonnet"
	fi

	if [[ -d $(dirname "$VSCODE_MODELS") ]]; then
		step "Copilot in VS Code ($VSCODE_MODELS)"
		backup_once "$VSCODE_MODELS"
		[[ -f $VSCODE_MODELS ]] || printf '[]\n' >"$VSCODE_MODELS"
		local entries
		entries="$(jq -n --arg base "$base" --arg o "$opus" --arg s "$sonnet" --arg h "$haiku" '
			[$o, $s, $h] | map(select(. != "")) | map({id: ., name: ("Magpie " + .), url: ($base + "/v1/messages"),
				toolCalling: true, vision: true, maxInputTokens: 200000, maxOutputTokens: 32000})')"
		local tmp
		tmp="$(mktemp)"
		jq --argjson models "$entries" '
			map(select((.name // "") | startswith("Magpie Gateway") | not))
			+ [{name: "Magpie Gateway", vendor: "customendpoint", apiType: "messages",
				apiKey: "${input:magpieKey}", models: $models}]' "$VSCODE_MODELS" >"$tmp" && cat "$tmp" >"$VSCODE_MODELS"
		rm -f "$tmp"
		say "  VS Code asks for the key once: paste the contents of $keyfile"
		say "  Inline completions still go to GitHub; chat and agent mode can pick the Magpie models."
	fi

	step "Done"
	say "New sessions use the gateway. Sessions already running keep their old settings."
	say "Undo with: ./magpie.sh unuse"
}

cmd_unuse() {
	need jq
	if [[ -f $CLAUDE_SETTINGS ]]; then
		json_edit "$CLAUDE_SETTINGS" '
			if (.env.MAGPIE_GATEWAY // "") != "" then
				.env |= del(.ANTHROPIC_BASE_URL, .ANTHROPIC_DEFAULT_OPUS_MODEL, .ANTHROPIC_DEFAULT_SONNET_MODEL,
					.ANTHROPIC_DEFAULT_HAIKU_MODEL, .MAGPIE_GATEWAY)
				| if (.apiKeyHelper // "") | test("\\.config/magpie/") then del(.apiKeyHelper) else . end
			else . end'
		say "Claude Code: gateway settings removed."
	fi
	codex_strip && [[ -f $CODEX_CONFIG ]] && say "Codex: gateway provider removed, previous model restored."
	local file
	for file in "$OPENCODE_DIR/opencode.json" "$OPENCODE_DIR/opencode.jsonc"; do
		[[ -f $file ]] && jq -e '.provider.magpie' "$file" >/dev/null 2>&1 || continue
		local previous=""
		[[ -f "$MAGPIE_HOME/opencode-previous-model" ]] && previous="$(<"$MAGPIE_HOME/opencode-previous-model")"
		json_edit "$file" --arg previous "$previous" '
			(if (.model // "") | startswith("magpie/") then
				(if $previous != "" then .model = $previous else del(.model) end)
			else . end)
			| if .provider then .provider |= del(.magpie) else . end
			| if .provider == {} then del(.provider) else . end'
		say "opencode: gateway provider removed from $file."
	done
	if [[ -f $VSCODE_MODELS ]]; then
		json_edit "$VSCODE_MODELS" 'map(select((.name // "") | startswith("Magpie Gateway") | not))'
		say "Copilot in VS Code: Magpie models removed."
	fi
	say "Backups from the first use are kept as <file>.before-magpie."
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
	code="$(curl -s -o /dev/null -w '%{http_code}' -c "$jar" \
		--data-urlencode "username=admin" --data-urlencode "password=$MAGPIE_ADMIN_PASSWORD" \
		"$base/api/auth/break-glass")"
	[[ $code == 303 || $code == 307 || $code == 302 ]] || die "Admin sign-in failed (HTTP $code)."
	curl -fsS -b "$jar" "$base/audit.csv" -o "$out/audit-log.csv"
	curl -fsS -b "$jar" "$base/use-cases.csv" -o "$out/ai-use-case-register.csv"
	curl -fsS -b "$jar" "$base/api/export" -o "$out/policy-export.json"
	rm -f "$jar"
	cp "$DIR/agentgateway.yaml" "$out/gateway-config.yaml"
	step "Report written to $out"
	ls -1 "$out" | sed 's/^/  /'
}

# ---------------------------------------------------------------- main

usage() {
	sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
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
	uitest) cmd_uitest "$@" ;;
	report) cmd_report "$@" ;;
	kubectl) cmd_kubectl "$@" ;;
	use) cmd_use "$@" ;;
	unuse) cmd_unuse ;;
	render) # for tests: print the proxy config
		load "${1:-}"
		render_proxy
		;;
	help | -h | --help | "") usage ;;
	*) usage 1 ;;
	esac
}

main "$@"
