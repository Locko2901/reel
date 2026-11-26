#!/usr/bin/env python3
"""The licence page from cargo-about's JSON of both workspaces (ci/licence-page.sh): every licence
text once, with the crates under it, and THIRD_PARTY.md's crate list from the same crates."""
import argparse
import html
import json
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BEGIN, END = "<!-- crates:begin -->", "<!-- crates:end -->"

# Licences whose text names its holder: a crate's own copy of it, never the SPDX text alone.
HOLDER = {"MIT", "MIT-0", "BSD-1-Clause", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "0BSD"}
SPDX_HOLDER_LINE = "Copyright (c) <year> <copyright holders>"
# Crates whose licence file cargo-about cannot reach, with the holder line of that file in their
# repository at the commit the crate was published from. governor names its repository with .git,
# which cargo-about's fetch does not take.
HOLDERS = {
    "governor": "Copyright (c) 2023 Andreas Fuchs",
}

MARK = (
    '<svg class="mark" viewBox="8.6 3 16.2 26" aria-hidden="true"><path d="M8.6 3L16 10.4V29H8.6Z"/>'
    '<path class="arm" d="M8.6 3H24.8V10.4H16Z"/></svg>'
)

STYLE = """
@font-face { font-family: "Geist"; font-weight: 400; font-display: swap; src: url(/assets/fonts/geist-latin-400-normal.woff2) format("woff2"); }
@font-face { font-family: "Geist"; font-weight: 500; font-display: swap; src: url(/assets/fonts/geist-latin-500-normal.woff2) format("woff2"); }
@font-face { font-family: "Geist"; font-weight: 600; font-display: swap; src: url(/assets/fonts/geist-latin-600-normal.woff2) format("woff2"); }
@font-face { font-family: "Geist Mono"; font-weight: 400; font-display: swap; src: url(/assets/fonts/geist-mono-latin-400-normal.woff2) format("woff2"); }
:root {
  --bg: #0d0b10; --panel: #15121a; --raised: #1c1822; --line: #2a2533; --line-2: #3b3446;
  --text: #ededf0; --text-2: #a7a2ae; --text-3: #908a98; --accent: #f282d8;
  --sans: "Geist", system-ui, sans-serif; --mono: "Geist Mono", ui-monospace, "SF Mono", Menlo, monospace;
  color-scheme: dark;
}
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; }
body { background: var(--bg); color: var(--text); font-family: var(--sans); font-size: 14px; line-height: 1.45; font-variant-numeric: tabular-nums; -webkit-font-smoothing: antialiased; min-width: 320px; }
::selection { background: rgba(242, 130, 216, 0.3); }
a { color: inherit; text-decoration: none; }
a:hover { color: var(--accent); text-decoration: underline; text-decoration-color: var(--accent); }
a:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; border-radius: 2px; }
.top { display: flex; align-items: center; height: 48px; padding: 0 20px; border-bottom: 1px solid var(--line); }
.wordmark { font-weight: 600; letter-spacing: -0.02em; font-size: 16px; display: inline-flex; align-items: baseline; }
.mark { flex: none; width: 0.47em; height: 0.75em; margin-right: 0.37em; position: relative; top: 0.074em; fill: var(--text); }
.mark .arm { fill: var(--accent); }
main { max-width: 974px; margin: 0 auto; padding: 24px 20px 8px; }
h1 { font-size: 22px; font-weight: 500; letter-spacing: -0.01em; margin: 0; line-height: 1.3; }
h2 { font-size: 14px; font-weight: 500; margin: 0; line-height: 1.3; }
p { margin: 0; }
.intro { color: var(--text-2); max-width: 70ch; margin: 8px 0 16px; }
.intro + .intro { margin-top: -8px; }
.panel { background: var(--panel); border: 1px solid var(--line); border-radius: 4px; }
.panel + .panel, .panel + section, section + section { margin-top: 16px; }
.panel > header { display: flex; flex-wrap: wrap; align-items: baseline; gap: 2px 12px; padding: 8px 12px; border-bottom: 1px solid var(--line); min-height: 36px; }
.panel > header .desc { color: var(--text-2); font-size: 12px; }
section > h2 { font-size: 16px; margin: 24px 0 8px; scroll-margin-top: 16px; }
section > h2 .desc { color: var(--text-2); font-size: 12px; font-weight: 400; margin-left: 8px; }
.toc { list-style: none; margin: 0; padding: 4px 0; }
.toc li { display: flex; gap: 12px; padding: 6px 12px; }
.toc li a { flex: 1 1 auto; min-width: 0; }
.toc li span { color: var(--text-2); font-size: 12px; white-space: nowrap; }
.crates { margin: 0; padding: 10px 12px; border-bottom: 1px solid var(--line); line-height: 1.7; }
.crates a, .crates span { white-space: nowrap; }
pre { margin: 0; padding: 12px; font-family: var(--mono); font-size: 12px; line-height: 1.5; color: var(--text-2); white-space: pre-wrap; overflow-wrap: anywhere; }
.foot { max-width: 974px; margin: 0 auto; padding: 24px 20px 28px; color: var(--text-3); font-size: 12px; display: flex; gap: 8px; align-items: baseline; flex-wrap: wrap; }
.foot .mark { fill: var(--text-2); }
@media (max-width: 600px) {
  main { padding: 16px 16px 8px; }
  .top { padding: 0 16px; }
  .foot { padding: 20px 16px 24px; }
}
"""


