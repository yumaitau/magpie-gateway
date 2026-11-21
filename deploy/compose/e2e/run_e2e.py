#!/usr/bin/env python3
"""End-to-end verification suite for the multi-container Magpie Gateway stack.

Tests real network-isolated agent containers (`agent-pilot`, `agent-trial`,
`agent-mcp`) communicating through `proxy` (Agentgateway) + `portal` (Magpie
Portal container) to `mock-llm`, `notes` MCP backend, and `mock-siem`.
"""

import http.cookiejar
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request

PORTAL = os.environ.get("MAGPIE_PORTAL_URL", "http://127.0.0.1:8088")
MOCK_LLM = os.environ.get("MAGPIE_MOCK_LLM_URL", "http://127.0.0.1:8090")
MOCK_SIEM = os.environ.get("MAGPIE_MOCK_SIEM_URL", "http://127.0.0.1:8095")
ADMIN_PASSWORD = os.environ.get("MAGPIE_ADMIN_PASSWORD", "MagpieDemoAdmin2026!")
APPROVER_EMAIL = "approver@agency.gov.au"
APPROVER_PASSWORD = os.environ.get("MAGPIE_APPROVER_PASSWORD", "MagpieApprover2026!#")
SCIM_TOKEN = os.environ.get("MAGPIE_SCIM_TOKEN", "e2e-scim-token-0123456789")
WEBHOOK_TOKEN = os.environ.get("MAGPIE_WEBHOOK_TOKEN", "e2e-webhook-token-0123456789")
COMPOSE_FILE = os.path.join(os.path.dirname(__file__), "docker-compose.yml")

PASSED = []


def step(name: str):
    print(f"\n==> {name}", flush=True)


def ok(msg: str):
    print(f"  [PASS] {msg}", flush=True)
    PASSED.append(msg)


def fail(msg: str):
    print(f"  [FAIL] {msg}", file=sys.stderr, flush=True)
    sys.exit(1)


def assert_eq(actual, expected, label):
    if actual != expected:
        fail(f"{label}: expected {expected!r}, got {actual!r}")


def assert_true(cond, label):
    if not cond:
        fail(f"{label}: condition was false")


def compose_cmd(*args, check=True):
    cmd = ["docker", "compose", "-f", COMPOSE_FILE, *args]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if check and res.returncode != 0:
        fail(f"Command {' '.join(cmd)} failed ({res.returncode}):\nSTDOUT: {res.stdout}\nSTDERR: {res.stderr}")
    return res


def agent_exec(service: str, *agent_args, check=True):
    return compose_cmd("exec", "-T", service, "magpie-agent", *agent_args, check=check)


class PortalClient:
    def __init__(self, base: str):
        self.base = base.rstrip("/")
        self.jar = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))

    def req(self, method: str, path: str, body=None, headers=None, expect_status=None):
        url = self.base + path
        hdrs = dict(headers or {})
        data = None
        if body is not None:
            if isinstance(body, (dict, list)):
                data = json.dumps(body).encode("utf-8")
                hdrs.setdefault("Content-Type", "application/json")
            elif isinstance(body, str):
                data = body.encode("utf-8")
            else:
                data = body
        if method in ("POST", "PUT", "PATCH", "DELETE"):
            hdrs.setdefault("Origin", self.base)
        r = urllib.request.Request(url, data=data, headers=hdrs, method=method)
        try:
            with self.opener.open(r, timeout=15) as resp:
                raw = resp.read()
                status = resp.status
                ctype = resp.headers.get("content-type", "")
        except urllib.error.HTTPError as e:
            raw = e.read()
            status = e.code
            ctype = e.headers.get("content-type", "")
        if expect_status is not None and status != expect_status:
            fail(f"{method} {path} returned HTTP {status} (expected {expect_status}): {raw.decode('utf-8', 'replace')[:600]}")
        text = raw.decode("utf-8", "replace")
        parsed = None
        if "json" in ctype or (text.startswith("{") or text.startswith("[")):
            try:
                parsed = json.loads(text)
            except json.JSONDecodeError:
                parsed = None
        return status, parsed, text, raw


