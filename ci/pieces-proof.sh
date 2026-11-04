#!/usr/bin/env bash
# Uploads in pieces through an nginx refusing any request over 100 MB, as Cloudflare does.
#   ci/pieces-proof.sh [the demo, http://127.0.0.1:8746] [the proxy's port, 8749]
set -euo pipefail
DEMO="${1:-http://127.0.0.1:8746}"
PORT="${2:-8749}"
FF="${REEL_FFMPEG:-ffmpeg}"
DIR=$(mktemp -d)
NAME="reel-pieces-proxy-$PORT"
trap 'docker rm -f "$NAME" >/dev/null 2>&1 || true; rm -rf "$DIR"' EXIT
clip() { # clip <file> <seconds> <size> <rate> <bitrate>
  "$FF" -hide_banner -loglevel error -f lavfi -i "testsrc2=size=$3:rate=$4,noise=alls=40:allf=t" \
    -f lavfi -i "sine=frequency=440:sample_rate=48000" -t "$2" -c:v libx264 -preset ultrafast \
    -b:v "$5" -maxrate "$5" -bufsize 80M -pix_fmt yuv420p -c:a aac -b:a 128k -threads 4 "$DIR/$1"
}
clip "a synthetic 500 MB clip.mp4" 105 1920x1080 60 38M
clip "shared from the phone.mp4" 60 1920x1080 30 20M
clip "dropped on the library.mp4" 4 640x360 30 2M
cat > "$DIR/nginx.conf" <<CONF
events {}
http {
  log_format sizes '\$request_method \$uri \$status \$request_length';
  access_log /var/log/nginx/sizes.log sizes;
  server {
    listen $PORT;
    client_max_body_size 100m;
    location / {
      proxy_pass $DEMO;
      proxy_http_version 1.1;
      proxy_set_header Host \$host;
      proxy_set_header Connection "";
      proxy_read_timeout 300s;
      proxy_send_timeout 300s;
    }
  }
}
CONF
docker run -d --name "$NAME" --network host -v "$DIR/nginx.conf:/etc/nginx/nginx.conf:ro" -v "$DIR:/var/log/nginx" nginx:1.27.5-alpine >/dev/null
for _ in $(seq 1 20); do curl -fsS -o /dev/null "http://127.0.0.1:$PORT/healthz" 2>/dev/null && break; sleep 1; done
code=$(curl -s -o /dev/null -w '%{http_code}' -F "file=@$DIR/a synthetic 500 MB clip.mp4" "http://127.0.0.1:$PORT/upload")
[ "$code" = 413 ] || { echo "FAIL the proxy took one request of 500 MB ($code)"; exit 1; }
echo "ok   the proxy refuses the 500 MB clip as one request (413), as Cloudflare would"
node ci/browser/check-pieces.mjs "http://127.0.0.1:$PORT" "$DIR/a synthetic 500 MB clip.mp4" "$DIR/shared from the phone.mp4" \
  "$DIR/dropped on the library.mp4" --log "$DIR/sizes.log" ${REEL_PIECES_SHOTS:+--shots "$REEL_PIECES_SHOTS"}
