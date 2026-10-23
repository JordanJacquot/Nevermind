# Nevermind : mises à jour automatiques depuis GitHub.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Mises à jour (GitHub)
# ---------------------------------------------------------------------------
# Script autonome: il tourne dans un fil séparé pour ne pas figer la fenêtre.
# Avec Beta, la version la plus récente est prise même si elle est marquée « préversion » sur GitHub.
$GetReleaseScript = {
    param($a)
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $h = @{ 'User-Agent' = 'Nevermind' }
        if ($a.Beta) {
            $list = Invoke-RestMethod -Uri "https://api.github.com/repos/$($a.Repo)/releases?per_page=10" -Headers $h -TimeoutSec 10
            $r = @($list) | Where-Object { -not $_.draft } | Select-Object -First 1
        } else {
            $r = Invoke-RestMethod -Uri "https://api.github.com/repos/$($a.Repo)/releases/latest" -Headers $h -TimeoutSec 10
        }
        if (-not $r) { return @{ Error = 'Aucune version publiée.' } }
        $asset = @($r.assets | Where-Object { $_.name -eq 'OptiGame.zip' }) | Select-Object -First 1
        if (-not $asset) { return @{ Error = 'Aucun fichier OptiGame.zip dans la dernière version publiée.' } }
        $sig = @($r.assets | Where-Object { $_.name -eq 'OptiGame.zip.sig' }) | Select-Object -First 1
        @{ Version = ([string]$r.tag_name -replace '^[vV]', ''); Url = [string]$asset.browser_download_url; SigUrl = [string]$sig.browser_download_url; Beta = [bool]$r.prerelease; Notes = [string]$r.body }
    } catch { @{ Error = $_.Exception.Message } }
}

# Notes d'une version précise (pour « Quoi de neuf » après une mise à jour).
$GetNotesScript = {
    param($a)
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        [string](Invoke-RestMethod -Uri "https://api.github.com/repos/$($a.Repo)/releases/tags/v$($a.Version)" -Headers @{ 'User-Agent' = 'Nevermind' } -TimeoutSec 10).body
    } catch { '' }
}

function Test-NewerVersion([string]$Remote, [string]$Local) {
    try { return ([version]$Remote -gt [version]$Local) } catch { return $false }
}

# Signature d'un zip publié (OptiGame.zip.sig : RSA, SHA-256), vérifiée avec les clés publiques livrées avec
# Nevermind (fichiers\cles-publiques\*.xml, les mêmes que Mindset). Un zip sans signature valide n'est jamais installé,
# même s'il vient bien de GitHub : quelqu'un qui volerait l'accès au compte GitHub ne pourrait pas faire installer un faux.
# Une liste de clés (clé de tous les jours, clé de secours, clé suivante pendant un changement) : la signature est bonne
# si UNE des clés la vérifie. Liste vide, fichier abîmé, clé privée ou trop courte (moins de 3072 bits) : tout est refusé.
# Même fonction que Test-Signature dans Mindset (signature.ps1).
$PublicKeyFile = Join-Path $AppDir 'cles-publiques'
function Test-Signature([byte[]]$Donnees, [byte[]]$Signature, [string]$ClePublique) {
    if (-not $Donnees -or -not $Signature -or -not $ClePublique) { return $false }
    if (Test-Path -LiteralPath $ClePublique -PathType Container) {
        $fichiers = @(Get-ChildItem -LiteralPath $ClePublique -File -Force -ErrorAction SilentlyContinue | Where-Object { $_.Extension -eq '.xml' } | ForEach-Object { $_.FullName })
    } elseif (Test-Path -LiteralPath $ClePublique -PathType Leaf) { $fichiers = @($ClePublique) }
    else { return $false }
    if (-not $fichiers.Count) { return $false }
    $cles = @()
    try {
        foreach ($f in $fichiers) {
            $rsa = New-Object System.Security.Cryptography.RSACryptoServiceProvider
            $cles += $rsa
            $rsa.FromXmlString([IO.File]::ReadAllText($f, [Text.Encoding]::UTF8))
            if ($rsa.KeySize -lt 3072 -or -not $rsa.PublicOnly) { return $false }
        }
        foreach ($rsa in $cles) {
            if ($rsa.VerifyData($Donnees, [System.Security.Cryptography.CryptoConfig]::MapNameToOID('SHA256'), $Signature)) { return $true }
        }
        $false
    } catch { $false }
    finally { foreach ($rsa in $cles) { $rsa.Dispose() } }
}

