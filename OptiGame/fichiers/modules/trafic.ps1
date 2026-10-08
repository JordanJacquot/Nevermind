# Nexo : ce qui sort du PC. Quels programmes communiquent avec Internet, avec qui, combien,
# et ce qui est anormal. Le contenu (chiffré en HTTPS) n'est jamais lu.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

$TrafficIndex = 11
$LolBins = '^(powershell|pwsh|cmd|wscript|cscript|mshta|rundll32|regsvr32|certutil|bitsadmin|msbuild|installutil|regasm|regsvcs|cmstp|wmic|forfiles|msiexec|hh)$'
$RemoteTools = '(?i)^(anydesk|teamviewer\w*|rustdesk|screenconnect\.\w+|connectwise\w*|logmein\w*|splashtop\w*|ultraviewer\w*|supremo\w*|rutserv|rfusclient|aeroadmin|ammyy\w*|remotepc\w*|zohoassist\w*|getscreen\w*)$'
$SusPorts = @{ 4444 = 'port souvent utilisé par les logiciels espions'; 1337 = 'port souvent utilisé par les logiciels espions'; 31337 = 'port souvent utilisé par les logiciels espions'; 6666 = 'discussion IRC (utilisée par des virus)'; 6667 = 'discussion IRC (utilisée par des virus)'; 6697 = 'discussion IRC (utilisée par des virus)'; 8333 = 'réseau Bitcoin'; 3333 = 'minage de cryptomonnaie'; 5555 = 'port souvent utilisé par les logiciels espions'; 9001 = 'réseau Tor'; 9030 = 'réseau Tor'; 9050 = 'réseau Tor'; 9150 = 'réseau Tor'; 12345 = 'port souvent utilisé par les logiciels espions'; 23 = 'Telnet (non chiffré)' }
$UploadOk = '(?i)^(onedrive|dropbox|googledrivefs|box|megasync|icloud\w*|obs\w*|streamlabs\w*|discord|steam\w*|teams|ms-teams|zoom|skype|chrome|msedge|firefox|opera|brave|vivaldi|qbittorrent|utorrent|bittorrent|transmission\w*|backblaze\w*|synology\w*|nvcontainer|nvidia share|shadowplay|medal\w*|outplayed|epicgameslauncher|claude|chatgpt|cursor|code|windsurf|copilot)$'
$PortNames = @{ 443 = 'web sécurisé (HTTPS)'; 80 = 'web (HTTP)'; 53 = 'noms de domaine (DNS)'; 853 = 'DNS chiffré'; 993 = 'mails (IMAP)'; 995 = 'mails (POP)'; 587 = 'envoi de mails'; 465 = 'envoi de mails'; 25 = 'envoi de mails'; 22 = 'accès à distance (SSH)'; 3389 = 'bureau à distance'; 5228 = 'notifications Google'; 5222 = 'messagerie'; 3478 = 'appels audio / vidéo'; 1194 = 'VPN'; 51820 = 'VPN'; 8080 = 'web (autre port)'; 27015 = 'jeux (Steam)'; 27036 = 'Steam'; 5938 = 'TeamViewer'; 7070 = 'AnyDesk'; 6568 = 'AnyDesk' }

function Format-Bytes([double]$B) {
    if ($B -ge 1GB) { return '{0:N1} Go' -f ($B / 1GB) }
    if ($B -ge 1MB) { return '{0:N1} Mo' -f ($B / 1MB) }
    if ($B -ge 1KB) { return '{0:N0} Ko' -f ($B / 1KB) }
    "$([int]$B) o"
}

function Test-PrivateIp([string]$Ip) {
    $Ip -match '^(10\.|127\.|192\.168\.|169\.254\.|172\.(1[6-9]|2\d|3[01])\.|0\.|::1$|fe80:|f[cd][0-9a-f]{2}:|ff)' -or $Ip -eq '::'
}

# ---------------------------------------------------------------------------
# Relevé toutes les 2 secondes
# ---------------------------------------------------------------------------
function New-TrafficState {
    @{ Conns = @{}; Pids = @{}; Apps = @{}; Dns = @{}; DnsAt = [datetime]::MinValue; Sig = @{}; SigQueue = (New-Object System.Collections.Queue)
       Started = Get-Date; Ticks = 0; Notified = @{}; AlertKeys = ''; Owner = @{}; OwnerMiss = @{}; LookupDone = @{}; LookupTries = @{}; LookupJob = $null }
}

# Programme d'un processus (plusieurs processus du même programme, comme Chrome, sont regroupés).
function Get-TrafficApp([int]$ProcId) {
    $st = $script:Traffic
    if ($st.Pids.ContainsKey($ProcId)) { return $st.Pids[$ProcId] }
    $p = Get-Process -Id $ProcId -ErrorAction SilentlyContinue
    $name = if ($ProcId -eq 4) { 'System' } elseif ($p) { $p.ProcessName } else { "Programme $ProcId" }
    $path = if ($p) { try { [string]$p.Path } catch { '' } } else { '' }
    if (-not $path -and $ProcId -gt 4) { try { $path = [TrafficMon]::GetProcessPath($ProcId) } catch {} }
    $svc = if ($name -eq 'svchost') { @(Get-SvcNames $ProcId) } else { @() }
    $key = if ($ProcId -eq $PID) { 'optigame' } elseif ($svc.Count) { 'svc:' + (($svc | ForEach-Object { $_.Name.ToLower() }) -join ',') } elseif ($path) { $path.ToLower() } else { "nom:$($name.ToLower())" }
    if (-not $st.Apps.ContainsKey($key)) {
        $desc = ''
        if ($path) { try { $desc = [string][Diagnostics.FileVersionInfo]::GetVersionInfo($path).FileDescription } catch {} }
        if ($svc.Count) { $desc = "Windows : $($svc[0].Title)$(if ($svc.Count -gt 1) { " (+$($svc.Count - 1))" })" }
        $st.Apps[$key] = @{ Key = $key; Name = $name; Path = $path; Services = $svc; Title = $(if ($ProcId -eq $PID) { 'Nexo (cette app)' } elseif ($desc -and $desc.Length -lt 90) { $desc } else { $name })
            OutClosed = [double]0; InClosed = [double]0; Out = [double]0; In = [double]0; Rate = [double]0; LastOut = [double]0
            Dest = @{}; Ports = @{}; Udp = $false; Pids = @{}; Sig = $null; Publisher = ''; First = Get-Date; Icon = $null; IsSelf = ($ProcId -eq $PID) }
        if ($path -and -not $st.Sig.ContainsKey($key)) { $st.SigQueue.Enqueue($key) } elseif (-not $path) { $st.Apps[$key].Sig = 'NoPath' }
    }
    $st.Apps[$key].Pids[$ProcId] = $true
    $st.Pids[$ProcId] = $key
    $key
}

function Update-TrafficDns {
    $st = $script:Traffic
    if (((Get-Date) - $st.DnsAt).TotalSeconds -lt 10) { return }
    $job = $st.DnsJob
    if ($job) {
        if (-not $job.Handle.IsCompleted) { return }
        try { foreach ($p in @($job.PS.EndInvoke($job.Handle))) { $x = ([string]$p) -split '\|', 2; if ($x.Count -eq 2) { $st.Dns[$x[0]] = $x[1] } } } catch {} finally { $job.PS.Dispose(); $st.DnsJob = $null }
    }
    $st.DnsAt = Get-Date
    $ps = [PowerShell]::Create()
    $ps.RunspacePool = $script:BgPool
    [void]$ps.AddScript('try { Get-DnsClientCache -ErrorAction Stop | Where-Object { $_.Type -in 1, 28 -and $_.Data } | ForEach-Object { "$($_.Data)|$(([string]$_.Entry).TrimEnd(''.''))" } } catch {}')
    $st.DnsJob = @{ PS = $ps; Handle = $ps.BeginInvoke() }
}

