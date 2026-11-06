# Exporte les « juges » du bloc Réseau de Nevermind en Rust (rust\nevermind-reseau\tests\juges\*.json) : la version
# PowerShell (la référence) est chargée fonction par fonction, nourrie de données inventées (scans simulés, audits
# simulés, connexions simulées), et ce qu'elle rend est enregistré. Les tests Rust rejouent ces cas.
#
#   & 'outils\exporter-juges-reseau.ps1'      (depuis l'outil PowerShell : aucune fenêtre, rien sur le vrai PC)
#
# Rien ne touche le réseau ni le PC : chaque commande qui lirait ou changerait quelque chose est remplacée par une
# fausse, et celles qui ne devraient jamais servir lèvent une erreur.

param([int]$Graine = 20261106, [int]$NbScans = 260, [int]$NbAudits = 320, [int]$NbPages = 320)

$ErrorActionPreference = 'Stop'
$racine   = Split-Path $PSScriptRoot -Parent
$fichiers = Join-Path $racine 'Nevermind\fichiers'
$modules  = Join-Path $fichiers 'modules'
$sortie   = Join-Path $racine 'rust\nevermind-reseau\tests\juges'
$travail  = Join-Path ([IO.Path]::GetTempPath()) "nevermind-juges-reseau-$PID"
New-Item -ItemType Directory -Force -Path $sortie, $travail | Out-Null
$env:NEVERMIND_TEST = '1'
$env:COMPUTERNAME = 'PC-ESSAI'   # (seulement pour ce PowerShell : les titres « ce PC » sont les mêmes partout)
Add-Type -AssemblyName WindowsBase, PresentationCore, PresentationFramework

# ---------------------------------------------------------------------------
# Rien de réel : ces commandes sont interdites pendant l'export
# ---------------------------------------------------------------------------
foreach ($c in 'Get-CimInstance', 'Get-WmiObject', 'Start-Process', 'Stop-Process', 'Invoke-WebRequest', 'Invoke-RestMethod', 'Get-NetNeighbor',
               'Get-NetAdapter', 'Get-NetRoute', 'Get-NetIPAddress', 'Set-NetConnectionProfile', 'Clear-DnsClientCache', 'Get-DnsClientServerAddress',
               'Invoke-Aide', 'Set-Setting', 'Set-Reg', 'netsh', 'netsh.exe', 'Get-SmbShare', 'Get-NetFirewallProfile', 'Add-ChangementDefinitif',
               'Add-History', 'Open-Url', 'Show-DeviceDetail') {
    Set-Item -Path "function:script:$c" -Value ([scriptblock]::Create("throw 'Export : $c est interdit.'"))
}

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
function Get-Ref([string]$Fichier, [string[]]$Noms, [hashtable]$Remplacer = @{}) {
    $t = Get-Definitions (Join-Path $modules $Fichier) $Noms
    foreach ($k in $Remplacer.Keys) { $t = $t.Replace($k, $Remplacer[$k]) }
    $t
}

# Le faux ping (Measure-Latency) : il rend les temps donnés par le cas, cible par cible
$script:FauxPings = @{}
function New-FauxPing {
    $o = New-Object psobject
    $o | Add-Member ScriptMethod Send { param($h, $t) $q = $script:FauxPings[[string]$h]; $v = if ($q -and $q.Count) { $q[0] } else { $null }; if ($q -and $q.Count) { $q.RemoveAt(0) }
        if ($null -eq $v -or [double]$v -lt 0) { [pscustomobject]@{ Status = 'TimedOut'; RoundtripTime = 0 } } else { [pscustomobject]@{ Status = 'Success'; RoundtripTime = [long]$v } } }
    $o | Add-Member ScriptMethod Dispose { }
    $o
}

. ([scriptblock]::Create((Get-Definitions (Join-Path $modules 'interface.ps1') @('$Colors'))))
. ([scriptblock]::Create((Get-Ref 'ecran-pc.ps1' @('$TonsStatut'))))
. ([scriptblock]::Create((Get-Ref 'reseau.ps1' @('Get-SubnetIps', 'Get-Vendor', 'Get-DeviceKind', 'Set-LocalDevice', 'Invoke-NetworkScan', '$NetFilters', 'Show-NetDevices', 'Invoke-NetWatch', '$NetScanWork'))))
. ([scriptblock]::Create((Get-Ref 'reseau-avance.ps1' @('$ServiceLabels', '$MdnsKeys', '$CameraVendors', '$CameraWords', 'Get-OsGuess', 'Update-DeviceIdentity', 'Merge-NetDeep', 'Invoke-NetDeepScan', '$NetDeepWork'))))
. ([scriptblock]::Create((Get-Ref 'audit-reseau.ps1' @('$AuditPorts', '$AuditTips', '$AuditCats', 'Add-AuditCheck', 'Get-NetAuditChecks', 'Get-AuditColor', 'Get-AuditLabel', 'Update-NetAuditCard', 'Invoke-NetAudit', '$NetAuditWork'))))
. ([scriptblock]::Create((Get-Ref 'ecran-reseau.ps1' @('$SortesReseau', 'Get-SorteReseau', 'Get-PagesReseau'))))
. ([scriptblock]::Create((Get-Ref 'systeme.ps1' @('$VitessesNorme', 'Get-CapaciteCarte', 'Get-BridageCarte', 'Format-VitesseCarte', 'Get-TexteBridage', 'Test-AlerteCarte', '$DnsChoices'))))
. ([scriptblock]::Create((Get-Ref 'ecran-protection.ps1' @('Get-EtatSecuriteKit', 'Get-PagesProtection', 'Get-DernierDebit'))))
. ([scriptblock]::Create((Get-Ref 'onglets.ps1' @('Measure-Latency', 'Invoke-NetTest') @{ 'New-Object System.Net.NetworkInformation.Ping' = 'New-FauxPing' })))
. ([scriptblock]::Create((Get-Ref 'assistance.ps1' @('Test-HistoriqueDefinitif', 'Test-HistoriqueNote', 'Get-RetourDefinitif'))))
# UpnpAdresse (natif.cs) : la classe seule
$natif = [IO.File]::ReadAllText((Join-Path $modules 'natif.cs'), [Text.Encoding]::UTF8)
$debut = $natif.IndexOf('public static class UpnpAdresse')
Add-Type -TypeDefinition ("using System;`n" + $natif.Substring($debut))
# Le faux OGNative : la progression du scan
Add-Type -TypeDefinition 'public static class OGNative { public static double Progress; public static string Phase = ""; public static bool Cancel; public static int Found; }'

