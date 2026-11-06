# Les juges du bloc « trafic » de Nevermind en Rust (Lag en ligne, page Trafic, ce que Windows envoie à Microsoft) :
# les fonctions de la version PowerShell (lag.ps1, trafic.ps1, microsoft.ps1, ecran-jeux.ps1, ecran-panneau.ps1)
# chargées en mode test, nourries d'entrées inventées (tirées au hasard avec une graine fixe, plus des cas limites),
# et leurs sorties enregistrées dans rust\nevermind-trafic\tests\juges\*.json. Les tests Rust rejouent ces cas.
#
# Rien ne touche le vrai PC : aucune lecture du réseau, aucun ping, aucun fichier de Nevermind, aucun réglage. Les
# commandes qui pourraient le faire sont remplacées par des fonctions qui s'arrêtent net. Les fenêtres WPF du panneau
# sont créées en mémoire, jamais montrées.
#
# À lancer depuis l'outil PowerShell (STA) : & 'outils\exporter-juges-trafic.ps1'

$ErrorActionPreference = 'Stop'
$env:NEVERMIND_TEST = '1'
$racine = Split-Path $PSScriptRoot -Parent
$fichiers = Join-Path $racine 'Nevermind\fichiers'
$modules = Join-Path $fichiers 'modules'
$sortie = Join-Path $racine 'rust\nevermind-trafic\tests\juges'
[void](New-Item -ItemType Directory -Force $sortie)

# ---------------------------------------------------------------------------
# Sécurité : rien ne doit pouvoir lire ou modifier le vrai PC
# ---------------------------------------------------------------------------
foreach ($c in 'Get-CimInstance', 'Get-WmiObject', 'Get-Process', 'Stop-Process', 'Start-Process', 'Set-ItemProperty', 'New-ItemProperty',
               'Remove-ItemProperty', 'Invoke-Aide', 'Set-Setting', 'Start-FluxReseau', 'Stop-FluxReseau', 'Show-Notify', 'taskkill', 'taskkill.exe',
               'Get-DnsClientCache', 'Invoke-WebRequest', 'Invoke-RestMethod', 'Set-Reg', 'Get-Service', 'Get-LocalUser', 'Get-RegValue', 'Open-Url') {
    Set-Item -Path "function:script:$c" -Value ([scriptblock]::Create("throw 'Export : $c est interdit.'"))
}
$script:Journal = New-Object System.Collections.Generic.List[string]
function script:Write-Log([string]$Message) { $script:Journal.Add($Message) }
$script:Reglages = @{}
function script:Get-Setting([string]$Name, $Default = $null) { if ($script:Reglages.ContainsKey($Name)) { $script:Reglages[$Name] } else { $Default } }
$DataDir = Join-Path $env:TEMP "nevermind-juges-trafic-$PID"

function Get-Definitions([string]$Fichier, [string[]]$Noms) {
    $tok = $null; $err = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseInput([IO.File]::ReadAllText($Fichier, [Text.Encoding]::UTF8), $Fichier, [ref]$tok, [ref]$err)
    if ($err) { throw "$Fichier n'est pas lisible : $($err[0].Message)" }
    $textes = foreach ($n in $Noms) {
        $def = if ($n.StartsWith('$')) {
            $ast.Find({ param($x) $x -is [System.Management.Automation.Language.AssignmentStatementAst] -and $x.Left.Extent.Text -eq $n -and $x.Parent -eq $ast.EndBlock }, $true)
        } else {
            $ast.Find({ param($x) $x -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $x.Name -eq $n }, $true)
        }
        if (-not $def) { throw "$n introuvable dans $([IO.Path]::GetFileName($Fichier))" }
        $def.Extent.Text
    }
    $textes -join "`n"
}
function Import-Defs([string]$Fichier, [string[]]$Noms) { . ([scriptblock]::Create((Get-Definitions (Join-Path $modules $Fichier) $Noms))) }

Add-Type -AssemblyName WindowsBase, PresentationCore, PresentationFramework
. ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'lag.ps1') @('$LagColors', '$LagNames', 'Test-LagMeasure', 'Test-LagConnexionOnly', 'Set-LagDiag', 'Format-ServerPlace',
    '$LagParticulierProprio', '$LagParticulierHote', 'Test-AdresseParticulier', 'Get-PingStats', 'Compress-PingSeries', 'Get-LagSpikeLinks', 'Get-SegStatus', 'Format-Ms',
    'Get-LagDiagnosis', 'Get-LagSessionLine', 'Stop-LagSession', 'Get-JobResult', 'Receive-LagOwners', 'Receive-LagGeo', 'Show-LagSession', 'New-LagChart', 'Get-LagSessions', 'Update-LagSession',
    'Remove-LagServer', 'Add-LagPair'))))
. ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'trafic.ps1') @('$LolBins', '$RemoteTools', '$SusPorts', '$UploadOk', '$PortNames', 'Format-Bytes', 'Test-PrivateIp',
    'New-TrafficState', 'Get-TrafficApp', 'Update-Traffic', 'Compress-TrafficDests', 'Get-TrafficDestCount', 'Get-TrafficMarks', 'Test-GamePath', 'Test-SuitePath', 'Get-TrafficAlerts',
    '$CountryNames', '$OwnerTypes', 'ConvertTo-IpHex', 'Get-ServerOwner', 'Format-Country', 'Get-OwnerLabel', '$DataTypes', 'Get-DestType', '$LocalType', 'Get-AppDestGroups',
    'Get-AppDataTypes', 'New-TrafficTypeBlock', 'Get-AppSigLabel', 'Get-DestName', 'Get-TrafficAlertActions', 'Show-TrafficApp', 'Add-TrafficSummary'))))
. ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'microsoft.ps1') @('$MsDomains', '$MsEndpoints', 'Get-MsService', 'Get-MsTraffic', 'Get-PrivacyItems', 'Get-PcIdentifiers',
    'New-PrivacyRow', 'Show-WindowsPrivacy', '$CdmKey', '$CdmValues', '$DiagKey', '$DiagPolicyKey', 'Get-DiagLevel', 'Get-RegNum'))))
. ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'jeu.ps1') @('Format-PlayTime', '$OverlayCorners'))))
. ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'ecran-pc.ps1') @('$TonsStatut', 'Get-TonKit'))))
. ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'ecran-jeux.ps1') @('Get-PagesJeux'))))
. ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'interface.ps1') @('$Colors'))))

function Save-Juge([string]$Nom, $Objet) {
    $json = ConvertTo-Json -InputObject $Objet -Depth 30 -Compress
    [IO.File]::WriteAllText((Join-Path $sortie $Nom), $json, (New-Object Text.UTF8Encoding $false))
    Write-Output "$Nom : $([math]::Round($json.Length / 1KB)) Ko"
}

$rnd = New-Object Random 2026
function Pick($Liste) { $l = @($Liste); $l[$rnd.Next($l.Count)] }
function Chance([double]$P) { $rnd.NextDouble() -lt $P }

$script:ServerCache = New-Object System.Collections.ArrayList
function script:Get-ServerCache { , $script:ServerCache }
function script:Save-ServerCache { }
function New-Entree([string]$De, [string]$A, [string]$O, [string]$N, [string]$C = 'FR') { @{ S = (ConvertTo-IpHex $De); E = (ConvertTo-IpHex $A); O = $O; N = $N; C = $C; D = '2026-10-22' } }

# ===========================================================================
# 1. Petites fonctions
# ===========================================================================
$cas = New-Object System.Collections.ArrayList
function Add-Cas([string]$F, $E, $S) { [void]$cas.Add([ordered]@{ f = $F; e = $E; s = $S }) }

$ips = @('10.0.0.1', '127.0.0.1', '192.168.1.254', '169.254.3.4', '172.16.0.1', '172.15.255.255', '172.31.1.1', '172.32.0.1', '0.0.0.0', '224.0.0.251', '239.255.255.250',
    '240.1.2.3', '255.255.255.255', '223.5.5.5', '35.207.109.101', '3.40.138.1', '1.1.1.1', '::1', '::', 'fe80::1', 'fc00::1', 'fd12:3456::1', 'ff02::1', '2a01:cb00::1',
    '100.64.0.1', '8.8.8.8', '', 'abc', '10', '192.169.0.1', '2001:db8::ff', 'FE80::1', 'FF02::2', '1.2.3.4.5', '22.0.0.1', '2.25.0.1', '25.1.1.1', '250.1.1.1', '256.1.1.1')
for ($i = 0; $i -lt 150; $i++) { $ips += "$($rnd.Next(0, 256)).$($rnd.Next(0, 256)).$($rnd.Next(0, 256)).$($rnd.Next(0, 256))" }
foreach ($ip in $ips) { Add-Cas 'Test-PrivateIp' $ip (Test-PrivateIp $ip) }
foreach ($ip in $ips) { Add-Cas 'ConvertTo-IpHex' $ip (ConvertTo-IpHex $ip) }

$octets = @(0, 1, 512, 1023, 1024, 1025, 1535, 1536, 1048575, 1048576, 1572864, 10485760, 1073741823, 1073741824, 1610612736, 5e12, 999.6, 1023.5, -5, 0.4, 0.5, 1.5, 2.5, (1048576 * 1.05), (1048576 * 1.25), (1048576 * 1.35))
for ($i = 0; $i -lt 150; $i++) { $octets += [math]::Round([math]::Pow(10, $rnd.NextDouble() * 11), $rnd.Next(0, 3)) }
foreach ($o in $octets) { Add-Cas 'Format-Bytes' $o (Format-Bytes $o) }

$ms = @($null, -1, 0, 0.4, 0.5, 1.5, 2.5, 12.5, 13.5, 999.5, 1000, 1234.5, 12345.678, 1e6)
for ($i = 0; $i -lt 60; $i++) { $ms += [math]::Round($rnd.NextDouble() * 2000, $rnd.Next(0, 3)) }
foreach ($m in $ms) { Add-Cas 'Format-Ms' $m (Format-Ms $m) }

$durees = @(0, 1, 30, 89, 89.5, 90, 91, 119, 150, 3599, 3600, 3629, 3630, 3631, 5400, 7199, 7200, 86400, (90 * 60 + 30))
for ($i = 0; $i -lt 80; $i++) { $durees += [math]::Round($rnd.NextDouble() * 20000, $rnd.Next(0, 2)) }
foreach ($d in $durees) { Add-Cas 'Format-PlayTime' $d (Format-PlayTime $d) }

