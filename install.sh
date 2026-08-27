#!/bin/sh
# Install the `noidroid` command.
#
#   curl -fsSL https://raw.githubusercontent.com/swalla02/noidroid/main/install.sh | sh
#
# Downloads the release binary for this machine, checks it against the SHA-256
# published beside it, and puts it on your PATH. Nothing is compiled, and nothing
# outside the install directory is touched.
#
# Environment:
#   NOIDROID_VERSION       the tag to install, e.g. v0.4.0. Default: the latest release.
#   NOIDROID_INSTALL_DIR   where the binary goes. Default: ~/.local/bin.
#   NOIDROID_BASE_URL      where releases are served from. Default: GitHub. Set this
#                          to install from a mirror, and to test this script against
#                          a build that is not published yet.
#
# POSIX sh on purpose: `curl … | sh` runs under whatever /bin/sh is, which on
# Debian and Ubuntu is dash, not bash.
set -eu

REPO="swalla02/noidroid"
INSTALL_DIR="${NOIDROID_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
die() { printf 'install.sh: %s\n' "$*" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || die "this needs $1, which is not on your PATH."
}

# Which build to fetch. An unsupported pair has to say what it saw and where to
# go next -- the alternative is a 404 from curl, which tells the reader nothing.
target_for() {
  os=$(uname -s)
  arch=$(uname -m)
  case "$os $arch" in
    'Linux x86_64')            echo x86_64-unknown-linux-gnu ;;
    'Linux aarch64' | 'Linux arm64') echo aarch64-unknown-linux-gnu ;;
    'Darwin arm64')            echo aarch64-apple-darwin ;;
    'Darwin x86_64')           echo x86_64-apple-darwin ;;
    *)
      die "no prebuilt binary for $os $arch.
    Builds exist for x86_64 and arm64 on Linux and macOS.
    To build from source: https://github.com/$REPO#quickstart"
      ;;
  esac
}

latest_tag() {
  # The API answers without a token at 60 requests/hour per IP, which is plenty
  # for installing. `grep -m1` reads the first tag_name rather than adding a jq
  # dependency to a script whose whole point is having no dependencies.
  curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep -m1 '"tag_name"' \
    | cut -d'"' -f4
}

# Hash $1 with whichever tool this machine has: sha256sum on Linux, shasum on
# macOS. Both read stdin, which keeps the file name out of the output.
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum < "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 < "$1" | cut -d' ' -f1
  else
    die "this needs sha256sum or shasum to check the download."
  fi
}

# Compare the tarball ($1) against the published checksum file ($2).
#
# Deliberately not `sha256sum -c`: that trusts the file name recorded in the
# checksum, and the releases up to v0.1.0 recorded `dist/<file>`, a path that
# exists on the build runner and nowhere else. Taking the hash field alone and
# comparing it ourselves verifies the same thing and cannot be broken by how the
# release happened to write the line.
verify() {
  expected=$(cut -d' ' -f1 < "$2")
  case "$expected" in
    [0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]*) ;;
    *) die "the published checksum for $name.tar.gz is not a SHA-256." ;;
  esac
  [ "${#expected}" -eq 64 ] ||
    die "the published checksum for $name.tar.gz is not a SHA-256."
  [ "$expected" = "$(sha256_of "$1")" ]
}

need curl
need tar

target=$(target_for)

version="${NOIDROID_VERSION:-}"
if [ -z "$version" ]; then
  version=$(latest_tag) || die "could not reach the GitHub releases API."
  [ -n "$version" ] || die "could not work out the latest release of $REPO."
fi

name="noidroid-$version-$target"
base="${NOIDROID_BASE_URL:-https://github.com/$REPO/releases/download}"
url="$base/$version"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "installing noidroid $version ($target)"

curl -fsSL -o "$tmp/$name.tar.gz" "$url/$name.tar.gz" ||
  die "no download for $target at $version.
    Check https://github.com/$REPO/releases for what that release shipped."
curl -fsSL -o "$tmp/$name.tar.gz.sha256" "$url/$name.tar.gz.sha256" ||
  die "the release has no checksum for $name.tar.gz. Refusing to install it."

verify "$tmp/$name.tar.gz" "$tmp/$name.tar.gz.sha256" ||
  die "$name.tar.gz does not match its published SHA-256. Refusing to install it."

tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
[ -f "$tmp/$name/noidroid" ] || die "$name.tar.gz did not contain a noidroid binary."

mkdir -p "$INSTALL_DIR"
# Move into place via a temporary name in the same directory, so an install over
# a running noidroid replaces the file rather than writing through it.
mv "$tmp/$name/noidroid" "$INSTALL_DIR/.noidroid.$$"
chmod 755 "$INSTALL_DIR/.noidroid.$$"
mv "$INSTALL_DIR/.noidroid.$$" "$INSTALL_DIR/noidroid"

say "installed $INSTALL_DIR/noidroid"

case ":$PATH:" in
  *":$INSTALL_DIR:"*)
    say ""
    say "Try:  noidroid stand"
    ;;
  *)
    say ""
    say "$INSTALL_DIR is not on your PATH. Add it:"
    say ""
    say "    export PATH=\"$INSTALL_DIR:\$PATH\""
    say ""
    say "Then:  noidroid stand"
    ;;
esac
