# Publie une nouvelle version d'OptiGame sur GitHub.
# Les apps déjà installées la proposent ensuite au lancement (bandeau « Nouvelle version disponible »).
#
#   .\outils\publier.ps1 -Version 1.1 -Notes "Ce qui change dans cette version"

param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Notes = ''
)
$ErrorActionPreference = 'Stop'
$racine = Split-Path $PSScriptRoot -Parent
$app    = Join-Path $racine 'OptiGame'
$ps1    = Join-Path $app 'fichiers\OptiGame.ps1'
$lisez  = Join-Path $app 'LISEZMOI.txt'
$utf8   = New-Object Text.UTF8Encoding($true)

if ($Version -notmatch '^\d+\.\d+(\.\d+)?$') { throw "Numéro de version invalide : $Version (exemple : 1.1)" }

# Un numéro déjà publié ne doit jamais être réutilisé : GitHub garde l'ancien zip en cache.
Push-Location $racine
try {
    $ErrorActionPreference = 'Continue'   # « introuvable » est la réponse attendue ici, pas une erreur
    git rev-parse -q --verify "refs/tags/v$Version" 2>$null | Out-Null
    $tagExiste = ($LASTEXITCODE -eq 0)
    gh release view "v$Version" 2>$null | Out-Null
    $releaseExiste = ($LASTEXITCODE -eq 0)
} finally { Pop-Location; $ErrorActionPreference = 'Stop' }
if ($tagExiste -or $releaseExiste) { throw "La version $Version existe déjà. Choisis un numéro plus grand (exemple : 1.0.2 ou 1.1)." }

function Invoke-Native([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE) { throw "Échec : $Exe $($Arguments -join ' ')" }
}

# 1. Numéro de version dans l'app et le LISEZMOI
$c = [IO.File]::ReadAllText($ps1, [Text.Encoding]::UTF8)
$c = [regex]::Replace($c, "(?m)^\`$AppVersion = '[^']*'", "`$AppVersion = '$Version'")
$c = [regex]::Replace($c, '(?m)^    OptiGame [\d\.]+\r?$', "    OptiGame $Version`r")
[IO.File]::WriteAllText($ps1, $c, $utf8)
$l = [IO.File]::ReadAllText($lisez, [Text.Encoding]::UTF8)
$l = [regex]::Replace($l, '^OptiGame [\d\.]+', "OptiGame $Version")
[IO.File]::WriteAllText($lisez, $l, $utf8)

# 2. Construction des .exe et du zip
& (Join-Path $PSScriptRoot 'construire.ps1')

# 3. Envoi sur GitHub et création de la version
Push-Location $racine
try {
    Invoke-Native git @('add', '-A')
    Invoke-Native git @('commit', '-m', "OptiGame $Version", '-m', 'Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>')
    Invoke-Native git @('tag', "v$Version")
    Invoke-Native git @('push', 'origin', 'HEAD', '--tags')
    if (-not $Notes) { $Notes = "OptiGame $Version" }
    Invoke-Native gh @('release', 'create', "v$Version", (Join-Path $racine 'OptiGame.zip'), '--title', "OptiGame $Version", '--notes', $Notes)
} finally { Pop-Location }

# 4. Mise à jour de la copie installée sur ce PC (celle du raccourci du bureau)
foreach ($f in Get-ChildItem -LiteralPath $app -Recurse -File) {
    $dest = Join-Path $racine $f.FullName.Substring($app.Length + 1)
    New-Item -ItemType Directory -Force -Path (Split-Path $dest -Parent) | Out-Null
    Copy-Item -LiteralPath $f.FullName -Destination $dest -Force
}
"Version $Version publiée."