$pays = @('', 'FR', 'fr', 'US', 'DE', 'XX', 'EU', 'united states', 'USA', 'France', 'GERMANY', 'royaume-uni', 'NEW ZEALAND', 'côte d''ivoire', 'A', 'ABC', 'éTATS-unis', 'south  korea', 'hong-kong')
foreach ($p in $pays) { Add-Cas 'Format-Country' $p (Format-Country $p) }
foreach ($e in @(@{ O = 'Valve Corporation'; N = 'VALVE'; C = 'US' }, @{ O = ''; N = 'GOOGLE-CLOUD'; C = '' }, @{ O = 'OVH SAS'; N = 'OVH'; C = 'france' }, @{ O = 'Riot'; N = ''; C = 'ZZ' }, @{ O = ''; N = ''; C = '' })) {
    Add-Cas 'Get-OwnerLabel' $e (Get-OwnerLabel $e)
}
foreach ($s in @($null, @{ Pays = ''; Ville = 'Paris' }, @{ Pays = 'DE'; Ville = '' }, @{ Pays = 'DE'; Ville = 'Frankfurt am Main' }, @{ Pays = 'nl'; Ville = 'Amsterdam' }, @{ Pays = 'Japan'; Ville = '' })) {
    Add-Cas 'Format-ServerPlace' $s (Format-ServerPlace $s)
}

# Séries de pings [t, ms...] : pertes, pics, séries trop courtes
for ($i = 0; $i -lt 220; $i++) {
    $n = Pick @(0, 1, 3, 7, 8, 9, 12, 20, 40, 80, 200)
    $base = $rnd.Next(1, 120); $perte = Pick @(0, 0, 0.02, 0.1, 0.5, 0.85, 1)
    $a = New-Object System.Collections.Generic.List[double]
    $t = 0.0
    for ($k = 0; $k -lt $n; $k++) {
        $t += 0.5 + [math]::Round($rnd.NextDouble() * 0.05, 3)
        $v = if (Chance $perte) { -1 } elseif (Chance 0.05) { $base + $rnd.Next(30, 400) } else { [math]::Max(0.5, $base + [math]::Round(($rnd.NextDouble() - 0.5) * (Pick @(0, 2, 10, 40)), 1)) }
        $a.Add([math]::Round($t, 3)); $a.Add($v)
    }
    $arr = $a.ToArray()
    Add-Cas 'Get-PingStats' $arr (Get-PingStats $arr)
    $step = Pick @(1.0, 1.0, 2.0, 3.0, 0.5)
    Add-Cas 'Compress-PingSeries' @{ a = $arr; step = $step } @(Compress-PingSeries $arr $step)
}

# Pics en même temps que la box ou un téléchargement
for ($i = 0; $i -lt 120; $i++) {
    $sp = @(for ($k = 0; $k -lt $rnd.Next(0, 25); $k++) { [math]::Round($rnd.NextDouble() * 300, 2) })
    $gw = @(for ($k = 0; $k -lt $rnd.Next(0, 25); $k++) { [math]::Round($rnd.NextDouble() * 300, 2) })
    $bg = @(for ($k = 0; $k -lt $rnd.Next(0, 40); $k++) { @{ T = [math]::Round($rnd.NextDouble() * 300, 1); Rate = (Pick @(0, 1000, 600KB, 2MB, 500KB, 512001)); Name = (Pick @('', 'Steam', 'Windows : Windows Update', 'OneDrive')); NameRate = $rnd.Next(0, 5MB) } })
    Add-Cas 'Get-LagSpikeLinks' @{ sp = $sp; gw = $gw; bg = $bg } (Get-LagSpikeLinks $sp $gw $bg)
}

function New-StatAuHasard {
    if (Chance 0.15) { return $null }
    if (Chance 0.1) { return @{ N = 20; Loss = 90.0; Dead = $true; Med = -1; P95 = -1; Jit = 0; Max = -1; Spikes = 0 } }
    $med = [math]::Round($rnd.NextDouble() * (Pick @(5, 30, 120)), 1)
    @{ N = $rnd.Next(8, 500); Loss = (Pick @(0, 0, 0.4, 0.5, 0.9, 1, 1.9, 2, 2.9, 3, 7.5)); Dead = $false; Med = $med; P95 = [math]::Round($med + (Pick @(0, 5, 19.9, 20, 34, 35, 36, 49, 50, 79, 80, 120)), 1)
       Jit = (Pick @(0, 1.2, 3.9, 4, 7.9, 8, 9.9, 10, 19.9, 20, 31)); Max = [math]::Round($med + 200, 1); Spikes = $rnd.Next(0, 30) }
}
for ($i = 0; $i -lt 150; $i++) { $st = New-StatAuHasard; $k = Pick @('gw', 'ref', 'srv', 'isp'); Add-Cas 'Get-SegStatus' @{ S = $st; kind = $k } (Get-SegStatus $st $k) }

# Particuliers (box d'un autre joueur) ou serveurs
$proprios = @(@('Comcast Cable Communications, LLC', 'CABLE-COMCAST-12'), @('Deutsche Telekom AG', 'DTAG-DIAL25'), @('Bouygues Telecom SA', 'FR-BOUYGTEL-FTTH'),
    @('Virgin Media Limited', 'VM-BROADBAND'), @('i3D.net B.V', 'I3D-NET'), @('OVH SAS', 'OVH'), @('Electronic Arts, Inc.', 'EA-NET'), @('Microsoft Corporation', 'MSFT'),
    @('Valve Corporation', 'VALVE-L1'), @('Amazon Data Services Ireland Limited', 'AMAZON-DUB'), @('Wanadoo France Technical', 'IP2000-ADSL-BAS'), @('Administrative', 'FR-PROXAD-ADSL'),
    @('', 'PPPOE-POOL'), @('Telefonica', 'RIMA-DYNAMIC'), @('Free SAS', 'subscribers-pool'), @('abonnes Orange', ''), @('Cabled', 'XCABLEX'), @('', ''), @('Bas Networks', 'X'))
