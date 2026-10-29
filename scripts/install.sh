#!/usr/bin/env bash
# ticktop user-local installer; compatible with macOS Bash 3.2.
set -euo pipefail

fail() { printf 'ticktop: %s\n' "$*" >&2; exit 1; }
usage() { printf '%s\n' 'Usage: install.sh [--shell zsh|bash] [--version vX.Y.Z] [--no-path]' 'Environment: TICKTOP_INSTALL_DIR (default ~/.local/bin), TICKTOP_VERSION'; }
shell_choice=''
version="${TICKTOP_VERSION:-latest}"
modify_path=1
while [ "$#" -gt 0 ]; do
    case "$1" in
        --shell) [ "$#" -ge 2 ] || fail '--shell needs zsh or bash'; shell_choice="$2"; shift 2 ;;
        --version) [ "$#" -ge 2 ] || fail '--version needs a value'; version="$2"; shift 2 ;;
        --no-path) modify_path=0; shift ;;
        -h|--help) usage; exit 0 ;;
        *) fail "Unknown option: $1" ;;
    esac
done
case "$shell_choice" in ''|bash|zsh) ;; *) fail 'Supported shells: zsh, bash' ;; esac
case "$version" in latest) ;; *) [[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][a-zA-Z0-9.-]+)?$ ]] || fail 'Invalid version (use v0.1.0)' ;; esac
[ -n "${HOME:-}" ] || fail 'HOME is not set'
for tool in curl tar mktemp; do command -v "$tool" >/dev/null 2>&1 || fail "Missing dependency: $tool"; done
if command -v sha256sum >/dev/null 2>&1; then checksum_tool=sha256sum
elif command -v shasum >/dev/null 2>&1; then checksum_tool=shasum
else fail 'sha256sum or shasum is required'; fi
case "$(uname -s)" in
    Linux) platform=unknown-linux-musl ;;
    Darwin) platform=apple-darwin ;;
    *) fail 'This release supports Linux and macOS only' ;;
esac
case "$(uname -m)" in
    x86_64|amd64) arch=x86_64 ;;
    aarch64|arm64) arch=aarch64 ;;
    *) fail 'Supported architectures: x86_64, aarch64/arm64' ;;
esac
asset="ticktop-${arch}-${platform}.tar.gz"
base='https://github.com/Nyakooo/terminal-stock-dashboard/releases'
if [ "$version" = latest ]; then download="$base/latest/download"; else download="$base/download/$version"; fi
install_dir="${TICKTOP_INSTALL_DIR:-$HOME/.local/bin}"
case "$install_dir" in /*) ;; *) fail 'TICKTOP_INSTALL_DIR must be absolute' ;; esac
case "$install_dir" in *$'\n'*) fail 'Installation path cannot contain newlines' ;; esac
work=$(mktemp -d "${TMPDIR:-/tmp}/ticktop-install.XXXXXXXX")
trap 'rm -rf "$work"' EXIT
printf 'Downloading %s (%s)…\n' "$asset" "$version"
fetch() { curl --proto '=https' --tlsv1.2 -fsSL --retry 2 --connect-timeout 10 --max-time 120 "$1" -o "$2"; }
fetch "$download/$asset" "$work/$asset" || fail 'Download failed. Confirm a GitHub release exists and check your network/proxy.'
fetch "$download/$asset.sha256" "$work/checksum" || fail 'Missing checksum; installation cancelled.'
read -r expected checksum_name < "$work/checksum" || fail 'Invalid checksum file'
[[ "$expected" =~ ^[a-fA-F0-9]{64}$ ]] || fail 'Invalid SHA-256 checksum'
[ "$checksum_name" = "$asset" ] || [ "$checksum_name" = "*$asset" ] || fail 'Checksum asset name mismatch'
if [ "$checksum_tool" = sha256sum ]; then actual=$(sha256sum "$work/$asset"); else actual=$(shasum -a 256 "$work/$asset"); fi
actual=${actual%% *}
[ "$actual" = "$expected" ] || fail 'Checksum mismatch; existing installation was not changed.'
# Extract only the expected file. Reject links and extra executable entries.
entries=$(tar -tzf "$work/$asset")
case "$entries" in ticktop) ;; *) fail 'Unexpected archive contents' ;; esac
listing=$(tar -tvzf "$work/$asset")
case "$listing" in -*) ;; *) fail 'Expected a regular binary file in archive' ;; esac
tar -xzf "$work/$asset" -C "$work" ticktop
[ -f "$work/ticktop" ] && [ ! -L "$work/ticktop" ] || fail 'Archive does not contain a regular ticktop binary'
chmod 755 "$work/ticktop"
"$work/ticktop" --version || fail 'Downloaded binary cannot run on this system'

rc=''
if [ "$modify_path" -eq 1 ]; then
    # SHELL describes the user shell, not the bash process reading this script.
    if [ -z "$shell_choice" ]; then shell_choice="${SHELL:-}"; shell_choice="${shell_choice##*/}"; fi
    if [ -z "$shell_choice" ] && command -v getent >/dev/null 2>&1; then
        shell_choice=$(getent passwd "$(id -un)" | cut -d: -f7); shell_choice="${shell_choice##*/}"
    fi
    case "$shell_choice" in
        zsh) rc="${ZDOTDIR:-$HOME}/.zshrc" ;;
        bash) rc="$HOME/.bashrc" ;;
        *) printf '%s\n' 'Shell not recognized; PATH unchanged. Use --shell zsh or --shell bash to configure it.' ;;
    esac
    if [ -n "$rc" ]; then
        mkdir -p "$(dirname "$rc")"
        if [ -e "$rc" ] && [ ! -w "$rc" ]; then fail "Cannot write $rc; use --no-path or fix its permissions"; fi
        # A managed block is appended once. Existing user configuration is preserved.
        if ! grep -Fq '# >>> ticktop >>>' "$rc" 2>/dev/null; then
            escaped_dir=$(printf '%q' "$install_dir")
            {
                printf '\n# >>> ticktop >>>\n'
                printf 'ticktop_bin=%s\n' "$escaped_dir"
                printf 'case ":$PATH:" in\n'
                printf '  *":$ticktop_bin:"*) ;;\n'
                printf '  *) export PATH="$ticktop_bin:$PATH" ;;\n'
                printf 'esac\nunset ticktop_bin\n# <<< ticktop <<<\n'
            } >> "$rc"
        fi
    fi
fi
mkdir -p "$install_dir"
# Stage in the destination filesystem so replacement is atomic; keep the old binary on failure.
staged=$(mktemp "$install_dir/.ticktop-install.XXXXXXXX")
trap 'rm -rf "$work"; [ -z "${staged:-}" ] || rm -f "$staged"' EXIT
cp "$work/ticktop" "$staged"
chmod 755 "$staged"
mv -f "$staged" "$install_dir/ticktop"
printf '\nInstalled: %s/ticktop\n' "$install_dir"
if [ -n "$rc" ]; then
    printf 'Open a new terminal, or run: source %q\n' "$rc"
fi
printf 'Run now: %q\n' "$install_dir/ticktop"
