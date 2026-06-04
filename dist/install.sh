#!/usr/bin/env bash
# Install plumbob-daemon as a systemd user service plus udev rule.
# Re-run safely: it's idempotent.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_SRC="${SCRIPT_DIR}/plumbob-daemon"
UDEV_SRC="${SCRIPT_DIR}/70-plumbob.rules"
UNIT_SRC="${SCRIPT_DIR}/plumbob-daemon.service"

BIN_DST="${HOME}/.local/bin/plumbob-daemon"
UNIT_DST="${HOME}/.config/systemd/user/plumbob-daemon.service"
UDEV_DST="/etc/udev/rules.d/70-plumbob.rules"

for f in "$BIN_SRC" "$UDEV_SRC" "$UNIT_SRC"; do
    [[ -f "$f" ]] || { echo "missing: $f" >&2; exit 1; }
done

echo ">> installing binary to ${BIN_DST}"
install -D -m 0755 "$BIN_SRC" "$BIN_DST"

echo ">> installing systemd user unit to ${UNIT_DST}"
install -D -m 0644 "$UNIT_SRC" "$UNIT_DST"

if [[ ! -f "$UDEV_DST" ]] || ! cmp -s "$UDEV_SRC" "$UDEV_DST"; then
    echo ">> installing udev rule to ${UDEV_DST} (needs sudo)"
    sudo install -D -m 0644 "$UDEV_SRC" "$UDEV_DST"
    sudo udevadm control --reload-rules
    sudo udevadm trigger --subsystem-match=hidraw
else
    echo ">> udev rule already up to date"
fi

systemctl --user daemon-reload
systemctl --user enable --now plumbob-daemon.service

echo ""
echo "Done. Status:"
systemctl --user --no-pager status plumbob-daemon.service | head -8 || true
echo ""
echo "Try: curl -X POST http://127.0.0.1:27301/emotion -H 'Content-Type: application/json' -d '{\"emotion\":\"Happy\"}'"
