# Test du panneau de détail et de la fiche dans l'écran (console), sans fenêtre, sans WebView2 et sans rien toucher au PC.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-panneau.ps1
#
# Bug signalé : dans la page Trafic, le détail d'un programme (trafic-app:N) et « Ce que Windows envoie à Microsoft »
# (bouton Voir) ne montraient plus rien. Le geste était traité et le panneau rempli, mais l'écran n'y menait qu'à la
# première ouverture : un panneau quitté par un onglet du haut ou le logo (sans Retour ni Fermer) restait ouvert
# derrière, et les ouvertures suivantes ne changeaient plus l'écran. Même chose pour la fiche (Ton PC).
# Ce script charge ecran-panneau.ps1 et ecran-pc.ps1 tels quels, avec de faux éléments à la place de la fenêtre et de
# l'écran (Open-EcranVue fait comme la page : il change d'écran et prévient l'appli, message « ecran »), et vérifie :
#   1. un panneau rouvert après un départ par un onglet mène de nouveau à l'écran du panneau ;
#   2. le panneau quitté par un onglet est fermé (comme par Retour), sauf si Fermer est grisé (test en cours) ;
#   3. même grisé et resté ouvert derrière, une nouvelle ouverture (« Ce que Windows envoie à Microsoft ») y mène ;
#   4. Fermer ramène à la page d'où l'on venait ; aller du panneau à une fiche le garde ouvert ;
#   5. la fiche : mêmes règles, et la vérification de l'écran après un changement de fréquence n'est jamais fermée.
# Code de sortie : 0 si tout passe, 1 sinon.

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0
function Test-Ok([bool]$Condition, [string]$Message) {
    $script:nbTests++
    if (-not $Condition) { $script:echecs.Add($Message) }
}

# ---------------------------------------------------------------------------
# Faux éléments : la fenêtre d'avant (panneau, fiche, leurs boutons) et l'écran
# ---------------------------------------------------------------------------
$ui = @{
    TestOverlay = [pscustomobject]@{ Visibility = 'Collapsed' }; BtnTestClose = [pscustomobject]@{ IsEnabled = $true }
    Overlay = [pscustomobject]@{ Visibility = 'Collapsed' }; SheetClose = [pscustomobject]@{ Visibility = 'Visible'; IsEnabled = $true }
}
$script:Kit = @{ Geste = (Get-Date).AddMinutes(-1) }
$script:Vue = @{ Ecran = 'trafic' }
$script:SheetMode = 'fix'
$script:Journal = New-Object System.Collections.Generic.List[string]
$script:Titre = ''
function Write-Log([string]$T) { $script:Journal.Add($T) }
function Show-TestPanel($Tile) { $script:Titre = $Tile.Title; $ui.TestOverlay.Visibility = 'Visible' }
function Hide-TestPanel { $ui.TestOverlay.Visibility = 'Collapsed' }
function Open-Sheet($Items) { $script:SheetMode = 'fix'; $script:Titre = [string]@($Items)[0]; $ui.Overlay.Visibility = 'Visible' }
function Close-Sheet { $ui.Overlay.Visibility = 'Collapsed' }
function Invoke-BoutonKit([string]$Nom) {
    if ($Nom -eq 'BtnTestClose') { if ($ui.BtnTestClose.IsEnabled) { Hide-TestPanel } }
    elseif ($Nom -eq 'SheetClose') { Close-Sheet }
}
function Update-EcranNevermind { }

. (Join-Path $modules 'ecran-pc.ps1')
. (Join-Path $modules 'ecran-panneau.ps1')

# (après le chargement : le contenu lu dans la vraie fenêtre est remplacé par le titre ouvert)
function Get-PanneauKit { @{ titre = $script:Titre; blocs = @() } }
function Get-FicheKit { @{ titre = $script:Titre; blocs = @() } }
# La page : elle change d'écran et, seulement s'il a changé, le dit à l'appli (message « ecran », accueil-console.ps1)
function Open-EcranVue([string]$Ecran) {
    if ($script:Vue.Ecran -eq $Ecran) { return }
    $script:Vue.Ecran = $Ecran
    if (Get-Command Close-QuitteKit -ErrorAction SilentlyContinue) { Close-QuitteKit $Ecran }
}
# Les 0,2 s de la minuterie de l'écran (Update-EcranKitVisible)
function Step-Ecran { Update-FicheKit; Update-PanneauKit }

# ---------------------------------------------------------------------------
# 1 et 2. Détail d'un programme du trafic, deux fois, en partant par l'onglet Performances entre les deux
# ---------------------------------------------------------------------------
foreach ($n in 1, 2) {
    Open-EcranVue 'trafic'
    Show-TestPanel @{ Title = "Programme $n" }; Step-Ecran
    Test-Ok ($script:Vue.Ecran -eq 'panneau') "détail du programme, fois $n : l'écran reste sur « $($script:Vue.Ecran) » au lieu du panneau"
    Test-Ok ($script:Kit.Panneau.titre -eq "Programme $n") "détail du programme, fois $n : titre « $($script:Kit.Panneau.titre) »"
    Open-EcranVue 'perf'; Step-Ecran
    Test-Ok ($ui.TestOverlay.Visibility -eq 'Collapsed') "fois $n : le panneau reste ouvert derrière après un départ par un onglet"
    Test-Ok ($script:Vue.Ecran -eq 'perf') "fois $n : la fermeture du panneau a ramené l'écran sur « $($script:Vue.Ecran) » au lieu de rester sur Performances"
}

