# Nevermind : accueil « Ordinateur » et navigation entre les pages.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Navigation : accueil « Ordinateur » rangé en trois familles d'outils
# ---------------------------------------------------------------------------
$HubIndex = 8
$GamesIndex = 9   # bibliothèque de jeux (bibliotheque.ps1)
$NetIndex = 10
$PageNames = @{ 0 = 'Tableau de bord'; 1 = 'Optimisation gaming'; 2 = 'Démarrage'; 3 = 'Connexion'; 4 = 'Nettoyage'; 5 = 'Tests'; 6 = 'Sécurité'; 7 = 'Sauvegarde' }
$HubFamilies = @(
    @{ Title = 'Performances'; Sub = 'Pour gagner des FPS'; Color = '#00E5FF'; Pages = @(
        @{ Index = 1; Glyph = 0xE7FC; Title = 'Optimisation gaming' },
        @{ Index = 2; Glyph = 0xE7E8; Title = 'Démarrage' },
        @{ Index = 4; Glyph = 0xE74D; Title = 'Nettoyage' }) },
    @{ Title = 'Santé du PC'; Sub = 'Pour vérifier le matériel'; Color = '#B04BFF'; Pages = @(
        @{ Index = 0; Glyph = 0xE80F; Title = 'Tableau de bord' },
        @{ Index = 5; Glyph = 0xE9D9; Title = 'Tests' }) },
    @{ Title = 'Protection'; Sub = 'Pour rester tranquille'; Color = '#FF2EB5'; Pages = @(
        @{ Index = 6; Glyph = 0xE72E; Title = 'Sécurité' },
        @{ Index = 3; Glyph = 0xE774; Title = 'Connexion' },
        @{ Index = 7; Glyph = 0xE777; Title = 'Sauvegarde' }) }
)
$HubPages = @($HubFamilies | ForEach-Object { $_.Pages })

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

# Prénom pour le « Salut » : nom complet du compte Windows, sinon le nom d'utilisateur
function Get-FirstName {
    if ($script:FirstName) { return $script:FirstName }
    $n = ''
    try {
        $full = ([adsi]"WinNT://$env:COMPUTERNAME/$env:USERNAME,user").FullName
        if ($full) { $n = ([string]$full).Trim().Split(' ')[0] }
    } catch {}
    if (-not $n) { $n = [string]$env:USERNAME }
    if ($n) { $n = $n.Substring(0, 1).ToUpper() + $n.Substring(1) }
    $script:FirstName = $n
    $n
}

