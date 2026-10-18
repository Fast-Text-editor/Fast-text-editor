<#
.SYNOPSIS
    Fast Text installer for Windows.

.DESCRIPTION
    Downloads the prebuilt release for your architecture and installs the
    fast-text, ft, textwrite and tw commands (plus fast-text-gui with -Gui),
    then records every installed file so -Uninstall can remove them exactly.
    When no prebuilt release exists it falls back to building from source
    with cargo.

    Works in Windows PowerShell 5.1 and PowerShell 7+, and on the pwsh
    you get from MSYS2/Git Bash/WSL.

.EXAMPLE
    iwr -useb https://raw.githubusercontent.com/Fast-Text-editor/Fast-text-editor/main/install.ps1 | iex

.EXAMPLE
    .\install.ps1 -Gui

.EXAMPLE
    .\install.ps1 -Version v0.1.0 -Prefix D:\Tools\fast-text

.EXAMPLE
    .\install.ps1 -Uninstall
#>
[CmdletBinding()]
param(
    # Install root; the executables go to <Prefix>\bin unless -BinDir says
    # otherwise. Defaults to %LOCALAPPDATA%\Programs\fast-text.
    [string]$Prefix,
    # Put the executables here (implies the prefix is its parent).
    [string]$BinDir,
    # Release tag to install, or "latest" (default). Env: FAST_TEXT_VERSION.
    [string]$Version = $(if ($env:FAST_TEXT_VERSION) { $env:FAST_TEXT_VERSION } else { 'latest' }),
    # Skip the prebuilt release and build with cargo.
    [switch]$FromSource,
    # Build from the checkout in this directory (implies -FromSource).
    [string]$Source,
    # Also install the desktop app.
    [switch]$Gui,
    # Do not write the PowerShell tab-completion script.
    [switch]$NoCompletions,
    # Add the bin directory to your user PATH.
    [switch]$AddToPath,
    # Remove everything a previous run installed.
    [switch]$Uninstall,
    # Compare the installed version with the newest release and print advice.
    [switch]$Check,
    # Install the newest release over the current install.
    [switch]$Update,
    # Accepted for symmetry with install.sh; the installer never prompts.
    [switch]$Yes,
    # Print what would happen and change nothing.
    [switch]$DryRun,
    # Print this help and exit.
    [Alias('h', '?')]
    [switch]$Help
)

$Program = 'fast-text'
$Repo = 'Fast-Text-editor/Fast-text-editor'
$Binaries = @('fast-text', 'ft', 'textwrite', 'tw')
$GuiBinary = 'fast-text-gui'

$ErrorActionPreference = 'Stop'
$script:Manifest = @()
$script:UpToDate = $false

# --------------------------------------------------------------------------
# Output helpers
# --------------------------------------------------------------------------

function Write-Log([string]$Message) {
    Write-Output $Message
}

function Write-WarningLine([string]$Message) {
    [Console]::Error.WriteLine("$Program installer: warning: $Message")
}

# Never `exit`: this script is also meant to be run via `iwr | iex`, where
# exiting would take the caller's session down with it.
function Fail([string]$Message) {
    throw "$Program installer: error: $Message"
}

function Warn([string]$Message) {
    Write-WarningLine $Message
}

function Need([string]$Command) {
    if (-not (Get-Command $Command -ErrorAction SilentlyContinue)) {
        Fail "'$Command' is required but was not found on PATH"
    }
}

