# Nevermind : section « Jeux », la bibliothèque de tous les jeux installés (comme celle de Steam).
# Jaquettes, recherche, filtre par launcher ; double clic ou « Lancer » pour jouer (par le launcher du jeu) ;
# temps de jeu, dernières parties et optimisation du jeu sélectionné.
# Chargé par OptiGame.ps1 après jeu.ps1 (liste des jeux) et diagnostic-fps.ps1.

$PlayFile = Join-Path $DataDir 'jeux.json'

# ---------------------------------------------------------------------------
# Temps de jeu (noté à chaque partie repérée par Nevermind, quel que soit le launcher)
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
    if ($ui.Tabs.SelectedIndex -eq $HubIndex -and $script:HubStats) { Update-Hub }
}

function Format-LastPlayed([string]$Iso) {
    if (-not $Iso) { return 'jamais lancé avec Nevermind ouvert' }
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

# ---------------------------------------------------------------------------
# Jaquettes manquantes : cherchées sur la boutique Steam (par le nom du jeu : la plupart des jeux Ubisoft,
# Epic ou Battle.net y sont aussi), puis sur Wikipédia. Seul le nom du jeu est envoyé.
# Gardées dans le dossier « jaquettes » des données de Nevermind ; une recherche ratée est retentée après 7 jours.
# ---------------------------------------------------------------------------
$CoverDir = Join-Path $DataDir 'jaquettes'
$CoverIndexFile = Join-Path $CoverDir 'index.json'

function Test-CoversOnline { [bool](Get-Setting 'LibCoversOnline' $true) }

function Get-CoverIndex {
    if ($null -ne $script:CoverIndex) { return $script:CoverIndex }
    $script:CoverIndex = @{}
    try {
        if (Test-Path -LiteralPath $CoverIndexFile) {
            $o = Get-Content -LiteralPath $CoverIndexFile -Raw -Encoding UTF8 | ConvertFrom-Json
            foreach ($p in $o.PSObject.Properties) { $script:CoverIndex[$p.Name] = @{ Cover = [string]$p.Value.Cover; Hero = [string]$p.Value.Hero; Logo = [string]$p.Value.Logo; Date = [string]$p.Value.Date } }
        }
    } catch {}
    $script:CoverIndex
}

# Jaquette : fichiers de Steam (ancien puis nouveau nom), sinon celle téléchargée
function Get-CoverFile($Game, [switch]$Twin) {
    if ($Game.Art -and $Game.Art.Cover -and (Test-Path -LiteralPath $Game.Art.Cover)) { return $Game.Art.Cover }
    $f = Get-SteamArt $Game 'library_600x900.jpg'
    if (-not $f) { $f = Get-SteamArt $Game 'library_capsule.jpg' }
    if (-not $f) { $c = (Get-CoverIndex)[$Game.Name]; if ($c -and $c.Cover -and (Test-Path -LiteralPath $c.Cover)) { $f = $c.Cover } }
    # Le même jeu acheté sur un autre launcher (Rocket League sur Epic et sur Steam) : sa jaquette
    if (-not $f -and -not $Twin) {
        $me = ConvertTo-SearchText ($Game.Name -replace '[®™©]', '')
        foreach ($o in @(Get-LibraryGames | Where-Object { $_.Name -ne $Game.Name -and (ConvertTo-SearchText ($_.Name -replace '[®™©]', '')) -eq $me })) { $f = Get-CoverFile $o -Twin; if ($f) { break } }
    }
    $f
}

function Get-LogoFile($Game) {
    if ($Game.Art -and $Game.Art.Logo -and (Test-Path -LiteralPath $Game.Art.Logo)) { return $Game.Art.Logo }
    $f = Get-SteamArt $Game 'logo.png'
    if (-not $f) { $c = (Get-CoverIndex)[$Game.Name]; if ($c -and $c.Logo -and (Test-Path -LiteralPath $c.Logo)) { $f = $c.Logo } }
    $f
}

function Get-HeroFile($Game) {
    if ($Game.Art -and $Game.Art.Hero -and (Test-Path -LiteralPath $Game.Art.Hero)) { return $Game.Art.Hero }
    foreach ($k in 'library_hero.jpg', 'header.jpg', 'library_header.jpg') { $f = Get-SteamArt $Game $k; if ($f) { return $f } }
    $c = (Get-CoverIndex)[$Game.Name]
    if ($c -and $c.Hero -and (Test-Path -LiteralPath $c.Hero)) { return $c.Hero }
    $null
}

# Fil séparé : pour chaque jeu, la boutique Steam (identifiant connu ou recherche par nom), puis Wikipédia
$CoverWork = {
    param($a)
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $norm = {
        param($s)
        $d = ([string]$s).ToLowerInvariant().Normalize([Text.NormalizationForm]::FormD)
        (-join ($d.ToCharArray() | Where-Object { [Globalization.CharUnicodeInfo]::GetUnicodeCategory($_) -ne 'NonSpacingMark' -and [char]::IsLetterOrDigit($_) }))
    }
    # Téléchargement d'une image ; un fichier vide ou une page d'erreur n'est pas gardé
    $get = {
        param($url, $file)
        try {
            Invoke-WebRequest -Uri $url -OutFile $file -UseBasicParsing -TimeoutSec 15 -UserAgent 'OptiGame (jaquettes de la bibliotheque)' -ErrorAction Stop
            if ((Get-Item -LiteralPath $file).Length -gt 3KB) { return $true }
        } catch {}
        if (Test-Path -LiteralPath $file) { [IO.File]::Delete($file) }
        $false
    }
    New-Item -ItemType Directory -Force -Path $a.Dir | Out-Null
    foreach ($g in $a.Games) {
        $key = & $norm $g.Name
        if (-not $key) { $key = 'jeu' + [math]::Abs($g.Name.GetHashCode()) }
        $res = @{ Name = $g.Name; Cover = ''; Hero = ''; Logo = ''; Net = $true }
        $clean = ($g.Name -replace '[®™©]', '').Trim()
        $id = $g.AppId
        if (-not $id) {
            try {
                $j = Invoke-RestMethod -Uri "https://store.steampowered.com/api/storesearch/?term=$([uri]::EscapeDataString($clean))&l=french&cc=FR" -TimeoutSec 10 -UserAgent 'OptiGame' -ErrorAction Stop
                $b = & $norm $clean
                foreach ($it in @($j.items)) {
                    $n = & $norm $it.name
                    # Même jeu : même nom, ou l'un contient l'autre (« Overwatch » et « Overwatch 2 »)
                    $close = $n -eq $b -or (($n.Contains($b) -or $b.Contains($n)) -and [math]::Min($n.Length, $b.Length) / [math]::Max(1.0, [math]::Max($n.Length, $b.Length)) -ge 0.6)
                    if ($close) { $id = $it.id; break }
                }
            } catch { $res.Net = $false }
        }
        if ($id) {
            $f = Join-Path $a.Dir "$key.jpg"
            foreach ($u in "https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/$id/library_600x900.jpg", "https://cdn.cloudflare.steamstatic.com/steam/apps/$id/library_600x900.jpg") { if (& $get $u $f) { $res.Cover = $f; break } }
            $h = Join-Path $a.Dir "$key-banniere.jpg"
            foreach ($u in "https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/$id/library_hero.jpg", "https://cdn.cloudflare.steamstatic.com/steam/apps/$id/header.jpg") { if (& $get $u $h) { $res.Hero = $h; break } }
        }
        # Pas sur Steam : l'image de la page Wikipédia du jeu. Plus haute que large : c'est la jaquette ;
        # sinon, si c'est son logo (Valorant, League of Legends, Dofus...), il sert pour la vignette.
        if (-not $res.Cover) {
            foreach ($lang in 'en', 'fr') {
                Start-Sleep -Milliseconds 400   # Wikipédia refuse les demandes trop rapprochées
                try {
                    $s = Invoke-RestMethod -Uri "https://$lang.wikipedia.org/api/rest_v1/page/summary/$([uri]::EscapeDataString(($clean -replace ' ', '_')))" -TimeoutSec 10 -UserAgent 'OptiGame (jaquettes de la bibliotheque)' -ErrorAction Stop
                    $img = $s.originalimage
                    if ($s.type -ne 'standard' -or -not $img -or "$($s.description) $($s.extract)" -notmatch '(?i)jeu|game') { continue }
                    # L'adresse se termine par « ?utm_source=... » : l'extension est avant
                    $src = [string]$img.source
                    $ext = [IO.Path]::GetExtension(($src -split '\?')[0]).ToLower()
                    if ($ext -notin '.jpg', '.jpeg', '.png') { continue }
                    if ($img.height -gt 1.15 * $img.width) {
                        $f = Join-Path $a.Dir "$key-wiki$ext"
                        if (& $get $src $f) { $res.Cover = $f; break }
                    } elseif ($ext -eq '.png' -and $src -match '(?i)logo|\.svg\.png' -and -not $res.Logo) {
                        $f = Join-Path $a.Dir "$key-logo.png"
                        if (& $get $src $f) { $res.Logo = $f }
                    }
                } catch {
                    if ($_.Exception.Message -match '429') { $res.Net = $false }   # trop de demandes : on réessaiera plus tard
                }
            }
        }
        $res
    }
}

function Start-CoverDownload {
    if ($script:CoverJob -or -not (Test-CoversOnline) -or $env:OPTIGAME_TEST) { return }
    $idx = Get-CoverIndex
    $todo = @(Get-LibraryGames | Where-Object {
        $c = $idx[$_.Name]
        -not (Get-SteamArt $_ 'library_600x900.jpg') -and -not (Get-SteamArt $_ 'library_capsule.jpg') -and -not ($_.Art -and $_.Art.Cover) -and
        (-not $c -or (-not $c.Cover -and $c.Date -and ((Get-Date) - [datetime]$c.Date).TotalDays -gt 7) -or ($c.Cover -and -not (Test-Path -LiteralPath $c.Cover)))
    } | ForEach-Object { @{ Name = [string]$_.Name; AppId = [string]$_.AppId } })
    if (-not $todo.Count) { return }
    $ps = [PowerShell]::Create(); $ps.RunspacePool = $script:BgPool
    [void]$ps.AddScript($CoverWork.ToString()).AddArgument(@{ Games = $todo; Dir = $CoverDir })
    $script:CoverJob = @{ PS = $ps; Handle = $ps.BeginInvoke() }
    if (-not $script:CoverTimer) {
        $script:CoverTimer = New-Object System.Windows.Threading.DispatcherTimer
        $script:CoverTimer.Interval = [TimeSpan]::FromSeconds(1)
        $script:CoverTimer.Add_Tick({ try { Receive-Covers } catch { Write-Log "Jaquettes: $_" } })
    }
    $script:CoverTimer.Start()
    Set-Status "Recherche des jaquettes de $($todo.Count) jeu$(if ($todo.Count -gt 1) {'x'})..."
}

function Receive-Covers {
    $j = $script:CoverJob
    if (-not $j -or -not $j.Handle.IsCompleted) { return }
    $script:CoverTimer.Stop()
    $script:CoverJob = $null
    $res = @()
    try { $res = @($j.PS.EndInvoke($j.Handle)) } catch { Write-Log "Jaquettes: $_" } finally { $j.PS.Dispose() }
    $idx = Get-CoverIndex
    $found = 0
    foreach ($r in $res) {
        if (-not $r.Net -and -not $r.Cover -and -not $r.Logo) { continue }   # pas de connexion : on réessaiera au prochain lancement
        $idx[$r.Name] = @{ Cover = [string]$r.Cover; Hero = [string]$r.Hero; Logo = [string]$r.Logo; Date = (Get-Date).ToString('s') }
        if ($r.Cover -or $r.Logo) { $found++ }
    }
    try { [IO.File]::WriteAllText($CoverIndexFile, (ConvertTo-Json -InputObject $idx -Depth 3 -Compress), (New-Object Text.UTF8Encoding($false))) } catch {}
    Write-Log "Jaquettes: $found trouvée(s) sur $(@($res).Count)"
    Set-Status "$found jaquette$(if ($found -gt 1) {'s'}) ajoutée$(if ($found -gt 1) {'s'}) à ta bibliothèque."
    if ($found -and $script:LibBuilt) { Update-LibraryView }
}

# Plus grande image d'un fichier .ico (les launchers Riot, Ubisoft... en fournissent en 256 px)
function Get-BigIcon($Game) {
    $ico = [string]$Game.Icon
    if ($ico -notmatch '\.ico$' -or -not (Test-Path -LiteralPath $ico)) { return $null }
    if (-not $script:LibIcons) { $script:LibIcons = @{} }
    if ($script:LibIcons.ContainsKey($ico)) { return $script:LibIcons[$ico] }
    $script:LibIcons[$ico] = try {
        $dec = [System.Windows.Media.Imaging.BitmapDecoder]::Create((New-Object Uri $ico), 'None', 'OnLoad')
        $fr = @($dec.Frames | Sort-Object PixelWidth -Descending)[0]
        if ($fr.PixelWidth -ge 64) { $fr.Freeze(); $fr } else { $null }
    } catch { $null }
    $script:LibIcons[$ico]
}

# Couleur stable tirée du nom du jeu (vignette sans jaquette)
function Get-GameHue([string]$Name) {
    $h = 0; foreach ($c in $Name.ToCharArray()) { $h = ($h * 31 + [int]$c) % 360 }
    $palette = @('#4EA8FF', '#B18CFF', '#22D37A', '#F5A524', '#FF5CC8', '#2EC4D6', '#FF6B5B', '#8FA8FF')
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
    $art = Get-ImageBrush (Get-CoverFile $Game) ([int]($W * 2))
    if ($art) { $b.Background = $art; return $b }
    $hue = Get-GameHue $Game.Name
    $b.Background = New-LinearBrush @($hue, '#15121E') 0 0 1 1
    $g = New-Object System.Windows.Controls.Grid
    # Logo officiel du jeu, sinon sa grande icône si son launcher en fournit une, sinon ses initiales
    $logo = Get-ImageBrush (Get-LogoFile $Game) 300
    $big = if ($logo) { $null } else { Get-BigIcon $Game }
    if ($logo) {
        $li = New-Object System.Windows.Controls.Image
        $li.Source = $logo.ImageSource; $li.Stretch = 'Uniform'
        $li.Width = $W * 0.8; $li.MaxHeight = $H * 0.4
        $li.HorizontalAlignment = 'Center'; $li.VerticalAlignment = 'Center'; $li.Margin = New-Thickness 0 0 0 24
        [void]$g.Children.Add($li)
    } elseif ($big) {
        $bi = New-Object System.Windows.Controls.Image
        $bi.Source = $big; $bi.Width = [math]::Round($W * 0.55); $bi.Height = $bi.Width
        $bi.HorizontalAlignment = 'Center'; $bi.VerticalAlignment = 'Center'; $bi.Margin = New-Thickness 0 0 0 24
        [void]$g.Children.Add($bi)
    } else {
        $ini = (@($Game.Name -split '[\s:\-]+' | Where-Object { $_ -match '^[A-Za-z0-9À-ÿ]' } | Select-Object -First 2 | ForEach-Object { $_.Substring(0, 1).ToUpper() }) -join '')
        $t = New-Text $ini ([math]::Round($W / 3.2)) '#FFFFFF' -Bold
        $t.Opacity = 0.9; $t.HorizontalAlignment = 'Center'; $t.VerticalAlignment = 'Center'
        [void]$g.Children.Add($t)
    }
    $icon = if ($big -or $logo) { $null } else { Get-GameIcon $Game }
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
# Toujours sans les droits administrateur de Nevermind (passe par l'Explorateur).
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
# Désinstaller : toujours par le désinstalleur du jeu ou de son launcher (qui demande confirmation),
# jamais en effaçant un dossier soi même. La liste se met à jour quand le jeu a disparu.
# ---------------------------------------------------------------------------
# Commande déclarée à Windows : « "C:\...\x.exe" arguments » ou « C:\...\x.exe arguments »
function Split-Command([string]$Cmd) {
    $Cmd = $Cmd.Trim()
    if ($Cmd -match '^"([^"]+)"\s*(.*)$') { return @{ Exe = $Matches[1]; Args = $Matches[2] } }
    if ($Cmd -match '^(.+?\.exe)\s*(.*)$') { return @{ Exe = $Matches[1]; Args = $Matches[2] } }
    @{ Exe = $Cmd; Args = '' }
}

function Get-GameUninstall($Game) {
    $u = [string]$Game.Uninstall
    # Jeu Steam sans numéro connu (installé à la main dans le dossier de Steam) : la bibliothèque de Steam
    if (-not $u -and (-not $Game.Source -or $Game.Source -eq 'Steam')) { return @{ Kind = 'launcher'; Path = 'steam://nav/games' } }
    if (-not $u) { return @{ Kind = 'none' } }
    if ($u -like 'launcher|*') { return @{ Kind = 'launcher'; Path = $u.Substring(9) } }
    if ($u -match '^[a-z][a-z0-9+.-]*://') { return @{ Kind = 'url'; Path = $u } }
    $c = Split-Command $u
    @{ Kind = 'exe'; Path = $c.Exe; Args = $c.Args }
}

function Uninstall-LibraryGame($Game) {
    if (-not $Game) { return }
    $src = if ($Game.Source) { $Game.Source } else { 'Steam' }
    $how = Get-GameUninstall $Game
    switch ($how.Kind) {
        'none' {
            Show-Message "Nevermind ne connaît pas le désinstalleur de « $($Game.Name) ».`n`nLa liste des applications de Windows va s'ouvrir : cherche le jeu et clique sur « Désinstaller »."
            Open-Url 'ms-settings:appsfeatures'
            return
        }
        'launcher' {
            Show-Message "« $($Game.Name) » se désinstalle depuis $src : Nevermind l'ouvre pour toi.`n`nDans $src, fais un clic droit sur le jeu (ou ouvre ses options), puis « Désinstaller »."
            Open-Url $how.Path
        }
        default {
            if (-not (Confirm-Action "Désinstaller « $($Game.Name) » ?`n`nLe désinstalleur de $src va s'ouvrir et te demander de confirmer. Tes sauvegardes dans le cloud ne sont pas touchées.")) { return }
            if ($how.Kind -eq 'url') { Open-Url $how.Path }
            else {
                if (-not (Test-Path -LiteralPath $how.Path)) { Show-Message "Le désinstalleur est introuvable :`n$($how.Path)`n`nDésinstalle le jeu depuis $src." 'Warning'; return }
                Start-Process -FilePath $how.Path -ArgumentList $how.Args -WorkingDirectory (Split-Path $how.Path -Parent)
            }
        }
    }
    Write-Log "Bibliothèque: désinstallation de $($Game.Name) demandée ($src)"
    Set-Status "Désinstallation de $($Game.Name) : suis les instructions de $src. La liste se mettra à jour toute seule."
    Watch-Uninstall $Game
}

# Toutes les 10 s pendant 15 min : le jeu a disparu ? la bibliothèque est rafraîchie
function Watch-Uninstall($Game) {
    $t = New-Object System.Windows.Threading.DispatcherTimer
    $t.Interval = [TimeSpan]::FromSeconds(10)
    $t.Tag = @{ Name = $Game.Name; Exe = [string]@($Game.Exes)[0]; Until = (Get-Date).AddMinutes(15) }
    $t.Add_Tick({
        param($s, $e)
        $x = $s.Tag
        if ((Get-Date) -gt $x.Until) { $s.Stop(); return }
        if ($x.Exe -and (Test-Path -LiteralPath $x.Exe)) { return }
        $s.Stop()
        try { Update-GameCache } catch { Write-Log "Bibliothèque: $_" }
        Set-Status "$($x.Name) est désinstallé."
        Write-Log "Bibliothèque: $($x.Name) désinstallé"
    })
    $t.Start()
}

# ---------------------------------------------------------------------------
# Restes de jeux désinstallés : Steam garde parfois le dossier d'un jeu après l'avoir désinstallé
# (sauvegardes, fichiers ajoutés, mods). Pas des jeux : la place qu'ils prennent est signalée, et ils
# peuvent être supprimés définitivement (sans passer par la corbeille, pour libérer la place tout de suite).
# ---------------------------------------------------------------------------
$LeftoverSizeWork = {
    param($dirs)
    foreach ($d in $dirs) {
        $sum = 0.0
        try { foreach ($f in [IO.Directory]::EnumerateFiles($d, '*', 'AllDirectories')) { try { $sum += (New-Object IO.FileInfo $f).Length } catch {} } } catch {}
        "$d|$sum"
    }
}

function Update-LeftoverBar {
    $list = @($script:Leftovers | Where-Object { $_ -and (Test-Path -LiteralPath $_.Dir) })
    if (-not $list.Count) { $ui.LibLeftoverBar.Visibility = 'Collapsed'; return }
    if (-not $script:LeftoverSizes) { $script:LeftoverSizes = @{} }
    $missing = @($list | Where-Object { -not $script:LeftoverSizes.ContainsKey($_.Dir) } | ForEach-Object { $_.Dir })
    if ($missing.Count -and -not $script:LeftoverJob) {
        # Taille calculée en arrière plan (un dossier peut contenir des centaines de milliers de fichiers)
        $ps = [PowerShell]::Create(); $ps.RunspacePool = $script:BgPool
        [void]$ps.AddScript($LeftoverSizeWork.ToString()).AddArgument($missing)
        $script:LeftoverJob = @{ PS = $ps; Handle = $ps.BeginInvoke() }
        $t = New-Object System.Windows.Threading.DispatcherTimer
        $t.Interval = [TimeSpan]::FromMilliseconds(500)
        $t.Add_Tick({
            param($s, $e)
            $j = $script:LeftoverJob
            if (-not $j -or -not $j.Handle.IsCompleted) { return }
            $s.Stop(); $script:LeftoverJob = $null
            try { foreach ($l in @($j.PS.EndInvoke($j.Handle))) { $x = ([string]$l) -split '\|'; $script:LeftoverSizes[$x[0]] = [double]$x[1] } } catch {} finally { $j.PS.Dispose() }
            Update-LeftoverBar
        })
        $t.Start()
    }
    # Affiché une fois les tailles connues, seulement pour les dossiers qui pèsent (les petits ne gardent que des réglages)
    if ($missing.Count) { $ui.LibLeftoverBar.Visibility = 'Collapsed'; return }
    $big = @(Get-BigLeftovers)
    if (-not $big.Count) { $ui.LibLeftoverBar.Visibility = 'Collapsed'; return }
    $size = ($big | ForEach-Object { $script:LeftoverSizes[$_.Dir] } | Measure-Object -Sum).Sum
    $names = (@($big | Select-Object -First 3 | ForEach-Object { $_.Name }) -join ', ') + $(if ($big.Count -gt 3) { '...' })
    $ui.LibLeftoverText.Text = "$($big.Count) jeu$(if ($big.Count -gt 1) {'x'}) désinstallé$(if ($big.Count -gt 1) {'s'}) $(if ($big.Count -gt 1) { 'ont' } else { 'a' }) laissé $(if ($big.Count -gt 1) { 'leur dossier' } else { 'son dossier' }) sur le disque ($names) : $(Format-Size $size) à récupérer."
    $ui.LibLeftoverBar.Visibility = 'Visible'
    if ($ui.TestOverlay.Visibility -eq 'Visible' -and $script:LeftoverPanelOpen) { Show-Leftovers }
}

# Restes de plus de 50 Mo, les plus gros d'abord
function Get-BigLeftovers {
    if (-not $script:LeftoverSizes) { return @() }
    @($script:Leftovers | Where-Object { $_ -and $script:LeftoverSizes.ContainsKey($_.Dir) -and $script:LeftoverSizes[$_.Dir] -ge 50MB -and (Test-Path -LiteralPath $_.Dir) } |
        Sort-Object @{ Expression = { $script:LeftoverSizes[$_.Dir] }; Descending = $true })
}

function Show-Leftovers {
    if ($script:TestRunning) { return }
    $list = @(Get-BigLeftovers)
    $script:LeftoverPanelOpen = $true
    Show-TestPanel @{ Tag = 'DEL'; Title = 'Restes de jeux désinstallés'; Sub = 'Dossiers que Steam a laissés après la désinstallation' }
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    Set-TestState 'info' "$($list.Count) dossier$(if ($list.Count -gt 1) {'s'})"
    $body = $ui.TestBody
    $intro = New-Text 'Ces jeux ne sont plus installés (Steam ne les connaît plus), mais leur dossier est resté sur le disque. La suppression est définitive : ils ne passent pas par la corbeille, la place est libérée tout de suite et ils ne peuvent pas être récupérés. S''ils contiennent des sauvegardes ou des mods, ouvre les pour vérifier avant.' 12.5 '#A6A1BC'
    $intro.Margin = New-Thickness 0 0 0 10
    [void]$body.Children.Add($intro)
    if ($list.Count -gt 1) {
        $total = ($list | ForEach-Object { $script:LeftoverSizes[$_.Dir] } | Measure-Object -Sum).Sum
        $all = New-Button "Tout supprimer définitivement ($(Format-Size $total))" 'BtnPrimary'
        $all.HorizontalAlignment = 'Left'; $all.Margin = New-Thickness 0 0 0 12
        $all.IsEnabled = -not $script:LeftoverDelete
        $all.Add_Click({ Invoke-Safe { Remove-Leftovers @(Get-BigLeftovers) } })
        [void]$body.Children.Add($all)
    }
    if ($script:LeftoverDelete) { Set-TestState 'run' 'Suppression en cours' }
    foreach ($l in $list) {
        $card = New-Card
        $g = New-Grid @('*', 'Auto', 'Auto')
        $sp = New-Object System.Windows.Controls.StackPanel
        $sz = if ($script:LeftoverSizes -and $script:LeftoverSizes.ContainsKey($l.Dir)) { Format-Size $script:LeftoverSizes[$l.Dir] } else { 'taille en cours de calcul' }
        [void]$sp.Children.Add((New-Text "$($l.Name)  ·  $sz" 14 '#FFFFFF' -Semi))
        $p = New-Text $l.Dir 11.5 '#655E7E'
        $p.TextTrimming = 'CharacterEllipsis'; $p.TextWrapping = 'NoWrap'
        [void]$sp.Children.Add($p)
        Add-ToGrid $g $sp 0
        $ob = New-Button 'Ouvrir'
        $ob.Margin = New-Thickness 12 0 0 0; $ob.VerticalAlignment = 'Center'; $ob.Tag = $l.Dir
        $ob.Add_Click({ param($s, $e) $d = [string]$s.Tag; Invoke-Safe { Open-Url $d } })
        Add-ToGrid $g $ob 1
        $db = New-Button 'Supprimer'
        $db.Margin = New-Thickness 8 0 0 0; $db.VerticalAlignment = 'Center'; $db.Tag = $l
        $db.IsEnabled = -not $script:LeftoverDelete
        $db.Add_Click({ param($s, $e) $x = $s.Tag; Invoke-Safe { Remove-Leftovers @($x) } })
        Add-ToGrid $g $db 2
        $card.Child = $g
        [void]$body.Children.Add($card)
    }
}

# Suppression définitive, en arrière plan (un dossier peut compter des centaines de milliers de fichiers).
# Les fichiers en lecture seule sont débloqués si la première tentative échoue.
$LeftoverDeleteWork = {
    param($dirs, $state)
    foreach ($d in $dirs) {
        $state.Current = $d
        $err = ''
        try { [IO.Directory]::Delete($d, $true) }
        catch {
            try {
                foreach ($f in [IO.Directory]::EnumerateFiles($d, '*', 'AllDirectories')) { try { [IO.File]::SetAttributes($f, 'Normal') } catch {} }
                [IO.Directory]::Delete($d, $true)
            } catch { $err = $_.Exception.Message }
        }
        if (-not $err -and [IO.Directory]::Exists($d)) { $err = 'des fichiers sont encore utilisés' }
        [void]$state.Results.Add("$d|$err")
        $state.Done++
    }
}

function Remove-Leftovers([array]$List, [switch]$Force) {
    if ($script:LeftoverDelete) { return }
    $installed = @($script:Games | ForEach-Object { [string]$_.Dir } | Where-Object { $_ } | ForEach-Object { $_.ToLower() })
    $known = @($script:Leftovers | ForEach-Object { ([string]$_.Dir).ToLower() })
    # Garde fous : un dossier de jeu dans « steamapps\common » (jamais ce dossier lui même), signalé comme
    # reste de jeu désinstallé, et qui n'appartient à aucun jeu installé
    $ok = @($List | Where-Object {
        $d = [string]$_.Dir
        $d -match '(?i)\\steamapps\\common\\[^\\]+$' -and $known -contains $d.ToLower() -and $installed -notcontains $d.ToLower() -and (Test-Path -LiteralPath $d)
    })
    if (-not $ok.Count) { Show-Message 'Aucun de ces dossiers ne peut être supprimé par Nevermind.' 'Warning'; return }
    $size = ($ok | ForEach-Object { [double]$script:LeftoverSizes[$_.Dir] } | Measure-Object -Sum).Sum
    $what = if ($ok.Count -eq 1) { "le dossier de « $($ok[0].Name) »" } else { "$($ok.Count) dossiers de jeux désinstallés" }
    if (-not $Force -and -not (Confirm-Action "Supprimer définitivement $what ($(Format-Size $size)) ?`n`nIls ne passent pas par la corbeille : la place est libérée tout de suite, mais ils ne pourront pas être récupérés. Les sauvegardes ou mods qu'ils contiennent seront perdus.")) { return }
    $state = [hashtable]::Synchronized(@{ Done = 0; Current = ''; Results = [Collections.ArrayList]::Synchronized((New-Object Collections.ArrayList)) })
    $ps = [PowerShell]::Create(); $ps.RunspacePool = $script:BgPool
    [void]$ps.AddScript($LeftoverDeleteWork.ToString()).AddArgument(@($ok | ForEach-Object { [string]$_.Dir })).AddArgument($state)
    $script:LeftoverDelete = @{ PS = $ps; Handle = $ps.BeginInvoke(); State = $state; Items = $ok; Size = $size }
    Write-Log "Bibliothèque: suppression définitive de $($ok.Count) reste(s) de jeux ($(Format-Size $size))"
    if ($ui.TestOverlay.Visibility -eq 'Visible' -and $script:LeftoverPanelOpen) { Show-Leftovers }
    $t = New-Object System.Windows.Threading.DispatcherTimer
    $t.Interval = [TimeSpan]::FromMilliseconds(400)
    $t.Add_Tick({ param($s, $e) try { Receive-LeftoverDelete $s } catch { $s.Stop(); $script:LeftoverDelete = $null; Write-Log "Bibliothèque: $_" } })
    $t.Start()
}

function Receive-LeftoverDelete($Timer) {
    $j = $script:LeftoverDelete
    if (-not $j) { $Timer.Stop(); return }
    $st = $j.State
    $cur = @($j.Items | Where-Object { $_.Dir -eq $st.Current })[0]
    if (-not $j.Handle.IsCompleted) {
        Set-Status "Suppression des restes de jeux : $([math]::Min($st.Done + 1, $j.Items.Count)) sur $($j.Items.Count)$(if ($cur) { " ($($cur.Name))" })..."
        return
    }
    $Timer.Stop()
    try { [void]$j.PS.EndInvoke($j.Handle) } catch {} finally { $j.PS.Dispose() }
    $script:LeftoverDelete = $null
    $freed = 0.0; $fail = @()
    foreach ($r in @($st.Results)) {
        $x = ([string]$r) -split '\|', 2
        $it = @($j.Items | Where-Object { $_.Dir -eq $x[0] })[0]
        if ($x[1]) { $fail += "$(if ($it) { $it.Name } else { $x[0] }) : $($x[1])" }
        else { $freed += [double]$script:LeftoverSizes[$x[0]]; $script:Leftovers = @($script:Leftovers | Where-Object { $_.Dir -ne $x[0] }) }
    }
    Write-Log "Bibliothèque: $(Format-Size $freed) libérés$(if ($fail.Count) { ", échecs : $($fail -join ' | ')" })"
    Set-Status "$(Format-Size $freed) libérés sur le disque."
    Update-LeftoverBar
    if ($script:LeftoverPanelOpen) { if (@(Get-BigLeftovers).Count) { Show-Leftovers } else { $script:LeftoverPanelOpen = $false; Hide-TestPanel } }
    if (-not $script:TestRunning) {
        $lines = @("$(Format-Size $freed) libérés sur le disque.")
        if ($fail.Count) { $lines += 'Pas supprimés (ferme le jeu ou son launcher, puis réessaie) :'; $lines += @($fail | ForEach-Object { "•  $_" }) }
        Show-ResultSheet $(if ($fail.Count) { 'Suppression terminée, avec des exceptions' } else { 'Place libérée' }) $lines $null $null
    }
    $script:LastLeftoverFreed = $freed
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
        $b.Background = Get-Brush $(if ($on) { $Colors.accent } else { '#16FFFFFF' })
        $b.Cursor = [System.Windows.Input.Cursors]::Hand
        $t = New-Text "$($c[0]) ($($c[1]))" 12 $(if ($on) { '#07060C' } else { '#D3CDE3' }) -Semi
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
        $e = New-Text $(if ($all.Count) { 'Aucun jeu ne correspond.' } else { 'Aucun jeu trouvé sur ce PC. Clique sur « Ajouter un jeu » pour ajouter le tien.' }) 13 '#A6A1BC'
        $e.Margin = New-Thickness 4 8 0 0
        [void]$ui.LibGrid.Children.Add($e)
    }
    foreach ($g in $shown) { [void]$ui.LibGrid.Children.Add((New-LibraryTile $g)) }
    if (-not $script:LibSelected -or -not @($shown | Where-Object { $_.Name -eq $script:LibSelected }).Count) { $script:LibSelected = if ($shown.Count) { $shown[0].Name } else { $null } }
    Set-LibrarySelection $script:LibSelected
    try { Update-LeftoverBar } catch { Write-Log "Bibliothèque: $_" }
    try { Start-CoverDownload } catch { Write-Log "Jaquettes: $_" }
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
        $bd.Child = (New-Text 'En jeu' 11 '#07060C' -Bold)
        [void]$g.Children.Add($bd)
    }
    $frame.Child = $g
    $move = New-Object System.Windows.Media.TranslateTransform
    $frame.RenderTransform = $move
    [void]$tile.Children.Add($frame)
    $n = New-Text $Game.Name 12.5 '#EEEBF7' -Semi
    $n.TextTrimming = 'CharacterEllipsis'; $n.TextWrapping = 'NoWrap'; $n.Margin = New-Thickness 2 6 0 0; $n.ToolTip = $Game.Name
    [void]$tile.Children.Add($n)
    $src = New-Text $(if ($Game.Source) { $Game.Source } else { 'Steam' }) 11 '#655E7E'
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
        if ($k -eq $Name) { $f.BorderBrush = Get-Brush $Colors.accent; $f.Effect = New-Glow $Colors.accent 14 0.6 }
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
        $e = New-Text 'Choisis un jeu à gauche.' 13 '#A6A1BC'
        $e.Margin = New-Thickness 20 20 20 20
        [void]$p.Children.Add($e)
        return
    }
    # Bannière (image large de Steam, sinon dégradé aux couleurs du jeu)
    $hero = New-Object System.Windows.Controls.Border
    $hero.Height = 150
    $img = Get-ImageBrush (Get-HeroFile $g) 720
    $hero.Background = if ($img) { $img } else { New-LinearBrush @((Get-GameHue $g.Name), '#15121E') 0 0 1 1 }
    $hg = New-Object System.Windows.Controls.Grid
    $shade = New-Object System.Windows.Controls.Border
    $shade.Background = New-LinearBrush @('#00000000', '#E615121E') 0 0 0 1
    [void]$hg.Children.Add($shade)
    $logo = Get-LogoFile $g
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
    $meta = New-Text "$(if ($g.Source) { $g.Source } else { 'Steam' })  ·  dernière partie : $(Format-LastPlayed $(if ($pl) { $pl.Last } else { '' }))" 12 '#A6A1BC'
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
    $hw = New-Text $(if ($how.Kind -eq 'url' -and $g.Source) { "Lancé par $($g.Source) (connexion, mises à jour et anti triche comme d'habitude)." } else { 'Lancé directement depuis son dossier.' }) 11 '#655E7E'
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
        [void]$c.Children.Add((New-Text $cells[$i][0] 10.5 '#655E7E' -Semi))
        [void]$c.Children.Add((New-Text $cells[$i][1] 16 '#FFFFFF' -Bold))
        Add-ToGrid $stats $c $i
    }
    [void]$body.Children.Add($stats)
    if ($last) {
        $lv = Get-FpsVerdict $last
        $lk = New-Button "Dernière partie mesurée : $('{0:N0}' -f $last.Avg) FPS, 1 % bas $('{0:N0}' -f $last.Low1). Voir le détail"
        $lk.Margin = New-Thickness 0 10 0 0; $lk.HorizontalAlignment = 'Stretch'; $lk.HorizontalContentAlignment = 'Left'
        $lk.Foreground = Get-Brush $(if ($lv[0] -eq 'ok') { '#D3CDE3' } else { $Colors.warn })
        $lk.Tag = [string]$last.Id
        $lk.Add_Click({ param($s, $e) $id = [string]$s.Tag; Invoke-Safe { Show-FpsSession $id } })
        [void]$body.Children.Add($lk)
    }

    # Optimisation du jeu
    $sec = New-SectionTitle 'OPTIMISATION DU JEU'
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
    if ($g.Custom) { $acts += , @('Retirer de la liste', 'remove') } else { $acts += , @('Désinstaller', 'uninstall') }
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
                    'uninstall' { Uninstall-LibraryGame $gm }
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
        $list += @{ Id = 'priority'; Title = 'Priorité haute'; Text = 'Le jeu passe avant les autres programmes (même Nevermind fermé).'; Ok = (Test-GamePriority $Game); Switch = $true }
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
    $dot.Fill = Get-Brush $(if ($It.Ok) { $Colors.ok } elseif ($It.Optional) { '#655E7E' } else { $Colors.warn })
    Add-ToGrid $row $dot 0
    $sp = New-Object System.Windows.Controls.StackPanel
    [void]$sp.Children.Add((New-Text $It.Title 13 '#FFFFFF' -Semi))
    $tx = New-Text $It.Text 11.5 '#A6A1BC'
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
    else { Show-ResultSheet "$($Game.Name) est optimisé" (@('Fait :') + @($done | ForEach-Object { "•  $_" }) + @('Joue une partie : Nevermind mesurera tes FPS pour vérifier.')) $null $null }
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
    $ui.ChkLibCovers.IsChecked = Test-CoversOnline
    $ui.ChkLibCovers.Add_Click({
        $on = [bool]$ui.ChkLibCovers.IsChecked
        Set-Setting 'LibCoversOnline' $on
        if ($on) { Start-CoverDownload } else { Set-Status 'Jaquettes depuis Internet désactivées (celles déjà trouvées restent).' }
    })
    $ui.BtnLibLeftovers.Add_Click({ Invoke-Safe { Show-Leftovers } })
    $ui.BtnLibAdd.Add_Click({ Invoke-Safe { Add-CustomGame; Update-LibraryView } })
    $ui.BtnLibRefresh.Add_Click({ Invoke-Safe { Set-Status 'Recherche de tes jeux...'; Update-GameCache; Update-LibraryView; Set-Status "$(@(Get-LibraryGames).Count) jeux trouvés." } })
}
