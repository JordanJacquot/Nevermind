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
       Desc = 'Captures, scripts et téléchargements des discussions Claude Code. Tes discussions ne sont pas effacées, mais une discussion en cours peut encore en avoir besoin : garde les si tu travailles avec Claude.' },
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
function Restore-AllSettings {
    $errors = @()
    try { if (@(Get-NetFirewallRule -DisplayName 'OptiGame : bloque *' -ErrorAction SilentlyContinue).Count) { [void](Invoke-Aide 'pare-feu.debloquer-tout') } } catch { $errors += "Pare-feu: $($_.Exception.Message)" }
    foreach ($e in @($script:Backup.Registry.Values)) {
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
        } catch { $errors += "$($e.Path)\$($e.Name): $($_.Exception.Message)" }
    }
    # Services arrêtés par Nevermind (télémétrie) : état d'origine
    foreach ($l in @(Get-Setting 'SvcOriginal' @())) {
        $x = ([string]$l) -split '\|'
        try { [void](Invoke-Aide 'service.regler' @{ Nom = $x[0]; Demarrage = $x[1]; Lancer = ($x[2] -eq 'True') }) }
        catch { $errors += "Service $($x[0]): $($_.Exception.Message)" }
    }
    Set-Setting 'SvcOriginal' @()
    # Réglages « batterie » changés pour l'onduleur
    foreach ($l in @(Get-Setting 'PcfgOriginal' @())) {
        $x = ([string]$l) -split '\|'
        try {
            if ($x[1] -ne '') { powercfg /setacvalueindex SCHEME_CURRENT e73a048d-bf27-4f12-9731-8b2076e8891f $x[0] ([int]$x[1]) | Out-Null }
            if ($x[2] -ne '') { powercfg /setdcvalueindex SCHEME_CURRENT e73a048d-bf27-4f12-9731-8b2076e8891f $x[0] ([int]$x[2]) | Out-Null }
        } catch { $errors += "Onduleur: $($_.Exception.Message)" }
    }
    if (@(Get-Setting 'PcfgOriginal' @()).Count) { powercfg /setactive SCHEME_CURRENT | Out-Null; Set-Setting 'PcfgOriginal' @() }
    if ($script:Backup.PowerScheme) { powercfg /setactive $script:Backup.PowerScheme | Out-Null }
    if ($script:Backup.Overlay) { try { [void][OGNative]::SetOverlay($script:Backup.Overlay) } catch { $errors += "Mode d'alimentation: $($_.Exception.Message)" } }
    if ($script:Backup.CreatedScheme -and $script:Backup.CreatedScheme -ne $script:Backup.PowerScheme) {
        powercfg /delete $script:Backup.CreatedScheme 2>$null | Out-Null
    }
    foreach ($k in @($script:Backup.Dns.Keys)) {
        try {
            $v = $script:Backup.Dns[$k]
            [void](Invoke-Aide 'dns.regler' @{ Index = [int]$k; Serveurs = @(([string]$v) -split '[,\s]+' | Where-Object { $_ }) })
        } catch { $errors += "DNS: $($_.Exception.Message)" }
    }
    foreach ($k in @($script:Backup.Displays.Keys)) {
        $r = [OGNative]::SetRefreshRate($k, [int]$script:Backup.Displays[$k])
        if ($r -ne 0) { $errors += "Écran $($k): fréquence non restaurée (code $r)" }
    }
    Sync-Mouse
    Remove-Item $BackupFile -ErrorAction SilentlyContinue
    Import-Backup
    , $errors
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
function Get-AppExe { Join-Path (Get-AppRoot) 'Nevermind.exe' }
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

function Test-DesktopShortcut { (Get-ShortcutTarget (Get-DesktopShortcutPath)) -eq (Get-AppExe) }