# ---------------------------------------------------------------------------
# Les fausses commandes du PC et de l'interface
# ---------------------------------------------------------------------------
$script:Journal = New-Object System.Collections.Generic.List[string]
function script:Write-Log([string]$Message) { $script:Journal.Add($Message) }
$script:Reglages = @{}
function script:Get-Setting([string]$Name, $Default = $null) { if ($script:Reglages.ContainsKey($Name)) { $script:Reglages[$Name] } else { $Default } }
$script:Statut = ''; $script:Messages = New-Object System.Collections.Generic.List[string]
function script:Set-Status([string]$Text) { $script:Statut = $Text }
function script:Set-Busy($b) { }
function script:Show-Message([string]$Text, $Icon) { $script:Messages.Add($Text) }
function script:Invoke-Safe([scriptblock]$Sb) { & $Sb }
function script:Update-ScanKit { }
function script:Start-Anim { }
function script:Start-WpfAnim { }
function script:Show-NetMap { }
function script:Show-NetAuditResult($A) { }
function script:Show-ResultTop { }
function script:Show-TestPanel($T) { }
function script:Set-TestState($a, $b) { }
function script:Set-TestButtons($m) { }
function script:New-Stepper($s) { @{ El = (New-Object System.Windows.Controls.StackPanel) } }
function script:Update-Stepper { }
function script:Update-NetInfo { }
function script:Show-NetHeroIdle { }
function script:New-Verdict { New-Object System.Windows.Controls.Border }
function script:New-DeviceTile($D, $I) { New-Object System.Windows.Controls.Border }
function script:New-Text([string]$Text) { $t = New-Object System.Windows.Controls.TextBlock; $t.Text = $Text; $t }
function script:New-Button([string]$Text) { $b = New-Object System.Windows.Controls.Button; $b.Content = $Text; $b }
function script:New-Card { New-Object System.Windows.Controls.Border }
function script:New-Grid { New-Object System.Windows.Controls.Grid }
function script:Add-ToGrid { }
function script:Get-Brush($c) { [System.Windows.Media.Brushes]::White }
function script:New-Thickness { [System.Windows.Thickness]::new(0) }
function script:Start-Sleep { }
function script:New-NetRadar {
    @{ El = (New-Object System.Windows.Controls.Grid); Dots = (New-Object System.Windows.Controls.Canvas); Rot = (New-Object System.Windows.Media.RotateTransform)
       Sweep = (New-Object System.Windows.Controls.Border); Num = (New-Object System.Windows.Controls.TextBlock); Icon = (New-Object System.Windows.Controls.TextBlock); Shown = 0 }
}
$script:Notes = New-Object System.Collections.Generic.List[object]
function script:Show-Notify([string]$Title, [string]$Text, $OnClick) { $script:Notes.Add(@{ Titre = $Title; Texte = $Text }) }
function script:New-ActionNotif([string]$Code, $Donnee) { $Code }
function script:Get-SecTrusted { @() }
function script:Get-BackupCount { 0 }
function script:Import-History { }
function script:Get-DetailsTechniques($H) { @() }
$script:Maintenant = [datetime]'2026-11-06T21:30:00'
function script:Get-Date { param([string]$Format) if ($Format) { $script:Maintenant.ToString($Format) } else { $script:Maintenant } }
$script:FauxNet = $null
function script:Get-ActiveNet { $script:FauxNet }
$script:FauxLocaux = @{}
function script:Get-LocalInterfaces { $script:FauxLocaux }
$script:FauxAsync = @{}
$script:ArgsAsync = @{}
function script:Invoke-Async($Sb, $A) {
    $nom = if ($Sb -eq $NetScanWork) { 'scan' } elseif ($Sb -eq $NetDeepWork) { 'deep' } elseif ($Sb -eq $NetAuditWork) { 'audit' } else { throw 'Export : Invoke-Async inattendu' }
    $script:ArgsAsync[$nom] = $A
    $script:FauxAsync[$nom]
}
function script:Read-JsonSur([string]$Path) {
    foreach ($p in $Path, "$Path.bak") {
        if (-not [IO.File]::Exists($p)) { continue }
        $t = [IO.File]::ReadAllText($p, [Text.Encoding]::UTF8)
        try { if (-not $t.Trim()) { throw 'vide' }; return (ConvertFrom-Json $t) } catch { continue }
    }
    $null
}
$script:Enregistre = @{}
function script:Save-JsonSur([string]$Path, $Objet) { $script:Enregistre[[IO.Path]::GetFileName($Path)] = (ConvertTo-Json -InputObject $Objet -Depth 6 -Compress) }

$KnownFile = Join-Path $travail 'appareils.json'
$AuditFile = Join-Path $travail 'audit.json'
$OuiFile = Join-Path $travail 'fabricants-absent.txt'

function New-Ui {
    $u = @{}
    foreach ($n in 'NetHero', 'NetScanInfo') { $u[$n] = New-Object System.Windows.Controls.StackPanel }
    foreach ($n in 'NetDevFilters', 'NetDevices') { $u[$n] = New-Object System.Windows.Controls.WrapPanel }
    foreach ($n in 'NetDevSummary', 'NetAuditText', 'NetAuditScore', 'SecScanNote') { $u[$n] = New-Object System.Windows.Controls.TextBlock }
    $u.NetDevHint = New-Object System.Windows.Controls.TextBlock; $u.NetDevHint.Text = 'Clique sur « Scanner le réseau » pour voir les appareils connectés.'
    $u.NetMapOverlay = New-Object System.Windows.Controls.Grid; $u.NetMapOverlay.Visibility = 'Collapsed'
    $u.NetAuditScoreBox = New-Object System.Windows.Controls.Border
    $u.BtnNetScan = New-Object System.Windows.Controls.Button; $u.BtnNetScan.Content = 'Scanner le réseau'
    $u.BtnNetAudit = New-Object System.Windows.Controls.Button; $u.BtnNetAudit.Content = 'Lancer l''audit'
    $u.BtnNetAuditView = New-Object System.Windows.Controls.Button; $u.BtnNetAuditView.Content = 'Voir le rapport'; $u.BtnNetAuditView.Visibility = 'Collapsed'
    $u.ChkNetWatch = New-Object System.Windows.Controls.CheckBox
    $u.TestBody = New-Object System.Windows.Controls.StackPanel
    $u.TestProgress = New-Object System.Windows.Controls.ProgressBar
    $u.TestPct = New-Object System.Windows.Controls.TextBlock
    $u.BtnTestStop = New-Object System.Windows.Controls.Button
    $u
}

# Un appareil tel que les tests Rust le lisent (chaque champ toujours présent, en texte, nombre, oui/non ou liste)
$ChampsTexte = 'Ip', 'Mac', 'Host', 'Vendor', 'Title', 'SvcText', 'MdnsHost', 'MdnsName', 'UpnpName', 'UpnpMaker', 'UpnpModel', 'UpnpType', 'NbName', 'NbGroup',
               'WsdTypes', 'WsdScopes', 'WsdName', 'WsdHardware', 'WebTitle', 'WebServer', 'Announced', 'Model', 'Maker', 'OsGuess', 'Adapter'
$ChampsOui = 'Self', 'Gateway', 'New', 'Hidden', 'Camera', 'Only6'
$ChampsListe = 'CameraWhy', 'Services', 'ServiceLabels', 'Ipv6', 'FoundBy'
function ConvertTo-AppareilJuge($D) {
    $o = [ordered]@{}
    foreach ($k in $ChampsTexte) { $o[$k] = [string]$D[$k] }
    $o.Ms = $(if ($null -ne $D.Ms) { [int]$D.Ms } else { $null })
    $o.Ttl = [int]$D.Ttl
    foreach ($k in $ChampsOui) { $o[$k] = [bool]$D[$k] }
    foreach ($k in $ChampsListe) { $o[$k] = @(@($D[$k]) | Where-Object { $null -ne $_ } | ForEach-Object { [string]$_ }) }
    $o.Ports = @(@($D.Ports) | Where-Object { $null -ne $_ } | ForEach-Object { [int]$_ })
    $o.Kind = [string]$D.KindInfo.Kind
    $o.Txt = [ordered]@{}; if ($D.Txt) { foreach ($k in $D.Txt.Keys) { $o.Txt[[string]$k] = [string]$D.Txt[$k] } }
    $o
}
# Et dans l'autre sens : un appareil de juge refait en appareil de Nevermind (pour la page, l'audit)
function ConvertFrom-AppareilJuge($J) {
    $d = @{}
    foreach ($p in $J.PSObject.Properties) { $d[$p.Name] = $p.Value }
    $d.KindInfo = @{ Kind = [string]$J.Kind }
    $d.Remove('Kind')
    $t = @{}; if ($J.Txt) { foreach ($p in $J.Txt.PSObject.Properties) { $t[$p.Name] = [string]$p.Value } }; $d.Txt = $t
    $d
}

