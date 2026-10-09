# Nevermind : animations, jauges, courbes et petits composants visuels.
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
    $fx.Color = [System.Windows.Media.ColorConverter]::ConvertFromString((ConvertTo-ThemeHex $Hex))
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

function Get-Color([string]$Hex) { [System.Windows.Media.ColorConverter]::ConvertFromString((ConvertTo-ThemeHex $Hex)) }

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

    # Halo très léger de la couleur au centre (style verre néon)
    $disc = New-Object System.Windows.Shapes.Ellipse
    $disc.Width = 88; $disc.Height = 88
    $rb = New-Object System.Windows.Media.RadialGradientBrush
    [void]$rb.GradientStops.Add([System.Windows.Media.GradientStop]::new((New-AlphaBrush $Color 34).Color, 0))
    [void]$rb.GradientStops.Add([System.Windows.Media.GradientStop]::new((New-AlphaBrush $Color 0).Color, 1))
    $disc.Fill = $rb
    [System.Windows.Controls.Canvas]::SetLeft($disc, $c - 44); [System.Windows.Controls.Canvas]::SetTop($disc, $c - 44)
    [void]$g.Children.Add($disc)

    # Couronne de LED (éteinte, puis allumée jusqu'à la valeur)
    $ledOff = New-LoaderArc $c 69 135 270 (Get-Brush '#1E2531') 4
    $ledOff.Visibility = 'Collapsed'
    $ledOff.StrokeStartLineCap = 'Flat'; $ledOff.StrokeEndLineCap = 'Flat'
    $ledOff.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(0.7, 1.3))
    [void]$g.Children.Add($ledOff)
    $led = New-LoaderArc $c 69 135 0.1 (Get-Brush $Color) 4
    $led.StrokeStartLineCap = 'Flat'; $led.StrokeEndLineCap = 'Flat'
    $led.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(0.7, 1.3))
    $led.Visibility = 'Collapsed'   # ancienne couronne de LED, retirée avec la DA néon
    [void]$g.Children.Add($led)

    # Piste et arc de valeur en dégradé lumineux
    [void]$g.Children.Add((New-LoaderArc $c $r 135 270 (Get-Brush '#22FFFFFF') 10))
    $arc = New-LoaderArc $c $r 135 0.1 (New-LinearBrush @($Color, (Get-LightHex $Color 0.45)) 0 1 1 0) 10
    $arc.Effect = New-Glow $Color 18 0.65
    [void]$g.Children.Add($arc)

    # Curseur lumineux au bout de l'arc
    $knob = New-Object System.Windows.Shapes.Ellipse
    $knob.Width = 14; $knob.Height = 14; $knob.Fill = Get-Brush '#FFFFFF'
    $knob.Stroke = Get-Brush $Color; $knob.StrokeThickness = 3
    $knob.Effect = New-Glow $Color 14 0.9
    $knob.Visibility = 'Hidden'
    $knob.Width = 10; $knob.Height = 10; $knob.Stroke = $null
    [void]$g.Children.Add($knob)

    # Valeur au centre
    $center = New-Object System.Windows.Controls.StackPanel
    $center.Width = 144
    [System.Windows.Controls.Canvas]::SetTop($center, $c - 24)
    $num = New-Text '0' 27 '#FFFFFF' -Bold
    $num.HorizontalAlignment = 'Center'; $num.TextWrapping = 'NoWrap'
    $num.FontFamily = New-Object System.Windows.Media.FontFamily $MonoFont
    $u = New-Text $Unit 11 '#958EAE'
    $u.HorizontalAlignment = 'Center'; $u.Margin = New-Thickness 0 $(if ($PackSizeAll) { 3 } else { -3 }) 0 0   # police pixel : pas de chevauchement
    [void]$center.Children.Add($num)
    [void]$center.Children.Add($u)
    [void]$g.Children.Add($center)
    [void]$root.Children.Add($g)
    $lbl = New-Text $Label 13 '#D3CDE3' -Semi
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
    [System.Windows.Controls.Canvas]::SetLeft($S.Knob, $S.C + $S.R * [math]::Cos($a) - $S.Knob.Width / 2)
    [System.Windows.Controls.Canvas]::SetTop($S.Knob, $S.C + $S.R * [math]::Sin($a) - $S.Knob.Height / 2)
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
        $ln.Stroke = Get-Brush '#1CFFFFFF'; $ln.StrokeThickness = 1
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
    $maxText = New-Text '' 11 '#655E7E'
    [System.Windows.Controls.Canvas]::SetLeft($maxText, 4); [System.Windows.Controls.Canvas]::SetTop($maxText, 2)
    [void]$cv.Children.Add($maxText)
    $border = New-Object System.Windows.Controls.Border
    $border.Background = New-LinearBrush @('#14FFFFFF', '#06FFFFFF') 0 0 0 1
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
        $lbl = New-Text $row.Label 13 $(if ($row.Mine) { '#FFFFFF' } else { '#A6A1BC' })
        if ($row.Mine) { $lbl.FontWeight = [System.Windows.FontWeights]::SemiBold }
        $lbl.VerticalAlignment = 'Center'
        Add-ToGrid $g $lbl 0
        $track = New-Object System.Windows.Controls.Border
        $track.Height = 14; $track.CornerRadius = [System.Windows.CornerRadius]::new(7)
        $track.Background = Get-Brush '#16FFFFFF'
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
        $val = New-Text '' 13 $(if ($row.Mine) { '#FFFFFF' } else { '#A6A1BC' }) -Semi
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
    $num.FontFamily = New-Object System.Windows.Media.FontFamily $MonoFont
    [void]$sp.Children.Add($num)
    [void]$sp.Children.Add((New-Text $Label 12.5 '#A6A1BC'))
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

