# Nevermind en Rust : vérifie le zip à publier (Nevermind.zip), sans rien lancer. Utilisé par la construction GitHub
# (.github\workflows\release.yml, job « Nevermind en Rust »), par outils\zip-rust.ps1 et par outils\publier.ps1 -Rust,
# juste avant la signature.
#
#   & '.\outils\verifier-zip-rust.ps1' -Zip <Nevermind.zip> [-Version 2.0.0] [-Ci]
#
# Le zip doit être celui que Mindset attend pour une appli en Rust (docs\contrat-appli.md et docs\regles-tauri.md du
# dépôt Mindset), avec exactement ce que décrit outils\contenu-rust.ps1 :
#   - exactement un dossier « Nevermind » et dedans exactement Mindset.exe, Nevermind.exe, NevermindAide.exe,
#     Désinstaller Nevermind.exe, version.json, LISEZMOI.txt et fichiers\outils-tiers\ (PresentMon, sa licence, son
#     SOURCES.md) : ni script PowerShell, ni DLL, ni fichier en plus, ni version.json à la racine du zip ;
#   - version.json en UTF-8 sans marque : appli « nevermind », version celle du Cargo.toml de nevermind-appli (ou
#     -Version, qui doit alors être la même), mindset_min celle de contenu-rust.ps1 ;
#   - les fichiers copiés identiques à ceux du dépôt (celui du dossier, ou celui de git : la copie de Jordan peut avoir
#     des fins de ligne CRLF que git n'a pas) ; PresentMon.exe avec l'empreinte de son SOURCES.md ;
#   - fiches de version : Mindset.exe décrit « Mindset » (une seule ligne « Mindset » dans le Gestionnaire des tâches),
#     les trois autres « Nevermind » ; tous à la version de version.json ;
#   - aucun exe ne connaît la construction d'essai (outils\verifier-sans-essai.ps1) ;
#   - aucun runtime C à installer à part (outils\verifier-imports.ps1, lu dans la table des imports).
# -Ci (tests de GitHub entre deux publications seulement) : NevermindAide.exe peut avoir la version de Nevermind.ps1
# quand outils\version-rust.ps1 ne l'a pas encore alignée (seulement signalé). Jamais pour une publication.
# Code de sortie 0 si tout va bien, 1 sinon (avec la liste des problèmes).
param(
    [Parameter(Mandatory = $true)][string]$Zip,
    [string]$Version = '',
    [switch]$Ci
)
$ErrorActionPreference = 'Stop'
$racine = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'contenu-rust.ps1')
$attendus = @(@($NevermindRust.Programmes.Keys) + @('version.json') + @($NevermindRust.Copies.Keys) | ForEach-Object { "Nevermind\$_" })
$problemes = New-Object Collections.Generic.List[string]

if (-not (Test-Path -LiteralPath $Zip -PathType Leaf)) { Write-Host "Zip introuvable : $Zip"; exit 1 }
$Zip = (Resolve-Path -LiteralPath $Zip).Path

