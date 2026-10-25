# Nevermind : fenêtre « Paramètres » (roue crantée en haut à droite).
# Un onglet par famille : Général, Jeux, Réseau et vie privée, Mises à jour, Aide.
# Les interrupteurs appellent les mêmes fonctions que les pages : un réglage changé ici est à jour partout.
# Chargé par OptiGame.ps1 après les pages et avant recherche.ps1.

$SettingsTabs = @(
    @{ Id = 'general'; Label = 'Général'; Glyph = 0xE80F; Panel = 'SetGeneral'; Sub = 'Raccourci, lancement au démarrage et visite guidée.' },
    @{ Id = 'jeux'; Label = 'Jeux'; Glyph = 0xE7FC; Panel = 'SetGames'; Sub = 'Ce que Nevermind fait pendant que tu joues.' },
    @{ Id = 'reseau'; Label = 'Réseau et vie privée'; Glyph = 0xE72E; Panel = 'SetNetwork'; Sub = 'Surveillance du réseau et ce qui est envoyé sur Internet.' },
    @{ Id = 'theme'; Label = 'Thème'; Glyph = 0xE790; Panel = 'SetTheme'; Sub = 'Change les couleurs et l''ambiance de Nevermind.' },
    @{ Id = 'maj'; Label = 'Mises à jour'; Glyph = 0xE895; Panel = 'SetUpdates'; Sub = 'Version de Nevermind et nouveautés.' },
    @{ Id = 'aide'; Label = 'Aide'; Glyph = 0xE897; Panel = 'SetHelp'; Sub = 'Un souci, une question ? C''est par ici.' }
)
if ($Managed) { @($SettingsTabs | Where-Object { $_.Id -eq 'theme' })[0].Sub = 'Le thème choisi ici change aussi celui de Mindset et de toute la suite.' }

# Ligne de réglage en verre : titre, explication, interrupteur
function New-SettingSwitch([string]$Title, [string]$Text, [bool]$On, [scriptblock]$OnClick) {
    $card = New-Card
    $card.Margin = New-Thickness 0 0 0 12
    $card.Padding = New-Thickness 18 16 18 16
    $row = New-SwitchRow $Title $Text $On $OnClick
    $row.Margin = New-Thickness 0
    $card.Child = $row
    $card
}

function New-SettingAction([string]$Title, [string]$Text, [string]$Button, [scriptblock]$OnClick) {
    $card = New-Card
    $card.Margin = New-Thickness 0 0 0 12
    $card.Padding = New-Thickness 18 16 18 16
    $g = New-Grid @('*', 'Auto')
    $sp = New-Object System.Windows.Controls.StackPanel
    [void]$sp.Children.Add((New-Text $Title 14 '#FFFFFF' -Semi))
    $t = New-Text $Text 12 '#A6A1BC'
    $t.Margin = New-Thickness 0 2 0 0
    [void]$sp.Children.Add($t)
    Add-ToGrid $g $sp 0
    $b = New-Button $Button
    $b.Margin = New-Thickness 16 0 0 0; $b.VerticalAlignment = 'Center'
    $b.Add_Click($OnClick)
    Add-ToGrid $g $b 1
    $card.Child = $g
    $card
}

function New-SettingGroup([string]$Text) {
    $t = New-SectionTitle $Text
    $t.Margin = New-Thickness 2 4 0 10
    $t
}