# Anneau néon de l'accueil : piste translucide, arc en dégradé lumineux, note au centre
function New-NeonRing([string]$Label, [double]$Value, [string[]]$Grad, [int]$Delay = 0) {
    $c = 66; $r = 54
    $root = New-Object System.Windows.Controls.StackPanel
    $cv = New-Object System.Windows.Controls.Canvas
    $cv.Width = 2 * $c; $cv.Height = 2 * $c
    $track = New-Object System.Windows.Shapes.Ellipse
    $track.Width = 2 * $r; $track.Height = 2 * $r
    $track.Stroke = Get-Brush '#22FFFFFF'; $track.StrokeThickness = 10
    [System.Windows.Controls.Canvas]::SetLeft($track, $c - $r); [System.Windows.Controls.Canvas]::SetTop($track, $c - $r)
    [void]$cv.Children.Add($track)
    $arc = New-LoaderArc $c $r -90 0.1 (New-LinearBrush $Grad 0 0 1 1) 10
    $arc.Effect = New-Glow $Grad[0] 18 0.7
    [void]$cv.Children.Add($arc)
    # La note seule, au centre exact de l'anneau
    $center = New-Object System.Windows.Controls.Grid
    $center.Width = 2 * $c; $center.Height = 2 * $c
    $num = New-Text '0' 34 '#FFFFFF' -Bold
    $num.HorizontalAlignment = 'Center'; $num.VerticalAlignment = 'Center'
    $num.FontFamily = New-Object System.Windows.Media.FontFamily $MonoFont
    Set-PackFont $num 0.6
    [void]$center.Children.Add($num)
    [void]$cv.Children.Add($center)
    $cv.HorizontalAlignment = 'Center'
    [void]$root.Children.Add($cv)
    $lbl = New-Text $Label 14 '#FFFFFF' -Semi
    $lbl.HorizontalAlignment = 'Center'; $lbl.Margin = New-Thickness 0 10 0 0
    [void]$root.Children.Add($lbl)
    if ($Value -ge 85) { $word = 'Excellent'; $col = $Colors.ok }
    elseif ($Value -ge 65) { $word = 'Bien'; $col = '#9BE15D' }
    elseif ($Value -ge 45) { $word = 'À améliorer'; $col = $Colors.warn }
    else { $word = 'À corriger vite'; $col = $Colors.bad }
    $v = New-Text $word 12 $col -Semi
    $v.HorizontalAlignment = 'Center'; $v.Margin = New-Thickness 0 2 0 0
    [void]$root.Children.Add($v)
    $root.Opacity = 0
    Start-WpfAnim $root ([System.Windows.UIElement]::OpacityProperty) 1 450 $Delay
    $state = @{ Arc = $arc; Num = $num; To = $Value; C = $c; R = $r }
    Start-Anim {
        param($e, $s)
        $val = $s.To * $e
        $s.Arc.Data = Get-ArcGeometry $s.C $s.R -90 ([math]::Max(0.1, [math]::Min(359.9, 3.6 * $val)))
        $s.Num.Text = '{0:N0}' -f $val
    } $state 1200 $Delay
    $root
}

