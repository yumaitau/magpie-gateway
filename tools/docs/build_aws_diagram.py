#!/usr/bin/env python3
"""Generate the official AWS Solution Architecture Diagram for Magpie Gateway
using genuine AWS Architecture Icon SVGs stored in docs/diagrams/aws-icons/.

Outputs:
  - docs/diagrams/magpie-aws-architecture.svg  (vector, self-contained with embedded AWS SVGs)
  - docs/diagrams/magpie-aws-architecture.png  (2x high-DPI PNG via Chromium for PDF/DOCX/slides)
"""

import base64
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ICONS_DIR = ROOT / "docs" / "diagrams" / "aws-icons"
OUT_SVG = ROOT / "docs" / "diagrams" / "magpie-aws-architecture.svg"


def icon_data_uri(name: str) -> str:
    svg_bytes = (ICONS_DIR / name).read_bytes()
    b64 = base64.b64encode(svg_bytes).decode("ascii")
    return f"data:image/svg+xml;base64,{b64}"


def node_card(x, y, w, h, icon_file, title, subtitle="", accent="#1B365D", fill="#FFFFFF", badge=""):
    uri = icon_data_uri(icon_file)
    lines = [l for l in subtitle.split("\n") if l] if subtitle else []
    badge_svg = ""
    if badge:
        bw = max(58, len(badge) * 5.8 + 14)
        # Place badge cleanly in bottom-right corner so it NEVER collides with titles
        bx = x + w - bw - 8
        by = y + h - 24
        badge_svg = f"""
        <rect x="{bx}" y="{by}" width="{bw}" height="17" rx="8.5" fill="{accent}" fill-opacity="0.11" stroke="{accent}" stroke-width="0.8"/>
        <text x="{bx + bw/2}" y="{by + 11.8}" font-size="9" font-weight="700" fill="{accent}" text-anchor="middle">{badge}</text>
        """
    sub_svg = ""
    for i, line in enumerate(lines):
        sub_svg += f'<text x="{x + 60}" y="{y + 42 + i * 14}" font-size="10.2" fill="#475569">{line}</text>'
    icon_size = 40 if h >= 64 else 34
    icon_y = y + (h - icon_size) / 2
    title_y = y + 24 if lines else y + h / 2 + 4
    return f"""
    <g filter="url(#shadow)">
      <rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="{fill}" stroke="#CBD5E1" stroke-width="1.2"/>
      <rect x="{x}" y="{y}" width="4" height="{h}" rx="2" fill="{accent}"/>
      <image href="{uri}" x="{x + 11}" y="{icon_y}" width="{icon_size}" height="{icon_size}"/>
      <text x="{x + 60}" y="{title_y}" font-size="12" font-weight="700" fill="#0F172A">{title}</text>
      {sub_svg}
      {badge_svg}
    </g>
    """


def group_header(x, y, icon_file, title, subtitle, color):
    uri = icon_data_uri(icon_file)
    return f"""
    <image href="{uri}" x="{x + 10}" y="{y + 8}" width="26" height="26"/>
    <text x="{x + 44}" y="{y + 21}" font-size="12" font-weight="800" fill="{color}">{title}</text>
    <text x="{x + 44}" y="{y + 35}" font-size="9.8" fill="#475569">{subtitle}</text>
    """


def pill_label(lx, ly, text, color):
    bw = len(text) * 5.7 + 14
    return f"""
    <rect x="{lx - bw/2}" y="{ly - 10.5}" width="{bw}" height="16" rx="4" fill="#FFFFFF" stroke="{color}" stroke-width="0.9"/>
    <text x="{lx}" y="{ly + 1}" font-size="9.2" font-weight="700" fill="{color}" text-anchor="middle">{text}</text>
    """


def path_arrow(d, color="#1B365D", label="", lx=0, ly=0, dashed=False, marker="arrow-navy"):
    dash = ' stroke-dasharray="5,4"' if dashed else ""
    lbl = pill_label(lx, ly, label, color) if label else ""
    return f'<path d="{d}" fill="none" stroke="{color}" stroke-width="2" stroke-linejoin="round"{dash} marker-end="url(#{marker})"/>{lbl}'


