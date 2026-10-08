# Nexo : carte du réseau en constellation. La box au centre comme un astre, Internet au dessus,
# les appareils en orbes lumineux regroupés par familles, reliés par des liaisons que parcourent
# des impulsions (plus le ping est court, plus elles vont vite). Un clic ouvre la fiche d'un appareil.
# Chargé par OptiGame.ps1 après reseau.ps1 et reseau-avance.ps1.

$MapFamilies = @(
    @{ Id = 'pc'; Label = 'PC et consoles'; Color = '#4EA8FF'; Kinds = @('Ce PC', 'Ordinateur', 'Console de jeu') },
    @{ Id = 'net'; Label = 'Réseau'; Color = '#2EE6C8'; Kinds = @('Routeur ou répéteur Wi-Fi') },
    @{ Id = 'media'; Label = 'TV et multimédia'; Color = '#FF7AB6'; Kinds = @('TV ou multimédia', 'Enceinte ou audio', 'Box ou décodeur TV') },
    @{ Id = 'mobile'; Label = 'Téléphones et tablettes'; Color = '#B18CFF'; Kinds = @('Téléphone ou tablette', 'Téléphone probable', 'Appareil Apple') },
    @{ Id = 'iot'; Label = 'Objets connectés'; Color = '#FFB547'; Kinds = @('Objet connecté', 'Caméra', 'Imprimante') },
    @{ Id = 'other'; Label = 'Autres'; Color = '#9AA3B2'; Kinds = @() }
)

function Get-MapFamily($D) {
    foreach ($f in $MapFamilies) { if ($f.Kinds -contains [string]$D.KindInfo.Kind) { return $f } }
    $MapFamilies[-1]
}

# Animation en boucle de la carte, notée pour être arrêtée quand la carte se ferme
function Start-MapLoop($Target, $Property, $Anim) {
    $Anim.RepeatBehavior = [System.Windows.Media.Animation.RepeatBehavior]::Forever
    $Target.BeginAnimation($Property, $Anim)
    [void]$script:MapLoops.Add(@{ T = $Target; P = $Property })
}

function Stop-NetMapAnims {
    if (-not $script:MapLoops) { return }
    foreach ($x in $script:MapLoops) { try { $x.T.BeginAnimation($x.P, $null) } catch {} }
    $script:MapLoops.Clear()
}

function New-MapDouble([double]$From, [double]$To, [int]$Ms, [bool]$Reverse, [int]$Delay = 0) {
    $a = New-Object System.Windows.Media.Animation.DoubleAnimation
    $a.From = $From; $a.To = $To; $a.AutoReverse = $Reverse
    $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds($Ms))
    $a.BeginTime = [TimeSpan]::FromMilliseconds($Delay)
    if ($Reverse) { $e = New-Object System.Windows.Media.Animation.SineEase; $e.EasingMode = 'EaseInOut'; $a.EasingFunction = $e }
    $a
}

# Dégradé radial « orbe » : reflet clair en haut à gauche, couleur, bord sombre
function New-OrbBrush([string]$Hex) {
    $b = New-Object System.Windows.Media.RadialGradientBrush
    $b.GradientOrigin = [System.Windows.Point]::new(0.35, 0.3)
    [void]$b.GradientStops.Add([System.Windows.Media.GradientStop]::new((Get-Color (Get-LightHex $Hex 0.55)), 0))
    [void]$b.GradientStops.Add([System.Windows.Media.GradientStop]::new((Get-Color $Hex), 0.55))
    $c = Get-Color $Hex
    [void]$b.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.Color]::FromRgb([byte]($c.R * 0.35), [byte]($c.G * 0.35), [byte]($c.B * 0.35)), 1))
    $b
}

# Halo : cercle de couleur qui s'efface vers l'extérieur (lueur sans effet coûteux)
function New-HaloBrush([string]$Hex, [byte]$Alpha) {
    $c = Get-Color $Hex
    $b = New-Object System.Windows.Media.RadialGradientBrush
    [void]$b.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.Color]::FromArgb($Alpha, $c.R, $c.G, $c.B), 0))
    [void]$b.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.Color]::FromArgb(0, $c.R, $c.G, $c.B), 1))
    $b
}

