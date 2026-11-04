#!/usr/bin/env bash
# External storage against the demo and a MinIO of its own; the demo's storage ends switched off.
#   ci/external-proof.sh [the demo, http://127.0.0.1:8746] [MinIO's port, 9102]
set -euo pipefail
DEMO="${1:-http://127.0.0.1:8746}"
PORT="${2:-9102}"
FF="${REEL_FFMPEG:-ffmpeg}"
USER_NAME="${REEL_DEMO_USER:-sam}"
PASSWORD="${REEL_DEMO_PASSWORD:?the password of the demo admin}"
MINIO=cgr.dev/chainguard/minio@sha256:2136d593beacf4461187035eca949fe49abd589d5079735025c711ca58fee0c8
MC=cgr.dev/chainguard/minio-client@sha256:d8a93c5d335509ff353774eefec8f82fa2a2a975e800139702232d8be3dbe6b9
NAME="reel-external-proof-$PORT"
KEY=reelproof SECRET="reelproof-$(head -c 9 /dev/urandom | od -An -tx1 | tr -d ' \n')"
PUBLIC="http://127.0.0.1:$PORT/previews"
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
cleanup() {
  [ -s "$JAR" ] && set_setting external.on false || true
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  rm -rf "$DIR"
}
trap cleanup EXIT

curl -fsS -c "$JAR" -o /dev/null "$DEMO/signin"
curl -fsS -b "$JAR" -c "$JAR" -o /dev/null -H 'Sec-Fetch-Site: same-origin' \
  --data-urlencode "name=$USER_NAME" --data-urlencode "password=$PASSWORD" --data-urlencode remember=on "$DEMO/signin"

docker run -d --name "$NAME" -p "127.0.0.1:$PORT:9000" -e MINIO_ROOT_USER="$KEY" -e MINIO_ROOT_PASSWORD="$SECRET" \
  "$MINIO" server /tmp/data >/dev/null
for _ in $(seq 1 30); do curl -fsS -o /dev/null "http://127.0.0.1:$PORT/minio/health/live" 2>/dev/null && break; sleep 1; done
mc() { docker run --rm --network host -e "MC_HOST_p=http://$KEY:$SECRET@127.0.0.1:$PORT" "$MC" "$@" >/dev/null; }
mc mb p/previews
mc anonymous set download p/previews
ok "MinIO ($(docker run --rm "$MINIO" --version | head -1 | cut -d' ' -f3)) on :$PORT, a bucket anyone may read"

for kv in "external.endpoint=http://127.0.0.1:$PORT" external.region=auto external.bucket=previews \
  "external.access_key=$KEY" "external.public_url=$PUBLIC" external.ahead=off; do
  set_setting "${kv%%=*}" "${kv#*=}"
done
set_setting external.secret_key "$SECRET"
set_setting external.on true
line=$(web POST /api/external/test | field line)
[[ "$line" == *"the storage works"* ]] || fail "the Test with the key: $line"
ok "Test: $line"
set_setting external.secret_key "not-the-secret"
refused=$(web POST /api/external/test | field error)
[[ "$refused" == *SignatureDoesNotMatch* ]] || fail "the Test with a refused key: $refused"
ok "Test with a refused key: $refused"
set_setting external.secret_key "$SECRET"

code=$(web POST /api/devices/pair-code | field code)
TOKEN=$(curl -fsS -X POST -H 'content-type: application/json' -d "{\"code\":\"$code\",\"name\":\"external proof\"}" \
  "$DEMO/api/device/pair" | field token)
dev() { curl -sS -H "authorization: Bearer $TOKEN" -H 'content-type: application/json' -X "$1" "$DEMO$2" ${3:+-d "$3"}; }
CLIP="$DIR/Valorant 2026-10-14 21-00-00.mp4"
"$FF" -hide_banner -loglevel error -f lavfi -i "testsrc2=size=1920x1080:rate=60,noise=alls=30:allf=t" \
  -f lavfi -i "sine=frequency=440:sample_rate=48000" -t 30 -c:v libx264 -preset ultrafast -b:v 30M -maxrate 30M \
  -bufsize 60M -pix_fmt yuv420p -c:a aac -b:a 128k "$CLIP"