def build_svg() -> str:
    W, H = 1640, 1020

    svg = f"""<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W} {H}" width="{W}" height="{H}" font-family="'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif">
  <defs>
    <filter id="shadow" x="-4%" y="-6%" width="108%" height="116%">
      <feDropShadow dx="0" dy="2" stdDeviation="2.5" flood-color="#0F172A" flood-opacity="0.07"/>
    </filter>
    <linearGradient id="hdr-grad" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0%" stop-color="#0F2544"/>
      <stop offset="55%" stop-color="#1B365D"/>
      <stop offset="100%" stop-color="#0E7490"/>
    </linearGradient>
    <marker id="arrow-navy" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6.5" markerHeight="6.5" orient="auto-start-reverse">
      <path d="M 0 1 L 10 5 L 0 9 z" fill="#1B365D"/>
    </marker>
    <marker id="arrow-teal" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6.5" markerHeight="6.5" orient="auto-start-reverse">
      <path d="M 0 1 L 10 5 L 0 9 z" fill="#0E7490"/>
    </marker>
    <marker id="arrow-amber" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6.5" markerHeight="6.5" orient="auto-start-reverse">
      <path d="M 0 1 L 10 5 L 0 9 z" fill="#D97706"/>
    </marker>
    <marker id="arrow-purple" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6.5" markerHeight="6.5" orient="auto-start-reverse">
      <path d="M 0 1 L 10 5 L 0 9 z" fill="#7C3AED"/>
    </marker>
    <marker id="arrow-green" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6.5" markerHeight="6.5" orient="auto-start-reverse">
      <path d="M 0 1 L 10 5 L 0 9 z" fill="#059669"/>
    </marker>
  </defs>

  <!-- Canvas -->
  <rect width="{W}" height="{H}" fill="#F8FAFC"/>

  <!-- Top Branded Header -->
  <rect x="0" y="0" width="{W}" height="78" fill="url(#hdr-grad)"/>
  <text x="30" y="34" font-size="22" font-weight="800" fill="#FFFFFF" letter-spacing="0.2">Magpie Gateway — AWS Sovereign Solution Architecture</text>
  <text x="30" y="57" font-size="12.5" fill="#CBD5E1">Australian Sovereign AI &amp; MCP Governance • Amazon EKS Auto Mode (ap-southeast-2) • Amazon Bedrock AU Profiles • Microsoft Entra ID &amp; Sentinel</text>
  <rect x="{W - 246}" y="20" width="216" height="36" rx="18" fill="#FFFFFF" fill-opacity="0.14" stroke="#38BDF8" stroke-width="1"/>
  <text x="{W - 138}" y="43" font-size="11.5" font-weight="700" fill="#FFFFFF" text-anchor="middle">ISM • PSPF • Essential Eight</text>

  <!-- ==================== LEFT COLUMN: ENTERPRISE WORKPLACE & IDENTITY ==================== -->
  <rect x="20" y="98" width="286" height="824" rx="12" fill="#FFFFFF" stroke="#CBD5E1" stroke-width="1.4"/>
  <text x="36" y="124" font-size="12.5" font-weight="800" fill="#1B365D" letter-spacing="0.4">ENTERPRISE WORKPLACE &amp; IDENTITY</text>
  <line x1="36" y1="134" x2="290" y2="134" stroke="#E2E8F0" stroke-width="1"/>

  {node_card(34, 150, 258, 114, "client.svg", "Onboarded AI Agents", "Claude Code • OpenAI Codex\nGitHub Copilot • Cursor • MCP\nIntune managed-settings.json", accent="#0E7490", badge="Virtual Keys Only")}

  {node_card(34, 282, 258, 108, "users.svg", "Security &amp; Compliance", "Control Centre • Approvals Inbox\nStaged Cohorts • Kill Switches\nLive Evidence Packs (PDF/CSV)", accent="#1B365D", badge="RBAC + MFA")}

  <!-- Microsoft Entra ID -->
  <g filter="url(#shadow)">
    <rect x="34" y="408" width="258" height="122" rx="8" fill="#EFF6FF" stroke="#93C5FD" stroke-width="1.2"/>
    <rect x="34" y="408" width="4" height="122" rx="2" fill="#2563EB"/>
    <rect x="46" y="446" width="40" height="40" rx="8" fill="#2563EB"/>
    <path d="M66 454 L78 476 L54 476 Z" fill="#FFFFFF"/>
    <path d="M66 462 L72 474 L60 474 Z" fill="#60A5FA"/>
    <text x="96" y="432" font-size="12" font-weight="700" fill="#0F172A">Microsoft Entra ID</text>
    <text x="96" y="449" font-size="10.2" fill="#334155">OIDC Single Sign-On (PKCE)</text>
    <text x="96" y="463" font-size="10.2" fill="#334155">SCIM 2.0 User &amp; Group Sync</text>
    <text x="96" y="477" font-size="10.2" fill="#334155">Instant Leaver Revocation</text>
    <text x="96" y="491" font-size="10.2" fill="#334155">Cohort Group Mapping</text>
    <rect x="206" y="505" width="78" height="17" rx="8.5" fill="#2563EB" fill-opacity="0.12" stroke="#2563EB" stroke-width="0.8"/>
    <text x="245" y="516.8" font-size="9" font-weight="700" fill="#1D4ED8" text-anchor="middle">OIDC + SCIM</text>
  </g>

  <!-- Microsoft Intune / Defender -->
  <g filter="url(#shadow)">
    <rect x="34" y="548" width="258" height="106" rx="8" fill="#F0FDF4" stroke="#86EFAC" stroke-width="1.2"/>
    <rect x="34" y="548" width="4" height="106" rx="2" fill="#15803D"/>
    <rect x="46" y="580" width="40" height="40" rx="8" fill="#15803D"/>
    <rect x="54" y="590" width="24" height="16" rx="2" fill="none" stroke="#FFFFFF" stroke-width="2.1"/>
    <line x1="60" y1="611" x2="72" y2="611" stroke="#FFFFFF" stroke-width="2.1"/>
    <text x="96" y="572" font-size="12" font-weight="700" fill="#0F172A">Intune &amp; Defender Egress</text>
    <text x="96" y="589" font-size="10.2" fill="#334155">Deploys managed agent config</text>
    <text x="96" y="603" font-size="10.2" fill="#334155">Blocks direct Shadow AI APIs</text>
    <text x="96" y="617" font-size="10.2" fill="#334155">Forces all agents via Magpie</text>
    <rect x="210" y="630" width="74" height="17" rx="8.5" fill="#15803D" fill-opacity="0.12" stroke="#15803D" stroke-width="0.8"/>
    <text x="247" y="641.8" font-size="9" font-weight="700" fill="#15803D" text-anchor="middle">Fleet Control</text>
  </g>

  <!-- Sovereign Controls Callout -->
  <rect x="34" y="674" width="258" height="232" rx="10" fill="#F8FAFC" stroke="#CBD5E1" stroke-width="1.1"/>
  <text x="48" y="698" font-size="11.2" font-weight="800" fill="#1B365D">SOVEREIGN CONTROL GUARANTEES</text>
  <line x1="48" y1="707" x2="278" y2="707" stroke="#E2E8F0" stroke-width="1"/>
  <text x="48" y="726" font-size="10.2" fill="#334155">• Zero-trust: agents hold virtual keys only</text>
  <text x="48" y="745" font-size="10.2" fill="#334155">• Guardrails localhost-only inside EKS pod</text>
  <text x="48" y="764" font-size="10.2" fill="#334155">• ALB returns HTTP 404 on /guardrails/*</text>
  <text x="48" y="783" font-size="10.2" fill="#334155">• Fail-closed webhooks (shared token auth)</text>
  <text x="48" y="802" font-size="10.2" fill="#334155">• AU DLP: TFN, ABN, Medicare, BSB, Keys</text>
  <text x="48" y="821" font-size="10.2" fill="#334155">• PSPF markings &amp; AU region enforcement</text>
  <text x="48" y="840" font-size="10.2" fill="#334155">• Staged Observe-Only → Enforcing cohorts</text>
  <text x="48" y="859" font-size="10.2" fill="#334155">• Instant global &amp; per-agent Kill Switches</text>
  <text x="48" y="878" font-size="10.2" fill="#334155">• SHA-256 tamper-evident Evidence Packs</text>

  <!-- ==================== AWS CLOUD OUTER BOUNDARY ==================== -->
  <rect x="356" y="98" width="914" height="824" rx="12" fill="#FFFFFF" stroke="#232F3E" stroke-width="2"/>
  {group_header(362, 102, "aws-cloud.svg", "AWS Cloud — Australian Sovereign Region (ap-southeast-2 Sydney / ap-southeast-4 Melbourne)", "Terraform-managed reference deployment (deploy/terraform/aws) • IRAP-assessed AWS services", "#232F3E")}

  <!-- Edge / Global Ingress Strip -->
  <rect x="376" y="152" width="874" height="96" rx="10" fill="#FFF7ED" stroke="#FDBA74" stroke-width="1.3"/>
  <text x="390" y="170" font-size="10.2" font-weight="800" fill="#C2410C" letter-spacing="0.4">EDGE SECURITY &amp; TLS 1.3 INGRESS</text>
  {node_card(390, 178, 198, 60, "route53.svg", "Amazon Route 53", "Authoritative DNS", accent="#EA580C")}
  {node_card(602, 178, 198, 60, "waf.svg", "AWS WAF", "Edge Rate &amp; OWASP", accent="#DC2626")}
  {node_card(814, 178, 218, 60, "cloudfront.svg", "Amazon CloudFront", "HTTPS + VPC Origin", accent="#7C3AED")}
  {node_card(1046, 178, 190, 60, "acm.svg", "AWS ACM", "Managed TLS Certs", accent="#DC2626")}

  <!-- Amazon VPC Boundary -->
  <rect x="376" y="264" width="874" height="642" rx="10" fill="#F0F9FF" stroke="#0284C7" stroke-width="1.8"/>
  {group_header(382, 268, "vpc.svg", "Amazon VPC (magpie-prod • 2 Availability Zones)", "Strict Security Groups • Zero public pod IPs • Private ingress via CloudFront VPC Origin", "#0369A1")}

  <!-- Public Subnet (top-right inside VPC) -->
  <rect x="986" y="316" width="248" height="106" rx="8" fill="#ECFDF5" stroke="#10B981" stroke-width="1.3" stroke-dasharray="4,3"/>
  {group_header(990, 318, "public-subnet.svg", "Public Subnet", "Controlled Outbound Egress", "#047857")}
  {node_card(998, 362, 224, 50, "nat-gateway.svg", "NAT Gateway", "Egress to OIDC / GHCR / SIEM", accent="#059669")}

  <!-- Private Subnet Container -->
  <rect x="392" y="316" width="578" height="574" rx="8" fill="#FFFFFF" stroke="#0EA5E9" stroke-width="1.5" stroke-dasharray="6,3"/>
  {group_header(398, 318, "private-subnet.svg", "Private Subnets (2 AZs • internal-elb + EKS Auto Mode)", "All compute, audit storage, and internal proxy traffic remain private", "#0369A1")}

  <!-- Internal ALB -->
  {node_card(410, 370, 542, 82, "alb.svg", "Internal Application Load Balancer (ALB)", "/v1/* → Proxy (:4000)   |   /* → Magpie Portal (:8088)\n/guardrails/* &amp; /mcp/check → HTTP 404 Denied at Edge (SG: CloudFront prefix list)", accent="#7C3AED", badge="TargetGroupBinding")}

  <!-- EKS Auto Mode Cluster Box -->
  <rect x="410" y="486" width="542" height="292" rx="10" fill="#F8FAFC" stroke="#EA580C" stroke-width="1.7"/>
  {group_header(416, 490, "eks.svg", "Amazon EKS Auto Mode Cluster (magpie namespace)", "Co-located Pod • Read-only root FS • Non-root UID 65532 • cap_drop ALL", "#C2410C")}

  <!-- Agentgateway Proxy container -->
  <g filter="url(#shadow)">
    <rect x="426" y="540" width="240" height="154" rx="8" fill="#FFFFFF" stroke="#0E7490" stroke-width="1.6"/>
    <rect x="426" y="540" width="240" height="28" rx="7" fill="#0E7490"/>
    <text x="546" y="558" font-size="11.5" font-weight="700" fill="#FFFFFF" text-anchor="middle">Agentgateway Proxy</text>
    <text x="438" y="586" font-size="10.2" font-weight="700" fill="#0F172A">Ports: :4000 LLM • :3000 MCP</text>
    <text x="438" y="602" font-size="10" fill="#475569">• Strict Virtual API Key auth</text>
    <text x="438" y="617" font-size="10" fill="#475569">• Derives actor/cohort via CEL</text>
    <text x="438" y="632" font-size="10" fill="#475569">• MCP tool allow-list (CEL)</text>
    <text x="438" y="647" font-size="10" fill="#475569">• Request &amp; Response webhooks</text>
    <text x="438" y="662" font-size="10" fill="#475569">• SigV4 Bedrock signing (Pod ID)</text>
    <text x="438" y="682" font-size="9.3" font-weight="700" fill="#0E7490">Admin API :15000 (pod-internal)</text>
  </g>

  <!-- Magpie Portal container -->
  <g filter="url(#shadow)">
    <rect x="696" y="540" width="240" height="154" rx="8" fill="#FFFFFF" stroke="#1B365D" stroke-width="1.6"/>
    <rect x="696" y="540" width="240" height="28" rx="7" fill="#1B365D"/>
    <text x="816" y="558" font-size="11.5" font-weight="700" fill="#FFFFFF" text-anchor="middle">Magpie Portal &amp; Policy Engine</text>
    <text x="708" y="586" font-size="10.2" font-weight="700" fill="#0F172A">Port: :8088 (Rust + React UI)</text>
    <text x="708" y="602" font-size="10" fill="#475569">• Control Centre &amp; Kill Switches</text>
    <text x="708" y="617" font-size="10" fill="#475569">• AU DLP (TFN/ABN/Medicare/BSB)</text>
    <text x="708" y="632" font-size="10" fill="#475569">• PSPF &amp; Sovereignty Engine</text>
    <text x="708" y="647" font-size="10" fill="#475569">• Staged Cohort Policy Resolver</text>
    <text x="708" y="662" font-size="10" fill="#475569">• SCIM 2.0 + SIEM Exporter</text>
    <text x="708" y="682" font-size="9.3" font-weight="700" fill="#1B365D">Webhooks: /guardrails/* + /mcp/check</text>
  </g>

  <!-- Pod-local bidirectional webhook arrows -->
  <line x1="666" y1="604" x2="694" y2="604" stroke="#D97706" stroke-width="2.2" marker-end="url(#arrow-amber)"/>
  <line x1="696" y1="624" x2="668" y2="624" stroke="#D97706" stroke-width="2.2" marker-end="url(#arrow-amber)"/>
  <rect x="626" y="700" width="110" height="18" rx="4" fill="#FEF3C7" stroke="#D97706" stroke-width="0.9"/>
  <text x="681" y="712.5" font-size="9" font-weight="700" fill="#92400E" text-anchor="middle">localhost + token</text>

  <!-- Optional Presidio sidecar strip -->
  <rect x="426" y="726" width="510" height="32" rx="6" fill="#F1F5F9" stroke="#94A3B8" stroke-width="1" stroke-dasharray="4,3"/>
  <text x="681" y="746" font-size="10.2" font-weight="600" fill="#334155" text-anchor="middle">Optional Sidecar: Microsoft Presidio PII Analyzer / Anonymizer (:5002 / :5001) for enhanced NER</text>

  <!-- Storage & KMS Row inside Private Subnet -->
  {node_card(410, 788, 264, 92, "ebs.svg", "Amazon EBS (gp3)", "SQLite WAL Audit Ledger, SCIM\nPolicies &amp; Evidence Store (/data)", accent="#15803D", badge="KMS Encrypted")}
  {node_card(688, 788, 264, 92, "kms.svg", "AWS KMS", "Customer-Managed Key (CMK)\nEncrypts EBS volume &amp; secrets", accent="#DC2626", badge="ISM Crypto")}

  <!-- Right Stack inside VPC: IAM Pod Identity, Secrets Manager, PrivateLink, CloudWatch -->
  {node_card(986, 438, 248, 100, "iam.svg", "EKS Pod Identity / IAM", "Role: magpie-proxy\nScoped to inference-profile/au.*\nZero static AWS keys", accent="#DC2626", badge="Least Privilege")}
  {node_card(986, 554, 248, 92, "secrets-manager.svg", "AWS Secrets Manager", "Session Secret • OIDC Secret\nWebhook &amp; SCIM Bearer Tokens", accent="#DC2626", badge="Auto-Rotated")}
  {node_card(986, 662, 248, 98, "privatelink.svg", "AWS PrivateLink", "VPC Interface Endpoints\nBedrock Runtime &amp; AWS STS", accent="#7C3AED", badge="Private Route")}
  {node_card(986, 776, 248, 114, "cloudwatch.svg", "Amazon CloudWatch", "Container Insights &amp; Logs\nProxy Telemetry &amp; Alarms\nSecurity Hub Findings", accent="#DB2777", badge="ISM Logging")}

  <!-- ==================== RIGHT COLUMN: SOVEREIGN AI, TOOLS & SIEM ==================== -->
  <rect x="1312" y="98" width="308" height="824" rx="12" fill="#FFFFFF" stroke="#CBD5E1" stroke-width="1.4"/>
  <text x="1328" y="124" font-size="12.5" font-weight="800" fill="#1B365D" letter-spacing="0.4">SOVEREIGN AI, TOOLS &amp; SIEM</text>
  <line x1="1328" y1="134" x2="1604" y2="134" stroke="#E2E8F0" stroke-width="1"/>

  {node_card(1326, 150, 280, 128, "bedrock.svg", "Amazon Bedrock (AU)", "Sovereign Inference Profiles (au.*)\nSydney (ap-southeast-2) &\nMelbourne (ap-southeast-4)\nClaude / Nova • Zero US Egress", accent="#059669", fill="#ECFDF5", badge="Sovereign AU")}

  <!-- Optional Secondary Azure AI Foundry -->
  <g filter="url(#shadow)">
    <rect x="1326" y="296" width="280" height="106" rx="8" fill="#F8FAFC" stroke="#93C5FD" stroke-width="1.2" stroke-dasharray="5,3"/>
    <rect x="1326" y="296" width="4" height="106" rx="2" fill="#0284C7"/>
    <rect x="1338" y="328" width="40" height="40" rx="8" fill="#0284C7"/>
    <text x="1358" y="353" font-size="14.5" font-weight="800" fill="#FFFFFF" text-anchor="middle">Az</text>
    <text x="1388" y="321" font-size="12" font-weight="700" fill="#0F172A">Azure AI Foundry (Optional)</text>
    <text x="1388" y="338" font-size="10" fill="#475569">australiaeast (DataZone / Standard)</text>
    <text x="1388" y="353" font-size="10" fill="#475569">Multi-cloud AU sovereign failover</text>
    <text x="1388" y="368" font-size="10" fill="#475569">Same DLP, PSPF &amp; audit controls</text>
    <rect x="1520" y="378" width="78" height="17" rx="8.5" fill="#0284C7" fill-opacity="0.12" stroke="#0284C7" stroke-width="0.8"/>
    <text x="1559" y="389.8" font-size="9" font-weight="700" fill="#0369A1" text-anchor="middle">Multi-Cloud</text>
  </g>

  <!-- Approved Internal MCP Tool Servers -->
  <g filter="url(#shadow)">
    <rect x="1326" y="420" width="280" height="122" rx="8" fill="#FFFFFF" stroke="#CBD5E1" stroke-width="1.2"/>
    <rect x="1326" y="420" width="4" height="122" rx="2" fill="#0E7490"/>
    <rect x="1338" y="460" width="40" height="40" rx="8" fill="#0E7490"/>
    <text x="1358" y="485" font-size="12" font-weight="800" fill="#FFFFFF" text-anchor="middle">MCP</text>
    <text x="1388" y="445" font-size="12" font-weight="700" fill="#0F172A">Approved MCP Tool Servers</text>
    <text x="1388" y="462" font-size="10" fill="#475569">SharePoint • Jira • ServiceNow • Git</text>
    <text x="1388" y="477" font-size="10" fill="#475569">Allow-listed per tool via CEL policy</text>
    <text x="1388" y="492" font-size="10" fill="#475569">Unapproved tools denied at proxy</text>
    <text x="1388" y="507" font-size="10" fill="#475569">Argument DLP &amp; injection scanning</text>
    <rect x="1518" y="518" width="80" height="17" rx="8.5" fill="#0E7490" fill-opacity="0.12" stroke="#0E7490" stroke-width="0.8"/>
    <text x="1558" y="529.8" font-size="9" font-weight="700" fill="#0E7490" text-anchor="middle">Allow-Listed</text>
  </g>

  <!-- AWS Security Hub -->
  {node_card(1326, 560, 280, 96, "security-hub.svg", "AWS Security Hub", "Centralised AWS posture &\nCloudWatch security findings", accent="#DC2626", badge="CSPM")}

  <!-- Microsoft Sentinel / SIEM & Teams -->
  <g filter="url(#shadow)">
    <rect x="1326" y="674" width="280" height="232" rx="8" fill="#FAF5FF" stroke="#C4B5FD" stroke-width="1.3"/>
    <rect x="1326" y="674" width="4" height="232" rx="2" fill="#7C3AED"/>
    <rect x="1338" y="706" width="40" height="40" rx="8" fill="#7C3AED"/>
    <text x="1358" y="731" font-size="12.5" font-weight="800" fill="#FFFFFF" text-anchor="middle">SIEM</text>
    <text x="1388" y="702" font-size="12" font-weight="700" fill="#0F172A">Microsoft Sentinel &amp; Teams</text>
    <text x="1388" y="720" font-size="10" fill="#475569">• Live audit sink exporter (HTTPS)</text>
    <text x="1388" y="735" font-size="10" fill="#475569">• OTLP / Splunk HEC / Log Analytics</text>
    <text x="1388" y="750" font-size="10" fill="#475569">• 6 built-in Sentinel KQL detections</text>
    <text x="1388" y="765" font-size="10" fill="#475569">• Teams Adaptive Cards v1.4 alerts</text>
    <line x1="1340" y1="784" x2="1592" y2="784" stroke="#DDD6FE" stroke-width="1"/>
    <text x="1340" y="806" font-size="11" font-weight="700" fill="#5B21B6">Tamper-Evident Evidence Packs</text>
    <text x="1340" y="824" font-size="10" fill="#475569">Monthly CSV, printable PDF &amp; SHA-256</text>
    <text x="1340" y="839" font-size="10" fill="#475569">JSON bundles for ISM / PSPF / IRAP</text>
    <text x="1340" y="854" font-size="10" fill="#475569">auditor verification &amp; sign-off.</text>
    <rect x="1508" y="880" width="90" height="17" rx="8.5" fill="#7C3AED" fill-opacity="0.12" stroke="#7C3AED" stroke-width="0.8"/>
    <text x="1553" y="891.8" font-size="9" font-weight="700" fill="#6D28D9" text-anchor="middle">Live Streaming</text>
  </g>

  <!-- ==================== CLEAN ORTHOGONAL CONNECTORS ==================== -->
  <!-- 1. Agents -> Edge (HTTPS /v1/*) -->
  {path_arrow("M 292 200 L 388 200", color="#0E7490", label="HTTPS /v1/*", lx=338, ly=190, marker="arrow-teal")}
  <!-- 2. Admins -> Edge (Portal UI) -->
  {path_arrow("M 292 332 L 330 332 L 330 220 L 388 220", color="#1B365D", label="Portal UI", lx=330, ly=282, marker="arrow-navy")}
  <!-- 3. Entra ID -> ALB/Portal (OIDC / SCIM 2.0) -->
  {path_arrow("M 292 465 L 334 465 L 334 412 L 408 412", color="#2563EB", label="OIDC / SCIM", lx=334, ly=442, dashed=True, marker="arrow-navy")}

  <!-- 4. CloudFront -> Internal ALB via VPC Origin (clean right-hand jog into ALB right edge!) -->
  {path_arrow("M 923 238 L 923 256 L 976 256 L 976 410 L 954 410", color="#7C3AED", label="VPC Origin", lx=976, ly=302, marker="arrow-purple")}

  <!-- 5. Internal ALB -> EKS Auto Mode (clean gutter between y=452 and y=486, NO text overlap!) -->
  {path_arrow("M 546 452 L 546 484", color="#0E7490", label="/v1/* + /mcp", lx=492, ly=470, marker="arrow-teal")}
  {path_arrow("M 816 452 L 816 484", color="#1B365D", label="Portal + SCIM", lx=872, ly=470, marker="arrow-navy")}

  <!-- 6. EKS Pod -> EBS & EBS -> KMS -->
  {path_arrow("M 546 778 L 546 794", color="#15803D", marker="arrow-green")}
  {path_arrow("M 674 836 L 686 836", color="#DC2626", marker="arrow-amber")}

  <!-- 7. EKS Pod -> IAM Pod Identity & Secrets Manager & PrivateLink -->
  {path_arrow("M 952 586 L 984 586", color="#DC2626", dashed=True, marker="arrow-amber")}
  {path_arrow("M 952 710 L 984 710", color="#7C3AED", marker="arrow-purple")}

  <!-- 8. PrivateLink -> Amazon Bedrock AU (clean vertical highway in x=1286 gutter) -->
  {path_arrow("M 1234 710 L 1286 710 L 1286 214 L 1324 214", color="#059669", label="SigV4 PrivateLink", lx=1286, ly=370, marker="arrow-green")}

  <!-- 9. EKS Proxy -> Approved MCP Tool Servers (cleanly routed out via x=964 -> y=474 -> x=1324!) -->
  {path_arrow("M 952 518 L 974 518 L 974 428 L 1264 428 L 1264 480 L 1324 480", color="#0E7490", label="MCP Allow-list", lx=1264, ly=456, marker="arrow-teal")}

  <!-- 10. CloudWatch -> AWS Security Hub (orthogonal) -->
  {path_arrow("M 1234 812 L 1298 812 L 1298 608 L 1324 608", color="#DB2777", dashed=True, marker="arrow-purple")}

  <!-- 11. Magpie Portal SIEM Exporter -> Microsoft Sentinel & Teams (bottom VPC corridor) -->
  {path_arrow("M 936 664 L 964 664 L 964 898 L 1304 898 L 1304 770 L 1324 770", color="#7C3AED", label="Signed SIEM / Teams", lx=1136, ly=898, marker="arrow-purple")}

  <!-- ==================== FOOTER LEGEND ==================== -->
  <rect x="20" y="942" width="1600" height="58" rx="10" fill="#FFFFFF" stroke="#CBD5E1" stroke-width="1.2"/>
  <text x="38" y="967" font-size="11" font-weight="800" fill="#1B365D">ARCHITECTURE FLOW LEGEND</text>
  <line x1="224" y1="963" x2="259" y2="963" stroke="#0E7490" stroke-width="2.5" marker-end="url(#arrow-teal)"/>
  <text x="268" y="967" font-size="10.5" fill="#334155">Agent AI &amp; MCP Traffic (Enforced)</text>
  <line x1="472" y1="963" x2="507" y2="963" stroke="#D97706" stroke-width="2.5" marker-end="url(#arrow-amber)"/>
  <text x="516" y="967" font-size="10.5" fill="#334155">Pod-Local Guardrail Webhooks (Fail-Closed)</text>
  <line x1="766" y1="963" x2="801" y2="963" stroke="#059669" stroke-width="2.5" marker-end="url(#arrow-green)"/>
  <text x="810" y="967" font-size="10.5" fill="#334155">Sovereign AU Bedrock (PrivateLink)</text>
  <line x1="1032" y1="963" x2="1067" y2="963" stroke="#7C3AED" stroke-width="2.5" marker-end="url(#arrow-purple)"/>
  <text x="1076" y="967" font-size="10.5" fill="#334155">VPC Origin &amp; Signed SIEM Stream</text>
  <line x1="1286" y1="963" x2="1321" y2="963" stroke="#1B365D" stroke-width="2.5" stroke-dasharray="5,4" marker-end="url(#arrow-navy)"/>
  <text x="1330" y="967" font-size="10.5" fill="#334155">Identity, Secrets &amp; Control Plane</text>
  <text x="38" y="987" font-size="9.8" fill="#64748B">Icons: Official AWS Architecture Icons • Diagram reflects Magpie Gateway reference Terraform &amp; Helm architecture (deploy/terraform/aws &amp; deploy/helm/magpie-gateway).</text>
</svg>
"""
    return svg


def main():
    OUT_SVG.parent.mkdir(parents=True, exist_ok=True)
    OUT_SVG.write_text(build_svg(), encoding="utf-8")
    print(f"Wrote {OUT_SVG} ({OUT_SVG.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