# Titre de section : un point néon cyan puis le titre en petites capitales discrètes
function New-SectionTitle([string]$Text) {
    $title = New-Object System.Windows.Controls.TextBlock
    $title.FontSize = Get-UiFontSize 11.5; $title.FontWeight = 'SemiBold'
    $title.TextWrapping = 'Wrap'
    $r1 = New-Object System.Windows.Documents.Run '●  '
    $r1.Foreground = Get-Brush $NexoCyan
    $r2 = New-Object System.Windows.Documents.Run $Text.ToUpper()
    Set-PackFont $title
    $r2.Foreground = Get-Brush '#8E88A8'
    $title.Inlines.Add($r1); $title.Inlines.Add($r2)
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
        Add-ToGrid $g (New-Text $r[0] 12.5 '#A6A1BC') 0
        $col = if ($r.Count -gt 2) { $r[2] } else { '#EEEBF7' }
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
        $b.Background = Get-Brush '#16FFFFFF'
        $txt = New-Text "○  $($Steps[$k])" 12.5 '#655E7E' -Semi
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
            $ch.B.Background = Get-Brush '#16FFFFFF'; $ch.T.Text = "○  $($ch.Label)"; $ch.T.Foreground = Get-Brush '#655E7E'
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
        [void]$b.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.ColorConverter]::ConvertFromString((ConvertTo-ThemeHex $Hex[$i])), $i / [math]::Max(1, $Hex.Count - 1)))
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

# ---------------------------------------------------------------------------
# Logo Nevermind : un N blanc avec ses échos cyan et magenta et deux tranches décalées (effet « glitch »).
# Il « saute » par moments : au survol, toutes les quelques secondes, et pendant le chargement.
# ---------------------------------------------------------------------------
$NexoCyan = '#00E5FF'
$MonoFont = if ($PackFont -and $ThemePack.FontScope -eq 'tout') { "$PackFont, Cascadia Code, Consolas" } else { 'Cascadia Code, Consolas' }   # police « code » de la DA : chiffres, titres de section, barre d'état
$NexoMagenta = '#FF2EB5'

# Forme du N dans un carré de côté $Size (trait épais aux bouts arrondis)
function Get-NexoN([double]$Size) {
    $x = $Size * 0.22; $y = $Size * 0.2; $w = $Size * 0.56; $h = $Size * 0.6
    $p = [System.Windows.Media.Geometry]::Parse("M $x,$($y + $h) L $x,$y L $($x + $w),$($y + $h) L $($x + $w),$y")
    $pen = New-Object System.Windows.Media.Pen ([System.Windows.Media.Brushes]::Black), ($Size * 0.19)
    $pen.StartLineCap = 'Round'; $pen.EndLineCap = 'Round'; $pen.LineJoin = 'Round'
    $wide = $p.GetWidenedPathGeometry($pen)
    $g = [System.Windows.Media.Geometry]::Combine($wide, $wide, 'Union', $null)
    $g.Freeze()
    $g
}

