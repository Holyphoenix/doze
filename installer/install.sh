#!/usr/bin/env bash
# Installs Doze on macOS, or updates an existing /Applications/Doze.app installation in place.
# User settings & secrets in ~/Library/Application Support/dev.doze.Doze are always preserved.
#
# Usage:
#   ./installer/install.sh                      # Builds from local repo and installs/updates
#   ./installer/install.sh --repo owner/repo    # Downloads latest macOS release from GitHub
#   ./installer/install.sh --uninstall          # Removes installed Doze.app

set -euo pipefail

FROM_SOURCE=0
REPO=""
VERSION="latest"
UNINSTALL=0
INSTALL_DIR="/Applications"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --from-source) FROM_SOURCE=1; shift ;;
        --repo) REPO="$2"; shift 2 ;;
        --version) VERSION="$2"; shift 2 ;;
        --install-dir) INSTALL_DIR="$2"; shift 2 ;;
        --uninstall) UNINSTALL=1; shift ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

# Fall back to ~/Applications if /Applications is not writable and not root
if [[ ! -w "$INSTALL_DIR" && "$EUID" -ne 0 ]]; then
    INSTALL_DIR="$HOME/Applications"
    mkdir -p "$INSTALL_DIR"
fi

APP_DEST="$INSTALL_DIR/Doze.app"
CLI_LINK="/usr/local/bin/doze"

stop_running_doze() {
    if pgrep -x "doze" >/dev/null 2>&1; then
        echo "Stopping running Doze instance..."
        osascript -e 'tell application "Doze" to quit' >/dev/null 2>&1 || true
        sleep 1
        pkill -x "doze" >/dev/null 2>&1 || true
    fi
}

if [[ "$UNINSTALL" -eq 1 ]]; then
    stop_running_doze
    rm -rf "$APP_DEST"
    [[ -L "$CLI_LINK" ]] && rm -f "$CLI_LINK" || true
    echo "Removed $APP_DEST. User settings & secrets in ~/Library/Application Support/dev.doze.Doze were preserved."
    exit 0
fi

EXISTING_VERSION=""
if [[ -f "$APP_DEST/Contents/Info.plist" ]]; then
    EXISTING_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_DEST/Contents/Info.plist" 2>/dev/null || echo 'unknown')"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

if [[ "$FROM_SOURCE" -eq 0 && -z "$REPO" ]]; then
    if [[ -f "$REPO_ROOT/Cargo.toml" ]]; then
        FROM_SOURCE=1
    else
        echo "Error: pass --repo owner/repo to download from GitHub Releases, or run inside the Doze repo with --from-source." >&2
        exit 1
    fi
fi

TMP_WORK="$(mktemp -d)"
trap 'rm -rf "$TMP_WORK"' EXIT

if [[ "$FROM_SOURCE" -eq 1 ]]; then
    NEW_VERSION="$(grep -m1 '^version' "$REPO_ROOT/Cargo.toml" | sed -E 's/.*"([^"]+)".*/\1/')"
    if [[ -n "$EXISTING_VERSION" ]]; then
        echo "Building & updating Doze ($EXISTING_VERSION -> $NEW_VERSION)..."
    else
        echo "Building & installing Doze $NEW_VERSION..."
    fi
    (cd "$REPO_ROOT" && cargo build --release)
    bash "$REPO_ROOT/installer/macos/package_macos.sh" "$REPO_ROOT/target/release/doze" "$(uname -m)" "$TMP_WORK"
    NEW_APP="$TMP_WORK/Doze.app"
else
    if [[ "$VERSION" == "latest" ]]; then
        API_URL="https://api.github.com/repos/$REPO/releases/latest"
    else
        API_URL="https://api.github.com/repos/$REPO/releases/tags/$VERSION"
    fi
    echo "Checking GitHub release at $API_URL ..."
    RELEASE_JSON="$(curl -fsSL -H 'User-Agent: Doze-Installer' "$API_URL")"
    TAR_URL="$(printf '%s' "$RELEASE_JSON" | grep -oE 'https://[^"]+Doze-macos-(universal|'"$(uname -m)"')\.tar\.gz' | head -n 1)"
    if [[ -z "$TAR_URL" ]]; then
        echo "Error: could not find a macOS .tar.gz release asset in $REPO." >&2
        exit 1
    fi
    echo "Downloading $TAR_URL ..."
    curl -fsSL "$TAR_URL" -o "$TMP_WORK/doze.tar.gz"
    tar -xzf "$TMP_WORK/doze.tar.gz" -C "$TMP_WORK"
    NEW_APP="$TMP_WORK/Doze.app"
    NEW_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$NEW_APP/Contents/Info.plist" 2>/dev/null || echo 'latest')"
fi

stop_running_doze

# Atomic swap: remove previous app bundle and move new bundle into place
rm -rf "$APP_DEST"
mv "$NEW_APP" "$APP_DEST"

# Optional CLI symlink if /usr/local/bin is writable
if [[ -w "/usr/local/bin" ]]; then
    ln -sf "$APP_DEST/Contents/MacOS/doze" "$CLI_LINK"
fi

if [[ -n "$EXISTING_VERSION" ]]; then
    echo "Successfully updated Doze ($EXISTING_VERSION -> $NEW_VERSION) at $APP_DEST"
else
    echo "Successfully installed Doze $NEW_VERSION to $APP_DEST"
fi
