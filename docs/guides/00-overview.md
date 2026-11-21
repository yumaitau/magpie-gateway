# About these walkthroughs

Magpie Gateway is an Australian AI governance and connectivity platform built on Agentgateway. It gives enterprise Microsoft organisations and Australian Government agencies visibility, policy enforcement, Australian Privacy Principles (APP) and Protective Security Policy Framework (PSPF) controls, staged group rollouts, and auditable assurance over AI model and MCP tool traffic.

These walkthrough guides take each role through day-to-day tasks end to end, using exact screen labels, button names, screenshots from the live console, and explicit verification checks.

## Role-to-walkthrough map

Read **Chapter 2 (Getting started)** first regardless of your role, then follow the chapters for your responsibilities.

| Portal role | Typical people | Primary walkthroughs |
|---|---|---|
| **Owner** | Accountable platform owner, principal architect | All chapters, plus the *Deployment Guide* and *Administrator SOPs* |
| **Admin** | Cloud / AI platform administrators, Microsoft 365 & Entra admins | Chapters 2, 3, 6, 7, 8 |
| **Policy Editor** | Information security managers, privacy officers, DLP specialists | Chapters 2, 3, 4, 5 |
| **Approver** | Authorising officers, ITSAs, business owners, change managers | Chapters 2, 4, 9 |
| **Auditor** | Internal audit, IRAP assessors, SOC analysts, risk managers | Chapters 2, 8, 9 |
| **Viewer** | Technical leads, service owners (read-only) | Chapters 2, 9 |
| **Developer / Engineer** | Software engineers using Claude Code, Codex CLI, VS Code, or GitHub Copilot | Chapter 2, Chapter 7 |

## How Magpie separates duties and prevents accidents

Magpie Gateway is designed so no single click silently exposes sensitive data or bypasses governance:

1. **Trial before enforcement.** New deployments start in **Log-only trial** (`log-only`). Magpie evaluates every Australian DLP detector, PSPF classification check, sovereign residency rule, and model/tool approval and records what it *would* do in **Decision audit**, without interrupting developers.
2. **Staged cohort rollout.** Administrators can switch a single Microsoft Entra ID group (for example `SG-AI-Pilot-Engineers`) to **Enforcing** while keeping the rest of the organisation in **Trial** — or vice versa — with immediate rollback on the next request.
3. **Second-person approval.** When **Second-person approval** is enabled (default on), saving changes to governance settings, posture presets, DLP detectors, or model/tool approvals stages a pending revision. The person who submitted the change cannot approve it; a different authorised **Approver** must review the diff and activate it.
4. **Immediate emergency stops.** Global and per-agent **Kill switches** are deliberately exempt from the approval queue so incident responders can halt enforced AI traffic immediately, with every activation recorded in the immutable audit log.
5. **Immutable decision audit.** SQLite triggers prevent updates or deletions of `audit_events` through the application. Raw prompt text is never stored in audit records or sent to alert channels.

## Companion documents

| Document | Audience | What it covers |
|---|---|---|
| **Magpie Gateway Walkthrough Guides** (this volume) | All portal users and operators | Step-by-step UI journeys with live screenshots |
| **Magpie Gateway Deployment Guide** | Azure & AWS cloud engineers | Production Azure Container Apps, Key Vault, Managed Identity, Entra ID, Sentinel DCE/DCR, and AWS EKS deployment |
| **Magpie Gateway Administrator SOPs** | Platform & security operations | Numbered day-2 and incident runbooks with verification checks |
| **Magpie Gateway Security & Assurance** | CISOs, ITSAs, IRAP assessors | Architecture, trust boundaries, threat model, and honest ISM / Essential Eight / PSPF / APP / DTA AI Policy 2.0 mappings |
