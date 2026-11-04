"""How long after a moment (date +%s.%N) the app's log first says some words, in ms.
Usage: ms-after.py <moment> <words> <the logs folder>; fails while the log has not said them."""
import datetime
import glob
import sys

moment, words, logs = float(sys.argv[1]), sys.argv[2], sys.argv[3]
for path in sorted(glob.glob(logs + "/reel-desktop.log*")):
    for line in open(path, errors="replace"):
        if words in line:
            t = datetime.datetime.fromisoformat(line.split()[0]).timestamp()
            if t >= moment:
                print(round((t - moment) * 1000))
                sys.exit(0)
sys.exit(1)
