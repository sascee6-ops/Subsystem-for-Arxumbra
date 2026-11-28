#!/usr/bin/env bash
set -u
BIN=$(readlink -f "$1")
HERE=$(dirname "$(readlink -f "$0")")
W=$(mktemp -d)
cd "$W"
fail=0

check() {
  if "$@"; then echo "PASS: $*"; else echo "FAIL: $*"; fail=1; fi
}

sudo modprobe vkms
sudo modprobe uinput
sudo mount -t debugfs none /sys/kernel/debug 2>/dev/null
CARD=
IDX=
for c in /sys/class/drm/card[0-9]*; do
  case "$c" in *-*) continue ;; esac
  if [[ $(readlink -f "$c/device") == *vkms* ]]; then
    IDX=${c##*card}
    CARD=/dev/dri/card$IDX
  fi
done
echo "vkms device: $CARD (minor $IDX)"
check test -n "$CARD"
D=/sys/kernel/debug/dri/$IDX/crtc-0/crc

sudo seatd -g video -l debug > seatd.log 2>&1 &
sleep 1
mkfifo cmd
(sudo python3 "$HERE/uinput.py" < cmd > uinput.log 2>&1 &)
exec 3> cmd
for i in $(seq 1 50); do grep -q ready uinput.log && break; sleep 0.1; done
sudo udevadm settle
sleep 1

sudo mkdir -p /tmp/xdg
sudo chmod 700 /tmp/xdg
sudo env XDG_RUNTIME_DIR=/tmp/xdg LIBSEAT_BACKEND=seatd COMPOSITOR_DRM_DEVICE=$CARD RUST_BACKTRACE=1 "$BIN" --drm > comp.log 2>&1 &
CP=$!
sleep 6
check kill -0 $CP
check sudo test -S /tmp/xdg/wayland-1

crc() {
  sudo timeout 2 cat $D/data 2>/dev/null | awk '{$1=""; print}' | sort -u
}
echo auto | sudo tee $D/control > /dev/null
A=$(crc)
echo "move 5 5 80" >&3
sleep 2
B=$(crc)
sudo env XDG_RUNTIME_DIR=/tmp/xdg WAYLAND_DISPLAY=wayland-1 weston-simple-shm > client.log 2>&1 &
sleep 4
C=$(crc)
echo "--- crc idle:";    echo "$A"
echo "--- crc pointer moved:"; echo "$B"
echo "--- crc with client:";   echo "$C"
check test -n "$A"
check test "$(echo "$A" | wc -l)" -eq 1
check test -n "$B"
check test "$A" != "$B"
check test "$(echo "$B" | wc -l)" -eq 1
check test "$C" != "$B"
check pgrep -x weston-simple-s

echo "key ctrl alt bs" >&3
for i in $(seq 1 60); do kill -0 $CP 2>/dev/null || break; sleep 0.1; done
check bash -c "! kill -0 $CP 2>/dev/null"
wait $CP
RC=$?
check test "$RC" -eq 0

sudo pkill weston-simple-s
sudo pkill seatd
sudo pkill -f uinput.py
echo "=== compositor log"; cat comp.log
echo "=== client log"; head -20 client.log
echo "=== seatd log"; tail -30 seatd.log
echo "=== uinput log"; cat uinput.log
echo "=== dmesg"; sudo dmesg | tail -20
exit $fail
