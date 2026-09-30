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

function Get-Color([string]$Hex) { [System.Windows.Media.ColorConverter]::ConvertFromString($Hex) }

# Couleur éclaircie (mélangée avec du blanc) pour les dégradés lumineux
function Get-LightHex([string]$Hex, [double]$Amount = 0.4) {
    $c = Get-Color $Hex
    $f = { param($v) [int]($v + (255 - $v) * $Amount) }
    '#{0:X2}{1:X2}{2:X2}' -f (& $f $c.R), (& $f $c.G), (& $f $c.B)
}

function New-AlphaBrush([string]$Hex, [byte]$Alpha) {
    $c = Get-Color $Hex
    New-Object System.Windows.Media.SolidColorBrush ([System.Windows.Media.Color]::FromArgb($Alpha, $c.R, $c.G, $c.B))
}

# Courbe lissée passant par tous les points (splines de Catmull-Rom converties en courbes de Bézier)
function Get-SmoothFigure([System.Collections.Generic.List[System.Windows.Point]]$P, [double]$MinY, [double]$MaxY) {
    $fig = New-Object System.Windows.Media.PathFigure
    $fig.StartPoint = $P[0]
    $n = $P.Count
    if ($n -eq 2) { [void]$fig.Segments.Add([System.Windows.Media.LineSegment]::new($P[1], $true)); return $fig }
    $pts = New-Object System.Windows.Media.PointCollection
    for ($i = 0; $i -lt $n - 1; $i++) {
        $p0 = $P[[math]::Max(0, $i - 1)]; $p1 = $P[$i]; $p2 = $P[$i + 1]; $p3 = $P[[math]::Min($n - 1, $i + 2)]
        $c1y = [math]::Min($MaxY, [math]::Max($MinY, $p1.Y + ($p2.Y - $p0.Y) / 6))
        $c2y = [math]::Min($MaxY, [math]::Max($MinY, $p2.Y - ($p3.Y - $p1.Y) / 6))
        [void]$pts.Add([System.Windows.Point]::new($p1.X + ($p2.X - $p0.X) / 6, $c1y))
        [void]$pts.Add([System.Windows.Point]::new($p2.X - ($p3.X - $p1.X) / 6, $c2y))
        [void]$pts.Add($p2)
    }
    [void]$fig.Segments.Add([System.Windows.Media.PolyBezierSegment]::new($pts, $true))
    $fig
}

