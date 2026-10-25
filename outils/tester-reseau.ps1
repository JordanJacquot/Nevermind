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
#   4. les petits textes : titre « Nevermind » des rapports, Mindset trouvé sur un autre disque, message d'erreur simple.
# Code de sortie : 0 si tout passe, 1 sinon.

param([switch]$VraiLagMon)

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$fichiers = Join-Path $racine 'OptiGame\fichiers'
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
        Write-Output "ok|$na|$na2|$nb|$actives"
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
    . ([scriptblock]::Create((Get-Definitions (Join-Path $fichiers 'OptiGame.ps1') @('Find-MindsetExe'))))
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
    public static double[] Series(int i) { return new double[0]; }
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
}

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
if ($echecs.Count) {
    foreach ($e in $echecs) { Write-Host "ÉCHEC  $e" -ForegroundColor Red }
    Write-Host "$($echecs.Count) échec(s) sur $nbTests vérifications." -ForegroundColor Red
    exit 1
}
Write-Host "Réseau (lag, Trafic, trace en veille) et petits textes : $nbTests vérifications, tout passe." -ForegroundColor Green
exit 0