function Update-Traffic {
    $st = $script:Traffic
    if (-not $st) { return }
    $st.Ticks++
    # Processus fermés : Windows réutilise leurs numéros, un nouveau programme serait sinon compté sous l'ancien nom
    if ($st.Ticks % 15 -eq 0) {
        $alive = @{}
        foreach ($p in [Diagnostics.Process]::GetProcesses()) { $alive[$p.Id] = $true; $p.Dispose() }
        foreach ($procId in @($st.Pids.Keys)) {
            if ($alive.ContainsKey($procId)) { continue }
            $a = $st.Apps[$st.Pids[$procId]]
            if ($a) { $a.Pids.Remove($procId) }
            $st.Pids.Remove($procId)
        }
    }
    Update-TrafficDns
    $now = Get-Date
    $seen = @{}
    foreach ($l in @([TrafficMon]::Tcp())) {
        $x = ([string]$l) -split '\|'
        if ($x.Count -lt 7) { continue }
        $procId = [int]$x[0]; $remote = $x[1]; $rport = [int]$x[2]
        if ($procId -eq 0) { continue }
        $ck = "$procId|$remote|$rport|$($x[3])"
        $seen[$ck] = $true
        $ak = Get-TrafficApp $procId
        $o = [double]$x[5]; $in = [double]$x[6]
        if ($st.Conns.ContainsKey($ck)) { $cn = $st.Conns[$ck]; $cn.Out = [math]::Max($cn.Out, $o); $cn.In = [math]::Max($cn.In, $in) }
        else { $st.Conns[$ck] = @{ App = $ak; Remote = $remote; Port = $rport; Out = $o; In = $in } }
    }
    # Connexions fermées : leurs octets restent comptés pour le programme et la destination
    foreach ($ck in @($st.Conns.Keys)) {
        if ($seen.ContainsKey($ck)) { continue }
        $cn = $st.Conns[$ck]
        $a = $st.Apps[$cn.App]
        if ($a) {
            $a.OutClosed += $cn.Out; $a.InClosed += $cn.In
            $dk = "$($cn.Remote)|$($cn.Port)"
            if ($a.Dest.ContainsKey($dk)) { $a.Dest[$dk].OutClosed += $cn.Out; $a.Dest[$dk].InClosed += $cn.In }
        }
        $st.Conns.Remove($ck)
    }
    # Totaux par programme et par destination
    foreach ($a in $st.Apps.Values) { $a.Out = $a.OutClosed; $a.In = $a.InClosed; $a.Live = 0; foreach ($d in $a.Dest.Values) { $d.Out = $d.OutClosed; $d.In = $d.InClosed; $d.Live = $false } }
    foreach ($cn in $st.Conns.Values) {
        $a = $st.Apps[$cn.App]
        if (-not $a) { continue }
        $a.Out += $cn.Out; $a.In += $cn.In; $a.Live++
        $dk = "$($cn.Remote)|$($cn.Port)"
        if (-not $a.Dest.ContainsKey($dk)) { $a.Dest[$dk] = @{ Remote = $cn.Remote; Port = $cn.Port; OutClosed = [double]0; InClosed = [double]0; Out = [double]0; In = [double]0; First = $now; Live = $true; Private = (Test-PrivateIp $cn.Remote) } }
        $d = $a.Dest[$dk]; $d.Out += $cn.Out; $d.In += $cn.In; $d.Live = $true; $d.Last = $now
    }
    foreach ($a in $st.Apps.Values) {
        $a.Rate = [math]::Max(0.0, ($a.Out - $a.LastOut) / 2)
        $a.LastOut = $a.Out
    }
    # UDP : seulement « ce programme en utilise » (Windows ne donne pas les destinations UDP)
    if ($st.Ticks % 5 -eq 1) {
        foreach ($l in @([TrafficMon]::Udp())) { $procId = [int](($l -split '\|')[0]); if ($procId -gt 4) { $st.Apps[(Get-TrafficApp $procId)].Udp = $true } }
    }
    Update-TrafficSignatures
    try { Update-ServerLookups } catch { Write-Log "Serveurs: $_" }
    Test-TrafficAlerts
    if ($ui.Tabs.SelectedIndex -eq $TrafficIndex -and $Window.IsVisible) { Update-TrafficView }
}

function Start-TrafficWatch {
    if (-not $script:Traffic) { $script:Traffic = New-TrafficState }
    if (-not $script:TrafficTimer) {
        $script:TrafficTimer = New-Object System.Windows.Threading.DispatcherTimer
        $script:TrafficTimer.Interval = [TimeSpan]::FromSeconds(2)
        $script:TrafficTimer.Add_Tick({ try { Update-Traffic } catch { Write-Log "Trafic: $_" } })
    }
    $script:TrafficTimer.Start()
    $ui.BtnTraffic.Content = 'Arrêter la surveillance'
    Update-Traffic
    Set-Status 'Surveillance de ce qui sort du PC activée.'
}

function Stop-TrafficWatch {
    if ($script:TrafficTimer) { $script:TrafficTimer.Stop() }
    $ui.BtnTraffic.Content = 'Reprendre la surveillance'
    Set-Status 'Surveillance arrêtée (les chiffres affichés sont gardés).'
}

# Signature de chaque programme, et pour les outils de Windows (PowerShell, cmd...) : qui les a lancés.
$TrafficInspectWork = {
    param($items)
    $sigOf = {
        param($path)
        $r = try {
            $sg = Get-AuthenticodeSignature -FilePath $path -ErrorAction Stop
            @([string]$sg.Status, $(if ($sg.SignerCertificate) { $sg.SignerCertificate.Subject -replace '^.*?CN="?([^",]+).*$', '$1' } else { '' }))
        } catch { @('Error', '') }
        # Applis du Microsoft Store : la signature est celle du paquet, pas celle du fichier .exe
        if ($r[0] -ne 'Valid' -and $path -match '(?i)\\WindowsApps\\([^\\_]+)_[^\\]*__([^\\]+)\\') {
            try {
                $pk = Get-AppxPackage -Name $Matches[1] -ErrorAction Stop | Where-Object { $_.PublisherId -eq $Matches[2] } | Select-Object -First 1
                if ($pk -and [string]$pk.SignatureKind -in 'Store', 'System') { $r = @('Valid', ($pk.Publisher -replace '^.*?CN="?([^",]+).*$', '$1')) }
            } catch {}
        }
        $r
    }
    foreach ($it in $items) {
        $s = & $sigOf $it.Path
        $parent = ''; $parentSig = ''; $parentPub = ''
        if ($it.Lol) {
            foreach ($procId in $it.Pids) {
                try {
                    $pp = (Get-CimInstance Win32_Process -Filter "ProcessId=$procId" -ErrorAction Stop).ParentProcessId
                    $par = Get-CimInstance Win32_Process -Filter "ProcessId=$pp" -ErrorAction Stop
                    if ($par -and $par.ExecutablePath) {
                        $ps = & $sigOf $par.ExecutablePath
                        $parent = [string]$par.Name; $parentSig = $ps[0]; $parentPub = $ps[1]
                        if ($parentSig -ne 'Valid') { break }   # un parent inconnu suffit à garder l'alerte
                    }
                } catch {}
            }
        }
        @{ Key = $it.Key; Sig = $s[0]; Publisher = $s[1]; Parent = $parent; ParentSig = $parentSig; ParentPub = $parentPub }
    }
}

function Update-TrafficSignatures {
    $st = $script:Traffic
    $job = $st.SigJob
    if ($job) {
        if (-not $job.Handle.IsCompleted) { return }
        $res = @()
        try { $res = @($job.PS.EndInvoke($job.Handle)) } catch {} finally { $job.PS.Dispose(); $st.SigJob = $null }
        foreach ($r in $res) {
            $a = $st.Apps[$r.Key]
            if (-not $a) { continue }
            # Vérification ratée (fichier en cours de mise à jour, disque occupé...) : on réessaie, ce n'est pas « non signé »
            if ($r.Sig -in 'Error', 'UnknownError') {
                $a.SigTries = [int]$a.SigTries + 1
                if ($a.SigTries -lt 3) { $st.SigQueue.Enqueue($r.Key) } else { $a.Sig = 'Unknown' }
                continue
            }
            $a.Sig = $r.Sig; $a.Publisher = $r.Publisher
            $a.Parent = $r.Parent; $a.ParentSig = $r.ParentSig; $a.ParentPub = $r.ParentPub
            $st.Sig[$r.Key] = $a.Sig
        }
    }
    if (-not $st.SigQueue.Count) { return }
    $items = @()
    while ($st.SigQueue.Count -and $items.Count -lt 6) {
        $a = $st.Apps[$st.SigQueue.Dequeue()]
        if ($a -and $a.Path) { $items += @{ Key = $a.Key; Path = $a.Path; Lol = [bool]($a.Name -match $LolBins); Pids = @($a.Pids.Keys) } }
    }
    if (-not $items.Count) { return }
    $ps = [PowerShell]::Create()
    $ps.RunspacePool = $script:BgPool
    [void]$ps.AddScript($TrafficInspectWork.ToString()).AddArgument($items)
    $st.SigJob = @{ PS = $ps; Handle = $ps.BeginInvoke() }
}

# ---------------------------------------------------------------------------
# Ce qui est anormal
# ---------------------------------------------------------------------------
# « C'est normal » (approuvé par l'utilisateur) et « analysé sans rien trouver » : mémorisés.
function Get-TrafficMarks([string]$Name) {
    $h = @{}
    foreach ($l in @(Get-Setting $Name @())) { $x = ([string]$l) -split '\|', 2; if ($x[0]) { $h[$x[0]] = $(if ($x.Count -gt 1) { $x[1] } else { '' }) } }
    $h
}
function Set-TrafficMark([string]$Name, [string]$Key, [bool]$On) {
    $h = Get-TrafficMarks $Name
    if ($On) { $h[$Key] = (Get-Date).ToString('dd/MM/yyyy') } else { $h.Remove($Key) }
    Set-Setting $Name @($h.GetEnumerator() | ForEach-Object { "$($_.Key)|$($_.Value)" })
}
function Refresh-TrafficAlerts {
    Test-TrafficAlerts
    $script:TrafficAlertKeys = $null
    Update-TrafficView
}
function Set-TrafficTrust($App, [bool]$On) {
    if ($On -and -not (Confirm-Action "Faire confiance à « $($App.Title) » ?`n`nIl ne sera plus signalé. Tu pourras revenir sur ce choix depuis sa fiche.")) { return }
    Set-TrafficMark 'TrafficTrusted' $App.Key $On
    Refresh-TrafficAlerts
    Set-Status $(if ($On) { "« $($App.Title) » est approuvé." } else { "« $($App.Title) » sera de nouveau surveillé." })
}
function Invoke-TrafficScan($App) {
    Invoke-DefenderScan 'CustomScan' @($App.Path) "Analyse de $($App.Title)"
    if ($script:LastScanResult -eq 'clean') { Set-TrafficMark 'TrafficScanned' $App.Key $true; Refresh-TrafficAlerts }
}
# Dossiers des jeux installés : les jeux sont souvent non signés, ce n'est pas suspect.
function Test-GamePath([string]$Path) {
    if (-not $Path) { return $false }
    if ($Path -match '(?i)\\steamapps\\common\\|\\Epic Games\\|\\Riot Games\\|\\Battle\.net\\|\\Ubisoft Game Launcher\\games\\|\\EA Games\\|\\GOG Galaxy\\Games\\|\\XboxGames\\|\\Netmarble Game\\|-Win64-Shipping\.exe$|\\Binaries\\Win64\\') { return $true }
    if ($script:GameIndex) { foreach ($g in $script:GameIndex.Values) { foreach ($e in $g.Exes) { if ($Path.ToLower() -eq $e) { return $true } } } }
    $false
}

