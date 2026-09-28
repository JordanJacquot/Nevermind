# OptiGame : connexion active, nettoyage, restauration et désinstallation.
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

$SizeScript = {
    param($paths)
    $sum = 0
    foreach ($p in $paths) {
        if (Test-Path -LiteralPath $p) {
            $m = Get-ChildItem -LiteralPath $p -Recurse -Force -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum
            if ($m.Sum) { $sum += $m.Sum }
        }
    }
    [double]$sum
}

$CleanScript = {
    param($paths)
    foreach ($p in $paths) {
        if (Test-Path -LiteralPath $p) {
            Get-ChildItem -LiteralPath $p -Force -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Format-Size([double]$Bytes) {
    if ($Bytes -ge 1TB) { return '{0:N1} To' -f ($Bytes / 1TB) }
    if ($Bytes -ge 1GB) { return '{0:N1} Go' -f ($Bytes / 1GB) }
    if ($Bytes -ge 1MB) { return '{0:N0} Mo' -f ($Bytes / 1MB) }
    '{0:N0} Ko' -f ($Bytes / 1KB)
}

# ---------------------------------------------------------------------------
# Restauration de tous les réglages modifiés par OptiGame
# ---------------------------------------------------------------------------
function Restore-AllSettings {
    $errors = @()
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
# Désinstallation
# ---------------------------------------------------------------------------
function Invoke-Uninstall {
    Import-Backup
    $n = $script:Backup.Registry.Count + $script:Backup.Dns.Count + $script:Backup.Displays.Count + $(if ($script:Backup.PowerScheme) { 1 } else { 0 }) + $(if ($script:Backup.Overlay) { 1 } else { 0 })
    $steps = @()
    if ($n) { $steps += "  - remettre les $n réglage$(if ($n -gt 1) {'s'}) de Windows modifié$(if ($n -gt 1) {'s'}) par OptiGame comme avant" }
    $steps += "  - supprimer ses données (sauvegarde, journal, préférences)"
    $steps += "  - supprimer les fichiers d'OptiGame de ce dossier"
    $q = "Désinstaller OptiGame ?`n`nL'application va :`n" + ($steps -join "`n") + "`n`nLes points de restauration Windows sont conservés."
    if ([System.Windows.MessageBox]::Show($q, 'Désinstaller OptiGame', 'YesNo', 'Question') -ne 'Yes') { return }

    $errors = @()
    if ($n) { $errors += Restore-AllSettings }
    try { Remove-Item -LiteralPath $DataDir -Recurse -Force -ErrorAction Stop } catch { $errors += "Données: $($_.Exception.Message)" }

    # Fichiers de l'application: uniquement ceux livrés avec OptiGame, jamais le reste du dossier.
    $here = $AppDir
    $root = if ((Split-Path $here -Leaf) -eq 'fichiers') { Split-Path $here -Parent } else { $here }
    $files = @(
        (Join-Path $root 'OptiGame.exe'),
        (Join-Path $root 'Désinstaller OptiGame.exe'),
        (Join-Path $root 'LISEZMOI.txt'),
        (Join-Path $here 'OptiGame.ps1'),
        (Join-Path $here 'OptiGame.ico'),
        (Join-Path $here 'Lancer OptiGame (secours).bat'),
        (Join-Path $here 'Désinstaller OptiGame (secours).bat')
    ) | Select-Object -Unique | Where-Object { Test-Path -LiteralPath $_ }

    $msg = 'OptiGame est désinstallé.'
    if ($n) { $msg += "`n`nRedémarre le PC pour que tous les réglages d'origine soient pris en compte." }
    if ($errors) { $msg += "`n`nCertains éléments n'ont pas pu être restaurés :`n" + ($errors -join "`n") }
    [System.Windows.MessageBox]::Show($msg, 'OptiGame', 'OK', 'Information') | Out-Null

    # Les fichiers sont supprimés juste après la fermeture de ce script (ils sont en cours d'utilisation).
    $cmd = 'ping 127.0.0.1 -n 4 >nul'
    foreach ($f in $files) { $cmd += " & del /f /q `"$f`"" }
    $cmd += " & rmdir /s /q `"$(Join-Path $here 'modules')`""
    if ($here -ne $root) { $cmd += " & rmdir `"$here`"" }
    $cmd += " & rmdir `"$root`""
    Start-Process -FilePath 'cmd.exe' -ArgumentList '/c', $cmd -WindowStyle Hidden -WorkingDirectory $env:TEMP
}