function Build-Hub {
    $ui.HubCards.Children.Clear()
    $script:HubStats = @{}
    if (-not $script:HubWired) {
        $script:HubWired = $true
        $ui.HubOptCard.Add_MouseLeftButtonUp({ Show-Page 0 })
        $ui.HubSecCard.Add_MouseLeftButtonUp({ Show-Page 6 })
        foreach ($cardEl in $ui.HubOptCard, $ui.HubSecCard) {
            $cardEl.Add_MouseEnter({ param($s, $e) $s.Background = Get-Brush 'card-hover' })
            $cardEl.Add_MouseLeave({ param($s, $e) $s.Background = Get-Brush 'card' })
        }
    }
    $n = 0
    foreach ($fam in $HubFamilies) {
        Step-UI
        # Panneau de verre bordé de la couleur de la famille
        $panel = New-Object System.Windows.Controls.Border
        $panel.CornerRadius = [System.Windows.CornerRadius]::new(24)
        $panel.Background = Get-Brush 'card'
        $panel.BorderBrush = New-LinearBrush @(('#77' + $fam.Color.Substring(1)), '#08FFFFFF') 0 0 1 1
        $panel.BorderThickness = New-Thickness 1 1 1 1
        $panel.Padding = New-Thickness 14 18 14 14
        $panel.Margin = New-Thickness 8 0 8 0
        $sp = New-Object System.Windows.Controls.StackPanel
        $head = New-Object System.Windows.Controls.StackPanel
        $head.Orientation = 'Horizontal'; $head.Margin = New-Thickness 8 0 0 0
        $dot = New-Object System.Windows.Shapes.Ellipse
        $dot.Width = 10; $dot.Height = 10; $dot.Fill = Get-Brush $fam.Color
        $dot.Effect = New-Glow $fam.Color 12 0.9
        $dot.VerticalAlignment = 'Center'; $dot.Margin = New-Thickness 0 1 10 0
        [void]$head.Children.Add($dot)
        $ft = New-Text $fam.Title 16 '#FFFFFF' -Bold; Set-PackFont $ft
        [void]$head.Children.Add($ft)
        [void]$sp.Children.Add($head)
        $sub = New-Text $fam.Sub 12 '#8E88A8'
        $sub.Margin = New-Thickness 28 2 0 12
        [void]$sp.Children.Add($sub)
        foreach ($pg in $fam.Pages) {
            Step-UI
            $row = New-Object System.Windows.Controls.Border
            $row.CornerRadius = [System.Windows.CornerRadius]::new(16)
            $row.Background = Get-Brush '#0CFFFFFF'
            $row.BorderBrush = Get-Brush '#00FFFFFF'
            $row.BorderThickness = New-Thickness 1 1 1 1
            $row.Padding = New-Thickness 10 9 12 9
            $row.Margin = New-Thickness 0 0 0 8
            $row.Cursor = [System.Windows.Input.Cursors]::Hand
            $g = New-Grid @('Auto', '*', 'Auto')
            $ic = New-Object System.Windows.Controls.Border
            $ic.Width = 36; $ic.Height = 36
            $ic.CornerRadius = [System.Windows.CornerRadius]::new(12)
            $ic.Background = New-AlphaBrush $fam.Color 48
            $gl = New-Object System.Windows.Controls.TextBlock
            $gl.Text = [string][char]$pg.Glyph
            $gl.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
            $gl.FontSize = 16; $gl.Foreground = Get-Brush $fam.Color
            $gl.HorizontalAlignment = 'Center'; $gl.VerticalAlignment = 'Center'
            $ic.Child = $gl
            Add-ToGrid $g $ic 0
            $txt = New-Object System.Windows.Controls.StackPanel
            $txt.VerticalAlignment = 'Center'; $txt.Margin = New-Thickness 12 0 8 0
            $rt = New-Text $pg.Title 14.5 '#FFFFFF' -Semi; Set-PackFont $rt
            [void]$txt.Children.Add($rt)
            $stat = New-Text ' ' 12 $fam.Color -Semi
            $stat.TextTrimming = 'CharacterEllipsis'; $stat.TextWrapping = 'NoWrap'
            [void]$txt.Children.Add($stat)
            Add-ToGrid $g $txt 1
            $chev = New-Object System.Windows.Controls.TextBlock
            $chev.Text = [string][char]0xE76C
            $chev.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
            $chev.FontSize = 12; $chev.Foreground = Get-Brush '#655E7E'; $chev.VerticalAlignment = 'Center'
            Add-ToGrid $g $chev 2
            $row.Child = $g
            $move = New-Object System.Windows.Media.TranslateTransform
            $row.RenderTransform = $move
            $row.Tag = @{ Index = $pg.Index; Color = $fam.Color; Move = $move; Chev = $chev }
            $row.Add_MouseEnter({
                param($s, $e)
                $s.Background = Get-Brush '#18FFFFFF'
                $s.BorderBrush = New-AlphaBrush $s.Tag.Color 110
                $s.Tag.Chev.Foreground = Get-Brush $s.Tag.Color
                Start-WpfAnim $s.Tag.Move ([System.Windows.Media.TranslateTransform]::XProperty) 3 160
            })
            $row.Add_MouseLeave({
                param($s, $e)
                $s.Background = Get-Brush '#0CFFFFFF'
                $s.BorderBrush = Get-Brush '#00FFFFFF'
                $s.Tag.Chev.Foreground = Get-Brush '#655E7E'
                Start-WpfAnim $s.Tag.Move ([System.Windows.Media.TranslateTransform]::XProperty) 0 160
            })
            $row.Add_MouseLeftButtonUp({ param($s, $e) Show-Page $s.Tag.Index })
            [void]$sp.Children.Add($row)
            $script:HubStats[$pg.Index] = $stat
        }
        $panel.Child = $sp
        $panel.Opacity = 0
        Start-WpfAnim $panel ([System.Windows.UIElement]::OpacityProperty) 1 450 (90 * $n)
        [void]$ui.HubCards.Children.Add($panel)
        $n++
    }
}