$cargo = Get-VersionNevermindRust $racine
if (-not $Version) { $Version = $cargo }
elseif ($Version -cne $cargo) { $problemes.Add("La version demandée ($Version) n'est pas celle du Cargo.toml de nevermind-appli ($cargo).") }

Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$tmp = Join-Path ([IO.Path]::GetTempPath()) "nevermind-zip-rust-$PID-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
    # Les entrées du zip (avec « \ » ou « / », comme Mindset les lit)
    $archive = [IO.Compression.ZipFile]::OpenRead($Zip)
    try {
        $noms = @($archive.Entries | ForEach-Object { $_.FullName.Replace('/', '\') })
        $fichiers = @($noms | Where-Object { -not $_.EndsWith('\') } | Sort-Object)
        $enTrop = @($fichiers | Where-Object { $attendus -cnotcontains $_ })
        $absents = @($attendus | Where-Object { $fichiers -cnotcontains $_ })
        if ($enTrop.Count) { $problemes.Add("Fichiers en trop dans le zip : $($enTrop -join ', ')") }
        if ($absents.Count) { $problemes.Add("Fichiers absents du zip : $($absents -join ', ')") }
        if (@($noms | Where-Object { -not $_.StartsWith('Nevermind\', [StringComparison]::Ordinal) }).Count) { $problemes.Add('Le zip a autre chose que le dossier Nevermind.') }
        foreach ($e in $archive.Entries) {
            $n = $e.FullName.Replace('/', '\')
            if ($attendus -cnotcontains $n) { continue }
            $dest = Join-Path $tmp $n
            New-Item -ItemType Directory -Force -Path (Split-Path $dest -Parent) | Out-Null
            [IO.Compression.ZipFileExtensions]::ExtractToFile($e, $dest, $true)
        }
    } finally { $archive.Dispose() }
    $d = Join-Path $tmp 'Nevermind'

    # version.json
    $vj = Join-Path $d 'version.json'
    if (Test-Path -LiteralPath $vj) {
        $o = [IO.File]::ReadAllBytes($vj)
        if ($o.Length -ge 3 -and $o[0] -eq 0xEF -and $o[1] -eq 0xBB -and $o[2] -eq 0xBF) { $problemes.Add('version.json a une marque UTF-8 (BOM).') }
        $j = $null
        try { $j = [Text.Encoding]::UTF8.GetString($o).TrimStart([char]0xFEFF) | ConvertFrom-Json } catch { $problemes.Add("version.json illisible : $($_.Exception.Message)") }
        if ($j) {
            Write-Host "version.json : appli $($j.appli), version $($j.version), mindset_min $($j.mindset_min)"
            if ($j.appli -cne 'nevermind') { $problemes.Add("version.json : appli « $($j.appli) » au lieu de « nevermind »") }
            if ($j.version -cne $Version) { $problemes.Add("version.json : version $($j.version) au lieu de $Version") }
            if ($j.mindset_min -cne $NevermindRust.MindsetMin) { $problemes.Add("version.json : mindset_min $($j.mindset_min) au lieu de $($NevermindRust.MindsetMin)") }
        }
    }

    # Les fichiers copiés : les mêmes que ceux du dépôt, octet pour octet. Le zip de GitHub a les fins de ligne de git,
    # un zip fait sur ce PC celles du dossier (git pour Windows peut y écrire des CRLF) : l'un ou l'autre convient
    # (leçon de Mindcare 0.91.0, refusé à tort le 04/11).
    foreach ($rel in $NevermindRust.Copies.Keys) {
        $f = Join-Path $d $rel
        if (-not (Test-Path -LiteralPath $f)) { continue }
        $source = $NevermindRust.Copies[$rel]
        $commeDossier = (Get-FileHash -LiteralPath (Join-Path $racine $source)).Hash -eq (Get-FileHash -LiteralPath $f).Hash
        $avantGit = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
        try {
            $dansGit = ((& git -C $racine rev-parse "HEAD:$($source.Replace('\', '/'))" 2>$null) -join '').Trim()
            $duZip = ((& git -C $racine hash-object --no-filters -- $f 2>$null) -join '').Trim()
        } finally { $ErrorActionPreference = $avantGit }
        $commeGit = $dansGit -match '^[0-9a-f]{40}$' -and $dansGit -eq $duZip
        if (-not ($commeDossier -or $commeGit)) { $problemes.Add("$rel du zip différent de $source (ni celui du dossier, ni celui de git)") }
    }
    $pm = Join-Path $d 'fichiers\outils-tiers\PresentMon.exe'
    if ((Test-Path -LiteralPath $pm) -and (Get-FileHash -LiteralPath $pm -Algorithm SHA256).Hash -ne $NevermindRust.PresentMonSha256) { $problemes.Add('PresentMon.exe : empreinte différente de celle de son SOURCES.md') }

    # Les fiches de version des programmes, et la construction d'essai
    $v4 = ConvertTo-Version4 $Version
    $versionPs = Get-VersionNevermindPs $racine
    foreach ($nom in $NevermindRust.Programmes.Keys) {
        $f = Join-Path $d $nom
        if (-not (Test-Path -LiteralPath $f)) { continue }
        $attendu = $NevermindRust.Programmes[$nom]
        $i = (Get-Item -LiteralPath $f).VersionInfo
        Write-Host "$nom : description « $($i.FileDescription) », produit « $($i.ProductName) », version $($i.ProductVersion), SHA-256 $((Get-FileHash -LiteralPath $f -Algorithm SHA256).Hash)"
        if ($i.FileDescription -cne $attendu.Description) { $problemes.Add("$nom : description « $($i.FileDescription) » au lieu de « $($attendu.Description) »") }
        if ($i.ProductName -cne $attendu.Produit) { $problemes.Add("$nom : produit « $($i.ProductName) » au lieu de « $($attendu.Produit) »") }
        $lue = ConvertTo-Version4 ([string]$i.ProductVersion)
        if ($lue -ne $v4) {
            if ($Ci -and $attendu.Bin -eq 'NevermindAide' -and $lue -eq (ConvertTo-Version4 $versionPs)) {
                Write-Host "  (tests de GitHub : $nom a la version de Nevermind.ps1, $versionPs ; outils\publier.ps1 -Rust l'aligne sur $Version)" -ForegroundColor Yellow
            } else { $problemes.Add("$nom : version $($i.ProductVersion) au lieu de $Version") }
        }
        & (Join-Path $PSScriptRoot 'verifier-sans-essai.ps1') -Exe $f | Out-Host
        if ($LASTEXITCODE) { $problemes.Add("$nom contient la construction d'essai") }
    }

    # Aucun runtime C à installer à part (lu dans la table des imports, sans rien lancer)
    $exes = @($NevermindRust.Programmes.Keys | ForEach-Object { Join-Path $d $_ } | Where-Object { Test-Path -LiteralPath $_ })
    if ($exes.Count) {
        & (Join-Path $PSScriptRoot 'verifier-imports.ps1') -Fichiers $exes -Liste | Out-Host
        if ($LASTEXITCODE) { $problemes.Add('un programme du zip a besoin d''un runtime C à installer à part') }
    }
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

if ($problemes.Count) {
    $problemes | ForEach-Object { Write-Host "  $_" -ForegroundColor Red }
    Write-Host "Zip refusé : $Zip ($($problemes.Count) problème(s))." -ForegroundColor Red
    exit 1
}
Write-Host "Zip conforme : $Zip (Nevermind $Version en Rust, SHA-256 $((Get-FileHash -LiteralPath $Zip -Algorithm SHA256).Hash))." -ForegroundColor Green
exit 0