# ---------------------------------------------------------------------------
# Jauge circulaire : disque en verre, couronne de LED, arc dégradé et curseur lumineux
# ---------------------------------------------------------------------------
function New-Gauge([string]$Label, [double]$Value, [double]$Max, [string]$Fmt, [string]$Unit, [string]$Color, [int]$Delay = 0) {
    $c = 72; $r = 56
    $root = New-Object System.Windows.Controls.StackPanel
    $root.Width = 150
    $root.Margin = New-Thickness 6 0 6 10
    $root.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5)
    $zoom = New-Object System.Windows.Media.ScaleTransform 0.9, 0.9
    $root.RenderTransform = $zoom
    $g = New-Object System.Windows.Controls.Canvas
    $g.Width = 144; $g.Height = 144

    # Disque central en verre
    $disc = New-Object System.Windows.Shapes.Ellipse
    $disc.Width = 88; $disc.Height = 88
    $rb = New-Object System.Windows.Media.RadialGradientBrush
    $rb.GradientOrigin = [System.Windows.Point]::new(0.4, 0.3)
    [void]$rb.GradientStops.Add([System.Windows.Media.GradientStop]::new((Get-Color '#222A38'), 0))
    [void]$rb.GradientStops.Add([System.Windows.Media.GradientStop]::new((Get-Color '#12161E'), 1))
    $disc.Fill = $rb; $disc.Stroke = New-AlphaBrush $Color 50; $disc.StrokeThickness = 1
    [System.Windows.Controls.Canvas]::SetLeft($disc, $c - 44); [System.Windows.Controls.Canvas]::SetTop($disc, $c - 44)
    [void]$g.Children.Add($disc)

    # Couronne de LED (éteinte, puis allumée jusqu'à la valeur)
    $ledOff = New-LoaderArc $c 69 135 270 (Get-Brush '#1E2531') 4
    $ledOff.StrokeStartLineCap = 'Flat'; $ledOff.StrokeEndLineCap = 'Flat'
    $ledOff.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(0.7, 1.3))
    [void]$g.Children.Add($ledOff)
    $led = New-LoaderArc $c 69 135 0.1 (Get-Brush $Color) 4
    $led.StrokeStartLineCap = 'Flat'; $led.StrokeEndLineCap = 'Flat'
    $led.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(0.7, 1.3))
    $led.Opacity = 0.75
    [void]$g.Children.Add($led)

    # Piste et arc de valeur en dégradé lumineux
    [void]$g.Children.Add((New-LoaderArc $c $r 135 270 (Get-Brush '#1A2029') 10))
    $arc = New-LoaderArc $c $r 135 0.1 (New-LinearBrush @($Color, (Get-LightHex $Color 0.45)) 0 1 1 0) 10
    $arc.Effect = New-Glow $Color 18 0.65
    [void]$g.Children.Add($arc)

    # Curseur lumineux au bout de l'arc
    $knob = New-Object System.Windows.Shapes.Ellipse
    $knob.Width = 14; $knob.Height = 14; $knob.Fill = Get-Brush '#FFFFFF'
    $knob.Stroke = Get-Brush $Color; $knob.StrokeThickness = 3
    $knob.Effect = New-Glow $Color 14 0.9
    $knob.Visibility = 'Hidden'
    [void]$g.Children.Add($knob)

    # Valeur au centre
    $center = New-Object System.Windows.Controls.StackPanel
    $center.Width = 144
    [System.Windows.Controls.Canvas]::SetTop($center, $c - 24)
    $num = New-Text '0' 27 '#FFFFFF' -Bold
    $num.HorizontalAlignment = 'Center'; $num.TextWrapping = 'NoWrap'
    $num.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe UI Variable Display, Segoe UI'
    $u = New-Text $Unit 11 '#8B95A7'
    $u.HorizontalAlignment = 'Center'; $u.Margin = New-Thickness 0 -3 0 0
    [void]$center.Children.Add($num)
    [void]$center.Children.Add($u)
    [void]$g.Children.Add($center)
    [void]$root.Children.Add($g)
    $lbl = New-Text $Label 13 '#C9CED8' -Semi
    $lbl.HorizontalAlignment = 'Center'; $lbl.TextAlignment = 'Center'
    $lbl.Margin = New-Thickness 0 -6 0 0
    [void]$root.Children.Add($lbl)

    # Entrée : léger zoom et fondu
    $root.Opacity = 0
    Start-WpfAnim $root ([System.Windows.UIElement]::OpacityProperty) 1 500 $Delay
    Start-WpfAnim $zoom ([System.Windows.Media.ScaleTransform]::ScaleXProperty) 1 600 $Delay
    Start-WpfAnim $zoom ([System.Windows.Media.ScaleTransform]::ScaleYProperty) 1 600 $Delay

    $state = @{ Arc = $arc; Led = $led; Knob = $knob; Num = $num; From = 0.0; To = $Value; Cur = 0.0; Max = [math]::Max(1e-6, $Max); Fmt = $Fmt; C = $c; R = $r }
    Start-Anim { param($e, $s) Update-GaugeVisual $s ($s.From + ($s.To - $s.From) * $e) } $state 1300 $Delay
    @{ El = $root; State = $state }
}

function Update-GaugeVisual($S, [double]$V) {
    $S.Cur = $V
    $f = [math]::Min(1.0, [math]::Max(0.0, $V / $S.Max))
    $sweep = [math]::Max(0.1, 270 * $f)
    $S.Arc.Data = Get-ArcGeometry $S.C $S.R 135 $sweep
    $S.Led.Data = Get-ArcGeometry $S.C 69 135 $sweep
    $a = (135 + $sweep) * [math]::PI / 180
    [System.Windows.Controls.Canvas]::SetLeft($S.Knob, $S.C + $S.R * [math]::Cos($a) - 7)
    [System.Windows.Controls.Canvas]::SetTop($S.Knob, $S.C + $S.R * [math]::Sin($a) - 7)
    $S.Knob.Visibility = if ($f -gt 0.01) { 'Visible' } else { 'Hidden' }
    $S.Num.Text = $S.Fmt -f $V
}

# Fait glisser une jauge vers une nouvelle valeur (mode « en direct »).
function Set-GaugeLive($Gauge, [double]$Value) {
    $s = $Gauge.State
    $s.From = $s.Cur; $s.To = $Value
    Start-Anim { param($e, $st) Update-GaugeVisual $st ($st.From + ($st.To - $st.From) * $e) } $s 700
}

