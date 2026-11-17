#!/bin/sh
# Install mcp-rs on macOS (Apple Silicon) or Linux (x64), #576.
#
#   curl -fsSL https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.sh | sh
#
# To read it before running it:
#
#   curl -fsSLO https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.sh
#   less install.sh
#   sh install.sh
#
# It downloads the ready-built mcp-rs of a release, checks it against the
# release's SHA256, and puts it in ~/.mcp-rs/bin. No administrator rights are
# needed, and nothing else is changed (no PATH or shell settings). Run it again
# to update. Then it offers to run `mcp-rs init`.
#
# Settings (environment variables):
#   MCP_RS_VERSION      Release to install, such as v0.25.0 (default: the latest)
#   MCP_RS_INSTALL_DIR  Folder for mcp-rs (default: ~/.mcp-rs/bin)
#   MCP_RS_NO_INIT=1    Do not offer to run mcp-rs init

set -eu

REPO="n-takatsu/mcp-rs"

say() {
    printf '%s\n' "$*"
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

# The release file for this system
target() {
    os=$(uname -s)
    arch=$(uname -m)
    case "${os}/${arch}" in
        Darwin/arm64) echo "aarch64-apple-darwin" ;;
        Linux/x86_64 | Linux/amd64) echo "x86_64-unknown-linux-gnu" ;;
        *)
            fail "no ready-built mcp-rs for ${os} (${arch}). Ready-built files exist for macOS (Apple Silicon), Linux (x64) and Windows (x64; use install.ps1). To build it yourself, see https://github.com/${REPO}#setup"
            ;;
    esac
}

# The tag of the latest release. github.com/<repo>/releases/latest redirects
# to .../releases/tag/<tag>. (The GitHub API has a low limit per address,
# which shared networks and CI runners can reach.)
latest_version() {
    curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/${REPO}/releases/latest" |
        sed -n 's|.*/releases/tag/\([^/?#]*\)$|\1|p'
}

sha256_of() {
    if command -v sha256sum > /dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    elif command -v shasum > /dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d ' ' -f 1
    else
        fail "neither sha256sum nor shasum was found, so the download cannot be checked"
    fi
}

main() {
    command -v curl > /dev/null 2>&1 || fail "curl is needed"
    command -v tar > /dev/null 2>&1 || fail "tar is needed"

    target=$(target)
    version="${MCP_RS_VERSION:-}"
    if [ -z "${version}" ]; then
        version=$(latest_version) || true
        [ -n "${version}" ] || fail "could not find the latest release of ${REPO}. Set MCP_RS_VERSION (for example v0.25.0) and run this again"
    fi
    case "${version}" in
        v*) ;;
        *) version="v${version}" ;;
    esac
    install_dir="${MCP_RS_INSTALL_DIR:-${HOME}/.mcp-rs/bin}"
    name="mcp-rs-${version}-${target}"
    url="https://github.com/${REPO}/releases/download/${version}/${name}.tar.gz"

    say "Installing mcp-rs ${version} (${target}) into ${install_dir}"

    work=$(mktemp -d)
    trap 'rm -rf "${work}"' EXIT INT TERM

    curl -fsSL "${url}" -o "${work}/${name}.tar.gz" ||
        fail "could not download ${url} (mcp-rs ${version} may not have a ready-built file for ${target}; they exist from v0.25.0)"
    curl -fsSL "${url}.sha256" -o "${work}/${name}.tar.gz.sha256" ||
        fail "could not download ${url}.sha256"

    expected=$(cut -d ' ' -f 1 < "${work}/${name}.tar.gz.sha256")
    actual=$(sha256_of "${work}/${name}.tar.gz")
    if [ -z "${expected}" ] || [ "${expected}" != "${actual}" ]; then
        fail "the SHA256 of the download does not match the release (expected ${expected}, got ${actual}). Nothing was installed"
    fi
    say "SHA256 checked: ${actual}"

    tar -xzf "${work}/${name}.tar.gz" -C "${work}"
    [ -f "${work}/${name}/mcp-rs" ] || fail "the download does not contain ${name}/mcp-rs"

    mkdir -p "${install_dir}"
    # Copy next to it first, check that it runs, then rename, so a running
    # mcp-rs keeps working and one that does not run replaces nothing
    new="${install_dir}/mcp-rs.new"
    cp "${work}/${name}/mcp-rs" "${new}"
    chmod 755 "${new}"
    if ! installed=$("${new}" --version 2>&1); then
        rm -f "${new}"
        fail "the downloaded mcp-rs does not run on this system: ${installed} (on Linux, glibc 2.35 or later is needed). Nothing was replaced"
    fi
    exe="${install_dir}/mcp-rs"
    mv -f "${new}" "${exe}"
    say "Installed ${installed}: ${exe}"
    say ""

    if [ "${MCP_RS_NO_INIT:-}" = "1" ]; then
        say "Next, set it up with: ${exe} init"
        return
    fi
    # Piped into sh, standard input is this script, so ask the terminal.
    # /dev/tty exists even without a terminal (Docker, CI, ssh without a
    # terminal), so check that it can be opened
    if (: < /dev/tty > /dev/tty) 2> /dev/null; then
        printf 'Run mcp-rs init now, to set up WordPress and register mcp-rs with your MCP clients? [Y/n] ' > /dev/tty
        answer=""
        read -r answer < /dev/tty || answer="n"
        case "${answer}" in
            "" | y | Y | yes | YES)
                "${exe}" init < /dev/tty > /dev/tty 2>&1 || true
                return
                ;;
        esac
    fi
    say "Next, set it up with: ${exe} init"
}

# Run only when the whole script has been downloaded
main "$@"