function Add-MapAt($Canvas, $El, [double]$X, [double]$Y, [int]$Z = 0) {
    [System.Windows.Controls.Canvas]::SetLeft($El, $X); [System.Windows.Controls.Canvas]::SetTop($El, $Y)
    if ($Z) { [System.Windows.Controls.Panel]::SetZIndex($El, $Z) }
    [void]$Canvas.Children.Add($El)
}

# Liaison courbe et dégradée de la box vers un appareil, et ses impulsions de lumière
function Add-MapLink($Canvas, [double]$X1, [double]$Y1, [double]$X2, [double]$Y2, [string]$Color, [bool]$Dashed, $Ms, [int]$Seed) {
    $dx = $X2 - $X1; $dy = $Y2 - $Y1; $len = [math]::Sqrt($dx * $dx + $dy * $dy)
    $cx = ($X1 + $X2) / 2 - $dy * 0.07; $cy = ($Y1 + $Y2) / 2 + $dx * 0.07
    $fig = New-Object System.Windows.Media.PathFigure
    $fig.StartPoint = [System.Windows.Point]::new($X1, $Y1)
    [void]$fig.Segments.Add([System.Windows.Media.QuadraticBezierSegment]::new([System.Windows.Point]::new($cx, $cy), [System.Windows.Point]::new($X2, $Y2), $true))
    $geo = New-Object System.Windows.Media.PathGeometry
    [void]$geo.Figures.Add($fig)
    $col = Get-Color $Color
    $br = New-Object System.Windows.Media.LinearGradientBrush
    $br.MappingMode = 'Absolute'
    $br.StartPoint = [System.Windows.Point]::new($X1, $Y1); $br.EndPoint = [System.Windows.Point]::new($X2, $Y2)
    [void]$br.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.Color]::FromArgb(25, $col.R, $col.G, $col.B), 0))
    [void]$br.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.Color]::FromArgb(170, $col.R, $col.G, $col.B), 1))
    $p = New-Object System.Windows.Shapes.Path
    $p.Data = $geo; $p.Stroke = $br; $p.StrokeThickness = 1.6
    if ($Dashed) { $p.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(3, 4)) }
    [void]$Canvas.Children.Add($p)
    # Impulsions : une lueur qui glisse le long de la liaison (vitesse selon le ping)
    if (-not $Dashed) {
        $ms = if ($null -ne $Ms) { [math]::Min(60.0, [double]$Ms) } else { 20.0 }
        $dur = [int](900 + $len * 2.2 + $ms * 40)
        foreach ($k in 0, 1) {
            $eg = New-Object System.Windows.Media.EllipseGeometry ([System.Windows.Point]::new($X1, $Y1)), 5, 5
            $dot = New-Object System.Windows.Shapes.Path
            $dot.Data = $eg
            $db = New-Object System.Windows.Media.RadialGradientBrush
            [void]$db.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.Colors]::White, 0))
            [void]$db.GradientStops.Add([System.Windows.Media.GradientStop]::new($col, 0.45))
            [void]$db.GradientStops.Add([System.Windows.Media.GradientStop]::new([System.Windows.Media.Color]::FromArgb(0, $col.R, $col.G, $col.B), 1))
            $dot.Fill = $db
            [void]$Canvas.Children.Add($dot)
            $pa = New-Object System.Windows.Media.Animation.PointAnimationUsingPath
            $pa.PathGeometry = $geo
            $pa.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds($dur))
            $pa.BeginTime = [TimeSpan]::FromMilliseconds((($Seed * 397) % $dur) + $k * $dur / 2)
            Start-MapLoop $eg ([System.Windows.Media.EllipseGeometry]::CenterProperty) $pa
        }
    }
    $p
}

