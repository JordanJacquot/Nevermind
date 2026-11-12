# Nevermind : le mode streameur essayé sur un VRAI Streamlabs Desktop, sur la machine jetable de GitHub Actions
# (workflow .github\workflows\streamlabs-reel.yml). Le vrai installateur officiel (streamlabs.com), installé en silence,
# lancé pour qu'il crée ses réglages ; puis le vrai chemin « Optimiser » et « Remettre comme avant » de Nevermind (le
# bloc stream entier, par l'exemple rust\nevermind-stream\examples\essai-streamlabs.rs) ; puis Streamlabs relancé, et ce
# qu'il a VRAIMENT pris lu par sa porte officielle (son API, le tube nommé \\.\pipe\slobs, JSON-RPC).
# JAMAIS sur un vrai PC : le script refuse de tourner hors de GitHub Actions.
#
#   .\outils\essai-streamlabs-reel.ps1 -Exemple <essai-streamlabs.exe>
#   -Source installateur   la version du bouton « Télécharger » de streamlabs.com (latest.yml, SHA-512 vérifié)
#   -Source mise-a-jour    la version que la mise à jour automatique de Streamlabs installe chez ses utilisateurs
#                          (latest.json, souvent plus récente), sur le même site officiel
# Streamlabs est lancé avec --skip-update : la version essayée reste celle installée.
#
# Code de sortie 1 si un cas échoue.
param([Parameter(Mandatory)][string]$Exemple, [ValidateSet('installateur', 'mise-a-jour')][string]$Source = 'installateur', [string]$Racine = 'C:\essai-sl')

if ($env:GITHUB_ACTIONS -ne 'true') {
    Write-Host 'Refusé : cet essai installe Streamlabs Desktop et change ses réglages pour de vrai. Il ne tourne que sur la machine jetable de GitHub Actions.' -ForegroundColor Red
    exit 2
}
$ErrorActionPreference = 'Stop'
$Reglages = Join-Path $env:APPDATA 'slobs-client'
$Donnees = Join-Path $Racine 'donnees'
$Captures = Join-Path $Racine 'captures'
New-Item -ItemType Directory -Force -Path $Racine, $Captures | Out-Null

