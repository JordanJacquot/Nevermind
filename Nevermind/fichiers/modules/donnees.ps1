# Nevermind : données, sauvegarde et accès au registre.
# Chargé par Nevermind.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Données et sauvegarde
# ---------------------------------------------------------------------------
# $DataDir : choisi tout au début de Nevermind.ps1 (Resolve-DataDir) : %LOCALAPPDATA%\OptiGame, ou
# %LOCALAPPDATA%\Mindset\Nevermind quand Nevermind est géré par Mindset. Tous les fichiers de l'app passent par lui.
$BackupFile = Join-Path $DataDir 'sauvegarde.json'
$LogFile    = Join-Path $DataDir 'journal.txt'
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
# Journal gardé petit : au delà de 1 Mo, les anciennes lignes passent dans journal-ancien.txt.
# Un gros fichier rouvert à chaque ligne écrite fait attendre la fenêtre (l'antivirus le relit à chaque fois).
try {
    $logInfo = New-Object IO.FileInfo $LogFile
    if ($logInfo.Exists -and $logInfo.Length -gt 1MB) {
        $logLines = [IO.File]::ReadAllLines($LogFile, [Text.Encoding]::UTF8)
        if ($logLines.Count -gt 3000) {
            $cut = $logLines.Count - 3000
            [IO.File]::WriteAllLines((Join-Path $DataDir 'journal-ancien.txt'), [string[]]$logLines[0..($cut - 1)], (New-Object Text.UTF8Encoding($false)))
            [IO.File]::WriteAllLines($LogFile, [string[]]$logLines[$cut..($logLines.Count - 1)], (New-Object Text.UTF8Encoding($false)))
        }
        $logLines = $null
    }
} catch {}

function Write-Log([string]$Message) {
    $line = "$(Get-Date -Format s) $Message`r`n"
    try { [IO.File]::AppendAllText($LogFile, $line, (New-Object Text.UTF8Encoding($false))) }
    catch {
        # Journal inaccessible : on garde la cause (une seule fois) dans les préférences, qui elles s'écrivent.
        if (-not $script:LogError) {
            $script:LogError = "$(Get-Date -Format s) $($_.Exception.GetType().Name): $($_.Exception.Message)"
            try { Set-Setting 'LogError' $script:LogError } catch {}
        }
    }
}