function Get-TrafficAlerts {
    $st = $script:Traffic
    $alerts = @()
    if (-not $st) { return $alerts }
    $trusted = Get-TrafficMarks 'TrafficTrusted'
    $scanned = Get-TrafficMarks 'TrafficScanned'
    foreach ($a in $st.Apps.Values) {
        if ($a.IsSelf -or $trusted.ContainsKey($a.Key)) { continue }
        $net = @($a.Dest.Values | Where-Object { -not $_.Private })
        if (-not $net.Count) { continue }
        $why = @(); $level = 'info'
        # Analysé sans rien trouver, ou jeu installé : « non signé » n'est plus un indice
        $unsigned = $a.Sig -in 'NotSigned', 'NotTrusted' -and -not $scanned.ContainsKey($a.Key) -and -not (Test-GamePath $a.Path)
        $risky = $a.Path -match '(?i)\\(AppData\\Local\\Temp|Temp|Downloads|Téléchargements|Users\\Public)\\' -or $a.Path -match '(?i)^[a-z]:\\ProgramData\\[^\\]+\.exe$' -or $a.Path -match '(?i)\\AppData\\Roaming\\[^\\]+\.exe$'
        if ($a.Sig -eq 'HashMismatch') { $why += 'sa signature numérique est invalide (le fichier a été modifié)'; $level = 'bad' }
        if ($unsigned -and $risky) { $why += 'il n''est signé par aucun éditeur et il est rangé dans un dossier où les virus aiment se cacher'; $level = 'bad' }
        elseif ($unsigned -and $a.Sig -ne 'HashMismatch') { $why += 'il n''est signé par aucun éditeur connu'; if ($level -ne 'bad') { $level = 'warn' } }
        if ($a.Name -match $LolBins) {
            # Lancé par un programme signé (Claude Code, VS Code, Git...) : normal, simple information
            if ($a.ParentSig -eq 'Valid') { $why += "outil de Windows lancé par $($a.Parent)$(if ($a.ParentPub) { " (signé : $($a.ParentPub))" }), c'est normal" }
            elseif ($a.Parent -or $a.Sig) { $why += "c'est un outil de Windows que les virus détournent souvent pour télécharger ou envoyer des données$(if ($a.Parent) { " (lancé par $($a.Parent), non signé)" })"; if ($level -ne 'bad') { $level = 'warn' } }
        }
        # Un programme signé par son éditeur (un jeu, par exemple) peut utiliser ces ports pour ses serveurs :
        # pour lui, seuls les ports du réseau Tor restent signalés.
        $signedOk = $a.Sig -eq 'Valid'
        $ports = @($net | Where-Object { $SusPorts.ContainsKey([int]$_.Port) -and (-not $signedOk -or [int]$_.Port -in 9001, 9030, 9050, 9150) } | ForEach-Object { "port $($_.Port) : $($SusPorts[[int]$_.Port])" } | Select-Object -Unique)
        if ($ports.Count) { $why += "il utilise un port inhabituel ($($ports -join ', '))"; if ($level -ne 'bad') { $level = 'warn' } }
        if ($a.Out -gt 200MB -and $a.Out -gt 3 * $a.In -and $a.Name -notmatch $UploadOk) { $why += "il envoie beaucoup plus qu'il ne reçoit ($(Format-Bytes $a.Out) envoyés)"; if ($level -ne 'bad') { $level = 'warn' } }
        $ips = @($net | ForEach-Object { $_.Remote } | Select-Object -Unique)
        if ($ips.Count -gt 150 -and $a.Name -notmatch $UploadOk) { $why += "il contacte énormément d'adresses différentes ($($ips.Count))"; if ($level -ne 'bad') { $level = 'warn' } }
        if ($a.Name -match $RemoteTools) { $why += 'c''est un logiciel de prise en main à distance : quelqu''un peut voir et contrôler ton écran'; if ($level -eq 'info') { $level = 'warn' } }
        if ($why.Count) {
            if ($scanned.ContainsKey($a.Key)) { $why += "analysé par l'antivirus le $($scanned[$a.Key]) : aucun virus trouvé" }
            $alerts += @{ App = $a; Level = $level; Why = $why }
        }
    }
    @($alerts | Sort-Object @{ Expression = { if ($_.Level -eq 'bad') { 0 } elseif ($_.Level -eq 'warn') { 1 } else { 2 } } })
}

# Prévient (une fois par programme) quand une alerte sérieuse apparaît, même si la fenêtre est réduite.
function Test-TrafficAlerts {
    $st = $script:Traffic
    $list = @(Get-TrafficAlerts)
    $st.Alerts = $list
    foreach ($al in $list) {
        if ($al.Level -eq 'info' -or $st.Notified.ContainsKey($al.App.Key)) { continue }
        $st.Notified[$al.App.Key] = $true
        Write-Log "Trafic anormal: $($al.App.Name) ($($al.App.Path)) : $($al.Why -join ' ; ')"
        $k = $al.App.Key
        Show-Notify "Activité réseau à vérifier : $($al.App.Title)" "$($al.Why[0]). Clique pour voir." ({ Show-Page $TrafficIndex; Show-TrafficApp $k }.GetNewClosure())
    }
}

# Bloque un programme dans le pare-feu de Windows (annulable depuis l'historique).
function Block-TrafficApp($App) {
    if (-not $App.Path) { Show-Message 'Impossible de bloquer ce programme : son emplacement est inconnu.'; return }
    if (-not (Confirm-Action "Bloquer l'accès à Internet pour « $($App.Title) » ?`n`n$($App.Path)`n`nLe programme ne pourra plus rien envoyer ni recevoir. Tu pourras annuler depuis la page Sauvegarde (historique).")) { return }
    $name = "OptiGame : bloque $($App.Title)"
    $svc = @($App.Services | Where-Object { $_ })
    # Un service Windows partage svchost.exe avec d'autres (DNS, réseau...) : la règle vise ce service seulement
    $targets = if ($svc.Count) { @($svc | ForEach-Object { @{ Service = $_.Name } }) } else { @(@{}) }
    foreach ($t in $targets) {
        foreach ($dir in 'Outbound', 'Inbound') {
            $p = @{ DisplayName = $name; Direction = $dir; Program = $App.Path; Action = 'Block'; Profile = 'Any'; ErrorAction = 'Stop' }
            if ($t.Service) { $p.Service = $t.Service }
            New-NetFirewallRule @p | Out-Null
        }
    }
    [void](Add-History "Internet bloqué pour $($App.Title)" @($App.Path) @(@{ Type = 'fw'; Name = $name }))
    # Les programmes de Windows ne sont pas fermés (ils font tourner d'autres choses)
    $sys = $svc.Count -or $App.Path -like "$env:windir\*"
    if (-not $sys) { Get-Process -ErrorAction SilentlyContinue | Where-Object { try { $_.Path -eq $App.Path } catch { $false } } | ForEach-Object { try { $_.Kill() } catch {} } }
    Set-Status "$($App.Title) ne peut plus accéder à Internet."
    Show-Message "« $($App.Title) » est bloqué$(if (-not $sys) { ' et a été fermé' }).`n`nPour annuler : page Sauvegarde, historique, « Annuler »."
}

# ---------------------------------------------------------------------------
# Serveurs sans nom : à qui ils appartiennent. Recherche inverse (DNS), puis annuaire public
# des adresses Internet (RDAP, via rdap.org). Seule l'adresse du serveur est envoyée.
# ---------------------------------------------------------------------------
$ServerFile = Join-Path $DataDir 'serveurs.json'
$CountryNames = @{ US = 'États-Unis'; FR = 'France'; IE = 'Irlande'; DE = 'Allemagne'; NL = 'Pays-Bas'; GB = 'Royaume-Uni'; BE = 'Belgique'; CH = 'Suisse'; ES = 'Espagne'; IT = 'Italie'; SE = 'Suède'; FI = 'Finlande'; PL = 'Pologne'; LU = 'Luxembourg'; AT = 'Autriche'; CA = 'Canada'; JP = 'Japon'; KR = 'Corée du Sud'; CN = 'Chine'; HK = 'Hong Kong'; TW = 'Taïwan'; SG = 'Singapour'; IN = 'Inde'; AU = 'Australie'; BR = 'Brésil'; RU = 'Russie'; UA = 'Ukraine'; IL = 'Israël'; AE = 'Émirats arabes unis'; ZA = 'Afrique du Sud'; EU = 'Europe' }
# Propriétaire connu : type de données le plus probable
$OwnerTypes = @(
    @('game', 'valve|riot games|epic games|blizzard|activision|electronic arts|ubisoft|netmarble|nexon|krafton|tencent|bandai|square enix|nintendo|sony interactive|take-two|rockstar|psyonix|mihoyo|cognosphere|garena|ncsoft|wargaming|bungie|faceit|embark|i3d\.net|multiplay'),
    @('chat', 'discord|telegram|whatsapp|zoom video|slack'),
    @('ai', 'anthropic|openai'),
    @('remote', 'teamviewer|anydesk|philandro'),
    @('stream', 'netflix|spotify|twitch|deezer'),
    @('ads', 'criteo|taboola|outbrain|pubmatic|rubicon|the trade desk|xandr|appnexus')
)

