# Run from a regular PowerShell prompt on a Windows x64 build machine.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$target = 'x86_64-pc-windows-msvc'
$distRoot = Join-Path $root 'dist'
$dist = Join-Path $distRoot 'windows-x64'
$stage = Join-Path $distRoot ('.windows-x64-' + [guid]::NewGuid().ToString('N'))
$crtNames = @('vcruntime140.dll', 'vcruntime140_1.dll')
$packageFiles = @('SleepChord.exe', 'README.txt', 'LICENSE', 'THIRD-PARTY-NOTICES.txt', 'SHA256SUMS.txt') + $crtNames

function Invoke-Checked([string] $Program, [string[]] $Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Program failed with exit code $LASTEXITCODE."
    }
}

function Assert-RegularDirectory([string] $Path) {
    if (Test-Path -LiteralPath $Path) {
        $item = Get-Item -LiteralPath $Path -Force
        if (!$item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Expected a regular directory at $Path."
        }
    }
}

function Clear-PackageDirectory([string] $Path) {
    Assert-RegularDirectory $Path
    if (!(Test-Path -LiteralPath $Path)) { return }
    $items = @(Get-ChildItem -LiteralPath $Path -Force)
    foreach ($item in $items) {
        if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
            $item.Name -notin $packageFiles) {
            throw "Unexpected entry at $($item.FullName). Move it out of the package folder and rerun."
        }
    }
    foreach ($item in $items) { Remove-Item -LiteralPath $item.FullName }
    Remove-Item -LiteralPath $Path
}

function Write-ThirdPartyNotices($Metadata, [string] $OutputPath) {
    $writer = [IO.StreamWriter]::new($OutputPath, $false, [Text.UTF8Encoding]::new($false))
    try {
        $writer.WriteLine((Get-Content -LiteralPath 'docs/windows-third-party.txt' -Raw -Encoding UTF8))
        $supplements = @{}
        foreach ($row in Import-Csv -LiteralPath 'docs/licenses/windows-notices.tsv' -Delimiter ([char]9)) {
            $supplements["$($row.name)@$($row.version)"] = $row
        }
        foreach ($name in $supplements.Values.supplement | Sort-Object -Unique) {
            $writer.WriteLine("===== Supplemental license text: $name =====")
            $writer.WriteLine((Get-Content -LiteralPath (Join-Path 'docs/licenses' $name) -Raw -Encoding UTF8))
        }
        foreach ($font in Get-ChildItem -Path 'assets/fonts/LICENSE-*.txt' -File | Sort-Object Name) {
            $writer.WriteLine("===== $($font.Name) =====")
            $writer.WriteLine((Get-Content -LiteralPath $font.FullName -Raw -Encoding UTF8))
        }
        $solver = Get-Content -LiteralPath 'assets/ejs/yt.solver.lib.min.js' -Raw -Encoding UTF8
        $writer.WriteLine($solver.Substring(0, $solver.IndexOf('*/') + 2))
        $ids = @{}
        foreach ($node in $Metadata.resolve.nodes) { $ids[$node.id] = $true }
        $deps = $Metadata.packages | Where-Object { $_.source -and $ids.ContainsKey($_.id) }
        foreach ($dep in $deps | Sort-Object name,version) {
            $writer.WriteLine("===== $($dep.name) $($dep.version) | $($dep.license) =====")
            $writer.WriteLine("Source: https://crates.io/crates/$($dep.name)/$($dep.version)")
            if ($dep.repository) { $writer.WriteLine("Upstream: $($dep.repository)") }
            $crateDir = Split-Path -Parent $dep.manifest_path
            $notices = @(Get-ChildItem -LiteralPath $crateDir -File -Recurse | Where-Object {
                ($_.Name -match '^(licen[cs]e|copying|notice|copyright|ofl|ufl|unlicense)([. _-]|$)' -and
                    $_.Extension -in '', '.txt', '.md', '.rst', '.adoc', '.mit', '.apache', '.bsd') -or
                ($dep.name -eq 'epaint_default_fonts' -and $_.Directory.Name -eq 'fonts' -and $_.Extension -eq '.txt')
            } | Sort-Object FullName)
            if (!$notices) {
                $row = $supplements["$($dep.name)@$($dep.version)"]
                if ($row) {
                    $vcsPath = Join-Path $crateDir '.cargo_vcs_info.json'
                    $vcs = Get-Content -LiteralPath $vcsPath -Raw | ConvertFrom-Json
                    if ($row.declared_license -ne $dep.license -or $row.upstream_commit -ne $vcs.git.sha1) {
                        throw "Notice supplement does not match the locked source for $($dep.name)."
                    }
                    $writer.WriteLine("Standalone license text supplied above: $($row.supplement)")
                    $writer.WriteLine("Locked source revision: $($row.upstream_commit)")
                    $writer.WriteLine("Notice reference: $($row.upstream)")
                    $headers = Get-ChildItem -LiteralPath $crateDir -File -Recurse | Where-Object {
                        $_.Extension -in '.rs', '.xml', '.h', '.c'
                    } | Select-String -Pattern '^\s*(?://!?|\*|<!--)?\s*Copyright\s+(?:\([cC]\)\s*|\u00a9\s*)?\d{4}' -Encoding UTF8 |
                        ForEach-Object { $_.Line.Trim() } | Sort-Object -Unique
                    foreach ($header in $headers) { $writer.WriteLine($header) }
                } else {
                    $writer.WriteLine('No standalone notice found in this crate archive. See the upstream source.')
                }
            }
            foreach ($notice in $notices) {
                $writer.WriteLine("-- $($notice.FullName.Substring($crateDir.Length + 1)) --")
                $writer.WriteLine((Get-Content -LiteralPath $notice.FullName -Raw -Encoding UTF8))
            }
        }
    } finally {
        $writer.Dispose()
    }
}