# Fichiers de données (préférences, historique, mesures, jeux...) : jamais perdus. Écrits d'abord à côté (.tmp), puis
# l'ancien est remplacé d'un coup et reste en copie de secours (.bak) : un arrêt brutal ne laisse jamais un fichier à
# moitié écrit. À la lecture, un fichier abîmé est mis de côté (.abime-<date>), noté au journal, et sa copie de secours
# reprise : jamais une liste vide qui écraserait tout à l'enregistrement suivant.
function Save-TexteSur([string]$Path, [string]$Texte, [bool]$Bom = $false) {
    $tmp = "$Path.tmp"
    [IO.File]::WriteAllText($tmp, $Texte, (New-Object Text.UTF8Encoding $Bom))
    if (-not [IO.File]::Exists($Path)) { [IO.File]::Move($tmp, $Path); return }
    try { [IO.File]::Replace($tmp, $Path, "$Path.bak") }
    catch {
        # (disque qui ne sait pas remplacer d'un coup : copie de secours, puis copie par dessus)
        [IO.File]::Copy($Path, "$Path.bak", $true)
        [IO.File]::Copy($tmp, $Path, $true)
        try { [IO.File]::Delete($tmp) } catch {}
    }
}
function Save-JsonSur([string]$Path, $Objet, [int]$Profondeur = 6, [switch]$Compress, [switch]$Bom) {
    # Fichier illisible au démarrage (pris par un autre programme) : pas réécrit, il garde peut-être plus que ce qu'on a
    if ($script:JsonBloques -and $script:JsonBloques[$Path]) { throw "$([IO.Path]::GetFileName($Path)) n'a pas pu être lu : il n'est pas réécrit" }
    $json = if ($Compress) { ConvertTo-Json -InputObject $Objet -Depth $Profondeur -Compress } else { ConvertTo-Json -InputObject $Objet -Depth $Profondeur }
    Save-TexteSur $Path $json ([bool]$Bom)
}
# Le contenu du fichier (ou de sa copie de secours), $null s'il n'y en a pas
function Read-JsonSur([string]$Path) {
    if (-not $script:JsonBloques) { $script:JsonBloques = @{} }
    $script:JsonBloques.Remove($Path)
    $occupe = $false
    foreach ($p in $Path, "$Path.bak") {
        if (-not [IO.File]::Exists($p)) { continue }
        $texte = $null; $err = $null
        for ($i = 0; $i -lt 3 -and $null -eq $texte; $i++) {
            try { $texte = [IO.File]::ReadAllText($p, [Text.Encoding]::UTF8) } catch { $err = $_; Start-Sleep -Milliseconds 50 }
        }
        # Pris par un autre programme (antivirus...) : il n'est pas abîmé, on n'y touche pas
        if ($null -eq $texte) { Write-Log "Fichier illisible pour l'instant ($p) : $($err.Exception.Message)"; $occupe = $true; continue }
        try {
            if (-not $texte.Trim()) { throw 'fichier vide' }
            $j = ConvertFrom-Json $texte
            if ($p -ne $Path) { Write-Log "$([IO.Path]::GetFileName($Path)) repris de sa copie de secours" }
            return $j   # (une liste arrive élément par élément : @() autour de l'appel la refait)
        } catch {
            $dest = "$p.abime-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
            Write-Log "Fichier abîmé ($p : $($_.Exception.Message)), mis de côté : $([IO.Path]::GetFileName($dest))"
            try { [IO.File]::Move($p, $dest) } catch { Write-Log "Fichier abîmé pas mis de côté ($p) : $($_.Exception.Message)" }
        }
    }
    if ($occupe) { $script:JsonBloques[$Path] = $true }
    $null
}

