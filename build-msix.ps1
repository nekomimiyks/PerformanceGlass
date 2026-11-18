param(
    [string]$Executable = (Join-Path $PSScriptRoot 'PerformanceGlass.exe'),
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'dist')
)
$ErrorActionPreference = 'Stop'
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$version = (Get-Item -LiteralPath $Executable).VersionInfo.ProductVersion
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'EXEの製品バージョンを取得できません。' }
$makeAppx = $env:MAKEAPPX
if (-not $makeAppx) {
    $sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
    $sdk = Get-ChildItem -LiteralPath $sdkRoot -Directory | Sort-Object Name -Descending | Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'x64\makeappx.exe') } | Select-Object -First 1
    if ($sdk) { $makeAppx = Join-Path $sdk.FullName 'x64\makeappx.exe' }
}
if (-not $makeAppx) { throw 'Windows SDKのMakeAppx.exeが必要です。MAKEAPPXにも指定できます。' }
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$OutputDirectory = (Resolve-Path -LiteralPath $OutputDirectory).Path
$stage = Join-Path $OutputDirectory ('msix-stage-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
Copy-Item -LiteralPath $Executable -Destination (Join-Path $stage 'PerformanceGlass.exe')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'LICENSE'),(Join-Path $PSScriptRoot 'THIRD_PARTY_NOTICES.txt') -Destination $stage
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'store\Assets') -Destination (Join-Path $stage 'Assets') -Recurse
[xml]$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'store\AppxManifest.xml') -Raw -Encoding UTF8
$manifest.Package.Identity.Version = "$version.0"
$manifest.Save((Join-Path $stage 'AppxManifest.xml'))
$output = Join-Path $OutputDirectory "PerformanceGlass-$version-x64.msix"
& $makeAppx pack /d $stage /p $output /o
if ($LASTEXITCODE -ne 0) { throw 'MSIXパッケージの検証・作成に失敗しました。' }
Write-Output $output
