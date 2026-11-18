param(
    [string]$TargetDirectory = (Join-Path ([IO.Path]::GetTempPath()) 'PerformanceGlass-build'),
    [ValidateSet('x86_64-pc-windows-gnu','x86_64-pc-windows-msvc')]
    [string]$Target = 'x86_64-pc-windows-gnu'
)
$ErrorActionPreference='Stop'
$savedRustFlags = $env:RUSTFLAGS
Push-Location $PSScriptRoot
try {
    # MinGW's linker needs an ASCII target path, even when the sources are in a Japanese folder.
    if ($Target -eq 'x86_64-pc-windows-gnu' -and $TargetDirectory -match '[^\x00-\x7F]') { throw 'TargetDirectoryには英数字のみの絶対パスを指定してください。' }
    if ($Target -eq 'x86_64-pc-windows-gnu') {
        if (-not $env:WINDRES) {
            $resourceCompiler = Get-Command windres.exe -ErrorAction SilentlyContinue
            if ($resourceCompiler) { $env:WINDRES = $resourceCompiler.Source }
        }
        if (-not $env:WINDRES) { throw 'MinGWのbinをPATHに追加するか、WINDRESにwindres.exeのパスを設定してください。' }
    } else {
        if (-not $env:RC) {
            $resourceCompiler = Get-Command rc.exe -ErrorAction SilentlyContinue
            if ($resourceCompiler) { $env:RC = $resourceCompiler.Source }
        }
        if (-not $env:RC) { throw 'Windows SDKを利用できる開発環境で実行するか、RCにrc.exeのパスを設定してください。' }
        $env:RUSTFLAGS = ($env:RUSTFLAGS + ' -C target-feature=+crt-static').Trim()
    }
    cargo build --release --locked --target $Target --target-dir $TargetDirectory
    if($LASTEXITCODE -ne 0){throw 'ビルドに失敗しました。'}
    $destination=Join-Path $PSScriptRoot 'PerformanceGlass.exe'
    Copy-Item -LiteralPath (Join-Path $TargetDirectory "$Target\release\performance-glass.exe") -Destination $destination -Force
    Write-Output $destination
} finally {
    $env:RUSTFLAGS = $savedRustFlags
    Pop-Location
}