# Un appareil : orbe lumineux avec son icône, étiquette en verre dessous
function New-MapNode($Canvas, [string]$Title, [string]$Sub, [int]$Glyph, [string]$Color, [double]$X, [double]$Y, [double]$Size, [string]$Ring, $Device, $Link, [int]$Delay, [bool]$Dim) {
    $W = if ($Size -le 40) { 124.0 } else { 170.0 }
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Width = $W
    $sp.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5 * $Size / ($Size + 48.0))
    $zoom = New-Object System.Windows.Media.ScaleTransform 0, 0
    $sp.RenderTransform = $zoom
    $orbBox = New-Object System.Windows.Controls.Grid
    $orbBox.Width = $Size * 2; $orbBox.Height = $Size * 2; $orbBox.Margin = New-Thickness 0 (-$Size / 2) 0 (-$Size / 2)
    $halo = New-Object System.Windows.Shapes.Ellipse
    $halo.Fill = New-HaloBrush $Color 110
    [void]$orbBox.Children.Add($halo)
    if ($Ring) {
        $rg = New-Object System.Windows.Shapes.Ellipse
        $rg.Width = $Size + 12; $rg.Height = $Size + 12
        $rg.Stroke = Get-Brush $Ring; $rg.StrokeThickness = 2
        $rg.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(3, 2))
        [void]$orbBox.Children.Add($rg)
        Start-MapLoop $rg ([System.Windows.UIElement]::OpacityProperty) (New-MapDouble 1 0.25 900 $true)
    }
    $orb = New-Object System.Windows.Shapes.Ellipse
    $orb.Width = $Size; $orb.Height = $Size
    $orb.Fill = New-OrbBrush $Color
    $orb.Stroke = New-AlphaBrush (Get-LightHex $Color 0.5) 200; $orb.StrokeThickness = 1.5
    [void]$orbBox.Children.Add($orb)
    $gl = New-Object System.Windows.Controls.TextBlock
    $gl.Text = [string][char]$Glyph
    $gl.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
    $gl.FontSize = $Size * 0.4; $gl.Foreground = Get-Brush '#FFFFFF'
    $gl.HorizontalAlignment = 'Center'; $gl.VerticalAlignment = 'Center'
    [void]$orbBox.Children.Add($gl)
    [void]$sp.Children.Add($orbBox)
    # Étiquette en verre
    $pill = New-Object System.Windows.Controls.Border
    $pill.Background = Get-Brush '#D90E1219'; $pill.BorderBrush = New-AlphaBrush $Color 90; $pill.BorderThickness = New-Thickness 1 1 1 1
    $pill.CornerRadius = [System.Windows.CornerRadius]::new(9); $pill.Padding = New-Thickness 9 3 9 4
    $pill.HorizontalAlignment = 'Center'; $pill.Margin = New-Thickness 0 6 0 0
    $ls = New-Object System.Windows.Controls.StackPanel
    $t = New-Text $Title 12.5 '#FFFFFF' -Semi
    $t.TextAlignment = 'Center'; $t.TextTrimming = 'CharacterEllipsis'; $t.TextWrapping = 'NoWrap'; $t.MaxWidth = $W - 20
    [void]$ls.Children.Add($t)
    if ($Sub) {
        $s = New-Text $Sub 10.5 '#8B95A7'
        $s.TextAlignment = 'Center'; $s.TextTrimming = 'CharacterEllipsis'; $s.TextWrapping = 'NoWrap'; $s.MaxWidth = $W - 20
        [void]$ls.Children.Add($s)
    }
    $pill.Child = $ls
    [void]$sp.Children.Add($pill)
    if ($Dim) { $orbBox.Opacity = 0.6 }
    Add-MapAt $Canvas $sp ($X - $W / 2) ($Y - $Size / 2) 10
    # Apparition : l'appareil jaillit de la box (avec un léger rebond)
    Start-Anim { param($k, $z) $v = $k + 0.18 * [math]::Sin([math]::PI * $k); $z.ScaleX = $v; $z.ScaleY = $v } $zoom 560 $Delay
    if ($Device) {
        $d = $Device
        $sp.Cursor = [System.Windows.Input.Cursors]::Hand
        $tip = @($d.Title, $d.KindInfo.Kind, "Adresse : $($d.Ip)")
        if ($d.Mac) { $tip += "Adresse physique : $($d.Mac)" }
        if ($d.Vendor) { $tip += "Fabricant : $($d.Vendor)" }
        if ($d.Model) { $tip += "Modèle : $(if ($d.Maker) { $d.Maker + ' ' })$($d.Model)" }
        if ($null -ne $d.Ms) { $tip += "Ping : $(if ($d.Ms -lt 1) { 'moins de 1 ms' } else { "$($d.Ms) ms" })" }
        if ($d.Hidden) { $tip += 'Discret : ne répond pas au ping' }
        if ($d.New) { $tip += 'Nouveau depuis le dernier scan' }
        if ($d.Camera) { $tip += 'Caméra possible' }
        $tip += 'Clique pour tout voir'
        $sp.ToolTip = $tip -join "`n"
        $sp.Tag = @{ D = $d; Zoom = $zoom; Link = $Link; Orb = $orb; Color = $Color }
        # Survol : l'orbe grossit et sa liaison s'illumine
        $sp.Add_MouseEnter({
            param($s, $e)
            Start-FromTo $s.Tag.Zoom ([System.Windows.Media.ScaleTransform]::ScaleXProperty) 1 1.15 180
            Start-FromTo $s.Tag.Zoom ([System.Windows.Media.ScaleTransform]::ScaleYProperty) 1 1.15 180
            $s.Tag.Orb.Effect = New-Glow $s.Tag.Color 26 0.9
            if ($s.Tag.Link) { $s.Tag.Link.StrokeThickness = 3.2; $s.Tag.Link.Effect = New-Glow $s.Tag.Color 10 0.9 }
        })
        $sp.Add_MouseLeave({
            param($s, $e)
            Start-FromTo $s.Tag.Zoom ([System.Windows.Media.ScaleTransform]::ScaleXProperty) 1.15 1 220
            Start-FromTo $s.Tag.Zoom ([System.Windows.Media.ScaleTransform]::ScaleYProperty) 1.15 1 220
            $s.Tag.Orb.Effect = $null
            if ($s.Tag.Link) { $s.Tag.Link.StrokeThickness = 1.6; $s.Tag.Link.Effect = $null }
        })
        $sp.Add_MouseLeftButtonUp({ param($s, $e) Invoke-Safe { Show-DeviceDetail $s.Tag.D } })
    }
    $sp
}

