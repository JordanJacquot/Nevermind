# Nevermind : thèmes de couleurs (Paramètres, onglet Thème).
# Toute l'app est dessinée avec la palette Néon ; un thème « traduit » chaque couleur au chargement :
# les couleurs de la marque (cyan, violet, magenta) prennent celles du thème, les gris prennent sa teinte.
# Les couleurs d'état (vert = bien, orange, rouge, bleu info) ne changent jamais.
# Chargé par OptiGame.ps1 juste après donnees.ps1, avant la lecture de la fenêtre (interface.ps1).

$AppThemes = [ordered]@{
    neon = @{ Name = 'Néon'; Desc = 'Le thème de Nevermind : cyan, violet et magenta sur fond de nuit.'
        P = '#00E5FF'; S = '#B04BFF'; T = '#FF2EB5'; Map = @{} ; NH = $null; NS = 1.0
        Hello = 'Salut {0}'; Ready = 'C''est prêt !'; Font = $null; Decor = $null }
    crepuscule = @{ Name = 'Crépuscule'; Desc = 'Le style Néon recoloré : orange, rose et or, comme un coucher de soleil.'
        P = '#FF8A3D'; S = '#FF3D7F'; T = '#FFC83D'; NH = 345; NS = 0.75
        Map = @{ '0C0920' = '1A0B12'; '06050F' = '0B0508'; '0B0820' = '170910'; '08060F' = '160806' }
        Hello = 'Salut {0}'; Ready = 'C''est prêt !'; Font = $null; Decor = $null }
    terminal = @{ Name = 'Terminal'; Desc = 'Pour les geeks : vert phosphore sur fond noir, tout en police de code.'
        P = '#33FF77'; S = '#B6FF3B'; T = '#00E0A0'; NH = 140; NS = 0.55
        Map = @{ '0C0920' = '030B06'; '06050F' = '000302'; '0B0820' = '020A05'; '08060F' = '001A08' }
        Hello = '> salut {0}_'; Ready = '> prêt.'; Font = 'Cascadia Code, Consolas'; Decor = 'terminal' }
    arcade = @{ Name = 'Arcade'; Desc = 'Jaune, rouge et bleu vifs sur fond bleu nuit, comme une vieille borne.'
        P = '#FBD000'; S = '#E52521'; T = '#1E88E5'; NH = 228; NS = 0.9
        Map = @{ '0C0920' = '0B1640'; '06050F' = '050A20'; '0B0820' = '0A1338'; '08060F' = '1A1000' }
        Hello = 'Salut {0}'; Ready = 'C''est prêt !'; Font = $null; Decor = $null }
    rubis = @{ Name = 'Rubis'; Desc = 'Rouge vif, jaune et bleu sur un gris anthracite sobre.'
        P = '#FF3B3B'; S = '#FFDE00'; T = '#3B6FFF'; NH = 0; NS = 0.15
        Map = @{ '0C0920' = '17171D'; '06050F' = '0A0A0D'; '0B0820' = '141419'; '08060F' = '1A0606' }
        Hello = 'Salut {0}'; Ready = 'C''est prêt !'; Font = $null; Decor = $null }
}

# Thème choisi (la copie de test peut en forcer un pour les captures)
$ThemeSetting = [string](Get-Setting 'Theme' 'neon')
if ($env:OPTIGAME_TEST -and $env:OPTIGAME_THEME) { $ThemeSetting = $env:OPTIGAME_THEME }
$ThemeId = @{ retro = 'arcade'; dresseur = 'rubis' }[$ThemeSetting], $ThemeSetting | Where-Object { $_ } | Select-Object -First 1   # anciens noms (1.0.63)
if (-not $AppThemes.Contains($ThemeId)) { $ThemeId = 'neon' }
$Theme = $AppThemes[$ThemeId]