$hotes = @('', 'lfbn-nan-1-234-56.w86-200.abo.wanadoo.fr', 'ec2-3-40-137-124.eu-west-1.compute.amazonaws.com', 'lns-bzn-21-78-192-3-4.adsl.proxad.net', 'host.dyn.example.net',
    'cpe-1-2-3-4.cable.net', 'srv1.valve.net', 'pool-71-1.verizon.net', 'abo.example', 'x.subs.proxad.net', 'customer123.isp.com', 'ppp-1.bbox.fr', 'a.t-ipconnect.de', 'dsl5.foo', 'ftth', 'abonnement.fr')
for ($i = 0; $i -lt 160; $i++) {
    $script:ServerCache.Clear(); $script:Traffic = $null
    $p = Pick $proprios
    $avecProprio = Chance 0.6
    if ($avecProprio) { [void]$script:ServerCache.Add((New-Entree '100.0.0.0' '100.0.0.255' $p[0] $p[1])) }
    $h = Pick $hotes
    $script:LagGeo = if (Chance 0.6) { @{ '100.0.0.7' = @{ Pays = 'FR'; Ville = 'Metz'; Hote = $h } } } else { $null }
    Add-Cas 'Test-AdresseParticulier' @{ proprio = $(if ($avecProprio) { @{ O = $p[0]; N = $p[1] } } else { $null }); hote = $(if ($script:LagGeo) { $h } else { $null }) } (Test-AdresseParticulier '100.0.0.7')
}
$script:LagGeo = $null; $script:ServerCache.Clear()

# Propriétaire d'une adresse (plages de l'annuaire, IPv4 et IPv6)
[void]$script:ServerCache.Add((New-Entree '35.207.0.0' '35.207.255.255' 'Google LLC' 'GOOGLE-CLOUD' 'US'))
[void]$script:ServerCache.Add((New-Entree '63.176.0.0' '63.179.255.255' 'Amazon.com, Inc.' 'AMAZO-4' 'US'))
[void]$script:ServerCache.Add((New-Entree '2a01:cb00::' '2a01:cbff:ffff:ffff:ffff:ffff:ffff:ffff' 'Orange' 'FR-ORANGE' 'FR'))
[void]$script:ServerCache.Add((New-Entree '90.0.0.0' '90.127.255.255' '' 'IP2000-ADSL-BAS' 'FR'))
foreach ($ip in @('35.207.0.0', '35.207.255.255', '35.208.0.0', '35.206.255.255', '63.178.238.177', '2a01:cb00::1', '2a01:cc00::1', '90.1.2.3', '1.1.1.1', 'abc', '')) {
    $script:Traffic = $null
    $o = Get-ServerOwner $ip
    Add-Cas 'Get-ServerOwner' $ip $(if ($o) { Get-OwnerLabel $o } else { $null })
}
$cacheOwner = @($script:ServerCache)
$script:ServerCache.Clear()

# Type des données d'après le nom du serveur, le port, les volumes, le propriétaire
$nomsServeurs = @('', 'ad.doubleclick.net', 'www.google-analytics.com', 'v10.events.data.microsoft.com', 'settings-win.data.microsoft.com', 'smartscreen-prod.microsoft.com', 'ocsp.digicert.com',
    'login.live.com', 'auth.riotgames.com', 'relay.anydesk.com', 'api.anthropic.com', 'claude.ai', 'chatgpt.com', 'onedrive.live.com', 'gateway.discord.gg', 'rr3---sn.googlevideo.com',
    'steamcommunity.com', 'cdn.steamcontent.com', 'download.windowsupdate.com', 'ec2-3-1.compute.amazonaws.com', 'xyz.1e100.net', 'random.example.org', 'x.cloudfront.net', 'stats.foo.com',
    'beacon.foo', 'feedback.foo', 'metrics.icloud.com', 'teams.microsoft.com', 'ADS.EXAMPLE.COM', 'example.ads', 'bads.example', 'www.spotify.com', 'contentx.net', 'pki.goog', 'accounts.google.com')
$appsNoms = @('chrome', 'anydesk', 'TeamViewer_Service', 'steam', 'malware', 'rustdesk')
for ($i = 0; $i -lt 300; $i++) {
    $n = Pick $nomsServeurs
    $port = Pick @(443, 443, 80, 53, 853, 27015, 4444, 3478, 0)
    $out = Pick @(0, 100, 1MB, 10MB, 10MB + 1, 50MB, 300MB)
    $in = Pick @(0, 100, 1MB, 1MB + 1, 3MB, 50MB, 400MB)
    $an = Pick $appsNoms
    $ow = if (Chance 0.4) { $pp = Pick $proprios; @{ O = $pp[0]; N = $pp[1] } } elseif (Chance 0.2) { @{ O = (Pick @('Valve Corporation', 'Discord Inc.', 'Anthropic, PBC', 'TeamViewer Germany', 'Netflix', 'Criteo SA', 'Riot Games')); N = '' } } else { $null }
    $t = Get-DestType $n $port $out $in @{ Name = $an } $ow
    Add-Cas 'Get-DestType' @{ nom = $n; port = $port; out = $out; in = $in; app = $an; proprio = $ow } $t.Id
}
foreach ($n in $nomsServeurs + @('watson.telemetry.microsoft.com', 'arc.msn.com', 'www.msn.com', 'client.wns.windows.com', 'activity.windows.com', 'tlu.dl.delivery.mp.microsoft.com',
        'displaycatalog.mp.microsoft.com', 'www.bing.com', 'xboxlive.com', 'wdcp.microsoft.com', 'edge.microsoft.com', 'www.msftconnecttest.com', 'outlook.office365.com', 'BING.NET')) {
    $m = Get-MsService $n
    Add-Cas 'Get-MsService' $n $(if ($m) { @{ Label = $m.Label; Text = $m.Text } } else { $null })
}