$script:Reussis = 0
$script:Echoues = New-Object Collections.Generic.List[string]
function Write-Partie([string]$Titre) { Write-Host ''; Write-Host $Titre -ForegroundColor Cyan }
function Write-Note([string]$Texte) { Write-Host "  NOTE   $Texte" -ForegroundColor Yellow }
function Write-Detail([string]$Texte) { Write-Host "         | $Texte" -ForegroundColor DarkGray }
function Test-Cas([string]$Nom, [scriptblock]$Condition) {
    $detail = ''
    try { $r = @(& $Condition) } catch { $r = @(); $detail = "$($_.Exception.Message) $($_.ScriptStackTrace)" }
    if ($r.Count -and $r[-1] -is [bool] -and $r[-1]) {
        $script:Reussis++
        Write-Host "  OK     $Nom" -ForegroundColor Green
    } else {
        $script:Echoues.Add($Nom)
        Write-Host "  ÉCHEC  $Nom$(if ($detail) { " ($detail)" })" -ForegroundColor Red
    }
}
function Wait-Jusque([scriptblock]$Condition, [int]$Secondes = 60) {
    $limite = (Get-Date).AddSeconds($Secondes)
    while ((Get-Date) -lt $limite) { if (& $Condition) { return $true }; Start-Sleep -Milliseconds 500 }
    [bool](& $Condition)
}
function Save-Capture([string]$Nom) {
    try {
        Add-Type -AssemblyName System.Windows.Forms, System.Drawing
        $b = [Windows.Forms.Screen]::PrimaryScreen.Bounds
        $img = New-Object Drawing.Bitmap $b.Width, $b.Height
        $g = [Drawing.Graphics]::FromImage($img)
        $g.CopyFromScreen($b.Location, [Drawing.Point]::Empty, $b.Size)
        $img.Save((Join-Path $Captures "$Nom.png"), [Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $img.Dispose()
    } catch { Write-Detail "capture $Nom impossible : $($_.Exception.Message)" }
}

# ---------- Streamlabs : processus, lancement, fermeture ----------
$Installe = $null
function Get-ProcessusStreamlabs {
    @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {
            $_.Name -eq 'Streamlabs OBS.exe' -or ($_.Name -in 'obs64.exe', 'crash-handler-process.exe' -and [string]$_.ExecutablePath -match 'Streamlabs|obs-studio-node')
        })
}
# Pour comprendre un lancement raté : les processus (fenêtre, ligne de commande) et la fin du journal de Streamlabs
function Show-Lancement {
    foreach ($p in @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object { [string]$_.ExecutablePath -match 'Streamlabs|slobs|updater' -or $_.Name -match 'updater|Streamlabs' })) {
        $titre = ''; try { $titre = (Get-Process -Id $p.ProcessId -ErrorAction Stop).MainWindowTitle } catch {}
        $cl = [string]$p.CommandLine; if ($cl.Length -gt 300) { $cl = $cl.Substring(0, 300) + '...' }
        Write-Detail "processus $($p.ProcessId) $($p.Name) fenêtre « $titre » : $cl"
    }
    foreach ($j in @(Get-ChildItem -LiteralPath $Reglages -Recurse -File -Filter '*.log' -ErrorAction SilentlyContinue | Where-Object { $_.Length -lt 2MB })) {
        Write-Detail "--- $($j.FullName.Substring($Reglages.Length + 1)) (fin)"
        foreach ($l in @(Get-Content -LiteralPath $j.FullName -Tail 60 -ErrorAction SilentlyContinue)) { Write-Detail "    $l" }
    }
}
function Test-Tube { try { @([IO.Directory]::GetFiles('\\.\pipe\')) -contains '\\.\pipe\slobs' } catch { $false } }
function Start-Streamlabs([string]$Nom) {
    $exe = Join-Path $script:Installe 'Streamlabs OBS.exe'
    # (--skip-update : la version essayée reste celle installée ; sans lui, Streamlabs lance son programme de mise à
    # jour au démarrage, qui attend en vain sur la machine de GitHub)
    $p = Start-Process -FilePath $exe -ArgumentList '--skip-update' -PassThru -WorkingDirectory $script:Installe
    Write-Detail "Streamlabs lancé ($Nom), processus $($p.Id)"
    $tube = Wait-Jusque { Test-Tube } 150
    if (-not $tube) {
        Write-Note "le tube \\.\pipe\slobs n'est pas apparu en 150 s ($Nom)"
        Save-Capture "$Nom-sans-tube"
        Show-Lancement
        return $false
    }
    # (le tube est là tôt : on attend que l'API réponde, puis que le moteur ait fini de charger)
    $pret = Wait-Jusque { $null -ne (Invoke-Slobs 'getModel' 'StreamingService' -Silencieux) } 120
    Start-Sleep -Seconds 10
    Save-Capture $Nom
    $pret
}
function Stop-Streamlabs([string]$Nom) {
    $l = @(Get-Process -Name 'Streamlabs OBS' -ErrorAction SilentlyContinue)
    if (-not $l.Count) { return 'déjà fermé' }
    foreach ($p in $l) { if ($p.MainWindowHandle -ne [IntPtr]::Zero) { [void]$p.CloseMainWindow() } }
    $ferme = Wait-Jusque { -not @(Get-ProcessusStreamlabs).Count } 90
    $comment = 'fermé par sa fenêtre'
    if (-not $ferme) {
        $comment = 'FORCÉ (pas fermé en 90 s)'
        foreach ($p in @(Get-ProcessusStreamlabs)) { try { Stop-Process -Id $p.ProcessId -Force -ErrorAction Stop } catch {} }
        [void](Wait-Jusque { -not @(Get-ProcessusStreamlabs).Count } 30)
    }
    Write-Detail "Streamlabs $comment ($Nom)"
    Start-Sleep -Seconds 3
    $comment
}

# ---------- l'API de Streamlabs (\\.\pipe\slobs, JSON-RPC 2.0, une réponse par ligne) ----------
$script:IdRpc = 0
function Invoke-Slobs([string]$Methode, [string]$Ressource, [object[]]$Arguments = @(), [switch]$Silencieux, [switch]$Brut) {
    $c = $null
    try {
        $c = New-Object IO.Pipes.NamedPipeClientStream('.', 'slobs', [IO.Pipes.PipeDirection]::InOut)
        $c.Connect(5000)
        $script:IdRpc++
        $id = $script:IdRpc
        $params = @{ resource = $Ressource }
        if ($Arguments.Count) { $params.args = $Arguments }
        $req = [ordered]@{ jsonrpc = '2.0'; id = $id; method = $Methode; params = $params } | ConvertTo-Json -Depth 30 -Compress
        $o = [Text.Encoding]::UTF8.GetBytes($req + "`n")
        $c.Write($o, 0, $o.Length); $c.Flush()
        $tampon = New-Object byte[] 65536
        $recu = New-Object IO.MemoryStream
        $limite = (Get-Date).AddSeconds(15)
        while ((Get-Date) -lt $limite) {
            $t = $c.ReadAsync($tampon, 0, $tampon.Length)
            if (-not $t.Wait(15000)) { break }
            if ($t.Result -le 0) { break }
            $recu.Write($tampon, 0, $t.Result)
            $texte = [Text.Encoding]::UTF8.GetString($recu.ToArray())
            foreach ($ligne in ($texte -split "`n")) {
                if (-not $ligne.Trim()) { continue }
                try { $j = $ligne | ConvertFrom-Json } catch { continue }
                if ($j.id -eq $id) {
                    if ($Brut) { return $j }
                    if ($j.error) { if (-not $Silencieux) { Write-Detail "API $Ressource.$Methode : erreur $($j.error | ConvertTo-Json -Compress -Depth 5)" }; return $null }
                    if ($null -eq $j.result) { return [pscustomobject]@{ vide = $true } }
                    return $j.result
                }
            }
        }
        if (-not $Silencieux) { Write-Detail "API $Ressource.$Methode : pas de réponse" }
        $null
    } catch {
        if (-not $Silencieux) { Write-Detail "API $Ressource.$Methode : $($_.Exception.Message)" }
        $null
    } finally { if ($c) { $c.Dispose() } }
}
function Show-Api([string]$Methode, [string]$Ressource, [object[]]$Arguments = @()) {
    $r = Invoke-Slobs $Methode $Ressource $Arguments -Brut
    $t = if ($null -eq $r) { '(rien)' } else { $r | ConvertTo-Json -Depth 30 -Compress }
    if ($t.Length -gt 6000) { $t = $t.Substring(0, 6000) + ' [...]' }
    Write-Detail "API $Ressource.$Methode($(($Arguments | ConvertTo-Json -Compress -Depth 5))) = $t"
}

# ---------- les fichiers de réglages ----------
$Exclus = 'Cache|GPUCache|Code Cache|Crashpad|blob_storage|Service Worker|DawnCache|DawnGraphiteCache|DawnWebGPUCache|Shared Dictionary|shared_proto_db|VideoDecodeStats|WebStorage|Network|Session Storage|Partitions|plugin_config|media-backup|Dictionaries|logs'
function Get-Empreintes {
    $h = @{}
    if (-not (Test-Path -LiteralPath $Reglages)) { return $h }
    foreach ($f in @(Get-ChildItem -LiteralPath $Reglages -Recurse -File -ErrorAction SilentlyContinue)) {
        $rel = $f.FullName.Substring($Reglages.Length + 1)
        if ($rel -match "^($Exclus)(\\|$)") { continue }
        try { $h[$rel] = (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash.Substring(0, 12) + " ($($f.Length) o)" } catch { $h[$rel] = '(illisible)' }
    }
    $h
}
function Compare-Empreintes([hashtable]$A, [hashtable]$B, [string]$Titre) {
    $l = @($A.Keys + $B.Keys | Sort-Object -Unique | Where-Object { $A[$_] -ne $B[$_] })
    Write-Detail "$Titre : $($l.Count) fichier(s) changé(s)"
    foreach ($k in $l) { Write-Detail "  $k : $($A[$k]) -> $($B[$k])" }
}
function Show-Fichiers([string]$Titre) {
    Write-Partie "Fichiers de réglages : $Titre"
    if (-not (Test-Path -LiteralPath $Reglages)) { Write-Detail 'slobs-client absent'; return }
    foreach ($f in @(Get-ChildItem -LiteralPath $Reglages -File -ErrorAction SilentlyContinue | Sort-Object Name)) {
        Write-Detail "$($f.Name) ($($f.Length) o, $($f.LastWriteTime.ToString('HH:mm:ss')))"
        if ($f.Extension -in '.ini' -or ($f.Extension -eq '.json' -and $f.Length -lt 20000)) {
            $t = [IO.File]::ReadAllText($f.FullName)
            if ($f.Name -eq 'service.json') { $t = $t -replace '("key"\s*:\s*")[^"]*"', '$1[caché]"' }
            foreach ($ligne in ($t -split "`r?`n")) { if ($ligne.Trim()) { Write-Detail "    $ligne" } }
        }
    }
    foreach ($d in @(Get-ChildItem -LiteralPath $Reglages -Directory -ErrorAction SilentlyContinue | Sort-Object Name)) {
        $n = @(Get-ChildItem -LiteralPath $d.FullName -Recurse -File -ErrorAction SilentlyContinue).Count
        Write-Detail "[$($d.Name)] $n fichier(s)"
    }
    # Où sont la résolution et les images par seconde : chaque fichier (hors caches) qui en parle
    $motif = '(?i)(baseWidth|baseHeight|outputWidth|outputHeight|outputRes|baseRes|fpsNum|fpsDen|fpsType|fpsCommon|fpsInt|OutputCX|OutputCY|videoSettings|FPSCommon)'
    foreach ($f in @(Get-ChildItem -LiteralPath $Reglages -Recurse -File -ErrorAction SilentlyContinue)) {
        $rel = $f.FullName.Substring($Reglages.Length + 1)
        if ($rel -match "^($Exclus)(\\|$)" -or $f.Length -gt 5MB) { continue }
        try {
            $o = [IO.File]::ReadAllBytes($f.FullName)
            $t = [Text.Encoding]::UTF8.GetString($o) -replace '[^\x20-\x7E\r\n]', '.'
            $m = [regex]::Matches($t, $motif)
            if (-not $m.Count) { continue }
            Write-Detail "$rel : $($m.Count) mention(s)"
            $vus = 0
            foreach ($x in $m) {
                $a = [Math]::Max(0, $x.Index - 60); $long = [Math]::Min($t.Length - $a, 220)
                Write-Detail "    ...$(($t.Substring($a, $long)) -replace '\s+', ' ')..."
                if (++$vus -ge 6) { break }
            }
        } catch {}
    }
}
function Read-Ini([string]$Fichier) {
    $r = @{}; $s = ''
    if (-not (Test-Path -LiteralPath $Fichier)) { return $r }
    foreach ($l in [IO.File]::ReadAllLines($Fichier)) {
        $l = $l.TrimStart([char]0xFEFF)
        if ($l -match '^\[(.+)\]\s*$') { $s = $Matches[1]; continue }
        if ($l -match '^([^=]+)=(.*)$') { $r["$s.$($Matches[1])"] = $Matches[2] }
    }
    $r
}

# ---------- Nevermind (le bloc stream, par l'exemple essai-streamlabs) ----------
function Invoke-Nevermind([string[]]$Arguments) {
    $sortie = Join-Path $Racine "nevermind-$([guid]::NewGuid().ToString('N').Substring(0,6)).json"
    $err = "$sortie.err"
    $p = Start-Process -FilePath $Exemple -ArgumentList (@($Donnees) + $Arguments) -NoNewWindow -Wait -PassThru -RedirectStandardOutput $sortie -RedirectStandardError $err
    $t = [IO.File]::ReadAllText($sortie, [Text.Encoding]::UTF8)
    $e = [IO.File]::ReadAllText($err)
    if ($e.Trim()) { Write-Detail "stderr : $e" }
    $j = $null; try { $j = $t | ConvertFrom-Json } catch {}
    Write-Detail "Nevermind $($Arguments -join ' ') (code $($p.ExitCode)) : annonces $(@($j.annonces) -join ' | ')"
    if ($j.plan) { Write-Detail "  plan : $($j.plan | ConvertTo-Json -Compress)" }
    foreach ($l in @($j.journal)) { if ($l) { Write-Detail "  journal : $l" } }
    $j
}

# ---------- ce que Streamlabs a pris (par son API) ----------
# La sortie : le formulaire relu dans son moteur (SettingsService.fetchSettingsFromObs, catégorie Output) ; la vidéo :
# VideoSettingsService.state (sortie horizontale), là où Streamlabs la garde depuis la sortie double
function Get-Pris([switch]$Muet) {
    $pris = [ordered]@{}
    $r = Invoke-Slobs 'fetchSettingsFromObs' 'SettingsService' @('Output') -Silencieux
    foreach ($g in @($r.formData)) { foreach ($p in @($g.parameters)) { if ($p.name) { $pris["Output.$($p.name)"] = $p.value } } }
    $s = Invoke-Slobs 'state' 'VideoSettingsService' -Silencieux
    if ($s -and $s.horizontal) { foreach ($p in $s.horizontal.PSObject.Properties) { $pris["Video.$($p.Name)"] = $p.Value } }
    $m = Invoke-Slobs 'getModel' 'StreamingService' -Silencieux
    $pris['Etat.enregistrement'] = [string]$m.recordingStatus
    if (-not $Muet) { Write-Detail "Pris par Streamlabs : $($pris | ConvertTo-Json -Compress -Depth 6)" }
    $pris
}
function Get-Valeurs($Pris) { [ordered]@{ sortie = "$($Pris['Video.outputWidth'])x$($Pris['Video.outputHeight'])"; ips = "$($Pris['Video.fpsNum'])/$($Pris['Video.fpsDen'])"; mode = [string]$Pris['Output.Mode']; debit = $(if ($Pris['Output.Mode'] -eq 'Advanced') { [string]$Pris['Output.bitrate'] } else { [string]$Pris['Output.VBitrate'] }); encodeur = $(if ($Pris['Output.Mode'] -eq 'Advanced') { [string]$Pris['Output.Encoder'] } else { [string]$Pris['Output.StreamEncoder'] }); image_cle = [string]$Pris['Output.keyint_sec']; dossier_avance = [string]$Pris['Output.RecFilePath']; dossier_simple = [string]$Pris['Output.FilePath'] } }
function Show-Valeurs([string]$Titre, $V) { Write-Detail "$Titre : $($V | ConvertTo-Json -Compress)" }
# Les champs du formulaire Sortie, par sous-partie (nom=valeur, et les choix d'un champ à liste)
function Show-Formulaire([string]$Titre) {
    $r = Invoke-Slobs 'fetchSettingsFromObs' 'SettingsService' @('Output') -Silencieux
    foreach ($g in @($r.formData)) {
        $l = @($g.parameters | ForEach-Object { $o = if ($_.options) { " [$((@($_.options | ForEach-Object { $_.value }) -join '|'))]" } else { '' }; "$($_.name)=$($_.value)$o" })
        Write-Detail "Sortie ($Titre) / $($g.nameSubCategory) : $($l -join ' ; ')"
    }
}
# Les clés qui diffèrent entre deux relevés (Video.* et Output.*)
function Get-Differences($A, $B) {
    @(@($A.Keys) + @($B.Keys) | Sort-Object -Unique | Where-Object { $_ -match '^(Video|Output)\.' -and ([string]($A[$_] | ConvertTo-Json -Compress -Depth 6)) -ne ([string]($B[$_] | ConvertTo-Json -Compress -Depth 6)) } |
        ForEach-Object { "$_ : $($A[$_] | ConvertTo-Json -Compress -Depth 6) -> $($B[$_] | ConvertTo-Json -Compress -Depth 6)" })
}

# =====================================================================================================================
$Version = '?'
Write-Partie '1. Le vrai Streamlabs Desktop (installateur officiel, signé), installé en silence'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$Installateur = Join-Path $Racine 'installateur\Streamlabs-Desktop-Setup.exe'
New-Item -ItemType Directory -Force -Path (Split-Path $Installateur) | Out-Null
$attendu = $null
if ($Source -eq 'mise-a-jour') {
    # La version que la mise à jour automatique de Streamlabs installe chez les utilisateurs (latest.json, celui que lit
    # son propre programme de mise à jour), prise sur le même site officiel
    $j = (Invoke-WebRequest -Uri 'https://slobs-cdn.streamlabs.com/latest.json' -UseBasicParsing).Content
    if ($j -is [byte[]]) { $j = [Text.Encoding]::UTF8.GetString($j) }
    $Version = [string]($j | ConvertFrom-Json).version
    Write-Detail "Version installée par la mise à jour automatique de Streamlabs : $Version"
    $lien = "https://slobs-cdn.streamlabs.com/Streamlabs+Desktop+Setup+$Version.exe"
} else {
    try {
        $yml = (Invoke-WebRequest -Uri 'https://slobs-cdn.streamlabs.com/latest.yml' -UseBasicParsing).Content
        if ($yml -is [byte[]]) { $yml = [Text.Encoding]::UTF8.GetString($yml) }
        if ($yml -match '(?m)^version:\s*(\S+)') { $Version = $Matches[1] }
        if ($yml -match '(?m)^sha512:\s*(\S+)') { $attendu = $Matches[1] }
    } catch { Write-Note "latest.yml illisible : $($_.Exception.Message)" }
    Write-Detail "Version du bouton « Télécharger » de streamlabs.com : $Version"
    $req = [Net.HttpWebRequest]::Create('https://streamlabs.com/streamlabs-desktop/download')
    $req.AllowAutoRedirect = $false; $req.UserAgent = 'Mozilla/5.0'
    $rep = $req.GetResponse(); $lien = $rep.Headers['Location']; $rep.Close()
}
Write-Detail "Lien officiel : $lien"
Invoke-WebRequest -Uri $lien -OutFile $Installateur -UseBasicParsing
$sig = Get-AuthenticodeSignature -LiteralPath $Installateur
$sha = [Convert]::ToBase64String([Security.Cryptography.SHA512]::Create().ComputeHash([IO.File]::ReadAllBytes($Installateur)))
Write-Detail "Installateur : $((Get-Item $Installateur).Length) o, signature $($sig.Status), signé par $($sig.SignerCertificate.Subject)"
Test-Cas "installateur officiel ($lien) signé Authenticode, signature valide" { $lien -match '^https://slobs-cdn\.streamlabs\.com/' -and $sig.Status -eq 'Valid' -and $sig.SignerCertificate.Subject -match 'Streamlabs|General Workings|Logitech' }
if ($attendu) { Test-Cas "installateur identique à celui annoncé par latest.yml ($Version, SHA-512)" { $sha -eq $attendu } }
$p = Start-Process -FilePath $Installateur -ArgumentList '/S' -PassThru -Wait
Write-Detail "Installateur fini (code $($p.ExitCode))"
foreach ($c in @("$env:ProgramFiles\Streamlabs OBS", "$env:LOCALAPPDATA\Programs\Streamlabs OBS", "${env:ProgramFiles(x86)}\Streamlabs OBS")) {
    if (Test-Path -LiteralPath (Join-Path $c 'Streamlabs OBS.exe')) { $Installe = $c; break }
}
$cles = @(Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*', 'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue | Where-Object { [string]$_.DisplayName -like 'Streamlabs*' })
foreach ($k in $cles) { Write-Detail "Désinstallation : $($k.DisplayName) $($k.DisplayVersion) ; $($k.InstallLocation) ; $($k.UninstallString)" }
Test-Cas "Streamlabs installé ($Installe)" { [bool]$Installe }
if (-not $Installe) { exit 1 }
$Version = (Get-Item (Join-Path $Installe 'Streamlabs OBS.exe')).VersionInfo.ProductVersion
Write-Note "Version de Streamlabs Desktop essayée : $Version"
[void](Wait-Jusque { -not @(Get-ProcessusStreamlabs).Count } 5)
if (@(Get-ProcessusStreamlabs).Count) { [void](Stop-Streamlabs 'après installation') }


Write-Partie '2. Premier lancement : Streamlabs crée ses réglages ; un réglage « à soi » (60 images par seconde)'
$lance = Start-Streamlabs 'premier'
Test-Cas 'Streamlabs démarre et son API répond (\\.\pipe\slobs)' { $lance }
if (-not $lance) { [void](Stop-Streamlabs 'premier'); Show-Fichiers 'premier lancement raté'; Write-Host 'Streamlabs ne démarre pas : essai arrêté.' -ForegroundColor Red; exit 1 }
# (l'utilisateur a mis 60 images par seconde dans ses Paramètres : ce que « Remettre comme avant » doit rendre)
Show-Formulaire 'mode Simple'
Show-Api 'setSettingValue' 'SettingsService' @('Output', 'Mode', 'Advanced')
Show-Formulaire 'mode Avancé'
Show-Api 'setSettingValue' 'SettingsService' @('Output', 'Mode', 'Simple')
Show-Api 'setSettings' 'VideoSettingsService' @(@{ fpsNum = 60; fpsDen = 1 }, 'horizontal')
Start-Sleep -Seconds 2
$Avant = Get-Pris
$Base = "$($Avant['Video.baseWidth'])x$($Avant['Video.baseHeight'])"
Show-Valeurs 'Avant' (Get-Valeurs $Avant)
Test-Cas 'réglage de départ : 60 images par seconde (pris par Streamlabs)' { [string]$Avant['Video.fpsNum'] -eq '60' }
[void](Stop-Streamlabs 'premier')
Show-Fichiers 'après le premier lancement'

Write-Partie '3. « Optimiser » Streamlabs fermé : refusé proprement, rien de changé'
$e1 = Get-Empreintes
$j = Invoke-Nevermind @('regler', '3', 'twitch', $Base)
$e2 = Get-Empreintes
Test-Cas 'refus « Ouvre Streamlabs puis reclique sur Optimiser »' { @($j.annonces) -contains 'Ouvre Streamlabs puis reclique sur Optimiser' }
Test-Cas 'aucune copie faite' { -not $j.copie -and -not (Test-Path -LiteralPath (Join-Path $Donnees 'streameur\streamlabs\copie.json')) }
Compare-Empreintes $e1 $e2 'pendant le refus'
Test-Cas 'aucun fichier de Streamlabs changé pendant le refus' { $l = @(@($e1.Keys) + @($e2.Keys) | Sort-Object -Unique | Where-Object { $e1[$_] -ne $e2[$_] }); $l.Count -eq 0 }

Write-Partie '4. Streamlabs ouvert, en train d''enregistrer : « Optimiser » refusé, rien de changé'
[void](Start-Streamlabs 'enregistrement')
Show-Api 'toggleRecording' 'StreamingService'
$enreg = Wait-Jusque { (Invoke-Slobs 'getModel' 'StreamingService' -Silencieux).recordingStatus -eq 'recording' } 30
$p1 = Get-Pris -Muet
$j = Invoke-Nevermind @('regler', '3', 'twitch', $Base)
$p2 = Get-Pris -Muet
Test-Cas 'Streamlabs enregistre' { $enreg }
Test-Cas 'refus « Streamlabs est en train de streamer ou d''enregistrer. Arrête, puis reclique sur Optimiser. »' { @($j.annonces) -contains 'Streamlabs est en train de streamer ou d''enregistrer. Arrête, puis reclique sur Optimiser.' }
$d = @(Get-Differences $p1 $p2)
foreach ($x in $d) { Write-Detail "changé : $x" }
Test-Cas 'aucun réglage changé pendant l''enregistrement, aucune copie' { $d.Count -eq 0 -and -not $j.copie }
Show-Api 'toggleRecording' 'StreamingService'
[void](Wait-Jusque { (Invoke-Slobs 'getModel' 'StreamingService' -Silencieux).recordingStatus -eq 'offline' } 30)

Write-Partie '5. « Optimiser » Streamlabs ouvert (Twitch, 3 Mb/s) : ce que Streamlabs a pris, tout de suite et après relance'
function Test-Plan([string]$Quand, $Pris, $Plan, [string]$Encodeur) {
    $v = Get-Valeurs $Pris
    $res = "$($Plan.sortie[0])x$($Plan.sortie[1])"
    Show-Valeurs $Quand $v
    Test-Cas "$Quand : résolution de sortie $res" { $v.sortie -eq $res }
    Test-Cas "$Quand : $($Plan.ips) images par seconde" { $v.ips -eq "$($Plan.ips)/1" }
    Test-Cas "$Quand : débit vidéo $($Plan.video_kbps) kb/s" { $v.debit -eq [string]$Plan.video_kbps }
    Test-Cas "$Quand : encodeur $Encodeur" { $v.encodeur -eq $Encodeur }
}
$j = Invoke-Nevermind @('regler', '3', 'twitch', $Base)
$Plan = $j.plan
Test-Cas "annonce « Streamlabs est réglé : ... »" { @($j.annonces | Where-Object { $_ -like 'Streamlabs est réglé : *' }).Count -eq 1 }
Start-Sleep -Seconds 2
Test-Plan 'pris tout de suite' (Get-Pris) $Plan 'x264'
[void](Stop-Streamlabs 'apres-optimiser')
Show-Fichiers 'après « Optimiser » et fermeture'
[void](Start-Streamlabs 'relance-optimise')
Test-Plan 'gardé après relance' (Get-Pris) $Plan 'x264'

Write-Partie '6. « Remettre comme avant » : Streamlabs fermé, refusé ; ouvert, tout revient (et reste après relance)'
[void](Stop-Streamlabs 'avant-remettre')
$j = Invoke-Nevermind @('remettre')
Test-Cas 'Streamlabs fermé : refus « Ouvre Streamlabs puis reclique sur Remettre comme avant », copie gardée' { @($j.annonces) -contains 'Ouvre Streamlabs puis reclique sur Remettre comme avant' -and $null -ne $j.copie }
[void](Start-Streamlabs 'remettre')
$j = Invoke-Nevermind @('remettre')
Test-Cas 'annonce « Les réglages d''avant sont revenus dans Streamlabs. »' { @($j.annonces) -contains 'Les réglages d''avant sont revenus dans Streamlabs.' }
Start-Sleep -Seconds 2
$d = @(Get-Differences $Avant (Get-Pris))
foreach ($x in $d) { Write-Detail "différent d'avant : $x" }
Test-Cas 'tout est revenu comme avant, tout de suite (API)' { $d.Count -eq 0 }
[void](Stop-Streamlabs 'apres-remettre')
[void](Start-Streamlabs 'relance-remis')
$d = @(Get-Differences $Avant (Get-Pris))
foreach ($x in $d) { Write-Detail "différent d'avant : $x" }
Test-Cas 'tout est resté comme avant après relance' { $d.Count -eq 0 }

Write-Partie '7. Kick (image clé de 2 s) : mode Avancé, enregistrements au même endroit ; puis tout revient'
# (l'utilisateur enregistre ailleurs que dans le dossier par défaut : son dossier doit suivre en mode Avancé)
$monDossier = Join-Path $Racine 'mes videos'
New-Item -ItemType Directory -Force -Path $monDossier | Out-Null
Show-Api 'setSettingValue' 'SettingsService' @('Output', 'FilePath', $monDossier)
Start-Sleep -Seconds 1
$AvantKick = Get-Pris
$j = Invoke-Nevermind @('regler', '8', 'kick', $Base)
$PlanK = $j.plan
Start-Sleep -Seconds 2
$pk = Get-Pris
$v = Get-Valeurs $pk
Test-Plan 'Kick, pris tout de suite' $pk $PlanK 'obs_x264'
Test-Cas 'Kick : sortie en mode Avancé' { $v.mode -eq 'Advanced' }
Test-Cas 'Kick : une image clé toutes les 2 secondes' { $v.image_cle -eq '2' }
Test-Cas "Kick : les enregistrements restent dans « $monDossier »" { $v.dossier_avance -eq $monDossier }
[void](Stop-Streamlabs 'kick')
[void](Start-Streamlabs 'relance-kick')
$v = Get-Valeurs (Get-Pris)
Test-Cas 'Kick : mode Avancé, image clé et débit gardés après relance' { $v.mode -eq 'Advanced' -and $v.image_cle -eq '2' -and $v.debit -eq [string]$PlanK.video_kbps }
$j = Invoke-Nevermind @('remettre')
Start-Sleep -Seconds 2
$d = @(Get-Differences $AvantKick (Get-Pris))
foreach ($x in $d) { Write-Detail "différent d'avant : $x" }
Test-Cas 'Kick : tout est revenu comme avant (mode Simple, vidéo, débit)' { $d.Count -eq 0 }
# (relevé : Streamlabs limite-t-il le débit à celui de sa plateforme, Twitch ici, en mode Avancé ?)
Show-Api 'setSettingValue' 'SettingsService' @('Output', 'Mode', 'Advanced')
Show-Api 'setSettingValue' 'SettingsService' @('Output', 'bitrate', 8000)
Write-Detail "débit en mode Avancé après avoir demandé 8000 : $((Get-Pris -Muet)['Output.bitrate'])"
Show-Api 'setSettingValue' 'SettingsService' @('Output', 'Mode', 'Simple')
[void](Stop-Streamlabs 'fin')

Write-Partie 'Bilan'
Write-Host "Streamlabs Desktop $Version : $($script:Reussis) réussi(s), $($script:Echoues.Count) échoué(s)"
foreach ($e in $script:Echoues) { Write-Host "  ÉCHEC  $e" -ForegroundColor Red }
if ($script:Echoues.Count) { exit 1 }
exit 0
