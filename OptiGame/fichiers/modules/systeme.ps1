# Nevermind : connexion active, nettoyage, restauration et désinstallation.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Réseau
# ---------------------------------------------------------------------------
function Get-ActiveNet {
    $n = Invoke-Async $ActiveNetWork | Select-Object -First 1
    if ($n) { $n } else { $null }
}

$DnsChoices = @(
    @(),
    @('1.1.1.1', '1.0.0.1'),
    @('8.8.8.8', '8.8.4.4'),
    @('9.9.9.9', '149.112.112.112')
)

# ---------------------------------------------------------------------------
# Nettoyage
# ---------------------------------------------------------------------------
# Catégories du nettoyage. Desc : ce que c'est, en une phrase. Default $false : décochée au départ (à garder le plus souvent).
# Exclude : sous dossiers jamais touchés (le dossier de travail de Claude Code est dans les temporaires de l'utilisateur).
$CleanTargets = @(
    @{ Id = 'temp'; Titre = 'Fichiers temporaires'; Paths = @($env:TEMP); Exclude = @('claude')
       Desc = 'Restes laissés par tes programmes et les installations. Ce qui a moins de 24 h ou sert encore est gardé.' },
    @{ Id = 'claude'; Titre = 'Fichiers de travail de Claude'; Paths = @("$env:TEMP\claude"); Default = $false; Warn = $true; Optional = $true
       Desc = 'Captures, scripts et téléchargements des discussions Claude Code. Tes discussions ne sont pas effacées, mais une discussion en cours peut encore en avoir besoin : garde-les si tu travailles avec Claude.' },
    # Dossiers de Windows : lus et vidés par l'Aide, qui a sa propre liste de dossiers (aide-actions.ps1, $AideNettoyage)
    @{ Id = 'wintemp'; Titre = 'Fichiers temporaires de Windows'; Paths = $AideNettoyage['wintemp']; Aide = $true
       Desc = 'Restes laissés par Windows et les programmes installés pour tout le PC.' },
    @{ Id = 'wu'; Titre = 'Mises à jour Windows déjà installées'; Paths = $AideNettoyage['wu']; Aide = $true
       Desc = 'Fichiers téléchargés par Windows Update, inutiles une fois les mises à jour installées.' },
    @{ Id = 'do'; Titre = 'Partage des mises à jour'; Paths = $AideNettoyage['do']; Aide = $true
       Desc = 'Morceaux de mises à jour que Windows garde pour les partager avec d''autres PC. Windows les retélécharge au besoin.' },
    @{ Id = 'wer'; Titre = 'Rapports d''erreurs'; Paths = $AideNettoyage['wer']; Aide = $true
       Desc = 'Rapports créés quand une appli plante, déjà envoyés ou inutiles.' },
    @{ Id = 'dump'; Titre = 'Rapports d''écran bleu'; Paths = $AideNettoyage['dump']; Aide = $true
       Desc = 'Créés lors d''un écran bleu. Utiles seulement pour comprendre un écran bleu récent.' }
)

# $CleanWalk, $CleanListScript et $CleanScript (parcours sans suivre les liens, analyse, nettoyage) sont dans
# aide-actions.ps1 : l'Aide s'en sert aussi pour les dossiers de Windows.

# Analyse d'une catégorie (taille, nombre, les plus gros fichiers), sur place ou par l'Aide. $null si impossible.
function Get-CleanInfo($Target, [int]$Top = 300) {
    if (-not $Target.Aide) { return (Invoke-Async $CleanListScript @{ Paths = $Target.Paths; Top = $Top; Exclude = $Target.Exclude } | Select-Object -First 1) }
    try { $r = Invoke-Aide 'nettoyage.lister' @{ Id = $Target.Id; Top = $Top } }
    catch { Write-Log "Nettoyage ($($Target.Id)) : $($_.Exception.Message)"; return $null }
    # Dates envoyées en ticks par l'Aide
    @{ Size = [double]$r.Size; Count = [int]$r.Count; Top = @(@($r.Top) | ForEach-Object { , @([string]$_[0], [double]$_[1], [datetime][long]$_[2]) }) }
}

# Nettoyage d'une catégorie, sur place ou par l'Aide : { Freed, Deleted, Skipped, Errors, Log }
function Invoke-CleanOne($Target) {
    if (-not $Target.Aide) { return (Invoke-Async $CleanScript @{ Paths = $Target.Paths; Exclude = $Target.Exclude } | Select-Object -First 1) }
    try { Invoke-Aide 'nettoyage.lancer' @{ Id = $Target.Id } 900000 }
    catch { @{ Freed = 0.0; Deleted = 0; Skipped = 0; Errors = 1; Log = @("[REFUSÉ]   $($_.Exception.Message)") } }
}

