<#
.SYNOPSIS
    今の作業に関係のないビルド成果物を消してから、cargo build を実行します。

.DESCRIPTION
    消すもの:
      1. このスクリプトを実行している worktree 以外の worktree にある target/
         （作り直せるものだけです。ソースや未コミットの変更には触れません）
      2. 各 worktree の直下にある target-* フォルダ
         （CARGO_TARGET_DIR に一時的に使ったもの。CACHEDIR.TAG があるものだけ消します）
      3. -RemoveWorktrees を付けたときだけ、次の条件をすべて満たす worktree そのもの
         - detached HEAD である
         - 未コミットの変更や追跡外のファイルがない（target/ などの無視されたファイルは除く）
         - HEAD が origin/develop に取り込み済み
      4. 今の target/ は、rustc のバージョンが前回と変わったときだけ cargo clean します。
         cargo-sweep が入っていれば、-SweepDays 日より古い成果物も消します。

.PARAMETER WhatIf
    消すものを表示するだけで、実際には消さず、ビルドもしません。

.PARAMETER RemoveWorktrees
    取り込み済みで変更のない detached HEAD の worktree も消します。

.PARAMETER NoBuild
    掃除だけして、ビルドはしません。

.PARAMETER SweepDays
    cargo-sweep があるとき、この日数より古い成果物を消します（既定 14）。

.PARAMETER CargoArgs
    cargo build に渡す引数。例: -CargoArgs '--release'

.EXAMPLE
    ./scripts/clean-build.ps1 -WhatIf
    ./scripts/clean-build.ps1
    ./scripts/clean-build.ps1 -RemoveWorktrees -CargoArgs '--release'
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [switch]$RemoveWorktrees,
    [switch]$NoBuild,
    [int]$SweepDays = 14,
    [string[]]$CargoArgs = @()
)

$ErrorActionPreference = 'Stop'

