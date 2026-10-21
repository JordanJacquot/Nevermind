# Nevermind : l'écran (le kit d'écrans de la suite, vue.ps1 et le dossier ui), DA « console d'un futur lointain ».
#
# L'accueil : une rangée de tuiles 3D (Ton PC, Performances, Protection, Jeux, Réseau, Trafic, Overlay) ; en haut, le
# logo (retour à l'accueil), les onglets et les petites icônes (Rechercher, Paramètres, Aide). Les pages sont en
# « module déplié » (ecran-*.ps1). La logique reste celle de toujours : les pages lisent l'état de Nevermind, et les
# gestes appellent les mêmes fonctions, ou appuient sur les mêmes boutons (« clic:BtnPing » : le bouton BtnPing de
# l'interface d'avant, qui reste chargée sans être montrée). La fiche (un point à régler...) est lue et montrée dans
# l'écran (ecran-pc.ps1). Les questions et les messages s'affichent dans l'écran, dans le même style.
# Sans WebView2 (très rare), Nevermind garde son interface d'avant.
#
# L'écran est par-dessus toute la fenêtre (une WebView2 se dessine toujours au-dessus) : une page pas encore passée au
# kit, ou un panneau de l'interface d'avant (Paramètres, carte du réseau, tests), le cache le temps qu'il est ouvert.
# Chargé par OptiGame.ps1 après vue.ps1 ; Initialize-AccueilConsole est appelé par evenements.ps1.

$script:Kit = @{ Actif = $false; Fiches = @(); Constats = @(); Resultats = @(); Recherche = ''; Stats = @{}; Envoye = ''; Geste = [datetime]::MinValue
    PagesLourdes = @('optimisation', 'jeux', 'jeu')
    # Les pages déjà passées au kit (les autres ouvrent encore la page d'avant)
    PagesKit = @('pc', 'tableau', 'perf', 'optimisation', 'demarrage', 'nettoyage', 'protection', 'securite', 'connexion', 'sauvegarde', 'jeux', 'jeu', 'overlay', 'trafic') }

# ---------------------------------------------------------------------------
# Ce que montre l'écran
# ---------------------------------------------------------------------------
function Get-StatKit([int]$Index, [string]$Defaut = '') {
    $s = $script:Kit.Stats[$Index]
    if ($s -and $s.T) { $s.T } else { $Defaut }
}
function Get-LigneOutil([int]$Index, [string]$Nom) {
    @{ type = 'ligne'; id = "page:$Index"; texte = $Nom; note = (Get-StatKit $Index ' '); valeur = 'Ouvrir' }
}

