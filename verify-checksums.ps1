[CmdletBinding()]
param(
    [string]$Directory = '.',
    [string]$ChecksumFile = 'SHA256SUMS.txt'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path -LiteralPath $Directory).Path
if (-not (Test-Path -LiteralPath $root -PathType Container)) {
    throw "Map niet gevonden: $Directory"
}

$checksumPath = if ([System.IO.Path]::IsPathRooted($ChecksumFile)) {
    $ChecksumFile
} else {
    Join-Path $root $ChecksumFile
}

if (-not (Test-Path -LiteralPath $checksumPath -PathType Leaf)) {
    throw "Checksum-bestand niet gevonden: $checksumPath"
}

$entries = @()
$lineNumber = 0
foreach ($line in Get-Content -LiteralPath $checksumPath) {
    $lineNumber++
    $text = $line.Trim()
    if (-not $text -or $text.StartsWith('#')) {
        continue
    }

    $match = [regex]::Match($text, '^([0-9A-Fa-f]{64})\s+\*?(.+)$')
    if (-not $match.Success) {
        throw "Ongeldige regel $lineNumber in $checksumPath`: $text"
    }

    $entries += [pscustomobject]@{
        Hash = $match.Groups[1].Value.ToLowerInvariant()
        Name = $match.Groups[2].Value
    }
}

if ($entries.Count -eq 0) {
    throw "Geen checksums gevonden in $checksumPath"
}

$failed = $false
foreach ($entry in $entries) {
    $file = Join-Path $root $entry.Name
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
        Write-Host "MISLUKT  $($entry.Name): bestand ontbreekt" -ForegroundColor Red
        $failed = $true
        continue
    }

    $actual = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $entry.Hash) {
        Write-Host "MISLUKT  $($entry.Name): SHA-256 komt niet overeen" -ForegroundColor Red
        $failed = $true
    } else {
        Write-Host "OK       $($entry.Name)"
    }
}

if ($failed) {
    exit 1
}

Write-Host "Alle $($entries.Count) bestanden zijn geverifieerd."
