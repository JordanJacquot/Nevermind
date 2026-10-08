# Nevermind : recherche approfondie des appareils du réseau (appareils discrets, noms, modèles, caméras).
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# Services annoncés par les appareils, en clair.
$ServiceLabels = [ordered]@{
    '_googlecast' = 'Chromecast / Google Cast'; '_airplay' = 'AirPlay'; '_raop' = 'Enceinte AirPlay'; '_companion-link' = 'Appareil Apple'
    '_ipp' = 'Impression'; '_ipps' = 'Impression'; '_printer' = 'Impression'; '_pdl-datastream' = 'Impression'; '_scanner' = 'Scanner'; '_uscan' = 'Scanner'
    '_hap' = 'Maison connectée (HomeKit)'; '_homekit' = 'Maison connectée (HomeKit)'; '_matter' = 'Maison connectée (Matter)'; '_matterc' = 'Maison connectée (Matter)'
    '_hue' = 'Philips Hue'; '_spotify-connect' = 'Spotify Connect'; '_sonos' = 'Sonos'; '_amzn-wplay' = 'Amazon Fire TV'
    '_androidtvremote2' = 'Android TV'; '_mediaremotetv' = 'Apple TV'; '_smb' = 'Partage de fichiers'; '_workstation' = 'Ordinateur'
    '_nvstream' = 'NVIDIA GameStream'; '_rtsp' = 'Flux vidéo'; '_http' = 'Page web'; '_alexa' = 'Amazon Alexa'
}
# Informations annoncées qu'on garde (jamais les numéros de série ni les clés).
$MdnsKeys = '^(model|md|manufacturer|fn|ty|am|integrator|rpMd|usb_MFG|usb_MDL|product|vendor|name)$'
$CameraVendors = '(?i)hikvision|dahua|ezviz|reolink|wyze|arlo|amcrest|axis comm|foscam|uniview|zhejiang uniview|imou|annke|lorex|swann|vivotek|hanwha|eufy|blink|yi technology|shenzhen reecam|sricam|vstarcam|hisilicon|ubiquiti.*protect'
$CameraWords = '(?i)camera|caméra|\bipc\b|\bnvr\b|\bdvr\b|webcam|ip ?cam|network ?video|onvif|video ?encoder'

