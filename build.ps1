param([string]$TargetDirectory = (Join-Path ([IO.Path]::GetTempPath()) 'PerformanceGlass-build'))
$ErrorActionPreference='Stop'
Push-Location $PSScriptRoot
try {
    # MinGW's linker needs an ASCII target path, even when the sources are in a Japanese folder.
    if ($TargetDirectory -match '[^\x00-\x7F]') { throw 'TargetDirectoryには英数字のみの絶対パスを指定してください。' }
    if (-not $env:WINDRES) {
        $resourceCompiler = Get-Command windres.exe -ErrorAction SilentlyContinue
        if ($resourceCompiler) { $env:WINDRES = $resourceCompiler.Source }
    }
    if (-not $env:WINDRES) { throw 'MinGWのbinをPATHに追加するか、WINDRESにwindres.exeのパスを設定してください。' }
    cargo build --release --locked --target-dir $TargetDirectory
    if($LASTEXITCODE -ne 0){throw 'ビルドに失敗しました。'}
    $destination=Join-Path $PSScriptRoot 'PerformanceGlass.exe'
    Copy-Item -LiteralPath (Join-Path $TargetDirectory 'release\performance-glass.exe') -Destination $destination -Force
    Write-Output $destination
} finally {Pop-Location}
