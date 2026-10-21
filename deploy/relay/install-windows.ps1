<#
.SYNOPSIS
    Set up a KhataOS relay on this Windows PC. See docs/deploy-relay.md.

.DESCRIPTION
    Uses a khata-server.exe you already have; this script downloads nothing.
    It:
      1. copies khata-server.exe into a folder of its own
         (default: %LOCALAPPDATA%\KhataOS-Relay) and creates the data folder;
      2. makes that folder readable only by you;
      3. creates a random admin key in admin.key (kept if it already exists);
      4. writes start-relay.ps1, which starts the relay with that key;
      5. registers the scheduled task "KhataOS Relay" to run it: at startup
         when this script runs as Administrator, otherwise when you sign in;
      6. starts it, checks it answers, and prints the next steps.
    Running it again (for example with a newer khata-server.exe) keeps the
    admin key and the data.

.PARAMETER ServerExe
    Path of khata-server.exe.

.PARAMETER InstallDir
    Folder for the relay's program, key, data and log.

.PARAMETER Port
    Port on this PC (only this PC can connect; Cloudflare Tunnel forwards to it).

.PARAMETER OpenSignup
    Let anyone who knows the relay's address create encrypted companies
    (within the relay's daily and storage limits). Without it, people need
    the sign-up key (the admin key) to create a company.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File .\install-windows.ps1 -ServerExe "$HOME\Downloads\khata-server.exe"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ServerExe,
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'KhataOS-Relay'),
    [ValidateRange(1024, 65535)]
    [int]$Port = 8787,
    [switch]$OpenSignup
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3

$TaskName = 'KhataOS Relay'
$User = [System.Security.Principal.WindowsIdentity]::GetCurrent().Name
$IsAdmin = ([System.Security.Principal.WindowsPrincipal][System.Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [System.Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not (Test-Path -LiteralPath $ServerExe -PathType Leaf)) {
    throw "khata-server.exe not found at: $ServerExe"
}
$ServerExe = (Resolve-Path -LiteralPath $ServerExe).Path
$Exe = Join-Path $InstallDir 'khata-server.exe'
$DataDir = Join-Path $InstallDir 'data'
$KeyFile = Join-Path $InstallDir 'admin.key'
$Starter = Join-Path $InstallDir 'start-relay.ps1'

function Set-OnlyMe([string]$Path, [string]$Grant) {
    # Remove inherited access and give this user alone full control.
    & icacls.exe $Path /inheritance:r /grant:r "${User}:$Grant" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "could not restrict access to $Path" }
}

# 1. Stop a running copy, then put the program in place.
if (Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) {
    Stop-ScheduledTask -TaskName $TaskName
}
Get-Process -Name 'khata-server' -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -eq $Exe } |
    Stop-Process -Force
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
if ($ServerExe -ne $Exe) {
    Copy-Item -LiteralPath $ServerExe -Destination $Exe -Force
}

# 2. The folder holds the key and the data: only this user may open it.
Set-OnlyMe $InstallDir '(OI)(CI)F'

# 3. The admin key (also the sign-up key). Never printed.
$NewKey = -not (Test-Path -LiteralPath $KeyFile)
if ($NewKey) {
    $bytes = New-Object byte[] 32
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    $rng.GetBytes($bytes)
    $rng.Dispose()
    [System.IO.File]::WriteAllText($KeyFile, (-join ($bytes | ForEach-Object { $_.ToString('x2') })))
}
Set-OnlyMe $KeyFile 'F'

# 4. The starter: reads the key and runs the relay, logging to relay.log.
$Open = if ($OpenSignup) { '1' } else { '0' }
@"
# Starts the KhataOS relay. Written by install-windows.ps1 and run by the
# "$TaskName" scheduled task. Messages since the last start: relay.log.
`$here = Split-Path -Parent `$MyInvocation.MyCommand.Path
`$env:KHATA_ADMIN_KEY = ([System.IO.File]::ReadAllText((Join-Path `$here 'admin.key'))).Trim()
`$env:KHATA_DATA_DIR = Join-Path `$here 'data'
`$env:KHATA_BIND = '127.0.0.1:$Port'
`$env:KHATA_OPEN_RELAY = '$Open'
# cloudflared on this PC tells the relay each client's address.
`$env:KHATA_TRUST_PROXY = '1'
`$p = Start-Process -FilePath (Join-Path `$here 'khata-server.exe') -NoNewWindow -Wait -PassThru ``
    -RedirectStandardError (Join-Path `$here 'relay.log') -RedirectStandardOutput (Join-Path `$here 'relay-output.log')
exit `$p.ExitCode
"@ | Set-Content -LiteralPath $Starter -Encoding UTF8

# 5. The scheduled task. No time limit, restart if it stops.
$Action = New-ScheduledTaskAction -Execute 'powershell.exe' -WorkingDirectory $InstallDir `
    -Argument "-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$Starter`""
$Settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable `
    -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1)
if ($IsAdmin) {
    # Runs at startup as this user, signed in or not (no password stored).
    $Trigger = New-ScheduledTaskTrigger -AtStartup
    $Principal = New-ScheduledTaskPrincipal -UserId $User -LogonType S4U -RunLevel Limited
    $When = 'whenever this PC starts'
} else {
    $Trigger = New-ScheduledTaskTrigger -AtLogOn -User $User
    $Principal = New-ScheduledTaskPrincipal -UserId $User -LogonType Interactive -RunLevel Limited
    $When = "when $User signs in (run this script as Administrator to start it at boot instead)"
}
Register-ScheduledTask -TaskName $TaskName -Action $Action -Trigger $Trigger -Principal $Principal `
    -Settings $Settings -Description 'KhataOS relay: end-to-end encrypted sync (docs/deploy-relay.md)' -Force | Out-Null

# 6. Start it and check.
Start-ScheduledTask -TaskName $TaskName
$Ok = $false
foreach ($i in 1..10) {
    Start-Sleep -Seconds 1
    try {
        $r = Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 -Uri "http://127.0.0.1:$Port/v1/health"
        if ($r.Content -eq 'ok') { $Ok = $true; break }
    } catch { }
}

Write-Host ''
if ($Ok) {
    Write-Host "The KhataOS relay is running on this PC (http://127.0.0.1:$Port). It starts $When." -ForegroundColor Green
} else {
    Write-Host "The relay did not answer yet. Look at $(Join-Path $InstallDir 'relay.log')." -ForegroundColor Yellow
}
Write-Host ''
Write-Host 'Next steps:'
Write-Host "  1. Make it reachable over HTTPS with Cloudflare Tunnel (docs/deploy-relay.md, part A):"
Write-Host '       winget install --id Cloudflare.cloudflared'
Write-Host "       then connect a tunnel's public hostname to http://localhost:$Port"
Write-Host "  2. In KhataOS: Exchange (Alt+Z) > Configure > Relay address = your https:// address."
if ($OpenSignup) {
    Write-Host '     Anyone with the address can create encrypted companies, within the daily and storage limits.'
} else {
    Write-Host "     When KhataOS asks for a sign-up key, open $KeyFile in Notepad and paste it."
    Write-Host '     Give it only to people who may create companies on this relay.'
}
if ($NewKey) {
    Write-Host "  3. Keep a copy of $KeyFile somewhere safe (a password manager), and back up"
} else {
    Write-Host "  3. The existing admin key was kept. Back up"
}
Write-Host "     $DataDir regularly (docs/deploy-relay.md, Backups)."
