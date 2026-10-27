# Tests du réseau (lag en ligne, Trafic, trace réseau en veille) et de petits textes, sur de fausses données, sans
# droits administrateur et sans rien lire ni modifier sur le PC.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-reseau.ps1
#
# Ce script contrôle :
#   1. Lag en ligne : quand le serveur du jeu change, l'ancien n'est plus pingé ; la box d'un autre joueur (pair à
#      pair, reconnue par son propriétaire ou son nom) n'est jamais prise pour le serveur du jeu. Un faux LagMon note
#      ce qui est pingé ; le vrai LagMon (natif.cs) est vérifié à part, dans un autre PowerShell, sur 127.0.0.1 seulement ;
#   2. Trafic : les programmes de la suite Mindset (non signés) ne sont plus signalés, les autres le restent ; le clic
#      sur la bulle d'une alerte ouvre le bon programme et n'exécute rien de caché dans son nom ;
#   3. la trace réseau en veille (avant BF6) n'est lancée que si un jeu guetté est installé, et arrêtée sinon ;
#   4. les petits textes : titre « Nevermind » des rapports, Mindset trouvé sur un autre disque, message d'erreur simple ;
#   5. UPnP : une adresse annoncée par un appareil n'est ouverte que si c'est http:// vers lui (audit du 25/10, constat 16) ;
#   6. longues sessions (audit du 25/10, 25c) : la fin d'une partie de 2 h mesurée ne fige plus la fenêtre et donne les
#      mêmes pics box et téléchargement qu'avant ; la page Trafic avec 5 000 destinations reste rapide, oublie les
#      destinations anciennes au delà de 500 sans perdre leur nombre, leurs adresses ni leurs octets.
# Code de sortie : 0 si tout passe, 1 sinon.

param([switch]$VraiLagMon)

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$fichiers = Join-Path $racine 'Nevermind\fichiers'
$modules = Join-Path $fichiers 'modules'

# ---------------------------------------------------------------------------
# Partie lancée dans un autre PowerShell : le vrai LagMon (natif.cs), qui ne pingue que 127.0.0.1
# ---------------------------------------------------------------------------
if ($VraiLagMon) {
    try {
        Add-Type -AssemblyName WindowsBase, PresentationCore, PresentationFramework, System.Drawing, System.Windows.Forms
        Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $modules 'natif.cs'), [Text.Encoding]::UTF8))
        [LagMon]::Start(100)
        $a = [LagMon]::Add('127.0.0.1')
        $b = [LagMon]::Add('127.0.0.1')
        Start-Sleep -Milliseconds 600
        [LagMon]::Remove($a)
        Start-Sleep -Milliseconds 1500
        $na = [LagMon]::Series($a).Count
        $actives = [LagMon]::Active
        Start-Sleep -Milliseconds 800
        $na2 = [LagMon]::Series($a).Count
        $nb = [LagMon]::Series($b).Count
        [LagMon]::Stop()
        # UPnP (audit du 25/10, constat 16) : seulement http:// vers l'appareil qui a répondu
        $upnp = @(foreach ($c in @(
            @('http://192.168.1.10:49152/rootDesc.xml', '192.168.1.10', $true),
            @('http://192.168.1.10/', '192.168.1.10', $true),
            @('file:///C:/Windows/win.ini', '192.168.1.10', $false),
            @('file://192.168.1.10/partage/x.xml', '192.168.1.10', $false),
            @('\\192.168.1.10\partage\x.xml', '192.168.1.10', $false),
            @('http://192.168.1.11/rootDesc.xml', '192.168.1.10', $false),
            @('https://192.168.1.10/rootDesc.xml', '192.168.1.10', $false),
            @('http://moi:secret@192.168.1.10/', '192.168.1.10', $false),
            @('http://box.local/rootDesc.xml', '192.168.1.10', $false),
            @('', '192.168.1.10', $false))) {
            if ([UpnpAdresse]::Permise($c[0], $c[1]) -ne $c[2]) { "$($c[0]) -> $(-not $c[2])" }
        })
        Write-Output "ok|$na|$na2|$nb|$actives|$($upnp -join ' ; ')"
    } catch { Write-Output "erreur|$($_.Exception.Message)" }
    exit 0
}

$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0
function Test-Ok([bool]$Condition, [string]$Message) {
    $script:nbTests++
    if (-not $Condition) { $script:echecs.Add($Message) }
}

# ---------------------------------------------------------------------------
# Sécurité du test : rien ne doit pouvoir lire ou modifier le vrai PC (une fonction passe avant une commande du même nom)
# ---------------------------------------------------------------------------
foreach ($c in 'Get-CimInstance', 'Get-WmiObject', 'Get-Process', 'Stop-Process', 'Start-Process', 'Set-ItemProperty', 'New-ItemProperty',
               'Remove-ItemProperty', 'Invoke-Aide', 'Set-Setting', 'Start-FluxReseau', 'Stop-FluxReseau', 'Show-Notify', 'taskkill', 'taskkill.exe') {
    Set-Item -Path "function:script:$c" -Value ([scriptblock]::Create("throw 'Test : $c est interdit pendant le test du réseau.'"))
}
$script:Journal = New-Object System.Collections.Generic.List[string]
function script:Write-Log([string]$Message) { $script:Journal.Add($Message) }
$script:Reglages = @{}
function script:Get-Setting([string]$Name, $Default = $null) { if ($script:Reglages.ContainsKey($Name)) { $script:Reglages[$Name] } else { $Default } }

