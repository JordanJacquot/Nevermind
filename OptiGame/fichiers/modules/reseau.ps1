# OptiGame : section Réseau : scan des appareils et fiche détaillée.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Section Réseau : scan des appareils connectés
# ---------------------------------------------------------------------------
$OuiFile = Join-Path $DataDir 'fabricants.txt'
$KnownFile = Join-Path $DataDir 'appareils.json'
$AuditFile = Join-Path $DataDir 'audit.json'

$NetScanWork = {
    param($a)
    try {
        [OGNative]::Phase = 'ping'
        $alive = @([OGNative]::PingSweep([string[]]$a.Ips, 800))
        if ([OGNative]::Cancel) { return @{ Cancelled = $true } }
        [OGNative]::Phase = 'arp'
        [OGNative]::Progress = 72
        Start-Sleep -Milliseconds 300
        # Seulement les vrais appareils du réseau (pas les adresses techniques multicast / broadcast)
        $inNet = @{}; foreach ($x in $a.Ips) { $inNet[$x] = $true }
        $arp = @(Get-NetNeighbor -InterfaceIndex $a.If -AddressFamily IPv4 -ErrorAction SilentlyContinue |
            Where-Object { $inNet.ContainsKey([string]$_.IPAddress) -and $_.State -notin 'Unreachable', 'Incomplete', 'Permanent' -and $_.LinkLayerAddress -and $_.LinkLayerAddress -notmatch '^(00-00-00-00-00-00|FF-FF-FF-FF-FF-FF|01-00-5E.*)$' } |
            ForEach-Object { "$($_.IPAddress)|$($_.LinkLayerAddress)|$($_.State)" })
        [OGNative]::Phase = 'names'
        [OGNative]::Progress = 80
        $ips = @(@($alive | ForEach-Object { ($_ -split '\|')[0] }) + @($arp | ForEach-Object { ($_ -split '\|')[0] }) | Select-Object -Unique)
        $names = @([OGNative]::ResolveNames([string[]]$ips, 2500))
        [OGNative]::Phase = 'vendors'
        [OGNative]::Progress = 92
        if (-not (Test-Path -LiteralPath $a.Oui)) {
            try {
                [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
                $tmp = "$($a.Oui).csv"
                Invoke-WebRequest 'https://standards-oui.ieee.org/oui/oui.csv' -OutFile $tmp -UseBasicParsing -Headers @{ 'User-Agent' = 'Mozilla/5.0 OptiGame' } -TimeoutSec 60
                Import-Csv -LiteralPath $tmp | ForEach-Object { "$($_.Assignment)|$($_.'Organization Name')" } | Set-Content -LiteralPath $a.Oui -Encoding UTF8
                Remove-Item -LiteralPath $tmp -ErrorAction SilentlyContinue
            } catch {}
        }
        $ouiMap = $null
        if ($a.LoadOui -and (Test-Path -LiteralPath $a.Oui)) {
            $ouiMap = @{}
            foreach ($l in [IO.File]::ReadLines($a.Oui)) { $i = $l.IndexOf('|'); if ($i -gt 0) { $ouiMap[$l.Substring(0, $i).Trim([char]0xFEFF)] = $l.Substring($i + 1) } }
        }
        [OGNative]::Progress = 100
        @{ Alive = $alive; Arp = $arp; Names = $names; OuiMap = $ouiMap }
    } catch { @{ Error = $_.Exception.GetBaseException().Message } }
}

function Get-SubnetIps([string]$Ip, [int]$Prefix) {
    if ($Prefix -lt 24) { $Prefix = 24 }
    $b = ([Net.IPAddress]::Parse($Ip)).GetAddressBytes(); [array]::Reverse($b)
    $n = [double][BitConverter]::ToUInt32($b, 0)
    $size = [math]::Pow(2, 32 - $Prefix)
    $net = [math]::Floor($n / $size) * $size
    for ($i = 1; $i -lt $size - 1; $i++) {
        $bb = [BitConverter]::GetBytes([uint32]($net + $i)); [array]::Reverse($bb)
        ([Net.IPAddress]::new($bb)).ToString()
    }
}

function Get-Vendor([string]$Mac) {
    if (-not $Mac) { return '' }
    $hex = ($Mac -replace '[-:]', '').ToUpper()
    if ($hex.Length -lt 6) { return '' }
    if ([Convert]::ToInt32($hex.Substring(1, 1), 16) -band 2) { return 'Adresse privée' }
    if (-not $script:Oui -and (Test-Path -LiteralPath $OuiFile)) {
        $script:Oui = @{}
        foreach ($l in [IO.File]::ReadLines($OuiFile)) { $i = $l.IndexOf('|'); if ($i -gt 0) { $script:Oui[$l.Substring(0, $i).Trim([char]0xFEFF)] = $l.Substring($i + 1) } }
    }
    if (-not $script:Oui) { return '' }
    $v = [string]$script:Oui[$hex.Substring(0, 6)]
    $v = $v -replace '(?i)[,\s]+(inc|incorporated|co|ltd|corporation|corp|gmbh|s\.?a\.?s|sarl|s\.a|limited|llc|b\.v|ag|oy|ab)\b\.?', ''
    $v = $v -replace '(?i)\s+(technologies|technology|electronics|communications|broadband)\b', ''
    $v.Trim(' ', ',', '.')
}

function Get-DeviceKind($D) {
    $t = "$($D.Host) $($D.Vendor) $($D.MdnsHost) $($D.Announced) $($D.Maker) $($D.Model) $($D.UpnpName) $($D.WebTitle)"
    $svc = [string]$D.SvcText
    if ($D.Self) { return @{ Kind = 'Ce PC'; Glyph = 0xE7F4; Color = $Colors.info } }
    if ($D.Gateway) { return @{ Kind = 'Box Internet'; Glyph = 0xE80F; Color = $Colors.ok } }
    if ($D.Camera) { return @{ Kind = 'Caméra'; Glyph = 0xE714; Color = $Colors.warn } }
    if ($svc -match '(?i)_ipp|_printer|_pdl-datastream|PrintDevice|\bPrinter\b') { return @{ Kind = 'Imprimante'; Glyph = 0xE749; Color = '#9AA3B2' } }
    if ($t -match '(?i)\brt-|router|routeur|archer|\bdeco\b|orbi|mesh|access.?point|repeater|répéteur|ubiquiti|unifi') { return @{ Kind = 'Routeur ou répéteur Wi-Fi'; Glyph = 0xE774; Color = $Colors.ok } }
    if ($D.NbName -or $svc -match '(?i)pub:Computer|_workstation|_smb\b') { return @{ Kind = 'Ordinateur'; Glyph = 0xE7F4; Color = $Colors.info } }
    if ($svc -match '(?i)_companion-link') { return @{ Kind = 'Appareil Apple'; Glyph = 0xE8EA; Color = '#B18CFF' } }
    if ($svc -match '(?i)_googlecast|_amzn-wplay|_androidtvremote2|MediaRenderer|_mediaremotetv|_airplay') { return @{ Kind = 'TV ou multimédia'; Glyph = 0xE7F4; Color = $Colors.warn } }
    if ($svc -match '(?i)_sonos|_raop') { return @{ Kind = 'Enceinte ou audio'; Glyph = 0xE7F5; Color = '#FF7AB6' } }
    if ($svc -match '(?i)_hap|_homekit|_matter|_hue|_alexa') { return @{ Kind = 'Objet connecté'; Glyph = 0xE80F; Color = '#4EA8FF' } }
    if ($t -match '(?i)\brt-|router|routeur|archer|\bdeco\b|orbi|mesh|access.?point|repeater|répéteur|ubiquiti|unifi') { return @{ Kind = 'Routeur ou répéteur Wi-Fi'; Glyph = 0xE774; Color = $Colors.ok } }
    if ($t -match '(?i)iphone|ipad|android|galaxy|pixel|redmi|oneplus|oppo|honor|phone|motorola|poco') { return @{ Kind = 'Téléphone ou tablette'; Glyph = 0xE8EA; Color = '#B18CFF' } }
    if ($t -match '(?i)playstation|\bps[345]\b|sony interactive|nintendo|xbox|switch') { return @{ Kind = 'Console de jeu'; Glyph = 0xE7FC; Color = '#FF7AB6' } }
    if ($t -match '(?i)webos|\btv\b|tizen|bravia|androidtv|chromecast|roku|fire.?tv|lg innotek|hisense|\btcl\b') { return @{ Kind = 'TV ou multimédia'; Glyph = 0xE7F4; Color = $Colors.warn } }
    if ($t -match '(?i)printer|imprimante|hewlett|\bhp\b|canon|epson|brother|lexmark|kyocera') { return @{ Kind = 'Imprimante'; Glyph = 0xE749; Color = '#9AA3B2' } }
    if ($t -match '(?i)sagemcom|sercomm|arcadyan|technicolor|freebox|livebox|bbox|decodeur|décodeur') { return @{ Kind = 'Box ou décodeur TV'; Glyph = 0xE80F; Color = $Colors.ok } }
    if ($t -match '(?i)espressif|tuya|shelly|sonoff|signify|philips lighting|amazon|google|nest|ring|meross|netatmo|tapo|xiaomi') { return @{ Kind = 'Objet connecté'; Glyph = 0xE80F; Color = '#4EA8FF' } }
    if ($t -match '(?i)\bapple\b') { return @{ Kind = 'Appareil Apple'; Glyph = 0xE8EA; Color = '#B18CFF' } }
    if ($t -match '(?i)desktop|laptop|\bpc|asustek|micro-star|gigabyte|dell|lenovo|acer|intel|realtek|killer') { return @{ Kind = 'Ordinateur'; Glyph = 0xE7F4; Color = $Colors.info } }
    if ($D.Vendor -eq 'Adresse privée') { return @{ Kind = 'Téléphone probable'; Glyph = 0xE8EA; Color = '#B18CFF' } }
    @{ Kind = 'Appareil'; Glyph = 0xE774; Color = '#9AA3B2' }
}

function New-NetRadar([switch]$Spin) {
    $g = New-Object System.Windows.Controls.Grid
    $g.Width = 200; $g.Height = 200
    $g.HorizontalAlignment = 'Center'
    foreach ($r in 96, 68, 40) {
        $e = New-Object System.Windows.Shapes.Ellipse
        $e.Width = $r * 2; $e.Height = $r * 2
        $e.Stroke = Get-Brush '#1F2633'; $e.StrokeThickness = 1.5
        [void]$g.Children.Add($e)
    }
    $dots = New-Object System.Windows.Controls.Canvas
    $dots.Width = 200; $dots.Height = 200
    [void]$g.Children.Add($dots)
    $sweep = New-Object System.Windows.Shapes.Path
    $sweep.Data = Get-ArcGeometry 100 94 -90 70
    $sweep.Stroke = Get-Brush $Colors.info; $sweep.StrokeThickness = 5
    $sweep.StrokeStartLineCap = 'Round'; $sweep.StrokeEndLineCap = 'Round'
    $sweep.Effect = New-Glow $Colors.info 18 0.9
    $sweep.CacheMode = New-Object System.Windows.Media.BitmapCache
    $sweep.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5)
    $rot = New-Object System.Windows.Media.RotateTransform
    $sweep.RenderTransform = $rot
    $sweep.Visibility = if ($Spin) { 'Visible' } else { 'Collapsed' }
    [void]$g.Children.Add($sweep)
    $center = New-Object System.Windows.Controls.Border
    $center.Width = 64; $center.Height = 64
    $center.CornerRadius = [System.Windows.CornerRadius]::new(32)
    $center.Background = Get-Brush '#1A2A40'
    $center.BorderBrush = Get-Brush $Colors.info; $center.BorderThickness = New-Thickness 2 2 2 2
    $center.Effect = New-Glow $Colors.info 20 0.5
    $num = New-Text '' 22 '#FFFFFF' -Bold
    $num.HorizontalAlignment = 'Center'; $num.VerticalAlignment = 'Center'
    $icon = New-Object System.Windows.Controls.TextBlock
    $icon.Text = [string][char]0xE774
    $icon.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
    $icon.FontSize = 26; $icon.Foreground = Get-Brush $Colors.info
    $icon.HorizontalAlignment = 'Center'; $icon.VerticalAlignment = 'Center'
    $inner = New-Object System.Windows.Controls.Grid
    [void]$inner.Children.Add($icon)
    [void]$inner.Children.Add($num)
    $center.Child = $inner
    $center.HorizontalAlignment = 'Center'; $center.VerticalAlignment = 'Center'
    [void]$g.Children.Add($center)
    if ($Spin) {
        $a = New-Object System.Windows.Media.Animation.DoubleAnimation
        $a.From = 0; $a.To = 360
        $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds(1400))
        $a.RepeatBehavior = [System.Windows.Media.Animation.RepeatBehavior]::Forever
        $rot.BeginAnimation([System.Windows.Media.RotateTransform]::AngleProperty, $a)
    }
    @{ El = $g; Dots = $dots; Rot = $rot; Sweep = $sweep; Num = $num; Icon = $icon; Shown = 0 }
}

