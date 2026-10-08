# Nexo : diagnostic du lag en ligne. Pendant une partie (ou un test de 30 s), mesure chaque
# étape du chemin : PC vers box, box vers fournisseur, Internet, serveur du jeu. Puis explique d'où
# vient le lag (Wi-Fi, téléchargement, box, serveur loin) et propose des corrections.
# Chargé par OptiGame.ps1 après jeu.ps1 et trafic.ps1.

$LagFile = Join-Path $DataDir 'lag.json'
$LagColors = @{ gw = '#4EA8FF'; isp = '#9AA3B2'; ref = '#B18CFF'; srv = '#22D37A' }
$LagNames = @{ gw = 'Ton PC vers ta box'; isp = 'Ta box vers ton fournisseur'; ref = 'Internet (référence)'; srv = 'Serveur du jeu' }

function Test-LagMeasure { [bool](Get-Setting 'LagMeasure' $true) }

function Get-LagSessions {
    if ($null -ne $script:LagSessions) { return $script:LagSessions.ToArray() }
    $script:LagSessions = New-Object System.Collections.ArrayList
    try {
        if (Test-Path $LagFile) {
            $arr = Get-Content $LagFile -Raw -Encoding UTF8 | ConvertFrom-Json
            foreach ($s in $arr) { [void]$script:LagSessions.Add($s) }
        }
    } catch { Write-Log "Lag: historique illisible ($_)" }
    $script:LagSessions.ToArray()
}

function Save-LagSessions {
    $null = Get-LagSessions
    $l = $script:LagSessions
    while ($l.Count -gt 30) { $l.RemoveAt(0) }
    try { [IO.File]::WriteAllText($LagFile, (ConvertTo-Json -InputObject @($l) -Depth 8 -Compress), (New-Object Text.UTF8Encoding($false))) } catch { Write-Log "Lag: $_" }
}

# ---------------------------------------------------------------------------
# Mesure
# ---------------------------------------------------------------------------
function Start-LagSession([string]$Game, [int]$ProcId, [int]$Seconds = 0) {
    if ($script:LagSession) { return }
    if (-not $script:Net) { $script:Net = Get-ActiveNet }
    $net = $script:Net
    $gw = if ($net -and $net.Gateway -and $net.Gateway -ne '0.0.0.0') { [string]$net.Gateway } else { '' }
    $etw = $false
    try { $etw = [NetFlow]::Start(); if (-not $etw) { Write-Log "Lag: repérage du serveur limité ($([NetFlow]::LastError))" } } catch { Write-Log "Lag: $_" }
    [LagMon]::Start(500)
    $t = @{}
    if ($gw) { $t.gw = [LagMon]::Add($gw) }
    $t.ref = [LagMon]::Add('1.1.1.1')
    $s = @{ Game = $Game; Pid = $ProcId; Start = Get-Date; Seconds = $Seconds; Gw = $gw; Wifi = [bool]($net -and $net.Wifi); Adapter = $(if ($net) { "$($net.Name) $($net.Desc)" } else { '' })
        Etw = $etw; T = $t; Isp = ''; IspOwner = $null; Server = $null; Ticks = 0; Prev = @{}; Names = @{}; Bg = (New-Object System.Collections.ArrayList)
        Signal = @(); Band = ''; Channel = ''; TraceJob = $null; SrvJob = $null; OwnerJob = $null; Gaps = 0; MaxGap = 0; GapBase = $null }
    # Chemin vers Internet en arrière plan : le premier routeur public est celui du fournisseur
    $ps = [PowerShell]::Create(); $ps.RunspacePool = $script:BgPool
    [void]$ps.AddScript('param($h) [LagMon]::TraceRoute($h, 10, 800)').AddArgument('1.1.1.1')
    $s.TraceJob = @{ PS = $ps; Handle = $ps.BeginInvoke() }
    $script:LagSession = $s
    if (-not $script:LagTimer) {
        $script:LagTimer = New-Object System.Windows.Threading.DispatcherTimer
        $script:LagTimer.Interval = [TimeSpan]::FromSeconds(2)
        $script:LagTimer.Add_Tick({ try { Update-LagSession } catch { Write-Log "Lag: $_" } })
    }
    $script:LagTimer.Start()
    Write-Log "Lag: mesure de la connexion ($Game)$(if ($etw) { ', serveur repérable en UDP' })"
    Update-LagLive
}

function Get-JobResult($Job) {
    if (-not $Job -or -not $Job.Handle.IsCompleted) { return $null }
    try { , @($Job.PS.EndInvoke($Job.Handle)) } catch { , @() } finally { $Job.PS.Dispose() }
}

# Nom lisible d'un processus (les services Windows par leur vrai nom)
function Get-LagProcName([int]$ProcId) {
    $s = $script:LagSession
    if ($s.Names.ContainsKey($ProcId)) { return $s.Names[$ProcId] }
    $n = "Programme $ProcId"
    $p = Get-Process -Id $ProcId -ErrorAction SilentlyContinue
    if ($p) {
        $n = $p.ProcessName
        if ($n -eq 'svchost') { $sv = @(Get-SvcNames $ProcId); if ($sv.Count) { $n = "Windows : $($sv[0].Title)" } }
        else { try { $d = [string]$p.MainModule.FileVersionInfo.FileDescription; if ($d -and $d.Length -lt 50) { $n = $d } } catch {} }
    }
    $s.Names[$ProcId] = $n
    $n
}