# Chargement de fonctions (et de variables) précises d'un fichier, sans rien exécuter d'autre
function Get-Definitions([string]$Fichier, [string[]]$Noms) {
    $tok = $null; $err = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($Fichier, [ref]$tok, [ref]$err)
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
try {
    . ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'lag.ps1') @('$LagParticulierProprio', '$LagParticulierHote', 'Test-AdresseParticulier',
        'Remove-LagServer', 'Add-LagPair', 'Update-LagSession', 'Get-JobResult', 'Receive-LagOwners', 'Receive-LagGeo', 'Test-FluxVeilleUtile', 'Stop-FluxVeille'))))
    . ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'trafic.ps1') @('Test-PrivateIp', 'ConvertTo-IpHex', 'Get-ServerOwner', 'Test-GamePath', 'Test-SuitePath',
        'Get-TrafficMarks', 'Get-TrafficAlerts', 'Format-Bytes', '$LolBins', '$RemoteTools', '$SusPorts', '$UploadOk'))))
    . ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'aide-client.ps1') @('$FpsVeilleConnus', 'Get-FpsVeilleJeux', 'Update-FpsVeille'))))
    . ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'interface.ps1') @('Get-MessageErreur', 'Invoke-Safe'))))
    . ([scriptblock]::Create((Get-Definitions (Join-Path $fichiers 'Nevermind.ps1') @('Find-MindsetExe'))))
} catch { Write-Host "Chargement impossible : $($_.Exception.Message)" -ForegroundColor Red; exit 1 }

# ---------------------------------------------------------------------------
# 1. Lag en ligne : anciens serveurs et box des autres joueurs
# ---------------------------------------------------------------------------
# Faux LagMon : note les adresses pingées et celles abandonnées (aucun ping envoyé)
Add-Type -TypeDefinition @'
using System.Collections.Generic;
public static class LagMon
{
    public static List<string> Hosts = new List<string>();
    public static List<int> Removed = new List<int>();
    public static void Start(int i) { }
    public static void Stop() { }
    public static int Add(string h) { Hosts.Add(h); return Hosts.Count - 1; }
    public static void Remove(int i) { Removed.Add(i); }
    public static Dictionary<int, double[]> Data = new Dictionary<int, double[]>();
    public static double[] Series(int i) { return Data.ContainsKey(i) ? Data[i] : new double[0]; }
    public static string[] TraceRoute(string h, int a, int b) { return new string[0]; }
}
'@
function script:Start-LagOwnerLookup([string]$Ip) { }
function script:Start-LagGeoLookup($S) { }
function script:Update-LagLive { }
function script:Get-LagProcName([int]$ProcId) { "Programme $ProcId" }
function script:Get-TrafficTcp { @() }
function script:Test-AnnuaireServeurs { $true }
$script:Traffic = $null
# Annuaire des adresses (comme serveurs.json) : serveurs de jeu et lignes de particuliers
function New-Entree([string]$De, [string]$A, [string]$O, [string]$N) { @{ S = (ConvertTo-IpHex $De); E = (ConvertTo-IpHex $A); O = $O; N = $N; C = 'FR'; D = '2026-10-22' } }
$script:ServerCache = New-Object System.Collections.ArrayList
foreach ($e in @(
    (New-Entree '35.207.0.0' '35.207.255.255' 'Google LLC' 'GOOGLE-CLOUD'),
    (New-Entree '63.176.0.0' '63.179.255.255' 'Amazon.com, Inc.' 'AMAZO-4'),
    (New-Entree '90.100.0.0' '90.101.255.255' 'Wanadoo France Technical' 'IP2000-ADSL-BAS'),
    (New-Entree '83.196.0.0' '83.196.255.255' 'Wanadoo France Technical' 'IP2000-ADSL-BAS'))) { [void]$script:ServerCache.Add($e) }
function script:Get-ServerCache { , $script:ServerCache }

# Ligne de la trace réseau : pid|proto|adresse|port|paquets envoyés|paquets reçus|octets envoyés|octets reçus|?|trous|plus long trou|vu il y a (ms)
function New-Flux([string]$Ip, [int]$Recus) { "1234|udp|$Ip|3659|$Recus|$Recus|1000|1000|0|0|0|100" }
function New-FausseSession {
    @{ Game = 'Jeu de test'; Pid = 1234; Start = (Get-Date).AddMinutes(-5); Seconds = 0; Gw = ''; Wifi = $false; Adapter = ''; Etw = $true; Flux = 'aide'
       T = @{ ref = [LagMon]::Add('1.1.1.1') }; Isp = ''; IspOwner = $null; Server = $null; Pairs = @{}; Ticks = 0; Prev = @{}; Names = @{}; Bg = (New-Object System.Collections.ArrayList)
       Signal = @(); Band = ''; Channel = ''; TraceJob = $null; SrvJob = $null; OwnerJob = $null; GeoJob = $null; Gaps = 0; MaxGap = 0; GapBase = $null }
}
$script:FluxVus = @()
function script:Get-FluxReseau([string]$Ou) { $script:FluxVus }
# Un passage de la minuterie (toutes les 2 s) ; $Jusqua : passer jusqu'au prochain changement de serveur permis (tous les 15 passages)
function Invoke-Passage([switch]$Jusqua) {
    do { Update-LagSession } while ($Jusqua -and $script:LagSession.Ticks % 15 -ne 0)
}

