#!/usr/bin/env python3
"""Build AVR firmware with PlatformIO and enforce repository memory budgets."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
RAM_RE = re.compile(
    r"RAM:\s+\[[^\]]*\]\s+[0-9.]+%\s+\(used\s+(\d+)\s+bytes\s+from\s+(\d+)\s+bytes\)"
)
FLASH_RE = re.compile(
    r"Flash:\s+\[[^\]]*\]\s+[0-9.]+%\s+\(used\s+(\d+)\s+bytes\s+from\s+(\d+)\s+bytes\)"
)


def parse_size(output: str) -> tuple[int, int, int, int]:
    clean = ANSI_RE.sub("", output)
    ram_match = RAM_RE.search(clean)
    flash_match = FLASH_RE.search(clean)
    if ram_match is None or flash_match is None:
        raise ValueError("PlatformIO memory usage lines were not found")

    ram_used, ram_total = (int(value) for value in ram_match.groups())
    flash_used, flash_total = (int(value) for value in flash_match.groups())
    return ram_used, ram_total, flash_used, flash_total


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-dir", type=Path, required=True)
    parser.add_argument("--environment", required=True)
    parser.add_argument("--max-static-ram", type=int, required=True)
    parser.add_argument("--max-flash", type=int, required=True)
    args = parser.parse_args()

    completed = subprocess.run(
        ["pio", "run", "-e", args.environment],
        cwd=args.project_dir,
        capture_output=True,
        text=True,
        check=False,
    )
    output = completed.stdout + completed.stderr
    sys.stdout.write(output)

    if completed.returncode != 0:
        return completed.returncode

    try:
        ram_used, ram_total, flash_used, flash_total = parse_size(output)
    except ValueError as exc:
        print(f"memory-budget: ERROR: {exc}", file=sys.stderr)
        return 2

    print(
        "memory-budget: "
        f"static_ram={ram_used}/{ram_total} bytes "
        f"flash={flash_used}/{flash_total} bytes"
    )

    failed = False
    if ram_used > args.max_static_ram:
        print(
            f"memory-budget: FAIL static RAM {ram_used} > {args.max_static_ram}",
            file=sys.stderr,
        )
        failed = True

    if flash_used > args.max_flash:
        print(
            f"memory-budget: FAIL flash {flash_used} > {args.max_flash}",
            file=sys.stderr,
        )
        failed = True

    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