function Update-LagSession {
    $s = $script:LagSession
    if (-not $s) { return }
    $s.Ticks++
    $elapsed = ((Get-Date) - $s.Start).TotalSeconds
    Receive-LagOwners
    # Premier routeur du fournisseur
    if ($s.TraceJob) {
        $r = Get-JobResult $s.TraceJob
        if ($null -ne $r) {
            $s.TraceJob = $null
            $hop = @($r | ForEach-Object { $x = ([string]$_) -split '\|'; if ($x[1] -and -not (Test-PrivateIp $x[1])) { $x[1] } } | Select-Object -First 1)[0]
            if ($hop -and $hop -ne '1.1.1.1') { $s.Isp = $hop; $s.T.isp = [LagMon]::Add($hop); Start-LagOwnerLookup $hop }
        }
    }
    # Réseau par programme (droits admin) : serveur du jeu et téléchargements en arrière plan
    $flows = @()
    if ($s.Etw) { try { $flows = @([NetFlow]::Snapshot()) } catch {} }
    $perPid = @{}
    $best = $null
    foreach ($l in $flows) {
        $x = ([string]$l) -split '\|'
        if ($x.Count -lt 12 -or (Test-PrivateIp $x[2])) { continue }
        $fpid = [int]$x[0]; $bytes = [double]$x[6] + [double]$x[7]; $k = "$($x[0])|$($x[1])|$($x[2])|$($x[3])"
        $prev = if ($s.Prev.ContainsKey($k)) { $s.Prev[$k] } else { 0 }
        $s.Prev[$k] = $bytes
        if ($fpid -eq $s.Pid) {
            $pk = [double]$x[4] + [double]$x[5]
            if ([double]$x[5] -gt 50 -and [int]$x[11] -lt 5000 -and (-not $best -or $pk -gt $best.Pk)) { $best = @{ Ip = $x[2]; Port = [int]$x[3]; Proto = $x[1]; Pk = $pk; Gaps = [int]$x[9]; MaxGap = [int]$x[10] } }
        } elseif ($fpid -ne $PID -and $fpid -gt 4) { $perPid[$fpid] = [double]$perPid[$fpid] + ($bytes - $prev) }
    }
    # Sans les droits admin : la connexion TCP du jeu (serveur de connexion, souvent dans la même région)
    if (-not $s.Etw -and $s.Pid -and -not $s.Server -and $s.Ticks -ge 3) {
        foreach ($l in @([TrafficMon]::Tcp())) {
            $x = ([string]$l) -split '\|'
            if ([int]$x[0] -eq $s.Pid -and -not (Test-PrivateIp $x[1])) { $best = @{ Ip = $x[1]; Port = [int]$x[2]; Proto = 'tcp'; Pk = 0; Gaps = 0; MaxGap = 0 }; break }
        }
    }
    if ($best) {
        if (-not $s.Server -or ($s.Server.Ip -ne $best.Ip -and $s.Ticks % 15 -eq 0)) {
            $s.Server = @{ Ip = $best.Ip; Port = $best.Port; Proto = $best.Proto; Ping = $null; Proxy = '' }
            $s.T.srv = [LagMon]::Add($best.Ip)
            $s.GapBase = $best.Gaps
            Start-LagOwnerLookup $best.Ip
        }
        if ($s.Server.Ip -eq $best.Ip -and $null -ne $s.GapBase) { $s.Gaps = $best.Gaps - $s.GapBase; $s.MaxGap = [math]::Max($s.MaxGap, $best.MaxGap) }
    }
    # Le serveur ne répond pas au ping : on mesure le dernier routeur avant lui
    if ($s.Server -and $null -eq $s.Server.Ping -and $null -ne $s.T.srv) {
        $a = [LagMon]::Series($s.T.srv)
        if ($a.Count -ge 16) {
            $ok = 0; for ($i = 1; $i -lt $a.Count; $i += 2) { if ($a[$i] -ge 0) { $ok++ } }
            $s.Server.Ping = $ok -gt 0
            if (-not $s.Server.Ping) {
                $ps = [PowerShell]::Create(); $ps.RunspacePool = $script:BgPool
                [void]$ps.AddScript('param($h) [LagMon]::TraceRoute($h, 20, 800)').AddArgument($s.Server.Ip)
                $s.SrvJob = @{ PS = $ps; Handle = $ps.BeginInvoke() }
            }
        }
    }
    if ($s.SrvJob) {
        $r = Get-JobResult $s.SrvJob
        if ($null -ne $r) {
            $s.SrvJob = $null
            $last = @($r | ForEach-Object { $x = ([string]$_) -split '\|'; if ($x[1] -and -not (Test-PrivateIp $x[1]) -and $x[1] -ne $s.Isp) { $x[1] } } | Select-Object -Last 1)[0]
            if ($last -and $last -ne $s.Server.Ip) { $s.Server.Proxy = $last; $s.T.proxy = [LagMon]::Add($last) }
        }
    }
    # Téléchargements des autres programmes (octets par seconde)
    $top = $null; $total = 0.0
    foreach ($k in $perPid.Keys) { $r = $perPid[$k] / 2; $total += $r; if (-not $top -or $r -gt $top.Rate) { $top = @{ Pid = $k; Rate = $r } } }
    $bgName = if ($top -and $top.Rate -gt 200KB) { Get-LagProcName $top.Pid } else { '' }
    [void]$s.Bg.Add(@{ T = [math]::Round($elapsed, 1); Rate = [math]::Round($total); Name = $bgName; NameRate = $(if ($top) { [math]::Round($top.Rate) } else { 0 }) })
    # Wi-Fi : force du signal et bande
    # Wi-Fi : force du signal et bande, lues en arrière plan (netsh figeait la fenêtre)
    if ($s.WifiJob) {
        $r = Get-JobResult $s.WifiJob
        if ($null -ne $r) {
            $s.WifiJob = $null
            foreach ($ln in @($r)) {
                if ($ln -match '^\s+Signal\s+:\s+(\d+)') { $s.Signal += [int]$Matches[1] }
                elseif ($ln -match '^\s+(Canal|Channel)\s+:\s+(\d+)') { $s.Channel = $Matches[2] }
                elseif ($ln -match '^\s+(Bande|Band)\s+:\s+(.+?)\s*$') { $s.Band = $Matches[2] }
            }
        }
    }
    if ($s.Wifi -and $s.Ticks % 5 -eq 1 -and -not $s.WifiJob) {
        $ps = [PowerShell]::Create(); $ps.RunspacePool = $script:BgPool
        [void]$ps.AddScript('@(netsh wlan show interfaces 2>$null)')
        $s.WifiJob = @{ PS = $ps; Handle = $ps.BeginInvoke() }
    }
    if ($s.Seconds -gt 0 -and $elapsed -ge $s.Seconds) { Stop-LagSession; return }
    Update-LagLive
}

