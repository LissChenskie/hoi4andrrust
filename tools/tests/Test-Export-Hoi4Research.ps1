# Integration smoke test for the Windows wrapper; Cargo is replaced by a local
# test command, so no installed HOI4, Rust build or proprietary data is needed.
$ErrorActionPreference = 'Stop'
$testRoot = Join-Path ([System.IO.Path]::GetTempPath()) ('hoi4-export-' + [guid]::NewGuid().ToString('N'))
$testProject = Join-Path $testRoot 'project with spaces'
$testGame = Join-Path $testRoot 'игра с пробелами'
$testCaller = Join-Path $testRoot 'caller outside project'
$testBin = Join-Path $testRoot 'commands'
$testUserData = Join-Path $testRoot 'user data'
$testLogWriter = $null
$testOriginalLocation = (Get-Location).ProviderPath
$testOriginalPath = $env:PATH
$testScript = Join-Path $testProject 'tools\Export-Hoi4Research.ps1'
$testSource = Join-Path (Split-Path -Parent $PSScriptRoot) 'Export-Hoi4Research.ps1'

function Assert-Equal($Expected, $Actual, [string]$Message) {
    if ($Expected -ne $Actual) { throw "$Message (expected: $Expected; actual: $Actual)" }
}
function Assert-Archive([string]$Output) {
    $archivePath = Join-Path $Output 'research-review.zip'
    if (!(Test-Path -LiteralPath $archivePath -PathType Leaf)) { throw "Missing archive: $archivePath" }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($archivePath)
    try {
        $names = @($archive.Entries | ForEach-Object { $_.FullName.Replace('\','/') })
        foreach ($expected in @('research-report.json','research-registry-v6.bin','research-review-sources/common/technologies/test.txt','research-review-sources/research-runtime-evidence.json')) {
            if ($names -notcontains $expected) { throw "Missing review archive entry: $expected" }
        }
    } finally { $archive.Dispose() }
}
function Assert-Call([string]$Project, [string]$Output) {
    $call = Get-Content -LiteralPath (Join-Path $Project 'cargo-call.json') -Raw | ConvertFrom-Json
    $auditIndex = [array]::IndexOf($call, 'audit')
    if ($auditIndex -ge 0) {
        Assert-Equal $Output $call[$auditIndex + 1] 'Wrong existing export directory'
        Assert-Equal -1 ([array]::IndexOf($call, '--out')) 'Reuse must not recopy the game'
        Assert-Equal -1 ([array]::IndexOf($call, '--scenario')) 'Reuse must preserve the previous scenario by default'
    } else {
        Assert-Equal $testGame $call[([array]::IndexOf($call, '--game') + 1)] 'Game path changed during argument forwarding'
        Assert-Equal $Output $call[([array]::IndexOf($call, '--out') + 1)] 'Wrong output directory'
    }
    Assert-Equal -1 ([array]::IndexOf($call, '--version')) 'EXE ProductVersion must not become the game patch'
    Assert-Equal $testCaller (Get-Location).ProviderPath 'Exporter did not restore caller location'
    Assert-Archive $Output
    $evidencePath = Join-Path $Output 'research-runtime-evidence.json'
    if ((Get-Item -LiteralPath $evidencePath).Length -gt 20000) { throw 'Evidence expanded beyond bounded plain strings' }
    $json = Get-Content -LiteralPath $evidencePath -Raw
    if ($json -match 'PSPath|PSParentPath|PSDrive|ImplementingType') { throw 'Provider metadata leaked into evidence' }
    $evidence = $json | ConvertFrom-Json
    Assert-Equal $false $evidence.active_dlc_verified 'Log evidence must not confirm the current DLC profile'
    Assert-Equal 3 @($evidence.logs[0].lines).Count 'Wrong evidence line selection'
    foreach ($line in $evidence.logs[0].lines) {
        if ($line -isnot [string] -or $line.Length -gt 4096) { throw 'Evidence must contain bounded strings' }
    }
    Assert-Equal 'Authored DLC' $evidence.disabled_dlcs[0] 'Disabled DLC evidence was lost'
    Assert-Equal 0 $evidence.enabled_mod_count 'Empty mod list must have count zero'
}

try {
    foreach ($dir in @($testProject, $testGame, $testCaller, $testBin, (Join-Path $testUserData 'logs'), (Split-Path -Parent $testScript))) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
    Copy-Item -LiteralPath $testSource -Destination $testScript
    Set-Content -LiteralPath (Join-Path $testProject 'Cargo.toml') -Value '# test project' -Encoding UTF8
    Copy-Item -LiteralPath $env:ComSpec -Destination (Join-Path $testGame 'hoi4.exe')
    [System.IO.File]::WriteAllLines((Join-Path $testUserData 'logs\system.log'), [string[]]@('Game version: authored fixture', 'unrelated line', 'Active DLC: Authored DLC', ('DLC: ' + ('x' * 12000))), [System.Text.UTF8Encoding]::new($false))
    [System.IO.File]::WriteAllText((Join-Path $testUserData 'dlc_load.json'), '{"disabled_dlcs":["Authored DLC"],"enabled_mods":[]}', [System.Text.UTF8Encoding]::new($false))
    # The original game may still be writing its journal during export.
    $testLogWriter = [System.IO.File]::Open((Join-Path $testUserData 'logs\system.log'), [System.IO.FileMode]::Open, [System.IO.FileAccess]::Write, [System.IO.FileShare]::ReadWrite)
    $cargoStub = @'
$ErrorActionPreference = 'Stop'
$call = @($args)
$call | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path (Get-Location).ProviderPath 'cargo-call.json') -Encoding UTF8
if (Test-Path -LiteralPath (Join-Path (Get-Location).ProviderPath 'fail-cargo')) {
    $global:LASTEXITCODE = 23
    return
}
$outIndex = [array]::IndexOf($call, '--out')
if ($outIndex -lt 0) { $outIndex = [array]::IndexOf($call, 'audit') }
if ($outIndex -lt 0) { throw 'Missing export or audit output' }
$outPath = $call[$outIndex + 1]
New-Item -ItemType Directory -Path (Join-Path $outPath 'cache') -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $outPath 'common\technologies') -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $outPath 'research-review-sources\common\technologies') -Force | Out-Null
Set-Content -LiteralPath (Join-Path $outPath 'research-review-sources\common\technologies\test.txt') -Value '# authored fixture' -Encoding UTF8
Set-Content -LiteralPath (Join-Path $outPath 'research-report.json') -Value '{"test_fixture":true}' -Encoding UTF8
[System.IO.File]::WriteAllBytes((Join-Path $outPath 'cache\research-registry-v6.bin'), [byte[]](1,2,3))
$global:LASTEXITCODE = 0
'@
    Set-Content -LiteralPath (Join-Path $testBin 'cargo.ps1') -Value $cargoStub -Encoding UTF8
    $env:PATH = $testBin + [System.IO.Path]::PathSeparator + $testOriginalPath
    Assert-Equal (Join-Path $testBin 'cargo.ps1') (Get-Command cargo).Source 'Cargo stub was not selected'
    Set-Location -LiteralPath $testCaller

    # The reported failure: omit ProjectPath, resolve the project from this file.
    & $testScript -GamePath $testGame -UserDataPath $testUserData -NoDlc
    Assert-Call $testProject (Join-Path $testProject 'data\hoi4')

    # Match the exact invocation recommended to the user: a fresh -File process.
    $testPowerShell = (Get-Process -Id $PID).Path
    & $testPowerShell -NoProfile -ExecutionPolicy Bypass -File $testScript -GamePath $testGame -UserDataPath $testUserData -NoDlc
    Assert-Equal 0 $LASTEXITCODE 'The -File process failed'
    Assert-Call $testProject (Join-Path $testProject 'data\hoi4')

    # An explicit override wins over the location of the exporter script.
    $alternateProject = Join-Path $testRoot 'alternate project'
    New-Item -ItemType Directory -Path $alternateProject | Out-Null
    Set-Content -LiteralPath (Join-Path $alternateProject 'Cargo.toml') -Value '# alternate test project' -Encoding UTF8
    & $testScript -ProjectPath $alternateProject -GamePath $testGame -UserDataPath $testUserData -OutputPath 'relative output' -NoDlc
    Assert-Call $alternateProject (Join-Path $alternateProject 'relative output')

    # Reimport existing data while the original installation is offline.
    & $testScript -ProjectPath $testProject -GamePath (Join-Path $testRoot 'offline game') -UserDataPath $testUserData -ReuseExport
    Assert-Call $testProject (Join-Path $testProject 'data\hoi4')

    # Compiler errors must propagate, while the caller's directory is restored.
    Set-Content -LiteralPath (Join-Path $testProject 'fail-cargo') -Value 'fail'
    $caughtFailure = $false
    try { & $testScript -GamePath $testGame -NoDlc } catch {
        if ($_.Exception.Message -notlike '*code 23*') { throw }
        $caughtFailure = $true
    }
    Assert-Equal $true $caughtFailure 'Cargo failure was swallowed'
    Assert-Equal $testCaller (Get-Location).ProviderPath 'Cargo failure changed caller location'
    Write-Host "Exporter wrapper checks passed in PowerShell $($PSVersionTable.PSVersion)."
    $global:LASTEXITCODE = 0
} finally {
    if ($null -ne $testLogWriter) { $testLogWriter.Dispose() }
    Set-Location -LiteralPath $testOriginalLocation
    $env:PATH = $testOriginalPath
    if (Test-Path -LiteralPath $testRoot) { Remove-Item -LiteralPath $testRoot -Recurse -Force }
}
