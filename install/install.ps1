# Install mcp-rs on Windows (x64), #576.
#
#   irm https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.ps1 | iex
#
# To read it before running it:
#
#   irm https://raw.githubusercontent.com/n-takatsu/mcp-rs/main/install/install.ps1 -OutFile install.ps1
#   notepad install.ps1
#   powershell -ExecutionPolicy Bypass -File install.ps1
#
# It downloads the ready-built mcp-rs of a release, checks it against the
# release's SHA256, and puts it in %USERPROFILE%\.mcp-rs\bin. No administrator
# rights are needed, and nothing else is changed (no PATH or other settings).
# Run it again to update. Then it offers to run `mcp-rs init`.
#
# Works in Windows PowerShell 5.1 and PowerShell 7.
#
# Settings (environment variables):
#   MCP_RS_VERSION      Release to install, such as v0.25.0 (default: the latest)
#   MCP_RS_INSTALL_DIR  Folder for mcp-rs (default: %USERPROFILE%\.mcp-rs\bin)
#   MCP_RS_NO_INIT=1    Do not offer to run mcp-rs init

# Everything is in one function, called on the last line, so a partly
# downloaded script does nothing. It never calls exit, which would close the
# PowerShell window when run with iex
function Install-McpRs {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'   # Invoke-WebRequest is much faster without it
    $repo = 'n-takatsu/mcp-rs'
    $target = 'x86_64-pc-windows-msvc'

    # Windows PowerShell 5.1 may not use TLS 1.2 by default
    if ($PSVersionTable.PSVersion.Major -lt 6) {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    }

    if (-not [Environment]::Is64BitOperatingSystem) {
        throw 'mcp-rs needs 64-bit Windows'
    }
    if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') {
        Write-Host 'Windows on Arm: installing the x64 mcp-rs, which Windows runs with its x64 emulation'
    }

    $version = $env:MCP_RS_VERSION
    if (-not $version) {
        # github.com/<repo>/releases/latest redirects to .../releases/tag/<tag>.
        # (The GitHub API has a low limit per address, which shared networks
        # can reach.)
        try {
            $response = Invoke-WebRequest -UseBasicParsing -Method Head "https://github.com/$repo/releases/latest"
            if ($PSVersionTable.PSVersion.Major -ge 6) {
                $final = $response.BaseResponse.RequestMessage.RequestUri.AbsoluteUri
            } else {
                $final = $response.BaseResponse.ResponseUri.AbsoluteUri
            }
            if ($final -match '/releases/tag/([^/?#]+)$') { $version = $Matches[1] }
            if (-not $version) { throw "unexpected address $final" }
        } catch {
            throw "could not find the latest release of ${repo}: $($_.Exception.Message). Set `$env:MCP_RS_VERSION (for example v0.25.0) and run this again"
        }
    }
    if (-not $version.StartsWith('v')) { $version = "v$version" }

    $installDir = $env:MCP_RS_INSTALL_DIR
    if (-not $installDir) { $installDir = Join-Path $env:USERPROFILE '.mcp-rs\bin' }
    $name = "mcp-rs-$version-$target"
    $url = "https://github.com/$repo/releases/download/$version/$name.zip"

    Write-Host "Installing mcp-rs $version ($target) into $installDir"

    $work = Join-Path ([IO.Path]::GetTempPath()) ("mcp-rs-install-" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $work | Out-Null
    try {
        $zip = Join-Path $work "$name.zip"
        try {
            Invoke-WebRequest -UseBasicParsing $url -OutFile $zip
            Invoke-WebRequest -UseBasicParsing "$url.sha256" -OutFile "$zip.sha256"
        } catch {
            throw "could not download $url ($($_.Exception.Message)). mcp-rs $version may not have a ready-built file; they exist from v0.25.0"
        }

        $expected = ((Get-Content -Raw "$zip.sha256").Trim() -split '\s+')[0].ToLower()
        $actual = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
        if (-not $expected -or $expected -ne $actual) {
            throw "the SHA256 of the download does not match the release (expected $expected, got $actual). Nothing was installed"
        }
        Write-Host "SHA256 checked: $actual"

        Expand-Archive -Path $zip -DestinationPath $work -Force
        $downloaded = Join-Path $work "$name\mcp-rs.exe"
        if (-not (Test-Path $downloaded)) { throw "the download does not contain $name\mcp-rs.exe" }

        New-Item -ItemType Directory -Force -Path $installDir | Out-Null
        $exe = Join-Path $installDir 'mcp-rs.exe'
        try {
            # A running mcp-rs.exe cannot be overwritten, but it can be
            # renamed: move it aside, so updating works while Claude Desktop
            # or VS Code is running it (they use the new one after a restart)
            $old = "$exe.old"
            if (Test-Path $old) { Remove-Item -Force $old -ErrorAction SilentlyContinue }
            if (Test-Path $exe) { Move-Item -Force $exe $old }
            Copy-Item -Force $downloaded $exe
            Remove-Item -Force $old -ErrorAction SilentlyContinue
        } catch {
            throw "could not replace $exe ($($_.Exception.Message)). If Claude Desktop or VS Code is running mcp-rs, quit it (Claude Desktop from the system tray) and run this again"
        }
        # Remove the mark Windows puts on downloaded files
        Unblock-File $exe
    } finally {
        Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
    }

    $installed = & $exe --version 2>&1
    if ($LASTEXITCODE -ne 0) { throw "mcp-rs was installed but does not run: $installed" }
    Write-Host "Installed ${installed}: $exe"
    Write-Host ''

    $next = "Next, set it up with: & `"$exe`" init"
    if ($env:MCP_RS_NO_INIT -eq '1' -or -not [Environment]::UserInteractive) {
        Write-Host $next
        return
    }
    $answer = Read-Host 'Run mcp-rs init now, to set up WordPress and register mcp-rs with your MCP clients? [Y/n]'
    if ($answer -eq '' -or $answer -match '^(y|yes)$') {
        & $exe init
    } else {
        Write-Host $next
    }
}

Install-McpRs