# ---------------------------------------------------------------------------
# Les données inventées
# ---------------------------------------------------------------------------
$rnd = New-Object System.Random $Graine
function Pick($a) { $a[$rnd.Next($a.Count)] }
function Chance([double]$p) { $rnd.NextDouble() -lt $p }
$Oui = [ordered]@{
    'F0B429' = 'Apple, Inc.'; 'A4C361' = 'Apple Inc.'; '3C5AB4' = 'Google, Inc.'; '28C2DD' = 'AzureWave Technology Inc.'; '001788' = 'Signify Netherlands B.V.'
    'B827EB' = 'Raspberry Pi Foundation'; 'C83A35' = 'Tenda Technology Co.,Ltd.Dongguan branch'; '30B5C2' = 'TP-LINK TECHNOLOGIES CO.,LTD.'; '04D9F5' = 'ASUSTek COMPUTER INC.'
    '4C11AE' = 'Espressif Inc.'; 'BC5436' = 'Hangzhou Hikvision Digital Technology Co.,Ltd.'; '9C8ECD' = 'Amcrest Technologies'; '0024E4' = 'Withings'
    '7C2EBD' = 'Google, Inc.'; 'F4F5D8' = 'Google, Inc.'; '00D861' = 'Micro-Star INTL CO., LTD.'; '1C697A' = 'EliteGroup Computer Systems Co., LTD'
    '0050F2' = 'MICROSOFT CORP.'; '7CBB8A' = 'Nintendo Co., Ltd.'; '0CFE45' = 'Sony Interactive Entertainment Inc.'; '64B5C6' = 'LG Innotek'
    'E8F2E2' = 'LG Innotek'; '388B59' = 'Google, Inc.'; '9C3DCF' = 'NETGEAR'; 'E4F4C6' = 'NETGEAR'; '00248C' = 'ASUSTek COMPUTER INC.'
    '3CA9F4' = 'Intel Corporate'; 'A0369F' = 'Intel Corporate'; 'B0BE76' = 'TP-LINK TECHNOLOGIES CO.,LTD.'; 'D8EB46' = 'Sagemcom Broadband SAS'
    '00A0D1' = 'INVENTEC CORPORATION'; '001A11' = 'Google, Inc.'; '5C628B' = 'TP-Link Corporation Limited'; '000E58' = 'Sonos, Inc.'; '0017C9' = 'Samsung Electronics Co.,Ltd'
    '8CF5A3' = 'Samsung Electronics Co.,Ltd'; '38F73D' = 'Amazon Technologies Inc.'; 'F0272D' = 'Amazon Technologies Inc.'; '00E04C' = 'REALTEK SEMICONDUCTOR CORP.'
    '3C8D20' = 'Google, Inc.'; '0011D9' = 'TiVo'; 'AC84C6' = 'TP-LINK TECHNOLOGIES CO.,LTD.'; '001E06' = 'WIBRAIN'; 'E0CB4E' = 'ASUSTek COMPUTER INC.'
    '00265A' = 'D-Link Corporation'; '001B11' = 'D-Link Corporation'; '30918F' = 'Technicolor CH USA Inc.'; '0007CB' = 'FREEBOX SAS'; '14144B' = 'Ruijie Networks Co.,LTD'
    'C0C1C0' = 'Cisco-Linksys, LLC'; '000D4B' = 'Roku, Inc.'; 'DCA632' = 'Raspberry Pi Trading Ltd'; '702AD5' = 'Samsung Electronics Co.,Ltd'; '88DEA9' = 'Roku, Inc.'
    '44D9E7' = 'Ubiquiti Inc'; '24A43C' = 'Ubiquiti Networks Inc.'; '00408C' = 'Axis Communications AB'; 'ECB5FA' = 'Philips Lighting BV'; '00C0EE' = 'KYOCERA Display Corporation'
    '3C2AF4' = 'Brother Industries, LTD.'; '0026AB' = 'Seiko Epson Corporation'; '00805F' = 'Hewlett Packard'; '4CEB42' = 'Intel Corporate'; '70B3D5' = 'IEEE Registration Authority'
    '1C1B0D' = 'GIGA-BYTE TECHNOLOGY CO.,LTD.'; '90E2BA' = 'Intel Corporation'; 'A4BB6D' = 'Dell Inc.'; '00FF11' = 'Société Générale d''Électronique, S.A.S.'
}
$Profils = @(
    @{ Id = 'tel'; Oui = @('F0B429', 'A4C361', '8CF5A3', '702AD5'); Noms = @('iPhone-de-Lea', 'Galaxy-S24', 'Pixel-8', 'redmi-note', 'android-7f3a9c', '') }
    @{ Id = 'tv'; Oui = @('64B5C6', 'E8F2E2', '0017C9', '000D4B'); Noms = @('LGwebOSTV', 'Samsung-TV', 'BRAVIA-KD55', 'Roku-Ultra', '') }
    @{ Id = 'cast'; Oui = @('3C5AB4', '7C2EBD', 'F4F5D8', '388B59'); Noms = @('Chromecast', 'Google-Home-Mini', 'Nest-Hub', '') }
    @{ Id = 'imp'; Oui = @('3C2AF4', '0026AB', '00805F'); Noms = @('BRN3C2AF4', 'EPSON-ET2750', 'HP-LaserJet', '') }
    @{ Id = 'cam'; Oui = @('BC5436', '9C8ECD', '00408C', '4C11AE'); Noms = @('IPC-Entree', 'camera-jardin', 'NVR-Maison', '') }
    @{ Id = 'jeu'; Oui = @('7CBB8A', '0CFE45', '0050F2'); Noms = @('PS5-123', 'Nintendo-Switch', 'XboxOne', '') }
    @{ Id = 'iot'; Oui = @('4C11AE', '001788', 'ECB5FA', '38F73D', 'F0272D', '0024E4'); Noms = @('ESP_3A2B1C', 'Philips-hue', 'Echo-Dot', 'tapo-plug', 'shelly1pm', '') }
    @{ Id = 'pc'; Oui = @('04D9F5', '00D861', '3CA9F4', 'A0369F', '1C1B0D', 'A4BB6D', '00E04C'); Noms = @('DESKTOP-4KQ2', 'LAPTOP-JORDAN', 'PC-Salon', 'NAS-DS220', '') }
    @{ Id = 'wifi'; Oui = @('30B5C2', 'B0BE76', '04D9F5', '9C3DCF', '44D9E7', '24A43C'); Noms = @('RT-AX88U-55C8', 'Deco-M5', 'Orbi-Satellite', 'repeteur-etage', 'UniFi-AP', '') }
    @{ Id = 'box'; Oui = @('D8EB46', '0007CB', '30918F'); Noms = @('bbox', 'freebox-player', 'livebox-decodeur', '') }
    @{ Id = 'inconnu'; Oui = @('70B3D5', '00A0D1', '001E06', '00FF11', 'ZZ'); Noms = @('', '', 'appareil', 'lan', 'LAN', 'Nom très long avec des accents éèàçù et des espaces qui ne finit jamais vraiment pour voir comment tout se passe') }
)
function New-Mac([string]$Oui) {
    if ($Oui -eq 'ZZ') { return '' }
    $b = ($Oui -replace '(..)(?!$)', '$1-')
    "$b-$('{0:X2}' -f $rnd.Next(256))-$('{0:X2}' -f $rnd.Next(256))-$('{0:X2}' -f $rnd.Next(256))"
}
function New-MacPrivee { $x = @('2', '6', 'A', 'E'); "$('{0:X1}' -f $rnd.Next(16))$(Pick $x)-$('{0:X2}' -f $rnd.Next(256))-$('{0:X2}' -f $rnd.Next(256))-$('{0:X2}' -f $rnd.Next(256))-$('{0:X2}' -f $rnd.Next(256))-$('{0:X2}' -f $rnd.Next(256))" }
function Get-CasseAuHasard([string]$s) { if (Chance 0.15) { $s.ToLower() } else { $s } }