def norm(text):
    lines = [line.rstrip() for line in text.replace("\r\n", "\n").replace("\r", "\n").split("\n")]
    return "\n".join(lines).strip("\n")


def anchor(spdx):
    return re.sub(r"[^a-z0-9]+", "-", spdx.lower()).strip("-")


def normalise(expr):
    expr = re.sub(r"\s+", " ", expr.replace("/", " OR ")).strip()
    if "(" not in expr and " AND " not in expr:
        parts = sorted(set(expr.split(" OR ")), key=lambda p: (p != "MIT", p != "Apache-2.0", p))
        return " OR ".join(parts)
    return expr


def lock_crates():
    found = set()
    for lock in (ROOT / "Cargo.lock", ROOT / "desktop" / "Cargo.lock"):
        for p in tomllib.loads(lock.read_text())["package"]:
            if "source" in p:
                found.add((p["name"], p["version"]))
    return found


def gather(reports):
    """Every text by licence and its words alone, the crates under each, and each crate's own
    licence expression; refuses a licence that names its holder in the SPDX text alone."""
    texts, declared, wanting = {}, {}, {}

    def add(spdx, name, text, crates):
        text = norm(text)
        entry = texts.setdefault((spdx, " ".join(text.split())), {"id": spdx, "name": name, "text": text, "crates": set()})
        entry["crates"].update(crates)

    for report in reports:
        data = json.loads(Path(report).read_text())
        for c in data["crates"]:
            p = c["package"]
            if p["source"] is not None:
                declared[(p["name"], p["version"])] = p.get("license") or "see its licence file"
        for lic in data["licenses"]:
            crates = {(u["crate"]["name"], u["crate"]["version"]) for u in lic["used_by"] if u["crate"]["source"] is not None}
            if lic["source_path"] or lic["id"] not in HOLDER:
                add(lic["id"], lic["name"], lic["text"], crates)
                continue
            for name, version in sorted(crates):
                holder = HOLDERS.get(name)
                if holder and SPDX_HOLDER_LINE in lic["text"]:
                    add(lic["id"], lic["name"], lic["text"].replace(SPDX_HOLDER_LINE, holder), {(name, version)})
                else:
                    wanting[(name, version)] = lic["id"]
    on_page = set().union(*(t["crates"] for t in texts.values())) if texts else set()
    for crate in set(declared) - on_page:
        wanting.setdefault(crate, "no licence text at all")
    return texts, declared, on_page, [f"{n} {v}: {why}" for (n, v), why in sorted(wanting.items())]


def crate_link(name, version):
    n, v = html.escape(name), html.escape(version)
    return f'<a href="https://crates.io/crates/{n}/{v}">{n} {v}</a>'


