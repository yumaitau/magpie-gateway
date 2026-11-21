#!/usr/bin/env python3
"""Builds Magpie Gateway Administrator Documents (Deployment Guide, Admin SOPs, Security & Assurance)
as branded PDF and editable Word (.docx) deliverables in docs/published/.
"""
import os
import re
import shutil
import subprocess
import sys
import tempfile

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BUILD = os.path.join(REPO, "tools", "docs", "build-doc.sh")

DOCS = [
    (
        "deployment-guide.md",
        "Magpie-Gateway-Deployment-Guide",
        "Deployment Guide",
        "Deploy, configure Entra ID, wire Microsoft Sentinel, verify, upgrade, and remove Magpie Gateway on Microsoft Azure (Australia) and AWS EKS",
    ),
    (
        "admin-sops.md",
        "Magpie-Gateway-Admin-SOPs",
        "Administrator SOPs",
        "Standard operating procedures for day-2 governance, incident kill switches, token rotation, SCIM offboarding, SIEM, and backup/restore",
    ),
    (
        "security-and-assurance.md",
        "Magpie-Gateway-Security-Assurance",
        "Security & Assurance",
        "Architecture, trust boundaries, threat model, and honest ISM September 2026, Essential Eight, PSPF, APP, and DTA Responsible AI 2.0 mapping",
    ),
]


def control_table(lines):
    rows, inside = [], False
    for line in lines:
        if line.startswith("## Document control"):
            inside = True
            continue
        if inside and line.startswith("## "):
            break
        if inside and line.startswith("|") and not line.startswith("|---") and "Item" not in line:
            cells = [c.strip() for c in line.strip().strip("|").split("|")]
            if len(cells) >= 2:
                rows.append((cells[0], cells[1]))
    return rows


def convert(text):
    lines = text.splitlines()
    title = lines[0].lstrip("# ").strip()
    rows = control_table(lines)
    body, skip = [], False
    for line in lines[1:]:
        if line.startswith("## Document control") or line.startswith("## Contents"):
            skip = True
            continue
        if skip and line.startswith("## "):
            skip = False
        if skip:
            continue
        body.append(line)

    out, fenced = [], False
    for line in body:
        if line.lstrip().startswith("```"):
            fenced = not fenced
        if not fenced:
            m = re.match(r"^(#{2,6})\s+(?:\d+(?:\.\d+)*\.?\s+)?(.*)$", line)
            if m:
                line = "#" * (len(m.group(1)) - 1) + " " + m.group(2)
            line = re.sub(r"\[([^\]]+)\]\((?!https?://)(?!#)[^)]*\)", r"\1", line)
            line = re.sub(r"\[([^\]]+)\]\(#[^)]*\)", r"\1", line)
        out.append(line)
    first = next((i for i, l in enumerate(out) if l.startswith("# ")), len(out))
    intro = "\n".join(out[:first]).strip()
    rest = "\n".join(out[first:])
    md = (f"# About this document {{.unnumbered}}\n\n{intro}\n\n" if intro else "") + rest + "\n"
    return title, rows, md


def metadata(title, kicker, note, rows):
    version = next((v for k, v in rows if k == "Version"), "2.0")
    date = next((v for k, v in rows if k == "Date"), "21-11-2026")
    table = "\n".join(f"  | {k} | {v} |" for k, v in rows)
    return f'''title: "{title}"
running-title: "{title}"
kicker: "{kicker}"
subtitle: "Sovereign AI Governance, Australian Ready"
note: "{note}"
version: "{version}"
date: "{date}"
status: "Published"
lang: en-AU
toc-title: Contents
control: |
  ## Document control

  | Item | Value |
  |---|---|
{table}
  | Source | `docs/admin/` in `magpie-gateway`; rebuilt with `python3 tools/docs/build-admin-docs.py` |

  Commands, settings, and API responses reflect Magpie Gateway at this version. Nothing here implies agency or assessor endorsement.
'''


def main():
    for source, name, kicker, note in DOCS:
        with open(os.path.join(REPO, "docs", "admin", source), encoding="utf-8") as f:
            title, rows, md = convert(f.read())
        work = tempfile.mkdtemp(prefix="magpie-admin-")
        try:
            with open(os.path.join(work, "01-body.md"), "w", encoding="utf-8") as f:
                f.write(md)
            with open(os.path.join(work, "metadata.yaml"), "w", encoding="utf-8") as f:
                f.write(metadata(title, kicker, note, rows))
            result = subprocess.run(["bash", BUILD, work, name], cwd=REPO, capture_output=True, text=True)
            if result.returncode != 0:
                sys.stderr.write(result.stderr + "\n")
                sys.exit(result.returncode)
            print(result.stdout.strip())
        finally:
            shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
