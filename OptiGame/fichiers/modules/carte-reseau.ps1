# OptiGame : carte du réseau en grand. Internet en haut, la box au centre, chaque appareil autour,
# relié à la box. Un clic sur un appareil ouvre sa fiche (tout ce que le scan a trouvé).
# Chargé par OptiGame.ps1 après reseau.ps1 et reseau-avance.ps1.

$MapKindOrder = @('Ce PC', 'Ordinateur', 'Routeur ou répéteur Wi-Fi', 'Box ou décodeur TV', 'TV ou multimédia', 'Console de jeu', 'Enceinte ou audio',
    'Téléphone ou tablette', 'Téléphone probable', 'Appareil Apple', 'Imprimante', 'Caméra', 'Objet connecté', 'Appareil')

function New-MapGlyph([int]$Code, [double]$Size, [string]$Color) {
    $t = New-Object System.Windows.Controls.TextBlock
    $t.Text = [string][char]$Code
    $t.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
    $t.FontSize = $Size; $t.Foreground = Get-Brush $Color
    $t.HorizontalAlignment = 'Center'; $t.VerticalAlignment = 'Center'
    $t
}

# Un appareil : rond avec son icône, son nom et son adresse dessous
function New-MapNode($Canvas, [string]$Title, [string]$Sub, [int]$Glyph, [string]$Color, [double]$X, [double]$Y, [double]$Size, [string]$Ring, $Device, [double]$Width, [array]$Tags) {
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Width = $Width
    $c = New-Object System.Windows.Controls.Border
    $c.Width = $Size; $c.Height = $Size
    $c.CornerRadius = [System.Windows.CornerRadius]::new($Size / 2)
    $c.Background = Get-Brush '#11151C'
    $c.BorderBrush = Get-Brush $(if ($Ring) { $Ring } else { $Color })
    $c.BorderThickness = New-Thickness $(if ($Ring) { 3 } else { 1.5 }) $(if ($Ring) { 3 } else { 1.5 }) $(if ($Ring) { 3 } else { 1.5 }) $(if ($Ring) { 3 } else { 1.5 })
    $c.HorizontalAlignment = 'Center'
    # Fond opaque (les liaisons passent dessous), puis la couleur du type par dessus
    $fill = New-Object System.Windows.Controls.Border
    $fill.CornerRadius = [System.Windows.CornerRadius]::new($Size / 2)
    $bg = Get-Brush $Color; $bg.Opacity = 0.16
    $fill.Background = $bg
    $fill.Child = New-MapGlyph $Glyph ($Size * 0.42) $Color
    $c.Child = $fill
    [void]$sp.Children.Add($c)
    $t = New-Text $Title $(if ($Width -lt 140) { 12 } else { 13 }) '#FFFFFF' -Semi
    $t.TextAlignment = 'Center'; $t.TextTrimming = 'CharacterEllipsis'; $t.TextWrapping = 'NoWrap'; $t.Margin = New-Thickness 0 5 0 0
    $t.HorizontalAlignment = 'Center'; $t.MaxWidth = $Width; $t.Background = Get-Brush '#11151C'; $t.Padding = New-Thickness 4 0 4 0
    [void]$sp.Children.Add($t)
    if ($Sub) {
        $s = New-Text $Sub 11 '#7C8596'
        $s.TextAlignment = 'Center'; $s.TextTrimming = 'CharacterEllipsis'; $s.TextWrapping = 'NoWrap'
        $s.HorizontalAlignment = 'Center'; $s.MaxWidth = $Width; $s.Background = Get-Brush '#11151C'; $s.Padding = New-Thickness 4 0 4 1
        [void]$sp.Children.Add($s)
    }
    if ($Tags.Count) {
        $wp = New-Object System.Windows.Controls.WrapPanel
        $wp.HorizontalAlignment = 'Center'; $wp.Margin = New-Thickness 0 2 0 0
        foreach ($tg in $Tags) { $bd = New-Badge $tg[0] $tg[1]; $bd.Margin = New-Thickness 2 0 2 0; $bd.Background = Get-Brush '#1E2230'; [void]$wp.Children.Add($bd) }
        [void]$sp.Children.Add($wp)
    }
    [System.Windows.Controls.Canvas]::SetLeft($sp, $X - $Width / 2)
    [System.Windows.Controls.Canvas]::SetTop($sp, $Y - $Size / 2)
    [System.Windows.Controls.Panel]::SetZIndex($sp, 10)
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
        $sp.Tag = @{ D = $d; C = $c; Color = $Color }
        $sp.Add_MouseEnter({ param($s, $e) $s.Tag.C.Effect = New-Glow $s.Tag.Color 22 0.9; $b = Get-Brush $s.Tag.Color; $b.Opacity = 0.32; $s.Tag.C.Child.Background = $b })
        $sp.Add_MouseLeave({ param($s, $e) $s.Tag.C.Effect = $null; $b = Get-Brush $s.Tag.Color; $b.Opacity = 0.16; $s.Tag.C.Child.Background = $b })
        $sp.Add_MouseLeftButtonUp({ param($s, $e) Invoke-Safe { Show-DeviceDetail $s.Tag.D } })
    }
    [void]$Canvas.Children.Add($sp)
}