function Get-FullPath([string]$Path) {
    [System.IO.Path]::GetFullPath($Path).TrimEnd('\', '/')
}

function Remove-BuildDir([string]$Path, [string]$Reason) {
    if (-not (Test-Path -LiteralPath $Path)) { return }
    if ($PSCmdlet.ShouldProcess($Path, "削除 ($Reason)")) {
        Write-Host "削除: $Path ($Reason)"
        # Remove-Item -Recurse は大きなフォルダでとても遅いため rd を使う
        cmd /c rd /s /q "`"$Path`"" 2>$null
        if (Test-Path -LiteralPath $Path) {
            Write-Warning "消しきれませんでした（ビルド中のプロセスが使っている可能性があります）: $Path"
        }
    }
}

function Test-CargoTargetDir([string]$Path) {
    # cargo は target ディレクトリに CACHEDIR.TAG を作る
    Test-Path -LiteralPath (Join-Path $Path 'CACHEDIR.TAG')
}

function Get-FreeBytes([string]$Path) {
    $root = [System.IO.Path]::GetPathRoot((Get-FullPath $Path))
    (New-Object System.IO.DriveInfo($root)).AvailableFreeSpace
}

$current = Get-FullPath (git rev-parse --show-toplevel)
if ($LASTEXITCODE -ne 0) { throw 'git リポジトリの中で実行してください。' }
$freeBefore = Get-FreeBytes $current

# --- ビルド先の target ディレクトリ --------------------------------------
# 掃除・ビルド・スタンプのすべてで、cargo build が実際に使う target ディレクトリを使う
# このディレクトリは一括削除の対象から外し、clean と sweep だけで扱う
# （優先順: -CargoArgs の --target-dir、CARGO_TARGET_DIR、<worktree>/target）
$targetDir = $null
for ($i = 0; $i -lt $CargoArgs.Count; $i++) {
    if ($CargoArgs[$i] -eq '--target-dir' -and $i + 1 -lt $CargoArgs.Count) {
        $targetDir = $CargoArgs[$i + 1]
    } elseif ($CargoArgs[$i] -like '--target-dir=*') {
        $targetDir = $CargoArgs[$i].Substring('--target-dir='.Length)
    }
}
if (-not $targetDir) { $targetDir = $env:CARGO_TARGET_DIR }
if (-not $targetDir) { $targetDir = Join-Path $current 'target' }
$targetDir = Get-FullPath ([System.IO.Path]::Combine($current, $targetDir))

# cargo clean / cargo sweep を、決めた target ディレクトリに向けて実行する
function Invoke-CargoInTarget([string[]]$Arguments) {
    $saved = $env:CARGO_TARGET_DIR
    $env:CARGO_TARGET_DIR = $targetDir
    Push-Location $current
    try {
        cargo @Arguments
        if ($LASTEXITCODE -ne 0) { throw "cargo $($Arguments -join ' ') が失敗しました（終了コード $LASTEXITCODE）。" }
    } finally {
        Pop-Location
        $env:CARGO_TARGET_DIR = $saved
    }
}

# 消そうとしているフォルダが、今のビルド先そのものか、ビルド先を中に含むか
# （rd /s は中身ごと消すので、親フォルダを消してもビルド先が消える）
function Test-ActiveTarget([string]$Path) {
    $full = Get-FullPath $Path
    ($targetDir -ieq $full) -or $targetDir.StartsWith($full + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)
}

# --- worktree の一覧 -------------------------------------------------------
$worktrees = @()
$entry = $null
foreach ($line in (git worktree list --porcelain)) {
    if ($line -like 'worktree *') {
        if ($entry) { $worktrees += $entry }
        $entry = [ordered]@{ Path = Get-FullPath $line.Substring(9); Head = $null; Detached = $false; Branch = $null; Bare = $false }
    } elseif ($line -like 'HEAD *') {
        $entry.Head = $line.Substring(5)
    } elseif ($line -eq 'bare') {
        $entry.Bare = $true
    } elseif ($line -eq 'detached') {
        $entry.Detached = $true
    } elseif ($line -like 'branch *') {
        $entry.Branch = $line.Substring(7)
    }
}
if ($entry) { $worktrees += $entry }

if ($PSCmdlet.ShouldProcess('origin/develop', 'git fetch')) {
    git fetch --quiet origin develop 2>$null
} else {
    Write-Host '取り込み判定は、手元にある origin/develop で行います（-WhatIf のため fetch しません）。'
}

foreach ($wt in $worktrees) {
    # bare リポジトリには作業ツリーも HEAD もないので扱わない
    if ($wt.Bare) { continue }
    $isCurrent = $wt.Path -ieq $current

    # 直下の target-*（一時的な CARGO_TARGET_DIR）
    Get-ChildItem -LiteralPath $wt.Path -Directory -Filter 'target-*' -ErrorAction SilentlyContinue |
        Where-Object { (Test-CargoTargetDir $_.FullName) -and -not (Test-ActiveTarget $_.FullName) } |
        ForEach-Object { Remove-BuildDir $_.FullName '一時的な target ディレクトリ' }

    if ($isCurrent) { continue }

    $label = if ($wt.Branch) { $wt.Branch -replace '^refs/heads/', '' } else { "detached $($wt.Head.Substring(0, 7))" }

    if ($RemoveWorktrees -and $wt.Detached) {
        $dirty = git -C $wt.Path status --porcelain
        # 一覧を取ったあとに HEAD が動いていることがあるので、判定の直前に読み直す
        $head = git -C $wt.Path rev-parse HEAD
        git merge-base --is-ancestor $head origin/develop 2>$null
        $merged = $LASTEXITCODE -eq 0
        $active = Test-ActiveTarget $wt.Path
        if (-not $dirty -and $merged -and -not $active) {
            if ($PSCmdlet.ShouldProcess($wt.Path, "worktree を削除 ($label, develop に取り込み済み)")) {
                Write-Host "worktree を削除: $($wt.Path) ($label)"
                # target/ は無視されたファイルなので、先に消してから worktree remove する
                # CACHEDIR.TAG のない target/ は追跡済みのファイルかもしれないので消さない
                $wtTarget = Join-Path $wt.Path 'target'
                if ((Test-CargoTargetDir $wtTarget) -and -not (Test-ActiveTarget $wtTarget)) {
                    Remove-BuildDir $wtTarget '削除する worktree のビルド成果物'
                }
                git worktree remove $wt.Path
            }
            continue
        }
        $why = @()
        if ($dirty) { $why += '未コミットの変更あり' }
        if (-not $merged) { $why += 'develop に未取り込み' }
        if ($active) { $why += '今のビルド先を含む' }
        Write-Host "worktree は残します: $($wt.Path) ($label, $($why -join ', '))"
    }

    $target = Join-Path $wt.Path 'target'
    if ((Test-CargoTargetDir $target) -and -not (Test-ActiveTarget $target)) {
        Remove-BuildDir $target "別の worktree ($label) のビルド成果物"
    }
}

if ($PSCmdlet.ShouldProcess('git worktree', 'prune')) { git worktree prune }

# --- 今の target/ -----------------------------------------------------------
$stamp = [System.IO.Path]::Combine($targetDir, '.rustc-version')
$rustc = (rustc -vV) -join "`n"
if ((Test-Path -LiteralPath $stamp) -and ((Get-Content -Raw -LiteralPath $stamp).Trim() -ne $rustc.Trim())) {
    if ($PSCmdlet.ShouldProcess($targetDir, 'cargo clean (rustc のバージョンが変わったため)')) {
        Write-Host 'rustc のバージョンが変わったので cargo clean します。'
        Invoke-CargoInTarget @('clean')
    }
}

if (Get-Command cargo-sweep -ErrorAction SilentlyContinue) {
    if ($PSCmdlet.ShouldProcess($targetDir, "cargo sweep --time $SweepDays")) {
        Invoke-CargoInTarget @('sweep', '--time', "$SweepDays")
    }
}

$freed = (Get-FreeBytes $current) - $freeBefore
if (-not $WhatIfPreference) {
    Write-Host ('空いた容量: {0:N1} GB' -f ($freed / 1GB))
}

# --- ビルド ---------------------------------------------------------------
if ($NoBuild -or $WhatIfPreference) { return }

Push-Location $current
try {
    cargo build @CargoArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    New-Item -ItemType Directory -Force $targetDir | Out-Null
    Set-Content -LiteralPath $stamp -Value $rustc -NoNewline
} finally {
    Pop-Location
}
