#!/usr/bin/env python3
# GitHub's newest release on the loopback for the checks: its JSON, an AppImage and its .sig.
#   standin-release.py <port> <version> <AppImage> <its .sig>
import http.server
import json
import sys

port, version, image, sig = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4]
base = f"http://127.0.0.1:{port}"
name = f"reel_{version}_amd64.AppImage"
with open(image, "rb") as f, open(sig, "rb") as g:
    files = {name: f.read(), name + ".sig": g.read()}
release = {
    "tag_name": f"v{version}",
    "html_url": f"{base}/releases/tag/v{version}",
    "published_at": "2026-11-11T10:10:00Z",
    "body": "### Bug Fixes\n\n* a release on the loopback, for the checks\n",
    "assets": [{"name": n, "browser_download_url": f"{base}/download/v{version}/{n}"} for n in files],
}


class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        print(f"asked {self.path} by {self.headers.get('user-agent', '')}", flush=True)
        last = self.path.rsplit("/", 1)[-1]
        if self.path.endswith("/releases/latest"):
            body, kind = json.dumps(release).encode(), "application/json"
        elif last in files:
            body, kind = files[last], "application/octet-stream"
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("content-type", kind)
        self.send_header("content-length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


print("release up", flush=True)
http.server.ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()