Push-Location -LiteralPath $root
try {
    Get-Command cargo, rustc -ErrorAction Stop | Out-Null
    $rust = Invoke-Checked rustc @('-vV')
    if ($rust -notcontains "host: $target") { throw "Install and select the Rust $target toolchain." }

    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (!(Test-Path -LiteralPath $vswhere -PathType Leaf)) {
        throw 'Install Visual Studio or Build Tools with the Desktop development with C++ workload.'
    }
    $vs = Invoke-Checked $vswhere @('-latest', '-products', '*', '-requires',
        'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath')
    if (!$vs) { throw 'No Visual Studio installation with the x64 MSVC tools was found.' }
    $dumpbin = Get-ChildItem -Path "$vs/VC/Tools/MSVC/*/bin/Hostx64/x64/dumpbin.exe" -File |
        Sort-Object { [version]$_.Directory.Parent.Parent.Parent.Name } -Descending | Select-Object -First 1
    if (!$dumpbin) { throw 'The MSVC x64 dumpbin tool is required to verify native dependencies.' }
    $redist = Get-ChildItem -Path "$vs/VC/Redist/MSVC/*/x64/Microsoft.VC*.CRT" -Directory |
        Where-Object { $_.Parent.Parent.Name -match '^\d+\.\d+' } |
        Sort-Object { [version]$_.Parent.Parent.Name } -Descending | Select-Object -First 1

    Assert-RegularDirectory $distRoot
    Assert-RegularDirectory $dist
    $meta = (Invoke-Checked cargo @('metadata', '--locked', '--format-version', '1', '--filter-platform', $target)) |
        ConvertFrom-Json
    $app = $meta.packages | Where-Object { $_.id -eq $meta.resolve.root }
    $previousDistributionFlag = $env:SLEEPCHORD_WINDOWS_DIST
    try {
        $env:SLEEPCHORD_WINDOWS_DIST = '1'
        $artifacts = Invoke-Checked cargo @('build', '--release', '--locked', '--message-format=json-render-diagnostics') |
            ForEach-Object { ConvertFrom-Json -InputObject $_ } | Where-Object {
                $_.reason -eq 'compiler-artifact' -and $_.package_id -eq $app.id -and $_.executable
            }
    } finally {
        $env:SLEEPCHORD_WINDOWS_DIST = $previousDistributionFlag
    }
    $exe = $artifacts.executable
    $file = Get-Item -LiteralPath $exe
    if ($file.VersionInfo.ProductName -ne 'SleepChord' -or $file.VersionInfo.OriginalFilename -ne 'SleepChord.exe' -or
        $file.VersionInfo.ProductVersion -ne $app.version) {
        throw 'Unexpected executable product name, filename resource, or version.'
    }

    $headers = (Invoke-Checked $dumpbin.FullName @('/nologo', '/headers', $exe)) -join "`n"
    if ($headers -notmatch '(?m)^\s*8664 machine \(x64\)' -or $headers -notmatch '(?m)^\s*2 subsystem \(Windows GUI\)') {
        throw 'The release executable must be an x64 Windows GUI application.'
    }
    $imports = Invoke-Checked $dumpbin.FullName @('/nologo', '/dependents', $exe) |
        ForEach-Object { if ($_ -match '^\s+([\w.-]+\.dll)\s*$') { $Matches[1].ToLowerInvariant() } }
    if (!$imports) { throw 'No executable dependencies were found in the PE import table.' }

    New-Item -ItemType Directory -Path $stage -Force | Out-Null
    # The legacy Cargo binary name is still used by upstream packaging.
    Copy-Item -LiteralPath $exe -Destination (Join-Path $stage 'SleepChord.exe')
    Copy-Item -LiteralPath 'docs/windows-release.txt' -Destination (Join-Path $stage 'README.txt')
    Copy-Item -LiteralPath 'LICENSE' -Destination $stage
    foreach ($dll in $imports | Sort-Object -Unique) {
        if ($dll -in $crtNames) {
            if (!$redist) { throw 'Install the MSVC x64 redistributable files in Visual Studio Installer.' }
            $source = Join-Path $redist.FullName $dll
            $signature = Get-AuthenticodeSignature -LiteralPath $source
            if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'Microsoft Corporation') {
                throw "Expected a Microsoft-signed VC redistributable at $source."
            }
            Copy-Item -LiteralPath $source -Destination $stage
        } elseif ($dll -notmatch '^(api|ext)-ms-win-' -and
            !(Test-Path -LiteralPath (Join-Path $env:SystemRoot "System32/$dll") -PathType Leaf)) {
            throw "Unrecognized runtime dependency $dll. Audit its origin and distribution terms."
        }
    }
    Write-ThirdPartyNotices $meta (Join-Path $stage 'THIRD-PARTY-NOTICES.txt')
    $staged = @(Get-ChildItem -LiteralPath $stage -File | Sort-Object Name)
    $checksums = foreach ($item in $staged) {
        '{0}  {1}' -f (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $item.Name
    }
    Set-Content -LiteralPath (Join-Path $stage 'SHA256SUMS.txt') -Value $checksums -Encoding ASCII

    Clear-PackageDirectory $dist
    Move-Item -LiteralPath $stage -Destination $dist
    $size = (Get-ChildItem -LiteralPath $dist -File | Measure-Object Length -Sum).Sum
    Write-Host "Cargo output: $exe"
    Write-Host "Executable: $(Join-Path $dist 'SleepChord.exe') ($($file.Length) bytes)"
    Write-Host "Distribution: $dist ($size bytes)"
} finally {
    Clear-PackageDirectory $stage
    Pop-Location
}