$script:LagSession = New-FausseSession
$s = $script:LagSession
$script:FluxVus = @(New-Flux '35.207.176.221' 200)
Invoke-Passage
Test-Ok ($s.Server -and $s.Server.Ip -eq '35.207.176.221') "Lag : le premier serveur du jeu n'est pas repéré ($($s.Server.Ip))"
$idxA = $s.T.srv
Test-Ok ([LagMon]::Hosts[$idxA] -eq '35.207.176.221') 'Lag : le serveur repéré n''est pas pingé'
# Le serveur change (autre serveur Amazon, plus de paquets) : l'ancien n'est plus pingé
$script:FluxVus = @((New-Flux '35.207.176.221' 60), (New-Flux '63.178.238.177' 400))
Invoke-Passage -Jusqua
Test-Ok ($s.Server.Ip -eq '63.178.238.177') "Lag : le changement de serveur n'est pas suivi ($($s.Server.Ip))"
Test-Ok ([LagMon]::Removed -contains $idxA) 'Lag : l''ancien serveur reste pingé après le changement (il l''était jusqu''à la fin de la partie)'
Test-Ok ($s.T.srv -ne $idxA -and [LagMon]::Hosts[$s.T.srv] -eq '63.178.238.177') 'Lag : le nouveau serveur n''est pas celui pingé'
$idxB = $s.T.srv
# La box d'un joueur d'Épinal (ligne ADSL d'Orange, propriétaire déjà connu) : jamais prise pour le serveur
$script:FluxVus = @((New-Flux '63.178.238.177' 60), (New-Flux '90.101.111.7' 900))
Invoke-Passage -Jusqua
Test-Ok ($s.Server.Ip -eq '63.178.238.177') "Lag : la box d'un autre joueur (90.101.111.7) a été prise pour le serveur du jeu"
Test-Ok ($s.Pairs.ContainsKey('90.101.111.7')) 'Lag : la box d''un autre joueur n''est pas écartée'
Test-Ok (-not ([LagMon]::Hosts -contains '90.101.111.7')) 'Lag : la box d''un autre joueur est pingée'
Test-Ok (-not ([LagMon]::Removed -contains $idxB)) 'Lag : le serveur du jeu a été abandonné à cause de la box d''un autre joueur'
Test-Ok (@($script:Journal | Where-Object { $_ -like '*90.101.111.7*autre joueur*' }).Count -eq 1) 'Lag : la box écartée n''est pas notée une fois au journal'
# Une box inconnue de l'annuaire au moment du choix (Free, Le Bouscat) : écartée dès que son propriétaire est connu
$script:FluxVus = @((New-Flux '63.178.238.177' 60), (New-Flux '88.165.8.13' 900))
Invoke-Passage -Jusqua
Test-Ok ($s.Server.Ip -eq '88.165.8.13') 'Lag (préparation) : une adresse inconnue devrait être suivie en attendant l''annuaire'
$idxC = $s.T.srv
[void]$script:ServerCache.Add((New-Entree '88.160.0.0' '88.191.255.255' 'Administrative' 'FR-PROXAD-ADSL'))
Invoke-Passage
Test-Ok ([LagMon]::Removed -contains $idxC) 'Lag : la box d''un autre joueur reste pingée une fois reconnue'
Test-Ok ($s.Pairs.ContainsKey('88.165.8.13')) 'Lag : la box reconnue après coup n''est pas écartée'
Test-Ok ($s.Server -and $s.Server.Ip -eq '63.178.238.177') "Lag : le vrai serveur n'est pas repris tout de suite après la box écartée ($($s.Server.Ip))"
# La box n'est plus jamais choisie, même avec plus de paquets que le serveur
$script:FluxVus = @((New-Flux '88.165.8.13' 900), (New-Flux '35.207.109.101' 300))
Invoke-Passage -Jusqua
Test-Ok ($s.Server -and $s.Server.Ip -eq '35.207.109.101') "Lag : la box écartée est reprise pour le serveur du jeu ($($s.Server.Ip))"
# Les annonces au réseau local (FC 27 : 255.255.255.255 port 9999, et les groupes 224 à 239) ne sont jamais le serveur
$script:FluxVus = @((New-Flux '255.255.255.255' 5000), (New-Flux '239.255.255.250' 3000), (New-Flux '224.0.0.251' 2000), (New-Flux '35.207.109.101' 300))
Invoke-Passage -Jusqua
Test-Ok ($s.Server -and $s.Server.Ip -eq '35.207.109.101') "Lag : une annonce au réseau local est prise pour le serveur du jeu ($($s.Server.Ip))"
Test-Ok (-not ([LagMon]::Hosts -match '^(2[2-5]\d)\.')) 'Lag : une adresse d''annonce au réseau local est pingée'
foreach ($ip in '255.255.255.255', '239.255.255.250', '224.0.0.1', '240.1.2.3') { Test-Ok (Test-PrivateIp $ip) "Trafic : $ip devrait compter comme une adresse du réseau local" }
foreach ($ip in '223.5.5.5', '35.207.109.101', '3.40.138.1') { Test-Ok (-not (Test-PrivateIp $ip)) "Trafic : $ip est une adresse d'Internet, pas du réseau local" }
# Nom de l'adresse (ipinfo) : « ...abo.wanadoo.fr » est une box, « ...compute.amazonaws.com » un serveur
$script:LagGeo = @{ '86.200.1.2' = @{ Pays = 'FR'; Ville = 'Metz'; Hote = 'lfbn-nan-1-234-56.w86-200.abo.wanadoo.fr' }
                    '3.40.137.124' = @{ Pays = 'IE'; Ville = 'Dublin'; Hote = 'ec2-3-40-137-124.eu-west-1.compute.amazonaws.com' }
                    '78.192.3.4' = @{ Pays = 'FR'; Ville = 'Lyon'; Hote = 'lns-bzn-21-78-192-3-4.adsl.proxad.net' } }
Test-Ok ((Test-AdresseParticulier '86.200.1.2') -eq $true) 'Lag : une box reconnue par son nom (abo.wanadoo.fr) n''est pas écartée'
Test-Ok ((Test-AdresseParticulier '78.192.3.4') -eq $true) 'Lag : une box reconnue par son nom (adsl.proxad.net) n''est pas écartée'
Test-Ok ((Test-AdresseParticulier '3.40.137.124') -eq $false) 'Lag : un serveur Amazon est pris pour une box'
Test-Ok ($null -eq (Test-AdresseParticulier '45.12.13.14')) 'Lag : une adresse inconnue devrait rester « pas encore connue »'
# Propriétaires jamais vus : serveurs de jeu et lignes de particuliers d'autres pays
foreach ($cas in @(
    @('Comcast Cable Communications, LLC', 'CABLE-COMCAST-12', $true), @('Deutsche Telekom AG', 'DTAG-DIAL25', $true),
    @('Bouygues Telecom SA', 'FR-BOUYGTEL-FTTH', $true), @('Virgin Media Limited', 'VM-BROADBAND', $true),
    @('i3D.net B.V', 'I3D-NET', $false), @('OVH SAS', 'OVH', $false), @('Electronic Arts, Inc.', 'EA-NET', $false),
    @('Microsoft Corporation', 'MSFT', $false), @('Valve Corporation', 'VALVE-L1', $false), @('Amazon Data Services Ireland Limited', 'AMAZON-DUB', $false))) {
    $script:ServerCache.Clear()
    [void]$script:ServerCache.Add((New-Entree '100.0.0.0' '100.0.0.255' $cas[0] $cas[1]))
    $r = Test-AdresseParticulier '100.0.0.7'
    Test-Ok ($r -eq $cas[2]) "Lag : $($cas[0]) ($($cas[1])) $(if ($cas[2]) { 'devrait être reconnu comme une box' } else { 'pris pour une box' })"
}
$script:LagSession = $null