function Get-EcranNevermind {
    $a = $script:LastAnalysis
    $fd = Get-HubFindings $a
    $fn = Get-FirstName
    $salut = try { (Get-ThemeText 'Hello') -f $(if ($ThemeId -eq 'terminal') { $fn.ToLower() } else { $fn }) } catch { '' }
    $todo = $fd.Important.Count
    $resume = if (-not $a) { 'Analyse en cours...' } elseif ($todo -eq 1) { 'Une chose à régler' } elseif ($todo -gt 1) { "$todo choses à régler" }
        elseif ($fd.Manual.Count) { 'Prêt pour jouer' } else { 'Au top pour jouer' }
    $sec = $script:SecurityScore

    # Jeux : le dernier jeu lancé, sa jaquette en fond de la tuile
    $jeux = @{ id = 'jeux'; nom = 'Jeux'; icone = 'manette'; sous = 'Ta bibliothèque'; couleur = 'S' }
    try {
        if ($null -ne $script:Games) {
            $n = @(Get-LibraryGames).Count
            $jeux.sous = "$n jeu$(if ($n -gt 1) { 'x' })"
            $last = Get-DernierJeuKit
            if ($last) {
                $f = Get-HeroFile $last.Game; if (-not $f) { $f = Get-CoverFile $last.Game }
                if ($f) { $jeux.fond = Get-ImageVue $f }
                $jeux.sous = [string]$last.Game.Name
                if ($last.Last -and $last.Last.Avg) { $jeux.sous += (' · {0:N0} FPS' -f $last.Last.Avg) }
            }
        }
    } catch { Write-Log "Écran, jeux : $_" }
    $nbApp = @($script:NetList).Count

    # Les pages : chaque groupe vient de son fichier (ecran-*.ps1) ; une page qui échoue n'empêche pas les autres
    $pages = [ordered]@{}
    foreach ($groupe in 'Get-PagesPc', 'Get-PagesPerf', 'Get-PagesProtection', 'Get-PagesJeux', 'Get-PagesDivers') {
        $chrono = [Diagnostics.Stopwatch]::StartNew()
        try { $p = & $groupe; foreach ($k in $p.Keys) { $pages[$k] = $p[$k] } } catch { Write-Log "Écran, $groupe : $($_.Exception.Message) $($_.InvocationInfo.PositionMessage)" }
        if ($chrono.ElapsedMilliseconds -gt 120) { Write-Log "Écran : $groupe en $($chrono.ElapsedMilliseconds) ms" }
    }
    if ($script:Kit.Panneau) { $pages.panneau = $script:Kit.Panneau }
    if ($script:Kit.Fiche) { $pages.fiche = $script:Kit.Fiche }

    [ordered]@{
        app = @{ nom = 'Nevermind'; version = $AppVersion; logo = 'logo.png'; phrase = $salut }
        accueil = @{
            tuiles = @(
                @{ id = 'pc'; nom = 'Ton PC'; gros = $(if ($a) { [int]$a.Score } else { '...' }); petit = $(if ($a) { '/100' } else { '' }); sous = $resume; couleur = 'P'; alerte = [bool]$todo }
                @{ id = 'perf'; nom = 'Performances'; icone = 'eclair'; sous = (Get-StatKit 1 'Pour gagner des FPS'); couleur = 'P' }
                @{ id = 'protection'; nom = 'Protection'; gros = $(if ($null -ne $sec) { [int]$sec } else { $null }); petit = $(if ($null -ne $sec) { '/100' }); icone = 'bouclier'; sous = 'Pour rester tranquille'; couleur = 'T'; alerte = [bool]($null -ne $sec -and $sec -lt 50) }
                $jeux
                @{ id = 'reseau'; nom = 'Réseau'; icone = 'radar'; sous = $(if ($nbApp) { "$nbApp appareil$(if ($nbApp -gt 1) { 's' })" } else { 'Les appareils chez toi' }); couleur = 'S' }
                @{ id = 'trafic'; nom = 'Trafic'; icone = 'courbe'; sous = 'Ce qui sort de ton PC'; couleur = 'T' }
                @{ id = 'overlay'; nom = 'Overlay'; icone = 'jauge'; sous = 'Tes FPS en jeu'; couleur = 'vert' }
            )
            ronds = @(
                @{ id = 'recherche'; nom = 'Rechercher'; icone = 'loupe'; couleur = 'S' }
                @{ id = 'parametres'; nom = 'Paramètres'; icone = 'reglages'; couleur = 'doux' }
                @{ id = 'aide'; nom = 'Aide'; icone = 'aide'; couleur = 'T' }
            )
        }
        pages = $pages
        focus = $script:Kit.Focus
        ecran = $script:Kit.RetourEcran
    }
}

# Renvoie l'écran (seulement s'il a changé, et seulement s'il est à l'écran : caché, il est refait en revenant)
function Update-EcranNevermind {
    if (-not $script:Kit.Actif) { return }
    if (-not $script:Vue.Controle -or $ui.EcranKit.Visibility -ne 'Visible') { $script:Kit.Envoye = ''; return }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $e = Get-EcranNevermind
    $cle = ($e | ConvertTo-Json -Depth 16 -Compress) -replace '":\{\}', '":null'
    if ($sw.ElapsedMilliseconds -gt 300) { Write-Log "Écran : préparé en $($sw.ElapsedMilliseconds) ms" }
    if ($cle -eq $script:Kit.Envoye) { return }
    $script:Kit.Envoye = $cle
    if ($EnTest -and $env:ESSAI_DOSSIER) { try { [IO.File]::WriteAllText((Join-Path $env:ESSAI_DOSSIER 'etat.json'), $cle) } catch {} }   # (essais : l'état envoyé)
    Send-Vue $e
}

