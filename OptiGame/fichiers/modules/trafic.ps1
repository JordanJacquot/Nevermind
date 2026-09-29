# OptiGame : ce qui sort du PC. Quels programmes communiquent avec Internet, avec qui, combien,
# et ce qui est anormal. Le contenu (chiffré en HTTPS) n'est jamais lu.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

$TrafficIndex = 10
$LolBins = '^(powershell|pwsh|cmd|wscript|cscript|mshta|rundll32|regsvr32|certutil|bitsadmin|msbuild|installutil|regasm|regsvcs|cmstp|wmic|forfiles|msiexec|hh)$'
$RemoteTools = '(?i)^(anydesk|teamviewer\w*|rustdesk|screenconnect\.\w+|connectwise\w*|logmein\w*|splashtop\w*|ultraviewer\w*|supremo\w*|rutserv|rfusclient|aeroadmin|ammyy\w*|remotepc\w*|zohoassist\w*|getscreen\w*)$'
$SusPorts = @{ 4444 = 'port souvent utilisé par les logiciels espions'; 1337 = 'port souvent utilisé par les logiciels espions'; 31337 = 'port souvent utilisé par les logiciels espions'; 6666 = 'discussion IRC (utilisée par des virus)'; 6667 = 'discussion IRC (utilisée par des virus)'; 6697 = 'discussion IRC (utilisée par des virus)'; 8333 = 'réseau Bitcoin'; 3333 = 'minage de cryptomonnaie'; 5555 = 'port souvent utilisé par les logiciels espions'; 9001 = 'réseau Tor'; 9030 = 'réseau Tor'; 9050 = 'réseau Tor'; 9150 = 'réseau Tor'; 12345 = 'port souvent utilisé par les logiciels espions'; 23 = 'Telnet (non chiffré)' }
$UploadOk = '(?i)^(onedrive|dropbox|googledrivefs|box|megasync|icloud\w*|obs\w*|streamlabs\w*|discord|steam\w*|teams|ms-teams|zoom|skype|chrome|msedge|firefox|opera|brave|vivaldi|qbittorrent|utorrent|bittorrent|transmission\w*|backblaze\w*|synology\w*|nvcontainer|nvidia share|shadowplay|medal\w*|outplayed|epicgameslauncher)$'
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
       Started = Get-Date; Ticks = 0; Notified = @{}; AlertKeys = '' }
}