$pf = if ($env:ProgramW6432) { $env:ProgramW6432 } else { $env:ProgramFiles }
$chemins = @('', 'C:\Program Files (x86)\Steam\steamapps\common\Game\game.exe', 'D:\Epic Games\Fortnite\f.exe', 'C:\Riot Games\x.exe', 'C:\Battle.net\b.exe', 'C:\Ubisoft Game Launcher\games\a.exe',
    'C:\EA Games\x.exe', 'C:\GOG Galaxy\Games\x.exe', 'C:\XboxGames\x.exe', 'C:\Netmarble Game\x.exe', 'C:\jeu\Game-Win64-Shipping.exe', 'C:\jeu\Binaries\Win64\x.exe', 'C:\jeu\x.exe',
    'D:\Mindset\Programmes\Mindset\Mindset.exe', 'D:\Mindset\Programmes\Nevermind\NevermindAide.exe', 'E:\Mindset\Programmes\Evermind\Mindset.exe', "$pf\Mindset\Mindset\Mindset.exe",
    "$pf\Mindset\Mindcare\Mindset.exe", 'D:\Mindset\Programmes\Nevermind\Désinstaller Nevermind.exe', 'C:\Users\Joueur\Mindset\Programmes\Faux\Faux.exe', 'D:\Mindset\Programmes\Mindset\Temp\virus.exe',
    'D:\Mindset\virus.exe', 'C:\Users\Joueur\Downloads\Mindset\Programmes\Mindset\Mindset.exe', 'c:\mindset\programmes\a\b.EXE', "$pf\Mindset\a\b\c.exe", "$($pf.ToLower())\mindset\x\y.exe",
    'C:\STEAMAPPS\COMMON\x.exe', 'D:\Mindset\Programmes\Mindset\Mindset.exe.bak')
foreach ($c in $chemins) { $script:GameIndex = $null; Add-Cas 'Test-GamePath' $c (Test-GamePath $c); Add-Cas 'Test-SuitePath' $c (Test-SuitePath $c) }
$script:GameIndex = @{ monjeu = @{ Game = 'Mon jeu'; Exes = @('c:\jeu\x.exe') } }
Add-Cas 'Test-GamePath+index' 'C:\Jeu\X.exe' (Test-GamePath 'C:\Jeu\X.exe')
$script:GameIndex = $null

foreach ($sg in @('Valid', 'HashMismatch', $null, 'Unknown', 'NotTrusted', 'NotSigned', 'NoPath', 'Error')) {
    foreach ($pub in @('', 'Valve Corp.')) { foreach ($path in @('', 'C:\a.exe')) {
        Add-Cas 'Get-AppSigLabel' @{ Sig = $sg; Publisher = $pub; Path = $path } @(Get-AppSigLabel @{ Sig = $sg; Publisher = $pub; Path = $path })
    } }
}

# ===========================================================================
# 2. Diagnostic d'une mesure du lag
# ===========================================================================
function New-MesureAuHasard {
    $stats = @{}
    foreach ($k in 'gw', 'ref', 'srv') { $s = New-StatAuHasard; if ($s) { $stats[$k] = $s } }
    $srv = if (Chance 0.7) { @{ Ip = (Pick @('35.207.1.2', '63.178.238.177', '185.25.182.1')); Port = (Pick @(3659, 27015, 443)); Proto = (Pick @('udp', 'udp', 'tcp')); ViaProxy = (Chance 0.2)
        Owner = (Pick @('', 'Valve Corporation (États-Unis)', 'Amazon.com, Inc. (États-Unis)')); Pays = (Pick @('', '', 'DE', 'FR', 'NL', 'XX')); Ville = (Pick @('', 'Frankfurt am Main', 'Paris')) } } else { $null }
    $apps = @(if (Chance 0.5) { for ($k = 0; $k -lt $rnd.Next(1, 4); $k++) { @{ Name = (Pick @('', 'Steam', 'Windows : Optimisation de livraison', 'Windows : Service de transfert intelligent en arrière-plan', 'OneDrive', 'Windows : Windows Update')); Rate = $rnd.Next(500KB, 30MB) } } })
    $spikes = $rnd.Next(0, 40)
    @{ Id = 'x'; Game = (Pick @('Test rapide', 'Battlefield 6', 'EA SPORTS FC 27')); Quick = (Chance 0.3); Seconds = (Pick @(30, 60, 61, 119, 120, 600, 3600, 7322)); Wifi = (Chance 0.5); Vpn = (Chance 0.1)
       Signal = (Pick @($null, $null, 35, 59, 60, 88)); SignalAvg = (Pick @($null, 70)); Band = (Pick @('', '2,4 GHz', '5 GHz', '2.4 GHz')); Channel = (Pick @('', '6', '11', '36', '149')); Etw = (Chance 0.6)
       Server = $srv; Isp = (Pick @('', 'Orange (France)')); Stats = $stats; Spikes = $spikes; LocalSpikes = $rnd.Next(0, $spikes + 1); BgSpikes = $rnd.Next(0, $spikes + 1); BgApps = $apps
       Gaps = (Pick @(0, 0, 1, 5, 30, 200)); MaxGap = (Pick @(0, 260, 900)) }
}
$diags = New-Object System.Collections.ArrayList
for ($i = 0; $i -lt 600; $i++) {
    $r = New-MesureAuHasard
    $d = Get-LagDiagnosis $r
    [void]$diags.Add([ordered]@{ e = $r; s = @{ Level = $d.Level; Title = $d.Title; Findings = @($d.Findings | ForEach-Object { @{ Status = $_.Status; Title = $_.Title; Detail = $_.Detail; Tips = @($_.Tips); Actions = @(@($_.Actions) | Where-Object { $_ } | ForEach-Object { $_.Label }) } }) }
        ligne = (Get-LagSessionLine $r) })
}
Save-Juge 'diagnostics.json' $diags