# ---------------------------------------------------------------------------
# Les gestes
# ---------------------------------------------------------------------------
# Une page pas encore passée au kit : celle d'avant. Au retour, la tuile ou la page d'où l'on vient reste choisie.
function Open-PageKit([int]$Index, [string]$Tuile, [string]$Page) {
    if ($Page) { $script:Kit.RetourEcran = $Page; $script:Kit.Focus = "l:page:$Index" }
    elseif ($Tuile) { $script:Kit.Focus = "t:$Tuile" }
    if ($script:OrgViewOn) { Show-OrgView $false }
    Show-Page $Index
}
# Toute navigation de Nevermind vers une page d'avant (bouton d'une fiche « tab:4 », résultat de recherche...) va à la
# page du kit quand elle existe
$KitPagesIndex = @{ 0 = 'tableau'; 1 = 'optimisation'; 2 = 'demarrage'; 3 = 'connexion'; 4 = 'nettoyage'; 5 = 'tests'; 6 = 'securite'; 7 = 'sauvegarde'
    $GamesIndex = 'jeux'; $NetIndex = 'reseau'; $TrafficIndex = 'trafic'; $OverlayIndex = 'overlay' }
$script:KitShowPage = ${function:Show-Page}
function Show-Page([int]$Index) {
    $id = if ($script:Kit.Actif) { $KitPagesIndex[$Index] } else { $null }
    if ($id -and $script:Kit.PagesKit -contains $id) {
        $script:Kit.RetourEcran = $id   # (si l'écran est en train de revenir : il ira sur la page dès qu'il est prêt)
        & $script:KitShowPage $HubIndex
        if ($script:Vue.Pret) { Open-EcranVue $id }
        return
    }
    & $script:KitShowPage $Index
}
$script:KitFindingAction = ${function:Invoke-FindingAction}
function Invoke-FindingAction([string]$Target) {
    if ($Target -like 'tab:*') { Show-Page ([int]$Target.Substring(4)); return }
    & $script:KitFindingAction $Target
}

