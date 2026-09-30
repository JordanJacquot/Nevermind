# OptiGame : animations, jauges, courbes et petits composants visuels.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Animations
# ---------------------------------------------------------------------------
$script:Anims = New-Object System.Collections.ArrayList
$script:AnimTimer = New-Object System.Windows.Threading.DispatcherTimer
$script:AnimTimer.Interval = [TimeSpan]::FromMilliseconds(20)
$script:AnimTimer.Add_Tick({
    $now = [DateTime]::Now
    foreach ($a in @($script:Anims)) {
        $el = ($now - $a.Start).TotalMilliseconds - $a.Delay
        if ($el -lt 0) { continue }
        $p = [math]::Min(1.0, $el / $a.Ms)
        $ease = 1 - [math]::Pow(1 - $p, 3)
        try { & $a.Step $ease $a.State } catch {}
        if ($p -ge 1) { $script:Anims.Remove($a) }
    }
    if (-not $script:Anims.Count) { $script:AnimTimer.Stop() }
})

# Anime une valeur de 0 à 1 (départ rapide, fin douce) en appelant $Step à chaque image.
function Start-Anim([scriptblock]$Step, $State, [int]$Ms = 1100, [int]$Delay = 0) {
    [void]$script:Anims.Add(@{ Step = $Step; State = $State; Ms = $Ms; Delay = $Delay; Start = [DateTime]::Now })
    if (-not $script:AnimTimer.IsEnabled) { $script:AnimTimer.Start() }
}

function Start-WpfAnim($Element, $Property, [double]$To, [int]$Ms = 900, [int]$Delay = 0) {
    $a = New-Object System.Windows.Media.Animation.DoubleAnimation
    $a.To = $To
    $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds($Ms))
    $a.BeginTime = [TimeSpan]::FromMilliseconds($Delay)
    $ease = New-Object System.Windows.Media.Animation.CubicEase
    $ease.EasingMode = 'EaseOut'
    $a.EasingFunction = $ease
    $Element.BeginAnimation($Property, $a)
}

# Animation d'une valeur de $From à $To (repart toujours de $From, même si une animation précédente tient la valeur)
function Start-FromTo($Element, $Property, [double]$From, [double]$To, [int]$Ms = 300) {
    $a = New-Object System.Windows.Media.Animation.DoubleAnimation
    $a.From = $From; $a.To = $To
    $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds($Ms))
    $ease = New-Object System.Windows.Media.Animation.CubicEase
    $ease.EasingMode = 'EaseOut'
    $a.EasingFunction = $ease
    $Element.BeginAnimation($Property, $a)
}

# Changement de page : fondu et léger glissement vers le haut
function Start-PageTransition {
    $h = $ui.Tabs.Template.FindName('PageHost', $ui.Tabs)
    if (-not $h) { return }
    Start-FromTo $h ([System.Windows.UIElement]::OpacityProperty) 0 1 260
    Start-FromTo $h.RenderTransform ([System.Windows.Media.TranslateTransform]::YProperty) 16 0 340
}

# Fenêtre ouverte par dessus l'app : le fond se floute
function Update-BackdropBlur {
    $on = $ui.TestOverlay.IsVisible -or $ui.Overlay.IsVisible -or $ui.NetMapOverlay.IsVisible
    if ($on -and -not $ui.Tabs.Effect) {
        $fx = New-Object System.Windows.Media.Effects.BlurEffect
        $fx.Radius = 0
        $ui.Tabs.Effect = $fx
        Start-FromTo $fx ([System.Windows.Media.Effects.BlurEffect]::RadiusProperty) 0 7 220
    } elseif (-not $on) { $ui.Tabs.Effect = $null }
}

function Start-Pulse($Element) {
    if (-not $Element.CacheMode) { $Element.CacheMode = New-Object System.Windows.Media.BitmapCache }
    $a = New-Object System.Windows.Media.Animation.DoubleAnimation
    $a.From = 1; $a.To = 0.25
    $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds(700))
    $a.AutoReverse = $true
    $a.RepeatBehavior = [System.Windows.Media.Animation.RepeatBehavior]::Forever
    $Element.BeginAnimation([System.Windows.UIElement]::OpacityProperty, $a)
}

function Stop-Pulse($Element) {
    $Element.BeginAnimation([System.Windows.UIElement]::OpacityProperty, $null)
    $Element.Opacity = 1
}

