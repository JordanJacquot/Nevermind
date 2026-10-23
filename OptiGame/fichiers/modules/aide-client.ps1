# Nevermind : demander à l'Aide ce que Nevermind ne peut pas faire sans les droits administrateur.
# Chargé par OptiGame.ps1 après systeme.ps1 (Get-AppExe) et aide-actions.ps1.
#
# L'Aide (NevermindAide.ps1) est lancée par la tâche planifiée « \Mindset\Nevermind (aide) », que Mindset crée quand il
# installe Nevermind : pas de demande de Windows. Elle ne fait que ce qui est dans aide-actions.ps1.
# Tant que Nevermind tourne lui-même en administrateur, il fait la demande lui-même (même liste stricte) quand l'Aide ne
# répond pas : une installation dont la tâche manque continue de marcher.

$AideTache = '\Mindset\Nevermind (aide)'
$AideFichierActions = Join-Path $ModulesDir 'aide-actions.ps1'
$script:EstAdmin = (New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
# Seule l'Aide de cette copie de Nevermind est écoutée (vérifié par AideClient à chaque échange)
[AideClient]::ExeAttendu = Join-Path (Get-AppRoot) 'NevermindAide.exe'
[AideClient]::Suffixe = Get-AideSuffixe (Get-AppRoot)
# La copie de test ne réveille jamais la vraie Aide (celle de la copie installée)
if ($env:OPTIGAME_TEST) { $script:AideSansTache = $true }

# En arrière plan : réveille l'Aide si besoin (sa tâche planifiée), envoie la demande, rapporte la réponse.
# Sans réponse et avec les droits administrateur, Nevermind fait la demande lui-même.
$AideTravail = {
    param($x)
    $rep = $null; $voie = ''; $erreur = ''
    try {
        if ($x.SurPlace) { throw 'désinstallation' }
        if (-not [AideClient]::Joignable(200) -and $x.Lancer) {
            $null = & "$env:SystemRoot\System32\schtasks.exe" /Run /TN $x.Tache 2>&1
            if ($LASTEXITCODE -eq 0) {
                $fin = (Get-Date).AddSeconds(20)
                while ((Get-Date) -lt $fin -and -not [AideClient]::Joignable(200)) { Start-Sleep -Milliseconds 150 }
            } else { $erreur = 'pas de tâche pour l''Aide' }
        }
        if ([AideClient]::Joignable(200)) { $rep = [AideClient]::Appeler($x.Demande, 3000, $x.Delai); $voie = 'aide' }
        elseif (-not $erreur) { $erreur = 'l''Aide ne répond pas' }
    } catch {
        $erreur = if ($_.Exception.InnerException) { $_.Exception.InnerException.Message } else { $_.Exception.Message }
    }
    if (-not $rep -and $x.Admin) {
        $voie = 'local'
        try {
            . $x.Actions
            $d = $x.Demande | ConvertFrom-Json
            $r = Invoke-AideAction ([string]$d.a) $d.p
            $rep = ConvertTo-Json -InputObject @{ ok = $true; r = $r } -Depth 8 -Compress
        } catch { $rep = ConvertTo-Json -InputObject @{ ok = $false; e = $_.Exception.Message } -Compress }
    }
    @{ Reponse = $rep; Voie = $voie; Erreur = $erreur }
}

# Fait faire une action de la liste de l'Aide et renvoie son résultat. Une demande refusée ou ratée lève une erreur
# avec le message de l'Aide, comme le faisait la commande de Windows avant.
function Invoke-Aide([string]$Action, $Parametres = @{}, [int]$Delai = 120000) {
    $x = @{
        Demande = (ConvertTo-Json -InputObject @{ a = $Action; p = $Parametres } -Depth 8 -Compress)
        Tache = $AideTache; Lancer = -not $script:AideSansTache; Admin = $script:EstAdmin; Actions = $AideFichierActions; Delai = $Delai
        SurPlace = -not (Get-Command Wait-Handle -ErrorAction SilentlyContinue)
    }
    # Pendant la désinstallation (en administrateur, sans fenêtre ni Invoke-Async), Nevermind fait tout lui-même sur place :
    # réveiller l'Aide la laisserait tenir Nevermind.exe, qu'il faut supprimer
    $o = if (-not $x.SurPlace) { Invoke-Async $AideTravail $x | Select-Object -First 1 } else { & $AideTravail $x }
    if ($o.Erreur -like 'pas de tâche*') { $script:AideSansTache = $true }
    if ($o.Voie -ne $script:AideVoie) {
        $script:AideVoie = $o.Voie
        Write-Log "Aide : $(switch ($o.Voie) { 'aide' { 'les actions administrateur passent par l''Aide' } 'local' { "Nevermind fait lui-même les actions administrateur ($($o.Erreur))" } default { "indisponible ($($o.Erreur))" } })"
    }
    if (-not $o.Reponse) {
        Write-Log "Aide : $Action impossible ($($o.Erreur))"
        throw "Nevermind a besoin de son Aide pour ça, mais elle ne répond pas ($($o.Erreur)). Ouvre Mindset et mets Nevermind à jour."
    }
    $j = $o.Reponse | ConvertFrom-Json
    if (-not $j.ok) { Write-Log "Aide : $Action refusé ou raté : $($j.e)"; throw [string]$j.e }
    $j.r
}

# Réveille l'Aide au lancement (en arrière plan) : la première action ne l'attend pas
function Start-AideFond {
    if (-not (Get-Command Wait-Handle -ErrorAction SilentlyContinue) -or $script:AideSansTache) { return }
    $ps = [PowerShell]::Create()
    if ($script:BgPool) { $ps.RunspacePool = $script:BgPool }
    [void]$ps.AddScript({
        param($t)
        if ([AideClient]::Joignable(50)) { return }
        $null = & "$env:SystemRoot\System32\schtasks.exe" /Run /TN $t 2>&1
    }.ToString()).AddArgument($AideTache)
    [void]$ps.BeginInvoke()
}

# ---------------------------------------------------------------------------
# Lectures réservées aux administrateurs : l'Aide complète ce que Nevermind n'a pas pu lire lui-même.
# En administrateur, Nevermind garde ses propres valeurs ; si l'Aide tourne déjà, il note ce qu'elle a vu (pour comparer).
# ---------------------------------------------------------------------------
function Get-AideLecture([string]$Action, $Parametres = @{}) {
    if ($script:EstAdmin -and -not [AideClient]::Joignable(1)) { return $null }
    try { Invoke-Aide $Action $Parametres 60000 } catch { $null }
}

# Analyse du PC ($AnalysisDataWork) : Secure Boot, puce TPM et santé des disques
function Add-AideLecturePc($Data) {
    if (-not $Data) { return }
    $l = Get-AideLecture 'lecture.pc'
    if (-not $l) { return }
    $faits = @()
    if ($null -eq $Data.SecureBoot -and $null -ne $l.SecureBoot) { $Data.SecureBoot = [bool]$l.SecureBoot; $faits += 'Secure Boot' }
    if (-not $Data.TpmOk -and $l.TpmOk) { $Data.TpmOk = $true; $Data.Tpm = $l.Tpm; $faits += 'TPM' }
    $n = 0
    foreach ($dd in @($Data.Disks)) {
        if (-not $dd -or $dd.Rel) { continue }
        $f = $l.Fiabilite.PSObject.Properties[[string]$dd.Disk.DeviceId]
        if ($f) { $dd.Rel = $f.Value; $n++ }
    }
    if ($n) { $faits += "santé de $n disque(s)" }
    $vu = "Secure Boot $($l.SecureBoot), TPM $($l.TpmOk), $(@($l.Fiabilite.PSObject.Properties).Count) disque(s)"
    Write-Log "Aide : lecture du PC $(if ($faits.Count) { "complétée ($($faits -join ', '))" } else { "reçue, rien à compléter ($vu)" })"
}

# Santé d'un disque (tuile des tests) : l'objet de Get-StorageReliabilityCounter, ou $null
function Get-AideFiabiliteDisque([string]$Id) {
    $l = Get-AideLecture 'lecture.pc'
    if (-not $l) { return $null }
    $f = $l.Fiabilite.PSObject.Properties[$Id]
    if ($f) { $f.Value } else { $null }
}

# Page Sécurité ($SecDataWork) : exclusions, menaces et détections de l'antivirus
function Add-AideLectureSecurite($D) {
    if (-not $D) { return }
    $l = Get-AideLecture 'lecture.securite'
    if (-not $l) { return }
    if ($script:EstAdmin) {
        Write-Log "Aide : lecture de l'antivirus reçue ($(@($l.Exclusions | Where-Object { $_ }).Count) exclusion(s), $(@($l.Menaces | Where-Object { $_ }).Count) menace(s), $(@($l.Detections | Where-Object { $_ }).Count) détection(s) ; Nevermind en a lu $(@($D.Exclusions).Count), $(@($D.Threats).Count), $(@($D.Detections).Count))"
        return
    }
    $D.Exclusions = @($l.Exclusions | Where-Object { $_ })
    $D.Threats = @(@($l.Menaces) | Where-Object { $_ } | ForEach-Object { [pscustomobject]@{ ThreatID = $_.ThreatID; ThreatName = $_.ThreatName; SeverityID = $_.SeverityID; IsActive = [bool]$_.IsActive } })
    $D.Active = @($D.Threats | Where-Object { $_.IsActive })
    $D.Detections = @(@($l.Detections) | Where-Object { $_ } | ForEach-Object { [pscustomobject]@{ ThreatID = $_.ThreatID; ThreatStatusID = $_.ThreatStatusID; InitialDetectionTime = [datetime][long]$_.Quand; Resources = @($_.Resources) } })
    # Tâches planifiées : l'Aide les voit toutes ; mêmes contrôles que $SecDataWork (commande cachée, programme dans un
    # dossier temporaire ou public)
    try {
        $tmp = $env:TEMP; $pub = "$env:SystemDrive\Users\Public"
        $D.Tasks = @(foreach ($t2 in @(Invoke-Aide 'lecture.taches' @{} 60000 | Where-Object { $_ })) {
            $exe = [Environment]::ExpandEnvironmentVariables([string]$t2.Exe).Trim('"')
            $argsTxt = [string]$t2.Args
            # L'installateur de Mindset (la suite) : sa tâche exacte, et son script que seul un administrateur peut modifier (ni le fichier
            # ni son dossier ne donnent de droit d'écriture à Tout le monde, aux Utilisateurs ou à l'utilisateur lui-même)
            $protege = { param($f) try { $sids = @('S-1-1-0', 'S-1-5-11', 'S-1-5-32-545', [Security.Principal.WindowsIdentity]::GetCurrent().User.Value)
                -not @(foreach ($x in $f, (Split-Path $f -Parent)) { (Get-Acl -LiteralPath $x).Access | Where-Object { $_.AccessControlType -eq 'Allow' -and ([int64]$_.FileSystemRights -band 0xD0046) -and
                    ($sids -contains $(try { $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value } catch { '' })) } }).Count } catch { $false } }
            if ([string]$t2.Chemin -eq '\Mindset\' -and [string]$t2.Nom -eq 'Mindset (installation)' -and $exe -match '\\System32\\WindowsPowerShell\\v1\.0\\powershell\.exe$' -and
                $argsTxt -match '^-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "([^"]+\\fichiers\\installer-admin\.ps1)" -Action demande -Demande "[^"]+\\installation\.demande"$' -and (Test-Path -LiteralPath $Matches[1]) -and (& $protege $Matches[1])) { continue }
            $cachee = $exe -match '(powershell|pwsh|cmd|wscript|cscript|mshta)(\.exe)?$' -and $argsTxt -match '(-enc|-encodedcommand|frombase64|downloadstring|downloadfile|invoke-expression|\biex\b|-w(indowstyle)?\s+h(idden)?|http)'
            $risque = $exe -like "$tmp*" -or $exe -like "$pub*"
            if ($cachee -or $risque) { @{ Name = [string]$t2.Nom; Path = [string]$t2.Chemin; Cmd = "$exe $argsTxt".Trim(); Bad = $cachee } }
        })
    } catch { Write-Log "Aide : tâches planifiées non lues ($($_.Exception.Message))" }
}

# ---------------------------------------------------------------------------
# Mesures en continu (FPS, compteurs du trafic, trace réseau du lag) : par l'Aide quand elle répond, sinon sur place.
# Sans droits, l'Aide est réveillée au besoin (Invoke-Aide) ; en administrateur, Nevermind ne l'attend pas.
# ---------------------------------------------------------------------------
function Test-MesuresAide {
    if ([AideClient]::Joignable(1)) { return $true }
    if ($script:EstAdmin) { return $false }
    # (sans réponse, nouvel essai dans une minute : Trafic demande toutes les 2 s)
    if ($script:MesuresAideEchec -and ((Get-Date) - $script:MesuresAideEchec).TotalSeconds -lt 60) { return $false }
    try { [void](Invoke-Aide 'aide.bonjour' @{} 10000); $script:MesuresAideEchec = $null; return $true } catch { $script:MesuresAideEchec = Get-Date; return $false }
}

# FPS d'un jeu (FrameMon) : $true si la mesure a démarré
function Start-FpsMesure([int]$ProcId) {
    if (Test-MesuresAide) {
        if ([FrameMon]::StartRemote($ProcId)) { Write-Log "Mesure des FPS : par l'Aide$(if ([FrameMon]::Attached) { ', reprise de la veille lancée avant le jeu' })"; return $true }
        Write-Log "Mesure des FPS par l'Aide impossible : $([FrameMon]::LastError)"
    }
    # Repli sur place (en administrateur si Nevermind l'est) : même contrôle que l'Aide (Get-AidePresentMon), avant chaque lancement
    $raison = Get-RaisonRefusPresentMon $PresentMonExe
    if ($raison) {
        [FrameMon]::LastError = "PresentMon refusé : $raison"
        Write-Log "Mesure des FPS : PresentMon n'est pas lancé, $raison ($PresentMonExe)"
        Set-Status 'L''outil de mesure des FPS a été modifié : Nevermind ne le lance pas. Réinstalle Nevermind depuis Mindset.'
        return $false
    }
    [FrameMon]::Start($PresentMonExe, $ProcId)
}

# PresentMon (Intel) n'est lancé que s'il est bien signé par Intel (voir outils-tiers\SOURCES.md) : $null si c'est bon,
# sinon la raison du refus (pour le journal). Un exe remplacé, même signé par un autre éditeur, est refusé.
function Get-RaisonRefusPresentMon([string]$Exe) {
    if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) { return 'fichier absent' }
    try { $s = Get-AuthenticodeSignature -LiteralPath $Exe -ErrorAction Stop } catch { return "signature illisible ($($_.Exception.Message))" }
    if ($s.Status -ne 'Valid') { return "signature non valable ($($s.Status))" }
    if ($s.SignerCertificate.Subject -notmatch 'CN=Intel Corporation(,|$)') { return "pas signé par Intel ($($s.SignerCertificate.Subject))" }
    $null
}

