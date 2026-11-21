#!/usr/bin/env python3
"""Mock SIEM / Microsoft Sentinel / OTLP HTTP collector for E2E verification.

Accepts POST requests on /ingest (or any subpath like /v1/logs, /services/collector)
and stores both headers and parsed JSON bodies so tests can prove Magpie Portal's
background SIEM exporter streams live audit events over the container network.
"""

import json
import os
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PORT = int(os.environ.get("PORT", "8095"))
LOCK = threading.Lock()
EVENTS = []


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        return

    def _send_json(self, status: int, payload: dict) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        path = self.path.split("?", 1)[0].rstrip("/")
        if path == "/healthz":
            self._send_json(200, {"status": "ok"})
            return
        if path == "/events":
            with LOCK:
                snapshot = list(EVENTS)
            self._send_json(200, {"count": len(snapshot), "batches": snapshot})
            return
        self._send_json(404, {"error": "not found"})

    def do_POST(self):
        path = self.path.split("?", 1)[0].rstrip("/")
        if path == "/reset":
            with LOCK:
                EVENTS.clear()
            self._send_json(200, {"reset": True})
            return
        length = int(self.headers.get("content-length", "0"))
        raw = self.rfile.read(length) if length else b""
        text = raw.decode("utf-8", errors="replace")
        try:
            parsed = json.loads(text) if text.strip() else None
        except json.JSONDecodeError:
            # Handle NDJSON / Splunk HEC multi-line payloads
            lines = [line for line in text.splitlines() if line.strip()]
            parsed = []
            for line in lines:
                try:
                    parsed.append(json.loads(line))
                except json.JSONDecodeError:
                    parsed.append({"raw": line})

        with LOCK:
            EVENTS.append(
                {
                    "ts": int(time.time()),
                    "path": path,
                    "content_type": self.headers.get("content-type", ""),
                    "has_auth": bool(self.headers.get("authorization")),
                    "payload": parsed,
                }
            )
        self._send_json(200, {"text": "Success", "code": 0})


def main():
    server = ThreadingHTTPServer(("0.0.0.0", PORT), Handler)
    server.serve_forever()


if __name__ == "__main__":
    main()