# ---------------------------------------------------------------------------
# 3. Fermer grisé (test en cours) : le panneau reste ouvert derrière, et le suivant y mène quand même
# ---------------------------------------------------------------------------
Open-EcranVue 'trafic'
Show-TestPanel @{ Title = 'Programme 3' }; Step-Ecran
$ui.BtnTestClose.IsEnabled = $false
Open-EcranVue 'trafic'; Step-Ecran
Test-Ok ($ui.TestOverlay.Visibility -eq 'Visible') 'panneau fermé en partant alors que Fermer était grisé'
$ui.BtnTestClose.IsEnabled = $true
Show-TestPanel @{ Title = 'Ce que Windows envoie à Microsoft' }; Step-Ecran
Test-Ok ($script:Vue.Ecran -eq 'panneau') "« Ce que Windows envoie à Microsoft » derrière un panneau resté ouvert : l'écran reste sur « $($script:Vue.Ecran) »"
Test-Ok ($script:Kit.Panneau.parent -eq 'trafic') "« Ce que Windows envoie à Microsoft » : page d'origine « $($script:Kit.Panneau.parent) » au lieu de Trafic"
# Rouvert alors qu'on est déjà dessus (Relancer un test) : rien ne bouge
$avant = $script:Kit.PanneauDe
Show-TestPanel @{ Title = 'Ce que Windows envoie à Microsoft' }; Step-Ecran
Test-Ok ($script:Vue.Ecran -eq 'panneau' -and $script:Kit.PanneauDe -eq $avant) "panneau rouvert sur lui-même : écran « $($script:Vue.Ecran) », origine « $($script:Kit.PanneauDe) »"

# ---------------------------------------------------------------------------
# 4. Une fiche ouverte depuis le panneau le garde ; Fermer ramène à la page d'origine
# ---------------------------------------------------------------------------
Open-Sheet @('Fiche depuis le panneau'); Step-Ecran
Test-Ok ($script:Vue.Ecran -eq 'fiche') "fiche ouverte depuis le panneau : écran « $($script:Vue.Ecran) »"
Test-Ok ($ui.TestOverlay.Visibility -eq 'Visible') 'le panneau se ferme quand une fiche s''ouvre par-dessus'
Close-Sheet; Step-Ecran
Test-Ok ($script:Vue.Ecran -eq 'panneau') "fiche fermée : retour sur « $($script:Vue.Ecran) » au lieu du panneau"
Hide-TestPanel; Step-Ecran
Test-Ok ($script:Vue.Ecran -eq 'trafic') "Fermer : retour sur « $($script:Vue.Ecran) » au lieu de Trafic"

# ---------------------------------------------------------------------------
# 5. La fiche (Ton PC) : mêmes règles
# ---------------------------------------------------------------------------
foreach ($n in 1, 2) {
    Open-EcranVue 'pc'
    Open-Sheet @("Fiche $n"); Step-Ecran
    Test-Ok ($script:Vue.Ecran -eq 'fiche') "fiche, fois $n : l'écran reste sur « $($script:Vue.Ecran) » au lieu de la fiche"
    Test-Ok ($script:Kit.Fiche.titre -eq "Fiche $n") "fiche, fois $n : titre « $($script:Kit.Fiche.titre) »"
    Open-EcranVue 'perf'; Step-Ecran
    Test-Ok ($ui.Overlay.Visibility -eq 'Collapsed') "fiche, fois $n : elle reste ouverte derrière après un départ par un onglet"
}
# La vérification de l'écran après un changement de fréquence n'est jamais fermée en partant (elle revient en arrière seule)
Open-EcranVue 'pc'
Open-Sheet @('Fiche 3'); Step-Ecran
$script:SheetMode = 'display'
Open-EcranVue 'perf'; Step-Ecran
Test-Ok ($ui.Overlay.Visibility -eq 'Visible') 'vérification de l''écran fermée en partant par un onglet'
Close-Sheet; $script:SheetMode = 'fix'; Step-Ecran

if ($echecs.Count) {
    Write-Host "ÉCHEC : $($echecs.Count) sur $nbTests" -ForegroundColor Red
    foreach ($e in $echecs) { Write-Host "  - $e" -ForegroundColor Red }
    exit 1
}
Write-Host "OK : $nbTests vérifications (panneau et fiche rouverts après un départ par un onglet, fermés comme par Retour, origine gardée)" -ForegroundColor Green
exit 0