$NetDeepWork = {
    param($a)
    try {
        [OGNative]::Phase = 'deep-arp'; [OGNative]::Progress = 5
        $arpTask = [NetProbe]::ArpSweepAsync([string[]]@($a.Missing), 128)
        [OGNative]::Phase = 'deep-listen'; [OGNative]::Progress = 15
        $mdns = @([NetProbe]::Mdns($a.Ip, 3000))
        [OGNative]::Progress = 30
        $wsd = @([NetProbe]::WsDiscovery($a.Ip, 2000))
        [OGNative]::Progress = 40
        # UPnP : annonces SSDP, puis la fiche de chaque appareil (nom, marque, modèle)
        $locs = @{}
        try {
            $u = New-Object Net.Sockets.UdpClient((New-Object Net.IPEndPoint([Net.IPAddress]::Parse($a.Ip), 0)))
            $u.Client.ReceiveTimeout = 300
            $msg = [Text.Encoding]::ASCII.GetBytes("M-SEARCH * HTTP/1.1`r`nHOST: 239.255.255.250:1900`r`nMAN: `"ssdp:discover`"`r`nMX: 2`r`nST: upnp:rootdevice`r`n`r`n")
            [void]$u.Send($msg, $msg.Length, '239.255.255.250', 1900)
            $ep = New-Object Net.IPEndPoint([Net.IPAddress]::Any, 0)
            $end = (Get-Date).AddSeconds(2.5)
            while ((Get-Date) -lt $end) {
                try { $r = $u.Receive([ref]$ep) } catch { continue }
                foreach ($l in ([Text.Encoding]::ASCII.GetString($r) -split "`r`n")) {
                    if ($l -match '^(?i)LOCATION:\s*(.+)$') { $ip = [string]$ep.Address; if (-not $locs.ContainsKey($ip)) { $locs[$ip] = @() }; if ($locs[$ip] -notcontains $Matches[1].Trim()) { $locs[$ip] += $Matches[1].Trim() } }
                }
            }
            $u.Close()
        } catch {}
        $upnp = @()
        foreach ($ip in $locs.Keys) {
            foreach ($loc in @($locs[$ip] | Select-Object -First 3)) {
                try {
                    $x = [xml](Invoke-WebRequest -Uri $loc -UseBasicParsing -TimeoutSec 3).Content
                    $dv = $x.root.device
                    if ($dv) {
                        $upnp += "$ip|$([string]$dv.friendlyName)|$([string]$dv.manufacturer)|$([string]$dv.modelName)|$([string]$dv.modelNumber)|$([string]$dv.deviceType)" -replace "`r|`n", ' '
                        if ([string]$dv.deviceType -notmatch 'WFADevice') { break }
                    }
                } catch {}
            }
        }
        [OGNative]::Progress = 50
        # IPv6 : un « ping tout le monde » remplit la liste des voisins IPv6 de Windows
        $v6 = @()
        try {
            $pg = New-Object Net.NetworkInformation.Ping
            [void]$pg.Send([Net.IPAddress]::Parse("ff02::1%$($a.If)"), 1000)
            Start-Sleep -Milliseconds 600
            $v6 = @(Get-NetNeighbor -InterfaceIndex $a.If -AddressFamily IPv6 -ErrorAction SilentlyContinue |
                Where-Object { $_.State -in 'Reachable', 'Stale', 'Delay', 'Probe' -and $_.LinkLayerAddress -and $_.LinkLayerAddress -notmatch '^(00-00-00-00-00-00|33-33-.*|FF-FF-FF-FF-FF-FF)$' -and [string]$_.IPAddress -notmatch '^ff' } |
                ForEach-Object { "$($_.IPAddress)|$($_.LinkLayerAddress)" })
        } catch {}
        [OGNative]::Phase = 'deep-names'; [OGNative]::Progress = 55
        $arp = @($arpTask.Result)
        $all = @(@($a.Found) + @($arp | ForEach-Object { ($_ -split '\|')[0] }) | Where-Object { $_ } | Select-Object -Unique)
        $nb = @([NetProbe]::NetBios([string[]]$all, 1500))
        [OGNative]::Phase = 'deep-web'
        $ports = @([OGNative]::ScanHosts([string[]]$all, [int[]]@(80, 443, 8080, 8443, 554, 8000, 37777, 9100, 631, 5000), 600, 60.0, 85.0))
        $urls = @(foreach ($l in $ports) {
            $p = $l -split '\|'
            switch ([int]$p[1]) { { $_ -in 80, 8080, 5000 } { "http://$($p[0]):$($p[1])" } { $_ -in 443, 8443 } { "https://$($p[0]):$($p[1])" } }
        })
        $titles = @(if ($urls.Count) { [NetProbe]::HttpTitles([string[]]$urls, 3000) })
        [OGNative]::Progress = 100
        @{ Arp = $arp; Mdns = $mdns; Wsd = $wsd; Upnp = $upnp; V6 = $v6; NetBios = $nb; Ports = $ports; Titles = $titles }
    } catch { @{ Error = $_.Exception.GetBaseException().Message } }
}

function Get-OsGuess($D) {
    $t = [int]$D.Ttl
    $svc = [string]$D.SvcText
    if ($D.NbName -or $svc -match '(?i)pub:Computer|_workstation') { return 'Windows' }
    if ($svc -match '(?i)_companion-link|_mediaremotetv' -or $D.Model -match '(?i)^(iPhone|iPad|Mac|AppleTV)') { return 'Apple (iPhone, iPad, Mac ou Apple TV)' }
    if ($t -gt 128) { return 'Système d''équipement réseau (box, routeur, imprimante...)' }
    if ($t -gt 64) { return 'Windows' }
    if ($t -gt 0) { return 'Linux, Android, iPhone ou Mac' }
    ''
}

# Recalcule le nom, le type, le modèle et les indices « caméra » d'un appareil à partir de tout ce qu'on sait.
function Update-DeviceIdentity($D) {
    $svcLabels = @(@($D.Services) | ForEach-Object { $k = ($_ -split '\.')[0]; if ($ServiceLabels.Contains($k)) { $ServiceLabels[$k] } } | Select-Object -Unique)
    $D.ServiceLabels = $svcLabels
    $D.SvcText = "$(@($D.Services) -join ' ') $($D.UpnpType) $($D.WsdTypes) $($svcLabels -join ' ')"
    $txt = $D.Txt
    $model = @($txt.md, $txt.model, $txt.am, $txt.rpMd, $txt.ty, $txt.usb_MDL, $txt.product, $D.UpnpModel, $D.WsdHardware) | Where-Object { $_ } | Select-Object -First 1
    $maker = @($txt.manufacturer, $txt.usb_MFG, $txt.integrator, $txt.vendor, $D.UpnpMaker) | Where-Object { $_ } | Select-Object -First 1
    $D.Model = [string]$model
    $D.Maker = [string]$maker
    $D.OsGuess = Get-OsGuess $D
    # Indices de caméra : un indice fort suffit, sinon il en faut deux (un décodeur TV a aussi un flux vidéo).
    $strong = @(); $weak = @()
    if ($D.Vendor -match $CameraVendors -or $D.Maker -match $CameraVendors) { $strong += "fabricant de caméras ($(if ($D.Maker -match $CameraVendors) { $D.Maker } else { $D.Vendor }))" }
    if ($D.WsdTypes -match '(?i)NetworkVideoTransmitter' -or $D.WsdScopes -match '(?i)onvif') { $strong += 'se déclare comme caméra (norme ONVIF)' }
    if ("$($D.Model) $($D.UpnpName) $($D.UpnpType) $($D.WebTitle) $($D.MdnsName)" -match $CameraWords) { $strong += 'son nom ou sa page web parle de caméra' }
    if (@($D.Ports) -contains 554 -or @($D.Services) -match '^_rtsp') { $weak += 'diffuse un flux vidéo (RTSP)' }
    if (@($D.Ports) -contains 8000 -or @($D.Ports) -contains 37777) { $weak += 'port typique des enregistreurs vidéo' }
    $D.Camera = [bool]($strong.Count -or $weak.Count -ge 2)
    $D.CameraWhy = @($strong + $weak)
    # Nom affiché : le nom annoncé par l'appareil, puis ceux trouvés ailleurs
    # « fn » n'est le vrai nom que sur un Chromecast (ailleurs c'est souvent un nom court comme « webOS »).
    $announced = if ($txt.fn -and @($D.Services) -match '^_googlecast') { $txt.fn } else { @($D.MdnsName, $D.UpnpName, $txt.fn, $D.WsdName, $D.NbName) | Where-Object { $_ } | Select-Object -First 1 }
    $D.Announced = [string]$announced
    if (-not $D.Self -and -not $D.Gateway) {
        $D.Title = if ($announced) { [string]$announced }
                   elseif ($D.Host -and $D.Host -ne 'lan') { $D.Host }
                   elseif ($D.MdnsHost) { $D.MdnsHost }
                   elseif ($D.Model) { "$(if ($D.Maker) { $D.Maker + ' ' })$($D.Model)" }
                   elseif ($D.Vendor -and $D.Vendor -ne 'Adresse privée') { $D.Vendor }
                   else { 'Appareil inconnu' }
    }
    $D.KindInfo = Get-DeviceKind $D
}

# Ajoute aux appareils tout ce que la recherche approfondie a trouvé. Retourne les appareils nouvellement découverts.
function Merge-NetDeep($R, $Net, [string[]]$Subnet) {
    $list = New-Object System.Collections.ArrayList
    foreach ($d in @($script:NetList)) { [void]$list.Add($d) }
    $byIp = @{}; $byMac = @{}
    $inNet = @{}; foreach ($x in $Subnet) { $inNet[$x] = $true }
    foreach ($d in $list) {
        $byIp[[string]$d.Ip] = $d
        if ($d.Mac) { $byMac[([string]$d.Mac).ToUpper()] = $d }
        foreach ($k in 'Services', 'Ports', 'Ipv6', 'FoundBy') { if ($null -eq $d[$k]) { $d[$k] = @() } }
        if ($null -eq $d.Txt) { $d.Txt = @{} }
        $d.FoundBy = @($(if ($null -ne $d.Ms) { 'réponse au ping' } else { 'liste des voisins de Windows' }))
        $d.Hidden = ($null -eq $d.Ms) -and -not $d.Self
    }
    $added = @()
    $newDev = {
        param($ip, $mac, $how)
        $n = @{ Ip = $ip; Ms = $null; Mac = $mac; Ttl = 0; Self = $false; Gateway = $false; Host = ''; Vendor = (Get-Vendor $mac); New = $false; Hidden = $true
                Services = @(); Ports = @(); Ipv6 = @(); FoundBy = @($how); Txt = @{} }
        [void]$list.Add($n)
        $byIp[$ip] = $n
        if ($mac) { $byMac[$mac.ToUpper()] = $n }
        $n
    }
    # Demande directe d'adresse physique : les appareils qui ignorent le ping
    foreach ($l in @($R.Arp)) {
        $p = ([string]$l) -split '\|'
        if (-not $inNet.ContainsKey($p[0])) { continue }
        $d = $byIp[$p[0]]
        if (-not $d) { $added += (& $newDev $p[0] $p[1] 'demande directe d''adresse physique (ARP)') }
        else { if (-not $d.Mac) { $d.Mac = $p[1]; $d.Vendor = Get-Vendor $p[1] }; if ($d.FoundBy -notcontains 'demande directe d''adresse physique (ARP)') { $d.FoundBy += 'demande directe d''adresse physique (ARP)' } }
    }
    # mDNS / Bonjour
    foreach ($l in @($R.Mdns)) {
        $p = ([string]$l) -split '\|', 4
        if ($p.Count -lt 4) { continue }
        switch ($p[1]) {
            'a' { $d = $byIp[$p[3]]; if ($d) { $d.MdnsHost = ($p[2] -replace '\.local$', '') } }
            'ptr' {
                $d = $byIp[$p[0]]
                if (-not $d -or $p[2] -eq '_services._dns-sd._udp.local') { continue }
                $type = ($p[2] -replace '\.local$', '')
                if ($d.Services -notcontains $type) { $d.Services += $type }
                $inst = ($p[3] -replace ([regex]::Escape(".$($p[2])") + '$'), '')
                if ($inst -and $inst -ne $p[3]) {
                    if ($null -eq $d.MdnsNames) { $d.MdnsNames = @{} }
                    $key = ($type -split '\.')[0]
                    if (-not $d.MdnsNames.ContainsKey($key)) { $d.MdnsNames[$key] = ($inst -replace '^[0-9A-F]{12}@', '') }
                }
            }
            'txt' {
                $d = $byIp[$p[0]]
                if (-not $d) { continue }
                # Le nom de l'enregistrement contient aussi le nom de l'appareil et le service : « [LG] webOS TV._airplay._tcp.local »
                if ($p[2] -match '^(.+?)\.(_[^.]+\._(?:tcp|udp))\.local$') {
                    $inst = $Matches[1]; $type = $Matches[2]; $key = ($type -split '\.')[0]
                    if ($d.Services -notcontains $type) { $d.Services += $type }
                    if ($null -eq $d.MdnsNames) { $d.MdnsNames = @{} }
                    if (-not $d.MdnsNames.ContainsKey($key)) { $d.MdnsNames[$key] = ($inst -replace '^[0-9A-F]{12}@', '') }
                }
                $kv = $p[3] -split '=', 2
                if ($kv.Count -eq 2 -and $kv[0] -match $MdnsKeys -and $kv[1] -and -not $d.Txt.ContainsKey($kv[0])) { $d.Txt[$kv[0]] = $kv[1] }
            }
        }
    }
    foreach ($d in $list) {
        if ($d.MdnsNames) {
            foreach ($k in '_googlecast', '_airplay', '_companion-link', '_mediaremotetv', '_hap', '_homekit', '_ipp', '_ipps', '_printer', '_sonos', '_amzn-wplay', '_androidtvremote2', '_hue', '_raop') {
                if ($d.MdnsNames.ContainsKey($k)) { $d.MdnsName = $d.MdnsNames[$k]; break }
            }
        }
    }
    foreach ($l in @($R.Upnp)) {
        $p = ([string]$l) -split '\|'
        $d = $byIp[$p[0]]; if (-not $d) { continue }
        $type = if ($p[5] -match ':device:([^:]+)') { $Matches[1] } else { '' }
        if ($type -eq 'WFADevice' -or $p[1] -match '(?i)^(WPS Access Point|Wi-?Fi Protected Setup)') { continue }
        if ($d.UpnpName) { continue }
        $d.UpnpName = $p[1]; $d.UpnpMaker = $p[2]; $d.UpnpModel = (@($p[3], $p[4]) | Where-Object { $_ }) -join ' '
        $d.UpnpType = $type
    }
    foreach ($l in @($R.NetBios)) {
        $p = ([string]$l) -split '\|'
        $d = $byIp[$p[0]]; if (-not $d) { continue }
        $d.NbName = $p[1]; $d.NbGroup = $p[2]
    }
    foreach ($l in @($R.Wsd)) {
        $p = ([string]$l) -split '\|'
        $d = $byIp[$p[0]]; if (-not $d) { continue }
        $d.WsdTypes = $p[1]; $d.WsdScopes = $p[2]
        foreach ($s in ($p[2] -split '\s+')) {
            if ($s -match '/name/(.+)$') { $d.WsdName = [uri]::UnescapeDataString($Matches[1]) }
            if ($s -match '/hardware/(.+)$') { $d.WsdHardware = [uri]::UnescapeDataString($Matches[1]) }
        }
    }
    # IPv6 : adresses des appareils connus, et appareils visibles seulement en IPv6
    foreach ($l in @($R.V6)) {
        $p = ([string]$l) -split '\|'
        $mac = $p[1].ToUpper()
        $d = $byMac[$mac]
        if (-not $d) { $d = & $newDev $p[0] $p[1] 'voisins IPv6'; $d.Only6 = $true; $added += $d }
        if ($d.Ipv6 -notcontains $p[0]) { $d.Ipv6 += $p[0] }
    }
    foreach ($l in @($R.Ports)) { $p = ([string]$l) -split '\|'; $d = $byIp[$p[0]]; if ($d -and $d.Ports -notcontains [int]$p[1]) { $d.Ports += [int]$p[1] } }
    foreach ($l in @($R.Titles)) {
        $p = ([string]$l) -split '\|'
        $ip = ([uri]$p[0]).Host
        $d = $byIp[$ip]; if (-not $d) { continue }
        if ($p[1] -and -not $d.WebTitle) { $d.WebTitle = $p[1] }
        if ($p[2] -and -not $d.WebServer) { $d.WebServer = $p[2] }
    }
    foreach ($d in $list) {
        if (@($d.Services).Count -or $d.UpnpName -or $d.NbName -or $d.WsdTypes) {
            foreach ($how in @($(if (@($d.Services).Count) { 'annonces Bonjour (mDNS)' }), $(if ($d.UpnpName) { 'annonces UPnP' }), $(if ($d.NbName) { 'nom Windows (NetBIOS)' }), $(if ($d.WsdTypes) { 'WS-Discovery' })) | Where-Object { $_ }) {
                if ($d.FoundBy -notcontains $how) { $d.FoundBy += $how }
            }
        }
        Update-DeviceIdentity $d
    }
    $locals = Get-LocalInterfaces
    $primary = if ($Net) { [string]$Net.Ip } else { '' }
    foreach ($d in $list) { if ([string]$d.Ip -ne $primary) { Set-LocalDevice $d $locals } }
    $script:NetList = @($list | Sort-Object @{ Expression = { if ($_.Self) { 0 } elseif ($_.Gateway) { 1 } else { 2 } } }, @{ Expression = { try { [version]$_.Ip } catch { [version]'255.255.255.255' } } })
    $added
}

# Lancée juste après le scan : complète les appareils affichés.
function Invoke-NetDeepScan($Net, [string[]]$Subnet, $Hero) {
    $script:NetScanning = $true
    $ui.BtnNetScan.IsEnabled = $false
    $bar = New-Object System.Windows.Controls.ProgressBar
    $bar.Width = 220; $bar.Height = 5; $bar.Margin = New-Thickness 0 8 0 0
    $line = New-Text 'Recherche approfondie : appareils discrets, noms et modèles...' 12 '#A6A1BC'
    $line.HorizontalAlignment = 'Center'; $line.Margin = New-Thickness 0 10 0 0
    [void]$ui.NetHero.Children.Add($line)
    [void]$ui.NetHero.Children.Add($bar)
    [OGNative]::Cancel = $false; [OGNative]::Progress = 0
    $script:NetDeepUi = @{ Bar = $bar; Line = $line }
    $tm = New-Object System.Windows.Threading.DispatcherTimer
    $tm.Interval = [TimeSpan]::FromMilliseconds(200)
    $tm.Add_Tick({
        $u = $script:NetDeepUi
        $u.Bar.Value = [OGNative]::Progress
        $u.Line.Text = switch ([OGNative]::Phase) {
            'deep-arp' { 'Recherche des appareils qui ne répondent pas au ping...' } 'deep-listen' { 'Écoute des appareils qui se présentent (TV, enceintes, imprimantes...)...' }
            'deep-names' { 'Recherche des noms Windows...' } 'deep-web' { 'Lecture des pages de réglages...' } default { 'Recherche approfondie...' } }
    })
    $tm.Start()
    try {
        $found = @($script:NetList | ForEach-Object { [string]$_.Ip })
        $missing = @($Subnet | Where-Object { $found -notcontains $_ })
        $r = Invoke-Async $NetDeepWork @{ Ip = $Net.Ip; If = $Net.IfIndex; Missing = $missing; Found = $found } | Select-Object -First 1
    } finally {
        $tm.Stop()
        $script:NetScanning = $false
        $ui.BtnNetScan.IsEnabled = $true
        $ui.NetHero.Children.Remove($bar)
    }
    if (-not $r -or $r.Error) {
        $line.Text = 'Recherche approfondie impossible.'
        if ($r.Error) { Write-Log "Recherche approfondie: $($r.Error)" }
        return
    }
    $added = @(Merge-NetDeep $r $Net $Subnet)
    # Nouveaux appareils trouvés maintenant : mémorisés, et signalés s'ils n'étaient jamais apparus
    $today = (Get-Date).ToString('yyyy-MM-dd')
    $first = -not @($script:KnownDevices.Keys).Count
    foreach ($d in $added) {
        if ($d.Mac -and -not $script:KnownDevices.ContainsKey([string]$d.Mac)) { $d.New = -not $first; $script:KnownDevices[[string]$d.Mac] = $today }
    }
    try { ConvertTo-Json -InputObject $script:KnownDevices | Set-Content -LiteralPath $KnownFile -Encoding UTF8 } catch {}
    $hidden = @($script:NetList | Where-Object { $_.Hidden }).Count
    $cams = @($script:NetList | Where-Object { $_.Camera }).Count
    $parts = @()
    if ($hidden) { $parts += "$hidden discret$(if ($hidden -gt 1) {'s'})" }
    if ($cams) { $parts += "$cams caméra$(if ($cams -gt 1) {'s'}) possible$(if ($cams -gt 1) {'s'})" }
    $line.Text = "Recherche approfondie terminée$(if ($parts) { ' : ' + ($parts -join ', ') })."
    $line.Foreground = Get-Brush $(if ($cams) { $Colors.warn } else { '#A6A1BC' })
    if ($hidden -or $cams) {
        $line.Text += '  Voir lesquels'
        $line.TextDecorations = [System.Windows.TextDecorations]::Underline
        $line.Cursor = [System.Windows.Input.Cursors]::Hand
        $line.Tag = $(if ($cams) { 'cam' } else { 'hidden' })
        $line.Add_MouseLeftButtonUp({ param($s, $e) Invoke-Safe { Set-NetFilter ([string]$s.Tag) } })
    }
    Show-NetDevices
    if ($Hero -and $Hero.Num) { $Hero.Num.Text = "$(@($script:NetList).Count)" }
    Set-Status "Scan terminé : $(@($script:NetList).Count) appareils$(if ($parts) { ', dont ' + ($parts -join ', ') })."
}
