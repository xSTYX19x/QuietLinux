#!/usr/bin/env bash
# QuietLinux: removes what install.sh installed.
#
#   ./uninstall.sh               programs, menu entry, service, udev rule
#   ./uninstall.sh --keep-udev   leave the udev rule
#   ./uninstall.sh --prefix DIR  as given to install.sh (default: ~/.local)
#
# Your settings in ~/.config/quietlinux stay; delete that folder to remove them
# too. The keyboard keeps what is stored in it (effect, bindings, macros).

set -euo pipefail

prefix="$HOME/.local"
udev=1

while [ $# -gt 0 ]; do
    case "$1" in
        --keep-udev) udev=0 ;;
        --prefix)
            [ $# -ge 2 ] || { echo "--prefix needs a directory" >&2; exit 2; }
            prefix="$2"
            shift
            ;;
        -h | --help)
            sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "unknown option: $1 (see --help)" >&2
            exit 2
            ;;
    esac
    shift
done

if [ "$(id -u)" = 0 ]; then
    echo "run this as your normal user, not with sudo: it removes what was installed for your user and asks sudo only for the udev rule" >&2
    exit 1
fi

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }

unit="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/quietlinux-daemon.service"
if [ -f "$unit" ]; then
    say "Stopping and removing the background service"
    systemctl --user disable --now quietlinux-daemon 2>/dev/null || true
    rm -f "$unit"
    systemctl --user daemon-reload 2>/dev/null || true
fi

say "Removing the programs from $prefix/bin"
rm -f "$prefix/bin/quietlinux" "$prefix/bin/quietlinux-gui" "$prefix/bin/quietlinux-daemon"
rm -f "$prefix/share/applications/quietlinux.desktop" \
    "$prefix/share/icons/hicolor/scalable/apps/quietlinux.svg"
if [ -f "$prefix/share/icons/hicolor/icon-theme.cache" ] && command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -q -f -t "$prefix/share/icons/hicolor" || true
fi
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true

rule=/etc/udev/rules.d/70-quietlinux.rules
if [ "$udev" = 1 ] && [ -f "$rule" ]; then
    say "Removing the udev rule (needs sudo)"
    sudo rm -f "$rule"
    sudo udevadm control --reload
fi

say "Done. Your settings are still in ${XDG_CONFIG_HOME:-$HOME/.config}/quietlinux."
