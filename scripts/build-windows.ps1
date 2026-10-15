param([string]$OutputDirectory = '')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$previous = @{}
foreach ($name in @('PATH', 'CARGO_HOME', 'RUSTUP_HOME', 'RUSTFLAGS')) {
    $previous[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
Push-Location $projectRoot
try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        $portableCargo = Join-Path $projectRoot '.tools\cargo\bin\cargo.exe'
        if (-not (Test-Path -LiteralPath $portableCargo)) { throw '请先安装 Rust stable MSVC 工具链。' }
        $env:CARGO_HOME = Join-Path $projectRoot '.tools\cargo'
        $env:RUSTUP_HOME = Join-Path $projectRoot '.tools\rustup'
        $env:PATH = (Split-Path $portableCargo) + ';' + $env:PATH
    }
    if ($env:RUSTFLAGS -notmatch '\+crt-static') { $env:RUSTFLAGS = (($env:RUSTFLAGS + ' -C target-feature=+crt-static').Trim()) }
    & npm.cmd run tauri build -- --target x86_64-pc-windows-msvc --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw 'Windows 打包失败；请检查上方编译错误，以及 MSVC C++ 工具和 Windows SDK。' }
    $config = Get-Content -LiteralPath 'src-tauri\tauri.conf.json' -Raw | ConvertFrom-Json
    $metadataText = & cargo metadata --manifest-path src-tauri/Cargo.toml --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw '无法定位构建输出。' }
    $metadata = $metadataText | ConvertFrom-Json
    $releaseRoot = Join-Path $metadata.target_directory 'x86_64-pc-windows-msvc\release'
    if (-not $OutputDirectory) { $OutputDirectory = Join-Path $projectRoot ("releases\" + $config.version + '\windows-x64') }
    [IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
    $installer = Get-ChildItem -LiteralPath (Join-Path $releaseRoot 'bundle\nsis') -Filter '*-setup.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $installer) { throw '未找到 NSIS 安装包。' }
    Copy-Item -LiteralPath $installer.FullName -Destination $OutputDirectory
    Copy-Item -LiteralPath (Join-Path $releaseRoot 'ai-selection-assistant.exe') -Destination (Join-Path $OutputDirectory 'AI-Selection-Assistant.exe')
    Copy-Item -LiteralPath 'LICENSE','THIRD_PARTY_NOTICES.md' -Destination $OutputDirectory
    Copy-Item -LiteralPath 'public\licenses\NotoSansSC-OFL.txt' -Destination $OutputDirectory
    $checksums = Get-ChildItem -LiteralPath $OutputDirectory -Filter '*.exe' | ForEach-Object {
        $hash = Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256
        $hash.Hash.ToLowerInvariant() + '  ' + $_.Name
    }
    [IO.File]::WriteAllLines((Join-Path $OutputDirectory 'SHA256SUMS.txt'), $checksums, [Text.UTF8Encoding]::new($false))
    Write-Host "Windows x64 安装包和直接运行版：$OutputDirectory"
} finally {
    Pop-Location
    foreach ($name in $previous.Keys) { [Environment]::SetEnvironmentVariable($name, $previous[$name], 'Process') }
}