function New-ScanAuHasard([int]$n) {
    $base = Pick @('192.168.1', '192.168.0', '10.0.0', '192.168.50', '172.16.4')
    $prefix = Pick @(24, 24, 24, 23, 16, 25, 8)
    $selfN = 2 + $rnd.Next(200); $gwN = Pick @(1, 1, 1, 254, 100)
    if ($gwN -eq $selfN) { $selfN++ }
    $ipSelf = "$base.$selfN"; $gw = "$base.$gwN"
    $wifi = Chance 0.5
    $net = @{ IfIndex = 7 + $rnd.Next(20); Gateway = $gw; Name = $(if ($wifi) { 'Wi-Fi' } else { 'Ethernet' }); Desc = 'Carte d''essai'; Speed = '1 Gbps'; Wifi = $wifi
        Guid = '{0F1E2D3C-4B5A-6978-8796-A5B4C3D2E1F0}'; Mac = (New-Mac (Pick @('04D9F5', '3CA9F4', '00E04C'))); Ip = $ipSelf; Prefix = $prefix; Dns = $gw; Bps = 1000000000; Vitesses = @(); VitessesReg = @() }
    if (Chance 0.05) { $net.Ip = $null }
    $alive = New-Object System.Collections.ArrayList; $arp = New-Object System.Collections.ArrayList; $names = New-Object System.Collections.ArrayList
    $deep = @{ Arp = @(); Mdns = @(); Wsd = @(); Upnp = @(); V6 = @(); NetBios = @(); Ports = @(); Titles = @() }
    $pris = @{ $selfN = $true }
    $appareils = @()
    # La box
    $appareils += @{ N = $gwN; Profil = 'box-gw' }
    for ($i = 0; $i -lt $n; $i++) {
        $k = 2 + $rnd.Next(252); if ($pris[$k]) { continue }; $pris[$k] = $true
        $appareils += @{ N = $k; Profil = (Pick $Profils).Id }
    }
    $locaux = @{}
    if (Chance 0.25) { $k = 2 + $rnd.Next(252); if (-not $pris[$k]) { $pris[$k] = $true; $locaux["$base.$k"] = @{ Mac = $(if (Chance 0.8) { New-Mac 'A0369F' } else { '' }); Name = 'Wi-Fi 2'; Desc = 'Intel(R) Wi-Fi 6 AX201'; Wifi = (Chance 0.6); Up = (Chance 0.5) }; $appareils += @{ N = $k; Profil = 'local' } } }
    if (Chance 0.3) { $locaux['169.254.10.20'] = @{ Mac = ''; Name = 'Ethernet 3'; Desc = 'VirtualBox'; Wifi = $false; Up = $false } }
    $locaux[$ipSelf] = @{ Mac = $net.Mac; Name = $net.Name; Desc = $net.Desc; Wifi = $wifi; Up = $true }
    $known = @{}
    foreach ($a in $appareils) {
        $ip = "$base.$($a.N)"
        $p = if ($a.Profil -eq 'box-gw') { @($Profils | Where-Object { $_.Id -eq 'box' })[0] } elseif ($a.Profil -eq 'local') { @($Profils | Where-Object { $_.Id -eq 'pc' })[0] } else { @($Profils | Where-Object { $_.Id -eq $a.Profil })[0] }
        $mac = if ($p.Id -eq 'tel' -and (Chance 0.5)) { New-MacPrivee } else { New-Mac (Pick $p.Oui) }
        if ($a.Profil -eq 'local' -and $locaux[$ip].Mac -and (Chance 0.5)) { $mac = $locaux[$ip].Mac }
        $mac = Get-CasseAuHasard $mac
        $ping = $a.Profil -eq 'box-gw' -or (Chance 0.6)
        $ms = if (Chance 0.2) { 0 } else { $rnd.Next(1, 120) }
        $ttl = Pick @(64, 128, 255, 63, 127, 254, 0)
        $etat = Pick @('Reachable', 'Stale', 'Delay', 'Probe', 'Unreachable', 'Incomplete', 'Permanent', 'reachable')
        if ($ping) { [void]$alive.Add("$ip|$ms|$ttl") }
        if ($ping -and (Chance 0.85)) { [void]$arp.Add("$ip|$mac|$etat") }
        elseif (-not $ping -and (Chance 0.6)) { [void]$arp.Add("$ip|$mac|$etat") }
        elseif (-not $ping) { $deep.Arp += "$ip|$mac" }
        if ($ping -and (Chance 0.1)) { $deep.Arp += "$ip|$mac" }
        $nom = Pick $p.Noms
        if ($nom -and (Chance 0.7)) { [void]$names.Add("$ip|$nom$(Pick @('', '.home', '.lan', '.local', '.LAN', '.box', '.fritz.box', '.bbox', '.station', '.localdomain', '.maison'))") }
        elseif (Chance 0.2) { [void]$names.Add("$ip|$ip") } elseif (Chance 0.2) { [void]$names.Add("$ip|") }
        # Recherche approfondie
        $inst = if ($nom) { $nom } else { "Appareil $($a.N)" }
        switch ($p.Id) {
            'tel' { if (Chance 0.5) { $deep.Mdns += "$ip|ptr|_companion-link._tcp.local|$inst._companion-link._tcp.local"; $deep.Mdns += "$ip|txt|$inst._companion-link._tcp.local|rpMd=iPhone15,2" } }
            'tv' { if (Chance 0.6) { $deep.Mdns += "$ip|txt|[LG] webOS TV OLED55._airplay._tcp.local|model=OLED55C3"; $deep.Mdns += "$ip|ptr|_airplay._tcp.local|[LG] webOS TV OLED55._airplay._tcp.local" }
                   if (Chance 0.5) { $deep.Upnp += "$ip|$inst|LG Electronics|WEBOS|55C3|urn:schemas-upnp-org:device:MediaRenderer:1" } }
            'cast' { $deep.Mdns += "$ip|ptr|_googlecast._tcp.local|Chromecast-$($rnd.Next(1000))-ABCDEF0123456789._googlecast._tcp.local"; $deep.Mdns += "$ip|txt|Chromecast-x._googlecast._tcp.local|fn=$(Pick @('Salon', 'Chambre de Léa', 'Cuisine'))"; $deep.Mdns += "$ip|txt|Chromecast-x._googlecast._tcp.local|md=Chromecast Ultra"; if (Chance 0.5) { $deep.Ports += "$ip|8008" } }
            'imp' { $deep.Mdns += "$ip|ptr|_ipp._tcp.local|$inst._ipp._tcp.local"; $deep.Mdns += "$ip|txt|$inst._ipp._tcp.local|usb_MFG=$(Pick @('Brother', 'EPSON', 'HP'))"; $deep.Mdns += "$ip|txt|$inst._ipp._tcp.local|usb_MDL=$(Pick @('HL-L2350DW', 'ET-2750', 'LaserJet M110'))"; $deep.Ports += "$ip|9100"; $deep.Ports += "$ip|631"; $deep.Ports += "$ip|80"
                    if (Chance 0.4) { $deep.Wsd += "$ip|wsdp:Device wprt:PrintDeviceType|http://schemas.example/name/Imprimante%20Bureau http://schemas.example/hardware/HL-L2350DW" } }
            'cam' { if (Chance 0.6) { $deep.Ports += "$ip|554" }; if (Chance 0.5) { $deep.Ports += "$ip|8000" }; if (Chance 0.3) { $deep.Ports += "$ip|37777" }
                    if (Chance 0.4) { $deep.Wsd += "$ip|dn:NetworkVideoTransmitter tds:Device|onvif://www.onvif.org/name/Camera%20Entree onvif://www.onvif.org/hardware/DS-2CD2143" }
                    if (Chance 0.5) { $deep.Titles += "http://$($ip):80|$(Pick @('IP Camera', 'NVR Login', 'Web Service', ''))|$(Pick @('App-webs/', 'lighttpd', ''))"; $deep.Ports += "$ip|80" }
                    if (Chance 0.2) { $deep.Mdns += "$ip|ptr|_rtsp._tcp.local|$inst._rtsp._tcp.local" } }
            'jeu' { if (Chance 0.3) { $deep.Upnp += "$ip|$inst|Sony Interactive Entertainment|PlayStation 5||urn:schemas-upnp-org:device:Basic:1" } }
            'iot' { if (Chance 0.5) { $deep.Mdns += "$ip|ptr|_hap._tcp.local|$inst._hap._tcp.local"; $deep.Mdns += "$ip|txt|$inst._hap._tcp.local|md=$(Pick @('Hue Bridge', 'Eve Energy', 'Meross Plug'))" }
                    if (Chance 0.3) { $deep.Mdns += "$ip|ptr|_hue._tcp.local|Philips Hue - 1A2B3C._hue._tcp.local" } }
            'pc' { if (Chance 0.6) { $deep.NetBios += "$ip|$(Pick @('DESKTOP-4KQ2', 'NAS', 'PC-SALON', 'LAPTOP'))|$(Pick @('WORKGROUP', 'MAISON', ''))" }
                   if (Chance 0.4) { $deep.Mdns += "$ip|ptr|_smb._tcp.local|$inst._smb._tcp.local" }; if (Chance 0.3) { $deep.Ports += "$ip|3389" }; if (Chance 0.2) { $deep.Ports += "$ip|445" }
                   if (Chance 0.2) { $deep.Wsd += "$ip|wsdp:Device pub:Computer|" } }
            'wifi' { if (Chance 0.5) { $deep.Upnp += "$ip|WPS Access Point|TP-Link|Archer AX55|1.0|urn:schemas-wifialliance-org:device:WFADevice:1" }
                     if (Chance 0.6) { $deep.Upnp += "$ip|$(Pick @('RT-AX88U', 'Deco M5', 'Orbi'))|$(Pick @('ASUSTeK', 'TP-Link', 'NETGEAR'))|$(Pick @('RT-AX88U', 'M5', 'RBS50'))|$(Pick @('3.0', '', '1'))|urn:schemas-upnp-org:device:InternetGatewayDevice:1" }
                     $deep.Ports += "$ip|80"; if (Chance 0.5) { $deep.Ports += "$ip|443"; $deep.Titles += "https://$($ip):443|$(Pick @('ASUS Login', 'Deco', 'Routeur'))|httpd" } }
            'box' { $deep.Ports += "$ip|80"; if (Chance 0.5) { $deep.Ports += "$ip|443" }; if (Chance 0.4) { $deep.Titles += "http://$($ip):80|$(Pick @('Bbox', 'Freebox OS', 'Livebox', 'Box | Accueil'))|" }
                    if (Chance 0.5) { $deep.Upnp += "$ip|$(Pick @('Bbox', 'Freebox Server', 'Livebox 6'))|$(Pick @('Sagemcom', 'Freebox SAS', 'Orange'))|$(Pick @('F@st5330b', 'Server', 'LB6'))||urn:schemas-upnp-org:device:InternetGatewayDevice:1" } }
            'inconnu' { if (Chance 0.2) { $deep.Mdns += "$ip|a|$(Pick @('esp32-cam', 'mystery', 'Appareil'))$(Pick @('.local', '.LOCAL', ''))|$ip" }; if (Chance 0.15) { $deep.Titles += "http://$($ip):8080|Camera|" } }
        }
        if (Chance 0.15) { $deep.V6 += "fe80::$('{0:x}' -f $rnd.Next(65536)):$('{0:x}' -f $rnd.Next(65536))|$($mac.ToUpper())" }
        if ($mac -and (Chance 0.55)) { $known[$mac.ToUpper()] = Pick @('2026-01-15', '2025-12-31', '2026-11-01', '') }
    }
    # Appareils visibles seulement en IPv6, ou par la demande ARP seule, et des lignes à jeter
    if (Chance 0.2) { $deep.V6 += "fe80::1234:$('{0:x}' -f $rnd.Next(65536))|$(New-MacPrivee)" }
    if (Chance 0.2) { $deep.Arp += "10.99.99.99|AA-BB-CC-DD-EE-FF" }
    if (Chance 0.2) { [void]$arp.Add("224.0.0.251|01-00-5E-00-00-FB|Permanent") }
    if (Chance 0.1) { $deep.Mdns += "$gw|ptr|_services._dns-sd._udp.local|_http._tcp.local" }
    if (Chance 0.1) { $deep.Mdns += "$gw|txt|bad" }
    if (Chance 0.1) { $deep.Upnp += "$base.250|absent|x|y|z|urn:x:device:Test:1" }
    if (Chance 0.1) { $deep.Ports += "$base.251|22" }
    $knownJson = switch ($rnd.Next(10)) {
        0 { $null } 1 { '[]' } 2 { '{}' } 3 { (ConvertTo-Json @($known.Keys | Select-Object -First 3) -Compress) } 4 { '"' + @($known.Keys + 'AA-AA-AA-AA-AA-AA')[0] + '"' } 5 { '{ abîmé' }
        default { ConvertTo-Json $known -Compress }
    }
    @{ Net = $net; Alive = @($alive); Arp = @($arp); Names = @($names); Deep = $(if (Chance 0.08) { @{ Error = 'Délai dépassé' } } else { $deep }); Known = $knownJson; Locaux = $locaux }
}