function ConvertTo-IpHex([string]$Ip) {
    try { -join ([Net.IPAddress]::Parse($Ip).GetAddressBytes() | ForEach-Object { $_.ToString('x2') }) } catch { '' }
}

function Get-ServerCache {
    if ($null -eq $script:ServerCache) {
        $script:ServerCache = New-Object System.Collections.ArrayList
        try {
            if (Test-Path $ServerFile) {
                $arr = Get-Content $ServerFile -Raw -Encoding UTF8 | ConvertFrom-Json
                foreach ($e in $arr) {
                    $age = try { ((Get-Date) - [datetime]::ParseExact([string]$e.D, 'yyyy-MM-dd', $null)).TotalDays } catch { 999 }
                    if ($e.S -and $e.O -and $age -lt 60) { [void]$script:ServerCache.Add(@{ S = [string]$e.S; E = [string]$e.E; O = [string]$e.O; C = [string]$e.C; N = [string]$e.N; D = [string]$e.D }) }
                }
            }
        } catch {}
    }
    , $script:ServerCache
}

function Save-ServerCache {
    $c = Get-ServerCache
    while ($c.Count -gt 3000) { $c.RemoveAt(0) }
    try { [IO.File]::WriteAllText($ServerFile, (ConvertTo-Json -InputObject @($c) -Depth 3 -Compress), (New-Object Text.UTF8Encoding($false))) } catch {}
}

# Propriétaire d'une adresse ($null si inconnu). Les échecs sont mémorisés tant que le cache ne change pas.
function Get-ServerOwner([string]$Ip) {
    $st = $script:Traffic
    if ($st -and $st.Owner.ContainsKey($Ip)) { return $st.Owner[$Ip] }
    $cache = Get-ServerCache
    if ($st -and $st.OwnerMiss[$Ip] -eq $cache.Count) { return $null }
    $h = ConvertTo-IpHex $Ip
    if ($h) {
        foreach ($e in $cache) {
            if ($e.S.Length -eq $h.Length -and [string]::CompareOrdinal($e.S, $h) -le 0 -and [string]::CompareOrdinal($h, $e.E) -le 0) {
                if ($st) { $st.Owner[$Ip] = $e }
                return $e
            }
        }
    }
    if ($st) { $st.OwnerMiss[$Ip] = $cache.Count }
    $null
}

function Format-Country([string]$C) {
    if (-not $C) { return '' }
    if ($C.Length -eq 2) { $n = $CountryNames[$C.ToUpper()]; return $(if ($n) { $n } else { $C.ToUpper() }) }
    if ($C -match '(?i)united states|^usa$') { return 'États-Unis' }
    (Get-Culture).TextInfo.ToTitleCase($C.ToLower())
}

# « Valve Corporation (États-Unis) »
function Get-OwnerLabel($E) {
    $n = if ($E.O) { $E.O } else { $E.N }
    $c = Format-Country $E.C
    "$n$(if ($c) { " ($c)" })"
}

$ServerLookupWork = {
    param($ips)
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $toHex = { param($a) try { -join ([Net.IPAddress]::Parse([string]$a).GetAddressBytes() | ForEach-Object { $_.ToString('x2') }) } catch { '' } }
    foreach ($ip in $ips) {
        $r = @{ Ip = $ip; Ptr = ''; Ok = $false; S = ''; E = ''; O = ''; C = ''; N = '' }
        # Nom officiel de l'adresse (recherche inverse)
        try {
            $ar = [Net.Dns]::BeginGetHostEntry($ip, $null, $null)
            if ($ar.AsyncWaitHandle.WaitOne(2500)) { $hn = [Net.Dns]::EndGetHostEntry($ar).HostName; if ($hn -and $hn -ne $ip) { $r.Ptr = $hn.TrimEnd('.') } }
        } catch {}
        # Propriétaire de l'adresse (annuaire public RDAP)
        try {
            $req = [Net.HttpWebRequest]::Create("https://rdap.org/ip/$ip")
            $req.Timeout = 8000; $req.ReadWriteTimeout = 8000; $req.UserAgent = 'OptiGame'; $req.Accept = 'application/rdap+json, application/json'
            $resp = $req.GetResponse()
            try { $sr = New-Object IO.StreamReader($resp.GetResponseStream(), [Text.Encoding]::UTF8); $o = $sr.ReadToEnd() | ConvertFrom-Json } finally { $resp.Close() }
            $r.N = [string]$o.name
            $r.C = ([string]$o.country).ToUpper()
            $r.S = & $toHex $o.startAddress; $r.E = & $toHex $o.endAddress
            $best = ''; $addr = ''
            foreach ($en in @($o.entities)) {
                if (@($en.roles) -notcontains 'registrant') { continue }
                $fn = ''; $kind = ''; $lab = ''
                foreach ($p in @($en.vcardArray[1])) {
                    if ($p[0] -eq 'fn') { $fn = [string]$p[3] }
                    elseif ($p[0] -eq 'kind') { $kind = [string]$p[3] }
                    elseif ($p[0] -eq 'adr' -and $p[1].label) { $lab = [string]$p[1].label }
                }
                # Identifiants techniques du registre (MN9099-MNT, ORG-VC43-RIPE...) : pas un nom
                if (-not $fn -or $fn -match '-MNT$|^ORG-' -or $fn -cmatch '^[A-Z0-9]+(-[A-Z0-9]+)+$') { continue }
                if ($kind -eq 'org' -or -not $best) { $best = $fn; $addr = $lab }
                if ($kind -eq 'org') { break }
            }
            # Pas d'entreprise déclarée comme titulaire (fréquent chez les fournisseurs) : le nom du service qui gère l'adresse
            if (-not $best) {
                foreach ($en in @($o.entities)) {
                    $fn = ''; $kind = ''
                    foreach ($p in @($en.vcardArray[1])) { if ($p[0] -eq 'fn') { $fn = [string]$p[3] } elseif ($p[0] -eq 'kind') { $kind = [string]$p[3] } }
                    if ($kind -notin 'org', 'group' -or -not $fn -or $fn -cmatch '^[A-Z0-9]+(-[A-Z0-9]+)+$') { continue }
                    $fn = ($fn -replace '(?i)^local internet registry\s+', '' -replace '(?i)\s+(abuse|noc|contact|team|role)\b.*$', '').Trim()
                    if ($fn.Length -ge 3) { $best = $fn; break }
                }
            }
            $r.O = $best
            if (-not $r.C -and $addr) { $r.C = (($addr -split "`n")[-1]).Trim() }
            $r.Ok = $true
        } catch {}
        $r
        Start-Sleep -Milliseconds 400
    }
}

function Update-ServerLookups {
    $st = $script:Traffic
    $job = $st.LookupJob
    if ($job) {
        if (-not $job.Handle.IsCompleted) { return }
        $res = @()
        try { $res = @($job.PS.EndInvoke($job.Handle)) } catch {} finally { $job.PS.Dispose(); $st.LookupJob = $null }
        $changed = $false
        foreach ($r in $res) {
            if ($r.Ptr -and -not $st.Dns[$r.Ip]) { $st.Dns[$r.Ip] = $r.Ptr }
            if ($r.Ok -and ($r.O -or $r.N) -and $r.S -and $r.E) {
                $e = @{ S = $r.S; E = $r.E; O = $r.O; C = $r.C; N = $r.N; D = (Get-Date).ToString('yyyy-MM-dd') }
                [void](Get-ServerCache).Add($e)
                $st.Owner[$r.Ip] = $e
                $changed = $true
            } elseif (-not $r.Ok) {
                # Annuaire injoignable ou trop sollicité : un seul nouvel essai
                $st.LookupTries[$r.Ip] = [int]$st.LookupTries[$r.Ip] + 1
                if ($st.LookupTries[$r.Ip] -lt 2) { $st.LookupDone.Remove($r.Ip) }
            }
        }
        if ($changed) { Save-ServerCache }
    }
    if (-not (Get-Setting 'TrafficLookup' $true)) { return }
    $ips = New-Object System.Collections.ArrayList
    foreach ($a in @($st.Apps.Values)) {
        foreach ($d in @($a.Dest.Values)) {
            $ip = [string]$d.Remote
            if ($d.Private -or $st.Dns[$ip] -or $st.LookupDone.ContainsKey($ip) -or $ips.Contains($ip)) { continue }
            if (Get-ServerOwner $ip) { continue }
            [void]$ips.Add($ip)
            if ($ips.Count -ge 5) { break }
        }
        if ($ips.Count -ge 5) { break }
    }
    if (-not $ips.Count) { return }
    foreach ($ip in $ips) { $st.LookupDone[$ip] = $true }
    $ps = [PowerShell]::Create()
    $ps.RunspacePool = $script:BgPool
    [void]$ps.AddScript($ServerLookupWork.ToString()).AddArgument(@($ips))
    $st.LookupJob = @{ PS = $ps; Handle = $ps.BeginInvoke() }
}