function New-GeoPath($Geo, [string]$Hex) {
    $p = New-Object System.Windows.Shapes.Path
    $p.Data = $Geo; $p.Fill = Get-Brush $Hex
    $p.RenderTransform = New-Object System.Windows.Media.TranslateTransform 0, 0
    $p
}

# Le N animable : { Root, Cyan, Mag, Base, Pieces (tranches), Size, Rest (décalages au repos) }
function New-NexoMark([double]$Size) {
    $n = Get-NexoN $Size
    $root = New-Object System.Windows.Controls.Grid
    $root.Width = $Size; $root.Height = $Size
    $cyan = New-GeoPath $n $NexoCyan; $cyan.Opacity = 0.92
    $mag = New-GeoPath $n $NexoMagenta; $mag.Opacity = 0.92
    # Le N blanc privé de ses deux tranches, et les tranches à part pour pouvoir les décaler
    $bands = @(@(0.40, 0.07), @(0.61, 0.05))
    $bandGeo = New-Object System.Windows.Media.GeometryGroup
    foreach ($b in $bands) { $bandGeo.Children.Add((New-Object System.Windows.Media.RectangleGeometry ([System.Windows.Rect]::new(-$Size, $Size * $b[0], 3 * $Size, $Size * $b[1])))) }
    $base = New-GeoPath ([System.Windows.Media.Geometry]::Combine($n, $bandGeo, 'Exclude', $null)) '#FFFFFF'
    $pieces = foreach ($b in $bands) {
        $r = New-Object System.Windows.Media.RectangleGeometry ([System.Windows.Rect]::new(-$Size, $Size * $b[0], 3 * $Size, $Size * $b[1]))
        New-GeoPath ([System.Windows.Media.Geometry]::Combine($n, $r, 'Intersect', $null)) '#FFFFFF'
    }
    foreach ($e in @($cyan, $mag, $base) + @($pieces)) { [void]$root.Children.Add($e) }
    $m = @{ Root = $root; Cyan = $cyan; Mag = $mag; Base = $base; Pieces = @($pieces); Size = $Size; Rest = @(-0.035, 0.035, 0.05, -0.04) }
    Set-NexoPose $m $m.Rest
    $m
}

# Pose : décalages (en fraction de la taille) du cyan, du magenta et des deux tranches
function Set-NexoPose($M, [double[]]$Pose) {
    $s = $M.Size
    $M.Cyan.RenderTransform.X = $Pose[0] * $s
    $M.Mag.RenderTransform.X = $Pose[1] * $s
    $M.Pieces[0].RenderTransform.X = $Pose[2] * $s
    $M.Pieces[1].RenderTransform.X = $Pose[3] * $s
}

# Le mot « Nevermind » avec les mêmes échos de couleur : { Root, Cyan, Mag, Text }
function New-NexoWord([double]$FontSize) {
    $root = New-Object System.Windows.Controls.Grid
    $mk = {
        param($hex, $dx)
        $t = New-Object System.Windows.Controls.TextBlock
        $t.Text = 'Nevermind'; $t.FontSize = Get-UiFontSize $FontSize; $t.FontWeight = 'Bold'
        $t.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe UI Variable Display, Segoe UI'
        $t.Foreground = Get-Brush $hex
        $t.RenderTransform = New-Object System.Windows.Media.TranslateTransform $dx, 0
        $t
    }
    $c = & $mk $NexoCyan (-$FontSize * 0.05); $c.Opacity = 0.85
    $m = & $mk $NexoMagenta ($FontSize * 0.05); $m.Opacity = 0.85
    $w = & $mk '#FFFFFF' 0
    foreach ($e in $c, $m, $w) { [void]$root.Children.Add($e) }
    @{ Root = $root; Cyan = $c; Mag = $m; Text = $w; Size = $FontSize }
}