function New-DesktopShortcut {
    $root = Get-AppRoot
    $exe = Get-AppExe
    if (-not (Test-Path -LiteralPath $exe)) { throw "Nevermind.exe est introuvable dans le dossier $root." }
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
    $exe = Join-Path (Split-Path $AppDir -Parent) 'Nevermind.exe'
    $act = if (Test-Path -LiteralPath $exe) { New-ScheduledTaskAction -Execute "`"$exe`"" -Argument '-Demarrage' -WorkingDirectory $env:USERPROFILE }
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
    $root = Get-AppRoot
    $newExe = Get-AppExe
    if (-not (Test-Path -LiteralPath $newExe)) { return }   # pas encore de Nevermind.exe (copie de développement)
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
    if (Test-DemarrageParMindset) { return }   # (Mindset s'en occupe : sa tâche a été reprise, on ne la recrée pas)
    $t = @(Get-TaskInfo @($AutoStartTask))[0]
    if (-not $t) { return }
    $ps1 = Join-Path $AppDir 'OptiGame.ps1'
    $exe = Join-Path (Split-Path $AppDir -Parent) 'Nevermind.exe'
    # La tâche doit lancer Nevermind.exe de ce dossier (les anciennes lançaient PowerShell avec le script)
    $ok = if (Test-Path -LiteralPath $exe) { $t.Exe -eq $exe -and $t.Args -eq '-Demarrage' } else { $t.Args -like "*`"$ps1`"*" }
    if (-not $ok) {
        try { Set-AutoStart $true; Write-Log "Démarrage automatique : chemin mis à jour ($ps1)" } catch { Write-Log "Démarrage automatique : $_" }
    }
}

# ---------------------------------------------------------------------------
# Désinstallation
# ---------------------------------------------------------------------------
function Invoke-Uninstall {
    Import-Backup
    $n = $script:Backup.Registry.Count + $script:Backup.Dns.Count + $script:Backup.Displays.Count + $(if ($script:Backup.PowerScheme) { 1 } else { 0 }) + $(if ($script:Backup.Overlay) { 1 } else { 0 })
    $steps = @()
    if ($n) { $steps += "  - remettre les $n réglage$(if ($n -gt 1) {'s'}) de Windows modifié$(if ($n -gt 1) {'s'}) par Nevermind comme avant" }
    $steps += "  - supprimer ses données (sauvegarde, journal, préférences)"
    if ((Test-AutoStart) -or (Test-DesktopShortcut)) { $steps += "  - retirer son raccourci du bureau et son lancement au démarrage" }
    $steps += "  - supprimer les fichiers de Nevermind de ce dossier"
    $q = "Désinstaller Nevermind ?`n`nL'application va :`n" + ($steps -join "`n") + "`n`nLes points de restauration Windows sont conservés."
    if ([System.Windows.MessageBox]::Show($q, 'Désinstaller Nevermind', 'YesNo', 'Question') -ne 'Yes') { return }

    $errors = @()
    # L'Aide ouverte garderait Nevermind.exe : on la ferme (le désinstalleur, lui, a les droits et fait tout lui-même)
    $exeAide = Join-Path (Get-AppRoot) 'NevermindAide.exe'
    foreach ($p in @(Get-Process -Name 'NevermindAide' -ErrorAction SilentlyContinue)) { try { if ($p.Path -eq $exeAide) { $p.Kill(); [void]$p.WaitForExit(3000) } } catch {} }
    if ($n) { $errors += Restore-AllSettings }
    try { Remove-Item -LiteralPath $DataDir -Recurse -Force -ErrorAction Stop } catch { $errors += "Données: $($_.Exception.Message)" }
    try { Set-AutoStart $false } catch { $errors += "Démarrage automatique: $($_.Exception.Message)" }
    foreach ($task in $OldAutoStartTasks) { try { if (Get-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue) { Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction Stop } } catch {} }
    # L'Aide : sa tâche planifiée (créée par Mindset) et ce qu'elle retenait (DNS d'origine)
    try { Unregister-ScheduledTask -TaskPath '\Mindset\' -TaskName 'Nevermind (aide)' -Confirm:$false -ErrorAction SilentlyContinue } catch {}
    try { Remove-Item -LiteralPath $AideMemoire -Recurse -Force -ErrorAction SilentlyContinue } catch {}
    if (Test-DesktopShortcut) { try { [IO.File]::Delete((Get-DesktopShortcutPath)) } catch {} }

    # Fichiers de l'application: uniquement ceux livrés avec Nevermind, jamais le reste du dossier.
    $here = $AppDir
    $root = if ((Split-Path $here -Leaf) -eq 'fichiers') { Split-Path $here -Parent } else { $here }
    $files = @(
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
    if ($n) { $msg += "`n`nRedémarre le PC pour que tous les réglages d'origine soient pris en compte." }
    if ($errors) { $msg += "`n`nCertains éléments n'ont pas pu être restaurés :`n" + ($errors -join "`n") }
    [System.Windows.MessageBox]::Show($msg, 'Nevermind', 'OK', 'Information') | Out-Null

    # Les fichiers sont supprimés juste après la fermeture de ce script (ils sont en cours d'utilisation).
    $cmd = 'ping 127.0.0.1 -n 4 >nul'
    foreach ($f in $files) { $cmd += " & del /f /q `"$f`"" }
    $cmd += " & rmdir /s /q `"$(Join-Path $here 'modules')`""
    $cmd += " & rmdir /s /q `"$(Join-Path $here 'outils-tiers')`""
    if ($here -ne $root) { $cmd += " & rmdir `"$here`"" }
    $cmd += " & rmdir `"$root`""
    Start-Process -FilePath 'cmd.exe' -ArgumentList '/c', $cmd -WindowStyle Hidden -WorkingDirectory $env:TEMP
}
