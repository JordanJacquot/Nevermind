# Nevermind : l'accueil façon console (le kit d'écrans de la suite, vue.ps1 et le dossier ui).
#
# À la place de l'accueil « Ordinateur » et des onglets du haut : une rangée de tuiles (Ton PC, Performances,
# Protection, Jeux, Réseau, Trafic, Overlay) et des boutons ronds (Analyser, Rechercher, Paramètres, Aide), sur le fond
# 3D aux couleurs du thème. Une tuile d'outil ouvre la page habituelle de Nevermind ; « Accueil » en haut de la page, ou
# Échap, ramène ici. Les pages passent au kit une par une ; en attendant, elles restent celles de toujours.
# Sans WebView2 (très rare), Nevermind garde son accueil et ses onglets d'avant.
#
# L'écran est par-dessus toute la fenêtre (une WebView2 se dessine toujours au-dessus) : il n'est montré que sur
# l'accueil, quand rien d'autre n'est ouvert par-dessus (Paramètres, fiche, panneau des tests, carte du réseau).
# Chargé par OptiGame.ps1 après recherche.ps1 et vue.ps1 ; Initialize-AccueilConsole est appelé par evenements.ps1.

$script:Kit = @{ Actif = $false; Fiches = @(); Resultats = @(); Recherche = ''; Stats = @{}; Envoye = '' }