# Appuie sur un bouton (ou une case) de l'interface d'avant : exactement le même geste, avec ses vérifications
function Invoke-BoutonKit([string]$Nom, $Valeur) {
    $b = $ui[$Nom]
    if (-not $b) { Write-Log "Écran : bouton inconnu ($Nom)"; return }
    if ($b -is [System.Windows.Controls.CheckBox]) { $b.IsChecked = [bool]$Valeur }
    elseif (-not ($b -is [System.Windows.Controls.Primitives.ButtonBase])) { return }
    if (-not $b.IsEnabled) { Show-MessageVue 'Un instant : Nevermind finit ce qu''il est en train de faire.'; return }
    $b.RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Primitives.ButtonBase]::ClickEvent)))
}
function Register-ActionsNevermind {
    Register-ActionVue 'analyser' { param($Valeur, $Ecran) Show-MessageVue 'Analyse de ton PC lancée'; Invoke-Safe { Invoke-Analysis } }
    Register-ActionVue 'parametres' { param($Valeur, $Ecran) Invoke-Safe { Show-Settings } }
    Register-ActionVue 'aide' { param($Valeur, $Ecran) Invoke-Safe { Show-Settings 'aide' } }
    Register-ActionVue 'jeux' { param($Valeur, $Ecran) Open-PageKit $GamesIndex 'jeux' }
    Register-ActionVue 'reseau' { param($Valeur, $Ecran) Open-PageKit $NetIndex 'reseau' }
    Register-ActionVue 'trafic' { param($Valeur, $Ecran) Open-PageKit $TrafficIndex 'trafic' }
    Register-ActionVue 'overlay' { param($Valeur, $Ecran) Open-PageKit $OverlayIndex 'overlay' }
    Register-ActionVue 'recherche' { param($Valeur, $Ecran)
        $q = ([string]$Valeur).Trim()
        $script:Kit.Recherche = $q
        $script:Kit.Resultats = @(if ($q) { Find-Settings $q 8 })
    }
    # La page (re)créée a reçu l'écran, avec la page et la tuile du retour : elles ne servent qu'une fois
    Register-ActionVue 'pret' { param($Valeur, $Ecran)
        $script:Kit.Focus = $null; $script:Kit.RetourEcran = $null; $script:Kit.Envoye = ''
        if ($script:Vue.Etat) { $script:Vue.Etat.Remove('focus'); $script:Vue.Etat.Remove('ecran') }
    }
    Register-ActionVue 'menu' { param($Valeur, $Ecran) if ($script:Embedded) { Request-MenuMindset } }
    # Les pages lourdes (profils par jeu...) ne sont préparées qu'à l'écran : en y arrivant, l'écran est refait
    Register-ActionVue 'ecran' { param($Id, $Rien) if ($script:Kit.PagesLourdes -contains $Id) { try { Update-EcranNevermind } catch { Write-Log "Écran : $_" } } }
    # Retour (B) sur une page qui le demande : la fiche se ferme comme avec son bouton
    Register-ActionVue 'retour' { param($Valeur, $Ecran)
        if ($Ecran -eq 'fiche' -and $script:SheetMode -ne 'display') { if ($ui.SheetClose.Visibility -eq 'Visible') { Invoke-BoutonKit 'SheetClose' } else { Close-Sheet } }
        elseif ($Ecran -eq 'panneau' -and $ui.BtnTestClose.IsEnabled) { Invoke-BoutonKit 'BtnTestClose' }
    }
    # Après chaque geste : l'écran à jour (et les messages de Nevermind des secondes qui suivent s'affichent)
    Register-ActionVue 'apres' { param($Id, $Ecran) $script:Kit.Geste = Get-Date; try { Update-EcranNevermind } catch { Write-Log "Écran : $_" } }
    Register-ActionsPc
    foreach ($f in 'Register-ActionsPerf', 'Register-ActionsProtection', 'Register-ActionsPanneau', 'Register-ActionsJeux', 'Register-ActionsDivers') { if (Get-Command $f -ErrorAction SilentlyContinue) { & $f } }
    Register-ActionVue '*' { param($Id, $Valeur, $Ecran)
        if ($Id -match '^(?:clic|coche):(\w{1,40})$') { Invoke-BoutonKit $Matches[1] $Valeur; return }
        foreach ($f in 'Invoke-ActionPerf', 'Invoke-ActionProtection', 'Invoke-ActionPanneau', 'Invoke-ActionJeux', 'Invoke-ActionDivers') { if ((Get-Command $f -ErrorAction SilentlyContinue) -and (& $f $Id $Valeur)) { return } }
        if ($Id -match '^page:(\d+)$') { Open-PageKit ([int]$Matches[1]) '' $(if ($Ecran -and $Ecran -ne 'accueil') { $Ecran }); return }
        if ($Id -match '^fiche:(\d+)$') { $f = $script:Kit.Fiches[[int]$Matches[1]]; if ($f) { Invoke-Safe { Open-Sheet @($f) } }; return }
        if ($Id -match '^constat:(\d+)$') { $f = $script:Kit.Constats[[int]$Matches[1]]; if ($f) { Invoke-Safe { Open-Sheet @($f) } }; return }
        if ($Id -match '^composant:(\d+)$') { $x = $script:Kit.ActionsComposants[[int]$Matches[1]]; if ($x) { Invoke-Safe { Invoke-FindingAction $x } }; return }
        if ($Id -match '^res:(\d+)$') { $r = $script:Kit.Resultats[[int]$Matches[1]]; if ($r) { Invoke-Safe { Open-SearchEntry $r } }; return }
        Write-Log "Écran : action sans suite ($Id)"
    }
}

# Retour au menu de Mindset (la console) : « menu » dans afficher-page.txt, puis le signal de Mindset (déjà ouvert).
# En test : rien (jamais le signal du vrai Mindset).
function Request-MenuMindset {
    if ($EnTest) { Write-Log 'Retour au menu de Mindset demandé (test : rien envoyé)'; return }
    try {
        $dir = Join-Path $env:LOCALAPPDATA 'Mindset'
        [IO.File]::WriteAllText((Join-Path $dir 'afficher-page.txt'), 'menu', (New-Object Text.UTF8Encoding($false)))
        $s = [System.Threading.EventWaitHandle]::OpenExisting('Local\Mindset-Afficher'); [void]$s.Set(); $s.Dispose()
        Write-Log 'Retour au menu de Mindset demandé'
    } catch { Write-Log "Retour au menu de Mindset impossible ($($_.Exception.Message))" }
}