# ===========================================================================
# 3. Alertes du Trafic
# ===========================================================================
$alertesCas = New-Object System.Collections.ArrayList
$cheminsApps = @('C:\Program Files\Google\Chrome\Application\chrome.exe', 'C:\Users\Joueur\AppData\Local\Temp\x.exe', 'C:\Users\Joueur\Downloads\setup.exe', 'C:\Users\Public\p.exe', 'C:\ProgramData\svc.exe',
    'C:\ProgramData\Dossier\svc.exe', 'C:\Users\Joueur\AppData\Roaming\upd.exe', 'C:\Users\Joueur\AppData\Roaming\X\upd.exe', 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe',
    'C:\Program Files (x86)\Steam\steamapps\common\Jeu\jeu.exe', 'D:\Mindset\Programmes\Mindset\Mindset.exe', '', 'C:\Program Files\AnyDesk\AnyDesk.exe', 'C:\Téléchargements\a.exe', 'C:\Temp\b.exe')
for ($i = 0; $i -lt 250; $i++) {
    $apps = @{}
    $n = $rnd.Next(0, 7)
    for ($k = 0; $k -lt $n; $k++) {
        $path = Pick $cheminsApps
        $name = if ($path) { [IO.Path]::GetFileNameWithoutExtension($path) } else { Pick @('svchost', 'System', 'cmd', 'teamviewer_service') }
        if (Chance 0.2) { $name = Pick @('powershell', 'cmd', 'mshta', 'rundll32', 'AnyDesk', 'TeamViewer_Service', 'rustdesk', 'discord', 'obs64', 'steam', 'onedrive', 'qbittorrent') }
        $dest = @{}
        $nd = Pick @(0, 1, 3, 10, 151, 160)
        for ($j = 0; $j -lt $nd; $j++) {
            $priv = Chance 0.1
            $remote = if ($priv) { "192.168.1.$($rnd.Next(2, 250))" } else { "$($rnd.Next(1, 223)).$($rnd.Next(0, 255)).$($rnd.Next(0, 255)).$($rnd.Next(1, 255))" }
            $port = Pick @(443, 443, 80, 4444, 9001, 9050, 6667, 23, 8333, 3333, 27015)
            $dest["$remote|$port"] = @{ Remote = $remote; Port = $port; Private = $priv; Out = 0; In = 0 }
        }
        $key = if ($path) { $path.ToLower() } else { "nom:$($name.ToLower())$k" }
        $apps[$key] = @{ Key = $key; Name = $name; Path = $path; Title = $name; Sig = (Pick @('Valid', 'NotSigned', 'NotTrusted', 'HashMismatch', $null, 'Unknown', 'NoPath')); IsSelf = (Chance 0.05)
            Out = (Pick @(0, 1MB, 200MB, 201MB, 900MB)); In = (Pick @(0, 1MB, 60MB, 500MB)); Dest = $dest; Parent = (Pick @('', 'Code.exe', 'inconnu.exe')); ParentSig = (Pick @('', 'Valid', 'NotSigned')); ParentPub = (Pick @('', 'Microsoft Corporation'))
            IpsAnciennes = $(if (Chance 0.1) { $h = New-Object 'System.Collections.Generic.HashSet[string]'; for ($j = 0; $j -lt (Pick @(5, 150)); $j++) { [void]$h.Add("9.9.$([int]($j / 250)).$($j % 250)") }; , $h } else { $null })
            DestAnciennes = $null }
    }
    $trusted = @(foreach ($a in $apps.Values) { if (Chance 0.1) { "$($a.Key)|01/11/2026" } })
    $scanned = @(foreach ($a in $apps.Values) { if (Chance 0.1) { "$($a.Key)|$(Pick @('02/11/2026', ''))" } })
    $script:Reglages = @{ TrafficTrusted = $trusted; TrafficScanned = $scanned }
    $script:Traffic = @{ Apps = $apps }
    $al = @(Get-TrafficAlerts)
    [void]$alertesCas.Add([ordered]@{
        apps = @($apps.Values | ForEach-Object { $a = $_; [ordered]@{ Key = $a.Key; Name = $a.Name; Path = $a.Path; Sig = $a.Sig; IsSelf = $a.IsSelf; Out = $a.Out; In = $a.In; Parent = $a.Parent; ParentSig = $a.ParentSig; ParentPub = $a.ParentPub
            Dest = @($a.Dest.Values | ForEach-Object { @{ Remote = $_.Remote; Port = $_.Port; Private = $_.Private } }); IpsAnciennes = @(if ($a.IpsAnciennes) { $a.IpsAnciennes }) } })
        trusted = $trusted; scanned = $scanned
        s = @($al | ForEach-Object { [ordered]@{ Key = $_.App.Key; Level = $_.Level; Why = @($_.Why) } } | Sort-Object { @{ bad = 0; warn = 1; info = 2 }[$_.Level] }, { $_.Key }) })
}
$script:Reglages = @{}
Save-Juge 'alertes.json' $alertesCas