function Show-Help {
    Write-Output @"
$Program installer (Windows)

Usage:
  install.ps1 [options]

Installs:
  fast-text, ft        the terminal editor
  textwrite, tw        the TextWrite language (also standalone)
  fast-text-gui        with -Gui, the desktop app

Options:
  -Prefix DIR          Install under DIR (default: %LOCALAPPDATA%\Programs\$Program);
                       executables go to DIR\bin
  -BinDir DIR          Put executables in DIR (overrides the prefix's bin)
  -Version TAG         Install a specific release tag (default: latest)
  -FromSource          Skip the prebuilt release and build with cargo
  -Source DIR          Build from the checkout in DIR (implies -FromSource)
  -Gui                 Also install the desktop app
  -NoCompletions       Do not install the tab-completion script
  -AddToPath           Add the bin directory to your user PATH
  -Uninstall           Remove everything a previous run installed
  -Check               Compare the installed version with the newest release
                       and print what to do next
  -Update              Install the newest release over the current install
  -DryRun              Print what would happen and change nothing
  -Yes                 Accepted for symmetry with install.sh (never prompts)
  -h, -Help            Show this help

Environment:
  FAST_TEXT_VERSION, FAST_TEXT_TARGET   behave like the options above
  FAST_TEXT_LATEST_TAG                  treat this as the newest release,
                                        skipping the GitHub query (for tests)

Examples:
  iwr -useb .../install.ps1 | iex
  .\install.ps1 -Gui -AddToPath
  .\install.ps1 -DryRun -FromSource
  .\install.ps1 -Uninstall
"@
}

# --------------------------------------------------------------------------
# Paths
# --------------------------------------------------------------------------

# Expands %VARS%, a leading ~, and makes the path absolute.
function Get-AbsolutePath([string]$Path) {
    if (-not $Path) { return $Path }
    $expanded = [Environment]::ExpandEnvironmentVariables($Path)
    if ($expanded.StartsWith('~')) {
        $homeDir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }
        $expanded = Join-Path $homeDir $expanded.Substring(1).TrimStart('/', '\')
    }
    if ([System.IO.Path]::IsPathRooted($expanded)) {
        return [System.IO.Path]::GetFullPath($expanded)
    }
    return [System.IO.Path]::GetFullPath((Join-Path (Get-Location).Path $expanded))
}

# Fills $Prefix and $BinDir.
function Resolve-Dirs {
    if ($BinDir) {
        $script:BinDir = Get-AbsolutePath $BinDir
        if (-not $Prefix) {
            $script:Prefix = Split-Path -Parent $script:BinDir
        } else {
            $script:Prefix = Get-AbsolutePath $Prefix
        }
    } else {
        if (-not $Prefix) {
            $root = if ($env:LOCALAPPDATA) {
                $env:LOCALAPPDATA
            } elseif ($env:USERPROFILE) {
                $env:USERPROFILE
            } else {
                $HOME
            }
            $script:Prefix = Join-Path (Join-Path $root 'Programs') $Program
        } else {
            $script:Prefix = Get-AbsolutePath $Prefix
        }
        $script:BinDir = Join-Path $script:Prefix 'bin'
    }
    $script:ManifestPath = Join-Path $script:Prefix 'installed-files.txt'
    $verb = if ($Uninstall) { 'Removing from' } elseif ($Check) { 'Looking at' } else { 'Installing into' }
    Write-Log "$verb $($script:BinDir)"
}

# --------------------------------------------------------------------------
# Platform detection
# --------------------------------------------------------------------------

function Get-Arch {
    if ($env:FAST_TEXT_ARCH) { return $env:FAST_TEXT_ARCH }
    $raw = $null
    try {
        $raw = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    } catch {
        $raw = $env:PROCESSOR_ARCHITECTURE
    }
    switch -Regex ($raw) {
        '^(x64|amd64|x86_64)$' { return 'x86_64' }
        '^(x86|i[3-6]86)$' { return 'i686' }
        '^(aarch64|arm64)$' { return 'aarch64' }
        '^(arm|armv7l|armv7)$' { return 'armv7' }
        default {
            Fail "unsupported architecture '$raw'; try -FromSource"
        }
    }
}

# pc-windows-msvc by default; FAST_TEXT_OS_PART overrides it (used by CI and
# by anyone running this script under pwsh on another platform).
function Get-Target {
    if ($env:FAST_TEXT_TARGET) { return $env:FAST_TEXT_TARGET }
    $osPart = if ($env:FAST_TEXT_OS_PART) { $env:FAST_TEXT_OS_PART } else { 'pc-windows-msvc' }
    return "$(Get-Arch)-$osPart"
}

# --------------------------------------------------------------------------
# Downloading a prebuilt release
# --------------------------------------------------------------------------

# The release workflow names each archive `fast-text-<target>.zip`
# (no version in the name), so the same asset works through both the
# rolling `latest` alias and a pinned `-Version <tag>`.
function Get-ReleaseUrl([string]$Target) {
    $asset = "$Program-$Target.zip"
    if ($Version -eq 'latest') {
        return "https://github.com/$Repo/releases/latest/download/$asset"
    }
    return "https://github.com/$Repo/releases/download/$Version/$asset"
}

function Invoke-Fetch([string]$Url, [string]$OutFile) {
    Write-Log "Downloading $Url"
    $previous = $ProgressPreference
    $ProgressPreference = 'SilentlyContinue'
    try {
        Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing -TimeoutSec 120 | Out-Null
        return $true
    } catch {
        return $false
    } finally {
        $ProgressPreference = $previous
    }
}

# Downloads and unpacks a release into $TMP\release; returns $false
# (without failing) when no suitable release exists.
function Download-Release([string]$Target) {
    $url = Get-ReleaseUrl $Target
    $archive = Join-Path $script:TmpDir "$Program.zip"
    if (-not (Invoke-Fetch $url $archive)) {
        Warn "no prebuilt release was found for $Target"
        return $false
    }
    $dest = $script:ReleaseDir
    try {
        if (Test-Path $dest) { Remove-Item -Recurse -Force $dest }
        New-Item -ItemType Directory -Path $dest -Force | Out-Null
        Expand-Archive -Path $archive -DestinationPath $dest -Force
    } catch {
        Warn 'the release archive could not be unpacked'
        return $false
    }
    return $true
}

# --------------------------------------------------------------------------
# Building from source
# --------------------------------------------------------------------------

function Build-FromSource {
    $src = $Source
    if (-not $src) {
        if ((Test-Path 'Cargo.toml') -and (Test-Path 'crates/fast-text-tui')) {
            $src = (Get-Location).Path
            Write-Log 'Building from the current checkout'
        } else {
            Need git
            $src = Join-Path $script:TmpDir 'src'
            Write-Log "Cloning https://github.com/$Repo.git"
            if ($Version -eq 'latest') {
                git clone --depth 1 "https://github.com/$Repo.git" $src
            } else {
                git clone --depth 1 --branch $Version "https://github.com/$Repo.git" $src
            }
            if ($LASTEXITCODE -ne 0) { Fail 'the clone failed' }
        }
    }
    if (-not (Test-Path $src)) { Fail "source directory '$src' does not exist" }
    if (-not (Test-Path (Join-Path $src 'Cargo.toml'))) { Fail "'$src' is not a Fast Text checkout" }
    Need cargo

    $cargoArgs = @('build', '--release', '-p', 'fast-text-tui', '-p', 'textwrite')
    if ($Gui) { $cargoArgs += @('-p', 'fast-text-gui') }
    Write-Log "cargo $($cargoArgs -join ' ')"
    if ($DryRun) {
        Write-Log "  would run in $src"
        return
    }

    Push-Location $src
    try {
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) { Fail 'the build failed' }
    } finally {
        Pop-Location
    }

    $releaseDir = $script:ReleaseDir
    New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null
    $built = Join-Path $src 'target\release'
    foreach ($name in ($Binaries + $GuiBinary)) {
        foreach ($candidate in @(("$name.exe"), $name)) {
            $path = Join-Path $built $candidate
            if (Test-Path $path) {
                Copy-Item $path (Join-Path $releaseDir $name) -Force
                break
            }
        }
    }
}