function Add-MapLink($Canvas, [double]$X1, [double]$Y1, [double]$X2, [double]$Y2, [string]$Color, [bool]$Dashed, [string]$Label) {
    $l = New-Object System.Windows.Shapes.Line
    $l.X1 = $X1; $l.Y1 = $Y1; $l.X2 = $X2; $l.Y2 = $Y2
    $b = Get-Brush $Color; $b.Opacity = 0.45
    $l.Stroke = $b; $l.StrokeThickness = 1.6
    if ($Dashed) { $l.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(4, 4)) }
    [void]$Canvas.Children.Add($l)
    if ($Label) {
        $t = New-Text $Label 10.5 '#7C8596'
        $bd = New-Object System.Windows.Controls.Border
        $bd.Background = Get-Brush '#11151C'; $bd.Padding = New-Thickness 4 0 4 0; $bd.CornerRadius = [System.Windows.CornerRadius]::new(4)
        $bd.Child = $t
        [System.Windows.Controls.Canvas]::SetLeft($bd, $X1 + ($X2 - $X1) * 0.55 - 16)
        [System.Windows.Controls.Canvas]::SetTop($bd, $Y1 + ($Y2 - $Y1) * 0.55 - 8)
        [System.Windows.Controls.Panel]::SetZIndex($bd, 5)
        [void]$Canvas.Children.Add($bd)
    }
}