function Start-LagOwnerLookup([string]$Ip) {
    if (Get-ServerOwner $Ip) { return }
    if (-not (Get-Setting 'TrafficLookup' $true)) { return }
    $ps = [PowerShell]::Create(); $ps.RunspacePool = $script:BgPool
    [void]$ps.AddScript($ServerLookupWork.ToString()).AddArgument(@($Ip))
    if (-not $script:LagOwnerJobs) { $script:LagOwnerJobs = New-Object System.Collections.ArrayList }
    [void]$script:LagOwnerJobs.Add(@{ PS = $ps; Handle = $ps.BeginInvoke() })
}

# Résultats de l'annuaire (propriétaire du serveur) : ajoutés au cache commun avec la page Trafic
function Receive-LagOwners {
    if (-not $script:LagOwnerJobs) { return }
    foreach ($j in @($script:LagOwnerJobs)) {
        if (-not $j.Handle.IsCompleted) { continue }
        [void]$script:LagOwnerJobs.Remove($j)
        foreach ($r in @(Get-JobResult $j)) {
            if ($r.Ok -and ($r.O -or $r.N) -and $r.S -and $r.E) { [void](Get-ServerCache).Add(@{ S = $r.S; E = $r.E; O = $r.O; C = $r.C; N = $r.N; D = (Get-Date).ToString('yyyy-MM-dd') }); Save-ServerCache }
        }
    }
}

# Statistiques d'une série de pings [t, ms, t, ms...] (ms -1 = perdu)
function Get-PingStats([double[]]$A) {
    $n = [int]($A.Count / 2)
    if ($n -lt 8) { return $null }
    $ok = New-Object System.Collections.Generic.List[double]
    $lost = 0
    for ($i = 0; $i -lt $n; $i++) { $r = $A[2 * $i + 1]; if ($r -lt 0) { $lost++ } else { $ok.Add($r) } }
    $loss = [math]::Round(100.0 * $lost / $n, 1)
    if ($ok.Count -lt [math]::Max(4, $n * 0.2)) { return @{ N = $n; Loss = $loss; Dead = $true; Med = -1; P95 = -1; Jit = 0; Max = -1; Spikes = @() } }
    $jit = 0.0
    for ($i = 1; $i -lt $ok.Count; $i++) { $jit += [math]::Abs($ok[$i] - $ok[$i - 1]) }
    $jit = $jit / [math]::Max(1, $ok.Count - 1)
    $sorted = $ok.ToArray(); [Array]::Sort($sorted)
    $med = $sorted[[int]($sorted.Count / 2)]
    $p95 = $sorted[[int][math]::Floor(0.95 * ($sorted.Count - 1))]
    $lim = $med + [math]::Max(30.0, 4 * $jit)
    $spikes = New-Object System.Collections.Generic.List[double]
    for ($i = 0; $i -lt $n; $i++) { $r = $A[2 * $i + 1]; if ($r -lt 0 -or $r -gt $lim) { $spikes.Add($A[2 * $i]) } }
    @{ N = $n; Loss = $loss; Dead = $false; Med = [math]::Round($med, 1); P95 = [math]::Round($p95, 1); Jit = [math]::Round($jit, 1); Max = [math]::Round($sorted[-1], 1); Spikes = $spikes.ToArray() }
}

# Série réduite pour le graphique : le pire ping de chaque tranche (-1 si tout est perdu)
function Compress-PingSeries([double[]]$A, [double]$Step) {
    $out = @(); $cur = -2; $b = 0
    for ($i = 0; $i -lt $A.Count; $i += 2) {
        $k = [int][math]::Floor($A[$i] / $Step)
        while ($b -lt $k) { $out += $cur; $cur = -2; $b++ }
        $r = $A[$i + 1]
        $cur = if ($r -lt 0) { $(if ($cur -eq -2) { -1 } else { $cur }) } else { [math]::Max($cur, $r) }
    }
    if ($cur -ne -2) { $out += $cur }
    @($out | ForEach-Object { if ($_ -eq -2) { -1 } else { [math]::Round($_) } })
}