# Les réglages venant des pages : relus à chaque ouverture
function Build-SettingsPanels {
    # Général : visite guidée
    $ui.SetGeneralMore.Children.Clear()
    [void]$ui.SetGeneralMore.Children.Add((New-SettingAction 'Visite guidée' 'Revoir en 3 étapes comment marche Nevermind.' 'Revoir la visite' { Hide-Settings; Invoke-Safe { Show-Tour } }))

    # Jeux
    $p = $ui.SetGames
    $p.Children.Clear()
    [void]$p.Children.Add((New-SettingGroup 'Pendant la partie'))
    [void]$p.Children.Add((New-SettingSwitch 'Mesurer mes FPS quand je joue' 'Moyenne, chutes et courbe de chaque partie, gardées dans Jeux, Perf. In Game.' (Test-FpsMeasure) {
        param($s, $e) $on = [bool]$s.IsChecked
        Invoke-Safe { Set-FpsMeasure $on }
    }))
    [void]$p.Children.Add((New-SettingAction 'Compteur de FPS à l''écran' 'Affichage, style et position du compteur par-dessus le jeu.' 'Ouvrir Overlay' { Hide-Settings; Show-Page $OverlayIndex }))
    [void]$p.Children.Add((New-SettingSwitch 'Mesurer ma connexion quand je joue' 'Box, Internet et serveur du jeu pendant la partie, pour savoir d''où vient le lag.' (Test-LagMeasure) {
        param($s, $e) $on = [bool]$s.IsChecked
        Invoke-Safe {
            Set-Setting 'LagMeasure' $on
            if (-not $on -and $script:LagSession -and -not $script:LagSession.Seconds) { Stop-LagSession }
            Update-GameWatch
            Build-LagPanel
            Set-Status $(if ($on) { 'Mesure du lag activée pendant les parties.' } else { 'Mesure du lag désactivée.' })
        }
    }))
    [void]$p.Children.Add((New-SettingSwitch 'Mode jeu : fermer des applis pendant que je joue' 'Les applis choisies (page Optimisation gaming, Mode jeu) sont fermées puis relancées après la partie.' ([bool](Get-Setting 'GameMode' $false)) {
        param($s, $e) $on = [bool]$s.IsChecked
        Invoke-Safe {
            Set-Setting 'GameMode' $on
            Update-GameWatch
            Build-GameModeCard
            Set-Status $(if ($on) { 'Mode jeu automatique activé.' } else { 'Mode jeu automatique désactivé.' })
        }
    }))
    [void]$p.Children.Add((New-SettingGroup 'Bibliothèque'))
    [void]$p.Children.Add((New-SettingSwitch 'Jaquettes depuis Internet' 'Cherche les images des jeux qui n''en ont pas sur la boutique Steam puis Wikipédia (seul le nom du jeu est envoyé).' (Test-CoversOnline) {
        param($s, $e) $on = [bool]$s.IsChecked
        Invoke-Safe {
            Set-Setting 'LibCoversOnline' $on
            $ui.ChkLibCovers.IsChecked = $on
            if ($on) { Start-CoverDownload } else { Set-Status 'Jaquettes depuis Internet désactivées (celles déjà trouvées restent).' }
        }
    }))

    # Réseau et vie privée
    $p = $ui.SetNetwork
    $p.Children.Clear()
    [void]$p.Children.Add((New-SettingSwitch 'Me prévenir quand un nouvel appareil se connecte' 'Vérifié toutes les 10 minutes sur ton réseau, tant que Nevermind est ouvert.' ([bool](Get-Setting 'NetWatch' $false)) {
        param($s, $e) $on = [bool]$s.IsChecked
        Invoke-Safe {
            Set-NetWatch $on
            $ui.ChkNetWatch.IsChecked = $on
            Set-Status $(if ($on) { 'Surveillance du réseau activée.' } else { 'Surveillance du réseau désactivée.' })
        }
    }))
    [void]$p.Children.Add((New-SettingSwitch 'Identifier les serveurs sans nom' 'Page Trafic et Lag en ligne : cherche à qui appartient une adresse inconnue (Valve, Riot, Amazon...) dans l''annuaire public rdap.org. Seule l''adresse du serveur est envoyée.' ([bool](Test-AnnuaireServeurs)) {
        param($s, $e) $on = [bool]$s.IsChecked
        Set-Setting 'TrafficLookup' $on
        if ($script:TrafficLookupSwitch) { $script:TrafficLookupSwitch.IsChecked = $on }
    }))
    $script:LagPaysSwitch = New-SettingSwitch 'Pays du serveur dans Lag en ligne' 'Lag en ligne : cherche dans quel pays se trouve le serveur du jeu (ipinfo.io). Seule l''adresse du serveur est envoyée.' (Test-PaysServeur) {
        param($s, $e) Set-Setting 'LagPays' ([bool]$s.IsChecked)
    }
    [void]$p.Children.Add($script:LagPaysSwitch)
    [void]$p.Children.Add((New-SettingSwitch 'Vérifier le pilote NVIDIA au lancement' 'Si le PC a une carte NVIDIA : demande à NVIDIA (geforce.com) le numéro du dernier pilote. Rien sur ton PC n''est envoyé. Coupé : Nevermind regarde seulement l''âge de ton pilote.' (Test-PiloteNvidia) {
        param($s, $e) Set-Setting 'PiloteNvidia' ([bool]$s.IsChecked)
    }))
    [void]$p.Children.Add((New-SettingAction 'Ce que Windows envoie à Microsoft' 'Identifiants, réglages qui envoient plus que le minimum et envois vus en direct.' 'Voir' { Hide-Settings; Show-Page $TrafficIndex; Invoke-Safe { Show-WindowsPrivacy } }))
    $note = New-Text 'Nevermind ne déchiffre jamais tes connexions et n''envoie rien sur toi : il regarde seulement qui parle à qui depuis ton PC.' 12 '#655E7E'
    $note.Margin = New-Thickness 4 4 0 0
    [void]$p.Children.Add($note)

    # Aide
    $p = $ui.SetHelpMore
    $p.Children.Clear()
    [void]$p.Children.Add((New-SettingAction 'Visite guidée' 'Les bases de Nevermind en 3 étapes.' 'Revoir la visite' { Hide-Settings; Invoke-Safe { Show-Tour } }))
    [void]$p.Children.Add((New-SettingAction 'Rechercher un réglage' 'Tape ce que tu cherches dans la barre en haut (ou Ctrl + K) : un clic t''y emmène.' 'Ouvrir la recherche' { Hide-Settings; Focus-Search }))
    $ui.SettingsVersion.Text = "Nevermind $AppVersion"
    $script:ThemePick = Get-CurrentThemeKey
    Build-ThemePanel
}