# ---------------------------------------------------------------------------
# Type de données échangées, déduit du serveur contacté (le contenu, chiffré, n'est jamais lu)
# ---------------------------------------------------------------------------
$DataTypes = @(
    @{ Id = 'ads'; Rx = 'doubleclick|googlesyndication|googleadservices|adservice|\bads?\.|adnxs|criteo|taboola|outbrain|scorecardresearch|connect\.facebook|facebook\.net|google-analytics|googletagmanager|analytics|segment\.(io|com)|mixpanel|amplitude|hotjar|appsflyer|adjust\.com|branch\.io|doubleverify|moatads|adsrvr|pubmatic|rubiconproject|quantserve'
       Label = 'Publicité et suivi'; Color = '#F5A524'; Text = 'Ce que tu regardes et où tu cliques, pour afficher de la publicité et mesurer l''audience.' },
    @{ Id = 'telemetry'; Rx = 'telemetry|vortex|watson\.|events\.data|browser\.events|self\.events|settings-win|sentry\.io|bugsnag|crashlytics|datadoghq|newrelic|nr-data|app-measurement|firebaselogging|clientlogging|metrics|diagnostic|\.events\.|beacons?\.|stats\.|feedback'
       Label = 'Statistiques d''utilisation'; Color = '#B18CFF'; Text = 'Rapports sur le fonctionnement du programme : plantages, performances, fonctions utilisées. Pas tes fichiers.' },
    @{ Id = 'security'; Rx = 'smartscreen|wdcp\.|wd\.microsoft|defender|safebrowsing|malwarebytes|avast|avg\.com|kaspersky|bitdefender|eset\.|norton|mcafee'
       Label = 'Protection'; Color = '#22D37A'; Text = 'Vérifie des fichiers ou des adresses de sites auprès de l''éditeur de sécurité.' },
    @{ Id = 'cert'; Rx = 'ocsp|\bcrl|pki\.|digicert|sectigo|letsencrypt|lencr\.org|globalsign|verisign|usertrust|comodoca|entrust|godaddy\.com/repository|ctldl\.windowsupdate'
       Label = 'Vérification de certificats'; Color = '#9AA3B2'; Text = 'Vérifie que les sites et les programmes sont authentiques. Très peu de données.' },
    @{ Id = 'auth'; Rx = '(^|\.)login\.|\bauth|oauth|accounts\.|identity|\bsso\.|signin|msauth|passport'
       Label = 'Connexion à ton compte'; Color = '#4EA8FF'; Text = 'Identifiants chiffrés pour ouvrir ou garder ta session.' },
    @{ Id = 'remote'; Rx = 'anydesk|teamviewer|rustdesk|screenconnect|splashtop|logmein'
       Label = 'Prise en main à distance'; Color = '#F04438'; Text = 'Images de ton écran, clavier et souris quand une session à distance est ouverte.' },
    @{ Id = 'ai'; Rx = 'anthropic|claude\.ai|openai|chatgpt|oaiusercontent|gemini|bard\.google|copilot|perplexity|mistral\.ai'
       Label = 'Assistant IA'; Color = '#FF7AB6'; Text = 'Tes questions et le contexte que tu envoies à l''assistant (textes, fichiers ouverts).' },
    @{ Id = 'sync'; Rx = 'onedrive|sharepoint|dropbox|drive\.google|docs\.google|googleusercontent|icloud|box\.com|\bmega\.(nz|io)|backblaze|pcloud|nextcloud'
       Label = 'Synchronisation de fichiers'; Color = '#4EA8FF'; Text = 'Tes fichiers envoyés vers (ou récupérés depuis) ton espace de stockage en ligne.' },
    @{ Id = 'chat'; Rx = 'discord|whatsapp|telegram|signal\.org|teams|skype|slack|zoom\.us|messenger|trouter|\.gateway\.'
       Label = 'Messagerie et appels'; Color = '#4EA8FF'; Text = 'Tes messages, ta voix ou ta vidéo pendant les appels, et ta présence en ligne.' },
    @{ Id = 'stream'; Rx = 'googlevideo|youtube|ytimg|nflxvideo|netflix|twitch|ttvnw|jtvnw|primevideo|aiv-cdn|disney|dssott|spotify|scdn\.co|deezer|dzcdn|soundcloud|crunchyroll'
       Label = 'Vidéo ou musique'; Color = '#9AA3B2'; Text = 'Tu reçois surtout de la vidéo ou du son. Ce qui part est minime (ce que tu regardes, ta position dans la vidéo).' },
    @{ Id = 'game'; Rx = 'steamcommunity|steampowered|steamserver|valve\.net|riotgames|leagueoflegends|pvp\.net|epicgames|unrealengine|battle\.net|blizzard|ea\.com|origin\.com|ubisoft|ubi\.com|xboxlive|playstation|nintendo|netmarble|playfab|gamesparks|photonengine|faceit|easyanticheat|battleye'
       Label = 'Jeu en ligne'; Color = '#22D37A'; Text = 'Tes actions en jeu, le chat, ton compte et parfois les vérifications de l''anti-triche.' },
    @{ Id = 'update'; Rx = 'windowsupdate|delivery\.mp\.microsoft|\bdl\.|download|update|steamcontent|epicgames-download|akamaized|akamai|cloudfront|fastly|cdn|edgesuite|edgekey|content'
       Label = 'Mise à jour ou téléchargement'; Color = '#9AA3B2'; Text = 'Le programme récupère des fichiers : mises à jour, jeux, images, pages.' },
    @{ Id = 'cloud'; Rx = '1e100\.net|amazonaws|azure|cloudapp|googleapis|gstatic|cloudflare|herokuapp|digitalocean|ovh\.|hetzner|linode|vultr'
       Label = 'Serveur de l''éditeur'; Color = '#9AA3B2'; Text = 'Échanges avec les serveurs du programme (hébergés dans un grand centre de données). Le contenu dépend du programme.' }
)

# Type d'une destination : d'après le nom du serveur, sinon d'après le port et le sens des échanges.
function Get-DestType($Name, [int]$Port, [double]$Out, [double]$In, $App, $Owner) {
    $n = ([string]$Name).ToLower()
    if ($Port -in 53, 853) { return @{ Id = 'dns'; Label = 'Recherche d''adresses'; Color = '#9AA3B2'; Text = 'Traduit les noms de sites en adresses. Très peu de données.' } }
    if ($n) { foreach ($t in $DataTypes) { if ($n -match $t.Rx) { return $t } } }
    # Propriétaire trouvé dans l'annuaire : Valve, Riot, Discord...
    $own = if ($Owner) { "$($Owner.O) $($Owner.N)".ToLower() } else { '' }
    if ($own.Trim()) { foreach ($x in $OwnerTypes) { if ($own -match $x[1]) { return @($DataTypes | Where-Object { $_.Id -eq $x[0] })[0] } } }
    if ($App -and $App.Name -match $RemoteTools) { return @($DataTypes | Where-Object { $_.Id -eq 'remote' })[0] }
    if ($Out -gt 10MB -and $Out -gt 3 * $In) { return @{ Id = 'upload'; Label = 'Envoi important'; Color = '#F5A524'; Text = 'Le programme envoie bien plus qu''il ne reçoit : fichiers, vidéo ou sauvegarde. À vérifier si tu ne sais pas pourquoi.' } }
    if ($In -gt 3 * [math]::Max(1.0, $Out) -and $In -gt 1MB) { return @{ Id = 'download'; Label = 'Téléchargement'; Color = '#9AA3B2'; Text = 'Le programme reçoit surtout des données (fichiers, contenus).' } }
    if (-not $n -and $own.Trim()) { return @{ Id = 'owned'; Label = 'Serveur d''une entreprise connue'; Color = '#9AA3B2'; Text = 'On sait à qui appartient le serveur (indiqué sous son adresse), mais pas précisément ce qui est échangé.' } }
    if (-not $n) { return @{ Id = 'unknown'; Label = 'Serveur non identifié'; Color = '#5B6475'; Text = 'Adresse sans nom ni propriétaire connu : impossible de savoir à quoi elle sert.' } }
    @{ Id = 'other'; Label = 'Échanges avec le serveur'; Color = '#5B6475'; Text = 'Le nom du serveur ne dit pas précisément ce qui est échangé.' }
}

# Échanges groupés par type pour un programme : { Type, Out, In, Count, Dests }.
# Le réseau local passe en dernier, sinon le plus gros volume d'abord.
$LocalType = @{ Id = 'local'; Label = 'Appareils de ton réseau'; Color = '#9AA3B2'; Text = 'Échanges avec des appareils de chez toi (box, imprimante, TV...) : rien ne sort sur Internet.' }
function Get-AppDestGroups($St, $A) {
    $sum = @{}
    foreach ($d in @($A.Dest.Values)) {
        $t = if ($d.Private) { $LocalType } else { Get-DestType (Get-DestName $St $d) $d.Port $d.Out $d.In $A (Get-ServerOwner $d.Remote) }
        if (-not $sum.ContainsKey($t.Id)) { $sum[$t.Id] = @{ Type = $t; Out = [double]0; In = [double]0; Count = 0; Dests = (New-Object System.Collections.ArrayList) } }
        $sum[$t.Id].Out += $d.Out; $sum[$t.Id].In += $d.In; $sum[$t.Id].Count++
        [void]$sum[$t.Id].Dests.Add($d)
    }
    @($sum.Values | Sort-Object @{ Expression = { $_.Type.Id -ne 'local' } }, @{ Expression = { $_.Out + $_.In } }, @{ Expression = { $_.Count } } -Descending)
}

