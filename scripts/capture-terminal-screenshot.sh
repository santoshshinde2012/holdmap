#!/usr/bin/env bash
# Captures one terminal screenshot (CLI or TUI) for docs/screenshots/ on Linux with X11.
#
#   scripts/demo-servers.sh start
#   cargo build -p portwise
#   scripts/capture-terminal-screenshot.sh NAME COLS ROWS "command" [key ...]
#
# Example: scripts/capture-terminal-screenshot.sh tui-help-dark 150 42 "portwise" question
#
# Keys are xdotool key names; `type:TEXT` types text and `sleep:SECS` pauses. Needs xterm,
# xdotool and ImageMagick (`import`), and an X display (e.g. `Xvfb :99 &`, DISPLAY=:99).
# Terminal shots use the dark xterm palette, so name them <surface>-<view>-dark.png.
set -euo pipefail
NAME=$1 COLS=$2 ROWS=$3 CMD=$4
shift 4
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${OUT_DIR:-$ROOT/docs/screenshots}
export DISPLAY=${DISPLAY:-:99}
xterm -T pwterm -geometry "${COLS}x${ROWS}+0+0" -fa "DejaVu Sans Mono" -fs 11 -bg "#0d1117" -fg "#c9d1d9" +sb \
  -xrm 'XTerm*allowSendEvents:true' -xrm 'XTerm*borderWidth:0' -xrm 'XTerm*internalBorder:12' \
  -e bash -c "printf '\\e[?25l'; unset NO_COLOR; export PATH=$ROOT/target/debug:\$PATH TERM=xterm-256color COLORTERM=truecolor; cd $ROOT; $CMD; sleep 60" &
XTERM_PID=$!
trap 'kill $XTERM_PID 2>/dev/null || true' EXIT
sleep 2.5
WIN=$(xdotool search --name '^pwterm$' | head -1)
for k in "$@"; do
  case $k in
    sleep:*) sleep "${k#sleep:}" ;;
    type:*) xdotool type --window "$WIN" --delay 40 "${k#type:}" ;;
    *) xdotool key --window "$WIN" "$k" ;;
  esac
  sleep 0.4
done
sleep 1.5
import -window "$WIN" "$OUT/$NAME.png"
echo "$OUT/$NAME.png"
