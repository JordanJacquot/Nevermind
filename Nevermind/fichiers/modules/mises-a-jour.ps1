# Nevermind : mises à jour automatiques depuis GitHub.
# Chargé par Nevermind.ps1, qui définit $AppDir et $ModulesDir.

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
        # Nevermind.zip ; OptiGame.zip (son ancien nom) dans les versions publiées avant le changement de nom. Une copie
        # dont le script porte l'ancien nom (installée avec le zip de compatibilité) prend le zip de compatibilité : elle
        # garde ses noms, ceux que cherchent les Mindset 2.12.0 et plus anciens (Nevermind.zip, ils ne le reconnaîtraient plus)
        $noms = if ($a.Ancien) { 'OptiGame.zip', 'Nevermind.zip' } else { 'Nevermind.zip', 'OptiGame.zip' }
        $asset = @(foreach ($n in $noms) { $r.assets | Where-Object { $_.name -eq $n } }) | Select-Object -First 1
        if (-not $asset) { return @{ Error = 'Aucun fichier Nevermind.zip dans la dernière version publiée.' } }
        $sig = @($r.assets | Where-Object { $_.name -eq "$($asset.name).sig" }) | Select-Object -First 1
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

# Signature d'un zip publié (Nevermind.zip.sig : RSA, SHA-256), vérifiée avec les clés publiques livrées avec
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
            $xml = [IO.File]::ReadAllText($f, [Text.Encoding]::UTF8)
            # (Modulus et Exponent sont obligatoires : sans eux, FromXmlString échoue)
            $rsa.FromXmlString($xml)
            if ($rsa.KeySize -lt 3072 -or -not $rsa.PublicOnly) { return $false }
            # Aucun élément de clé privée, même vide ou incomplet (.NET ne regarde que P : une clé avec D seul passait
            # pour publique). Lu par le même analyseur que FromXmlString ; même règle que Mindset (signature.ps1).
            $aVoir = New-Object System.Collections.Stack
            $aVoir.Push([System.Security.SecurityElement]::FromString($xml))
            while ($aVoir.Count) {
                $el = $aVoir.Pop()
                foreach ($nom in 'P', 'Q', 'DP', 'DQ', 'InverseQ', 'D') {
                    if ($el.Tag -ceq $nom -or $el.Tag.EndsWith(":$nom", [StringComparison]::Ordinal)) { return $false }
                }
                foreach ($enfant in @($el.Children)) { if ($enfant) { $aVoir.Push($enfant) } }
            }
        }
        foreach ($rsa in $cles) {
            if ($rsa.VerifyData($Donnees, [System.Security.Cryptography.CryptoConfig]::MapNameToOID('SHA256'), $Signature)) { return $true }
        }
        $false
    } catch { $false }
    finally { foreach ($rsa in $cles) { $rsa.Dispose() } }
}

