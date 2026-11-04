#!/usr/bin/env bash
# Save and share's path on the server timed against the demo, idle and busy, over loopback.
#   ci/share-speed.sh [the demo, http://127.0.0.1:8746] [MinIO's port, 9103]
set -euo pipefail
DEMO="${1:-http://127.0.0.1:8746}"
PORT="${2:-9103}"
FF="${REEL_FFMPEG:-ffmpeg}"
USER_NAME="${REEL_DEMO_USER:-sam}"
PASSWORD="${REEL_DEMO_PASSWORD:?the password of the demo admin}"
CACHE="${REEL_PROOF_CACHE:-$HOME/.cache/reel-proof}"
MINIO=cgr.dev/chainguard/minio@sha256:2136d593beacf4461187035eca949fe49abd589d5079735025c711ca58fee0c8
MC=cgr.dev/chainguard/minio-client@sha256:d8a93c5d335509ff353774eefec8f82fa2a2a975e800139702232d8be3dbe6b9
NAME="reel-share-speed-$PORT"
KEY=reelproof SECRET="reelproof-$(head -c 9 /dev/urandom | od -An -tx1 | tr -d ' \n')"
DIR=$(mktemp -d)
JAR="$DIR/jar"
fail() { printf 'FAIL %s\n' "$*"; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
field() { python3 -c 'import json,sys; v=json.load(sys.stdin)
for k in sys.argv[1].split("."): v=v.get(k) if isinstance(v, dict) else None
print("" if v is None else v)' "$1"; }
web() { # web <method> <path> [json body]: the demo signed in, as its pages ask
  curl -sS -b "$JAR" -H 'Sec-Fetch-Site: same-origin' -H 'content-type: application/json' -X "$1" "$DEMO$2" ${3:+-d "$3"}
}
set_setting() { web PATCH /api/settings "{\"$1\":\"$2\"}" >/dev/null; }
LINKS=""
CLIPS=""
cleanup() {
  for s in $LINKS; do web DELETE "/api/shares/$s" >/dev/null 2>&1 || true; done
  for c in $CLIPS; do web DELETE "/api/clips/$c" >/dev/null 2>&1 || true; done
  [ -n "$CLIPS" ] && web POST /api/storage/purge-trash >/dev/null 2>&1 || true
  [ -s "$JAR" ] && set_setting external.on false || true
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  rm -rf "$DIR"
}
trap cleanup EXIT

mkdir -p "$CACHE"
REPLAY="$CACHE/Apex Legends 2026-10-14 19-20-37.mkv"
if [ ! -s "$REPLAY" ]; then
  "$FF" -hide_banner -loglevel error -f lavfi -i "testsrc2=size=2560x1440:rate=60,noise=alls=12:allf=t" \
    -f lavfi -i "sine=frequency=440:sample_rate=48000" -f lavfi -i "sine=frequency=660:sample_rate=48000" \
    -f lavfi -i "anoisesrc=color=pink:amplitude=0.05:sample_rate=48000" -map 0:v -map 1:a -map 2:a -map 3:a \
    -t 180 -c:v libx264 -preset ultrafast -b:v 10M -maxrate 10M -bufsize 20M -g 120 -pix_fmt yuv420p \
    -c:a aac -b:a 160k "$REPLAY.part.mkv"
  mv "$REPLAY.part.mkv" "$REPLAY"
fi
ok "the replay: 3 min of 1440p60 with three audio tracks, $(($(stat -c %s "$REPLAY") / 1000000)) MB"

curl -fsS -c "$JAR" -o /dev/null "$DEMO/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode remember=on "$DEMO/signin"
docker run -d --name "$NAME" -p "127.0.0.1:$PORT:9000" -e MINIO_ROOT_USER="$KEY" -e MINIO_ROOT_PASSWORD="$SECRET" \
  "$MINIO" server /tmp/data >/dev/null
for _ in $(seq 1 30); do curl -fsS -o /dev/null "http://127.0.0.1:$PORT/minio/health/live" 2>/dev/null && break; sleep 1; done
mc() { docker run --rm --network host -e "MC_HOST_p=http://$KEY:$SECRET@127.0.0.1:$PORT" "$MC" "$@" >/dev/null; }
mc mb p/previews
mc anonymous set download p/previews
for kv in "external.endpoint=http://127.0.0.1:$PORT" external.region=auto external.bucket=previews \
  "external.access_key=$KEY" "external.public_url=http://127.0.0.1:$PORT/previews" external.ahead=off; do
  set_setting "${kv%%=*}" "${kv#*=}"
done
set_setting external.secret_key "$SECRET"
set_setting external.on true
ok "the storage on: a MinIO of its own on :$PORT"
code=$(web POST /api/devices/pair-code | field code)
TOKEN=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$code\",\"name\":\"share speed\"}" \
  "$DEMO/api/device/pair" | field token)
dev() { curl -sS -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' -X "$1" "$DEMO$2" ${3:+-d "$3"}; }
idle() { web GET /api/jobs | python3 -c 'import json,sys
sys.exit(1 if any(j["state"] in ("queued", "running") for j in json.load(sys.stdin)["jobs"]) else 0)'; }
stop_web() { # the busy run's web copy, once the link is out
  web GET /api/jobs | python3 -c 'import json,sys
for j in json.load(sys.stdin)["jobs"]:
    if j["kind"] == "web" and j["state"] in ("queued", "running"): print(j["id"])' | while read -r j; do
    web POST "/api/jobs/$j/cancel" >/dev/null || true
  done
}
wait_idle() { for _ in $(seq 1 600); do idle && return 0; sleep 1; done; fail "the demo's jobs did not end"; }

# the busy run's long job: a web copy of the master, for a link pinned to it
busy() {
  local master share
  master=$(web GET "/api/clips/$1" | python3 -c 'import json,sys
print(next(r["id"] for r in json.load(sys.stdin)["renditions"] if r["kind"] == "master"))')
  share=$(web POST "/api/clips/$1/shares" "{\"rendition_id\":\"$master\"}")
  LINKS="$LINKS $(echo "$share" | field id)"
  curl -fsS -o /dev/null "$DEMO/s/$(echo "$share" | field token)"
  for _ in $(seq 1 60); do
    web GET /api/jobs | python3 -c 'import json,sys
sys.exit(0 if any(j["kind"] == "web" and j["state"] == "running" for j in json.load(sys.stdin)["jobs"]) else 1)' && return 0
    sleep 0.5
  done
  fail "the web copy of the 3 min master did not start"
}

run() { # run <label> <replay> [the clip whose master's web copy runs meanwhile]
  wait_idle
  [ -n "${3:-}" ] && busy "$3"
  local file="$2" size sha up id offset started_up finished clip status answer step steps="" t link_at
  size=$(stat -c %s "$file")
  sha=$(sha256sum "$file" | cut -d' ' -f1)
  up=$(dev POST /api/device/uploads "{\"file_name\":\"$(basename "$file")\",\"size\":$size,\"sha256\":\"$sha\",\"keep_last\":30,\"game\":\"Apex Legends\"}")
  [ "$(echo "$up" | field status)" != exists ] || fail "the demo has this replay already: $up"
  id=$(echo "$up" | field upload_id)
  [ -n "$id" ] || fail "no upload: $up"
  rm -f "$DIR"/piece.*
  split -b 90000000 -d "$file" "$DIR/piece."
  started_up=$(date +%s.%N)
  offset=0
  for p in "$DIR"/piece.*; do
    offset=$(curl -fsS -H "authorization: Bearer $TOKEN" -X PUT --data-binary "@$p" "$DEMO/api/device/uploads/$id?offset=$offset" | field offset)
  done
  clip=$(dev POST "/api/device/uploads/$id/finish" | field clip_id)
  finished=$(date +%s.%N)
  [ -n "$clip" ] || fail "the upload did not finish"
  CLIPS="$CLIPS $clip"
  LAST_CLIP=$clip
  for _ in $(seq 1 3000); do
    answer=$(dev POST /api/device/share "{\"clip_id\":\"$clip\"}")
    status=$(echo "$answer" | field status)
    [ "$status" = ready ] && break
    step=$(echo "$answer" | field step)
    t=$(python3 -c "print(round($(date +%s.%N) - $finished, 1))")
    [[ "$steps" == *" $step@"* ]] || steps="$steps $step@${t}s"
    sleep 0.2
  done
  [ "$status" = ready ] || fail "no link: $answer"
  link_at=$(python3 -c "print(round($(date +%s.%N) - $finished, 1))")
  ok "$1: the upload $(python3 -c "print(round($finished - $started_up, 1))") s over the loopback; then the link after $link_at s (steps:${steps:- none})"
  [ -n "${3:-}" ] && stop_web
  wait_idle
  web GET "/api/jobs?show=history" > "$DIR/jobs.json"
  python3 - "$finished" "$DIR/jobs.json" "$clip" <<'PY'
import datetime, json, sys
t0 = float(sys.argv[1])
at = lambda s: round(datetime.datetime.fromisoformat(s).timestamp() - t0, 1) if s else None
jobs = [j for j in json.load(open(sys.argv[2]))["jobs"] if j.get("clip_id") == sys.argv[3]]
for j in sorted(jobs, key=lambda j: j.get("started_at") or ""):
    print(f"       {j['kind']:<8} priority {j['priority']}: queued at {at(j['created_at'])} s, ran {at(j['started_at'])} to {at(j['finished_at'])} s{', ' + j['route'] if j.get('route') else ''}")
PY
}

TWIN="$CACHE/Apex Legends 2026-10-14 19-24-02.mkv"
[ -s "$TWIN" ] || "$FF" -hide_banner -loglevel error -i "$REPLAY" -map 0 -c copy -metadata title=twin "$TWIN"
run "an idle demo" "$REPLAY"
run "a demo busy with a web copy of 3 min of 1440p" "$TWIN" "$LAST_CLIP"
