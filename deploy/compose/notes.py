#!/usr/bin/env python3
"""Tiny streamable-HTTP MCP server for the Compose smoke test.

Two tools. `example_search` is on the proxy allow list. `example_admin`
is not. Each call appends its name to CALL_LOG so the smoke test can see
whether the proxy forwarded it.
"""

import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

CALL_LOG = os.environ.get("CALL_LOG", "/tmp/calls.log")
PORT = int(os.environ.get("PORT", "3001"))
SESSION = "notes-session"

TOOLS = [
    {
        "name": "example_search",
        "description": "Approved lookup.",
        "inputSchema": {
            "type": "object",
            "properties": {"q": {"type": "string"}},
        },
    },
    {
        "name": "example_admin",
        "description": "Not approved.",
        "inputSchema": {
            "type": "object",
            "properties": {"q": {"type": "string"}},
        },
    },
]


def log_call(name: str) -> None:
    with open(CALL_LOG, "a", encoding="utf-8") as handle:
        handle.write(name + "\n")


def result(request_id, value):
    return {"jsonrpc": "2.0", "id": request_id, "result": value}


def tool_text(name: str) -> str:
    if name == "example_admin":
        return "REACHED_BACKEND"
    return "search-ok"


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        return

    def do_GET(self):
        if self.path.rstrip("/") == "/healthz":
            body = b"ok"
            self.send_response(200)
            self.send_header("content-type", "text/plain")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_error(404)

    def do_POST(self):
        if self.path.rstrip("/") != "/mcp":
            self.send_error(404)
            return
        length = int(self.headers.get("content-length", "0"))
        raw = self.rfile.read(length) if length else b""
        try:
            message = json.loads(raw.decode("utf-8") or "{}")
        except json.JSONDecodeError:
            self.send_error(400)
            return
        method = message.get("method")
        request_id = message.get("id")
        if method == "notifications/initialized":
            self.send_response(202)
            self.send_header("content-length", "0")
            self.end_headers()
            return
        if method == "initialize":
            params = message.get("params") or {}
            payload = result(
                request_id,
                {
                    "protocolVersion": params.get("protocolVersion", "2025-06-18"),
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "magpie-notes", "version": "0.0.1"},
                },
            )
            self._json(200, payload, session=True)
            return
        if self.headers.get("mcp-session-id") != SESSION:
            self._json(
                400,
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "error": {"code": -32000, "message": "missing session"},
                },
            )
            return
        if method == "tools/list":
            self._json(200, result(request_id, {"tools": TOOLS}), session=True)
            return
        if method == "tools/call":
            params = message.get("params") or {}
            name = params.get("name", "")
            log_call(name)
            self._json(
                200,
                result(
                    request_id,
                    {
                        "content": [{"type": "text", "text": tool_text(name)}],
                        "isError": False,
                    },
                ),
                session=True,
            )
            return
        self._json(
            200,
            {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {"code": -32601, "message": "method not found"},
            },
            session=True,
        )

    def _json(self, status, payload, session=False):
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        if session:
            self.send_header("mcp-session-id", SESSION)
        self.end_headers()
        self.wfile.write(body)


def main():
    server = ThreadingHTTPServer(("0.0.0.0", PORT), Handler)
    server.serve_forever()


if __name__ == "__main__":
    main()