function Invoke-UpdateCheck([switch]$Manual) {
    # Géré par Mindset : c'est lui qui installe les mises à jour de Nevermind, pas de vérification ici
    if ($Managed) { $ui.UpdateStatus.Text = "Version $AppVersion. Les mises à jour de Nevermind passent par Mindset : ouvre Mindset pour les installer."; return }
    # Recherche au lancement coupée (Paramètres, Mises à jour) : seulement quand on clique sur « Vérifier les mises à jour »
    if (-not $Manual -and -not (Test-MajAuto)) { $ui.UpdateStatus.Text = "Version $AppVersion. Clique sur « Vérifier les mises à jour » pour chercher une nouvelle version."; return }
    $ui.UpdateStatus.Text = 'Recherche d''une nouvelle version...'
    $r = Invoke-Async $GetReleaseScript @{ Repo = $UpdateRepo; Beta = [bool](Get-Setting 'Beta' $false) } | Select-Object -First 1
    if (-not $r -or $r.Error) {
        $ui.UpdateStatus.Text = "Version $AppVersion. Impossible de vérifier les mises à jour (pas de connexion ?)."
        if ($r.Error) { Write-Log "Mise à jour: $($r.Error)" }
        if ($Manual) { Show-Message "Impossible de vérifier les mises à jour pour le moment.`n`nVérifie ta connexion Internet et réessaie." 'Warning' }
        return
    }
    if (Test-NewerVersion $r.Version $AppVersion) {
        $script:PendingUpdate = $r
        $ui.UpdateText.Text = "Nouvelle version $(if ($r.Beta) { 'bêta ' })$($r.Version) disponible (tu as la $AppVersion)."
        $ui.UpdateBanner.Visibility = 'Visible'
        $ui.UpdateStatus.Text = "Version $AppVersion. La version $($r.Version) est disponible."
        $ui.BtnCheckUpdate.Content = 'Mettre à jour'
    } else {
        $ui.UpdateStatus.Text = "Version $AppVersion : tu as la dernière version."
        if ($Manual) { Set-Status 'Nevermind est à jour.' }
    }
}