# Le vrai LagMon (natif.cs) : une cible abandonnée n'est plus pingée, les autres continuent, ses mesures restent lisibles
$psExe = "$env:windir\System32\WindowsPowerShell\v1.0\powershell.exe"
$sortie = @(& $psExe -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath -VraiLagMon) | Where-Object { $_ -match '^(ok|erreur)\|' } | Select-Object -Last 1
$x = ([string]$sortie) -split '\|'
if ($x[0] -ne 'ok') { Test-Ok $false "LagMon (natif.cs) : $sortie" }
else {
    Test-Ok ([int]$x[1] -gt 0) 'LagMon : la cible abandonnée a perdu ses mesures'
    Test-Ok ([int]$x[2] -eq [int]$x[1]) "LagMon : la cible abandonnée est encore pingée ($($x[1]) puis $($x[2]) mesures)"
    Test-Ok ([int]$x[3] -gt [int]$x[2]) 'LagMon : l''autre cible n''est plus pingée'
    Test-Ok ([int]$x[4] -eq 1) "LagMon : $($x[4]) cible(s) encore pingée(s) au lieu d'une"
    Test-Ok ($x.Count -ge 6 -and -not $x[5]) "UPnP : adresses mal jugées par UpnpAdresse (natif.cs) : $(if ($x.Count -ge 6) { $x[5] } else { 'UpnpAdresse absent' })"
}
# UPnP : chaque adresse reçue du réseau passe par UpnpAdresse avant d'être ouverte (fiche des appareils, box, commandes)
$texteAvance = [IO.File]::ReadAllText((Join-Path $modules 'reseau-avance.ps1'))
$texteAudit = [IO.File]::ReadAllText((Join-Path $modules 'audit-reseau.ps1'))
Test-Ok ($texteAvance -match '\[UpnpAdresse\]::Permise\(\$loc, \$ip\)\) \{ continue \}\s+try \{\s+\$x = \[xml\]\(Invoke-WebRequest -Uri \$loc') 'UPnP : la fiche d''un appareil est ouverte sans contrôle de son adresse (reseau-avance.ps1)'
Test-Ok ($texteAudit -match '\[UpnpAdresse\]::Permise\(\$p\[2\], \$a\.Gateway\)' -and $texteAudit -match '\[UpnpAdresse\]::Permise\(\$url, \$a\.Gateway\)') 'UPnP : l''adresse de la box ou de ses commandes est ouverte sans contrôle (audit-reseau.ps1)'

# Audit : le WPS d'un routeur qui n'est pas la box ouvre la page de ce routeur, et aucun point de l'audit ne propose
# « C'est moi » (il n'a pas d'éléments à approuver ; @($c.Trust) compte un élément quand Trust est absent)
try {
    Invoke-Expression (Get-Definitions (Join-Path $modules 'audit-reseau.ps1') @('Add-AuditCheck', 'Get-NetAuditChecks'))
    $R = @{ Ssdp = @('192.168.1.10|urn:schemas-wifialliance-org:device:WFADevice:1|x', '192.168.1.1|urn:schemas-wifialliance-org:service:WFAWLANConfig:1|y'); Profiles = @(); Ports = @(); Dns = @{ Servers = @() }; Pc = @{} }
    $devs = @([pscustomobject]@{ Ip = '192.168.1.10'; Title = 'rt-ax88u-55c8' }, [pscustomobject]@{ Ip = '192.168.1.1'; Title = 'Box Internet' })
    $checks = @(Get-NetAuditChecks $R $devs @{ Gateway = '192.168.1.1' })
    $wps = @($checks | Where-Object { $_.Title -match 'WPS semble' })[0]
    $cibles = @($wps.Actions | ForEach-Object { [string]$_.Arg })
    Test-Ok ($cibles -contains 'http://192.168.1.10' -and $cibles -contains 'http://192.168.1.1') "Audit : le WPS du routeur n'ouvre pas sa propre page ($($cibles -join ', '))"
    Test-Ok (-not @($checks | Where-Object { @($_.Trust | Where-Object { $_ }).Count }).Count) 'Audit : un point propose « C''est moi »'
    $texteSecu = [IO.File]::ReadAllText((Join-Path $modules 'securite.ps1')) + [IO.File]::ReadAllText((Join-Path $modules 'ecran-protection.ps1'))
    Test-Ok ($texteSecu -notmatch 'if \(@\(\$(Check|c)\.Trust\)\.Count\)') 'Sécurité : « C''est moi » compté avec @(...Trust).Count (vaut 1 sans Trust)'
} catch { Test-Ok $false "Audit : $($_.Exception.Message)" }

# ---------------------------------------------------------------------------
# 2. Trafic : les programmes de la suite ne sont pas suspects
# ---------------------------------------------------------------------------
$script:GameIndex = $null
function New-App([string]$Chemin, [string]$Sig = 'NotSigned') {
    @{ Key = $Chemin.ToLower(); Name = [IO.Path]::GetFileNameWithoutExtension($Chemin); Path = $Chemin; Title = [IO.Path]::GetFileNameWithoutExtension($Chemin); Sig = $Sig
       IsSelf = $false; Out = 1MB; In = 5MB; Dest = @{ a = @{ Private = $false; Remote = '20.1.2.3'; Port = 443 } }; Parent = ''; ParentSig = ''; ParentPub = '' }
}
$pf = if ($env:ProgramW6432) { $env:ProgramW6432 } else { $env:ProgramFiles }
$suite = @('D:\Mindset\Programmes\Mindset\Mindset.exe', 'D:\Mindset\Programmes\Nevermind\NevermindAide.exe', 'E:\Mindset\Programmes\Evermind\Mindset.exe',
           "$pf\Mindset\Mindset\Mindset.exe", "$pf\Mindset\Mindcare\Mindset.exe", 'D:\Mindset\Programmes\Nevermind\Désinstaller Nevermind.exe')
