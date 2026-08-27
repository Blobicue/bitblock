$ErrorActionPreference = 'Stop'

$manifest = Import-Csv -LiteralPath (Join-Path $PSScriptRoot 'source-manifest.tsv') -Delimiter `t
if ($manifest.Count -ne 2) {
    throw "Expected exactly two pinned source rows, found $($manifest.Count)."
}

foreach ($entry in $manifest) {
    $path = Join-Path $PSScriptRoot $entry.path
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Pinned source is missing: $($entry.path)"
    }
    $actual = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $entry.sha256) {
        throw "SHA-256 mismatch for $($entry.path): expected $($entry.sha256), got $actual"
    }
}

Push-Location $PSScriptRoot
try {
    lake build
    lake env lean --trust=0 BitblockLadderTransition.lean
}
finally {
    Pop-Location
}