function Stop-LagSession {
    $s = $script:LagSession
    $script:LagSession = $null
    if ($script:LagTimer) { $script:LagTimer.Stop() }
    if (-not $s) { return }
    [LagMon]::Stop()
    if ($s.Etw) { try { [NetFlow]::Stop() } catch {} }
    foreach ($j in @($s.TraceJob, $s.SrvJob)) { if ($j) { try { [void]$j.PS.BeginStop($null, $null) } catch {} } }
    Receive-LagOwners
    $dur = ((Get-Date) - $s.Start).TotalSeconds
    if ($dur -lt $(if ($s.Seconds -gt 0) { 6 } else { 60 })) { Update-LagLive; return }
    $step = [math]::Max(1.0, [math]::Ceiling($dur / 240))
    $stats = @{}; $series = @{}
    foreach ($k in @($s.T.Keys)) {
        $a = [double[]][LagMon]::Series($s.T[$k])
        $st = Get-PingStats $a
        if ($st) { $stats[$k] = $st; $series[$k] = Compress-PingSeries $a $step }
    }
    # Le serveur ne répond pas au ping : le dernier routeur avant lui le remplace
    if ($s.Server -and $stats.srv -and $stats.srv.Dead -and $stats.proxy -and -not $stats.proxy.Dead) { $stats.srv = $stats.proxy; $series.srv = $series.proxy; $s.Server.ViaProxy = $true }
    $stats.Remove('proxy'); $series.Remove('proxy')
    # Pics de ping de la partie : en même temps qu'un pic box (Wi-Fi) ? qu'un téléchargement ?
    $main = if ($stats.srv -and -not $stats.srv.Dead) { $stats.srv } elseif ($stats.ref) { $stats.ref } else { $null }
    $local = 0; $bgHit = 0; $bgNames = @{}
    if ($main -and $main.Spikes.Count) {
        foreach ($t in $main.Spikes) {
            if ($stats.gw -and @($stats.gw.Spikes | Where-Object { [math]::Abs($_ - $t) -le 1 }).Count) { $local++ }
            $bs = @($s.Bg | Where-Object { [math]::Abs($_.T - $t) -le 2.5 -and $_.Rate -gt 500KB })
            if ($bs.Count) { $bgHit++; foreach ($b in $bs) { if ($b.Name) { $bgNames[$b.Name] = [math]::Max([double]$bgNames[$b.Name], [double]$b.NameRate) } } }
        }
    }
    $srvOwner = if ($s.Server) { Get-ServerOwner $s.Server.Ip }
    $ispOwner = if ($s.Isp) { Get-ServerOwner $s.Isp }
    $rec = @{
        Id = [guid]::NewGuid().ToString('N').Substring(0, 10); Date = (Get-Date).ToString('o'); Game = $s.Game; Quick = ($s.Seconds -gt 0); Seconds = [int]$dur
        Wifi = $s.Wifi; Vpn = [bool]($s.Adapter -match '(?i)vpn|wireguard|openvpn|tap-windows|nord|proton|mullvad|surfshark|expressvpn|tunnel')
        Signal = $(if ($s.Signal.Count) { ($s.Signal | Measure-Object -Minimum).Minimum } else { $null }); SignalAvg = $(if ($s.Signal.Count) { [math]::Round(($s.Signal | Measure-Object -Average).Average) } else { $null })
        Band = $s.Band; Channel = $s.Channel; Etw = $s.Etw
        Server = $(if ($s.Server) { @{ Ip = $s.Server.Ip; Port = $s.Server.Port; Proto = $s.Server.Proto; ViaProxy = [bool]$s.Server.ViaProxy; Owner = $(if ($srvOwner) { Get-OwnerLabel $srvOwner } else { '' }) } } else { $null })
        Isp = $(if ($ispOwner) { Get-OwnerLabel $ispOwner } else { '' })
        Stats = @{}; Step = $step; Series = $series
        Spikes = $(if ($main) { $main.Spikes.Count } else { 0 }); LocalSpikes = $local; BgSpikes = $bgHit
        BgApps = @($bgNames.GetEnumerator() | Sort-Object Value -Descending | Select-Object -First 3 | ForEach-Object { @{ Name = $_.Key; Rate = $_.Value } })
        Gaps = $s.Gaps; MaxGap = $s.MaxGap
    }
    foreach ($k in $stats.Keys) { $x = $stats[$k]; $rec.Stats[$k] = @{ Med = $x.Med; P95 = $x.P95; Jit = $x.Jit; Loss = $x.Loss; Max = $x.Max; Dead = $x.Dead; N = $x.N; Spikes = $x.Spikes.Count } }
    # Diagnostic gardé sans les blocs de code des actions (recréés à l'affichage)
    $d = Get-LagDiagnosis $rec
    $rec.Diag = @{ Level = $d.Level; Title = $d.Title; Findings = @($d.Findings | ForEach-Object { @{ Status = $_.Status; Title = $_.Title; Detail = $_.Detail; Tips = @($_.Tips); Actions = @(@($_.Actions) | Where-Object { $_ } | ForEach-Object { @{ Label = $_.Label } }) } }) }
    if (-not $rec.Quick -and -not $rec.Server -and $rec.Diag.Level -eq 'ok') {
        Write-Log "Lag: $($s.Game) sans serveur en ligne repéré et connexion stable, mesure non gardée"
        Update-LagLive
        return
    }
    $obj = $rec | ConvertTo-Json -Depth 8 | ConvertFrom-Json
    $null = Get-LagSessions
    [void]$script:LagSessions.Add($obj)
    Save-LagSessions
    Write-Log "Lag: $($s.Game), $([int]($dur / 60)) min, $($rec.Diag.Title)"
    Build-LagPanel
    $open = { Show-Page 1; Set-GamingSubPage 'lag'; Show-LagSession $obj.Id }.GetNewClosure()
    if ($rec.Quick) { if (-not $script:TestRunning) { Show-LagSession $obj.Id } }
    else { Show-Notify "Connexion pendant $($s.Game)" "$($rec.Diag.Title). Clique pour voir le détail." $open }
}

# ---------------------------------------------------------------------------
# Diagnostic
# ---------------------------------------------------------------------------
function Get-SegStatus($S, [string]$Kind) {
    if (-not $S -or $S.Dead) { return 'none' }
    if ($Kind -eq 'gw') {
        if ($S.Loss -ge 2 -or $S.Jit -ge 10 -or $S.P95 -ge 50) { return 'bad' }
        if ($S.Loss -ge 0.5 -or $S.Jit -ge 4 -or $S.P95 -ge 20) { return 'warn' }
        return 'ok'
    }
    if ($S.Loss -ge 3 -or $S.Jit -ge 20 -or $S.P95 -ge $S.Med + 80) { return 'bad' }
    if ($S.Loss -ge 1 -or $S.Jit -ge 8 -or $S.P95 -ge $S.Med + 35) { return 'warn' }
    'ok'
}

function Format-Ms($V) { if ($null -eq $V -or $V -lt 0) { '?' } else { '{0:N0} ms' -f $V } }