# Un « saut » de quelques centaines de millisecondes : décalages au hasard, puis retour au repos
function Start-NexoGlitch($Mark, $Word = $null, [int]$Ms = 360) {
    if (-not $Mark -or $Mark.Busy) { return }
    $Mark.Busy = $true
    $t = New-Object System.Windows.Threading.DispatcherTimer
    $t.Interval = [TimeSpan]::FromMilliseconds(45)
    $t.Tag = @{ Mark = $Mark; Word = $Word; Until = [DateTime]::Now.AddMilliseconds($Ms); Rnd = (New-Object Random) }
    $t.Add_Tick({
        param($s, $e)
        $x = $s.Tag; $r = $x.Rnd
        if ([DateTime]::Now -gt $x.Until) {
            $s.Stop()
            Set-NexoPose $x.Mark $x.Mark.Rest
            $x.Mark.Base.RenderTransform.X = 0
            if ($x.Word) { $x.Word.Cyan.RenderTransform.X = -$x.Word.Size * 0.05; $x.Word.Mag.RenderTransform.X = $x.Word.Size * 0.05; $x.Word.Text.RenderTransform.X = 0 }
            $x.Mark.Busy = $false
            return
        }
        $j = { param($a) ($r.NextDouble() * 2 - 1) * $a }
        Set-NexoPose $x.Mark @((& $j 0.09), (& $j 0.09), (& $j 0.14), (& $j 0.14))
        $x.Mark.Base.RenderTransform.X = (& $j 0.03) * $x.Mark.Size
        if ($x.Word) { $f = $x.Word.Size; $x.Word.Cyan.RenderTransform.X = & $j ($f * 0.18); $x.Word.Mag.RenderTransform.X = & $j ($f * 0.18); $x.Word.Text.RenderTransform.X = & $j ($f * 0.05) }
    })
    $t.Start()
}