# ---------------------------------------------------------------------------
# Ce que montre l'accueil
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
    $script:Kit.Fiches = @(@($fd.Important) + @($fd.Manual) | Select-Object -First 8)
    $fn = Get-FirstName
    $salut = try { (Get-ThemeText 'Hello') -f $(if ($ThemeId -eq 'terminal') { $fn.ToLower() } else { $fn }) } catch { '' }
    $todo = $fd.Important.Count
    $resume = if (-not $a) { 'Analyse en cours...' } elseif ($todo -eq 1) { 'Une chose à régler' } elseif ($todo -gt 1) { "$todo choses à régler" }
        elseif ($fd.Manual.Count) { 'Prêt pour jouer' } else { 'Au top pour jouer' }
    $sec = $script:SecurityScore

    # Jeux : le dernier jeu lancé, sa jaquette en fond de la tuile
    $jeux = @{ id = 'jeux'; nom = 'Jeux'; icone = 'manette'; sous = 'Ta bibliothèque'; couleur = 'P' }
    try {
        if ($null -ne $script:Games) {
            $n = @(Get-LibraryGames).Count
            $jeux.sous = "$n jeu$(if ($n -gt 1) { 'x' })"
            $last = Get-LastPlayedGame
            if ($last) {
                $f = Get-HeroFile $last.Game; if (-not $f) { $f = Get-CoverFile $last.Game }
                if ($f) { $jeux.fond = Get-ImageVue $f }
                $jeux.sous = [string]$last.Game.Name
                if ($last.Last -and $last.Last.Avg) { $jeux.sous += (' · {0:N0} FPS' -f $last.Last.Avg) }
            }
        }
    } catch { Write-Log "Accueil console, jeux : $_" }
    $nbApp = @($script:NetList).Count

    # Ton PC : la note, ce qu'il reste à faire, les composants
    $lignesFaire = @(for ($k = 0; $k -lt $script:Kit.Fiches.Count; $k++) {
        $f = $script:Kit.Fiches[$k]
        $auto = $f.Fix -and $f.Fix.Auto
        @{ type = 'ligne'; id = "fiche:$k"; texte = [string]$f.Titre; note = $(if ($auto) { 'Nevermind le fait pour toi' } else { 'À faire toi-même, Nevermind explique comment' })
            valeur = $(if ($f.Gain -gt 0) { "+$([int]$f.Gain) points" } else { 'Voir' }); ton = $(if ($f.Status -eq 'bad') { 'urgent' } else { 'attention' }) }
    })
    $resumePc = @(
        $(if ($a) { @{ type = 'score'; valeur = [int]$a.Score; sur = 100; titre = $(if ($todo) { 'Encore un peu de marge' } else { 'Ton PC est au top' }); texte = [string]$ui.HubSub.Text } }
          else { @{ type = 'texte'; texte = 'Je regarde ton PC, ça prend quelques secondes.' } })
        $(if ($lignesFaire.Count) { @{ type = 'titre'; texte = 'Pour gagner des points' } })
    ) + $lignesFaire + @(@{ type = 'bouton'; id = 'analyser'; texte = 'Relancer l''analyse' }, @{ type = 'bouton'; id = 'page:0'; texte = 'Tableau de bord'; style = 'second' })
    $info = if ($a -and $a.Info) { $a.Info } else { @{} }
    $composants = @(@{ type = 'titre'; texte = $env:COMPUTERNAME }) + @(foreach ($k in 'Processeur', 'Carte graphique', 'Mémoire', 'Système') { if ($info[$k]) { @{ type = 'ligne'; texte = $k; valeur = [string]$info[$k] } } })

    $pages = [ordered]@{
        pc = @{ titre = 'Ton PC'; sections = @(
            @{ nom = 'Résumé'; blocs = @($resumePc | Where-Object { $_ }) }
            @{ nom = 'Outils'; blocs = @(@{ type = 'titre'; texte = 'Santé du PC' }; (Get-LigneOutil 0 'Tableau de bord'); (Get-LigneOutil 5 'Tests des composants')) }
            @{ nom = 'Composants'; blocs = $composants }
        ) }
        perf = @{ titre = 'Performances'; sections = @(
            @{ nom = 'Pour gagner des FPS'; blocs = @(@{ type = 'titre'; texte = 'Pour gagner des FPS' }; (Get-LigneOutil 1 'Optimisation gaming'); (Get-LigneOutil 2 'Démarrage'); (Get-LigneOutil 4 'Nettoyage')) }
        ) }
        protection = @{ titre = 'Protection'; sections = @(
            @{ nom = 'Pour rester tranquille'; blocs = @(
                $(if ($null -ne $sec) { @{ type = 'score'; valeur = [int]$sec; sur = 100; titre = $(if ($sec -ge 80) { 'Bien protégé' } elseif ($sec -ge 50) { 'Quelques points à voir' } else { 'Vérifie ta protection' }); texte = 'Antivirus, programmes cachés, réseau, sauvegarde.' } }
                  else { @{ type = 'texte'; texte = 'Ouvre Sécurité pour vérifier ta protection.' } })
                @{ type = 'titre'; texte = 'Outils' }; (Get-LigneOutil 6 'Sécurité'); (Get-LigneOutil 3 'Connexion'); (Get-LigneOutil 7 'Sauvegarde')) }
        ) }
        recherche = @{ titre = 'Rechercher'; sections = @(
            @{ nom = 'Rechercher'; blocs = @(@{ type = 'titre'; texte = 'Un réglage, un outil, un jeu' }; @{ type = 'champ'; id = 'recherche'; texte = 'Que cherches-tu ?'; valeur = $script:Kit.Recherche; indice = 'Par exemple : mode jeu' }) + @(
                for ($k = 0; $k -lt $script:Kit.Resultats.Count; $k++) { $r = $script:Kit.Resultats[$k]; @{ type = 'ligne'; id = "res:$k"; texte = [string]$r.T; note = [string]$r.Where; valeur = 'Y aller' } }) + @(
                if ($script:Kit.Recherche -and -not $script:Kit.Resultats.Count) { @{ type = 'texte'; texte = 'Rien trouvé. Essaie avec un autre mot.' } }) }
        ) }
    }
    [ordered]@{
        app = @{ nom = 'Nevermind'; version = $AppVersion; logo = 'logo.png'; phrase = $salut }
        accueil = @{
            tuiles = @(
                @{ id = 'pc'; nom = 'Ton PC'; gros = $(if ($a) { [int]$a.Score } else { '...' }); petit = '/100'; sous = $resume; couleur = $(if ($fd.Important | Where-Object { $_.Status -eq 'bad' }) { 'rouge' } elseif ($todo) { 'orange' } else { 'P' }) }
                @{ id = 'perf'; nom = 'Performances'; icone = 'eclair'; sous = (Get-StatKit 1 'Pour gagner des FPS'); couleur = 'P' }
                @{ id = 'protection'; nom = 'Protection'; gros = $(if ($null -ne $sec) { [int]$sec } else { $null }); petit = $(if ($null -ne $sec) { '/100' }); icone = 'bouclier'; sous = 'Pour rester tranquille'; couleur = 'T' }
                $jeux
                @{ id = 'reseau'; nom = 'Réseau'; icone = 'radar'; sous = $(if ($nbApp) { "$nbApp appareil$(if ($nbApp -gt 1) { 's' })" } else { 'Les appareils chez toi' }); couleur = 'S' }
                @{ id = 'trafic'; nom = 'Trafic'; icone = 'courbe'; sous = 'Ce qui sort de ton PC'; couleur = 'T' }
                @{ id = 'overlay'; nom = 'Overlay'; icone = 'jauge'; sous = 'Tes FPS en jeu'; couleur = 'vert' }
            )
            ronds = @(
                @{ id = 'analyser'; nom = 'Analyser'; icone = 'actu'; couleur = 'P' }
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

# Renvoie l'accueil à l'écran (seulement s'il a changé)
function Update-EcranNevermind {
    if (-not $script:Kit.Actif) { return }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $e = Get-EcranNevermind
    $cle = $e | ConvertTo-Json -Depth 12 -Compress
    if ($sw.ElapsedMilliseconds -gt 300) { Write-Log "Accueil console : préparé en $($sw.ElapsedMilliseconds) ms" }
    if ($cle -eq $script:Kit.Envoye) { return }
    $script:Kit.Envoye = $cle
    Send-Vue $e
}

# ---------------------------------------------------------------------------
# Les gestes
# ---------------------------------------------------------------------------
# Au retour : la tuile d'où l'on vient reste choisie ; ouvert depuis une page de l'accueil (Ton PC, Performances,
# Protection), on y revient, sur la ligne de l'outil
function Open-PageKit([int]$Index, [string]$Tuile, [string]$Page) {
    if ($Page) { $script:Kit.RetourEcran = $Page; $script:Kit.Focus = "l:page:$Index" }
    elseif ($Tuile) { $script:Kit.Focus = "t:$Tuile" }
    if ($script:OrgViewOn) { Show-OrgView $false }
    Show-Page $Index
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
        Update-EcranNevermind
    }
    # La page (re)créée a reçu l'accueil, avec la page et la tuile du retour : elles ne servent qu'une fois
    Register-ActionVue 'pret' { param($Valeur, $Ecran)
        $script:Kit.Focus = $null; $script:Kit.RetourEcran = $null; $script:Kit.Envoye = ''
        if ($script:Vue.Etat) { $script:Vue.Etat.Remove('focus'); $script:Vue.Etat.Remove('ecran') }
    }
    Register-ActionVue 'menu' { param($Valeur, $Ecran) if ($script:Embedded) { Request-MenuMindset } }
    Register-ActionVue '*' { param($Id, $Valeur, $Ecran)
        if ($Id -match '^page:(\d+)$') { Open-PageKit ([int]$Matches[1]) '' $(if ($Ecran -in 'pc', 'perf', 'protection') { $Ecran }); return }
        if ($Id -match '^fiche:(\d+)$') {
            $f = $script:Kit.Fiches[[int]$Matches[1]]
            if ($f) { Show-Page 0; Invoke-Safe { Open-Sheet @($f) } }
            return
        }
        if ($Id -match '^res:(\d+)$') { $r = $script:Kit.Resultats[[int]$Matches[1]]; if ($r) { Invoke-Safe { Open-SearchEntry $r } }; return }
        Write-Log "Accueil console : action sans suite ($Id)"
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
# Quand montrer l'écran, et la navigation des pages habituelles
# ---------------------------------------------------------------------------
function Test-PanneauOuvert {
    foreach ($n in 'SettingsOverlay', 'Overlay', 'NetMapOverlay', 'TestOverlay', 'StartupOverlay') { if ($ui[$n] -and $ui[$n].Visibility -eq 'Visible') { return $true } }
    $false
}
# Sur l'accueil, rien par-dessus : l'écran est montré. Un petit panneau posé sur l'accueil (Paramètres, fiche, visite
# guidée, chargement) le cache seulement. Une autre page, la carte du réseau ou le panneau des tests : l'écran est fermé
# tout à fait (une WebView2 présente fige les grosses animations de Nevermind, voir Close-Vue), et recréé au retour.
function Update-EcranKitVisible {
    if (-not $script:Kit.Actif) { return }
    $surAccueil = $ui.Tabs.SelectedIndex -eq $HubIndex
    $voir = $surAccueil -and -not (Test-PanneauOuvert)
    $fermer = ($ui.StartupOverlay.Visibility -ne 'Visible') -and (-not $surAccueil -or $ui.NetMapOverlay.Visibility -eq 'Visible' -or $ui.TestOverlay.Visibility -eq 'Visible')   # (pas pendant le chargement)
    if ($voir -and -not $script:Vue.Controle) { Initialize-Vue $ui.EcranKit; $script:Kit.Envoye = '' }
    $v = if ($voir) { 'Visible' } else { 'Collapsed' }
    if ($ui.EcranKit.Visibility -ne $v) {
        $ui.EcranKit.Visibility = $v
        if ($voir) { Update-EcranNevermind }
    }
    if ($fermer -and $script:Vue.Controle) { [void](Close-Vue) }
}

# Les pages habituelles : plus d'onglets en haut, mais « ← Accueil » et le nom de la page
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
# L'habillage des pages habituelles, pour qu'elles ressemblent à l'accueil console (en attendant de passer au kit)
# ---------------------------------------------------------------------------
# Le fond : le sol en grille qui file vers l'horizon et les étoiles de l'accueil, dessinés une fois (rien d'animé)
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
    # L'en-tête des pages : le logo, « Nevermind › Page », comme sur l'accueil (le grand titre de la page fait doublon)
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
    # En bas à droite : « B Retour », comme sur l'accueil (cliquable)
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
    if ($raison) { Write-Log "Accueil console impossible ($raison) : accueil d'avant"; return }
    $script:VueVerifiee = $true   # (Initialize-Vue ne refait pas la vérification)
    $script:Kit.Actif = $true
    # La visite guidée du premier lancement parle de ce nouvel accueil
    $script:TourSteps = @(
        @{ Title = 'Bienvenue dans Nevermind'; Lines = @(
            'Nevermind analyse ton PC et le règle pour que tes jeux tournent au mieux.',
            'Rien n''est modifié sans ton accord : chaque changement passe par un bouton sur lequel tu cliques.') },
        @{ Title = 'L''accueil'; Lines = @(
            'Les grandes tuiles : Ton PC et sa note sur 100, Performances, Protection, Jeux, Réseau, Trafic et Overlay.',
            'Choisis à la souris, ou avec les flèches du clavier : Entrée pour ouvrir, Échap pour revenir.',
            'Dans Ton PC, choisis un point à régler, lis la fiche, puis « Exécuter » pour que Nevermind le corrige.') },
        @{ Title = 'Tout est annulable'; Lines = @(
            'Chaque changement est sauvegardé avant d''être fait.',
            'Après une correction, « Revenir en arrière » annule tout de suite. Plus tard, la page Sauvegarde (dans Protection) garde l''historique : tu peux annuler n''importe quel changement.',
            'Les boutons ronds de l''accueil : Analyser, Rechercher un réglage (ou Ctrl + K), Paramètres (raccourci, démarrage, mises à jour) et Aide (avec « Signaler un problème »).') }
    )
    Register-ActionsNevermind
    Initialize-Vue $ui.EcranKit
    # Plus de barre d'onglets : la navigation passe par l'accueil
    $barre = $ui.Tabs.Template.FindName('BarreHaut', $ui.Tabs)
    if ($barre) { $barre.Visibility = 'Collapsed' }
    $back = $ui.Tabs.Template.FindName('NavBack', $ui.Tabs)
    if ($back) { $back.Content = '←  Accueil' }
    try { Initialize-HabillageConsole } catch { Write-Log "Habillage console : $_" }
    Update-NavBarKit
    $ui.Tabs.Add_SelectionChanged({ param($s, $e) if ($e.OriginalSource -eq $ui.Tabs) { Update-NavBarKit; Update-EcranKitVisible } })
    # Échap sur une page : retour à l'accueil (avant les autres touches ; un panneau ouvert se ferme d'abord comme avant)
    # Ctrl+K : la recherche de l'accueil
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
    $t.Add_Tick({ try { Update-EcranKitVisible } catch {} })
    $t.Start()
    $script:Kit.Minuterie = $t
    Update-EcranNevermind
    Write-Log 'Accueil console prêt'
}
