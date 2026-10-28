#!/bin/sh
# After installing or removing the .deb/.rpm: apply the udev rule (or its
# removal) to a Light Mount that is plugged in now. Nothing else is changed.
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload >/dev/null 2>&1 || true
    udevadm trigger --subsystem-match=hidraw >/dev/null 2>&1 || true
fi
exit 0
