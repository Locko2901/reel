#!/bin/sh
# Fails on dashes, special spaces, typographic quotes and pictographs in tracked text files.
set -eu
cd "$(dirname "$0")/.."
export LC_ALL=C.UTF-8

BAD='[\x{00A0}\x{00AD}\x{2010}-\x{2015}\x{2018}\x{2019}\x{201C}\x{201D}\x{2026}\x{2000}-\x{200B}\x{202F}\x{205F}\x{2212}\x{3000}\x{FEFF}\x{2190}-\x{21FF}\x{2600}-\x{27BF}\x{2B00}-\x{2BFF}\x{FE0F}\x{1F000}-\x{1FFFF}]'

if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git ls-files -z
else
  find . -path ./.git -prune -o -path ./target -prune -o -path ./site -prune \
    -o -path ./dev -prune -o -path '*/node_modules' -prune -o -type f -print0 | sed -z 's|^\./||'
fi | grep -zv '^dev/' | xargs -0 grep -InP "$BAD" -- >"${TMPDIR:-/tmp}/reel-check-text.$$" || true

if [ -s "${TMPDIR:-/tmp}/reel-check-text.$$" ]; then
  cat "${TMPDIR:-/tmp}/reel-check-text.$$"
  rm -f "${TMPDIR:-/tmp}/reel-check-text.$$"
  echo "check-text: the lines above hold a dash, a special space, a typographic quote or ellipsis, or a pictograph; write plain ASCII (CONTRIBUTING.md)" >&2
  exit 1
fi
rm -f "${TMPDIR:-/tmp}/reel-check-text.$$"
echo "check-text: clean"
