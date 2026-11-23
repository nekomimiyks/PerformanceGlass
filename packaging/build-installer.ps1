param(
    [Parameter(Mandatory)][string]$PackageDirectory,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\dist'),
    [string]$Compiler = 'ISCC.exe'
)
$ErrorActionPreference = 'Stop'
$package = (Resolve-Path -LiteralPath $PackageDirectory).Path
$output = (New-Item -ItemType Directory -Path $OutputDirectory -Force).FullName
$exe = Get-Item -LiteralPath (Join-Path $package 'PerformanceGlass.exe')
$version = $exe.VersionInfo.ProductVersion
if ($version -notmatch '^\d+\.\d+\.\d+$') {
    throw 'EXEの製品バージョンを取得できません。'
}
$pawnIO = Join-Path $package 'cpu-sensor\PawnIO_setup.exe'
if ((Get-FileHash -LiteralPath $pawnIO -Algorithm SHA256).Hash -ne '1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032') {
    throw 'PawnIO公式インストーラーのSHA-256が一致しません。'
}
& $Compiler "/DAppVersion=$version" "/DPackageDir=$package" "/O$output" (Join-Path $PSScriptRoot 'PerformanceGlass.iss')
if ($LASTEXITCODE -ne 0) {
    throw 'Windowsインストーラーの作成に失敗しました。'
}