function New-Glow([string]$Hex, [double]$Blur = 16, [double]$Opacity = 0.55) {
    $fx = New-Object System.Windows.Media.Effects.DropShadowEffect
    $fx.Color = [System.Windows.Media.ColorConverter]::ConvertFromString($Hex)
    $fx.BlurRadius = $Blur; $fx.ShadowDepth = 0; $fx.Opacity = $Opacity
    $fx
}

# ---------------------------------------------------------------------------
# Jauges circulaires, courbes en direct, barres de comparaison
# ---------------------------------------------------------------------------
function Get-ArcGeometry([double]$C, [double]$R, [double]$Start, [double]$Sweep) {
    if ($Sweep -le 0.05) { return $null }
    $a1 = $Start * [math]::PI / 180
    $a2 = ($Start + $Sweep) * [math]::PI / 180
    $p1 = [System.Windows.Point]::new($C + $R * [math]::Cos($a1), $C + $R * [math]::Sin($a1))
    $p2 = [System.Windows.Point]::new($C + $R * [math]::Cos($a2), $C + $R * [math]::Sin($a2))
    $seg = [System.Windows.Media.ArcSegment]::new($p2, [System.Windows.Size]::new($R, $R), 0.0, ($Sweep -gt 180), [System.Windows.Media.SweepDirection]::Clockwise, $true)
    $fig = New-Object System.Windows.Media.PathFigure
    $fig.StartPoint = $p1
    [void]$fig.Segments.Add($seg)
    $geo = New-Object System.Windows.Media.PathGeometry
    [void]$geo.Figures.Add($fig)
    $geo
}

function New-Gauge([string]$Label, [double]$Value, [double]$Max, [string]$Fmt, [string]$Unit, [string]$Color, [int]$Delay = 0) {
    $c = 72; $r = 60
    $root = New-Object System.Windows.Controls.StackPanel
    $root.Width = 150
    $root.Margin = New-Thickness 6 0 6 10
    $g = New-Object System.Windows.Controls.Grid
    $g.Width = 144; $g.Height = 144
    foreach ($spec in @(@{ Hex = '#232937'; Sweep = 270 }, @{ Hex = $Color; Sweep = 0 })) {
        $path = New-Object System.Windows.Shapes.Path
        $path.Stroke = Get-Brush $spec.Hex
        $path.StrokeThickness = 11
        $path.StrokeStartLineCap = 'Round'; $path.StrokeEndLineCap = 'Round'
        $path.Data = Get-ArcGeometry $c $r 135 $spec.Sweep
        [void]$g.Children.Add($path)
        $arc = $path
    }
    $arc.Effect = New-Glow $Color 18 0.6
    $center = New-Object System.Windows.Controls.StackPanel
    $center.VerticalAlignment = 'Center'; $center.HorizontalAlignment = 'Center'
    $num = New-Text '0' 26 '#FFFFFF' -Bold
    $num.HorizontalAlignment = 'Center'; $num.TextWrapping = 'NoWrap'
    $u = New-Text $Unit 11.5 '#9AA3B2'
    $u.HorizontalAlignment = 'Center'
    [void]$center.Children.Add($num)
    [void]$center.Children.Add($u)
    [void]$g.Children.Add($center)
    [void]$root.Children.Add($g)
    $lbl = New-Text $Label 13 '#C9CED8' -Semi
    $lbl.HorizontalAlignment = 'Center'; $lbl.TextAlignment = 'Center'
    $lbl.Margin = New-Thickness 0 -8 0 0
    [void]$root.Children.Add($lbl)
    $state = @{ Arc = $arc; Num = $num; From = 0.0; To = $Value; Cur = 0.0; Max = [math]::Max(1e-6, $Max); Fmt = $Fmt; C = $c; R = $r }
    Start-Anim { param($e, $s) $v = $s.From + ($s.To - $s.From) * $e; $s.Cur = $v; $f = [math]::Min(1.0, [math]::Max(0.0, $v / $s.Max)); $s.Arc.Data = Get-ArcGeometry $s.C $s.R 135 (270 * $f); $s.Num.Text = $s.Fmt -f $v } $state 1300 $Delay
    @{ El = $root; State = $state }
}

# Fait glisser une jauge vers une nouvelle valeur (mode « en direct »).
function Set-GaugeLive($Gauge, [double]$Value) {
    $s = $Gauge.State
    $s.From = $s.Cur; $s.To = $Value
    Start-Anim { param($e, $st) $v = $st.From + ($st.To - $st.From) * $e; $st.Cur = $v; $f = [math]::Min(1.0, [math]::Max(0.0, $v / $st.Max)); $st.Arc.Data = Get-ArcGeometry $st.C $st.R 135 (270 * $f); $st.Num.Text = $st.Fmt -f $v } $s 700
}