# Constats du plus grave au moins grave : @{ Status; Title; Detail; Tips; Actions }
function Get-LagDiagnosis($R) {
    $st = $R.Stats
    $gw = $st.gw; $ref = $st.ref; $srv = $st.srv
    $gS = Get-SegStatus $gw 'gw'; $rS = Get-SegStatus $ref 'ref'; $sS = Get-SegStatus $srv 'srv'
    $f = @()
    $pct = { param($a, $b) if ($b -gt 0) { $a / $b } else { 0 } }
    # 1. Entre le PC et la box
    if ($gS -in 'bad', 'warn') {
        if ($R.Wifi) {
            $tips = @('Le plus efficace : brancher le PC à la box avec un câble Ethernet (ou un boîtier CPL si la box est loin).')
            if ($R.Band -match '^2' -or ($R.Channel -and [int]$R.Channel -le 14)) { $tips += 'Ton PC est connecté en Wi-Fi 2,4 GHz : choisis le réseau 5 GHz de ta box, il est plus rapide et moins encombré.' }
            if ($null -ne $R.Signal -and $R.Signal -lt 60) { $tips += "Le signal est descendu à $($R.Signal) % : rapproche le PC de la box ou ajoute un répéteur." }
            $tips += 'Évite les gros téléchargements sur les autres appareils pendant que tu joues.'
            $f += @{ Status = $gS; Title = 'Le Wi-Fi fait laguer'
                Detail = "Entre ton PC et ta box : $($gw.Loss) % de paquets perdus, un ping qui varie de $($gw.Jit) ms en moyenne, avec des pics à $(Format-Ms $gw.P95). En jeu, ça donne des téléportations et des retours en arrière."
                Tips = $tips }
        } else {
            $f += @{ Status = $gS; Title = 'Problème entre le PC et la box'
                Detail = "Même en câble, $($gw.Loss) % des paquets se perdent entre ton PC et ta box, avec des pics à $(Format-Ms $gw.P95)."
                Tips = @('Essaie un autre câble Ethernet et une autre prise de la box.', 'Si tu passes par des boîtiers CPL, branche le PC directement sur la box pour comparer.', 'Mets à jour le pilote de ta carte réseau (site du fabricant de ta carte mère).') }
        }
    }
    # 2. Entre la box et Internet
    if ($gS -notin 'bad', 'warn' -and $rS -in 'bad', 'warn') {
        $apps = @($R.BgApps | Where-Object { $_.Name })
        if ($apps.Count -and (& $pct $R.BgSpikes $R.Spikes) -ge 0.3) {
            $names = ($apps | ForEach-Object { "$($_.Name) ($(Format-Bytes $_.Rate)/s)" }) -join ', '
            $acts = @()
            if (@($apps | Where-Object { $_.Name -match 'Windows : .*(Optimisation de livraison|Optimisation de la distribution|Delivery Optimization|Windows Update|BITS|transfert intelligent)' }).Count) {
                $acts += @{ Label = 'Limiter Windows Update'; NoRefresh = $true; Arg = $null; Script = { param($x) Set-Reg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization' 'DOPercentageMaxBackgroundBandwidth' 10; Set-Reg 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization' 'DODownloadMode' 0; Set-Status 'Windows Update est limité à 10 % de ta connexion en arrière plan. Annulable (page Sauvegarde).' } }
            }
            $f += @{ Status = $rS; Title = 'Un téléchargement sature ta connexion'
                Detail = "Pendant les pics de ping, ton PC téléchargeait ou envoyait en même temps : $names. La connexion était pleine, les paquets du jeu attendaient leur tour."
                Tips = @('Dans Steam : Paramètres, Téléchargements, décoche « Autoriser les téléchargements pendant une partie ».', 'Active le Mode jeu de Nexo : il ferme les launchers et la synchronisation pendant tes parties.', 'Lance tes mises à jour et téléchargements avant ou après tes parties.')
                Actions = $acts }
        } else {
            $isp = if ($R.Isp) { " ($($R.Isp))" } else { '' }
            $tips = @('Regarde si quelqu''un de la maison regarde une vidéo, télécharge ou sauvegarde en ligne pendant tes parties.', 'Redémarre ta box : débranche la 30 secondes.', 'Fais le test « Latence en charge » de la page Tests : il montre si ta box gère mal les gros téléchargements.', "Si ça dure plusieurs jours, contacte ton fournisseur$isp avec ces chiffres.")
            if (-not $R.Etw) { $tips += 'Lance Nexo en administrateur : il pourra voir si un programme de ce PC télécharge pendant tes parties.' }
            $f += @{ Status = $rS; Title = 'Ta connexion Internet sature ou décroche'
                Detail = "Ton PC et ta box communiquent bien, mais le ping vers Internet a des pics (jusqu'à $(Format-Ms $ref.P95), $($ref.Loss) % perdus). Soit un autre appareil de la maison utilise beaucoup la connexion, soit ta box ou ton fournisseur a un souci."
                Tips = $tips }
        }
    }
    # 3. Le serveur du jeu
    $owner = if ($R.Server -and $R.Server.Owner) { $R.Server.Owner } else { 'un hébergeur inconnu' }
    if ($srv -and -not $srv.Dead -and $ref -and -not $ref.Dead) {
        if ($srv.Med -gt 70 -and $srv.Med - $ref.Med -gt 45) {
            $f += @{ Status = 'warn'; Title = 'Le serveur du jeu est loin'
                Detail = "Le serveur appartient à $owner : $(Format-Ms $srv.Med) de ping, alors qu'Internet répond en $(Format-Ms $ref.Med) depuis chez toi. Ce délai vient de la distance, pas de ta connexion."
                Tips = @('Dans les options du jeu, choisis une région de serveurs plus proche (Europe, France).', 'Si tu joues avec des amis loin de toi, le jeu peut choisir un serveur entre vous.') }
        }
        if ($sS -in 'bad', 'warn' -and $rS -eq 'ok' -and $gS -notin 'bad', 'warn') {
            $f += @{ Status = 'warn'; Title = 'Le serveur du jeu avait des soucis'
                Detail = "Ta connexion était stable, mais le ping vers le serveur ($owner) variait beaucoup : pics à $(Format-Ms $srv.P95), $($srv.Loss) % perdus. Le problème est du côté du serveur ou de la route jusqu'à lui."
                Tips = @('Change de serveur ou de région si le jeu le permet.', 'Regarde si d''autres joueurs signalent des soucis (réseaux sociaux du jeu).') }
        }
    }
    $mins = [math]::Max(1.0, $R.Seconds / 60)
    if ($R.Gaps -and $R.Gaps / $mins -ge 2 -and $gS -notin 'bad', 'warn' -and $rS -notin 'bad', 'warn') {
        $f += @{ Status = 'warn'; Title = 'Le serveur envoyait ses données par à-coups'
            Detail = "$($R.Gaps) fois, le serveur du jeu n'a rien envoyé pendant plus d'un quart de seconde (le plus long : $($R.MaxGap) ms), alors que ta connexion était stable. Ça vient du serveur ou de la route jusqu'à lui."
            Tips = @('Change de serveur ou de région si le jeu le permet.') }
    }
    # 4. VPN
    if ($R.Vpn) {
        $f += @{ Status = 'warn'; Title = 'Un VPN rallonge le chemin'; Detail = 'Ta connexion passe par un VPN : chaque paquet fait un détour par le serveur du VPN, ce qui ajoute du ping.'; Tips = @('Coupe le VPN pendant tes parties, ou choisis un serveur VPN proche de chez toi.') }
    }
    # 5. Serveur non repéré
    if (-not $R.Quick -and -not $R.Server) {
        $f += @{ Status = 'info'; Title = 'Serveur du jeu non repéré'
            Detail = $(if ($R.Etw) { 'Le jeu n''a pas échangé assez avec un serveur pour que Nexo le reconnaisse (menu, partie hors ligne ?).' } else { 'Lance Nexo en administrateur : il pourra repérer le serveur du jeu (même en UDP) et vérifier s''il est loin.' }) }
    }
    if (-not @($f | Where-Object { $_.Status -in 'bad', 'warn' }).Count) {
        $parts = @(); if ($gw -and -not $gw.Dead) { $parts += "box $(Format-Ms $gw.Med)" }; if ($ref -and -not $ref.Dead) { $parts += "Internet $(Format-Ms $ref.Med)" }; if ($srv -and -not $srv.Dead) { $parts += "serveur $(Format-Ms $srv.Med)" }
        $f = @(@{ Status = 'ok'; Title = 'Ta connexion était stable'; Detail = "Ping : $($parts -join ', '), sans pic ni perte notable. Si le jeu saccadait, ça vient plutôt de tes FPS (onglet Mes parties)." }) + $f
    }
    $lvl = if (@($f | Where-Object { $_.Status -eq 'bad' }).Count) { 'bad' } elseif (@($f | Where-Object { $_.Status -eq 'warn' }).Count) { 'warn' } else { 'ok' }
    @{ Level = $lvl; Title = @($f | Where-Object { $_.Status -eq $lvl })[0].Title; Findings = $f }
}

