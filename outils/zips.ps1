# Les deux zips d'une version de Nevermind, faits depuis son dossier prêt à partager (« Nevermind », construit par
# construire.ps1, ou signé par SignPath dans .github\workflows\release.yml) :
#   Nevermind.zip  le dossier « Nevermind », script fichiers\Nevermind.ps1 ;
#   OptiGame.zip   le zip de compatibilité, publié aussi pendant le changement de nom : le même contenu sous les anciens
#                  noms (dossier « OptiGame », script fichiers\OptiGame.ps1), ceux que cherchent les Mindset 2.12.0 et plus
#                  anciens (et Nevermind 1.11.0 pour ses propres mises à jour). Seul le nom du script change : il reste
#                  signé (la signature ne dépend pas du nom du fichier), et le lanceur et le script lui-même le trouvent
#                  sous ses deux noms. Un seul script principal par zip : jamais deux à côté.
#
#   .\outils\zips.ps1 -Dossier .\Nevermind -Sortie .
param(
    [Parameter(Mandatory = $true)][string]$Dossier,
    [Parameter(Mandatory = $true)][string]$Sortie
)
$ErrorActionPreference = 'Stop'
$Dossier = [IO.Path]::GetFullPath($Dossier).TrimEnd('\')
if ((Split-Path $Dossier -Leaf) -ne 'Nevermind') { throw "Le dossier doit s'appeler Nevermind (c'est le nom du dossier dans le zip) : $Dossier" }
if (-not (Test-Path -LiteralPath (Join-Path $Dossier 'fichiers\Nevermind.ps1'))) { throw "fichiers\Nevermind.ps1 absent de $Dossier" }
if (Test-Path -LiteralPath (Join-Path $Dossier 'fichiers\OptiGame.ps1')) { throw "fichiers\OptiGame.ps1 ne doit pas être dans $Dossier (deux scripts principaux)" }
New-Item -ItemType Directory -Force -Path $Sortie | Out-Null
$Sortie = [IO.Path]::GetFullPath($Sortie)

$zip = Join-Path $Sortie 'Nevermind.zip'
Compress-Archive -Path $Dossier -DestinationPath $zip -Force

$tmp = Join-Path ([IO.Path]::GetTempPath()) "nevermind-zips-$PID-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
try {
    $ancien = Join-Path $tmp 'OptiGame'
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    Copy-Item -LiteralPath $Dossier -Destination $ancien -Recurse
    Rename-Item -LiteralPath (Join-Path $ancien 'fichiers\Nevermind.ps1') -NewName 'OptiGame.ps1'
    $compat = Join-Path $Sortie 'OptiGame.zip'
    Compress-Archive -Path $ancien -DestinationPath $compat -Force
} finally { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }
$zip
$compat
