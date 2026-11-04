#!/usr/bin/env python3
# Sends obs-websocket requests to the OBS on this machine; fails on the first refused request.
# Usage: python3 desktop/ci/obs-ws.py <port> <password> <RequestType> '<json data>' [...]
import base64
import hashlib
import json
import os
import socket
import struct
import sys


def b64sha(s):
    return base64.b64encode(hashlib.sha256(s.encode()).digest()).decode()


class Ws:
    def __init__(self, port):
        self.s = socket.create_connection(("127.0.0.1", port), timeout=10)
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall(
            (
                f"GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\n"
                f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n"
                "Sec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: obswebsocket.json\r\n\r\n"
            ).encode()
        )
        buf = b""
        while b"\r\n\r\n" not in buf:
            chunk = self.s.recv(4096)
            if not chunk:
                raise SystemExit("OBS closed the connection before the handshake")
            buf += chunk
        head, self.buf = buf.split(b"\r\n\r\n", 1)
        if b" 101 " not in head.split(b"\r\n")[0]:
            raise SystemExit(f"no WebSocket: {head!r}")

    def _take(self, n):
        while len(self.buf) < n:
            chunk = self.s.recv(65536)
            if not chunk:
                raise SystemExit("OBS closed the connection")
            self.buf += chunk
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def recv(self):
        data = b""
        while True:
            b0, b1 = self._take(2)
            n = b1 & 0x7F
            if n == 126:
                n = struct.unpack("!H", self._take(2))[0]
            elif n == 127:
                n = struct.unpack("!Q", self._take(8))[0]
            payload = self._take(n)
            op = b0 & 0x0F
            if op == 9:
                self.send(payload, 0xA)
                continue
            if op == 8:
                raise SystemExit(f"OBS closed the connection: {payload!r}")
            data += payload
            if b0 & 0x80:
                return json.loads(data)

    def send(self, data, op=0x1):
        if isinstance(data, str):
            data = data.encode()
        mask = os.urandom(4)
        n = len(data)
        if n < 126:
            head = struct.pack("!BB", 0x80 | op, 0x80 | n)
        elif n < 65536:
            head = struct.pack("!BBH", 0x80 | op, 0x80 | 126, n)
        else:
            head = struct.pack("!BBQ", 0x80 | op, 0x80 | 127, n)
        self.s.sendall(head + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))


def main():
    port, password, pairs = int(sys.argv[1]), sys.argv[2], sys.argv[3:]
    ws = Ws(port)
    hello = ws.recv()["d"]
    ident = {"rpcVersion": 1, "eventSubscriptions": 0}
    auth = hello.get("authentication")
    if auth:
        ident["authentication"] = b64sha(b64sha(password + auth["salt"]) + auth["challenge"])
    ws.send(json.dumps({"op": 1, "d": ident}))
    if ws.recv()["op"] != 2:
        raise SystemExit("OBS did not identify the client")
    for i in range(0, len(pairs), 2):
        kind, data = pairs[i], json.loads(pairs[i + 1])
        ws.send(json.dumps({"op": 6, "d": {"requestType": kind, "requestId": str(i), "requestData": data}}))
        while True:
            msg = ws.recv()
            if msg["op"] == 7 and msg["d"]["requestId"] == str(i):
                break
        st = msg["d"]["requestStatus"]
        print(kind, json.dumps(msg["d"].get("responseData", {})))
        if not st["result"]:
            raise SystemExit(f"{kind} refused: {st.get('code')} {st.get('comment', '')}")


main()
