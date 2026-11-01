# install.ps1 end to end with no GitHub release: serves a release-layout .zip on 127.0.0.1 (built beforehand with
# scripts/bench/package_like_release.sh into <Srv>\good\<tag>\, plus a copy with a wrong .sha256 in <Srv>\bad\<tag>\) and
# installs it with -DownloadBase, then the `irm | iex` form (environment variables), then the tampered copy (must fail).
#   pwsh -File scripts/bench/test_install_ps1.ps1 -Srv <dir> [-Tag v0.2.0] [-AddToPath]
#   powershell -ExecutionPolicy Bypass -File scripts/bench/test_install_ps1.ps1 -Srv <dir>
param([Parameter(Mandatory = $true)][string]$Srv, [string]$Tag = 'v0.14.0', [switch]$AddToPath)
$ErrorActionPreference = 'Stop'
"PowerShell $($PSVersionTable.PSVersion) ($($PSVersionTable.PSEdition))"
$oldPath = $env:Path
$oldUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$l = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0); $l.Start(); $port = $l.LocalEndpoint.Port; $l.Stop()
$server = Start-Process -FilePath python -ArgumentList @('scripts/bench/serve.py', $Srv, "$port") -PassThru -WindowStyle Hidden
$base = "http://127.0.0.1:$port"
try {
    for ($i = 0; $i -lt 50; $i++) { try { Invoke-WebRequest -UseBasicParsing -Uri "$base/" -TimeoutSec 2 | Out-Null; break } catch { Start-Sleep -Milliseconds 200 } }
    $root = Join-Path ([IO.Path]::GetTempPath()) ('probbit-ps-test-' + [guid]::NewGuid().ToString('N'))

    "== 1. & .\install.ps1 -DownloadBase $base/good -Version $Tag -InstallDir <tmp>\bin1$(if ($AddToPath) { ' -AddToPath' })"
    $d1 = Join-Path $root 'bin1'
    if ($AddToPath) { & .\install.ps1 -DownloadBase "$base/good" -Version $Tag -InstallDir $d1 -AddToPath }
    else { & .\install.ps1 -DownloadBase "$base/good" -Version $Tag -InstallDir $d1 }
    $cmd = Get-Command probbit -ErrorAction Stop
    "`$ (Get-Command probbit).Source: $($cmd.Source)"
    if ((Resolve-Path $cmd.Source).Path -ne (Resolve-Path (Join-Path $d1 'probbit.exe')).Path) { throw "FAIL: probbit resolves to $($cmd.Source)" }
    "`$ probbit version: $(probbit version)"
    $how = 'probbit demo --tasks 12 | probbit decide'
    $d = probbit demo --tasks 12 | probbit decide | ConvertFrom-Json
    "`$ $how | ConvertFrom-Json: exit $LASTEXITCODE, verdict $($d.verdict), released $(@($d.released).Count) of $($d.tasks)"
    if ($LASTEXITCODE -ne 0 -or $d.verdict -ne 'exact') { throw 'FAIL: demo pipeline' }
    if ($AddToPath) {
        $u = [Environment]::GetEnvironmentVariable('Path', 'User')
        if (-not ($u -split ';' -contains $d1)) { throw "FAIL: -AddToPath did not add $d1 to the user PATH" }
        "ok: user PATH now starts with $(($u -split ';')[0])"
    }

    '== 2. irm | iex form: PROBBIT_* environment variables, script text through Invoke-Expression'
    $d2 = Join-Path $root 'bin2'
    $env:PROBBIT_DOWNLOAD_BASE = "$base/good"; $env:PROBBIT_VERSION = $Tag.TrimStart('v'); $env:PROBBIT_INSTALL_DIR = $d2
    try { Get-Content -Raw .\install.ps1 | Invoke-Expression }
    finally { Remove-Item Env:PROBBIT_DOWNLOAD_BASE, Env:PROBBIT_VERSION, Env:PROBBIT_INSTALL_DIR }
    if (-not (Test-Path (Join-Path $d2 'probbit.exe'))) { throw "FAIL: $d2\probbit.exe missing" }
    "ok: $(& (Join-Path $d2 'probbit.exe') version) in $d2"

    '== 3. a wrong .sha256 must fail and install nothing'
    $d3 = Join-Path $root 'bin3'
    $failed = $false
    try { & .\install.ps1 -DownloadBase "$base/bad" -Version $Tag -InstallDir $d3 } catch { $failed = $true; "rejected: $($_.Exception.Message)" }
    if (-not $failed) { throw 'FAIL: a wrong checksum was accepted' }
    if (Test-Path (Join-Path $d3 'probbit.exe')) { throw 'FAIL: probbit.exe installed despite the checksum mismatch' }

    '== 4. -DownloadBase without -Version must fail'
    $failed = $false
    try { & .\install.ps1 -DownloadBase "$base/good" -InstallDir (Join-Path $root 'bin4') } catch { $failed = $true; "rejected: $($_.Exception.Message)" }
    if (-not $failed) { throw 'FAIL: accepted' }

    '== 5. failed update preserves an existing install'
    $before = (Get-FileHash -LiteralPath (Join-Path $d1 'probbit.exe') -Algorithm SHA256).Hash
    $failed = $false
    try { & .\install.ps1 -DownloadBase "$base/bad" -Version $Tag -InstallDir $d1 } catch { $failed = $true }
    if (-not $failed) { throw 'FAIL: accepted damaged update' }
    if ((Get-FileHash -LiteralPath (Join-Path $d1 'probbit.exe') -Algorithm SHA256).Hash -ne $before) { throw 'FAIL: existing install changed' }

    '== 6. valid replacement uses the same destination'
    & .\install.ps1 -DownloadBase "$base/good" -Version $Tag -InstallDir $d1
    if ((Get-FileHash -LiteralPath (Join-Path $d1 'probbit.exe') -Algorithm SHA256).Hash -ne $before) { throw 'FAIL: replacement changed the binary' }
    "install.ps1: all checks passed (PowerShell $($PSVersionTable.PSVersion))"
} finally {
    Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
    $env:Path = $oldPath
    if ($AddToPath) { [Environment]::SetEnvironmentVariable('Path', $oldUserPath, 'User') }
    if ($root -and (Test-Path -LiteralPath $root)) { Remove-Item -LiteralPath $root -Recurse -Force }
}