$autres = @('C:\Users\Joueur\Mindset\Programmes\Faux\Faux.exe', 'D:\Mindset\Programmes\Mindset\Temp\virus.exe', 'D:\Mindset\virus.exe',
            'D:\Jeux\Outil\outil.exe', 'C:\Users\Joueur\Downloads\Mindset\Programmes\Mindset\Mindset.exe')
$script:Traffic = @{ Apps = @{} }
foreach ($c in @($suite) + @($autres)) { $a = New-App $c; $script:Traffic.Apps[$a.Key] = $a }
$alertes = @(Get-TrafficAlerts)
foreach ($c in $suite) { Test-Ok (-not @($alertes | Where-Object { $_.App.Path -eq $c }).Count) "Trafic : $c (suite Mindset) est signalé comme suspect" }
foreach ($c in $autres) { Test-Ok (@($alertes | Where-Object { $_.App.Path -eq $c }).Count -eq 1) "Trafic : $c (non signé, hors de la suite) n'est plus signalé" }
# Un fichier de la suite modifié (signature invalide) reste signalé
$script:Traffic = @{ Apps = @{} }
$a = New-App 'D:\Mindset\Programmes\Mindset\Mindset.exe' 'HashMismatch'; $script:Traffic.Apps[$a.Key] = $a
Test-Ok (@(Get-TrafficAlerts).Count -eq 1) 'Trafic : un programme de la suite à la signature invalide n''est plus signalé'
$script:Traffic = $null

# Bulle « Activité réseau à vérifier » (constat 10) : le chemin du programme n'entre jamais dans le code du clic. Un
# dossier « d’Arthur » (apostrophe typographique, que PowerShell prend pour un guillemet) cassait la bulle et sautait
# les alertes suivantes ; un nom piégé faisait exécuter du code au clic.
try {
    . ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'trafic.ps1') @('Test-TrafficAlerts'))))
    . ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'assistance.ps1') @('New-ActionNotif'))))
} catch { Test-Ok $false "Trafic : chargement impossible ($($_.Exception.Message))" }
$script:Bulles = New-Object System.Collections.Generic.List[object]
function script:Show-Notify([string]$Title, [string]$Text, [scriptblock]$OnClick) { $script:Bulles.Add($OnClick) }
$script:Ouverts = New-Object System.Collections.Generic.List[string]
function script:Show-Page($i) {}
function script:Show-TrafficApp([string]$Key) { $script:Ouverts.Add($Key) }
$TrafficIndex = 3
$script:codeexecute = 0
$pieges = @("C:\Jeux d’Arthur\app.exe", "C:\Users\Joueur\Downloads\a’; `$script:codeexecute = 1; ’.exe", "C:\Users\Joueur\Downloads\b'; `$script:codeexecute = 2; '.exe",
            'C:\Users\Joueur\Downloads\c$($script:codeexecute = 3).exe', 'C:\Users\Joueur\Downloads\d`$script:codeexecute.exe')
$script:Traffic = @{ Apps = @{}; Notified = @{} }
foreach ($c in $pieges) { $a = New-App $c; $script:Traffic.Apps[$a.Key] = $a }
$erreur = ''
try { Test-TrafficAlerts } catch { $erreur = $_.Exception.Message }
Test-Ok (-not $erreur) "Trafic : la recherche d'alertes s'arrête sur un nom de programme ($erreur)"
Test-Ok ($script:Bulles.Count -eq $pieges.Count) "Trafic : $($script:Bulles.Count) bulle(s) au lieu de $($pieges.Count)"
foreach ($b in $script:Bulles) { try { & $b } catch { Test-Ok $false "Trafic : clic sur la bulle en erreur ($($_.Exception.Message))" } }
Test-Ok ($script:codeexecute -eq 0) "Trafic : du code caché dans un nom de programme a été exécuté au clic ($script:codeexecute)"
foreach ($c in $pieges) { Test-Ok ($script:Ouverts -ccontains $c.ToLower()) "Trafic : le clic n'ouvre pas le bon programme ($c)" }
Test-Ok ($script:ActionsNotif.Count -eq 0) "Trafic : $($script:ActionsNotif.Count) donnée(s) de bulle gardée(s) après le clic"
# Bulles jamais cliquées : 50 gardées au plus
foreach ($i in 1..60) { [void](New-ActionNotif 'Show-TrafficApp $d' "x$i") }
Test-Ok ($script:ActionsNotif.Count -eq 50) "Trafic : $($script:ActionsNotif.Count) données de bulles gardées au lieu de 50 au plus"
$script:Traffic = $null

# ---------------------------------------------------------------------------
# 3. Trace réseau en veille : seulement si un jeu guetté (BF6) est installé
# ---------------------------------------------------------------------------
$script:Arrets = 0
function script:Stop-FluxVeille { $script:Arrets++ }
$script:LagSession = $null; $script:FluxVeille = $false
$script:GameIndex = @{ fc27 = @{ Game = 'EA SPORTS FC 27'; Exes = @('c:\jeux\fc27.exe') } }
Test-Ok (-not (Test-FluxVeilleUtile $true)) 'Veille : la trace réseau est lancée alors qu''aucun jeu guetté n''est installé'
Test-Ok ($script:Arrets -eq 0) 'Veille : un arrêt est demandé alors qu''aucune trace n''a été lancée'
$script:GameIndex.bf6 = @{ Game = 'Battlefield 6'; Exes = @('c:\jeux\bf6.exe') }
Test-Ok (Test-FluxVeilleUtile $true) 'Veille : la trace réseau n''est pas lancée alors que BF6 est installé'
Test-Ok (-not (Test-FluxVeilleUtile $false)) 'Veille : la trace réseau est gardée alors que la mesure du lag est coupée'
Test-Ok ($script:Arrets -eq 1) 'Veille : la trace réseau lancée n''est pas arrêtée quand elle ne sert plus'
[void](Test-FluxVeilleUtile $true)
$script:LagSession = @{ Game = 'Test rapide' }
[void](Test-FluxVeilleUtile $false)
Test-Ok ($script:Arrets -eq 1) 'Veille : la trace réseau est arrêtée en pleine mesure du lag'
$script:LagSession = $null
$script:GameIndex.Remove('bf6')
[void](Test-FluxVeilleUtile $true)
Test-Ok ($script:Arrets -eq 2) 'Veille : la trace réseau continue après la désinstallation de BF6'
$script:GameIndex = $null
Test-Ok (-not (Test-FluxVeilleUtile $true)) 'Veille : la trace réseau est lancée avant que la liste des jeux soit connue'
# Update-FpsVeille (aide-client.ps1) passe bien par ce contrôle : sans BF6 installé, rien n'est demandé à l'Aide
function script:Test-FpsMeasure { $false }
function script:Test-LagMeasure { $true }
$script:FpsVeilleTache = $null; $script:FpsVeilleDemande = $null; $script:FpsTarget = $null
$script:GameIndex = @{ fc27 = @{ Game = 'EA SPORTS FC 27'; Exes = @('c:\jeux\fc27.exe') } }
$script:FluxVeille = $true; $script:Arrets = 0
try { Update-FpsVeille; $ok = $true } catch { $ok = $false; $msg = $_.Exception.Message }
Test-Ok $ok "Veille : Update-FpsVeille a voulu lancer la trace réseau sans jeu guetté installé ($msg)"
Test-Ok ($script:Arrets -eq 1 -and -not $script:FpsVeilleTache) 'Veille : Update-FpsVeille n''arrête pas la trace réseau devenue inutile'

