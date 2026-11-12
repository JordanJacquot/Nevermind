# Nevermind : le mode streameur essayé sur un VRAI OBS Studio, sur la machine jetable de GitHub Actions
# (workflow .github\workflows\obs-reel.yml). Le vrai installateur officiel (la release GitHub d'obsproject/obs-studio,
# signature Authenticode et empreinte vérifiées), installé en silence, lancé une fois pour qu'il crée ses réglages ;
# puis le vrai chemin « Optimiser » et « Remettre comme avant » de Nevermind (le bloc stream entier, par l'exemple
# rust\nevermind-stream\examples\essai-obs-reel.rs) ; puis OBS relancé, et ce qu'il a VRAIMENT pris lu par sa porte
# officielle obs-websocket (intégrée à OBS depuis la 28), et par son journal pendant un court enregistrement (les
# réglages que son encodeur a reçus).
# JAMAIS sur un vrai PC : le script refuse de tourner hors de GitHub Actions. obs-websocket n'est activé que sur la
# machine jetable, pour l'essai, par son fichier de réglages.
#
#   .\outils\essai-obs-reel.ps1 -Exemple <essai-obs-reel.exe> -Version derniere|31.1.2|30.2.3
#
# Premier lancement sans l'assistant de configuration automatique : « [General] FirstRun=true » écrit avant (global.ini
# jusqu'à OBS 30, user.ini depuis la 31), comme OBS l'écrit lui-même après son premier lancement.
#
# Code de sortie 1 si un cas échoue.
param([Parameter(Mandatory)][string]$Exemple, [string]$Version = 'derniere', [string]$Racine = 'C:\essai-obs')

if ($env:GITHUB_ACTIONS -ne 'true') {
    Write-Host 'Refusé : cet essai installe OBS Studio et change ses réglages pour de vrai. Il ne tourne que sur la machine jetable de GitHub Actions.' -ForegroundColor Red
    exit 2
}
$ErrorActionPreference = 'Stop'
$Reglages = Join-Path $env:APPDATA 'obs-studio'
$Donnees = Join-Path $Racine 'donnees'
$Captures = Join-Path $Racine 'captures'
$MonDossier = Join-Path $Racine 'mes videos'
$Port = 4455
New-Item -ItemType Directory -Force -Path $Racine, $Captures, $MonDossier | Out-Null
$Utf8 = New-Object Text.UTF8Encoding($false)

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
function Write-Texte([string]$Fichier, [string]$Texte) {
    New-Item -ItemType Directory -Force -Path (Split-Path $Fichier) | Out-Null
    [IO.File]::WriteAllText($Fichier, $Texte, $Utf8)
}

