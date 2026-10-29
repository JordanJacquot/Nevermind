# Le zip d'une version de Nevermind, Nevermind.zip (dossier « Nevermind », script fichiers\Nevermind.ps1), fait depuis son
# dossier prêt à partager (« Nevermind », construit par construire.ps1, ou signé par SignPath dans
# .github\workflows\release.yml). Un seul script principal dans le zip : jamais deux à côté.
# Le zip de compatibilité OptiGame.zip (le même contenu sous les anciens noms, pour les Mindset 2.12.0 et plus anciens) a
# été publié de Nevermind 1.12.0 à 1.15.4, pendant le changement de nom ; il n'est plus fait (décision de Jordan du
# 29/10) : un ancien Mindset se met d'abord à jour lui-même, et Mindset 2.16 connaît Nevermind.zip.
#
# Ce qui part dans le zip (audit du 25/10, constat 24) : seulement les fichiers de Nevermind suivis par git, pris dans
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
# NevermindPont.dll (passage en Rust, étape 1a : les règles de l'Aide pour Nevermind, rust\nevermind-pont), seulement
# quand le dépôt la construit ; sans rust\nevermind-pont, le zip reste le même
if (Test-Path -LiteralPath (Join-Path (Split-Path $PSScriptRoot -Parent) 'rust\nevermind-pont\Cargo.toml')) { $Programmes += 'NevermindPont.dll' }

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
if (-not @($Liste).Count) { throw 'Aucun fichier de Nevermind dans git : zip pas construit' }
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
    if ($laisses.Count) { Write-Host "Laissé hors du zip (pas dans git) : $($laisses -join ', ')" -ForegroundColor Yellow }

    $zip = Join-Path $Sortie 'Nevermind.zip'
    Compress-Archive -Path $copie -DestinationPath $zip -Force
} finally { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }
$zip
