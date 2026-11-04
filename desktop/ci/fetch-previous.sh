#!/bin/sh
# The newest older release's installer with OBS, for check-obs-start.sh's update.
set -eu
INTO="$1" VERSION="${2:-}"
[ -n "$VERSION" ] || VERSION=$(sed -n 's/^ *"version": *"\([^"]*\)".*/\1/p' /work/desktop/app/tauri.conf.json | head -1)
. /work/desktop/ci/packages.sh
REPO="${GITHUB_REPOSITORY:-Locko2901/reel}"
API="${GITHUB_API_URL:-https://api.github.com}"
mkdir -p "$INTO"
rm -f "$INTO"/with-obs_*
get() { # get <accept> <url> <file>: the token in curl's config on its input, never on a line
  { [ -z "${GITHUB_TOKEN:-}" ] || printf 'header = "Authorization: Bearer %s"\n' "$GITHUB_TOKEN"; } |
    curl -fsSL --retry 3 -K - -H "Accept: $1" -o "$3" "$2"
}
get application/vnd.github+json "$API/repos/$REPO/releases?per_page=30" "$INTO/releases.json"
PICK=$(python3 - "$INTO/releases.json" "$VERSION" "$WITH_OBS_FILE" "$PLAIN_FILE" <<'EOF'
import json, re, sys
releases = json.load(open(sys.argv[1]))
with_obs, plain = sys.argv[3], sys.argv[4]
def key(v):
    return tuple(int(x) for x in re.findall(r"\d+", v.split("-")[0])[:3])
this = key(sys.argv[2])
for r in releases:
    if r.get("draft") or r.get("prerelease"):
        continue
    v = r["tag_name"].lstrip("v")
    if key(v) >= this:
        continue
    assets = {a["name"]: a["url"] for a in r.get("assets", [])}
    # releases up to 1.10 call the one with OBS reel-with-obs, and the one without reel
    named = f"{plain}_{v}_x64-setup.exe" in assets
    name = f"{with_obs}_{v}_x64-setup.exe" if named else f"reel-with-obs_{v}_x64-setup.exe"
    if name in assets and f"{name}.sha256" in assets:
        print(name, assets[name], assets[f"{name}.sha256"])
        break
EOF
)
rm -f "$INTO/releases.json"
if [ -z "$PICK" ]; then
  echo "previous: no release older than $VERSION has an installer with OBS"
  exit 0
fi
set -- $PICK
to="with-obs_$(echo "$1" | sed 's/^.*_\([^_]*\)_x64-setup\.exe$/\1/')_x64-setup.exe"
get application/octet-stream "$2" "$INTO/$to.part"
get application/octet-stream "$3" "$INTO/$to.sha256"
want=$(cut -d' ' -f1 < "$INTO/$to.sha256")
have=$(sha256sum "$INTO/$to.part" | cut -d' ' -f1)
[ "$want" = "$have" ] || { echo "previous: $1 does not match its .sha256"; rm -f "$INTO/$to.part"; exit 1; }
mv "$INTO/$to.part" "$INTO/$to"
echo "previous: $1, checksum ok, as $to"
