#!/usr/bin/env python3
"""Build Josh's branded Client Demo Playbook (PDF and editable Word .docx) into docs/published/."""

import os
import subprocess
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BUILD = os.path.join(REPO, "tools", "docs", "build-doc.sh")
SRC = os.path.join(REPO, "docs", "demo")
OUT_NAME = "Magpie-Gateway-Client-Demo-Guide"


def main():
    result = subprocess.run(["bash", BUILD, SRC, OUT_NAME], cwd=REPO, capture_output=True, text=True)
    if result.returncode != 0:
        sys.stderr.write(result.stdout + "\n" + result.stderr + "\n")
        sys.exit(result.returncode)
    print(result.stdout.strip())


if __name__ == "__main__":
    main()
