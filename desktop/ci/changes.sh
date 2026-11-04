#!/bin/sh
# Which Desktop and CI jobs a pull request's push runs, as key=value lines into $GITHUB_OUTPUT.
# GREEN: the heads of this workflow's passed runs on the pull request, newest first.
set -eu
out() { printf '%s=%s\n' "$1" "$2" >> "${GITHUB_OUTPUT:-/dev/stdout}"; }
# Not docs/configuration.md: make check proves it is made from the settings.
DOCS='^(docs/.*|[^/]*\.md)$'
CHECKED='^docs/configuration\.md$'
INSTALLERS='^(desktop/app/nsis/.*|desktop/app/tauri[^/]*\.json|desktop/packages\.json|desktop/obs/.*|desktop/ffmpeg/.*|desktop/ci/(build-windows\.sh|packages\.sh|fetch-obs\.sh|fetch-ffmpeg\.sh|fetch-previous\.sh|check-installers\.sh|check-nsis\.sh|check-obs-start\.sh|icuuc-stub\.c|desktop\.Dockerfile|changes\.sh)|Makefile|\.github/workflows/desktop\.yml)$'
# The files that rebuild the installer without OBS too, not the one with OBS alone.
PLAIN='^(desktop/app/nsis/(plain|common|English)\.nsh|desktop/app/nsis/installer\.nsi|desktop/app/tauri\.conf\.json|desktop/packages\.json|desktop/ci/(build-windows|check-installers|check-nsis|packages)\.sh)$'
labelled() { printf '%s\n' "${LABELS:-}" | tr ',' '\n' | grep -qx installers; }

if [ "${EVENT:-}" != pull_request ]; then
  out heavy true; out windows true; out plain true
  out why "a ${EVENT:-manual} run: everything"
  exit 0
fi
pr=$(git diff --name-only "$(git merge-base "$BASE" "$AFTER")" "$AFTER")
if printf '%s\n' "$pr" | grep -qx 'desktop/obs/pinned.json'; then
  out heavy true; out windows true; out plain true
  out why "the pull request moves OBS's pin: every check and both installers, on each push"
  exit 0
fi
since=""
for g in ${GREEN:-}; do
  if git cat-file -e "$g^{commit}" 2>/dev/null && git merge-base --is-ancestor "$g" "$AFTER" 2>/dev/null; then
    since=$g
    break
  fi
done
if [ -z "$since" ]; then
  heavy=true
  why="no passed run of this pull request is in this push's history"
elif changed=$(git diff --name-only "$since" "$AFTER") && [ -n "$(printf '%s\n' "$changed" | grep -vE "$DOCS" | grep -v '^$' || true)$(printf '%s\n' "$changed" | grep -E "$CHECKED" || true)" ]; then
  heavy=true
  why="more than documentation changed since ${since%"${since#???????}"}, the last passed run"
else
  heavy=false
  why="documentation only since ${since%"${since#???????}"}, the last passed run: the checks and builds are skipped"
fi
affected=false
printf '%s\n' "$pr" | grep -qE "$INSTALLERS" && affected=true
windows=false
if [ "$heavy" = true ] && { [ "$affected" = true ] || labelled; }; then
  windows=true
fi
if [ "${ACTION:-}" = labeled ] && [ "${NEW_LABEL:-}" = installers ]; then
  windows=true
fi
plain=false
if [ "$windows" = true ] && { printf '%s\n' "$pr" | grep -qE "$PLAIN" || labelled; }; then
  plain=true
fi
case "$windows/$plain" in
  true/true) why="$why; both installers" ;;
  true/false) why="$why; the installer with OBS alone" ;;
  *) why="$why; no installer: the pull request does not touch them and is not labelled installers" ;;
esac
out heavy "$heavy"; out windows "$windows"; out plain "$plain"; out why "$why"
