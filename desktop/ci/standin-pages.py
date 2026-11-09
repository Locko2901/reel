#!/usr/bin/env python3
"""A stand-in server for the app's window: reel's own page scripts at /, with one part that
throws and the last part saying it ran, and at /broken a page whose script is missing.

standin-pages.py <port> <reel's web/assets folder>; each request is a line on stdout.
"""
import http.server
import os
import sys

PORT = int(sys.argv[1])
ASSETS = os.path.realpath(sys.argv[2])
PAGE = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>reel</title></head>
<body><main><h1>{title}</h1></main><script type="module" src="{script}"></script></body></html>
"""
STUBS = {
    "/assets/js/library.js": "export function library() { throw new Error('a part that throws, on purpose'); }\n",
    "/assets/js/share-target.js": "export function shareTarget() {}\nexport function pwa() { fetch('/the-last-part-ran'); }\n",
}


class Pages(http.server.BaseHTTPRequestHandler):
    def send(self, status, kind, body):
        data = body.encode()
        self.send_response(status)
        self.send_header("content-type", kind)
        self.send_header("content-length", str(len(data)))
        self.send_header("cache-control", "no-store")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        path = self.path.split("?")[0]
        if path == "/":
            return self.send(200, "text/html", PAGE.format(title="a part throws", script="/assets/reel.js"))
        if path == "/broken":
            return self.send(200, "text/html", PAGE.format(title="a script is missing", script="/assets/missing.js"))
        if path == "/the-last-part-ran":
            return self.send(200, "text/plain", "heard")
        if path in STUBS:
            return self.send(200, "text/javascript", STUBS[path])
        if path.startswith("/assets/") and path.endswith(".js"):
            file = os.path.realpath(os.path.join(ASSETS, path[len("/assets/"):]))
            if file.startswith(ASSETS + os.sep) and os.path.isfile(file):
                with open(file, encoding="utf-8") as f:
                    return self.send(200, "text/javascript", f.read())
        return self.send(404, "text/plain", "not here")

    def log_message(self, fmt, *args):
        print(f"GET {self.path} {args[1] if len(args) > 1 else ''}", flush=True)


http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Pages).serve_forever()