# ---------------------------------------------------------------------------
# Les messages, les questions et le travail en cours : dans l'écran quand il est là
# ---------------------------------------------------------------------------
function Test-EcranKitMontre { $script:Kit.Actif -and $script:Vue.Pret -and $ui.EcranKit.Visibility -eq 'Visible' }
function Show-Message([string]$Text, [string]$Icon = 'Information') {
    if (Test-EcranKitMontre) { $r = Read-QuestionVue $Text '' @(@{ id = 'ok'; texte = 'OK' }); if ($null -ne $r) { return } }
    [System.Windows.MessageBox]::Show($Window, $Text, 'Nevermind', 'OK', $Icon) | Out-Null
}
function Confirm-Action([string]$Text) {
    if (Test-EcranKitMontre) { $r = Read-QuestionVue $Text '' @(@{ id = 'oui'; texte = 'Oui' }, @{ id = 'non'; texte = 'Non'; style = 'second' }); if ($null -ne $r) { return $r -eq 'oui' } }
    ([System.Windows.MessageBox]::Show($Window, $Text, 'Nevermind', 'YesNo', 'Question')) -eq 'Yes'
}
# Occupé : la pastille « en cours » montre ce que fait Nevermind ; un message qui arrive juste après un geste s'affiche
$script:KitSetBusy = ${function:Set-Busy}
$script:KitSetStatus = ${function:Set-Status}
function Set-Busy([bool]$Busy) {
    & $script:KitSetBusy $Busy
    if (-not $script:Kit.Actif) { return }
    $script:Kit.Occupe = $Busy
    if ($Busy) { Set-OccupeVue $(if ($ui.StatusText.Text -and $ui.StatusText.Text -ne 'Prêt.') { [string]$ui.StatusText.Text } else { 'Un instant...' }) }
    else { Set-OccupeVue '' }
}
function Set-Status([string]$Text) {
    & $script:KitSetStatus $Text
    if (-not $script:Kit.Actif -or $script:Starting -or -not $Text) { return }
    if ($script:Kit.Occupe) { Set-OccupeVue $Text }
    elseif (((Get-Date) - $script:Kit.Geste).TotalSeconds -lt 4 -and (Test-EcranKitMontre)) { Show-MessageVue $Text }
}

# ---------------------------------------------------------------------------
# Quand montrer l'écran
# ---------------------------------------------------------------------------
# Les panneaux de l'interface d'avant encore utilisés (la fiche et le panneau de détail, eux, sont montrés dans l'écran)
function Test-PanneauOuvert {
    foreach ($n in 'SettingsOverlay', 'NetMapOverlay', 'StartupOverlay') { if ($ui[$n] -and $ui[$n].Visibility -eq 'Visible') { return $true } }
    $false
}
# Sur l'accueil (page d'avant n° HubIndex), rien par-dessus : l'écran est montré. Un panneau d'avant le cache seulement.
# Une autre page d'avant, la carte du réseau ou le panneau des tests : l'écran est fermé tout à fait (une WebView2
# présente fige les grosses animations de l'interface d'avant, voir Close-Vue), et recréé au retour.
function Update-EcranKitVisible {
    if (-not $script:Kit.Actif) { return }
    $surAccueil = $ui.Tabs.SelectedIndex -eq $HubIndex
    $voir = $surAccueil -and -not (Test-PanneauOuvert)
    $fermer = ($ui.StartupOverlay.Visibility -ne 'Visible') -and (-not $surAccueil -or $ui.NetMapOverlay.Visibility -eq 'Visible')   # (pas pendant le chargement)
    if ($voir -and -not $script:Vue.Controle) { Initialize-Vue $ui.EcranKit; $script:Kit.Envoye = '' }
    $v = if ($voir) { 'Visible' } else { 'Collapsed' }
    if ($ui.EcranKit.Visibility -ne $v) {
        $ui.EcranKit.Visibility = $v
        if ($voir) { Update-EcranNevermind }
    }
    if ($fermer -and $script:Vue.Controle) { [void](Close-Vue) }
    if ($voir) { Update-FicheKit; Update-PanneauKit }
}