# ---------------------------------------------------------------------------
# Packs de thème : un dossier (ou un zip à importer) hors de l'app, avec pack.json et ses images.
# Ils restent sur le PC (dossier des données de Nevermind) et ne passent jamais par GitHub :
# on peut y mettre ses propres images, même de personnages qui ne nous appartiennent pas, pour un usage privé.
#
# pack.json :
#   Name, Desc        nom et description affichés dans Paramètres, Thème
#   Base              thème de couleurs utilisé (neon, crepuscule, terminal, arcade, rubis)
#   Hello, Ready      bonjour de l'accueil (« {0} » = prénom) et fin du chargement (facultatifs)
#   Loader            écran de chargement : { File = planche PNG (images côte à côte), Frames, Delay (ms), Flip }
#   Footer            dessin animé au centre de la barre du bas : { File = planche PNG, Frames, Delay (ms), Height (px affichés) }
#   TabIcons          icônes animées des onglets du haut : { jeux = { File, Frames, Delay }, reseau, trafic, overlay, ordinateur }
#                     (immobiles au repos, animées au survol de l'onglet)
#   (Loader.Static : l'animation reste centrée au lieu d'avancer avec la barre ; TabIcons.*.Glow / LogoGlow :
#    couleur d'une lueur, l'image fixe lévite et brille au lieu de sauter)
#   Logo              image à la place du N de Nevermind (PNG transparent, carré), qui se secoue de temps en temps
#   Colors            couleurs propres au pack (au lieu d'un thème de base) : { P, S, T, NH, NS, Map }
#   FontPixel         false pour une police lisse (pas de rendu « pixel », tailles inchangées)
#   Font, FontScope   police du pack (fichier .ttf) : sur les titres, onglets, boutons et chiffres (« titres »), ou partout (« tout »)
# ---------------------------------------------------------------------------
$PacksDir = Join-Path $DataDir 'packs'
if ($env:OPTIGAME_TEST -and $env:OPTIGAME_PACKS) { $PacksDir = $env:OPTIGAME_PACKS }

