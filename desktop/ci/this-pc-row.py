#!/usr/bin/env python3
"""Where to click Revert on one of this PC's rows in Advanced's Settings history, from a shot of
the window: the nth "this PC" badge from the top (1, the newest, by default), found by its
colour, its row's Revert at the table's right. Prints "x y", or nothing when there is none."""
import subprocess
import sys

shot = sys.argv[1]
nth = int(sys.argv[2]) if len(sys.argv) > 2 else 1
revert_x = int(sys.argv[3]) if len(sys.argv) > 3 else 1088
w, h = map(int, subprocess.check_output(["identify", "-format", "%w %h", shot]).split())
rgb = subprocess.check_output(["convert", shot, "-depth", "8", "rgb:-"])
# the badge's background, reel.css's --raised-2 in the dark theme
BADGE = bytes.fromhex("241f2b")
WIDE, HIGH = 30, 12


def px(x, y):
    i = (y * w + x) * 3
    return rgb[i : i + 3]


# a badge's top edge: a run of its colour above its words, then its left edge down from there;
# below the header and the table's head
y, found = 80, 0
while y < h - HIGH:
    row = rgb[y * w * 3 : (y + 1) * w * 3]
    at = row.find(BADGE * WIDE)
    while at != -1 and at % 3:
        at = row.find(BADGE * WIDE, at + 1)
    x = at // 3 + 1
    if at != -1 and all(px(x, y + d) == BADGE for d in range(HIGH)):
        found += 1
        if found == nth:
            print(revert_x, y + 17)
            break
        y += 2 * HIGH
    y += 1
