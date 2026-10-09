# Nevermind : audit de sécurité du réseau.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Audit de sécurité du réseau
# ---------------------------------------------------------------------------
$AuditPorts = @(21, 23, 80, 139, 443, 445, 554, 3389, 5900)
$AuditTips = @(
    'Si beaucoup de monde connaît le mot de passe de ton Wi-Fi, change le de temps en temps depuis la page de ta box.',
    'Crée un Wi-Fi invité (souvent proposé par la box) pour tes visiteurs et tes objets connectés.',
    'Change le mot de passe par défaut des caméras, NAS et imprimantes réseau.',
    'Laisse les mises à jour automatiques activées sur ta box, ton PC et tes appareils.'
)
$AuditCats = @{ wifi = 'Wi-Fi'; box = 'Box'; dns = 'DNS'; devices = 'Appareils'; pc = 'Ce PC' }

# Travail lancé dans un fil séparé: seules les fonctions de [OGNative] y sont disponibles.
$NetAuditWork = {
    param($a)
    $out = @{}
    try {
        $openRe = '(?i)^(ouvrir|ouvert|open)'
        $field = {
            param($lines, $re)
            foreach ($l in $lines) { if ($l -match "^\s*($re)\s*:\s*(.+?)\s*$") { return $Matches[2] } }
            $null
        }

        # Wi-Fi utilisé et Wi-Fi mémorisés
        [OGNative]::Phase = 'wifi'; [OGNative]::Progress = 2
        $lines = @(netsh wlan show interfaces 2>$null)
        $ssid = & $field $lines 'SSID'
        if ($ssid) { $out.Wifi = @{ Ssid = [string]$ssid; Auth = [string](& $field $lines 'Authentication|Authentification'); Cipher = [string](& $field $lines 'Cipher|Chiffrement') } }
        $names = @(netsh wlan show profiles 2>$null | ForEach-Object { if ($_ -match '^\s{2,}[^:<]+:\s(.+?)\s*$') { $Matches[1] } }) | Select-Object -First 40
        $out.Profiles = @(foreach ($n in $names) {
            $pl = @(netsh wlan show profile "name=$n" 2>$null)
            $auth = [string](& $field $pl 'Authentication|Authentification')
            $ciph = [string](& $field $pl 'Cipher|Chiffrement')
            $mode = [string](& $field $pl 'Connection mode|Mode de connexion')
            $isOpen = ($auth -match $openRe) -or ($ciph -match 'WEP')
            "$n|$auth|$ciph|$(if ($mode -match '(?i)auto') { 1 } else { 0 })|$(if ($isOpen) { 1 } else { 0 })"
        })
        [OGNative]::Progress = 8

        # Box : annonces UPnP (SSDP), WPS, règles d'ouverture de ports
        [OGNative]::Phase = 'box'
        $ssdp = @()
        try {
            $u = New-Object Net.Sockets.UdpClient((New-Object Net.IPEndPoint([Net.IPAddress]::Parse($a.Ip), 0)))
            $u.Client.ReceiveTimeout = 400
            $msg = [Text.Encoding]::ASCII.GetBytes("M-SEARCH * HTTP/1.1`r`nHOST: 239.255.255.250:1900`r`nMAN: `"ssdp:discover`"`r`nMX: 2`r`nST: ssdp:all`r`n`r`n")
            [void]$u.Send($msg, $msg.Length, '239.255.255.250', 1900)
            $ep = New-Object Net.IPEndPoint([Net.IPAddress]::Any, 0)
            $end = (Get-Date).AddSeconds(3)
            while ((Get-Date) -lt $end) {
                try { $r = $u.Receive([ref]$ep) } catch { continue }
                $st = ''; $loc = ''
                foreach ($l in ([Text.Encoding]::ASCII.GetString($r) -split "`r`n")) {
                    if ($l -match '^(?i)ST:\s*(.+)$') { $st = $Matches[1].Trim() } elseif ($l -match '^(?i)LOCATION:\s*(.+)$') { $loc = $Matches[1].Trim() }
                }
                $ssdp += "$($ep.Address)|$st|$loc"
                [OGNative]::Progress = [math]::Min(18.0, [OGNative]::Progress + 0.2)
            }
            $u.Close()
        } catch {}
        $out.Ssdp = @($ssdp | Select-Object -Unique)
        [OGNative]::Progress = 18

        $gwLoc = @($out.Ssdp | ForEach-Object { $p = $_ -split '\|', 3; if ($p[0] -eq $a.Gateway -and $p[1] -match 'InternetGatewayDevice|WAN(IP|PPP)Connection') { $p[2] } }) | Select-Object -First 1
        if ($gwLoc) {
            $igd = @{ Maps = @(); External = '' }
            try {
                $x = [xml](Invoke-WebRequest -Uri $gwLoc -UseBasicParsing -TimeoutSec 4).Content
                $svc = @($x.GetElementsByTagName('service') | Where-Object { $_.serviceType -match 'WAN(IP|PPP)Connection' }) | Select-Object -First 1
                if ($svc) {
                    $ctl = [string]$svc.controlURL
                    $url = if ($ctl -match '^https?://') { $ctl } else { ([uri]$gwLoc).GetLeftPart('Authority') + $(if ($ctl.StartsWith('/')) { $ctl } else { '/' + $ctl }) }
                    $type = [string]$svc.serviceType
                    $soap = {
                        param($action, $inner)
                        $body = "<?xml version=`"1.0`"?><s:Envelope xmlns:s=`"http://schemas.xmlsoap.org/soap/envelope/`" s:encodingStyle=`"http://schemas.xmlsoap.org/soap/encoding/`"><s:Body><u:$action xmlns:u=`"$type`">$inner</u:$action></s:Body></s:Envelope>"
                        try { [xml](Invoke-WebRequest -Uri $url -Method Post -Body $body -ContentType 'text/xml; charset="utf-8"' -Headers @{ SOAPAction = "`"$type#$action`"" } -UseBasicParsing -TimeoutSec 3).Content } catch { $null }
                    }
                    $e = & $soap 'GetExternalIPAddress' ''
                    if ($e) { $igd.External = [string]($e.GetElementsByTagName('NewExternalIPAddress') | Select-Object -First 1).InnerText }
                    for ($i = 0; $i -lt 64; $i++) {
                        $r = & $soap 'GetGenericPortMappingEntry' "<NewPortMappingIndex>$i</NewPortMappingIndex>"
                        if (-not $r) { break }
                        $m = @{}
                        foreach ($n in $r.GetElementsByTagName('*')) { $m[$n.LocalName] = [string]$n.InnerText }
                        $igd.Maps += "$($m.NewExternalPort)|$($m.NewProtocol)|$($m.NewInternalPort)|$($m.NewInternalClient)|$(($m.NewPortMappingDescription) -replace '\|', ' ')"
                    }
                }
            } catch {}
            $out.Igd = $igd
        }
        [OGNative]::Progress = 25

        # DNS : réponses truquées ?
        [OGNative]::Phase = 'dns'
        $dns = @{ Servers = @(); Redirect = @(); NxHijack = '' }
        try { $dns.Servers = @(Get-DnsClientServerAddress -InterfaceIndex $a.If -ErrorAction Stop | ForEach-Object { $_.ServerAddresses } | Where-Object { $_ -notlike 'fec0:*' } | ForEach-Object { [string]$_ }) } catch {}
        foreach ($t in @(@('one.one.one.one', '1.1.1.1', '1.0.0.1'), @('dns.google', '8.8.8.8', '8.8.4.4'))) {
            try {
                $ips = @([Net.Dns]::GetHostAddresses($t[0]) | Where-Object { $_.AddressFamily -eq 'InterNetwork' } | ForEach-Object { $_.IPAddressToString })
                $wrong = @($ips | Where-Object { $_ -notin @($t[1], $t[2]) })
                if ($wrong.Count) { $dns.Redirect += "$($t[0]) répond $($wrong -join ', ') au lieu de $($t[1])" }
            } catch {}
        }
        try {
            $fake = "optigame-$([guid]::NewGuid().ToString('N').Substring(0, 12)).com"
            $got = @([Net.Dns]::GetHostAddresses($fake) | ForEach-Object { $_.IPAddressToString })
            if ($got.Count) { $dns.NxHijack = $got -join ', ' }
        } catch {}
        $out.Dns = $dns
        try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; $out.PublicIp = [string](Invoke-RestMethod 'https://api.ipify.org' -TimeoutSec 5) } catch {}
        [OGNative]::Progress = 35

        # Services ouverts sur les appareils
        [OGNative]::Phase = 'devices'
        $out.Ports = @([OGNative]::ScanHosts([string[]]@($a.Ips), [int[]]@($a.Ports), 800, 35.0, 85.0))

        # Ce PC
        [OGNative]::Phase = 'pc'; [OGNative]::Progress = 86
        $pc = @{ Smb1 = $false; FirewallOff = @(); Shares = @() }
        try { $pc.Smb1 = [bool](Get-SmbServerConfiguration -ErrorAction Stop).EnableSMB1Protocol } catch {}
        $pc.Smb1Client = Test-Path 'HKLM:\SYSTEM\CurrentControlSet\Services\mrxsmb10'
        $pc.Rdp = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server' -ErrorAction SilentlyContinue).fDenyTSConnections -eq 0
        try { $pc.FirewallOff = @(Get-NetFirewallProfile -ErrorAction Stop | Where-Object { [string]$_.Enabled -eq 'False' } | ForEach-Object { [string]$_.Name }) } catch {}
        try { $pc.Category = [string](Get-NetConnectionProfile -InterfaceIndex $a.If -ErrorAction Stop | Select-Object -First 1).NetworkCategory } catch {}
        [OGNative]::Progress = 92
        try {
            $every = @('Everyone', 'Tout le monde')
            try { $every += ([Security.Principal.SecurityIdentifier]'S-1-1-0').Translate([Security.Principal.NTAccount]).Value } catch {}
            foreach ($s in @(Get-SmbShare -ErrorAction Stop | Where-Object { -not $_.Special -and $_.Name -notmatch '\$$' })) {
                $rights = @(Get-SmbShareAccess -Name $s.Name -ErrorAction SilentlyContinue | Where-Object { $every -contains ([string]$_.AccountName -replace '^.*\\', '') -and [string]$_.AccessControlType -eq 'Allow' } | ForEach-Object { [string]$_.AccessRight })
                $pc.Shares += "$($s.Name)|$($s.Path)|$($rights -join ',')"
            }
        } catch {}
        $out.Pc = $pc
        [OGNative]::Progress = 100
        $out
    } catch { @{ Error = $_.Exception.GetBaseException().Message } }
}