function Get-ThemePack([string]$Id) {
    $dir = Join-Path $PacksDir $Id
    $f = Join-Path $dir 'pack.json'
    if (-not (Test-Path -LiteralPath $f)) { return $null }
    try { $j = Get-Content -LiteralPath $f -Raw -Encoding UTF8 | ConvertFrom-Json } catch { Write-Log "Pack $Id illisible : $_"; return $null }
    $p = @{ Id = $Id; Dir = $dir; Name = [string]$j.Name; Desc = [string]$j.Desc; Base = [string]$j.Base; Hello = [string]$j.Hello; Ready = [string]$j.Ready; Loader = $null; Font = $null; FontScope = 'titres' }
    if ($j.Footer -and $j.Footer.Files) {
        $fs = @(@($j.Footer.Files) | ForEach-Object { Join-Path $dir ([string]$_) } | Where-Object { Test-Path -LiteralPath $_ })
        if ($fs.Count) { $p.Footer = @{ Files = $fs; Height = [math]::Max(16, [int]$j.Footer.Height) } }
    } elseif ($j.Footer -and $j.Footer.File -and (Test-Path -LiteralPath (Join-Path $dir ([string]$j.Footer.File)))) {
        $p.Footer = @{ File = (Join-Path $dir ([string]$j.Footer.File)); Frames = [math]::Max(1, [int]$j.Footer.Frames); Delay = [math]::Max(40, [int]$j.Footer.Delay); Height = [math]::Max(16, [int]$j.Footer.Height) }
    }
    $p.TabIcons = @{}
    if ($j.TabIcons) {
        foreach ($pr in $j.TabIcons.PSObject.Properties) {
            $v = $pr.Value
            if ($v.File -and (Test-Path -LiteralPath (Join-Path $dir ([string]$v.File)))) {
                $p.TabIcons[$pr.Name] = @{ File = (Join-Path $dir ([string]$v.File)); Frames = [math]::Max(1, [int]$v.Frames); Delay = [math]::Max(40, [int]$v.Delay); Glow = [string]$v.Glow }
            }
        }
    }
    $p.LogoGlow = [string]$j.LogoGlow
    # Fond d'écran : image derrière toute l'app, sous un voile sombre (BackgroundOpacity = visibilité de l'image, 0 à 1)
    $p.Background = if ($j.Background -and (Test-Path -LiteralPath (Join-Path $dir ([string]$j.Background)))) { Join-Path $dir ([string]$j.Background) } else { $null }
    $p.BackgroundOpacity = if ($j.BackgroundOpacity) { [math]::Min(1.0, [math]::Max(0.05, [double]$j.BackgroundOpacity)) } else { 0.35 }
    $p.LogoFrames = [math]::Max(1, [int]$j.LogoFrames); $p.LogoDelay = [math]::Max(30, [int]$j.LogoDelay)
    $p.Logo = if ($j.Logo -and (Test-Path -LiteralPath (Join-Path $dir ([string]$j.Logo)))) { Join-Path $dir ([string]$j.Logo) } else { $null }
    if ($j.Font -and (Test-Path -LiteralPath (Join-Path $dir ([string]$j.Font)))) {
        $p.Font = Join-Path $dir ([string]$j.Font)
        if ([string]$j.FontScope -eq 'tout') { $p.FontScope = 'tout' }
    }
    if (-not $p.Name) { $p.Name = $Id }
    # Couleurs propres au pack : un thème « pack-<dossier> » ajouté à la liste (jamais affiché comme thème de base)
    if ($j.Colors -and $j.Colors.P -and $j.Colors.S -and $j.Colors.T) {
        $map = @{}
        if ($j.Colors.Map) { foreach ($pr in $j.Colors.Map.PSObject.Properties) { $map[$pr.Name.TrimStart('#').ToUpper()] = ([string]$pr.Value).TrimStart('#').ToUpper() } }
        $key = "pack-$Id"
        $AppThemes[$key] = @{ Name = $p.Name; Desc = $p.Desc; P = [string]$j.Colors.P; S = [string]$j.Colors.S; T = [string]$j.Colors.T
            NH = $(if ($null -ne $j.Colors.NH) { [double]$j.Colors.NH } else { $null }); NS = $(if ($j.Colors.NS) { [double]$j.Colors.NS } else { 1.0 })
            Map = $map; Hello = 'Salut {0}'; Ready = 'C''est prêt !'; Font = $null; Decor = $null; Pack = $true }
        $p.Base = $key
    }
    if (-not $AppThemes.Contains($p.Base)) { $p.Base = 'neon' }
    $p.FontPixel = if ($null -ne $j.FontPixel) { [bool]$j.FontPixel } else { $true }
    if ($j.Loader -and $j.Loader.File -and (Test-Path -LiteralPath (Join-Path $dir ([string]$j.Loader.File)))) {
        $p.Loader = @{ File = (Join-Path $dir ([string]$j.Loader.File)); Frames = [math]::Max(1, [int]$j.Loader.Frames); Delay = [math]::Max(40, [int]$j.Loader.Delay); Flip = [bool]$j.Loader.Flip; Static = [bool]$j.Loader.Static; Height = [int]$j.Loader.Height; Gap = [int]$j.Loader.Gap }
    }
    $p
}

function Get-ThemePacks {
    if (-not (Test-Path -LiteralPath $PacksDir)) { return @() }
    @(Get-ChildItem -LiteralPath $PacksDir -Directory | ForEach-Object { Get-ThemePack $_.Name } | Where-Object { $_ })
}

# Thème choisi « pack:<dossier> » : les couleurs de son thème de base, ses textes et son écran de chargement
$ThemePack = $null
if ($ThemeSetting -like 'pack:*') {
    $ThemePack = Get-ThemePack $ThemeSetting.Substring(5)
    if ($ThemePack) { $ThemeId = $ThemePack.Base; $Theme = $AppThemes[$ThemeId] }
}

# Police du pack, utilisable partout comme un nom de police (« file:///dossier/#Nom, police de secours »)
$PackFont = $null
if ($ThemePack -and $ThemePack.Font) {
    try {
        $gt = New-Object System.Windows.Media.GlyphTypeface (New-Object Uri $ThemePack.Font)
        $fam = @($gt.FamilyNames.Values)[0]
        $PackFont = 'file:///' + ($ThemePack.Dir -replace '\\', '/') + '/#' + $fam
    } catch { Write-Log "Police du pack illisible : $_" }
}

