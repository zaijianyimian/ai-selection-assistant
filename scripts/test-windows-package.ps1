param([Parameter(Mandatory=$true)][string]$ReleaseDirectory)
$ErrorActionPreference = 'Stop'
$release = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
$version = (Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
$portable = Join-Path $release 'AI-Selection-Assistant.exe'
$installers = @(Get-ChildItem -LiteralPath $release -Filter '*-setup.exe')
if ($installers.Count -ne 1 -or -not (Test-Path -LiteralPath $portable)) { throw 'Expected one NSIS installer and one portable EXE.' }
$checked = @()
function Test-Startup([string]$Executable) {
    $process = Start-Process -FilePath $Executable -PassThru
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            Start-Sleep -Milliseconds 500
            $process.Refresh()
            if ($process.HasExited) { throw "Application exited during startup: $($process.ExitCode)" }
            if ($process.MainWindowTitle -like '*AI Selection Assistant*') { return }
        } while ([DateTime]::UtcNow -lt $deadline)
        throw 'Application did not create its main window.'
    } finally {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force; $process.WaitForExit() }
    }
}
foreach ($file in @($portable, $installers[0].FullName)) {
    $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($file)
    if ($info.ProductVersion -ne $version -and -not $info.ProductVersion.StartsWith($version + '.')) { throw "Version mismatch in $file : $($info.ProductVersion)" }
}
Test-Startup $portable
$checked += 'Portable EXE creates main window'
$installRoot = Join-Path $env:RUNNER_TEMP ('asa-install-test-' + [Guid]::NewGuid().ToString('N'))
$install = Start-Process -FilePath $installers[0].FullName -ArgumentList @('/S', "/D=$installRoot") -Wait -PassThru
if ($install.ExitCode -ne 0) { throw "NSIS install failed: $($install.ExitCode)" }
$installed = Join-Path $installRoot 'ai-selection-assistant.exe'
if (-not (Test-Path -LiteralPath $installed)) { throw 'Installed executable missing.' }
$checked += 'NSIS silent installation succeeds'
Test-Startup $installed
$checked += 'Installed EXE creates main window'
$uninstaller = @(Get-ChildItem -LiteralPath $installRoot -Filter '*uninstall*.exe')
if ($uninstaller.Count -ne 1) { throw 'Uninstaller missing.' }
$uninstall = Start-Process -FilePath $uninstaller[0].FullName -ArgumentList @('/S', "_?=$installRoot") -Wait -PassThru
if ($uninstall.ExitCode -ne 0) { throw "NSIS uninstall failed: $($uninstall.ExitCode)" }
if (Test-Path -LiteralPath $installed) { throw 'Application remains after uninstall.' }
$checked += 'NSIS silent uninstall removes application'
$result = @{ version=$version; platform='windows-x64'; commit=$env:GITHUB_SHA; checks=$checked; manualDesktopAcceptance='pending' }
$result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $release 'windows-package-verification.json') -Encoding utf8
$result | ConvertTo-Json -Depth 5 | Write-Host
