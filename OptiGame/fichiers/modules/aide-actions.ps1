# Nevermind : la liste stricte de ce que l'Aide accepte de faire avec les droits administrateur.
# Chargé par NevermindAide.ps1 (l'Aide, lancée en administrateur par la tâche « \Mindset\Nevermind (aide) ») et par
# OptiGame.ps1 (Nevermind s'en sert lui-même tant qu'il tourne en administrateur et que l'Aide ne répond pas).
#
# Autonome : pas de fenêtre, pas d'autre module, et aucun fichier de Nevermind n'est lu (ils sont modifiables sans
# droits). Chaque demande est vérifiée seule : ce qui n'est pas prévu ici est refusé.
# Ce qui affaiblirait la sécurité du PC n'est jamais accepté, même pour « revenir en arrière » : rallumer le Bureau à
# distance, couper le contrôle des comptes ou le pare-feu, ajouter une exclusion à l'antivirus, mettre un DNS inconnu,
# bloquer Internet à l'antivirus ou à Windows Update.

$AideVersion = 3
# Santé d'un disque (Get-StorageReliabilityCounter) : les champs dont Nevermind se sert
$AideChampsDisque = @('Temperature', 'TemperatureMax', 'Wear', 'PowerOnHours', 'StartStopCycleCount', 'ReadErrorsCorrected', 'ReadErrorsUncorrected',
                      'WriteErrorsUncorrected', 'ReadLatencyMax', 'WriteLatencyMax')
$AideMemoire = 'HKLM:\SOFTWARE\Mindset\Nevermind\Aide'   # ce que l'Aide retient elle-même (modifiable seulement en administrateur)
$AideDossierModules = $PSScriptRoot   # (dossier protégé de Nevermind : natif.cs et PresentMon y sont lus)

# Les mesureurs de Nevermind (natif.cs : compteurs TCP, trace réseau), chargés dans l'Aide à la première mesure
function Initialize-AideNatif {
    if ('TrafficMon' -as [type]) { return }
    try { Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $AideDossierModules 'natif.cs'), [Text.Encoding]::UTF8)) }
    catch { if (-not ('TrafficMon' -as [type])) { throw } }   # (une autre demande l'a chargé au même moment)
}

# Programmes de Windows et de l'antivirus : jamais fermés, jamais bloqués, jamais de priorité changée
$AideExesProteges = '^(csrss|lsass|lsaiso|smss|wininit|winlogon|services|svchost|System|Registry|Idle|MsMpEng|NisSrv|MpCmdRun|MpDefenderCoreService|SecurityHealthService|SecurityHealthSystray|SgrmBroker|dwm|explorer|fontdrvhost|sihost|ctfmon|conhost|audiodg|spoolsv|taskhostw|RuntimeBroker|TrustedInstaller|TiWorker|wuauclt|MoUsoCoreWorker|Nevermind|Mindset)(\.exe)?$'
$AideServicesProteges = '^(WinDefend|WdNisSvc|WdFilter|Sense|SecurityHealthService|wscsvc|mpssvc|BFE|wuauserv|UsoSvc|WaaSMedicSvc|BITS|DoSvc|CryptSvc|TrustedInstaller|Dnscache|Dhcp|NlaSvc|nsi|W32Time|EventLog|RpcSs)$'

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
       Nom = 'CpuPriorityClass'; Type = 'DWord'; Min = 1; Max = 6; Effacer = $true }
    # Programmes lancés avec Windows pour tous les utilisateurs : seulement les marquer actifs ou coupés
    @{ Motif = '^HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\(Run|Run32|StartupFolder)$'
       NomMotif = '^[^\x00-\x1f]{1,260}$'; Type = 'Binary'; Longueur = 12; Premier = @(2, 3, 6, 7); Effacer = $true }
)

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