# ---------------------------------------------------------------------------
# Les juges
# ---------------------------------------------------------------------------
function Save-Juge([string]$Nom, $Cas) {
    $json = ConvertTo-Json -InputObject @($Cas) -Depth 30 -Compress
    [IO.File]::WriteAllText((Join-Path $sortie "$Nom.json"), $json, (New-Object Text.UTF8Encoding $false))
    Write-Host "$Nom : $(@($Cas).Count) cas"
}
$OuiMap = @{}; foreach ($k in $Oui.Keys) { $OuiMap[$k] = $Oui[$k] }
[IO.File]::WriteAllText((Join-Path $sortie 'fabricants.json'), (ConvertTo-Json $Oui -Compress), (New-Object Text.UTF8Encoding $false))

# 1. Le fabricant d'une adresse physique
$cas = @(foreach ($m in @('', 'F0-B4-29-01-02-03', 'f0:b4:29:01:02:03', 'F0B429', 'F0B4', '3C-5A-B4-AA-BB-CC', '02-11-22-33-44-55', 'DA-A1-19-00-00-01', '00-FF-11-00-00-00',
                           '30-B5-C2-11-22-33', 'D8-EB-46-00-00-00', '00-17-88-00-00-00', 'BC-54-36-00-00-01', '00-40-8C-00-00-00', 'AB-CD-EF-01-02-03', '0E-00-00-00-00-00', '0C-FE-45-00-00-00',
                           '3C-2A-F4-00-00-00', '00-26-AB-12-12-12', '44-D9-E7-00-00-00', '00-C0-EE-00-00-00', '70-B3-D5-01-01-01', '1C-1B-0D-00-00-00', 'a4bb6d000000', '00-0E-58-00-00-00', '3C-A9-F4-00-00-00')) {
    $script:Oui = $OuiMap
    @{ mac = $m; vendeur = (Get-Vendor $m) }
})
$script:Oui = $null
$cas += @{ mac = 'F0-B4-29-01-02-03'; sansFabricants = $true; vendeur = (Get-Vendor 'F0-B4-29-01-02-03') }
Save-Juge 'vendeur' $cas

# 2. Les adresses du réseau à scanner
$cas = @(foreach ($c in @(@('192.168.1.20', 24), @('192.168.1.20', 16), @('10.0.0.5', 8), @('192.168.1.130', 25), @('192.168.1.130', 26), @('192.168.1.3', 30), @('192.168.1.3', 31), @('192.168.1.3', 32),
                         @('172.16.4.250', 23), @('192.168.0.255', 24), @('0.0.0.1', 24), @('255.255.255.1', 28), @('192.168.1.200', 27))) {
    @{ ip = $c[0]; prefixe = $c[1]; ips = @(Get-SubnetIps $c[0] $c[1]) }
})
Save-Juge 'sous-reseau' $cas