# Jeux dont l'anti-triche refuse une mesure démarrée après leur lancement (BF6 et EA Javelin), mais laisse tourner
# celle qui existait avant : l'Aide lance PresentMon en veille sur le nom de leur programme, et la mesure du jeu la
# reprend (fps.demarrer répond « rattachee »). Même chose pour la trace réseau de « Lag en ligne » (flux.veiller), qui
# montre le vrai serveur de la partie. Nevermind retient aussi les jeux qu'il voit refuser (réglage FpsVeille).
$FpsVeilleConnus = @('bf6.exe')
function Get-FpsVeilleJeux { @(@($FpsVeilleConnus) + @(Get-Setting 'FpsVeille' @()) | Where-Object { $_ } | ForEach-Object { ([string]$_).ToLowerInvariant() } | Select-Object -Unique) }

function Add-FpsVeilleJeu([string]$Exe) {
    $e = $Exe.ToLowerInvariant()
    if ($e -notlike '*.exe') { $e += '.exe' }
    if ((Get-FpsVeilleJeux) -contains $e) { return }
    Set-Setting 'FpsVeille' @(@(Get-Setting 'FpsVeille' @()) + $e)
    Write-Log "Mesure des FPS : $e refuse une mesure démarrée après lui, elle sera lancée avant lui la prochaine fois"
}

