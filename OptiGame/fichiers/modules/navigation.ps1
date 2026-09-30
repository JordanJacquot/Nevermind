# OptiGame : accueil « Ordinateur » et navigation entre les pages.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Navigation : accueil « Ordinateur » avec une carte par fonction
# ---------------------------------------------------------------------------
$HubIndex = 8
$NetIndex = 9
$PageNames = @{ 0 = 'Tableau de bord'; 1 = 'Optimisation gaming'; 2 = 'Démarrage'; 3 = 'Connexion'; 4 = 'Nettoyage'; 5 = 'Tests'; 6 = 'Sécurité'; 7 = 'Sauvegarde' }
$HubPages = @(
    @{ Index = 0; Glyph = 0xE80F; Title = 'Tableau de bord'; Desc = 'Santé des composants, score et ce qui peut être amélioré.'; Color = '#22D37A' },
    @{ Index = 1; Glyph = 0xE7FC; Title = 'Optimisation gaming'; Desc = 'Les réglages de Windows qui font gagner des FPS.'; Color = '#B18CFF' },
    @{ Index = 5; Glyph = 0xE9D9; Title = 'Tests'; Desc = 'Vitesse et santé de chaque composant.'; Color = '#4EA8FF' },
    @{ Index = 6; Glyph = 0xE72E; Title = 'Sécurité'; Desc = 'Antivirus et recherche de tout ce qui est suspect.'; Color = '#22D37A' },
    @{ Index = 2; Glyph = 0xE7E8; Title = 'Démarrage'; Desc = 'Les programmes qui se lancent avec Windows.'; Color = '#F5A524' },
    @{ Index = 3; Glyph = 0xE774; Title = 'Connexion'; Desc = 'Ping, stabilité de la connexion et serveur DNS.'; Color = '#4EA8FF' },
    @{ Index = 4; Glyph = 0xE74D; Title = 'Nettoyage'; Desc = 'Libère de la place sur le disque.'; Color = '#FF7AB6' },
    @{ Index = 7; Glyph = 0xE777; Title = 'Sauvegarde'; Desc = 'Tout annuler, rapport du PC et mises à jour.'; Color = '#9AA3B2' }
)

function Show-Page([int]$Index) { $ui.Tabs.SelectedIndex = $Index }

function Update-NavBar {
    $i = $ui.Tabs.SelectedIndex
    $sub = $i -ge 0 -and $i -lt $HubIndex
    $ui.Tabs.Items[$HubIndex].Tag = if ($sub) { 'parent' } else { $null }
    if ($script:NavBar) {
        $script:NavBar.Visibility = if ($sub) { 'Visible' } else { 'Collapsed' }
        if ($sub) { $script:NavCrumb.Text = $PageNames[$i] }
    }
}

function Build-Hub {
    $ui.HubCards.Children.Clear()
    $script:HubStats = @{}
    $n = 0
    foreach ($pg in $HubPages) {
        $card = New-Object System.Windows.Controls.Border
        $card.Background = Get-Brush 'card'
        $card.BorderBrush = Get-Brush 'card-border'
        $card.BorderThickness = New-Thickness 1 1 1 1
        $card.CornerRadius = [System.Windows.CornerRadius]::new(14)
        $card.Padding = New-Thickness 18 16 18 16
        $card.Margin = New-Thickness 0 0 12 12
        $card.Cursor = [System.Windows.Input.Cursors]::Hand
        $move = New-Object System.Windows.Media.TranslateTransform
        $card.RenderTransform = $move
        $sp = New-Object System.Windows.Controls.StackPanel
        $head = New-Grid @('Auto', '*', 'Auto')
        $ic = New-Object System.Windows.Controls.Border
        $ic.Width = 46; $ic.Height = 46
        $ic.CornerRadius = [System.Windows.CornerRadius]::new(12)
        $bg = Get-Brush $pg.Color; $bg.Opacity = 0.15
        $ic.Background = $bg
        $gl = New-Object System.Windows.Controls.TextBlock
        $gl.Text = [string][char]$pg.Glyph
        $gl.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
        $gl.FontSize = 20
        $gl.Foreground = Get-Brush $pg.Color
        $gl.HorizontalAlignment = 'Center'; $gl.VerticalAlignment = 'Center'
        $ic.Child = $gl
        Add-ToGrid $head $ic 0
        $chev = New-Text '›' 24 '#5B6475' -Bold
        $chev.VerticalAlignment = 'Center'
        Add-ToGrid $head $chev 2
        [void]$sp.Children.Add($head)
        $t1 = New-Text $pg.Title 16 '#FFFFFF' -Semi
        $t1.Margin = New-Thickness 0 12 0 0
        [void]$sp.Children.Add($t1)
        $d = New-Text $pg.Desc 12.5 '#9AA3B2'
        $d.Margin = New-Thickness 0 3 0 0
        $d.MinHeight = 34
        [void]$sp.Children.Add($d)
        $stat = New-Text ' ' 13 $pg.Color -Semi
        $stat.Margin = New-Thickness 0 10 0 0
        [void]$sp.Children.Add($stat)
        $card.Child = $sp
        $card.Tag = @{ Index = $pg.Index; Color = $pg.Color; Move = $move; Chev = $chev }
        $card.Add_MouseEnter({
            param($s, $e)
            $s.BorderBrush = Get-Brush $s.Tag.Color
            $s.Background = Get-Brush 'card-hover'
            $s.Tag.Chev.Foreground = Get-Brush $s.Tag.Color
            $s.Effect = New-Glow $s.Tag.Color 28 0.35
            Start-WpfAnim $s.Tag.Move ([System.Windows.Media.TranslateTransform]::YProperty) -3 180
        })
        $card.Add_MouseLeave({
            param($s, $e)
            $s.BorderBrush = Get-Brush 'card-border'
            $s.Background = Get-Brush 'card'
            $s.Tag.Chev.Foreground = Get-Brush '#5B6475'
            $s.Effect = $null
            Start-WpfAnim $s.Tag.Move ([System.Windows.Media.TranslateTransform]::YProperty) 0 180
        })
        $card.Add_MouseLeftButtonUp({ param($s, $e) Show-Page $s.Tag.Index })
        $card.Opacity = 0
        Start-WpfAnim $card ([System.Windows.UIElement]::OpacityProperty) 1 400 (60 * $n)
        [void]$ui.HubCards.Children.Add($card)
        $script:HubStats[$pg.Index] = $stat
        $n++
    }
}

