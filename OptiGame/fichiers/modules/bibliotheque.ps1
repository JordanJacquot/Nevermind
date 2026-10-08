# OptiGame : section « Jeux », la bibliothèque de tous les jeux installés (comme celle de Steam).
# Jaquettes, recherche, filtre par launcher ; double clic ou « Lancer » pour jouer (par le launcher du jeu) ;
# temps de jeu, dernières parties et optimisation du jeu sélectionné.
# Chargé par OptiGame.ps1 après jeu.ps1 (liste des jeux) et diagnostic-fps.ps1.

$PlayFile = Join-Path $DataDir 'jeux.json'

# ---------------------------------------------------------------------------
# Temps de jeu (noté à chaque partie repérée par OptiGame, quel que soit le launcher)
# ---------------------------------------------------------------------------
function Get-PlayLog {
    if ($null -ne $script:PlayLog) { return $script:PlayLog }
    $script:PlayLog = @{}
    try {
        if (Test-Path -LiteralPath $PlayFile) {
            $o = Get-Content -LiteralPath $PlayFile -Raw -Encoding UTF8 | ConvertFrom-Json
            foreach ($p in $o.PSObject.Properties) { $script:PlayLog[$p.Name] = @{ Last = [string]$p.Value.Last; Seconds = [double]$p.Value.Seconds; Count = [int]$p.Value.Count } }
        }
    } catch { Write-Log "Temps de jeu illisible: $_" }
    $script:PlayLog
}

function Add-PlayTime([string]$Game, [datetime]$Start) {
    if (-not $Game) { return }
    $log = Get-PlayLog
    $e = if ($log.ContainsKey($Game)) { $log[$Game] } else { @{ Last = ''; Seconds = 0.0; Count = 0 } }
    $e.Seconds += [math]::Max(0.0, ((Get-Date) - $Start).TotalSeconds)
    $e.Count++
    $e.Last = (Get-Date).ToString('s')
    $log[$Game] = $e
    try { [IO.File]::WriteAllText($PlayFile, (ConvertTo-Json -InputObject $log -Depth 4 -Compress), (New-Object Text.UTF8Encoding($false))) } catch { Write-Log "Temps de jeu: $_" }
    if ($script:LibBuilt) { Update-LibraryView }
}

function Format-LastPlayed([string]$Iso) {
    if (-not $Iso) { return 'jamais lancé avec OptiGame ouvert' }
    $d = [datetime]$Iso
    $days = ((Get-Date).Date - $d.Date).Days
    if ($days -le 0) { "aujourd'hui à $($d.ToString('HH:mm'))" } elseif ($days -eq 1) { 'hier' } elseif ($days -lt 7) { "il y a $days jours" } else { "le $($d.ToString('dd/MM/yyyy'))" }
}

# ---------------------------------------------------------------------------
# Jaquettes : celles que Steam garde sur le PC, sinon une vignette aux couleurs du jeu
# ---------------------------------------------------------------------------
function Get-SteamArt($Game, [string]$Kind) {
    if (-not $Game.AppId) { return $null }
    if (-not $script:LibArt) { $script:LibArt = @{} }
    $ak = "$($Game.AppId)|$Kind"
    if (-not $script:LibArt.ContainsKey($ak)) { $script:LibArt[$ak] = Find-SteamArt $Game $Kind }
    $script:LibArt[$ak]
}

