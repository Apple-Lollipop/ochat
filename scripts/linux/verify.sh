#!/usr/bin/env bash
set -euo pipefail

# This installs into the dedicated build environment, never the Windows host.
output_dir=${1:?Pass the Linux path containing the release packages}
version=${2:?Pass the expected application version}
deb="$output_dir/Ochat_${version}_amd64.deb"
appimage="$output_dir/Ochat_${version}_amd64.AppImage"
test -f "$deb" && test -f "$appimage"
test "$(dpkg-deb --field "$deb" Version)" = "$version"
test "$(dpkg-deb --field "$deb" Architecture)" = amd64
dpkg-deb --info "$deb"
dpkg --install "$deb"
file /usr/bin/ochat
ldd /usr/bin/ochat > "$output_dir/ldd.txt"
if grep -q 'not found' "$output_dir/ldd.txt"; then
  cat "$output_dir/ldd.txt"
  exit 1
fi

chmod +x "$appimage"
install -d -o ochat-smoke -g ochat-smoke /opt/ochat-smoke
install -d -m 700 -o ochat-smoke -g ochat-smoke /opt/ochat-smoke/runtime
for kind in deb appimage; do
  if test "$kind" = deb; then binary=/usr/bin/ochat; else binary=$appimage; fi
  runuser -u ochat-smoke -- env \
    GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 APPIMAGE_EXTRACT_AND_RUN=1 \
    XDG_RUNTIME_DIR=/opt/ochat-smoke/runtime \
    dbus-run-session -- xvfb-run -a -s '-screen 0 1280x900x24' \
    bash -c '
      set -eu
      binary=$1
      kind=$2
      timeout 90s "$binary" > "/opt/ochat-smoke/$kind.log" 2>&1 &
      supervisor=$!
      found=false
      for attempt in $(seq 1 70); do
        kill -0 "$supervisor"
        if xdotool search --onlyvisible --name "^Ochat$" >/dev/null 2>&1; then
          found=true
          break
        fi
        sleep 1
      done
      test "$found" = true
      painted=false
      for frame in $(seq 1 45); do
        kill -0 "$supervisor"
        import -window root "/opt/ochat-smoke/$kind.png"
        colors=$(identify -format %k "/opt/ochat-smoke/$kind.png")
        if test "$colors" -gt 32; then painted=true; break; fi
        sleep 1
      done
      test "$painted" = true
      kill -TERM "$supervisor"
      set +e
      wait "$supervisor"
      status=$?
      test "$status" = 143 || test "$status" = 124 || test "$status" = 0
    ' bash "$binary" "$kind"
  cp "/opt/ochat-smoke/$kind.log" "$output_dir/smoke-$kind.log"
  cp "/opt/ochat-smoke/$kind.png" "$output_dir/smoke-$kind.png"
done