# ---------------------------------------------------------------------------
# Affichage
# ---------------------------------------------------------------------------
function Update-LagLive {
    $t = $script:LagLive
    if (-not $t) { return }
    $s = $script:LagSession
    if (-not $s) { $t.Visibility = 'Collapsed'; if ($script:LagTestBtn) { $script:LagTestBtn.IsEnabled = $true }; return }
    if ($script:LagTestBtn) { $script:LagTestBtn.IsEnabled = $false }
    $parts = @()
    foreach ($k in 'gw', 'isp', 'ref', 'srv') {
        if ($null -eq $s.T[$k]) { continue }
        $a = [LagMon]::Series($s.T[$k])
        if ($a.Count -lt 2) { continue }
        $v = $a[-1]
        $parts += "$(@{ gw = 'box'; isp = 'fournisseur'; ref = 'Internet'; srv = 'serveur' }[$k]) $(if ($v -lt 0) { 'perdu' } else { '{0:N0} ms' -f $v })"
    }
    $el = [int]((Get-Date) - $s.Start).TotalSeconds
    $t.Text = "Mesure en cours$(if ($s.Seconds -gt 0) { " ($el / $($s.Seconds) s)" } else { " pendant $($s.Game) ($([int]($el / 60)) min)" }) : $($parts -join ', ')"
    $t.Visibility = 'Visible'
}

function Get-LagSessionLine($S) {
    $p = @()
    foreach ($k in 'gw', 'ref', 'srv') { $x = $S.Stats.$k; if ($x -and -not $x.Dead) { $p += "$(@{ gw = 'Box'; ref = 'Internet'; srv = 'Serveur' }[$k]) $(Format-Ms $x.Med)" } }
    $p -join ', '
}

function Build-LagPanel {
    $panel = $ui.LagPanel
    if (-not $panel) { return }
    $panel.Children.Clear()
    $card = New-Card
    $card.Margin = New-Thickness 0 0 0 16
    $sp = New-Object System.Windows.Controls.StackPanel
    [void]$sp.Children.Add((New-SwitchRow 'Mesurer ma connexion quand je joue' 'Pour tes jeux en ligne : Nexo mesure ta box, Internet et le serveur du jeu pendant la partie, puis t''explique d''où vient le lag.' (Test-LagMeasure) {
        param($s, $e)
        Set-Setting 'LagMeasure' ([bool]$s.IsChecked)
        if (-not $s.IsChecked -and $script:LagSession -and -not $script:LagSession.Seconds) { Stop-LagSession }
        Update-GameWatch
    }))
    $row = New-Object System.Windows.Controls.WrapPanel
    $btn = New-Button 'Tester ma connexion maintenant (30 s)' 'BtnPrimary'
    $btn.Margin = New-Thickness 0 0 10 0
    $btn.IsEnabled = -not $script:LagSession
    $btn.Add_Click({ Invoke-Safe { Start-LagSession 'Test rapide' 0 30 } })
    $script:LagTestBtn = $btn
    [void]$row.Children.Add($btn)
    [void]$sp.Children.Add($row)
    $script:LagLive = New-Text '' 12.5 $Colors.info -Semi
    $script:LagLive.Margin = New-Thickness 0 10 0 0
    $script:LagLive.Visibility = 'Collapsed'
    [void]$sp.Children.Add($script:LagLive)
    $card.Child = $sp
    [void]$panel.Children.Add($card)
    Update-LagLive

    $all = @(Get-LagSessions)
    if (-not $all.Count) {
        [void]$panel.Children.Add((New-Text 'Aucune mesure pour l''instant. Joue une partie en ligne (au moins une minute) ou lance le test rapide : le résultat apparaîtra ici.' 13 '#5B6475'))
        return
    }
    $h = New-Text 'Dernières mesures (clique pour le détail)' 13 '#9AA3B2' -Semi
    $h.Margin = New-Thickness 0 0 0 6
    [void]$panel.Children.Add($h)
    foreach ($s in @($all | Sort-Object { [datetime]$_.Date } -Descending | Select-Object -First 15)) {
        $c = New-Card
        $c.Padding = New-Thickness 14 10 14 10
        $c.Margin = New-Thickness 0 0 0 6
        $c.Cursor = [System.Windows.Input.Cursors]::Hand
        $g = New-Grid @('Auto', '*', 'Auto')
        $dot = New-Object System.Windows.Shapes.Ellipse
        $dot.Width = 10; $dot.Height = 10; $dot.Fill = Get-Brush $Colors[[string]$s.Diag.Level]; $dot.Margin = New-Thickness 0 0 12 0; $dot.VerticalAlignment = 'Center'
        Add-ToGrid $g $dot 0
        $tx = New-Object System.Windows.Controls.StackPanel
        [void]$tx.Children.Add((New-Text "$($s.Game) : $($s.Diag.Title)" 14 '#FFFFFF' -Semi))
        [void]$tx.Children.Add((New-Text "$(([datetime]$s.Date).ToString('dd/MM à HH:mm')), $(Format-PlayTime $s.Seconds). $(Get-LagSessionLine $s)" 12 '#9AA3B2'))
        Add-ToGrid $g $tx 1
        $ch = New-Text '›' 22 '#5B6475'; $ch.VerticalAlignment = 'Center'
        Add-ToGrid $g $ch 2
        $c.Child = $g
        $c.Tag = [string]$s.Id
        $c.Add_MouseEnter({ param($x, $e) $x.Background = Get-Brush 'card-hover' })
        $c.Add_MouseLeave({ param($x, $e) $x.Background = Get-Brush 'card' })
        $c.Add_MouseLeftButtonUp({ param($x, $e) Invoke-Safe { Show-LagSession $x.Tag } })
        [void]$panel.Children.Add($c)
    }
}

