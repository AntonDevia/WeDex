#!/bin/sh
# Invoked only through an authenticated SSH channel, as root or via sudo.
set -eu
umask 077
stage=$1
hash=$2
version=$3
port=$4
exec 9>/run/lock/wedex-install.lock
flock -n 9 || { echo 'Another WeDex installation is running' >&2; exit 1; }
test "$(id -u)" = 0
test "$(sha256sum "$stage/wedex-server" | cut -d ' ' -f 1)" = "$hash"
test "$("$stage/wedex-server" --version)" = "$version"
for dir in /opt/wedex /etc/wedex /var/lib/wedex; do
    if test -e "$dir" && ! test -f /etc/wedex/install.json; then
        echo "Unmanaged directory: $dir" >&2; exit 1
    fi
    test ! -L "$dir" || { echo "Symlink not allowed: $dir" >&2; exit 1; }
done
if test -f /etc/wedex/server.json; then
    cmp -s "$stage/server.json" /etc/wedex/server.json || {
        echo 'Existing address or port differs; change requires explicit migration' >&2; exit 1;
    }
fi
printf 'WEDEX_STEP storage\n'
if getent passwd wedex >/dev/null && ! test -f /etc/wedex/install.json; then
    echo 'The wedex account already exists and is not managed by this installer' >&2; exit 1
fi
getent passwd wedex >/dev/null || useradd --system --home-dir /var/lib/wedex --shell /usr/sbin/nologin wedex
install -d -m 0755 /opt/wedex /opt/wedex/releases
install -d -m 0750 -o root -g wedex /etc/wedex
if ! test -f /etc/wedex/install.json; then
    printf '{"product":"wedex","schema":1,"state":"installing"}\n' > /etc/wedex/install.json
fi
install -d -m 0700 -o wedex -g wedex /var/lib/wedex
previous=$(readlink /opt/wedex/current || true)
backup=/var/backups/wedex/$(date -u +%Y%m%dT%H%M%SZ)-$$
rollback() {
    code=$?
    if test "$code" != 0 && test -n "$previous"; then
        systemctl stop wedex.service || true
        ln -sfn "$previous" /opt/wedex/current.rollback
        mv -Tf /opt/wedex/current.rollback /opt/wedex/current
        test ! -f "$backup/wedex.service" || cp -p "$backup/wedex.service" /etc/systemd/system/wedex.service
        systemctl daemon-reload || true
        systemctl start wedex.service || true
        echo "Previous executable restored; backup: $backup" >&2
    fi
    exit "$code"
}
trap rollback EXIT
if test -n "$previous"; then
    printf 'WEDEX_STEP backup\n'
    systemctl stop wedex.service
    install -d -m 0700 "$backup"
    cp -a /var/lib/wedex "$backup/data"
    cp -a /etc/wedex "$backup/config"
    test ! -f /etc/systemd/system/wedex.service || cp -p /etc/systemd/system/wedex.service "$backup/wedex.service"
fi
printf 'WEDEX_STEP service\n'
release=/opt/wedex/releases/$version-$hash
install -d -m 0755 "$release"
install -m 0755 "$stage/wedex-server" "$release/wedex-server"
install -m 0640 -o root -g wedex "$stage/server.json" /etc/wedex/server.json
install -m 0644 "$stage/wedex.service" /etc/systemd/system/wedex.service
install -d -m 0700 /var/backups/wedex
install -m 0644 "$stage/wedex-update.service" /etc/systemd/system/wedex-update.service
install -m 0644 "$stage/wedex-update.path" /etc/systemd/system/wedex-update.path
ln -sfn "$release" /opt/wedex/current.next
mv -Tf /opt/wedex/current.next /opt/wedex/current
systemctl daemon-reload
systemctl enable wedex.service >/dev/null
systemctl enable --now wedex-update.path >/dev/null
systemctl restart wedex.service
printf 'WEDEX_STEP health\n'
ready=0
for attempt in 1 2 3 4 5 6 7 8 9 10; do
    if /opt/wedex/current/wedex-server --config /etc/wedex/server.json --invite-json > "$stage/bootstrap.json" 2>/dev/null; then
        ready=1; break
    fi
    sleep 1
done
test "$ready" = 1 || { echo 'Local TLS health check failed; inspect journalctl -u wedex' >&2; exit 1; }
if command -v ufw >/dev/null && ufw status | grep -q '^Status: active'; then
    ufw allow "$port/tcp" comment 'WeDex Cloud' >/dev/null
fi
install -m 0644 "$stage/install.json" /etc/wedex/install.json
printf 'WEDEX_STEP installed\n'