# --------------------------------------------------------------------------
# Installing files
# --------------------------------------------------------------------------

# Finds `$Name` or `$Name.exe` anywhere under `$Dir`.
function Find-Binary([string]$Name, [string]$Dir) {
    if (-not (Test-Path $Dir)) { return $null }
    $hit = Get-ChildItem -Path $Dir -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.BaseName -eq $Name } |
        Select-Object -First 1
    if ($hit) { return $hit.FullName }
    return $null
}

function Copy-Executable([string]$Source, [string]$Destination) {
    Copy-Item -Path $Source -Destination $Destination -Force
    try {
        # chmod +x for the sake of MSYS2/Git Bash callers.
        (Get-Item -Path $Destination).Attributes = (Get-Item -Path $Destination).Attributes
    } catch { }
}

function Install-Binaries {
    if ($DryRun) {
        if ($Gui) {
            Write-Log "Would install into $($script:BinDir): $($Binaries + $GuiBinary)"
        } else {
            Write-Log "Would install into $($script:BinDir): $($Binaries -join ' ')"
        }
        return
    }

    Write-Log "Installing executables into $($script:BinDir)"
    New-Item -ItemType Directory -Path $script:BinDir -Force | Out-Null

    $installed = @()
    foreach ($name in $Binaries) {
        $srcFile = Find-Binary $name $script:ReleaseDir
        if (-not $srcFile) {
            Warn "'$name' was not produced; skipping"
            continue
        }
        $dest = Join-Path $script:BinDir "$name.exe"
        Copy-Item -Path $srcFile -Destination $dest -Force
        $script:Manifest += $dest
        $installed += $name
    }

    if ($Gui) {
        $srcFile = Find-Binary $GuiBinary $script:ReleaseDir
        if (-not $srcFile) {
            Warn "'$GuiBinary' was not produced; skipping"
        } else {
            $dest = Join-Path $script:BinDir "$GuiBinary.exe"
            Copy-Item -Path $srcFile -Destination $dest -Force
            $script:Manifest += $dest
            $installed += $GuiBinary
        }
    }

    if ($installed.Count -eq 0) { Fail 'no executables were installed' }
    Write-Log "Installed: $($installed -join ' ')"
}