Save-Juge 'fonctions.json' $cas

# ===========================================================================
# 4. Relevés du Trafic (Update-Traffic) et la page Trafic (Get-PagesJeux)
# ===========================================================================
Add-Type -TypeDefinition @'
using System.Collections.Generic;
public static class TrafficMon
{
    public static Dictionary<int, string> Chemins = new Dictionary<int, string>();
    public static string[] UdpLignes = new string[0];
    public static string GetProcessPath(int pid) { return Chemins.ContainsKey(pid) ? Chemins[pid] : ""; }
    public static string[] Udp() { return UdpLignes; }
}
'@
. ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'ecran-perf.ps1') @('Get-IconeExeKit'))))
. ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'trafic.ps1') @('Test-TrafficAlerts'))))
function script:Request-TrafficAidePaths($Lignes) { }
function script:Update-TrafficDns { foreach ($k in $script:DnsFaux.Keys) { $script:Traffic.Dns[$k] = $script:DnsFaux[$k] } }
function script:Update-ServerLookups { }
function script:Update-TrafficSignatures {
    $st = $script:Traffic
    while ($st.SigQueue.Count) { $k = $st.SigQueue.Dequeue(); $a = $st.Apps[$k]; if ($a -and $script:SigFaux.ContainsKey($k)) { $x = $script:SigFaux[$k]; $a.Sig = $x[0]; $a.Publisher = $x[1]; $st.Sig[$k] = $a.Sig } }
}
function script:Get-SvcNames([int]$ProcId) { $r = $script:SvcFaux[$ProcId]; if ($r) { , @($r) } else { , @() } }
function script:Get-TrafficTcp { $script:TrafficCountersOk = $true; @($script:TcpFaux) }
function script:New-ActionNotif($a, $b) { { } }
$script:Bulles = New-Object System.Collections.Generic.List[string]
function script:Show-Notify([string]$Title, [string]$Text, $OnClick) { $script:Bulles.Add("$Title|$Text") }
$ui = @{ Tabs = @{ SelectedIndex = 0 }; LibLeftoverBar = @{ Visibility = 'Collapsed' }; ChkLibCovers = @{ IsChecked = $false }; BtnTraffic = @{ Content = 'Démarrer la surveillance' } }
$Window = @{ IsVisible = $false }
$script:Kit = @{}; $script:Vue = @{ Ecran = 'trafic' }; $script:Games = $null
function script:Get-PlayLog { @{} }
function script:Get-DernierJeuKit { $null }
function script:Get-FpsSessions { @() }
function script:Test-FpsMeasure { $true }
function script:Test-FpsOverlay { $false }
function script:Get-FpsOverlayStyle { 'complet' }
function script:Get-FpsOverlayCorner { 'hg' }
function script:Test-OrgVisible { $false }
$PresentMonExe = $PSCommandPath
function script:Test-AnnuaireServeurs { [bool](Get-Setting 'TrafficLookup' $false) }

$releves = New-Object System.Collections.ArrayList
$cheminsFaux = @('C:\Program Files\Google\Chrome\Application\chrome.exe', 'C:\Users\Joueur\AppData\Local\Temp\x.exe', 'C:\Program Files (x86)\Steam\steam.exe',
    'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe', 'C:\Users\Joueur\Downloads\outil.exe', 'C:\Program Files\AnyDesk\AnyDesk.exe', '', '')