# Logo de la barre de gauche : saute au survol et de temps en temps (toutes les 6 à 12 s)
function Initialize-NexoLogo($MarkHost, $WordHost) {
    if (-not $MarkHost -or -not $WordHost) { return }
    if ($ThemePack -and $ThemePack.Logo) {
        try { Initialize-PackLogo $MarkHost $WordHost; return } catch { Write-Log "Logo du pack : $_" }
    }
    $script:LogoMark = New-NexoMark 30
    $script:LogoWord = New-NexoWord 22
    $MarkHost.Child = $script:LogoMark.Root
    $WordHost.Children.Clear(); [void]$WordHost.Children.Add($script:LogoWord.Root)
    $hover = { Start-NexoGlitch $script:LogoMark $script:LogoWord 420 }
    $MarkHost.Add_MouseEnter($hover); $WordHost.Add_MouseEnter($hover)
    $script:LogoTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:LogoTimer.Interval = [TimeSpan]::FromSeconds(8)
    $script:LogoTimer.Add_Tick({
        param($s, $e)
        $s.Interval = [TimeSpan]::FromSeconds((Get-Random -Minimum 6 -Maximum 13))
        if ($Window.IsVisible -and $Window.IsActive) { Start-NexoGlitch $script:LogoMark $script:LogoWord }
    })
    $script:LogoTimer.Start()
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
    $col = [System.Windows.Media.ColorConverter]::ConvertFromString((ConvertTo-ThemeHex $Hex))
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

# Écran de chargement : le N de Nevermind qui « glitche », un anneau de progression cyan vers magenta,
# un halo qui respire et une ligne de balayage, comme un vieil écran qui s'allume.
function Start-StartupLoader {
    $lh = $ui.StartupLoaderHost
    if (-not $lh) { return }
    # Pack de thème avec son propre écran de chargement (personnage qui court)
    if ($ThemePack -and $ThemePack.Loader) {
        try { Start-SpriteLoader; return } catch { Write-Log "Chargement du pack : $_" }
    }
    $lh.Children.Clear()
    $S = 260.0; $C = 130.0
    $script:Loader = @{ Loops = (New-Object System.Collections.ArrayList); Shown = 0.0 }
    $cv = New-Object System.Windows.Controls.Canvas
    $cv.Width = $S; $cv.Height = $S

    # Halo qui respire derrière le logo
    $halo = New-Object System.Windows.Shapes.Ellipse
    $halo.Width = 220; $halo.Height = 220
    $rb = New-Object System.Windows.Media.RadialGradientBrush
    [void]$rb.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.ColorConverter]::ConvertFromString((ConvertTo-ThemeHex '#4000E5FF')), 0))
    [void]$rb.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.ColorConverter]::ConvertFromString((ConvertTo-ThemeHex '#1AFF2EB5')), 0.6))
    [void]$rb.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.ColorConverter]::ConvertFromString((ConvertTo-ThemeHex '#00FF2EB5')), 1))
    $halo.Fill = $rb
    $halo.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5)
    $hs = New-Object System.Windows.Media.ScaleTransform 1, 1
    $halo.RenderTransform = $hs
    [System.Windows.Controls.Canvas]::SetLeft($halo, 20); [System.Windows.Controls.Canvas]::SetTop($halo, 20)
    [void]$cv.Children.Add($halo)
    Start-LoaderLoop $hs ([System.Windows.Media.ScaleTransform]::ScaleXProperty) 0.85 1.1 1600 $true
    Start-LoaderLoop $hs ([System.Windows.Media.ScaleTransform]::ScaleYProperty) 0.85 1.1 1600 $true

    # Anneau de fond et anneau de progression (dégradé cyan vers magenta)
    [void]$cv.Children.Add((New-LoaderArc $C 112 -90 359.9 (Get-Brush '#16FFFFFF') 6))
    $prog = New-LoaderArc $C 112 -90 0.1 (New-LinearBrush @($NexoCyan, $NexoMagenta) 0 0 1 1) 6
    $prog.Effect = New-Glow $NexoCyan 14 0.7
    [void]$cv.Children.Add($prog)
    # Petite comète magenta qui tourne en sens inverse, plus loin
    [void](New-LoaderSpinner $cv $C 124 60 $NexoMagenta 2 2600 $true)

    # Le N, au centre
    $mark = New-NexoMark 128
    [System.Windows.Controls.Canvas]::SetLeft($mark.Root, $C - 64); [System.Windows.Controls.Canvas]::SetTop($mark.Root, $C - 72)
    [void]$cv.Children.Add($mark.Root)

    # Pourcentage sous le N
    $txt = New-Text '0 %' 18 '#FFFFFF' -Bold
    $txt.Width = $S; $txt.TextAlignment = 'Center'; $txt.FontFamily = New-Object System.Windows.Media.FontFamily 'Cascadia Code, Consolas'
    [System.Windows.Controls.Canvas]::SetTop($txt, $C + 58)
    [void]$cv.Children.Add($txt)

    # Ligne de balayage qui descend en boucle (dans le cercle)
    $scanHost = New-Object System.Windows.Controls.Canvas
    $scanHost.Width = $S; $scanHost.Height = $S
    $scanHost.Clip = New-Object System.Windows.Media.EllipseGeometry ([System.Windows.Point]::new($C, $C)), 106, 106
    $scan = New-Object System.Windows.Shapes.Rectangle
    $scan.Width = $S; $scan.Height = 3
    $scan.Fill = New-LinearBrush @('#0000E5FF', '#8000E5FF', '#0000E5FF') 0 0 1 0
    $st = New-Object System.Windows.Media.TranslateTransform 0, 0
    $scan.RenderTransform = $st
    [void]$scanHost.Children.Add($scan)
    [void]$cv.Children.Add($scanHost)
    Start-LoaderLoop $st ([System.Windows.Media.TranslateTransform]::YProperty) 20 240 2200 $false

    [void]$lh.Children.Add($cv)
    $script:Loader.Arc = $prog; $script:Loader.Text = $txt; $script:Loader.Mark = $mark

    # Petits sauts réguliers du logo pendant le chargement
    $gt = New-Object System.Windows.Threading.DispatcherTimer
    $gt.Interval = [TimeSpan]::FromMilliseconds(1300)
    $gt.Add_Tick({ if ($script:Loader) { Start-NexoGlitch $script:Loader.Mark $null 260 } })
    $gt.Start()
    $script:Loader.GlitchTimer = $gt

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

# L'anneau se remplit, le pourcentage défile, et le logo saute à chaque étape
function Set-LoaderProgress([double]$Pct) {
    $L = $script:Loader
    if (-not $L) { return }
    if ($L.Kind -eq 'sprite') {
        Start-Anim { param($k, $s) if ($script:Loader -and $script:Loader.Kind -eq 'sprite') { Set-SpriteLoaderPos ($s.From + ($s.To - $s.From) * $k) } } @{ From = $L.Shown; To = $Pct } 700
        return
    }
    Start-NexoGlitch $L.Mark $null 300
    Start-Anim {
        param($k, $s)
        $L2 = $script:Loader
        if (-not $L2) { return }
        $v = $s.From + ($s.To - $s.From) * $k
        $L2.Shown = $v
        $L2.Arc.Data = Get-ArcGeometry 130 112 -90 ([math]::Max(0.1, [math]::Min(359.9, 3.6 * $v)))
        $L2.Text.Text = '{0:N0} %' -f $v
    } @{ From = $L.Shown; To = $Pct } 700
}