# Dézippe dans $Dest les octets dont la signature vient d'être vérifiée (jamais le fichier relu sur le disque, qui
# pourrait avoir changé entre-temps), entrée par entrée, avec les contrôles de Mindset (paquets.ps1, Install-Paquet) que
# .NET ne fait pas :
# - une entrée marquée « chiffrée » est refusée (.NET en écrirait les octets tels quels) ;
# - le contenu doit avoir exactement la taille et le CRC annoncés par le zip, et un deflate illisible est refusé
#   (.NET écrirait sans contrôle ce qu'il tire d'un deflate abîmé : parfois rien du tout, sans erreur).
# Un zip abîmé n'est jamais installé, même signé.
# Le drapeau, le CRC annoncé et le calcul du CRC sont des parties internes de .NET : absentes, l'entrée est refusée.
# S'arrête sur une erreur au premier problème. Tourne dans un fil séparé (Install-Update la lui passe).
function Expand-ZipVerifie([byte[]]$Octets, [string]$Dest) {
    Add-Type -AssemblyName System.IO.Compression
    $abime = 'Le zip est abîmé : rien n''a été installé.'
    $interne = [Reflection.BindingFlags]'NonPublic,Instance'
    $champDrapeaux = [IO.Compression.ZipArchiveEntry].GetField('_generalPurposeBitFlag', $interne)
    $champCrc = [IO.Compression.ZipArchiveEntry].GetField('_crc32', $interne)
    $calculCrc = [IO.Compression.ZipArchive].Assembly.GetType('System.IO.Compression.Crc32Helper')
    if ($calculCrc) { $calculCrc = $calculCrc.GetMethod('UpdateCrc32', [Reflection.BindingFlags]'NonPublic,Public,Static', $null, [type[]]@([uint32], [byte[]], [int], [int]), $null) }
    $copier = {
        param($Entree, [IO.Stream]$Vers)
        if (-not $champDrapeaux -or ([int]$champDrapeaux.GetValue($Entree) -band 1)) { throw 'Le zip contient un fichier chiffré : rien n''a été installé.' }
        if (-not $calculCrc -or -not $champCrc) { throw $abime }
        $flux = $Entree.Open()
        try {
            $tampon = [byte[]]::new(81920)   # (New-Object l'envelopperait : Invoke ne le prendrait pas)
            $total = [long]0
            [uint32]$crc = 0
            while ($true) {
                try { $n = $flux.Read($tampon, 0, $tampon.Length) } catch { throw $abime }   # (deflate illisible)
                if ($n -le 0) { break }
                $total += $n
                if ($total -gt $Entree.Length) { throw $abime }
                $crc = $calculCrc.Invoke($null, [object[]]@($crc, $tampon, 0, $n))
                $Vers.Write($tampon, 0, $n)
            }
        } finally { $flux.Dispose() }
        if ($total -ne $Entree.Length -or $crc -ne [uint32]$champCrc.GetValue($Entree)) { throw $abime }
    }
    $racine = [IO.Path]::GetFullPath($Dest).TrimEnd('\') + '\'
    try { $archive = New-Object IO.Compression.ZipArchive((New-Object IO.MemoryStream (, $Octets)), [IO.Compression.ZipArchiveMode]::Read) }
    catch { throw $abime }   # (illisible pour .NET)
    try {
        New-Item -ItemType Directory -Force -Path $racine | Out-Null
        foreach ($e in $archive.Entries) {
            # (comme Expand-Archive : rien hors du dossier, un dossier pour une entrée sans nom de fichier, fichier
            # remplacé s'il existe, date de modification reprise du zip)
            $cible = [IO.Path]::GetFullPath((Join-Path $racine $e.FullName.Replace('/', '\')))
            if (-not $cible.StartsWith($racine, [StringComparison]::OrdinalIgnoreCase)) { throw "Chemin refusé dans le zip : $($e.FullName)" }
            if (-not $e.Name) { New-Item -ItemType Directory -Force -Path $cible | Out-Null; continue }
            New-Item -ItemType Directory -Force -Path (Split-Path $cible -Parent) | Out-Null
            $fichier = [IO.File]::Open($cible, [IO.FileMode]::Create, [IO.FileAccess]::Write, [IO.FileShare]::None)
            try { & $copier $e $fichier } finally { $fichier.Dispose() }
            [IO.File]::SetLastWriteTime($cible, $e.LastWriteTime.DateTime)
        }
    } catch [IO.InvalidDataException] { throw $abime }
    finally { $archive.Dispose() }
}

function Invoke-UpdateCheck([switch]$Manual) {
    # Géré par Mindset : c'est lui qui installe les mises à jour de Nevermind, pas de vérification ici
    if ($Managed) { $ui.UpdateStatus.Text = "Version $AppVersion. Les mises à jour de Nevermind passent par Mindset : ouvre Mindset pour les installer."; return }
    # Recherche au lancement coupée (Paramètres, Mises à jour) : seulement quand on clique sur « Vérifier les mises à jour »
    if (-not $Manual -and -not (Test-MajAuto)) { $ui.UpdateStatus.Text = "Version $AppVersion. Clique sur « Vérifier les mises à jour » pour chercher une nouvelle version."; return }
    $ui.UpdateStatus.Text = 'Recherche d''une nouvelle version...'
    $r = Invoke-Async $GetReleaseScript @{ Repo = $UpdateRepo; Beta = [bool](Get-Setting 'Beta' $false); Ancien = ((Split-Path (Get-AppScript) -Leaf) -eq 'OptiGame.ps1') } | Select-Object -First 1
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
        Write-Log "Mise à jour $($rel.Version) refusée : pas de signature (Nevermind.zip.sig) dans la version publiée"
        Show-Message ($refused -f 'absente') 'Warning'
        return
    }
    Set-Busy $true
    $ui.UpdateBanner.Visibility = 'Collapsed'
    Set-Status "Téléchargement de la version $($rel.Version)..."
    $tmp = Join-Path $env:TEMP "Nevermind-maj-$(Get-Random)"
    $zip = Join-Path $tmp 'Nevermind.zip'
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
        $signed = $false; $octets = $null
        try { $octets = [IO.File]::ReadAllBytes($zip); $signed = Test-Signature $octets ([IO.File]::ReadAllBytes("$zip.sig")) $PublicKeyFile } catch {}
        if (-not $signed) {
            Write-Log "Mise à jour $($rel.Version) refusée : signature invalide"
            try { [IO.Directory]::Delete($tmp, $true) } catch {}
            Show-Message ($refused -f 'fausse ou illisible') 'Warning'
            Set-Status 'Mise à jour annulée.'
            return
        }
        # Dézippé depuis les octets dont la signature vient d'être vérifiée, pas depuis le fichier relu
        $res = Invoke-Async {
            param($a)
            try { Set-Item -Path 'function:Expand-ZipVerifie' -Value $a.Code; Expand-ZipVerifie $a.Octets (Join-Path $a.Dir 'x'); 'OK' } catch { $_.Exception.Message }
        } @{ Dir = $tmp; Octets = $octets; Code = ${function:Expand-ZipVerifie}.ToString() } | Select-Object -First 1
        # Zip signé mais abîmé ou chiffré : rien n'est installé, et réessayer ne changerait rien
        if ("$res" -like 'Le zip *rien n''a été installé.') {
            Write-Log "Mise à jour $($rel.Version) refusée : $res"
            try { [IO.Directory]::Delete($tmp, $true) } catch {}
            Show-Message "La version $($rel.Version) n'a pas été installée.`n`n$res Ton Nevermind reste tel quel.`n`nInstalle Mindset pour recevoir les mises à jour." 'Warning'
            Set-Status 'Mise à jour annulée.'
            return
        }
    }
    # (dossier « Nevermind » et script Nevermind.ps1 ; « OptiGame » et OptiGame.ps1, leurs anciens noms, dans une version d'avant)
    $src = @('Nevermind', 'OptiGame' | ForEach-Object { Join-Path $tmp "x\$_" } | Where-Object { Test-Path -LiteralPath $_ })[0]
    $scriptMaj = if ($src) { @('Nevermind.ps1', 'OptiGame.ps1' | ForEach-Object { Join-Path $src "fichiers\$_" } | Where-Object { Test-Path -LiteralPath $_ })[0] }
    if ("$res" -ne 'OK' -or -not $scriptMaj) {
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
    $script:Relaunch = Join-Path $appRoot "fichiers\$(Split-Path $scriptMaj -Leaf)"
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
# (Une copie non gérée ne s'ouvre pas, sauf un dossier de travail : voir le début de Nevermind.ps1.)
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
