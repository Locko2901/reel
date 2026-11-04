#!/bin/sh
# Slow to close when asked, as OBS stopping its replay buffer and saving is.
/target/debug/examples/fake_obs --port 4466 --dir "$STANDIN_DIR" --clip "$STANDIN_CLIP" &
FAKE=$!
trap 'sleep "${STANDIN_CLOSE_S:-3}"; kill $FAKE; exit 0' TERM
wait $FAKE