# 3. UPnP : une adresse annoncée n'est ouverte que si c'est http:// vers l'appareil
$cas = @(foreach ($c in @(
    @('http://192.168.1.10:49152/rootDesc.xml', '192.168.1.10'), @('http://192.168.1.10/', '192.168.1.10'), @('file:///C:/Windows/win.ini', '192.168.1.10'),
    @('file://192.168.1.10/partage/x.xml', '192.168.1.10'), @('\\192.168.1.10\partage\x.xml', '192.168.1.10'), @('http://192.168.1.11/rootDesc.xml', '192.168.1.10'),
    @('https://192.168.1.10/rootDesc.xml', '192.168.1.10'), @('http://moi:secret@192.168.1.10/', '192.168.1.10'), @('http://box.local/rootDesc.xml', '192.168.1.10'),
    @('', '192.168.1.10'), @('http://192.168.1.10/', ''), @('  http://192.168.1.10:80/desc.xml  ', ' 192.168.1.10 '), @('HTTP://192.168.1.10/x', '192.168.1.10'),
    @('http://[fe80::1]/x', 'fe80::1'), @('http://[::ffff:192.168.1.10]/x', '192.168.1.10'), @('http://192.168.1.10:99999/x', '192.168.1.10'), @('http://192.168.001.010/x', '192.168.1.10'),
    @('http://3232235786/x', '192.168.1.10'), @('http://0xC0A8010A/x', '192.168.1.10'), @('http:/192.168.1.10/x', '192.168.1.10'), @('http://192.168.1.10', '192.168.1.10'),
    @('http://192.168.1.10./x', '192.168.1.10'), @('http://192.168.1/x', '192.168.0.1'), @('ftp://192.168.1.10/x', '192.168.1.10'), @('http://192.168.1.10#x', '192.168.1.10'),
    @('http://192.168.1.10?x=1', '192.168.1.10'), @('http://@192.168.1.10/', '192.168.1.10'), @('http://192.168.1.10:/x', '192.168.1.10'), @('//192.168.1.10/x', '192.168.1.10'),
    @('http://[FE80::1]/', 'fe80::1'), @('http://192.168.1.10/x', '::ffff:192.168.1.10'), @('http://192.168.1.10\x', '192.168.1.10'), @("http://192.168.1.10/`tx", '192.168.1.10'))) {
    @{ adresse = $c[0]; ip = $c[1]; permise = [UpnpAdresse]::Permise($c[0], $c[1]) }
})
Save-Juge 'upnp' $cas

# 4. Le scan complet : assemblage, recherche approfondie, appareils connus, textes de la liste
$cas = @()
for ($i = 0; $i -lt $NbScans; $i++) {
    $s = New-ScanAuHasard ($rnd.Next(0, 22))
    $script:Oui = $null; $script:NetList = $null; $script:NetScanning = $false; $script:KnownDevices = $null; $script:NetFilter = $null; $script:NetFirstScan = $null
    $script:Journal.Clear(); $script:Messages.Clear(); $script:Statut = ''; $script:Enregistre = @{}
    $ui = New-Ui
    $script:FauxNet = $s.Net; $script:FauxLocaux = $s.Locaux
    $avecOui = Chance 0.85
    $script:FauxAsync = @{ scan = $(if (Chance 0.03) { @{ Error = 'Accès refusé' } } else { @{ Alive = $s.Alive; Arp = $s.Arp; Names = $s.Names; OuiMap = $(if ($avecOui) { $OuiMap } else { $null }) } }); deep = $s.Deep }
    Remove-Item -LiteralPath $KnownFile, "$KnownFile.bak" -ErrorAction SilentlyContinue
    if ($null -ne $s.Known) { [IO.File]::WriteAllText($KnownFile, $s.Known) }
    $err = ''
    try { Invoke-NetworkScan } catch { $err = $_.Exception.Message }
    $cas += [ordered]@{
        entree = [ordered]@{ net = $s.Net; locaux = $s.Locaux; scan = $(if ($script:FauxAsync.scan.Error) { $script:FauxAsync.scan } else { @{ Alive = $s.Alive; Arp = $s.Arp; Names = $s.Names } }); deep = $s.Deep; connus = $s.Known; fabricants = $avecOui; aujourdhui = '2026-11-06'; poste = 'PC-ESSAI' }
        sortie = [ordered]@{
            erreur = $err
            appareils = @(@($script:NetList) | Where-Object { $_ } | ForEach-Object { ConvertTo-AppareilJuge $_ })
            connus = $(if ($script:Enregistre['appareils.json']) { ConvertFrom-Json $script:Enregistre['appareils.json'] } else { $null })
            resume = [string]$ui.NetDevSummary.Text; conseil = [string]$ui.NetDevHint.Text; premier = [bool]$script:NetFirstScan
            statut = $script:Statut; messages = @($script:Messages); ipsScan = $(if ($script:ArgsAsync.scan) { @($script:ArgsAsync.scan.Ips).Count } else { 0 })
            manquants = $(if ($script:ArgsAsync.deep) { @($script:ArgsAsync.deep.Missing).Count } else { 0 }); trouves = @(if ($script:ArgsAsync.deep) { $script:ArgsAsync.deep.Found })
        }
    }
    $script:ArgsAsync = @{}
}
Save-Juge 'scan' $cas

# 5. La page Réseau (et la carte de l'audit, les textes de la liste)
$devsScans = @($cas | ForEach-Object { , @($_.sortie.appareils) } | Where-Object { $_.Count })
$cas = @()
for ($i = 0; $i -lt $NbPages; $i++) {
    $ui = New-Ui
    $liste = if ($i -lt 6) { @() } else { @((Pick $devsScans) | ForEach-Object { ConvertFrom-AppareilJuge ((ConvertTo-Json $_ -Depth 6) | ConvertFrom-Json) }) }
    if ($liste.Count -and (Chance 0.3)) { foreach ($d in $liste) { if (Chance 0.2) { $d.New = $true }; if (Chance 0.1) { $d.Camera = $true }; if (Chance 0.15) { $d.Hidden = -not $d.Hidden }; if (Chance 0.1) { $d.Ms = $null } } }
    $script:NetList = if ($liste.Count) { $liste } elseif (Chance 0.5) { $null } else { @() }
    $script:NetFirstScan = Chance 0.3
    $script:NetFilter = Pick @($null, 'all', 'all', 'hidden', 'cam', 'new')
    $script:NetScanning = Chance 0.2
    [OGNative]::Progress = Pick @(0.0, 12.5, 70.0, 72.0, 80.0, 92.0, 100.0, 33.333, 2.5, 3.5, 99.5)
    $script:Kit = @{ Actif = $true; ReseauFiltre = (Pick @($null, '', 'all', 'hidden', 'new', 'cam', 'inconnu')); ScanTexte = (Pick @($null, '', 'Recherche des appareils...', 'Identification des fabricants...')); ScanTrouves = (Pick @(0, 1, 2, 17)) }
    $script:NetAuditSaved = $null; $script:NetAudit = $null
    Remove-Item -LiteralPath $AuditFile, "$AuditFile.bak" -ErrorAction SilentlyContinue
    $auditTexte = switch ($rnd.Next(8)) {
        0 { $null } 1 { $null } 2 { '{"Score":92,"Date":"05/11/2026 à 20:14","Bad":0,"Warn":1}' } 3 { '{"Score":61,"Date":"01/11/2026 à 09:00","Bad":1,"Warn":2}' }
        4 { '{"Score":20,"Date":"30/10/2026 à 23:59","Bad":3,"Warn":3}' } 5 { '{"Score":85,"Date":"06/11/2026 à 07:05","Bad":0,"Warn":0}' } 6 { '{ abîmé' } 7 { '{"Score":59,"Date":"","Bad":2,"Warn":0}' }
    }
    if ($auditTexte) { [IO.File]::WriteAllText($AuditFile, $auditTexte) }
    $auditComplet = [bool]($auditTexte -and (Chance 0.4))
    if ($auditComplet) { $script:NetAudit = @{ Score = 1 } }
    $ui.ChkNetWatch.IsChecked = Chance 0.3
    if (@($script:NetList | Where-Object { $_ }).Count) { Show-NetDevices }
    Update-NetAuditCard
    $pages = Get-PagesReseau
    $cas += [ordered]@{
        entree = [ordered]@{ appareils = @(@($liste) | ForEach-Object { ConvertTo-AppareilJuge $_ }); premier = [bool]$script:NetFirstScan; filtreListe = $(if ($null -eq $script:NetFilter) { '' } else { [string]$script:NetFilter })
            scan = [bool]$script:NetScanning; progression = [OGNative]::Progress; filtre = [string]$script:Kit.ReseauFiltre; scanTexte = [string]$script:Kit.ScanTexte; scanTrouves = [int]$script:Kit.ScanTrouves
            audit = $auditTexte; auditComplet = $auditComplet; surveillance = [bool]$ui.ChkNetWatch.IsChecked }
        sortie = [ordered]@{ pages = $pages; resume = [string]$ui.NetDevSummary.Text; conseil = [string]$ui.NetDevHint.Text }
    }
}
Save-Juge 'pages-reseau' $cas