# Refus de Windows (anti-triche) : message de PresentMon (« access denied... Performance Log Users ») ou de l'Aide
function Test-FpsRefus([string]$Err) { $Err -match 'access denied|accès refusé|Performance Log Users' }

# Hors partie, toutes les 5 s (Test-GameRunning) : veille demandée à l'Aide au plus toutes les 30 s (elle ne relance
# rien si elle guette déjà les mêmes jeux). Pas pour un jeu déjà ouvert : la mesure serait refusée.
function Update-FpsVeille {
    $v = $script:FpsVeilleTache
    if ($v -and $v.H.IsCompleted) {
        $script:FpsVeilleTache = $null
        $r = try { [string]@($v.PS.EndInvoke($v.H))[0] } catch { $_.Exception.Message } finally { $v.PS.Dispose() }
        $etat = if ($r) { "veille impossible ($r)" } else { "en veille avant $($v.Noms -join ', ') ($($v.Quoi))" }
        if ($etat -ne $script:FpsVeilleEtat) { $script:FpsVeilleEtat = $etat; Write-Log "Mesure des FPS : $etat" }
    }
    $fps = (Test-FpsMeasure) -and -not $script:FpsTarget
    $flux = (Test-LagMeasure) -and -not $script:LagSession
    if ($script:FpsVeilleTache -or -not ($fps -or $flux)) { return }
    if ($script:FpsVeilleDemande -and ((Get-Date) - $script:FpsVeilleDemande).TotalSeconds -lt 30) { return }
    $script:FpsVeilleDemande = Get-Date
    $noms = @(Get-FpsVeilleJeux | Where-Object { -not [Diagnostics.Process]::GetProcessesByName(($_ -replace '\.exe$', '')).Length })
    if (-not $noms.Count -or -not [AideClient]::Joignable(1)) { return }
    $ps = [PowerShell]::Create()
    if ($script:BgPool) { $ps.RunspacePool = $script:BgPool }
    [void]$ps.AddScript({
        param($j, $fps, $flux)
        $err = @()
        if ($fps) { try { [void][AideClient]::Texte('fps.veiller', $j, 15000) } catch { $err += "FPS : $($_.Exception.Message)" } }
        if ($flux) { try { [void][AideClient]::Texte('flux.veiller', '{}', 15000) } catch { $err += "trace réseau : $($_.Exception.Message)" } }
        $err -join ' ; '
    }.ToString()).AddArgument((ConvertTo-Json -InputObject @{ Noms = $noms } -Compress)).AddArgument($fps).AddArgument($flux)
    $script:FpsVeilleTache = @{ PS = $ps; H = $ps.BeginInvoke(); Noms = $noms; Quoi = @($(if ($fps) { 'FPS' }), $(if ($flux) { 'trace réseau' }) | Where-Object { $_ }) -join ' et ' }
}

