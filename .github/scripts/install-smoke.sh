#!/usr/bin/env bash
# Prove install.sh installs a working noidroid, on this machine, from a release
# it has never seen.
#
#   install-smoke.sh          run from the repository root
#
# Testing an installer against the real GitHub release would test whatever was
# published last, not the code in this branch -- and it could only ever run
# after a release, which is too late to learn the installer is broken. So this
# builds the current source into the exact tarball layout the release workflow
# produces, serves it over localhost, and points install.sh at that.
#
# The checksum is written the way releases up to v0.1.0 wrote it, with a `dist/`
# prefix the installer will never find on disk. That is deliberate: install.sh
# compares hashes rather than running `sha256sum -c`, and this is the case that
# proves it.
set -euo pipefail

PORT="${PORT:-8731}"
VERSION=v0.0.0-smoke

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
target=$(rustc -vV | awk '/^host:/ {print $2}')
name="noidroid-$VERSION-$target"

work=$(mktemp -d)
serve="$work/serve/$VERSION"
mkdir -p "$serve/stage/$name"

cleanup() {
  [ -n "${server_pid:-}" ] && kill "$server_pid" 2>/dev/null
  rm -rf "$work"
}
trap cleanup EXIT INT TERM

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum < "$1" | cut -d' ' -f1
  else
    shasum -a 256 < "$1" | cut -d' ' -f1
  fi
}

echo "building $target"
cargo build --release -p noidroid-cli --quiet

cp "$root/target/release/noidroid" "$serve/stage/$name/"
tar -C "$serve/stage" -czf "$serve/$name.tar.gz" "$name"
rm -rf "$serve/stage"
printf '%s  dist/%s\n' "$(sha256_of "$serve/$name.tar.gz")" "$name.tar.gz" \
  > "$serve/$name.tar.gz.sha256"

python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$work/serve" \
  >/dev/null 2>&1 &
server_pid=$!

for _ in $(seq 1 50); do
  curl -fsS "http://127.0.0.1:$PORT/$VERSION/$name.tar.gz" -o /dev/null 2>/dev/null && break
  sleep 0.2
done

export NOIDROID_BASE_URL="http://127.0.0.1:$PORT"
export NOIDROID_VERSION="$VERSION"
export NOIDROID_INSTALL_DIR="$work/bin"

# 1. A good download installs, and the thing it installs runs.
sh "$root/install.sh"
[ -x "$work/bin/noidroid" ] || { echo "install.sh produced no binary"; exit 1; }
"$work/bin/noidroid" stand | grep -q "PARANOID ANDROID"
"$work/bin/noidroid" --version
echo "ok   - install.sh installs a working noidroid"

# 2. A tarball that does not match its checksum is refused, and nothing is left
#    behind. An installer that fails open is worse than no installer.
rm -f "$work/bin/noidroid"
printf 'corrupted' >> "$serve/$name.tar.gz"
if sh "$root/install.sh" >/dev/null 2>&1; then
  echo "FAIL - install.sh accepted a tarball that failed its checksum"
  exit 1
fi
[ ! -e "$work/bin/noidroid" ] || {
  echo "FAIL - install.sh left a binary behind after refusing the download"
  exit 1
}
echo "ok   - a corrupted download is refused"

# 3. An unsupported platform says so, naming what it saw, rather than 404ing.
out=$(PATH="$work/fake:$PATH" sh -c '
  mkdir -p '"$work"'/fake
  printf "#!/bin/sh\ncase \$1 in -s) echo Plan9 ;; -m) echo pdp11 ;; esac\n" > '"$work"'/fake/uname
  chmod +x '"$work"'/fake/uname
  sh '"$root"'/install.sh 2>&1' ) && status=0 || status=$?
[ "$status" -ne 0 ] || { echo "FAIL - an unsupported platform was not refused"; exit 1; }
grep -q "Plan9 pdp11" <<<"$out" || {
  echo "FAIL - the refusal did not name the platform: $out"; exit 1;
}
echo "ok   - an unsupported platform is named, not 404ed"

echo
echo "install.sh smoke test passed on $target"