function New-GaugeRow([array]$Gauges) {
    $wp = New-Object System.Windows.Controls.WrapPanel
    $wp.HorizontalAlignment = 'Center'
    $wp.Margin = New-Thickness 0 6 0 4
    foreach ($g in $Gauges) { [void]$wp.Children.Add($g.El) }
    $wp
}

function New-LiveChart([string]$Color, [string]$Unit, [string]$Fmt = '{0:N0}') {
    $w = 660; $h = 150
    $cv = New-Object System.Windows.Controls.Canvas
    $cv.Width = $w; $cv.Height = $h; $cv.ClipToBounds = $true
    foreach ($y in 0.25, 0.5, 0.75) {
        $ln = New-Object System.Windows.Shapes.Line
        $ln.X1 = 0; $ln.X2 = $w; $ln.Y1 = $h * $y; $ln.Y2 = $h * $y
        $ln.Stroke = Get-Brush '#1C212B'; $ln.StrokeThickness = 1
        [void]$cv.Children.Add($ln)
    }
    $col = [System.Windows.Media.ColorConverter]::ConvertFromString($Color)
    $grad = New-Object System.Windows.Media.LinearGradientBrush
    $grad.StartPoint = [System.Windows.Point]::new(0, 0); $grad.EndPoint = [System.Windows.Point]::new(0, 1)
    $top = [System.Windows.Media.Color]::FromArgb(110, $col.R, $col.G, $col.B)
    $bottom = [System.Windows.Media.Color]::FromArgb(0, $col.R, $col.G, $col.B)
    [void]$grad.GradientStops.Add([System.Windows.Media.GradientStop]::new($top, 0))
    [void]$grad.GradientStops.Add([System.Windows.Media.GradientStop]::new($bottom, 1))
    $fill = New-Object System.Windows.Shapes.Polygon
    $fill.Fill = $grad
    [void]$cv.Children.Add($fill)
    $ref = New-Object System.Windows.Shapes.Line
    $ref.X1 = 0; $ref.X2 = $w; $ref.Stroke = Get-Brush '#F5A524'; $ref.StrokeThickness = 1.2
    $ref.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(4, 4))
    $ref.Visibility = 'Collapsed'
    [void]$cv.Children.Add($ref)
    $refText = New-Text '' 11 '#F5A524'
    $refText.Visibility = 'Collapsed'
    [void]$cv.Children.Add($refText)
    $line = New-Object System.Windows.Shapes.Polyline
    $line.Stroke = Get-Brush $Color; $line.StrokeThickness = 2.5
    $line.StrokeLineJoin = 'Round'
    $line.Effect = New-Glow $Color 10 0.7
    [void]$cv.Children.Add($line)
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 11; $dot.Height = 11; $dot.Fill = Get-Brush '#FFFFFF'
    $dot.Effect = New-Glow $Color 14 0.9
    $dot.Visibility = 'Hidden'
    [void]$cv.Children.Add($dot)
    $maxText = New-Text '' 11 '#5B6475'
    [System.Windows.Controls.Canvas]::SetLeft($maxText, 4); [System.Windows.Controls.Canvas]::SetTop($maxText, 2)
    [void]$cv.Children.Add($maxText)
    $border = New-Object System.Windows.Controls.Border
    $border.Background = Get-Brush '#10131A'
    $border.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $border.Padding = New-Thickness 12 10 12 10
    $border.Margin = New-Thickness 0 10 0 6
    $border.Child = $cv
    @{ El = $border; Line = $line; Fill = $fill; Dot = $dot; MaxText = $maxText; Ref = $ref; RefText = $refText; RefValue = $null
       Values = New-Object System.Collections.ArrayList; W = $w; H = $h; Unit = $Unit; Fmt = $Fmt }
}

function Add-ChartPoint($Chart, [double]$Value) {
    [void]$Chart.Values.Add($Value)
    if ($Chart.Values.Count -gt 160) { $Chart.Values.RemoveAt(0) }
    Update-Chart $Chart
}