size=$(stat -c %s "$CLIP")
sha=$(sha256sum "$CLIP" | cut -d' ' -f1)
up=$(dev POST /api/device/uploads "{\"file_name\":\"$(basename "$CLIP")\",\"size\":$size,\"sha256\":\"$sha\",\"keep_last\":20}")
id=$(echo "$up" | field upload_id)
[ -n "$id" ] || fail "no upload: $up"
split -b 40000000 -d "$CLIP" "$DIR/piece."
offset=0
for p in "$DIR"/piece.*; do
  offset=$(curl -fsS -H "authorization: Bearer $TOKEN" -X PUT --data-binary "@$p" "$DEMO/api/device/uploads/$id?offset=$offset" | field offset)
done
CLIP_ID=$(dev POST "/api/device/uploads/$id/finish" | field clip_id)
ok "a 30 s replay of $((size / 1000000)) MB uploaded by a paired app in pieces, keeping its last 20 s"

started=$(date +%s.%N)
upload_from=""
steps=""
for _ in $(seq 1 1500); do
  answer=$(dev POST /api/device/share "{\"clip_id\":\"$CLIP_ID\"}")
  status=$(echo "$answer" | field status)
  [ "$status" = ready ] && break
  step=$(echo "$answer" | field step)
  [[ " $steps " == *" $step "* ]] || steps="$steps $step"
  [ "$step" = upload ] && [ -z "$upload_from" ] && upload_from=$(date +%s.%N)
  sleep 0.2