function Get-AideCarte([int]$Index) {
    if ($Index -le 0) { throw 'Demande refusée : carte réseau non valable.' }
    $ad = Get-NetAdapter -InterfaceIndex $Index -IncludeHidden -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $ad) { throw 'La carte réseau de ce réglage n''existe plus sur ce PC.' }
    $ad
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
# ---------------------------------------------------------------------------
$AideNettoyage = [ordered]@{
    wintemp = @("$env:windir\Temp")
    wu      = @("$env:windir\SoftwareDistribution\Download")
    do      = @("$env:windir\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache")
    wer     = @("$env:ProgramData\Microsoft\Windows\WER\ReportArchive", "$env:ProgramData\Microsoft\Windows\WER\ReportQueue")
    dump    = @("$env:windir\Minidump")
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
    $defender = @("$env:ProgramFiles\Windows Defender\", "${env:ProgramFiles(x86)}\Windows Defender\", "$env:ProgramData\Microsoft\Windows Defender\")
    foreach ($d in $defender) { if ($plein.StartsWith($d, [StringComparison]::OrdinalIgnoreCase)) { throw "Demande refusée : $nom fait partie de l'antivirus." } }
    if (-not $WindowsPermis -and $plein.StartsWith("$env:windir\", [StringComparison]::OrdinalIgnoreCase)) { throw "Demande refusée : $nom fait partie de Windows." }
    $plein
}

# ---------------------------------------------------------------------------
# Les demandes
# ---------------------------------------------------------------------------
# -Verifier : contrôle la demande sans rien faire (tests automatiques). Renvoie un résultat simple (texte, nombre,
# table) que l'Aide renvoie à Nevermind en JSON. Une demande refusée lève une erreur dont le message est montré tel quel.
function Invoke-AideAction([string]$Action, $P, [switch]$Verifier) {
    switch -Exact ($Action) {
        'aide.bonjour' {
            $admin = (New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
            return @{ Version = $AideVersion; Admin = $admin; Pid = $PID }
        }

        'reg.ecrire' {
            $cle = ConvertTo-AideCle (Get-AideParam $P 'Cle')
            $nom = [string](Get-AideParam $P 'Nom')
            $r = Get-AideRegle $cle $nom
            $v = ConvertTo-AideValeur $r (Get-AideParam $P 'Valeur') ([string](Get-AideParam $P 'Type'))
            if ($Verifier) { return 'ok' }
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
            $k = Open-AideCle $cle $false
            if ($k) { try { $k.DeleteValue($nom, $false) } finally { $k.Close() } }
            return 'ok'
        }

        'service.regler' {
            $nom = [string](Get-AideParam $P 'Nom')
            if ($nom -notin @('DiagTrack')) { throw "Demande refusée : le service $nom n'est pas dans la liste de l'Aide." }
            $dem = [string](Get-AideParam $P 'Demarrage')
            if ($dem -notin @('Disabled', 'Manual', 'Automatic')) { throw "Demande refusée : type de démarrage $dem." }
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
            $connu = (-not $serveurs.Count) -or @($AideDnsChoix | Where-Object { ($_ -join ',') -eq $texte }).Count -gt 0 -or
                     -not @($serveurs | Where-Object { -not (Test-AideAdresseLocale $_) }).Count
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
            if ($serveurs.Count) { Set-DnsClientServerAddress -InterfaceIndex $ad.ifIndex -ServerAddresses $serveurs -ErrorAction Stop }
            else { Set-DnsClientServerAddress -InterfaceIndex $ad.ifIndex -ResetServerAddresses -ErrorAction Stop }
            Clear-DnsClientCache -ErrorAction SilentlyContinue
            return 'ok'
        }

        'pare-feu.bloquer' {
            $nom = Get-AideTexte $P 'Nom' 200
            if ($nom -notmatch '^OptiGame : bloque [^*?\[\]]+$') { throw 'Demande refusée : nom de règle non valable.' }
            $services = @(Get-AideListe $P 'Services')
            foreach ($s in $services) {
                if ($s -notmatch '^[\w.\-]{1,80}$') { throw "Demande refusée : service $s non valable." }
                if ($s -match $AideServicesProteges) { throw "Demande refusée : le service $s est indispensable à Windows ou à l'antivirus." }
            }
            $prog = Get-AideProgramme ([string](Get-AideParam $P 'Programme')) $true
            $court = [IO.Path]::GetFileName($prog)
            if ($court -match $AideExesProteges -and -not ($court -eq 'svchost.exe' -and $services.Count)) { throw "Demande refusée : $court est indispensable à Windows ou à l'antivirus." }
            if ($Verifier) { return 'ok' }
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
            if ($nom -notlike 'OptiGame : bloque *') { throw 'Demande refusée : seules les règles de Nevermind peuvent être retirées.' }
            if ($Verifier) { return 'ok' }
            @(Get-NetFirewallRule -DisplayName 'OptiGame : bloque *' -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName -eq $nom }) | Remove-NetFirewallRule -ErrorAction Stop
            return 'ok'
        }

        'pare-feu.debloquer-tout' {
            if ($Verifier) { return 'ok' }
            Get-NetFirewallRule -DisplayName 'OptiGame : bloque *' -ErrorAction SilentlyContinue | Remove-NetFirewallRule -ErrorAction Stop
            return 'ok'
        }

        'pare-feu.activer' {
            if ($Verifier) { return 'ok' }
            Set-NetFirewallProfile -Profile Domain, Public, Private -Enabled True -ErrorAction Stop
            return 'ok'
        }

        'restauration.creer' {
            $desc = [string](Get-AideParam $P 'Description')
            if ($desc -notin @('Nevermind', 'Nevermind (manuel)')) { throw 'Demande refusée : description non valable.' }
            if ($Verifier) { return 'ok' }
            Checkpoint-Computer -Description $desc -RestorePointType MODIFY_SETTINGS -ErrorAction Stop
            return 'ok'
        }

        'antivirus.retirer-exclusions' {
            $el = @(Get-AideListe $P 'Elements')
            if (-not $el.Count -or $el.Count -gt 200 -or @($el | Where-Object { -not $_ -or $_.Length -gt 1000 -or $_ -match '[\x00-\x1f]' }).Count) { throw 'Demande refusée : liste d''exclusions non valable.' }
            if ($Verifier) { return 'ok' }
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
            try { $r.SecureBoot = [bool](Confirm-SecureBootUEFI -ErrorAction Stop) } catch {}
            try {
                $t = @(Get-CimInstance -Namespace 'root\cimv2\security\microsofttpm' -ClassName Win32_Tpm -OperationTimeoutSec 3 -ErrorAction Stop)
                $r.TpmOk = $true
                if ($t.Count) { $r.Tpm = @{ SpecVersion = [string]$t[0].SpecVersion } }
            } catch {}
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

        # Antivirus : exclusions, menaces connues et dernières détections
        'lecture.securite' {
            if ($Verifier) { return 'ok' }
            $r = @{ Exclusions = @(); Menaces = @(); Detections = @() }
            try {
                $pref = Get-MpPreference -ErrorAction Stop
                $r.Exclusions = @(@($pref.ExclusionPath) + @($pref.ExclusionProcess) + @($pref.ExclusionExtension) | Where-Object { $_ -and $_ -notmatch '^N/A' } | ForEach-Object { [string]$_ })
            } catch {}
            try { $r.Menaces = @(Get-MpThreat -ErrorAction Stop | ForEach-Object { @{ ThreatID = [string]$_.ThreatID; ThreatName = [string]$_.ThreatName; SeverityID = [int]$_.SeverityID; IsActive = [bool]$_.IsActive } }) } catch {}
            try {
                $r.Detections = @(Get-MpThreatDetection -ErrorAction Stop | Sort-Object InitialDetectionTime -Descending | Select-Object -First 15 | ForEach-Object {
                    @{ ThreatID = [string]$_.ThreatID; ThreatStatusID = [int]$_.ThreatStatusID; Quand = [long]([datetime]$_.InitialDetectionTime).Ticks; Resources = @(@($_.Resources) | ForEach-Object { [string]$_ }) }
                })
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
            $pm = Join-Path (Split-Path $AideDossierModules -Parent) 'outils-tiers\PresentMon.exe'
            if (-not (Test-Path -LiteralPath $pm)) { throw 'PresentMon est absent du dossier de Nevermind.' }
            [AideFps]::Start($pm, $idJeu)
            return 'ok'
        }
        # Veille avant le lancement de jeux dont l'anti-triche refuse une mesure démarrée après eux : noms de programmes seulement
        'fps.veiller' {
            $noms = @(Get-AideListe $P 'Noms' | ForEach-Object { ([string]$_).ToLowerInvariant() } | Select-Object -Unique)
            if (-not $noms.Count -or $noms.Count -gt 10 -or @($noms | Where-Object { $_ -notmatch '^[a-z0-9][a-z0-9 ._()-]{0,60}\.exe$' }).Count) { throw 'Demande refusée : noms de programmes non valables.' }
            if ($Verifier) { return 'ok' }
            if (-not ('AideFps' -as [type])) { throw 'La mesure des FPS se fait seulement dans l''Aide.' }
            $pm = Join-Path (Split-Path $AideDossierModules -Parent) 'outils-tiers\PresentMon.exe'
            if (-not (Test-Path -LiteralPath $pm)) { throw 'PresentMon est absent du dossier de Nevermind.' }
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
        'lecture.taches' {
            if ($Verifier) { return 'ok' }
            return , @(foreach ($t in @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskPath -notlike '\Microsoft\*' -and $_.State -ne 'Disabled' })) {
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
            if (-not $profils.Count -or $profils.Count -gt 50 -or @($profils | Where-Object { -not $_ -or $_.Length -gt 64 -or $_ -match '["\x00-\x1f]' }).Count) { throw 'Demande refusée : noms de Wi-Fi non valables.' }
            if ($Verifier) { return 'ok' }
            foreach ($n in $profils) { & "$env:windir\System32\netsh.exe" wlan set profileparameter "name=$n" connectionmode=manual | Out-Null }
            return 'ok'
        }

        'smb1.couper' {
            $client = [bool](Get-AideParam $P 'Client')
            if ($Verifier) { return 'ok' }
            Set-SmbServerConfiguration -EnableSMB1Protocol $false -Force -Confirm:$false -ErrorAction SilentlyContinue
            if ($client) { Disable-WindowsOptionalFeature -Online -FeatureName SMB1Protocol -NoRestart -ErrorAction SilentlyContinue | Out-Null }
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
            $nom = [string](Get-AideParam $P 'Nom'); $chemin = [string](Get-AideParam $P 'Chemin')
            if ($chemin) { $chemin = Get-AideProgramme $chemin $false; $nom = [IO.Path]::GetFileNameWithoutExtension($chemin) }
            elseif ($nom -notmatch '^[\w .()+\-]{1,100}$') { throw 'Demande refusée : nom de programme non valable.' }
            if ($nom -match $AideExesProteges) { throw "Demande refusée : $nom fait partie de Windows ou de l'antivirus." }
            if ($Verifier) { return 'ok' }
            $n = 0
            foreach ($pr in @(Get-Process -Name $nom -ErrorAction SilentlyContinue)) {
                $pc = $null; try { $pc = $pr.Path } catch {}
                if (-not $pc -or $pc.StartsWith("$env:windir\", [StringComparison]::OrdinalIgnoreCase)) { continue }
                if ($chemin -and $pc -ne $chemin) { continue }
                try { $pr.Kill(); $n++ } catch {}
            }
            return $n
        }

        default { throw "L'Aide ne connaît pas la demande « $Action »." }
    }
}