function Set-SettingsTab([string]$Id) {
    $tab = @($SettingsTabs | Where-Object { $_.Id -eq $Id })[0]
    if (-not $tab) { $tab = $SettingsTabs[0] }
    foreach ($t in $SettingsTabs) {
        $ui[$t.Panel].Visibility = if ($t.Id -eq $tab.Id) { 'Visible' } else { 'Collapsed' }
        $b = $script:SettingsTabButtons[$t.Id]
        if (-not $b) { continue }
        $sel = $t.Id -eq $tab.Id
        $b.Background = if ($sel) { $Window.FindResource('AccentBg') } else { Get-Brush '#00FFFFFF' }
        $b.Effect = if ($sel) { New-Glow '#00E5FF' 14 0.45 } else { $null }
        foreach ($x in $b.Child.Children) { $x.Foreground = Get-Brush $(if ($sel) { '#08060F' } else { '#C9C3DD' }) }
    }
    $ui.SettingsTitle.Text = $tab.Label
    $ui.SettingsSub.Text = $tab.Sub
    $ui.SettingsScroll.ScrollToTop()
    $script:SettingsTab = $tab.Id
}

function Initialize-Settings {
    $script:SettingsTabButtons = @{}
    $ui.SettingsTabs.Children.Clear()
    foreach ($t in $SettingsTabs) {
        $b = New-Object System.Windows.Controls.Border
        $b.CornerRadius = [System.Windows.CornerRadius]::new(14)
        $b.Padding = New-Thickness 12 9 12 9
        $b.Margin = New-Thickness 0 0 0 4
        $b.Cursor = [System.Windows.Input.Cursors]::Hand
        $sp = New-Object System.Windows.Controls.StackPanel
        $sp.Orientation = 'Horizontal'
        $ic = New-Text ([string][char]$t.Glyph) 14 '#C9C3DD'
        $ic.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'; $ic.FontSize = 14
        $ic.Margin = New-Thickness 0 1 12 0; $ic.VerticalAlignment = 'Center'
        [void]$sp.Children.Add($ic)
        $lb = New-Text $t.Label 13.5 '#C9C3DD' -Semi
        $lb.VerticalAlignment = 'Center'
        [void]$sp.Children.Add($lb)
        $b.Child = $sp
        $b.Tag = $t.Id
        $b.Add_MouseEnter({ param($s, $e) if ($s.Tag -ne $script:SettingsTab) { $s.Background = Get-Brush '#14FFFFFF' } })
        $b.Add_MouseLeave({ param($s, $e) if ($s.Tag -ne $script:SettingsTab) { $s.Background = Get-Brush '#00FFFFFF' } })
        $b.Add_MouseLeftButtonUp({ param($s, $e) Set-SettingsTab ([string]$s.Tag) })
        [void]$ui.SettingsTabs.Children.Add($b)
        $script:SettingsTabButtons[$t.Id] = $b
    }
    $ui.SettingsClose.Add_Click({ Hide-Settings })
    $ui.SettingsBackdrop.Add_MouseLeftButtonUp({ Hide-Settings })
}