function New-GaugeRow([array]$Gauges) {
    $wp = New-Object System.Windows.Controls.WrapPanel
    $wp.HorizontalAlignment = 'Center'
    $wp.Margin = New-Thickness 0 6 0 4
    foreach ($g in $Gauges) { [void]$wp.Children.Add($g.El) }
    $wp
}

# ---------------------------------------------------------------------------
# Courbe en direct : ligne lissée et dégradée, zone en fondu, bulle de la valeur actuelle
# ---------------------------------------------------------------------------
function New-LiveChart([string]$Color, [string]$Unit, [string]$Fmt = '{0:N0}') {
    $w = 660; $h = 150
    $cv = New-Object System.Windows.Controls.Canvas
    $cv.Width = $w; $cv.Height = $h; $cv.ClipToBounds = $true
    foreach ($y in 0.25, 0.5, 0.75) {
        $ln = New-Object System.Windows.Shapes.Line
        $ln.X1 = 0; $ln.X2 = $w; $ln.Y1 = $h * $y; $ln.Y2 = $h * $y
        $ln.Stroke = Get-Brush '#232A37'; $ln.StrokeThickness = 1
        $ln.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(2, 4))
        [void]$cv.Children.Add($ln)
    }
    $col = Get-Color $Color
    $grad = New-Object System.Windows.Media.LinearGradientBrush
    $grad.StartPoint = [System.Windows.Point]::new(0, 0); $grad.EndPoint = [System.Windows.Point]::new(0, 1)
    [void]$grad.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.Color]::FromArgb(120, $col.R, $col.G, $col.B), 0))
    [void]$grad.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.Color]::FromArgb(0, $col.R, $col.G, $col.B), 1))
    $fill = New-Object System.Windows.Shapes.Path
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
    $line = New-Object System.Windows.Shapes.Path
    $lb = New-Object System.Windows.Media.LinearGradientBrush
    $lb.StartPoint = [System.Windows.Point]::new(0, 0); $lb.EndPoint = [System.Windows.Point]::new(1, 0)
    [void]$lb.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.Color]::FromArgb(90, $col.R, $col.G, $col.B), 0))
    [void]$lb.GradientStops.Add([System.Windows.Media.GradientStop]::new($col, 0.6))
    [void]$lb.GradientStops.Add([System.Windows.Media.GradientStop]::new((Get-Color (Get-LightHex $Color 0.4)), 1))
    $line.Stroke = $lb; $line.StrokeThickness = 2.5
    $line.StrokeLineJoin = 'Round'
    $line.Effect = New-Glow $Color 12 0.75
    [void]$cv.Children.Add($line)
    $halo = New-Object System.Windows.Shapes.Ellipse
    $halo.Width = 22; $halo.Height = 22; $halo.Fill = New-AlphaBrush $Color 60
    $halo.Visibility = 'Hidden'
    [void]$cv.Children.Add($halo)
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 11; $dot.Height = 11; $dot.Fill = Get-Brush '#FFFFFF'
    $dot.Stroke = Get-Brush $Color; $dot.StrokeThickness = 2.5
    $dot.Effect = New-Glow $Color 14 0.9
    $dot.Visibility = 'Hidden'
    [void]$cv.Children.Add($dot)
    # Bulle de la valeur actuelle
    $bub = New-Object System.Windows.Controls.Border
    $bub.Background = New-AlphaBrush $Color 50; $bub.BorderBrush = New-AlphaBrush $Color 140; $bub.BorderThickness = New-Thickness 1 1 1 1
    $bub.CornerRadius = [System.Windows.CornerRadius]::new(8); $bub.Padding = New-Thickness 7 2 7 2
    $bubText = New-Text '' 11.5 '#FFFFFF' -Semi
    $bubText.TextWrapping = 'NoWrap'
    $bub.Child = $bubText
    $bub.Visibility = 'Hidden'
    [void]$cv.Children.Add($bub)
    $maxText = New-Text '' 11 '#5B6475'
    [System.Windows.Controls.Canvas]::SetLeft($maxText, 4); [System.Windows.Controls.Canvas]::SetTop($maxText, 2)
    [void]$cv.Children.Add($maxText)
    $border = New-Object System.Windows.Controls.Border
    $border.Background = New-LinearBrush @('#141922', '#0E1117') 0 0 0 1
    $border.BorderBrush = Get-Brush 'card-border'; $border.BorderThickness = New-Thickness 1 1 1 1
    $border.CornerRadius = [System.Windows.CornerRadius]::new(14)
    $border.Padding = New-Thickness 12 10 12 10
    $border.Margin = New-Thickness 0 10 0 6
    $border.Child = $cv
    @{ El = $border; Line = $line; Fill = $fill; Dot = $dot; Halo = $halo; Bubble = $bub; BubbleText = $bubText; MaxText = $maxText; Ref = $ref; RefText = $refText; RefValue = $null
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
    $max = 0.0
    foreach ($v in $vals) { if ([double]$v -gt $max) { $max = [double]$v } }
    if ($Chart.RefValue) { $max = [math]::Max($max, $Chart.RefValue) }
    if ($max -le 0) { $max = 1 }
    $max *= 1.2
    $w = $Chart.W; $h = $Chart.H
    $step = $w / 159
    $pts = New-Object 'System.Collections.Generic.List[System.Windows.Point]'
    for ($i = 0; $i -lt $n; $i++) { $pts.Add([System.Windows.Point]::new($i * $step, $h - [double]$vals[$i] / $max * ($h - 8))) }
    $fig = Get-SmoothFigure $pts 0 $h
    $lg = New-Object System.Windows.Media.PathGeometry
    [void]$lg.Figures.Add($fig)
    $Chart.Line.Data = $lg
    # Zone : la même courbe, fermée par le bas
    $af = $fig.Clone()
    [void]$af.Segments.Add([System.Windows.Media.LineSegment]::new([System.Windows.Point]::new(($n - 1) * $step, $h), $false))
    [void]$af.Segments.Add([System.Windows.Media.LineSegment]::new([System.Windows.Point]::new(0, $h), $false))
    $af.IsClosed = $true
    $ag = New-Object System.Windows.Media.PathGeometry
    [void]$ag.Figures.Add($af)
    $Chart.Fill.Data = $ag
    $last = $pts[$n - 1]
    [System.Windows.Controls.Canvas]::SetLeft($Chart.Dot, $last.X - 5.5); [System.Windows.Controls.Canvas]::SetTop($Chart.Dot, $last.Y - 5.5)
    [System.Windows.Controls.Canvas]::SetLeft($Chart.Halo, $last.X - 11); [System.Windows.Controls.Canvas]::SetTop($Chart.Halo, $last.Y - 11)
    $Chart.Dot.Visibility = 'Visible'; $Chart.Halo.Visibility = 'Visible'
    $Chart.BubbleText.Text = "$($Chart.Fmt -f [double]$vals[$n - 1]) $($Chart.Unit)"
    $bx = [math]::Min($w - 90.0, [math]::Max(0.0, $last.X - 96))
    $by = if ($last.Y -lt 34) { $last.Y + 12 } else { $last.Y - 30 }
    [System.Windows.Controls.Canvas]::SetLeft($Chart.Bubble, $bx); [System.Windows.Controls.Canvas]::SetTop($Chart.Bubble, $by)
    $Chart.Bubble.Visibility = 'Visible'
    $Chart.MaxText.Text = "max $($Chart.Fmt -f ($max / 1.2)) $($Chart.Unit)"
    if ($Chart.RefValue) {
        $y = $h - $Chart.RefValue / $max * ($h - 8)
        $Chart.Ref.Y1 = $y; $Chart.Ref.Y2 = $y; $Chart.Ref.Visibility = 'Visible'
        [System.Windows.Controls.Canvas]::SetLeft($Chart.RefText, 8)
        [System.Windows.Controls.Canvas]::SetTop($Chart.RefText, $y - 16)
        $Chart.RefText.Visibility = 'Visible'
    }
}

