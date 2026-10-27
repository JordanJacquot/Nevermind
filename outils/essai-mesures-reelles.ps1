# Essai en vrai des mesures continues de l'Aide (plan Rust, étape 1b), sur la machine jetable de GitHub, EN
# ADMINISTRATEUR : l'Aide PowerShell (NevermindAide.ps1) puis l'Aide Rust (NevermindAide.exe) font chacune les vraies
# mesures, avec les vrais appels de Windows :
#   - trafic.tcp : comptage accepté par Windows, octets d'une vraie connexion (github.com:443) comptés ;
#   - flux.* : vraie trace réseau ETW, une vraie question DNS (UDP, 1.1.1.1:53) vue avec un âge en millisecondes
#     plausible (le piège FILETIME / QPC : un âge négatif ou de milliards de ms fait échouer l'essai) ;
#   - fps.* : vrai PresentMon (signé par Intel) lancé par l'Aide sur le client, sans refus de Windows.
# Chaque Aide tourne dans sa propre copie ; le client qui lui parle est « Mindset.exe » de cette copie (une copie de
# powershell.exe : en administrateur, l'Aide ne répond qu'à lui). Jamais sur le PC de Jordan : il faut les droits
# administrateur, et l'essai ouvre une session de trace et lance PresentMon.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\essai-mesures-reelles.ps1 -Exe <NevermindAide.exe construit>
#
# Code de sortie : 0 si les deux Aides passent tout, 1 sinon.
param([Parameter(Mandatory)][string]$Exe, [string]$Client)
$ErrorActionPreference = 'Stop'