# Programme d'un processus (plusieurs processus du même programme, comme Chrome, sont regroupés).
function Get-TrafficApp([int]$ProcId) {
    $st = $script:Traffic
    if ($st.Pids.ContainsKey($ProcId)) { return $st.Pids[$ProcId] }
    $p = Get-Process -Id $ProcId -ErrorAction SilentlyContinue
    $name = if ($ProcId -eq 4) { 'System' } elseif ($p) { $p.ProcessName } else { "Programme $ProcId" }
    $path = if ($p) { try { [string]$p.Path } catch { '' } } else { '' }
    if (-not $path -and $ProcId -gt 4) { try { $path = [TrafficMon]::GetProcessPath($ProcId) } catch {} }
    $key = if ($ProcId -eq $PID) { 'optigame' } elseif ($path) { $path.ToLower() } else { "nom:$($name.ToLower())" }
    if (-not $st.Apps.ContainsKey($key)) {
        $desc = ''
        if ($path) { try { $desc = [string][Diagnostics.FileVersionInfo]::GetVersionInfo($path).FileDescription } catch {} }
        $st.Apps[$key] = @{ Key = $key; Name = $name; Path = $path; Title = $(if ($ProcId -eq $PID) { 'OptiGame (cette app)' } elseif ($desc -and $desc.Length -lt 60) { $desc } else { $name })
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
    $st.DnsAt = Get-Date
    try {
        foreach ($e in @(Get-DnsClientCache -ErrorAction Stop | Where-Object { $_.Type -in 1, 28 -and $_.Data })) {
            $st.Dns[[string]$e.Data] = ([string]$e.Entry).TrimEnd('.')
        }
    } catch {}
}

function Update-Traffic {
    $st = $script:Traffic
    if (-not $st) { return }
    $st.Ticks++
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
        try {
            $sg = Get-AuthenticodeSignature -FilePath $path -ErrorAction Stop
            @([string]$sg.Status, $(if ($sg.SignerCertificate) { $sg.SignerCertificate.Subject -replace '^.*?CN="?([^",]+).*$', '$1' } else { '' }))
        } catch { @('Error', '') }
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
    $ps.RunspacePool = $script:Pool
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
    New-NetFirewallRule -DisplayName $name -Direction Outbound -Program $App.Path -Action Block -Profile Any -ErrorAction Stop | Out-Null
    New-NetFirewallRule -DisplayName $name -Direction Inbound -Program $App.Path -Action Block -Profile Any -ErrorAction Stop | Out-Null
    [void](Add-History "Internet bloqué pour $($App.Title)" @($App.Path) @(@{ Type = 'fw'; Name = $name }))
    Get-Process -ErrorAction SilentlyContinue | Where-Object { try { $_.Path -eq $App.Path } catch { $false } } | ForEach-Object { try { $_.Kill() } catch {} }
    Set-Status "$($App.Title) ne peut plus accéder à Internet."
    Show-Message "« $($App.Title) » est bloqué et a été fermé.`n`nPour annuler : page Sauvegarde, historique, « Annuler »."
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
    $n = New-Text 'Le contenu des échanges est chiffré (HTTPS) : OptiGame voit quel programme parle à qui et combien il envoie, jamais ce qu''il y a dedans. Les volumes comptent depuis le début de la surveillance. Pour les échanges UDP (certains jeux, appels vidéo), Windows ne donne pas les destinations.' 11.5 '#5B6475'
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
    $card.Add_MouseEnter({ param($s, $e) $s.Background = Get-Brush '#1C212B' })
    $card.Add_MouseLeave({ param($s, $e) $s.Background = Get-Brush '#181C24' })
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
        $script:TrafficCounters.Text = 'Windows refuse de compter les données (OptiGame doit être lancé en administrateur) : seules les connexions sont affichées.'
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
    [void]$body.Children.Add((New-InfoRows @(
        @('Nom', "$($a.Title) ($($a.Name))"),
        @('Éditeur', $sig[0], $sig[1]),
        @('Emplacement', $(if ($a.Path) { $a.Path } else { 'Programme de Windows' })),
        @('Envoyé', (Format-Bytes $a.Out)),
        @('Reçu', (Format-Bytes $a.In)),
        @('Utilise aussi l''UDP', $(if ($a.Udp) { 'Oui (jeux, appels, vidéo : destinations non visibles)' } else { 'Non' }))
    )))
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
    [void]$body.Children.Add((New-SectionTitle 'AVEC QUI IL COMMUNIQUE'))
    $dests = @($a.Dest.Values | Sort-Object @{ Expression = { $_.Out + $_.In } } -Descending | Select-Object -First 40)
    if (-not $dests.Count) { [void]$body.Children.Add((New-Text 'Aucune connexion vue pour l''instant.' 13 '#5B6475')) }
    foreach ($d in $dests) {
        $row = New-Grid @('*', 'Auto')
        $row.Margin = New-Thickness 0 3 0 3
        $left = New-Object System.Windows.Controls.StackPanel
        $nm = Get-DestName $st $d
        $t = New-Text $(if ($nm) { $nm } else { $d.Remote }) 13 $(if ($d.Live) { '#FFFFFF' } else { '#9AA3B2' }) -Semi
        $t.TextTrimming = 'CharacterEllipsis'; $t.TextWrapping = 'NoWrap'
        [void]$left.Children.Add($t)
        $pn = $PortNames[[int]$d.Port]
        $warnPort = $SusPorts.ContainsKey([int]$d.Port)
        [void]$left.Children.Add((New-Text "$(if ($nm) { $d.Remote + ', ' })port $($d.Port)$(if ($pn) { ' : ' + $pn } elseif ($warnPort) { ' : ' + $SusPorts[[int]$d.Port] })$(if ($d.Live) { ', connexion ouverte' })" 11.5 $(if ($warnPort) { $Colors.warn } else { '#5B6475' })))
        Add-ToGrid $row $left 0
        $v = New-Text "↑ $(Format-Bytes $d.Out)   ↓ $(Format-Bytes $d.In)" 12 '#C9CED8'
        $v.VerticalAlignment = 'Center'
        Add-ToGrid $row $v 1
        [void]$body.Children.Add($row)
    }
}
