# Génère les icônes de Nexo : un N blanc « glitch » (échos cyan et magenta, tranches décalées) sur fond sombre.
#   OptiGame.ico              icône de l'application (nom de fichier gardé pour les mises à jour)
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

function Col([string]$Hex, [int]$Alpha = 255) { $c = [Drawing.ColorTranslator]::FromHtml($Hex); [Drawing.Color]::FromArgb($Alpha, $c.R, $c.G, $c.B) }

# Le N : un trait épais aux bouts arrondis, transformé en forme pleine
function New-NPath([float]$s) {
    $p = New-Object Drawing.Drawing2D.GraphicsPath
    $x = $s * 0.32; $y = $s * 0.28; $w = $s * 0.36; $h = $s * 0.44
    $p.AddLines([Drawing.PointF[]]@([Drawing.PointF]::new($x, $y + $h), [Drawing.PointF]::new($x, $y), [Drawing.PointF]::new($x + $w, $y + $h), [Drawing.PointF]::new($x + $w, $y)))
    $pen = New-Object Drawing.Pen ([Drawing.Color]::Black), ([float]($s * 0.13))
    $pen.StartCap = 'Round'; $pen.EndCap = 'Round'; $pen.LineJoin = 'Round'
    $p.Widen($pen)
    $p
}

function Fill-Shifted($g, $path, $brush, [float]$dx) {
    $m = New-Object Drawing.Drawing2D.Matrix
    $m.Translate($dx, 0)
    $c = $path.Clone(); $c.Transform($m)
    $g.FillPath($brush, $c)
}

function New-IconBitmap([int]$s, [bool]$Uninstall) {
    $bmp = New-Object Drawing.Bitmap $s, $s, ([Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = 'AntiAlias'
    $g.PixelOffsetMode = 'HighQuality'
    $g.Clear([Drawing.Color]::Transparent)

    # Fond : carré arrondi sombre, liseré discret
    $m = [math]::Max(0.5, $s * 0.03)
    $bg = New-RoundedRect $m $m ($s - 2 * $m) ($s - 2 * $m) ($s * 0.22)
    $bgBrush = New-Object Drawing.Drawing2D.LinearGradientBrush ([Drawing.PointF]::new(0, 0)), ([Drawing.PointF]::new(0, $s)), (Col '#1D1C2B'), (Col '#07070A')
    $g.FillPath($bgBrush, $bg)
    if ($s -ge 32) { $g.DrawPath((New-Object Drawing.Pen (Col '#FFFFFF' 50), ([float]($s / 128))), $bg) }
    $g.SetClip($bg)
    # Lignes d'écran (seulement en grand, invisibles en petit)
    if ($s -ge 64) { for ($y = 0.08; $y -lt 0.95; $y += 0.03) { $g.FillRectangle((New-Object Drawing.SolidBrush (Col '#FFFFFF' 12)), 0, [float]($s * $y), $s, [float][math]::Max(1, $s * 0.006)) } }

    # Le N et ses échos de couleur (au moins un pixel de décalage, même en 16 px)
    $n = New-NPath $s
    $dx = [float][math]::Max(1, $s * 0.03)
    Fill-Shifted $g $n (New-Object Drawing.SolidBrush (Col '#00E5FF' 225)) (-$dx)
    Fill-Shifted $g $n (New-Object Drawing.SolidBrush (Col '#FF2EB5' 225)) $dx
    $g.FillPath([Drawing.Brushes]::White, $n)

    # Tranches décalées : l'effet « glitch » (à partir de 32 px, sinon c'est du bruit)
    if ($s -ge 32) {
        foreach ($band in @(@(0.40, 0.05, 0.05), @(0.60, 0.035, -0.04))) {
            $r = [Drawing.RectangleF]::new(0, [float]($s * $band[0]), $s, [float][math]::Max(1, $s * $band[1]))
            $g.SetClip($r, 'Intersect')
            $g.FillRectangle($bgBrush, $r)
            Fill-Shifted $g $n (New-Object Drawing.SolidBrush (Col '#00E5FF')) ([float]($s * $band[2]))
            Fill-Shifted $g $n ([Drawing.Brushes]::White) ([float]($s * $band[2] * 0.6))
            $g.ResetClip(); $g.SetClip($bg)
        }
    }
    $g.ResetClip()

    # Badge rouge pour le désinstalleur
    if ($Uninstall) {
        $br = $s * 0.23; $bx = $s - $br - $s * 0.02; $by = $s - $br - $s * 0.02
        $g.FillEllipse((New-Object Drawing.SolidBrush (Col '#0E1014')), [float]($bx - $br - $s * 0.03), [float]($by - $br - $s * 0.03), [float](2 * $br + $s * 0.06), [float](2 * $br + $s * 0.06))
        $g.FillEllipse((New-Object Drawing.SolidBrush (Col '#F04438')), [float]($bx - $br), [float]($by - $br), [float](2 * $br), [float](2 * $br))
        $pen = New-Object Drawing.Pen ([Drawing.Color]::White), ([float][math]::Max(1.2, $s * 0.07)); $pen.StartCap = 'Round'; $pen.EndCap = 'Round'
        $g.DrawLine($pen, [float]($bx - $br * 0.5), [float]$by, [float]($bx + $br * 0.5), [float]$by)
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

# Aperçu pour vérifier le rendu à plusieurs tailles, sur fond clair et sombre
$prev = New-Object Drawing.Bitmap 600, 320
$g = [Drawing.Graphics]::FromImage($prev)
$g.Clear((Col '#F3F3F3'))
$g.FillRectangle((New-Object Drawing.SolidBrush (Col '#202020')), 300, 0, 300, 320)
$x = 10
foreach ($s in 128, 64, 32, 16) { $g.DrawImage((New-IconBitmap $s $false), $x, 10, $s, $s); $x += $s + 10 }
$x = 310
foreach ($s in 128, 64, 32, 16) { $g.DrawImage((New-IconBitmap $s $false), $x, 10, $s, $s); $x += $s + 10 }
$g.DrawImage((New-IconBitmap 128 $true), 10, 170, 128, 128)
$g.DrawImage((New-IconBitmap 32 $true), 150, 220, 32, 32)
$g.Dispose()
$prev.Save((Join-Path $OutDir 'apercu.png'), [Drawing.Imaging.ImageFormat]::Png)
'icônes créées'