done
[ "$status" = ready ] || fail "the one-key share gave no link: $answer"
done_at=$(date +%s.%N)
URL=$(echo "$answer" | field url)
TOKEN_FOLLOWS=${URL##*/}
page() { curl -fsS "$DEMO/s/$1" | sed -n 's/.*<meta property="og:video" content="\([^"]*\)".*/\1/p' | head -1; }
object() { curl -s -o /dev/null -w '%{http_code} %{redirect_url}\n' "$1"; }
VIDEO=$(page "$TOKEN_FOLLOWS")
[[ "$VIDEO" == *"/s/$TOKEN_FOLLOWS/video.mp4?r="* ]] || fail "og:video is not the demo's address: $VIDEO"
read -r hop OBJECT < <(object "$VIDEO")
[[ "$hop" == 302 && "$OBJECT" == "$PUBLIC/reel/"* ]] || fail "og:video does not lead to the object: $hop $OBJECT"
got=$(curl -fsSL -o /dev/null -w '%{http_code} %{content_type} %{size_download}' "$VIDEO")
[[ "$got" == "200 video/mp4 "* ]] || fail "the object does not answer: $got"
wait_upload=$(python3 -c "print(round($done_at - ${upload_from:-$done_at}, 2))")
total=$(python3 -c "print(round($done_at - $started, 1))")
ok "one-key share: the steps$steps, then the link after ${total} s; it waited ${wait_upload} s for its preview's upload to MinIO on this machine"
ok "og:video is the demo's address and leads to the object, which answers $got bytes"
size_object=$(echo "$got" | cut -d' ' -f3)
redirect=$(curl -s -o /dev/null -w '%{time_total}' "$VIDEO")
through=$(curl -s -L -o /dev/null -w '%{time_total}' "$VIDEO")
direct=$(curl -s -o /dev/null -w '%{time_total}' "$OBJECT")
head=$(curl -s -I -L "$VIDEO" | tr -d '\r' | awk 'tolower($1) ~ /^http\// {c=$2} tolower($1)=="content-length:" {n=$2} END {print c, n}')
range=$(curl -s -L -r 0-99 -o /dev/null -w '%{http_code} %{size_download}' "$VIDEO")
[[ "$range" == "206 100" ]] || fail "a range does not follow the redirect: $range"
[[ "$head" == "200 $size_object" ]] || fail "a HEAD does not follow the redirect: $head"
ok "the redirect itself takes ${redirect} s; the preview through it ${through} s, straight from the object ${direct} s; a HEAD (status and length: $head) and a range (status and bytes: $range) follow it"

current=$(web GET "/api/clips/$CLIP_ID" | field clip.current_id)
pinned=$(web POST "/api/clips/$CLIP_ID/shares" "{\"rendition_id\":\"$current\"}")
PINNED_ID=$(echo "$pinned" | field id)
PINNED_TOKEN=$(echo "$pinned" | field token)
[ "$(echo "$pinned" | field preview)" = "preview from storage" ] || fail "the pinned link: $pinned"
[ "$(object "$(page "$PINNED_TOKEN")" | cut -d' ' -f2)" = "$OBJECT" ] || fail "the pinned link has another object"
ok "a link pinned to the same version shares its object"

job=$(web POST "/api/clips/$CLIP_ID/cuts" '{"start":2.0,"end":8.0}' | field job_id)
for _ in $(seq 1 300); do
  state=$(web GET "/api/jobs/$job" | field state)
  [ "$state" = done ] && break
  sleep 1
done
[ "$state" = done ] || fail "the edit did not finish: $state"
for _ in $(seq 1 120); do
  moved=$(object "$(page "$TOKEN_FOLLOWS")" | cut -d' ' -f2)
  [[ "$moved" == "$PUBLIC/reel/"* && "$moved" != "$OBJECT" ]] && break
  sleep 1
done
[[ "$moved" == "$PUBLIC/reel/"* && "$moved" != "$OBJECT" ]] || fail "the following link did not move: $moved"
[ "$(object "$(page "$PINNED_TOKEN")" | cut -d' ' -f2)" = "$OBJECT" ] || fail "the pinned link lost its object"
ok "an edit moved the following link to its own object; the pinned one kept the original's"

web DELETE "/api/shares/$PINNED_ID" >/dev/null
for _ in $(seq 1 60); do
  [ "$(curl -s -o /dev/null -w '%{http_code}' "$OBJECT")" = 404 ] && break
  sleep 1
done
[ "$(curl -s -o /dev/null -w '%{http_code}' "$OBJECT")" = 404 ] || fail "the revoked link's object is still there"
old=$(curl -s -L -o /dev/null -w '%{http_code} %{size_download}' "$VIDEO")
[[ "$old" == "200 "* ]] || fail "the og:video of a message posted before the edit no longer plays: $old"
ok "revoking the pinned link deleted the original's object; the og:video posted before the edit still plays ($old bytes, the link's current preview)"

docker stop "$NAME" >/dev/null
fallen=$(web POST "/api/clips/$CLIP_ID/shares" "{\"rendition_id\":\"$current\"}")
FALLEN_TOKEN=$(echo "$fallen" | field token)
[ "$(echo "$fallen" | field preview)" = "preview from your server: the storage cannot be reached" ] \
  || fail "with MinIO stopped: $(echo "$fallen" | field preview) $(echo "$fallen" | field preview_note)"
FALLEN_VIDEO=$(page "$FALLEN_TOKEN")
[ "$(object "$FALLEN_VIDEO")" = "200 " ] || fail "while MinIO is down the demo does not serve the preview itself: $(object "$FALLEN_VIDEO")"
status_line=$(web GET /status | python3 -c 'import html,re,sys
m=re.search(r"<dt>link storage</dt><dd>(.*?)</dd>", sys.stdin.read(), re.S)
print(html.unescape(re.sub(r"<[^>]*>", "", m.group(1))).strip() if m else "")')
[[ "$status_line" == *"served from your server"* ]] || fail "Status does not say so: $status_line"
note=$(echo "$fallen" | field preview_note)
ok "MinIO stopped: a new link's preview comes from the demo itself; the link's note: $note Status: $status_line"

docker start "$NAME" >/dev/null
for _ in $(seq 1 30); do curl -fsS -o /dev/null "http://127.0.0.1:$PORT/minio/health/live" 2>/dev/null && break; sleep 1; done
web POST /api/external/check >/dev/null
for _ in $(seq 1 60); do
  back=$(object "$FALLEN_VIDEO" | cut -d' ' -f2)
  [[ "$back" == "$PUBLIC/reel/"* ]] && break
  web POST /api/external/check >/dev/null
  sleep 1
done
[[ "$back" == "$PUBLIC/reel/"* ]] || fail "MinIO came back and the preview did not go up again: $back"
ok "MinIO back: the link's preview went up again, and its og:video leads to the object"
web DELETE "/api/clips/$CLIP_ID" >/dev/null
ok "the proof's clip is in the demo's trash; the storage is switched off again"