# Les pages d'avant : plus d'onglets en haut, mais « ← Accueil » et le nom de la page
$KitNomsPages = @{ $GamesIndex = 'Jeux'; $NetIndex = 'Réseau'; $TrafficIndex = 'Trafic'; $OverlayIndex = 'Overlay' }
function Update-NavBarKit {
    if (-not $script:Kit.Actif -or -not $script:NavBar) { return }
    $i = $ui.Tabs.SelectedIndex
    $sur = $i -ne $HubIndex
    $script:NavBar.Visibility = if ($sur) { 'Visible' } else { 'Collapsed' }
    if ($script:Kit.BoutonRetour) { $script:Kit.BoutonRetour.Visibility = $script:NavBar.Visibility }
    if ($sur) { $script:NavCrumb.Text = $(if ($PageNames[$i]) { $PageNames[$i] } else { $KitNomsPages[$i] }) }
}

# ---------------------------------------------------------------------------
# L'habillage des pages d'avant encore utilisées, pour qu'elles ressemblent à l'écran
# ---------------------------------------------------------------------------
# Le fond : le sol en grille qui file vers l'horizon et les étoiles, dessinés une fois (rien d'animé)
function New-FondConsole {
    $p = [System.Windows.Media.ColorConverter]::ConvertFromString((ConvertTo-ThemeHex '#00E5FF'))
    $s = [System.Windows.Media.ColorConverter]::ConvertFromString((ConvertTo-ThemeHex '#B04BFF'))
    $g = New-Object System.Windows.Media.DrawingGroup
    # (un cadre transparent de 1600 x 900 : le dessin garde ses proportions)
    $cadre = New-Object System.Windows.Media.GeometryDrawing ([System.Windows.Media.Brushes]::Transparent), $null, ([System.Windows.Media.RectangleGeometry]::new([System.Windows.Rect]::new(0, 0, 1600, 900)))
    $g.Children.Add($cadre)
    # Les étoiles (toujours les mêmes : le dessin ne change pas d'une ouverture à l'autre)
    $hasard = New-Object System.Random 7
    $etoiles = New-Object System.Windows.Media.GeometryGroup
    for ($i = 0; $i -lt 170; $i++) {
        $r = 0.6 + $hasard.NextDouble() * 1.1
        $etoiles.Children.Add([System.Windows.Media.EllipseGeometry]::new([System.Windows.Point]::new($hasard.NextDouble() * 1600, $hasard.NextDouble() * 520), $r, $r))
    }
    $ce = [System.Windows.Media.Color]::FromArgb(150, [byte](($p.R + 255 * 2) / 3), [byte](($p.G + 255 * 2) / 3), [byte](($p.B + 255 * 2) / 3))
    $g.Children.Add((New-Object System.Windows.Media.GeometryDrawing (New-Object System.Windows.Media.SolidColorBrush $ce), $null, $etoiles))
    # Le sol : des lignes qui partent vers l'horizon, et des traverses de plus en plus serrées au loin
    $h = 560.0; $bas = 900.0; $cx = 800.0
    $sol = New-Object System.Windows.Media.GeometryGroup
    for ($x = -3200; $x -le 4800; $x += 160) { $sol.Children.Add([System.Windows.Media.LineGeometry]::new([System.Windows.Point]::new($cx + ($x - $cx) * 0.06, $h), [System.Windows.Point]::new($x, $bas))) }
    for ($z = 1.0; $z -le 16; $z += 1) { $y = $h + ($bas - $h) / $z; $sol.Children.Add([System.Windows.Media.LineGeometry]::new([System.Windows.Point]::new(0, $y), [System.Windows.Point]::new(1600, $y))) }
    $trait = New-Object System.Windows.Media.Pen (New-Object System.Windows.Media.SolidColorBrush ([System.Windows.Media.Color]::FromArgb(70, $s.R, $s.G, $s.B))), 1.2
    $g.Children.Add((New-Object System.Windows.Media.GeometryDrawing $null, $trait, $sol))
    $g.Freeze()
    $pinceau = New-Object System.Windows.Media.DrawingBrush $g
    $pinceau.Stretch = 'UniformToFill'; $pinceau.AlignmentY = 'Bottom'
    $rect = New-Object System.Windows.Shapes.Rectangle
    $rect.Fill = $pinceau; $rect.IsHitTestVisible = $false
    # Le sol s'efface vers l'horizon
    $masque = New-Object System.Windows.Media.LinearGradientBrush
    $masque.StartPoint = '0,0'; $masque.EndPoint = '0,1'
    foreach ($a in @(@(0.0, 255), @(0.55, 255), @(0.64, 60), @(1.0, 255))) { $masque.GradientStops.Add((New-Object System.Windows.Media.GradientStop ([System.Windows.Media.Color]::FromArgb([byte]$a[1], 0, 0, 0)), $a[0])) }
    $rect.OpacityMask = $masque
    $rect.CacheMode = New-Object System.Windows.Media.BitmapCache
    $rect
}

