<#
.SYNOPSIS
    Installs Doze on Windows, or updates an existing installation in place.
.DESCRIPTION
    - If already installed, cleanly closes any running doze.exe, updates the binary & shortcuts,
      and preserves all user settings and local secrets in %APPDATA%\doze\Doze\.
    - Supports installing locally from source (-FromSource, default when run inside the repo)
      or downloading the latest release from GitHub (-Repo "owner/repo").
.EXAMPLE
    .\installer\install.ps1
.EXAMPLE
    .\installer\install.ps1 -Repo "your-org/doze"
.EXAMPLE
    .\installer\install.ps1 -FromSource -DesktopShortcut
#>
[CmdletBinding()]
param(
    [switch]$FromSource,
    [string]$Repo = "",
    [string]$Version = "latest",
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA "Programs\Doze"),
    [switch]$DesktopShortcut,
    [switch]$NoPath,
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$UninstallRegPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Doze"

function Stop-RunningDoze {
    $procs = Get-Process -Name "doze" -ErrorAction SilentlyContinue
    if ($procs) {
        Write-Host "Stopping running Doze instance(s)..." -ForegroundColor Yellow
        $procs | Stop-Process -Force -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 600
    }
}

function Remove-FromUserPath([string]$Dir) {
    $current = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $current) { return }
    $parts = $current -split ';' | Where-Object { $_ -and ($_.TrimEnd('\') -ine $Dir.TrimEnd('\')) }
    [Environment]::SetEnvironmentVariable("Path", ($parts -join ';'), "User")
}

function Add-ToUserPath([string]$Dir) {
    $current = [Environment]::GetEnvironmentVariable("Path", "User")
    $parts = @()
    if ($current) { $parts = $current -split ';' | Where-Object { $_ } }
    if (-not ($parts | Where-Object { $_.TrimEnd('\') -ieq $Dir.TrimEnd('\') })) {
        $parts += $Dir
        [Environment]::SetEnvironmentVariable("Path", ($parts -join ';'), "User")
        Write-Host "Added '$Dir' to user PATH." -ForegroundColor Cyan
    }
}

function New-AppShortcut([string]$ShortcutPath, [string]$TargetExe, [string]$IconPath) {
    $shell = New-Object -ComObject WScript.Shell
    $sc = $shell.CreateShortcut($ShortcutPath)
    $sc.TargetPath = $TargetExe
    $sc.WorkingDirectory = Split-Path $TargetExe -Parent
    $sc.Description = "Doze — REST / WebSocket / RPC Client"
    if (Test-Path $IconPath) { $sc.IconLocation = "$IconPath,0" }
    $sc.Save()
}

if ($Uninstall) {
    Stop-RunningDoze
    if (Test-Path $InstallDir) {
        Remove-Item -Recurse -Force $InstallDir
        Write-Host "Removed $InstallDir" -ForegroundColor Green
    }
    $startShortcut = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Doze.lnk"
    $deskShortcut = Join-Path ([Environment]::GetFolderPath("Desktop")) "Doze.lnk"
    if (Test-Path $startShortcut) { Remove-Item -Force $startShortcut }
    if (Test-Path $deskShortcut) { Remove-Item -Force $deskShortcut }
    if (Test-Path $UninstallRegPath) { Remove-Item -Recurse -Force $UninstallRegPath }
    Remove-FromUserPath $InstallDir
    Write-Host "Doze uninstalled. User settings/secrets in %APPDATA%\doze\Doze were preserved." -ForegroundColor Green
    exit 0
}

# Detect existing installation
$existingVersion = $null
if (Test-Path $UninstallRegPath) {
    $existingVersion = (Get-ItemProperty -Path $UninstallRegPath -ErrorAction SilentlyContinue).DisplayVersion
} elseif (Test-Path (Join-Path $InstallDir "doze.exe")) {
    $existingVersion = "unknown"
}

# Auto-select -FromSource when invoked inside the repository and no -Repo was passed
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir
if (-not $FromSource -and [string]::IsNullOrWhiteSpace($Repo)) {
    if (Test-Path (Join-Path $repoRoot "Cargo.toml")) {
        $FromSource = $true
    } else {
        throw "Please pass -Repo 'owner/repo' to download a GitHub Release, or run from inside the Doze repo with -FromSource."
    }
}

# Option 1: Install/Update from GitHub Releases
if (-not $FromSource) {
    $apiUrl = if ($Version -eq "latest") {
        "https://api.github.com/repos/$Repo/releases/latest"
    } else {
        "https://api.github.com/repos/$Repo/releases/tags/$Version"
    }
    Write-Host "Checking GitHub release at $apiUrl ..." -ForegroundColor Cyan
    $release = Invoke-RestMethod -Uri $apiUrl -Headers @{ "User-Agent" = "Doze-Installer" }
    $tagVer = $release.tag_name.TrimStart('v')

    if ($existingVersion) {
        Write-Host "Updating Doze ($existingVersion -> $tagVer)..." -ForegroundColor Cyan
    } else {
        Write-Host "Installing Doze $tagVer..." -ForegroundColor Cyan
    }

    $setupAsset = $release.assets | Where-Object { $_.name -like "Doze-Setup-*.exe" } | Select-Object -First 1
    if ($setupAsset) {
        $tmpSetup = Join-Path $env:TEMP $setupAsset.name
        Write-Host "Downloading $($setupAsset.name)..." -ForegroundColor Cyan
        Invoke-WebRequest -Uri $setupAsset.browser_download_url -OutFile $tmpSetup
        Stop-RunningDoze
        Write-Host "Running silent installer/updater..." -ForegroundColor Cyan
        $proc = Start-Process -FilePath $tmpSetup -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS" -Wait -PassThru
        Remove-Item -Force $tmpSetup -ErrorAction SilentlyContinue
        if ($proc.ExitCode -ne 0) {
            throw "Installer exited with code $($proc.ExitCode)"
        }
        Write-Host "Doze $tagVer is installed and ready!" -ForegroundColor Green
        exit 0
    }

    $zipAsset = $release.assets | Where-Object { $_.name -like "Doze-windows-*.zip" } | Select-Object -First 1
    if (-not $zipAsset) {
        throw "No Windows release asset found in $Repo ($($release.tag_name))."
    }

    $tmpZip = Join-Path $env:TEMP $zipAsset.name
    $tmpExtract = Join-Path $env:TEMP ("doze-extract-" + [Guid]::NewGuid().ToString("N"))
    Invoke-WebRequest -Uri $zipAsset.browser_download_url -OutFile $tmpZip
    Expand-Archive -Path $tmpZip -DestinationPath $tmpExtract -Force
    $sourceExe = Join-Path $tmpExtract "doze.exe"
    $sourceIco = Join-Path $tmpExtract "doze-icon.ico"
    $newVersion = $tagVer
} else {
    # Option 2: Build locally from source and install/update
    Push-Location $repoRoot
    try {
        $cargoToml = Get-Content -Raw (Join-Path $repoRoot "Cargo.toml")
        $newVersion = if ($cargoToml -match 'version\s*=\s*"([^"]+)"') { $Matches[1] } else { "0.1.0" }

        if ($existingVersion) {
            Write-Host "Building & updating Doze ($existingVersion -> $newVersion)..." -ForegroundColor Cyan
        } else {
            Write-Host "Building & installing Doze $newVersion..." -ForegroundColor Cyan
        }

        cargo build --release
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build --release failed"
        }
        $sourceExe = Join-Path $repoRoot "target\release\doze.exe"
        $sourceIco = Join-Path $repoRoot "assets\doze-icon.ico"
    } finally {
        Pop-Location
    }
}

if (-not (Test-Path $sourceExe)) {
    throw "Built/extracted executable not found at $sourceExe"
}

Stop-RunningDoze
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$destExe = Join-Path $InstallDir "doze.exe"
$destIco = Join-Path $InstallDir "doze-icon.ico"
Copy-Item -Force $sourceExe $destExe
if (Test-Path $sourceIco) {
    Copy-Item -Force $sourceIco $destIco
}

# Copy self as uninstaller helper
Copy-Item -Force $MyInvocation.MyCommand.Path (Join-Path $InstallDir "install.ps1")

# Start Menu shortcut (and optional Desktop shortcut)
$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null
New-AppShortcut (Join-Path $startMenuDir "Doze.lnk") $destExe $destIco
if ($DesktopShortcut) {
    $deskPath = Join-Path ([Environment]::GetFolderPath("Desktop")) "Doze.lnk"
    New-AppShortcut $deskPath $destExe $destIco
}

# Register in Windows Add/Remove Programs (tracks installed version for future updates!)
New-Item -Path $UninstallRegPath -Force | Out-Null
Set-ItemProperty -Path $UninstallRegPath -Name "DisplayName" -Value "Doze"
Set-ItemProperty -Path $UninstallRegPath -Name "DisplayVersion" -Value $newVersion
Set-ItemProperty -Path $UninstallRegPath -Name "Publisher" -Value "Doze Contributors"
Set-ItemProperty -Path $UninstallRegPath -Name "InstallLocation" -Value $InstallDir
Set-ItemProperty -Path $UninstallRegPath -Name "DisplayIcon" -Value "$destExe,0"
Set-ItemProperty -Path $UninstallRegPath -Name "UninstallString" -Value "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$InstallDir\install.ps1`" -Uninstall"
Set-ItemProperty -Path $UninstallRegPath -Name "NoModify" -Value 1 -Type DWord
Set-ItemProperty -Path $UninstallRegPath -Name "NoRepair" -Value 1 -Type DWord

if (-not $NoPath) {
    Add-ToUserPath $InstallDir
}

if ($tmpZip -and (Test-Path $tmpZip)) { Remove-Item -Force $tmpZip -ErrorAction SilentlyContinue }
if ($tmpExtract -and (Test-Path $tmpExtract)) { Remove-Item -Recurse -Force $tmpExtract -ErrorAction SilentlyContinue }

if ($existingVersion) {
    Write-Host "Successfully updated Doze to v$newVersion at $InstallDir" -ForegroundColor Green
} else {
    Write-Host "Successfully installed Doze v$newVersion to $InstallDir" -ForegroundColor Green
}
