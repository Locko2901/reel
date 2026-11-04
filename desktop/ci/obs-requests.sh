#!/bin/sh
# The obs-websocket requests reel's code sends, one a line, read from the code: the literal
# each request or obs_request call names, and the microphone filters' requests.
# With --check: fails when a call names its request another way, which this would not read.
set -eu
D=$(cd "$(dirname "$0")/.." && pwd)
SRC="$D/core/src $D/app/src"
# shellcheck disable=SC2086
list() {
  {
    grep -rhoPz '\b(?:request|obs_request)\(\s*"\K[A-Za-z]+(?=")' $SRC
    grep -rhoPz 'out\.push\(\(\s*"\K[A-Z][A-Za-z]+(?="\.to_string\(\))' $SRC
  } | tr '\0' '\n' | grep -v '^$' | sort -u
}
if [ "${1:-}" != --check ]; then
  list
  exit 0
fi
# shellcheck disable=SC2086
other=$(grep -rhoPz '(?:\bfn\s+)?\b(?:\.request|obs_request)\(\s*[^"\s)][^,)]*' $SRC | tr '\n' ' ' | tr '\0' '\n' |
  grep -v '^$' | grep -vE '^fn |\(&kind$' || true)
[ -z "$other" ] || { printf 'FAIL a request named another way than by a literal:\n%s\n' "$other"; exit 1; }
for must in GetVersion SaveReplayBuffer StartReplayBuffer CreateSourceFilter; do
  list | grep -qx "$must" || { echo "FAIL $must is not in the list read from the code"; exit 1; }
done
echo "ok   $(list | wc -l) request types read from the code, each named by a literal"