function Find-SteamArt($Game, [string]$Kind) {
    if ($null -eq $script:SteamCache) {
        $script:SteamCache = ''
        $sp = (Get-ItemProperty 'HKCU:\Software\Valve\Steam' -ErrorAction SilentlyContinue).SteamPath
        if ($sp) { $p = Join-Path ($sp -replace '/', '\') 'appcache\librarycache'; if (Test-Path -LiteralPath $p) { $script:SteamCache = $p } }
    }
    if (-not $script:SteamCache) { return $null }
    $dir = Join-Path $script:SteamCache ([string]$Game.AppId)
    $f = Join-Path $dir $Kind
    if (Test-Path -LiteralPath $f) { return $f }
    # Steam récent : images rangées dans un sous dossier
    @(Get-ChildItem -LiteralPath $dir -Filter $Kind -Recurse -File -ErrorAction SilentlyContinue | Select-Object -First 1 | ForEach-Object { $_.FullName })[0]
}

function Get-ImageBrush([string]$Path, [int]$Width) {
    if (-not $Path) { return $null }
    if (-not $script:LibImages) { $script:LibImages = @{} }
    $ck = "$Path|$Width"
    if ($script:LibImages.ContainsKey($ck)) { return $script:LibImages[$ck] }
    $script:LibImages[$ck] = $null
    $script:LibImages[$ck] = try {
        $bi = New-Object System.Windows.Media.Imaging.BitmapImage
        $bi.BeginInit()
        $bi.UriSource = New-Object Uri $Path
        $bi.DecodePixelWidth = $Width
        $bi.CacheOption = 'OnLoad'
        $bi.EndInit()
        $bi.Freeze()
        $b = New-Object System.Windows.Media.ImageBrush $bi
        $b.Stretch = 'UniformToFill'
        $b.Freeze()
        $b
    } catch { $null }
    $script:LibImages[$ck]
}

# Couleur stable tirée du nom du jeu (vignette sans jaquette)
function Get-GameHue([string]$Name) {
    $h = 0; foreach ($c in $Name.ToCharArray()) { $h = ($h * 31 + [int]$c) % 360 }
    $palette = @('#4EA8FF', '#B18CFF', '#22D37A', '#F5A524', '#FF7AB6', '#2EC4D6', '#FF6B5B', '#8FA8FF')
    $palette[$h % $palette.Count]
}

function Get-GameIcon($Game) {
    $exe = @($Game.Exes)[0]
    if (-not $exe -or -not (Test-Path -LiteralPath $exe)) { return $null }
    if (-not $script:LibIcons) { $script:LibIcons = @{} }
    if ($script:LibIcons.ContainsKey($exe)) { return $script:LibIcons[$exe] }
    $script:LibIcons[$exe] = try {
        $ic = [System.Drawing.Icon]::ExtractAssociatedIcon($exe)
        $src = [System.Windows.Interop.Imaging]::CreateBitmapSourceFromHIcon($ic.Handle, [System.Windows.Int32Rect]::Empty, [System.Windows.Media.Imaging.BitmapSizeOptions]::FromEmptyOptions())
        $src.Freeze(); $ic.Dispose()
        $src
    } catch { $null }
    $script:LibIcons[$exe]
}

# Jaquette (2:3) ou vignette générée : fond dégradé, icône du jeu et initiales
function New-GameCover($Game, [double]$W, [double]$H) {
    $b = New-Object System.Windows.Controls.Border
    $b.Width = $W; $b.Height = $H
    $b.CornerRadius = [System.Windows.CornerRadius]::new(10)
    # Jaquette Steam : ancien nom, puis celui des versions récentes de Steam
    $file = Get-SteamArt $Game 'library_600x900.jpg'
    if (-not $file) { $file = Get-SteamArt $Game 'library_capsule.jpg' }
    $art = Get-ImageBrush $file ([int]($W * 2))
    if ($art) { $b.Background = $art; return $b }
    $hue = Get-GameHue $Game.Name
    $b.Background = New-LinearBrush @($hue, '#141820') 0 0 1 1
    $g = New-Object System.Windows.Controls.Grid
    $ini = (@($Game.Name -split '[\s:\-]+' | Where-Object { $_ -match '^[A-Za-z0-9À-ÿ]' } | Select-Object -First 2 | ForEach-Object { $_.Substring(0, 1).ToUpper() }) -join '')
    $t = New-Text $ini ([math]::Round($W / 3.2)) '#FFFFFF' -Bold
    $t.Opacity = 0.9; $t.HorizontalAlignment = 'Center'; $t.VerticalAlignment = 'Center'
    [void]$g.Children.Add($t)
    $icon = Get-GameIcon $Game
    if ($icon) {
        $img = New-Object System.Windows.Controls.Image
        $img.Source = $icon; $img.Width = 32; $img.Height = 32
        $img.HorizontalAlignment = 'Left'; $img.VerticalAlignment = 'Top'; $img.Margin = New-Thickness 10 10 0 0
        [void]$g.Children.Add($img)
    }
    $n = New-Text $Game.Name 11 '#FFFFFF' -Semi
    $n.TextWrapping = 'Wrap'; $n.TextAlignment = 'Center'; $n.Opacity = 0.85
    $n.VerticalAlignment = 'Bottom'; $n.Margin = New-Thickness 8 0 8 10
    [void]$g.Children.Add($n)
    $b.Child = $g
    $b
}

# ---------------------------------------------------------------------------
# Lancer un jeu : par son launcher (connexion, mises à jour, anti triche), sinon son exécutable.
# Toujours sans les droits administrateur d'OptiGame (passe par l'Explorateur).
# ---------------------------------------------------------------------------
function Get-GameLaunch($Game) {
    $l = [string]$Game.Launch
    if ($l -like 'exe|*') { $x = $l -split '\|', 3; return @{ Kind = 'exe'; Path = $x[1]; Args = $x[2] } }
    if ($l -like 'xbox:*') {
        $name = $l.Substring(5)
        $app = @(try { Get-StartApps -ErrorAction Stop } catch { @() }) | Where-Object { $_.Name -eq $name -or $_.Name -like "$name*" } | Select-Object -First 1
        if ($app) { return @{ Kind = 'url'; Path = "shell:AppsFolder\$($app.AppID)" } }
        $l = ''
    }
    if ($l) { return @{ Kind = 'url'; Path = $l } }
    $exe = @($Game.Exes)[0]
    @{ Kind = 'exe'; Path = $exe; Args = '' }
}

function Start-LibraryGame($Game) {
    if (-not $Game) { return }
    $how = Get-GameLaunch $Game
    if ($how.Kind -eq 'exe' -and -not (Test-Path -LiteralPath $how.Path)) { Show-Message "Le jeu « $($Game.Name) » est introuvable :`n$($how.Path)`n`nIl a peut être été désinstallé ou déplacé. Clique sur « Actualiser »." 'Warning'; return }
    Write-Log "Bibliothèque: lancement de $($Game.Name) ($($how.Path))"
    if ($how.Kind -eq 'exe') {
        # Raccourci temporaire : le jeu démarre dans son dossier, comme depuis son icône
        $lnk = Join-Path $env:TEMP "OptiGame-jeu-$([IO.Path]::GetFileNameWithoutExtension($how.Path)).lnk"
        $sh = New-Object -ComObject WScript.Shell
        try {
            $sc = $sh.CreateShortcut($lnk)
            $sc.TargetPath = $how.Path; $sc.Arguments = [string]$how.Args; $sc.WorkingDirectory = Split-Path $how.Path -Parent
            $sc.Save()
        } finally { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($sh) }
        Open-Url $lnk
    } else { Open-Url $how.Path }
    $script:LibLaunching = @{ Name = $Game.Name; At = Get-Date }
    Set-Status "Lancement de $($Game.Name)$(if ($Game.Source -and $how.Kind -eq 'url') { " par $($Game.Source)" })..."
    Update-LibraryDetail
}

# ---------------------------------------------------------------------------
# Page
# ---------------------------------------------------------------------------
function Get-LibraryGames {
    # Tri par bloc de script : Sort-Object Name ne voit pas les clés d'une table (tous les jeux seraient « égaux »)
    $seen = @{}
    @($script:Games | Where-Object { $_ -and $_.Name } | Sort-Object { $_.Name } | Where-Object { if ($seen.ContainsKey($_.Name)) { $false } else { $seen[$_.Name] = $true; $true } })
}

function Build-Library {
    $script:LibBuilt = $true
    if (-not $script:LibFilter) { $script:LibFilter = 'Tous' }
    if ($null -eq $script:Games) { Update-GameCache }
    Update-LibraryView
}

function Update-LibraryView {
    if (-not $script:LibBuilt) { return }
    $all = Get-LibraryGames
    $log = Get-PlayLog
    $ui.LibSub.Text = "$($all.Count) jeu$(if ($all.Count -gt 1) {'x'}) installé$(if ($all.Count -gt 1) {'s'}), tous launchers confondus. Double clique sur un jeu pour jouer."
    # Filtres : tous, récents, puis un par launcher
    $ui.LibFilters.Children.Clear()
    $chips = @(@('Tous', $all.Count), @('Récents', @($all | Where-Object { $log.ContainsKey($_.Name) }).Count))
    $chips += @($all | Group-Object { if ($_.Source) { $_.Source } else { 'Steam' } } | Sort-Object Count -Descending | ForEach-Object { , @($_.Name, $_.Count) })
    foreach ($c in $chips) {
        if ($c[0] -eq 'Récents' -and -not $c[1]) { continue }
        $on = $script:LibFilter -eq $c[0]
        $b = New-Object System.Windows.Controls.Border
        $b.CornerRadius = [System.Windows.CornerRadius]::new(15); $b.Padding = New-Thickness 12 5 12 5; $b.Margin = New-Thickness 0 2 6 2
        $b.Background = Get-Brush $(if ($on) { '#22D37A' } else { '#1A1F29' })
        $b.Cursor = [System.Windows.Input.Cursors]::Hand
        $t = New-Text "$($c[0]) ($($c[1]))" 12 $(if ($on) { '#0B0D10' } else { '#C9CED8' }) -Semi
        $t.TextWrapping = 'NoWrap'
        $b.Child = $t
        $b.Tag = [string]$c[0]
        $b.Add_MouseLeftButtonUp({ param($s, $e) $script:LibFilter = [string]$s.Tag; Update-LibraryView })
        [void]$ui.LibFilters.Children.Add($b)
    }
    # Jeux affichés : filtre, recherche, puis les plus récemment joués d'abord
    $q = ([string]$ui.LibSearch.Text).Trim()
    $shown = @($all | Where-Object {
        $src = if ($_.Source) { $_.Source } else { 'Steam' }
        ($script:LibFilter -eq 'Tous' -or ($script:LibFilter -eq 'Récents' -and $log.ContainsKey($_.Name)) -or $src -eq $script:LibFilter) -and
        (-not $q -or (ConvertTo-SearchText $_.Name).Contains((ConvertTo-SearchText $q)))
    } | Sort-Object @{ Expression = { if ($log.ContainsKey($_.Name)) { $log[$_.Name].Last } else { '' } }; Descending = $true }, @{ Expression = { $_.Name } })
    $ui.LibGrid.Children.Clear()
    $script:LibTiles = @{}
    if (-not $shown.Count) {
        $e = New-Text $(if ($all.Count) { 'Aucun jeu ne correspond.' } else { 'Aucun jeu trouvé sur ce PC. Clique sur « Ajouter un jeu » pour ajouter le tien.' }) 13 '#9AA3B2'
        $e.Margin = New-Thickness 4 8 0 0
        [void]$ui.LibGrid.Children.Add($e)
    }
    foreach ($g in $shown) { [void]$ui.LibGrid.Children.Add((New-LibraryTile $g)) }
    if (-not $script:LibSelected -or -not @($shown | Where-Object { $_.Name -eq $script:LibSelected }).Count) { $script:LibSelected = if ($shown.Count) { $shown[0].Name } else { $null } }
    Set-LibrarySelection $script:LibSelected
}

function New-LibraryTile($Game) {
    $w = 132; $h = 198
    $tile = New-Object System.Windows.Controls.StackPanel
    $tile.Width = $w; $tile.Margin = New-Thickness 0 0 14 16
    $tile.Cursor = [System.Windows.Input.Cursors]::Hand
    $frame = New-Object System.Windows.Controls.Border
    $frame.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $frame.BorderThickness = New-Thickness 2 2 2 2
    $frame.BorderBrush = [System.Windows.Media.Brushes]::Transparent
    $frame.Padding = New-Thickness 0 0 0 0
    $g = New-Object System.Windows.Controls.Grid
    [void]$g.Children.Add((New-GameCover $Game ($w - 4) ($h - 4)))
    # Pastille « En jeu »
    if ($script:GameSession -and $script:GameSession.Game -eq $Game.Name) {
        $bd = New-Object System.Windows.Controls.Border
        $bd.Background = Get-Brush $Colors.ok; $bd.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $bd.Padding = New-Thickness 8 2 8 2; $bd.Margin = New-Thickness 0 8 8 0
        $bd.HorizontalAlignment = 'Right'; $bd.VerticalAlignment = 'Top'
        $bd.Child = (New-Text 'En jeu' 11 '#0B0D10' -Bold)
        [void]$g.Children.Add($bd)
    }
    $frame.Child = $g
    $move = New-Object System.Windows.Media.TranslateTransform
    $frame.RenderTransform = $move
    [void]$tile.Children.Add($frame)
    $n = New-Text $Game.Name 12.5 '#E6E8EE' -Semi
    $n.TextTrimming = 'CharacterEllipsis'; $n.TextWrapping = 'NoWrap'; $n.Margin = New-Thickness 2 6 0 0; $n.ToolTip = $Game.Name
    [void]$tile.Children.Add($n)
    $src = New-Text $(if ($Game.Source) { $Game.Source } else { 'Steam' }) 11 '#5B6475'
    $src.Margin = New-Thickness 2 0 0 0
    [void]$tile.Children.Add($src)
    $tile.Tag = $Game.Name
    $tile.Add_MouseEnter({ param($s, $e) $s.Children[0].RenderTransform.Y = -3 })
    $tile.Add_MouseLeave({ param($s, $e) $s.Children[0].RenderTransform.Y = 0 })
    $tile.Add_MouseLeftButtonDown({
        param($s, $e)
        $name = [string]$s.Tag
        if ($e.ClickCount -ge 2) { $g = @(Get-LibraryGames | Where-Object { $_.Name -eq $name })[0]; Invoke-Safe { Start-LibraryGame $g }; $e.Handled = $true; return }
        Set-LibrarySelection $name
    })
    $script:LibTiles[$Game.Name] = $frame
    $tile
}

function Set-LibrarySelection([string]$Name) {
    $script:LibSelected = $Name
    foreach ($k in @($script:LibTiles.Keys)) {
        $f = $script:LibTiles[$k]
        if ($k -eq $Name) { $f.BorderBrush = Get-Brush $Colors.ok; $f.Effect = New-Glow $Colors.ok 14 0.6 }
        else { $f.BorderBrush = [System.Windows.Media.Brushes]::Transparent; $f.Effect = $null }
    }
    Update-LibraryDetail
}

# ---------------------------------------------------------------------------
# Panneau du jeu sélectionné : bannière, Lancer, temps de jeu, FPS, optimisation
# ---------------------------------------------------------------------------
function Update-LibraryDetail {
    $p = $ui.LibDetailPanel
    $p.Children.Clear()
    $g = @(Get-LibraryGames | Where-Object { $_.Name -eq $script:LibSelected })[0]
    if (-not $g) {
        $e = New-Text 'Choisis un jeu à gauche.' 13 '#9AA3B2'
        $e.Margin = New-Thickness 20 20 20 20
        [void]$p.Children.Add($e)
        return
    }
    # Bannière (image large de Steam, sinon dégradé aux couleurs du jeu)
    $hero = New-Object System.Windows.Controls.Border
    $hero.Height = 150
    $img = Get-ImageBrush (Get-SteamArt $g 'library_hero.jpg') 720
    if (-not $img) { $img = Get-ImageBrush (Get-SteamArt $g 'header.jpg') 720 }
    if (-not $img) { $img = Get-ImageBrush (Get-SteamArt $g 'library_header.jpg') 720 }
    $hero.Background = if ($img) { $img } else { New-LinearBrush @((Get-GameHue $g.Name), '#141820') 0 0 1 1 }
    $hg = New-Object System.Windows.Controls.Grid
    $shade = New-Object System.Windows.Controls.Border
    $shade.Background = New-LinearBrush @('#00000000', '#E6141820') 0 0 0 1
    [void]$hg.Children.Add($shade)
    $logo = Get-SteamArt $g 'logo.png'
    if ($logo) {
        $li = New-Object System.Windows.Controls.Image
        $li.Source = (Get-ImageBrush $logo 480).ImageSource
        $li.MaxHeight = 70; $li.MaxWidth = 240; $li.HorizontalAlignment = 'Left'; $li.VerticalAlignment = 'Bottom'; $li.Margin = New-Thickness 18 0 0 12
        [void]$hg.Children.Add($li)
    }
    $hero.Child = $hg
    [void]$p.Children.Add($hero)

    $body = New-Object System.Windows.Controls.StackPanel
    $body.Margin = New-Thickness 18 12 18 18
    $title = New-Text $g.Name 19 '#FFFFFF' -Bold
    $title.TextWrapping = 'Wrap'
    [void]$body.Children.Add($title)
    $log = Get-PlayLog
    $pl = $log[$g.Name]
    $meta = New-Text "$(if ($g.Source) { $g.Source } else { 'Steam' })  ·  dernière partie : $(Format-LastPlayed $(if ($pl) { $pl.Last } else { '' }))" 12 '#9AA3B2'
    $meta.Margin = New-Thickness 0 2 0 12
    [void]$body.Children.Add($meta)

    # Lancer
    $running = $script:GameSession -and $script:GameSession.Game -eq $g.Name
    $starting = $script:LibLaunching -and $script:LibLaunching.Name -eq $g.Name -and ((Get-Date) - $script:LibLaunching.At).TotalSeconds -lt 20
    $play = New-Button $(if ($running) { 'En cours de jeu' } elseif ($starting) { 'Lancement...' } else { 'Lancer' }) 'BtnPrimary'
    $play.FontSize = 15; $play.Height = 44; $play.HorizontalAlignment = 'Stretch'
    $play.IsEnabled = -not $running
    $play.Tag = $g.Name
    $play.Add_Click({ param($s, $e) $n = [string]$s.Tag; $x = @(Get-LibraryGames | Where-Object { $_.Name -eq $n })[0]; Invoke-Safe { Start-LibraryGame $x } })
    [void]$body.Children.Add($play)
    $how = Get-GameLaunch $g
    $hw = New-Text $(if ($how.Kind -eq 'url' -and $g.Source) { "Lancé par $($g.Source) (connexion, mises à jour et anti triche comme d'habitude)." } else { 'Lancé directement depuis son dossier.' }) 11 '#5B6475'
    $hw.Margin = New-Thickness 0 6 0 0; $hw.TextWrapping = 'Wrap'
    [void]$body.Children.Add($hw)

    # Chiffres : temps de jeu, parties, FPS de la dernière partie mesurée
    $fi = Get-Item -LiteralPath $FpsFile -ErrorAction SilentlyContinue
    $stamp = if ($fi) { $fi.LastWriteTimeUtc.Ticks } else { 0 }
    if (-not $script:LibFps -or $script:LibFps.Stamp -ne $stamp) { $script:LibFps = @{ Stamp = $stamp; List = @(Get-FpsSessions) } }
    $sess = @($script:LibFps.List | Where-Object { (Get-SessionName $_) -eq $g.Name -or @($g.Exes | ForEach-Object { [IO.Path]::GetFileNameWithoutExtension($_).ToLower() }) -contains [string]$_.Key } | Sort-Object { [datetime]$_.Date } -Descending)
    $last = $sess | Select-Object -First 1
    $stats = New-Grid @('*', '*', '*')
    $stats.Margin = New-Thickness 0 16 0 0
    $cells = @(
        @('TEMPS DE JEU', $(if ($pl) { Format-PlayTime $pl.Seconds } else { '-' })),
        @('PARTIES', $(if ($pl) { [string]$pl.Count } else { '0' })),
        @('FPS MOYENS', $(if ($last) { '{0:N0}' -f $last.Avg } else { '-' }))
    )
    for ($i = 0; $i -lt 3; $i++) {
        $c = New-Object System.Windows.Controls.StackPanel
        [void]$c.Children.Add((New-Text $cells[$i][0] 10.5 '#5B6475' -Semi))
        [void]$c.Children.Add((New-Text $cells[$i][1] 16 '#FFFFFF' -Bold))
        Add-ToGrid $stats $c $i
    }
    [void]$body.Children.Add($stats)
    if ($last) {
        $lv = Get-FpsVerdict $last
        $lk = New-Button "Dernière partie mesurée : $('{0:N0}' -f $last.Avg) FPS, 1 % bas $('{0:N0}' -f $last.Low1). Voir le détail"
        $lk.Margin = New-Thickness 0 10 0 0; $lk.HorizontalAlignment = 'Stretch'; $lk.HorizontalContentAlignment = 'Left'
        $lk.Foreground = Get-Brush $(if ($lv[0] -eq 'ok') { '#C9CED8' } else { $Colors.warn })
        $lk.Tag = [string]$last.Id
        $lk.Add_Click({ param($s, $e) $id = [string]$s.Tag; Invoke-Safe { Show-FpsSession $id } })
        [void]$body.Children.Add($lk)
    }

    # Optimisation du jeu
    $sec = New-Text 'OPTIMISATION DU JEU' 11 '#5B6475' -Semi
    $sec.Margin = New-Thickness 0 20 0 6
    [void]$body.Children.Add($sec)
    $items = @(Get-GameOptimizations $g)
    $todo = @($items | Where-Object { -not $_.Ok -and ($_.Fix -or $_.Switch) -and -not $_.Optional })
    $sum = New-Grid @('*', 'Auto')
    $st = New-Text $(if ($todo.Count) { "$($todo.Count) point$(if ($todo.Count -gt 1) {'s'}) à améliorer pour ce jeu." } else { 'Ce jeu est optimisé.' }) 13 $(if ($todo.Count) { $Colors.warn } else { $Colors.ok }) -Semi
    $st.VerticalAlignment = 'Center'
    Add-ToGrid $sum $st 0
    if ($todo.Count) {
        $all = New-Button 'Tout optimiser' 'BtnPrimary'
        $all.Margin = New-Thickness 10 0 0 0
        $all.Tag = $g.Name
        $all.Add_Click({ param($s, $e) $n = [string]$s.Tag; $x = @(Get-LibraryGames | Where-Object { $_.Name -eq $n })[0]; Invoke-Safe { Invoke-GameOptimize $x } })
        Add-ToGrid $sum $all 1
    }
    [void]$body.Children.Add($sum)
    foreach ($it in $items) { [void]$body.Children.Add((New-OptimRow $g $it)) }

    # Autres actions
    $wp = New-Object System.Windows.Controls.WrapPanel
    $wp.Margin = New-Thickness 0 16 0 0
    $acts = @(@('Ouvrir le dossier', 'folder'), @('Profils par jeu', 'profiles'))
    if ($g.Custom) { $acts += , @('Retirer de la liste', 'remove') }
    foreach ($a in $acts) {
        $b = New-Button $a[0]
        $b.Margin = New-Thickness 0 0 8 8
        $b.Tag = @{ Name = $g.Name; Do = $a[1] }
        $b.Add_Click({
            param($s, $e)
            $x = $s.Tag; $gm = @(Get-LibraryGames | Where-Object { $_.Name -eq $x.Name })[0]
            Invoke-Safe {
                switch ($x.Do) {
                    'folder' { $d = if ($gm.Dir) { $gm.Dir } else { Split-Path @($gm.Exes)[0] -Parent }; Open-Url $d }
                    'profiles' { Show-Page 1; Set-GamingSubPage 'profiles' }
                    'remove' { Remove-CustomGame @($gm.Exes)[0]; Update-LibraryView }
                }
            }
        })
        [void]$wp.Children.Add($b)
    }
    [void]$body.Children.Add($wp)
    [void]$p.Children.Add($body)
}

# Points d'optimisation d'un jeu : { Title, Text, Ok, Fix (scriptblock, $null = information), Switch (interrupteur) }
function Get-GameOptimizations($Game) {
    $list = @()
    $gpuNames = @($script:AnalysisData.GPUs | ForEach-Object { [string]$_.Name } | Where-Object { $_ -notmatch 'Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta|Microsoft Basic' })
    $prio = (@(Get-GameExeNames $Game).Count -gt 0)
    if ($prio) {
        $list += @{ Id = 'priority'; Title = 'Priorité haute'; Text = 'Le jeu passe avant les autres programmes (même OptiGame fermé).'; Ok = (Test-GamePriority $Game); Switch = $true }
    }
    if ($gpuNames.Count -ge 2) {
        $list += @{ Id = 'gpu'; Title = 'Carte graphique puissante'; Text = 'Le jeu utilise la grosse carte, pas la puce intégrée.'; Ok = ((Get-GpuPreference @($Game.Exes)[0]) -match 'GpuPreference=2'); Switch = $true }
    }
    if (-not $script:LibTweaks -or ((Get-Date) - $script:LibTweaks.At).TotalSeconds -gt 30) {
        $script:LibTweaks = @{ At = Get-Date; List = @(Get-AvailableTweaks | Where-Object { $_.Recommended -ne $false -and -not (Test-Tweak $_) }) }
    }
    $pending = @($script:LibTweaks.List)
    $list += @{ Id = 'tweaks'; Title = 'Réglages Windows pour les jeux'; Text = $(if ($pending.Count) { "$($pending.Count) à appliquer : $((@($pending | Select-Object -First 3 | ForEach-Object { $_.Titre })) -join ', ')$(if ($pending.Count -gt 3) { '...' })." } else { 'Tous appliqués (valables pour tous tes jeux).' }); Ok = (-not $pending.Count); Fix = $(if ($pending.Count) { 'tweaks' }); Ids = @($pending | ForEach-Object { $_.Id }) }
    $list += @{ Id = 'fps'; Title = 'Mesure des FPS'; Text = 'Tes FPS sont mesurés à chaque partie, avec un diagnostic si ça rame.'; Ok = (Test-FpsMeasure); Switch = $true }
    $list += @{ Id = 'mode'; Title = 'Mode jeu (fermer des applis)'; Text = 'OneDrive, Teams... fermés pendant la partie, relancés après.'; Ok = [bool](Get-Setting 'GameMode' $false); Switch = $true; Optional = $true }
    # Disque du jeu
    $exe = @($Game.Exes)[0]
    if ($exe -and $exe.Length -gt 1) {
        $drive = $exe.Substring(0, 1).ToUpper()
        $dd = @($script:AnalysisData.Disks | Where-Object { @($_.Letters) -contains $drive })[0]
        if ($dd) {
            $hdd = [string]$dd.Disk.MediaType -eq 'HDD'
            $list += @{ Id = 'disk'; Title = $(if ($hdd) { "Installé sur un disque dur ($($drive):)" } else { "Installé sur un SSD ($($drive):)" }); Text = $(if ($hdd) { 'Chargements plus longs et saccades possibles : déplace le jeu sur un SSD depuis son launcher.' } else { 'Chargements rapides.' }); Ok = (-not $hdd) }
        }
    }
    $list
}

function New-OptimRow($Game, $It) {
    $row = New-Grid @('Auto', '*', 'Auto')
    $row.Margin = New-Thickness 0 8 0 0
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 8; $dot.Height = 8; $dot.Margin = New-Thickness 0 6 10 0; $dot.VerticalAlignment = 'Top'
    $dot.Fill = Get-Brush $(if ($It.Ok) { $Colors.ok } elseif ($It.Optional) { '#5B6475' } else { $Colors.warn })
    Add-ToGrid $row $dot 0
    $sp = New-Object System.Windows.Controls.StackPanel
    [void]$sp.Children.Add((New-Text $It.Title 13 '#FFFFFF' -Semi))
    $tx = New-Text $It.Text 11.5 '#9AA3B2'
    $tx.TextWrapping = 'Wrap'
    [void]$sp.Children.Add($tx)
    Add-ToGrid $row $sp 1
    if ($It.Switch) {
        $sw = New-Object System.Windows.Controls.CheckBox
        $sw.Style = $Window.FindResource('Switch')
        $sw.IsChecked = [bool]$It.Ok
        $sw.VerticalAlignment = 'Center'; $sw.Margin = New-Thickness 10 0 0 0
        $sw.Tag = @{ Name = $Game.Name; Id = $It.Id }
        $sw.Add_Click({ param($s, $e) $x = $s.Tag; $on = [bool]$s.IsChecked; Invoke-Safe { Set-GameOptimization $x.Name $x.Id $on } })
        Add-ToGrid $row $sw 2
    } elseif ($It.Fix -eq 'tweaks') {
        $b = New-Button 'Appliquer'
        $b.Margin = New-Thickness 10 0 0 0; $b.VerticalAlignment = 'Center'
        $b.Tag = @($It.Ids)
        $b.Add_Click({ param($s, $e) $ids = @($s.Tag); Invoke-Safe { Invoke-TweakFix $ids; $script:LibTweaks = $null; Update-LibraryDetail } })
        Add-ToGrid $row $b 2
    }
    $row
}

function Set-GameOptimization([string]$Name, [string]$Id, [bool]$On) {
    $g = @(Get-LibraryGames | Where-Object { $_.Name -eq $Name })[0]
    if (-not $g) { return }
    switch ($Id) {
        'priority' { Set-GameProfile $g 'priority' $On }
        'gpu' { Set-GameProfile $g 'gpu' $On }
        'fps' { Set-Setting 'FpsMeasure' $On; Update-GameWatch; Update-FpsHotkey; Build-FpsPanel; Build-OverlayPanel }
        'mode' { Set-Setting 'GameMode' $On; Update-GameWatch; Build-GameModeCard }
    }
    if ($Id -in 'priority', 'gpu') { Build-GameProfiles }
    Update-LibraryDetail
}

# « Tout optimiser » : profil du jeu, mesure des FPS, puis réglages Windows recommandés (annulable depuis Sauvegarde)
function Invoke-GameOptimize($Game) {
    $items = @(Get-GameOptimizations $Game | Where-Object { -not $_.Ok -and ($_.Switch -or $_.Fix) -and -not $_.Optional })
    if (-not $items.Count) { Set-Status "$($Game.Name) est déjà optimisé."; return }
    if (-not (Confirm-Action "Optimiser $($Game.Name) ?`n`n$(($items | ForEach-Object { "•  $($_.Title)" }) -join "`n")`n`nTout est annulable depuis la page Sauvegarde.")) { return }
    $done = @()
    foreach ($it in $items) {
        switch ($it.Id) {
            'priority' { Set-GameProfile $Game 'priority' $true; $done += 'Priorité haute' }
            'gpu' { Set-GameProfile $Game 'gpu' $true; $done += 'Carte graphique puissante' }
            'fps' { Set-Setting 'FpsMeasure' $true; Update-GameWatch; Update-FpsHotkey; Build-FpsPanel; Build-OverlayPanel; $done += 'Mesure des FPS' }
        }
    }
    Build-GameProfiles
    $tw = @($items | Where-Object { $_.Id -eq 'tweaks' })[0]
    if ($tw) { Invoke-TweakFix @($tw.Ids); $script:LibTweaks = $null }
    else { Show-ResultSheet "$($Game.Name) est optimisé" (@('Fait :') + @($done | ForEach-Object { "•  $_" }) + @('Joue une partie : OptiGame mesurera tes FPS pour vérifier.')) $null $null }
    Update-LibraryDetail
}

function Initialize-Library {
    $script:LibSearchTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:LibSearchTimer.Interval = [TimeSpan]::FromMilliseconds(220)
    $script:LibSearchTimer.Add_Tick({ $script:LibSearchTimer.Stop(); Update-LibraryView })
    $ui.LibSearch.Add_TextChanged({
        $ui.LibSearchHint.Visibility = if ($ui.LibSearch.Text) { 'Collapsed' } else { 'Visible' }
        $script:LibSearchTimer.Stop(); $script:LibSearchTimer.Start()
    })
    $ui.BtnLibAdd.Add_Click({ Invoke-Safe { Add-CustomGame; Update-LibraryView } })
    $ui.BtnLibRefresh.Add_Click({ Invoke-Safe { Set-Status 'Recherche de tes jeux...'; Update-GameCache; Update-LibraryView; Set-Status "$(@(Get-LibraryGames).Count) jeux trouvés." } })
}