# Texte en police du pack, net (une police pixel floutée par le lissage perd tout son charme)
# La police pixel est bien plus large qu'une police normale : le texte est réduit d'autant (Scale)
function Set-PackFont($El, [double]$Scale = 0.82) {
    if (-not $PackFont -or -not $El) { return }
    if (-not $ThemePack.FontPixel) {
        # Police lisse : même taille, rendu normal
        $El.FontFamily = New-Object System.Windows.Media.FontFamily "$PackFont, Segoe UI Variable Display, Segoe UI"
        return
    }
    # Police partout : la taille est déjà convertie (Get-UiFontSize)
    if ($ThemePack.FontScope -ne 'tout' -and $Scale -ne 1 -and $El.FontSize) { $El.FontSize = [math]::Round($El.FontSize * $Scale) }
    $El.FontFamily = New-Object System.Windows.Media.FontFamily "$PackFont, Segoe UI Variable Display, Segoe UI"
    [System.Windows.Media.TextOptions]::SetTextRenderingMode($El, 'Aliased')
    [System.Windows.Media.TextOptions]::SetTextFormattingMode($El, 'Display')
}

# Un GIF (fond clair accepté) devient une planche PNG transparente, images côte à côte, toutes à la même hauteur.
function Convert-GifToSheet([string]$Gif, [string]$OutPng, [int]$Height = 160) {
    Add-Type -AssemblyName System.Drawing
    $img = [System.Drawing.Image]::FromFile($Gif)
    try {
        $fd = New-Object System.Drawing.Imaging.FrameDimension $img.FrameDimensionsList[0]
        $n = $img.GetFrameCount($fd)
        $delay = 100
        # Durée la plus courante des images (la première dure souvent plus longtemps que les autres)
        try { $pi = $img.GetPropertyItem(0x5100); $ds = @(for ($k = 0; $k + 3 -lt $pi.Value.Length; $k += 4) { [BitConverter]::ToInt32($pi.Value, $k) }); $delay = [math]::Max(20, 10 * [int](($ds | Group-Object | Sort-Object Count -Descending | Select-Object -First 1).Name)) } catch {}
        $frames = @(); $box = $null
        for ($i = 0; $i -lt $n; $i++) {
            [void]$img.SelectActiveFrame($fd, $i)
            $bmp = New-Object System.Drawing.Bitmap $img.Width, $img.Height, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
            $g = [System.Drawing.Graphics]::FromImage($bmp); $g.DrawImage($img, 0, 0, $img.Width, $img.Height); $g.Dispose()
            $rect = New-Object System.Drawing.Rectangle 0, 0, $bmp.Width, $bmp.Height
            $data = $bmp.LockBits($rect, 'ReadWrite', $bmp.PixelFormat)
            $px = New-Object byte[] ($data.Stride * $bmp.Height)
            [Runtime.InteropServices.Marshal]::Copy($data.Scan0, $px, 0, $px.Length)
            # Fond : la couleur du coin de l'image (blanc, damier gris, mauve...), ou rien s'il est déjà transparent
            if ($px[3] -ne 0) { [void][SpriteTools]::RemoveBackgroundColor($px, $bmp.Width, $bmp.Height, $px[2], $px[1], $px[0], 60) }
            [void][SpriteTools]::RemoveLightBackground($px, $bmp.Width, $bmp.Height, 215, 28)
            [Runtime.InteropServices.Marshal]::Copy($px, 0, $data.Scan0, $px.Length)
            $bmp.UnlockBits($data)
            $b = [SpriteTools]::OpaqueBounds($px, $bmp.Width, $bmp.Height)
            if ($b) { $box = if ($box) { @([math]::Min($box[0], $b[0]), [math]::Min($box[1], $b[1]), [math]::Max($box[2], $b[2]), [math]::Max($box[3], $b[3])) } else { $b } }
            $frames += $bmp
        }
    } finally { $img.Dispose() }
    if (-not $box) { throw 'Image vide après le détourage.' }
    $cw = $box[2] - $box[0]; $ch = $box[3] - $box[1]
    $native = $Height -le 0
    if ($native) { $Height = $ch }   # taille d'origine : du pixel art reste net
    $fw = [int][math]::Round($cw * $Height / $ch)
    $sheet = New-Object System.Drawing.Bitmap ($fw * $frames.Count), $Height, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($sheet)
    if ($native) { $g.InterpolationMode = 'NearestNeighbor'; $g.PixelOffsetMode = 'Half' }
    else { $g.InterpolationMode = 'HighQualityBicubic'; $g.PixelOffsetMode = 'HighQuality'; $g.CompositingQuality = 'HighQuality' }
    for ($i = 0; $i -lt $frames.Count; $i++) {
        $g.DrawImage($frames[$i], (New-Object System.Drawing.Rectangle ($i * $fw), 0, $fw, $Height), (New-Object System.Drawing.Rectangle $box[0], $box[1], $cw, $ch), 'Pixel')
        $frames[$i].Dispose()
    }
    $g.Dispose()
    $sheet.Save($OutPng, [System.Drawing.Imaging.ImageFormat]::Png)
    $sheet.Dispose()
    @{ Frames = $frames.Count; Delay = $delay; Width = $fw; Height = $Height }
}