# Connexions TCP avec leurs octets (Trafic, lag) ; $script:TrafficCountersOk : Windows compte-t-il les octets ?
function Get-TrafficTcp {
    if (Test-MesuresAide) {
        try {
            $r = Invoke-Aide 'trafic.tcp' @{} 15000
            $script:TrafficCountersOk = [bool]$r.CompteursOk
            if (-not $script:TrafficAideNote) { $script:TrafficAideNote = $true; Write-Log "Trafic : compteurs lus par l'Aide (comptage $(if ($r.CompteursOk) { 'actif' } else { 'refusé' }))" }
            return @($r.Lignes | Where-Object { $_ } | ForEach-Object { [string]$_ })
        } catch { Write-Log "Trafic : compteurs non lus par l'Aide ($($_.Exception.Message))" }
    }
    $l = @([TrafficMon]::Tcp())
    $script:TrafficCountersOk = [bool][TrafficMon]::CountersOk
    $l
}

# Trace réseau (lag en ligne) : 'aide', 'local' ou '' si elle n'a pas démarré
function Start-FluxReseau {
    # (refus de l'Aide, en administrateur : seul celui là peut venir d'un anti-triche)
    $script:FluxErreur = ''
    if (Test-MesuresAide) {
        try { [void](Invoke-Aide 'flux.demarrer' @{} 20000); Write-Log "Lag : trace réseau par l'Aide"; return 'aide' }
        catch { $script:FluxErreur = $_.Exception.Message; Write-Log "Lag : trace réseau par l'Aide impossible ($($_.Exception.Message))" }
    }
    try { if ([NetFlow]::Start()) { return 'local' }; Write-Log "Lag: repérage du serveur limité ($([NetFlow]::LastError))" } catch { Write-Log "Lag: $_" }
    ''
}
function Get-FluxReseau([string]$Ou) {
    if ($Ou -eq 'aide') { return @(Invoke-Aide 'flux.lire' @{} 15000 | Where-Object { $_ } | ForEach-Object { [string]$_ }) }
    if ($Ou -eq 'local') { return @([NetFlow]::Snapshot()) }
    @()
}
function Stop-FluxReseau([string]$Ou) {
    if ($Ou -eq 'aide') { try { [void](Invoke-Aide 'flux.arreter' @{} 15000) } catch {} }
    elseif ($Ou -eq 'local') { try { [NetFlow]::Stop() } catch {} }
}

# Ferme tous les programmes d'un nom : ceux de l'utilisateur sur place, ceux lancés en administrateur par l'Aide
function Stop-ProgrammeNom([string]$Nom) {
    $reste = 0
    foreach ($p in @(Get-Process -Name $Nom -ErrorAction SilentlyContinue)) { try { $p.Kill() } catch { $reste++ } }
    if ($reste) { [void](Invoke-Aide 'processus.fermer' @{ Nom = $Nom }) }
}

# Même chose pour un programme précis (son chemin). Sans droits, le chemin d'un programme lancé en administrateur est
# illisible : ceux-là aussi passent par l'Aide.
function Stop-ProgrammeChemin([string]$Chemin) {
    $reste = 0
    foreach ($p in @(Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($Chemin)) -ErrorAction SilentlyContinue)) {
        $pc = $null; try { $pc = $p.Path } catch {}
        if (-not $pc) { $reste++; continue }
        if ($pc -ne $Chemin) { continue }
        try { $p.Kill() } catch { $reste++ }
    }
    if ($reste) { [void](Invoke-Aide 'processus.fermer' @{ Chemin = $Chemin }) }
}