# --- La partie client : lancée sous le nom Mindset.exe, elle fait le scénario et écrit ce qu'elle a vu (JSON)
if ($Client) {
    $c = Get-Content -LiteralPath $Client -Raw | ConvertFrom-Json
    function Send-Demande([string]$Action, $P) {
        $t = New-Object IO.Pipes.NamedPipeClientStream('.', $c.Canal, [IO.Pipes.PipeDirection]::InOut)
        try {
            $t.Connect(10000)
            $enc = New-Object Text.UTF8Encoding($false)
            $w = New-Object IO.StreamWriter($t, $enc, 65536, $true); $w.WriteLine((ConvertTo-Json -InputObject @{ a = $Action; p = $P } -Compress -Depth 5)); $w.Flush()
            (New-Object IO.StreamReader($t, $enc, $false, 65536, $true)).ReadLine() | ConvertFrom-Json
        } finally { $t.Dispose() }
    }
    $r = [ordered]@{ Pid = $PID; Controles = New-Object Collections.Generic.List[string] }
    function Test-Ok([bool]$Ok, [string]$Texte) { $r.Controles.Add("$(if ($Ok) { 'OK' } else { 'ECHEC' }) $Texte") }
    try {
        # 1. Compteurs TCP : comptage demandé, puis des octets échangés, puis relus
        $tcp = New-Object Net.Sockets.TcpClient('github.com', 443)
        $port = $tcp.Client.LocalEndPoint.Port
        $a = Send-Demande 'trafic.tcp' @{}
        $s = $tcp.GetStream(); $o = [Text.Encoding]::ASCII.GetBytes("HEAD / HTTP/1.1`r`nHost: github.com`r`n`r`n"); $s.Write($o, 0, $o.Length)
        Start-Sleep -Seconds 2
        $b = Send-Demande 'trafic.tcp' @{}
        $tcp.Dispose()
        $l = @($b.r.Lignes | Where-Object { ($_ -split '\|')[0] -eq "$PID" -and ($_ -split '\|')[3] -eq "$port" })
        Test-Ok ($a.ok -and $b.ok -and $b.r.CompteursOk -eq $true) "trafic.tcp : comptage accepté ($($b.r.CompteursOk))"
        $x = if ($l.Count) { $l[0] -split '\|' } else { @() }
        Test-Ok ($l.Count -eq 1 -and [long]$x[5] -gt 0) "trafic.tcp : connexion vue, octets envoyés comptés ($($l -join ' ; '))"
        # 2. Trace réseau : une question DNS en UDP, vue avec un âge plausible
        $d = Send-Demande 'flux.demarrer' @{}
        Test-Ok $d.ok "flux.demarrer : $($d.e)"
        Start-Sleep -Seconds 2
        $u = New-Object Net.Sockets.UdpClient
        $q = [byte[]](0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 6) + [Text.Encoding]::ASCII.GetBytes('github') + [byte[]](3) + [Text.Encoding]::ASCII.GetBytes('com') + [byte[]](0, 0, 1, 0, 1)
        [void]$u.Send($q, $q.Length, '1.1.1.1', 53)
        $u.Client.ReceiveTimeout = 5000; $ip = $null; try { [void]$u.Receive([ref]$ip) } catch {}
        $u.Dispose()
        Start-Sleep -Seconds 3
        $f = Send-Demande 'flux.lire' @{}
        $v = @($f.r | Where-Object { $_ -like "$PID|udp|1.1.1.1|53|*" })
        $y = if ($v.Count) { $v[0] -split '\|' } else { @() }
        Test-Ok ($f.ok -and $v.Count -eq 1 -and [long]$y[4] -ge 1) "flux.lire : question DNS vue ($($v -join ' ; ') ; $(@($f.r).Count) échanges)"
        Test-Ok ($v.Count -eq 1 -and [long]$y[11] -ge 0 -and [long]$y[11] -lt 60000) "flux.lire : âge du dernier paquet plausible, en heure système ($($y[11]) ms)"
        $w2 = Send-Demande 'flux.veiller' @{}
        Test-Ok ($w2.ok -and (Send-Demande 'flux.lire' @{}).ok) 'flux.veiller : la trace continue'
        Test-Ok (Send-Demande 'flux.arreter' @{}).ok 'flux.arreter'
        Test-Ok (-not (Send-Demande 'flux.lire' @{}).ok) 'flux.lire après l''arrêt : la trace ne tourne plus'
        # 3. PresentMon sur ce client : lancé sans refus de Windows
        $p = Send-Demande 'fps.demarrer' @{ Pid = $PID }
        Test-Ok ($p.ok -and $p.r -eq 'ok') "fps.demarrer : $($p.r)$($p.e)"
        $texte = ''
        for ($i = 0; $i -lt 16; $i++) { Start-Sleep -Milliseconds 250; $texte += [string](Send-Demande 'fps.lire' @{}).r }
        Test-Ok ($texte -notmatch 'access denied|Performance Log Users') "fps.lire : pas de refus de Windows ($(($texte -split "`n" | Select-Object -First 3) -join ' / '))"
        Test-Ok (Send-Demande 'fps.arreter' @{}).ok 'fps.arreter'
    } catch { Test-Ok $false "client interrompu : $($_.Exception.Message)" }
    [IO.File]::WriteAllText($c.Resultat, (ConvertTo-Json -InputObject $r -Depth 5), (New-Object Text.UTF8Encoding($false)))
    exit 0
}

# --- La partie essai : les deux copies, leurs Aides, leurs clients
$racine = Split-Path $PSScriptRoot -Parent
$moi = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$admin = (New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { Write-Host 'Cet essai demande les droits administrateur (machine de GitHub seulement).' -ForegroundColor Red; exit 1 }
$psExe = Join-Path ([Environment]::GetFolderPath('System')) 'WindowsPowerShell\v1.0\powershell.exe'
$essai = Join-Path ([IO.Path]::GetTempPath()) "nevermind-mesures-reelles-$PID"
$echecs = 0
foreach ($sorte in 'PowerShell', 'Rust') {
    $copie = Join-Path $essai $sorte
    [void][IO.Directory]::CreateDirectory($copie)
    Copy-Item -LiteralPath (Join-Path $racine 'Nevermind\fichiers') -Destination $copie -Recurse
    Copy-Item -LiteralPath $psExe -Destination (Join-Path $copie 'Mindset.exe')
    $d = ([IO.Path]::GetFullPath($copie).TrimEnd('\')).ToLowerInvariant()
    $h = [Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($d))
    $canal = "Nevermind-Aide-$moi-$(-join ($h[0..5] | ForEach-Object { $_.ToString('x2') }))"
    $aide = if ($sorte -eq 'Rust') {
        Copy-Item -LiteralPath $Exe -Destination (Join-Path $copie 'NevermindAide.exe')
        Start-Process -FilePath (Join-Path $copie 'NevermindAide.exe') -ArgumentList '-Sid', $moi -PassThru
    } else {
        Start-Process -FilePath $psExe -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$copie\fichiers\NevermindAide.ps1`"", '-Sid', $moi -WindowStyle Hidden -PassThru
    }
    try {
        $fin = (Get-Date).AddSeconds(60)
        while ((Get-Date) -lt $fin -and -not [IO.Directory]::GetFiles('\\.\pipe\').Contains("\\.\pipe\$canal")) { Start-Sleep -Milliseconds 200 }
        $config = Join-Path $copie 'client.json'
        $resultat = Join-Path $copie 'resultat.json'
        [IO.File]::WriteAllText($config, (ConvertTo-Json -InputObject @{ Canal = $canal; Resultat = $resultat }), (New-Object Text.UTF8Encoding($false)))
        $cl = Start-Process -FilePath (Join-Path $copie 'Mindset.exe') -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$PSCommandPath`"", '-Exe', 'x', '-Client', "`"$config`"" -WindowStyle Hidden -PassThru -Wait
        $r = Get-Content -LiteralPath $resultat -Raw | ConvertFrom-Json
        Write-Host "Aide $sorte (client $($r.Pid)) :"
        foreach ($l in @($r.Controles)) {
            if ($l -like 'OK *') { Write-Host "  $l" -ForegroundColor Green } else { Write-Host "  $l" -ForegroundColor Red; $echecs++ }
        }
        if (@($r.Controles).Count -lt 10) { Write-Host "  $(@($r.Controles).Count) contrôles seulement (10 attendus)" -ForegroundColor Red; $echecs++ }
    } catch {
        Write-Host "Aide $sorte : essai interrompu ($($_.Exception.Message))" -ForegroundColor Red; $echecs++
    } finally {
        try { if (-not $aide.HasExited) { $aide.Kill() } } catch {}
    }
}
Start-Sleep -Milliseconds 500
Remove-Item -LiteralPath $essai -Recurse -Force -ErrorAction SilentlyContinue
if ($echecs) { Write-Host "$echecs contrôle(s) en échec." -ForegroundColor Red; exit 1 }
Write-Host 'Les deux Aides font les vraies mesures.' -ForegroundColor Green
exit 0