# ---------------------------------------------------------------------------
# Barres de comparaison : dégradé, reflet brillant, halo sur la tienne
# ---------------------------------------------------------------------------
function New-CompareBars([array]$Rows, [string]$Unit) {
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Margin = New-Thickness 0 8 0 0
    $max = 0.0
    foreach ($row in $Rows) { if ([double]$row.Value -gt $max) { $max = [double]$row.Value } }
    $barMax = 430.0
    $i = 0
    foreach ($row in $Rows) {
        $g = New-Grid @('150', '440', '*')
        $g.Margin = New-Thickness 0 6 0 6
        $lbl = New-Text $row.Label 13 $(if ($row.Mine) { '#FFFFFF' } else { '#9AA3B2' })
        if ($row.Mine) { $lbl.FontWeight = [System.Windows.FontWeights]::SemiBold }
        $lbl.VerticalAlignment = 'Center'
        Add-ToGrid $g $lbl 0
        $track = New-Object System.Windows.Controls.Border
        $track.Height = 14; $track.CornerRadius = [System.Windows.CornerRadius]::new(7)
        $track.Background = Get-Brush '#1A1F29'
        $track.Width = $barMax; $track.HorizontalAlignment = 'Left'; $track.VerticalAlignment = 'Center'
        $bar = New-Object System.Windows.Controls.Border
        $bar.Height = 14; $bar.CornerRadius = [System.Windows.CornerRadius]::new(7)
        $bar.HorizontalAlignment = 'Left'; $bar.Width = 0
        $hex = if ($row.Mine) { $row.Color } else { '#3A4252' }
        $bar.Background = New-LinearBrush @($hex, (Get-LightHex $hex 0.35)) 0 0 1 0
        $shine = New-Object System.Windows.Controls.Border
        $shine.CornerRadius = [System.Windows.CornerRadius]::new(7)
        $shine.Background = New-LinearBrush @('#50FFFFFF', '#00FFFFFF') 0 0 0 1
        $bar.Child = $shine
        if ($row.Mine) { $bar.Effect = New-Glow $row.Color 14 0.7 }
        $track.Child = $bar
        Add-ToGrid $g $track 1
        $val = New-Text '' 13 $(if ($row.Mine) { '#FFFFFF' } else { '#9AA3B2' }) -Semi
        $val.VerticalAlignment = 'Center'; $val.Margin = New-Thickness 12 0 0 0
        Add-ToGrid $g $val 2
        [void]$sp.Children.Add($g)
        $target = [math]::Max(6.0, $barMax * $row.Value / [math]::Max(1.0, $max))
        Start-WpfAnim $bar ([System.Windows.FrameworkElement]::WidthProperty) $target 1000 (150 * $i)
        Start-Anim { param($e, $s) $s.T.Text = ('{0:N0} ' -f ($s.V * $e)) + $s.U } @{ T = $val; V = [double]$row.Value; U = $Unit } 1000 (150 * $i)
        $i++
    }
    $sp
}