# Ajoute un point lumineux sur le radar pour chaque appareil trouvé.
$script:Rnd = New-Object System.Random
function Add-RadarDot($Radar) {
    $rnd = $script:Rnd
    $ang = $rnd.NextDouble() * 2 * [math]::PI
    $dist = 45 + $rnd.NextDouble() * 45
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 9; $dot.Height = 9
    $dot.Fill = Get-Brush '#FFFFFF'
    $dot.Effect = New-Glow $Colors.info 14 1
    [System.Windows.Controls.Canvas]::SetLeft($dot, 100 + $dist * [math]::Cos($ang) - 4.5)
    [System.Windows.Controls.Canvas]::SetTop($dot, 100 + $dist * [math]::Sin($ang) - 4.5)
    $dot.Opacity = 0
    [void]$Radar.Dots.Children.Add($dot)
    Start-WpfAnim $dot ([System.Windows.UIElement]::OpacityProperty) 1 400
}

function Update-NetScanInfo {
    $ui.NetScanInfo.Children.Clear()
    $net = Get-ActiveNet
    if (-not $net) { [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Connexion' 'Aucune' 'bad')); return }
    $ipInfo = if ($net.Ip) { @{ IPAddress = $net.Ip; PrefixLength = [int]$net.Prefix } } else { $null }
    if ($net.Wifi) {
        $w = netsh wlan show interfaces 2>$null
        $ssid = ($w | Where-Object { $_ -match '^\s+SSID\s+:\s+(.+)$' } | Select-Object -First 1) -replace '^\s+SSID\s+:\s+', ''
        $sig = ($w | Where-Object { $_ -match '^\s+Signal\s+:\s+(\d+)' } | Select-Object -First 1) -replace '^\s+Signal\s+:\s+', ''
        [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Connexion' "Wi-Fi « $ssid »  ($sig)" $(if ([int]($sig -replace '\D', '') -ge 60) { 'ok' } else { 'warn' })))
    } else {
        [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Connexion' "Câble Ethernet, $($net.Speed)" 'ok'))
    }
    if ($ipInfo) {
        [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Adresse de ce PC' $ipInfo.IPAddress 'info'))
        $count = [math]::Pow(2, 32 - [math]::Max(24.0, $ipInfo.PrefixLength)) - 2
        [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Taille du réseau' "$count adresses possibles" 'info'))
    }
    [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Box' $net.Gateway 'ok'))
    $pub = Invoke-Async { try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; [string](Invoke-RestMethod 'https://api.ipify.org' -TimeoutSec 5) } catch { '' } } | Select-Object -First 1
    if ("$pub") { $script:PublicIp = "$pub"; [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Adresse Internet' "$pub" 'info')) }
}

function Show-NetHeroIdle {
    $ui.NetHero.Children.Clear()
    $r = New-NetRadar
    [void]$ui.NetHero.Children.Add($r.El)
    $t = New-Text 'Prêt à scanner ton réseau' 14 '#FFFFFF' -Semi
    $t.HorizontalAlignment = 'Center'; $t.Margin = New-Thickness 0 12 0 0
    [void]$ui.NetHero.Children.Add($t)
    $h = New-Text 'Clique sur « Scanner le réseau » en haut à droite.' 12.5 '#9AA3B2'
    $h.HorizontalAlignment = 'Center'; $h.Margin = New-Thickness 0 4 0 0
    [void]$ui.NetHero.Children.Add($h)
}

function New-DeviceTile($D, [int]$Index) {
    $card = New-Card
    $card.Padding = New-Thickness 16 14 16 14
    $card.Margin = New-Thickness 0 0 12 12
    $g = New-Grid @('Auto', '*')
    $k = $D.KindInfo
    $ic = New-Object System.Windows.Controls.Border
    $ic.Width = 46; $ic.Height = 46
    $ic.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $bg = Get-Brush $k.Color; $bg.Opacity = 0.15
    $ic.Background = $bg
    $ic.VerticalAlignment = 'Top'
    $gl = New-Object System.Windows.Controls.TextBlock
    $gl.Text = [string][char]$k.Glyph
    $gl.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
    $gl.FontSize = 20; $gl.Foreground = Get-Brush $k.Color
    $gl.HorizontalAlignment = 'Center'; $gl.VerticalAlignment = 'Center'
    $ic.Child = $gl
    Add-ToGrid $g $ic 0
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Margin = New-Thickness 12 0 0 0
    $name = New-Text $D.Title 14.5 '#FFFFFF' -Semi
    $name.TextTrimming = 'CharacterEllipsis'; $name.TextWrapping = 'NoWrap'; $name.ToolTip = $D.Title
    [void]$sp.Children.Add($name)
    [void]$sp.Children.Add((New-Text $k.Kind 12 '#9AA3B2'))
    $badges = New-Object System.Windows.Controls.WrapPanel
    $badges.Margin = New-Thickness -10 6 0 0
    if ($D.Self) { [void]$badges.Children.Add((New-Badge 'Ce PC' $Colors.info)) }
    if ($D.Gateway) { [void]$badges.Children.Add((New-Badge 'Ta box' $Colors.ok)) }
    if ($D.New) { [void]$badges.Children.Add((New-Badge 'Nouveau' $Colors.warn)) }
    if ($D.Camera) { $cb = New-Badge 'Caméra ?' $Colors.warn; $cb.ToolTip = "Indices : $(@($D.CameraWhy) -join ', ')"; [void]$badges.Children.Add($cb) }
    if ($D.Hidden) { $hb = New-Badge 'Discret' '#B18CFF'; $hb.ToolTip = 'Ne répond pas au ping : trouvé autrement. C''est normal pour beaucoup de téléphones et de PC protégés.'; [void]$badges.Children.Add($hb) }
    if ($null -ne $D.Ms) { [void]$badges.Children.Add((New-Badge $(if ($D.Ms -lt 1) { '< 1 ms' } else { "$($D.Ms) ms" }) '#9AA3B2')) }
    if ($badges.Children.Count) { [void]$sp.Children.Add($badges) }
    $det = New-Text "$($D.Ip)$(if ($D.Model) { '   ' + $D.Model } elseif ($D.Vendor) { '   ' + $D.Vendor })" 11.5 '#5B6475'
    $det.Margin = New-Thickness 0 8 0 0
    $det.TextTrimming = 'CharacterEllipsis'; $det.TextWrapping = 'NoWrap'
    $det.ToolTip = "Adresse : $($D.Ip)`nAdresse physique : $($D.Mac)`nFabricant : $($D.Vendor)"
    [void]$sp.Children.Add($det)
    if ($D.Gateway) {
        $b = New-Button 'Ouvrir la box'
        $b.HorizontalAlignment = 'Left'
        $b.Margin = New-Thickness 0 10 0 0
        $b.Tag = "http://$($D.Ip)"
        $b.Add_Click({ param($s, $e) Open-Url $s.Tag })
        [void]$sp.Children.Add($b)
    }
    Add-ToGrid $g $sp 1
    $card.Child = $g
    if ($D.New) { $card.BorderBrush = Get-Brush $Colors.warn }
    $card.Cursor = [System.Windows.Input.Cursors]::Hand
    $card.Tag = $D
    $card.Add_MouseEnter({ param($s, $e) $s.BorderBrush = Get-Brush $s.Tag.KindInfo.Color; $s.Background = Get-Brush 'card-hover' })
    $card.Add_MouseLeave({ param($s, $e) $s.BorderBrush = Get-Brush $(if ($s.Tag.New) { $Colors.warn } else { 'card-border' }); $s.Background = Get-Brush 'card' })
    $card.Add_MouseLeftButtonUp({ param($s, $e) Invoke-Safe { Show-DeviceDetail $s.Tag } })
    $card.Opacity = 0
    $move = New-Object System.Windows.Media.TranslateTransform 0, 12
    $card.RenderTransform = $move
    Start-WpfAnim $card ([System.Windows.UIElement]::OpacityProperty) 1 450 (70 * $Index)
    Start-WpfAnim $move ([System.Windows.Media.TranslateTransform]::YProperty) 0 450 (70 * $Index)
    $card
}

# Adresses de ce PC sur toutes ses cartes réseau (câble, Wi-Fi...). Windows ne met jamais ses propres
# adresses dans sa liste de voisins : sans ça, une deuxième carte apparaîtrait comme un appareil inconnu.
function Get-LocalInterfaces {
    $r = @{}
    try {
        foreach ($ni in [System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces()) {
            # Même déconnectée, une carte garde souvent son adresse (et Windows y répond lui-même)
            if ([string]$ni.NetworkInterfaceType -eq 'Loopback') { continue }
            $up = [string]$ni.OperationalStatus -eq 'Up'
            $raw = $ni.GetPhysicalAddress().ToString()
            $mac = if ($raw.Length -eq 12) { (($raw -split '(..)') | Where-Object { $_ }) -join '-' } else { '' }
            $wifi = [string]$ni.NetworkInterfaceType -eq 'Wireless80211'
            foreach ($ua in $ni.GetIPProperties().UnicastAddresses) {
                if ([string]$ua.Address.AddressFamily -eq 'InterNetwork') { $r[$ua.Address.ToString()] = @{ Mac = $mac; Name = [string]$ni.Name; Desc = [string]$ni.Description; Wifi = $wifi; Up = $up } }
            }
        }
    } catch {}
    $r
}

# Un appareil du scan qui est en fait une carte réseau de ce PC : même ordinateur, avec sa vraie adresse physique
function Set-LocalDevice($D, $Locals) {
    $li = $Locals[[string]$D.Ip]
    if (-not $li) { return }
    $D.Self = $true
    if ($li.Mac) { $D.Mac = $li.Mac; $D.Vendor = Get-Vendor $li.Mac }
    $D.Host = $env:COMPUTERNAME
    $how = if ($li.Wifi) { 'Wi-Fi' } else { 'câble' }
    $D.Title = if ($li.Up) { "$env:COMPUTERNAME (ce PC, $how)" } else { "$env:COMPUTERNAME (ce PC, ancienne adresse $how)" }
    $D.Adapter = "$($li.Name) : $($li.Desc)$(if (-not $li.Up) { ', déconnectée : Windows répond lui-même à son ancienne adresse' })"
    $D.Hidden = $false; $D.New = $false
    $D.KindInfo = Get-DeviceKind $D
}

function Invoke-NetworkScan {
    if ($script:NetScanning) { return }
    $net = Get-ActiveNet
    if (-not $net) { Show-Message 'Aucune connexion réseau détectée.'; return }
    $ipInfo = if ($net.Ip) { @{ IPAddress = $net.Ip; PrefixLength = [int]$net.Prefix } } else { $null }
    if (-not $ipInfo) { Show-Message 'Impossible de lire l''adresse de ce PC.'; return }
    $ips = @(Get-SubnetIps $ipInfo.IPAddress $ipInfo.PrefixLength)
    $script:NetScanning = $true
    $ui.BtnNetScan.IsEnabled = $false
    Set-Status 'Scan du réseau...'

    $ui.NetHero.Children.Clear()
    $radar = New-NetRadar -Spin
    $radar.Icon.Visibility = 'Collapsed'
    $radar.Num.Text = '0'
    [void]$ui.NetHero.Children.Add($radar.El)
    $phase = New-Text 'Recherche des appareils...' 13 '#9AA3B2' -Semi
    $phase.HorizontalAlignment = 'Center'; $phase.Margin = New-Thickness 0 12 0 8
    [void]$ui.NetHero.Children.Add($phase)
    $bar = New-Object System.Windows.Controls.ProgressBar
    $bar.Width = 220; $bar.Height = 5
    [void]$ui.NetHero.Children.Add($bar)

    [OGNative]::Cancel = $false; [OGNative]::Found = 0; [OGNative]::Progress = 0; [OGNative]::Phase = ''
    $script:NetScanUi = @{ Radar = $radar; Phase = $phase; Bar = $bar }
    $script:NetTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:NetTimer.Interval = [TimeSpan]::FromMilliseconds(120)
    $script:NetTimer.Add_Tick({
        $u = $script:NetScanUi
        $f = [OGNative]::Found
        while ($u.Radar.Shown -lt $f) { Add-RadarDot $u.Radar; $u.Radar.Shown++ }
        $u.Radar.Num.Text = "$f"
        $u.Bar.Value = [OGNative]::Progress
        $u.Phase.Text = switch ([OGNative]::Phase) { 'ping' { 'Recherche des appareils...' } 'arp' { 'Recherche des appareils discrets...' } 'names' { 'Récupération des noms...' } 'vendors' { 'Identification des fabricants...' } default { 'Préparation...' } }
    })
    $script:NetTimer.Start()
    try {
        $r = Invoke-Async $NetScanWork @{ Ips = $ips; If = $net.IfIndex; Oui = $OuiFile; LoadOui = (-not $script:Oui) } | Select-Object -First 1
        if ($r -and $r.OuiMap) { $script:Oui = $r.OuiMap }
    } finally {
        $script:NetTimer.Stop()
        $script:NetScanning = $false
        $ui.BtnNetScan.IsEnabled = $true
    }
    if (-not $r -or $r.Error) {
        Show-NetHeroIdle
        Show-Message "Le scan n'a pas pu se faire : $($r.Error)" 'Warning'
        return
    }

    # Assemblage des appareils
    $inNet = @{}; foreach ($x in $ips) { $inNet[$x] = $true }
    $devs = @{}
    foreach ($l in @($r.Alive)) { $p = ([string]$l) -split '\|'; $devs[$p[0]] = @{ Ip = $p[0]; Ms = [int]$p[1]; Mac = $null; Ttl = $(if ($p.Count -gt 2) { [int]$p[2] } else { 0 }) } }
    foreach ($l in @($r.Arp)) {
        $p = ([string]$l) -split '\|'
        if (-not $inNet.ContainsKey($p[0])) { continue }
        if ($devs.ContainsKey($p[0])) { $devs[$p[0]].Mac = $p[1] }
        elseif ($p[2] -in 'Reachable', 'Stale', 'Delay', 'Probe') { $devs[$p[0]] = @{ Ip = $p[0]; Ms = $null; Mac = $p[1] } }
    }
    $self = $ipInfo.IPAddress
    $selfMac = $net.Mac
    if (-not $devs.ContainsKey($self)) { $devs[$self] = @{ Ip = $self; Ms = 0; Mac = $null } }
    $devs[$self].Mac = $selfMac
    $names = @{}
    foreach ($l in @($r.Names)) { $p = ([string]$l) -split '\|', 2; if ($p[1] -and $p[1] -ne $p[0]) { $names[$p[0]] = ($p[1] -replace '(?i)\.(home|lan|local|localdomain|box|fritz\.box|station|bbox)$', '') } }

    # Appareils déjà vus, avec la date de leur première apparition
    $knownMap = @{}
    if (Test-Path -LiteralPath $KnownFile) {
        try {
            $j = ConvertFrom-Json (Get-Content -LiteralPath $KnownFile -Raw -Encoding UTF8)
            if ($j -is [string]) { $knownMap[$j] = '' }
            elseif ($j -is [array]) { foreach ($m in $j) { $knownMap[[string]$m] = '' } }
            elseif ($j) { foreach ($pp in $j.PSObject.Properties) { $knownMap[$pp.Name] = [string]$pp.Value } }
        } catch {}
    }
    $known = @($knownMap.Keys)
    $first = -not $known.Count
    $locals = Get-LocalInterfaces
    $list = foreach ($d in $devs.Values) {
        $d.Self = $d.Ip -eq $self
        $d.Gateway = $d.Ip -eq $net.Gateway
        $d.Host = if ($d.Self) { $env:COMPUTERNAME } else { [string]$names[$d.Ip] }
        $d.Vendor = Get-Vendor $d.Mac
        $d.KindInfo = Get-DeviceKind $d
        $d.Title = if ($d.Self) { "$env:COMPUTERNAME (ce PC)" } elseif ($d.Host -and $d.Host -ne 'lan') { $d.Host } elseif ($d.Gateway) { 'Box Internet' } elseif ($d.Vendor -and $d.Vendor -ne 'Adresse privée') { $d.Vendor } else { 'Appareil inconnu' }
        $d.New = (-not $first) -and $d.Mac -and ($known -notcontains $d.Mac) -and -not $d.Self
        if ($d.Ip -ne $self) { Set-LocalDevice $d $locals }
        $d
    }
    $list = @($list | Sort-Object @{ Expression = { if ($_.Self) { 0 } elseif ($_.Gateway) { 1 } else { 2 } } }, @{ Expression = { [version]$_.Ip } })
    $today = (Get-Date).ToString('yyyy-MM-dd')
    foreach ($dv in $list) { if ($dv.Mac -and -not $knownMap.ContainsKey([string]$dv.Mac)) { $knownMap[[string]$dv.Mac] = $today } }
    $script:KnownDevices = $knownMap
    $script:NetList = $list
    try { ConvertTo-Json -InputObject $knownMap | Set-Content -LiteralPath $KnownFile -Encoding UTF8 } catch {}
    $newCount = @($list | Where-Object { $_.New }).Count

    # Résultat
    $radar.Rot.BeginAnimation([System.Windows.Media.RotateTransform]::AngleProperty, $null)
    $radar.Sweep.Visibility = 'Collapsed'
    $ui.NetHero.Children.Remove($bar)
    Start-Anim { param($e, $s) $s.T.Text = '{0:N0}' -f ($s.V * $e) } @{ T = $radar.Num; V = [double]$list.Count } 900
    $phase.Text = "appareil$(if ($list.Count -gt 1) {'s'}) connecté$(if ($list.Count -gt 1) {'s'})"
    $phase.Foreground = Get-Brush '#FFFFFF'
    $script:NetScanAt = Get-Date
    # Clic sur le radar : la carte du réseau en grand
    $radar.El.Cursor = [System.Windows.Input.Cursors]::Hand
    $radar.El.ToolTip = 'Voir la carte de ton réseau'
    $radar.El.Background = [System.Windows.Media.Brushes]::Transparent
    $radar.El.Add_MouseLeftButtonUp({ Invoke-Safe { Show-NetMap } })
    $mb = New-Button 'Voir la carte'
    $mb.HorizontalAlignment = 'Center'; $mb.Margin = New-Thickness 0 12 0 0
    $mb.Add_Click({ Invoke-Safe { Show-NetMap } })
    [void]$ui.NetHero.Children.Add($mb)
    if ($newCount) {
        $nw = New-Text "dont $newCount nouveau$(if ($newCount -gt 1) {'x'}) depuis le dernier scan" 12.5 $Colors.warn -Semi
        $nw.HorizontalAlignment = 'Center'
        [void]$ui.NetHero.Children.Add($nw)
    }
    $script:NetFirstScan = $first
    Show-NetDevices
    Invoke-NetDeepScan $net $ips $radar
}

# Filtres de la liste : tous, discrets, nouveaux, caméras
$NetFilters = @(
    @{ Id = 'all'; Label = 'Tous'; Test = { $true } },
    @{ Id = 'hidden'; Label = 'Discrets'; Test = { $_.Hidden } },
    @{ Id = 'new'; Label = 'Nouveaux'; Test = { $_.New } },
    @{ Id = 'cam'; Label = 'Caméras possibles'; Test = { $_.Camera } }
)
function Set-NetFilter([string]$Id) {
    $script:NetFilter = $Id
    Show-NetDevices
    try { $ui.NetDevFilters.BringIntoView() } catch {}
}

function Show-NetDevices {
    $list = @($script:NetList)
    $first = $script:NetFirstScan
    $newCount = @($list | Where-Object { $_.New }).Count
    $hidden = @($list | Where-Object { $_.Hidden }).Count
    $cur = if ($script:NetFilter) { $script:NetFilter } else { 'all' }
    $ui.NetDevFilters.Children.Clear()
    foreach ($f in $NetFilters) {
        $cnt = @($list | Where-Object $f.Test).Count
        if ($f.Id -ne 'all' -and -not $cnt) { if ($cur -eq $f.Id) { $cur = 'all' }; continue }
        $on = $f.Id -eq $cur
        $b = New-Object System.Windows.Controls.Border
        $b.CornerRadius = [System.Windows.CornerRadius]::new(14)
        $b.Padding = New-Thickness 14 6 14 6
        $b.Margin = New-Thickness 0 0 8 0
        $b.Cursor = [System.Windows.Input.Cursors]::Hand
        $b.Background = Get-Brush $(if ($on) { '#22D37A' } else { '#1A1F29' })
        $t = New-Text "$($f.Label) ($cnt)" 12.5 $(if ($on) { '#0B0D10' } else { '#C9CED8' }) -Semi
        $t.TextWrapping = 'NoWrap'
        $b.Child = $t
        $b.Tag = $f.Id
        $b.Add_MouseLeftButtonUp({ param($s, $e) Invoke-Safe { Set-NetFilter ([string]$s.Tag) } })
        [void]$ui.NetDevFilters.Children.Add($b)
    }
    $script:NetFilter = $cur
    $test = @($NetFilters | Where-Object { $_.Id -eq $cur })[0].Test
    $ui.NetDevices.Children.Clear()
    $i = 0
    foreach ($d in @($list | Where-Object $test)) { [void]$ui.NetDevices.Children.Add((New-DeviceTile $d $i)); $i++ }
    $ui.NetDevSummary.Text = "$($list.Count) appareil$(if ($list.Count -gt 1) {'s'})" + $(if ($newCount) { ", $newCount nouveau$(if ($newCount -gt 1) {'x'})" } else { '' }) + $(if ($hidden) { ", $hidden discret$(if ($hidden -gt 1) {'s'})" } else { '' })
    $ui.NetDevHint.Text = if ($cur -eq 'hidden') {
        'Les appareils discrets ne répondent pas au ping : OptiGame les a trouvés autrement (la table de ta box, leurs annonces sur le réseau). C''est normal pour beaucoup de téléphones, de PC protégés par un pare-feu et d''objets connectés en veille. Clique dessus pour voir ce qui a été trouvé.'
    } elseif ($first) {
        'Premier scan : ces appareils sont mémorisés, OptiGame te signalera tout nouvel appareil au prochain scan. Clique sur un appareil pour voir ses détails.'
    } elseif ($newCount) {
        'Un appareil « Nouveau » n''était pas là au scan précédent. Si tu ne le reconnais pas, change le mot de passe de ton Wi-Fi depuis la page de ta box. Attention : les téléphones récents changent parfois d''adresse et peuvent apparaître comme nouveaux.'
    } else {
        'Aucun nouvel appareil depuis le dernier scan. Clique sur un appareil pour voir ses détails, son ping en direct et ses services ouverts.'
    }
    Set-Status "Scan terminé : $($list.Count) appareils trouvés."
    if ($ui.NetMapOverlay.Visibility -eq 'Visible') { Show-NetMap }
}

# ---------------------------------------------------------------------------
# Surveillance : un scan discret toutes les 10 minutes, notification si un nouvel appareil arrive
# ---------------------------------------------------------------------------
function Set-NetWatch([bool]$On) {
    Set-Setting 'NetWatch' $On
    if (-not $script:NetWatchTimer) {
        $script:NetWatchTimer = New-Object System.Windows.Threading.DispatcherTimer
        $script:NetWatchTimer.Interval = [TimeSpan]::FromMinutes(10)
        $script:NetWatchTimer.Add_Tick({ try { Invoke-NetWatch } catch { Write-Log "Surveillance réseau: $_" } })
    }
    if ($On) { $script:NetWatchTimer.Start() } else { $script:NetWatchTimer.Stop() }
}

$script:NetWatchSeen = @{}
function Invoke-NetWatch {
    if ($script:NetScanning -or $script:TestRunning) { return }
    $net = Get-ActiveNet
    if (-not $net -or -not $net.Ip) { return }
    $ips = @(Get-SubnetIps $net.Ip ([int]$net.Prefix))
    $script:NetScanning = $true
    try {
        [OGNative]::Cancel = $false; [OGNative]::Found = 0; [OGNative]::Progress = 0
        $r = Invoke-Async $NetScanWork @{ Ips = $ips; If = $net.IfIndex; Oui = $OuiFile; LoadOui = (-not $script:Oui) } | Select-Object -First 1
    } finally { $script:NetScanning = $false }
    if (-not $r -or $r.Error) { return }
    if ($r.OuiMap) { $script:Oui = $r.OuiMap }
    $known = @{}
    if (Test-Path -LiteralPath $KnownFile) {
        try {
            $j = ConvertFrom-Json (Get-Content -LiteralPath $KnownFile -Raw -Encoding UTF8)
            if ($j -is [string]) { $known[$j] = '' } elseif ($j -is [array]) { foreach ($x in $j) { $known[[string]$x] = '' } } elseif ($j) { foreach ($pp in $j.PSObject.Properties) { $known[$pp.Name] = [string]$pp.Value } }
        } catch {}
    }
    if (-not $known.Count) { return }   # jamais scanné : rien à comparer
    $inNet = @{}; foreach ($x in $ips) { $inNet[$x] = $true }
    $today = (Get-Date).ToString('yyyy-MM-dd')
    $new = @()
    foreach ($l in @($r.Arp)) {
        $p = ([string]$l) -split '\|'
        if (-not $inNet.ContainsKey($p[0]) -or -not $p[1] -or $p[0] -eq $net.Ip) { continue }
        if ($p[2] -notin 'Reachable', 'Stale', 'Delay', 'Probe') { continue }
        # Déjà signalé pendant cette session : pas de nouvelle notification. L'appareil n'est PAS ajouté aux
        # appareils connus : le prochain scan et l'audit le montreront toujours comme « Nouveau ».
        if ($known.ContainsKey($p[1]) -or $script:NetWatchSeen.ContainsKey($p[1])) { continue }
        $script:NetWatchSeen[$p[1]] = $today
        $v = Get-Vendor $p[1]
        $new += "$(if ($v -and $v -ne 'Adresse privée') { $v } else { 'Appareil inconnu' }) ($($p[0]))"
    }
    if (-not $new.Count) { return }
    Write-Log "Nouvel appareil sur le réseau: $($new -join ', ')"
    Show-Notify $(if ($new.Count -gt 1) { "$($new.Count) nouveaux appareils sur ton réseau" } else { 'Nouvel appareil sur ton réseau' }) "$($new -join ', '). Si tu ne le reconnais pas, ouvre la section Réseau d'OptiGame."
}

# ---------------------------------------------------------------------------
# Fiche détaillée d'un appareil du réseau
# ---------------------------------------------------------------------------
$DevTags = @{
    'Ce PC' = 'PC'; 'Box Internet' = 'BOX'; 'Routeur ou répéteur Wi-Fi' = 'WIFI'; 'Téléphone ou tablette' = 'TEL'
    'Console de jeu' = 'JEU'; 'TV ou multimédia' = 'TV'; 'Imprimante' = 'IMP'; 'Box ou décodeur TV' = 'BOX'
    'Objet connecté' = 'IOT'; 'Appareil Apple' = 'APP'; 'Ordinateur' = 'PC'; 'Téléphone probable' = 'TEL'; 'Appareil' = 'NET'; 'Caméra' = 'CAM'; 'Enceinte ou audio' = 'AUD'
}

# Port : nom, niveau (info, warn, bad), explication, adresse web éventuelle
$PortInfo = @{
    21    = @('Transfert de fichiers (FTP)', 'warn', 'Les fichiers et les mots de passe passent en clair sur le réseau.', '')
    22    = @('Accès à distance sécurisé (SSH)', 'info', 'Permet de prendre la main sur l''appareil à distance, de façon chiffrée.', '')
    23    = @('Accès à distance non protégé (Telnet)', 'bad', 'Accès à distance sans aucun chiffrement : à désactiver dans les réglages de l''appareil.', '')
    25    = @('Envoi de mails (SMTP)', 'info', 'Serveur d''envoi de mails.', '')
    53    = @('Serveur DNS', 'info', 'Traduit les noms de sites en adresses. Normal pour une box ou un routeur.', '')
    80    = @('Page web', 'info', 'Page de réglages accessible depuis un navigateur.', 'http://{0}')
    110   = @('Réception de mails (POP3)', 'info', 'Serveur de mails.', '')
    135   = @('Services Windows', 'info', 'Communication interne de Windows. Normal sur un PC Windows.', '')
    139   = @('Partage de fichiers (ancien)', 'warn', 'Ancienne version du partage de fichiers Windows. Normal sur un PC, mais à éviter ailleurs.', '')
    143   = @('Réception de mails (IMAP)', 'info', 'Serveur de mails.', '')
    443   = @('Page web sécurisée', 'info', 'Page de réglages chiffrée, accessible depuis un navigateur.', 'https://{0}')
    445   = @('Partage de fichiers Windows', 'info', 'Dossiers ou imprimantes partagés. Vérifie que tu partages seulement ce que tu veux.', '')
    515   = @('Impression', 'info', 'Service d''impression réseau.', '')
    548   = @('Partage de fichiers Apple', 'info', 'Partage de fichiers d''un Mac ou d''un NAS.', '')
    554   = @('Flux vidéo', 'info', 'Souvent une caméra de surveillance ou un décodeur TV.', '')
    631   = @('Impression', 'info', 'Service d''impression réseau.', '')
    1883  = @('Objets connectés (MQTT)', 'info', 'Messagerie utilisée par la domotique.', '')
    3389  = @('Bureau à distance Windows', 'warn', 'Permet de prendre le contrôle du PC à distance. Désactive le si tu ne t''en sers pas.', '')
    5000  = @('Interface web (NAS, AirPlay)', 'info', 'Page de réglages ou service de diffusion.', 'http://{0}:5000')
    5001  = @('Interface web sécurisée (NAS)', 'info', 'Page de réglages chiffrée.', 'https://{0}:5001')
    5357  = @('Découverte réseau Windows', 'info', 'Permet aux autres appareils de voir ce PC sur le réseau.', '')
    5900  = @('Contrôle à distance (VNC)', 'warn', 'Prise de contrôle de l''écran à distance, souvent mal protégée.', '')
    7000  = @('AirPlay', 'info', 'Diffusion depuis un iPhone, un iPad ou un Mac.', '')
    8008  = @('Google Cast', 'info', 'Diffusion depuis un téléphone (Chromecast).', '')
    8009  = @('Google Cast', 'info', 'Diffusion depuis un téléphone (Chromecast).', '')
    8080  = @('Page web (autre port)', 'info', 'Page de réglages accessible depuis un navigateur.', 'http://{0}:8080')
    8123  = @('Home Assistant', 'info', 'Interface de domotique.', 'http://{0}:8123')
    8443  = @('Page web sécurisée (autre port)', 'info', 'Page de réglages chiffrée.', 'https://{0}:8443')
    9100  = @('Imprimante réseau', 'info', 'Impression directe sur l''imprimante.', '')
    9295  = @('PlayStation Remote Play', 'info', 'Jouer à distance sur la console.', '')
    32400 = @('Serveur Plex', 'info', 'Serveur de films et séries.', 'http://{0}:32400/web')
    62078 = @('Synchronisation iPhone ou iPad', 'info', 'Synchronisation avec un ordinateur. Normal sur un appareil Apple.', '')
}

function New-InfoRows([array]$Rows) {
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Margin = New-Thickness 0 6 0 0
    foreach ($r in $Rows) {
        $g = New-Grid @('200', '*')
        $g.Margin = New-Thickness 0 3 0 3
        Add-ToGrid $g (New-Text $r[0] 13 '#9AA3B2') 0
        $v = New-Text ([string]$r[1]) 13 $(if ($r.Count -gt 2) { $r[2] } else { '#FFFFFF' }) -Semi
        $v.TextWrapping = 'Wrap'
        Add-ToGrid $g $v 1
        [void]$sp.Children.Add($g)
    }
    $sp
}

# Un ping toutes les 400 ms, sans bloquer la fenêtre.
function Update-DevPing {
    $m = $script:DevPing
    if (-not $m) { return }
    if ($m.Task) {
        if (-not $m.Task.IsCompleted) { return }
        $m.Sent++
        $rtt = $null
        try { if (-not $m.Task.IsFaulted -and [string]$m.Task.Result.Status -eq 'Success') { $rtt = [double]$m.Task.Result.RoundtripTime } } catch {}
        if ($null -ne $rtt) {
            [void]$m.Times.Add($rtt)
            if ($m.Times.Count -gt 60) { $m.Times.RemoveAt(0) }
            $m.Gauge.State.Max = [math]::Max(100.0, ($m.Times | Measure-Object -Maximum).Maximum * 1.2)
            Set-GaugeLive $m.Gauge $rtt
            Add-ChartPoint $m.Chart $rtt
        } else {
            $m.Lost++
            Add-ChartPoint $m.Chart 0
        }
        $loss = 100 * $m.Lost / [math]::Max(1.0, $m.Sent)
        if ($m.Times.Count) {
            $avg = ($m.Times | Measure-Object -Average).Average
            $jit = 0
            for ($i = 1; $i -lt $m.Times.Count; $i++) { $jit += [math]::Abs($m.Times[$i] - $m.Times[$i - 1]) }
            if ($m.Times.Count -gt 1) { $jit = $jit / ($m.Times.Count - 1) }
            $m.S.Avg.Text = if ($avg -lt 1) { '< 1 ms' } else { '{0:N0} ms' -f $avg }
            $m.S.Min.Text = '{0:N0} ms' -f ($m.Times | Measure-Object -Minimum).Minimum
            $m.S.Max.Text = '{0:N0} ms' -f ($m.Times | Measure-Object -Maximum).Maximum
            $m.S.Jit.Text = '{0:N1} ms' -f $jit
        }
        $m.S.Loss.Text = '{0:N0} %' -f $loss
        $m.S.Loss.Foreground = Get-Brush $(if ($loss -gt 0) { $Colors.warn } else { $Colors.ok })
        if ($m.Sent -ge 3) {
            if (-not $m.Times.Count) {
                $m.S.Verdict.Text = 'Cet appareil ne répond pas au ping. C''est normal pour certains téléphones, consoles ou PC qui le bloquent.'
                $m.S.Verdict.Foreground = Get-Brush '#9AA3B2'
            } elseif ($loss -ge 5 -or $jit -gt 15) {
                $m.S.Verdict.Text = 'Connexion instable : des réponses se perdent ou arrivent en retard. Souvent le signe d''un Wi-Fi faible.'
                $m.S.Verdict.Foreground = Get-Brush $Colors.warn
            } else {
                $m.S.Verdict.Text = 'Connexion stable.'
                $m.S.Verdict.Foreground = Get-Brush $Colors.ok
            }
        }
        $m.Task = $null
    }
    try { $m.Task = $m.Ping.SendPingAsync($m.Ip, 1000) } catch { $m.Task = $null }
}

function Show-DeviceDetail($D) {
    if ($script:TestRunning) { return }
    $token = [guid]::NewGuid()
    $script:DevToken = $token
    if ($script:MonitorTimer) { $script:MonitorTimer.Stop(); $script:MonitorTimer = $null }
    $tag = $DevTags[$D.KindInfo.Kind]; if (-not $tag) { $tag = 'NET' }
    Show-TestPanel @{ Tag = $tag; Title = $D.Title; Sub = $D.KindInfo.Kind }
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    Set-TestState 'live' 'Ping en direct'
    $ui.TestProgress.Value = 0
    $body = $ui.TestBody

    # Identité
    [void]$body.Children.Add((New-SectionTitle 'IDENTITÉ'))
    $first = [string]$script:KnownDevices[[string]$D.Mac]
    $firstTxt = if ($D.Self) { 'Ce PC' } elseif ($first) { ([datetime]$first).ToString('dd/MM/yyyy') } elseif ($D.Mac) { 'Avant la mise à jour 1.0.8' } else { 'Inconnue' }
    $rows = @(
        @('Adresse sur le réseau', $D.Ip),
        @('Adresse physique (MAC)', $(if ($D.Mac) { $D.Mac } else { 'Inconnue' })),
        @('Fabricant', $(if ($D.Vendor) { $D.Vendor } else { 'Inconnu' })),
        @('Type', $D.KindInfo.Kind)
    )
    if ($D.Host) { $rows += , @('Nom sur le réseau', $D.Host) }
    if ($D.Adapter) { $rows += , @('Carte réseau', $D.Adapter) }
    if ($D.Announced -and $D.Announced -ne $D.Host) { $rows += , @('Nom annoncé par l''appareil', $D.Announced) }
    if ($D.Model) { $rows += , @('Modèle', "$(if ($D.Maker) { $D.Maker + ' ' })$($D.Model)") }
    if ($D.NbName) { $rows += , @('Nom Windows', "$($D.NbName)$(if ($D.NbGroup) { " (groupe $($D.NbGroup))" })") }
    if ($D.OsGuess) { $rows += , @('Système probable', $D.OsGuess) }
    if (@($D.ServiceLabels).Count) { $rows += , @('Ce qu''il propose', (@($D.ServiceLabels) -join ', ')) }
    if ($D.WebTitle) { $rows += , @('Sa page de réglages', $D.WebTitle) }
    if (@($D.Ipv6).Count) { $rows += , @('Adresse IPv6', ((@($D.Ipv6) | Select-Object -First 2) -join ', ')) }
    if (@($D.FoundBy).Count) { $rows += , @('Trouvé grâce à', (@($D.FoundBy) -join ', '), '#9AA3B2') }
    if ($D.Hidden) { $rows += , @('Appareil discret', 'Il ne répond pas au ping. C''est normal pour beaucoup de téléphones et de PC protégés par un pare-feu.', '#9AA3B2') }
    if ($D.Camera) { $rows += , @('Caméra possible', "Indices : $(@($D.CameraWhy) -join ', '). Vérifie que tu sais à qui elle est et où elle filme.", $Colors.warn) }
    $rows += , @('Vu pour la première fois', $firstTxt)
    if ($D.New) { $rows += , @('Statut', 'Nouvel appareil depuis le dernier scan', $Colors.warn) }
    if ($D.Vendor -eq 'Adresse privée') { $rows += , @('Bon à savoir', 'Les téléphones récents cachent leur vraie adresse physique : le fabricant ne peut pas être connu.', '#9AA3B2') }
    [void]$body.Children.Add((New-InfoRows $rows))

    # Ping en direct
    [void]$body.Children.Add((New-SectionTitle 'TEMPS DE RÉPONSE EN DIRECT'))
    $row = New-Grid @('Auto', '*')
    $gauge = New-Gauge 'Temps de réponse' 0 100 '{0:N0}' 'ms' $Colors.info 0
    Add-ToGrid $row $gauge.El 0
    $stats = @{}
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.VerticalAlignment = 'Center'
    $sp.Margin = New-Thickness 18 0 0 0
    foreach ($s in @(@('Avg', 'Moyenne'), @('Min', 'Plus rapide'), @('Max', 'Plus lent'), @('Jit', 'Variation (gigue)'), @('Loss', 'Réponses perdues'))) {
        $g = New-Grid @('170', '*')
        $g.Margin = New-Thickness 0 3 0 3
        Add-ToGrid $g (New-Text $s[1] 13 '#9AA3B2') 0
        $v = New-Text '...' 13 '#FFFFFF' -Semi
        Add-ToGrid $g $v 1
        [void]$sp.Children.Add($g)
        $stats[$s[0]] = $v
    }
    $verdict = New-Text 'Mesure en cours...' 12.5 '#9AA3B2' -Semi
    $verdict.Margin = New-Thickness 0 10 0 0
    [void]$sp.Children.Add($verdict)
    $stats.Verdict = $verdict
    Add-ToGrid $row $sp 1
    [void]$body.Children.Add($row)
    $chart = New-LiveChart $Colors.info 'ms' '{0:N0}'
    [void]$body.Children.Add($chart.El)

    $script:DevPing = @{ Ip = $D.Ip; Ping = (New-Object System.Net.NetworkInformation.Ping); Task = $null; Times = (New-Object System.Collections.ArrayList); Lost = 0; Sent = 0; Gauge = $gauge; Chart = $chart; S = $stats }
    $script:MonitorTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:MonitorTimer.Interval = [TimeSpan]::FromMilliseconds(400)
    $script:MonitorTimer.Add_Tick({ try { Update-DevPing } catch {} })
    $script:MonitorTimer.Start()

    # Services ouverts (en arrière plan)
    [void]$body.Children.Add((New-SectionTitle 'SERVICES OUVERTS'))
    $wait = New-Text 'Recherche des services proposés par cet appareil...' 13 '#9AA3B2'
    [void]$body.Children.Add($wait)
    $ports = [int[]]@($PortInfo.Keys)
    $open = @(Invoke-Async { param($a) [OGNative]::ScanPorts($a.Ip, [int[]]$a.Ports, 600) } @{ Ip = $D.Ip; Ports = $ports })
    if ($script:DevToken -ne $token -or $ui.TestOverlay.Visibility -ne 'Visible') { return }
    $body.Children.Remove($wait)
    $worst = 'ok'
    $notes = @()
    if (-not $open.Count) {
        [void]$body.Children.Add((New-Text 'Aucun service ouvert parmi les plus courants. C''est normal pour un téléphone, une console ou une TV : ils n''acceptent pas de connexions.' 13 '#9AA3B2'))
    }
    $i = 0
    foreach ($port in $open) {
        $pi = $PortInfo[[int]$port]
        if (-not $pi) { continue }
        $col = switch ($pi[1]) { 'bad' { $Colors.bad } 'warn' { $Colors.warn } default { $Colors.info } }
        if ($pi[1] -eq 'bad') { $worst = 'bad'; $notes += $pi[0] } elseif ($pi[1] -eq 'warn' -and $worst -ne 'bad') { $worst = 'warn'; $notes += $pi[0] }
        $card = New-Object System.Windows.Controls.Border
        $card.Background = Get-Brush '#1A1F29'
        $card.CornerRadius = [System.Windows.CornerRadius]::new(10)
        $card.Padding = New-Thickness 12 9 12 9
        $card.Margin = New-Thickness 0 0 0 6
        $g = New-Grid @('Auto', '*', 'Auto')
        $badge = New-Object System.Windows.Controls.Border
        $bg = Get-Brush $col; $bg.Opacity = 0.16
        $badge.Background = $bg
        $badge.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $badge.MinWidth = 62
        $badge.Padding = New-Thickness 8 4 8 4
        $badge.VerticalAlignment = 'Center'
        $bt = New-Text "$port" 13 $col -Bold
        $bt.HorizontalAlignment = 'Center'
        $badge.Child = $bt
        Add-ToGrid $g $badge 0
        $txt = New-Object System.Windows.Controls.StackPanel
        $txt.Margin = New-Thickness 12 0 8 0
        [void]$txt.Children.Add((New-Text $pi[0] 13.5 '#FFFFFF' -Semi))
        [void]$txt.Children.Add((New-Text $pi[2] 12 '#9AA3B2'))
        Add-ToGrid $g $txt 1
        if ($pi[3]) {
            $b = New-Button 'Ouvrir'
            $b.Tag = $pi[3] -f $D.Ip
            $b.Add_Click({ param($s, $e) Open-Url $s.Tag })
            Add-ToGrid $g $b 2
        }
        $card.Child = $g
        $card.Opacity = 0
        Start-WpfAnim $card ([System.Windows.UIElement]::OpacityProperty) 1 350 (80 * $i)
        [void]$body.Children.Add($card)
        $i++
    }
    $txt = switch ($worst) {
        'bad'  { "À corriger : $($notes -join ', '). Désactive ce service dans les réglages de l'appareil, ou demande à la personne qui l'a installé." }
        'warn' { "À surveiller : $($notes -join ', '). Si tu ne sais pas pourquoi c'est ouvert, désactive le dans les réglages de l'appareil." }
        default { if ($open.Count) { 'Rien d''inhabituel pour ce type d''appareil.' } else { 'Rien à signaler.' } }
    }
    [void]$body.Children.Add((New-Verdict $worst $txt))
    $note = New-Text "Seuls les $($ports.Count) services les plus courants sont vérifiés." 11.5 '#5B6475'
    $note.Margin = New-Thickness 0 8 0 0
    [void]$body.Children.Add($note)
}