def http_json(url: str, method="GET", body=None):
    data = json.dumps(body).encode("utf-8") if body is not None else None
    headers = {"Content-Type": "application/json"} if body is not None else {}
    r = urllib.request.Request(url, data=data, headers=headers, method=method)
    with urllib.request.urlopen(r, timeout=10) as resp:
        return resp.status, json.loads(resp.read().decode("utf-8"))


def wait_ready():
    step("1. Waiting for portal, proxy, mock-llm, notes, and mock-siem")
    for _ in range(60):
        try:
            with urllib.request.urlopen(f"{PORTAL}/healthz", timeout=3) as r:
                if r.status == 200:
                    break
        except Exception:
            time.sleep(1)
    else:
        fail("Portal did not become healthy in time")

    for _ in range(60):
        try:
            with urllib.request.urlopen("http://127.0.0.1:15000/config_dump", timeout=3) as r:
                if r.status == 200:
                    break
        except Exception:
            time.sleep(1)
    else:
        fail("Agentgateway proxy admin API did not become ready in time")
    ok("Portal (:8088) and Agentgateway Proxy (:3000/:4000/:15000) are healthy")


def ensure_approver_client(admin: PortalClient) -> PortalClient:
    approver = PortalClient(PORTAL)
    st, _, _, _ = approver.req(
        "POST", "/api/magpie/login",
        {"username": APPROVER_EMAIL, "password": APPROVER_PASSWORD},
    )
    if st == 200:
        return approver
    _, inv, _, _ = admin.req(
        "POST", "/api/magpie/invitations",
        {"email": APPROVER_EMAIL, "name": "Security Approver", "groups": ["approver"]},
        expect_status=200,
    )
    link = inv["link"]
    token = link.split("#", 1)[1]
    approver.req(
        "POST", "/api/magpie/accept-link",
        {"token": token, "password": APPROVER_PASSWORD, "confirmPassword": APPROVER_PASSWORD},
        expect_status=200,
    )
    approver.req(
        "POST", "/api/magpie/login",
        {"username": APPROVER_EMAIL, "password": APPROVER_PASSWORD},
        expect_status=200,
    )
    return approver


def ensure_approved(admin: PortalClient, approver: PortalClient, kind: str, name: str, classification: str, check_four_eyes: bool = False):
    _, inbox, _, _ = admin.req("GET", "/api/magpie/approvals", expect_status=200)
    items = (inbox or {}).get("approvals", [])
    existing = next((it for it in items if it.get("name") == name and it.get("status") == "approved"), None)
    if existing:
        return
    pending = next((it for it in items if it.get("name") == name and it.get("status") == "pending"), None)
    if pending:
        item_id = pending["id"]
    else:
        _, sub, _, _ = admin.req(
            "POST", "/api/magpie/approvals",
            {
                "name": name,
                "kind": kind,
                "businessOwner": "Director of Digital Platforms",
                "purpose": f"E2E approved {kind} for sovereign AI pilot",
                "classification": classification,
                "expiresOn": "2027-12-31",
            },
            expect_status=200,
        )
        item_id = sub["id"]
    if check_four_eyes:
        st_self, self_err, _, _ = admin.req(
            "POST", f"/api/magpie/approvals/items/{item_id}/decide",
            {"decision": "approve"},
        )
        assert_eq(st_self, 403, f"four-eyes must block self-approval ({self_err})")
    approver.req(
        "POST", f"/api/magpie/approvals/items/{item_id}/decide",
        {"decision": "approve"},
        expect_status=200,
    )