$distantes = @('142.250.1.1', '20.190.1.2', '192.168.1.10', '13.107.4.50', '35.207.1.9', '185.25.182.77', '8.8.8.8', '104.16.1.1', '2a00:1450::1', '10.0.0.5', '151.101.1.1')
for ($sc = 0; $sc -lt 60; $sc++) {
    $script:Traffic = New-TrafficState
    $mins = Pick @(0, 0.4, 0.6, 1, 2.5, 3, 61)
    $script:Traffic.Started = (Get-Date).AddMinutes(-$mins)
    [TrafficMon]::Chemins.Clear()
    $script:SvcFaux = @{}; $script:SigFaux = @{}; $script:DnsFaux = @{}
    $npid = $rnd.Next(1, 7)
    $pids = @(for ($k = 0; $k -lt $npid; $k++) { 7000000 + 4 * $rnd.Next(1, 40) })
    if (Chance 0.2) { $pids += $PID }
    $infos = @()
    foreach ($p in ($pids | Select-Object -Unique)) {
        $c = Pick $cheminsFaux
        if ($p -ne $PID -and $c) { [TrafficMon]::Chemins[$p] = $c }
        $infos += $p
    }
    if (Chance 0.3) { $sp = 7000400; $infos += $sp; $script:SvcFaux[$sp] = @(@{ Name = 'BITS'; Title = 'Service de transfert intelligent en arrière-plan' }, @{ Name = 'DoSvc'; Title = 'Optimisation de livraison' }) }
    foreach ($c in $cheminsFaux) { if ($c -and (Chance 0.7)) { $script:SigFaux[$c.ToLower()] = @((Pick @('Valid', 'NotSigned', 'HashMismatch', 'NotTrusted')), (Pick @('', 'Google LLC', 'Valve Corp.'))) } }
    foreach ($d in $distantes) { if (Chance 0.4) { $script:DnsFaux[$d] = Pick @('www.google.com', 'login.live.com', 'v10.events.data.microsoft.com', 'steamcommunity.com', 'ad.doubleclick.net', 'relay.anydesk.com', 'cdn.example.net') } }
    [TrafficMon]::UdpLignes = @(if (Chance 0.5) { "$(Pick $infos)|50000" })
    $conns = [ordered]@{}
    $passages = New-Object System.Collections.ArrayList
    $n = Pick @(1, 2, 3, 5, 16, 31)
    for ($t = 0; $t -lt $n; $t++) {
        foreach ($k in @($conns.Keys)) { if (Chance 0.15) { $conns.Remove($k) } else { $conns[$k].O += $rnd.Next(0, 3MB); $conns[$k].I += $rnd.Next(0, 9MB) } }
        for ($j = 0; $j -lt $rnd.Next(0, 4); $j++) {
            $p = Pick $infos; $dst = Pick $distantes; $rp = Pick @(443, 443, 80, 4444, 9050, 27015, 53); $lp = $rnd.Next(49152, 65535)
            $conns["$p|$dst|$rp|$lp"] = @{ P = $p; D = $dst; R = $rp; L = $lp; O = $rnd.Next(0, 1MB); I = $rnd.Next(0, 2MB); E = (Pick @(5, 5, 5, 2, 11)) }
        }
        $script:TcpFaux = @(foreach ($c in $conns.Values) { "$($c.P)|$($c.D)|$($c.R)|$($c.L)|$($c.E)|$($c.O)|$($c.I)" }) + @(if (Chance 0.1) { '0|1.2.3.4|80|5000|5|0|0' }) + @('ligne abîmée')
        [void]$passages.Add(@($script:TcpFaux))
        Update-Traffic
    }
    $actif = Chance 0.85
    $script:TrafficTimer = @{ IsEnabled = $actif }
    $ui.BtnTraffic.Content = if ($actif) { 'Arrêter la surveillance' } else { 'Reprendre la surveillance' }
    $script:Reglages = @{ TrafficLookup = (Chance 0.3) }
    $p = Get-PagesJeux
    $st = $script:Traffic
    [void]$releves.Add([ordered]@{
        moi = $PID; minutes = $mins; chemins = @([TrafficMon]::Chemins.GetEnumerator() | ForEach-Object { @{ Pid = $_.Key; Chemin = $_.Value } }); services = @($script:SvcFaux.GetEnumerator() | ForEach-Object { @{ Pid = $_.Key; Services = @($_.Value) } })
        signatures = $script:SigFaux; dns = $script:DnsFaux; udp = @([TrafficMon]::UdpLignes); passages = $passages; actif = $actif; annuaire = [bool]$script:Reglages.TrafficLookup
        apps = @($st.Apps.Values | ForEach-Object { $a = $_; [ordered]@{ Key = $a.Key; Name = $a.Name; Title = $a.Title; Path = $a.Path; Out = $a.Out; In = $a.In; Rate = $a.Rate; Live = $a.Live; Udp = $a.Udp; Sig = $a.Sig
            Dest = @($a.Dest.Values | ForEach-Object { @{ Remote = $_.Remote; Port = $_.Port; Out = $_.Out; In = $_.In; Live = $_.Live; Private = $_.Private } }) } })
        page = $p.trafic; kitApps = @($script:Kit.TraficApps | ForEach-Object { $_.Key }); bulles = @($script:Bulles) })
    $script:Bulles.Clear()
}
Save-Juge 'releves.json' $releves
$script:Traffic = $null; $script:Reglages = @{}

# ===========================================================================
# 5. L'onglet Lag en ligne (Get-PagesJeux) sur des mesures gardées
# ===========================================================================
$ongletsLag = New-Object System.Collections.ArrayList
for ($sc = 0; $sc -lt 40; $sc++) {
    $n = Pick @(0, 1, 2, 5, 16, 20)
    $l = New-Object System.Collections.ArrayList
    for ($k = 0; $k -lt $n; $k++) {
        $r = New-MesureAuHasard
        $r.Id = '{0:x10}' -f $rnd.Next(); $r.Date = (Get-Date).AddMinutes(-$rnd.Next(0, 200000)).ToString('o')
        Set-LagDiag $r
        [void]$l.Add(($r | ConvertTo-Json -Depth 8 | ConvertFrom-Json))
    }
    $script:LagSessions = $l
    $script:Reglages = @{ LagMeasure = (Chance 0.7) }
    $script:LagSession = if (Chance 0.2) { @{ Game = 'Test rapide'; Seconds = 30 } } else { $null }
    $script:Traffic = $null; $script:TrafficTimer = $null
    $p = Get-PagesJeux
    $lag = @(@($p.jeux.onglets)[1].onglets)[1]
    [void]$ongletsLag.Add([ordered]@{ sessions = @($l); mesure = [bool]$script:Reglages.LagMeasure; enCours = [bool]$script:LagSession; module = $lag.module; blocs = $lag.blocs; trafic = $p.trafic })
}
Save-Juge 'onglet-lag.json' $ongletsLag
$script:LagSessions = $null; $script:LagSession = $null; $script:Reglages = @{}
