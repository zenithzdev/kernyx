#!/usr/bin/env bash
# KERNYX one-command APT bootstrap installer.
# https://github.com/zenithzdev/kernyx
set -euo pipefail

REPO_URL="https://raw.githubusercontent.com/zenithzdev/kernyx/gh-pages"
KEY_URL="$REPO_URL/kernyx.asc"
EXPECTED_FINGERPRINT="864FE821DCAF96A4B8F9F71EFB1D9A2606C3F536"
KEYRING="/etc/apt/keyrings/kernyx.gpg"
SOURCE_LIST="/etc/apt/sources.list.d/kernyx.list"

C_RESET="\033[0m"
C_BOLD="\033[1m"
C_CYAN="\033[1;36m"
C_GREEN="\033[1;32m"
C_YELLOW="\033[1;33m"
C_RED="\033[1;31m"
C_DIM="\033[90m"

banner() {
    printf '%s\n' "${C_CYAN}"
    printf '╔══════════════════════════════════════════════════════╗\n'
    printf '║                 K E R N Y X  I N S T A L L E R       ║\n'
    printf '║        Adaptive Linux Performance Optimization        ║\n'
    printf '╚══════════════════════════════════════════════════════╝%s\n' "${C_RESET}"
}

step() {
    printf '%s[•]%s %s\n' "${C_CYAN}" "${C_RESET}" "$*"
}

ok() {
    printf '%s[✓]%s %s\n' "${C_GREEN}" "${C_RESET}" "$*"
}

warn() {
    printf '%s[!]%s %s\n' "${C_YELLOW}" "${C_RESET}" "$*"
}

die() {
    printf '%s[✗] %s%s\n' "${C_RED}" "$*" "${C_RESET}" >&2
    exit 1
}

[[ $(id -u) -eq 0 ]] || die "Run as root: curl -fsSL https://raw.githubusercontent.com/zenithzdev/kernyx/main/install.sh | sudo bash"

banner

# Detect APT
command -v apt-get >/dev/null 2>&1 || die "This installer requires an APT-based distribution."

# Detect architecture
ARCH=$(dpkg --print-architecture 2>/dev/null || uname -m)
case "$ARCH" in
    amd64|x86_64) ARCH=amd64 ;;
    arm64|aarch64) ARCH=arm64 ;;
    *) die "Unsupported architecture: $ARCH" ;;
esac

# Detect distro
[[ -r /etc/os-release ]] || die "Cannot read /etc/os-release"
# shellcheck disable=SC1091
source /etc/os-release
OS_ID="${ID:-unknown}"
OS_VER="${VERSION_ID:-unknown}"
SUPPORTED=0
case "$OS_ID" in
    ubuntu)
        case "$OS_VER" in
            20.04|22.04|24.04|26.04) SUPPORTED=1 ;;
        esac ;;
    debian)
        case "$OS_VER" in
            11|12|13) SUPPORTED=1 ;;
        esac ;;
    linuxmint|pop|raspbian)
        SUPPORTED=1 ;;
esac
(( SUPPORTED == 1 )) || die "Unsupported distribution: $OS_ID $OS_VER"

step "Detected: $OS_ID $OS_VER ($ARCH)"

# Clean stale KERNYX sources
step "Cleaning stale KERNYX repository entries"
rm -f /etc/apt/sources.list.d/*kernyx*.list
if [[ -f /etc/apt/sources.list ]]; then
    sed -i '/kernyx/d' /etc/apt/sources.list
fi
ok "Stale entries removed"

# Prerequisites
step "Installing prerequisites"
apt-get update -qq
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
    ca-certificates curl gpg apt-transport-https
for tool in curl gpg apt-get; do
    command -v "$tool" >/dev/null 2>&1 || die "Required tool missing: $tool"
done
ok "Prerequisites ready"

install -d -m 0755 /etc/apt/keyrings

TMPDIR_KY=$(mktemp -d)
trap 'rm -rf "$TMPDIR_KY"' EXIT

# Key
step "Downloading and verifying repository key"
curl -fsSL "$KEY_URL" -o "$TMPDIR_KY/kernyx.asc" || die "Failed to download public key"
FPR=$(gpg --show-keys --with-colons "$TMPDIR_KY/kernyx.asc" 2>/dev/null | awk -F: '$1=="fpr"{print $10; exit}')
[[ "$FPR" == "$EXPECTED_FINGERPRINT" ]] || die "Key fingerprint mismatch"
gpg --batch --yes --dearmor -o "$TMPDIR_KY/kernyx.gpg" "$TMPDIR_KY/kernyx.asc"
install -m 0644 "$TMPDIR_KY/kernyx.gpg" "$KEYRING"
ok "Repository key installed"

# Source
step "Configuring APT repository"
printf 'deb [arch=%s signed-by=%s] %s stable main\n' "$ARCH" "$KEYRING" "$REPO_URL" >"$TMPDIR_KY/kernyx.list"
install -m 0644 "$TMPDIR_KY/kernyx.list" "$SOURCE_LIST"
ok "APT source configured"

# Update
step "Updating package index"
apt-get update -qq
ok "Package index updated"

# Install
step "Installing kernyx"
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq kernyx
ok "kernyx installed"

# Verify
command -v kernyx >/dev/null 2>&1 || die "kernyx executable not found after install"
VERSION=$(kernyx version 2>/dev/null | head -1)

printf '\n%s╔══════════════════════════════════════════════════════╗%s\n' "${C_GREEN}" "${C_RESET}"
printf '%s║               INSTALLATION COMPLETE                   ║%s\n' "${C_GREEN}" "${C_RESET}"
printf '%s╚══════════════════════════════════════════════════════╝%s\n' "${C_GREEN}" "${C_RESET}"
printf '  Installed: %s\n' "$VERSION"
printf '  Next:      %skernyx detect%s  or  %skernyx fetch%s\n\n' "${C_CYAN}" "${C_RESET}" "${C_CYAN}" "${C_RESET}"
