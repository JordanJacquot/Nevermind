# Nevermind : la liste stricte de ce que l'Aide accepte de faire avec les droits administrateur.
# Chargé par NevermindAide.ps1 (l'Aide, lancée en administrateur par la tâche « \Mindset\Nevermind (aide) »), et par
# Nevermind seulement pendant sa désinstallation (aide-client.ps1, SurPlace). Nevermind lui-même ne s'en sert plus :
# la règle « fermable » et le nom du canal lui viennent de NevermindPont.dll (le même code que l'Aide en Rust).
#
# Autonome : pas de fenêtre, pas d'autre module, et aucun fichier de Nevermind n'est lu (ils sont modifiables sans
# droits). Chaque demande est vérifiée seule : ce qui n'est pas prévu ici est refusé.
# Ce qui affaiblirait la sécurité du PC n'est jamais accepté, même pour « revenir en arrière » : rallumer le Bureau à
# distance, couper le contrôle des comptes ou le pare-feu, ajouter une exclusion à l'antivirus, mettre un DNS inconnu,
# bloquer Internet à l'antivirus ou à Windows Update.

$AideVersion = 5   # (4 : l'Aide en SYSTEM, pour l'utilisateur inscrit dans sa tâche ; 5 : demandes resserrées, audit du 25/10)
# Santé d'un disque (Get-StorageReliabilityCounter) : les champs dont Nevermind se sert
$AideChampsDisque = @('Temperature', 'TemperatureMax', 'Wear', 'PowerOnHours', 'StartStopCycleCount', 'ReadErrorsCorrected', 'ReadErrorsUncorrected',
                      'WriteErrorsUncorrected', 'ReadLatencyMax', 'WriteLatencyMax')
$AideMemoire = 'HKLM:\SOFTWARE\Mindset\Nevermind\Aide'   # ce que l'Aide retient elle-même (modifiable seulement en administrateur)
$AideDossierModules = $PSScriptRoot   # (dossier protégé de Nevermind : natif.cs et PresentMon y sont lus)
# Les dossiers de Windows, tels que Windows les donne : jamais lus dans $env: (la tâche planifiée de l'Aide reprend les
# variables de l'utilisateur, qu'un programme sans droits peut changer). Ni Documents ni AppData : l'utilisateur peut
# les déplacer, même lus ainsi.
$AideWindows = [Environment]::GetFolderPath('Windows')
$AideProgrammes = [Environment]::GetFolderPath('ProgramFiles')
$AideProgrammesX86 = [Environment]::GetFolderPath('ProgramFilesX86')
$AideDonneesCommunes = [Environment]::GetFolderPath('CommonApplicationData')

# Modules de Windows : l'Aide coupe leur chargement automatique (NevermindAide.ps1), un module posé ailleurs par un
# programme sans droits ne peut donc pas passer pour celui de Windows. Chaque demande charge ceux dont elle se sert,
# par leur chemin complet dans le dossier de Windows (les modules de base sont déjà là). tester-aide vérifie qu'aucune
# commande d'un autre module n'est utilisée sans ce chargement.
$AideDossierPS = [IO.Path]::Combine([Environment]::GetFolderPath('System'), 'WindowsPowerShell\v1.0\Modules')
# (« ConfigDefender/Defender » : le premier présent sur ce Windows)
function Import-AideModule([string[]]$Noms) {
    foreach ($choix in $Noms) {
        $noms = $choix -split '/'
        if (Get-Module -Name $noms) { continue }
        $n = @($noms | Where-Object { [IO.Directory]::Exists([IO.Path]::Combine($AideDossierPS, $_)) })[0]
        if (-not $n) { throw "Module de Windows introuvable : $choix" }
        Import-Module -Name ([IO.Path]::Combine($AideDossierPS, $n)) -Global -ErrorAction Stop
    }
}

# Les mesureurs de Nevermind (natif.cs : compteurs TCP, trace réseau), chargés dans l'Aide à la première mesure
function Initialize-AideNatif {
    if ('TrafficMon' -as [type]) { return }
    try { Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $AideDossierModules 'natif.cs'), [Text.Encoding]::UTF8)) }
    catch { if (-not ('TrafficMon' -as [type])) { throw } }   # (une autre demande l'a chargé au même moment)
}

# Programmes de Windows et de l'antivirus : jamais fermés, jamais bloqués, jamais de priorité changée
$AideExesWindows = '^(csrss|lsass|lsaiso|smss|wininit|winlogon|services|svchost|System|Registry|Idle|MsMpEng|NisSrv|MpCmdRun|MpDefenderCoreService|MpSigStub|MRT|smartscreen|MsSense|SenseCncProxy|SenseIR|SenseNdr|SenseTVM|SecurityHealthService|SecurityHealthSystray|SecurityHealthHost|SgrmBroker|dwm|explorer|fontdrvhost|sihost|ctfmon|conhost|audiodg|spoolsv|taskhostw|RuntimeBroker|TrustedInstaller|TiWorker|wuauclt|MoUsoCoreWorker|ApplicationFrameHost)(\.exe)?$'
# (plus, pour les demandes par nom : Nevermind et Mindset, le nom de toutes les applis de la suite)
$AideExesProteges = $AideExesWindows -replace '\)\(\\\.exe\)\?\$$', '|Nevermind|Mindset)(\.exe)?$'
# Les services que la page RAM passe en « manuel » (mises à jour, logiciels du fabricant : la liste de memoire.ps1), et
# remet comme avant. Jamais un service de Windows.
$AideServicesMemoire = '^(edgeupdate|edgeupdatem|gupdate|gupdatem|GoogleUpdater(Internal)?Service[\d.]{0,20}|AdobeARMservice|AdobeUpdateService|MozillaMaintenance|HPAppHelperCap|HPDiagsCap|HPSysInfoCap|HpTouchpointAnalyticsService|SupportAssistAgent|DDVDataCollector|DDVCollectorSvcApi|DDVRulesProcessor|ASUSSystemAnalysis|ASUSSystemDiagnosis|ASUSLinkNear|ASUSLinkRemote|ESRV_SVC_QUEENCREEK|SystemUsageReportSvc_QUEENCREEK|NvTelemetryContainer)$'
$AideServicesProteges ='^(WinDefend|WdNisSvc|WdFilter|Sense|SecurityHealthService|wscsvc|mpssvc|BFE|wuauserv|UsoSvc|WaaSMedicSvc|BITS|DoSvc|CryptSvc|TrustedInstaller|Dnscache|Dhcp|NlaSvc|nsi|W32Time|EventLog|RpcSs)$'