function Update-Hub {
    $a = $script:LastAnalysis
    $info = if ($a) { $a.Info } else { @{} }
    $ui.HubSub.Text = "$env:COMPUTERNAME" + $(if ($info['Windows']) { "   /   $($info['Windows'])" } else { '' })

    # Jauges
    $ui.HubGaugeOpt.Children.Clear(); $ui.HubGaugeSec.Children.Clear()
    if ($a) {
        $col = if ($a.Score -ge 85) { $Colors.ok } elseif ($a.Score -ge 65) { '#9BE15D' } elseif ($a.Score -ge 45) { $Colors.warn } else { $Colors.bad }
        [void]$ui.HubGaugeOpt.Children.Add((New-Gauge 'Optimisation' $a.Score 100 '{0:N0}' 'sur 100' $col 0).El)
    }
    if ($null -ne $script:SecurityScore) {
        $s = $script:SecurityScore
        $col = if ($s -ge 80) { $Colors.ok } elseif ($s -ge 50) { $Colors.warn } else { $Colors.bad }
        [void]$ui.HubGaugeSec.Children.Add((New-Gauge 'Protection' $s 100 '{0:N0}' 'sur 100' $col 150).El)
    }

    # Résumé du PC
    $ui.HubSummary.Children.Clear()
    foreach ($k in 'Processeur', 'Carte graphique', 'Mémoire', 'Disque système', 'Réseau') {
        if ($info[$k]) {
            $g = New-Grid @('130', '*')
            $g.Margin = New-Thickness 0 4 0 4
            Add-ToGrid $g (New-Text $k 12.5 '#9AA3B2') 0
            $v = New-Text ([string]$info[$k]) 12.5 '#E6E8EE' -Semi
            $v.TextTrimming = 'CharacterEllipsis'; $v.TextWrapping = 'NoWrap'
            Add-ToGrid $g $v 1
            [void]$ui.HubSummary.Children.Add($g)
        }
    }

    # Infos en direct sur chaque carte
    $st = $script:HubStats
    if (-not $st) { return }
    if ($a) {
        $st[0].Text = "Score $($a.Score) sur 100"
        $todo = @($a.Active | Where-Object { $_.Id -like 'tweak:*' -and $_.Status -eq 'warn' }).Count
        $st[1].Text = if ($todo) { "$todo réglage$(if ($todo -gt 1) {'s'}) à faire" } else { 'Tout est optimisé' }
    } else { $st[0].Text = 'Analyse en cours...'; $st[1].Text = ' ' }
    $tested = @($ui.TestsPanel.Children | Where-Object { $_.Child -and $_.Child.Children.Count -gt 2 -and $_.Child.Children[2].Children.Count -and -not ($_.Child.Children[2].Children[0] -is [System.Windows.Controls.TextBlock]) }).Count
    $st[5].Text = if ($tested) { "$tested composant$(if ($tested -gt 1) {'s'}) testé$(if ($tested -gt 1) {'s'})" } else { 'Aucun test pour le moment' }
    $st[6].Text = if ($null -ne $script:SecurityScore) { "Protection $($script:SecurityScore) sur 100" } else { 'Clique pour vérifier' }
    $on = @($script:StartupEntries | Where-Object { $_.Item.Enabled }).Count
    $st[2].Text = "$on programme$(if ($on -gt 1) {'s'}) au démarrage"
    $ping = @($script:PingResults | Where-Object { $_.Label -like 'Internet*' } | Select-Object -First 1)
    $st[3].Text = if ($ping.Count) { "Ping $($ping[0].Avg) ms" } else { 'Tester ma connexion' }
    $st[4].Text = 'Clique pour analyser'
    $n = Get-BackupCount
    $st[7].Text = if ($n) { "$n réglage$(if ($n -gt 1) {'s'}) modifié$(if ($n -gt 1) {'s'})" } else { "Version $AppVersion" }
}