function Stop-StartupLoader {
    $L = $script:Loader
    if (-not $L) { return }
    foreach ($x in $L.Loops) { try { $x.T.BeginAnimation($x.P, $null) } catch {} }
    if ($L.GlitchTimer) { $L.GlitchTimer.Stop() }
    $script:Loader = $null
    $ui.StartupLoaderHost.Children.Clear()
}
# Écran de chargement d'un pack de thème : le personnage du pack court au-dessus d'une barre
# et avance avec la progression (planche PNG : images côte à côte, lues une à une).
function Start-SpriteLoader {
    $lh = $ui.StartupLoaderHost
    $L = $ThemePack.Loader
    $bi = New-Object System.Windows.Media.Imaging.BitmapImage
    $bi.BeginInit(); $bi.UriSource = New-Object Uri $L.File; $bi.CacheOption = 'OnLoad'; $bi.EndInit(); $bi.Freeze()
    $fw = [int]($bi.PixelWidth / $L.Frames); $fh = $bi.PixelHeight
    $frames = @(for ($i = 0; $i -lt $L.Frames; $i++) { $c = New-Object System.Windows.Media.Imaging.CroppedBitmap $bi, ([System.Windows.Int32Rect]::new($i * $fw, 0, $fw, $fh)); $c.Freeze(); $c })
    $W = 400.0; $barY = 168.0; $sh = 120.0; $sw = $sh * $fw / $fh
    $lh.Children.Clear()
    $lh.Width = $W; $lh.Height = 210
    $cv = New-Object System.Windows.Controls.Canvas
    $cv.Width = $W; $cv.Height = 210
    # Ombre sous le personnage, puis le personnage
    $shadow = New-Object System.Windows.Shapes.Ellipse
    $shadow.Width = $sw * 0.6; $shadow.Height = 8; $shadow.Fill = Get-Brush '#55000000'
    [System.Windows.Controls.Canvas]::SetTop($shadow, $barY - 7)
    [void]$cv.Children.Add($shadow)
    $img = New-Object System.Windows.Controls.Image
    $img.Width = $sw; $img.Height = $sh; $img.Source = $frames[0]
    if ($L.Flip) { $img.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5); $img.RenderTransform = New-Object System.Windows.Media.ScaleTransform -1, 1 }
    [System.Windows.Controls.Canvas]::SetTop($img, $barY - $sh + 4)
    [void]$cv.Children.Add($img)
    # Barre : piste translucide et remplissage aux couleurs du thème
    $track = New-Object System.Windows.Controls.Border
    $track.Width = $W; $track.Height = 14; $track.CornerRadius = [System.Windows.CornerRadius]::new(7)
    $track.Background = Get-Brush '#1EFFFFFF'; $track.BorderBrush = Get-Brush '#26FFFFFF'; $track.BorderThickness = New-Thickness 1 1 1 1
    [System.Windows.Controls.Canvas]::SetTop($track, $barY)
    [void]$cv.Children.Add($track)
    $fill = New-Object System.Windows.Controls.Border
    $fill.Width = 14; $fill.Height = 14; $fill.CornerRadius = [System.Windows.CornerRadius]::new(7)
    $fill.Background = $Window.FindResource('AccentBg')
    $fill.Effect = New-Glow '#00E5FF' 14 0.7
    [System.Windows.Controls.Canvas]::SetTop($fill, $barY)
    [void]$cv.Children.Add($fill)
    $txt = New-Text '0 %' 15 '#FFFFFF' -Bold
    $txt.Width = $W; $txt.TextAlignment = 'Center'
    $txt.FontFamily = New-Object System.Windows.Media.FontFamily $MonoFont
    Set-PackFont $txt 0.85
    [System.Windows.Controls.Canvas]::SetTop($txt, $barY + 22)
    [void]$cv.Children.Add($txt)
    [void]$lh.Children.Add($cv)
    $script:Loader = @{ Kind = 'sprite'; Loops = (New-Object System.Collections.ArrayList); Shown = 0.0; Img = $img; Shadow = $shadow; Fill = $fill; Text = $txt; Frames = $frames; Frame = 0; W = $W; SpriteW = $sw }
    Set-SpriteLoaderPos 0
    # Les images de la course défilent en boucle
    $t = New-Object System.Windows.Threading.DispatcherTimer
    $t.Interval = [TimeSpan]::FromMilliseconds($L.Delay)
    $t.Add_Tick({
        $S = $script:Loader
        if (-not $S -or $S.Kind -ne 'sprite') { return }
        $S.Frame = ($S.Frame + 1) % $S.Frames.Count
        $S.Img.Source = $S.Frames[$S.Frame]
    })
    $t.Start()
    $script:Loader.GlitchTimer = $t
}

