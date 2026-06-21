#!/usr/bin/env bash
# Regenerate the demo screenshots in docs/images/ from the bundled, offline
# snapshots in assets/snapshots/. Reproducible: same input -> same output, no
# network needed. Run from the repo root:  ./scripts/render-demos.sh
set -euo pipefail

cd "$(dirname "$0")/.."
cargo build --release --quiet
ROBIN="cargo run --release --quiet --"

mkdir -p docs/images

# Hacker News: the top of the front page.
$ROBIN assets/snapshots/hackernews.html \
    --png docs/images/hackernews.png --width 900 --clip-height 760

# Wikipedia: skip the (long, no-float) nav + table of contents and screenshot
# the article lead. The offset is stable because the snapshot is committed.
$ROBIN assets/snapshots/wikipedia.html \
    --png docs/images/wikipedia.png --width 1000 --clip-top 10950 --clip-height 820

# Google's no-JS landing page.
$ROBIN assets/snapshots/google.html \
    --png docs/images/google.png --width 900 --clip-height 480

echo "Wrote docs/images/{hackernews,wikipedia,google}.png"