# Pack (zip contenant pack.json) ajouté dans le dossier des packs ; renvoie son identifiant
function Import-ThemePack([string]$Zip) {
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $tmp = Join-Path $env:TEMP ("nevermind-pack-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
    [IO.Compression.ZipFile]::ExtractToDirectory($Zip, $tmp)
    $json = @(Get-ChildItem -LiteralPath $tmp -Recurse -Filter 'pack.json' | Select-Object -First 1)
    if (-not $json.Count) { throw 'Ce zip ne contient pas de pack.json : ce n''est pas un pack de thème Nevermind.' }
    $src = $json[0].DirectoryName
    $id = ([IO.Path]::GetFileNameWithoutExtension($Zip) -replace '[^\w\-]', '-').ToLower()
    $dest = Join-Path $PacksDir $id
    New-Item -ItemType Directory -Force -Path $dest | Out-Null
    foreach ($f in Get-ChildItem -LiteralPath $src -Recurse -File) {
        $rel = $f.FullName.Substring($src.Length + 1)
        $to = Join-Path $dest $rel
        New-Item -ItemType Directory -Force -Path (Split-Path $to -Parent) | Out-Null
        Copy-Item -LiteralPath $f.FullName -Destination $to -Force
    }
    try { [IO.Directory]::Delete($tmp, $true) } catch {}
    if (-not (Get-ThemePack $id)) { throw 'Le pack.json de ce pack est illisible.' }
    $id
}

# Attention : PowerShell ne distingue pas $R de $r, d'où des noms différents pour les valeurs 0 à 1
function ConvertTo-Hsl([int]$Red, [int]$Green, [int]$Blue) {
    $r = $Red / 255.0; $g = $Green / 255.0; $b = $Blue / 255.0
    $max = [math]::Max($r, [math]::Max($g, $b)); $min = [math]::Min($r, [math]::Min($g, $b))
    $l = ($max + $min) / 2
    if ($max -eq $min) { return @(0.0, 0.0, $l) }
    $d = $max - $min
    $s = if ($l -gt 0.5) { $d / (2 - $max - $min) } else { $d / ($max + $min) }
    $h = if ($max -eq $r) { (($g - $b) / $d) + $(if ($g -lt $b) { 6 } else { 0 }) } elseif ($max -eq $g) { (($b - $r) / $d) + 2 } else { (($r - $g) / $d) + 4 }
    @(($h * 60.0), $s, $l)
}

function ConvertFrom-Hsl([double]$H, [double]$S, [double]$L) {
    $H = (($H % 360) + 360) % 360
    $c = (1 - [math]::Abs(2 * $L - 1)) * $S
    $x = $c * (1 - [math]::Abs((($H / 60.0) % 2) - 1))
    $m = $L - $c / 2
    $rgb = switch ([int][math]::Floor($H / 60.0)) { 0 { @($c, $x, 0.0) } 1 { @($x, $c, 0.0) } 2 { @(0.0, $c, $x) } 3 { @(0.0, $x, $c) } 4 { @($x, 0.0, $c) } default { @($c, 0.0, $x) } }
    '{0:X2}{1:X2}{2:X2}' -f [int][math]::Round(255 * ($rgb[0] + $m)), [int][math]::Round(255 * ($rgb[1] + $m)), [int][math]::Round(255 * ($rgb[2] + $m))
}

# Couleur Néon (« #RRGGBB » ou « #AARRGGBB ») traduite dans un thème
$script:ThemeCache = @{}
function ConvertTo-ThemeHex([string]$Hex, [string]$Id = $ThemeId) {
    if ($Id -eq 'neon' -or -not $Hex -or $Hex[0] -ne '#' -or ($Hex.Length -ne 7 -and $Hex.Length -ne 9)) { return $Hex }
    $key = "$Id|$Hex"
    $hit = $script:ThemeCache[$key]
    if ($hit) { return $hit }
    $th = $AppThemes[$Id]
    $alpha = if ($Hex.Length -eq 9) { $Hex.Substring(1, 2) } else { '' }
    $rgb = $Hex.Substring($Hex.Length - 6).ToUpper()
    $out = $rgb
    if ($th.Map.ContainsKey($rgb)) { $out = $th.Map[$rgb] }
    elseif ($rgb -eq '00E5FF') { $out = $th.P.Substring(1) } elseif ($rgb -eq 'B04BFF') { $out = $th.S.Substring(1) } elseif ($rgb -eq 'FF2EB5') { $out = $th.T.Substring(1) }
    else {
        $r = [Convert]::ToInt32($rgb.Substring(0, 2), 16); $g = [Convert]::ToInt32($rgb.Substring(2, 2), 16); $b = [Convert]::ToInt32($rgb.Substring(4, 2), 16)
        $hsl = ConvertTo-Hsl $r $g $b
        $h = $hsl[0]; $s = $hsl[1]; $l = $hsl[2]
        $brand = $null
        if ($s -gt 0.5 -and $l -gt 0.2 -and $l -lt 0.92) {
            if ($h -ge 170 -and $h -lt 200) { $brand = $th.P } elseif ($h -ge 228 -and $h -lt 292) { $brand = $th.S } elseif ($h -ge 292 -and $h -lt 336) { $brand = $th.T }
        }
        if ($brand) {
            # Même clarté que la couleur d'origine (une variante claire reste claire), teinte et saturation du thème
            $bh = ConvertTo-Hsl ([Convert]::ToInt32($brand.Substring(1, 2), 16)) ([Convert]::ToInt32($brand.Substring(3, 2), 16)) ([Convert]::ToInt32($brand.Substring(5, 2), 16))
            $dl = $l - 0.5
            $out = if ([math]::Abs($dl) -lt 0.06) { $brand.Substring(1).ToUpper() } else { ConvertFrom-Hsl $bh[0] $bh[1] ([math]::Min(0.95, [math]::Max(0.05, $bh[2] + $dl))) }
        } elseif (($s -lt 0.35 -or $l -lt 0.16) -and $l -gt 0.015 -and $l -lt 0.97 -and $null -ne $th.NH) {
            # Gris et fonds : la teinte du thème, même clarté
            $out = ConvertFrom-Hsl $th.NH ([math]::Min(1.0, $s * $th.NS)) $l
        }
    }
    $res = "#$alpha$out"
    $script:ThemeCache[$key] = $res
    $res
}

# Texte de la fenêtre (interface.xaml) traduit avant d'être chargé
# ---------------------------------------------------------------------------
# Police du pack partout (FontScope « tout ») : une police pixel est presque deux fois plus large.
# Chaque taille de texte est convertie en taille « pixel exacte » (8, 12, 16 ou 24) : nette, et à peu près
# aussi large qu'avant, donc rien ne déborde. Les icônes et le compteur de FPS par dessus les jeux
# (fenêtre à part, police normale) ne changent pas.
# ---------------------------------------------------------------------------
function Get-PackFontSize([double]$Size) {
    if ($Size -lt 15) { 8 } elseif ($Size -lt 21) { 12 } elseif ($Size -lt 30) { 16 } else { 24 }
}

$PackSizeAll = [bool]($PackFont -and $ThemePack.FontScope -eq 'tout' -and $ThemePack.FontPixel)

# Taille d'un texte de l'app : convertie en taille pixel quand la police du pack est partout
function Get-UiFontSize([double]$Size) { if ($PackSizeAll) { Get-PackFontSize $Size } else { $Size } }

# Fenêtre : chaque FontSize (attribut ou style) converti, sauf sur les icônes
function Convert-PackFontSizes([string]$Text) {
    $Text = [regex]::Replace($Text, '<[A-Za-z][^<>]*?FontSize="[0-9.]+"[^<>]*>', {
        param($m)
        if ($m.Value -match 'Fluent Icons|MDL2') { return $m.Value }
        [regex]::Replace($m.Value, 'FontSize="([0-9.]+)"', { param($n) 'FontSize="' + (Get-PackFontSize ([double]::Parse($n.Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture))) + '"' })
    })
    $Text = [regex]::Replace($Text, '<Setter Property="FontSize" Value="([0-9.]+)"/>', { param($m) '<Setter Property="FontSize" Value="' + (Get-PackFontSize ([double]::Parse($m.Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture))) + '"/>' })
    # taille de base de la fenêtre (textes sans taille précise)
    [regex]::Replace($Text, '(<Window [^>]*?)FontFamily=', '$1FontSize="8" FontFamily=', 1)
}

# Police du pack dans la fenêtre : titres (style H1, H2, mot « Nevermind » du chargement) ;
# avec FontScope « tout », toute l'app
function Add-PackFontXaml([string]$Text) {
    $spec = [Security.SecurityElement]::Escape("$PackFont, Segoe UI Variable Display, Segoe UI")
    $crisp = if ($ThemePack.FontPixel) { '<Setter Property="TextOptions.TextRenderingMode" Value="Aliased"/><Setter Property="TextOptions.TextFormattingMode" Value="Display"/>' } else { '' }
    $Text = $Text.Replace('Segoe UI Variable Display, Segoe UI', $spec)
    # (pas les boutons : la police pixel, bien plus large, couperait leur texte)
    foreach ($style in '<Style x:Key="H2" TargetType="TextBlock">') {
        $Text = $Text.Replace($style, $style + '<Setter Property="FontFamily" Value="' + $spec + '"/>' + $crisp)
    }
    # H1 a déjà sa police (remplacée juste au-dessus) : seulement le rendu net
    $Text = $Text.Replace('<Style x:Key="H1" TargetType="TextBlock">', '<Style x:Key="H1" TargetType="TextBlock">' + $crisp)
    if ($ThemePack.FontScope -eq 'tout') {
        $Text = $Text.Replace('Segoe UI Variable Text, Segoe UI', $spec).Replace('Cascadia Code, Consolas', $spec)
        # rendu net pour toute la fenêtre (hérité par chaque texte)
        # Rendu net hérité par chaque texte ; un espacement de lignes minimum (certaines polices pixel n'en ont aucun :
        # deux lignes de texte se touchaient). Les textes plus grands gardent leur espacement naturel.
        if ($ThemePack.FontPixel) {
            $Text = [regex]::Replace($Text, '(<Window [^>]*?)FontFamily=', '$1TextOptions.TextRenderingMode="Aliased" TextOptions.TextFormattingMode="Display" Block.LineHeight="12" Block.LineStackingStrategy="MaxHeight" FontFamily=', 1)
            $Text = Convert-PackFontSizes $Text
        }
    }
    $Text
}

function Convert-ThemeXaml([string]$Text, [string]$Id = $ThemeId) {
    if ($PackFont -and $Id -eq $ThemeId) { $Text = Add-PackFontXaml $Text }
    if ($Id -eq 'neon') { return $Text }
    $th = $AppThemes[$Id]
    $Text = [regex]::Replace($Text, '(?<![&\w])#([0-9A-Fa-f]{8}|[0-9A-Fa-f]{6})(?![0-9A-Fa-f])', { param($m) ConvertTo-ThemeHex $m.Value $Id })
    if ($th.Font) { $Text = $Text.Replace('Segoe UI Variable Display, Segoe UI', $th.Font).Replace('Segoe UI Variable Text, Segoe UI', $th.Font) }
    $Text
}

function Get-ThemeText([string]$Key) { if ($ThemePack -and $ThemePack[$Key]) { [string]$ThemePack[$Key] } else { [string]$Theme[$Key] } }

# ---------------------------------------------------------------------------
# Décor de fond propre au thème (discret, derrière tout, ne capte jamais la souris)
# ---------------------------------------------------------------------------
function New-RawBrush([string]$Hex) { [System.Windows.Media.BrushConverter]::new().ConvertFromString($Hex) }

function Add-DecorAt($Parent, $El, [string]$H, [string]$V, [double]$L, [double]$T, [double]$R, [double]$B, [double]$Opacity) {
    $El.HorizontalAlignment = $H; $El.VerticalAlignment = $V
    $El.Margin = [System.Windows.Thickness]::new($L, $T, $R, $B)
    $El.Opacity = $Opacity
    [void]$Parent.Children.Add($El)
}

function Add-ThemeDecor {
    $d = $ui.BackdropDeco
    if (-not $d) { return }
    $d.Children.Clear()
    # Fond d'écran d'un pack : l'image, puis un voile sombre qui garde le texte lisible
    if ($ThemePack -and $ThemePack.Background) {
        $bi = New-Object System.Windows.Media.Imaging.BitmapImage
        $bi.BeginInit(); $bi.UriSource = New-Object Uri $ThemePack.Background; $bi.DecodePixelWidth = 1920; $bi.CacheOption = 'OnLoad'; $bi.EndInit(); $bi.Freeze()
        $img = New-Object System.Windows.Controls.Image
        $img.Source = $bi; $img.Stretch = 'UniformToFill'; $img.HorizontalAlignment = 'Center'; $img.VerticalAlignment = 'Center'
        $img.Opacity = $ThemePack.BackgroundOpacity
        [void]$d.Children.Add($img)
        $veil = New-Object System.Windows.Shapes.Rectangle
        $veil.Fill = New-RawGradient @('#99000000', '#33000000', '#AA000000') 0 1
        [void]$d.Children.Add($veil)
    }
    switch ($Theme.Decor) {
        'terminal' {
            # Lignes de balayage d'un vieil écran et une invite de commande dans le coin
            $lines = New-Object System.Windows.Media.DrawingBrush
            $lines.TileMode = 'Tile'; $lines.Viewport = [System.Windows.Rect]::new(0, 0, 4, 4); $lines.ViewportUnits = 'Absolute'
            $lines.Drawing = New-Object System.Windows.Media.GeometryDrawing (New-RawBrush '#2233FF77'), $null, ([System.Windows.Media.RectangleGeometry]::new([System.Windows.Rect]::new(0, 0, 4, 1)))
            $scan = New-Object System.Windows.Shapes.Rectangle
            $scan.Fill = $lines
            Add-DecorAt $d $scan 'Stretch' 'Stretch' 0 0 0 0 0.5
            $txt = New-Object System.Windows.Controls.TextBlock
            $txt.Text = "nevermind@pc:~`$ ./optimiser --jeux`n[ok] analyse terminée`n[ok] fps sous surveillance`n_"
            $txt.FontFamily = New-Object System.Windows.Media.FontFamily 'Cascadia Code, Consolas'
            $txt.FontSize = 13; $txt.Foreground = New-RawBrush '#33FF77'
            Add-DecorAt $d $txt 'Right' 'Bottom' 0 0 40 40 0.16
        }
    }
}
