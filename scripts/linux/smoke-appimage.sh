#!/usr/bin/env bash
set -euo pipefail
image=$(realpath "${1:?Pass the AppImage path}")
output_dir=$(realpath "${2:?Pass the report directory}")
version=${3:?Pass the expected version}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
test -f "$image"
file "$image"
chmod +x "$image"
work=$(mktemp -d "/opt/ochat-smoke/appimage-$version-XXXXXX")
mkdir -p "$work/data" "$work/config" "$work/cache" "$work/runtime"
chmod 700 "$work/runtime"
chown -R ochat-smoke:ochat-smoke "$work"
# This dedicated account and fresh data/cache directories cannot open user profiles.
runuser -u ochat-smoke -- env \
  GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 APPIMAGE_EXTRACT_AND_RUN=1 \
  XDG_DATA_HOME="$work/data" XDG_CONFIG_HOME="$work/config" XDG_CACHE_HOME="$work/cache" \
  XDG_RUNTIME_DIR="$work/runtime" \
  dbus-run-session -- xvfb-run -a -s '-screen 0 1280x900x24' \
  bash -c '
    set -euo pipefail
    image=$1; work=$2
    timeout 90s "$image" > "$work/application.log" 2>&1 &
    supervisor=$!
    trap '\''kill -TERM "$supervisor" 2>/dev/null || true'\'' EXIT
    found=false
    for attempt in $(seq 1 65); do
      kill -0 "$supervisor"
      if xdotool search --onlyvisible --name "^Ochat$" >/dev/null 2>&1; then found=true; break; fi
      sleep 1
    done
    test "$found" = true
    painted=false
    for attempt in $(seq 1 20); do
      kill -0 "$supervisor"
      import -window root "$work/application.png"
      if test "$(identify -format %k "$work/application.png")" -gt 32; then painted=true; break; fi
      sleep 1
    done
    test "$painted" = true
    kill -TERM "$supervisor"
    set +e
    wait "$supervisor"
    result=$?
    test "$result" = 143 || test "$result" = 124 || test "$result" = 0
  ' bash "$image" "$work"
cp "$work/application.log" "$output_dir/linux-$version-appimage-smoke.log"
cp "$work/application.png" "$output_dir/linux-$version-appimage-smoke.png"
printf 'AppImage rendered in an isolated Linux X11 session: %s\n' "$version"
