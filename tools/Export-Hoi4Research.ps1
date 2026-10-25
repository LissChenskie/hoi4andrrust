[CmdletBinding()]
param(
    [string]$GamePath = 'E:\перекид с ссд\games\Hearts of Iron IV',
    [string]$ProjectPath = (Split-Path -Parent $PSScriptRoot),
    [string]$OutputPath,
    [ValidateSet('1936','1939')][string]$Scenario = '1936',
    [string[]]$EnabledDlc = @(),
    [switch]$NoDlc,
    [string[]]$DataLayers = @(),
    [string]$Checksum
)
$ErrorActionPreference = 'Stop'
$ProjectPath = [System.IO.Path]::GetFullPath($ProjectPath)
$GamePath = [System.IO.Path]::GetFullPath($GamePath)
if (!(Test-Path -LiteralPath (Join-Path $ProjectPath 'Cargo.toml'))) { throw 'ProjectPath must contain Cargo.toml.' }
if (!(Test-Path -LiteralPath (Join-Path $GamePath 'hoi4.exe'))) { throw "hoi4.exe not found in $GamePath" }
if (!$OutputPath) { $OutputPath = Join-Path $ProjectPath 'data\hoi4' }
elseif (![System.IO.Path]::IsPathRooted($OutputPath)) { $OutputPath = Join-Path $ProjectPath $OutputPath }
$OutputPath = [System.IO.Path]::GetFullPath($OutputPath)
$exe = Get-Item -LiteralPath (Join-Path $GamePath 'hoi4.exe')
$arguments = @('run','--release','--bin','hoi4-research','--','export','--game',$GamePath,'--out',$OutputPath,'--scenario',$Scenario)
$exeVersion = $exe.VersionInfo.ProductVersion
if ($exeVersion) { $arguments += @('--version',$exeVersion) }
$arguments += @('--exe-sha256',(Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash)
if ($Checksum) { $arguments += @('--checksum',$Checksum) }
if ($NoDlc -and $EnabledDlc.Count) { throw 'Choose NoDlc or EnabledDlc.' }
if ($NoDlc) { $arguments += '--no-dlc' }
foreach ($dlc in $EnabledDlc) { $arguments += @('--dlc',$dlc) }
foreach ($layer in $DataLayers) { $arguments += @('--layer',$layer) }
if ($DataLayers.Count) { $arguments += '--layers-verified' }
Push-Location -LiteralPath $ProjectPath
try {
    & cargo @arguments
    if ($LASTEXITCODE -ne 0) { throw "Exporter exited with code $LASTEXITCODE" }
} finally { Pop-Location }
if (!$NoDlc -and !$EnabledDlc.Count) {
    Write-Warning 'DLC selection is unknown. Re-run with -EnabledDlc and exact enabled DLC names, or -NoDlc. Installed DLC is not proof that it is active.'
}
Write-Host "Ready: $OutputPath\research-report.json"
Write-Host 'The HOI4 checksum cannot be calculated from hoi4.exe. Pass -Checksum only after reading it in the original game.'

$reviewFiles = @((Join-Path $OutputPath 'research-report.json'), (Join-Path $OutputPath 'cache\research-registry-v4.bin'))
$reviewPath = Join-Path $OutputPath 'research-review.zip'
Compress-Archive -LiteralPath $reviewFiles -DestinationPath $reviewPath -Force
Write-Host "For verification, attach: $reviewPath"