function Update-Chart($Chart) {
    $vals = $Chart.Values
    $n = $vals.Count
    if ($n -lt 2) { return }
    $max = [double]($vals | Measure-Object -Maximum).Maximum
    if ($Chart.RefValue) { $max = [math]::Max($max, $Chart.RefValue) }
    if ($max -le 0) { $max = 1 }
    $max *= 1.15
    $w = $Chart.W; $h = $Chart.H
    $step = $w / 159
    $pts = New-Object System.Windows.Media.PointCollection
    for ($i = 0; $i -lt $n; $i++) { [void]$pts.Add([System.Windows.Point]::new($i * $step, $h - [double]$vals[$i] / $max * ($h - 8))) }
    $Chart.Line.Points = $pts
    $fp = New-Object System.Windows.Media.PointCollection
    foreach ($pt in $pts) { [void]$fp.Add($pt) }
    [void]$fp.Add([System.Windows.Point]::new(($n - 1) * $step, $h))
    [void]$fp.Add([System.Windows.Point]::new(0, $h))
    $Chart.Fill.Points = $fp
    $last = $pts[$n - 1]
    [System.Windows.Controls.Canvas]::SetLeft($Chart.Dot, $last.X - 5.5)
    [System.Windows.Controls.Canvas]::SetTop($Chart.Dot, $last.Y - 5.5)
    $Chart.Dot.Visibility = 'Visible'
    $Chart.MaxText.Text = "max $($Chart.Fmt -f ($max / 1.15)) $($Chart.Unit)"
    if ($Chart.RefValue) {
        $y = $h - $Chart.RefValue / $max * ($h - 8)
        $Chart.Ref.Y1 = $y; $Chart.Ref.Y2 = $y; $Chart.Ref.Visibility = 'Visible'
        [System.Windows.Controls.Canvas]::SetLeft($Chart.RefText, $w - 150)
        [System.Windows.Controls.Canvas]::SetTop($Chart.RefText, $y - 16)
        $Chart.RefText.Visibility = 'Visible'
    }
}

# Barres horizontales qui se remplissent: ton composant comparé à des références.
function New-CompareBars([array]$Rows, [string]$Unit) {
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Margin = New-Thickness 0 8 0 0
    $max = [double](($Rows | ForEach-Object { $_.Value }) | Measure-Object -Maximum).Maximum
    $barMax = 430.0
    $i = 0
    foreach ($row in $Rows) {
        $g = New-Grid @('150', '440', '*')
        $g.Margin = New-Thickness 0 5 0 5
        $lbl = New-Text $row.Label 13 $(if ($row.Mine) { '#FFFFFF' } else { '#9AA3B2' })
        if ($row.Mine) { $lbl.FontWeight = [System.Windows.FontWeights]::SemiBold }
        $lbl.VerticalAlignment = 'Center'
        Add-ToGrid $g $lbl 0
        $track = New-Object System.Windows.Controls.Border
        $track.Height = 12; $track.CornerRadius = [System.Windows.CornerRadius]::new(6)
        $track.Background = Get-Brush '#1D222C'
        $track.Width = $barMax; $track.HorizontalAlignment = 'Left'; $track.VerticalAlignment = 'Center'
        $bar = New-Object System.Windows.Controls.Border
        $bar.Height = 12; $bar.CornerRadius = [System.Windows.CornerRadius]::new(6)
        $bar.HorizontalAlignment = 'Left'; $bar.Width = 0
        $bar.Background = Get-Brush $(if ($row.Mine) { $row.Color } else { '#3A4252' })
        if ($row.Mine) { $bar.Effect = New-Glow $row.Color 12 0.6 }
        $track.Child = $bar
        Add-ToGrid $g $track 1
        $val = New-Text '' 13 $(if ($row.Mine) { '#FFFFFF' } else { '#9AA3B2' }) -Semi
        $val.VerticalAlignment = 'Center'; $val.Margin = New-Thickness 12 0 0 0
        Add-ToGrid $g $val 2
        [void]$sp.Children.Add($g)
        $target = [math]::Max(4.0, $barMax * $row.Value / [math]::Max(1.0, $max))
        Start-WpfAnim $bar ([System.Windows.FrameworkElement]::WidthProperty) $target 1000 (150 * $i)
        Start-Anim { param($e, $s) $s.T.Text = ('{0:N0} ' -f ($s.V * $e)) + $s.U } @{ T = $val; V = [double]$row.Value; U = $Unit } 1000 (150 * $i)
        $i++
    }
    $sp
}

# Grande tuile chiffrée (score, nombre d'erreurs...) qui compte jusqu'à sa valeur.
function New-StatTile([string]$Label, [double]$Value, [string]$Fmt, [string]$Color = '#FFFFFF', [int]$Delay = 0) {
    $b = New-Object System.Windows.Controls.Border
    $b.Background = Get-Brush '#1A1F29'
    $b.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $b.Padding = New-Thickness 18 12 18 12
    $b.Margin = New-Thickness 0 0 10 10
    $b.MinWidth = 150
    $sp = New-Object System.Windows.Controls.StackPanel
    $num = New-Text '0' 28 $Color -Bold
    $num.TextWrapping = 'NoWrap'
    [void]$sp.Children.Add($num)
    [void]$sp.Children.Add((New-Text $Label 12.5 '#9AA3B2'))
    $b.Child = $sp
    Start-Anim { param($e, $s) $s.T.Text = $s.F -f ($s.V * $e) } @{ T = $num; V = $Value; F = $Fmt } 1200 $Delay
    $b
}

