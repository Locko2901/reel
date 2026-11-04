"""The user site's build hooks.

The privacy guard: every page as written and as built, and every text file the site
publishes from docs/, is searched for the words of DOCS_PRIVATE_WORDS (comma separated, from
a secret that is never committed: a deployment's addresses, host names, a person's name). Any
one found fails the build, naming the file and the line. An email address fails it too,
unless it is on example.com, .org or .net.

Some words are listed for a person to review, never failing the build: Discord, named only
where a page is really about it; a private repository; the GitHub account's name anywhere but
the repository's own addresses.

The site uses the web app's own fonts and icon, taken from web/assets as they are.
"""

import logging
import os
import re

from mkdocs.exceptions import PluginError
from mkdocs.structure.files import File

REVIEW = [
    ("Discord: only where the page is really about it", re.compile(r"discord", re.I)),
    ("a private repository", re.compile(r"private repo", re.I)),
    ("the GitHub account's name outside the repository's addresses", re.compile(r"locko2901(?!/reel\b|\.github\.io/reel)", re.I)),
]
EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,})")
EXAMPLE = re.compile(r"(?:^|\.)example\.(?:com|org|net)$", re.I)
TEXT = (".md", ".html", ".css", ".js", ".json", ".svg", ".txt", ".xml", ".yml", ".yaml")

found = []
review = []
log = logging.getLogger("mkdocs.hooks.review")


def rules():
    words = [w.strip() for w in os.environ.get("DOCS_PRIVATE_WORDS", "").split(",") if w.strip()]
    return [("a word of DOCS_PRIVATE_WORDS", re.compile(edge(w[0], r"(?<!\w)") + re.escape(w) + edge(w[-1], r"(?!\w)"), re.I)) for w in words]


def edge(c, guard):
    return guard if re.match(r"\w", c) else ""


def check(where, text):
    for n, line in enumerate(text.splitlines(), 1):
        for what, rule in rules():
            if rule.search(line):
                found.append(f"{where}:{n}: {what}")
        for m in EMAIL.finditer(line):
            if not EXAMPLE.search(m.group(1)):
                found.append(f"{where}:{n}: an email address")


def look(where, text):
    for n, line in enumerate(text.splitlines(), 1):
        for what, rule in REVIEW:
            if rule.search(line):
                review.append(f"{where}:{n}: {what}")


def on_pre_build(config):
    found.clear()
    review.clear()


def on_files(files, config):
    root = os.path.dirname(config["config_file_path"])
    web = os.path.join(root, "web")
    names = [f"assets/fonts/{n}" for n in sorted(os.listdir(os.path.join(web, "assets", "fonts")))]
    for path in names + ["assets/icon.svg"]:
        files.append(File(path, web, config["site_dir"], config["use_directory_urls"]))
    for f in files:
        if f.abs_src_path.startswith(config["docs_dir"]) and f.src_uri.endswith(TEXT) and not f.is_documentation_page():
            with open(f.abs_src_path, encoding="utf-8", errors="replace") as fh:
                check(f"docs/{f.src_uri}", fh.read())
    return files


def on_page_markdown(markdown, page, config, files):
    check(f"docs/{page.file.src_uri}", markdown)
    look(f"docs/{page.file.src_uri}", markdown)
    return markdown


def on_post_page(output, page, config):
    check(f"{page.file.dest_uri} (as built)", output)
    return output


def on_post_build(config):
    if review:
        log.info("to review, not failing the build (%d lines):\n  %s", len(review), "\n  ".join(review))
    if found:
        raise PluginError("private details on the site:\n  " + "\n  ".join(dict.fromkeys(found)))