# La sauvegarde ne dépend pas d'un seul fichier (issue #3) : sauvegarde.json, sa copie de secours (.bak, la version
# d'avant), et les valeurs d'origine gardées dans le registre (réglages du PC : par l'Aide, dans HKLM, protégé ;
# réglages de l'utilisateur : par Nevermind, dans HKCU). Un fichier abîmé n'est jamais remplacé en silence par une
# sauvegarde vide : il est mis de côté, et $script:BackupProbleme le dit (« Tout restaurer », désinstallation).
$OriginesMachine = 'SOFTWARE\Mindset\Nevermind\Aide\Origines'
$OriginesUtilisateur = 'Software\Mindset\Nevermind\Origines'
# Copie de test : jamais la vraie clé (les réglages d'essai des tests y restaient, et la vraie appli les reprenait).
# Une clé à part, donnée par outils\tester.ps1 (NEVERMIND_TEST_REG, effacée à la fin du test).
if ($env:NEVERMIND_TEST) { $OriginesUtilisateur = "$(if ($env:NEVERMIND_TEST_REG -like 'Software\MindsetTest*') { $env:NEVERMIND_TEST_REG } else { 'Software\MindsetTest' })\Origines" }
function Get-CleRegNormale([string]$Path) {
    $c = ([string]$Path).Trim() -replace '^Registry::', ''
    $c -replace '^(HKEY_LOCAL_MACHINE|HKLM:?)\\', 'HKLM:\' -replace '^(HKEY_USERS|HKU:?)\\', 'HKU:\' -replace '^(HKEY_CURRENT_USER|HKCU:?)\\', 'HKCU:\'
}
function Read-OriginesRegistre {
    @(foreach ($r in @(@([Microsoft.Win32.Registry]::LocalMachine, $OriginesMachine), @([Microsoft.Win32.Registry]::CurrentUser, $OriginesUtilisateur))) {
        try {
            $k = $r[0].OpenSubKey($r[1])
            if (-not $k) { continue }
            try { foreach ($n in $k.GetValueNames()) { try { [string]$k.GetValue($n) | ConvertFrom-Json } catch {} } } finally { $k.Close() }
        } catch {}
    })
}
function Read-FichierSauvegarde([string]$Fichier) {
    if (-not (Test-Path -LiteralPath $Fichier)) { return $null }
    $j = Get-Content -LiteralPath $Fichier -Raw -Encoding UTF8 | ConvertFrom-Json   # (lève une erreur si abîmé)
    if ($null -eq $j -or -not $j.PSObject.Properties['Registry']) { throw 'contenu inattendu' }
    $j
}
function Import-Backup {
    # Oublies : réglages déjà remis comme avant par « Tout restaurer » alors que d'autres restent à remettre. Leur copie
    # dans le registre (gardée par l'Aide) n'est alors pas reprise : elle sert jusqu'à ce que tout soit remis.
    $script:Backup = @{ Registry = @{}; PowerScheme = $null; CreatedScheme = $null; Overlay = $null; Dns = @{}; Displays = @{}; Oublies = @() }
    $script:BackupProbleme = $null
    $j = $null
    try { $j = Read-FichierSauvegarde $BackupFile }
    catch {
        Write-Log "Sauvegarde abîmée ($($_.Exception.Message)) : mise de côté"
        $script:BackupProbleme = 'abimee'
        try { Move-Item -LiteralPath $BackupFile -Destination (Join-Path $DataDir "sauvegarde-abimee-$(Get-Date -Format 'yyyyMMdd-HHmmss').json") -Force } catch {}
    }
    if ($null -eq $j) { try { $j = Read-FichierSauvegarde "$BackupFile.bak"; if ($j) { Write-Log 'Sauvegarde reprise de sa copie de secours' } } catch {} }
    try {
        if ($null -eq $j) { throw 'pas de fichier' }
        if ($j.Registry) {
            foreach ($p in $j.Registry.PSObject.Properties) {
                $v = $p.Value
                $script:Backup.Registry[$p.Name] = @{ Path = $v.Path; Name = $v.Name; Existed = [bool]$v.Existed; Value = $v.Value; Kind = $v.Kind }
            }
        }
        if ($j.PowerScheme)   { $script:Backup.PowerScheme = [string]$j.PowerScheme }
        if ($j.CreatedScheme) { $script:Backup.CreatedScheme = [string]$j.CreatedScheme }
        if ($j.Dns) { foreach ($p in $j.Dns.PSObject.Properties) { $script:Backup.Dns[$p.Name] = [string]$p.Value } }
        if ($j.Displays) { foreach ($p in $j.Displays.PSObject.Properties) { $script:Backup.Displays[$p.Name] = [int]$p.Value } }
        if ($j.Overlay) { $script:Backup.Overlay = [string]$j.Overlay }
        if ($j.PSObject.Properties['Oublies']) { $script:Backup.Oublies = @(@($j.Oublies) | Where-Object { $_ } | ForEach-Object { [string]$_ }) }
    } catch { if ($_.Exception.Message -ne 'pas de fichier') { Write-Log "Lecture de la sauvegarde impossible: $_" } }
    # Les valeurs d'origine du registre complètent ce que le fichier n'a plus (fichier effacé, abîmé, ou plus ancien)
    $connues = @{}; foreach ($e in @($script:Backup.Registry.Values)) { $connues["$(Get-CleRegNormale $e.Path)|$($e.Name)"] = $true }
    foreach ($o in @($script:Backup.Oublies)) { $connues[$o] = $true }
    $reprises = 0
    foreach ($o in @(Read-OriginesRegistre)) {
        if (-not $o -or -not $o.Path -or -not $o.Name) { continue }
        $cle = "$(Get-CleRegNormale $o.Path)|$($o.Name)"
        if ($connues[$cle]) { continue }
        $script:Backup.Registry["$($o.Path)|$($o.Name)"] = @{ Path = [string]$o.Path; Name = [string]$o.Name; Existed = [bool]$o.Existed; Value = $o.Value; Kind = $o.Kind }
        $connues[$cle] = $true; $reprises++
    }
    if ($reprises) { Write-Log "Sauvegarde : $reprises valeur(s) d'origine reprise(s) du registre" }
    # Plus rien alors que l'historique montre des changements pas annulés : la sauvegarde a disparu
    if (-not $script:BackupProbleme -and -not $script:Backup.Registry.Count) {
        try {
            if ($null -eq $script:History) { Import-History }
            if (@(@($script:History) | Where-Object { $_ -and -not $_.Annule -and @(@($_.Log) | Where-Object { $_.Type -eq 'reg' }).Count }).Count) { $script:BackupProbleme = 'absente' }
        } catch {}
    }
}

