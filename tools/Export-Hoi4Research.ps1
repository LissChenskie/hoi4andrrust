[CmdletBinding()]
param(
    [string]$GamePath = 'E:\перекид с ссд\games\Hearts of Iron IV',
    [string]$ProjectPath,
    [string]$OutputPath,
    [switch]$ReuseExport,
    [string]$UserDataPath,
    [ValidateSet('1936','1939')][string]$Scenario = '1936',
    [string[]]$EnabledDlc = @(),
    [switch]$NoDlc,
    [switch]$AllInstalledDlc,
    [string[]]$DataLayers = @(),
    [string]$Checksum
)
$ErrorActionPreference = 'Stop'
# Script-scoped automatic variables can be empty while parameter defaults bind
# in Windows PowerShell. Resolve the project only after entering the script body.
if ([string]::IsNullOrWhiteSpace($ProjectPath)) {
    $exportScriptPath = $PSCommandPath
    if ([string]::IsNullOrWhiteSpace($exportScriptPath)) { $exportScriptPath = $MyInvocation.MyCommand.Path }
    if (![string]::IsNullOrWhiteSpace($exportScriptPath)) {
        $exportScriptPath = [System.IO.Path]::GetFullPath($exportScriptPath)
        $ProjectPath = Split-Path -Parent (Split-Path -Parent $exportScriptPath)
    } elseif (Test-Path -LiteralPath (Join-Path (Get-Location).ProviderPath 'Cargo.toml') -PathType Leaf) {
        $ProjectPath = (Get-Location).ProviderPath
    } else {
        throw 'Cannot locate the project. Pass -ProjectPath with the folder containing Cargo.toml.'
    }
}
$ProjectPath = [System.IO.Path]::GetFullPath($ProjectPath)
$GamePath = [System.IO.Path]::GetFullPath($GamePath)
if (!(Test-Path -LiteralPath (Join-Path $ProjectPath 'Cargo.toml'))) { throw 'ProjectPath must contain Cargo.toml.' }
if (!$OutputPath) { $OutputPath = Join-Path $ProjectPath 'data\hoi4' }
elseif (![System.IO.Path]::IsPathRooted($OutputPath)) { $OutputPath = Join-Path $ProjectPath $OutputPath }
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
if ($ReuseExport) {
    if ($DataLayers.Count) { throw 'ReuseExport keeps the existing layers. Omit DataLayers or make a fresh export.' }
    if (!(Test-Path -LiteralPath (Join-Path $OutputPath 'common\technologies') -PathType Container)) { throw "No existing export in $OutputPath" }
    $arguments = @('run','--release','--bin','hoi4-research','--','audit',$OutputPath)
    if (Test-Path -LiteralPath $GamePath -PathType Container) { $arguments += @('--game',$GamePath) }
} else {
    if (!(Test-Path -LiteralPath (Join-Path $GamePath 'hoi4.exe') -PathType Leaf)) { throw "hoi4.exe not found in $GamePath" }
    $arguments = @('run','--release','--bin','hoi4-research','--','export','--game',$GamePath,'--out',$OutputPath)
}
if (!$ReuseExport -or $PSBoundParameters.ContainsKey('Scenario')) { $arguments += @('--scenario',$Scenario) }
if (Test-Path -LiteralPath (Join-Path $GamePath 'hoi4.exe') -PathType Leaf) {
    $exe = Get-Item -LiteralPath (Join-Path $GamePath 'hoi4.exe')
    $exeVersion = $exe.VersionInfo.ProductVersion
    if ($exeVersion) { $arguments += @('--exe-version',$exeVersion) }
    $arguments += @('--exe-sha256',(Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash)
}
if ($Checksum) { $arguments += @('--checksum',$Checksum) }
if (($NoDlc -and $AllInstalledDlc) -or ($EnabledDlc.Count -gt 0 -and ($NoDlc -or $AllInstalledDlc))) { throw 'Choose one of NoDlc, EnabledDlc, AllInstalledDlc.' }
if ($NoDlc) { $arguments += '--no-dlc' }
if ($AllInstalledDlc) { $arguments += '--all-installed-dlc' }
foreach ($dlc in $EnabledDlc) { $arguments += @('--dlc',$dlc) }
foreach ($layer in $DataLayers) { $arguments += @('--layer',$layer) }
if ($DataLayers.Count) { $arguments += '--layers-verified' }
Push-Location -LiteralPath $ProjectPath
try {
    & cargo @arguments
    if ($LASTEXITCODE -ne 0) { throw "Exporter exited with code $LASTEXITCODE" }
} finally { Pop-Location }
$dlcVerified = $NoDlc -or $AllInstalledDlc -or $EnabledDlc.Count -gt 0
$profilePath = Join-Path $OutputPath 'research-import.json'
if (Test-Path -LiteralPath $profilePath -PathType Leaf) {
    $profile = Get-Content -LiteralPath $profilePath -Raw | ConvertFrom-Json
    $dlcVerified = $profile.dlc_selection_verified
}
if (!$dlcVerified) {
    Write-Warning 'DLC selection is unknown. Use -AllInstalledDlc only if all installed DLC are enabled, -EnabledDlc with exact names, or -NoDlc.'
}
Write-Host "Ready: $OutputPath\research-report.json"
Write-Host 'The HOI4 checksum cannot be calculated from hoi4.exe. Pass -Checksum only after reading it in the original game.'

# Logs may be from an earlier launch. Keep them as evidence, not proof of active DLC.
if (!$UserDataPath) {
    $documents = [Environment]::GetFolderPath('MyDocuments')
    if ($documents) { $UserDataPath = Join-Path $documents 'Paradox Interactive\Hearts of Iron IV' }
}
$evidence = [ordered]@{ collected_at_utc = [DateTime]::UtcNow.ToString('o'); active_dlc_verified = $false; logs = @(); disabled_dlcs = @(); enabled_mod_count = $null }
if ($UserDataPath -and (Test-Path -LiteralPath $UserDataPath -PathType Container)) {
    foreach ($name in @('system.log','game.log')) {
        $logPath = Join-Path $UserDataPath ('logs\' + $name)
        if (Test-Path -LiteralPath $logPath -PathType Leaf) {
            try {
                # Get-Content decorates strings with provider/drive properties.
                # Windows PowerShell serializes those properties recursively.
                # StreamReader returns plain strings and bounds the evidence.
                $lines = [System.Collections.Generic.List[string]]::new()
                $stream = [System.IO.File]::Open($logPath, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Read, [System.IO.FileShare]::ReadWrite)
                try {
                    $reader = [System.IO.StreamReader]::new($stream, [System.Text.Encoding]::UTF8, $true)
                    try {
                        for ($lineIndex = 0; $lineIndex -lt 1000; $lineIndex++) {
                            $line = $reader.ReadLine()
                            if ($null -eq $line) { break }
                            if ($line -match '(?i)(game version|version:|checksum|dlc)') {
                                $lines.Add($line.Substring(0, [Math]::Min(4096, $line.Length)))
                                if ($lines.Count -ge 100) { break }
                            }
                        }
                    } finally { $reader.Dispose() }
                } finally { $stream.Dispose() }
                $evidence.logs += [ordered]@{ name = $name; modified_utc = (Get-Item -LiteralPath $logPath).LastWriteTimeUtc.ToString('o'); lines = $lines.ToArray() }
            } catch { Write-Warning "Could not read optional $name evidence: $($_.Exception.Message)" }
        }
    }
    $dlcLoadPath = Join-Path $UserDataPath 'dlc_load.json'
    if (Test-Path -LiteralPath $dlcLoadPath -PathType Leaf) {
        try {
            $dlcLoad = Get-Content -LiteralPath $dlcLoadPath -Raw | ConvertFrom-Json
            $evidence.disabled_dlcs = @($dlcLoad.disabled_dlcs)
            $evidence.enabled_mod_count = @($dlcLoad.enabled_mods | Where-Object { $_ }).Count
        } catch { Write-Warning "Could not read optional DLC evidence: $($_.Exception.Message)" }
    }
}
$evidencePath = Join-Path $OutputPath 'research-runtime-evidence.json'
[System.IO.File]::WriteAllText($evidencePath, ($evidence | ConvertTo-Json -Depth 8), [System.Text.UTF8Encoding]::new($false))
$sourcePath = Join-Path $OutputPath 'research-review-sources'
Copy-Item -LiteralPath $evidencePath -Destination (Join-Path $sourcePath 'research-runtime-evidence.json') -Force
$reviewFiles = @((Join-Path $OutputPath 'research-report.json'), (Join-Path $OutputPath 'cache\research-registry-v6.bin'), $sourcePath)
$reviewPath = Join-Path $OutputPath 'research-review.zip'
Compress-Archive -LiteralPath $reviewFiles -DestinationPath $reviewPath -Force
Write-Host "For verification, attach: $reviewPath"
