#!/usr/bin/env python3
"""A made up Steam folder for the app's checks: made-up-steam.py <steam> <second library> [--version N].

Two libraries of made up apps, each with its manifest and folder, Steam's appinfo.vdf of version
41 (or N, any number for one reel cannot read) with each app's type, and one program added to
Steam by hand. What reel should make of it: 7 games, 2 not sorted yet (an application and the
program added by hand), 3 left out (a runtime, a soundtrack, a Proton build), 1 still installing.
"""
import struct
import sys
from pathlib import Path

FIRST = [
    (1957780, "Cinder Road", "Cinder Road", "Game"),
    (1402310, "Lantern Bay", "Lantern Bay", "Game"),
    (1883250, "Ridgeback Rally", "Ridgeback", "Game"),
    (1628350, "Steam Linux Runtime 3.0 (sniper)", "SteamLinuxRuntime_sniper", "Tool"),
    (2004450, "Cinder Road Soundtrack", "Cinder Road Soundtrack", "Music"),
]
SECOND = [
    (2610450, "Hollow Pines", "Hollow Pines", "Game"),
    (2236710, "Tidewater", "Tidewater", "Game"),
    (2817340, "Starfall Tactics: The Definitive Remastered Collection", "Starfall Tactics DRC", "Game"),
    (2391820, "Desk Painter", "Desk Painter", "Application"),
    (2484120, "Quarry Kings", "Quarry Kings", "Game"),
    (2900010, "Half There", "Half There", "Game"),
    (2805730, "Proton 9.0", "Proton 9.0", "Tool"),
]
INSTALLING = {2900010}


def manifest(app_id, name, folder, flags):
    return (f'"AppState"\n{{\n\t"appid"\t\t"{app_id}"\n\t"name"\t\t"{name}"\n'
            f'\t"StateFlags"\t\t"{flags}"\n\t"installdir"\t\t"{folder}"\n}}\n')


def library(root, apps):
    sa = root / "steamapps"
    (sa / "common").mkdir(parents=True, exist_ok=True)
    for app_id, name, folder, kind in apps:
        flags = 1026 if app_id in INSTALLING else 4
        (sa / f"appmanifest_{app_id}.acf").write_text(manifest(app_id, name, folder, flags))
        (sa / ("music" if kind == "Music" else "common") / folder).mkdir(parents=True, exist_ok=True)


def appinfo(version, apps):
    strings = []

    def key(k):
        if version >= 41:
            if k not in strings:
                strings.append(k)
            return struct.pack("<i", strings.index(k))
        return k.encode() + b"\0"

    body = b""
    for app_id, name, _, kind in apps:
        vdf = (b"\x00" + key("appinfo") + b"\x02" + key("appid") + struct.pack("<i", app_id)
               + b"\x00" + key("common") + b"\x01" + key("name") + name.encode() + b"\0"
               + b"\x01" + key("type") + kind.encode() + b"\0" + b"\x08" + b"\x08" + b"\x08")
        body += struct.pack("<II", app_id, 60 + len(vdf)) + b"\0" * 60 + vdf
    body += struct.pack("<I", 0)
    head = struct.pack("<II", 0x07564400 | version, 1)
    if version < 41:
        return head + body
    table = struct.pack("<I", len(strings)) + b"".join(s.encode() + b"\0" for s in strings)
    return head + struct.pack("<q", len(head) + 8 + len(body)) + body + table


def shortcuts(steam, items):
    b = b"\x00shortcuts\x00"
    for i, (app_id, name, exe) in enumerate(items):
        b += (b"\x00" + str(i).encode() + b"\0" + b"\x02appid\x00" + struct.pack("<I", app_id)
              + b"\x01AppName\x00" + name.encode() + b"\0" + b'\x01Exe\x00"' + exe.encode() + b'"\0'
              + b"\x01StartDir\x00\x00\x00tags\x00\x08\x08")
    b += b"\x08\x08"
    d = steam / "userdata" / "123123123" / "config"
    d.mkdir(parents=True, exist_ok=True)
    (d / "shortcuts.vdf").write_bytes(b)


def main():
    steam, second = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    version = int(sys.argv[sys.argv.index("--version") + 1]) if "--version" in sys.argv else 41
    library(steam, FIRST)
    library(second, SECOND)
    (steam / "steamapps" / "libraryfolders.vdf").write_text(
        '"libraryfolders"\n{\n'
        f'\t"0"\n\t{{\n\t\t"path"\t\t"{steam}"\n\t}}\n'
        f'\t"1"\n\t{{\n\t\t"path"\t\t"{second}"\n\t}}\n}}\n')
    (steam / "appcache").mkdir(exist_ok=True)
    (steam / "appcache" / "appinfo.vdf").write_bytes(appinfo(version, FIRST + SECOND))
    racer = steam.parent / "Games" / "Moonlit Racer" / "racer_x64"
    racer.parent.mkdir(parents=True, exist_ok=True)
    racer.write_text("")
    shortcuts(steam, [(3000000001, "Moonlit Racer", str(racer))])


if __name__ == "__main__":
    main()
