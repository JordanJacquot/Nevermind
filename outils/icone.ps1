# Génère les icônes d'OptiGame (jauge verte sur fond sombre).
#   OptiGame.ico              icône de l'application
#   OptiGame-desinstaller.ico même icône avec un badge rouge
param([string]$OutDir = (Join-Path $PSScriptRoot 'icones'))

Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

function New-RoundedRect([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $p = New-Object Drawing.Drawing2D.GraphicsPath
    $d = 2 * $r
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    $p
}

function New-Pen([string]$Hex, [float]$Width) {
    $pen = New-Object Drawing.Pen ([Drawing.ColorTranslator]::FromHtml($Hex)), $Width
    $pen.StartCap = 'Round'; $pen.EndCap = 'Round'
    $pen
}

function New-IconBitmap([int]$s, [bool]$Uninstall) {
    $bmp = New-Object Drawing.Bitmap $s, $s, ([Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.PixelOffsetMode = 'HighQuality'
    $g.Clear([Drawing.Color]::Transparent)

    # Fond: carré arrondi sombre
    $m = [math]::Max(0.5, $s * 0.03)
    $bg = New-RoundedRect $m $m ($s - 2 * $m) ($s - 2 * $m) ($s * 0.22)
    $grad = New-Object Drawing.Drawing2D.LinearGradientBrush ([Drawing.PointF]::new(0, 0)), ([Drawing.PointF]::new(0, $s)),
        ([Drawing.ColorTranslator]::FromHtml('#252C39')), ([Drawing.ColorTranslator]::FromHtml('#0E1014'))
    $g.FillPath($grad, $bg)
    if ($s -ge 32) { $g.DrawPath((New-Object Drawing.Pen ([Drawing.ColorTranslator]::FromHtml('#343C4C')), ([float]($s / 64))), $bg) }

    # Jauge
    $cx = $s / 2; $cy = $s * 0.56; $r = $s * 0.30
    $arcRect = [Drawing.RectangleF]::new($cx - $r, $cy - $r, 2 * $r, 2 * $r)
    $w = [math]::Max(1.6, $s * 0.105)
    $g.DrawArc((New-Pen '#2E3544' $w), $arcRect, 135, 270)
    $g.DrawArc((New-Pen '#22D37A' $w), $arcRect, 135, 205)

    # Aiguille
    $a = (135 + 205) * [math]::PI / 180
    $len = $r * 0.80
    $g.DrawLine((New-Pen '#FFFFFF' ([math]::Max(1.2, $s * 0.065))), [float]$cx, [float]$cy, [float]($cx + $len * [math]::Cos($a)), [float]($cy + $len * [math]::Sin($a)))
    $hub = [math]::Max(1.5, $s * 0.075)
    $g.FillEllipse([Drawing.Brushes]::White, [float]($cx - $hub), [float]($cy - $hub), [float](2 * $hub), [float](2 * $hub))

    # Badge rouge pour le désinstalleur
    if ($Uninstall) {
        $br = $s * 0.23; $bx = $s - $br - $s * 0.02; $by = $s - $br - $s * 0.02
        $g.FillEllipse((New-Object Drawing.SolidBrush ([Drawing.ColorTranslator]::FromHtml('#0E1014'))), [float]($bx - $br - $s * 0.03), [float]($by - $br - $s * 0.03), [float](2 * $br + $s * 0.06), [float](2 * $br + $s * 0.06))
        $g.FillEllipse((New-Object Drawing.SolidBrush ([Drawing.ColorTranslator]::FromHtml('#F04438'))), [float]($bx - $br), [float]($by - $br), [float](2 * $br), [float](2 * $br))
        $g.DrawLine((New-Pen '#FFFFFF' ([math]::Max(1.2, $s * 0.07))), [float]($bx - $br * 0.5), [float]$by, [float]($bx + $br * 0.5), [float]$by)
    }
    $g.Dispose()
    $bmp
}

function Save-Ico([string]$Path, [bool]$Uninstall) {
    $sizes = 16, 20, 24, 32, 40, 48, 64, 128, 256
    $pngs = foreach ($s in $sizes) {
        $b = New-IconBitmap $s $Uninstall
        $ms = New-Object IO.MemoryStream
        $b.Save($ms, [Drawing.Imaging.ImageFormat]::Png)
        $b.Dispose()
        , $ms.ToArray()
    }
    $out = New-Object IO.MemoryStream
    $bw = New-Object IO.BinaryWriter $out
    $bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$sizes.Count)
    $offset = 6 + 16 * $sizes.Count
    for ($i = 0; $i -lt $sizes.Count; $i++) {
        $s = $sizes[$i]
        $bw.Write([byte]$(if ($s -ge 256) { 0 } else { $s }))
        $bw.Write([byte]$(if ($s -ge 256) { 0 } else { $s }))
        $bw.Write([byte]0); $bw.Write([byte]0)
        $bw.Write([uint16]1); $bw.Write([uint16]32)
        $bw.Write([uint32]$pngs[$i].Length); $bw.Write([uint32]$offset)
        $offset += $pngs[$i].Length
    }
    foreach ($p in $pngs) { $bw.Write($p) }
    $bw.Flush()
    [IO.File]::WriteAllBytes($Path, $out.ToArray())
}

Save-Ico (Join-Path $OutDir 'OptiGame.ico') $false
Save-Ico (Join-Path $OutDir 'OptiGame-desinstaller.ico') $true

# Aperçu pour vérifier le rendu à plusieurs tailles
$prev = New-Object Drawing.Bitmap 560, 300
$g = [Drawing.Graphics]::FromImage($prev)
$g.Clear([Drawing.ColorTranslator]::FromHtml('#F3F3F3'))
$x = 10
foreach ($s in 256, 64, 32, 16) {
    $g.DrawImage((New-IconBitmap $s $false), $x, 10, $s, $s)
    $x += $s + 14
}
$g.DrawImage((New-IconBitmap 128 $true), 10, 160, 128, 128)
$g.DrawImage((New-IconBitmap 48 $true), 150, 200, 48, 48)
$g.DrawImage((New-IconBitmap 32 $true), 210, 208, 32, 32)
$g.Dispose()
$prev.Save((Join-Path $OutDir 'apercu.png'), [Drawing.Imaging.ImageFormat]::Png)
'icônes créées'