# Résumé par type vers Internet (sans le réseau local), du plus gros envoi au plus petit.
function Get-AppDataTypes($St, $A) {
    @(Get-AppDestGroups $St $A | Where-Object { $_.Type.Id -ne 'local' } | Sort-Object @{ Expression = { $_.Out } } -Descending)
}

# Bloc d'un type de données dans la fiche : titre, explication, volumes, puis ses serveurs.
function New-TrafficTypeBlock($St, $G) {
    $b = New-Object System.Windows.Controls.Border
    $b.Background = Get-Brush '#1E232D'
    $b.BorderBrush = Get-Brush $G.Type.Color
    $b.BorderThickness = New-Thickness 3 0 0 0
    $b.CornerRadius = [System.Windows.CornerRadius]::new(8)
    $b.Padding = New-Thickness 16 12 16 10
    $b.Margin = New-Thickness 0 0 0 8
    $sp = New-Object System.Windows.Controls.StackPanel
    $hd = New-Grid @('*', 'Auto')
    $tl = New-Object System.Windows.Controls.StackPanel
    [void]$tl.Children.Add((New-Text $G.Type.Label 14.5 $G.Type.Color -Semi))
    $desc = New-Text $G.Type.Text 12 '#9AA3B2'
    $desc.Margin = New-Thickness 0 3 0 0
    [void]$tl.Children.Add($desc)
    Add-ToGrid $hd $tl 0
    $vol = New-Object System.Windows.Controls.StackPanel
    $vol.Margin = New-Thickness 20 0 0 0
    $up = New-Text "↑ $(Format-Bytes $G.Out)" 14 '#FFFFFF' -Semi
    $up.HorizontalAlignment = 'Right'; $up.ToolTip = 'Envoyé'
    $dn = New-Text "↓ $(Format-Bytes $G.In)" 12 '#9AA3B2'
    $dn.HorizontalAlignment = 'Right'; $dn.ToolTip = 'Reçu'
    [void]$vol.Children.Add($up); [void]$vol.Children.Add($dn)
    Add-ToGrid $hd $vol 1
    [void]$sp.Children.Add($hd)
    $sep = New-Object System.Windows.Controls.Border
    $sep.Height = 1; $sep.Background = Get-Brush '#2A303B'; $sep.Margin = New-Thickness 0 10 0 4
    [void]$sp.Children.Add($sep)
    $dests = @($G.Dests | Sort-Object @{ Expression = { $_.Out + $_.In } } -Descending)
    foreach ($d in @($dests | Select-Object -First 6)) {
        $row = New-Grid @('18', '*', 'Auto')
        $row.Margin = New-Thickness 0 4 0 4
        $dot = New-Text '●' 9 $(if ($d.Live) { $Colors.ok } else { '#5B6475' })
        $dot.VerticalAlignment = 'Center'
        $dot.ToolTip = $(if ($d.Live) { 'Connexion ouverte' } else { 'Connexion terminée' })
        Add-ToGrid $row $dot 0
        $nm = $St.Dns[[string]$d.Remote]
        $left = New-Object System.Windows.Controls.StackPanel
        $t = New-Text $(if ($nm) { $nm } else { $d.Remote }) 12.5 '#E6E8EE'
        $t.TextTrimming = 'CharacterEllipsis'; $t.TextWrapping = 'NoWrap'
        [void]$left.Children.Add($t)
        $ow = if (-not $d.Private) { Get-ServerOwner $d.Remote }
        if ($ow) {
            $ot = New-Text "Appartient à $(Get-OwnerLabel $ow)" 11.5 '#C9CED8'
            $ot.TextTrimming = 'CharacterEllipsis'; $ot.TextWrapping = 'NoWrap'
            [void]$left.Children.Add($ot)
        }
        $ms = Get-MsService $nm
        if ($ms) {
            $mt = New-Text "Microsoft, $($ms.Label) : $($ms.Text)" 11.5 '#C9CED8'
            $mt.TextTrimming = 'CharacterEllipsis'; $mt.TextWrapping = 'NoWrap'; $mt.ToolTip = $mt.Text
            [void]$left.Children.Add($mt)
        }
        $pn = $PortNames[[int]$d.Port]
        $warnPort = $SusPorts.ContainsKey([int]$d.Port)
        $info = New-Text "$(if ($nm) { $d.Remote + ', ' })port $($d.Port)$(if ($pn) { ' (' + $pn + ')' } elseif ($warnPort) { ' (' + $SusPorts[[int]$d.Port] + ')' })" 11 $(if ($warnPort) { $Colors.warn } else { '#5B6475' })
        $info.TextTrimming = 'CharacterEllipsis'; $info.TextWrapping = 'NoWrap'
        [void]$left.Children.Add($info)
        Add-ToGrid $row $left 1
        $v = New-Text "↑ $(Format-Bytes $d.Out)    ↓ $(Format-Bytes $d.In)" 11.5 '#9AA3B2'
        $v.VerticalAlignment = 'Center'; $v.Margin = New-Thickness 16 0 0 0
        Add-ToGrid $row $v 2
        [void]$sp.Children.Add($row)
    }
    if ($dests.Count -gt 6) {
        $more = New-Text "+ $($dests.Count - 6) autre$(if ($dests.Count -gt 7) {'s'}) serveur$(if ($dests.Count -gt 7) {'s'}) du même type" 11.5 '#5B6475'
        $more.Margin = New-Thickness 18 2 0 2
        [void]$sp.Children.Add($more)
    }
    $b.Child = $sp
    $b
}

# ---------------------------------------------------------------------------
# Affichage
# ---------------------------------------------------------------------------
function Get-AppSigLabel($A) {
    switch ($A.Sig) {
        'Valid' { @($(if ($A.Publisher) { "Signé : $($A.Publisher)" } else { 'Signé' }), $Colors.ok) }
        'HashMismatch' { @('Signature invalide', $Colors.bad) }
        $null { @('Vérification...', '#9AA3B2') }
        'Unknown' { @('Signature non vérifiable', '#9AA3B2') }
        'NotTrusted' { @('Certificat non reconnu', $Colors.warn) }
        default { @($(if ($A.Path) { 'Non signé' } else { 'Programme système' }), $(if ($A.Path) { $Colors.warn } else { '#9AA3B2' })) }
    }
}

function Get-DestName($St, $D) {
    $n = $St.Dns[[string]$D.Remote]
    if ($n) { return $n }
    if ($D.Private) { return 'ton réseau local' }
    ''
}

function Build-TrafficPage {
    $p = $ui.TrafficPanel
    $p.Children.Clear()
    $stats = New-Object System.Windows.Controls.Primitives.UniformGrid
    $stats.Columns = 4
    $script:TrafficStats = @{}
    foreach ($s in @(@('Out', 'Envoyé'), @('In', 'Reçu'), @('Apps', 'Programmes connectés'), @('Alerts', 'À vérifier'))) {
        $c = New-Card
        $c.Margin = New-Thickness 0 0 10 0
        $c.Padding = New-Thickness 16 12 16 12
        $sp = New-Object System.Windows.Controls.StackPanel
        $v = New-Text '...' 22 '#FFFFFF' -Bold
        [void]$sp.Children.Add($v)
        [void]$sp.Children.Add((New-Text $s[1] 12 '#9AA3B2'))
        $c.Child = $sp
        [void]$stats.Children.Add($c)
        $script:TrafficStats[$s[0]] = $v
    }
    [void]$p.Children.Add($stats)
    $script:TrafficCounters = New-Text '' 12 $Colors.warn -Semi
    $script:TrafficCounters.Margin = New-Thickness 0 8 0 0
    $script:TrafficCounters.Visibility = 'Collapsed'
    [void]$p.Children.Add($script:TrafficCounters)
    $lk = New-Grid @('*', 'Auto')
    $lk.Margin = New-Thickness 0 12 0 0
    $lkt = New-Object System.Windows.Controls.StackPanel
    [void]$lkt.Children.Add((New-Text 'Identifier les serveurs sans nom' 13 '#FFFFFF' -Semi))
    [void]$lkt.Children.Add((New-Text 'Cherche à qui appartient chaque adresse inconnue (Valve, Riot, Amazon...) dans l''annuaire public des adresses Internet (rdap.org). Seule l''adresse du serveur est envoyée.' 11.5 '#9AA3B2'))
    Add-ToGrid $lk $lkt 0
    $sw = New-Object System.Windows.Controls.CheckBox
    $sw.Style = $Window.FindResource('Switch')
    $sw.IsChecked = [bool](Get-Setting 'TrafficLookup' $true)
    $sw.VerticalAlignment = 'Center'; $sw.Margin = New-Thickness 16 0 0 0
    $sw.Add_Click({ param($sender, $e) Set-Setting 'TrafficLookup' ([bool]$sender.IsChecked) })
    Add-ToGrid $lk $sw 1
    [void]$p.Children.Add($lk)
    $mc = New-Card
    $mc.Margin = New-Thickness 0 12 0 0
    $mc.Padding = New-Thickness 16 12 16 12
    $mg = New-Grid @('*', 'Auto')
    $mt = New-Object System.Windows.Controls.StackPanel
    [void]$mt.Children.Add((New-Text 'Ce que Windows envoie à Microsoft' 14 '#FFFFFF' -Semi))
    [void]$mt.Children.Add((New-Text 'Les identifiants de ton PC (appareil, pub, compte), les réglages qui en envoient plus que le minimum, et les envois vus en direct.' 12 '#9AA3B2'))
    Add-ToGrid $mg $mt 0
    $mb = New-Button 'Voir'
    $mb.VerticalAlignment = 'Center'; $mb.Margin = New-Thickness 16 0 0 0
    $mb.Add_Click({ Invoke-Safe { Show-WindowsPrivacy } })
    Add-ToGrid $mg $mb 1
    $mc.Child = $mg
    [void]$p.Children.Add($mc)
    $script:TrafficAlertBox = New-Object System.Windows.Controls.StackPanel
    $script:TrafficAlertBox.Margin = New-Thickness 0 14 0 0
    [void]$p.Children.Add($script:TrafficAlertBox)
    $h = New-Object System.Windows.Controls.DockPanel
    $h.Margin = New-Thickness 0 18 0 8
    $script:TrafficSince = New-Text '' 12 '#9AA3B2'
    $script:TrafficSince.VerticalAlignment = 'Bottom'
    [System.Windows.Controls.DockPanel]::SetDock($script:TrafficSince, 'Right')
    [void]$h.Children.Add($script:TrafficSince)
    [void]$h.Children.Add((New-Text 'Programmes qui communiquent avec Internet' 16 '#FFFFFF' -Semi))
    [void]$p.Children.Add($h)
    $script:TrafficList = New-Object System.Windows.Controls.StackPanel
    [void]$p.Children.Add($script:TrafficList)
    $n = New-Text 'Le contenu des échanges est chiffré (HTTPS) : Nexo voit quel programme parle à qui et combien il envoie, jamais ce qu''il y a dedans. Les volumes comptent depuis le début de la surveillance. Pour les échanges UDP (certains jeux, appels vidéo), Windows ne donne pas les destinations.' 11.5 '#5B6475'
    $n.Margin = New-Thickness 0 12 0 0
    [void]$p.Children.Add($n)
    $script:TrafficAlertKeys = $null
    $script:TrafficBuilt = $true
}