# Position du personnage et longueur de la barre pour un pourcentage
function Set-SpriteLoaderPos([double]$V) {
    $S = $script:Loader
    $S.Shown = $V
    $w = [math]::Max(14.0, $S.W * $V / 100)
    $S.Fill.Width = $w
    $x = [math]::Max(0.0, [math]::Min($S.W - $S.SpriteW, $w - $S.SpriteW * 0.7))
    [System.Windows.Controls.Canvas]::SetLeft($S.Img, $x)
    [System.Windows.Controls.Canvas]::SetLeft($S.Shadow, $x + $S.SpriteW * 0.2)
    $S.Text.Text = '{0:N0} %' -f $V
}

# Logo d'un pack de thème (image) : il se secoue au survol et de temps en temps, comme une balle qui hésite
function Initialize-PackLogo($MarkHost, $WordHost) {
    $bi = New-Object System.Windows.Media.Imaging.BitmapImage
    $bi.BeginInit(); $bi.UriSource = New-Object Uri $ThemePack.Logo; $bi.CacheOption = 'OnLoad'; $bi.EndInit(); $bi.Freeze()
    $img = New-Object System.Windows.Controls.Image
    $img.Source = $bi; $img.Stretch = 'Uniform'
    [System.Windows.Media.RenderOptions]::SetBitmapScalingMode($img, 'HighQuality')
    $img.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.85)
    $rot = New-Object System.Windows.Media.RotateTransform 0
    $img.RenderTransform = $rot
    $MarkHost.Background = [System.Windows.Media.Brushes]::Transparent
    $MarkHost.Padding = New-Thickness 2 2 2 2
    $MarkHost.Child = $img
    $script:LogoMark = @{ Pack = $true; Img = $img; Rot = $rot }
    $script:LogoWord = New-NexoWord 22
    $WordHost.Children.Clear(); [void]$WordHost.Children.Add($script:LogoWord.Root)
    $shake = { Start-PackLogoShake }
    $MarkHost.Add_MouseEnter($shake); $WordHost.Add_MouseEnter($shake)
    $script:LogoTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:LogoTimer.Interval = [TimeSpan]::FromSeconds(7)
    $script:LogoTimer.Add_Tick({
        param($s, $e)
        $s.Interval = [TimeSpan]::FromSeconds((Get-Random -Minimum 6 -Maximum 13))
        if ($Window.IsVisible -and $Window.IsActive) { Start-PackLogoShake }
    })
    $script:LogoTimer.Start()
}

function Start-PackLogoShake {
    $m = $script:LogoMark
    if (-not $m -or -not $m.Pack) { return }
    $a = New-Object System.Windows.Media.Animation.DoubleAnimationUsingKeyFrames
    foreach ($k in @(@(0, 0), @(110, -22), @(240, 18), @(360, -12), @(470, 6), @(580, 0))) {
        [void]$a.KeyFrames.Add((New-Object System.Windows.Media.Animation.EasingDoubleKeyFrame ([double]$k[1]), ([System.Windows.Media.Animation.KeyTime]::FromTimeSpan([TimeSpan]::FromMilliseconds($k[0])))))
    }
    $m.Rot.BeginAnimation([System.Windows.Media.RotateTransform]::AngleProperty, $a)
}

# Planche PNG (images côte à côte) découpée en images prêtes à afficher
function Get-SheetFrames([string]$File, [int]$Count) {
    $bi = New-Object System.Windows.Media.Imaging.BitmapImage
    $bi.BeginInit(); $bi.UriSource = New-Object Uri $File; $bi.CacheOption = 'OnLoad'; $bi.EndInit(); $bi.Freeze()
    $fw = [int]($bi.PixelWidth / $Count)
    @(for ($i = 0; $i -lt $Count; $i++) { $c = New-Object System.Windows.Media.Imaging.CroppedBitmap $bi, ([System.Windows.Int32Rect]::new($i * $fw, 0, $fw, $bi.PixelHeight)); $c.Freeze(); $c })
}