# 6. L'audit : les points, la note, la carte
function New-AuditAuHasard($Devs, $Net) {
    $gw = $Net.Gateway
    $ips = @($Devs | Where-Object { -not $_.Self } | ForEach-Object { $_.Ip })
    $R = @{}
    if (Chance 0.8) { $R.Wifi = @{ Ssid = (Pick @('Livebox-12AB', 'Freebox', 'Wi-Fi de Léa', 'Bbox-XYZ', '')); Auth = (Pick @('WPA3-Personnel', 'WPA2-Personnel', 'WPA2 - Personnel', 'WPA-Personnel', 'WEP', 'Ouvrir', 'Open', 'Ouvert', 'Inconnu', '', 'WPA2-Enterprise', 'wpa3-sae')); Cipher = (Pick @('CCMP', 'TKIP', 'GCMP', 'WEP', '', 'tkip')) } }
    $R.Profiles = @(for ($k = 0; $k -lt $rnd.Next(0, 6); $k++) { "$(Pick @('Gare SNCF', 'McDo WiFi', 'Maison', 'Hotel Ibis', 'Wi-Fi de Léa', 'FreeWifi'))$k|$(Pick @('WPA2-Personnel', 'Ouvrir', 'Open'))|$(Pick @('CCMP', 'Aucun', 'WEP'))|$(Pick @('0', '1'))|$(Pick @('0', '1'))" })
    if (Chance 0.1) { $R.Profiles += 'ligne|abîmée' }
    $R.Ssdp = @()
    if (Chance 0.3) { $R.Ssdp += "$gw|urn:schemas-wifialliance-org:service:WFAWLANConfig:1|http://$gw/wps.xml" }
    foreach ($ip in $ips) { if (Chance 0.08) { $R.Ssdp += "$ip|urn:schemas-wifialliance-org:device:WFADevice:1|http://$ip/x" } }
    if (Chance 0.5) { $R.Ssdp += "$gw|urn:schemas-upnp-org:device:InternetGatewayDevice:1|http://$($gw):5000/rootDesc.xml" }
    if (Chance 0.5) {
        $maps = @(for ($k = 0; $k -lt $rnd.Next(0, 5); $k++) { $int = Pick @(3074, 22, 23, 80, 445, 3389, 25565, 554, 8080, 9295, 21, 5900); "$($int + (Pick @(0, 0, 1000)))|$(Pick @('UDP', 'TCP'))|$int|$(Pick @($ips + @($gw, '192.168.1.99')))|$(Pick @('Xbox', 'Minecraft', '', 'Camera P2P', 'NAT-PMP|x'))" })
        if (Chance 0.1) { $maps += 'abîmé|x' }
        $R.Igd = @{ Maps = $maps; External = (Pick @('', '82.64.12.3', '10.12.0.4', '100.64.3.2', '100.127.255.1', '100.128.0.1', '172.20.1.1', '172.32.1.1', '192.168.0.4')) }
    }
    $R.Dns = @{ Servers = @(Pick @(@($gw), @('1.1.1.1', '1.0.0.1'), @('8.8.8.8'), @('192.168.1.53', 'fe80::1'), @('89.2.0.1', '89.2.0.2'), @(), @('9.9.9.9', '9.9.9.9'))); Redirect = @(); NxHijack = '' }
    if (Chance 0.1) { $R.Dns.Redirect = @('one.one.one.one répond 10.0.0.1 au lieu de 1.1.1.1') }
    if (Chance 0.15) { $R.Dns.NxHijack = '92.242.132.24' }
    $R.Ports = @(foreach ($ip in @($ips + $gw)) { foreach ($p in $AuditPorts) { if (Chance 0.07) { "$ip|$p" } } })
    if (Chance 0.4) { $R.Ports += "$gw|80" }
    $R.Pc = @{ Smb1 = (Chance 0.15); Smb1Client = (Chance 0.1); Rdp = (Chance 0.2); FirewallOff = @(if (Chance 0.15) { Pick @('Public', 'Private', 'Domain') }); Category = (Pick @('Private', 'Public', 'DomainAuthenticated', '')); Shares = @() }
    if (Chance 0.3) { $R.Pc.Shares += "Musique|D:\Musique|$(Pick @('Read', 'Full', 'Change', 'Read,Change', ''))" }
    if (Chance 0.2) { $R.Pc.Shares += "Photos|C:\Users\Léa\Photos|" }
    $R
}
$cas = @()
for ($i = 0; $i -lt $NbAudits; $i++) {
    $liste = @((Pick $devsScans) | ForEach-Object { ConvertFrom-AppareilJuge ((ConvertTo-Json $_ -Depth 6) | ConvertFrom-Json) })
    if (Chance 0.3) { foreach ($d in $liste) { if (Chance 0.15) { $d.New = $true }; if (Chance 0.1) { $d.Camera = $true; $d.CameraWhy = @('port typique des enregistreurs vidéo', 'diffuse un flux vidéo (RTSP)') } } }
    $gwD = @($liste | Where-Object { $_.Gateway })[0]; $selfD = @($liste | Where-Object { $_.Self })[0]
    $net = @{ IfIndex = 12; Gateway = $(if ($gwD) { $gwD.Ip } else { '192.168.1.1' }); Ip = $(if ($selfD) { $selfD.Ip } elseif (Chance 0.5) { '192.168.1.42' } else { $null }); Wifi = $true }
    $R = New-AuditAuHasard $liste $net
    $script:NetList = $liste; $script:FauxNet = $net; $script:TestRunning = $false; $script:NetScanning = (Chance 0.03); $script:NetAudit = $null; $script:NetAuditSaved = $null
    $script:FauxAsync = @{ audit = $(if (Chance 0.03) { @{ Error = 'Délai dépassé' } } else { $R }) }
    $script:TestTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:Messages.Clear(); $script:Statut = ''; $script:Enregistre = @{}
    Remove-Item -LiteralPath $AuditFile, "$AuditFile.bak" -ErrorAction SilentlyContinue
    $ui = New-Ui
    $err = ''
    try { Invoke-NetAudit } catch { $err = $_.Exception.Message }
    $A = $script:NetAudit
    $cas += [ordered]@{
        entree = [ordered]@{ appareils = @($liste | ForEach-Object { ConvertTo-AppareilJuge $_ }); net = $net; resultat = $script:FauxAsync.audit; scanEnCours = [bool]$script:NetScanning }
        sortie = [ordered]@{
            erreur = $err; messages = @($script:Messages); statut = $script:Statut
            ips = @(if ($script:ArgsAsync.audit) { $script:ArgsAsync.audit.Ips }); ports = @(if ($script:ArgsAsync.audit) { $script:ArgsAsync.audit.Ports })
            points = @(if ($A) { foreach ($c in $A.Checks) { [ordered]@{ Status = $c.Status; Cat = $c.Cat; Title = $c.Title; Detail = $c.Detail; Items = @($c.Items | ForEach-Object { [string]$_ })
                Actions = @(foreach ($act in $c.Actions) { [ordered]@{ Label = [string]$act.Label; Arg = $act.Arg; Confirm = [string]$act.Confirm; NoRefresh = [bool]$act.NoRefresh; After = [bool]$act.After } }) } } })
            note = $(if ($A) { $A.Score } else { $null }); nombre = $(if ($A) { $A.Count } else { $null })
            enregistre = $(if ($script:Enregistre['audit.json']) { ConvertFrom-Json $script:Enregistre['audit.json'] } else { $null })
            carte = [ordered]@{ texte = [string]$ui.NetAuditText.Text; bouton = [string]$ui.BtnNetAudit.Content; voir = ($ui.BtnNetAuditView.Visibility -eq 'Visible') }
        }
    }
    $script:ArgsAsync = @{}
}
Save-Juge 'audit' $cas