function Install-Completions {
    $dir = Join-Path $script:Prefix 'completions'
    $dest = Join-Path $dir 'fast-text.ps1'
    $content = @'
# Fast Text PowerShell tab completion.
# Add this line to your $PROFILE:
#   . "<Prefix>\completions\fast-text.ps1"
Register-ArgumentCompleter -CommandName fast-text, ft, textwrite, tw -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $flags = @(
        '-c', '--config', '-h', '--help', '-V', '--version',
        '-Prefix', '-BinDir', '-Version', '-Source', '-FromSource', '-Gui',
        '-NoCompletions', '-AddToPath', '-Uninstall', '-Check', '-Update',
        '-DryRun', '-Yes'
    )
    $flags |
        Where-Object { $_ -like "$wordToComplete*" } |
        ForEach-Object {
            [System.Management.Automation.CompletionResult]::new($_, $_, 'ParameterValue', $_)
        }
    if ($wordToComplete -notmatch '^-') {
        Get-ChildItem -File -Filter *.tw -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like "$wordToComplete*" } |
            ForEach-Object {
                [System.Management.Automation.CompletionResult]::new($_.Name, $_.Name, 'ParameterValue', $_.Name)
            }
    }
}
'@
    if ($DryRun) {
        Write-Log "Would write $dest"
        return
    }
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
    Set-Content -Path $dest -Value $content -Encoding UTF8
    $script:Manifest += $dest
    Write-Log "Tab completion installed at $dest"
    Write-Log "  enable it by adding this line to `"$PROFILE`":"
    Write-Log "    . `"$dest`""
}

function Save-Manifest {
    if ($DryRun) {
        Write-Log "Would write $($script:ManifestPath)"
        return
    }
    New-Item -ItemType Directory -Path (Split-Path -Parent $script:ManifestPath) -Force | Out-Null
    Set-Content -Path $script:ManifestPath -Value ($script:Manifest -join "`r`n") -Encoding UTF8
}

# --------------------------------------------------------------------------
# PATH
# --------------------------------------------------------------------------