# Graphique : une courbe par étape du chemin (pire ping de chaque tranche, pertes en rouge)
function New-LagChart($S) {
    $w = 640; $h = 150
    $cv = New-Object System.Windows.Controls.Canvas
    $cv.Width = $w; $cv.Height = $h; $cv.ClipToBounds = $true
    $max = 20.0
    foreach ($k in 'gw', 'ref', 'srv') { foreach ($v in @($S.Series.$k)) { if ($v -gt $max) { $max = [double]$v } } }
    $max = [math]::Min(400, [math]::Ceiling($max * 1.15 / 10) * 10)
    foreach ($y in 0.25, 0.5, 0.75) {
        $ln = New-Object System.Windows.Shapes.Line
        $ln.X1 = 0; $ln.X2 = $w; $ln.Y1 = $h * $y; $ln.Y2 = $h * $y
        $ln.Stroke = Get-Brush '#232A37'; $ln.StrokeThickness = 1
        $ln.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(2, 4))
        [void]$cv.Children.Add($ln)
    }
    foreach ($k in 'ref', 'gw', 'srv') {
        $vals = @($S.Series.$k)
        if ($vals.Count -lt 2) { continue }
        # Morceaux continus (une perte coupe la courbe), chacun lissé
        $parts = New-Object System.Collections.ArrayList
        $cur = New-Object 'System.Collections.Generic.List[System.Windows.Point]'
        for ($i = 0; $i -lt $vals.Count; $i++) {
            $x = $w * $i / ($vals.Count - 1)
            $v = [double]$vals[$i]
            if ($v -lt 0) {
                $m = New-Object System.Windows.Controls.Border
                $m.Width = [math]::Max(4, $w / $vals.Count); $m.Height = 6; $m.CornerRadius = [System.Windows.CornerRadius]::new(3)
                $m.Background = Get-Brush $Colors.bad; $m.Effect = New-Glow $Colors.bad 8 0.8
                [System.Windows.Controls.Canvas]::SetLeft($m, $x - 2); [System.Windows.Controls.Canvas]::SetTop($m, $h - 7)
                [void]$cv.Children.Add($m)
                if ($cur.Count) { [void]$parts.Add($cur); $cur = New-Object 'System.Collections.Generic.List[System.Windows.Point]' }
                continue
            }
            $cur.Add([System.Windows.Point]::new($x, $h - 4 - ($h - 8) * [math]::Min(1.0, $v / $max)))
        }
        if ($cur.Count) { [void]$parts.Add($cur) }
        $geo = New-Object System.Windows.Media.PathGeometry
        $area = New-Object System.Windows.Media.PathGeometry
        foreach ($p in $parts) {
            if ($p.Count -lt 2) { continue }
            $fig = Get-SmoothFigure $p 0 $h
            [void]$geo.Figures.Add($fig)
            $af = $fig.Clone()
            [void]$af.Segments.Add([System.Windows.Media.LineSegment]::new([System.Windows.Point]::new($p[$p.Count - 1].X, $h), $false))
            [void]$af.Segments.Add([System.Windows.Media.LineSegment]::new([System.Windows.Point]::new($p[0].X, $h), $false))
            $af.IsClosed = $true
            [void]$area.Figures.Add($af)
        }
        $fillPath = New-Object System.Windows.Shapes.Path
        $fillPath.Data = $area
        $col = Get-Color $LagColors[$k]
        $fillPath.Fill = New-LinearBrush @(('#{0:X2}{1:X2}{2:X2}{3:X2}' -f 50, $col.R, $col.G, $col.B), ('#00{0:X2}{1:X2}{2:X2}' -f $col.R, $col.G, $col.B)) 0 0 0 1
        [void]$cv.Children.Add($fillPath)
        $pl = New-Object System.Windows.Shapes.Path
        $pl.Data = $geo
        $pl.Stroke = New-LinearBrush @($LagColors[$k], (Get-LightHex $LagColors[$k] 0.35)) 0 0 1 0
        $pl.StrokeThickness = 2.2; $pl.StrokeLineJoin = 'Round'
        $pl.Effect = New-Glow $LagColors[$k] 10 0.7
        [void]$cv.Children.Add($pl)
    }
    $mx = New-Text "$max ms" 11 '#5B6475'
    [System.Windows.Controls.Canvas]::SetLeft($mx, 4); [System.Windows.Controls.Canvas]::SetTop($mx, 2)
    [void]$cv.Children.Add($mx)
    $sp = New-Object System.Windows.Controls.StackPanel
    $frame = New-Object System.Windows.Controls.Border
    $frame.Background = New-LinearBrush @('#141922', '#0E1117') 0 0 0 1
    $frame.BorderBrush = Get-Brush 'card-border'; $frame.BorderThickness = New-Thickness 1 1 1 1
    $frame.CornerRadius = [System.Windows.CornerRadius]::new(14); $frame.Padding = New-Thickness 12 10 12 10
    $frame.Child = $cv
    [void]$sp.Children.Add($frame)
    $leg = New-Object System.Windows.Controls.WrapPanel
    $leg.Margin = New-Thickness 0 6 0 0
    foreach ($k in 'gw', 'ref', 'srv') {
        if (-not $S.Series.$k) { continue }
        $tb = New-Text "●  $($LagNames[$k])" 11.5 $LagColors[$k]
        $tb.Margin = New-Thickness 0 0 16 0
        [void]$leg.Children.Add($tb)
    }
    [void]$leg.Children.Add((New-Text '▬  paquets perdus' 11.5 $Colors.bad))
    [void]$sp.Children.Add($leg)
    $sp
}