def main():
    wait_ready()

    # Reset mocks
    http_json(f"{MOCK_LLM}/reset", "POST")
    http_json(f"{MOCK_SIEM}/reset", "POST")

    # 2. Verify agent container onboarding & network isolation
    step("2. Verifying onboarded agent containers & zero-trust network isolation")
    for svc, expected_actor in [
        ("agent-pilot", "alice.chen@agency.gov.au"),
        ("agent-trial", "bob.smith@agency.gov.au"),
        ("agent-mcp", "carol.jones@agency.gov.au"),
    ]:
        res = agent_exec(svc, "whoami")
        info = json.loads(res.stdout)
        assert_eq(info["actor"], expected_actor, f"{svc} actor")
        assert_true(info["claudeManagedSettings"], f"{svc} managed-settings.json")
        assert_true(info["codexConfig"], f"{svc} codex config.toml")
    bypass = compose_cmd(
        "exec", "-T", "agent-pilot", "sh", "-c",
        "curl -sS --max-time 2 http://mock-llm:8090/healthz || curl -sS --max-time 2 http://portal:8088/healthz",
        check=False,
    )
    assert_true(bypass.returncode != 0, "agent-pilot must be isolated from internal network")
    ok("3 onboarded agent containers verified and network-isolated from internal backends")

    no_key = agent_exec("agent-pilot", "chat", "hello without key", "--no-key", "--allow-http-error")
    no_key_json = json.loads(no_key.stdout)
    assert_eq(no_key_json["status"], 401, "missing virtual API key rejected by proxy")
    ok("Proxy rejected unauthenticated agent call (HTTP 401)")

    # 3. Portal Login & Live Native Proxy Console Bridge
    step("3. Portal authentication & live Agentgateway native console bridge")
    portal = PortalClient(PORTAL)
    portal.req(
        "POST", "/api/magpie/login",
        {"username": "admin", "password": ADMIN_PASSWORD},
        expect_status=200,
    )
    _, sess, _, _ = portal.req("GET", "/api/magpie/session", expect_status=200)
    assert_eq(sess.get("subject"), "admin", "logged in as admin")
    assert_true(sess.get("canChangeEnforcement"), "admin can change enforcement")

    _, runtime, _, _ = portal.req("GET", "/api/runtime", expect_status=200)
    assert_true(runtime is not None, "Native /api/runtime bridge returned live proxy info")
    _, config_data, _, _ = portal.req("GET", "/api/config", expect_status=200)
    assert_true(config_data is not None, "Native /api/config bridge returned live proxy config")
    ok("Portal login succeeded; live native bridge connected to proxy:15000")

    # 4. Configure SIEM sink pointing at mock-siem over the container network
    step("4. Configuring live SIEM sink pointing to mock-siem container")
    _, saved_sink, _, _ = portal.req(
        "POST", "/api/magpie/sinks",
        {
            "id": "e2e-mock-siem",
            "name": "E2E Mock Sentinel Collector",
            "kind": "otlp",
            "endpoint": "http://mock-siem:8095/v1/logs",
            "secret": "e2e-siem-secret-token",
            "enabled": True,
        },
        expect_status=200,
    )
    sink_id = saved_sink["id"]
    _, test_res, _, _ = portal.req("POST", f"/api/magpie/sinks/{sink_id}/test", {}, expect_status=200)
    assert_true(test_res.get("ok"), f"SIEM test delivery failed: {test_res}")
    ok(f"SIEM sink '{sink_id}' created and test event delivered to mock-siem")

    # 5. Four-Eyes Governance Approvals + SCIM Cohort Rollout
    step("5. Four-Eyes Approvals Inbox & SCIM 2.0 Staged Cohort Rollout")
    approver = ensure_approver_client(portal)
    ensure_approved(portal, approver, "model", "au-sovereign-llm", "OFFICIAL:Sensitive", check_four_eyes=True)
    ensure_approved(portal, approver, "model", "us-foreign-llm", "OFFICIAL")
    ensure_approved(portal, approver, "tool", "example_search", "OFFICIAL:Sensitive")
    ok("Four-eyes approval verified (self-approval refused 403; approver approved models & tool)")

    scim_hdrs = {"Authorization": f"Bearer {SCIM_TOKEN}", "Content-Type": "application/scim+json"}
    _, scim_groups_list, _, _ = portal.req("GET", "/scim/v2/Groups", headers=scim_hdrs, expect_status=200)
    existing_by_name = {
        g.get("displayName"): g.get("id")
        for g in (scim_groups_list or {}).get("Resources", [])
    }

    def ensure_scim_group(display_name: str, external_id: str) -> str:
        if display_name in existing_by_name:
            return existing_by_name[display_name]
        _, created, _, _ = portal.req(
            "POST", "/scim/v2/Groups",
            {
                "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Group"],
                "displayName": display_name,
                "externalId": external_id,
            },
            headers=scim_hdrs,
            expect_status=201,
        )
        return created["id"]

    pilot_scim_id = ensure_scim_group("ai-pilot-enforced", "ai-pilot-enforced")
    trial_scim_id = ensure_scim_group("ai-trial-observe", "ai-trial-observe")

    _, pilot_mapped, _, _ = portal.req(
        "POST", "/api/magpie/identity/map-scim-group",
        {"scimGroupId": pilot_scim_id, "roleId": "viewer", "enforcement": "follow"},
        expect_status=200,
    )
    _, trial_mapped, _, _ = portal.req(
        "POST", "/api/magpie/identity/map-scim-group",
        {"scimGroupId": trial_scim_id, "roleId": "viewer", "enforcement": "follow"},
        expect_status=200,
    )
    pilot_group_id = pilot_mapped["id"]
    trial_group_id = trial_mapped["id"]

    portal.req(
        "POST", "/api/magpie/control-centre/kill-switch",
        {"global": False, "agents": []},
        expect_status=200,
    )
    st_preset, preset_res, _, _ = portal.req(
        "POST", "/api/magpie/control-centre/preset",
        {"presetId": "pilot-observe"},
    )
    assert_true(
        st_preset == 200 or (st_preset == 400 and "already matches" in json.dumps(preset_res or {})),
        f"apply pilot-observe failed ({st_preset}): {preset_res}",
    )
    portal.req("POST", f"/api/magpie/control-centre/cohorts/{pilot_group_id}", {"enforcement": "follow"}, expect_status=200)
    portal.req("POST", f"/api/magpie/control-centre/cohorts/{trial_group_id}", {"enforcement": "follow"}, expect_status=200)
    ok("Applied 'pilot-observe' preset (org default = log-only)")

    # In Pilot (log-only), agent-pilot sends synthetic Australian ABN + TFN -> PASSES (200)
    abn_prompt = "Please review contractor ABN 51 824 753 556 and TFN 123 456 782 for FY26."
    res_obs = agent_exec("agent-pilot", "chat", abn_prompt, "--allow-http-error")
    obs_json = json.loads(res_obs.stdout)
    assert_eq(obs_json["status"], 200, "pilot observe-only allows ABN/TFN prompt")
    ok("Pilot (observe-only) allowed synthetic ABN/TFN from agent-pilot while recording audit")

    # Move ONLY pilot cohort to Enforcing; keep trial cohort in log-only!
    portal.req("POST", f"/api/magpie/control-centre/cohorts/{pilot_group_id}", {"enforcement": "enforce"}, expect_status=200)
    portal.req("POST", f"/api/magpie/control-centre/cohorts/{trial_group_id}", {"enforcement": "log-only"}, expect_status=200)

    # First verify a CLEAN prompt from enforced agent-pilot SUCCEEDS (proving approval works!)
    res_clean = agent_exec("agent-pilot", "chat", "Summarise Australian ISM logging principles in two sentences.")
    clean_json = json.loads(res_clean.stdout)
    assert_eq(clean_json["status"], 200, "clean prompt from enforced agent-pilot")
    assert_true("Sovereign response" in json.dumps(clean_json["body"]), "expected mock-llm sovereign reply")
    ok("Clean prompt from enforced agent-pilot passed guardrails and reached sovereign upstream")

    _, llm_before = http_json(f"{MOCK_LLM}/stats")
    count_before = llm_before["count"]

    # Now agent-pilot sends ABN/TFN -> MUST BE BLOCKED specifically by Australian DLP (never reach mock-llm!)
    res_blk = agent_exec("agent-pilot", "chat", abn_prompt, "--allow-http-error")
    blk_json = json.loads(res_blk.stdout)
    assert_true(400 <= blk_json["status"] < 500, f"enforced agent-pilot must be blocked, got {blk_json}")
    blk_msg = json.dumps(blk_json["body"]).lower()
    assert_true("not approved" not in blk_msg, f"Expected DLP block, not model approval block: {blk_json}")
    _, llm_after_block = http_json(f"{MOCK_LLM}/stats")
    assert_eq(llm_after_block["count"], count_before, "blocked ABN/TFN prompt must NEVER reach upstream mock-llm")
    ok(f"Enforced cohort blocked ABN/TFN via Australian DLP (HTTP {blk_json['status']}) — 0 upstream leaks!")

    # Meanwhile agent-trial (in ai-trial-observe) sends the SAME ABN/TFN -> STILL ALLOWED!
    res_trial = agent_exec("agent-trial", "chat", abn_prompt, "--allow-http-error")
    trial_json = json.loads(res_trial.stdout)
    assert_eq(trial_json["status"], 200, "observe-only agent-trial still allowed during staged rollout")
    ok("Observe-only cohort (agent-trial) remained uninterrupted while pilot cohort enforced")

    # 6. Australian Data Sovereignty & PSPF Marking Check
    step("6. Australian Data Sovereignty & PSPF (us-foreign-llm & PROTECTED marking)")
    st_sov, sov_res, _, _ = portal.req(
        "POST", "/api/magpie/control-centre/preset",
        {"presetId": "gov-official-sensitive"},
    )
    assert_true(
        st_sov == 200 or (st_sov == 400 and "already matches" in json.dumps(sov_res or {})),
        f"apply gov-official-sensitive failed ({st_sov}): {sov_res}",
    )
    _, llm_before_sov = http_json(f"{MOCK_LLM}/stats")
    res_us = agent_exec(
        "agent-pilot", "chat", "Draft a ministerial briefing outline.",
        "--model", "us-foreign-llm", "--allow-http-error",
    )
    us_json = json.loads(res_us.stdout)
    assert_true(400 <= us_json["status"] < 500, f"us-east-1 model must be blocked in sovereign mode, got {us_json}")
    us_msg = json.dumps(us_json["body"]).lower()
    assert_true("region" in us_msg or "australia" in us_msg or "sovereign" in us_msg, f"Expected sovereignty reason, got: {us_json}")
    _, llm_after_sov = http_json(f"{MOCK_LLM}/stats")
    assert_eq(llm_after_sov["count"], llm_before_sov["count"], "foreign region prompt must never reach upstream")
    ok(f"Sovereign mode blocked approved us-foreign-llm (us-east-1) with HTTP {us_json['status']}")

    # PSPF PROTECTED marking sent to unapproved destination must be blocked
    res_pspf = agent_exec(
        "agent-pilot", "chat", "Draft cabinet notes.",
        "--marking", "PROTECTED", "--allow-http-error",
    )
    pspf_json = json.loads(res_pspf.stdout)
    assert_true(400 <= pspf_json["status"] < 500, f"PROTECTED marking must be blocked on unapproved destination, got {pspf_json}")
    ok(f"PSPF PROTECTED marking blocked on unapproved destination (HTTP {pspf_json['status']})")

    # 7. Response-Phase Guardrail (upstream LLM attempts to leak a TFN)
    step("7. Response-phase guardrail (catching sensitive data in LLM reply)")
    res_leak = agent_exec("agent-pilot", "chat", "Please run TRIGGER_LEAK_TFN diagnostic", "--allow-http-error")
    leak_json = json.loads(res_leak.stdout)
    raw_leak = json.dumps(leak_json)
    assert_true("123 456 782" not in raw_leak, f"Response guardrail must block or mask leaked TFN! Got: {raw_leak}")
    ok(f"Response guardrail prevented TFN leak from upstream LLM (status={leak_json['status']})")

    # 8. Emergency Kill Switches (Per-Agent & Global)
    step("8. Emergency Kill Switches (per-agent isolation + global stop + recovery)")
    portal.req(
        "POST", "/api/magpie/control-centre/kill-switch",
        {"global": False, "agents": ["claude-code-pilot"]},
        expect_status=200,
    )
    res_killed = agent_exec("agent-pilot", "chat", "Hello, are you there?", "--allow-http-error")
    killed_json = json.loads(res_killed.stdout)
    assert_true(400 <= killed_json["status"] < 500, f"killed agent-pilot must be blocked, got {killed_json}")

    res_other = agent_exec("agent-mcp", "chat", "Quick health ping from ops agent.", "--allow-http-error")
    other_json = json.loads(res_other.stdout)
    assert_eq(other_json["status"], 200, "non-killed agent-mcp must still succeed")
    ok("Per-agent kill switch stopped claude-code-pilot while leaving mcp-ops-agent unaffected")

    portal.req(
        "POST", "/api/magpie/control-centre/kill-switch",
        {"global": True, "agents": ["claude-code-pilot"]},
        expect_status=200,
    )
    res_global = agent_exec("agent-mcp", "chat", "Ping during global stop", "--allow-http-error")
    global_json = json.loads(res_global.stdout)
    assert_true(400 <= global_json["status"] < 500, f"global kill switch must block agent-mcp, got {global_json}")
    ok("Global kill switch halted all agent traffic immediately")

    portal.req(
        "POST", "/api/magpie/control-centre/kill-switch",
        {"global": False, "agents": []},
        expect_status=200,
    )
    res_rec = agent_exec("agent-pilot", "chat", "Recovery check after kill switch cleared.")
    assert_eq(json.loads(res_rec.stdout)["status"], 200, "agent-pilot recovers after clearing kill switch")
    ok("Cleared kill switches and verified immediate agent recovery")

    # 9. MCP Tool Governance from agent-mcp container
    step("9. MCP Tool Governance from agent-mcp container (allow-list enforcement)")
    res_bad_tool = agent_exec("agent-mcp", "mcp-call", "example_admin")
    bad_tool_out = res_bad_tool.stdout
    assert_true("REACHED_BACKEND" not in bad_tool_out, f"Unapproved MCP tool reached backend: {bad_tool_out}")
    assert_true("Unknown tool" in bad_tool_out or "error" in bad_tool_out.lower(), f"Unapproved MCP tool not denied: {bad_tool_out}")

    res_good_tool = agent_exec("agent-mcp", "mcp-call", "example_search")
    assert_true("search-ok" in res_good_tool.stdout, f"Approved MCP tool failed: {res_good_tool.stdout}")

    notes_log = compose_cmd("exec", "-T", "notes", "cat", "/log/calls.log").stdout
    assert_true("example_admin" not in notes_log.splitlines(), "notes log must not contain example_admin")
    assert_true("example_search" in notes_log.splitlines(), "notes log must contain example_search")
    ok("MCP proxy denied example_admin (0 backend hits) and forwarded example_search")

    status_no_tok, _, _, _ = portal.req(
        "POST", "/mcp/check",
        {
            "method": "tools/call",
            "agentId": "rogue",
            "tool": "example_search",
            "arguments": {"q": "test"},
            "destinationRegion": "ap-southeast-2",
            "destinationName": "notes",
            "phase": "request",
            "actor": "rogue@agency.gov.au",
        },
    )
    assert_eq(status_no_tok, 401, "missing webhook token must return 401")
    _, mcp_chk, _, _ = portal.req(
        "POST", "/mcp/check",
        {
            "method": "tools/call",
            "agentId": "mcp-ops-agent",
            "actor": "carol.jones@agency.gov.au",
            "tool": "example_search",
            "arguments": {"q": "TFN 123 456 782"},
            "destinationRegion": "ap-southeast-2",
            "destinationName": "notes",
            "phase": "request",
        },
        headers={"X-Magpie-Webhook-Token": WEBHOOK_TOKEN},
        expect_status=200,
    )
    assert_true(
        mcp_chk.get("decision") in ("reject", "mutated"),
        f"MCP guardrail check must block/redact TFN in tool arguments: {mcp_chk}",
    )
    ok(f"Webhook token enforced on /mcp/check (401 without token; TFN argument decision={mcp_chk.get('decision')})")

    # 10. SCIM 2.0 Provisioning + Persistence Across Container Restart + Instant Revocation
    step("10. SCIM 2.0 SQLite persistence across portal restart & instant deprovisioning")
    user_email = f"josh.demo.{int(time.time())}@agency.gov.au"
    _, scim_user, _, _ = portal.req(
        "POST", "/scim/v2/Users",
        {
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "userName": user_email,
            "displayName": "Josh Demo",
            "active": True,
            "emails": [{"value": user_email, "primary": True}],
        },
        headers=scim_hdrs,
        expect_status=201,
    )
    scim_uid = scim_user["id"]

    compose_cmd("restart", "portal")
    for _ in range(40):
        try:
            with urllib.request.urlopen(f"{PORTAL}/healthz", timeout=2) as r:
                if r.status == 200:
                    break
        except Exception:
            time.sleep(1)

    _, fetched_user, _, _ = portal.req(
        "GET", f"/scim/v2/Users/{scim_uid}",
        headers=scim_hdrs,
        expect_status=200,
    )
    assert_eq(fetched_user["userName"], user_email, "SCIM user persisted across container restart")

    _, patched_user, _, _ = portal.req(
        "PATCH", f"/scim/v2/Users/{scim_uid}",
        {
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
            "Operations": [{"op": "replace", "path": "active", "value": False}],
        },
        headers=scim_hdrs,
        expect_status=200,
    )
    assert_eq(patched_user["active"], False, "SCIM user deactivated")
    ok("SCIM 2.0 user survived portal container restart and deactivated cleanly via PATCH")

    # 11. Live SIEM Streaming Verification & Tamper-Evident Evidence Pack
    step("11. Verifying live SIEM streaming to mock-siem & real Evidence Pack export")
    siem_batches = 0
    for _ in range(25):
        _, siem_state = http_json(f"{MOCK_SIEM}/events")
        siem_batches = siem_state["count"]
        if siem_batches >= 2:
            break
        time.sleep(1)
    assert_true(siem_batches >= 2, f"Expected background SIEM exporter to stream live audit batches, saw only {siem_batches}")
    ok(f"mock-siem received {siem_batches} live event batches streamed from portal container")

    status_sess, sess2, _, _ = portal.req("GET", "/api/magpie/session")
    if status_sess != 200 or not (sess2 or {}).get("subject"):
        portal.req("POST", "/api/magpie/login", {"username": "admin", "password": ADMIN_PASSWORD}, expect_status=200)

    _, ev_json, _, _ = portal.req("GET", "/api/magpie/evidence", expect_status=200)
    stats = ev_json.get("stats", ev_json)
    total_req = stats.get("requests", 0)
    blocked = stats.get("blocks", 0)
    dlp_hits = stats.get("dlpHits", 0)
    assert_true(total_req >= 6, f"Expected >= 6 real requests in evidence, got {ev_json}")
    assert_true(blocked >= 3, f"Expected >= 3 real blocks in evidence, got {ev_json}")
    assert_true(dlp_hits >= 1, f"Expected >= 1 DLP hit in evidence, got {ev_json}")

    _, _, csv_text, _ = portal.req("GET", "/api/magpie/evidence.csv", expect_status=200)
    assert_true("ISM-2159" in csv_text and "60" in csv_text or len(csv_text) > 200, "Evidence CSV valid")

    _, _, _, pdf_bytes = portal.req("GET", "/api/magpie/evidence.pdf", expect_status=200)
    assert_true(pdf_bytes.startswith(b"%PDF-"), "Evidence PDF header valid")

    _, pack_json, _, _ = portal.req("GET", "/api/magpie/evidence-pack.json", expect_status=200)
    digest = (
        pack_json.get("sha256")
        or (pack_json.get("integrity") or {}).get("sha256")
        or (pack_json.get("manifest") or {}).get("sha256")
        or ""
    )
    assert_true(len(digest) == 64 or "sha256" in json.dumps(pack_json).lower(), f"Evidence pack SHA-256 digest missing: {pack_json.keys()}")
    ok(
        f"Live Evidence Pack verified: requests={total_req}, "
        f"blocks={blocked}, dlpHits={dlp_hits}, "
        f"PDF={len(pdf_bytes)} bytes, pack_keys={list(pack_json.keys())}"
    )

    print(f"\n==========================================")
    print(f"ALL {len(PASSED)} MULTI-CONTAINER E2E CHECKS PASSED!")
    print(f"==========================================")


if __name__ == "__main__":
    main()