function Install-Update {
    $rel = $script:PendingUpdate
    if (-not $rel -or $Managed) { return }   # copie gérée : Mindset s'en occupe
    if (-not (Confirm-Action "Installer la version $($rel.Version) de Nevermind ?`n`nL'app va se fermer, se mettre à jour puis se relancer. Tes réglages et ta sauvegarde sont conservés.")) { return }
    $refused = "La version $($rel.Version) n'a pas été installée : sa signature numérique est {0}.`n`nPar sécurité, Nevermind n'installe que les versions signées par son auteur. Rien n'a été modifié sur ton PC.`n`nRéessaie plus tard, ou installe Mindset pour recevoir les mises à jour."
    if (-not $rel.SigUrl) {
        Write-Log "Mise à jour $($rel.Version) refusée : pas de signature (OptiGame.zip.sig) dans la version publiée"
        Show-Message ($refused -f 'absente') 'Warning'
        return
    }
    Set-Busy $true
    $ui.UpdateBanner.Visibility = 'Collapsed'
    Set-Status "Téléchargement de la version $($rel.Version)..."
    $tmp = Join-Path $env:TEMP "Nevermind-maj-$(Get-Random)"
    $zip = Join-Path $tmp 'OptiGame.zip'
    # Le zip et sa signature d'abord : rien n'est extrait avant que la signature soit vérifiée
    $res = Invoke-Async {
        param($a)
        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            New-Item -ItemType Directory -Force -Path $a.Dir | Out-Null
            Invoke-WebRequest -Uri $a.Url -OutFile $a.Zip -UseBasicParsing -Headers @{ 'User-Agent' = 'Nevermind' } -TimeoutSec 120
            Invoke-WebRequest -Uri $a.SigUrl -OutFile "$($a.Zip).sig" -UseBasicParsing -Headers @{ 'User-Agent' = 'Nevermind' } -TimeoutSec 30
            'OK'
        } catch { $_.Exception.Message }
    } @{ Url = $rel.Url; SigUrl = $rel.SigUrl; Dir = $tmp; Zip = $zip } | Select-Object -First 1
    if ("$res" -eq 'OK') {
        $signed = $false
        try { $signed = Test-Signature ([IO.File]::ReadAllBytes($zip)) ([IO.File]::ReadAllBytes("$zip.sig")) $PublicKeyFile } catch {}
        if (-not $signed) {
            Write-Log "Mise à jour $($rel.Version) refusée : signature invalide"
            try { [IO.Directory]::Delete($tmp, $true) } catch {}
            Show-Message ($refused -f 'fausse ou illisible') 'Warning'
            Set-Status 'Mise à jour annulée.'
            return
        }
        $res = Invoke-Async {
            param($a)
            try { Expand-Archive -LiteralPath $a.Zip -DestinationPath (Join-Path $a.Dir 'x') -Force; 'OK' } catch { $_.Exception.Message }
        } @{ Dir = $tmp; Zip = $zip } | Select-Object -First 1
    }
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
            # Nevermind.exe est en cours d'exécution (c'est lui qui fait tourner l'app) : Windows interdit de l'écraser,
            # mais autorise de le renommer. L'ancien est mis de côté (.old), effacé au prochain lancement.
            if ($dest -like '*.exe' -and (Test-Path -LiteralPath $dest)) {
                $old = "$dest.old"
                try { if (Test-Path -LiteralPath $old) { [IO.File]::Delete($old) } } catch {}
                try { [IO.File]::Move($dest, $old) } catch {}
            }
            Copy-Item -LiteralPath $f.FullName -Destination $dest -Force -ErrorAction Stop
            Unblock-File -LiteralPath $dest -ErrorAction SilentlyContinue
        } catch { $errors += "$($f.Name): $($_.Exception.Message)" }
    }
    # Fichiers de l'ancienne version qui n'existent plus dans la nouvelle (seulement dans « fichiers »).
    $newFiles = @{}
    foreach ($f in Get-ChildItem -LiteralPath (Join-Path $src 'fichiers') -Recurse -File) { $newFiles[$f.FullName.Substring($src.Length + 1).ToLower()] = $true }
    foreach ($f in @(Get-ChildItem -LiteralPath (Join-Path $appRoot 'fichiers') -Recurse -File -ErrorAction SilentlyContinue)) {
        $rel2 = $f.FullName.Substring($appRoot.Length + 1).ToLower()
        if (-not $newFiles.ContainsKey($rel2)) {
            try { [IO.File]::Delete($f.FullName); Write-Log "Fichier obsolète supprimé: $rel2" } catch {}
        }
    }
    [IO.Directory]::Delete($tmp, $true)
    if ($errors) {
        Write-Log "Mise à jour incomplète: $($errors -join ' | ')"
        Show-Message "La mise à jour n'a pas pu remplacer tous les fichiers :`n`n$($errors -join "`n")" 'Warning'
        return
    }
    Write-Log "Mise à jour installée: $AppVersion -> $($rel.Version)"
    $script:Relaunch = Join-Path $appRoot 'fichiers\OptiGame.ps1'
    $script:AllowClose = $true
    $Window.Close()
}

# Après une mise à jour : affiche les nouveautés de la version installée.
function Show-WhatsNew([string]$Last) {
    Set-Setting 'LastVersion' $AppVersion
    if (-not $Last -or $Last -eq $AppVersion) { return }
    # Notes lues sur GitHub, sauf si c'est coupé (Paramètres, Mises à jour)
    $enLigne = Test-NouveautesEnLigne
    $notes = if ($enLigne) { [string](Invoke-Async $GetNotesScript @{ Repo = $UpdateRepo; Version = $AppVersion } | Select-Object -First 1) } else { '' }
    $lines = @($notes -split "`r?`n" | ForEach-Object { $_.Trim() -replace '^[-*]\s+', '•  ' } | Where-Object { $_ })
    if (-not $enLigne) { $lines = @('Le détail des nouveautés n''est pas lu sur Internet : c''est coupé dans Paramètres, Mises à jour.') }
    elseif (-not $lines.Count) { $lines = @('Corrections et améliorations.') }
    Show-ResultSheet "Quoi de neuf dans la version $AppVersion" $lines $null $(if ($Last -eq 'précédente') { 'Nevermind vient d''être mis à jour.' } else { "Tu avais la version $Last." })
}

function Set-BetaChannel([bool]$On) {
    Set-Setting 'Beta' $On
    Set-Status $(if ($On) { 'Versions bêta activées.' } else { 'Versions bêta désactivées.' })
    Invoke-UpdateCheck -Manual
}

# ---------------------------------------------------------------------------
# Mindset, le portail de la suite
# ---------------------------------------------------------------------------
# Copie gérée : pas de bouton de mise à jour ni de versions bêta (Mindset installe les versions), pas de raccourci
# sur le bureau (Mindset le crée). Le lancement au démarrage passe par Mindset quand il est administrateur (sa tâche
# « \Mindset\Mindset (démarrage) » lance Nevermind selon l'interrupteur « S'ouvre avec Mindset ») ; sinon il reste ici :
# seule la tâche planifiée de Nevermind peut l'ouvrir au démarrage sans demande de Windows.
# (Une copie non gérée ne s'ouvre pas, sauf un dossier de travail : voir le début d'OptiGame.ps1.)
function Test-DemarrageParMindset {
    if (-not $Managed) { return $false }
    if ($null -eq $script:DemarrageParMindset) {
        & "$env:SystemRoot\System32\schtasks.exe" /Query /TN '\Mindset\Mindset (démarrage)' 2>$null | Out-Null
        $script:DemarrageParMindset = ($LASTEXITCODE -eq 0)
    }
    $script:DemarrageParMindset
}

function Initialize-Suite {
    if (-not $Managed) { return }
    $ui.BtnCheckUpdate.Visibility = 'Collapsed'
    $ui.ChkBeta.Parent.Visibility = 'Collapsed'
    $ui.UpdateStatus.Text = "Version $AppVersion. Les mises à jour de Nevermind passent par Mindset : ouvre Mindset pour les installer."
    if (Test-DemarrageParMindset) { $ui.AutoStartRow.Visibility = 'Collapsed'; Update-ShortcutCard }
    else { $ui.BtnShortcut.Visibility = 'Collapsed' }
}