# Ce que « Tout restaurer » et la désinstallation disent quand la sauvegarde a un problème
function Get-TexteProblemeSauvegarde {
    if (-not $script:BackupProbleme) { return '' }
    "$(if ($script:BackupProbleme -eq 'abimee') { 'Le fichier qui garde les réglages d''origine est abîmé' } else { 'Le fichier qui garde les réglages d''origine a disparu' }) : certains réglages changés par Nevermind ne pourront peut-être pas être remis comme avant. Pour tout remettre, utilise le point de restauration de Windows créé par Nevermind (page Sauvegarde, « Restauration du système »)."
}

# Points que l'utilisateur a choisi d'ignorer (ex: un écran volontairement en 60 Hz).
$IgnoreFile = Join-Path $DataDir 'ignores.json'

function Import-Ignored {
    $script:Ignored = @()
    try { $arr = Read-JsonSur $IgnoreFile; if ($null -ne $arr) { $script:Ignored = @(@($arr) | Where-Object { $_ } | ForEach-Object { [string]$_ }) } } catch {}
}

function Save-Ignored {
    Save-JsonSur $IgnoreFile @($script:Ignored)
}

# Préférences de l'utilisateur (versions bêta, dernière version vue, visite guidée...).
$SettingsFile = Join-Path $DataDir 'parametres.json'

function Get-Setting([string]$Name, $Default = $null) {
    if ($null -eq $script:Settings) {
        $script:Settings = @{}
        # (abîmé : sa copie de secours est reprise ; elles gardent aussi SvcOriginal et PcfgOriginal, de quoi remettre
        # les services et l'alimentation comme avant)
        try { $j = Read-JsonSur $SettingsFile; if ($j) { foreach ($p in $j.PSObject.Properties) { $script:Settings[$p.Name] = $p.Value } } } catch { Write-Log "Lecture des préférences impossible: $_" }
    }
    if ($script:Settings.ContainsKey($Name)) { $script:Settings[$Name] } else { $Default }
}

# -Exiger : la valeur doit être écrite sur le disque (état d'origine d'un service ou de l'alimentation, sans lequel
# « Tout restaurer » ne pourrait pas tout remettre). Sinon, rien n'est gardé en mémoire et l'erreur remonte : celui
# qui appelle ne change rien.
function Set-Setting([string]$Name, $Value, [switch]$Exiger) {
    [void](Get-Setting $Name)
    $avait = $script:Settings.ContainsKey($Name); $avant = $script:Settings[$Name]
    $script:Settings[$Name] = $Value
    try { Save-JsonSur $SettingsFile $script:Settings }
    catch {
        Write-Log "Écriture des préférences impossible: $_"
        if ($Exiger) {
            if ($avait) { $script:Settings[$Name] = $avant } else { $script:Settings.Remove($Name) }
            throw "Nevermind n'a pas pu noter l'état d'origine (son fichier de préférences est pris par un autre programme) : rien n'a été changé. Réessaie dans un instant."
        }
    }
}

# L'état d'origine noté dans sauvegarde.json AVANT le changement : s'il n'a pas pu être écrit, il est oublié (le
# prochain essai le relira) et l'erreur remonte, sans rien changer. Sinon, un 2e essai changerait sans sauvegarde.
function Save-BackupOuOublier([scriptblock]$Oublier) {
    try { Save-Backup } catch { & $Oublier; throw "Nevermind n'a pas pu noter l'état d'origine (sauvegarde.json : $($_.Exception.Message)) : rien n'a été changé." }
}