# Dessin animé d'un pack au centre de la barre du bas (pixels nets, sans flou d'agrandissement)
function Initialize-PackFooter {
    $f = if ($ThemePack) { $ThemePack.Footer } else { $null }
    if (-not $f) { return }
    $frames = Get-SheetFrames $f.File $f.Frames
    $img = New-Object System.Windows.Controls.Image
    $img.Source = $frames[0]; $img.Height = $f.Height; $img.Stretch = 'Uniform'
    # Pixels bruts à la taille d'origine ou agrandi ; lissage de qualité si le dessin est réduit (sinon pixels irréguliers)
    [System.Windows.Media.RenderOptions]::SetBitmapScalingMode($img, $(if ($f.Height -ge $frames[0].PixelHeight) { 'NearestNeighbor' } else { 'HighQuality' }))
    $ui.FooterArt.Child = $img
    $ui.FooterArt.Visibility = 'Visible'
    $ui.StatusBar.Padding = New-Thickness 36 2 36 6
    $script:Footer = @{ Img = $img; Frames = $frames; Frame = 0 }
    $t = New-Object System.Windows.Threading.DispatcherTimer
    $t.Interval = [TimeSpan]::FromMilliseconds($f.Delay)
    $t.Add_Tick({
        $F = $script:Footer
        if (-not $F -or -not $Window.IsVisible) { return }
        $F.Frame = ($F.Frame + 1) % $F.Frames.Count
        $F.Img.Source = $F.Frames[$F.Frame]
    })
    $t.Start()
    $script:Footer.Timer = $t
    # Le texte d'état reste dans la moitié gauche, jamais sous le dessin
    $fit = { if ($script:Footer) { $ui.StatusText.MaxWidth = [math]::Max(80.0, ($ui.StatusBar.ActualWidth - 72 - $script:Footer.Img.ActualWidth) / 2 - 16) } }
    $ui.StatusBar.Add_SizeChanged($fit)
    $script:Footer.Img.Add_SizeChanged($fit)
}

# Icônes animées des onglets du haut (pack de thème) : première image au repos, animation au survol de l'onglet
function Initialize-PackTabIcons {
    if (-not $ThemePack -or -not $ThemePack.TabIcons -or -not $ThemePack.TabIcons.Count) { return }
    $tabs = @{ ordinateur = $HubIndex; jeux = $GamesIndex; reseau = $NetIndex; trafic = $TrafficIndex; overlay = $OverlayIndex }
    $script:TabIcons = @{}
    foreach ($k in @($ThemePack.TabIcons.Keys)) {
        if (-not $tabs.ContainsKey($k)) { continue }
        $ti = $ui.Tabs.Items[$tabs[$k]]
        $sp = $ti.Header
        if (-not ($sp -is [System.Windows.Controls.StackPanel]) -or -not $sp.Children.Count) { continue }
        $def = $ThemePack.TabIcons[$k]
        $frames = Get-SheetFrames $def.File $def.Frames
        $img = New-Object System.Windows.Controls.Image
        $img.Source = $frames[0]; $img.Height = 24; $img.Stretch = 'Uniform'
        $img.Margin = New-Thickness 0 -4 6 -4; $img.VerticalAlignment = 'Center'
        [System.Windows.Media.RenderOptions]::SetBitmapScalingMode($img, 'HighQuality')
        $sp.Children.RemoveAt(0)
        $sp.Children.Insert(0, $img)
        $t = New-Object System.Windows.Threading.DispatcherTimer
        $t.Interval = [TimeSpan]::FromMilliseconds($def.Delay)
        $st = @{ Img = $img; Frames = $frames; Frame = 0; Timer = $t }
        $t.Tag = $st
        $t.Add_Tick({ param($s, $e) $x = $s.Tag; $x.Frame = ($x.Frame + 1) % $x.Frames.Count; $x.Img.Source = $x.Frames[$x.Frame] })
        $script:TabIcons[$k] = $st
        $ti.Add_MouseEnter({ param($s, $e) $x = Get-TabIconState $s; if ($x) { $x.Timer.Start() } })
        $ti.Add_MouseLeave({ param($s, $e) $x = Get-TabIconState $s; if ($x) { $x.Timer.Stop(); $x.Frame = 0; $x.Img.Source = $x.Frames[0] } })
    }
}

function Get-TabIconState($TabItem) {
    if (-not $script:TabIcons) { return $null }
    foreach ($x in $script:TabIcons.Values) { if ($TabItem.Header.Children.Contains($x.Img)) { return $x } }
    $null
}
