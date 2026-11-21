#!/usr/bin/env python3
"""Builds Magpie Gateway branded User & Operator Walkthrough Guides as PDF and editable Word (.docx).

Sources: numbered Markdown files in docs/guides/ (00-overview.md .. 08-*.md).
Outputs in docs/published/:
  Magpie-Gateway-Walkthrough-Guides.pdf / .docx   (combined complete handbook)
  Magpie-Gateway-Walkthrough-NN-<Title>.pdf / .docx (one standalone deliverable per walkthrough)
"""
import os
import re
import shutil
import subprocess
import sys
import tempfile

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BUILD = os.path.join(REPO, "tools", "docs", "build-doc.sh")
GUIDES = os.path.join(REPO, "docs", "guides")
VERSION = "2.0"
DATE = "21-11-2026"


def sources():
    files = sorted(f for f in os.listdir(GUIDES) if re.match(r"^\d\d-.*\.md$", f))
    out = []
    for f in files:
        with open(os.path.join(GUIDES, f), encoding="utf-8") as fh:
            text = fh.read()
        title = text.splitlines()[0].lstrip("# ").strip()
        out.append((f, title, text))
    return out


def file_name(number, title):
    words = re.sub(r"[^A-Za-z0-9 ]+", " ", title).split()
    return f"Magpie-Gateway-Walkthrough-{number}-" + "-".join(w[:1].upper() + w[1:] for w in words[:6])


def metadata(title, running, note, chapters):
    rows = "\n".join(f"  | {n} | {t} |" for n, t in chapters)
    return f'''title: "{title}"
running-title: "{running}"
kicker: "User & Operator Walkthrough"
subtitle: "Sovereign AI Governance, Australian Ready"
note: "{note}"
version: "{VERSION}"
date: "{DATE}"
status: "Published"
lang: en-AU
toc-title: Contents
control: |
  ## Document control

  | Item | Value |
  |---|---|
  | Document | {title} |
  | Version | {VERSION} |
  | Date | {DATE} |
  | Status | Published |
  | Audience | Enterprise Microsoft administrators, Australian Government security & governance teams, approvers, auditors, and developers |
  | Source | `docs/guides/` in `magpie-gateway`; rebuilt with `python3 tools/docs/build-guides.py` |
  | Related | Deployment Guide; Administrator SOPs; Security & Assurance Guide |

  ## Contents of this edition

  | Chapter | Walkthrough |
  |---|---|
{rows}

  UI labels, API endpoints, PowerShell commands, and policy controls match the Magpie Gateway source at this version. Example users, domains, and identifiers are synthetic. Framework mappings clearly state whether a link is direct or interpretive; Magpie Gateway does not claim agency certification on its own.
'''


def build(name, title, running, note, files):
    work = tempfile.mkdtemp(prefix="magpie-guides-")
    try:
        chapters = []
        for i, (src, chapter_title, text) in enumerate(files, start=1):
            with open(os.path.join(work, f"{i:02d}-{src[3:]}"), "w", encoding="utf-8") as fh:
                fh.write(text if text.endswith("\n") else text + "\n")
            chapters.append((str(i), chapter_title))
        with open(os.path.join(work, "metadata.yaml"), "w", encoding="utf-8") as fh:
            fh.write(metadata(title, running, note, chapters))
        result = subprocess.run(["bash", BUILD, work, name], cwd=REPO, capture_output=True, text=True)
        if result.returncode != 0:
            sys.stderr.write(result.stderr + "\n")
            sys.exit(result.returncode)
        print(result.stdout.strip())
    finally:
        shutil.rmtree(work, ignore_errors=True)


def main():
    combined_only = "--combined-only" in sys.argv[1:]
    files = sources()
    build(
        "Magpie-Gateway-Walkthrough-Guides",
        "Magpie Gateway: Enterprise & Government Walkthrough Guides",
        "Magpie Gateway Walkthrough Guides",
        "End-to-end illustrated operator guides for Control Centre, Microsoft Entra ID, Intune, Microsoft Sentinel, and Australian Government assurance",
        files,
    )
    if combined_only:
        return
    for src, title, text in files:
        number = src[:2]
        build(
            file_name(number, title),
            f"Magpie Gateway Walkthrough: {title}",
            f"Magpie Gateway: {title}",
            "Step-by-step operator walkthrough",
            [(src, title, text)],
        )


if __name__ == "__main__":
    main()
