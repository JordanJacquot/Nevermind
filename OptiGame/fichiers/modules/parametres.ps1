# Nevermind : fenêtre « Paramètres » (roue crantée en haut à droite).
# Un onglet par famille : Général, Jeux, Réseau et vie privée, Mises à jour, Aide.
# Les interrupteurs appellent les mêmes fonctions que les pages : un réglage changé ici est à jour partout.
# Chargé par OptiGame.ps1 après les pages et avant recherche.ps1.

$SettingsTabs = @(
    @{ Id = 'general'; Label = 'Général'; Glyph = 0xE80F; Panel = 'SetGeneral'; Sub = 'Raccourci, lancement au démarrage et visite guidée.' },
    @{ Id = 'jeux'; Label = 'Jeux'; Glyph = 0xE7FC; Panel = 'SetGames'; Sub = 'Ce que Nevermind fait pendant que tu joues.' },
    @{ Id = 'reseau'; Label = 'Réseau et vie privée'; Glyph = 0xE72E; Panel = 'SetNetwork'; Sub = 'Surveillance du réseau et ce qui est envoyé sur Internet.' },
    @{ Id = 'maj'; Label = 'Mises à jour'; Glyph = 0xE895; Panel = 'SetUpdates'; Sub = 'Version de Nevermind et nouveautés.' },
    @{ Id = 'aide'; Label = 'Aide'; Glyph = 0xE897; Panel = 'SetHelp'; Sub = 'Un souci, une question ? C''est par ici.' }
)

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
    [void]$p.Children.Add((New-SettingSwitch 'Mesurer mes FPS quand je joue' 'Moyenne, chutes et courbe de chaque partie, gardées dans Mes parties.' (Test-FpsMeasure) {
        param($s, $e) $on = [bool]$s.IsChecked
        Invoke-Safe { Set-FpsMeasure $on }
    }))
    [void]$p.Children.Add((New-SettingAction 'Compteur de FPS à l''écran' 'Affichage, style et position du compteur par dessus le jeu.' 'Ouvrir Overlay' { Hide-Settings; Show-Page $OverlayIndex }))
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
    [void]$p.Children.Add((New-SettingSwitch 'Jaquettes depuis Internet' 'Cherche les images des jeux qui n''en ont pas (seul le nom du jeu est envoyé).' (Test-CoversOnline) {
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
    [void]$p.Children.Add((New-SettingSwitch 'Identifier les serveurs sans nom' 'Page Trafic : cherche à qui appartient une adresse inconnue dans l''annuaire public rdap.org. Seule l''adresse du serveur est envoyée.' ([bool](Get-Setting 'TrafficLookup' $true)) {
        param($s, $e) $on = [bool]$s.IsChecked
        Set-Setting 'TrafficLookup' $on
        if ($script:TrafficLookupSwitch) { $script:TrafficLookupSwitch.IsChecked = $on }
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
        $ic.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
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
    Close-Search -Clear
    Build-SettingsPanels
    Set-SettingsTab $Tab
    $o = $ui.SettingsOverlay
    $o.Opacity = 0
    $o.Visibility = 'Visible'
    Start-WpfAnim $o ([System.Windows.UIElement]::OpacityProperty) 1 180
    $ui.SettingsZoom.ScaleX = 0.96; $ui.SettingsZoom.ScaleY = 0.96
    Start-WpfAnim $ui.SettingsZoom ([System.Windows.Media.ScaleTransform]::ScaleXProperty) 1 220
    Start-WpfAnim $ui.SettingsZoom ([System.Windows.Media.ScaleTransform]::ScaleYProperty) 1 220
    # La roue fait un tour
    if ($script:SettingsGear) { Start-FromTo $script:SettingsGear.RenderTransform ([System.Windows.Media.RotateTransform]::AngleProperty) 0 180 450 }
}

function Hide-Settings {
    if ($ui.SettingsOverlay.Visibility -ne 'Visible') { return }
    $ui.SettingsOverlay.Visibility = 'Collapsed'
}

# Fenêtre étroite (jusqu'à 920 px) : les 5 onglets restent visibles. On cache d'abord le mot « Nevermind »
# à côté du logo, puis la recherche raccourcit (jamais sous 150 px). À taille normale, rien ne change.
function Update-TopBarFit {
    $tabs = $script:TopBar.Tabs; $word = $script:TopBar.Word; $box = $script:TopBar.Search
    if (-not $tabs -or -not $box) { return }
    $tabs.Measure([System.Windows.Size]::new([double]::PositiveInfinity, [double]::PositiveInfinity))
    $tw = $tabs.DesiredSize.Width
    $inner = $Window.ActualWidth - 86   # marges de la barre et petit espace de sécurité
    $fixed = 38 + 18 + 58                       # logo, marge, roue crantée
    $wordW = 122
    $showWord = $inner -ge ($fixed + $wordW + $tw + 220)
    if ($word) { $word.Visibility = if ($showWord) { 'Visible' } else { 'Collapsed' } }
    $left = $fixed + $(if ($showWord) { $wordW } else { 0 })
    $box.Width = [math]::Max(150.0, [math]::Min(250.0, $inner - $left - $tw))
    if ($script:TopBar.Key) { $script:TopBar.Key.Visibility = if ($box.Width -ge 215) { 'Visible' } else { 'Collapsed' } }
}

function Initialize-TopBarFit {
    $script:TopBar = @{
        Tabs = $ui.Tabs.Template.FindName('TopTabsHost', $ui.Tabs); Word = $ui.Tabs.Template.FindName('LogoWordHost', $ui.Tabs)
        Search = $ui.Tabs.Template.FindName('SearchBox', $ui.Tabs); Key = $ui.Tabs.Template.FindName('SearchKey', $ui.Tabs)
    }
    $Window.Add_SizeChanged({ try { Update-TopBarFit } catch {} })
    Update-TopBarFit
}