function Initialize-HabillageConsole {
    # Le fond, sous le décor d'un thème perso
    $fond = $ui.Backdrop
    $i = $fond.Children.IndexOf($ui.BackdropDeco)
    $fond.Children.Insert([math]::Max(0, $i), (New-FondConsole))
    # L'en-tête des pages : le logo, « Nevermind › Page », comme dans l'écran (le grand titre de la page fait doublon)
    $nav = $script:NavBar
    if ($nav -and $script:NavCrumb) {
        $nav.Children.Clear()
        $logo = New-Object System.Windows.Controls.Image
        $logo.Source = New-Object System.Windows.Media.Imaging.BitmapImage (New-Object Uri (Join-Path $UiDir 'logo.png'))
        $logo.Width = 40; $logo.Height = 40; $logo.Margin = New-Thickness 0 0 14 0
        [System.Windows.Media.RenderOptions]::SetBitmapScalingMode($logo, 'HighQuality')
        [void]$nav.Children.Add($logo)
        $nom = New-Text 'Nevermind' 21 '#A9A3C9' -Semi; $nom.VerticalAlignment = 'Center'
        [void]$nav.Children.Add($nom)
        $sep = New-Text '›' 21 '#6F6894'; $sep.Margin = New-Thickness 12 0 12 2; $sep.VerticalAlignment = 'Center'
        [void]$nav.Children.Add($sep)
        $script:NavCrumb.FontSize = 21; $script:NavCrumb.FontWeight = 'Bold'; $script:NavCrumb.Foreground = Get-Brush '#FFFFFF'
        [void]$nav.Children.Add($script:NavCrumb)
        Set-PackFont $nom; Set-PackFont $script:NavCrumb
    }
    $h1 = $Window.FindResource('H1')
    foreach ($item in $ui.Tabs.Items) {
        $c = $item.Content
        $tete = if ($c -is [System.Windows.Controls.Panel] -and $c.Children.Count) { $c.Children[0] } else { $null }
        if ($tete -is [System.Windows.Controls.StackPanel] -and $tete.Children.Count -and $tete.Children[0].Style -eq $h1) { $tete.Children[0].Visibility = 'Collapsed' }
    }
    # En bas à droite : « B Retour », comme dans l'écran (cliquable)
    $dock = $ui.StatusText.Parent
    $b = New-Object System.Windows.Controls.StackPanel
    $b.Orientation = 'Horizontal'; $b.Cursor = [System.Windows.Input.Cursors]::Hand; $b.Margin = New-Thickness 18 0 0 0; $b.Background = [System.Windows.Media.Brushes]::Transparent
    $rond = New-Object System.Windows.Controls.Border
    $rond.Width = 26; $rond.Height = 26; $rond.CornerRadius = [System.Windows.CornerRadius]::new(13); $rond.Background = Get-Brush '#ECEAF7'
    $lettre = New-Text 'B' 13 '#14102A' -Bold; $lettre.HorizontalAlignment = 'Center'; $lettre.VerticalAlignment = 'Center'
    $rond.Child = $lettre
    [void]$b.Children.Add($rond)
    $t = New-Text 'Retour' 15 '#ECEAF7'; $t.Margin = New-Thickness 8 0 0 0; $t.VerticalAlignment = 'Center'
    [void]$b.Children.Add($t)
    $b.Add_MouseLeftButtonUp({ if ($ui.Tabs.SelectedIndex -ne $HubIndex -and -not (Test-PanneauOuvert)) { if ($script:OrgViewOn) { Show-OrgView $false }; Show-Page $HubIndex } })
    [System.Windows.Controls.DockPanel]::SetDock($b, 'Right')
    $dock.Children.Insert(0, $b)
    $script:Kit.BoutonRetour = $b
}