function Set-DisplayRate([string]$Device, [int]$Hz) {
    $current = @([OGNative]::GetDisplays()) | Where-Object { ($_ -split '\|')[0] -eq $Device } | Select-Object -First 1
    if ($current -and -not $script:Backup.Displays.ContainsKey($Device)) {
        $script:Backup.Displays[$Device] = [int](($current -split '\|')[4])
        Save-BackupOuOublier { $script:Backup.Displays.Remove($Device) }
    }
    $r = [OGNative]::SetRefreshRate($Device, $Hz)
    if ($r -ne 0) { throw "Windows a refusé le changement de fréquence (code $r)." }
    if ($null -ne $script:RunLog -and $current) {
        [void]$script:RunLog.Add(@{ Type = 'display'; Device = $Device; Hz = [int](($current -split '\|')[4]); Nouveau = $Hz })
    }
}

# Les lectures du registre passent par .NET, en lecture seule : Test-Path et Get-ItemProperty mettent plus de 30 ms à
# dire qu'une clé de HKLM n'existe pas (une lecture par jeu pour la priorité haute : plus d'une demi-seconde de fenêtre
# figée), .NET moins d'une milliseconde. Clé illisible sans droits administrateur : une erreur, comme avec Get-Item.
# Ruche et sous-clé d'un chemin PowerShell (HKCU:\..., HKLM:\..., Registry::HKEY_USERS\...) ; $null pour les autres.
function Split-RegChemin([string]$Path) {
    $pth = $Path -replace '^Registry::', ''
    if ($pth -match '^(HKCU:|HKEY_CURRENT_USER)\\?(.*)$')     { return @{ Ruche = [Microsoft.Win32.Registry]::CurrentUser; Cle = $matches[2] } }
    if ($pth -match '^(HKLM:|HKEY_LOCAL_MACHINE)\\?(.*)$') { return @{ Ruche = [Microsoft.Win32.Registry]::LocalMachine; Cle = $matches[2] } }
    if ($pth -match '^HKEY_USERS\\?(.*)$')                 { return @{ Ruche = [Microsoft.Win32.Registry]::Users; Cle = $matches[1] } }
    $null
}

# État actuel d'une valeur du registre (pour pouvoir revenir en arrière).
function Get-RegState([string]$Path, [string]$Name) {
    $s = @{ Existed = $false; Value = $null; Kind = $null }
    $r = Split-RegChemin $Path
    if ($r) {
        $k = $r.Ruche.OpenSubKey($r.Cle, $false)
        if ($k) {
            try {
                if ($k.GetValueNames() -contains $Name) {
                    $s.Existed = $true
                    $s.Value = $k.GetValue($Name, $null, 'DoNotExpandEnvironmentNames')
                    $s.Kind = $k.GetValueKind($Name).ToString()
                }
            } finally { $k.Close() }
        }
    } elseif (Test-Path -LiteralPath $Path) {
        $item = Get-Item -LiteralPath $Path
        if ($item.GetValueNames() -contains $Name) {
            $s.Existed = $true
            $s.Value = $item.GetValue($Name, $null, 'DoNotExpandEnvironmentNames')
            $s.Kind = $item.GetValueKind($Name).ToString()
        }
    }
    $s
}

# Écrit d'abord à côté, puis remplace : un arrêt brutal ne laisse jamais un fichier à moitié écrit ; la version
# d'avant reste en copie de secours (.bak)
function Save-Backup {
    Save-TexteSur $BackupFile ($script:Backup | ConvertTo-Json -Depth 6) $true
}

function Get-RegValue([string]$Path, [string]$Name) {
    try {
        $r = Split-RegChemin $Path
        if (-not $r) { $p = Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop; return , $p.$Name }
        $k = $r.Ruche.OpenSubKey($r.Cle, $false)
        if (-not $k) { return $null }
        try { $v = $k.GetValue($Name) } finally { $k.Close() }
        if ($null -eq $v) { return $null }
        return , $v
    } catch { return $null }
}

