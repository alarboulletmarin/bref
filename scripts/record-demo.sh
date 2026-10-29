#!/usr/bin/env bash
# Usage: scripts/record-demo.sh [output.gif]   (default: docs/screenshots/demo.gif)
#
# Records the GIF of the README: Bref typing a note on a virtual X screen, in a fresh vault and
# configuration. Linux only; needs Xvfb, xdotool, ffmpeg and a Vulkan driver (Mesa's software
# one, lavapipe, works without a GPU: mesa-vulkan-drivers on Ubuntu).
set -euo pipefail
cd "$(dirname "$0")/.."
out=$(realpath -m "${1:-docs/screenshots/demo.gif}")
cargo build --release --locked
bref=$PWD/target/release/bref
work=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null; rm -rf "$work"' EXIT
mkdir -p "$work/vault" "$work/config/bref" "$work/home" "$work/run"
chmod 700 "$work/run"
echo "$work/vault" > "$work/config/bref/config"
echo "theme=Bref Light" > "$work/config/bref/settings"
printf '# Welcome\n\nNotes are plain Markdown files.\n' > "$work/vault/Welcome.md"

export DISPLAY=:97
Xvfb "$DISPLAY" -screen 0 1280x800x24 &
sleep 2
lavapipe=$(ls /usr/share/vulkan/icd.d/lvp_icd*.json 2>/dev/null | head -1 || true)
if [ -n "$lavapipe" ]; then export VK_ICD_FILENAMES=$lavapipe; fi
XDG_RUNTIME_DIR=$work/run XDG_CONFIG_HOME=$work/config HOME=$work/home LANG=en_US.UTF-8 "$bref" &
sleep 8
window=$(xdotool search --name . | head -1)
xdotool windowmove "$window" 0 0
xdotool windowsize "$window" 1000 640
sleep 1
# A first click draws the window; the pointer then waits out of the way.
xdotool mousemove 500 300 click 1
sleep 1
xdotool mousemove 999 300
sleep 1

ffmpeg -loglevel error -y -f x11grab -framerate 20 -video_size 1000x640 -i "$DISPLAY+0,0" -t 40 "$work/demo.mp4" &
recording=$!
sleep 1.5
write() { xdotool type --delay 55 -- "$1"; }
press() { xdotool key --delay 120 "$@"; }
write '# Trip to Lisbon'; press Return Return; sleep 0.4
write 'Plain **Markdown**, styled as you *type*.'; press Return Return; sleep 0.4
write '- [ ] Book the ==flights=='; press Return; write 'Pack the camera'; press Return Return Return; sleep 0.4
write '| Day | Cost |'; press Tab; sleep 0.6
write 'Friday'; press Tab; write '120'; press Tab; sleep 0.3
write 'Saturday'; press Tab; write '80'; press Tab; sleep 0.3
write 'Total'; press Tab; write '=sum(above)'; sleep 0.5
press Up; sleep 0.6; press Up; sleep 0.6
xdotool mousemove 600 450 click 1; sleep 0.3; xdotool mousemove 999 300; sleep 0.8
press ctrl+Home; sleep 2.5
kill -INT "$recording"; wait "$recording" || true

ffmpeg -loglevel error -y -i "$work/demo.mp4" \
  -vf "fps=12,scale=800:-1:flags=lanczos,palettegen=max_colors=96:stats_mode=diff" "$work/palette.png"
ffmpeg -loglevel error -y -i "$work/demo.mp4" -i "$work/palette.png" \
  -lavfi "fps=12,scale=800:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=none:diff_mode=rectangle" -loop 0 "$out"
echo "$out"
