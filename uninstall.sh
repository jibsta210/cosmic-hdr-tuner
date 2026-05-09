#!/usr/bin/env bash
# cosmic-hdr-tuner uninstaller — reverses install.sh.
#
# Removes:
#   - /usr/local/bin/cosmic-comp-hdr
#   - /usr/local/bin/cosmic-hdr-tuner
#   - /usr/share/applications/cosmic-hdr-tuner.desktop
#   - /usr/share/wayland-sessions/cosmic-hdr.desktop
#
# Optionally also removes:
#   - ~/.local/state/cosmic-comp-hdr/  (HDR-session state, with --purge-state)
#   - ~/.local/share/cosmic-hdr-tuner/ (cloned source, with --purge-source)
#
# Usage:
#   ./uninstall.sh                        # remove installed binaries + .desktop files only
#   ./uninstall.sh --purge-state          # also remove HDR-session state
#   ./uninstall.sh --purge-source         # also remove cloned source tree
#   ./uninstall.sh --purge-state --purge-source

set -euo pipefail

PURGE_STATE=0
PURGE_SOURCE=0
for arg in "$@"; do
    case "$arg" in
        --purge-state) PURGE_STATE=1 ;;
        --purge-source) PURGE_SOURCE=1 ;;
        -h|--help)
            sed -n '2,/^set -euo/p' "$0" | sed 's/^# \?//' | head -n -2
            exit 0
            ;;
        *) echo "unknown arg: $arg (use --help)"; exit 1 ;;
    esac
done

RED=$'\e[31m'; GREEN=$'\e[32m'; BLUE=$'\e[34m'; YELLOW=$'\e[33m'; RESET=$'\e[0m'
log()  { printf "%s==>%s %s\n" "$BLUE"  "$RESET" "$*"; }
warn() { printf "%s==>%s %s\n" "$YELLOW" "$RESET" "$*" >&2; }
ok()   { printf "%s==>%s %s\n" "$GREEN" "$RESET" "$*"; }

# refuse if cosmic-comp-hdr is currently running — they'd lose their session
if pgrep -x cosmic-comp >/dev/null 2>&1; then
    warn "cosmic-comp is currently running. Log out of any HDR session first,"
    warn "then re-run this script. (We won't kill it ourselves — refusing.)"
    exit 1
fi

log "Removing installed binaries (sudo)"
sudo rm -f /usr/local/bin/cosmic-comp-hdr
sudo rm -f /usr/local/bin/cosmic-hdr-tuner

log "Removing .desktop entries"
sudo rm -f /usr/share/applications/cosmic-hdr-tuner.desktop
sudo rm -f /usr/share/wayland-sessions/cosmic-hdr.desktop

# refresh desktop database so the launcher entry disappears immediately
sudo update-desktop-database /usr/share/applications 2>/dev/null || true

if [ "$PURGE_STATE" -eq 1 ]; then
    log "Purging HDR-session state (~/.local/state/cosmic-comp-hdr/)"
    rm -rf "$HOME/.local/state/cosmic-comp-hdr"
fi

if [ "$PURGE_SOURCE" -eq 1 ]; then
    log "Purging cloned source (~/.local/share/cosmic-hdr-tuner/)"
    rm -rf "$HOME/.local/share/cosmic-hdr-tuner"
fi

ok ""
ok "Uninstalled."
[ "$PURGE_STATE" -eq 0 ]  && ok "  HDR-session state preserved at ~/.local/state/cosmic-comp-hdr/"
[ "$PURGE_STATE" -eq 0 ]  && ok "    (re-running install.sh later will pick it up.)"
[ "$PURGE_SOURCE" -eq 0 ] && ok "  Source tree preserved at ~/.local/share/cosmic-hdr-tuner/"
[ "$PURGE_SOURCE" -eq 0 ] && ok "    (re-running install.sh --rebuild will skip the clone step.)"
ok ""
ok "  Vanilla cosmic-comp is unaffected — the HDR fork wrote to its own"
ok "  isolated state path so your regular session config is untouched."