# ---------- obs-websocket (protocole 5 : Hello, Identify, Request ; sans mot de passe, machine jetable) ----------
function Receive-Ws($Ws) {
    $tampon = New-Object byte[] 65536
    $recu = New-Object IO.MemoryStream
    do {
        $t = $Ws.ReceiveAsync([ArraySegment[byte]]::new($tampon), [Threading.CancellationToken]::None)
        if (-not $t.Wait(10000)) { throw 'pas de réponse en 10 s' }
        if ($t.Result.MessageType -eq [Net.WebSockets.WebSocketMessageType]::Close) { throw "fermé par OBS ($($Ws.CloseStatusDescription))" }
        $recu.Write($tampon, 0, $t.Result.Count)
    } while (-not $t.Result.EndOfMessage)
    [Text.Encoding]::UTF8.GetString($recu.ToArray()) | ConvertFrom-Json
}
function Send-Ws($Ws, $Objet) {
    $o = [Text.Encoding]::UTF8.GetBytes(($Objet | ConvertTo-Json -Depth 20 -Compress))
    if (-not $Ws.SendAsync([ArraySegment[byte]]::new($o), [Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).Wait(10000)) { throw 'envoi bloqué' }
}
function Invoke-Obs([string]$Type, [hashtable]$Donnees = @{}, [switch]$Silencieux) {
    $ws = New-Object Net.WebSockets.ClientWebSocket
    try {
        if (-not $ws.ConnectAsync([Uri]"ws://127.0.0.1:$Port", [Threading.CancellationToken]::None).Wait(5000)) { throw 'connexion impossible en 5 s' }
        $h = Receive-Ws $ws
        if ($h.op -ne 0) { throw "pas de Hello ($($h.op))" }
        Send-Ws $ws @{ op = 1; d = @{ rpcVersion = 1; eventSubscriptions = 0 } }
        $i = Receive-Ws $ws
        if ($i.op -ne 2) { throw "pas identifié ($($i | ConvertTo-Json -Compress -Depth 5))" }
        Send-Ws $ws @{ op = 6; d = @{ requestType = $Type; requestId = 'essai'; requestData = $Donnees } }
        for ($n = 0; $n -lt 20; $n++) {
            $r = Receive-Ws $ws
            if ($r.op -ne 7 -or $r.d.requestId -ne 'essai') { continue }
            if (-not $r.d.requestStatus.result) {
                if (-not $Silencieux) { Write-Detail "obs-websocket $Type : refusé ($($r.d.requestStatus.code) $($r.d.requestStatus.comment))" }
                return $null
            }
            if ($null -eq $r.d.responseData) { return [pscustomobject]@{ vide = $true } }
            return $r.d.responseData
        }
        throw 'pas de réponse à la demande'
    } catch {
        if (-not $Silencieux) { Write-Detail "obs-websocket $Type : $($_.Exception.Message)" }
        $null
    } finally { $ws.Dispose() }
}

# ---------- OBS : installation, lancement, fermeture ----------
$script:Installe = $null
$script:Majeure = 0
function Get-ProcessusObs { @(Get-Process -Name 'obs64', 'obs32' -ErrorAction SilentlyContinue) }
function Start-Obs([string]$Nom) {
    $bin = Join-Path $script:Installe 'bin\64bit'
    # (OBS se lance depuis son dossier ; sans vérification de mise à jour, sans la question « mode sans échec » après
    # une fermeture forcée, sans la recherche de fichiers manquants)
    $p = Start-Process -FilePath (Join-Path $bin 'obs64.exe') -ArgumentList '--disable-updater', '--disable-shutdown-check', '--disable-missing-files-check' -WorkingDirectory $bin -PassThru
    Write-Detail "OBS lancé ($Nom), processus $($p.Id)"
    $pret = Wait-Jusque { $null -ne (Invoke-Obs 'GetVersion' -Silencieux) } 150
    if (-not $pret) {
        Write-Note "obs-websocket ne répond pas en 150 s ($Nom)"
        Save-Capture "$Nom-sans-reponse"
        foreach ($x in Get-ProcessusObs) { Write-Detail "processus $($x.Id) $($x.ProcessName) fenêtre « $($x.MainWindowTitle) »" }
        Show-Journal 80
        return $false
    }
    Start-Sleep -Seconds 3
    Save-Capture $Nom
    $true
}
function Stop-Obs([string]$Nom) {
    $l = @(Get-ProcessusObs)
    if (-not $l.Count) { return 'déjà fermé' }
    foreach ($p in $l) { if ($p.MainWindowHandle -ne [IntPtr]::Zero) { [void]$p.CloseMainWindow() } }
    $ferme = Wait-Jusque { -not @(Get-ProcessusObs).Count } 60
    $comment = 'fermé par sa fenêtre'
    if (-not $ferme) {
        Save-Capture "$Nom-pas-ferme"
        $comment = 'FORCÉ (pas fermé en 60 s)'
        foreach ($p in @(Get-ProcessusObs)) { try { Stop-Process -Id $p.Id -Force -ErrorAction Stop } catch {} }
        [void](Wait-Jusque { -not @(Get-ProcessusObs).Count } 30)
    }
    Write-Detail "OBS $comment ($Nom)"
    Start-Sleep -Seconds 2
    $comment
}
function Get-DernierJournal { Get-ChildItem -LiteralPath (Join-Path $Reglages 'logs') -File -Filter '*.txt' -ErrorAction SilentlyContinue | Sort-Object LastWriteTime | Select-Object -Last 1 }
function Read-Partage([string]$Fichier) {
    $f = [IO.File]::Open($Fichier, 'Open', 'Read', 'ReadWrite, Delete')
    try { (New-Object IO.StreamReader($f, [Text.Encoding]::UTF8)).ReadToEnd() } finally { $f.Dispose() }
}
function Show-Journal([int]$Lignes = 40) {
    $j = Get-DernierJournal
    if (-not $j) { Write-Detail 'aucun journal d''OBS'; return }
    Write-Detail "--- journal d'OBS $($j.Name) (fin)"
    foreach ($l in @((Read-Partage $j.FullName) -split "`r?`n" | Select-Object -Last $Lignes)) { Write-Detail "    $l" }
}

# ---------- les fichiers de réglages ----------
function Get-Empreintes {
    $h = @{}
    foreach ($f in @(Get-ChildItem -LiteralPath $Reglages -Recurse -File -ErrorAction SilentlyContinue)) {
        $rel = $f.FullName.Substring($Reglages.Length + 1)
        if ($rel -notmatch '^(basic\\profiles\\|global\.ini$|user\.ini$)') { continue }
        try { $h[$rel] = (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash.Substring(0, 12) + " ($($f.Length) o)" } catch { $h[$rel] = '(illisible)' }
    }
    $h
}
function Get-Changes([hashtable]$A, [hashtable]$B) { @(@($A.Keys) + @($B.Keys) | Sort-Object -Unique | Where-Object { $A[$_] -ne $B[$_] }) }
function Show-Fichiers([string]$Titre) {
    Write-Partie "Fichiers de réglages : $Titre"
    foreach ($f in @(Get-ChildItem -LiteralPath $Reglages -Recurse -File -ErrorAction SilentlyContinue | Sort-Object FullName)) {
        $rel = $f.FullName.Substring($Reglages.Length + 1)
        if ($rel -notmatch '^(basic\\profiles\\|global\.ini$|user\.ini$|plugin_config\\obs-websocket\\)') { continue }
        Write-Detail "$rel ($($f.Length) o, $($f.LastWriteTime.ToString('HH:mm:ss')))"
        if ($f.Extension -notin '.ini', '.json' -or $f.Length -gt 20000) { continue }
        $t = Read-Partage $f.FullName
        # (la clé de stream n'est jamais montrée)
        if ($f.Name -like 'service.json*') { $t = $t -replace '("(key|password|bearer_token)"\s*:\s*")[^"]*"', '$1[caché]"' }
        foreach ($ligne in ($t -split "`r?`n")) { if ($ligne.Trim()) { Write-Detail "    $ligne" } }
    }
}

# ---------- Nevermind (le bloc stream, par l'exemple essai-obs-reel) ----------
function Invoke-Nevermind([string[]]$Arguments) {
    $sortie = Join-Path $Racine "nevermind-$([guid]::NewGuid().ToString('N').Substring(0,6)).json"
    $err = "$sortie.err"
    $p = Start-Process -FilePath $Exemple -ArgumentList (@("`"$Donnees`"") + $Arguments) -NoNewWindow -Wait -PassThru -RedirectStandardOutput $sortie -RedirectStandardError $err
    $t = [IO.File]::ReadAllText($sortie, [Text.Encoding]::UTF8)
    $e = [IO.File]::ReadAllText($err)
    if ($e.Trim()) { Write-Detail "stderr : $e" }
    $j = $null; try { $j = $t | ConvertFrom-Json } catch {}
    Write-Detail "Nevermind $($Arguments -join ' ') (code $($p.ExitCode)) : annonces $(@($j.annonces) -join ' | ')"
    if ($j.plan) { Write-Detail "  plan : $($j.plan | ConvertTo-Json -Compress -Depth 5)" }
    if ($j.page.module) { Write-Detail "  page : « $($j.page.module.titre) » $($j.page.module.texte)" }
    foreach ($l in @($j.journal)) { if ($l) { Write-Detail "  journal : $l" } }
    $j
}

# ---------- ce qu'OBS a pris (obs-websocket) ----------
$Parametres = @(
    @('Output', 'Mode'),
    @('Video', 'FPSType'), @('Video', 'FPSCommon'), @('Video', 'FPSInt'), @('Video', 'FPSNum'), @('Video', 'FPSDen'),
    @('SimpleOutput', 'StreamEncoder'), @('SimpleOutput', 'VBitrate'), @('SimpleOutput', 'ABitrate'), @('SimpleOutput', 'Preset'),
    @('SimpleOutput', 'NVENCPreset2'), @('SimpleOutput', 'FilePath'), @('SimpleOutput', 'RecFormat2'), @('SimpleOutput', 'RecQuality'),
    @('AdvOut', 'Encoder'), @('AdvOut', 'RecFilePath'), @('AdvOut', 'RecFormat2'), @('AdvOut', 'RecEncoder'), @('AdvOut', 'TrackIndex'),
    @('AdvOut', 'Track1Bitrate'), @('AdvOut', 'Rescale'), @('AdvOut', 'RescaleFilter')
)
function Get-Pris([switch]$Muet) {
    $pris = [ordered]@{}
    $v = Invoke-Obs 'GetVideoSettings'
    if ($v) { foreach ($p in $v.PSObject.Properties) { $pris["Video.$($p.Name)"] = [string]$p.Value } }
    foreach ($c in $Parametres) {
        $r = Invoke-Obs 'GetProfileParameter' @{ parameterCategory = $c[0]; parameterName = $c[1] } -Silencieux
        # (la valeur que le profil donne, sinon celle par défaut qu'OBS prend)
        $pris["$($c[0]).$($c[1])"] = if ($null -eq $r) { '(illisible)' } elseif ($null -ne $r.parameterValue) { [string]$r.parameterValue } else { "(défaut) $($r.defaultParameterValue)" }
    }
    # (le service de stream : jamais la clé, seulement son genre, son nom et son serveur)
    $s = Invoke-Obs 'GetStreamServiceSettings'
    $pris['Service.genre'] = [string]$s.streamServiceType
    $pris['Service.nom'] = [string]$s.streamServiceSettings.service
    $pris['Service.serveur'] = [string]$s.streamServiceSettings.server
    $pris['Service.cle_presente'] = [string](-not [string]::IsNullOrEmpty([string]$s.streamServiceSettings.key))
    $pris['Profil'] = [string](Invoke-Obs 'GetProfileList').currentProfileName
    if (-not $Muet) { Write-Detail "Pris par OBS : $($pris | ConvertTo-Json -Compress)" }
    $pris
}
function Get-Valeur($Pris, [string]$Cle) { ([string]$Pris[$Cle]) -replace '^\(défaut\) ', '' }
function Get-Differences($A, $B) {
    @(@($A.Keys) + @($B.Keys) | Sort-Object -Unique | Where-Object { (Get-Valeur $A $_) -ne (Get-Valeur $B $_) } | ForEach-Object { "$_ : $($A[$_]) -> $($B[$_])" })
}
# Un court enregistrement : OBS lance l'encodeur du stream (l'enregistrement le partage) et écrit dans son journal les
# réglages que l'encodeur a vraiment reçus. Rend le chemin de la vidéo et le journal de cette session.
function Invoke-Enregistrement([string]$Nom) {
    $journal = (Get-DernierJournal).FullName
    [void](Invoke-Obs 'StartRecord')
    $actif = Wait-Jusque { (Invoke-Obs 'GetRecordStatus' -Silencieux).outputActive -eq $true } 20
    Start-Sleep -Seconds 4
    $r = Invoke-Obs 'StopRecord'
    [void](Wait-Jusque { (Invoke-Obs 'GetRecordStatus' -Silencieux).outputActive -ne $true } 20)
    Write-Detail "enregistrement ($Nom) : actif $actif, vidéo « $($r.outputPath) »"
    # (OBS donne le chemin avec des « / » : remis à la façon de Windows pour le comparer)
    [pscustomobject]@{ actif = $actif; chemin = ([string]$r.outputPath).Replace([char]47, [char]92); journal = $journal }
}
# Les réglages du dernier encodeur x264 lancé, lus dans le journal d'OBS (après sa fermeture : tout y est écrit)
function Get-Encodeur([string]$Journal) {
    $t = Read-Partage $Journal
    $m = @([regex]::Matches($t, "\[x264 encoder: '([^']+)'\] settings:"))
    if (-not $m.Count) { Write-Detail 'aucun réglage d''encodeur x264 dans le journal'; return $null }
    $dernier = $m[-1]
    $bloc = $t.Substring($dernier.Index, [Math]::Min(1500, $t.Length - $dernier.Index))
    $e = [ordered]@{ nom = $dernier.Groups[1].Value }
    foreach ($k in 'rate_control', 'bitrate', 'fps_num', 'fps_den', 'width', 'height', 'keyint') {
        if ($bloc -match "(?m)\s$($k):\s+(\S+)") { $e[$k] = $Matches[1] }
    }
    $p = @([regex]::Matches($t, "\[x264 encoder: '$([regex]::Escape($e.nom))'\] preset: (\S+)"))
    if ($p.Count) { $e['preset'] = $p[-1].Groups[1].Value }
    Write-Detail "encodeur dans le journal d'OBS : $($e | ConvertTo-Json -Compress)"
    $e
}
function Test-Plan([string]$Quand, $Pris, $Plan, [string]$Mode) {
    $res = "$($Plan.sortie[0])x$($Plan.sortie[1])"
    Test-Cas "$Quand : sortie $res (GetVideoSettings)" { "$($Pris['Video.outputWidth'])x$($Pris['Video.outputHeight'])" -eq $res }
    Test-Cas "$Quand : $($Plan.ips) images par seconde (GetVideoSettings)" { $Pris['Video.fpsNumerator'] -eq [string]$Plan.ips -and $Pris['Video.fpsDenominator'] -eq '1' }
    Test-Cas "$Quand : sortie en mode $Mode" { (Get-Valeur $Pris 'Output.Mode') -eq $Mode }
    if ($Mode -eq 'Simple') {
        Test-Cas "$Quand : débit vidéo $($Plan.video_kbps) kb/s, son $($Plan.son_kbps) kb/s, encodeur $($Plan.encodeur)" {
            (Get-Valeur $Pris 'SimpleOutput.VBitrate') -eq [string]$Plan.video_kbps -and (Get-Valeur $Pris 'SimpleOutput.ABitrate') -eq [string]$Plan.son_kbps -and (Get-Valeur $Pris 'SimpleOutput.StreamEncoder') -eq $Plan.encodeur
        }
    } else {
        Test-Cas "$Quand : encodeur $($Plan.encodeur_avance), son $($Plan.son_kbps) kb/s" { (Get-Valeur $Pris 'AdvOut.Encoder') -eq $Plan.encodeur_avance -and (Get-Valeur $Pris 'AdvOut.Track1Bitrate') -eq [string]$Plan.son_kbps }
    }
}
function Test-Encodeur([string]$Quand, $Enc, $Plan, [int]$ImageCle = 0) {
    Test-Cas "$Quand : l'encodeur d'OBS a reçu $($Plan.video_kbps) kb/s en CBR, $($Plan.sortie[0])x$($Plan.sortie[1]) à $($Plan.ips) i/s (journal d'OBS)" {
        $Enc -and $Enc.bitrate -eq [string]$Plan.video_kbps -and $Enc.rate_control -eq 'CBR' -and $Enc.width -eq [string]$Plan.sortie[0] -and $Enc.height -eq [string]$Plan.sortie[1] -and $Enc.fps_num -eq [string]$Plan.ips
    }
    if ($Plan.encodeur -eq 'x264') { Test-Cas "$Quand : préréglage « veryfast » reçu par x264" { $Enc.preset -eq 'veryfast' } }
    if ($ImageCle) { Test-Cas "$Quand : une image clé toutes les $ImageCle s ($($ImageCle * $Plan.ips) images, journal d'OBS)" { $Enc.keyint -eq [string]($ImageCle * $Plan.ips) } }
}

# =====================================================================================================================
Write-Partie "1. Le vrai OBS Studio ($Version), installateur officiel de la release GitHub, signé, installé en silence"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$entetes = @{ 'User-Agent' = 'essai-nevermind'; Accept = 'application/vnd.github+json' }
if ($env:GH_TOKEN) { $entetes.Authorization = "Bearer $env:GH_TOKEN" }
$api = if ($Version -eq 'derniere') { 'https://api.github.com/repos/obsproject/obs-studio/releases/latest' } else { "https://api.github.com/repos/obsproject/obs-studio/releases/tags/$Version" }
$release = Invoke-RestMethod -Uri $api -Headers $entetes -UseBasicParsing
$asset = @($release.assets | Where-Object { $_.name -match '^OBS-Studio-[\d.]+-(Windows(-x64)?-Installer|Full-Installer-x64)\.exe$' }) | Select-Object -First 1
Write-Detail "Release $($release.tag_name) ($($release.published_at)), préversion : $($release.prerelease) ; installateur : $($asset.name) ($($asset.size) o)"
Test-Cas "release officielle obsproject/obs-studio $($release.tag_name), pas une préversion, avec son installateur Windows" { -not $release.prerelease -and -not $release.draft -and [bool]$asset }
if (-not $asset) { exit 1 }
$Installateur = Join-Path $Racine "installateur\$($asset.name)"
New-Item -ItemType Directory -Force -Path (Split-Path $Installateur) | Out-Null
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $Installateur -UseBasicParsing
$sig = Get-AuthenticodeSignature -LiteralPath $Installateur
$sha = (Get-FileHash -LiteralPath $Installateur -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Detail "Installateur : $((Get-Item $Installateur).Length) o, SHA-256 $sha, signature $($sig.Status), signé par $($sig.SignerCertificate.Subject)"
Test-Cas 'installateur signé Authenticode par OBS Project, signature valide' { $asset.browser_download_url -like 'https://github.com/obsproject/obs-studio/releases/download/*' -and $sig.Status -eq 'Valid' -and $sig.SignerCertificate.Subject -match 'OBS Project' }
if ($asset.digest) { Test-Cas "installateur identique à celui de la release ($($asset.digest))" { $asset.digest -eq "sha256:$sha" } } else { Write-Note 'la release ne donne pas d''empreinte pour ce fichier (seule la signature est vérifiée)' }
$p = Start-Process -FilePath $Installateur -ArgumentList '/S' -PassThru -Wait
Write-Detail "Installateur fini (code $($p.ExitCode))"
$cle = (Get-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\OBS Studio' -ErrorAction SilentlyContinue).'(default)'
foreach ($c in @($cle, "$env:ProgramFiles\obs-studio")) {
    if ($c -and (Test-Path -LiteralPath (Join-Path $c 'bin\64bit\obs64.exe'))) { $script:Installe = $c; break }
}
Write-Detail "Clé de l'installateur (HKLM\SOFTWARE\OBS Studio) : « $cle »"
Test-Cas "OBS installé ($script:Installe)" { [bool]$script:Installe }
if (-not $script:Installe) { exit 1 }
$VersionObs = (Get-Item (Join-Path $script:Installe 'bin\64bit\obs64.exe')).VersionInfo.ProductVersion
if ("$VersionObs" -match '^(\d+)') { $script:Majeure = [int]$Matches[1] }
Write-Note "Version d'OBS Studio essayée : $VersionObs"
if (@(Get-ProcessusObs).Count) { [void](Stop-Obs 'après installation') }

Write-Partie '2. Avant le premier lancement : Nevermind trouve OBS, qui n''a pas encore créé ses réglages'
# (premier lancement sans l'assistant de configuration automatique, et obs-websocket allumé, sans mot de passe, pour
# l'essai seulement : ce que la machine jetable de GitHub permet)
$general = "[General]`r`nFirstRun=true`r`n"
$ws = "[OBSWebSocket]`r`nFirstLoad=false`r`nServerEnabled=true`r`nServerPort=$Port`r`nAlertsEnabled=false`r`nAuthRequired=false`r`n"
Write-Texte (Join-Path $Reglages 'global.ini') ($general + $ws)
if ($script:Majeure -ge 31) { Write-Texte (Join-Path $Reglages 'user.ini') $general }
Write-Texte (Join-Path $Reglages 'plugin_config\obs-websocket\config.json') "{`"alerts_enabled`": false, `"auth_required`": false, `"first_load`": false, `"server_enabled`": true, `"server_password`": `"essai-nevermind`", `"server_port`": $Port}"
$j = Invoke-Nevermind @('etat')
Test-Cas 'la page dit « Ouvre OBS une première fois »' { $j.page.module.titre -eq 'Ouvre OBS une première fois' }

Write-Partie '3. Premier lancement : OBS crée ses réglages ; des réglages « à soi » (50 images par seconde, 3 500 kb/s, un dossier de vidéos)'
$lance = Start-Obs 'premier'
Test-Cas 'OBS démarre sans assistant et obs-websocket répond' { $lance }
if (-not $lance) { [void](Stop-Obs 'premier'); Show-Fichiers 'premier lancement raté'; Write-Host 'OBS ne démarre pas : essai arrêté.' -ForegroundColor Red; exit 1 }
$v = Invoke-Obs 'GetVersion'
Write-Detail "OBS $($v.obsVersion), obs-websocket $($v.obsWebSocketVersion), plateforme $($v.platformDescription)"
[void](Invoke-Obs 'SetVideoSettings' @{ fpsNumerator = 50; fpsDenominator = 1 })
[void](Invoke-Obs 'SetProfileParameter' @{ parameterCategory = 'SimpleOutput'; parameterName = 'VBitrate'; parameterValue = '3500' })
[void](Invoke-Obs 'SetProfileParameter' @{ parameterCategory = 'SimpleOutput'; parameterName = 'FilePath'; parameterValue = $MonDossier })
Start-Sleep -Seconds 1
$Avant = Get-Pris
Test-Cas 'réglages de départ pris par OBS : 50 images par seconde, 3 500 kb/s, mode Simple' { $Avant['Video.fpsNumerator'] -eq '50' -and (Get-Valeur $Avant 'SimpleOutput.VBitrate') -eq '3500' -and (Get-Valeur $Avant 'Output.Mode') -eq 'Simple' }

Write-Partie '4. « Optimiser » pendant qu''OBS est ouvert : refusé, rien de touché'
$e1 = Get-Empreintes
$j = Invoke-Nevermind @('regler', '3', 'twitch')
$e2 = Get-Empreintes
$Profil = [string]$j.plan.profil
Test-Cas 'refus « Ferme OBS puis reclique sur Optimiser »' { @($j.annonces) -contains 'Ferme OBS puis reclique sur Optimiser' }
Test-Cas 'aucune copie faite' { -not $j.copie }
$d = @(Get-Changes $e1 $e2); foreach ($x in $d) { Write-Detail "changé : $x ($($e1[$x]) -> $($e2[$x]))" }
Test-Cas 'aucun fichier de réglages d''OBS changé' { $d.Count -eq 0 }
[void](Stop-Obs 'premier')
Show-Fichiers 'après le premier lancement'
$j = Invoke-Nevermind @('etat')
Test-Cas "OBS fermé, la page dit « Prêt à régler OBS » (profil « $Profil »)" { $j.page.module.titre -eq 'Prêt à régler OBS' }

Write-Partie '5. « Optimiser » OBS fermé (Twitch, 3 Mb/s) ; OBS relancé : ce qu''il a pris'
$e1 = Get-Empreintes
$j = Invoke-Nevermind @('regler', '3', 'twitch')
$Plan = $j.plan
$e2 = Get-Empreintes
foreach ($x in @(Get-Changes $e1 $e2)) { Write-Detail "fichier changé par Nevermind : $x" }
Test-Cas 'annonce « OBS est réglé : ... »' { @($j.annonces | Where-Object { $_ -like 'OBS est réglé : *' }).Count -eq 1 }
Test-Cas 'une copie des anciens réglages est faite' { $null -ne $j.copie }
Show-Fichiers 'après « Optimiser »'
[void](Start-Obs 'relance-optimise')
$Pris = Get-Pris
Test-Plan 'Twitch, relu par OBS' $Pris $Plan 'Simple'
Test-Cas 'service de stream, serveur et clé pas touchés' { @(Get-Differences $Avant $Pris | Where-Object { $_ -like 'Service.*' }).Count -eq 0 }
$rec = Invoke-Enregistrement 'twitch'
Test-Cas "les enregistrements vont toujours dans « $MonDossier »" { $rec.chemin -like "$MonDossier\*" }

Write-Partie '6. L''utilisateur change lui-même son débit (2 222 kb/s) ; « Remettre comme avant » le garde, et remet le reste'
[void](Invoke-Obs 'SetProfileParameter' @{ parameterCategory = 'SimpleOutput'; parameterName = 'VBitrate'; parameterValue = '2222' })
$e1 = Get-Empreintes
$j = Invoke-Nevermind @('remettre')
$e2 = Get-Empreintes
Test-Cas 'OBS ouvert : refus « Ferme OBS puis reclique sur Remettre comme avant », rien de touché, copie gardée' { @($j.annonces) -contains 'Ferme OBS puis reclique sur Remettre comme avant' -and (Get-Changes $e1 $e2).Count -eq 0 -and $null -ne $j.copie }
[void](Stop-Obs 'avant-remettre')
Test-Encodeur 'Twitch' (Get-Encodeur $rec.journal) $Plan
$j = Invoke-Nevermind @('remettre')
Test-Cas 'annonce « Les réglages d''avant sont revenus dans OBS. Ce que tu as changé toi-même depuis est gardé. »' { @($j.annonces) -contains 'Les réglages d''avant sont revenus dans OBS. Ce que tu as changé toi-même depuis est gardé.' }
Test-Cas 'la copie est effacée' { -not $j.copie }
Show-Fichiers 'après « Remettre comme avant »'
[void](Start-Obs 'relance-remis')
$Remis = Get-Pris
$d = @(Get-Differences $Avant $Remis); foreach ($x in $d) { Write-Detail "différent d'avant : $x" }
Test-Cas 'tout est revenu comme avant, sauf le débit changé par l''utilisateur (2 222 kb/s gardé)' { $d.Count -eq 1 -and $d[0] -like 'SimpleOutput.VBitrate :*' -and (Get-Valeur $Remis 'SimpleOutput.VBitrate') -eq '2222' }
# (l'utilisateur remet son débit : le point de départ des cas suivants)
[void](Invoke-Obs 'SetProfileParameter' @{ parameterCategory = 'SimpleOutput'; parameterName = 'VBitrate'; parameterValue = '3500' })
[void](Stop-Obs 'remis')

Write-Partie '7. Kick (8 Mb/s) : mode Avancé, une image clé toutes les 2 s, les enregistrements au même endroit ; puis tout revient'
$j = Invoke-Nevermind @('regler', '8', 'kick')
$PlanK = $j.plan
Test-Cas 'annonce « OBS est réglé : ... » (Kick)' { @($j.annonces | Where-Object { $_ -like 'OBS est réglé : *' }).Count -eq 1 }
Show-Fichiers 'après « Optimiser » pour Kick'
[void](Start-Obs 'relance-kick')
$pk = Get-Pris
Test-Plan 'Kick, relu par OBS' $pk $PlanK 'Advanced'
Test-Cas "Kick : le dossier des enregistrements du mode Avancé est « $MonDossier »" { (Get-Valeur $pk 'AdvOut.RecFilePath') -eq $MonDossier }
Test-Cas 'Kick : le format des enregistrements reste le même' { (Get-Valeur $pk 'AdvOut.RecFormat2') -eq (Get-Valeur $Avant 'SimpleOutput.RecFormat2') }
$rec = Invoke-Enregistrement 'kick'
Test-Cas "Kick : la vidéo enregistrée est dans « $MonDossier »" { $rec.chemin -like "$MonDossier\*" }
[void](Stop-Obs 'kick')
Test-Encodeur 'Kick' (Get-Encodeur $rec.journal) $PlanK 2
$j = Invoke-Nevermind @('remettre')
Test-Cas 'Kick : annonce « Les réglages d''avant sont revenus dans OBS. »' { @($j.annonces) -contains 'Les réglages d''avant sont revenus dans OBS.' }
[void](Start-Obs 'relance-kick-remis')
$d = @(Get-Differences $Avant (Get-Pris)); foreach ($x in $d) { Write-Detail "différent d'avant : $x" }
Test-Cas 'Kick : tout est revenu comme avant (mode Simple, vidéo, débit)' { $d.Count -eq 0 }

Write-Partie '8. Un autre profil que celui par défaut : c''est lui qui est réglé, puis remis ; l''autre n''est pas touché'
$NomProfil = 'Mon profil de stream'
[void](Invoke-Obs 'CreateProfile' @{ profileName = $NomProfil })
[void](Wait-Jusque { (Invoke-Obs 'GetProfileList' -Silencieux).currentProfileName -eq $NomProfil } 15)
[void](Invoke-Obs 'SetVideoSettings' @{ fpsNumerator = 24; fpsDenominator = 1 })
Start-Sleep -Seconds 1
$AvantP = Get-Pris
Test-Cas "profil « $NomProfil » ouvert dans OBS, à 24 images par seconde" { $AvantP['Profil'] -eq $NomProfil -and $AvantP['Video.fpsNumerator'] -eq '24' }
[void](Stop-Obs 'profil')
$e1 = Get-Empreintes
$j = Invoke-Nevermind @('regler', '6', 'twitch')
$PlanP = $j.plan
$e2 = Get-Empreintes
$d = @(Get-Changes $e1 $e2); foreach ($x in $d) { Write-Detail "fichier changé par Nevermind : $x" }
Test-Cas "Nevermind règle le profil « $NomProfil »" { $PlanP.profil -eq $NomProfil -and @($j.annonces | Where-Object { $_ -like 'OBS est réglé : *' }).Count -eq 1 }
Test-Cas 'le profil par défaut n''est pas touché (seuls les fichiers du profil réglé changent)' { $d.Count -gt 0 -and @($d | Where-Object { $_ -notlike "basic\profiles\$(Split-Path -Leaf $PlanP.dossier_profil)\*" }).Count -eq 0 }
[void](Start-Obs 'relance-profil')
$pp = Get-Pris
Test-Cas "OBS rouvre « $NomProfil »" { $pp['Profil'] -eq $NomProfil }
Test-Plan 'autre profil, relu par OBS' $pp $PlanP 'Simple'
[void](Stop-Obs 'profil-optimise')
$j = Invoke-Nevermind @('remettre')
Test-Cas 'autre profil : annonce « Les réglages d''avant sont revenus dans OBS. »' { @($j.annonces) -contains 'Les réglages d''avant sont revenus dans OBS.' }
[void](Start-Obs 'relance-profil-remis')
$d = @(Get-Differences $AvantP (Get-Pris)); foreach ($x in $d) { Write-Detail "différent d'avant : $x" }
Test-Cas 'autre profil : tout est revenu comme avant (24 images par seconde)' { $d.Count -eq 0 }
[void](Stop-Obs 'fin')

Write-Partie 'Bilan'
Write-Host "OBS Studio $VersionObs : $($script:Reussis) réussi(s), $($script:Echoues.Count) échoué(s)"
foreach ($e in $script:Echoues) { Write-Host "  ÉCHEC  $e" -ForegroundColor Red }
if ($script:Echoues.Count) { exit 1 }
exit 0
