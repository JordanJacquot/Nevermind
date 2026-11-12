# Nevermind : le mode streameur essayé sur un VRAI Streamlabs Desktop, sur la machine jetable de GitHub Actions
# (workflow .github\workflows\streamlabs-reel.yml). Le vrai installateur officiel (streamlabs.com), installé en silence,
# lancé pour qu'il crée ses réglages ; puis le vrai chemin « Optimiser » et « Remettre comme avant » de Nevermind (le
# bloc stream entier, par l'exemple rust\nevermind-stream\examples\essai-streamlabs.rs) ; puis Streamlabs relancé, et ce
# qu'il a VRAIMENT pris lu par sa porte officielle (son API, le tube nommé \\.\pipe\slobs, JSON-RPC).
# JAMAIS sur un vrai PC : le script refuse de tourner hors de GitHub Actions.
#
#   .\outils\essai-streamlabs-reel.ps1 -Exemple <essai-streamlabs.exe>
#
# Code de sortie 1 si un cas échoue.
param([Parameter(Mandatory)][string]$Exemple, [string]$Racine = 'C:\essai-sl')

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
    $p = Start-Process -FilePath $exe -PassThru -WorkingDirectory $script:Installe
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
    $j
}

# ---------- ce que Streamlabs a pris (par son API) ----------
function Get-Formulaire([string]$Categorie) {
    $r = Invoke-Slobs 'getSettingsFormData' 'SettingsService' @($Categorie) -Silencieux
    if ($null -eq $r) { return $null }
    $h = @{}
    foreach ($g in @($r)) { foreach ($p in @($g.parameters)) { if ($p.name) { $h[$p.name] = $p.value } } }
    $h
}
function Get-Pris {
    $v = Get-Formulaire 'Video'; $o = Get-Formulaire 'Output'
    $pris = [ordered]@{ api_video = ($null -ne $v); api_sortie = ($null -ne $o) }
    if ($v) { foreach ($k in $v.Keys) { $pris["Video.$k"] = $v[$k] } }
    if ($o) { foreach ($k in $o.Keys) { $pris["Output.$k"] = $o[$k] } }
    Write-Detail "Pris par Streamlabs : $($pris | ConvertTo-Json -Compress -Depth 6)"
    $pris
}

# =====================================================================================================================
$Version = '?'
Write-Partie '1. Le vrai Streamlabs Desktop (installateur officiel, signé), installé en silence'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$Installateur = Join-Path $Racine 'installateur\Streamlabs-Desktop-Setup.exe'
New-Item -ItemType Directory -Force -Path (Split-Path $Installateur) | Out-Null
$attendu = $null
try {
    $yml = (Invoke-WebRequest -Uri 'https://slobs-cdn.streamlabs.com/latest.yml' -UseBasicParsing).Content
    if ($yml -is [byte[]]) { $yml = [Text.Encoding]::UTF8.GetString($yml) }
    if ($yml -match '(?m)^version:\s*(\S+)') { $Version = $Matches[1] }
    if ($yml -match '(?m)^sha512:\s*(\S+)') { $attendu = $Matches[1] }
} catch { Write-Note "latest.yml illisible : $($_.Exception.Message)" }
Write-Detail "Dernière version annoncée par Streamlabs : $Version"
$req = [Net.HttpWebRequest]::Create('https://streamlabs.com/streamlabs-desktop/download')
$req.AllowAutoRedirect = $false; $req.UserAgent = 'Mozilla/5.0'
$rep = $req.GetResponse(); $lien = $rep.Headers['Location']; $rep.Close()
Write-Detail "Lien officiel : $lien"
Invoke-WebRequest -Uri $lien -OutFile $Installateur -UseBasicParsing
$sig = Get-AuthenticodeSignature -LiteralPath $Installateur
$sha = [Convert]::ToBase64String([Security.Cryptography.SHA512]::Create().ComputeHash([IO.File]::ReadAllBytes($Installateur)))
Write-Detail "Installateur : $((Get-Item $Installateur).Length) o, signature $($sig.Status), signé par $($sig.SignerCertificate.Subject)"
Test-Cas "installateur officiel ($lien) signé Authenticode, signature valide" { $lien -match '^https://slobs-cdn\.streamlabs\.com/' -and $sig.Status -eq 'Valid' -and $sig.SignerCertificate.Subject -match 'Streamlabs|General Workings|Logitech' }
Test-Cas "installateur identique à celui annoncé par latest.yml ($Version, SHA-512)" { $attendu -and $sha -eq $attendu }
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

