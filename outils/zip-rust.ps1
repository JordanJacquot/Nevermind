# Nevermind en Rust : fait Nevermind.zip à partir du dossier assemblé par outils\construire-rust.ps1, au même format que
# les autres zips de la suite (Compress-Archive du dossier « Nevermind » : le « dossier_zip » de l'entrée apps.json, avec
# Nevermind.exe dedans), puis le vérifie (outils\verifier-zip-rust.ps1). Utilisé par la construction GitHub
# (.github\workflows\release.yml) ; sur le PC, seulement pour un essai : le zip publié vient toujours de GitHub.
#
#   & '.\outils\zip-rust.ps1' -Dossier <sortie>\Nevermind -Zip <dossier>\Nevermind.zip [-Ci]
#
# Le zip déjà là est remplacé. Code de sortie 1 (et le zip effacé) si la vérification échoue. -Ci : voir
# outils\verifier-zip-rust.ps1 (tests de GitHub entre deux publications seulement).
param(
    [Parameter(Mandatory = $true)][string]$Dossier,
    [Parameter(Mandatory = $true)][string]$Zip,
    [switch]$Ci
)
$ErrorActionPreference = 'Stop'
$Dossier = [IO.Path]::GetFullPath($Dossier).TrimEnd('\')
$Zip = [IO.Path]::GetFullPath($Zip)
if ((Split-Path $Dossier -Leaf) -cne 'Nevermind') { throw "Le dossier à zipper doit s'appeler Nevermind : $Dossier" }
if (-not (Test-Path -LiteralPath (Join-Path $Dossier 'Nevermind.exe') -PathType Leaf)) { throw "Nevermind.exe introuvable dans $Dossier" }
if (Test-Path -LiteralPath (Join-Path $Dossier 'fichiers\Nevermind.ps1')) { throw "$Dossier est le dossier de la version PowerShell (fichiers\Nevermind.ps1) : son zip se fait avec outils\zips.ps1." }
New-Item -ItemType Directory -Force -Path (Split-Path $Zip -Parent) | Out-Null
Remove-Item -LiteralPath $Zip -Force -ErrorAction SilentlyContinue
Compress-Archive -LiteralPath $Dossier -DestinationPath $Zip -Force
& (Join-Path $PSScriptRoot 'verifier-zip-rust.ps1') -Zip $Zip -Ci:$Ci
if ($LASTEXITCODE) { Remove-Item -LiteralPath $Zip -Force -ErrorAction SilentlyContinue; exit 1 }
exit 0