# ---------------------------------------------------------------------------
# 4. Petits textes
# ---------------------------------------------------------------------------
foreach ($f in 'onglets.ps1', 'audit-reseau.ps1') {
    $t = [IO.File]::ReadAllText((Join-Path $modules $f), [Text.Encoding]::UTF8)
    Test-Ok ($t -notmatch 'Opti<span>Game') "Textes : le rapport HTML de $f s'affiche encore « OptiGame »"
    Test-Ok ($t -match 'Never<span>mind</span>') "Textes : le rapport HTML de $f n'a pas le titre « Nevermind »"
}
# Message d'erreur : celui écrit pour l'utilisateur tel quel, une erreur technique en mots simples (sans fichier ni ligne)
$script:Messages = New-Object System.Collections.Generic.List[string]
function script:Show-Message([string]$Texte, $Icone) { $script:Messages.Add($Texte) }
function script:Set-Status([string]$Texte) { }
function script:Set-Busy($On) { }
Invoke-Safe { $x = $null; $x.Methode() }
Invoke-Safe { [int]::Parse('pas un nombre') }
Invoke-Safe { throw 'Impossible de bloquer ce programme : son emplacement est inconnu.' }
Test-Ok ($script:Messages.Count -eq 3) 'Erreurs : un message manque'
foreach ($m in $script:Messages[0], $script:Messages[1]) {
    Test-Ok ($m -notmatch '\.ps1|ligne \d|Null|Parse|argument|Exception') "Erreurs : message technique montré à l'utilisateur : $m"
    Test-Ok ($m -match 'mal passé') "Erreurs : message simple absent : $m"
}
Test-Ok ($script:Messages[2] -match 'son emplacement est inconnu' -and $script:Messages[2] -notmatch '\.ps1') "Erreurs : le message écrit pour l'utilisateur n'est plus montré : $($script:Messages[2])"
Test-Ok (@($script:Journal | Where-Object { $_ -like 'ERREUR:*Methode*' }).Count -eq 1) 'Erreurs : le détail technique n''est plus noté au journal'
foreach ($m in $script:Messages) { Test-Ok ($m -notmatch '[\u2013\u2014]') "Erreurs : tiret long dans le message : $m" }

# Mindset installé sur un autre disque : trouvé (inscription de Windows, ou dossier X:\Mindset\Programmes\Mindset)
$script:Inscrit = $null; $script:Existe = @()
function script:Get-ItemProperty { param([Parameter(Position = 0)][string]$Path) if ($script:Inscrit) { [pscustomobject]@{ InstallLocation = $script:Inscrit } } else { throw 'Clé absente (faux registre)' } }
function script:Test-Path { param([string]$LiteralPath) $script:Existe -contains $LiteralPath }
$script:Inscrit = 'Z:\Mindset\Programmes\Mindset'; $script:Existe = @('Z:\Mindset\Programmes\Mindset\Mindset.exe')
Test-Ok ((Find-MindsetExe) -eq 'Z:\Mindset\Programmes\Mindset\Mindset.exe') "Mindset : l'installation inscrite sur un autre disque n'est pas trouvée ($(Find-MindsetExe))"
$disque = @([IO.DriveInfo]::GetDrives() | Where-Object { [string]$_.DriveType -eq 'Fixed' -and $_.IsReady } | Select-Object -Last 1)[0]
if ($disque) {
    $attendu = Join-Path $disque.RootDirectory.FullName 'Mindset\Programmes\Mindset\Mindset.exe'
    $script:Inscrit = $null; $script:Existe = @($attendu)
    Test-Ok ((Find-MindsetExe) -eq $attendu) "Mindset : $attendu (sans inscription) n'est pas trouvé ($(Find-MindsetExe))"
}
$pfMindset = Join-Path $pf 'Mindset\Mindset\Mindset.exe'
$script:Inscrit = 'Z:\Ancien\Mindset'; $script:Existe = @($pfMindset)
Test-Ok ((Find-MindsetExe) -eq $pfMindset) 'Mindset : une inscription périmée (dossier disparu) est prise au lieu de Program Files'
$script:Inscrit = $null; $script:Existe = @()
Test-Ok ((Find-MindsetExe) -eq $pfMindset) 'Mindset : sans Mindset installé, le chemin par défaut a changé'
Remove-Item function:script:Test-Path, function:script:Get-ItemProperty

