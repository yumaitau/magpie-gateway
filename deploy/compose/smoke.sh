#!/bin/sh
# Call an unapproved tool through the proxy and require that the notes
# server never saw it. Then call the approved tool and require that it did.
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)"
compose() {
	docker compose -f "$ROOT/deploy/compose/docker-compose.yml" "$@"
}
PROXY="${MAGPIE_PROXY_URL:-http://127.0.0.1:3000/mcp}"
PORTAL="${MAGPIE_PORTAL_URL:-http://127.0.0.1:8088/healthz}"

i=0
until curl -fsS "$PORTAL" >/dev/null; do
	i=$((i + 1))
	if [ "$i" -gt 60 ]; then
		echo "portal did not become ready" >&2
		compose logs portal >&2 || true
		exit 1
	fi
	sleep 1
done

headers=""
i=0
while [ "$i" -lt 60 ]; do
	headers="$(curl -si "$PROXY" \
		-H 'content-type: application/json' \
		-H 'accept: application/json, text/event-stream' \
		-d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"magpie-smoke","version":"0"}}}' || true)"
	session="$(printf '%s\n' "$headers" | awk 'BEGIN{IGNORECASE=1} /^mcp-session-id:/ {print $2}' | tr -d '\r')"
	if [ -n "$session" ]; then
		break
	fi
	i=$((i + 1))
	sleep 1
done
if [ -z "${session:-}" ]; then
	echo "proxy did not return a session" >&2
	printf '%s\n' "$headers" >&2
	compose logs proxy notes >&2 || true
	exit 1
fi

curl -fsS -o /dev/null "$PROXY" \
	-H 'content-type: application/json' \
	-H 'accept: application/json, text/event-stream' \
	-H "mcp-session-id: $session" \
	-d '{"jsonrpc":"2.0","method":"notifications/initialized"}'

call() {
	name="$1"
	curl -sS "$PROXY" \
		-H 'content-type: application/json' \
		-H 'accept: application/json, text/event-stream' \
		-H "mcp-session-id: $session" \
		-d "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"$name\",\"arguments\":{\"q\":\"x\"}}}"
}

denied="$(call example_admin)"
printf '%s\n' "$denied"
printf '%s\n' "$denied" | grep -q 'Unknown tool' || {
	echo "unapproved tool was not denied by the proxy" >&2
	exit 1
}
printf '%s\n' "$denied" | grep -q 'REACHED_BACKEND' && {
	echo "unapproved tool reached the notes server" >&2
	exit 1
}

allowed="$(call example_search)"
printf '%s\n' "$allowed"
printf '%s\n' "$allowed" | grep -q 'search-ok' || {
	echo "approved tool did not pass the proxy" >&2
	exit 1
}

log="$(compose exec -T notes cat /log/calls.log 2>/dev/null || true)"
printf '%s\n' "$log" | grep -qx 'example_admin' && {
	echo "notes log contains the denied tool" >&2
	exit 1
}
printf '%s\n' "$log" | grep -qx 'example_search' || {
	echo "notes log is missing the approved tool" >&2
	printf '%s\n' "$log" >&2
	exit 1
}

echo "proxy denied example_admin and forwarded example_search"
