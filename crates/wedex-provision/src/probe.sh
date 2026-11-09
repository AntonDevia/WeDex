set -eu
. /etc/os-release
printf 'OS=%s\nVERSION=%s\nARCH=%s\n' "$ID" "$VERSION_ID" "$(uname -m)"
printf 'DISK_KB=%s\n' "$(df -Pk /var | awk 'NR==2 {print $4}')"
if test -d /run/systemd/system && command -v systemctl >/dev/null; then printf 'SYSTEMD=yes\n'; else printf 'SYSTEMD=no\n'; fi
for utility in ss sha256sum install useradd flock; do command -v "$utility" >/dev/null || { echo "Missing utility: $utility" >&2; exit 1; }; done
if test -f /etc/wedex/install.json && grep -q '"product"[[:space:]]*:[[:space:]]*"wedex"' /etc/wedex/install.json; then
    printf 'MANAGED=yes\n'
    if test -x /opt/wedex/current/wedex-server; then printf 'VERSION_INSTALLED=%s\n' "$(/opt/wedex/current/wedex-server --version)"; fi
elif test -e /opt/wedex || test -e /etc/wedex || test -e /var/lib/wedex || systemctl cat wedex.service >/dev/null 2>&1; then
    printf 'MANAGED=foreign\n'
else printf 'MANAGED=no\n'; fi