function Initialize-AccueilConsole {
    $raison = Test-VuePossible
    if ($raison) { Write-Log "Écran impossible ($raison) : interface d'avant"; return }
    $script:VueVerifiee = $true   # (Initialize-Vue ne refait pas la vérification)
    $script:Kit.Actif = $true
    # La visite guidée du premier lancement parle de cet écran
    $script:TourSteps = @(
        @{ Title = 'Bienvenue dans Nevermind'; Lines = @(
            'Nevermind analyse ton PC et le règle pour que tes jeux tournent au mieux.',
            'Rien n''est modifié sans ton accord : chaque changement passe par un bouton sur lequel tu cliques.') },
        @{ Title = 'L''accueil'; Lines = @(
            'Les grandes tuiles : Ton PC et sa note sur 100, Performances, Protection, Jeux, Réseau, Trafic et Overlay.',
            'Fais glisser la rangée à la souris, ou les flèches du clavier : un clic sur la tuile du milieu (ou Entrée) l''ouvre, Échap revient.',
            'En haut, le logo ramène à l''accueil ; les onglets mènent aux mêmes pages, et les petites icônes à droite : Rechercher, Paramètres et Aide.') },
        @{ Title = 'Tout est annulable'; Lines = @(
            'Chaque changement est sauvegardé avant d''être fait.',
            'Après une correction, « Revenir en arrière » annule tout de suite. Plus tard, la page Sauvegarde (dans Protection) garde l''historique : tu peux annuler n''importe quel changement.',
            'Dans Ton PC, choisis un point à régler, lis la fiche, puis « Exécuter » pour que Nevermind le corrige.') }
    )
    Register-ActionsNevermind
    Initialize-Vue $ui.EcranKit
    # Plus de barre d'onglets : la navigation passe par l'écran
    $barre = $ui.Tabs.Template.FindName('BarreHaut', $ui.Tabs)
    if ($barre) { $barre.Visibility = 'Collapsed' }
    $back = $ui.Tabs.Template.FindName('NavBack', $ui.Tabs)
    if ($back) { $back.Content = '←  Accueil' }
    try { Initialize-HabillageConsole } catch { Write-Log "Habillage console : $_" }
    Update-NavBarKit
    $ui.Tabs.Add_SelectionChanged({ param($s, $e) if ($e.OriginalSource -eq $ui.Tabs) { Update-NavBarKit; Update-EcranKitVisible } })
    # Échap sur une page d'avant : retour à l'écran (avant les autres touches ; un panneau ouvert se ferme d'abord)
    # Ctrl+K : la recherche
    $Window.Add_PreviewKeyDown({
        param($s, $e)
        if (-not $script:Kit.Actif) { return }
        if (($e.Key -eq 'K' -or $e.Key -eq 'F') -and [System.Windows.Input.Keyboard]::Modifiers -eq 'Control') {
            $e.Handled = $true
            if (Test-PanneauOuvert) { return }
            Show-Page $HubIndex; Open-EcranVue 'recherche'; return
        }
        if ($e.Key -ne 'Escape' -or $ui.Tabs.SelectedIndex -eq $HubIndex -or (Test-PanneauOuvert)) { return }
        if ($script:OrgViewOn) { return }   # (l'organizer a son propre Échap)
        $e.Handled = $true
        Show-Page $HubIndex
    })
    $t = New-Object System.Windows.Threading.DispatcherTimer
    $t.Interval = [TimeSpan]::FromMilliseconds(200)
    $script:KitTic = 0
    $t.Add_Tick({
        try { Update-EcranKitVisible } catch {}
        # Toutes les 1,6 s : les mesures en direct ; toutes les 4 s : l'écran à jour (tests, analyses en fond...)
        $script:KitTic++
        if ($script:KitTic % 8 -eq 0) { try { Send-MesuresKit } catch {} }
        if ($script:KitTic % 20 -eq 0 -and $ui.EcranKit.Visibility -eq 'Visible' -and -not $script:Vue.Cadre) { try { Update-EcranNevermind } catch { Write-Log "Écran : $_" } }
    })
    $t.Start()
    $script:Kit.Minuterie = $t
    Update-EcranNevermind
    Write-Log 'Écran prêt (accueil)'
}