function Show-NetMap {
    $list = @($script:NetList)
    if (-not $list.Count) { Set-Status 'Lance d''abord un scan du réseau.'; return }
    $cv = $ui.NetMapCanvas
    $cv.Children.Clear()
    $W = $cv.Width; $H = $cv.Height
    $bx = $W / 2; $by = 410
    $gw = @($list | Where-Object { $_.Gateway })[0]
    $others = @($list | Where-Object { -not $_.Gateway } | Sort-Object @{ Expression = { $i = [array]::IndexOf($MapKindOrder, [string]$_.KindInfo.Kind); if ($i -lt 0) { 99 } else { $i } } }, @{ Expression = { $_.Title } })
    $n = $others.Count

    # Internet et la box
    $pub = if ($script:PublicIp) { $script:PublicIp } else { '' }
    Add-MapLink $cv $bx $by $bx 92 $Colors.info $false ''
    New-MapNode $cv 'Internet' $pub 0xE12B $Colors.info $bx 92 58 '' $null 170
    $gTitle = if ($gw) { $gw.Title } else { 'Box Internet' }
    $gSub = if ($gw) { $gw.Ip } else { '' }
    # Anneaux : un seul jusqu'à 14 appareils, deux au delà
    $rings = if ($n -le 14) { , @(@{ Rx = 470; Ry = 268 }) } else { @(@{ Rx = 290; Ry = 160 }, @{ Rx = 505; Ry = 290 }) }
    $size = if ($n -le 14) { 56 } elseif ($n -le 30) { 48 } else { 40 }
    $width = if ($n -le 14) { 160 } elseif ($n -le 30) { 130 } else { 110 }
    $pos = @()
    if ($rings.Count -eq 1) {
        for ($i = 0; $i -lt $n; $i++) { $pos += @{ R = 0; A = 295 + 310 * ($i + 0.5) / $n } }
    } else {
        $nIn = [math]::Floor($n * 0.38); $nOut = $n - $nIn
        for ($i = 0; $i -lt $nIn; $i++) { $pos += @{ R = 0; A = 300 + 300 * ($i + 0.5) / $nIn } }
        for ($i = 0; $i -lt $nOut; $i++) { $pos += @{ R = 1; A = 295 + 310 * ($i + 0.5) / $nOut } }
        # Le premier anneau reçoit les premiers appareils de chaque type, dans le même ordre
        $pos = @($pos | Sort-Object { $_.A })
    }
    for ($i = 0; $i -lt $n; $i++) {
        $d = $others[$i]; $p = $pos[$i]; $r = $rings[$p.R]
        $a = $p.A * [math]::PI / 180
        $x = $bx + $r.Rx * [math]::Cos($a); $y = $by + $r.Ry * [math]::Sin($a)
        $k = $d.KindInfo
        $ms = if ($n -le 14 -and $null -ne $d.Ms -and -not $d.Self) { $(if ($d.Ms -lt 1) { '< 1 ms' } else { "$($d.Ms) ms" }) } else { '' }
        Add-MapLink $cv $bx $by $x $y $k.Color ([bool]$d.Hidden) $ms
        $ring = if ($d.New -or $d.Camera) { $Colors.warn } elseif ($d.Self) { $Colors.info } else { '' }
        $sub = if ($d.Self) { "Ce PC, $($d.Ip)" } else { $d.Ip }
        $tags = @()
        if ($d.Hidden) { $tags += , @('Discret', '#B18CFF') }
        if ($d.New) { $tags += , @('Nouveau', $Colors.warn) }
        if ($d.Camera) { $tags += , @('Caméra ?', $Colors.warn) }
        New-MapNode $cv $d.Title $sub $k.Glyph $k.Color $x $y $size $ring $d $width $tags
    }
    New-MapNode $cv $gTitle $gSub 0xE80F $Colors.ok $bx $by 82 $Colors.ok $gw 190

    # Légende : nombre d'appareils par type
    $ui.NetMapLegend.Children.Clear()
    foreach ($grp in @($list | Group-Object { $_.KindInfo.Kind } | Sort-Object @{ Expression = { $i = [array]::IndexOf($MapKindOrder, [string]$_.Name); if ($i -lt 0) { 99 } else { $i } } })) {
        $col = $grp.Group[0].KindInfo.Color
        $lg = New-Text "●  $($grp.Name) ($($grp.Count))" 12 $col
        $lg.Margin = New-Thickness 0 0 18 4
        [void]$ui.NetMapLegend.Children.Add($lg)
    }
    $hid = @($list | Where-Object { $_.Hidden }).Count
    $new = @($list | Where-Object { $_.New }).Count
    $when = if ($script:NetScanAt) { " à $($script:NetScanAt.ToString('HH:mm'))" } else { '' }
    $hidNames = @($list | Where-Object { $_.Hidden } | Select-Object -First 4 | ForEach-Object { "$($_.Title) ($($_.Ip))" })
    $ui.NetMapSub.Text = "$($list.Count) appareils trouvés au dernier scan$when$(if ($new) { ", $new nouveau$(if ($new -gt 1) {'x'})" }). $(if ($hid) { "Discret$(if ($hid -gt 1) {'s'}) (ne répond$(if ($hid -gt 1) {'ent'}) pas au ping, liaison en pointillés) : $($hidNames -join ', ')$(if ($hid -gt 4) { '...' }). " })Clique sur un appareil pour voir tout ce qu'OptiGame sait de lui."
    if ($ui.NetMapOverlay.Visibility -ne 'Visible') {
        $ui.NetMapOverlay.Visibility = 'Visible'
        $ui.NetMapOverlay.Opacity = 0
        Start-WpfAnim $ui.NetMapOverlay ([System.Windows.UIElement]::OpacityProperty) 1 250
    }
}

function Hide-NetMap { $ui.NetMapOverlay.Visibility = 'Collapsed' }
