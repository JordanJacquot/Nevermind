# Nevermind en Rust : écrit le numéro de version partout où un programme en Rust le prend, et nulle part ailleurs.
# Utilisé par outils\publier.ps1 -Rust.
#   - rust\nevermind-appli\Cargo.toml (ligne « version = » de [package]) et sa ligne de rust\Cargo.lock (sinon
#     « cargo build --locked » refuse de construire) : version.json, Mindset.exe, le relais et le désinstalleur ;
#   - $AppVersion et l'en-tête de Nevermind\fichiers\Nevermind.ps1 : NevermindAide.exe (rust\nevermind-aide\build.rs) ;
#   - la première ligne de outils\LISEZMOI-rust.txt (le LISEZMOI du zip en Rust).
# Fins de ligne et encodage de chaque fichier gardés.
#
#   & '.\outils\version-rust.ps1' -Version 2.0.0
param([Parameter(Mandatory = $true)][string]$Version)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Numéro de version invalide : $Version (exemple : 2.0.0)" }
$racine = Split-Path $PSScriptRoot -Parent
$cargoAppli = Join-Path $racine 'rust\nevermind-appli\Cargo.toml'
$cargoLock = Join-Path $racine 'rust\Cargo.lock'
$ps1 = Join-Path $racine 'Nevermind\fichiers\Nevermind.ps1'
$lisez = Join-Path $PSScriptRoot 'LISEZMOI-rust.txt'
$sansBom = New-Object Text.UTF8Encoding($false)
$avecBom = New-Object Text.UTF8Encoding($true)

# Cargo.toml : la ligne « version = "..." » de la section [package] (les autres « version = » sont celles des
# bibliothèques)
$texte = [IO.File]::ReadAllText($cargoAppli, [Text.Encoding]::UTF8)
$lignes = [regex]::Split($texte, '(?<=\n)')
$section = ''; $fait = $false
for ($i = 0; $i -lt $lignes.Count; $i++) {
    $l = $lignes[$i].Trim()
    if ($l.StartsWith('[')) { $section = $l; continue }
    if ($section -eq '[package]' -and $l -match '^version\s*=\s*"[^"]*"\s*$') { $lignes[$i] = [regex]::Replace($lignes[$i], '"[^"]*"', "`"$Version`"", 1); $fait = $true; break }
}
if (-not $fait) { throw "Ligne « version » de [package] introuvable dans $cargoAppli" }
[IO.File]::WriteAllText($cargoAppli, ($lignes -join ''), $sansBom)

# Cargo.lock : l'entrée de nevermind-appli seulement
$lock = [IO.File]::ReadAllText($cargoLock, [Text.Encoding]::UTF8)
$motif = '(?m)^(name = "nevermind-appli"\r?\nversion = ")[^"]*(")'
if (-not [regex]::IsMatch($lock, $motif)) { throw "nevermind-appli introuvable dans $cargoLock" }
[IO.File]::WriteAllText($cargoLock, [regex]::Replace($lock, $motif, "`${1}$Version`${2}"), $sansBom)

# Nevermind.ps1 : $AppVersion et l'en-tête « Nevermind x.y.z », comme outils\publier.ps1 (version PowerShell)
$c = [IO.File]::ReadAllText($ps1, [Text.Encoding]::UTF8)
if ($c -notmatch "(?m)^\`$AppVersion = '[^']*'") { throw "Ligne `$AppVersion introuvable dans $ps1" }
$c = [regex]::Replace($c, "(?m)^\`$AppVersion = '[^']*'", "`$AppVersion = '$Version'")
$c = [regex]::Replace($c, '(?m)^(    Nevermind) [\d\.]+(\r?)$', "`$1 $Version`$2")
[IO.File]::WriteAllText($ps1, $c, $avecBom)

# LISEZMOI du zip en Rust : sa première ligne
$l = [IO.File]::ReadAllText($lisez, [Text.Encoding]::UTF8)
if ($l -notmatch '^﻿?Nevermind [\d\.]+') { throw "Première ligne « Nevermind x.y.z » introuvable dans $lisez" }
[IO.File]::WriteAllText($lisez, [regex]::Replace($l, '^﻿?Nevermind [\d\.]+', "Nevermind $Version"), $avecBom)

"Nevermind en Rust : version $Version écrite dans rust\nevermind-appli\Cargo.toml, rust\Cargo.lock, Nevermind\fichiers\Nevermind.ps1 et outils\LISEZMOI-rust.txt."