function Show-Settings([string]$Tab = 'general') {
    if (-not $script:SettingsTabButtons) { Initialize-Settings }
    Build-SettingsPanels
    Set-SettingsTab $Tab
    $o = $ui.SettingsOverlay
    $o.Opacity = 0
    $o.Visibility = 'Visible'
    Start-WpfAnim $o ([System.Windows.UIElement]::OpacityProperty) 1 180
    $ui.SettingsZoom.ScaleX = 0.96; $ui.SettingsZoom.ScaleY = 0.96
    Start-WpfAnim $ui.SettingsZoom ([System.Windows.Media.ScaleTransform]::ScaleXProperty) 1 220
    Start-WpfAnim $ui.SettingsZoom ([System.Windows.Media.ScaleTransform]::ScaleYProperty) 1 220
}

function Hide-Settings {
    if ($ui.SettingsOverlay.Visibility -ne 'Visible') { return }
    $ui.SettingsOverlay.Visibility = 'Collapsed'
}

# ---------------------------------------------------------------------------
# Onglet « Thème » : 5 cartes avec un aperçu dessiné dans les couleurs de chaque thème
# ---------------------------------------------------------------------------
function New-RawGradient([string[]]$Hex, [double]$X2 = 1, [double]$Y2 = 1) {
    $b = New-Object System.Windows.Media.LinearGradientBrush
    $b.StartPoint = [System.Windows.Point]::new(0, 0); $b.EndPoint = [System.Windows.Point]::new($X2, $Y2)
    for ($i = 0; $i -lt $Hex.Count; $i++) {
        [void]$b.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.ColorConverter]::ConvertFromString($Hex[$i]), $i / [math]::Max(1, $Hex.Count - 1)))
    }
    $b
}