function New-TrafficRow($A) {
    $card = New-Card
    $card.Padding = New-Thickness 14 10 14 10
    $card.Margin = New-Thickness 0 0 0 6
    $card.Cursor = [System.Windows.Input.Cursors]::Hand
    $g = New-Grid @('Auto', '*', 'Auto', 'Auto')
    $img = New-Object System.Windows.Controls.Image
    $img.Width = 28; $img.Height = 28; $img.Margin = New-Thickness 0 0 12 0; $img.VerticalAlignment = 'Center'
    if ($A.Path -and -not $A.Icon) { $A.Icon = Get-ExeIcon $A.Path }
    if ($A.Icon) { $img.Source = $A.Icon }
    Add-ToGrid $g $img 0
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.VerticalAlignment = 'Center'
    $t = New-Text $A.Title 14 '#FFFFFF' -Semi
    $t.TextTrimming = 'CharacterEllipsis'; $t.TextWrapping = 'NoWrap'
    [void]$sp.Children.Add($t)
    $sig = Get-AppSigLabel $A
    if ((Get-TrafficMarks 'TrafficTrusted').ContainsKey($A.Key)) { $sig = @("Approuvé par toi   $($sig[0])", '#9AA3B2') }
    $dests = @($A.Dest.Values | Where-Object { -not $_.Private }).Count
    $sub = New-Object System.Windows.Controls.TextBlock
    $sub.FontSize = 11.5; $sub.TextTrimming = 'CharacterEllipsis'
    $r1 = New-Object System.Windows.Documents.Run $sig[0]; $r1.Foreground = Get-Brush $sig[1]
    $r2 = New-Object System.Windows.Documents.Run "   $dests destination$(if ($dests -gt 1) {'s'})$(if ($A.Udp) { ', UDP' })$(if ($A.Live) { "   $($A.Live) connexion$(if ($A.Live -gt 1) {'s'}) ouverte$(if ($A.Live -gt 1) {'s'})" })"
    $r2.Foreground = Get-Brush '#9AA3B2'
    [void]$sub.Inlines.Add($r1); [void]$sub.Inlines.Add($r2)
    # Type principal : le plus gros volume, un type identifié passe avant « non identifié »
    $main = @(Get-AppDataTypes $script:Traffic $A | Sort-Object @{ Expression = { $_.Type.Id -notin 'unknown', 'other' } }, @{ Expression = { $_.Out + $_.In } }, @{ Expression = { $_.Count } } -Descending)[0]
    if ($main) {
        $r3 = New-Object System.Windows.Documents.Run "   $($main.Type.Label)"
        $r3.Foreground = Get-Brush $main.Type.Color
        [void]$sub.Inlines.Add($r3)
    }
    [void]$sp.Children.Add($sub)
    Add-ToGrid $g $sp 1
    $vol = New-Object System.Windows.Controls.StackPanel
    $vol.HorizontalAlignment = 'Right'; $vol.VerticalAlignment = 'Center'; $vol.Margin = New-Thickness 12 0 12 0
    $up = New-Text "↑ $(Format-Bytes $A.Out)" 13.5 $(if ($A.Rate -gt 50KB) { $Colors.warn } else { '#FFFFFF' }) -Semi
    $up.HorizontalAlignment = 'Right'
    [void]$vol.Children.Add($up)
    $down = New-Text "↓ $(Format-Bytes $A.In)$(if ($A.Rate -gt 1KB) { "   ↑ $(Format-Bytes $A.Rate)/s" })" 11.5 '#9AA3B2'
    $down.HorizontalAlignment = 'Right'
    [void]$vol.Children.Add($down)
    Add-ToGrid $g $vol 2
    $chev = New-Text '›' 22 '#5B6475'
    $chev.VerticalAlignment = 'Center'
    Add-ToGrid $g $chev 3
    $card.Child = $g
    $card.Tag = $A.Key
    $card.Add_MouseEnter({ param($s, $e) $s.Background = Get-Brush 'card-hover' })
    $card.Add_MouseLeave({ param($s, $e) $s.Background = Get-Brush 'card' })
    $card.Add_MouseLeftButtonUp({ param($s, $e) Invoke-Safe { Show-TrafficApp $s.Tag } })
    $card
}