function Show-LagSession([string]$Id) {
    if ($script:TestRunning) { return }
    $s = @(Get-LagSessions | Where-Object { $_.Id -eq $Id })[0]
    if (-not $s) { return }
    Show-TestPanel @{ Tag = 'LAG'; Title = "Connexion : $($s.Game)"; Sub = "$(([datetime]$s.Date).ToString('dd/MM/yyyy à HH:mm')), $(Format-PlayTime $s.Seconds) de mesure" }
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    Set-TestState $s.Diag.Level $(switch ([string]$s.Diag.Level) { 'ok' { 'Stable' } 'warn' { 'À améliorer' } default { 'Lag trouvé' } })
    $body = $ui.TestBody

    # Le chemin, étape par étape
    [void]$body.Children.Add((New-SectionTitle 'LE CHEMIN DE TES PAQUETS'))
    $segs = @(
        @('gw', 'Ton PC vers ta box', $(if ($s.Wifi) { "Wi-Fi$(if ($s.Band) { " $($s.Band)" })$(if ($null -ne $s.Signal) { ", signal $($s.SignalAvg) % (au plus bas $($s.Signal) %)" })" } else { 'Câble Ethernet' })),
        @('isp', 'Ta box vers ton fournisseur', $(if ($s.Isp) { $s.Isp } else { 'Premier routeur de ton fournisseur' })),
        @('ref', 'Internet', 'Serveur de référence (Cloudflare, 1.1.1.1)'),
        @('srv', 'Serveur du jeu', $(if ($s.Server) { "$(if ($s.Server.Owner) { $s.Server.Owner } else { $s.Server.Ip }), $($s.Server.Proto.ToUpper()) port $($s.Server.Port)$(if ($s.Server.ViaProxy) { ', mesuré au dernier routeur (le serveur ne répond pas au ping)' })" } else { 'Non repéré' }))
    )
    foreach ($seg in $segs) {
        $x = $s.Stats.($seg[0])
        if ($seg[0] -eq 'srv' -and -not $s.Server -and $s.Quick) { continue }
        $status = Get-SegStatus $x $seg[0]
        # Les routeurs des fournisseurs répondent au ping quand ils ont le temps : si Internet va bien, ce n'est pas un vrai souci
        if ($seg[0] -eq 'isp' -and $status -in 'bad', 'warn' -and (Get-SegStatus $s.Stats.ref 'ref') -eq 'ok') { $status = 'ok' }
        $b = New-Object System.Windows.Controls.Border
        $b.Background = Get-Brush '#1E232D'
        $b.BorderBrush = Get-Brush $(if ($status -eq 'none') { '#5B6475' } else { $Colors[$status] })
        $b.BorderThickness = New-Thickness 3 0 0 0
        $b.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $b.Padding = New-Thickness 16 10 16 10
        $b.Margin = New-Thickness 0 0 0 6
        $g = New-Grid @('*', 'Auto')
        $l = New-Object System.Windows.Controls.StackPanel
        [void]$l.Children.Add((New-Text $seg[1] 14 '#FFFFFF' -Semi))
        $sub = New-Text $seg[2] 12 '#9AA3B2'
        $sub.TextTrimming = 'CharacterEllipsis'; $sub.TextWrapping = 'NoWrap'
        [void]$l.Children.Add($sub)
        Add-ToGrid $g $l 0
        $r = New-Object System.Windows.Controls.StackPanel
        $r.Margin = New-Thickness 16 0 0 0; $r.VerticalAlignment = 'Center'
        if ($x -and -not $x.Dead) {
            $v = New-Text (Format-Ms $x.Med) 15 '#FFFFFF' -Semi; $v.HorizontalAlignment = 'Right'
            [void]$r.Children.Add($v)
            $d = New-Text "variation $($x.Jit) ms, pics $(Format-Ms $x.P95), perdus $($x.Loss) %" 11.5 $(if ($status -in 'bad', 'warn') { $Colors[$status] } else { '#9AA3B2' }); $d.HorizontalAlignment = 'Right'
            [void]$r.Children.Add($d)
        } else {
            [void]$r.Children.Add((New-Text $(if ($x) { 'Ne répond pas au ping' } else { 'Pas mesuré' }) 12 '#5B6475'))
        }
        Add-ToGrid $g $r 1
        $b.Child = $g
        [void]$body.Children.Add($b)
    }
    [void]$body.Children.Add((New-SectionTitle 'LE PING PENDANT LA MESURE'))
    [void]$body.Children.Add((New-LagChart $s))

    [void]$body.Children.Add((New-SectionTitle 'CE QUE NEXO A TROUVÉ'))
    foreach ($fd in @($s.Diag.Findings)) {
        $acts = @()
        foreach ($a in @($fd.Actions)) {
            if (-not $a -or -not $a.Label) { continue }
            # Les actions sont recréées depuis le diagnostic (les blocs de code ne sont pas gardés dans le fichier)
            $live = @((Get-LagDiagnosis $s).Findings | Where-Object { $_.Title -eq $fd.Title })[0]
            foreach ($la in @($live.Actions)) { if ($la.Label -eq $a.Label) { $acts += $la } }
        }
        $c = New-SecurityCard @{ Status = [string]$fd.Status; Title = $fd.Title; Detail = $fd.Detail; Items = @($fd.Tips | Where-Object { $_ }); ShowAll = $true; Actions = $acts; NoRefresh = $true }
        $c.Margin = New-Thickness 0 6 0 0
        [void]$body.Children.Add($c)
    }
    $n = New-Text "Pings envoyés deux fois par seconde vers chaque étape. « Variation » : de combien le ping change d'un envoi à l'autre (au dessus de 10 ms, ça se sent en jeu). « Pics » : 95 % des pings étaient en dessous. $(if (-not $s.Etw) { 'Sans les droits administrateur, le serveur du jeu est repéré par sa connexion TCP seulement.' })" 11.5 '#5B6475'
    $n.Margin = New-Thickness 0 12 0 0
    [void]$body.Children.Add($n)
}