# Mémorise la valeur d'origine avant la première modification.
function Save-Original([string]$Path, [string]$Name) {
    $key = "$Path|$Name"
    if ($script:Backup.Registry.ContainsKey($key)) { return }
    $exists = $false; $val = $null; $kind = $null
    if (Test-Path -LiteralPath $Path) {
        $item = Get-Item -LiteralPath $Path
        if ($item.GetValueNames() -contains $Name) {
            $exists = $true
            $val = $item.GetValue($Name, $null, 'DoNotExpandEnvironmentNames')
            $kind = $item.GetValueKind($Name).ToString()
            if ($val -is [byte[]]) { $val = [int[]]$val }
        }
    }
    $script:Backup.Registry[$key] = @{ Path = $Path; Name = $Name; Existed = $exists; Value = $val; Kind = $kind }
    Save-BackupOuOublier { $script:Backup.Registry.Remove($key) }
    # Réglage de l'utilisateur : une copie dans son registre aussi (ceux du PC sont gardés par l'Aide, dans HKLM)
    if (Test-RegUtilisateur $Path) {
        try {
            $k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($OriginesUtilisateur)
            try { if ($null -eq $k.GetValue($key, $null)) { $k.SetValue($key, (ConvertTo-Json -InputObject $script:Backup.Registry[$key] -Compress -Depth 3), 'String') } } finally { $k.Close() }
        } catch {}
    }
}

# Ouvre une clé du registre à partir d'un chemin PowerShell (HKCU:\..., HKLM:\..., Registry::HKEY_USERS\...).
function Open-RegKey([string]$Path, [bool]$Create) {
    $r = Split-RegChemin $Path
    if (-not $r) { throw "Chemin de registre non pris en charge: $Path" }
    if ($Create) { return $r.Ruche.CreateSubKey($r.Cle) }
    $r.Ruche.OpenSubKey($r.Cle, $true)
}

# Réglage de l'utilisateur (HKCU) : Nevermind l'écrit lui-même. Le reste (réglages du PC) passe par l'Aide (aide-client.ps1).
function Test-RegUtilisateur([string]$Path) { ($Path -replace '^Registry::', '') -match '^(HKCU:|HKEY_CURRENT_USER)' }

# Écrit ou supprime une valeur, noms spéciaux compris (crochets, antislashs).
function Write-RegValue([string]$Path, [string]$Name, $Value, [string]$Kind) {
    if (-not (Test-RegUtilisateur $Path)) {
        $v = if ($Value -is [byte[]]) { [int[]]$Value } else { $Value }
        [void](Invoke-Aide 'reg.ecrire' @{ Cle = $Path; Nom = $Name; Valeur = $v; Type = $Kind })
        return
    }
    $k = Open-RegKey $Path $true
    try { $k.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]$Kind) } finally { $k.Close() }
}

function Remove-RegValue([string]$Path, [string]$Name) {
    if (-not (Test-RegUtilisateur $Path)) { [void](Invoke-Aide 'reg.effacer' @{ Cle = $Path; Nom = $Name }); return }
    $k = Open-RegKey $Path $false
    if ($k) { try { $k.DeleteValue($Name, $false) } finally { $k.Close() } }
}

function Clear-Reg([string]$Path, [string]$Name) {
    $st = Get-RegState $Path $Name
    if (-not $st.Existed) { return }
    Save-Original $Path $Name
    if ($null -ne $script:RunLog) { [void]$script:RunLog.Add(@{ Type = 'reg'; Path = $Path; Name = $Name; Existed = $true; Value = $st.Value; Kind = $st.Kind; Efface = $true }) }
    Remove-RegValue $Path $Name
}

function Set-Reg([string]$Path, [string]$Name, $Value, [string]$Kind = 'DWord') {
    Save-Original $Path $Name
    if ($null -ne $script:RunLog) {
        $st = Get-RegState $Path $Name
        [void]$script:RunLog.Add(@{ Type = 'reg'; Path = $Path; Name = $Name; Existed = $st.Existed; Value = $st.Value; Kind = $st.Kind; Nouveau = $(if ($Value -is [byte[]]) { [int[]]$Value } else { $Value }); NouveauType = $Kind })
    }
    Write-RegValue $Path $Name $Value $Kind
}