function Show-NetMap {
    $list = @($script:NetList)
    if (-not $list.Count) { Set-Status 'Lance d''abord un scan du réseau.'; return }
    # Carte déjà ouverte et appareils inchangés (ex : filtre de la liste changé) : rien à redessiner
    $sig = ($list | ForEach-Object { "$($_.Ip)|$($_.Title)|$([bool]$_.Hidden)|$([bool]$_.New)|$([bool]$_.Camera)" }) -join ';'
    if ($ui.NetMapOverlay.Visibility -eq 'Visible' -and $sig -eq $script:MapSig) { return }
    $script:MapSig = $sig
    if (-not $script:MapLoops) { $script:MapLoops = New-Object System.Collections.ArrayList }
    Stop-NetMapAnims
    if ($ui.NetMapOverlay.Visibility -ne 'Visible') {
        $ui.NetMapOverlay.Visibility = 'Visible'
        $ui.NetMapOverlay.Opacity = 0
        Start-WpfAnim $ui.NetMapOverlay ([System.Windows.UIElement]::OpacityProperty) 1 250
    }
    $cv = $ui.NetMapCanvas
    $cv.Children.Clear()
    $W = [double]$cv.Width; $H = [double]$cv.Height
    $bx = $W / 2; $by = 430.0
    $rnd = New-Object System.Random 7

    # Ciel : dégradé bleu nuit et étoiles (quelques unes scintillent)
    $sky = New-Object System.Windows.Shapes.Rectangle
    $sky.Width = $W; $sky.Height = $H; $sky.RadiusX = 16; $sky.RadiusY = 16
    $sb = New-Object System.Windows.Media.RadialGradientBrush
    $sb.Center = [System.Windows.Point]::new(0.5, 0.55); $sb.GradientOrigin = [System.Windows.Point]::new(0.5, 0.55)
    $sb.RadiusX = 0.7; $sb.RadiusY = 0.8
    [void]$sb.GradientStops.Add([System.Windows.Media.GradientStop]::new((Get-Color '#15233A'), 0))
    [void]$sb.GradientStops.Add([System.Windows.Media.GradientStop]::new((Get-Color '#0B0F18'), 1))
    $sky.Fill = $sb
    Add-MapAt $cv $sky 0 0
    for ($i = 0; $i -lt 110; $i++) {
        $st = New-Object System.Windows.Shapes.Ellipse
        $sz = 1 + $rnd.NextDouble() * 1.8
        $st.Width = $sz; $st.Height = $sz; $st.Fill = Get-Brush '#FFFFFF'
        $st.Opacity = 0.08 + $rnd.NextDouble() * 0.45
        Add-MapAt $cv $st ($rnd.NextDouble() * $W) ($rnd.NextDouble() * $H)
        if ($i % 9 -eq 0) { Start-MapLoop $st ([System.Windows.UIElement]::OpacityProperty) (New-MapDouble 0.1 0.9 (1200 + $rnd.Next(1800)) $true ($rnd.Next(2000))) }
    }

    # Orbites en pointillés autour de la box
    $orbits = @(@{ Rx = 285.0; Ry = 170.0 }, @{ Rx = 470.0; Ry = 272.0 })
    foreach ($o in $orbits) {
        $e = New-Object System.Windows.Shapes.Ellipse
        $e.Width = 2 * $o.Rx; $e.Height = 2 * $o.Ry
        $e.Stroke = Get-Brush '#26324A'; $e.StrokeThickness = 1
        $e.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(2, 5))
        Add-MapAt $cv $e ($bx - $o.Rx) ($by - $o.Ry)
    }

    # Familles : un secteur par famille, proportionnel à son nombre d'appareils
    $gw = @($list | Where-Object { $_.Gateway })[0]
    $others = @($list | Where-Object { -not $_.Gateway })
    $groups = @()
    foreach ($f in $MapFamilies) {
        $ds = @($others | Where-Object { (Get-MapFamily $_).Id -eq $f.Id } | Sort-Object @{ Expression = { -not $_.Self } }, @{ Expression = { $_.Title } })
        if ($ds.Count) { $groups += @{ F = $f; D = $ds } }
    }
    $n = $others.Count
    $gap = 10.0; $span = 316.0 - $gap * [math]::Max(0, $groups.Count - 1)
    $angle = 292.0
    $placed = @()
    foreach ($grp in $groups) {
        $width = [math]::Max(26.0, $span * $grp.D.Count / [math]::Max(1, $n))
        # Étiquette de la famille, au bord extérieur de son secteur
        $mid = ($angle + $width / 2) * [math]::PI / 180
        $lab = New-Text $grp.F.Label.ToUpper() 11.5 $grp.F.Color -Semi
        $lab.Opacity = 0.85; $lab.Width = 200; $lab.TextAlignment = 'Center'
        Add-MapAt $cv $lab ([math]::Min($W - 200.0, [math]::Max(0.0, $bx + 575 * [math]::Cos($mid) - 100))) ([math]::Min($H - 22.0, [math]::Max(8.0, $by + 345 * [math]::Sin($mid) - 8))) 5
        # Lueur du secteur
        $wedge = New-Object System.Windows.Shapes.Ellipse
        $wedge.Width = 360; $wedge.Height = 260; $wedge.Fill = New-HaloBrush $grp.F.Color 26
        Add-MapAt $cv $wedge ($bx + 380 * [math]::Cos($mid) - 180) ($by + 228 * [math]::Sin($mid) - 130)
        for ($i = 0; $i -lt $grp.D.Count; $i++) {
            $a = $angle + $width * ($i + 0.5) / $grp.D.Count
            # Deux orbites en alternance quand la famille est nombreuse
            $orb = if ($grp.D.Count -ge 4 -and $i % 2 -eq 1) { $orbits[0] } else { $orbits[1] }
            if ($n -le 6) { $orb = $orbits[1] }
            $placed += @{ D = $grp.D[$i]; F = $grp.F; A = $a; O = $orb }
        }
        $angle += $width + $gap
    }

    # Internet au dessus de la box
    $pub = if ($script:PublicIp) { $script:PublicIp } else { '' }
    [void](Add-MapLink $cv $bx $by $bx 82 $Colors.info $false 12 3)
    [void](New-MapNode $cv 'Internet' $pub 0xE12B $Colors.info $bx 82 52 '' $null $null 0 $false)

    # Appareils
    $size = if ($n -le 14) { 52 } elseif ($n -le 30) { 44 } else { 38 }
    $i = 0
    foreach ($p in $placed) {
        $d = $p.D; $f = $p.F
        $ar = $p.A * [math]::PI / 180
        $x = $bx + $p.O.Rx * [math]::Cos($ar); $y = $by + $p.O.Ry * [math]::Sin($ar)
        $link = Add-MapLink $cv $bx $by $x $y $f.Color ([bool]$d.Hidden) $d.Ms ($i + 1)
        $ring = if ($d.New -or $d.Camera) { $Colors.warn } elseif ($d.Self) { '#FFFFFF' } else { '' }
        $sub = if ($d.Self) { 'Ce PC' } elseif ($d.Hidden) { "$($d.Ip), discret" } elseif ($null -ne $d.Ms) { "$($d.Ip), $(if ($d.Ms -lt 1) { '< 1' } else { $d.Ms }) ms" } else { $d.Ip }
        [void](New-MapNode $cv $d.Title $sub $d.KindInfo.Glyph $f.Color $x $y $(if ($f.Id -eq 'net' -or $d.Self) { $size + 8 } else { $size }) $ring $d $link (180 + 45 * $i) ([bool]$d.Hidden))
        $i++
    }

    # La box : astre central, halo qui respire et anneau qui tourne
    $core = New-Object System.Windows.Controls.Canvas
    $bh = New-Object System.Windows.Shapes.Ellipse
    $bh.Width = 240; $bh.Height = 240; $bh.Fill = New-HaloBrush $Colors.ok 120
    $bh.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5)
    $bhs = New-Object System.Windows.Media.ScaleTransform 1, 1
    $bh.RenderTransform = $bhs
    Add-MapAt $core $bh ($bx - 120) ($by - 120)
    Start-MapLoop $bhs ([System.Windows.Media.ScaleTransform]::ScaleXProperty) (New-MapDouble 0.85 1.1 1800 $true)
    Start-MapLoop $bhs ([System.Windows.Media.ScaleTransform]::ScaleYProperty) (New-MapDouble 0.85 1.1 1800 $true)
    $spin = New-Object System.Windows.Shapes.Ellipse
    $spin.Width = 124; $spin.Height = 124
    $spin.Stroke = New-AlphaBrush $Colors.ok 150; $spin.StrokeThickness = 2
    $spin.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(6, 5))
    $spin.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5)
    $srot = New-Object System.Windows.Media.RotateTransform 0
    $spin.RenderTransform = $srot
    Add-MapAt $core $spin ($bx - 62) ($by - 62)
    Start-MapLoop $srot ([System.Windows.Media.RotateTransform]::AngleProperty) (New-MapDouble 0 360 14000 $false)
    [System.Windows.Controls.Panel]::SetZIndex($core, 8)
    [void]$cv.Children.Add($core)
    $gTitle = if ($gw) { $gw.Title } else { 'Box Internet' }
    [void](New-MapNode $cv $gTitle $(if ($gw) { $gw.Ip } else { '' }) 0xE80F $Colors.ok $bx $by 86 '' $gw $null 0 $false)

    # Légende : familles et nombre d'appareils
    $ui.NetMapLegend.Children.Clear()
    foreach ($grp in $groups) {
        $pb = New-Object System.Windows.Controls.Border
        $pb.Background = New-AlphaBrush $grp.F.Color 28; $pb.BorderBrush = New-AlphaBrush $grp.F.Color 110; $pb.BorderThickness = New-Thickness 1 1 1 1
        $pb.CornerRadius = [System.Windows.CornerRadius]::new(12); $pb.Padding = New-Thickness 12 4 12 4; $pb.Margin = New-Thickness 0 0 8 4
        $pb.Child = New-Text "●  $($grp.F.Label)  $($grp.D.Count)" 12 $grp.F.Color -Semi
        [void]$ui.NetMapLegend.Children.Add($pb)
    }
    $hid = @($list | Where-Object { $_.Hidden }).Count
    $new = @($list | Where-Object { $_.New }).Count
    $when = if ($script:NetScanAt) { " à $($script:NetScanAt.ToString('HH:mm'))" } else { '' }
    $hidNames = @($list | Where-Object { $_.Hidden } | Select-Object -First 4 | ForEach-Object { "$($_.Title) ($($_.Ip))" })
    $ui.NetMapSub.Text = "$($list.Count) appareils trouvés au dernier scan$when$(if ($new) { ", $new nouveau$(if ($new -gt 1) {'x'})" }). Les impulsions vont d'autant plus vite que l'appareil répond vite. $(if ($hid) { "Discret$(if ($hid -gt 1) {'s'}) (liaison en pointillés) : $($hidNames -join ', ')$(if ($hid -gt 4) { '...' }). " })Clique sur un appareil pour tout voir."
}

function Hide-NetMap {
    $ui.NetMapOverlay.Visibility = 'Collapsed'
    $script:MapSig = $null
    Stop-NetMapAnims
}