# Carte mise en avant de l'accueil.
# « À faire » seulement quand ça vaut le coup : un vrai problème, ou une correction que Nevermind fait en un clic.
# Sinon « Ta dernière partie ». Ce qui se fait à la main (un pilote à installer...) reste en petite ligne dessous.
function Get-HubFindings($A) {
    $all = @(if ($A) { $A.Active | Where-Object { $_.Status -ne 'ok' -and $_.Gain -gt 0 } })
    $sort = @{ Expression = { $_.Gain }; Descending = $true }
    $important = @($all | Where-Object { $_.Status -eq 'bad' -or ($_.Fix -and $_.Fix.Auto) } |
        Sort-Object @{ Expression = { $_.Status -ne 'bad' } }, $sort)
    $manual = @($all | Where-Object { $important -notcontains $_ } | Sort-Object $sort)
    @{ Important = $important; Manual = $manual }
}

# Le dernier jeu lancé (encore installé), avec sa dernière partie mesurée
function Get-LastPlayedGame {
    if ($null -eq $script:Games) { return $null }
    $games = @(Get-LibraryGames)
    if (-not $games.Count) { return $null }
    $log = Get-PlayLog
    $best = $null; $bestDate = [datetime]::MinValue
    foreach ($g in $games) {
        $e = $log[$g.Name]
        if (-not $e -or -not $e.Last) { continue }
        try { $d = [datetime]$e.Last } catch { continue }
        if ($d -gt $bestDate) { $best = $g; $bestDate = $d }
    }
    if (-not $best) { return $null }
    $ss = @(Get-FpsSessions | Where-Object { (Get-SessionName $_) -eq $best.Name } | Sort-Object { [datetime]$_.Date })
    @{ Game = $best; Log = $log[$best.Name]; Last = $(if ($ss.Count) { $ss[-1] }); Prev = $(if ($ss.Count -gt 1) { $ss[-2] }) }
}

function Open-LibraryGame([string]$Name) {
    Show-Page $GamesIndex
    $ui.LibSearch.Text = ''; $script:LibFilter = 'Tous'
    Update-LibraryView
    Set-LibrarySelection $Name
}

function Set-HubCardLook([string]$Look, $Art) {
    $ui.HubTodoArt.Background = $Art
    $ui.HubTodoShade.Visibility = if ($Art) { 'Visible' } else { 'Collapsed' }
    $ui.HubTodo.Background = if ($Art) { Get-Brush '#0B0820' } else { New-LinearBrush @('#2200E5FF', '#22FF2EB5') 0 0 1 1 }
}

function New-HubTag([string]$Text, [string]$Color) {
    $t = New-Text $Text 11.5 $Color -Bold
    $t.FontFamily = New-Object System.Windows.Media.FontFamily $MonoFont
    Set-PackFont $t 0.85
    $t
}

# Petite ligne « Aussi : ... » pour ce qui se fait à la main
function New-HubManualLine($Manual) {
    $f = $Manual[0]
    $row = New-Object System.Windows.Controls.Border
    $row.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $row.Background = Get-Brush '#14FFFFFF'
    $row.Padding = New-Thickness 12 7 12 7
    $row.Margin = New-Thickness 0 16 0 0
    $row.HorizontalAlignment = 'Left'
    $row.Cursor = [System.Windows.Input.Cursors]::Hand
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Orientation = 'Horizontal'
    $ic = New-Text ([string][char]0xE946) 12 $Colors.warn
    $ic.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'; $ic.FontSize = 12
    $ic.VerticalAlignment = 'Center'; $ic.Margin = New-Thickness 0 1 8 0
    [void]$sp.Children.Add($ic)
    $label = "Quand tu as le temps : $($f.Titre)"
    if ($Manual.Count -gt 1) { $label += "  (+$($Manual.Count - 1))" }
    $t = New-Text $label 12 '#D3CDE3' -Semi
    $t.VerticalAlignment = 'Center'; $t.TextTrimming = 'CharacterEllipsis'; $t.MaxWidth = 380
    [void]$sp.Children.Add($t)
    $ch = New-Text ([string][char]0xE76C) 10 '#8E88A8'
    $ch.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'; $ch.FontSize = 10
    $ch.VerticalAlignment = 'Center'; $ch.Margin = New-Thickness 10 1 0 0
    [void]$sp.Children.Add($ch)
    $row.Child = $sp
    $row.Tag = $Manual
    $row.Add_MouseEnter({ param($s, $e) $s.Background = Get-Brush '#22FFFFFF' })
    $row.Add_MouseLeave({ param($s, $e) $s.Background = Get-Brush '#14FFFFFF' })
    $row.Add_MouseLeftButtonUp({ param($s, $e) if (@($s.Tag).Count -gt 1) { Show-Page 0 } else { Invoke-Safe { Open-Sheet @($s.Tag[0]) } } })
    $row
}

