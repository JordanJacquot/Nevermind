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
$CleanTargets = @(
    @{ Titre = 'Fichiers temporaires (utilisateur)'; Paths = @($env:TEMP) },
    @{ Titre = 'Fichiers temporaires (Windows)'; Paths = @("$env:windir\Temp") },
    @{ Titre = 'Fichiers de mises à jour Windows déjà installées'; Paths = @("$env:windir\SoftwareDistribution\Download") },
    @{ Titre = "Cache d'optimisation de la distribution"; Paths = @("$env:windir\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache") },
    @{ Titre = "Rapports d'erreurs Windows"; Paths = @("$env:ProgramData\Microsoft\Windows\WER\ReportArchive", "$env:ProgramData\Microsoft\Windows\WER\ReportQueue") },
    @{ Titre = 'Anciens rapports de plantage (minidumps)'; Paths = @("$env:windir\Minidump") }
)

# Parcours d'un dossier sans suivre les liens (jonctions, liens symboliques) : on ne sort jamais du dossier prévu.
$CleanWalk = @'
function Get-CleanFiles([string]$Root, $Dirs) {
    foreach ($e in @(Get-ChildItem -LiteralPath $Root -Force -ErrorAction SilentlyContinue)) {
        if ($e.PSIsContainer) {
            if ($e.Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
            if ($null -ne $Dirs) { [void]$Dirs.Add($e.FullName) }
            Get-CleanFiles $e.FullName $Dirs
        } elseif ($e.LastWriteTime -lt $script:CleanBefore) { $e }
    }
}
# Fichiers de moins de 24 h laissés : une installation ou un programme en cours peut encore s'en servir
$script:CleanBefore = (Get-Date).AddDays(-1)
'@

# Analyse d'une catégorie : nombre de fichiers, taille totale et les plus gros (pour « Voir les fichiers »)
$CleanListScript = [scriptblock]::Create($CleanWalk + @'

$a = $args[0]
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

$paths = $args[0]
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
        $r = Invoke-Async $CleanScript $t.Paths | Select-Object -First 1
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
    try { Get-NetFirewallRule -DisplayName 'OptiGame : bloque *' -ErrorAction SilentlyContinue | Remove-NetFirewallRule -ErrorAction Stop } catch { $errors += "Pare-feu: $($_.Exception.Message)" }
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
        try {
            Set-Service -Name $x[0] -StartupType $x[1] -ErrorAction Stop
            if ($x[2] -eq 'True') { Start-Service -Name $x[0] -ErrorAction Stop }
        } catch { $errors += "Service $($x[0]): $($_.Exception.Message)" }
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
            if ($v) { Set-DnsClientServerAddress -InterfaceIndex ([int]$k) -ServerAddresses @($v -split '[,\s]+' | Where-Object { $_ }) -ErrorAction Stop }
            else { Set-DnsClientServerAddress -InterfaceIndex ([int]$k) -ResetServerAddresses -ErrorAction Stop }
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
    $act = New-ScheduledTaskAction -Execute "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -Argument "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$ps1`" -Demarrage" -WorkingDirectory $env:USERPROFILE
    $trg = New-ScheduledTaskTrigger -AtLogOn -User $user
    $trg.Delay = 'PT15S'   # laisse Windows finir d'ouvrir la session
    $pr = New-ScheduledTaskPrincipal -UserId $user -LogonType Interactive -RunLevel Highest
    $set = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskName $AutoStartTask -Action $act -Trigger $trg -Principal $pr -Settings $set -Description 'Lance Nevermind à l''ouverture de session, réduit près de l''horloge. Se règle dans Nevermind, page Sauvegarde.' -Force -ErrorAction Stop | Out-Null
}

# Changement de nom (OptiGame, puis Nexo, puis Nevermind) : les anciennes tâches de démarrage, les anciens
# raccourcis du bureau et les anciens lanceurs sont remplacés. Seulement ce qui appartient à cette installation.
function Invoke-NameMigration {
    $root = Get-AppRoot
    $newExe = Get-AppExe
    if (-not (Test-Path -LiteralPath $newExe)) { return }   # pas encore de Nevermind.exe (copie de développement)
    foreach ($task in $OldAutoStartTasks) {
        try {
            if (Get-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue) {
                Set-AutoStart $true
                Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction Stop
                Write-Log "Nevermind : tâche de démarrage « $task » renommée"
            }
        } catch { Write-Log "Nevermind, tâche de démarrage : $_" }
    }
    foreach ($old in $OldAppNames) {
        try {
            $oldLnk = Get-DesktopShortcutPath $old
            if ((Get-ShortcutTarget $oldLnk) -eq (Join-Path $root "$old.exe")) {
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
    $t = Get-AutoStartTask
    if (-not $t) { return }
    $ps1 = Join-Path $AppDir 'OptiGame.ps1'
    if ([string]@($t.Actions)[0].Arguments -notlike "*`"$ps1`"*") {
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
    if ($n) { $errors += Restore-AllSettings }
    try { Remove-Item -LiteralPath $DataDir -Recurse -Force -ErrorAction Stop } catch { $errors += "Données: $($_.Exception.Message)" }
    try { Set-AutoStart $false } catch { $errors += "Démarrage automatique: $($_.Exception.Message)" }
    foreach ($task in $OldAutoStartTasks) { try { if (Get-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue) { Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction Stop } } catch {} }
    if (Test-DesktopShortcut) { try { [IO.File]::Delete((Get-DesktopShortcutPath)) } catch {} }

    # Fichiers de l'application: uniquement ceux livrés avec Nevermind, jamais le reste du dossier.
    $here = $AppDir
    $root = if ((Split-Path $here -Leaf) -eq 'fichiers') { Split-Path $here -Parent } else { $here }
    $files = @(
        (Join-Path $root 'Nevermind.exe'),
        (Join-Path $root 'Désinstaller Nevermind.exe'),
        (Join-Path $root 'OptiGame.exe'),
        (Join-Path $root 'Désinstaller OptiGame.exe'),
        (Join-Path $root 'Nexo.exe'),
        (Join-Path $root 'Désinstaller Nexo.exe'),
        (Join-Path $root 'LISEZMOI.txt'),
        (Join-Path $here 'OptiGame.ps1'),
        (Join-Path $here 'OptiGame.ico'),
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