function New-StatRow([array]$Tiles) {
    $wp = New-Object System.Windows.Controls.WrapPanel
    $wp.Margin = New-Thickness 0 10 0 0
    foreach ($t2 in $Tiles) { [void]$wp.Children.Add($t2) }
    $wp
}

function New-Verdict([string]$Status, [string]$Text) {
    $b = New-Object System.Windows.Controls.Border
    $bg = Get-Brush $Colors[$Status]; $bg.Opacity = 0.12
    $b.Background = $bg
    $b.BorderBrush = Get-Brush $Colors[$Status]; $b.BorderThickness = New-Thickness 0 0 0 0
    $b.CornerRadius = [System.Windows.CornerRadius]::new(10)
    $b.Padding = New-Thickness 14 11 14 11
    $b.Margin = New-Thickness 0 14 0 0
    $g = New-Grid @('Auto', '*')
    $icon = New-Text $(switch ($Status) { 'ok' { '✓' } 'bad' { '!' } 'warn' { '!' } default { 'i' } }) 15 $Colors[$Status] -Bold
    $icon.Margin = New-Thickness 0 0 12 0
    Add-ToGrid $g $icon 0
    Add-ToGrid $g (New-Text $Text 13.5 $Colors[$Status] -Semi) 1
    $b.Child = $g
    $b.Opacity = 0
    Start-WpfAnim $b ([System.Windows.UIElement]::OpacityProperty) 1 600 700
    $b
}

function New-SectionTitle([string]$Text) {
    $title = New-Text $Text 12 '#5B6475' -Semi
    $title.Margin = New-Thickness 0 16 0 2
    $title
}

# Détails repliables: « Voir toutes les infos ».
function New-Details([array]$Rows) {
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Margin = New-Thickness 0 12 0 0
    $btn = New-Button 'Voir toutes les infos  ▾'
    $btn.HorizontalAlignment = 'Left'
    $box = New-Object System.Windows.Controls.StackPanel
    $box.Visibility = 'Collapsed'
    $box.Margin = New-Thickness 4 10 0 0
    foreach ($r in $Rows) {
        $g = New-Grid @('240', '*')
        $g.Margin = New-Thickness 0 3 0 3
        Add-ToGrid $g (New-Text $r[0] 12.5 '#9AA3B2') 0
        $col = if ($r.Count -gt 2) { $r[2] } else { '#E6E8EE' }
        Add-ToGrid $g (New-Text ([string]$r[1]) 12.5 $col) 1
        [void]$box.Children.Add($g)
    }
    $btn.Tag = $box
    $btn.Add_Click({
        param($s, $e)
        $open = $s.Tag.Visibility -ne 'Visible'
        $s.Tag.Visibility = if ($open) { 'Visible' } else { 'Collapsed' }
        $s.Content = if ($open) { 'Masquer les infos  ▴' } else { 'Voir toutes les infos  ▾' }
    })
    [void]$sp.Children.Add($btn)
    [void]$sp.Children.Add($box)
    $sp
}

# Étapes du test: ✓ faites, en cours (clignote), à venir.
function New-Stepper($Steps) {
    $wp = New-Object System.Windows.Controls.WrapPanel
    $wp.Margin = New-Thickness 0 2 0 8
    $chips = @{}
    foreach ($k in $Steps.Keys) {
        $b = New-Object System.Windows.Controls.Border
        $b.CornerRadius = [System.Windows.CornerRadius]::new(14)
        $b.Padding = New-Thickness 12 5 12 5
        $b.Margin = New-Thickness 0 0 8 6
        $b.Background = Get-Brush '#1A1F29'
        $txt = New-Text "○  $($Steps[$k])" 12.5 '#5B6475' -Semi
        $txt.TextWrapping = 'NoWrap'
        $b.Child = $txt
        [void]$wp.Children.Add($b)
        $chips[$k] = @{ B = $b; T = $txt; Label = $Steps[$k] }
    }
    @{ El = $wp; Chips = $chips; Keys = @($Steps.Keys); Cur = $null }
}