# Mini fenêtre Nevermind dans les couleurs d'un thème (sans passer par la traduction du thème actuel)
function New-ThemePreview([string]$Id) {
    $c = { param($h) ConvertTo-ThemeHex $h $Id }
    $th = $AppThemes[$Id]
    $g = New-Object System.Windows.Controls.Grid
    $g.Height = 118
    $bg = New-Object System.Windows.Controls.Border
    $bg.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $bg.Background = New-RawGradient @((& $c '#0C0920'), (& $c '#06050F'))
    $bg.ClipToBounds = $true
    $cv = New-Object System.Windows.Controls.Canvas
    foreach ($blob in @(@($th.P, 10, -30), @($th.T, 200, 50))) {
        $e = New-Object System.Windows.Shapes.Ellipse
        $e.Width = 140; $e.Height = 120
        $rb = New-Object System.Windows.Media.RadialGradientBrush
        [void]$rb.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.ColorConverter]::ConvertFromString('#55' + $blob[0].Substring(1)), 0))
        [void]$rb.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.ColorConverter]::ConvertFromString('#00' + $blob[0].Substring(1)), 1))
        $e.Fill = $rb
        [System.Windows.Controls.Canvas]::SetLeft($e, $blob[1]); [System.Windows.Controls.Canvas]::SetTop($e, $blob[2])
        [void]$cv.Children.Add($e)
    }
    # Barre du haut : logo, onglet choisi en dégradé, deux onglets
    $bar = New-Object System.Windows.Controls.Border
    $bar.Width = 236; $bar.Height = 22; $bar.CornerRadius = [System.Windows.CornerRadius]::new(8)
    $bar.Background = New-RawBrush '#18FFFFFF'; $bar.BorderBrush = New-RawGradient @(('#88' + $th.P.Substring(1)), ('#88' + $th.T.Substring(1))) 1 0; $bar.BorderThickness = [System.Windows.Thickness]::new(1)
    [System.Windows.Controls.Canvas]::SetLeft($bar, 10); [System.Windows.Controls.Canvas]::SetTop($bar, 8)
    $bs = New-Object System.Windows.Controls.StackPanel
    $bs.Orientation = 'Horizontal'; $bs.Margin = [System.Windows.Thickness]::new(6, 0, 0, 0); $bs.VerticalAlignment = 'Center'
    $logo = New-Object System.Windows.Controls.TextBlock
    $logo.Text = 'N'; $logo.FontWeight = 'Black'; $logo.FontSize = Get-UiFontSize 11; $logo.Foreground = [System.Windows.Media.Brushes]::White
    $logo.Margin = [System.Windows.Thickness]::new(0, 0, 8, 0); $logo.VerticalAlignment = 'Center'
    [void]$bs.Children.Add($logo)
    $pill = New-Object System.Windows.Controls.Border
    $pill.Width = 44; $pill.Height = 12; $pill.CornerRadius = [System.Windows.CornerRadius]::new(6)
    $pill.Background = New-RawGradient @($th.P, $th.S) 1 0
    [void]$bs.Children.Add($pill)
    foreach ($k in 1..3) {
        $o = New-Object System.Windows.Controls.Border
        $o.Width = 26; $o.Height = 5; $o.CornerRadius = [System.Windows.CornerRadius]::new(3); $o.Margin = [System.Windows.Thickness]::new(8, 0, 0, 0)
        $o.Background = New-RawBrush (& $c '#668E88A8')
        [void]$bs.Children.Add($o)
    }
    $bar.Child = $bs
    [void]$cv.Children.Add($bar)
    # Deux anneaux et une carte « À faire »
    $x = 12
    foreach ($ring in @(@($th.P, $th.S), @($th.T, $th.S))) {
        $tr = New-Object System.Windows.Shapes.Ellipse
        $tr.Width = 44; $tr.Height = 44; $tr.StrokeThickness = 5; $tr.Stroke = New-RawBrush '#22FFFFFF'
        [System.Windows.Controls.Canvas]::SetLeft($tr, $x); [System.Windows.Controls.Canvas]::SetTop($tr, 44)
        [void]$cv.Children.Add($tr)
        $arc = New-Object System.Windows.Shapes.Path
        $arc.Data = [System.Windows.Media.Geometry]::Parse('M 22,2.5 A 19.5,19.5 0 1 1 3.4,27.9')
        $arc.Stroke = New-RawGradient @($ring[0], $ring[1]); $arc.StrokeThickness = 5; $arc.StrokeStartLineCap = 'Round'; $arc.StrokeEndLineCap = 'Round'
        $arc.Effect = New-Object System.Windows.Media.Effects.DropShadowEffect -Property @{ Color = [System.Windows.Media.ColorConverter]::ConvertFromString($ring[0]); BlurRadius = 10; ShadowDepth = 0; Opacity = 0.8 }
        [System.Windows.Controls.Canvas]::SetLeft($arc, $x); [System.Windows.Controls.Canvas]::SetTop($arc, 44)
        [void]$cv.Children.Add($arc)
        $x += 54
    }
    $todo = New-Object System.Windows.Controls.Border
    $todo.Width = 116; $todo.Height = 56; $todo.CornerRadius = [System.Windows.CornerRadius]::new(9)
    $todo.Background = New-RawGradient @(('#33' + $th.P.Substring(1)), ('#33' + $th.T.Substring(1)))
    $todo.BorderBrush = New-RawGradient @(('#AA' + $th.P.Substring(1)), ('#AA' + $th.T.Substring(1))); $todo.BorderThickness = [System.Windows.Thickness]::new(1)
    $ts = New-Object System.Windows.Controls.StackPanel
    $ts.Margin = [System.Windows.Thickness]::new(8, 7, 8, 0)
    $t1 = New-Object System.Windows.Controls.Border
    $t1.Width = 30; $t1.Height = 4; $t1.HorizontalAlignment = 'Left'; $t1.Background = New-RawBrush $th.P
    $t2 = New-Object System.Windows.Controls.Border
    $t2.Width = 80; $t2.Height = 7; $t2.HorizontalAlignment = 'Left'; $t2.Margin = [System.Windows.Thickness]::new(0, 6, 0, 0); $t2.Background = [System.Windows.Media.Brushes]::White; $t2.CornerRadius = [System.Windows.CornerRadius]::new(3)
    $t3 = New-Object System.Windows.Controls.Border
    $t3.Width = 40; $t3.Height = 11; $t3.HorizontalAlignment = 'Left'; $t3.Margin = [System.Windows.Thickness]::new(0, 7, 0, 0); $t3.CornerRadius = [System.Windows.CornerRadius]::new(6)
    $t3.Background = New-RawGradient @($th.P, $th.S) 1 0
    foreach ($y in $t1, $t2, $t3) { [void]$ts.Children.Add($y) }
    $todo.Child = $ts
    [System.Windows.Controls.Canvas]::SetLeft($todo, 122); [System.Windows.Controls.Canvas]::SetTop($todo, 42)
    [void]$cv.Children.Add($todo)
    $bg.Child = $cv
    [void]$g.Children.Add($bg)
    $g
}

