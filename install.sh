#!/usr/bin/env bash
# QuietLinux installer: builds the programs and installs them for your user.
#
#   ./install.sh                 command line tool, window, udev rule
#   ./install.sh --service       also start the background service now and at login
#   ./install.sh --no-gui        without the window (needs no Rust 1.92, no fontconfig)
#   ./install.sh --no-udev       without the udev rule (if you installed it yourself)
#   ./install.sh --prefix DIR    install to DIR/bin etc. (default: ~/.local)
#
# Only the udev rule needs root: the script asks sudo for that one step.
# ./uninstall.sh removes everything again.

set -euo pipefail

cd "$(dirname "$0")"

prefix="$HOME/.local"
gui=1
udev=1
service=0

while [ $# -gt 0 ]; do
    case "$1" in
        --service) service=1 ;;
        --no-gui) gui=0 ;;
        --no-udev) udev=0 ;;
        --prefix)
            [ $# -ge 2 ] || { echo "--prefix needs a directory" >&2; exit 2; }
            prefix="$2"
            shift
            ;;
        -h | --help)
            sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "unknown option: $1 (see --help)" >&2
            exit 2
            ;;
    esac
    shift
done

say() { printf '\033[1m==> %s\033[0m\n' "$*"; }
warn() { printf '\033[33mnote: %s\033[0m\n' "$*" >&2; }

# --- requirements -----------------------------------------------------------------

command -v cargo >/dev/null || {
    echo "cargo not found: install Rust (https://rustup.rs or your distribution's rust package)" >&2
    exit 1
}

if [ "$gui" = 1 ]; then
    rust_minor=$(rustc --version | sed -E 's/^rustc 1\.([0-9]+).*/\1/')
    if [ "${rust_minor:-0}" -lt 92 ]; then
        warn "the window needs Rust 1.92 or newer ($(rustc --version)): installing without it"
        gui=0
    elif ! pkg-config --exists fontconfig 2>/dev/null; then
        warn "the window needs the fontconfig headers and pkg-config (Debian/Ubuntu: libfontconfig-dev pkg-config, Fedora: fontconfig-devel, Arch: fontconfig pkgconf): installing without it"
        gui=0
    fi
fi

# --- build ------------------------------------------------------------------------

packages=(-p quietlinux-cli -p quietlinux-daemon)
[ "$gui" = 1 ] && packages+=(-p quietlinux-gui)
say "Building (cargo build --release; the first build takes a few minutes)"
cargo build --release --locked "${packages[@]}"

# --- install ----------------------------------------------------------------------

bin="$prefix/bin"
say "Installing to $bin"
install -Dm755 target/release/quietlinux "$bin/quietlinux"
install -Dm755 target/release/quietlinux-daemon "$bin/quietlinux-daemon"
if [ "$gui" = 1 ]; then
    install -Dm755 target/release/quietlinux-gui "$bin/quietlinux-gui"
    say "Adding QuietLinux to the application menu"
    install -Dm644 packaging/desktop/quietlinux.desktop "$prefix/share/applications/quietlinux.desktop"
    install -Dm644 crates/quietlinux-gui/ui/icon.svg "$prefix/share/icons/hicolor/scalable/apps/quietlinux.svg"
    command -v update-desktop-database >/dev/null && update-desktop-database -q "$prefix/share/applications" || true
fi

# the service runs from $bin
unit="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/quietlinux-daemon.service"
install -Dm644 packaging/systemd/quietlinux-daemon.service "$unit"
sed -i "s|^ExecStart=.*|ExecStart=$bin/quietlinux-daemon|" "$unit"

if [ "$udev" = 1 ]; then
    rule=/etc/udev/rules.d/70-quietlinux.rules
    if cmp -s packaging/udev/70-quietlinux.rules "$rule" 2>/dev/null; then
        say "udev rule already installed"
    else
        say "Installing the udev rule to $rule (needs sudo)"
        sudo install -m 644 packaging/udev/70-quietlinux.rules "$rule"
        sudo udevadm control --reload
        sudo udevadm trigger --subsystem-match=hidraw
        warn "replug the keyboard (or log out and in) so that the rule applies"
    fi
fi

if command -v systemctl >/dev/null; then
    systemctl --user daemon-reload || true
    if [ "$service" = 1 ]; then
        say "Starting the background service (now and at login)"
        systemctl --user enable --now quietlinux-daemon
    fi
fi

case ":$PATH:" in
    *":$bin:"*) ;;
    *) warn "$bin is not in your PATH: add it, or call the programs with their full path" ;;
esac

say "Done"
echo "  quietlinux status          check that the keyboard is found"
[ "$gui" = 1 ] && echo "  quietlinux-gui             the window (also in the application menu)"
if [ "$service" = 0 ]; then
    echo "  systemctl --user enable --now quietlinux-daemon"
    echo "                             the background service (per-key colors after replug/sleep, website keys)"
fi