function Update-Stepper($Stepper, [string]$Phase, [switch]$AllDone) {
    if (-not $AllDone -and (-not $Phase -or $Stepper.Cur -eq $Phase -or -not $Stepper.Chips.ContainsKey($Phase))) { return }
    $idx = if ($AllDone) { $Stepper.Keys.Count } else { [array]::IndexOf($Stepper.Keys, $Phase) }
    for ($i = 0; $i -lt $Stepper.Keys.Count; $i++) {
        $ch = $Stepper.Chips[$Stepper.Keys[$i]]
        Stop-Pulse $ch.B
        if ($i -lt $idx) {
            $bg = Get-Brush $Colors.ok; $bg.Opacity = 0.15
            $ch.B.Background = $bg; $ch.T.Text = "✓  $($ch.Label)"; $ch.T.Foreground = Get-Brush $Colors.ok
        } elseif ($i -eq $idx) {
            $bg = Get-Brush $Colors.info; $bg.Opacity = 0.2
            $ch.B.Background = $bg; $ch.T.Text = "●  $($ch.Label)"; $ch.T.Foreground = Get-Brush '#FFFFFF'
            Start-Pulse $ch.B
        } else {
            $ch.B.Background = Get-Brush '#1A1F29'; $ch.T.Text = "○  $($ch.Label)"; $ch.T.Foreground = Get-Brush '#5B6475'
        }
    }
    $Stepper.Cur = $Phase
}

# ---------------------------------------------------------------------------
# Écran de chargement : compteur de vitesse animé (l'aiguille monte avec le chargement)
# ---------------------------------------------------------------------------
function New-LinearBrush([string[]]$Hex, [double]$X1 = 0, [double]$Y1 = 0, [double]$X2 = 1, [double]$Y2 = 0) {
    $b = New-Object System.Windows.Media.LinearGradientBrush
    $b.StartPoint = [System.Windows.Point]::new($X1, $Y1); $b.EndPoint = [System.Windows.Point]::new($X2, $Y2)
    for ($i = 0; $i -lt $Hex.Count; $i++) {
        [void]$b.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.ColorConverter]::ConvertFromString($Hex[$i]), $i / [math]::Max(1, $Hex.Count - 1)))
    }
    $b
}

function New-LoaderArc([double]$C, [double]$R, [double]$Start, [double]$Sweep, $Stroke, [double]$Thick) {
    $p = New-Object System.Windows.Shapes.Path
    $p.Data = Get-ArcGeometry $C $R $Start $Sweep
    $p.Stroke = $Stroke; $p.StrokeThickness = $Thick
    $p.StrokeStartLineCap = 'Round'; $p.StrokeEndLineCap = 'Round'
    $p
}

# Animation en boucle, notée pour être arrêtée quand l'écran disparaît
function Start-LoaderLoop($Target, $Property, [double]$From, [double]$To, [int]$Ms, [bool]$Reverse) {
    $a = New-Object System.Windows.Media.Animation.DoubleAnimation
    $a.From = $From; $a.To = $To; $a.AutoReverse = $Reverse
    $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds($Ms))
    $a.RepeatBehavior = [System.Windows.Media.Animation.RepeatBehavior]::Forever
    if ($Reverse) { $e = New-Object System.Windows.Media.Animation.SineEase; $e.EasingMode = 'EaseInOut'; $a.EasingFunction = $e }
    $Target.BeginAnimation($Property, $a)
    [void]$script:Loader.Loops.Add(@{ T = $Target; P = $Property })
}

# Un anneau qui tourne autour du compteur (un arc lumineux en forme de comète)
function New-LoaderSpinner($Canvas, [double]$C, [double]$R, [double]$Sweep, [string]$Hex, [double]$Thick, [int]$Ms, [bool]$Reverse) {
    $g = New-Object System.Windows.Controls.Canvas
    $g.Width = 2 * $C; $g.Height = 2 * $C
    $rot = New-Object System.Windows.Media.RotateTransform 0, $C, $C
    $g.RenderTransform = $rot
    $col = [System.Windows.Media.ColorConverter]::ConvertFromString($Hex)
    $b = New-Object System.Windows.Media.LinearGradientBrush
    $b.StartPoint = [System.Windows.Point]::new(0, 1); $b.EndPoint = [System.Windows.Point]::new(1, 0)
    [void]$b.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.Color]::FromArgb(0, $col.R, $col.G, $col.B), 0))
    [void]$b.GradientStops.Add([System.Windows.Media.GradientStop]::new($col, 1))
    $arc = New-LoaderArc $C $R 0 $Sweep $b $Thick
    [void]$g.Children.Add($arc)
    [void]$Canvas.Children.Add($g)
    Start-LoaderLoop $rot ([System.Windows.Media.RotateTransform]::AngleProperty) $(if ($Reverse) { 360 } else { 0 }) $(if ($Reverse) { 0 } else { 360 }) $Ms $false
    $arc
}