# Nettoie les catégories et écrit le journal (dossier « nettoyage » des données de Nevermind, 10 derniers gardés)
function Invoke-CleanTargets([array]$Targets) {
    $now = Get-Date
    $dir = Join-Path $DataDir 'nettoyage'
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add("Nettoyage Nevermind du $($now.ToString('dd/MM/yyyy à HH:mm:ss'))")
    $lines.Add('SUPPRIMÉ : effacé.  LAISSÉ : utilisé par un programme, il sera effacé une prochaine fois.  REFUSÉ : Windows n''autorise pas à l''effacer.')
    $tot = @{ Freed = 0.0; Deleted = 0; Skipped = 0; Errors = 0 }
    foreach ($t in $Targets) {
        Set-Status "Nettoyage: $($t.Titre)..."
        $r = Invoke-CleanOne $t
        $lines.Add('')
        $lines.Add("== $($t.Titre) ($($t.Paths -join ', ')) ==")
        if (-not $r) { $lines.Add('(rien à nettoyer)'); continue }
        foreach ($l in @($r.Log)) { $lines.Add($l) }
        if (-not @($r.Log).Count) { $lines.Add('(dossier déjà vide)') }
        $tot.Freed += $r.Freed; $tot.Deleted += $r.Deleted; $tot.Skipped += $r.Skipped; $tot.Errors += $r.Errors
    }
    $lines.Add('')
    $lines.Add("Total : $(Format-Size $tot.Freed) libérés, $($tot.Deleted) fichier(s) supprimé(s), $($tot.Skipped) laissé(s), $($tot.Errors) refusé(s).")
    $file = Join-Path $dir "nettoyage-$($now.ToString('yyyy-MM-dd_HH-mm-ss')).log"
    try { [IO.File]::WriteAllLines($file, $lines, (New-Object Text.UTF8Encoding($true))) } catch { Write-Log "Nettoyage: journal non écrit ($_)"; $file = '' }
    foreach ($old in @(Get-ChildItem -LiteralPath $dir -Filter 'nettoyage-*.log' -ErrorAction SilentlyContinue | Sort-Object Name -Descending | Select-Object -Skip 10)) { try { [IO.File]::Delete($old.FullName) } catch {} }
    Set-Setting 'LastClean' @{ Date = $now.ToString('o'); Freed = $tot.Freed; Deleted = $tot.Deleted; Skipped = $tot.Skipped; Errors = $tot.Errors; File = $file }
    Write-Log "Nettoyage: $(Format-Size $tot.Freed) libérés, $($tot.Deleted) supprimés, $($tot.Skipped) laissés, $($tot.Errors) refusés"
    $tot.File = $file
    $tot
}

function Format-Size([double]$Bytes) {
    if ($Bytes -ge 1TB) { return '{0:N1} To' -f ($Bytes / 1TB) }
    if ($Bytes -ge 1GB) { return '{0:N1} Go' -f ($Bytes / 1GB) }
    if ($Bytes -ge 1MB) { return '{0:N0} Mo' -f ($Bytes / 1MB) }
    '{0:N0} Ko' -f ($Bytes / 1KB)
}

