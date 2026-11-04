#!/usr/bin/env python3
"""Rewrites the crate list in THIRD_PARTY.md from cargo deny and cargo metadata (make third-party)."""
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BEGIN, END = "<!-- crates:begin -->", "<!-- crates:end -->"


def run(args, cwd):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True).stdout


def normalise(expr):
    expr = re.sub(r"\s+", " ", expr.replace("/", " OR ")).strip()
    if "(" not in expr and " AND " not in expr:
        parts = sorted(set(expr.split(" OR ")), key=lambda p: (p != "MIT", p != "Apache-2.0", p))
        return " OR ".join(parts)
    return expr


def crates(workspace):
    used = set()
    for _, names in json.loads(run(["cargo", "deny", "--locked", "list", "-f", "json", "-l", "license"], workspace))["licenses"]:
        for c in names:
            name, version = c.split(" ")[:2]
            used.add((name, version))
    meta = json.loads(run(["cargo", "metadata", "--format-version", "1", "--locked"], workspace))
    own = {p["name"] for p in meta["packages"] if p["source"] is None}
    exprs = {(p["name"], p["version"]): p.get("license") or "see its licence file" for p in meta["packages"]}
    return {k: normalise(exprs[k]) for k in used if k[0] not in own}


def main():
    found = {**crates(ROOT), **crates(ROOT / "desktop")}
    groups = {}
    for (name, version), expr in sorted(found.items()):
        groups.setdefault(expr, []).append(f"{name} {version}")
    lines = [BEGIN, "", f"{len(found)} crates, by the licence each declares:", ""]
    for expr in sorted(groups, key=lambda e: (-len(groups[e]), e)):
        lines.append(f"- **{expr}** ({len(groups[expr])}): " + ", ".join(groups[expr]))
    lines += ["", END]
    path = ROOT / "THIRD_PARTY.md"
    text = path.read_text()
    start, end = text.index(BEGIN), text.index(END) + len(END)
    path.write_text(text[:start] + "\n".join(lines) + text[end:])
    print(f"THIRD_PARTY.md: {len(found)} crates", file=sys.stderr)


if __name__ == "__main__":
    main()