function Update-HubTodo($A) {
    $p = $ui.HubTodoPanel
    $p.Children.Clear()
    $fd = Get-HubFindings $A
    $btns = New-Object System.Windows.Controls.StackPanel
    $btns.Orientation = 'Horizontal'; $btns.Margin = New-Thickness 0 16 0 0
    $title = $null; $detail = $null; $tag = $null; $art = $null; $extra = $null
    $last = if (-not $fd.Important.Count) { Get-LastPlayedGame }

    if (-not $A) {
        $tag = New-HubTag 'UN INSTANT' '#00E5FF'
        $title = 'Analyse de ton PC en cours...'
        $detail = 'Encore quelques secondes et je te dis s''il y a quelque chose à faire.'
    } elseif ($fd.Important.Count) {
        $top = $fd.Important[0]
        $tag = New-HubTag 'À FAIRE' $(if ($top.Status -eq 'bad') { $Colors.bad } else { '#00E5FF' })
        $title = [string]$top.Titre; $detail = [string]$top.Detail
        if ($top.Fix -and $top.Fix.Auto) {
            $b = New-Button 'Corriger maintenant' 'BtnPrimary'
            $b.Tag = $top
            $b.Add_Click({ param($s, $e) Invoke-Safe { Open-Sheet @($s.Tag) } })
        } else {
            $b = New-Button 'Voir comment faire' 'BtnPrimary'
            $b.Tag = $top
            $b.Add_Click({ param($s, $e) Invoke-Safe { Open-Sheet @($s.Tag) } })
        }
        [void]$btns.Children.Add($b)
        $more = $fd.Important.Count - 1
        if ($more -gt 0) {
            $b2 = New-Button "Voir les $more autres"
            $b2.Margin = New-Thickness 10 0 0 0
            $b2.Add_Click({ Show-Page 0 })
            [void]$btns.Children.Add($b2)
        }
    } elseif ($null -ne $script:SecurityScore -and $script:SecurityScore -lt 50) {
        $tag = New-HubTag 'À FAIRE' $Colors.bad
        $title = 'Vérifie ta protection'
        $detail = 'Des points de sécurité importants méritent un coup d''oeil.'
        $b = New-Button 'Ouvrir Sécurité' 'BtnPrimary'
        $b.Add_Click({ Show-Page 6 })
        [void]$btns.Children.Add($b)
    } elseif ($last) {
        # Ta dernière partie : image du jeu en fond, FPS de la dernière partie mesurée
        $g = $last.Game
        $art = Get-ImageBrush (Get-HeroFile $g) 900
        if (-not $art) { $art = Get-ImageBrush (Get-CoverFile $g) 600 }
        if ($art) { $art = $art.Clone(); $art.AlignmentX = 'Right'; $art.Stretch = 'UniformToFill' }
        $tag = New-HubTag 'TA DERNIÈRE PARTIE' '#00E5FF'
        $title = $g.Name
        $detail = "Joué $(Format-LastPlayed $last.Log.Last), $(Format-PlayTime $last.Log.Seconds) au total"
        if ($last.Last) {
            $s = $last.Last
            $fps = New-Object System.Windows.Controls.StackPanel
            $fps.Orientation = 'Horizontal'; $fps.Margin = New-Thickness 0 12 0 0
            $n = New-Text ('{0:N0}' -f $s.Avg) 26 '#FFFFFF' -Bold
            $n.FontFamily = New-Object System.Windows.Media.FontFamily $MonoFont
            [void]$fps.Children.Add($n)
            $u = New-Text ('FPS moyens   /   1 % bas {0:N0}' -f $s.Low1) 12.5 '#B9B3CC'
            $u.VerticalAlignment = 'Bottom'; $u.Margin = New-Thickness 8 0 0 5
            [void]$fps.Children.Add($u)
            if ($last.Prev -and $last.Prev.Avg -gt 0) {
                $pct = 100 * ($s.Avg - $last.Prev.Avg) / $last.Prev.Avg
                if ([math]::Abs($pct) -ge 3) {
                    $d = New-Text ('{0}{1:N0} %' -f $(if ($pct -gt 0) { '+' } else { '' }), $pct) 12.5 $(if ($pct -gt 0) { $Colors.ok } else { $Colors.warn }) -Bold
                    $d.VerticalAlignment = 'Bottom'; $d.Margin = New-Thickness 12 0 0 5
                    $d.ToolTip = 'Par rapport à la partie d''avant'
                    [void]$fps.Children.Add($d)
                }
            }
            $extra = $fps
        }
        $b = New-Button 'Rejouer' 'BtnPrimary'
        $b.Tag = $g
        $b.Add_Click({ param($s, $e) Invoke-Safe { Start-LibraryGame $s.Tag } })
        [void]$btns.Children.Add($b)
        $b2 = New-Button 'Optimiser ce jeu'
        $b2.Margin = New-Thickness 10 0 0 0
        $b2.Tag = $g.Name
        $b2.Add_Click({ param($s, $e) Invoke-Safe { Open-LibraryGame $s.Tag } })
        [void]$btns.Children.Add($b2)
    } else {
        $tag = New-HubTag 'PRÊT À JOUER' $Colors.ok
        $title = 'Tout est en ordre'
        $detail = 'Rien d''important à corriger. Lance un jeu depuis Jeux : ta dernière partie s''affichera ici.'
        $b = New-Button 'Ouvrir mes jeux' 'BtnPrimary'
        $b.Add_Click({ Show-Page $GamesIndex })
        [void]$btns.Children.Add($b)
    }

    Set-HubCardLook $(if ($art) { 'game' } else { 'todo' }) $art
    [void]$p.Children.Add($tag)
    $t = New-Text $title 22 '#FFFFFF' -Bold
    Set-PackFont $t
    $t.Margin = New-Thickness 0 8 0 0; $t.TextWrapping = 'Wrap'; $t.MaxWidth = 460; $t.HorizontalAlignment = 'Left'
    [void]$p.Children.Add($t)
    if ($detail) {
        $d = New-Text $detail 13 '#B9B3CC'
        $d.Margin = New-Thickness 0 6 0 0; $d.TextWrapping = 'Wrap'; $d.MaxWidth = 460; $d.HorizontalAlignment = 'Left'
        $d.MaxHeight = 56; $d.TextTrimming = 'CharacterEllipsis'
        [void]$p.Children.Add($d)
    }
    if ($extra) { [void]$p.Children.Add($extra) }
    if ($btns.Children.Count) { [void]$p.Children.Add($btns) }
    if ($A -and $fd.Manual.Count) { [void]$p.Children.Add((New-HubManualLine $fd.Manual)) }
}