# Copie gérée par Mindset : les réglages communs à la suite (pour Nevermind, le thème, packs de la suite compris) se règlent dans Mindset.
# Mindset lit %LOCALAPPDATA%\Mindset\afficher-page.txt à son lancement (ou quand on le relance, il se ramène devant).
# Son dossier vient du registre (clé de désinstallation, écrite par un administrateur), jamais d'un fichier modifiable,
# et il est lancé par l'Explorateur : sans les droits administrateur de Nevermind.
$ManagedThemeText = 'Le thème choisi ici devient celui de toute la suite : Mindset et ses applis le prennent aussi.'
function Open-MindsetSettings([string]$Page = 'apparence') {
    $dir = ''
    try { $dir = ([string](Get-ItemProperty -LiteralPath 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Mindset' -Name InstallLocation -ErrorAction Stop).InstallLocation).Trim().Trim('"') } catch {}
    $exe = if ($dir -and [IO.Path]::IsPathRooted($dir)) { Join-Path $dir 'Mindset.exe' } else { '' }
    if (-not $exe -or -not (Test-Path -LiteralPath $exe -PathType Leaf)) {
        Write-Log "Mindset introuvable (registre : « $dir »)"
        Show-Message "Mindset est introuvable sur ce PC. Ouvre-le depuis son raccourci pour changer ce réglage de la suite." 'Warning'
        return
    }
    if ($Page -notin 'general', 'apparence', 'son') { $Page = 'general' }
    try { [IO.File]::WriteAllText((Join-Path (Split-Path $SuiteFile) 'afficher-page.txt'), "parametres:$Page", (New-Object Text.UTF8Encoding $false)) }
    catch { Write-Log "Mindset, page à ouvrir : $($_.Exception.Message)" }
    Open-Url $exe
    Set-Status 'Réglages de Mindset ouverts.'
}

function Get-CurrentThemeKey { if ($ThemePack) { "pack:$($ThemePack.Id)" } else { $ThemeId } }

# Les choix de l'onglet : les 5 thèmes de base puis les packs installés sur ce PC
# (géré par Mindset : seulement les packs de la suite, que Mindset et les autres applis ont aussi)
function Get-ThemeChoices {
    $list = @(foreach ($id in @($AppThemes.Keys | Where-Object { -not $AppThemes[$_].Pack })) { $t = $AppThemes[$id]; @{ Key = $id; Base = $id; Name = $t.Name; Desc = $t.Desc; Font = $t.Font; Pack = $null } })
    foreach ($pk in @(Get-ThemePacks | Where-Object { -not $Managed -or (Test-PackSuite $_.Id) })) { $list += @{ Key = "pack:$($pk.Id)"; Base = $pk.Base; Name = $pk.Name; Desc = $pk.Desc; Font = $AppThemes[$pk.Base].Font; Pack = $pk } }
    $list
}

function Build-ThemePanel {
    $p = $ui.SetTheme
    $p.Children.Clear()
    # Copie gérée : le thème choisi ici devient celui de toute la suite (Mindset et ses applis le prennent aussi)
    $current = Get-CurrentThemeKey
    if (-not $script:ThemePick) { $script:ThemePick = $current }
    $choices = @(Get-ThemeChoices)
    $grid = New-Object System.Windows.Controls.Primitives.UniformGrid
    $grid.Columns = 2
    foreach ($ch in $choices) {
        $th = $AppThemes[$ch.Base]
        $sel = $ch.Key -eq $script:ThemePick
        $card = New-Object System.Windows.Controls.Border
        $card.CornerRadius = [System.Windows.CornerRadius]::new(18)
        $card.Padding = New-Thickness 10 10 10 12
        $card.Margin = New-Thickness 0 0 12 12
        $card.Background = Get-Brush $(if ($sel) { '#18FFFFFF' } else { '#0AFFFFFF' })
        $w = if ($sel) { 2 } else { 1 }
        $card.BorderThickness = New-Thickness $w $w $w $w
        # Contour aux couleurs du thème de la carte (pas du thème actuel)
        $card.BorderBrush = if ($sel) { New-RawGradient @($th.P, $th.S) } else { Get-Brush '#1CFFFFFF' }
        if ($sel) { $card.Effect = New-Object System.Windows.Media.Effects.DropShadowEffect -Property @{ Color = [System.Windows.Media.ColorConverter]::ConvertFromString($th.P); BlurRadius = 18; ShadowDepth = 0; Opacity = 0.45 } }
        $card.Cursor = [System.Windows.Input.Cursors]::Hand
        $sp = New-Object System.Windows.Controls.StackPanel
        $pv = New-ThemePreview $ch.Base
        # Pack : son personnage (1re image de l'écran de chargement) posé sur l'aperçu
        if ($ch.Pack -and $ch.Pack.Loader) {
            try {
                $bi = New-Object System.Windows.Media.Imaging.BitmapImage
                $bi.BeginInit(); $bi.UriSource = New-Object Uri $ch.Pack.Loader.File; $bi.DecodePixelHeight = 160; $bi.CacheOption = 'OnLoad'; $bi.EndInit()
                $fw = [int]($bi.PixelWidth / $ch.Pack.Loader.Frames)
                $im = New-Object System.Windows.Controls.Image
                $im.Source = New-Object System.Windows.Media.Imaging.CroppedBitmap $bi, ([System.Windows.Int32Rect]::new(0, 0, $fw, $bi.PixelHeight))
                $im.Height = 70; $im.HorizontalAlignment = 'Right'; $im.VerticalAlignment = 'Bottom'; $im.Margin = New-Thickness 0 0 8 6
                [void]$pv.Children.Add($im)
            } catch {}
        }
        [void]$sp.Children.Add($pv)
        $nm = New-Object System.Windows.Controls.StackPanel
        $nm.Orientation = 'Horizontal'; $nm.Margin = New-Thickness 2 10 0 0
        $title = New-Text $ch.Name 14.5 '#FFFFFF' -Bold
        $title.VerticalAlignment = 'Center'
        if ($ch.Font) { $title.FontFamily = New-Object System.Windows.Media.FontFamily $ch.Font }
        [void]$nm.Children.Add($title)
        foreach ($tag in @($(if ($ch.Pack) { 'Pack' }), $(if ($ch.Key -eq $current) { 'Actuel' })) | Where-Object { $_ }) {
            $badge = New-Object System.Windows.Controls.Border
            $badge.CornerRadius = [System.Windows.CornerRadius]::new(8); $badge.Padding = New-Thickness 8 2 8 2; $badge.Margin = New-Thickness 8 0 0 0
            $badge.VerticalAlignment = 'Center'
            $badge.Background = New-RawBrush ('#33' + $th.P.Substring(1))
            $bt = New-Text $tag 11 '#FFFFFF' -Semi; $bt.VerticalAlignment = 'Center'
            $badge.Child = $bt
            [void]$nm.Children.Add($badge)
        }
        [void]$sp.Children.Add($nm)
        $d = New-Text $ch.Desc 11.5 '#8E88A8'
        $d.Margin = New-Thickness 2 3 0 0
        [void]$sp.Children.Add($d)
        $card.Child = $sp
        $card.Tag = $ch.Key
        $card.Add_MouseLeftButtonUp({ param($s, $e) $script:ThemePick = [string]$s.Tag; Build-ThemePanel })
        [void]$grid.Children.Add($card)
    }
    [void]$p.Children.Add($grid)

    # Bas : appliquer (redémarre Nevermind, le thème s'applique au chargement de la fenêtre)
    $foot = New-Grid @('*', 'Auto')
    $foot.Margin = New-Thickness 0 4 12 8
    $changed = $script:ThemePick -ne $current
    $pickName = @($choices | Where-Object { $_.Key -eq $script:ThemePick })[0].Name
    $msg = if ($changed -and $Managed) { "« $pickName » s'applique à toute la suite : Nevermind et Mindset se relancent (quelques secondes). Tes réglages ne changent pas." }
        elseif ($changed) { "« $pickName » s'applique en relançant Nevermind (quelques secondes). Tes réglages ne changent pas." }
        elseif ($Managed) { $ManagedThemeText }
        else { 'Choisis un thème pour le voir en grand : Nevermind se relance pour l''appliquer.' }
    $mt = New-Text $msg 12 '#A6A1BC'
    $mt.VerticalAlignment = 'Center'
    Add-ToGrid $foot $mt 0
    if ($changed) {
        $b = New-Button 'Appliquer et relancer' 'BtnPrimary'
        $b.Margin = New-Thickness 16 0 0 0
        $b.Add_Click({ Invoke-Safe { Select-AppTheme $script:ThemePick } })
        Add-ToGrid $foot $b 1
    }
    [void]$p.Children.Add($foot)

    # Géré par Mindset : les thèmes personnalisés de la suite s'importent dans Mindset (pour toutes les applis à la fois)
    if ($Managed) {
        $mc = New-SettingAction 'Thèmes personnalisés' 'Un nouveau thème personnalisé s''importe dans Mindset, pour toute la suite.' 'Ouvrir les réglages de Mindset' { Invoke-Safe { Open-MindsetSettings } }
        $mc.Margin = New-Thickness 0 6 12 8
        [void]$p.Children.Add($mc)
        return
    }
    # Packs : juste les deux boutons
    $pk = New-Object System.Windows.Controls.StackPanel
    $pk.Orientation = 'Horizontal'; $pk.Margin = New-Thickness 0 6 12 8
    $imp = New-Button 'Importer un pack'
    $imp.Add_Click({
        Invoke-Safe {
            $dlg = New-Object Microsoft.Win32.OpenFileDialog
            $dlg.Title = 'Choisis le pack de thème (.zip)'; $dlg.Filter = 'Pack de thème (*.zip)|*.zip'
            if (-not $dlg.ShowDialog($Window)) { return }
            $id = Import-ThemePack $dlg.FileName
            $script:ThemePick = "pack:$id"
            Build-ThemePanel
            Set-Status "Pack « $((Get-ThemePack $id).Name) » ajouté : clique sur « Appliquer et relancer »."
        }
    })
    [void]$pk.Children.Add($imp)
    $open = New-Button 'Ouvrir le dossier'
    $open.Margin = New-Thickness 10 0 0 0
    $open.Add_Click({ New-Item -ItemType Directory -Force -Path $PacksDir | Out-Null; Open-Url $PacksDir })
    [void]$pk.Children.Add($open)
    [void]$p.Children.Add($pk)
}

# Thème choisi ici (Paramètres, Thème, dans la fenêtre ou dans l'écran). Géré par Mindset : il devient d'abord le thème
# de la suite (suite.json), sinon il serait remplacé par celui de la suite au prochain lancement.
function Select-AppTheme([string]$Id) {
    if ($Managed) {
        if ($Id -like 'pack:*' -and -not (Test-PackSuite $Id.Substring(5))) {
            Show-Message 'Ce thème personnalisé n''est pas importé dans Mindset. Importe-le dans Mindset (Paramètres, Apparence) pour toute la suite.' 'Warning'
            return
        }
        if (-not (Write-SuiteTheme $Id)) {
            Show-Message 'Le thème de la suite n''a pas pu être enregistré. Choisis-le dans Mindset (Paramètres, Apparence).' 'Warning'
            return
        }
    }
    Set-AppTheme $Id
}

# (Copie gérée par Mindset : appelé aussi quand le thème de la suite change dans Mindset, suite.json surveillé)
function Set-AppTheme([string]$Id) {
    if ($Id -like 'pack:*') { if (-not (Get-ThemePack $Id.Substring(5))) { return } }
    elseif (-not $AppThemes.Contains($Id)) { return }
    Set-Setting 'Theme' $Id
    Write-Log "Thème : $Id"
    if ($env:OPTIGAME_TEST) { Set-Status "Thème « $Id » choisi (copie de test : pas de relance)."; return }
    $script:RelaunchHidden = -not $Window.IsVisible   # caché près de l'horloge : il y retourne après la relance
    $script:Relaunch = Join-Path $AppDir 'OptiGame.ps1'
    $script:AllowClose = $true
    $Window.Close()
}
