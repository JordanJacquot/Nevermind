# OptiGame : mises à jour automatiques depuis GitHub.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Mises à jour (GitHub)
# ---------------------------------------------------------------------------
# Script autonome: il tourne dans un fil séparé pour ne pas figer la fenêtre.
$GetReleaseScript = {
    param($repo)
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $r = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers @{ 'User-Agent' = 'OptiGame' } -TimeoutSec 10
        $asset = @($r.assets | Where-Object { $_.name -eq 'OptiGame.zip' }) | Select-Object -First 1
        if (-not $asset) { return @{ Error = 'Aucun fichier OptiGame.zip dans la dernière version publiée.' } }
        @{ Version = ([string]$r.tag_name -replace '^[vV]', ''); Url = [string]$asset.browser_download_url }
    } catch { @{ Error = $_.Exception.Message } }
}

function Test-NewerVersion([string]$Remote, [string]$Local) {
    try { return ([version]$Remote -gt [version]$Local) } catch { return $false }
}

function Invoke-UpdateCheck([switch]$Manual) {
    $ui.UpdateStatus.Text = 'Recherche d''une nouvelle version...'
    $r = Invoke-Async $GetReleaseScript $UpdateRepo | Select-Object -First 1
    if (-not $r -or $r.Error) {
        $ui.UpdateStatus.Text = "Version $AppVersion. Impossible de vérifier les mises à jour (pas de connexion ?)."
        if ($r.Error) { Write-Log "Mise à jour: $($r.Error)" }
        if ($Manual) { Show-Message "Impossible de vérifier les mises à jour pour le moment.`n`nVérifie ta connexion Internet et réessaie." 'Warning' }
        return
    }
    if (Test-NewerVersion $r.Version $AppVersion) {
        $script:PendingUpdate = $r
        $ui.UpdateText.Text = "Nouvelle version $($r.Version) disponible (tu as la $AppVersion)."
        $ui.UpdateBanner.Visibility = 'Visible'
        $ui.UpdateStatus.Text = "Version $AppVersion. La version $($r.Version) est disponible."
        $ui.BtnCheckUpdate.Content = 'Mettre à jour'
    } else {
        $ui.UpdateStatus.Text = "Version $AppVersion : tu as la dernière version."
        if ($Manual) { Set-Status 'OptiGame est à jour.' }
    }
}

function Install-Update {
    $rel = $script:PendingUpdate
    if (-not $rel) { return }
    if (-not (Confirm-Action "Installer la version $($rel.Version) d'OptiGame ?`n`nL'app va se fermer, se mettre à jour puis se relancer. Tes réglages et ta sauvegarde sont conservés.")) { return }
    Set-Busy $true
    $ui.UpdateBanner.Visibility = 'Collapsed'
    Set-Status "Téléchargement de la version $($rel.Version)..."
    $tmp = Join-Path $env:TEMP "OptiGame-maj-$(Get-Random)"
    $res = Invoke-Async {
        param($a)
        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            New-Item -ItemType Directory -Force -Path $a.Dir | Out-Null
            $zip = Join-Path $a.Dir 'OptiGame.zip'
            Invoke-WebRequest -Uri $a.Url -OutFile $zip -UseBasicParsing -Headers @{ 'User-Agent' = 'OptiGame' } -TimeoutSec 120
            Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $a.Dir 'x') -Force
            'OK'
        } catch { $_.Exception.Message }
    } @{ Url = $rel.Url; Dir = $tmp } | Select-Object -First 1
    $src = Join-Path $tmp 'x\OptiGame'
    if ("$res" -ne 'OK' -or -not (Test-Path -LiteralPath (Join-Path $src 'fichiers\OptiGame.ps1'))) {
        Write-Log "Échec de la mise à jour: $res"
        Show-Message "La mise à jour n'a pas pu être téléchargée.`n`n$res`n`nRéessaie plus tard." 'Warning'
        Set-Status 'Mise à jour annulée.'
        return
    }
    Set-Status 'Installation de la mise à jour...'
    $errors = @()
    foreach ($f in Get-ChildItem -LiteralPath $src -Recurse -File) {
        $dest = Join-Path $appRoot $f.FullName.Substring($src.Length + 1)
        if ((Test-Path -LiteralPath $dest) -and (Get-FileHash -LiteralPath $dest).Hash -eq (Get-FileHash -LiteralPath $f.FullName).Hash) { continue }
        try {
            New-Item -ItemType Directory -Force -Path (Split-Path $dest -Parent) | Out-Null
            Copy-Item -LiteralPath $f.FullName -Destination $dest -Force -ErrorAction Stop
            Unblock-File -LiteralPath $dest -ErrorAction SilentlyContinue
        } catch { $errors += "$($f.Name): $($_.Exception.Message)" }
    }
    [IO.Directory]::Delete($tmp, $true)
    if ($errors) {
        Write-Log "Mise à jour incomplète: $($errors -join ' | ')"
        Show-Message "La mise à jour n'a pas pu remplacer tous les fichiers :`n`n$($errors -join "`n")" 'Warning'
        return
    }
    Write-Log "Mise à jour installée: $AppVersion -> $($rel.Version)"
    $script:Relaunch = Join-Path $appRoot 'fichiers\OptiGame.ps1'
    $Window.Close()
}
