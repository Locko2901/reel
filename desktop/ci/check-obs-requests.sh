#!/bin/sh
# Usage: check-obs-requests.sh <port> <password>
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
sh "$HERE/obs-requests.sh" > "$T/reel"
python3 "$HERE/obs-ws.py" "$1" "$2" GetVersion '{}' > "$T/obs" || { echo "FAIL GetVersion: $(cat "$T/obs")"; exit 1; }
python3 - "$T/obs" "$T/reel" <<'PY'
import json, sys
obs = json.loads(open(sys.argv[1]).read().split(" ", 1)[1])
reel = open(sys.argv[2]).read().split()
lacks = [r for r in reel if r not in obs["availableRequests"]]
if len(reel) < 20 or lacks:
    sys.exit(f"FAIL OBS {obs['obsVersion']} lacks {len(lacks)} of the {len(reel)} requests reel sends: {' '.join(lacks)}")
print(f"ok   OBS {obs['obsVersion']}, obs-websocket {obs['obsWebSocketVersion']}: each of the {len(reel)} requests reel's code sends is one it has")
PY