function Start-StartupLoader {
    $lh = $ui.StartupLoaderHost
    if (-not $lh) { return }
    $lh.Children.Clear()
    $S = 260.0; $C = 130.0
    $script:Loader = @{ Loops = (New-Object System.Collections.ArrayList); Shown = 0.0; Ticks = @() }
    $cv = New-Object System.Windows.Controls.Canvas
    $cv.Width = $S; $cv.Height = $S

    # Halo qui respire derrière le compteur
    $halo = New-Object System.Windows.Shapes.Ellipse
    $halo.Width = 220; $halo.Height = 220
    $rb = New-Object System.Windows.Media.RadialGradientBrush
    [void]$rb.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.ColorConverter]::ConvertFromString('#4022D37A'), 0))
    [void]$rb.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.ColorConverter]::ConvertFromString('#0022D37A'), 1))
    $halo.Fill = $rb
    $halo.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5)
    $hs = New-Object System.Windows.Media.ScaleTransform 1, 1
    $halo.RenderTransform = $hs
    [System.Windows.Controls.Canvas]::SetLeft($halo, 20); [System.Windows.Controls.Canvas]::SetTop($halo, 20)
    [void]$cv.Children.Add($halo)
    Start-LoaderLoop $hs ([System.Windows.Media.ScaleTransform]::ScaleXProperty) 0.85 1.12 1600 $true
    Start-LoaderLoop $hs ([System.Windows.Media.ScaleTransform]::ScaleYProperty) 0.85 1.12 1600 $true

    # Anneau fin, et deux comètes qui tournent en sens inverse
    $ring = New-Object System.Windows.Shapes.Ellipse
    $ring.Width = 228; $ring.Height = 228; $ring.Stroke = Get-Brush '#1A2130'; $ring.StrokeThickness = 1.5
    [System.Windows.Controls.Canvas]::SetLeft($ring, 16); [System.Windows.Controls.Canvas]::SetTop($ring, 16)
    [void]$cv.Children.Add($ring)
    $c1 = New-LoaderSpinner $cv $C 114 120 '#22D37A' 3 1500 $false
    $c1.Effect = New-Glow '#22D37A' 14 0.9
    [void](New-LoaderSpinner $cv $C 122 70 '#4EA8FF' 2 2600 $true)

    # Particules en orbite
    foreach ($pt in @(@(104, 3200, '#9022D37A', 4), @(126, 4300, '#904EA8FF', 3), @(96, 2400, '#70FFFFFF', 3))) {
        $g = New-Object System.Windows.Controls.Canvas
        $rot = New-Object System.Windows.Media.RotateTransform 0, $C, $C
        $g.RenderTransform = $rot
        $d = New-Object System.Windows.Shapes.Ellipse
        $d.Width = $pt[3]; $d.Height = $pt[3]; $d.Fill = Get-Brush $pt[2]
        [System.Windows.Controls.Canvas]::SetLeft($d, $C + $pt[0] - $pt[3] / 2); [System.Windows.Controls.Canvas]::SetTop($d, $C - $pt[3] / 2)
        [void]$g.Children.Add($d)
        [void]$cv.Children.Add($g)
        $r0 = Get-Random -Minimum 0 -Maximum 360
        Start-LoaderLoop $rot ([System.Windows.Media.RotateTransform]::AngleProperty) $r0 ($r0 + 360) $pt[1] $false
    }

    # Graduations du compteur (elles s'allument quand l'aiguille passe)
    for ($i = 0; $i -le 10; $i++) {
        $ang = (135 + 27 * $i) * [math]::PI / 180
        $ln = New-Object System.Windows.Shapes.Line
        $r1 = if ($i % 5 -eq 0) { 88 } else { 92 }
        $ln.X1 = $C + $r1 * [math]::Cos($ang); $ln.Y1 = $C + $r1 * [math]::Sin($ang)
        $ln.X2 = $C + 99 * [math]::Cos($ang); $ln.Y2 = $C + 99 * [math]::Sin($ang)
        $ln.Stroke = Get-Brush '#2A3242'; $ln.StrokeThickness = $(if ($i % 5 -eq 0) { 3 } else { 2 })
        $ln.StrokeStartLineCap = 'Round'; $ln.StrokeEndLineCap = 'Round'
        [void]$cv.Children.Add($ln)
        $script:Loader.Ticks += , @($ln, (10 * $i))
    }

    # Arc du compteur : fond et remplissage dégradé vert vers cyan
    [void]$cv.Children.Add((New-LoaderArc $C 78 135 270 (Get-Brush '#1B212C') 10))
    $prog = New-LoaderArc $C 78 135 0.1 (New-LinearBrush @('#22D37A', '#4EE0FF') 0 1 1 0) 10
    $prog.Effect = New-Glow '#22D37A' 16 0.7
    [void]$cv.Children.Add($prog)

    # Aiguille et moyeu
    $needle = New-Object System.Windows.Shapes.Polygon
    foreach ($p in @(@(($C - 10), ($C - 3)), @(($C + 64), $C), @(($C - 10), ($C + 3)))) { [void]$needle.Points.Add([System.Windows.Point]::new($p[0], $p[1])) }
    $needle.Fill = New-LinearBrush @('#FFFFFF', '#5CF0AA') 0 0 1 0
    $nrot = New-Object System.Windows.Media.RotateTransform 135, $C, $C
    $needle.RenderTransform = $nrot
    $needle.Effect = New-Glow '#5CF0AA' 10 0.8
    [void]$cv.Children.Add($needle)
    $hub = New-Object System.Windows.Shapes.Ellipse
    $hub.Width = 20; $hub.Height = 20; $hub.Fill = Get-Brush '#0E1116'; $hub.Stroke = Get-Brush '#22D37A'; $hub.StrokeThickness = 3
    [System.Windows.Controls.Canvas]::SetLeft($hub, $C - 10); [System.Windows.Controls.Canvas]::SetTop($hub, $C - 10)
    [void]$cv.Children.Add($hub)

    # Pourcentage sous le moyeu
    $txt = New-Text '0 %' 24 '#FFFFFF' -Bold
    $txt.Width = $S; $txt.TextAlignment = 'Center'; $txt.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe UI Variable Display, Segoe UI'
    [System.Windows.Controls.Canvas]::SetTop($txt, $C + 36)
    [void]$cv.Children.Add($txt)

    [void]$lh.Children.Add($cv)
    $script:Loader.Arc = $prog; $script:Loader.Needle = $nrot; $script:Loader.Text = $txt

    # Halo de fond qui dérive lentement
    if ($ui.StartupHalo) {
        $pa = New-Object System.Windows.Media.Animation.PointAnimation
        $pa.From = [System.Windows.Point]::new(0.3, 0.2); $pa.To = [System.Windows.Point]::new(0.7, 0.8)
        $pa.Duration = [System.Windows.Duration]::new([TimeSpan]::FromSeconds(6)); $pa.AutoReverse = $true
        $pa.RepeatBehavior = [System.Windows.Media.Animation.RepeatBehavior]::Forever
        $ui.StartupHalo.BeginAnimation([System.Windows.Media.RadialGradientBrush]::CenterProperty, $pa)
        [void]$script:Loader.Loops.Add(@{ T = $ui.StartupHalo; P = [System.Windows.Media.RadialGradientBrush]::CenterProperty })
    }
}

