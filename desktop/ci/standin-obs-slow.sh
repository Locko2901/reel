#!/bin/sh
# A stand-in for reel's OBS that takes STANDIN_CLOSE_S seconds to close when asked, as OBS
# stopping its replay buffer and saving does.
/target/debug/examples/fake_obs --port 4466 --dir "$STANDIN_DIR" --clip "$STANDIN_CLIP" &
FAKE=$!
trap 'sleep "${STANDIN_CLOSE_S:-3}"; kill $FAKE; exit 0' TERM
wait $FAKE