# ---------------------------------------------------------------------------
# Registre : les seules valeurs que l'Aide écrit ou efface
# ---------------------------------------------------------------------------
# Cle (exacte) ou Motif (expression), Nom (exact) ou NomMotif ; Type ; Valeurs permises (liste, ou Min et Max) ;
# Effacer : l'Aide peut aussi retirer la valeur (retour à l'état d'origine) ; Garde : pourquoi seule une valeur sûre passe.
$AideRegles = @(
    @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers'; Nom = 'HwSchMode'; Type = 'DWord'; Valeurs = @(1, 2); Effacer = $true }
    @{ Cle = 'HKU:\S-1-5-20\Software\Microsoft\Windows\CurrentVersion\DeliveryOptimization\Settings'; Nom = 'DownloadMode'; Type = 'DWord'; Valeurs = @(0, 1, 2, 3); Effacer = $true }
    @{ Cle = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization'; Nom = 'DOPercentageMaxBackgroundBandwidth'; Type = 'DWord'; Min = 0; Max = 100; Effacer = $true }
    @{ Cle = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization'; Nom = 'DODownloadMode'; Type = 'DWord'; Valeurs = @(0, 1, 2, 3, 99, 100); Effacer = $true }
    @{ Cle = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection'; Nom = 'AllowTelemetry'; Type = 'DWord'; Min = 0; Max = 3; Effacer = $true }
    @{ Cle = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection'; Nom = 'AllowTelemetry'; Type = 'DWord'; Min = 0; Max = 3; Effacer = $true }
    @{ Cle = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore'; Nom = 'SystemRestorePointCreationFrequency'; Type = 'DWord'; Min = 0; Max = 100000; Effacer = $true }
    @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server'; Nom = 'fDenyTSConnections'; Type = 'DWord'; Valeurs = @(1); Effacer = $false
       Garde = 'le Bureau à distance reste coupé : l''Aide ne le rallume jamais' }
    @{ Cle = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'; Nom = 'EnableLUA'; Type = 'DWord'; Valeurs = @(1); Effacer = $false
       Garde = 'le contrôle des comptes reste allumé : l''Aide ne le coupe jamais' }
    # Profils par jeu : priorité du processeur (PerfOptions seulement, jamais d'autre valeur de cette clé sensible)
    @{ Motif = '^HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Image File Execution Options\\(?<exe>[\w .()+\-]{1,100}\.exe)\\PerfOptions$'
       Nom = 'CpuPriorityClass'; Type = 'DWord'; Valeurs = @(1, 2, 3, 5, 6); Effacer = $true }   # (jamais 4 : « temps réel », qui peut figer le PC)
    # Programmes lancés avec Windows pour tous les utilisateurs : seulement les marquer actifs ou coupés
    @{ Motif = '^HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\(Run|Run32|StartupFolder)$'
       NomMotif = '^[^\x00-\x1f]{1,260}$'; Type = 'Binary'; Longueur = 12; Premier = @(2, 3, 6, 7); Effacer = $true }
)

# Le programme qui parle à l'Aide (son chemin complet, lu par Windows) est-il bien Nevermind ? Seulement Mindset.exe du
# dossier de cette copie de Nevermind (le programme qui fait tourner Nevermind, voir outils\lanceur.cs). Jamais par le nom :
# toutes les applis de la suite tournent dans un « Mindset.exe », et un Mindset.exe ailleurs (Mindset lui-même, une autre
# appli, une copie posée par n'importe qui) n'est pas écouté. Le relais Nevermind.exe ne parle jamais à l'Aide.
function Test-AideClient([string]$Chemin, [string]$Racine) {
    if (-not $Chemin -or -not $Racine) { return $false }
    try {
        $attendu = [IO.Path]::GetFullPath((Join-Path $Racine 'Mindset.exe'))
        # (le chemin reçu est comparé tel quel : rien n'est « nettoyé » qui pourrait faire passer un autre fichier pour lui)
        return [string]::Equals($Chemin, $attendu, [StringComparison]::OrdinalIgnoreCase)
    } catch { return $false }
}

# Nom du canal et du verrou de l'Aide : propres à l'utilisateur et au dossier de cette copie de Nevermind (une copie de
# test ne parle jamais à l'Aide de la copie installée, et chacune a la sienne)
function Get-AideSuffixe([string]$Racine) {
    $dossier = ([IO.Path]::GetFullPath($Racine).TrimEnd('\')).ToLowerInvariant()
    $o = [Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($dossier))
    -join ($o[0..5] | ForEach-Object { $_.ToString('x2') })
}

# Un paramètre d'une demande (table ou objet lu dans du JSON)
function Get-AideParam($P, [string]$Nom) {
    if ($null -eq $P) { return $null }
    if ($P -is [Collections.IDictionary]) { return , $P[$Nom] }
    $pr = $P.PSObject.Properties[$Nom]
    if ($pr) { return , $pr.Value }
    $null
}

# Une liste de textes (vide si absente). Get-AideParam rend la liste d'un bloc : on la parcourt depuis une variable.
function Get-AideListe($P, [string]$Nom) {
    $v = Get-AideParam $P $Nom
    if ($null -eq $v) { return }
    foreach ($x in $v) { if ($null -ne $x -and "$x" -ne '') { [string]$x } }
}

function Get-AideTexte($P, [string]$Nom, [int]$Max = 260) {
    $v = Get-AideParam $P $Nom
    if ($v -isnot [string] -or -not $v -or $v.Length -gt $Max -or $v -match '[\x00-\x1f]') { throw "Demande refusée : « $Nom » n'est pas valable." }
    $v
}

# Chemin de registre sous une seule forme : HKLM:\... ou HKU:\...
function ConvertTo-AideCle([string]$Chemin) {
    $c = ([string]$Chemin).Trim() -replace '^Registry::', ''
    $c = $c -replace '^(HKEY_LOCAL_MACHINE|HKLM:?)\\', 'HKLM:\' -replace '^(HKEY_USERS|HKU:?)\\', 'HKU:\'
    if ($c -notmatch '^HK(LM|U):\\' -or $c -match '\\\\|\\$|[\x00-\x1f*?]') { throw "Demande refusée : clé de registre non valable ($Chemin)." }
    $c
}

function Get-AideRegle([string]$Cle, [string]$Nom) {
    foreach ($r in $AideRegles) {
        $exe = $null
        if ($r.Cle) { if ($Cle -ne $r.Cle) { continue } }
        else {
            if ($Cle -notmatch $r.Motif) { continue }
            $exe = $Matches['exe']
        }
        $okNom = if ($r.Nom) { $Nom -eq $r.Nom } else { $Nom -match $r.NomMotif }
        if (-not $okNom) { continue }
        if ($exe -and $exe -match $AideExesProteges) { throw "Demande refusée : $exe est un programme de Windows ou de l'antivirus." }
        return $r
    }
    throw "Demande refusée : $Cle\$Nom n'est pas dans la liste de l'Aide."
}

function ConvertTo-AideValeur($Regle, $Valeur, [string]$Type) {
    if ($Type -ne $Regle.Type) { throw "Demande refusée : type $Type au lieu de $($Regle.Type)." }
    $refus = if ($Regle.Garde) { "L'Aide garde ce réglage : $($Regle.Garde)." } else { 'Demande refusée : valeur hors des valeurs permises.' }
    if ($Regle.Type -eq 'DWord') {
        $v = 0L
        if ($null -eq $Valeur -or $Valeur -is [array] -or -not [long]::TryParse([string]$Valeur, [ref]$v)) { throw $refus }
        if ($null -ne $Regle.Valeurs -and $Regle.Valeurs -notcontains $v) { throw $refus }
        if ($null -ne $Regle.Min -and ($v -lt $Regle.Min -or $v -gt $Regle.Max)) { throw $refus }
        return [int]$v
    }
    try { $o = [byte[]]@($Valeur) } catch { throw $refus }
    if ($o.Length -ne $Regle.Longueur -or $Regle.Premier -notcontains $o[0]) { throw $refus }
    , $o
}

function Open-AideCle([string]$Cle, [bool]$Creer) {
    if ($Cle -match '^HKLM:\\(.+)$') { $ruche = [Microsoft.Win32.Registry]::LocalMachine }
    elseif ($Cle -match '^HKU:\\(.+)$') { $ruche = [Microsoft.Win32.Registry]::Users }
    else { throw "Clé non prise en charge : $Cle" }
    $sous = $Matches[1]
    if ($Creer) { return $ruche.CreateSubKey($sous) }
    $ruche.OpenSubKey($sous, $true)
}

# ---------------------------------------------------------------------------
# DNS : les serveurs connus, ou ceux du réseau local (la box), ou ceux d'origine que l'Aide a retenus elle-même
# ---------------------------------------------------------------------------
$AideDnsChoix = @(
    @('1.1.1.1', '1.0.0.1'),
    @('8.8.8.8', '8.8.4.4'),
    @('9.9.9.9', '149.112.112.112')
)

function Test-AideAdresseLocale([string]$Ip) {
    $a = $null
    if (-not [Net.IPAddress]::TryParse($Ip, [ref]$a)) { return $false }
    $o = $a.GetAddressBytes()
    if ($o.Length -eq 4) { return ($o[0] -eq 10 -or ($o[0] -eq 172 -and $o[1] -ge 16 -and $o[1] -le 31) -or ($o[0] -eq 192 -and $o[1] -eq 168)) }
    ($o[0] -eq 0xfe -and ($o[1] -band 0xc0) -eq 0x80) -or (($o[0] -band 0xfe) -eq 0xfc)   # fe80::/10, fc00::/7
}

# Les adresses du réseau local qu'un DNS peut avoir sur cette carte : sa passerelle (la box) et les serveurs DNS donnés
# par la box (DHCP). Une autre adresse privée n'est pas acceptée : elle pourrait être un faux DNS posé sur le réseau.
function ConvertTo-AideIp([string]$Texte) {
    $a = $null
    if (-not [Net.IPAddress]::TryParse(([string]$Texte).Trim(), [ref]$a)) { return $null }
    $t = $a.ToString() -replace '%.*$', ''
    if ($t -in '0.0.0.0', '::') { return $null }
    $t
}
function Get-AideDnsLocaux($Carte) {
    $r = [Collections.Generic.List[string]]::new()
    Import-AideModule NetTCPIP
    try {
        foreach ($rt in @(Get-NetRoute -InterfaceIndex $Carte.ifIndex -DestinationPrefix '0.0.0.0/0', '::/0' -ErrorAction Stop)) {
            $ip = ConvertTo-AideIp ([string]$rt.NextHop); if ($ip) { $r.Add($ip) }
        }
    } catch {}
    $guid = '{' + ([string]$Carte.InterfaceGuid).Trim('{', '}') + '}'
    foreach ($base in 'Tcpip', 'Tcpip6') {
        try {
            $k = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey("SYSTEM\CurrentControlSet\Services\$base\Parameters\Interfaces\$guid")
            if (-not $k) { continue }
            try {
                foreach ($nom in 'DhcpNameServer', 'DhcpDefaultGateway') {
                    foreach ($x in @($k.GetValue($nom, $null))) { foreach ($y in ([string]$x -split '[,\s]+')) { $ip = ConvertTo-AideIp $y; if ($ip) { $r.Add($ip) } } }
                }
                $b = $k.GetValue('Dhcpv6DNSServers', $null)
                if ($b -is [byte[]]) {
                    for ($i = 0; $i + 16 -le $b.Length; $i += 16) { $ip = ConvertTo-AideIp ([Net.IPAddress]::new([byte[]]$b[$i..($i + 15)]).ToString()); if ($ip) { $r.Add($ip) } }
                }
            } finally { $k.Close() }
        } catch {}
    }
    , @($r | Select-Object -Unique)
}

function Get-AideCarte([int]$Index) {
    if ($Index -le 0) { throw 'Demande refusée : carte réseau non valable.' }
    Import-AideModule NetAdapter
    $ad = Get-NetAdapter -InterfaceIndex $Index -IncludeHidden -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $ad) { throw 'La carte réseau de ce réglage n''existe plus sur ce PC.' }
    $ad
}

# Valeurs d'origine des réglages du PC, gardées par l'Aide avant leur première modification (HKLM, modifiable
# seulement en administrateur) : une seconde copie de sauvegarde.json, qu'un nettoyeur de disque ne peut pas effacer.
# Nevermind les relit sans droits (lecture seule) ; effacées après une restauration complète (« origines.oublier »).
$AideOrigines = 'SOFTWARE\Mindset\Nevermind\Aide\Origines'
function Save-AideOrigine([string]$Cle, [string]$Nom) {
    try {
        $id = "$Cle|$Nom"
        $k = [Microsoft.Win32.Registry]::LocalMachine.CreateSubKey($AideOrigines)
        try {
            if ($null -ne $k.GetValue($id, $null)) { return }   # (la toute première valeur seulement)
            $e = [ordered]@{ Path = $Cle; Name = $Nom; Existed = $false; Value = $null; Kind = $null }
            $src = Open-AideCle $Cle $false
            if ($src) {
                try {
                    if ($src.GetValueNames() -contains $Nom) {
                        $v = $src.GetValue($Nom, $null, 'DoNotExpandEnvironmentNames')
                        $e.Existed = $true; $e.Kind = $src.GetValueKind($Nom).ToString(); $e.Value = $(if ($v -is [byte[]]) { [int[]]$v } else { $v })
                    }
                } finally { $src.Close() }
            }
            $k.SetValue($id, (ConvertTo-Json -InputObject $e -Compress -Depth 3), 'String')
        } finally { $k.Close() }
    } catch {}
}

# PresentMon (Intel) : lancé en administrateur par l'Aide, donc seulement s'il est bien signé par Intel (voir outils-tiers\SOURCES.md)
function Get-AidePresentMon {
    $pm = Join-Path (Split-Path $AideDossierModules -Parent) 'outils-tiers\PresentMon.exe'
    if (-not (Test-Path -LiteralPath $pm)) { throw 'PresentMon est absent du dossier de Nevermind.' }
    $s = Get-AuthenticodeSignature -LiteralPath $pm
    if ($s.Status -ne 'Valid' -or $s.SignerCertificate.Subject -notmatch 'CN=Intel Corporation(,|$)') { throw "PresentMon n'est pas signé par Intel ($($s.Status)) : l'Aide ne le lance pas." }
    $pm
}

function Get-AideDnsOrigine([string]$Guid) {
    try {
        $k = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SOFTWARE\Mindset\Nevermind\Aide\Dns')
        if (-not $k) { return $null }
        try { $v = $k.GetValue($Guid, $null) } finally { $k.Close() }
        if ($null -ne $v) { return [string]$v }
    } catch {}
    $null
}

# ---------------------------------------------------------------------------
# Nettoyage : les dossiers de Windows que seule l'Aide peut vider (jamais un chemin envoyé par Nevermind)
# Seulement des dossiers où un utilisateur sans droits ne peut pas écrire : l'Aide efface par le chemin, et un dossier
# remplacé par un lien entre la liste et l'effacement lui ferait effacer ailleurs. Les fichiers temporaires de Windows
# (C:\Windows\Temp) et les rapports d'erreurs (WER) n'en font donc plus partie : Windows les vide lui-même.
# ---------------------------------------------------------------------------
$AideNettoyage = [ordered]@{
    wu      = @("$AideWindows\SoftwareDistribution\Download")
    do      = @("$AideWindows\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache")
    dump    = @("$AideWindows\Minidump")
}

# Parcours d'un dossier sans suivre les liens (jonctions, liens symboliques) : on ne sort jamais du dossier prévu.
$CleanWalk = @'
function Get-CleanFiles([string]$Root, $Dirs) {
    foreach ($e in @(Get-ChildItem -LiteralPath $Root -Force -ErrorAction SilentlyContinue)) {
        if ($e.PSIsContainer) {
            if ($e.Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
            if ($script:CleanSkip -and $script:CleanSkip.ContainsKey($e.FullName.ToLower())) { continue }   # dossier protégé (Claude...)
            if ($null -ne $Dirs) { [void]$Dirs.Add($e.FullName) }
            Get-CleanFiles $e.FullName $Dirs
        } elseif ($e.LastWriteTime -lt $script:CleanBefore) { $e }
    }
}
function Set-CleanSkip($Paths, $Exclude) {
    $script:CleanSkip = @{}
    foreach ($p in @($Paths)) { foreach ($x in @($Exclude | Where-Object { $_ })) { $script:CleanSkip[(Join-Path $p $x).ToLower()] = $true } }
}
# Fichiers de moins de 24 h laissés : une installation ou un programme en cours peut encore s'en servir
$script:CleanBefore = (Get-Date).AddDays(-1)
'@

# Analyse d'une catégorie : nombre de fichiers, taille totale et les plus gros (pour « Voir les fichiers »)
$CleanListScript = [scriptblock]::Create($CleanWalk + @'

$a = $args[0]
Set-CleanSkip $a.Paths $a.Exclude
$sum = 0.0; $n = 0
$list = New-Object System.Collections.Generic.List[object]
foreach ($p in $a.Paths) {
    if (-not (Test-Path -LiteralPath $p)) { continue }
    foreach ($f in (Get-CleanFiles $p $null)) { $sum += $f.Length; $n++; $list.Add(@($f.FullName, [double]$f.Length, $f.LastWriteTime)) }
}
$top = @($list | Sort-Object { $_[1] } -Descending | Select-Object -First $a.Top)
@{ Size = $sum; Count = $n; Top = $top }
'@)

# Nettoyage d'une catégorie, fichier par fichier, avec une ligne de journal pour chacun
$CleanScript = [scriptblock]::Create($CleanWalk + @'

$paths = $args[0].Paths
Set-CleanSkip $paths $args[0].Exclude
$log = New-Object System.Collections.Generic.List[string]
$freed = 0.0; $del = 0; $skip = 0; $err = 0
$fmt = { param($b) if ($b -ge 1GB) { '{0:N1} Go' -f ($b / 1GB) } elseif ($b -ge 1MB) { '{0:N1} Mo' -f ($b / 1MB) } else { '{0:N0} Ko' -f [math]::Max(1, [math]::Ceiling($b / 1KB)) } }
foreach ($p in $paths) {
    if (-not (Test-Path -LiteralPath $p)) { continue }
    $dirs = New-Object System.Collections.Generic.List[string]
    foreach ($f in @(Get-CleanFiles $p $dirs)) {
        $len = [double]$f.Length
        try {
            if ($f.Attributes -band [IO.FileAttributes]::ReadOnly) { $f.Attributes = [IO.FileAttributes]::Normal }
            [IO.File]::Delete($f.FullName)
            $freed += $len; $del++
            $log.Add("[SUPPRIMÉ] $($f.FullName) - $(& $fmt $len)")
        } catch [System.UnauthorizedAccessException] {
            $err++; $log.Add("[REFUSÉ]   $($f.FullName) - $(& $fmt $len) - Accès refusé")
        } catch [System.IO.IOException] {
            $skip++; $log.Add("[LAISSÉ]   $($f.FullName) - $(& $fmt $len) - Fichier utilisé par un programme")
        } catch {
            $err++; $log.Add("[REFUSÉ]   $($f.FullName) - $(& $fmt $len) - $($_.Exception.Message)")
        }
    }
    # Dossiers restés vides : du plus profond au moins profond (le dossier de départ reste)
    foreach ($d in @($dirs | Sort-Object { $_.Length } -Descending)) {
        try { if (-not [IO.Directory]::EnumerateFileSystemEntries($d).GetEnumerator().MoveNext()) { [IO.Directory]::Delete($d) } } catch {}
    }
}
@{ Freed = $freed; Deleted = $del; Skipped = $skip; Errors = $err; Log = $log.ToArray() }
'@)

# ---------------------------------------------------------------------------
# Programmes : fermer, bloquer
# ---------------------------------------------------------------------------
# Un programme qu'on peut fermer ou bloquer : un .exe qui existe, hors de Windows et de l'antivirus
function Get-AideProgramme([string]$Chemin, [bool]$WindowsPermis) {
    if (-not $Chemin -or -not [IO.Path]::IsPathRooted($Chemin) -or $Chemin -notmatch '\.exe$' -or $Chemin -match '[\x00-\x1f*?"<>|]') { throw "Demande refusée : programme non valable ($Chemin)." }
    $plein = [IO.Path]::GetFullPath($Chemin)
    if ($plein -ne $Chemin) { throw "Demande refusée : chemin non valable ($Chemin)." }
    if (-not [IO.File]::Exists($plein)) { throw "Ce programme n'existe plus : $plein" }
    $nom = [IO.Path]::GetFileName($plein)
    if ($nom -match $AideExesProteges) { throw "Demande refusée : $nom fait partie de Windows ou de l'antivirus." }
    $defender = @("$AideProgrammes\Windows Defender\", "$AideProgrammesX86\Windows Defender\", "$AideDonneesCommunes\Microsoft\Windows Defender\",
                  "$AideProgrammes\Windows Defender Advanced Threat Protection\", "$AideDonneesCommunes\Microsoft\Windows Defender Advanced Threat Protection\")
    foreach ($d in $defender) { if ($plein.StartsWith($d, [StringComparison]::OrdinalIgnoreCase)) { throw "Demande refusée : $nom fait partie de l'antivirus." } }
    if (-not $WindowsPermis -and $plein.StartsWith("$AideWindows\", [StringComparison]::OrdinalIgnoreCase)) { throw "Demande refusée : $nom fait partie de Windows." }
    $plein
}

# Ce que Windows dit d'un processus, même pour un autre compte : sa session, son compte, s'il est protégé (les moteurs
# d'antivirus tournent en processus protégés). Chargé à la première demande qui s'en sert.
function Initialize-AideProcessus {
    if ('AideProcessus' -as [type]) { return }
    try {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
public static class AideProcessus
{
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    [DllImport("kernel32.dll")] static extern bool ProcessIdToSessionId(uint pid, out uint session);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool GetProcessInformation(IntPtr h, int classe, out uint info, int taille);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern bool QueryFullProcessImageName(IntPtr h, int flags, StringBuilder name, ref int size);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool OpenProcessToken(IntPtr h, uint access, out IntPtr jeton);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool GetTokenInformation(IntPtr jeton, int classe, IntPtr info, int taille, out int requis);
    const uint Lecture = 0x1000;   // PROCESS_QUERY_LIMITED_INFORMATION

    // Session Windows du processus, -1 si inconnue
    public static int Session(int pid) { uint s; return ProcessIdToSessionId((uint)pid, out s) ? (int)s : -1; }

    // Compte du processus (SID), "" si Windows ne le dit pas
    public static string Compte(int pid)
    {
        IntPtr h = OpenProcess(Lecture, false, pid);
        if (h == IntPtr.Zero) return "";
        IntPtr jeton = IntPtr.Zero, info = IntPtr.Zero;
        try
        {
            if (!OpenProcessToken(h, 0x8, out jeton)) return "";   // TOKEN_QUERY
            int requis;
            GetTokenInformation(jeton, 1, IntPtr.Zero, 0, out requis);   // TokenUser
            if (requis <= 0) return "";
            info = Marshal.AllocHGlobal(requis);
            if (!GetTokenInformation(jeton, 1, info, requis, out requis)) return "";
            return new SecurityIdentifier(Marshal.ReadIntPtr(info)).Value;
        }
        catch { return ""; }
        finally
        {
            if (info != IntPtr.Zero) Marshal.FreeHGlobal(info);
            if (jeton != IntPtr.Zero) CloseHandle(jeton);
            CloseHandle(h);
        }
    }

    // Niveau de protection : 0 aucun, 1 protégé (antivirus, Windows), -1 inconnu (traité comme protégé)
    public static int Protection(int pid)
    {
        IntPtr h = OpenProcess(Lecture, false, pid);
        if (h == IntPtr.Zero) return -1;
        try
        {
            uint niveau;
            if (!GetProcessInformation(h, 7, out niveau, 4)) return -1;   // ProcessProtectionLevelInfo
            return niveau == 0xFFFFFFFE ? 0 : 1;   // PROTECTION_LEVEL_NONE
        }
        finally { CloseHandle(h); }
    }

    // Lancé en administrateur : 0 non, 1 oui, -1 inconnu (jeton illisible : traité comme oui)
    public static int Eleve(int pid)
    {
        IntPtr h = OpenProcess(Lecture, false, pid);
        if (h == IntPtr.Zero) return -1;
        IntPtr jeton = IntPtr.Zero, info = IntPtr.Zero;
        try
        {
            if (!OpenProcessToken(h, 0x8, out jeton)) return -1;   // TOKEN_QUERY
            info = Marshal.AllocHGlobal(4);
            int requis;
            if (!GetTokenInformation(jeton, 20, info, 4, out requis)) return -1;   // TokenElevation
            return Marshal.ReadInt32(info) != 0 ? 1 : 0;
        }
        catch { return -1; }
        finally
        {
            if (info != IntPtr.Zero) Marshal.FreeHGlobal(info);
            if (jeton != IntPtr.Zero) CloseHandle(jeton);
            CloseHandle(h);
        }
    }

    public static string Chemin(int pid)
    {
        IntPtr h = OpenProcess(Lecture, false, pid);
        if (h == IntPtr.Zero) return "";
        try { var sb = new StringBuilder(1024); int n = sb.Capacity; return QueryFullProcessImageName(h, 0, sb, ref n) ? sb.ToString() : ""; }
        finally { CloseHandle(h); }
    }
}
'@
    } catch { if (-not ('AideProcessus' -as [type])) { throw } }   # (une autre demande l'a chargé au même moment)
}

# Un chemin écrit par Windows (« %ProgramFiles%\...\x.exe », « "C:\...\x.exe" -k ... ») : le programme seul, chemin complet.
# Les variables sont remplacées par les vrais dossiers de Windows, jamais par celles de l'utilisateur.
function ConvertFrom-AideCheminWindows([string]$Texte) {
    $t = ([string]$Texte).Trim()
    if ($t.StartsWith('"')) { $t = $t.Substring(1); $i = $t.IndexOf('"'); if ($i -ge 0) { $t = $t.Substring(0, $i) } }
    else { $i = $t.IndexOf('.exe', [StringComparison]::OrdinalIgnoreCase); if ($i -ge 0) { $t = $t.Substring(0, $i + 4) } }
    $vars = @{ windir = $AideWindows; systemroot = $AideWindows; programfiles = $AideProgrammes; programw6432 = $AideProgrammes
               'programfiles(x86)' = $AideProgrammesX86; programdata = $AideDonneesCommunes; allusersprofile = $AideDonneesCommunes
               commonprogramfiles = "$AideProgrammes\Common Files"; 'commonprogramfiles(x86)' = "$AideProgrammesX86\Common Files" }
    foreach ($m in @([regex]::Matches($t, '%([^%]{1,40})%'))) {
        $v = $vars[$m.Groups[1].Value.ToLowerInvariant()]
        if (-not $v) { return $null }   # (variable inconnue : on ne devine pas)
        $t = $t.Replace($m.Value, $v)
    }
    if ($t -match '^\\SystemRoot\\') { $t = $AideWindows + $t.Substring(11) }
    if ($t -notmatch '^[A-Za-z]:\\' -or $t -notmatch '\.exe$') { return $null }   # (« windowsdefender:// »...)
    try { [IO.Path]::GetFullPath($t) } catch { $null }
}

# Ce que Windows déclare, lu une fois par demande : les programmes des antivirus inscrits au Centre de sécurité, et les
# services (programme et processus). Pas de liste à tenir à jour : un nouvel antivirus est reconnu tout seul.
# (la page RAM s'en sert aussi : Reference, le programme pour qui on ferme, est ajouté par celui qui demande)
function Get-AideContexte {
    $c = @{ Antivirus = Get-AideAntivirus; Windows = $AideWindows
            ServicesExe = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
            ServicesPid = [Collections.Generic.HashSet[int]]::new() }
    foreach ($s in @(Get-CimInstance -ClassName Win32_Service -OperationTimeoutSec 10 -ErrorAction Stop)) {
        $p = ConvertFrom-AideCheminWindows ([string]$s.PathName); if ($p) { [void]$c.ServicesExe.Add($p) }
        if ([int]$s.ProcessId -gt 0) { [void]$c.ServicesPid.Add([int]$s.ProcessId) }
    }
    $c
}
# Les programmes des antivirus inscrits au Centre de sécurité de Windows (chemins complets)
function Get-AideAntivirus {
    $l = [Collections.Generic.List[string]]::new()
    try {   # (absent des Windows Server : aucun antivirus inscrit)
        foreach ($av in @(Get-CimInstance -Namespace 'root\SecurityCenter2' -ClassName AntiVirusProduct -OperationTimeoutSec 5 -ErrorAction Stop)) {
            foreach ($x in [string]$av.pathToSignedProductExe, [string]$av.pathToSignedReportingExe) { $p = ConvertFrom-AideCheminWindows $x; if ($p) { $l.Add($p) } }
        }
    } catch {}
    , $l
}

# Un programme d'antivirus ? Son dossier entier compte (moteur, service, interface), sauf dans le dossier de Windows où
# seul le fichier déclaré compte.
function Test-AideAntivirus($Contexte, [string]$Chemin) {
    foreach ($av in $Contexte.Antivirus) {
        if ([string]::Equals($av, $Chemin, [StringComparison]::OrdinalIgnoreCase)) { return $true }
        $d = [IO.Path]::GetDirectoryName($av)
        if ($d.StartsWith("$AideWindows\", [StringComparison]::OrdinalIgnoreCase) -or $d.Length -le 3) { continue }
        if ($Chemin.StartsWith("$d\", [StringComparison]::OrdinalIgnoreCase)) { return $true }
    }
    $false
}

# Pourquoi ce programme ne peut pas être fermé ni bloqué ($null : il peut l'être). -ServicePermis : svchost.exe pour un
# service précis (la règle du pare-feu ne vise que ce service, déjà vérifié par $AideServicesProteges).
function Get-AideRefusProgramme($Contexte, [string]$Chemin, [switch]$ServicePermis) {
    if (Test-AideAntivirus $Contexte $Chemin) { return 'est un programme de ton antivirus' }
    if ($ServicePermis) { return $null }
    if ($Contexte.ServicesExe.Contains($Chemin)) { return 'fait tourner un service (un programme qui tourne en fond pour Windows ou un logiciel)' }
    Initialize-AideProcessus
    foreach ($p in [Diagnostics.Process]::GetProcesses()) {
        try {
            if ([string]::Equals([AideProcessus]::Chemin($p.Id), $Chemin, [StringComparison]::OrdinalIgnoreCase) -and [AideProcessus]::Protection($p.Id) -ne 0) { return 'est protégé par Windows (antivirus)' }
        } finally { $p.Dispose() }
    }
    $null
}

# ---------------------------------------------------------------------------
# Fermer un processus : LA règle, une seule, pour l'Aide (« processus.fermer ») et pour la page RAM de Nevermind
# (memoire.ps1, bouton « Fermer »). Seulement un programme ordinaire, du même compte et de la même session que celui qui
# demande (Nevermind). Jamais : un service, un processus protégé (moteur d'antivirus), un programme d'antivirus (inscrit
# au Centre de sécurité, ou de Microsoft Defender), un programme de Windows (son nom ou son dossier), une appli du
# Microsoft Store, une appli de la suite Mindset (reconnue à son emplacement), un programme lancé en administrateur
# quand celui qui demande ne l'est pas. Dans le doute (compte, session, chemin ou protection illisible) : refusé.
# ---------------------------------------------------------------------------
# Ce que Windows dit d'un processus (une « fiche ») : @{ Id; Chemin; Session; Compte; Protection; Eleve }
# (Protection et Eleve : 0 non, 1 oui, -1 illisible). Les tests donnent de fausses fiches à Get-RefusFermeture.
function Read-AideFiche([int]$Id, [string]$Chemin) {
    Initialize-AideProcessus
    $c = [AideProcessus]::Chemin($Id); if (-not $c) { $c = $Chemin }
    @{ Id = $Id; Chemin = $c; Session = [AideProcessus]::Session($Id); Compte = [AideProcessus]::Compte($Id); Protection = [AideProcessus]::Protection($Id); Eleve = [AideProcessus]::Eleve($Id) }
}
# Celui pour qui on ferme (Nevermind) : @{ Id; Session; Compte; Eleve }
function Get-AideReference([int]$Id) {
    Initialize-AideProcessus
    @{ Id = $Id; Session = [AideProcessus]::Session($Id); Compte = [AideProcessus]::Compte($Id); Eleve = [AideProcessus]::Eleve($Id) }
}

# Les dossiers de la suite Mindset, reconnue à son EMPLACEMENT (toutes ses applis s'appellent Mindset.exe : un
# Mindset.exe ailleurs n'est pas protégé par cette règle) : ses programmes (Program Files\Mindset, X:\Mindset\Programmes)
# et le dossier « ia » des données de Mindcare, d'où tourne son assistant.
function Test-AideCheminSuite([string]$Chemin) {
    if (-not $Chemin) { return $false }
    if ($AideProgrammes -and $Chemin.StartsWith("$AideProgrammes\Mindset\", [StringComparison]::OrdinalIgnoreCase)) { return $true }
    $Chemin -match '^[A-Za-z]:\\Mindset\\(Programmes|Donnees\\Mindcare\\ia)\\' -or $Chemin -match '\\AppData\\Local\\Mindset\\Mindcare( \(travail\))?\\ia\\'
}
# Une appli de la suite : à son emplacement, celui qui demande lui-même, ou (si le contexte connaît les processus,
# @{ pid = @{ Parent; Chemin } }) un processus lancé par la suite (sa WebView2) ou qui a lancé celui qui demande
function Test-AideSuite($Contexte, [int]$Id, [string]$Chemin) {
    if (Test-AideCheminSuite $Chemin) { return $true }
    $ref = if ($Contexte.Reference) { [int]$Contexte.Reference.Id } else { 0 }
    if ($ref -and $Id -eq $ref) { return $true }
    $procs = $Contexte.Processus
    if (-not $procs) { return $false }
    $x = $Id
    for ($i = 0; $i -lt 8; $i++) {
        $p = $procs[$x]; if (-not $p) { break }
        $x = [int]$p.Parent; if ($x -le 4) { break }
        if ($x -eq $ref -or (Test-AideCheminSuite ([string]$procs[$x].Chemin))) { return $true }
    }
    $x = $ref
    for ($i = 0; $ref -and $i -lt 8; $i++) {
        $p = $procs[$x]; if (-not $p) { break }
        $x = [int]$p.Parent; if ($x -le 4) { break }
        if ($x -eq $Id) { return $true }
    }
    $false
}

# Pourquoi ce processus ne peut pas être fermé ($null : il peut l'être). $Contexte : Get-AideContexte (Antivirus,
# ServicesPid, Windows), plus Reference (Get-AideReference) et, si connus, Processus. -ElevePermis : l'Aide, en
# administrateur, ferme aussi un programme de l'utilisateur lancé en administrateur.
function Get-RefusFermeture($Contexte, $Fiche, [switch]$ElevePermis) {
    $id = [int]$Fiche.Id; $chemin = [string]$Fiche.Chemin; $ref = $Contexte.Reference
    if ($id -le 4) { return 'fait partie de Windows' }
    if ($null -eq $Contexte.ServicesPid -or $Contexte.ServicesPid.Contains($id)) { return 'est un service' }
    if (-not $ref -or $null -eq $ref.Session -or [int]$ref.Session -lt 0 -or -not $ref.Compte) { return 'programme qui fait la demande inconnu' }
    if ($null -eq $Fiche.Session -or [int]$Fiche.Session -ne [int]$ref.Session) { return 'ne tourne pas dans ta session' }
    if (-not $Fiche.Compte -or [string]$Fiche.Compte -ne [string]$ref.Compte) { return 'n''appartient pas à ton compte' }
    if ($null -eq $Fiche.Protection -or [int]$Fiche.Protection -ne 0) { return 'est protégé par Windows (antivirus)' }
    if (-not $chemin) { return 'emplacement illisible' }
    $windows = if ($Contexte.Windows) { ([string]$Contexte.Windows).TrimEnd('\') } else { $AideWindows }
    if ([IO.Path]::GetFileName($chemin) -match $AideExesWindows -or $chemin.StartsWith("$windows\", [StringComparison]::OrdinalIgnoreCase)) { return 'fait partie de Windows' }
    if ((Test-AideAntivirus $Contexte $chemin) -or $chemin -match '\\(Windows|Microsoft) Defender( Advanced Threat Protection)?\\') { return 'est un programme de ton antivirus' }
    if ($chemin -match '\\WindowsApps\\') { return 'est une appli du Microsoft Store' }
    if (Test-AideSuite $Contexte $id $chemin) { return 'fait partie de la suite Mindset' }
    if (-not $ElevePermis -and [int]$ref.Eleve -ne 1 -and ($null -eq $Fiche.Eleve -or [int]$Fiche.Eleve -ne 0)) { return 'tourne en administrateur' }
    $null
}

# Pour l'Aide : la même règle, au nom du programme qui fait la demande (Nevermind, -Client)
function Get-AideRefusProcessus($Contexte, [int]$Client, [int]$Id, [string]$Chemin) {
    $Contexte.Reference = Get-AideReference $Client
    Get-RefusFermeture $Contexte (Read-AideFiche $Id $Chemin) -ElevePermis
}

# ---------------------------------------------------------------------------
# Ce que l'Aide refuse même à Nevermind (audit du 25/10, constat 8) : tout programme du compte peut lui parler, chaque
# demande doit donc rester sûre venant d'un programme malveillant.
# ---------------------------------------------------------------------------
# Un programme lancé avec Windows qui protège le PC (Sécurité Windows, un antivirus inscrit au Centre de sécurité) :
# l'Aide ne le coupe jamais au démarrage. $Cle : la clé StartupApproved (Run, Run32 ou StartupFolder) ; $Commande : la
# commande de cette entrée, lue dans la clé Run de Windows ($null : lue ici). $Antivirus : programmes des antivirus.
function Test-AideDemarrageSecurite([string]$Cle, [string]$Nom, $Commande, $Antivirus) {
    if ($Nom -match '^(SecurityHealth|Windows ?Defender|Microsoft ?Defender|Windows Security|Sécurité Windows)' -or [IO.Path]::GetFileNameWithoutExtension($Nom) -match $AideExesWindows) { return $true }
    if ($null -eq $Commande -and $Cle -match '\\(Run|Run32)$') {
        $sous = if ($Matches[1] -eq 'Run32') { 'SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run' } else { 'SOFTWARE\Microsoft\Windows\CurrentVersion\Run' }
        try {
            $base = [Microsoft.Win32.RegistryKey]::OpenBaseKey('LocalMachine', 'Registry64')
            try { $k = $base.OpenSubKey($sous); if ($k) { try { $Commande = [string]$k.GetValue($Nom, '', 'DoNotExpandEnvironmentNames') } finally { $k.Close() } } } finally { $base.Close() }
        } catch {}
    }
    if (-not $Commande) { return $false }
    $chemin = ConvertFrom-AideCheminWindows ([string]$Commande)
    if (-not $chemin) { return $false }
    if ([IO.Path]::GetFileName($chemin) -match $AideExesWindows -or $chemin -match '\\(Windows|Microsoft) Defender( Advanced Threat Protection)?\\') { return $true }
    if ($null -eq $Antivirus) { $Antivirus = Get-AideAntivirus }
    Test-AideAntivirus @{ Antivirus = $Antivirus } $chemin
}

# Points de restauration : un par 24 h au plus (de nombreux points chasseraient les anciens, ceux qui servent). $Derniere :
# l'heure du dernier (UTC, en ticks ; 0 si aucun).
$AideRestaurationCle = 'SOFTWARE\Mindset\Nevermind\Aide'
function Test-AideRestaurationRecente([long]$Derniere, [datetime]$Maintenant = [DateTime]::UtcNow) {
    if ($Derniere -le 0) { return $false }
    $ecart = $Maintenant - [DateTime]::new($Derniere, [DateTimeKind]::Utc)
    $ecart.TotalHours -lt 24 -and $ecart.TotalMinutes -gt -5   # (une heure dans le futur : horloge changée, pas un vrai point)
}
function Get-AideDerniereRestauration {
    try {
        $k = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($AideRestaurationCle)
        if (-not $k) { return [long]0 }
        try { $v = $k.GetValue('DerniereRestauration', $null) } finally { $k.Close() }
        $t = [long]0
        if ($null -ne $v -and [long]::TryParse([string]$v, [ref]$t)) { return $t }
    } catch {}
    [long]0
}

# Tâches planifiées : seulement celles qui tournent pour le compte qui demande (ou pour un groupe dont il fait partie
# d'office), et celles des comptes du système. Jamais celles des autres comptes du PC.
$AideComptesSysteme = @('S-1-5-18', 'S-1-5-19', 'S-1-5-20')
$AideGroupesDeTous = @('S-1-1-0', 'S-1-5-4', 'S-1-5-11', 'S-1-5-32-545')
function ConvertTo-AideSid([string]$Compte) {
    $c = $Compte.Trim()
    if (-not $c) { return '' }
    if ($c -match '^S-1-[\d-]+$') { return $c }
    try { (New-Object Security.Principal.NTAccount($c)).Translate([Security.Principal.SecurityIdentifier]).Value } catch { '' }
}
function Test-AideTacheDuCompte([string]$UserId, [string]$GroupId, [string]$Sid) {
    if (-not $Sid) { return $false }
    $u = ConvertTo-AideSid $UserId
    if ($u) { return ($u -eq $Sid -or $AideComptesSysteme -contains $u) }
    $g = ConvertTo-AideSid $GroupId
    [bool]($g -and $AideGroupesDeTous -contains $g)
}

# ---------------------------------------------------------------------------
# Les demandes
# ---------------------------------------------------------------------------
# -Verifier : contrôle la demande sans rien faire (tests automatiques). Renvoie un résultat simple (texte, nombre,
# table) que l'Aide renvoie à Nevermind en JSON. Une demande refusée lève une erreur dont le message est montré tel quel.
# Les règles du pare-feu créées par Nevermind (« Nevermind : bloque ... », ou « OptiGame : bloque ... » avant la 1.3.6)
function Get-ReglesNevermind { Import-AideModule NetSecurity; @(Get-NetFirewallRule -DisplayName 'Nevermind : bloque *' -ErrorAction SilentlyContinue) + @(Get-NetFirewallRule -DisplayName 'OptiGame : bloque *' -ErrorAction SilentlyContinue) }

# Une fois, au démarrage de l'Aide : les anciennes règles prennent le nom de Nevermind (on les retrouve en cherchant « Nevermind »)
function Rename-AnciennesRegles {
    Import-AideModule NetSecurity
    foreach ($r in @(Get-NetFirewallRule -DisplayName 'OptiGame : bloque *' -ErrorAction SilentlyContinue)) {
        try { Set-NetFirewallRule -Name $r.Name -NewDisplayName ($r.DisplayName -replace '^OptiGame : ', 'Nevermind : ') -ErrorAction Stop } catch {}
    }
}

# -Client : le processus qui fait la demande (Nevermind), lu par Windows sur le canal ; « processus.fermer » ne ferme que
# ce qui tourne sous son compte et dans sa session.
function Invoke-AideAction([string]$Action, $P, [switch]$Verifier, [int]$Client) {
    switch -Exact ($Action) {
        'aide.bonjour' {
            $moi = [Security.Principal.WindowsIdentity]::GetCurrent()
            $admin = (New-Object Security.Principal.WindowsPrincipal($moi)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
            # (Compte : S-1-5-18 quand l'Aide tourne en SYSTEM ; Temp : où elle compile, jamais un dossier de l'utilisateur)
            return @{ Version = $AideVersion; Admin = $admin; Pid = $PID; Compte = $moi.User.Value; Temp = [IO.Path]::GetTempPath() }
        }

        'reg.ecrire' {
            $cle = ConvertTo-AideCle (Get-AideParam $P 'Cle')
            $nom = [string](Get-AideParam $P 'Nom')
            $r = Get-AideRegle $cle $nom
            $v = ConvertTo-AideValeur $r (Get-AideParam $P 'Valeur') ([string](Get-AideParam $P 'Type'))
            # (programmes lancés avec Windows : jamais couper Sécurité Windows ni un antivirus ; les remettre actifs reste
            # permis. StartupApproved : premier octet impair = coupé, 2 ou 6 = actif)
            $coupe = -not (@($v).Count -and ([int](@($v)[0]) -band 1) -eq 0)
            if ($r.NomMotif -and $cle -match '\\StartupApproved\\' -and $coupe -and (Test-AideDemarrageSecurite $cle $nom $null $null)) {
                throw "Demande refusée : $nom protège ton PC (Sécurité Windows ou antivirus), l'Aide ne le coupe jamais au démarrage."
            }
            if ($Verifier) { return 'ok' }
            Save-AideOrigine $cle $nom
            $k = Open-AideCle $cle $true
            try { $k.SetValue($nom, $v, [Microsoft.Win32.RegistryValueKind]$r.Type) } finally { $k.Close() }
            return 'ok'
        }

        'reg.effacer' {
            $cle = ConvertTo-AideCle (Get-AideParam $P 'Cle')
            $nom = [string](Get-AideParam $P 'Nom')
            $r = Get-AideRegle $cle $nom
            if (-not $r.Effacer) { throw "L'Aide garde ce réglage : $($r.Garde)." }
            if ($Verifier) { return 'ok' }
            Save-AideOrigine $cle $nom
            $k = Open-AideCle $cle $false
            if ($k) { try { $k.DeleteValue($nom, $false) } finally { $k.Close() } }
            return 'ok'
        }

        'service.regler' {
            $nom = [string](Get-AideParam $P 'Nom')
            if ($nom -notin @('DiagTrack') -and $nom -notmatch $AideServicesMemoire) { throw "Demande refusée : le service $nom n'est pas dans la liste de l'Aide." }
            $dem = [string](Get-AideParam $P 'Demarrage')
            if ($dem -notin @('Disabled', 'Manual', 'Automatic')) { throw "Demande refusée : type de démarrage $dem." }
            # (« Désactivé » : seulement la télémétrie. Les services de la page RAM passent en « Manuel », jamais plus : un
            # service de mise à jour désactivé couperait pour de bon les mises à jour du navigateur)
            if ($dem -eq 'Disabled' -and $nom -ne 'DiagTrack') { throw "Demande refusée : l'Aide ne désactive pas $nom (seulement « Manuel »), ses mises à jour doivent continuer." }
            $lancer = [bool](Get-AideParam $P 'Lancer'); $arreter = [bool](Get-AideParam $P 'Arreter')
            if ($Verifier) { return 'ok' }
            Set-Service -Name $nom -StartupType $dem -ErrorAction Stop
            if ($arreter) { Stop-Service -Name $nom -Force -ErrorAction Stop }
            elseif ($lancer) { Start-Service -Name $nom -ErrorAction Stop }
            return 'ok'
        }

        'dns.regler' {
            $index = [int](Get-AideParam $P 'Index')
            $serveurs = @(Get-AideListe $P 'Serveurs')
            if ($serveurs.Count -gt 4) { throw 'Demande refusée : trop de serveurs DNS.' }
            $ad = Get-AideCarte $index
            $guid = ([string]$ad.InterfaceGuid).Trim('{', '}').ToLower()
            $texte = ($serveurs -join ',')
            $connu = (-not $serveurs.Count) -or @($AideDnsChoix | Where-Object { ($_ -join ',') -eq $texte }).Count -gt 0
            if (-not $connu) {
                # Ceux de la box : des adresses privées, et seulement la passerelle de cette carte ou les DNS que la box donne
                $locaux = Get-AideDnsLocaux $ad
                $connu = -not @($serveurs | Where-Object { -not (Test-AideAdresseLocale $_) -or (ConvertTo-AideIp $_) -notin $locaux }).Count
            }
            $origine = Get-AideDnsOrigine $guid
            if (-not $connu -and $null -ne $origine -and (@($origine -split '[,\s]+' | Where-Object { $_ }) -join ',') -eq $texte) { $connu = $true }
            if (-not $connu) { throw "L'Aide garde ce réglage : elle ne met que des DNS connus (Cloudflare, Google, Quad9), ceux de ta box, ou ceux que tu avais avant Nevermind." }
            if ($Verifier) { return 'ok' }
            if ($null -eq $origine) {
                # Premier changement sur cette carte : l'Aide retient les serveurs d'origine (seule elle peut les remettre)
                $avant = ''
                try { $avant = [string](Get-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{$guid}" -Name NameServer -ErrorAction Stop).NameServer } catch {}
                $k = [Microsoft.Win32.Registry]::LocalMachine.CreateSubKey('SOFTWARE\Mindset\Nevermind\Aide\Dns')
                try { $k.SetValue($guid, $avant, 'String') } finally { $k.Close() }
            }
            Import-AideModule DnsClient
            if ($serveurs.Count) { Set-DnsClientServerAddress -InterfaceIndex $ad.ifIndex -ServerAddresses $serveurs -ErrorAction Stop }
            else { Set-DnsClientServerAddress -InterfaceIndex $ad.ifIndex -ResetServerAddresses -ErrorAction Stop }
            Clear-DnsClientCache -ErrorAction SilentlyContinue
            return 'ok'
        }

        'pare-feu.bloquer' {
            $nom = Get-AideTexte $P 'Nom' 200
            if ($nom -notmatch '^(Nevermind|OptiGame) : bloque [^*?\[\]]+$') { throw 'Demande refusée : nom de règle non valable.' }
            $services = @(Get-AideListe $P 'Services')
            foreach ($s in $services) {
                if ($s -notmatch '^[\w.\-]{1,80}$') { throw "Demande refusée : service $s non valable." }
                if ($s -match $AideServicesProteges) { throw "Demande refusée : le service $s est indispensable à Windows ou à l'antivirus." }
            }
            $prog = Get-AideProgramme ([string](Get-AideParam $P 'Programme')) $true
            $court = [IO.Path]::GetFileName($prog)
            $pourService = $court -eq 'svchost.exe' -and $services.Count
            if ($court -match $AideExesProteges -and -not $pourService) { throw "Demande refusée : $court est indispensable à Windows ou à l'antivirus." }
            $refus = Get-AideRefusProgramme (Get-AideContexte) $prog -ServicePermis:$pourService
            if ($refus) { throw "Demande refusée : $court $refus." }
            if ($Verifier) { return 'ok' }
            Import-AideModule NetSecurity
            $cibles = if ($services.Count) { @($services | ForEach-Object { @{ Service = $_ } }) } else { @(@{}) }
            foreach ($t in $cibles) {
                foreach ($dir in 'Outbound', 'Inbound') {
                    $p2 = @{ DisplayName = $nom; Direction = $dir; Program = $prog; Action = 'Block'; Profile = 'Any'; ErrorAction = 'Stop' }
                    if ($t.Service) { $p2.Service = $t.Service }
                    New-NetFirewallRule @p2 | Out-Null
                }
            }
            return 'ok'
        }

        'pare-feu.debloquer' {
            $nom = Get-AideTexte $P 'Nom' 200
            if ($nom -notmatch '^(Nevermind|OptiGame) : bloque ') { throw 'Demande refusée : seules les règles de Nevermind peuvent être retirées.' }
            if ($Verifier) { return 'ok' }
            # (un blocage noté sous l'ancien nom « OptiGame : bloque ... » a pu être renommé depuis : les deux noms sont cherchés)
            $suite = $nom -replace '^(Nevermind|OptiGame) : bloque ', ''
            Import-AideModule NetSecurity
            @(Get-ReglesNevermind | Where-Object { $_.DisplayName -in "Nevermind : bloque $suite", "OptiGame : bloque $suite" }) | Remove-NetFirewallRule -ErrorAction Stop
            return 'ok'
        }

        'origines.oublier' {   # (après une restauration complète : ces valeurs ne doivent pas resservir des mois plus tard)
            if ($Verifier) { return 'ok' }
            try { [Microsoft.Win32.Registry]::LocalMachine.DeleteSubKeyTree($AideOrigines, $false) } catch {}
            return 'ok'
        }

        'pare-feu.debloquer-tout' {
            if ($Verifier) { return 'ok' }
            Import-AideModule NetSecurity
            @(Get-ReglesNevermind) | Remove-NetFirewallRule -ErrorAction Stop
            return 'ok'
        }

        'pare-feu.activer' {
            if ($Verifier) { return 'ok' }
            Import-AideModule NetSecurity
            Set-NetFirewallProfile -Profile Domain, Public, Private -Enabled True -ErrorAction Stop
            return 'ok'
        }

        'restauration.creer' {
            $desc = [string](Get-AideParam $P 'Description')
            if ($desc -notin @('Nevermind', 'Nevermind (manuel)')) { throw 'Demande refusée : description non valable.' }
            if ($Verifier) { return 'ok' }
            # Un par 24 h au plus : déjà un de moins de 24 h, c'est lui qui sert (« recent »)
            if (Test-AideRestaurationRecente (Get-AideDerniereRestauration)) { return 'recent' }
            Checkpoint-Computer -Description $desc -RestorePointType MODIFY_SETTINGS -ErrorAction Stop
            $k = [Microsoft.Win32.Registry]::LocalMachine.CreateSubKey($AideRestaurationCle)
            try { $k.SetValue('DerniereRestauration', [string][DateTime]::UtcNow.Ticks, 'String') } finally { $k.Close() }
            return 'ok'
        }

        'antivirus.retirer-exclusions' {
            $el = @(Get-AideListe $P 'Elements')
            if (-not $el.Count -or $el.Count -gt 200 -or @($el | Where-Object { -not $_ -or $_.Length -gt 1000 -or $_ -match '[\x00-\x1f]' }).Count) { throw 'Demande refusée : liste d''exclusions non valable.' }
            if ($Verifier) { return 'ok' }
            Import-AideModule ConfigDefender/Defender
            foreach ($x in $el) {
                Remove-MpPreference -ExclusionPath $x -ErrorAction SilentlyContinue
                Remove-MpPreference -ExclusionProcess $x -ErrorAction SilentlyContinue
                Remove-MpPreference -ExclusionExtension $x -ErrorAction SilentlyContinue
            }
            return 'ok'
        }

        # --- Lectures : ce que Windows ne montre qu'à un administrateur. Seulement des textes et des nombres en retour.

        # Secure Boot, puce TPM, santé de chaque disque (températures, usure, erreurs)
        'lecture.pc' {
            if ($Verifier) { return 'ok' }
            $r = @{ SecureBoot = $null; TpmOk = $false; Tpm = $null; Fiabilite = @{} }
            try { Import-AideModule SecureBoot; $r.SecureBoot = [bool](Confirm-SecureBootUEFI -ErrorAction Stop) } catch {}
            try {
                $t = @(Get-CimInstance -Namespace 'root\cimv2\security\microsofttpm' -ClassName Win32_Tpm -OperationTimeoutSec 3 -ErrorAction Stop)
                $r.TpmOk = $true
                if ($t.Count) { $r.Tpm = @{ SpecVersion = [string]$t[0].SpecVersion } }
            } catch {}
            try { Import-AideModule Storage } catch {}   # (sans lui : aucune santé de disque, le reste est rendu)
            foreach ($d in @(Get-PhysicalDisk -ErrorAction SilentlyContinue)) {
                $rel = $null
                try { $rel = $d | Get-StorageReliabilityCounter -ErrorAction Stop } catch {}
                if (-not $rel) { continue }
                $champs = @{}
                foreach ($n in $AideChampsDisque) { if ($null -ne $rel.$n) { $champs[$n] = [double]$rel.$n } }
                $r.Fiabilite[[string]$d.DeviceId] = $champs
            }
            return $r
        }

        # Antivirus : nombre d'exclusions, menaces connues et dernières détections
        'lecture.securite' {
            if ($Verifier) { return 'ok' }
            # (DetectionsLues : $true seulement si Defender a bien rendu ses détections ; sinon une liste vide ne veut pas dire
            # « aucune menace », et Nevermind affiche « Résultat inconnu »)
            # Exclusions : leur NOMBRE seulement, jamais les chemins. Windows les cache aux comptes sans droits : elles disent
            # où un programme malveillant échapperait à l'antivirus. (-1 : pas lues)
            $r = @{ ExclusionsNombre = -1; Menaces = @(); Detections = @(); DetectionsLues = $false }
            try { Import-AideModule ConfigDefender/Defender } catch {}   # (antivirus de Windows absent : listes vides)
            try {
                $pref = Get-MpPreference -ErrorAction Stop
                $r.ExclusionsNombre = @(@($pref.ExclusionPath) + @($pref.ExclusionProcess) + @($pref.ExclusionExtension) | Where-Object { $_ -and $_ -notmatch '^N/A' }).Count
            } catch {}
            try { $r.Menaces = @(Get-MpThreat -ErrorAction Stop | ForEach-Object { @{ ThreatID = [string]$_.ThreatID; ThreatName = [string]$_.ThreatName; SeverityID = [int]$_.SeverityID; IsActive = [bool]$_.IsActive } }) } catch {}
            try {
                $r.Detections = @(Get-MpThreatDetection -ErrorAction Stop | Sort-Object InitialDetectionTime -Descending | Select-Object -First 15 | ForEach-Object {
                    @{ ThreatID = [string]$_.ThreatID; ThreatStatusID = [int]$_.ThreatStatusID; Quand = [long]([datetime]$_.InitialDetectionTime).Ticks; Resources = @(@($_.Resources) | ForEach-Object { [string]$_ }) }
                })
                $r.DetectionsLues = $true
            } catch {}
            return $r
        }

        # --- Mesures en continu : l'Aide les fait tourner, Nevermind lit les résultats ---

        # FPS d'un jeu : PresentMon (celui du dossier de Nevermind) sur ce processus seulement ; ses lignes sont relayées telles quelles
        'fps.demarrer' {
            $idJeu = 0
            if (-not [int]::TryParse([string](Get-AideParam $P 'Pid'), [ref]$idJeu) -or $idJeu -le 4) { throw 'Demande refusée : processus non valable.' }
            if (-not (Get-Process -Id $idJeu -ErrorAction SilentlyContinue)) { throw "Le jeu n'est plus lancé." }
            if ($Verifier) { return 'ok' }
            if (-not ('AideFps' -as [type])) { throw 'La mesure des FPS se fait seulement dans l''Aide.' }
            # Jeu guetté par la veille (lancée avant lui) : la mesure continue sur celle là
            $nomJeu = try { "$((Get-Process -Id $idJeu -ErrorAction Stop).ProcessName).exe" } catch { '' }
            if ($nomJeu -and [AideFps]::Rattacher($nomJeu)) { return 'rattachee' }
            $pm = Get-AidePresentMon
            [AideFps]::Start($pm, $idJeu)
            return 'ok'
        }
        # Veille avant le lancement de jeux dont l'anti-triche refuse une mesure démarrée après eux : noms de programmes seulement
        'fps.veiller' {
            $noms = @(Get-AideListe $P 'Noms' | ForEach-Object { ([string]$_).ToLowerInvariant() } | Select-Object -Unique)
            if (-not $noms.Count -or $noms.Count -gt 10 -or @($noms | Where-Object { $_ -notmatch '^[a-z0-9][a-z0-9 ._()-]{0,60}\.exe$' }).Count) { throw 'Demande refusée : noms de programmes non valables.' }
            if ($Verifier) { return 'ok' }
            if (-not ('AideFps' -as [type])) { throw 'La mesure des FPS se fait seulement dans l''Aide.' }
            $pm = Get-AidePresentMon
            [AideFps]::Veiller($pm, [string[]]$noms)
            return 'ok'
        }
        'fps.lire' {
            if ($Verifier) { return 'ok' }
            if (-not ('AideFps' -as [type])) { return "#FIN`n" }
            return [AideFps]::Lire()
        }
        'fps.arreter' {
            if ($Verifier) { return 'ok' }
            if ('AideFps' -as [type]) { [AideFps]::Stop() }
            return 'ok'
        }

        # Connexions TCP avec les octets envoyés et reçus de chacune (Windows ne compte qu'à la demande d'un administrateur)
        'trafic.tcp' {
            if ($Verifier) { return 'ok' }
            Initialize-AideNatif
            $l = @([TrafficMon]::Tcp() | ForEach-Object { [string]$_ })
            return @{ Lignes = $l; CompteursOk = [bool][TrafficMon]::CountersOk }
        }

        # Trace réseau de Windows (chaque échange, UDP compris) : le vrai serveur du jeu pendant « Lag en ligne »
        'flux.demarrer' {
            if ($Verifier) { return 'ok' }
            Initialize-AideNatif
            if (-not [NetFlow]::Start()) { throw "Trace réseau refusée par Windows : $([NetFlow]::LastError)" }
            return 'ok'
        }
        # Veille avant un jeu dont l'anti-triche refuse une trace démarrée après lui : la trace tourne déjà quand la
        # partie commence (flux.demarrer la reprend). Redemandée toutes les 30 s : les vieilles connexions sont oubliées.
        'flux.veiller' {
            if ($Verifier) { return 'ok' }
            Initialize-AideNatif
            if ([NetFlow]::Running) { [NetFlow]::Purge(120); return 'ok' }
            if (-not [NetFlow]::Start()) { throw "Trace réseau refusée par Windows : $([NetFlow]::LastError)" }
            return 'ok'
        }
        'flux.lire' {
            if ($Verifier) { return 'ok' }
            if (-not ('NetFlow' -as [type]) -or -not [NetFlow]::Running) { throw 'La trace réseau ne tourne pas.' }
            return , @([NetFlow]::Snapshot() | ForEach-Object { [string]$_ })
        }
        'flux.arreter' {
            if ($Verifier) { return 'ok' }
            if (('NetFlow' -as [type]) -and [NetFlow]::Running) { [NetFlow]::Stop() }
            return 'ok'
        }

        # Tâches planifiées (hors Microsoft, actives) : sans droits, Windows peut en cacher. Nom, dossier, programme, arguments.
        # Seulement celles du compte qui demande et des comptes du système (Test-AideTacheDuCompte), jamais celles des
        # autres comptes du PC.
        'lecture.taches' {
            if ($Verifier) { return 'ok' }
            if ($Client -le 0) { throw 'Demande refusée : programme qui fait la demande inconnu.' }
            Initialize-AideProcessus
            $sidClient = [AideProcessus]::Compte($Client)
            if (-not $sidClient) { throw 'Demande refusée : compte du programme qui fait la demande illisible.' }
            Import-AideModule ScheduledTasks
            return , @(foreach ($t in @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskPath -notlike '\Microsoft\*' -and $_.State -ne 'Disabled' })) {
                if (-not (Test-AideTacheDuCompte ([string]$t.Principal.UserId) ([string]$t.Principal.GroupId) $sidClient)) { continue }
                foreach ($act in @($t.Actions)) { @{ Nom = [string]$t.TaskName; Chemin = [string]$t.TaskPath; Exe = [string]$act.Execute; Args = [string]$act.Arguments } }
            })
        }

        # Emplacement de programmes en cours (services, programmes lancés en administrateur), pour Trafic
        'lecture.chemins' {
            $ids = @(Get-AideListe $P 'Pids')
            if (-not $ids.Count -or $ids.Count -gt 500 -or @($ids | Where-Object { $_ -notmatch '^\d{1,9}$' }).Count) { throw 'Demande refusée : liste de processus non valable.' }
            if ($Verifier) { return 'ok' }
            $voulus = @{}; foreach ($i in $ids) { $voulus[[int]$i] = $true }
            $r = @{}
            foreach ($pr in @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)) {
                if ($voulus.ContainsKey([int]$pr.ProcessId) -and $pr.ExecutablePath) { $r[[string]$pr.ProcessId] = [string]$pr.ExecutablePath }
            }
            return $r
        }

        'wifi.manuel' {
            $profils = @(Get-AideListe $P 'Profils')
            if (-not $profils.Count -or $profils.Count -gt 50 -or @($profils | Where-Object { -not $_ -or $_.Length -gt 64 -or $_ -match '["\x00-\x1f]' -or $_ -match '\\$' }).Count) { throw 'Demande refusée : noms de Wi-Fi non valables.' }
            if ($Verifier) { return 'ok' }
            foreach ($n in $profils) { & ([IO.Path]::Combine([Environment]::GetFolderPath('System'), 'netsh.exe')) wlan set profileparameter "name=$n" connectionmode=manual | Out-Null }
            return 'ok'
        }

        'smb1.couper' {
            $smbClient = [bool](Get-AideParam $P 'Client')
            if ($Verifier) { return 'ok' }
            Import-AideModule SmbShare, Dism
            Set-SmbServerConfiguration -EnableSMB1Protocol $false -Force -Confirm:$false -ErrorAction SilentlyContinue
            if ($smbClient) { Disable-WindowsOptionalFeature -Online -FeatureName SMB1Protocol -NoRestart -ErrorAction SilentlyContinue | Out-Null }
            return 'ok'
        }

        'nettoyage.lister' {
            $id = [string](Get-AideParam $P 'Id')
            if (-not $AideNettoyage.Contains($id)) { throw "Demande refusée : catégorie de nettoyage $id inconnue." }
            $top = [int](Get-AideParam $P 'Top'); if ($top -lt 0 -or $top -gt 1000) { $top = 300 }
            if ($Verifier) { return 'ok' }
            $r = & $CleanListScript @{ Paths = $AideNettoyage[$id]; Top = $top; Exclude = @() }
            # Dates en ticks : elles traversent le JSON sans changer de forme
            return @{ Size = [double]$r.Size; Count = [int]$r.Count; Top = @(@($r.Top) | ForEach-Object { , @([string]$_[0], [double]$_[1], [long]$_[2].Ticks) }) }
        }

        'nettoyage.lancer' {
            $id = [string](Get-AideParam $P 'Id')
            if (-not $AideNettoyage.Contains($id)) { throw "Demande refusée : catégorie de nettoyage $id inconnue." }
            if ($Verifier) { return 'ok' }
            return (& $CleanScript @{ Paths = $AideNettoyage[$id]; Exclude = @() })
        }

        'processus.fermer' {
            # Par nom (programme gourmand pendant une partie) ou par chemin (programme bloqué dans Trafic)
            $nom = ([string](Get-AideParam $P 'Nom')).Trim(); $chemin = [string](Get-AideParam $P 'Chemin')   # (« MsMpEng  » = MsMpEng)
            if ($chemin) { $chemin = Get-AideProgramme $chemin $false; $nom = [IO.Path]::GetFileNameWithoutExtension($chemin) }
            elseif ($nom -notmatch '^[\w .()+\-]{1,100}$') { throw 'Demande refusée : nom de programme non valable.' }
            if ($nom -match $AideExesProteges) { throw "Demande refusée : $nom fait partie de Windows ou de l'antivirus." }
            $ctx = Get-AideContexte
            if ($chemin) { $refus = Get-AideRefusProgramme $ctx $chemin; if ($refus) { throw "Demande refusée : $([IO.Path]::GetFileName($chemin)) $refus." } }
            if ($Verifier) { return 'ok' }
            if ($Client -le 0) { throw 'Demande refusée : programme qui fait la demande inconnu.' }
            $n = 0
            # (par son nom exact : « [ » ou « * » dans un nom de programme ne sont pas des jokers)
            foreach ($pr in @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessName -eq $nom })) {
                $pc = $null; try { $pc = $pr.Path } catch {}
                if (-not $pc -or $pc.StartsWith("$AideWindows\", [StringComparison]::OrdinalIgnoreCase)) { continue }
                if ($chemin -and $pc -ne $chemin) { continue }
                # Seulement un programme de l'utilisateur, dans sa session : jamais un service, l'antivirus, un autre compte
                if (Get-AideRefusProcessus $ctx $Client $pr.Id $pc) { continue }
                try { $pr.Kill(); $n++ } catch {}
            }
            return $n
        }

        default { throw "L'Aide ne connaît pas la demande « $Action »." }
    }
}