function Set-HubStat([int]$Index, [string]$Text, [string]$Color) {
    $t = $script:HubStats[$Index]
    if (-not $t) { return }
    $t.Text = $Text
    if ($Color) { $t.Foreground = Get-Brush $Color }
}

function Update-Hub {
    $a = $script:LastAnalysis
    $info = if ($a) { $a.Info } else { @{} }
    $fn = Get-FirstName
    $ui.HubHello.Text = (Get-ThemeText 'Hello') -f $(if ($ThemeId -eq 'terminal') { $fn.ToLower() } else { $fn })

    # Phrase de résumé sous le bonjour
    $fd = Get-HubFindings $a
    $todo = $fd.Important.Count
    $ui.HubSub.Text = if (-not $a) { 'Je regarde ton PC, ça prend quelques secondes.' }
        elseif ($todo -eq 1) { 'Ton PC tourne bien. Il reste une chose à régler pour être au top.' }
        elseif ($todo -gt 1) { "Ton PC tourne bien. Il reste $todo choses à régler pour être au top." }
        elseif ($fd.Manual.Count) { 'Ton PC est prêt pour jouer. Une petite chose à faire quand tu auras le temps.' }
        else { 'Ton PC est au top pour jouer. Rien à corriger.' }

    # Anneaux néon
    $ui.HubGaugeOpt.Children.Clear(); $ui.HubGaugeSec.Children.Clear()
    if ($a) {
        [void]$ui.HubGaugeOpt.Children.Add((New-NeonRing 'Optimisation' $a.Score @('#00E5FF', '#7A7BFF') 0))
    } else {
        [void]$ui.HubGaugeOpt.Children.Add((New-Text 'Optimisation...' 13 '#8E88A8'))
    }
    if ($null -ne $script:SecurityScore) {
        [void]$ui.HubGaugeSec.Children.Add((New-NeonRing 'Protection' $script:SecurityScore @('#FF2EB5', '#B04BFF') 150))
    } else {
        [void]$ui.HubGaugeSec.Children.Add((New-Text 'Protection...' 13 '#8E88A8'))
    }
    Update-HubTodo $a

    # Le PC en une ligne, discrète, sous les familles
    $specs = @(foreach ($k in 'Processeur', 'Carte graphique', 'Mémoire') { if ($info[$k]) { [string]$info[$k] } })
    $ui.HubSpecs.Text = (@("$env:COMPUTERNAME") + $specs) -join '   /   '

    # État de chaque outil, dans la couleur de sa famille (orange quand il y a quelque chose à faire)
    $st = $script:HubStats
    if (-not $st) { return }
    if ($a) {
        Set-HubStat 0 "Score $($a.Score) sur 100"
        $tw = @($a.Active | Where-Object { $_.Id -like 'tweak:*' -and $_.Status -eq 'warn' }).Count
        if ($tw) { Set-HubStat 1 "$tw réglage$(if ($tw -gt 1) {'s'}) à faire" $Colors.warn } else { Set-HubStat 1 'Tout est optimisé' '#00E5FF' }
    } else { Set-HubStat 0 'Analyse en cours...'; Set-HubStat 1 'Analyse en cours...' }
    $tested = @($ui.TestsPanel.Children | Where-Object { $_.Child -and $_.Child.Children.Count -gt 2 -and $_.Child.Children[2].Children.Count -and -not ($_.Child.Children[2].Children[0] -is [System.Windows.Controls.TextBlock]) }).Count
    Set-HubStat 5 $(if ($tested) { "$tested composant$(if ($tested -gt 1) {'s'}) testé$(if ($tested -gt 1) {'s'})" } else { 'Aucun test pour le moment' })
    if ($null -ne $script:SecurityScore) {
        Set-HubStat 6 "Protection $($script:SecurityScore) sur 100" $(if ($script:SecurityScore -lt 50) { $Colors.bad } elseif ($script:SecurityScore -lt 80) { $Colors.warn } else { '#FF2EB5' })
    } else { Set-HubStat 6 'Clique pour vérifier' }
    $on = @($script:StartupEntries | Where-Object { $_.Item.Enabled }).Count
    Set-HubStat 2 "$on programme$(if ($on -gt 1) {'s'}) au démarrage"
    $ping = @($script:PingResults | Where-Object { $_.Label -like 'Internet*' } | Select-Object -First 1)
    Set-HubStat 3 $(if ($ping.Count) { "Ping $($ping[0].Avg) ms" } else { 'Tester ma connexion' })
    Set-HubStat 4 'Libère de la place'
    $nb = Get-BackupCount
    Set-HubStat 7 $(if ($nb) { "$nb réglage$(if ($nb -gt 1) {'s'}) modifié$(if ($nb -gt 1) {'s'})" } else { 'Rien de modifié' })
}
