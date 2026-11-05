# Writes THIRD-PARTY-NOTICES.txt: every Rust crate compiled into LSP Converter
# (the app in native/ and the converter in converter/, and any crate a
# build feature adds), with the licence and notice files
# each crate publishes. MIT, BSD and Apache require those to travel with the
# binaries (LEGAL-REVIEW.md, "GPL obligations").
#
# Reads only what cargo already has: `cargo tree --offline` and the crate
# sources in ~/.cargo/registry/src. Installs nothing. Proc-macro crates run
# only at build time and aren't in the binaries, so they're left out.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\third-party-notices.ps1
#
# Run it again whenever Cargo.lock changes, and before every release.

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$out = Join-Path $repo 'THIRD-PARTY-NOTICES.txt'
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$registries = Get-ChildItem (Join-Path $cargoHome 'registry\src') -Directory | ForEach-Object FullName

# what ships
$builds = @(
    @{ dir = 'native';    features = '' },
    @{ dir = 'converter'; features = '' }
)

$crates = @{}   # "name version" -> licence expression
foreach ($b in $builds) {
    $cargoArgs = @('tree', '--offline', '-e', 'normal', '--prefix', 'none', '-f', '{p}|{l}', '--target', 'x86_64-pc-windows-msvc')
    if ($b.features) { $cargoArgs += @('--features', $b.features) }
    Push-Location (Join-Path $repo $b.dir)
    try { $lines = & cargo @cargoArgs 2>$null } finally { Pop-Location }
    if ($LASTEXITCODE -ne 0) { throw "cargo tree failed in $($b.dir) ($($b.features))" }
    foreach ($line in $lines) {
        $pkg, $lic = $line -split '\|', 2
        if ($pkg -match '\(proc-macro\)') { continue }
        if ($pkg -match '\([A-Za-z]:\\') { continue }          # our own crates (local paths)
        $pkg = ($pkg -replace '\s*\(\*\)', '').Trim()
        if ($pkg -notmatch '^(\S+) v(\S+)$') { continue }
        $crates["$($Matches[1]) $($Matches[2])"] = ($lic -replace '\s*\(\*\)', '').Trim()
    }
}

# each crate's licence and notice files, grouped by identical text
$texts = [ordered]@{}   # text -> list of "name version"
$missing = New-Object System.Collections.Generic.List[string]
$needed = @{}           # standard licences the crates without a file need
foreach ($key in ($crates.Keys | Sort-Object)) {
    $name, $ver = $key -split ' '
    $src = $registries | ForEach-Object { Join-Path $_ "$name-$ver" } | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $src) { throw "no source for $name $ver in $($registries -join ', '): run a build first" }
    $files = @(Get-ChildItem $src -File | Where-Object { $_.Name -match '^(LICEN[CS]E|COPYING|NOTICE|COPYRIGHT|UNLICENSE|AUTHORS)' } | Sort-Object Name)
    # C code vendored inside a crate (libogg, libvorbis, zstd, xz, ...) carries its own
    # licence files further down. xz's GPL/LGPL files cover its command-line tools,
    # not liblzma (0BSD), so they're left out.
    $nested = @(Get-ChildItem $src -Recurse -Depth 3 -File | Where-Object {
        $_.DirectoryName -ne $src -and $_.Name -match '^(LICEN[CS]E|COPYING)' -and $_.Extension -ne '.sh' `
            -and $_.Name -notmatch '^COPYING\.(L?GPL)' -and $_.FullName -notmatch '\\(tests?|examples?|benches)\\'
    } | Sort-Object FullName)
    if (-not $files.Count) {
        # no file: name the licence we use it under, its authors and its source
        $toml = [IO.File]::ReadAllText((Join-Path $src 'Cargo.toml'))
        $authors = if ($toml -match '(?s)\nauthors\s*=\s*\[(.*?)\]') { (($Matches[1] -split ',') | ForEach-Object { $_.Trim().Trim('"') } | Where-Object { $_ }) -join ', ' } else { 'its authors' }
        $repoUrl = if ($toml -match '\nrepository\s*=\s*"([^"]+)"') { $Matches[1] } else { "https://crates.io/crates/$name" }
        $lic = $crates[$key]
        $use = if ($lic -match '\bMIT\b') { 'MIT' } elseif ($lic -match 'BSD-3-Clause') { 'BSD-3-Clause' } elseif ($lic -match 'MPL-2\.0') { 'MPL-2.0' } else { $lic }
        $missing.Add("$name $ver, used under $use. Copyright (c) $authors. Source: $repoUrl")
        $needed[$use] = $true
    }
    foreach ($f in ($files + $nested)) {
        $t = ([IO.File]::ReadAllText($f.FullName) -replace "`r`n", "`n").Trim()
        if (-not $t) { continue }
        if (-not $texts.Contains($t)) { $texts[$t] = New-Object System.Collections.Generic.List[string] }
        $label = "$name $ver ($($f.FullName.Substring($src.Length + 1).Replace('\', '/')))"
        if (-not $texts[$t].Contains($label)) { $texts[$t].Add($label) }
    }
}