# ---------------------------------------------------------------------------
# Restauration de tous les réglages modifiés par Nevermind
# ---------------------------------------------------------------------------
# Seuls les réglages vraiment remis quittent la sauvegarde : ceux qui n'ont pas pu l'être (Aide administrateur qui ne
# répond pas, service refusé...) y restent, avec leur valeur d'origine, pour « Réessayer » plus tard. La sauvegarde
# n'est effacée que quand tout est remis. Renvoie la liste de ce qui n'a pas pu être remis (vide si tout est remis).
function Restore-AllSettings {
    $errors = @()
    $script:RestaurationPareFeu = $true
    try { if (@(Get-NetFirewallRule -DisplayName 'Nevermind : bloque *' -ErrorAction SilentlyContinue).Count + @(Get-NetFirewallRule -DisplayName 'OptiGame : bloque *' -ErrorAction SilentlyContinue).Count) { [void](Invoke-Aide 'pare-feu.debloquer-tout') } } catch { $errors += "Pare-feu: $($_.Exception.Message)"; $script:RestaurationPareFeu = $false }
    $oublies = New-Object System.Collections.ArrayList
    foreach ($cle in @($script:Backup.Registry.Keys)) {
        $e = $script:Backup.Registry[$cle]
        try {
            if ($e.Existed) {
                $val = switch ($e.Kind) {
                    'Binary' { [byte[]]@($e.Value) }
                    'DWord'  { [int]$e.Value }
                    'QWord'  { [long]$e.Value }
                    default  { $e.Value }
                }
                Write-RegValue $e.Path $e.Name $val $e.Kind
            } else {
                Remove-RegValue $e.Path $e.Name
            }
            $script:Backup.Registry.Remove($cle)
            [void]$oublies.Add("$(Get-CleRegNormale $e.Path)|$($e.Name)")
            # (sa copie dans le registre de l'utilisateur ne sert plus)
            if (Test-RegUtilisateur $e.Path) {
                try { $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($OriginesUtilisateur, $true); if ($k) { try { $k.DeleteValue($cle, $false) } finally { $k.Close() } } } catch {}
            }
        } catch { $errors += "$($e.Path)\$($e.Name): $($_.Exception.Message)" }
    }
    # Services arrêtés par Nevermind (télémétrie) : état d'origine ; ceux qui n'ont pas pu être remis restent notés
    $svcReste = @()
    foreach ($l in @(Get-Setting 'SvcOriginal' @())) {
        $x = ([string]$l) -split '\|'
        try { [void](Invoke-Aide 'service.regler' @{ Nom = $x[0]; Demarrage = $x[1]; Lancer = ($x[2] -eq 'True') }) }
        catch { $errors += "Service $($x[0]): $($_.Exception.Message)"; $svcReste += [string]$l }
    }
    if (@(Get-Setting 'SvcOriginal' @()).Count) { Set-Setting 'SvcOriginal' @($svcReste) }
    # Réglages « batterie » changés pour l'onduleur
    $pcfg = @(Get-Setting 'PcfgOriginal' @())
    $pcfgReste = @()
    foreach ($l in $pcfg) {
        $x = ([string]$l) -split '\|'
        try {
            if ($x[1] -ne '') { $global:LASTEXITCODE = 0; powercfg /setacvalueindex SCHEME_CURRENT e73a048d-bf27-4f12-9731-8b2076e8891f $x[0] ([int]$x[1]) | Out-Null; if ($global:LASTEXITCODE) { throw "powercfg a répondu $global:LASTEXITCODE" } }
            if ($x[2] -ne '') { $global:LASTEXITCODE = 0; powercfg /setdcvalueindex SCHEME_CURRENT e73a048d-bf27-4f12-9731-8b2076e8891f $x[0] ([int]$x[2]) | Out-Null; if ($global:LASTEXITCODE) { throw "powercfg a répondu $global:LASTEXITCODE" } }
        } catch { $errors += "Onduleur: $($_.Exception.Message)"; $pcfgReste += [string]$l }
    }
    if ($pcfg.Count) { powercfg /setactive SCHEME_CURRENT | Out-Null; Set-Setting 'PcfgOriginal' @($pcfgReste) }
    if ($script:Backup.PowerScheme) {
        $global:LASTEXITCODE = 0; powercfg /setactive $script:Backup.PowerScheme | Out-Null
        if ($global:LASTEXITCODE) { $errors += "Plan d'alimentation: pas remis (powercfg a répondu $global:LASTEXITCODE)" }
        else {
            if ($script:Backup.CreatedScheme -and $script:Backup.CreatedScheme -ne $script:Backup.PowerScheme) { powercfg /delete $script:Backup.CreatedScheme 2>$null | Out-Null }
            $script:Backup.PowerScheme = $null; $script:Backup.CreatedScheme = $null
        }
    } elseif ($script:Backup.CreatedScheme) {
        powercfg /delete $script:Backup.CreatedScheme 2>$null | Out-Null
        $script:Backup.CreatedScheme = $null
    }
    if ($script:Backup.Overlay) {
        try {
            $r = [OGNative]::SetOverlay($script:Backup.Overlay)
            if ($r -ne 0) { throw "code $r" }
            $script:Backup.Overlay = $null
        } catch { $errors += "Mode d'alimentation: $($_.Exception.Message)" }
    }
    foreach ($k in @($script:Backup.Dns.Keys)) {
        try {
            $v = $script:Backup.Dns[$k]
            [void](Invoke-Aide 'dns.regler' @{ Index = [int]$k; Serveurs = @(([string]$v) -split '[,\s]+' | Where-Object { $_ }) })
            $script:Backup.Dns.Remove($k)
        } catch { $errors += "DNS: $($_.Exception.Message)" }
    }
    foreach ($k in @($script:Backup.Displays.Keys)) {
        $r = [OGNative]::SetRefreshRate($k, [int]$script:Backup.Displays[$k])
        if ($r -ne 0) { $errors += "Écran $($k): fréquence non restaurée (code $r)" } else { $script:Backup.Displays.Remove($k) }
    }
    try { Sync-Mouse } catch {}
    # Plus aucun réglage du registre à remettre : leurs copies dans le registre ne servent plus
    $originesOubliees = $false
    if (-not $script:Backup.Registry.Count) {
        try { [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($OriginesUtilisateur, $false) } catch {}
        try { [void](Invoke-Aide 'origines.oublier'); $originesOubliees = $true }
        catch { try { [Microsoft.Win32.Registry]::LocalMachine.DeleteSubKeyTree($OriginesMachine, $false); $originesOubliees = $true } catch {} }
    }
    $reste = $script:Backup.Registry.Count + $script:Backup.Dns.Count + $script:Backup.Displays.Count + [int][bool]$script:Backup.PowerScheme + [int][bool]$script:Backup.Overlay
    if (-not $reste -and $originesOubliees) {
        Remove-Item -LiteralPath $BackupFile, "$BackupFile.bak", "$BackupFile.tmp" -ErrorAction SilentlyContinue
    } else {
        # Ce qui reste à remettre est gardé ; les réglages remis sont « oubliés » (leur copie gardée par l'Aide n'est pas reprise)
        $script:Backup.Oublies = @(@($script:Backup.Oublies) + @($oublies) | Where-Object { $_ } | Select-Object -Unique)
        try { Save-Backup } catch { $errors += "Sauvegarde: $($_.Exception.Message)" }
        if ($reste) { Write-Log "Tout restaurer : $reste réglage(s) pas remis, gardé(s) dans la sauvegarde" }
    }
    Import-Backup
    , $errors
}

# Une entrée de l'historique est entièrement remise quand plus rien de ce qu'elle a changé n'attend dans la sauvegarde
# (après « Tout restaurer » : seules celles-là sont marquées annulées)
function Test-HistoriqueRemis($H) {
    $regReste = @{}; foreach ($e in @($script:Backup.Registry.Values)) { $regReste["$(Get-CleRegNormale $e.Path)|$($e.Name)"] = $true }
    $svcReste = @(@(Get-Setting 'SvcOriginal' @()) | ForEach-Object { (([string]$_) -split '\|')[0] })
    $pcfgReste = @(@(Get-Setting 'PcfgOriginal' @()) | ForEach-Object { (([string]$_) -split '\|')[0] })
    foreach ($e in @($H.Log)) {
        if (-not $e) { continue }
        $resteIci = switch ([string]$e.Type) {
            'reg'     { [bool]$regReste["$(Get-CleRegNormale ([string]$e.Path))|$($e.Name)"] }
            'svc'     { $svcReste -contains [string]$e.Name }
            'pcfg'    { [bool]($pcfgReste | Where-Object { $_ -and ($_ -eq [string]$e.Guid -or $_ -eq [string]$e.Key) }) }
            'power'   { [bool]$script:Backup.PowerScheme }
            'overlay' { [bool]$script:Backup.Overlay }
            'dns'     { [bool]$script:Backup.Dns.Count }
            'display' { $script:Backup.Displays.ContainsKey([string]$e.Device) }
            'fw'      { $script:RestaurationPareFeu -eq $false }
            default   { $false }
        }
        if ($resteIci) { return $false }
    }
    $true
}

# ---------------------------------------------------------------------------
# Raccourci sur le bureau et lancement au démarrage du PC
# ---------------------------------------------------------------------------
# Le démarrage passe par une tâche planifiée « avec les droits les plus élevés » : Nevermind s'ouvre
# sans la demande d'autorisation de Windows (une simple entrée « Exécuter » la ferait apparaître à chaque démarrage).
$AutoStartTask = 'Nevermind (démarrage)'
# Anciens noms de l'app : OptiGame (jusqu'à 1.0.55), Nexo (1.0.56)
$OldAppNames = @('OptiGame', 'Nexo')
$OldAutoStartTasks = @($OldAppNames | ForEach-Object { "$_ (démarrage)" })

function Get-AppRoot { if ((Split-Path $AppDir -Leaf) -eq 'fichiers') { Split-Path $AppDir -Parent } else { $AppDir } }
# Le programme à lancer : Mindset.exe du dossier (celui qui fait tourner Nevermind, outils\lanceur.cs). Nevermind.exe n'est plus
# qu'un relais vers lui, gardé pour les anciens raccourcis et les anciennes tâches : les deux sont reconnus (Get-AppExes).
function Get-AppExe { Join-Path (Get-AppRoot) 'Mindset.exe' }
function Get-AppExes { @((Join-Path (Get-AppRoot) 'Mindset.exe'), (Join-Path (Get-AppRoot) 'Nevermind.exe')) }
function Get-DesktopShortcutPath([string]$Name = 'Nevermind') {
    $desk = if ($script:DesktopDir) { $script:DesktopDir } else { [Environment]::GetFolderPath('Desktop') }
    Join-Path $desk "$Name.lnk"
}

# Cible d'un raccourci (vide s'il n'existe pas ou est illisible)
function Get-ShortcutTarget([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return '' }
    try {
        $sh = New-Object -ComObject WScript.Shell
        try { [string]$sh.CreateShortcut($Path).TargetPath } finally { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($sh) }
    } catch { '' }
}

function Test-DesktopShortcut { $t = Get-ShortcutTarget (Get-DesktopShortcutPath); [bool]($t -and (Get-AppExes) -contains $t) }

# Son raccourci du menu Démarrer (dossier « Mindset »), écrit par Mindset : il porte l'identité de Nevermind
# (JordanJacquot.Nevermind), d'où vient le nom « Nevermind » de son bouton dans la barre des tâches. Retiré à la
# désinstallation s'il lance ce Nevermind (le dossier aussi, s'il est alors vide).
function Get-MenuShortcutPath {
    $menu = if ($script:MenuDir) { $script:MenuDir } else { [Environment]::GetFolderPath('Programs') }
    Join-Path $menu 'Mindset\Nevermind.lnk'
}
function Remove-MenuShortcut {
    $l = Get-MenuShortcutPath
    $t = Get-ShortcutTarget $l
    if (-not $t -or (Get-AppExes) -notcontains $t) { return }
    [IO.File]::Delete($l)
    $d = Split-Path $l -Parent
    if (-not @(Get-ChildItem -LiteralPath $d -Force -ErrorAction SilentlyContinue).Count) { Remove-Item -LiteralPath $d -Force -ErrorAction SilentlyContinue }
}

function New-DesktopShortcut {
    $root = Get-AppRoot
    $exe = Get-AppExe
    if (-not (Test-Path -LiteralPath $exe)) { throw "Mindset.exe est introuvable dans le dossier $root." }
    $sh = New-Object -ComObject WScript.Shell
    try {
        $lnk = $sh.CreateShortcut((Get-DesktopShortcutPath))
        $lnk.TargetPath = $exe
        $lnk.WorkingDirectory = $root
        $lnk.IconLocation = "$exe,0"
        $lnk.Description = 'Nevermind : ton PC, tes jeux, ton réseau'
        $lnk.Save()
    } finally { [void][Runtime.InteropServices.Marshal]::ReleaseComObject($sh) }
}

function Get-AutoStartTask { Get-ScheduledTask -TaskName $AutoStartTask -ErrorAction SilentlyContinue | Select-Object -First 1 }
function Test-AutoStart { $null -ne (Get-AutoStartTask) }

function Set-AutoStart([bool]$On) {
    if (-not $On) {
        if (Get-AutoStartTask) { Unregister-ScheduledTask -TaskName $AutoStartTask -Confirm:$false -ErrorAction Stop }
        return
    }
    $ps1 = Join-Path $AppDir 'OptiGame.ps1'
    $user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    $exe = @(Get-AppExes | Where-Object { Test-Path -LiteralPath $_ })[0]   # (Mindset.exe, ou Nevermind.exe d'une ancienne version)
    $act = if ($exe) { New-ScheduledTaskAction -Execute "`"$exe`"" -Argument '-Demarrage' -WorkingDirectory $env:USERPROFILE }
           else { New-ScheduledTaskAction -Execute "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -Argument "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$ps1`" -Demarrage" -WorkingDirectory $env:USERPROFILE }
    $trg = New-ScheduledTaskTrigger -AtLogOn -User $user
    $trg.Delay = 'PT15S'   # laisse Windows finir d'ouvrir la session
    $pr = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Highest
    $set = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $AutoStartTask -Action $act -Trigger $trg -Principal $pr -Settings $set -Description 'Lance Nevermind à l''ouverture de session, réduit près de l''horloge. Se règle dans Nevermind, page Sauvegarde.' -Force -ErrorAction Stop | Out-Null
}

# Changement de nom (OptiGame, puis Nexo, puis Nevermind) : les anciennes tâches de démarrage, les anciens
# raccourcis du bureau et les anciens lanceurs sont remplacés. Seulement ce qui appartient à cette installation.
# Tâches planifiées présentes parmi ces noms, avec leurs arguments : une seule lecture, en arrière plan
# (lire les tâches dans la fenêtre la figeait plus d'une seconde au démarrage).
function Get-TaskInfo([string[]]$Names) {
    @(Invoke-Async {
        param($names)
        foreach ($n in $names) {
            $t = Get-ScheduledTask -TaskName $n -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($t) { @{ Name = $n; Args = [string]@($t.Actions)[0].Arguments; Exe = ([string]@($t.Actions)[0].Execute).Trim('"') } }
        }
    } ([string[]]$Names) | Where-Object { $_ })
}

function Invoke-NameMigration {
    if ($Managed) { return }   # (copie de Mindset : installée par lui sous ce nom, dans un dossier protégé)
    $root = Get-AppRoot
    $newExe = Get-AppExe
    if (-not (Test-Path -LiteralPath $newExe)) { return }   # pas encore de Mindset.exe (copie de développement)
    $found = @(Get-TaskInfo $OldAutoStartTasks | ForEach-Object { $_.Name })
    foreach ($task in $OldAutoStartTasks) {
        try {
            if ($found -contains $task) {
                if (-not (Test-DemarrageParMindset)) { Set-AutoStart $true }
                Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction Stop
                Write-Log "Nevermind : tâche de démarrage « $task » renommée"
            }
        } catch { Write-Log "Nevermind, tâche de démarrage : $_" }
    }
    foreach ($old in $OldAppNames) {
        try {
            $oldLnk = Get-DesktopShortcutPath $old
            # (copie gérée par Mindset : c'est lui qui crée le raccourci, on n'en crée jamais)
            if (-not $Managed -and (Get-ShortcutTarget $oldLnk) -eq (Join-Path $root "$old.exe")) {
                New-DesktopShortcut
                [IO.File]::Delete($oldLnk)
                Write-Log "Nevermind : raccourci « $old » du bureau remplacé"
            }
        } catch { Write-Log "Nevermind, raccourci : $_" }
        foreach ($n in "$old.exe", "Désinstaller $old.exe") {
            $p = Join-Path $root $n
            if (Test-Path -LiteralPath $p) { try { [IO.File]::Delete($p); Write-Log "Nevermind : ancien lanceur retiré ($n)" } catch {} }
        }
    }
}

# Dossier de Nevermind déplacé ou renommé : la tâche de démarrage suit
function Update-AutoStartPath {
    if ($Managed -or (Test-DemarrageParMindset)) { return }   # (le démarrage avec Windows passe par Mindset)   # (Mindset s'en occupe : sa tâche a été reprise, on ne la recrée pas)
    $t = @(Get-TaskInfo @($AutoStartTask))[0]
    if (-not $t) { return }
    $ps1 = Join-Path $AppDir 'OptiGame.ps1'
    $exes = @(Get-AppExes | Where-Object { Test-Path -LiteralPath $_ })
    # La tâche doit lancer Mindset.exe de ce dossier, ou son relais Nevermind.exe (tâche d'une version d'avant : elle marche,
    # on n'y touche pas) ; les plus anciennes lançaient PowerShell avec le script
    $ok = if ($exes.Count) { ($exes -contains $t.Exe) -and $t.Args -eq '-Demarrage' } else { $t.Args -like "*`"$ps1`"*" }
    if (-not $ok) {
        try { Set-AutoStart $true; Write-Log "Démarrage automatique : chemin mis à jour ($ps1)" } catch { Write-Log "Démarrage automatique : $_" }
    }
}

# ---------------------------------------------------------------------------
# Désinstallation
# ---------------------------------------------------------------------------
# Les données (Mindset issue #2) : gardées, sauf si l'utilisateur demande de les supprimer. Quoi qu'il arrive, Nevermind
# remet d'abord les réglages de Windows comme avant ; ce qui les décrivait part alors toujours : sauvegarde.json, sa copie
# de secours, les sauvegardes abîmées mises de côté, et l'historique des changements (tout y est annulé). Une réinstallation
# ne doit jamais « restaurer » des valeurs vieilles de plusieurs mois. L'historique n'est gardé que si des réglages n'ont
# pas pu être remis comme avant (ou si la sauvegarde avait un problème) : il montre alors ce qui reste à défaire.
# Partent aussi les demandes de passage (*.demande). Le reste est gardé : préférences, jeux et jaquettes, mesures (FPS,
# lag), organiseur, appareils du réseau, thèmes, journal. « Supprimer » retire tout le dossier des données.
# Le choix vient de la ligne de commande (« Désinstaller Nevermind.exe -GarderDonnees » ou « -SupprimerDonnees », utilisé
# par le désinstalleur de Mindset : aucune question), sinon il est demandé dans la fenêtre de désinstallation.
# Le lanceur (outils\lanceur.cs) le transmet par $env:NEVERMIND_DONNEES (« garder » ou « supprimer ») ; OptiGame.ps1 peut
# aussi le recevoir en paramètres ($GarderDonnees, $SupprimerDonnees).
function Get-ChoixDonneesImpose {
    $s = [bool](Get-Variable -Name SupprimerDonnees -ValueOnly -ErrorAction SilentlyContinue) -or "$env:NEVERMIND_DONNEES" -eq 'supprimer'
    $g = [bool](Get-Variable -Name GarderDonnees -ValueOnly -ErrorAction SilentlyContinue) -or "$env:NEVERMIND_DONNEES" -eq 'garder'
    if ($s -and -not $g) { 'supprimer' } elseif ($g -and -not $s) { 'garder' } else { '' }
}

# Les données après la restauration. $GarderHistorique : des réglages n'ont pas pu être remis comme avant.
# Renvoie la liste des erreurs (vide si tout s'est bien passé).
function Remove-DonneesDesinstallation([bool]$Garder, [bool]$GarderHistorique) {
    $errors = @()
    if (-not [IO.Directory]::Exists($DataDir)) { return , $errors }
    if (-not $Garder) {
        try { Remove-Item -LiteralPath $DataDir -Recurse -Force -ErrorAction Stop } catch { $errors += "Données: $($_.Exception.Message)" }
        return , $errors
    }
    $partent = @('sauvegarde.json', 'sauvegarde.json.bak', 'sauvegarde.json.tmp', 'sauvegarde-abimee-*.json', '*.demande')
    if (-not $GarderHistorique) { $partent += 'historique.json', 'historique.json.bak', 'historique.json.tmp' }   # (sa copie de secours serait reprise)
    foreach ($m in $partent) {
        foreach ($e in @(Get-ChildItem -LiteralPath $DataDir -Filter $m -File -Force -ErrorAction SilentlyContinue)) {
            try { [IO.File]::Delete($e.FullName) } catch { $errors += "Données ($($e.Name)): $($_.Exception.Message)" }
        }
    }
    # Plus rien à garder : le dossier part aussi
    try { if (-not @([IO.Directory]::EnumerateFileSystemEntries($DataDir)).Count) { [IO.Directory]::Delete($DataDir) } } catch {}
    , $errors
}

# La fenêtre de désinstallation : ce qui va être fait, et la case « Supprimer aussi mes données » (décochée au départ).
# Renvoie 'garder', 'supprimer', ou $null si l'utilisateur annule.
# Tests (outils\tester-desinstallation.ps1) : -Essai 0 laisse la case, 1 la coche, 2 annule ; la fenêtre est alors hors
# de l'écran, sans prendre la main, et se ferme seule. -Image : la fenêtre est aussi enregistrée en image.
function Show-ChoixDesinstallation([string]$Texte, [int]$Essai = -1, [string]$Image = '') {
    $xaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="Désinstaller Nevermind" Width="520" SizeToContent="Height" ResizeMode="NoResize" WindowStartupLocation="CenterScreen"
        Background="#110F19" BorderBrush="#2E2843" BorderThickness="1" FontFamily="Segoe UI Variable Text, Segoe UI">
  <Window.Resources>
    <Style x:Key="Btn" TargetType="Button">
      <Setter Property="Foreground" Value="#E6E1F5"/><Setter Property="FontSize" Value="13"/><Setter Property="FontWeight" Value="SemiBold"/>
      <Setter Property="Padding" Value="18,9"/><Setter Property="Cursor" Value="Hand"/><Setter Property="Background" Value="#14FFFFFF"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Border x:Name="b" CornerRadius="12" Padding="{TemplateBinding Padding}" Background="{TemplateBinding Background}" BorderBrush="#2EFFFFFF" BorderThickness="1">
              <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/>
            </Border>
            <ControlTemplate.Triggers><Trigger Property="IsMouseOver" Value="True"><Setter TargetName="b" Property="Opacity" Value="0.85"/></Trigger></ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>
  </Window.Resources>
  <StackPanel Margin="28,24,28,22">
    <TextBlock Text="Désinstaller Nevermind ?" Foreground="White" FontSize="18" FontWeight="Bold" FontFamily="Segoe UI Variable Display, Segoe UI"/>
    <ScrollViewer MaxHeight="340" VerticalScrollBarVisibility="Auto" Margin="0,10,0,0">
      <TextBlock x:Name="Texte" Foreground="#A6A1BC" FontSize="13" TextWrapping="Wrap" LineHeight="19"/>
    </ScrollViewer>
    <CheckBox x:Name="CaseDonnees" Margin="0,18,0,0" Foreground="#E6E1F5" FontSize="13" Content="Supprimer aussi mes données (préférences, jeux, mesures, journal)"/>
    <TextBlock x:Name="Note" Margin="20,6,0,0" FontSize="12.5" TextWrapping="Wrap" Foreground="#A6A1BC"/>
    <StackPanel Orientation="Horizontal" HorizontalAlignment="Right" Margin="0,22,0,0">
      <Button x:Name="BtnOui" Style="{StaticResource Btn}" Content="Désinstaller" Background="#C2334B" Foreground="White" Margin="0,0,10,0"/>
      <Button x:Name="BtnNon" Style="{StaticResource Btn}" Content="Annuler" IsCancel="True" IsDefault="True"/>
    </StackPanel>
  </StackPanel>
</Window>
'@
    $w = [Windows.Markup.XamlReader]::Parse($xaml)
    $w.FindName('Texte').Text = $Texte
    $case = $w.FindName('CaseDonnees'); $note = $w.FindName('Note')
    $majNote = {
        if ($case.IsChecked) { $note.Text = 'Attention : tes données seront effacées pour de bon.'; $note.Foreground = [Windows.Media.BrushConverter]::new().ConvertFromString('#FF6B81') }
        else { $note.Text = 'Tes données sont gardées : si tu réinstalles Nevermind, tu retrouveras tes préférences, tes jeux et tes mesures.'; $note.Foreground = [Windows.Media.BrushConverter]::new().ConvertFromString('#A6A1BC') }
    }.GetNewClosure()
    & $majNote
    $case.Add_Checked($majNote); $case.Add_Unchecked($majNote)
    $ico = Join-Path $AppDir 'OptiGame.ico'
    if (Test-Path -LiteralPath $ico) {
        try { $w.Icon = [System.Windows.Media.Imaging.BitmapDecoder]::Create((New-Object IO.MemoryStream (, [IO.File]::ReadAllBytes($ico))), 'None', 'OnLoad').Frames | Sort-Object PixelWidth | Where-Object { $_.PixelWidth -ge 32 } | Select-Object -First 1 } catch {}
    }
    $etat = @{ Choix = $null }
    $w.FindName('BtnOui').Add_Click({ $etat.Choix = if ($case.IsChecked) { 'supprimer' } else { 'garder' }; $w.Close() }.GetNewClosure())
    $w.FindName('BtnNon').Add_Click({ $w.Close() }.GetNewClosure())
    $w.Add_Loaded({ [void]$w.FindName('BtnNon').Focus() }.GetNewClosure())   # Entrée par erreur = Annuler
    if ($Essai -ge 0) {
        $w.WindowStartupLocation = 'Manual'; $w.Left = -10000; $w.Top = -10000; $w.ShowActivated = $false; $w.ShowInTaskbar = $false
        $w.Add_ContentRendered({
            if ($Essai -eq 1) { $case.IsChecked = $true }
            if ($Image) {
                $w.UpdateLayout()
                $bmp = New-Object System.Windows.Media.Imaging.RenderTargetBitmap ([int]$w.ActualWidth), ([int]$w.ActualHeight), 96, 96, 'Pbgra32'
                $bmp.Render($w)
                $enc = New-Object System.Windows.Media.Imaging.PngBitmapEncoder
                $enc.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($bmp))
                $fs = [IO.File]::Create($Image); try { $enc.Save($fs) } finally { $fs.Dispose() }
            }
            $bouton = if ($Essai -eq 2) { 'BtnNon' } else { 'BtnOui' }
            $w.FindName($bouton).RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Button]::ClickEvent)))
        }.GetNewClosure())
    }
    [void]$w.ShowDialog()
    $etat.Choix
}

# Le message de fin (une fonction à part : le test de la désinstallation le lit au lieu de l'afficher)
function Show-MessageFinDesinstallation([string]$Texte) { [System.Windows.MessageBox]::Show($Texte, 'Nevermind', 'OK', 'Information') | Out-Null }

function Invoke-Uninstall {
    Import-Backup
    # (services et réglages « batterie » remis par Restore-AllSettings : comptés aussi, sinon ils restaient changés)
    $n = $script:Backup.Registry.Count + $script:Backup.Dns.Count + $script:Backup.Displays.Count + $(if ($script:Backup.PowerScheme) { 1 } else { 0 }) + $(if ($script:Backup.Overlay) { 1 } else { 0 }) +
         @(Get-Setting 'SvcOriginal' @()).Count + @(Get-Setting 'PcfgOriginal' @()).Count
    $steps = @()
    if ($n) { $steps += "  - remettre les $n réglage$(if ($n -gt 1) {'s'}) de Windows modifié$(if ($n -gt 1) {'s'}) par Nevermind comme avant" }
    if ((Test-AutoStart) -or (Test-DesktopShortcut)) { $steps += "  - retirer son raccourci du bureau et son lancement au démarrage" }
    $steps += "  - supprimer les fichiers de Nevermind de ce dossier"
    # Les changements de sécurité définitifs restent (ils protègent le PC) : ils sont dits, avec la façon de revenir en arrière.
    # Même dans une entrée annulée (« Tout restaurer ») : sa part définitive, elle, est toujours en place.
    if ($null -eq $script:History) { try { Import-History } catch {} }
    $restent = @(@($script:History) | Where-Object { $_ } | ForEach-Object { @($_.Log) } | Where-Object { $_.Type -eq 'definitif' } | ForEach-Object { "  - $($_.Quoi) (pour revenir en arrière : $($_.Retour))" } | Select-Object -Unique)
    $avertir = ''
    if ($script:BackupProbleme) { $avertir += "`n`nAttention : $(Get-TexteProblemeSauvegarde)" }
    if ($restent.Count) { $avertir += "`n`nCes changements protègent ton PC et restent en place :`n" + ($restent -join "`n") }
    $choix = Get-ChoixDonneesImpose
    if ($choix) {
        # Piloté par Mindset : pas de question (l'utilisateur a déjà choisi), ce qu'il faut savoir est dit à la fin
        Write-Log "Désinstallation demandée par Mindset (données $(if ($choix -eq 'garder') { 'gardées' } else { 'supprimées' }))"
    } else {
        $q = "Nevermind va :`n" + ($steps -join "`n") + "`n`nLes points de restauration Windows sont conservés." + $avertir
        $choix = Show-ChoixDesinstallation $q
        if (-not $choix) { return }
    }
    $garder = $choix -eq 'garder'

    $errors = @()
    # L'Aide ouverte garderait NevermindAide.exe : on la ferme (le désinstalleur, lui, a les droits et fait tout lui-même).
    # Reconnue par son chemin, jamais par son nom seul (seule l'Aide de ce dossier)
    $exeAide = Join-Path (Get-AppRoot) 'NevermindAide.exe'
    foreach ($p in @(Get-Process -ErrorAction SilentlyContinue)) { try { if ($p.Path -eq $exeAide) { $p.Kill(); [void]$p.WaitForExit(3000) } } catch {} }
    if ($n) { $errors += Restore-AllSettings }
    if ($garder) { Write-Log "Désinstallation : réglages de Windows remis comme avant$(if ($errors) { " sauf $($errors.Count)" }), données gardées" }
    $errDonnees = Remove-DonneesDesinstallation $garder ([bool]($errors -or $script:BackupProbleme))   # (déjà une liste)
    $errors += $errDonnees
    try { Set-AutoStart $false } catch { $errors += "Démarrage automatique: $($_.Exception.Message)" }
    foreach ($task in $OldAutoStartTasks) { try { if (Get-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue) { Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction Stop } } catch {} }
    # L'Aide : sa tâche planifiée (créée par Mindset) et ce qu'elle retenait (DNS d'origine)
    try { Unregister-ScheduledTask -TaskPath '\Mindset\' -TaskName 'Nevermind (aide)' -Confirm:$false -ErrorAction SilentlyContinue } catch {}
    try { Remove-Item -LiteralPath $AideMemoire -Recurse -Force -ErrorAction SilentlyContinue } catch {}
    if (Test-DesktopShortcut) { try { [IO.File]::Delete((Get-DesktopShortcutPath)) } catch {} }
    try { Remove-MenuShortcut } catch {}

    # Fichiers de l'application: uniquement ceux livrés avec Nevermind, jamais le reste du dossier.
    $here = $AppDir
    $root = if ((Split-Path $here -Leaf) -eq 'fichiers') { Split-Path $here -Parent } else { $here }
    $files = @(
        (Join-Path $root 'Mindset.exe'),
        (Join-Path $root 'Nevermind.exe'),
        (Join-Path $root 'Désinstaller Nevermind.exe'),
        (Join-Path $root 'NevermindAide.exe'),
        (Join-Path $root 'OptiGame.exe'),
        (Join-Path $root 'Désinstaller OptiGame.exe'),
        (Join-Path $root 'Nexo.exe'),
        (Join-Path $root 'Désinstaller Nexo.exe'),
        (Join-Path $root 'LISEZMOI.txt'),
        (Join-Path $here 'OptiGame.ps1'),
        (Join-Path $here 'NevermindAide.ps1'),
        (Join-Path $here 'OptiGame.ico'),
        (Join-Path $here 'cle-publique.xml'),
        (Join-Path $here 'Lancer Nevermind (secours).bat'),
        (Join-Path $here 'Désinstaller Nevermind (secours).bat'),
        (Join-Path $here 'Lancer OptiGame (secours).bat'),
        (Join-Path $here 'Lancer Nexo (secours).bat'),
        (Join-Path $here 'Désinstaller Nexo (secours).bat'),
        (Join-Path $here 'Désinstaller OptiGame (secours).bat')
    ) | Select-Object -Unique | Where-Object { Test-Path -LiteralPath $_ }

    $msg = 'Nevermind est désinstallé.'
    $msg += if (-not $garder) { "`n`nSes données sont supprimées$(if ($errDonnees.Count) { ', sauf ce qui est indiqué plus bas' })." }
            elseif ([IO.Directory]::Exists($DataDir)) { "`n`nTes données sont gardées dans $DataDir : si tu réinstalles Nevermind, tu retrouveras tes préférences, tes jeux et tes mesures." }
            else { '' }
    if ($n) { $msg += "`n`nRedémarre le PC pour que tous les réglages d'origine soient pris en compte." }
    if ($errors) { $msg += "`n`nCertains éléments n'ont pas pu être restaurés :`n" + ($errors -join "`n") }
    # Piloté par Mindset, la question n'a pas été posée : ce qu'elle disait d'important est dit ici
    if (Get-ChoixDonneesImpose) { $msg += $avertir }
    Show-MessageFinDesinstallation $msg

    # Les fichiers sont supprimés juste après la fermeture de ce script (ils sont en cours d'utilisation).
    $cmd = 'ping 127.0.0.1 -n 4 >nul'
    foreach ($f in $files) { $cmd += " & del /f /q `"$f`"" }
    $cmd += " & rmdir /s /q `"$(Join-Path $here 'modules')`""
    $cmd += " & rmdir /s /q `"$(Join-Path $here 'outils-tiers')`""
    $cmd += " & rmdir /s /q `"$(Join-Path $here 'cles-publiques')`""
    if ($here -ne $root) { $cmd += " & rmdir `"$here`"" }
    $cmd += " & rmdir `"$root`""
    Start-Process -FilePath 'cmd.exe' -ArgumentList '/c', $cmd -WindowStyle Hidden -WorkingDirectory $env:TEMP
}
