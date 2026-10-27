#!/usr/bin/env bash
# Run only after staging and smoke-testing the Linux deliverables.
set -euo pipefail
project=${1:?Pass the mounted Ochat project directory}
version=${2:?Pass the staged version}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
project=$(realpath "$project")
test -f "$project/Cargo.toml"
for suffix in deb AppImage; do test -s "$project/releases/Linux/Ochat_${version}_amd64.$suffix"; done
test -s "$project/releases/reports/linux-${version}-appimage-smoke.png"
export CARGO_HOME=/usr/local/cargo RUSTUP_HOME=/usr/local/rustup
export PATH=/opt/ochat-node/bin:/usr/local/cargo/bin:$PATH
build_source="/opt/ochat-source-$version"
test "$(realpath "$build_source")" = "$build_source"
test "$(realpath /opt/ochat-target)" = /opt/ochat-target
cargo clean --manifest-path "$build_source/Cargo.toml" --target-dir /opt/ochat-target
for directory in /opt/ochat-source /opt/ochat-source-*; do
  [[ -d "$directory" && ! -L "$directory" && -f "$directory/package.json" ]] || continue
  [[ "$directory" =~ ^/opt/ochat-source(-[0-9]+\.[0-9]+\.[0-9]+)?$ ]] || continue
  test "$(realpath "$directory")" = "$directory"
  npm --prefix "$directory" prune --omit=dev --ignore-scripts --no-audit --no-fund
done
python3 - <<'PY'
import pathlib,re,shutil
root=pathlib.Path('/tmp')
for directory in root.iterdir():
    if not re.fullmatch(r'appimage_extracted_[a-f0-9]{32}',directory.name): continue
    if directory.is_symlink() or not directory.is_dir() or directory.resolve()!=directory: continue
    if not (directory/'usr/bin/ochat').is_file(): continue
    # Preserve any extraction still mapped into a running process.
    active=False
    for process in pathlib.Path('/proc').iterdir():
        if not process.name.isdigit(): continue
        try:
            if str(directory)+'/' in (process/'maps').read_text(errors='replace'): active=True;break
        except (OSError,PermissionError): pass
    if active:
        print('Preserved active extraction:',directory);continue
    print('Removing inactive Ochat AppImage extraction:',directory)
    shutil.rmtree(directory)
# This Cargo home belongs to the dedicated Ochat build distribution only.
# The compiler stays installed; dependencies will be downloaded again on the next build.
for name in ('cache','src','index'):
    directory=pathlib.Path('/usr/local/cargo/registry')/name
    if directory.is_dir() and not directory.is_symlink() and directory.resolve()==directory:
        print('Removing rebuildable Cargo dependency cache:',directory)
        shutil.rmtree(directory)
PY
apt-get clean
sync
fstrim -v / || true
df -h /
du -sh /opt/ochat-target /tmp /usr/local/cargo/registry 2>/dev/null || true