$rule = '=' * 78
$sb = New-Object System.Text.StringBuilder
[void]$sb.AppendLine('LSP Converter: third-party notices')
[void]$sb.AppendLine('')
[void]$sb.AppendLine('LSP Converter is free software under the GNU General Public License,')
[void]$sb.AppendLine('version 3 or later (LICENSE.txt). It is built on ACECM by WhoaThatCombo and')
[void]$sb.AppendLine('the ACECM contributors (CREDITS.md), and on the open-source Rust crates below,')
[void]$sb.AppendLine('each under its own licence. Their licence and notice files follow, as each')
[void]$sb.AppendLine('crate publishes them; crates whose files are identical share one copy.')
[void]$sb.AppendLine('')
[void]$sb.AppendLine("Generated by tools/third-party-notices.ps1 on $(Get-Date -Format 'd MMMM yyyy').")
[void]$sb.AppendLine('')
[void]$sb.AppendLine($rule)
[void]$sb.AppendLine("CRATES ($($crates.Count))")
[void]$sb.AppendLine($rule)
foreach ($key in ($crates.Keys | Sort-Object)) { [void]$sb.AppendLine("$key  -  $($crates[$key])") }
if ($missing.Count) {
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine($rule)
    [void]$sb.AppendLine('CRATES THAT PUBLISH NO LICENCE FILE')
    [void]$sb.AppendLine($rule)
    [void]$sb.AppendLine('These are used under the licence named in their package metadata. The standard')
    [void]$sb.AppendLine('text of each licence follows the list; "the copyright holders" there are the')
    [void]$sb.AppendLine('authors named here.')
    [void]$sb.AppendLine('')
    foreach ($m in $missing) { [void]$sb.AppendLine("- $m") }
    if ($needed['MIT']) {
        [void]$sb.AppendLine('')
        [void]$sb.AppendLine('--- MIT License (standard text) ---')
        [void]$sb.AppendLine('')
        [void]$sb.AppendLine(@'
Copyright (c) the copyright holders named above

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
'@)
    }
    if ($needed['BSD-3-Clause']) {
        [void]$sb.AppendLine('')
        [void]$sb.AppendLine('--- BSD 3-Clause License (standard text) ---')
        [void]$sb.AppendLine('')
        [void]$sb.AppendLine(@'
Copyright (c) the copyright holders named above

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
'@)
    }
    if ($needed['MPL-2.0']) {
        # the full MPL text is long; another crate in this file already carries it
        $mpl = $texts.Keys | Where-Object { $_ -match 'Mozilla Public License Version 2\.0' } | Select-Object -First 1
        if (-not $mpl) { throw 'an MPL-2.0 crate has no licence file and no other crate carries the MPL-2.0 text' }
        [void]$sb.AppendLine('')
        [void]$sb.AppendLine("--- Mozilla Public License 2.0: the full text is below, with $($texts[$mpl][0]). ---")
        [void]$sb.AppendLine('MPL-2.0 code is used unmodified; its source is at the address given above.')
    }
    foreach ($u in $needed.Keys) {
        if ($u -notin @('MIT', 'BSD-3-Clause', 'MPL-2.0')) { throw "no standard text for ${u}: add it to this script" }
    }
}
foreach ($t in $texts.Keys) {
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine($rule)
    foreach ($who in $texts[$t]) { [void]$sb.AppendLine($who) }
    [void]$sb.AppendLine($rule)
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine($t)
}
# the UI's fonts (web/fonts), each with its SIL Open Font License as published
$fonts = Get-ChildItem (Join-Path $PSScriptRoot '..\web\fonts') -Filter 'OFL-*.txt' -ErrorAction SilentlyContinue | Sort-Object Name
foreach ($f in $fonts) {
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine($rule)
    [void]$sb.AppendLine("FONT: $($f.BaseName -replace '^OFL-', '') (web/fonts)")
    [void]$sb.AppendLine($rule)
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine(([IO.File]::ReadAllText($f.FullName)).TrimEnd())
}
[IO.File]::WriteAllText($out, ($sb.ToString() -replace "`r?`n", "`r`n"), (New-Object System.Text.UTF8Encoding($false)))
Write-Host "$out : $($crates.Count) crates, $($texts.Count) distinct licence texts, $($missing.Count) without a licence file"
