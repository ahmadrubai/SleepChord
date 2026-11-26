[CmdletBinding()]
param(
    [string] $CompilerPath,
    [switch] $ValidateOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-ReleaseFile([string] $Path) {
    $file = Get-Item -LiteralPath $Path -Force
    if ($file.PSIsContainer -or !$file.Length -or ($file.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Expected a nonempty regular file at $Path."
    }
    $file
}

function Assert-ReleaseImage([IO.FileInfo] $File, [switch] $Gui) {
    $reader = [IO.BinaryReader]::new($File.OpenRead())
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw "Invalid DOS header in $($File.Name)." }
        $reader.BaseStream.Position = 0x3C
        $pe = $reader.ReadUInt32()
        $reader.BaseStream.Position = $pe
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) {
            throw "Expected an x64 Windows PE image in $($File.Name)."
        }
        if ($Gui) {
            $reader.BaseStream.Position = $pe + 24
            if ($reader.ReadUInt16() -ne 0x20B) { throw "Expected a PE32+ image in $($File.Name)." }
            $reader.BaseStream.Position = $pe + 24 + 68
            if ($reader.ReadUInt16() -ne 2) { throw "Expected a Windows GUI executable in $($File.Name)." }
        }
    } finally {
        $reader.Dispose()
    }
}

function Find-InnoCompiler([string] $Path) {
    if ($Path) { return (Get-ReleaseFile $Path).FullName }
    $command = Get-Command ISCC.exe -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($command) { return $command.Source }
    $programFilesX86 = [Environment]::GetEnvironmentVariable('ProgramFiles(x86)')
    foreach ($base in @($env:ProgramFiles, $programFilesX86, (Join-Path $env:LOCALAPPDATA 'Programs'))) {
        if (!$base) { continue }
        foreach ($product in @('Inno Setup 7', 'Inno Setup 6')) {
            $candidate = Join-Path $base "$product\ISCC.exe"
            if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
        }
    }
    throw 'Inno Setup was not found. Install it from https://jrsoftware.org/isdl.php or pass -CompilerPath.'
}

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$config = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
$package = [regex]::Match($config, '(?ms)^\[package\][ \t]*\r?\n(?<body>.*?)(?=^\[|\z)')
$version = [regex]::Match($package.Groups['body'].Value, '(?m)^version[ \t]*=[ \t]*"(\d+\.\d+\.\d+)"').Groups[1].Value
if (!$version) { throw 'No numeric application version found in the Cargo.toml package table.' }

$release = Join-Path $root 'dist\windows-x64'
$manifestName = 'SHA256SUMS.txt'
$payload = @('SleepChord.exe', 'vcruntime140.dll', 'vcruntime140_1.dll', 'README.txt', 'LICENSE', 'THIRD-PARTY-NOTICES.txt')
$releaseFolder = Get-Item -LiteralPath $release -Force
if (!$releaseFolder.PSIsContainer -or ($releaseFolder.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
    throw "Expected a regular release directory at $release."
}
foreach ($entry in Get-ChildItem -LiteralPath $release -Force) {
    if ($entry.Name -notin ($payload + $manifestName)) { throw "Unexpected entry in the release: $($entry.Name)." }
}

$manifest = Get-ReleaseFile (Join-Path $release $manifestName)
$hashes = @{}
foreach ($line in Get-Content -LiteralPath $manifest.FullName) {
    if ($line -notmatch '^([0-9a-fA-F]{64})  ([^\\/]+)$') { throw 'Invalid release checksum entry.' }
    $hash, $name = $Matches[1], $Matches[2]
    if ($name -notin $payload -or $hashes.ContainsKey($name)) { throw "Unexpected checksum entry for $name." }
    $hashes[$name] = $hash
}
foreach ($name in $payload) {
    $file = Get-ReleaseFile (Join-Path $release $name)
    $actual = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
    if ($actual -ne $hashes[$name]) { throw "Release checksum missing or mismatched for $name." }
    if ($name -eq 'SleepChord.exe') {
        Assert-ReleaseImage $file -Gui
        $info = $file.VersionInfo
        if ($info.ProductName -ne 'SleepChord' -or $info.ProductVersion -ne $version -or
            $info.FileVersion -ne $version -or $info.OriginalFilename -ne $name) {
            throw "SleepChord.exe metadata does not match Cargo.toml version $version."
        }
    } elseif ($file.Extension -eq '.dll') {
        Assert-ReleaseImage $file
        $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
        if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'Microsoft Corporation') {
            throw "Expected a Microsoft-signed VC runtime file for $name."
        }
    }
}
Write-Host "Verified SleepChord $version Windows x64 release and all payload checksums."
if ($ValidateOnly) { return }

$compiler = Find-InnoCompiler $CompilerPath
$compilerVersion = (Get-Item -LiteralPath $compiler).VersionInfo
if ($compilerVersion.FileMajorPart -gt 0 -and
    $compilerVersion.FileMajorPart -lt 7) {
    throw 'Inno Setup 7 or later is required to build the native x64 installer.'
}

$script = Join-Path $root 'installer\SleepChord.iss'
$output = Join-Path $root 'dist\installer'
$installerName = "SleepChord-Setup-$version-x64.exe"
if (Test-Path -LiteralPath $output) {
    $directory = Get-Item -LiteralPath $output -Force
    if (!$directory.PSIsContainer -or ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Expected a regular output directory at $output."
    }
} else {
    New-Item -ItemType Directory -Path $output | Out-Null
}
$stage = Join-Path $output ('.build-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
$stagedSetup = Join-Path $stage $installerName
$stagedManifest = Join-Path $stage $manifestName
try {
    & $compiler "/DAppVersion=$version" "/O$stage" $script
    if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed with exit code $LASTEXITCODE." }
    $installer = Get-ReleaseFile $stagedSetup
    Assert-ReleaseImage $installer -Gui
    $info = $installer.VersionInfo
    if ($info.ProductName.Trim() -ne 'SleepChord' -or
        "$($info.FileMajorPart).$($info.FileMinorPart).$($info.FileBuildPart)" -ne $version) {
        throw 'Compiled installer metadata does not match the release.'
    }
    $hash = (Get-FileHash -LiteralPath $stagedSetup -Algorithm SHA256).Hash.ToLowerInvariant()
    Set-Content -LiteralPath $stagedManifest -Value "$hash  $installerName" -Encoding ASCII
    foreach ($name in @($installerName, $manifestName)) {
        $destination = Join-Path $output $name
        if (Test-Path -LiteralPath $destination) { $null = Get-ReleaseFile $destination }
    }
    $finalExe = Join-Path $output $installerName
    $size = $installer.Length
    Move-Item -LiteralPath $stagedSetup -Destination $finalExe -Force
    Move-Item -LiteralPath $stagedManifest -Destination (Join-Path $output $manifestName) -Force
    Write-Host "Installer: $finalExe ($size bytes)"
    Write-Host "SHA-256: $hash"
} finally {
    foreach ($path in @($stagedSetup, $stagedManifest)) {
        if (Test-Path -LiteralPath $path -PathType Leaf) { Remove-Item -LiteralPath $path }
    }
    if (!(Get-ChildItem -LiteralPath $stage -Force)) { Remove-Item -LiteralPath $stage }
}