def page(texts, on_page, elsewhere, version, tool):
    by_licence = {}
    for t in texts.values():
        by_licence.setdefault(t["id"], []).append(t)
    order = sorted(by_licence, key=lambda i: (-len(set().union(*(t["crates"] for t in by_licence[i]))), i))
    reel_licence = norm((ROOT / "LICENSE").read_text())
    out = [
        "<!doctype html>",
        '<html lang="en">',
        "<head>",
        '<meta charset="utf-8">',
        '<meta name="viewport" content="width=device-width, initial-scale=1">',
        '<meta name="robots" content="noindex">',
        f'<meta name="reel-crates" content="{len(on_page)}">',
        f"<title>Licences · reel {html.escape(version)}</title>",
        '<link rel="icon" href="/assets/icon.svg" type="image/svg+xml">',
        f"<style>{STYLE}</style>",
        "</head>",
        "<body>",
        f'<header class="top"><span class="wordmark">{MARK}reel</span></header>',
        "<main>",
        "<h1>Licences</h1>",
        f'<p class="intro">reel {html.escape(version)} is under the MIT licence. The server and the app are built from '
        f"{len(on_page)} Rust crates by others, unmodified, each under its own licence: here is each licence's text once, "
        "with the crates it covers and a link to each one's source.</p>",
        '<p class="intro">A crate that offers a choice of licences is under the first of these it offers: Apache 2.0, '
        "whose text names no holder, then the others. OBS Studio, FFmpeg, Debian's libraries and the fonts are on "
        f'<a href="https://github.com/Locko2901/reel/blob/v{html.escape(version)}/THIRD_PARTY.md">THIRD_PARTY.md</a>, '
        "with where each one's source is.</p>",
        '<nav class="panel" aria-label="The licences on this page"><header><h2>On this page</h2></header><ul class="toc">',
        '<li><a href="#reel">reel</a><span>MIT</span></li>',
    ]
    for spdx in order:
        group = by_licence[spdx]
        n = len(set().union(*(t["crates"] for t in group)))
        words = f"{n} crate{'s' * (n != 1)}" + (f", {len(group)} texts" if len(group) > 1 else "")
        out.append(f'<li><a href="#{anchor(spdx)}">{html.escape(group[0]["name"])}</a><span>{words}</span></li>')
    if elsewhere:
        out.append(f'<li><a href="#elsewhere">In no build of reel</a><span>{len(elsewhere)} crates</span></li>')
    out += [
        "</ul></nav>",
        '<section id="reel"><h2>reel<span class="desc">MIT License</span></h2>',
        f'<div class="panel"><pre>{html.escape(reel_licence)}</pre></div></section>',
    ]
    for spdx in order:
        group = sorted(by_licence[spdx], key=lambda t: (-len(t["crates"]), sorted(t["crates"])))
        out.append(f'<section id="{anchor(spdx)}"><h2>{html.escape(group[0]["name"])}<span class="desc">{html.escape(spdx)}</span></h2>')
        for t in group:
            crates = ", ".join(crate_link(n, v) for n, v in sorted(t["crates"]))
            out.append(f'<div class="panel"><p class="crates">{crates}</p><pre>{html.escape(t["text"])}</pre></div>')
        out.append("</section>")
    if elsewhere:
        names = ", ".join(f"<span>{html.escape(n)} {html.escape(v)}</span>" for n, v in sorted(elsewhere))
        out += [
            f'<section id="elsewhere"><h2>In no build of reel<span class="desc">{len(elsewhere)} crates</span></h2>',
            '<div class="panel"><header><span class="desc">In the lock files, for other systems (macOS, Android, the web) '
            "or for reel's tests alone: none of them is in the server or the app.</span></header>",
            f'<p class="crates">{names}</p></div></section>',
        ]
    out += [
        "</main>",
        f'<footer class="foot">{MARK}<span>reel {html.escape(version)}: this page is made by {html.escape(tool)} from '
        "Cargo.lock and desktop/Cargo.lock as the server and the app are built.</span></footer>",
        "</body>",
        "</html>",
        "",
    ]
    return "\n".join(out)


def third_party(declared):
    groups = {}
    for (name, version), expr in sorted(declared.items()):
        groups.setdefault(normalise(expr), []).append(f"{name} {version}")
    lines = [BEGIN, "", f"{len(declared)} crates, by the licence each declares:", ""]
    for expr in sorted(groups, key=lambda e: (-len(groups[e]), e)):
        lines.append(f"- **{expr}** ({len(groups[expr])}): " + ", ".join(groups[expr]))
    lines += ["", END]
    path = ROOT / "THIRD_PARTY.md"
    text = path.read_text()
    start, end = text.index(BEGIN), text.index(END) + len(END)
    path.write_text(text[:start] + "\n".join(lines) + text[end:])


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("out", help="where the page goes; written only when it changes")
    ap.add_argument("reports", nargs=2, help="cargo-about's JSON of the server's workspace and of the app's")
    ap.add_argument("--tool", default="cargo-about")
    ap.add_argument("--third-party", action="store_true", help="also write THIRD_PARTY.md's crate list")
    args = ap.parse_args()

    texts, declared, on_page, wanting = gather(args.reports)
    if wanting:
        print("These crates have no licence text of their own that cargo-about can find:", file=sys.stderr)
        for w in wanting:
            print(f"  {w}", file=sys.stderr)
        print("Give each a clarify entry in about.toml: its licence file, in the crate or in its repository.", file=sys.stderr)
        sys.exit(1)
    lock = lock_crates()
    if on_page - lock:
        sys.exit(f"cargo-about read crates the lock files do not hold: {sorted(on_page - lock)[:5]}")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    text = page(texts, on_page, lock - on_page, version, args.tool)
    out = Path(args.out)
    if not out.exists() or out.read_text() != text:
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(text)
    print(f"{out}: {len(on_page)} crates, {len(texts)} texts, {len(text.encode())} bytes", file=sys.stderr)
    if args.third_party:
        third_party(declared)
        print(f"THIRD_PARTY.md: {len(declared)} crates", file=sys.stderr)


if __name__ == "__main__":
    main()
