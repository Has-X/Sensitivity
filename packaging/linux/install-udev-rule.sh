#!/bin/sh
set -eu

rule_name=70-sensitivity.rules
rule_source=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)/$rule_name
rule_target=/etc/udev/rules.d/$rule_name

if [ "$(id -u)" -ne 0 ]; then
    if command -v sudo >/dev/null 2>&1; then
        exec sudo "$0" "$@"
    fi
    echo "Run this installer as root (sudo is not installed)." >&2
    exit 1
fi

if [ "${1-}" = "--uninstall" ]; then
    rm -f "$rule_target"
    echo "Removed $rule_target"
else
    install -m 0644 "$rule_source" "$rule_target"
    echo "Installed $rule_target"
fi

if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules
    udevadm trigger --subsystem-match=usb --attr-match=idVendor=2717 || true
fi

echo "Reconnect the phone before running 'sensitivity doctor'."
