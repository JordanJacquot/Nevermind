# Nevermind : demander à l'Aide ce que Nevermind ne peut pas faire sans les droits administrateur.
# Chargé par OptiGame.ps1 après systeme.ps1 (Get-AppExe) et aide-actions.ps1.
#
# L'Aide (NevermindAide.ps1) est lancée par la tâche planifiée « \Mindset\Nevermind (aide) », que Mindset crée quand il
# installe Nevermind : pas de demande de Windows. Elle ne fait que ce qui est dans aide-actions.ps1.
# Cette tâche tourne en SYSTEM, pour l'utilisateur inscrit dans ses arguments (-Sid) : Nevermind ne s'en sert que si
# c'est bien elle (Get-AideEtatTache). Sinon, ou si elle ne répond pas, une action demandée par l'utilisateur passe par
# la demande normale de Windows (il clique « Oui »), JAMAIS par l'ancienne tâche « avec les droits les plus élevés »
# (jusqu'à Nevermind 1.10.0), qui reprenait l'environnement de l'utilisateur : Mindset la remplace.
# Tant que Nevermind tourne lui-même en administrateur, il fait la demande lui-même (même liste stricte) quand l'Aide ne
# répond pas : une installation dont la tâche manque continue de marcher.

$AideTache = '\Mindset\Nevermind (aide)'
$AideFichierActions = Join-Path $ModulesDir 'aide-actions.ps1'
$AideExe = Join-Path (Get-AppRoot) 'NevermindAide.exe'
$AideSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$script:EstAdmin = (New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
# Seule l'Aide de cette copie de Nevermind est écoutée (vérifié par AideClient à chaque échange)
[AideClient]::ExeAttendu = $AideExe
[AideClient]::Suffixe = Get-AideSuffixe (Get-AppRoot)
# La copie de test ne réveille jamais la vraie Aide (celle de la copie installée), et ne fait aucune demande de Windows
if ($env:OPTIGAME_TEST) { $script:AideSansTache = $true; $script:AideSansDemande = $true }
# Demande de Windows refusée : pas de nouvelle demande avant 10 minutes. Attentes (en secondes) de l'Aide lancée par sa
# tâche, puis par la demande de Windows (une fois « Oui » cliqué).
$AideDelaiRefus = 600
$AideAttenteTache = 20
$AideAttenteWindows = 30
$AideSchtasks = Join-Path ([Environment]::GetFolderPath('System')) 'schtasks.exe'
$script:AideRefus = $null

# La tâche de l'Aide, telle que Windows la décrit (XML de schtasks /Query, que l'utilisateur a le droit de lire) :
#   'systeme'  la bonne : en SYSTEM, NevermindAide.exe de ce dossier, pour cet utilisateur (-Sid), active ;
#   'autre'    une tâche à ce nom, mais pas celle-là (l'ancienne « droits les plus élevés », une tâche désactivée...) ;
#   ''         pas de tâche (ou illisible pour cet utilisateur).
function Get-AideEtatTache([string]$Xml, [string]$Exe, [string]$Sid) {
    if (-not $Xml) { return '' }
    $x = $null
    try { $x = [xml]$Xml } catch { return 'autre' }
    $compte = [string]$x.Task.Principals.Principal.UserId
    if ($compte -and $compte -notmatch '^S-1-') { try { $compte = (New-Object Security.Principal.NTAccount($compte)).Translate([Security.Principal.SecurityIdentifier]).Value } catch { $compte = '' } }
    $actions = @($x.Task.Actions.Exec)
    $active = [string]$x.Task.Settings.Enabled -ne 'false'
    if ($compte -eq 'S-1-5-18' -and $active -and $actions.Count -eq 1 -and ([string]$actions[0].Command).Trim('"') -ieq $Exe -and
        ([string]$actions[0].Arguments).Trim() -ceq "-Sid $Sid") { return 'systeme' }
    'autre'
}
function Read-AideTache([string]$Tache, [string]$Schtasks) {
    try {
        $t = @(& $Schtasks /Query /TN $Tache /XML 2>$null)
        if ($LASTEXITCODE -ne 0 -or -not $t.Count) { return '' }
        $t -join "`n"
    } catch { '' }
}

# En arrière plan : réveille l'Aide si besoin (sa tâche SYSTEM, sinon la demande de Windows si elle est permise), envoie
# la demande, rapporte la réponse. Sans réponse et avec les droits administrateur, Nevermind fait la demande lui-même.
$AideTravail = {
    param($x)
    $rep = $null; $voie = ''; $erreur = ''; $etat = $null; $refus = $false
    try {
        if ($x.SurPlace) { throw 'désinstallation' }
        if (-not [AideClient]::Joignable(200) -and $x.Lancer) {
            . ([scriptblock]::Create($x.Fonctions))
            $etat = Get-AideEtatTache (Read-AideTache $x.Tache $x.Schtasks) $x.Exe $x.Sid
            if ($etat -eq 'systeme') {
                $null = & $x.Schtasks /Run /TN $x.Tache 2>&1
                if ($LASTEXITCODE -eq 0) {
                    $fin = (Get-Date).AddSeconds($x.AttenteTache)
                    while ((Get-Date) -lt $fin -and -not [AideClient]::Joignable(200)) { Start-Sleep -Milliseconds 150 }
                    if (-not [AideClient]::Joignable(1)) { $erreur = 'la tâche de l''Aide ne répond pas' }
                } else { $erreur = 'la tâche de l''Aide n''a pas démarré' }
            } elseif ($etat -eq 'autre') { $erreur = 'ancienne tâche de l''Aide (Mindset la remplacera)' }
            else { $erreur = 'pas de tâche pour l''Aide' }
        }
        # Repli : la demande normale de Windows (l'utilisateur clique « Oui »), jamais une autre tâche
        if (-not [AideClient]::Joignable(1) -and $x.Demander) {
            try {
                Start-Process -FilePath $x.Exe -ArgumentList '-Sid', $x.Sid -Verb RunAs -WorkingDirectory ([Environment]::GetFolderPath('System')) -ErrorAction Stop | Out-Null
                $fin = (Get-Date).AddSeconds($x.AttenteWindows)
                while ((Get-Date) -lt $fin -and -not [AideClient]::Joignable(200)) { Start-Sleep -Milliseconds 150 }
                if (-not [AideClient]::Joignable(1)) { $erreur = 'l''Aide lancée par la demande de Windows ne répond pas' }
            } catch { $refus = $true; $erreur = 'demande de Windows refusée' }
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
            $r = Invoke-AideAction ([string]$d.a) $d.p -Client $PID
            $rep = ConvertTo-Json -InputObject @{ ok = $true; r = $r } -Depth 8 -Compress
        } catch { $rep = ConvertTo-Json -InputObject @{ ok = $false; e = $_.Exception.Message } -Compress }
    }
    @{ Reponse = $rep; Voie = $voie; Erreur = $erreur; Etat = $etat; Refus = $refus }
}

# Fait faire une action de la liste de l'Aide et renvoie son résultat. Une demande refusée ou ratée lève une erreur
# avec le message de l'Aide, comme le faisait la commande de Windows avant.
# -SansDemande : jamais de demande de Windows (lectures et mesures en fond : seulement l'Aide déjà lancée ou sa tâche)
function Invoke-Aide([string]$Action, $Parametres = @{}, [int]$Delai = 120000, [switch]$SansDemande) {
    $demander = -not $SansDemande -and -not $script:AideSansDemande -and -not $script:EstAdmin -and
                -not ($script:AideRefus -and ((Get-Date) - $script:AideRefus).TotalSeconds -lt $AideDelaiRefus)
    $x = @{
        Demande = (ConvertTo-Json -InputObject @{ a = $Action; p = $Parametres } -Depth 8 -Compress)
        Tache = $AideTache; Lancer = -not $script:AideSansTache; Admin = $script:EstAdmin; Actions = $AideFichierActions; Delai = $Delai
        Exe = $AideExe; Sid = $AideSid; Demander = $demander; Schtasks = $AideSchtasks; AttenteTache = $AideAttenteTache; AttenteWindows = $AideAttenteWindows
        Fonctions = "function Get-AideEtatTache {${function:Get-AideEtatTache}}`nfunction Read-AideTache {${function:Read-AideTache}}"
        SurPlace = -not (Get-Command Wait-Handle -ErrorAction SilentlyContinue)
    }
    # Pendant la désinstallation (en administrateur, sans fenêtre ni Invoke-Async), Nevermind fait tout lui-même sur place :
    # réveiller l'Aide la laisserait tenir NevermindAide.exe, qu'il faut supprimer
    $o = if (-not $x.SurPlace) { Invoke-Async $AideTravail $x | Select-Object -First 1 } else { & $AideTravail $x }
    # (tâche absente ou pas la bonne : plus de schtasks jusqu'au prochain lancement de Nevermind ; la demande de Windows reste)
    if ($o.Etat -in '', 'autre' -and $null -ne $o.Etat) { $script:AideSansTache = $true }
    if ($o.Refus) { $script:AideRefus = Get-Date }
    if ($o.Voie -ne $script:AideVoie) {
        $script:AideVoie = $o.Voie
        Write-Log "Aide : $(switch ($o.Voie) { 'aide' { "les actions administrateur passent par l'Aide$(if ($o.Erreur) { " ($($o.Erreur), demande de Windows acceptée)" })" } 'local' { "Nevermind fait lui-même les actions administrateur ($($o.Erreur))" } default { "indisponible ($($o.Erreur))" } })"
    }
    if (-not $o.Reponse) {
        Write-Log "Aide : $Action impossible ($($o.Erreur))"
        if ($o.Refus) { throw 'Nevermind a besoin de ton accord pour ça : réessaie, et clique « Oui » dans la demande de Windows.' }
        throw "Nevermind a besoin de son Aide pour ça, mais elle ne répond pas ($($o.Erreur)). Ouvre Mindset et mets Nevermind à jour."
    }
    $j = $o.Reponse | ConvertFrom-Json
    if (-not $j.ok) { Write-Log "Aide : $Action refusé ou raté : $($j.e)"; throw [string]$j.e }
    $j.r
}

# Réveille l'Aide au lancement (en arrière plan) : la première action ne l'attend pas. Seulement par sa tâche SYSTEM, si
# c'est bien elle (jamais de demande de Windows au lancement).
function Start-AideFond {
    if (-not (Get-Command Wait-Handle -ErrorAction SilentlyContinue) -or $script:AideSansTache) { return }
    $ps = [PowerShell]::Create()
    if ($script:BgPool) { $ps.RunspacePool = $script:BgPool }
    [void]$ps.AddScript({
        param($t, $f, $exe, $sid, $schtasks)
        if ([AideClient]::Joignable(50)) { return }
        . ([scriptblock]::Create($f))
        if ((Get-AideEtatTache (Read-AideTache $t $schtasks) $exe $sid) -ne 'systeme') { return }
        $null = & $schtasks /Run /TN $t 2>&1
    }.ToString()).AddArgument($AideTache).AddArgument("function Get-AideEtatTache {${function:Get-AideEtatTache}}`nfunction Read-AideTache {${function:Read-AideTache}}").AddArgument($AideExe).AddArgument($AideSid).AddArgument($AideSchtasks)
    [void]$ps.BeginInvoke()
}

# ---------------------------------------------------------------------------
# Lectures réservées aux administrateurs : l'Aide complète ce que Nevermind n'a pas pu lire lui-même.
# En administrateur, Nevermind garde ses propres valeurs ; si l'Aide tourne déjà, il note ce qu'elle a vu (pour comparer).
# ---------------------------------------------------------------------------
function Get-AideLecture([string]$Action, $Parametres = @{}) {
    if ($script:EstAdmin -and -not [AideClient]::Joignable(1)) { return $null }
    try { Invoke-Aide $Action $Parametres 60000 -SansDemande } catch { $null }
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
        $D.Tasks = @(foreach ($t2 in @(Invoke-Aide 'lecture.taches' @{} 60000 -SansDemande | Where-Object { $_ })) {
            $exe = [Environment]::ExpandEnvironmentVariables([string]$t2.Exe).Trim('"')
            $argsTxt = [string]$t2.Args
            # L'installateur de Mindset (la suite) : sa tâche exacte (en SYSTEM pour l'utilisateur -Sid depuis Mindset 2.10.0), et son script que seul un administrateur peut modifier (ni le fichier
            # ni son dossier ne donnent de droit d'écriture à Tout le monde, aux Utilisateurs ou à l'utilisateur lui-même)
            $protege = { param($f) try { $sids = @('S-1-1-0', 'S-1-5-11', 'S-1-5-32-545', [Security.Principal.WindowsIdentity]::GetCurrent().User.Value)
                -not @(foreach ($x in $f, (Split-Path $f -Parent)) { (Get-Acl -LiteralPath $x).Access | Where-Object { $_.AccessControlType -eq 'Allow' -and ([int64]$_.FileSystemRights -band 0xD0046) -and
                    ($sids -contains $(try { $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value } catch { '' })) } }).Count } catch { $false } }
            if ([string]$t2.Chemin -eq '\Mindset\' -and [string]$t2.Nom -eq 'Mindset (installation)' -and $exe -match '\\System32\\WindowsPowerShell\\v1\.0\\powershell\.exe$' -and
                $argsTxt -match '^-NoProfile (-NonInteractive )?-ExecutionPolicy Bypass -WindowStyle Hidden -File "(?<script>[^"]+\\fichiers\\installer-admin\.ps1)" -Action demande (-Sid S-1-5-21-[\d-]+ )?-Demande "[^"]+\\installation\.demande"$' -and
                (Test-Path -LiteralPath $Matches['script']) -and (& $protege $Matches['script'])) { continue }
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
    # (un réveil déjà en cours, attendu plus bas dans la pile : pas un deuxième schtasks ni une deuxième attente de 20 s)
    if ($script:MesuresAideReveil) { return $false }
    $script:MesuresAideReveil = $true
    try { [void](Invoke-Aide 'aide.bonjour' @{} 10000 -SansDemande); $script:MesuresAideEchec = $null; return $true } catch { $script:MesuresAideEchec = Get-Date; return $false }
    finally { $script:MesuresAideReveil = $false }
}

# FPS d'un jeu (FrameMon) : $true si la mesure a démarré
function Start-FpsMesure([int]$ProcId) {
    if (Test-MesuresAide) {
        if ([FrameMon]::StartRemote($ProcId)) { Write-Log "Mesure des FPS : par l'Aide$(if ([FrameMon]::Attached) { ', reprise de la veille lancée avant le jeu' })"; return $true }
        Write-Log "Mesure des FPS par l'Aide impossible : $([FrameMon]::LastError)"
    }
    # Repli sur place (en administrateur si Nevermind l'est) : même contrôle que l'Aide (Get-AidePresentMon), avant chaque lancement.
    # Le fichier est d'abord bloqué en lecture seule, puis contrôlé : celui qui est lancé est bien celui qui a été contrôlé.
    $raison = if ([FrameMon]::Verrouiller($PresentMonExe)) { Get-RaisonRefusPresentMon $PresentMonExe }
              elseif (Test-Path -LiteralPath $PresentMonExe -PathType Leaf) { "fichier illisible ($([FrameMon]::LastError))" }
              else { 'fichier absent' }
    if ($raison) {
        [FrameMon]::Relacher()
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
    $flux = Test-FluxVeilleUtile $flux   # (lag.ps1 : trace réseau seulement si un jeu guetté est installé, sinon arrêtée)
    if ($script:FpsVeilleTache -or -not ($fps -or $flux)) { return }
    if ($script:FpsVeilleDemande -and ((Get-Date) - $script:FpsVeilleDemande).TotalSeconds -lt 30) { return }
    $noms = @(Get-FpsVeilleJeux)
    # (lot 4, FPS) Un jeu guetté est ouvert (sa partie commence, ou il tournait déjà) : la veille reste telle quelle.
    # Redemandée sans lui, l'Aide relancerait PresentMon sans ce jeu, juste quand sa mesure doit s'y rattacher.
    if (@($noms | Where-Object { [Diagnostics.Process]::GetProcessesByName(($_ -replace '\.exe$', '')).Length }).Count) { return }
    $script:FpsVeilleDemande = Get-Date
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
            $r = Invoke-Aide 'trafic.tcp' @{} 15000 -SansDemande
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
        try { [void](Invoke-Aide 'flux.demarrer' @{} 20000 -SansDemande); Write-Log "Lag : trace réseau par l'Aide"; return 'aide' }
        catch { $script:FluxErreur = $_.Exception.Message; Write-Log "Lag : trace réseau par l'Aide impossible ($($_.Exception.Message))" }
    }
    try { if ([NetFlow]::Start()) { return 'local' }; Write-Log "Lag: repérage du serveur limité ($([NetFlow]::LastError))" } catch { Write-Log "Lag: $_" }
    ''
}
function Get-FluxReseau([string]$Ou) {
    if ($Ou -eq 'aide') { return @(Invoke-Aide 'flux.lire' @{} 15000 -SansDemande | Where-Object { $_ } | ForEach-Object { [string]$_ }) }
    if ($Ou -eq 'local') { return @([NetFlow]::Snapshot()) }
    @()
}
function Stop-FluxReseau([string]$Ou) {
    if ($Ou -eq 'aide') { try { [void](Invoke-Aide 'flux.arreter' @{} 15000 -SansDemande) } catch {} }
    elseif ($Ou -eq 'local') { try { [NetFlow]::Stop() } catch {} }
}

# Ferme tous les programmes d'un nom : ceux de l'utilisateur sur place, ceux lancés en administrateur par l'Aide.
# Jamais ceux de la suite : Mindset et toutes ses applis (Nevermind compris) tournent dans un « Mindset.exe ».
function Stop-ProgrammeNom([string]$Nom) {
    if ($Nom.Trim() -match '^(Mindset|Nevermind|NevermindAide)(\.exe)?$') { throw "$Nom fait partie de la suite Mindset : Nevermind ne le ferme pas." }
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