Write-Partie '2. Premier lancement : Streamlabs crée ses réglages'
$lance = Start-Streamlabs 'premier'
Test-Cas 'Streamlabs démarre et son API répond (\\.\pipe\slobs)' { $lance }
if (-not $lance) { [void](Stop-Streamlabs 'premier'); Show-Fichiers 'premier lancement raté'; Write-Host 'Streamlabs ne démarre pas : essai arrêté.' -ForegroundColor Red; exit 1 }
foreach ($r in 'SettingsService', 'VideoSettingsService', 'StreamingService', 'StreamSettingsService', 'DualOutputService', 'VideoService', 'OutputSettingsService', 'UserService', 'OnboardingService', 'AppService') {
    Show-Api 'getModel' $r
}
Show-Api 'getCategories' 'SettingsService'
Show-Api 'getSettingsFormData' 'SettingsService' @('Video')
Show-Api 'getSettingsFormData' 'SettingsService' @('Output')
Show-Api 'getSettingsFormData' 'SettingsService' @('Stream')
$Avant = Get-Pris
[void](Stop-Streamlabs 'premier')
$EmpA = Get-Empreintes
Show-Fichiers 'après le premier lancement'

Write-Partie '3. Streamlabs ouvert pendant « Optimiser » : refusé proprement, rien de changé'
[void](Start-Streamlabs 'ouvert')
$e1 = Get-Empreintes
$j = Invoke-Nevermind @('regler', '3')
$e2 = Get-Empreintes
Test-Cas 'refus « Ferme Streamlabs puis reclique sur Optimiser »' { @($j.annonces) -contains 'Ferme Streamlabs puis reclique sur Optimiser' }
Test-Cas 'aucune copie faite, aucun fichier de Streamlabs touché par Nevermind' { -not (Test-Path -LiteralPath (Join-Path $Donnees 'streameur\streamlabs\copie.json')) -and -not $j.copie }
Compare-Empreintes $e1 $e2 'pendant le refus (Streamlabs ouvert : ses propres écritures comptent aussi)'
[void](Stop-Streamlabs 'ouvert')

Write-Partie '4. « Optimiser » Streamlabs fermé (Twitch, 3 Mb/s), puis Streamlabs relancé : ce qu''il a pris'
$EmpAvant = Get-Empreintes
$j = Invoke-Nevermind @('regler', '3', 'twitch')
$Plan = $j.plan
$EmpApres = Get-Empreintes
Compare-Empreintes $EmpAvant $EmpApres 'écrit par Nevermind'
Show-Fichiers 'juste après « Optimiser »'
Test-Cas 'Optimiser : annonce « Streamlabs est réglé »' { @($j.annonces | Where-Object { $_ -like 'Streamlabs est réglé*' }).Count -eq 1 }
[void](Start-Streamlabs 'apres-optimiser')
$Pris = Get-Pris
$sortie = "$($Plan.sortie[0])x$($Plan.sortie[1])"
Test-Cas "résolution de sortie prise par Streamlabs : $sortie (plan de Nevermind)" { [string]$Pris['Video.Output'] -eq $sortie }
Test-Cas "images par seconde prises par Streamlabs : $($Plan.ips)" { [string]$Pris['Video.FPSCommon'] -eq [string]$Plan.ips -or [string]$Pris['Video.FPSInt'] -eq [string]$Plan.ips }
Test-Cas "débit vidéo pris par Streamlabs : $($Plan.video_kbps) kb/s" { [string]$Pris['Output.VBitrate'] -eq [string]$Plan.video_kbps -or [string]$Pris['Output.bitrate'] -eq [string]$Plan.video_kbps }
[void](Stop-Streamlabs 'apres-optimiser')
Show-Fichiers 'après la relance (Streamlabs a relu, puis réécrit ?)'

Write-Partie '5. « Remettre comme avant », puis Streamlabs relancé : tout est revenu'
$j = Invoke-Nevermind @('remettre')
Show-Fichiers 'juste après « Remettre comme avant »'
[void](Start-Streamlabs 'apres-remettre')
$Remis = Get-Pris
foreach ($k in @($Avant.Keys | Where-Object { $_ -match '^(Video|Output)\.' })) {
    if ([string]($Avant[$k] | ConvertTo-Json -Compress -Depth 6) -ne [string]($Remis[$k] | ConvertTo-Json -Compress -Depth 6)) { Write-Detail "différent après remise : $k : $($Avant[$k] | ConvertTo-Json -Compress -Depth 6) -> $($Remis[$k] | ConvertTo-Json -Compress -Depth 6)" }
}
Test-Cas 'tout est revenu comme avant (Vidéo et Sortie, lus par l''API)' {
    $Avant.api_video -and @($Avant.Keys | Where-Object { $_ -match '^(Video|Output)\.' -and [string]($Avant[$_] | ConvertTo-Json -Compress -Depth 6) -ne [string]($Remis[$_] | ConvertTo-Json -Compress -Depth 6) }).Count -eq 0
}
[void](Stop-Streamlabs 'apres-remettre')

Write-Partie 'Bilan'
Write-Host "Streamlabs Desktop $Version : $($script:Reussis) réussi(s), $($script:Echoues.Count) échoué(s)"
foreach ($e in $script:Echoues) { Write-Host "  ÉCHEC  $e" -ForegroundColor Red }
if ($script:Echoues.Count) { exit 1 }
exit 0