function Get-TrafficAlertActions($A) {
    $acts = @()
    $acts += @{ Label = 'C''est normal, je lui fais confiance'; NoRefresh = $true; Arg = $A; Script = { param($x) Hide-TestPanel; Set-TrafficTrust $x $true } }
    if ($A.Path) {
        $acts += @{ Label = 'Analyser avec l''antivirus'; NoRefresh = $true; Arg = $A; Script = { param($x) Invoke-TrafficScan $x } }
        $acts += @{ Label = 'Bloquer l''accès à Internet'; NoRefresh = $true; Arg = $A; Script = { param($x) Block-TrafficApp $x } }
        $acts += @{ Label = 'Ouvrir l''emplacement'; NoRefresh = $true; Arg = $A.Path; Script = { param($x) Start-Process 'explorer.exe' -ArgumentList "/select,`"$x`"" } }
    }
    $acts
}

function Update-TrafficView {
    $st = $script:Traffic
    if (-not $st -or -not $script:TrafficBuilt) { return }
    $apps = @($st.Apps.Values | Where-Object { @($_.Dest.Values | Where-Object { -not $_.Private }).Count -or $_.Out -gt 0 })
    $sumOut = 0.0; $sumIn = 0.0
    foreach ($a in $apps) { $sumOut += $a.Out; $sumIn += $a.In }
    $script:TrafficStats.Out.Text = Format-Bytes $sumOut
    $script:TrafficStats.In.Text = Format-Bytes $sumIn
    $script:TrafficStats.Apps.Text = "$($apps.Count)"
    # « Bon à savoir » (ex : PowerShell lancé par Claude Code) : visible dans la fiche du programme, pas dans la liste à vérifier
    $alerts = @($st.Alerts | Where-Object { $_.Level -ne 'info' })
    $serious = $alerts.Count
    $script:TrafficStats.Alerts.Text = "$serious"
    $script:TrafficStats.Alerts.Foreground = Get-Brush $(if ($serious) { $Colors.warn } else { $Colors.ok })
    $mins = [int]((Get-Date) - $st.Started).TotalMinutes
    $script:TrafficSince.Text = "depuis $(if ($mins -lt 1) { 'moins d''une minute' } else { "$mins min" })"
    if (-not [TrafficMon]::CountersOk -and $st.Ticks -gt 2 -and @($st.Conns.Values).Count) {
        $script:TrafficCounters.Text = 'Windows refuse de compter les données (Nexo doit être lancé en administrateur) : seules les connexions sont affichées.'
        $script:TrafficCounters.Visibility = 'Visible'
    } else { $script:TrafficCounters.Visibility = 'Collapsed' }
    # Alertes : reconstruites seulement si elles changent (sinon les boutons clignoteraient)
    $keys = ($alerts | ForEach-Object { "$($_.App.Key)|$($_.Level)|$($_.Why.Count)" }) -join ';'
    if ($keys -ne $script:TrafficAlertKeys) {
        $script:TrafficAlertKeys = $keys
        $box = $script:TrafficAlertBox
        $box.Children.Clear()
        if (-not $alerts.Count) {
            [void]$box.Children.Add((New-Verdict 'ok' 'Rien d''anormal pour l''instant : les programmes connectés sont signés par leur éditeur et se comportent normalement.'))
        } else {
            [void]$box.Children.Add((New-Text 'À VÉRIFIER' 12 '#5B6475' -Semi))
            foreach ($al in $alerts) {
                $c = New-SecurityCard @{ Status = $al.Level; Title = "$($al.App.Title) : $(if ($al.Level -eq 'bad') { 'comportement suspect' } elseif ($al.Level -eq 'warn') { 'à vérifier' } else { 'bon à savoir' })"
                    Detail = "Pourquoi : $($al.Why -join ' ; ')."; Items = @($al.App.Path | Where-Object { $_ }); Actions = (Get-TrafficAlertActions $al.App); ShowAll = $true }
                $c.Margin = New-Thickness 0 6 0 0
                [void]$box.Children.Add($c)
            }
        }
    }
    $list = $script:TrafficList
    $list.Children.Clear()
    $sorted = @($apps | Sort-Object @{ Expression = { $_.Out + $_.In } } -Descending | Select-Object -First 40)
    if (-not $sorted.Count) { [void]$list.Children.Add((New-Text 'Aucun programme ne communique avec Internet pour le moment.' 13 '#5B6475')) }
    foreach ($a in $sorted) { [void]$list.Children.Add((New-TrafficRow $a)) }
}

# Fiche d'un programme : qui il contacte, combien, et pourquoi il est signalé.
function Show-TrafficApp([string]$Key) {
    if ($script:TestRunning) { return }
    $st = $script:Traffic
    $a = $st.Apps[$Key]
    if (-not $a) { return }
    Show-TestPanel @{ Tag = 'NET'; Title = $a.Title; Sub = $(if ($a.Path) { $a.Path } else { $a.Name }) }
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    $al = @($st.Alerts | Where-Object { $_.App.Key -eq $Key })[0]
    Set-TestState $(if ($al) { $al.Level } else { 'ok' }) $(if ($al -and $al.Level -eq 'bad') { 'Suspect' } elseif ($al -and $al.Level -eq 'warn') { 'À vérifier' } else { 'Normal' })
    $body = $ui.TestBody
    $sig = Get-AppSigLabel $a
    [void]$body.Children.Add((New-SectionTitle 'LE PROGRAMME'))
    $svcList = @($a.Services | Where-Object { $_ })
    $rows = @(
        @('Nom', "$($a.Title) ($($a.Name))"),
        @('Éditeur', $sig[0], $sig[1]),
        @('Emplacement', $(if ($a.Path) { $a.Path } else { 'Programme de Windows' })),
        @('Envoyé', (Format-Bytes $a.Out)),
        @('Reçu', (Format-Bytes $a.In)),
        @('Utilise aussi l''UDP', $(if ($a.Udp) { 'Oui (jeux, appels, vidéo : destinations non visibles)' } else { 'Non' }))
    )
    if ($svcList.Count) { $rows += , @('Services Windows', ((@($svcList) | ForEach-Object { "$($_.Title) ($($_.Name))" }) -join ', ')) }
    [void]$body.Children.Add((New-InfoRows $rows))
    $trustDate = (Get-TrafficMarks 'TrafficTrusted')[$Key]
    if ($null -ne $trustDate) {
        $tr = New-Grid @('*', 'Auto')
        $tr.Margin = New-Thickness 0 10 0 0
        Add-ToGrid $tr (New-Text "Tu as approuvé ce programme$(if ($trustDate) { " le $trustDate" }) : il n'est plus signalé." 12.5 '#9AA3B2') 0
        $ub = New-Button 'Ne plus lui faire confiance'
        $ub.Tag = $a
        $ub.Add_Click({ param($s, $e) $x = $s.Tag; Invoke-Safe { Hide-TestPanel; Set-TrafficTrust $x $false } })
        Add-ToGrid $tr $ub 1
        [void]$body.Children.Add($tr)
    }
    if ($al) {
        [void]$body.Children.Add((New-SectionTitle 'POURQUOI IL EST SIGNALÉ'))
        $c = New-SecurityCard @{ Status = $al.Level; Title = $(if ($al.Level -eq 'bad') { 'Comportement suspect' } else { 'À vérifier' }); Detail = ($al.Why -join ' ; ') + '.'; Actions = (Get-TrafficAlertActions $a) }
        [void]$body.Children.Add($c)
    } elseif ($a.Path) {
        $row = New-Object System.Windows.Controls.WrapPanel
        $row.Margin = New-Thickness 0 10 0 0
        foreach ($act in @(Get-TrafficAlertActions $a | Select-Object -Skip 1)) {
            $b = New-Button $act.Label
            $b.Margin = New-Thickness 0 0 8 0
            $b.Tag = $act
            $b.Add_Click({ param($s, $e) $x = $s.Tag; Invoke-Safe { & $x.Script $x.Arg } })
            [void]$row.Children.Add($b)
        }
        [void]$body.Children.Add($row)
    }
    $groups = @(Get-AppDestGroups $st $a)
    # En résumé (en premier) : ce qui se mesure (volumes) et ce qui a été repéré ou non (serveurs)
    $ids = @($groups | Where-Object { $_.Type.Id -ne 'local' } | ForEach-Object { $_.Type.Id })
    if ($ids.Count) { Add-TrafficSummary $body $a $ids }
    # Ce qu'il envoie : un bloc par type de données, avec ses serveurs dedans
    [void]$body.Children.Add((New-SectionTitle 'CE QU''IL ENVOIE (PROBABLEMENT)'))
    $cav = New-Text "Deviné d'après le nom des serveurs et les volumes : le contenu, chiffré, n'est jamais lu.$(if ($a.Udp) { ' Les échanges UDP (jeu, voix) ne sont pas comptés.' })" 11.5 '#5B6475'
    $cav.Margin = New-Thickness 0 0 0 10
    [void]$body.Children.Add($cav)
    if (-not $groups.Count) { [void]$body.Children.Add((New-Text 'Aucune connexion vue pour l''instant.' 13 '#5B6475')) }
    foreach ($g in $groups) { [void]$body.Children.Add((New-TrafficTypeBlock $st $g)) }
}

function Add-TrafficSummary($body, $a, $ids) {
    $lines = @()
    if ($a.Out -lt 5MB) { $lines += , @('ok', "Aucun gros envoi : $(Format-Bytes $a.Out) en tout, pas de fichiers ni de vidéo envoyés en masse") }
    elseif ($a.Out -gt 3 * $a.In -and $a.Out -gt 50MB) { $lines += , @('warn', "Gros envoi : $(Format-Bytes $a.Out) envoyés contre $(Format-Bytes $a.In) reçus (fichiers, vidéo ou sauvegarde)") }
    elseif ($a.Out -gt 3 * $a.In) { $lines += , @('warn', "Envoie bien plus qu'il ne reçoit : $(Format-Bytes $a.Out) contre $(Format-Bytes $a.In)") }
    else { $lines += , @('info', "Envoi moyen ($(Format-Bytes $a.Out)) : normal pour discuter, jouer ou naviguer") }
    # Aucun serveur reconnu : on ne peut rien affirmer sur le reste
    if (-not @($ids | Where-Object { $_ -notin 'unknown', 'other', 'upload', 'download', 'owned' }).Count) {
        $lines += , @('warn', 'Les serveurs ne disent pas à quoi ils servent : impossible de deviner quel type de données part')
    } else {
        $lines += , $(if ('ads' -in $ids) { @('warn', 'Publicité et suivi : ce que tu fais est mesuré') } else { @('ok', 'Aucune publicité ni pistage repéré') })
        $lines += , $(if ('telemetry' -in $ids) { @('info', 'Envoie des statistiques d''utilisation (pas tes fichiers)') } else { @('ok', 'Aucune statistique d''utilisation repérée') })
        $lines += , $(if ('remote' -in $ids) { @('warn', 'Peut transmettre l''image de ton écran (prise en main à distance)') } else { @('ok', 'Aucune prise en main à distance') })
    }
    [void]$body.Children.Add((New-SectionTitle 'EN RÉSUMÉ'))
    $box = New-Object System.Windows.Controls.Border
    $box.Background = Get-Brush '#1E232D'
    $box.CornerRadius = [System.Windows.CornerRadius]::new(8)
    $box.Padding = New-Thickness 14 8 14 8
    $box.Margin = New-Thickness 0 6 0 0
    $sp = New-Object System.Windows.Controls.StackPanel
    foreach ($l in $lines) {
        $row = New-Grid @('26', '*')
        $row.Margin = New-Thickness 0 4 0 4
        $ic = New-Text $(switch ($l[0]) { 'ok' { '✓' } 'warn' { '!' } default { 'i' } }) 13.5 $Colors[$l[0]] -Bold
        Add-ToGrid $row $ic 0
        Add-ToGrid $row (New-Text $l[1] 13 $(if ($l[0] -eq 'warn') { $Colors.warn } else { '#E6E8EE' })) 1
        [void]$sp.Children.Add($row)
    }
    $box.Child = $sp
    [void]$body.Children.Add($box)
}