function Add-BinToPath {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @()
    if ($userPath) {
        $entries = @($userPath -split ';' | Where-Object { $_ })
    }
    if ($entries | Where-Object { $_.TrimEnd('\') -ieq $script:BinDir.TrimEnd('\') }) {
        Write-Log "$($script:BinDir) is already on your PATH"
        return
    }
    if ($DryRun) {
        Write-Log "Would add $($script:BinDir) to your user PATH"
        return
    }
    $newPath = (@($entries) + $script:BinDir) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Log "Added $($script:BinDir) to your user PATH (open a new terminal to pick it up)"
}

# --------------------------------------------------------------------------
# Uninstall
# --------------------------------------------------------------------------

function Invoke-Uninstall {
    Write-Log "Uninstalling from $($script:Prefix)"
    $files = @()
    if (Test-Path $script:ManifestPath) {
        $files = @(Get-Content -Path $script:ManifestPath -ErrorAction SilentlyContinue |
            Where-Object { $_ })
    } else {
        Warn "no manifest at $($script:ManifestPath); removing the known executables instead"
        foreach ($name in ($Binaries + $GuiBinary)) {
            $files += Join-Path $script:BinDir "$name.exe"
        }
        $files += Get-VersionFilePath
    }

    foreach ($file in $files) {
        if (-not $file) { continue }
        if ($DryRun) {
            if (Test-Path $file) { Write-Log "  would remove $file" }
            continue
        }
        if (Test-Path $file) {
            Remove-Item -Path $file -Force
            Write-Log "  removed $file"
        }
    }

    if (-not $DryRun) {
        if (Test-Path $script:ManifestPath) { Remove-Item -Path $script:ManifestPath -Force }
        foreach ($dir in @((Join-Path $script:Prefix 'completions'), $script:BinDir)) {
            if ((Test-Path $dir) -and -not (Get-ChildItem -Path $dir -Force -ErrorAction SilentlyContinue)) {
                Remove-Item -Path $dir -Force
            }
        }
    }

    $configDir = Split-Path -Parent (Get-ConfigPath)
    Write-Log "Done. Your settings and plugins under $configDir were left alone."
}

# The editor reads its settings from $FAST_TEXT_CONFIG, else
# $XDG_CONFIG_HOME/fast-text/config.toml, else ~/.config/fast-text/config.toml.
function Get-ConfigPath {
    if ($env:FAST_TEXT_CONFIG) { return $env:FAST_TEXT_CONFIG }
    if ($env:XDG_CONFIG_HOME) { return (Join-Path $env:XDG_CONFIG_HOME 'fast-text\config.toml') }
    $homeDir = if ($env:USERPROFILE) { $env:USERPROFILE } else { $HOME }
    return (Join-Path (Join-Path $homeDir '.config') 'fast-text\config.toml')
}

# --------------------------------------------------------------------------
# Version checks and updates
# --------------------------------------------------------------------------

# Where the installed version is recorded. It is listed in the manifest, so
# -Uninstall removes it with the executables.
function Get-VersionFilePath {
    return (Join-Path $script:Prefix 'installed-version.txt')
}

# The version a previous install recorded, or $null when there is none.
function Get-InstalledVersion {
    $file = Get-VersionFilePath
    if (-not (Test-Path $file)) { return $null }
    $line = Get-Content -Path $file -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($line) { return ([string]$line).Trim() }
    return $null
}

# Returns 1 when $A is newer than $B, -1 when older, 0 when equal.
function Compare-Version([string]$A, [string]$B) {
    $pa = @(($A.TrimStart('v', 'V') -split '\.') | ForEach-Object {
            if ($_ -match '^\d+') { [int]$Matches[0] } else { 0 }
        })
    $pb = @(($B.TrimStart('v', 'V') -split '\.') | ForEach-Object {
            if ($_ -match '^\d+') { [int]$Matches[0] } else { 0 }
        })
    for ($i = 0; $i -lt 3; $i++) {
        $va = if ($i -lt $pa.Count) { $pa[$i] } else { 0 }
        $vb = if ($i -lt $pb.Count) { $pb[$i] } else { 0 }
        if ($va -gt $vb) { return 1 }
        if ($va -lt $vb) { return -1 }
    }
    return 0
}

# The newest release tag, from the GitHub API. FAST_TEXT_LATEST_TAG skips the
# network query (used by the offline tests).
function Get-LatestTag {
    if ($env:FAST_TEXT_LATEST_TAG) { return $env:FAST_TEXT_LATEST_TAG }
    $url = "https://api.github.com/repos/$Repo/releases/latest"
    $previous = $ProgressPreference
    $ProgressPreference = 'SilentlyContinue'
    try {
        $release = Invoke-RestMethod -Uri $url -UseBasicParsing -TimeoutSec 20
    } catch {
        Fail "could not reach GitHub to check for a newer release: $($_.Exception.Message)"
    } finally {
        $ProgressPreference = $previous
    }
    if ($release -and $release.tag_name) { return [string]$release.tag_name }
    Fail 'GitHub did not name a newest release'
}

# Records which version was installed, so later -Check/-Update runs and the
# editor's own update check have something to compare against.
function Write-VersionFile {
    $file = Get-VersionFilePath
    $version = $null
    $exe = Join-Path $script:BinDir "$Program.exe"
    if (-not $DryRun -and (Test-Path $exe)) {
        # Ask the freshly installed editor itself — the most reliable source.
        try {
            $out = & $exe --version 2>$null
            if ($out) { $version = ([string]$out).Trim() -replace '^\S+\s+', '' }
        } catch {
            $version = $null
        }
    }
    if (-not $version) {
        try { $version = Get-LatestTag } catch { $version = $null }
    }
    if (-not $version) {
        Warn 'could not work out which version was installed'
        return
    }
    $version = $version.TrimStart('v', 'V')
    if ($DryRun) {
        Write-Log "  would record version $version in $file"
        return
    }
    New-Item -ItemType Directory -Path (Split-Path -Parent $file) -Force | Out-Null
    Set-Content -Path $file -Value $version -Encoding UTF8
    $script:Manifest += $file
}

# Prints the installed version and the newest release, plus what to do next.
function Invoke-Check {
    $current = Get-InstalledVersion
    $latest = (Get-LatestTag).TrimStart('v', 'V')
    if (-not $current) {
        Write-Log "No Fast Text installation was found under $($script:Prefix)."
        Write-Log "The newest release is v$latest."
        Write-Log "Install it with: .\install.ps1 -Version v$latest -Prefix '$($script:Prefix)'"
        return
    }
    $current = $current.TrimStart('v', 'V')
    if ((Compare-Version $latest $current) -gt 0) {
        Write-Log "Update available: v$current is installed, v$latest has been released."
        Write-Log "Install it with: .\install.ps1 -Update -Prefix '$($script:Prefix)'"
    } else {
        Write-Log "Fast Text v$current is up to date (newest release: v$latest)."
    }
}

# Points $Version at the newest release when an update is actually warranted.
# Sets $script:UpToDate when there is nothing to do.
function Invoke-Update {
    if ($Version -ne 'latest') {
        Write-Log "Updating to $Version"
        return
    }
    $latest = (Get-LatestTag).TrimStart('v', 'V')
    $current = Get-InstalledVersion
    if ($current) { $current = $current.TrimStart('v', 'V') }
    if ($current -and (Compare-Version $latest $current) -le 0) {
        Write-Log "Fast Text v$current is already up to date (newest release: v$latest)."
        $script:UpToDate = $true
        return
    }
    if ($current) {
        Write-Log "Updating from v$current to v$latest"
    } else {
        Write-Log "Installing the newest release, v$latest"
    }
    $script:Version = "v$latest"

    # Refresh the desktop app too when an earlier run installed it.
    if (-not $Gui -and (Test-Path $script:ManifestPath)) {
        $hasGui = Get-Content -Path $script:ManifestPath -ErrorAction SilentlyContinue |
            Where-Object { $_ -like "*$GuiBinary*" }
        if ($hasGui) {
            $script:Gui = $true
            Write-Log 'The desktop app was installed before; it will be updated as well'
        }
    }
}

# --------------------------------------------------------------------------
# Main
# --------------------------------------------------------------------------

function main {
    if ($Help) {
        Show-Help
        return
    }

    Resolve-Dirs

    if ($Uninstall) {
        Invoke-Uninstall
        return
    }

    if ($Check) {
        Invoke-Check
        return
    }

    if ($Update) {
        Invoke-Update
        if ($script:UpToDate) { return }
    }

    $script:TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("$Program-install-" + [Guid]::NewGuid().ToString('N'))
    $script:ReleaseDir = Join-Path $script:TmpDir 'release'
    New-Item -ItemType Directory -Path $script:TmpDir -Force | Out-Null

    try {
        if ($DryRun) {
            Write-Log 'Dry run: nothing will be changed.'
        }

        $target = Get-Target
        if ($Source) { $FromSource = $true }

        if ($FromSource) {
            Write-Log "Building from source for $target"
            Build-FromSource
        } elseif ($DryRun) {
            Write-Log "Would try the prebuilt release at $(Get-ReleaseUrl $target)"
            Write-Log 'Falling back to a source build when it is missing'
            Build-FromSource
        } else {
            Write-Log "Looking for a prebuilt release for $target"
            if (-not (Download-Release $target)) {
                Write-Log 'Falling back to a source build'
                Build-FromSource
            }
        }

        Install-Binaries
        Write-VersionFile
        if (-not $NoCompletions) { Install-Completions }
        Save-Manifest
        if ($AddToPath) { Add-BinToPath }

        Write-Log 'Done.'
        Write-Log "Your config lives at $(Get-ConfigPath)"
        Write-Log "Uninstall later with: .\install.ps1 -Uninstall$(if ($Prefix) { " -Prefix '$Prefix'" })"
        if (-not $AddToPath) {
            Write-Log "Make sure $($script:BinDir) is on your PATH (rerun with -AddToPath to have it added)."
        }
    } finally {
        if ($script:TmpDir -and (Test-Path $script:TmpDir)) {
            Remove-Item -Path $script:TmpDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

main
