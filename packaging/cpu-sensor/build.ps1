param(
    [Parameter(Mandatory)][string]$OutputDirectory,
    [Parameter(Mandatory)][string]$CacheDirectory
)
$ErrorActionPreference = 'Stop'
$version = '0.9.6'
$hash = '086d9f1b5a99e643edc2cfaaac16051685b551e4c5ac0b32a57c58c0e529c001'
$project = 'https://github.com/LibreHardwareMonitor/LibreHardwareMonitor'
$source = "https://raw.githubusercontent.com/LibreHardwareMonitor/LibreHardwareMonitor/v$version"
$cache = (New-Item -ItemType Directory -Path $CacheDirectory -Force).FullName
$output = (New-Item -ItemType Directory -Path $OutputDirectory -Force).FullName
$archive = Join-Path $cache 'LibreHardwareMonitor.zip'
$pawnVersion = '2.2.0'
$pawnRelease = "https://github.com/namazso/PawnIO.Setup/releases/tag/$pawnVersion"
$pawnHash = '1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032'
$installer = Join-Path $cache 'PawnIO_setup.exe'
if (-not (Test-Path -LiteralPath $installer)) {
    Invoke-WebRequest -Uri "https://github.com/namazso/PawnIO.Setup/releases/download/$pawnVersion/PawnIO_setup.exe" -OutFile $installer
}
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $installer).Hash -ne $pawnHash) {
    throw 'PawnIO installer SHA-256 mismatch.'
}
$signature = Get-AuthenticodeSignature -LiteralPath $installer
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.GetNameInfo('SimpleName', $false) -ne 'namazso.eu') {
    throw 'PawnIO installer publisher signature could not be verified.'
}
Copy-Item -LiteralPath $installer -Destination $output -Force
if (-not (Test-Path -LiteralPath $archive)) {
    Invoke-WebRequest -Uri "$project/releases/download/v$version/LibreHardwareMonitor.zip" -OutFile $archive
}
if ((Get-FileHash -Algorithm SHA256 -LiteralPath $archive).Hash -ne $hash) {
    throw "LibreHardwareMonitor v$version SHA-256 mismatch."
}
$vendor = Join-Path $cache 'vendor'
Expand-Archive -LiteralPath $archive -DestinationPath $vendor -Force
$libraries = @(
    'LibreHardwareMonitorLib', 'BlackSharp.Core', 'DiskInfoToolkit', 'HidSharp', 'RAMSPDToolkit-NDD',
    'System.Buffers', 'System.Memory', 'System.Numerics.Vectors', 'System.Runtime.CompilerServices.Unsafe'
)
foreach ($name in $libraries) {
    Copy-Item -LiteralPath (Join-Path $vendor "$name.dll") -Destination $output -Force
}

$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$helper = Join-Path $output 'PerformanceGlass.CpuSensor.exe'
$library = Join-Path $output 'LibreHardwareMonitorLib.dll'
& $compiler /nologo /target:winexe /platform:x64 /optimize+ "/out:$helper" "/reference:$library" (Join-Path $PSScriptRoot 'Program.cs')
if ($LASTEXITCODE -ne 0) { throw 'CPU temperature helper build failed.' }

Invoke-WebRequest -Uri "$source/LICENSE" -OutFile (Join-Path $output 'LibreHardwareMonitor-LICENSE.txt')
Invoke-WebRequest -Uri "$source/THIRD-PARTY-NOTICES.txt" -OutFile (Join-Path $output 'LibreHardwareMonitor-NOTICES.txt')
Invoke-WebRequest -Uri 'https://raw.githubusercontent.com/dotnet/runtime/v10.0.3/LICENSE.TXT' -OutFile (Join-Path $output 'dotnet-LICENSE.txt')
Invoke-WebRequest -Uri 'https://www.apache.org/licenses/LICENSE-2.0.txt' -OutFile (Join-Path $output 'HidSharp-LICENSE.txt')
@(
    "LibreHardwareMonitor $version (MPL-2.0). Corresponding source: $project/tree/v$version"
    "PawnIO Setup $pawnVersion (Copyright 2026 namazso). Official installer redistributed unmodified under its redistribution notice: $pawnRelease"
    "PawnIO driver open-source project: https://github.com/namazso/PawnIO/tree/$pawnVersion"
    'PawnIO.Modules 0.2.2 (LGPL-2.1): https://github.com/namazso/PawnIO.Modules/tree/0.2.2'
    'DiskInfoToolkit 1.1.2 (MPL-2.0): https://github.com/Blacktempel/DiskInfoToolkit'
    'RAMSPDToolkit-NDD 1.4.2 (MPL-2.0): https://github.com/Blacktempel/RAMSPDToolkit'
    'BlackSharp.Core 1.0.7 (MPL-2.0): https://github.com/Blacktempel/BlackSharp'
    'HidSharp 2.6.4 (Apache-2.0), Copyright 2010-2025 James F. Bellinger: https://software.seekye.com/hidsharp'
    'System.Buffers, System.Memory, System.Numerics.Vectors and System.Runtime.CompilerServices.Unsafe (MIT): https://github.com/dotnet/runtime'
) | Set-Content -LiteralPath (Join-Path $output 'SOURCE.txt')
