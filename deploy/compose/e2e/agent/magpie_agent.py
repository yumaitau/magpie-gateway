#!/usr/bin/env python3
"""Onboarded AI agent CLI running inside isolated agent containers.

Talks exclusively to Magpie Gateway's proxy endpoints (OPENAI_BASE_URL for LLM
calls, MAGPIE_MCP_URL for MCP tools) using its assigned virtual API key.
"""

import argparse
import json
import os
import sys
import urllib.error
import urllib.request


BASE_URL = os.environ.get("OPENAI_BASE_URL", "http://proxy:4000/v1").rstrip("/")
API_KEY = os.environ.get("OPENAI_API_KEY", "")
MCP_URL = os.environ.get("MAGPIE_MCP_URL", "http://proxy:3000/mcp")
DEFAULT_MODEL = os.environ.get("MAGPIE_DEFAULT_MODEL", "au-sovereign-llm")


def cmd_whoami(_args):
    info = {
        "agentName": os.environ.get("AGENT_NAME", "unknown"),
        "actor": os.environ.get("AGENT_ACTOR", "unknown"),
        "cohort": os.environ.get("AGENT_COHORT", "unknown"),
        "baseUrl": BASE_URL,
        "mcpUrl": MCP_URL,
        "hasVirtualKey": bool(API_KEY),
        "claudeManagedSettings": os.path.exists(os.path.expanduser("~/.claude/managed-settings.json")),
        "codexConfig": os.path.exists(os.path.expanduser("~/.codex/config.toml")),
    }
    print(json.dumps(info, indent=2))


def cmd_chat(args):
    model = args.model or DEFAULT_MODEL
    headers = {
        "Content-Type": "application/json",
    }
    if not args.no_key and API_KEY:
        headers["Authorization"] = f"Bearer {API_KEY}"
    if args.marking:
        headers["x-magpie-marking"] = args.marking
    payload = json.dumps(
        {
            "model": model,
            "messages": [{"role": "user", "content": args.prompt}],
        }
    ).encode("utf-8")
    req = urllib.request.Request(f"{BASE_URL}/chat/completions", data=payload, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            body = resp.read().decode("utf-8", errors="replace")
            print(json.dumps({"status": resp.status, "body": _try_json(body)}, indent=2))
    except urllib.error.HTTPError as err:
        body = err.read().decode("utf-8", errors="replace")
        print(json.dumps({"status": err.code, "body": _try_json(body)}, indent=2))
        if not args.allow_http_error:
            sys.exit(2)


def _mcp_post(payload: dict, session_id: str | None = None):
    headers = {
        "Content-Type": "application/json",
        "Accept": "application/json, text/event-stream",
    }
    if session_id:
        headers["mcp-session-id"] = session_id
    req = urllib.request.Request(
        MCP_URL,
        data=json.dumps(payload).encode("utf-8"),
        headers=headers,
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            sid = resp.headers.get("mcp-session-id")
            body = resp.read().decode("utf-8", errors="replace")
            return resp.status, sid, body
    except urllib.error.HTTPError as err:
        sid = err.headers.get("mcp-session-id") if err.headers else None
        body = err.read().decode("utf-8", errors="replace")
        return err.code, sid, body


def _mcp_handshake():
    status, sid, body = _mcp_post(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": os.environ.get("AGENT_NAME", "magpie-agent"), "version": "1.0"},
            },
        }
    )
    if not sid:
        raise RuntimeError(f"MCP initialize failed ({status}): {body}")
    _mcp_post({"jsonrpc": "2.0", "method": "notifications/initialized"}, sid)
    return sid


def _parse_sse_or_json(raw: str):
    raw = raw.strip()
    if raw.startswith("{"):
        return json.loads(raw)
    for line in raw.splitlines():
        if line.startswith("data:"):
            part = line[5:].strip()
            if part:
                try:
                    return json.loads(part)
                except json.JSONDecodeError:
                    pass
    return {"raw": raw}


def cmd_mcp_list(_args):
    sid = _mcp_handshake()
    status, _, body = _mcp_post({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}, sid)
    print(json.dumps({"status": status, "response": _parse_sse_or_json(body)}, indent=2))


def cmd_mcp_call(args):
    sid = _mcp_handshake()
    arguments = json.loads(args.arguments) if args.arguments else {}
    status, _, body = _mcp_post(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": args.tool, "arguments": arguments},
        },
        sid,
    )
    print(json.dumps({"status": status, "response": _parse_sse_or_json(body)}, indent=2))


def _try_json(raw: str):
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return raw


def main():
    parser = argparse.ArgumentParser(prog="magpie-agent")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_who = sub.add_parser("whoami")
    p_who.set_defaults(func=cmd_whoami)

    p_chat = sub.add_parser("chat")
    p_chat.add_argument("prompt")
    p_chat.add_argument("--model", default=None)
    p_chat.add_argument("--marking", default=None)
    p_chat.add_argument("--no-key", action="store_true")
    p_chat.add_argument("--allow-http-error", action="store_true")
    p_chat.set_defaults(func=cmd_chat)

    p_ml = sub.add_parser("mcp-list")
    p_ml.set_defaults(func=cmd_mcp_list)

    p_mc = sub.add_parser("mcp-call")
    p_mc.add_argument("tool")
    p_mc.add_argument("--arguments", default='{"q":"budget-briefing"}')
    p_mc.set_defaults(func=cmd_mcp_call)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
