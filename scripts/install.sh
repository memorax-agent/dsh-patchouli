#!/bin/sh
set -eu

repository="memorax-agent/dsh-patchouli"
version="${PATCHOULI_VERSION:-latest}"
install_dir="${PATCHOULI_INSTALL_DIR:-${HOME:?HOME is required}/.local/bin}"
patchouli_home="${PATCHOULI_HOME:-${HOME:?HOME is required}/.patchouli}"

case "$(uname -s)" in
  Linux) platform="linux" ;;
  Darwin) platform="macos" ;;
  *) echo "unsupported operating system: $(uname -s)" >&2; exit 1 ;;
esac

case "$(uname -m)" in
  x86_64|amd64) architecture="x86_64" ;;
  arm64|aarch64) architecture="aarch64" ;;
  *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

asset="patchouli-db-${platform}-${architecture}"
if [ "$version" = "latest" ]; then
  release_url="https://github.com/${repository}/releases/latest/download"
else
  release_url="https://github.com/${repository}/releases/download/${version}"
fi

command -v curl >/dev/null 2>&1 || { echo "curl is required" >&2; exit 1; }
mkdir -p "$install_dir"
target="${install_dir}/patchouli-db"
if [ -L "$target" ] || { [ -e "$target" ] && [ ! -f "$target" ]; }; then
  echo "installation target must be a regular file: $target" >&2
  exit 1
fi
temporary_dir="$(mktemp -d "${TMPDIR:-/tmp}/patchouli.XXXXXX")"
downloaded_binary="${temporary_dir}/${asset}"
downloaded_checksum="${temporary_dir}/${asset}.sha256"
staged_binary=""
cleanup() {
  rm -f -- "$downloaded_binary" "$downloaded_checksum"
  if [ -n "$staged_binary" ]; then rm -f -- "$staged_binary"; fi
  rmdir "$temporary_dir" 2>/dev/null || true
}
trap cleanup EXIT HUP INT TERM

curl --proto '=https' --tlsv1.2 -fLSs "${release_url}/${asset}" -o "$downloaded_binary"
curl --proto '=https' --tlsv1.2 -fLSs "${release_url}/${asset}.sha256" -o "$downloaded_checksum"
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$temporary_dir" && sha256sum -c "${asset}.sha256")
elif command -v shasum >/dev/null 2>&1; then
  (cd "$temporary_dir" && shasum -a 256 -c "${asset}.sha256")
else
  echo "sha256sum or shasum is required" >&2
  exit 1
fi

staged_binary="$(mktemp "${install_dir}/.patchouli.XXXXXX")"
cp "$downloaded_binary" "$staged_binary"
chmod 0755 "$staged_binary"
"$staged_binary" init --root "$patchouli_home"
mv -f "$staged_binary" "$target"
staged_binary=""

echo "installed Patchouli DB to $target"
case ":${PATH}:" in
  *":${install_dir}:"*) ;;
  *) echo "add ${install_dir} to PATH to invoke patchouli-db directly" ;;
esac