# 7. La carte réseau bridée, sa bulle, et la page Connexion
function New-NetAuHasard {
    if (Chance 0.08) { return $null }
    $wifi = Chance 0.3
    $bps = Pick @(1e9, 1e8, 1e7, 2.5e9, 0, 5e8, 9.99e8, 1e10, 866.7e6, 144e6, 3e8)
    $vit = Pick @(@(), @('Négociation automatique', '10 Mbit/s Duplex intégral', '100 Mbit/s Duplex intégral', '1,0 Gbit/s Duplex intégral'), @('Auto Negotiation', '2.5 Gbps Full Duplex', '1.0 Gbps Full Duplex'),
                  @('10 Mbps Half Duplex', '100 Mbps Full Duplex'), @('Auto', '1 Gbit'), @('Vitesse 10 Gb/s'), @('10Mbps', '100Mb Full'), @('1.0 GB Full'))
    $reg = Pick @(@(), @('0', '1', '2', '3', '4', '6'), @('0', '4'), @('0', '2500', '1000'), @('x', '7'), @('5'), @('10000'), @('999'))
    @{ IfIndex = 3 + $rnd.Next(30); Gateway = (Pick @('192.168.1.1', '192.168.0.254', '10.0.0.1', '0.0.0.0', '')); Name = 'Ethernet'; Desc = (Pick @('Realtek PCIe GbE Family Controller', 'Intel(R) Ethernet Controller I225-V', 'Killer E2600', 'Intel(R) Wi-Fi 6 AX201 160MHz'))
       Speed = (Pick @('1 Gbps', '100 Mbps', '2.5 Gbps', '866.7 Mbps', '10 Mbps')); Wifi = $wifi; Guid = (Pick @('{A}', '{B}')); Mac = '00-11-22-33-44-55'; Ip = '192.168.1.42'; Prefix = 24
       Dns = (Pick @('', '192.168.1.1', '1.1.1.1, 1.0.0.1', '8.8.8.8, 8.8.4.4', '9.9.9.9', '149.112.112.112', '1.0.0.1')); Bps = $bps; Vitesses = $vit; VitessesReg = $reg }
}
$cas = @()
for ($i = 0; $i -lt 300; $i++) {
    $n = New-NetAuHasard
    $b = Get-BridageCarte $n
    $cas += [ordered]@{ net = $n; capacite = $(if ($n) { Get-CapaciteCarte $n } else { $null }); bridage = $b; texte = $(if ($b) { Get-TexteBridage $b } else { $null })
        vitesse = $(if ($n) { Format-VitesseCarte $n.Bps } else { $null }) }
}
foreach ($v in 0, 1, 999, 1e6, 1.5e6, 9.99e6, 1e7, 1.25e7, 9.95e7, 1e8, 1.05e8, 3.333e8, 9.999e8, 1e9, 1.05e9, 2.5e9, 2.55e9, 1e10, 1.234e10) { $cas += [ordered]@{ format = $v; vitesse = (Format-VitesseCarte $v) } }
Save-Juge 'carte-bridee' $cas
# La bulle : une seule par épisode, jamais pendant une partie
$cas = @()
for ($i = 0; $i -lt 60; $i++) {
    $script:CarteAlertee = $null; $script:GameSession = $null; $script:LagSession = $null; $script:Notes.Clear(); $script:Journal.Clear()
    $etapes = @(for ($k = 0; $k -lt 8; $k++) {
        $n = New-NetAuHasard
        $partie = Chance 0.2
        $script:GameSession = $(if ($partie) { @{ Game = 'Essai' } } else { $null })
        $avant = $script:Notes.Count
        $r = Test-AlerteCarte $n
        [ordered]@{ net = $n; partie = $partie; resultat = $r; bulle = $(if ($script:Notes.Count -gt $avant) { $script:Notes[$script:Notes.Count - 1] } else { $null }); alertee = $script:CarteAlertee }
    })
    $cas += , @{ etapes = $etapes; journal = @($script:Journal) }
}
Save-Juge 'carte-bulle' $cas

# La page Connexion (le test « Tester ma connexion » compris, avec un faux ping)
$cas = @()
for ($i = 0; $i -lt 260; $i++) {
    $n = New-NetAuHasard
    $script:FauxNet = $n; $script:Net = $null; $script:PingResults = @(); $script:Kit = @{}; $script:History = @()
    $ui = New-Ui; $ui.PingPanel = New-Object System.Windows.Controls.StackPanel
    $pings = @{}
    $testee = Chance 0.75
    if ($testee) {
        foreach ($h in @($(if ($n) { $n.Gateway }), '1.1.1.1', '8.8.8.8')) {
            if (-not $h) { continue }
            $profil = $rnd.Next(7)
            $pings[$h] = @(for ($k = 0; $k -lt 20; $k++) { switch ($profil) { 0 { $rnd.Next(1, 4) } 1 { $rnd.Next(5, 70) } 2 { if (Chance 0.1) { -1 } else { $rnd.Next(10, 30) } } 3 { -1 } 4 { $rnd.Next(80, 200) } 5 { if (Chance 0.03) { -1 } else { $rnd.Next(20, 25) } } 6 { Pick @(1, 40, 2, 60) } } })
        }
        $script:FauxPings = @{}; foreach ($k in $pings.Keys) { $script:FauxPings[$k] = [System.Collections.ArrayList]@($pings[$k]) }
        Invoke-NetTest
    } else { $script:Net = $n }
    $debit = Pick @($null, $null, @(0, 940.4, 512.6), @(0, 1234.5, 98.5), @(0, -1, 20), @(0, 12.5), @(1, 0.4, 0.5), @(0, 2500, 2500.5))
    function script:Get-BoutonDebit { if ($null -ne $debit) { @{ Tag = @{ T = @{ Last = @{ Res = @{ R = $debit } } } } } } }
    $pages = Get-PagesProtection
    $cas += [ordered]@{ entree = [ordered]@{ net = $n; pings = $pings; teste = $testee; debit = $debit }; sortie = [ordered]@{ connexion = $pages.connexion; resultats = @($script:PingResults) } }
}
Save-Juge 'connexion' $cas

# 8. La surveillance (toutes les 10 minutes) : les nouveaux appareils signalés une fois
$cas = @()
for ($i = 0; $i -lt 50; $i++) {
    $s = New-ScanAuHasard ($rnd.Next(1, 12))
    if (-not $s.Net.Ip) { $s.Net.Ip = '192.168.1.42' }
    $script:Oui = $OuiMap; $script:NetWatchSeen = @{}; $script:NetScanning = $false; $script:TestRunning = $false
    Remove-Item -LiteralPath $KnownFile, "$KnownFile.bak" -ErrorAction SilentlyContinue
    if ($null -ne $s.Known) { [IO.File]::WriteAllText($KnownFile, $s.Known) }
    $script:FauxNet = $s.Net
    $etapes = @(for ($k = 0; $k -lt 3; $k++) {
        $arp = @($s.Arp) + @(if (Chance 0.5) { "$($s.Net.Gateway -replace '\.\d+$', '').$(200 + $k)|$(New-Mac (Pick @('F0B429', '3C5AB4', 'ZZ')))|$(Pick @('Reachable', 'Stale', 'Unreachable'))" })
        $script:FauxAsync = @{ scan = $(if (Chance 0.05) { @{ Error = 'x' } } else { @{ Arp = $arp; Alive = @(); Names = @(); OuiMap = $null } }) }
        $script:Notes.Clear(); $script:Journal.Clear()
        Invoke-NetWatch
        [ordered]@{ arp = $arp; erreur = [bool]$script:FauxAsync.scan.Error; bulles = $script:Notes.ToArray(); journal = $script:Journal.ToArray() }
    })
    $cas += [ordered]@{ net = $s.Net; connus = $s.Known; etapes = $etapes }
}
Save-Juge 'surveillance' $cas

Remove-Item -LiteralPath $travail -Recurse -Force -ErrorAction SilentlyContinue
Write-Host 'Juges du réseau exportés.' -ForegroundColor Green
