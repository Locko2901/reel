#!/bin/sh
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
R=$(mktemp -d)
trap 'rm -rf "$R"' EXIT
cd "$R"
git init -q -b main
git config user.email ci@example.org
git config user.name ci
put() { mkdir -p "$(dirname "$1")"; printf '%s\n' "$2" >> "$1"; git add "$1"; git commit -qm "$1"; git rev-parse HEAD; }
put README.md "reel" >/dev/null
put docs/a.md "a" >/dev/null
put desktop/app/src/x.rs "fn x() {}" >/dev/null
put desktop/app/nsis/plain.nsh "; plain" >/dev/null
M=$(put desktop/app/nsis/with-obs.nsh "; with OBS")
git checkout -qb pr
CODE=$(put desktop/app/src/x.rs "fn y() {}")
DOC=$(put docs/a.md "b")
RULES=$(put CONTRIBUTING.md "- a rule")
SHOT=$(put docs/img/shot.png "a picture")
TOP=$(put CHANGELOG.md "1.0.0")
WITH=$(put desktop/app/nsis/with-obs.nsh "; the page")
PLAIN=$(put desktop/app/nsis/plain.nsh "; the page")
MAKE=$(put Makefile "check:")
CONF=$(put docs/configuration.md "| key |")
PIN=$(put desktop/obs/pinned.json '{ "version": "33.1.0" }')
PINDOC=$(put docs/a.md "c")
git checkout -qb names "$M"
NAMES=$(put desktop/packages.json "{}")
git checkout -q main
OTHER=$(put desktop/app/src/x.rs "fn z() {}")
git checkout -q pr

fail=0
case_() { # case_ <what> <want heavy windows plain> <env...>
  what=$1 want=$2
  shift 2
  o=$(mktemp)
  env -i PATH="$PATH" HOME="$R" GITHUB_OUTPUT="$o" EVENT=pull_request ACTION=synchronize BASE="$M" "$@" sh "$HERE/changes.sh"
  got="$(sed -n 's/^heavy=//p' "$o") $(sed -n 's/^windows=//p' "$o") $(sed -n 's/^plain=//p' "$o")"
  if [ "$got" = "$want" ]; then
    printf 'ok   %s: %s (%s)\n' "$what" "$got" "$(sed -n 's/^why=//p' "$o")"
  else
    printf 'FAIL %s: want %s, got %s (%s)\n' "$what" "$want" "$got" "$(sed -n 's/^why=//p' "$o")"
    fail=1
  fi
  rm -f "$o"
}
case_ "a manual run" "true true true" EVENT=workflow_dispatch AFTER="$MAKE"
case_ "a new pull request with code" "true false false" ACTION=opened AFTER="$CODE"
case_ "documentation pushed after a passed run" "false false false" AFTER="$DOC" GREEN="$CODE"
case_ "documentation after a cancelled or failed run (none passed)" "true false false" AFTER="$DOC"
case_ "more documentation, the newest passed run first" "false false false" AFTER="$TOP" GREEN="$DOC $CODE"
case_ "a picture of the docs counts as documentation" "false false false" AFTER="$SHOT" GREEN="$RULES"
case_ "a passed run only before code" "true false false" AFTER="$DOC" GREEN="$M"
case_ "a force push: no passed run in its history" "true false false" AFTER="$DOC" GREEN="$OTHER"
case_ "a passed run this clone does not have is passed over" "false false false" AFTER="$DOC" GREEN="0123456789abcdef0123456789abcdef01234567 $CODE"
case_ "the hooks with OBS changed" "true true false" AFTER="$WITH" GREEN="$TOP"
case_ "the hooks without OBS changed" "true true true" AFTER="$PLAIN" GREEN="$WITH"
case_ "the Makefile builds the installers" "true true true" AFTER="$MAKE" GREEN="$PLAIN"
case_ "docs/configuration.md is checked by make check: not documentation alone" "true true true" AFTER="$CONF" GREEN="$MAKE"
case_ "the packages' names alone: both installers" "true true true" AFTER="$NAMES"
case_ "labelled installers, with code" "true true true" ACTION=opened AFTER="$CODE" LABELS=installers
case_ "labelled installers, documentation after a passed run" "false false false" AFTER="$DOC" GREEN="$CODE" LABELS="bug,installers"
case_ "the label installers added to a passed head" "false true true" ACTION=labeled NEW_LABEL=installers AFTER="$DOC" GREEN="$DOC" LABELS=installers
case_ "another label added to a passed head" "false false false" ACTION=labeled NEW_LABEL=bug AFTER="$DOC" GREEN="$DOC" LABELS=bug
case_ "a new OBS pinned: every check and both installers" "true true true" AFTER="$PIN" GREEN="$CONF"
case_ "documentation pushed to a bump after a passed run: everything again" "true true true" AFTER="$PINDOC" GREEN="$PIN"
exit $fail