# ---------------------------------------------------------------------------
# Tuile chiffrée : verre, liseré de couleur lumineux, chiffre qui compte jusqu'à sa valeur
# ---------------------------------------------------------------------------
function New-StatTile([string]$Label, [double]$Value, [string]$Fmt, [string]$Color = '#FFFFFF', [int]$Delay = 0) {
    $accent = if ($Color -eq '#FFFFFF') { $Colors.info } else { $Color }
    $b = New-Object System.Windows.Controls.Border
    $b.Background = Get-Brush 'card'
    $b.BorderBrush = Get-Brush 'card-border'; $b.BorderThickness = New-Thickness 1 1 1 1
    $b.CornerRadius = [System.Windows.CornerRadius]::new(14)
    $b.Margin = New-Thickness 0 0 10 10
    $b.MinWidth = 160
    $g = New-Object System.Windows.Controls.Grid
    $bar = New-Object System.Windows.Controls.Border
    $bar.Height = 3; $bar.VerticalAlignment = 'Top'; $bar.Margin = New-Thickness 14 0 14 0
    $bar.CornerRadius = [System.Windows.CornerRadius]::new(0, 0, 3, 3)
    $bar.Background = New-LinearBrush @($accent, (Get-LightHex $accent 0.4)) 0 0 1 0
    $bar.Effect = New-Glow $accent 10 0.8
    [void]$g.Children.Add($bar)
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Margin = New-Thickness 18 14 18 12
    $num = New-Text '0' 28 $Color -Bold
    $num.TextWrapping = 'NoWrap'
    $num.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe UI Variable Display, Segoe UI'
    [void]$sp.Children.Add($num)
    [void]$sp.Children.Add((New-Text $Label 12.5 '#9AA3B2'))
    [void]$g.Children.Add($sp)
    $b.Child = $g
    $b.Opacity = 0
    Start-WpfAnim $b ([System.Windows.UIElement]::OpacityProperty) 1 500 $Delay
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