function Add-AuditCheck($List, [string]$Status, [string]$Cat, [string]$Title, [string]$Detail, $Items, $Actions) {
    [void]$List.Add(@{
        Status = $Status; Cat = $Cat; Title = $Title; Detail = $Detail
        Items = @(@($Items) | Where-Object { $_ }); Actions = @(@($Actions) | Where-Object { $_ })
    })
}

function Get-NetAuditChecks($R, $Devs, $Net) {
    $list = New-Object System.Collections.ArrayList
    $gw = [string]$Net.Gateway
    $boxAct = @{ Label = 'Ouvrir la box'; Arg = "http://$gw"; Script = { param($u) Open-Url $u }; NoRefresh = $true }
    $dnsAct = @{ Label = 'Changer de DNS'; NoRefresh = $true; Script = { Hide-TestPanel; Show-Page 3 } }
    $rerun = { Invoke-NetAudit }
    $openRe = '(?i)^(ouvrir|ouvert|open)'
    $byIp = @{}
    foreach ($d in @($Devs)) { $byIp[[string]$d.Ip] = $d }
    $nameOf = { param($ip) $dv = $byIp[[string]$ip]; if ($dv) { "$($dv.Title) ($ip)" } else { [string]$ip } }
    $plural = { param($n, $one, $many) if ($n -gt 1) { "$n $many" } else { "$n $one" } }

    # Wi-Fi
    $w = $R.Wifi
    $auth = ''
    if ($w) {
        $auth = [string]$w.Auth; $ciph = [string]$w.Cipher; $ssid = [string]$w.Ssid
        if ($auth -match 'WPA3') { Add-AuditCheck $list 'ok' 'wifi' "Wi-Fi « $ssid » protégé en WPA3" 'Le meilleur niveau de protection actuel.' }
        elseif ($auth -match 'WPA2' -and $ciph -match 'TKIP') { Add-AuditCheck $list 'warn' 'wifi' "Wi-Fi « $ssid » : chiffrement ancien (TKIP)" 'Le WPA2 est bon, mais le chiffrement TKIP est dépassé et ralentit le Wi-Fi. Choisis « WPA2 (AES) » ou « WPA2/WPA3 » dans les réglages Wi-Fi de ta box.' $null @($boxAct) }
        elseif ($auth -match 'WPA2') { Add-AuditCheck $list 'ok' 'wifi' "Wi-Fi « $ssid » protégé en WPA2" 'Bonne protection. Si ta box propose le mode WPA2/WPA3, il est encore plus sûr.' }
        elseif ($auth -match 'WPA') { Add-AuditCheck $list 'bad' 'wifi' "Wi-Fi « $ssid » : protection dépassée (WPA)" 'Cette ancienne méthode se casse facilement. Passe en WPA2 ou WPA3 dans les réglages Wi-Fi de ta box.' $null @($boxAct) }
        elseif ($auth -match 'WEP' -or $ciph -match 'WEP') { Add-AuditCheck $list 'bad' 'wifi' "Wi-Fi « $ssid » protégé en WEP" 'Le WEP se casse en quelques minutes : n''importe qui à portée peut entrer sur ton réseau. Passe en WPA2 ou WPA3 dans les réglages Wi-Fi de ta box.' $null @($boxAct) }
        elseif ($auth -match $openRe) { Add-AuditCheck $list 'bad' 'wifi' "Wi-Fi « $ssid » sans mot de passe" 'Tout le monde à portée peut se connecter et voir ce qui passe en clair. Si c''est ton Wi-Fi, mets un mot de passe WPA2 ou WPA3 depuis la page de ta box. Si c''est un Wi-Fi public, évite les sites sensibles.' $null @($boxAct) }
        else { Add-AuditCheck $list 'info' 'wifi' "Wi-Fi « $ssid » : protection non reconnue ($auth)" 'Vérifie dans la page de ta box que le Wi-Fi est en WPA2 ou WPA3.' $null @($boxAct) }
    } else {
        Add-AuditCheck $list 'info' 'wifi' 'Ce PC est branché par câble' 'La protection du Wi-Fi ne peut pas être lue depuis ce PC. Vérifie dans la page de ta box qu''il est en WPA2 ou WPA3.' $null @($boxAct)
    }
    $profiles = @(foreach ($l in @($R.Profiles)) { $x = ([string]$l) -split '\|'; if ($x.Count -ge 5) { [pscustomobject]@{ Name = $x[0]; Auto = $x[3] -eq '1'; Open = $x[4] -eq '1' } } })
    $openAuto = @($profiles | Where-Object { $_.Auto -and $_.Open } | ForEach-Object { $_.Name })
    if ($openAuto.Count) {
        Add-AuditCheck $list 'warn' 'wifi' "Connexion automatique à $(& $plural $openAuto.Count 'Wi-Fi ouvert' 'Wi-Fi ouverts')" 'Ton PC se reconnecte tout seul à ces réseaux sans protection. Un pirate peut créer un faux Wi-Fi avec le même nom pour espionner ta connexion.' $openAuto @(
            @{ Label = 'Passer en connexion manuelle'; Arg = $openAuto; After = $rerun
               Confirm = 'Ces Wi-Fi ne se connecteront plus automatiquement. Tu pourras toujours t''y connecter en cliquant dessus. Continuer ?'
               Script = { param($names) foreach ($n in $names) { netsh wlan set profileparameter "name=$n" connectionmode=manual | Out-Null } } })
    } elseif ($profiles.Count) {
        Add-AuditCheck $list 'ok' 'wifi' 'Aucun Wi-Fi ouvert en connexion automatique' "$(& $plural $profiles.Count 'Wi-Fi mémorisé' 'Wi-Fi mémorisés') sur ce PC, tous protégés ou en connexion manuelle."
    }

    # Box : WPS, UPnP, double NAT, page de réglages
    $ssdp = @(foreach ($l in @($R.Ssdp)) { $x = ([string]$l) -split '\|', 3; [pscustomobject]@{ Ip = $x[0]; St = $x[1] } })
    $wpsIps = @($ssdp | Where-Object { $_.St -match 'wifialliance-org:(device:WFADevice|service:WFAWLANConfig)' } | ForEach-Object { $_.Ip } | Select-Object -Unique)
    if ($wpsIps.Count) {
        $title = if ($wpsIps.Count -eq 1 -and $wpsIps[0] -eq $gw) { 'Le WPS semble activé sur ta box' } else { "Le WPS semble activé sur $(& $plural $wpsIps.Count 'appareil' 'appareils')" }
        Add-AuditCheck $list 'warn' 'box' $title 'Le WPS permet de connecter un appareil en appuyant sur un bouton, mais son code PIN est une faille connue qui permet de trouver le mot de passe du Wi-Fi. Si tu ne t''en sers pas, désactive le dans les réglages Wi-Fi de ta box et de tes répéteurs.' @($wpsIps | ForEach-Object { & $nameOf $_ }) @($boxAct)
    } else {
        Add-AuditCheck $list 'ok' 'box' 'WPS non annoncé sur le réseau' 'Aucun appareil ne propose la connexion par code PIN WPS.'
    }
    $risky = @{ 21 = 'FTP'; 22 = 'SSH'; 23 = 'Telnet'; 80 = 'page web'; 139 = 'partage Windows'; 445 = 'partage Windows'; 554 = 'caméra'; 3389 = 'Bureau à distance'; 5900 = 'VNC'; 8080 = 'page web' }
    $igd = $R.Igd
    if ($igd) {
        $maps = @(foreach ($m in @($igd.Maps)) { $x = ([string]$m) -split '\|', 5; if ($x.Count -ge 5) { [pscustomobject]@{ Ext = $x[0]; Proto = $x[1]; Int = [int]$x[2]; Client = $x[3]; Desc = $x[4] } } })
        $mapText = { param($m) "Port $($m.Ext) ($($m.Proto)) vers $(& $nameOf $m.Client)$(if ($m.Desc) { ' : ' + $m.Desc })" }
        $danger = @($maps | Where-Object { $risky.ContainsKey($_.Int) })
        $safe = @($maps | Where-Object { -not $risky.ContainsKey($_.Int) })
        if ($danger.Count) {
            Add-AuditCheck $list 'bad' 'box' "$(& $plural $danger.Count 'service sensible ouvert' 'services sensibles ouverts') sur Internet" 'Ces règles, créées automatiquement par un appareil (UPnP), rendent un service sensible joignable depuis tout Internet : n''importe qui peut essayer de s''y connecter. Supprime les dans la page de ta box (rubrique NAT, PAT ou UPnP) si tu ne sais pas à quoi elles servent.' @($danger | ForEach-Object { "$(& $mapText $_) ($($risky[$_.Int]))" }) @($boxAct)
        }
        if ($safe.Count -or -not $maps.Count) {
            $title = if ($safe.Count) { "UPnP activé : $(& $plural $safe.Count 'port ouvert' 'ports ouverts') automatiquement" } else { 'UPnP activé, aucun port ouvert' }
            Add-AuditCheck $list 'ok' 'box' $title 'L''UPnP permet à tes jeux et consoles d''ouvrir les ports dont ils ont besoin (NAT ouvert). Rien de sensible dans ces règles.' @($safe | ForEach-Object { & $mapText $_ })
        }
        $ext = [string]$igd.External
        if ($ext -match '^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.)') {
            Add-AuditCheck $list 'info' 'box' 'Double NAT détecté' "Ta box n'a pas directement une adresse Internet ($ext) : elle est derrière un autre routeur, ou ton opérateur partage l'adresse entre plusieurs clients. En jeu, ça peut donner un « NAT strict » et des soucis pour héberger une partie."
        }
    } else {
        Add-AuditCheck $list 'ok' 'box' 'UPnP non détecté sur ta box' 'Aucun appareil ne peut ouvrir de port vers Internet tout seul. Si tu as un « NAT strict » en jeu, activer l''UPnP dans ta box peut aider.'
    }
    $open = @{}
    foreach ($l in @($R.Ports)) { $x = ([string]$l) -split '\|'; if (-not $open.ContainsKey($x[0])) { $open[$x[0]] = @() }; $open[$x[0]] += [int]$x[1] }
    $gwPorts = @($open[$gw])
    if ($gwPorts -contains 80 -and $gwPorts -notcontains 443) {
        Add-AuditCheck $list 'info' 'box' 'Page de réglages de la box non chiffrée' 'La page de ta box est en http simple : son mot de passe passe en clair sur ton réseau. Pas grave tant que seules des personnes de confiance sont connectées chez toi.' $null @($boxAct)
    }

    # DNS
    $dns = $R.Dns
    $knownDns = @{ '1.1.1.1' = 'Cloudflare'; '1.0.0.1' = 'Cloudflare'; '8.8.8.8' = 'Google'; '8.8.4.4' = 'Google'; '9.9.9.9' = 'Quad9'; '149.112.112.112' = 'Quad9'; '208.67.222.222' = 'OpenDNS'; '208.67.220.220' = 'OpenDNS'; '94.140.14.14' = 'AdGuard'; '94.140.15.15' = 'AdGuard' }
    $srv = @(@($dns.Servers) | Where-Object { $_ } | Select-Object -Unique | ForEach-Object {
        $lbl = if ($_ -eq $gw) { 'ta box' } elseif ($knownDns[$_]) { $knownDns[$_] } elseif ($_ -match '^(fe80:|192\.168\.|10\.|172\.)') { 'réseau local' } else { 'fournisseur d''accès ou autre' }
        "Serveur $_ ($lbl)"
    })
    if (@($dns.Redirect).Count) {
        Add-AuditCheck $list 'bad' 'dns' 'Tes recherches Internet sont détournées' 'Des adresses connues ne donnent pas la bonne réponse : ton serveur DNS peut t''envoyer vers de faux sites. Si tu n''utilises pas de filtre (contrôle parental, bloqueur de pub), change de DNS et vérifie les réglages DNS de ta box.' (@($dns.Redirect) + $srv) @($dnsAct, $boxAct)
    } elseif ($dns.NxHijack) {
        Add-AuditCheck $list 'warn' 'dns' 'Ton DNS redirige les adresses qui n''existent pas' 'Quand tu tapes une mauvaise adresse, tu arrives sur une page de pub au lieu d''une erreur. Pas dangereux, mais un DNS comme Cloudflare (1.1.1.1) évite ça et répond souvent plus vite.' $srv @($dnsAct)
    } else {
        Add-AuditCheck $list 'ok' 'dns' 'DNS fiable' 'Les réponses DNS sont correctes : pas de redirection vers de faux sites.' $srv
    }

    # Appareils
    $telnet = @(); $ftp = @(); $remote = @(); $cams = @()
    foreach ($ip in $open.Keys) {
        $ps = $open[$ip]; $nm = & $nameOf $ip
        if ($ps -contains 23) { $telnet += $nm }
        if ($ps -contains 21) { $ftp += $nm }
        if ($ps -contains 3389) { $remote += "$nm : Bureau à distance" }
        if ($ps -contains 5900) { $remote += "$nm : VNC" }
        if ($ps -contains 554) { $cams += $nm }
    }
    $checked = @($Devs | Where-Object { -not $_.Self }).Count
    if ($telnet.Count) { Add-AuditCheck $list 'bad' 'devices' "Telnet ouvert sur $(& $plural $telnet.Count 'appareil' 'appareils')" 'Telnet permet de prendre la main sur un appareil sans aucun chiffrement, et ces appareils ont souvent un mot de passe par défaut connu de tous. Désactive le dans les réglages de l''appareil, ou mets à jour son logiciel.' $telnet }
    if ($ftp.Count) { Add-AuditCheck $list 'warn' 'devices' "Transfert de fichiers FTP ouvert sur $(& $plural $ftp.Count 'appareil' 'appareils')" 'Le FTP envoie les fichiers et les mots de passe en clair. Normal sur certains NAS ou box : si tu ne t''en sers pas, désactive le dans les réglages de l''appareil.' $ftp }
    if ($remote.Count) { Add-AuditCheck $list 'warn' 'devices' 'Prise de contrôle à distance ouverte' 'Ces appareils acceptent qu''on prenne le contrôle de leur écran. Si ce n''est pas voulu, désactive le dans leurs réglages, et utilise toujours un mot de passe fort.' $remote }
    if ($cams.Count) { Add-AuditCheck $list 'info' 'devices' "Flux vidéo sur $(& $plural $cams.Count 'appareil' 'appareils')" 'Souvent une caméra ou un décodeur TV. Vérifie que tes caméras sont protégées par un mot de passe que tu as choisi toi même.' $cams }
    if (-not ($telnet.Count + $ftp.Count + $remote.Count)) { Add-AuditCheck $list 'ok' 'devices' "Aucun service dangereux sur tes $(& $plural $checked 'appareil' 'appareils')" 'Pas de Telnet, de FTP ni de prise de contrôle à distance ouverts.' }
    $devText = { param($d) "$($d.Title) ($($d.Ip))$(if ($d.Vendor -eq 'Adresse privée') { ', adresse masquée' } elseif ($d.Vendor) { ', ' + $d.Vendor })" }
    $camDevs = @($Devs | Where-Object { $_.Camera })
    if ($camDevs.Count) {
        Add-AuditCheck $list 'warn' 'devices' "$(& $plural $camDevs.Count 'caméra possible' 'caméras possibles') sur ton réseau" 'Vérifie que tu sais à qui elles sont et où elles filment (utile aussi en location ou en colocation). Une caméra doit avoir un mot de passe que tu as choisi toi même.' @($camDevs | ForEach-Object { "$(& $devText $_) : $(@($_.CameraWhy) -join ', ')" }) $null
    }
    $new = @($Devs | Where-Object { $_.New })
    $unk = @($Devs | Where-Object { $_.Title -eq 'Appareil inconnu' -and -not $_.New })
    if ($new.Count) { Add-AuditCheck $list 'warn' 'devices' "$(& $plural $new.Count 'nouvel appareil' 'nouveaux appareils') depuis le dernier scan" 'Si tu ne les reconnais pas, regarde la liste des appareils dans la page de ta box et change le mot de passe du Wi-Fi.' @($new | ForEach-Object { & $devText $_ }) @($boxAct) }
    if ($unk.Count) { Add-AuditCheck $list 'info' 'devices' "$(& $plural $unk.Count 'appareil non identifié' 'appareils non identifiés')" 'Leur nom et leur fabricant sont cachés : souvent des téléphones récents qui masquent leur adresse. Tu peux les retrouver dans la liste des appareils de ta box.' @($unk | ForEach-Object { & $devText $_ }) @($boxAct) }
    if (-not $new.Count -and -not $unk.Count) { Add-AuditCheck $list 'ok' 'devices' 'Tous les appareils sont identifiés' 'Aucun appareil inconnu ou nouveau sur ton réseau.' }

    # Ce PC
    $pc = $R.Pc
    if (@($pc.FirewallOff).Count) {
        Add-AuditCheck $list 'bad' 'pc' 'Pare-feu de Windows désactivé' "Il est coupé sur : $(@($pc.FirewallOff) -join ', '). Ton PC accepte alors toutes les connexions venant du réseau." $null @(
            @{ Label = 'Réactiver le pare-feu'; After = $rerun; Confirm = 'Réactiver le pare-feu de Windows sur tous les réseaux ?'; Script = { Set-NetFirewallProfile -Profile Domain, Public, Private -Enabled True } })
    } else {
        Add-AuditCheck $list 'ok' 'pc' 'Pare-feu de Windows activé' 'Il bloque les connexions non voulues vers ce PC.'
    }
    if ($pc.Smb1 -or $pc.Smb1Client) {
        Add-AuditCheck $list 'bad' 'pc' 'Ancien partage de fichiers SMBv1 activé' 'Cette vieille version du partage de fichiers Windows est la faille utilisée par le virus WannaCry. Plus aucun appareil récent n''en a besoin.' $null @(
            @{ Label = 'Désactiver SMBv1'; Arg = [bool]$pc.Smb1Client; After = $rerun
               Confirm = 'Désactiver SMBv1 ? Un redémarrage peut être nécessaire. Seuls de très vieux appareils (NAS ou imprimantes d''avant 2010) pourraient ne plus accéder à ce PC.'
               Script = {
                   param($client)
                   Set-SmbServerConfiguration -EnableSMB1Protocol $false -Force -Confirm:$false -ErrorAction SilentlyContinue
                   if ($client) {
                       Set-Status 'Désactivation de SMBv1...'
                       [void](Invoke-Async { Disable-WindowsOptionalFeature -Online -FeatureName SMB1Protocol -NoRestart -ErrorAction SilentlyContinue | Out-Null })
                   }
               } })
    } else {
        Add-AuditCheck $list 'ok' 'pc' 'SMBv1 désactivé' 'L''ancienne version du partage de fichiers (faille WannaCry) est coupée.'
    }
    if ($pc.Rdp) {
        Add-AuditCheck $list 'warn' 'pc' 'Bureau à distance activé' 'N''importe qui sur ton réseau peut essayer de se connecter à ce PC avec ton mot de passe Windows. Si tu ne t''en sers pas, désactive le.' $null @(
            @{ Label = 'Désactiver'; After = $rerun
               Confirm = 'Désactiver le Bureau à distance ? Tu pourras revenir en arrière depuis l''onglet Sauvegarde.'
               Script = { Set-Reg 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server' 'fDenyTSConnections' 1; Update-BackupSummary } })
    } else {
        Add-AuditCheck $list 'ok' 'pc' 'Bureau à distance désactivé' 'Personne ne peut prendre le contrôle de ce PC à distance.'
    }
    $cat = [string]$pc.Category
    if ($w -and $auth -match $openRe -and $cat -eq 'Private') {
        Add-AuditCheck $list 'bad' 'pc' 'Wi-Fi ouvert réglé en réseau privé' 'Sur un Wi-Fi sans mot de passe, ton PC doit être en mode public pour rester invisible des autres appareils.' $null @(
            @{ Label = 'Passer en public'; Arg = $Net.IfIndex; After = $rerun; Script = { param($i) Set-NetConnectionProfile -InterfaceIndex $i -NetworkCategory Public -ErrorAction Stop } })
    } elseif ($cat -eq 'Public') {
        Add-AuditCheck $list 'ok' 'pc' 'Réseau en mode public' 'Ton PC est invisible pour les autres appareils du réseau. C''est le réglage le plus sûr (le partage de fichiers entre PC est alors bloqué).'
    } elseif ($cat -eq 'Private') {
        Add-AuditCheck $list 'ok' 'pc' 'Réseau en mode privé' 'Normal chez toi : les autres appareils peuvent voir ce PC. Sur un Wi-Fi d''hôtel ou de gare, choisis toujours « Public ».'
    }
    $rightsTxt = @{ Full = 'contrôle total'; Change = 'modification'; Read = 'lecture' }
    $shares = @(foreach ($l in @($pc.Shares)) { $x = ([string]$l) -split '\|', 3; [pscustomobject]@{ Name = $x[0]; Path = $x[1]; Every = $x[2] } })
    $everyone = @($shares | Where-Object { $_.Every })
    $shareAct = @{ Label = 'Gérer les partages'; NoRefresh = $true; Script = { Start-Process 'fsmgmt.msc' } }
    if ($everyone.Count) {
        Add-AuditCheck $list 'warn' 'pc' "$(& $plural $everyone.Count 'dossier partagé' 'dossiers partagés') avec tout le monde" 'N''importe quel appareil de ton réseau peut ouvrir ces dossiers. Vérifie que c''est voulu, et retire « Tout le monde » des autorisations sinon.' @($everyone | ForEach-Object { "$($_.Name) ($($_.Path)) : $((@($_.Every -split ',') | ForEach-Object { if ($rightsTxt[$_]) { $rightsTxt[$_] } else { $_ } }) -join ', ')" }) @($shareAct)
    } elseif ($shares.Count) {
        Add-AuditCheck $list 'info' 'pc' "$(& $plural $shares.Count 'dossier partagé' 'dossiers partagés') sur le réseau" 'Seules les personnes autorisées peuvent y accéder.' @($shares | ForEach-Object { "$($_.Name) ($($_.Path))" }) @($shareAct)
    } else {
        Add-AuditCheck $list 'ok' 'pc' 'Aucun dossier partagé' 'Tes fichiers ne sont pas accessibles depuis le réseau.'
    }
    $list
}

function Get-AuditColor([int]$Score) { if ($Score -ge 85) { $Colors.ok } elseif ($Score -ge 60) { $Colors.warn } else { $Colors.bad } }
function Get-AuditLabel([int]$Score) { if ($Score -ge 85) { 'Réseau bien protégé' } elseif ($Score -ge 60) { 'Quelques points à améliorer' } else { 'Réseau à risque' } }

function New-AuditLine($C) {
    $g = New-Grid @('Auto', '*')
    $g.Margin = New-Thickness 0 5 0 5
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 9; $dot.Height = 9; $dot.Fill = Get-Brush $Colors[$C.Status]
    $dot.Margin = New-Thickness 2 6 12 0; $dot.VerticalAlignment = 'Top'
    Add-ToGrid $g $dot 0
    $sp = New-Object System.Windows.Controls.StackPanel
    $head = New-Object System.Windows.Controls.TextBlock
    $head.TextWrapping = 'Wrap'
    $cat = New-Object System.Windows.Documents.Run ("$($AuditCats[$C.Cat])   ")
    $cat.Foreground = Get-Brush '#655E7E'; $cat.FontSize = Get-UiFontSize 11.5; $cat.FontWeight = 'SemiBold'
    $tt = New-Object System.Windows.Documents.Run $C.Title
    $tt.Foreground = Get-Brush '#FFFFFF'; $tt.FontSize = Get-UiFontSize 13.5; $tt.FontWeight = 'SemiBold'
    [void]$head.Inlines.Add($cat); [void]$head.Inlines.Add($tt)
    [void]$sp.Children.Add($head)
    [void]$sp.Children.Add((New-Text $C.Detail 12 '#A6A1BC'))
    foreach ($i in @($C.Items | Select-Object -First 6)) {
        $it = New-Text "•  $i" 11.5 '#6B7486'
        $it.TextTrimming = 'CharacterEllipsis'; $it.TextWrapping = 'NoWrap'; $it.ToolTip = [string]$i
        [void]$sp.Children.Add($it)
    }
    Add-ToGrid $g $sp 1
    $g
}

function Show-NetAuditResult($A) {
    $body = $ui.TestBody
    $bad = @($A.Checks | Where-Object { $_.Status -eq 'bad' }).Count
    $warn = @($A.Checks | Where-Object { $_.Status -eq 'warn' }).Count
    $ok = @($A.Checks | Where-Object { $_.Status -eq 'ok' }).Count
    [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
    $row = New-Grid @('Auto', '*')
    $col = Get-AuditColor $A.Score
    $gauge = New-Gauge 'Sécurité du réseau' $A.Score 100 '{0:N0}' 'sur 100' $col 0
    Add-ToGrid $row $gauge.El 0
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.VerticalAlignment = 'Center'
    $sp.Margin = New-Thickness 20 0 0 0
    [void]$sp.Children.Add((New-Text (Get-AuditLabel $A.Score) 20 $col -Bold))
    $sub = New-Text "Audit du $($A.Date.ToString('dd/MM/yyyy à HH:mm')), $($A.Count) appareils vérifiés." 12.5 '#A6A1BC'
    $sub.Margin = New-Thickness 0 4 0 0
    [void]$sp.Children.Add($sub)
    $chips = New-Object System.Windows.Controls.WrapPanel
    $chips.Margin = New-Thickness 0 10 0 0
    foreach ($c in @(@($bad, 'à corriger', 'bad'), @($warn, 'à surveiller', 'warn'), @($ok, 'OK', 'ok'))) {
        if (-not $c[0] -and $c[2] -ne 'ok') { continue }
        $b = New-Badge "$($c[0]) $($c[1])" $Colors[$c[2]]
        $b.Margin = New-Thickness 0 0 8 6
        [void]$chips.Children.Add($b)
    }
    [void]$sp.Children.Add($chips)
    $exp = New-Button 'Exporter le rapport'
    $exp.HorizontalAlignment = 'Left'
    $exp.Margin = New-Thickness 0 6 0 0
    $exp.Add_Click({ Invoke-Safe { Export-NetAudit } })
    [void]$sp.Children.Add($exp)
    Add-ToGrid $row $sp 1
    [void]$body.Children.Add($row)

    foreach ($sec in @(@('bad', 'À CORRIGER'), @('warn', 'À SURVEILLER'), @('info', 'BON À SAVOIR'))) {
        $items = @($A.Checks | Where-Object { $_.Status -eq $sec[0] })
        if (-not $items.Count) { continue }
        [void]$body.Children.Add((New-SectionTitle $sec[1]))
        $i = 0
        foreach ($c in $items) {
            $card = New-SecurityCard $c
            $card.Margin = New-Thickness 0 4 0 4
            $card.Opacity = 0
            Start-WpfAnim $card ([System.Windows.UIElement]::OpacityProperty) 1 350 (80 * $i)
            [void]$body.Children.Add($card)
            $i++
        }
    }
    $oks = @($A.Checks | Where-Object { $_.Status -eq 'ok' })
    if ($oks.Count) {
        [void]$body.Children.Add((New-SectionTitle 'TOUT VA BIEN'))
        $box = New-Object System.Windows.Controls.Border
        $box.Background = Get-Brush '#16FFFFFF'
        $box.CornerRadius = [System.Windows.CornerRadius]::new(10)
        $box.Padding = New-Thickness 14 8 14 8
        $box.Margin = New-Thickness 0 4 0 0
        $list = New-Object System.Windows.Controls.StackPanel
        foreach ($c in $oks) { [void]$list.Children.Add((New-AuditLine $c)) }
        $box.Child = $list
        [void]$body.Children.Add($box)
    }
    [void]$body.Children.Add((New-SectionTitle 'BONS RÉFLEXES'))
    foreach ($t in $AuditTips) {
        $tip = New-Text "•  $t" 13 '#D3CDE3'
        $tip.Margin = New-Thickness 2 3 0 3
        [void]$body.Children.Add($tip)
    }
    $st = if ($bad) { 'bad' } elseif ($warn) { 'warn' } else { 'ok' }
    Set-TestState $st $(switch ($st) { 'bad' { 'Points à corriger' } 'warn' { 'À surveiller' } default { 'Tout va bien' } })
}

function Update-NetAuditCard {
    if (-not $script:NetAuditSaved -and (Test-Path -LiteralPath $AuditFile)) {
        try { $script:NetAuditSaved = ConvertFrom-Json (Get-Content -LiteralPath $AuditFile -Raw -Encoding UTF8) } catch {}
    }
    $s = $script:NetAuditSaved
    if (-not $s) {
        $ui.NetAuditText.Text = 'Vérifie la sécurité de ton Wi-Fi, de ta box, de tes appareils et de ce PC. Tu obtiens une note, un rapport clair et des corrections en un clic.'
        return
    }
    $col = Get-AuditColor ([int]@($s.Score)[0])
    $ui.NetAuditScore.Text = "$($s.Score)"
    $ui.NetAuditScore.Foreground = Get-Brush $col
    $ui.NetAuditScoreBox.BorderBrush = Get-Brush $col
    $parts = @()
    if ([int]@($s.Bad)[0]) { $parts += "$($s.Bad) à corriger" }
    if ([int]@($s.Warn)[0]) { $parts += "$($s.Warn) à surveiller" }
    $detail = if ($parts.Count) { $parts -join ', ' } else { 'rien à corriger' }
    $ui.NetAuditText.Text = "$(Get-AuditLabel ([int]@($s.Score)[0])). Dernier audit le $($s.Date) : $detail."
    $ui.BtnNetAudit.Content = 'Relancer l''audit'
    $ui.BtnNetAuditView.Visibility = if ($script:NetAudit) { 'Visible' } else { 'Collapsed' }
}

function Invoke-NetAudit {
    if ($script:TestRunning) { return }
    if ($script:NetScanning) { Show-Message 'Attends la fin du scan du réseau, puis relance l''audit.'; return }
    if (-not $script:NetList) { Invoke-NetworkScan }
    if (-not $script:NetList) { return }
    $net = Get-ActiveNet
    if (-not $net -or -not $net.Ip) { Show-Message 'Aucune connexion réseau détectée.'; return }
    $script:DevPing = $null
    if ($script:MonitorTimer) { $script:MonitorTimer.Stop(); $script:MonitorTimer = $null }
    $script:TestRunning = $true
    $ui.BtnNetAudit.IsEnabled = $false
    $ui.BtnNetAuditView.IsEnabled = $false
    Show-TestPanel @{ Tag = 'SÉCU'; Title = 'Audit de sécurité du réseau'; Sub = 'Wi-Fi, box, DNS, appareils et ce PC' }
    Set-TestState 'run' 'Audit en cours'
    Set-TestButtons 'run'
    $ui.BtnTestStop.Visibility = 'Collapsed'
    $body = $ui.TestBody
    $stepper = New-Stepper ([ordered]@{ wifi = 'Wi-Fi'; box = 'Box et UPnP'; dns = 'DNS'; devices = 'Appareils'; pc = 'Ce PC' })
    [void]$body.Children.Add($stepper.El)
    $count = @($script:NetList | Where-Object { -not $_.Self }).Count
    $wait = New-Text "Vérification de ton Wi-Fi, de ta box et de $count appareils. Ça prend une vingtaine de secondes, rien n'est modifié pendant l'audit." 13 '#A6A1BC'
    $wait.Margin = New-Thickness 0 6 0 0
    [void]$body.Children.Add($wait)
    [OGNative]::Cancel = $false; [OGNative]::Progress = 0; [OGNative]::Phase = ''
    $script:CurTest = @{ Def = @{}; Stepper = $stepper; Chart = $null; Freq = @{} }
    $script:TestTimer.Start()
    try {
        $ips = [string[]]@($script:NetList | Where-Object { -not $_.Self } | ForEach-Object { $_.Ip })
        $r = Invoke-Async $NetAuditWork @{ Ip = $net.Ip; Gateway = $net.Gateway; If = $net.IfIndex; Ips = $ips; Ports = $AuditPorts } | Select-Object -First 1
    } finally {
        $script:TestTimer.Stop()
        $script:CurTest = $null
        $script:TestRunning = $false
        $ui.BtnNetAudit.IsEnabled = $true
        $ui.BtnNetAuditView.IsEnabled = $true
        Set-TestButtons 'done'
    }
    $script:LastRun = @{ Fn = { Invoke-NetAudit }; Tile = $null; Ctx = $null }
    if (-not $r -or $r.Error) {
        if ($r.Error) { Write-Log "Audit réseau: $($r.Error)" }
        Set-TestState 'bad' 'Échec'
        [void]$body.Children.Add((New-Verdict 'bad' "L'audit n'a pas pu aller au bout : $(if ($r.Error) { $r.Error } else { 'erreur inconnue' })"))
        return
    }
    Update-Stepper $stepper '' -AllDone
    $body.Children.Remove($wait)
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = '100 %'
    $checks = @(Get-NetAuditChecks $r $script:NetList $net)
    $bad = @($checks | Where-Object { $_.Status -eq 'bad' }).Count
    $warn = @($checks | Where-Object { $_.Status -eq 'warn' }).Count
    $score = [int][math]::Max(0.0, 100.0 - 20 * $bad - 7 * $warn)
    $script:NetAudit = @{ Score = $score; Date = (Get-Date); Checks = $checks; Count = $count; Net = $net }
    $script:NetAuditSaved = [pscustomobject]@{ Score = $score; Date = (Get-Date).ToString('dd/MM/yyyy à HH:mm'); Bad = $bad; Warn = $warn }
    try { ConvertTo-Json -InputObject $script:NetAuditSaved | Set-Content -LiteralPath $AuditFile -Encoding UTF8 } catch {}
    Show-NetAuditResult $script:NetAudit
    Show-ResultTop
    Update-NetAuditCard
    Set-Status "Audit du réseau : $score sur 100."
}

function Show-NetAuditReport {
    if (-not $script:NetAudit -or $script:TestRunning) { return }
    Show-TestPanel @{ Tag = 'SÉCU'; Title = 'Audit de sécurité du réseau'; Sub = 'Wi-Fi, box, DNS, appareils et ce PC' }
    Set-TestButtons 'done'
    $script:LastRun = @{ Fn = { Invoke-NetAudit }; Tile = $null; Ctx = $null }
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    Show-NetAuditResult $script:NetAudit
}

function Export-NetAudit {
    $A = $script:NetAudit
    if (-not $A) { return }
    $dlg = New-Object Microsoft.Win32.SaveFileDialog
    $dlg.Filter = 'Page web (*.html)|*.html'
    $dlg.FileName = "Audit réseau Nevermind $(Get-Date -Format 'yyyy-MM-dd').html"
    $dlg.InitialDirectory = [Environment]::GetFolderPath('Desktop')
    if ($dlg.ShowDialog($Window) -ne $true) { return }
    $enc = { param($s) [System.Net.WebUtility]::HtmlEncode([string]$s) }
    $labels = @{ bad = 'À corriger'; warn = 'À surveiller'; info = 'Bon à savoir'; ok = 'Tout va bien' }
    $sections = foreach ($st in 'bad', 'warn', 'info', 'ok') {
        $items = @($A.Checks | Where-Object { $_.Status -eq $st })
        if (-not $items.Count) { continue }
        $cards = foreach ($c in $items) {
            $li = if ($c.Items) { '<ul>' + ((@($c.Items) | ForEach-Object { "<li>$(& $enc $_)</li>" }) -join '') + '</ul>' } else { '' }
            "<div class='f'><span class='dot' style='background:$($Colors[$st])'></span><div><span class='cat'>$(& $enc $AuditCats[$c.Cat])</span><b>$(& $enc $c.Title)</b><p>$(& $enc $c.Detail)</p>$li</div></div>"
        }
        "<h2 style='color:$($Colors[$st])'>$(& $enc $labels[$st])</h2>`n$($cards -join "`n")"
    }
    $tips = ($AuditTips | ForEach-Object { "<li>$(& $enc $_)</li>" }) -join ''
    $col = Get-AuditColor $A.Score
    $html = @"
<!DOCTYPE html>
<html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Audit réseau Nevermind</title>
<style>
body{margin:0;background:#0D0B14;color:#EEEBF7;font:15px/1.5 'Segoe UI',system-ui,sans-serif}
main{max-width:860px;margin:0 auto;padding:32px 16px}
h1{margin:0;font-size:28px}h1 span{color:#00D9F5}
h2{margin:32px 0 12px;font-size:18px}
.sub{color:#A6A1BC}
.score{display:flex;align-items:center;gap:20px;background:#18151F;border:1px solid #231E33;border-radius:12px;padding:20px;margin-top:24px}
.score b{font-size:48px;color:$col}
.f{display:flex;gap:14px;background:#18151F;border:1px solid #231E33;border-radius:12px;padding:12px 16px;margin-bottom:8px}
.f p{margin:2px 0 0;color:#A6A1BC;font-size:14px}
.f ul{margin:6px 0 0;padding-left:18px;color:#D3CDE3;font-size:13px}
.cat{display:inline-block;margin-right:10px;color:#655E7E;font-size:12px;font-weight:600;text-transform:uppercase}
.dot{flex:none;width:12px;height:12px;border-radius:50%;margin-top:6px}
.tips{background:#18151F;border:1px solid #231E33;border-radius:12px;padding:12px 16px 12px 34px;color:#D3CDE3}
</style></head><body><main>
<h1>Opti<span>Game</span></h1>
<div class="sub">Audit de sécurité du réseau, depuis $(& $enc $env:COMPUTERNAME), le $($A.Date.ToString('dd/MM/yyyy à HH:mm'))</div>
<div class="score"><b>$($A.Score)</b><div><div style="font-size:20px;font-weight:600">$(& $enc (Get-AuditLabel $A.Score))</div><div class="sub">Note de sécurité sur 100, $($A.Count) appareils vérifiés</div></div></div>
$($sections -join "`n")
<h2>Bons réflexes</h2>
<ul class="tips">$tips</ul>
</main></body></html>
"@
    Set-Content -Path $dlg.FileName -Value $html -Encoding UTF8
    Open-Url $dlg.FileName
    Set-Status 'Rapport d''audit exporté.'
}