# L'aiguille monte (avec un petit rebond de moteur), l'arc se remplit, le pourcentage défile
function Set-LoaderProgress([double]$Pct) {
    $L = $script:Loader
    if (-not $L) { return }
    $a = New-Object System.Windows.Media.Animation.DoubleAnimation
    $a.To = 135 + 270 * $Pct / 100
    $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds(750))
    $e = New-Object System.Windows.Media.Animation.BackEase; $e.Amplitude = 0.5; $e.EasingMode = 'EaseOut'
    $a.EasingFunction = $e
    $L.Needle.BeginAnimation([System.Windows.Media.RotateTransform]::AngleProperty, $a)
    Start-Anim {
        param($k, $s)
        $L2 = $script:Loader
        if (-not $L2) { return }
        $v = $s.From + ($s.To - $s.From) * $k
        $L2.Shown = $v
        $L2.Arc.Data = Get-ArcGeometry 130 78 135 ([math]::Max(0.1, 270 * $v / 100))
        $L2.Text.Text = '{0:N0} %' -f $v
        foreach ($t in $L2.Ticks) { if ($v -ge $t[1] -and -not $t[0].Tag) { $t[0].Tag = 1; $t[0].Stroke = Get-Brush '#5CF0AA' } }
    } @{ From = $L.Shown; To = $Pct } 700
}

function Stop-StartupLoader {
    $L = $script:Loader
    if (-not $L) { return }
    foreach ($x in $L.Loops) { try { $x.T.BeginAnimation($x.P, $null) } catch {} }
    $script:Loader = $null
    $ui.StartupLoaderHost.Children.Clear()
}
