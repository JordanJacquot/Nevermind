# Les deux zips d'une version de Nevermind, faits depuis son dossier prêt à partager (« Nevermind », construit par
# construire.ps1, ou signé par SignPath dans .github\workflows\release.yml) :
#   Nevermind.zip  le dossier « Nevermind », script fichiers\Nevermind.ps1 ;
#   OptiGame.zip   le zip de compatibilité, publié aussi pendant le changement de nom : le même contenu sous les anciens
#                  noms (dossier « OptiGame », script fichiers\OptiGame.ps1), ceux que cherchent les Mindset 2.12.0 et plus
#                  anciens (et Nevermind 1.11.0 pour ses propres mises à jour). Seul le nom du script change : il reste
#                  signé (la signature ne dépend pas du nom du fichier), et le lanceur et le script lui-même le trouvent
#                  sous ses deux noms. Un seul script principal par zip : jamais deux à côté.
#
# Ce qui part dans les zips (audit du 25/10, constat 24) : seulement les fichiers de Nevermind suivis par git, pris dans
# ce dossier (le dossier signé garde ainsi ses fichiers signés), et les 4 programmes construits. Jamais un fichier
# ignoré ou oublié qui traîne dans le dossier : aide-temp\ (laissé par une Aide lancée depuis le dépôt), un thème
# personnel (ignoré par git, il ne se voyait pas dans « git status »)... Ceux-là sont seulement nommés à l'écran.
#
#   .\outils\zips.ps1 -Dossier .\Nevermind -Sortie .
#   -Liste : les chemins à prendre (relatifs au dossier), à la place de ceux de git (tests)
param(
    [Parameter(Mandatory = $true)][string]$Dossier,
    [Parameter(Mandatory = $true)][string]$Sortie,
    [string[]]$Liste
)
$ErrorActionPreference = 'Stop'
$Dossier = [IO.Path]::GetFullPath($Dossier).TrimEnd('\')
if ((Split-Path $Dossier -Leaf) -ne 'Nevermind') { throw "Le dossier doit s'appeler Nevermind (c'est le nom du dossier dans le zip) : $Dossier" }
if (-not (Test-Path -LiteralPath (Join-Path $Dossier 'fichiers\Nevermind.ps1'))) { throw "fichiers\Nevermind.ps1 absent de $Dossier" }
if (Test-Path -LiteralPath (Join-Path $Dossier 'fichiers\OptiGame.ps1')) { throw "fichiers\OptiGame.ps1 ne doit pas être dans $Dossier (deux scripts principaux)" }
New-Item -ItemType Directory -Force -Path $Sortie | Out-Null
$Sortie = [IO.Path]::GetFullPath($Sortie)

# Les programmes construits par construire.ps1 (ignorés par git)
$Programmes = @('Mindset.exe', 'Nevermind.exe', 'NevermindAide.exe', 'Désinstaller Nevermind.exe')

# Les fichiers du dossier Nevermind suivis par git (chemins relatifs à ce dossier)
function Get-FichiersGit {
    $depot = Split-Path $PSScriptRoot -Parent
    $avant = [Console]::OutputEncoding
    try {
        [Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
        $sortieGit = & git -C $depot -c core.quotepath=false ls-files -z -- Nevermind 2>$null
        if ($LASTEXITCODE -ne 0) { throw "git ne donne pas la liste des fichiers de $depot (code $LASTEXITCODE)" }
    } finally { [Console]::OutputEncoding = $avant }
    @(($sortieGit -join '') -split "`0" | Where-Object { $_ -like 'Nevermind/*' } | ForEach-Object { $_.Substring(10) -replace '/', '\' })
}
if (-not $Liste) { $Liste = Get-FichiersGit }
if (-not @($Liste).Count) { throw 'Aucun fichier de Nevermind dans git : zips pas construits' }
$voulus = New-Object 'Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
foreach ($f in @($Liste) + $Programmes) { [void]$voulus.Add(($f -replace '/', '\').TrimStart('\')) }

$tmp = Join-Path ([IO.Path]::GetTempPath()) "nevermind-zips-$PID-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
try {
    # Une copie avec seulement ces fichiers-là
    $copie = Join-Path $tmp 'Nevermind'
    $laisses = New-Object Collections.Generic.List[string]
    foreach ($f in [IO.Directory]::GetFiles($Dossier, '*', 'AllDirectories')) {
        $rel = $f.Substring($Dossier.Length + 1)
        if (-not $voulus.Contains($rel)) { $laisses.Add($rel); continue }
        $dest = Join-Path $copie $rel
        [void][IO.Directory]::CreateDirectory((Split-Path $dest -Parent))
        [IO.File]::Copy($f, $dest)
    }
    if ($laisses.Count) { Write-Host "Laissé hors des zips (pas dans git) : $($laisses -join ', ')" -ForegroundColor Yellow }

    $zip = Join-Path $Sortie 'Nevermind.zip'
    Compress-Archive -Path $copie -DestinationPath $zip -Force

    # Le zip de compatibilité : le même contenu, sous les anciens noms
    $ancien = Join-Path $tmp 'OptiGame'
    [IO.Directory]::Move($copie, $ancien)
    Rename-Item -LiteralPath (Join-Path $ancien 'fichiers\Nevermind.ps1') -NewName 'OptiGame.ps1'
    $compat = Join-Path $Sortie 'OptiGame.zip'
    Compress-Archive -Path $ancien -DestinationPath $compat -Force
} finally { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }
$zip
$compat