# ---------------------------------------------------------------------------
# 6. Longues sessions (audit du 25/10, 25c) : fin d'une longue partie mesurée, page Trafic qui tourne des heures
# ---------------------------------------------------------------------------
# (fonctions chargées une à une : sur l'ancien code, celles qui manquent font échouer leur vérification, pas le script)
foreach ($x in @(@('lag.ps1', 'Stop-LagSession'), @('lag.ps1', 'Get-PingStats'), @('lag.ps1', 'Compress-PingSeries'), @('lag.ps1', 'Get-LagSpikeLinks'),
                 @('trafic.ps1', 'Update-Traffic'), @('trafic.ps1', 'Compress-TrafficDests'), @('trafic.ps1', 'Get-TrafficDestCount'))) {
    try { . ([scriptblock]::Create((Get-Definitions (Join-Path $modules $x[0]) @($x[1])))) } catch { Write-Host "Info : $($x[1]) absent de $($x[0])" }
}
# Partie simulée : un ping toutes les 0,5 s vers la box, Internet et le serveur ; débit des autres programmes toutes les 2 s
function New-SerieEssai([int]$N, [int]$Graine, [double]$Base) {
    $rnd = New-Object Random($Graine); $a = New-Object double[] (2 * $N)
    for ($i = 0; $i -lt $N; $i++) { $a[2 * $i] = $i * 0.5; $x = $rnd.NextDouble(); $a[2 * $i + 1] = if ($x -lt 0.005) { -1 } elseif ($x -lt 0.03) { $Base + 60 + $rnd.Next(200) } else { $Base + $rnd.Next(4) } }
    , $a
}
function New-PartieEssai([double]$Heures) {
    $n = [int]($Heures * 7200)
    [LagMon]::Data[900] = New-SerieEssai $n 1 2; [LagMon]::Data[901] = New-SerieEssai $n 2 12; [LagMon]::Data[902] = New-SerieEssai $n 4 30
    $bg = New-Object System.Collections.ArrayList
    $rnd = New-Object Random(9)
    for ($i = 0; $i -lt [int]($Heures * 1800); $i++) {
        $gros = $rnd.NextDouble() -lt 0.05
        [void]$bg.Add(@{ T = [math]::Round($i * 2.0, 1); Rate = $(if ($gros) { 2MB } else { 10KB }); Name = $(if ($gros) { @('Steam', 'OneDrive')[$i % 2] } else { '' }); NameRate = $(if ($gros) { 1MB + $i } else { 0 }) })
    }
    $script:LagSession = @{ Game = 'Essai'; Seconds = 0; Start = (Get-Date).AddHours(-$Heures); Etw = $false; T = @{ gw = 900; ref = 901; srv = 902 }; Bg = $bg
        Server = @{ Ip = '1.2.3.4'; Port = 7000; Proto = 'udp'; Pays = '' }; Isp = ''; Wifi = $false; Adapter = 'Ethernet'; Signal = @(); Band = ''; Channel = ''; Gaps = 0; MaxGap = 0 }
}
# Ce que donnait l'ancien calcul (chaque pic comparé à toute la partie), sur une partie courte pour rester rapide
function Get-LiensReference($Pics, $Box, $Bg) {
    $local = 0; $hit = 0; $noms = @{}
    foreach ($t in $Pics) {
        if (@($Box | Where-Object { [math]::Abs($_ - $t) -le 1 }).Count) { $local++ }
        $bs = @($Bg | Where-Object { [math]::Abs($_.T - $t) -le 2.5 -and $_.Rate -gt 500KB })
        if ($bs.Count) { $hit++; foreach ($b in $bs) { if ($b.Name) { $noms[$b.Name] = [math]::Max([double]$noms[$b.Name], [double]$b.NameRate) } } }
    }
    @{ Local = $local; Hit = $hit; Noms = (@($noms.GetEnumerator() | Sort-Object Value -Descending | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ',') }
}
$script:Sessions = New-Object System.Collections.ArrayList
function script:Get-LagSessions { , $script:LagSessions }
function script:Save-LagSessions { }
function script:Set-LagDiag($R) { $R.Diag = @{ Level = 'warn'; Title = 'Essai' } }
function script:Request-LagPanel { }
function script:Show-LagSession($Id) { }
function script:Get-ServerOwner([string]$Ip) { $null }
function script:Update-LagLive { }
$script:LagTimer = $null; $script:LagOwnerJobs = $null
try {
    foreach ($h in 0.25, 2) {
        New-PartieEssai $h
        $bgEssai = $script:LagSession.Bg
        $script:LagSessions = New-Object System.Collections.ArrayList
        $chrono = [Diagnostics.Stopwatch]::StartNew()
        Stop-LagSession
        $ms = $chrono.ElapsedMilliseconds
        $r = @($script:LagSessions)[-1]
        Test-Ok ($null -ne $r) "Longue partie ($h h) : mesure non enregistrée"
        if ($h -eq 2) { Test-Ok ($ms -lt 3000) "Longue partie : la fin d'une partie de 2 h fige la fenêtre $([math]::Round($ms / 1000.0, 1)) s (avant la correction : 11 s et plus)" }
        else {
            # Mêmes résultats que l'ancien calcul
            $srv = Get-PingStats ([LagMon]::Data[902]); $box = Get-PingStats ([LagMon]::Data[900])
            $ref = Get-LiensReference $srv.Spikes $box.Spikes $bgEssai
            $noms = (@($r.BgApps | ForEach-Object { "$($_.Name)=$($_.Rate)" }) -join ',')
            Test-Ok ($r -and $r.Spikes -eq $srv.Spikes.Count -and $r.LocalSpikes -eq $ref.Local -and $r.BgSpikes -eq $ref.Hit -and $noms -eq $ref.Noms) "Longue partie : pics box ou téléchargement différents de l'ancien calcul (box $($r.LocalSpikes) au lieu de $($ref.Local), téléchargement $($r.BgSpikes) au lieu de $($ref.Hit), programmes « $noms » au lieu de « $($ref.Noms) »)"
            Test-Ok ($ref.Local -gt 0 -and $ref.Hit -gt 0) "Longue partie : la partie d'essai n'a aucun pic commun (essai sans valeur)"
        }
        Write-Host "Info : fin d'une partie de $h h calculée en $ms ms"
    }
} catch { Test-Ok $false "Longue partie : $($_.Exception.Message) $($_.InvocationInfo.PositionMessage)" }
$script:LagSession = $null; [LagMon]::Data.Clear()

# Trafic : un programme qui a parlé à 5 000 adresses il y a 2 heures, plus 20 connexions ouvertes et un port signalé
function script:Get-TrafficTcp { $script:LignesTcp }
function script:Get-TrafficApp([int]$ProcId) { 'essai' }
function script:Request-TrafficAidePaths($Lignes) { }
function script:Update-TrafficDns { }
function script:Update-TrafficSignatures { }
function script:Update-ServerLookups { }
$ui = @{ Tabs = @{ SelectedIndex = 0 } }
$TrafficIndex = 3
$script:Bulles.Clear()
try {
    $vieux = (Get-Date).AddHours(-2)
    $dest = @{}
    for ($i = 0; $i -lt 5000; $i++) {
        $ip = "20.$([math]::Floor($i / 250)).$($i % 250).7"
        $dest["$ip|443"] = @{ Remote = $ip; Port = 443; OutClosed = [double]1000; InClosed = [double]2000; Out = [double]1000; In = [double]2000; First = $vieux; Last = $vieux; Live = $false; Private = $false }
    }
    $dest['45.1.1.1|4444'] = @{ Remote = '45.1.1.1'; Port = 4444; OutClosed = [double]10; InClosed = [double]10; Out = [double]10; In = [double]10; First = $vieux; Last = $vieux; Live = $false; Private = $false }
    $dest['192.168.1.1|80'] = @{ Remote = '192.168.1.1'; Port = 80; OutClosed = [double]10; InClosed = [double]10; Out = [double]10; In = [double]10; First = $vieux; Last = $vieux; Live = $false; Private = $true }
    $app = @{ Key = 'essai'; Name = 'outil'; Path = 'C:\Outils\outil.exe'; Title = 'Outil'; Sig = 'NotSigned'; IsSelf = $false; OutClosed = [double]5MB; InClosed = [double]10MB; Out = [double]0; In = [double]0
        Rate = [double]0; LastOut = [double]0; Dest = $dest; Ports = @{}; Udp = $false; Pids = @{}; Parent = ''; ParentSig = ''; ParentPub = '' }
    $script:Traffic = @{ Conns = @{}; Pids = @{}; Apps = @{ essai = $app }; Ticks = 28; Notified = @{}; Alerts = @() }
    $script:LignesTcp = @(for ($i = 0; $i -lt 20; $i++) { "1234|30.0.0.$i|443|5$i|5|100|200" })
    # (numéros de passage choisis : le relevé UDP, réservé à la vraie appli, ne tombe jamais)
    $chrono = [Diagnostics.Stopwatch]::StartNew()
    Update-Traffic
    $ms1 = $chrono.ElapsedMilliseconds
    Test-Ok ($ms1 -lt 1000) "Trafic : un passage avec 5 000 destinations prend $ms1 ms (avant la correction : plus de 2 s)"
    $al = @($script:Traffic.Alerts | Where-Object { $_.App.Key -eq 'essai' })[0]
    Test-Ok ($al -and ($al.Why -join ' ') -match 'énormément d''adresses différentes \(5021\)' -and ($al.Why -join ' ') -match 'port 4444') "Trafic : alertes fausses avant l'oubli ($(if ($al) { $al.Why -join ' ; ' }))"
    # Passage suivant (toutes les minutes) : les destinations anciennes sont oubliées
    $script:Traffic.Ticks = 29; Update-Traffic
    $script:Traffic.Ticks = 31; $chrono.Restart(); Update-Traffic; $ms2 = $chrono.ElapsedMilliseconds
    $n = $app.Dest.Count
    Test-Ok ($n -le 520) "Trafic : $n destinations gardées en mémoire au lieu de 500 environ (les anciennes ne sont jamais oubliées)"
    Test-Ok ($app.Dest.ContainsKey('45.1.1.1|4444')) 'Trafic : la destination sur un port signalé a été oubliée'
    Test-Ok ($app.Dest.ContainsKey('30.0.0.3|443') -and $app.Dest['30.0.0.3|443'].Live) 'Trafic : une connexion ouverte a été oubliée'
    $cpt = try { Get-TrafficDestCount $app } catch { -1 }
    Test-Ok ($cpt -eq 5021) "Trafic : $cpt destinations sur Internet comptées au lieu de 5021 après l'oubli"
    Test-Ok ($app.Out -eq 5MB + 20 * 100 -and $app.In -eq 10MB + 20 * 200) "Trafic : octets du programme changés par l'oubli ($($app.Out) envoyés, $($app.In) reçus)"
    $al = @($script:Traffic.Alerts | Where-Object { $_.App.Key -eq 'essai' })[0]
    Test-Ok ($al -and ($al.Why -join ' ') -match 'énormément d''adresses différentes \(5021\)' -and ($al.Why -join ' ') -match 'port 4444') "Trafic : alertes changées par l'oubli ($(if ($al) { $al.Why -join ' ; ' }))"
    Test-Ok ($ms2 -lt 500) "Trafic : un passage après l'oubli prend encore $ms2 ms"
    # Une adresse oubliée qui revient n'est pas comptée deux fois
    $revient = if ($app.DestAnciennes) { @($app.DestAnciennes)[0] } else { '20.0.5.7|443' }
    $script:LignesTcp = @($script:LignesTcp) + "1234|$($revient.Split('|')[0])|443|99|5|10|10"
    $script:Traffic.Ticks = 32; Update-Traffic
    Test-Ok ($app.Dest.ContainsKey($revient)) "Trafic : l'adresse revenue ($revient) n'est pas dans la liste"
    $cpt = try { Get-TrafficDestCount $app } catch { -1 }
    Test-Ok ($cpt -eq 5021) "Trafic : une adresse oubliée qui revient est comptée deux fois ($cpt au lieu de 5021)"
    Write-Host "Info : Trafic, passage avec 5 000 destinations en $ms1 ms, après l'oubli en $ms2 ms ($n gardées)"
} catch { Test-Ok $false "Trafic : $($_.Exception.Message) $($_.InvocationInfo.PositionMessage)" }
$script:Traffic = $null

# ---------------------------------------------------------------------------
if ($echecs.Count) {
    foreach ($e in $echecs) { Write-Host "ÉCHEC  $e" -ForegroundColor Red }
    Write-Host "$($echecs.Count) échec(s) sur $nbTests vérifications." -ForegroundColor Red
    exit 1
}
Write-Host "Réseau (lag, Trafic, trace en veille) et petits textes : $nbTests vérifications, tout passe." -ForegroundColor Green
exit 0
