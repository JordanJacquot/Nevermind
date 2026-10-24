# Tests de la page RAM (« Qu'est-ce qui prend ma mémoire ? », memoire.ps1), sur de fausses données, sans droits
# administrateur et sans rien lire ni modifier sur le PC.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-memoire.ps1
#
# Ce script contrôle, sur un faux PC (liste de processus, services, compteurs inventés) :
#   1. le regroupement par appli : les processus d'une même appli additionnés, les « Hôte de service » rattachés à leurs
#      services, la mémoire compressée à part, le tri du plus gros au plus petit ;
#   2. les étiquettes (« Windows en a besoin », « Inutile au quotidien », « À vérifier ») ;
#   3. les alertes : mémoire des pilotes selon la taille de la mémoire (alerte franche, « à surveiller », rien), pilote
#      suspect retrouvé par son étiquette, mémoire réservée au matériel ;
#   4. les corrections : seulement les services de la liste « inutile » en démarrage automatique (passés en manuel, jamais
#      désactivés), le lancement en avance de Edge ou Chrome ; jamais un service de Windows ; l'Aide administrateur accepte
#      chacun de ces services et refuse ceux de Windows ; le module ne ferme jamais un processus ;
#   5. le rapport (texte lisible, posé dans un dossier temporaire) et la page de l'écran.
# Code de sortie : 0 si tout passe, 1 sinon.

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'OptiGame\fichiers\modules'
$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0
function Test-Ok([bool]$Condition, [string]$Message) {
    $script:nbTests++
    if (-not $Condition) { $script:echecs.Add($Message) }
}

# ---------------------------------------------------------------------------
# Sécurité du test : rien ne doit pouvoir lire ou modifier le vrai PC (une fonction passe avant une commande du même nom)
# ---------------------------------------------------------------------------
foreach ($c in 'Get-CimInstance', 'Get-WmiObject', 'Get-Process', 'Get-Service', 'Set-Service', 'Stop-Service', 'Stop-Process', 'Start-Process', 'Set-ItemProperty',
               'New-ItemProperty', 'Remove-ItemProperty', 'Invoke-Aide', 'Set-Reg', 'Clear-Reg', 'Set-ServiceOff', 'Set-Declarations', 'taskkill', 'taskkill.exe') {
    Set-Item -Path "function:script:$c" -Value ([scriptblock]::Create("throw 'Test : $c est interdit pendant le test de la mémoire.'"))
}
function script:Write-Log([string]$Message) { }
function script:Get-RegValue([string]$Path, [string]$Name) { throw 'Test : le registre ne doit pas être lu.' }

# ---------------------------------------------------------------------------
# Chargement : memoire.ps1 (que des définitions), Format-Size (systeme.ps1), la page (ecran-perf.ps1)
# ---------------------------------------------------------------------------
$fichier = Join-Path $modules 'memoire.ps1'
$tok = $null; $err = $null
$astMemoire = [System.Management.Automation.Language.Parser]::ParseFile($fichier, [ref]$tok, [ref]$err)
if ($err) { Write-Host "memoire.ps1 n'est pas lisible : $($err[0].Message)" -ForegroundColor Red; exit 1 }
$autres = @($astMemoire.EndBlock.Statements | Where-Object { $_ -isnot [System.Management.Automation.Language.FunctionDefinitionAst] -and $_ -isnot [System.Management.Automation.Language.AssignmentStatementAst] })
Test-Ok (-not $autres.Count) "memoire.ps1 fait autre chose que définir au chargement : $(@($autres | ForEach-Object { $_.Extent.StartLineNumber }) -join ', ')"
. $fichier
foreach ($x in @(@('systeme', 'Format-Size'), @('ecran-perf', 'Get-PageMemoireKit'))) {
    $a = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$($x[0]).ps1"), [ref]$tok, [ref]$err)
    $def = $a.Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $x[1] }, $false)
    if (-not $def) { Write-Host "Fonction $($x[1]) introuvable dans $($x[0]).ps1" -ForegroundColor Red; exit 1 }
    . ([scriptblock]::Create($def.Extent.Text))
}

# ---------------------------------------------------------------------------
# Le faux PC
# ---------------------------------------------------------------------------
function New-Proc([int]$Id, [string]$Nom, [double]$Mo, [string]$Chemin = '', [string]$Ligne = '', [string]$Produit = '', [string]$Description = '') {
    @{ Pid = $Id; Nom = $Nom; Octets = $Mo * 1MB; Chemin = $Chemin; Ligne = $Ligne; Produit = $Produit; Description = $Description; Societe = '' }
}
function New-Svc([string]$Nom, [string]$Titre, [int]$Id, [string]$Demarrage = 'Auto') { @{ Nom = $Nom; Titre = $Titre; Pid = $Id; Demarrage = $Demarrage; Etat = $(if ($Id) { 'Running' } else { 'Stopped' }) } }
function New-FauxPc {
    @{ Pc = 'PC-ESSAI'; Date = [datetime]'2026-10-24 14:30'; Windir = 'C:\Windows'
        Installee = 16GB; Visible = 15.7GB; Disponible = 4GB; Cache = 3GB; Modifiee = 0.1GB; PoolNP = 0.4GB; PoolP = 0.6GB
        Processus = @(
            New-Proc 0 'Idle' 0
            New-Proc 4 'System' 2
            New-Proc 10 'msedge.exe' 100 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe' '"msedge.exe" --no-startup-window --win-session-start' 'Microsoft Edge'
            New-Proc 11 'msedge.exe' 100 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe' '"msedge.exe" --type=renderer' 'Microsoft Edge'
            New-Proc 12 'msedge.exe' 100 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe' '"msedge.exe" --type=gpu-process' 'Microsoft Edge'
            New-Proc 13 'msedge.exe' 100 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe' '"msedge.exe" --type=utility' 'Microsoft Edge'
            New-Proc 14 'msedge.exe' 100 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe' '"msedge.exe" --type=renderer' 'Microsoft Edge'
            New-Proc 20 'chrome.exe' 200 'C:\Program Files\Google\Chrome\Application\chrome.exe' '"chrome.exe"' 'Google Chrome'
            New-Proc 21 'chrome.exe' 200 'C:\Program Files\Google\Chrome\Application\chrome.exe' '"chrome.exe" --type=renderer' 'Google Chrome'
            New-Proc 100 'svchost.exe' 30 'C:\Windows\System32\svchost.exe'
            New-Proc 101 'svchost.exe' 50 'C:\Windows\System32\svchost.exe'
            New-Proc 102 'svchost.exe' 25 'C:\Windows\System32\svchost.exe'
            New-Proc 200 'armsvc.exe' 5 'C:\Program Files (x86)\Common Files\Adobe\ARM\1.0\armsvc.exe' '' 'Adobe Acrobat Update Service'
            New-Proc 201 'updater.exe' 8 'C:\Program Files (x86)\Google\GoogleUpdater\140.0.1.0\updater.exe' '' 'Google Updater'
            New-Proc 202 'HPSysInfoCap.exe' 40 'C:\Windows\System32\DriverStore\FileRepository\hp\HPSysInfoCap.exe'
            New-Proc 300 'MsMpEng.exe' 200
            New-Proc 301 'Memory Compression' 300
            New-Proc 302 'explorer.exe' 150 'C:\Windows\explorer.exe' '' 'Système d&apos;exploitation Microsoft® Windows®' 'Explorateur Windows'
            New-Proc 400 'MonLogiciel.exe' 70 'C:\Program Files\MonLogiciel\MonLogiciel.exe' '' 'Mon Logiciel'
            New-Proc 401 'MonLogicielAgent.exe' 30 'C:\Program Files\MonLogiciel\MonLogicielAgent.exe' '' 'Mon Logiciel'
            New-Proc 500 'LightingService.exe' 60 'C:\Program Files (x86)\LightingService\LightingService.exe' '' 'LightingService'
            New-Proc 600 'Mindset.exe' 120 'C:\Jeux\Nevermind\Mindset.exe'
            New-Proc 601 'Mindset.exe' 80 'D:\Mindset\Mindset.exe'
            New-Proc 700 'Outil.exe' 1 'C:\Outil\Outil.exe'
        )
        Services = @(
            New-Svc 'Dnscache' 'Client DNS' 100
            New-Svc 'nsi' 'Service Interface du magasin réseau' 100
            New-Svc 'WSearch' 'Windows Search' 101
            New-Svc 'SysMain' 'SysMain' 102
            New-Svc 'AdobeARMservice' 'Adobe Acrobat Update Service' 200
            New-Svc 'GoogleUpdaterService140.0.1.0' 'Google Updater Service' 201
            New-Svc 'HPSysInfoCap' 'HP System Info HSA Service' 202
            New-Svc 'edgeupdate' 'Microsoft Edge Update Service' 0 'Manual'
            New-Svc 'WinDefend' 'Antivirus Microsoft Defender' 300
            New-Svc 'MozillaMaintenance' 'Mozilla Maintenance Service' 0 'Disabled'
        )
        Demarrage = @()
        Etiquettes = @() }
}
function Find-Appli($C, [string]$Exe) { @(@($C.Applis) | Where-Object { @($_.Exes) -contains $Exe })[0] }

# ---------------------------------------------------------------------------
# 1 et 2. Regroupement, tri, étiquettes
# ---------------------------------------------------------------------------
$c = Get-ConstatMemoire (New-FauxPc)
$edge = Find-Appli $c 'msedge.exe'
Test-Ok ($edge -and $edge.Nb -eq 5 -and [math]::Abs($edge.Octets - 500MB) -lt 1) "Edge : les 5 processus ne sont pas additionnés ($($edge.Nb) processus, $($edge.Octets) octets)"
Test-Ok (@(@($c.Applis) | Where-Object { @($_.Exes) -contains 'msedge.exe' }).Count -eq 1) 'Edge : plusieurs lignes au lieu d''une'
Test-Ok ($edge.Kind -eq 'inutile' -and $edge.Etiquette -eq 'Inutile au quotidien') "Edge lancé en avance : étiquette « $($edge.Etiquette) »"
Test-Ok (-not @($edge.Corrections).Count -and $edge.Note -like '*Boost de démarrage*') 'Edge lancé en avance : une correction rattachée, ou pas l''endroit où le couper soi-même'
$chrome = Find-Appli $c 'chrome.exe'
Test-Ok ($chrome.Kind -eq 'verifier' -and $chrome.Nb -eq 2) "Chrome ouvert normalement : « $($chrome.Etiquette) », $($chrome.Nb) processus"
$dns = @(@($c.Applis) | Where-Object { @($_.Services) -contains 'Dnscache' })[0]
Test-Ok ($dns -and $dns.Nom -eq 'Client DNS et 1 autre service' -and $dns.Kind -eq 'windows' -and @($dns.Services) -contains 'nsi') "Hôte de service : rattaché à « $($dns.Nom) » ($($dns.Kind))"
$ws = @(@($c.Applis) | Where-Object { @($_.Services) -contains 'WSearch' })[0]
Test-Ok ($ws.Nom -eq 'Windows Search' -and $ws.Etiquette -eq 'Windows en a besoin') "Hôte de service seul : « $($ws.Nom) » [$($ws.Etiquette)]"
Test-Ok ($null -eq (Find-Appli $c 'Memory Compression.exe') -and [math]::Abs($c.Compressee - 300MB) -lt 1) "Mémoire compressée : $($c.Compressee) octets, ou rangée dans les applis"
Test-Ok ($null -eq (Find-Appli $c 'Idle.exe')) 'Le processus inactif (Idle) est compté comme une appli'
$mon = Find-Appli $c 'MonLogiciel.exe'
Test-Ok ($mon -and $mon.Nb -eq 2 -and $mon.Nom -eq 'Mon Logiciel' -and @($mon.Exes) -contains 'MonLogicielAgent.exe' -and $mon.Kind -eq 'verifier') "Deux programmes d'un même logiciel : pas regroupés (« $($mon.Nom) », $($mon.Nb))"
# D'où vient chaque appli : la page Démarrage n'est conseillée que si le programme y est vraiment (lui, un de ses
# parents, ou un .exe nommé dans la commande : Discord se lance par « Update.exe --processStart Discord.exe »)
$d = New-FauxPc
$d.Processus = @($d.Processus) + @(
    New-Proc 800 'Discord.exe' 300 'C:\Users\x\AppData\Local\Discord\app-1.0\Discord.exe' '' 'Discord'
    New-Proc 810 'steam.exe' 50 'C:\Steam\steam.exe' '' 'Steam'
    New-Proc 811 'steamwebhelper.exe' 200 'C:\Steam\bin\steamwebhelper.exe' '' 'Steam Client WebHelper'
    New-Proc 820 'LEDKeeper2.exe' 100 'C:\Program Files (x86)\MSI\LEDKeeper2.exe' '' 'LEDKeeper2'
    New-Proc 830 'Code.exe' 400 'C:\Users\x\AppData\Local\Programs\Microsoft VS Code\Code.exe' '' 'Visual Studio Code')
foreach ($x in $d.Processus) { $x.Parent = switch ([int]$x.Pid) { 811 { 810 } 820 { 101 } default { 0 } } }
$d.Demarrage = @('update.exe', 'discord.exe', 'steam.exe', 'onedrive.exe')
$co = Get-ConstatMemoire $d
foreach ($cas in @(@('Discord.exe', 'demarrage'), @('steamwebhelper.exe', 'demarrage'), @('LEDKeeper2.exe', 'windows'), @('Code.exe', 'ouvert'), @('MonLogiciel.exe', 'ouvert'))) {
    $a = Find-Appli $co $cas[0]
    $ok = switch ($cas[1]) {
        'demarrage' { $a.Note -like '*coupe-le dans la page Démarrage*' }
        'windows' { $a.Note -like '*pas dans la page Démarrage*' -and $a.Note -like '*désinstaller*' }
        'ouvert' { $a.Note -like '*ouvert en ce moment*' -and $a.Note -notlike '*page Démarrage*' }
    }
    Test-Ok $ok "$($cas[0]) ($($cas[1])) : conseil « $($a.Note) »"
}
Test-Ok (-not @(@($co.Applis) | Where-Object { $_.Kind -eq 'windows' -and $_.Note -like '*Démarrage*' }).Count) 'Un programme de Windows renvoie à la page Démarrage'
# Contre-épreuve : sans la page Démarrage lue, plus personne n'y est envoyé
$d.Demarrage = @(); $co2 = Get-ConstatMemoire $d
Test-Ok (-not @(@($co2.Applis) | Where-Object { $_.Note -like '*coupe-le dans la page Démarrage*' }).Count) 'Page Démarrage conseillée pour un programme qui n''y est pas'

$explo = Find-Appli $c 'explorer.exe'
Test-Ok ($explo.Kind -eq 'windows' -and $explo.Nom -notmatch 'apos|exploitation') "Explorateur : « $($explo.Nom) » [$($explo.Etiquette)]"
Test-Ok ((Find-Appli $c 'MsMpEng.exe').Kind -eq 'windows') 'L''antivirus de Windows n''est pas « Windows en a besoin »'
$rgb = Find-Appli $c 'LightingService.exe'
Test-Ok ($rgb.Kind -eq 'inutile' -and -not @($rgb.Corrections).Count) "Lumières (RGB) : « $($rgb.Etiquette) », corrections $(@($rgb.Corrections) -join ',')"
$suite = Find-Appli $c 'Mindset.exe'
Test-Ok ($suite.Nb -eq 2 -and $suite.Kind -ne 'inutile') 'Les applis de la suite Mindset : mal regroupées ou dites inutiles'
$adobe = Find-Appli $c 'armsvc.exe'
Test-Ok ($adobe.Kind -eq 'inutile' -and @($adobe.Corrections) -contains 'svc:AdobeARMservice') "Service de mise à jour d'Adobe : « $($adobe.Etiquette) », corrections $(@($adobe.Corrections) -join ',')"
$tri = @($c.Applis | ForEach-Object { $_.Octets })
$trie = $true; for ($i = 1; $i -lt $tri.Count; $i++) { if ($tri[$i] -gt $tri[$i - 1]) { $trie = $false } }
Test-Ok $trie 'Les applis ne sont pas triées du plus gros au plus petit'
Test-Ok (@($c.Applis)[0].Exes -contains 'msedge.exe') "La plus grosse appli n'est pas Edge (500 Mo) : $(@($c.Applis)[0].Nom)"
Test-Ok (-not @(@($c.Applis) | Where-Object { $_.Etiquette -notin 'Windows en a besoin', 'Inutile au quotidien', 'À vérifier' }).Count) 'Une appli sans étiquette connue'
Test-Ok ($c.Pourcent -eq [int][math]::Round(100 * (15.7GB - 4GB) / 15.7GB)) "Pourcentage utilisé : $($c.Pourcent)"
Test-Ok ([math]::Abs($c.Reservee - 0.3GB) -lt 1MB) "Réservé au matériel : $($c.Reservee)"
Test-Ok (-not @($c.Alertes).Count) "Alertes sur un PC normal : $(@($c.Alertes | ForEach-Object { $_.Titre }) -join ' / ')"
$cachee = @(Get-PartCacheeMemoire $c)
Test-Ok (@($cachee | Where-Object { $_.Nom -like 'En cache*' -and $_.Note -match 'ne compte pas|rend' }).Count -eq 1) 'Part cachée : le cache n''est pas expliqué comme « ne compte pas »'
Test-Ok (@($cachee | Where-Object { $_.Nom -like 'Réservé au matériel*' }).Count -eq 1 -and @($cachee | Where-Object { $_.Nom -like 'Pilotes*' }).Count -eq 2 -and @($cachee | Where-Object { $_.Nom -like 'Mémoire compressée*' }).Count -eq 1) 'Part cachée : une ligne manque (matériel, pilotes, compressée)'

# ---------------------------------------------------------------------------
# 4. Corrections : seulement ce qui est sûr
# ---------------------------------------------------------------------------
$ids = @($c.Corrections | ForEach-Object { $_.Id } | Sort-Object)
$attendus = @('svc:AdobeARMservice', 'svc:GoogleUpdaterService140.0.1.0', 'svc:HPSysInfoCap')
Test-Ok (($ids -join ',') -eq ($attendus -join ',')) "Corrections : $($ids -join ', ') au lieu de $($attendus -join ', ')"
$decl = @(Get-ChangementsMemoire $c)
Test-Ok (-not @($decl | Where-Object { $_.Type -ne 'svc' }).Count) 'Corrections : autre chose qu''un service (Edge et Chrome ne sont jamais touchés : « géré par ton organisation »)'
Test-Ok (-not @($decl | Where-Object { $_.Type -eq 'svc' -and $_.Demarrage -ne 'Manual' }).Count) 'Corrections : un service n''est pas passé en « manuel » (désactivé ?)'
# Chrome en arrière-plan : étiquette « inutile » et où le couper, mais aucune correction
$d = New-FauxPc
$d.Processus = @($d.Processus) + @(New-Proc 22 'chrome.exe' 50 'C:\Program Files\Google\Chrome\Application\chrome.exe' '"chrome.exe" --no-startup-window' 'Google Chrome')
$c2 = Get-ConstatMemoire $d
Test-Ok (-not @($c2.Corrections | Where-Object { $_.Id -in 'edge', 'chrome' }).Count -and (Find-Appli $c2 'chrome.exe').Kind -eq 'inutile' -and (Find-Appli $c2 'chrome.exe').Note -like '*Paramètres*') 'Chrome en arrière-plan : correction proposée, mauvaise étiquette ou pas l''endroit où le couper'
# Sans aucun lancement en avance : pas de correction de navigateur
$d = New-FauxPc; $d.Processus = @($d.Processus | Where-Object { $_.Ligne -notmatch 'no-startup-window' })
$c3 = Get-ConstatMemoire $d
Test-Ok (-not @($c3.Corrections | Where-Object { $_.Id -in 'edge', 'chrome' }).Count -and (Find-Appli $c3 'msedge.exe').Kind -eq 'verifier') 'Edge ouvert normalement : dit inutile, ou correction proposée'

# Jamais un service de Windows ; l'Aide accepte exactement les services de la liste
. (Join-Path $modules 'aide-actions.ps1')
function Test-AideAccepte([string]$Nom, [string]$Demarrage = 'Manual') { try { [void](Invoke-AideAction 'service.regler' @{ Nom = $Nom; Demarrage = $Demarrage; Arreter = $true } -Verifier); $true } catch { $false } }
$exemples = 'edgeupdate', 'edgeupdatem', 'gupdate', 'gupdatem', 'GoogleUpdaterService140.0.1.0', 'GoogleUpdaterInternalService140.0.1.0', 'AdobeARMservice', 'AdobeUpdateService', 'MozillaMaintenance',
            'HPAppHelperCap', 'HPDiagsCap', 'HPSysInfoCap', 'HpTouchpointAnalyticsService', 'SupportAssistAgent', 'DDVDataCollector', 'DDVCollectorSvcApi', 'DDVRulesProcessor',
            'ASUSSystemAnalysis', 'ASUSSystemDiagnosis', 'ASUSLinkNear', 'ASUSLinkRemote', 'ESRV_SVC_QUEENCREEK', 'SystemUsageReportSvc_QUEENCREEK', 'NvTelemetryContainer'
foreach ($n in $exemples) {
    Test-Ok (@($ServicesInutilesMemoire | Where-Object { $n -match $_.Motif }).Count -eq 1) "Service $n : pas reconnu (ou reconnu deux fois) par la liste de memoire.ps1"
    Test-Ok (Test-AideAccepte $n) "Service $n : refusé par l'Aide, la correction échouerait"
    Test-Ok (Test-AideAccepte $n 'Automatic') "Service $n : l'Aide refuserait de le remettre comme avant"
    Test-Ok ($n -notmatch $AideServicesProteges) "Service $n : c'est un service protégé de Windows"
}
foreach ($i in $ServicesInutilesMemoire) { Test-Ok (@($exemples | Where-Object { $_ -match $i.Motif }).Count -ge 1) "Liste de memoire.ps1 : « $($i.Nom) » n'est pas couvert par les exemples de ce test" }
$windows = 'WinDefend', 'WSearch', 'SysMain', 'wuauserv', 'Dnscache', 'Audiosrv', 'Spooler', 'BITS', 'mpssvc', 'EventLog', 'gupdate; calc', 'edgeupdate2', 'xedgeupdate', 'GoogleUpdaterService1.0 x'
foreach ($n in $windows) {
    Test-Ok (-not (Test-AideAccepte $n)) "Service $n : accepté par l'Aide"
    Test-Ok (-not @($ServicesInutilesMemoire | Where-Object { $n -match $_.Motif }).Count) "Service $n : dans la liste « inutile » de memoire.ps1"
}

# Le module ne ferme jamais un processus et ne touche pas lui-même aux services ni au registre
$cmds = @($astMemoire.FindAll({ param($n) $n -is [System.Management.Automation.Language.CommandAst] }, $true) | ForEach-Object { $_.GetCommandName() } | Where-Object { $_ })
$interdits = @($cmds | Where-Object { $_ -in 'Stop-Process', 'Stop-Service', 'Set-Service', 'taskkill', 'taskkill.exe', 'Set-ItemProperty', 'New-ItemProperty', 'Remove-ItemProperty', 'Set-Reg', 'Clear-Reg', 'Invoke-Aide', 'Set-ServiceOff' } | Select-Object -Unique)
Test-Ok (-not $interdits.Count) "memoire.ps1 appelle $($interdits -join ', ') en direct (tout doit passer par Set-Declarations)"
$kill = @($astMemoire.FindAll({ param($n) $n -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and "$($n.Member)" -in 'Kill', 'CloseMainWindow' }, $true))
Test-Ok (-not $kill.Count) 'memoire.ps1 ferme un processus (Kill)'
Test-Ok (([IO.File]::ReadAllText($fichier)) -notmatch 'processus\.fermer') 'memoire.ps1 demande à l''Aide de fermer un processus'

# ---------------------------------------------------------------------------
# 3. Alertes : la mémoire des pilotes selon la taille de la mémoire, le matériel
# ---------------------------------------------------------------------------
function Get-AlertePilotes([double]$Memoire, [double]$Np, $Etiquettes = @()) {
    $d = New-FauxPc; $d.Installee = $Memoire; $d.Visible = $Memoire - 0.2GB; $d.PoolNP = $Np; $d.Etiquettes = $Etiquettes
    @((Get-ConstatMemoire $d).Alertes | Where-Object { $_.Id -eq 'pilotes' })[0]
}
$cas = @(
    @(32GB, 1.3GB, 'attention'),   # (le vrai cas de Jordan : un peu élevé sur 32 Go)
    @(32GB, 1.7GB, 'urgent'),
    @(32GB, 0.7GB, ''),
    @(16GB, 1.3GB, 'urgent'),
    @(16GB, 0.9GB, 'attention'),
    @(16GB, 0.5GB, ''),
    @(8GB, 1.1GB, 'urgent'),
    @(8GB, 0.8GB, 'attention'),
    @(64GB, 1.9GB, 'attention'),
    @(64GB, 2.1GB, 'urgent')
)
foreach ($x in $cas) {
    $a = Get-AlertePilotes $x[0] $x[1]
    $ton = if ($a) { $a.Ton } else { '' }
    Test-Ok ($ton -eq $x[2]) "Pilotes $(Format-Size $x[1]) sur $(Format-Size $x[0]) : « $ton » au lieu de « $($x[2]) »"
}
$a = Get-AlertePilotes 16GB 1.3GB
Test-Ok ($a.Texte -match 'Killer' -and $a.Texte -match 'Realtek' -and $a.Texte -match 'antivirus' -and $a.Texte -match 'RGB' -and $a.Quoi) 'Alerte des pilotes : les suspects classiques ou « que faire » manquent'
Test-Ok ($a.Texte -notmatch 'probablement') 'Alerte des pilotes : un pilote suspect inventé sans étiquette'
$et = @(
    @{ Tag = 'Ntfx'; Octets = 600MB; Pilotes = @(@{ Fichier = 'ntfs.sys'; Description = 'Pilote NT File System'; Societe = 'Microsoft Corporation' }) }
    @{ Tag = 'RTKx'; Octets = 450MB; Pilotes = @(@{ Fichier = 'rt640x64.sys'; Description = 'Realtek PCIe GbE Family Controller'; Societe = 'Realtek' }) }
)
$a = Get-AlertePilotes 16GB 1.3GB $et
Test-Ok ($a.Texte -match 'rt640x64\.sys' -and $a.Texte -match 'carte réseau Realtek' -and $a.Texte -match '450 Mo') "Alerte des pilotes : le pilote d'un fabricant n'est pas désigné : $($a.Texte)"
Test-Ok ($a.Quoi -match 'Realtek PCIe GbE') "Alerte des pilotes : « que faire » ne nomme pas le pilote : $($a.Quoi)"
$a = Get-AlertePilotes 16GB 1.3GB @(@{ Tag = 'Ntfx'; Octets = 600MB; Pilotes = @(@{ Fichier = 'ntfs.sys'; Description = 'Pilote NT File System'; Societe = 'Microsoft Corporation' }) })
Test-Ok ($a.Texte -match 'ntfs\.sys') 'Alerte des pilotes : seul un pilote de Windows porte l''étiquette, il n''est pas cité'
$a = Get-AlertePilotes 16GB 1.3GB @(@{ Tag = 'Xxxx'; Octets = 600MB; Pilotes = @() })
Test-Ok ($a -and $a.Texte -notmatch 'probablement') 'Alerte des pilotes : étiquette sans pilote retrouvé, un suspect inventé'
foreach ($g in @(@('Killer E3100G 2.5 Gigabit Ethernet', 'carte réseau Killer'), @('Realtek PCIe GbE Family Controller', 'carte réseau Realtek'), @('Intel(R) Wi-Fi 6 AX201', 'carte réseau'), @('Avast Antivirus', 'antivirus'), @('ASUS Aura lighting', 'logiciel RGB ou de carte mère'))) {
    Test-Ok ((Get-GenrePilote $g[0]) -eq $g[1]) "Genre du pilote « $($g[0]) » : « $(Get-GenrePilote $g[0]) » au lieu de « $($g[1]) »"
}
# Réservé au matériel
$d = New-FauxPc; $d.Installee = 16GB; $d.Visible = 14.5GB
$m = @((Get-ConstatMemoire $d).Alertes | Where-Object { $_.Id -eq 'materiel' })[0]
Test-Ok ($m -and $m.Quoi -match 'BIOS') 'Réservé au matériel (1,5 Go) : pas d''alerte, ou sans « que faire »'
$d = New-FauxPc; $d.Installee = 0
$c4 = Get-ConstatMemoire $d
Test-Ok ($c4.Reservee -eq 0 -and -not @($c4.Alertes | Where-Object { $_.Id -eq 'materiel' }).Count -and -not @(Get-PartCacheeMemoire $c4 | Where-Object { $_.Nom -like 'Réservé*' }).Count) 'Mémoire installée illisible : un « réservé au matériel » inventé'
# Données vides : pas d'erreur
try { $c5 = Get-ConstatMemoire @{ Visible = 8GB; Disponible = 2GB }; Test-Ok (@($c5.Applis).Count -eq 0) 'Données vides : des applis inventées' } catch { Test-Ok $false "Données vides : erreur $($_.Exception.Message)" }

# ---------------------------------------------------------------------------
# 5. Le rapport et la page
# ---------------------------------------------------------------------------
$d = New-FauxPc; $d.PoolNP = 1.3GB; $d.Etiquettes = $et
$c = Get-ConstatMemoire $d
$r = Format-RapportMemoire $c
foreach ($morceau in 'Rapport mémoire de Nevermind', 'PC-ESSAI', 'EN BREF', 'A REGARDER', 'PAR APPLI', 'LA PART CACHEE', 'CE QUE NEVERMIND PEUT COUPER', 'Microsoft Edge (5 processus)', '[Inutile au quotidien]', '[Windows en a besoin]', '[À vérifier]', 'ne compte pas', 'rt640x64.sys', 'Réservé au matériel') {
    Test-Ok ($r.Contains($morceau)) "Rapport : « $morceau » manque"
}
Test-Ok ($r -notmatch '[\u2013\u2014]') 'Rapport : tiret long'
Test-Ok ($r.IndexOf('Microsoft Edge') -lt $r.IndexOf('Mon Logiciel')) 'Rapport : les applis ne sont pas du plus gros au plus petit'
$dossier = Join-Path $env:TEMP "nevermind-test-memoire-$PID"
[void][IO.Directory]::CreateDirectory($dossier)
try {
    $f = Save-RapportMemoire $c $dossier
    Test-Ok ([IO.File]::Exists($f) -and [IO.Path]::GetFileName($f) -eq 'Nevermind, rapport mémoire du 24-10-2026 à 14h30.txt') "Rapport : fichier « $f »"
    $o = [IO.File]::ReadAllBytes($f)
    Test-Ok ($o[0] -eq 0xEF -and $o[1] -eq 0xBB -and $o[2] -eq 0xBF) 'Rapport : pas en UTF-8 avec BOM (accents cassés dans le Bloc-notes)'
    Test-Ok ([IO.File]::ReadAllText($f, [Text.Encoding]::UTF8) -eq $r) 'Rapport : le fichier n''a pas le texte du rapport'
} finally { foreach ($x in [IO.Directory]::GetFiles($dossier)) { [IO.File]::Delete($x) }; [IO.Directory]::Delete($dossier) }

$p = Get-PageMemoireKit $null
Test-Ok ($p.titre -eq 'RAM' -and $p.parent -eq 'perf' -and @($p.module.boutons | Where-Object { $_.id -eq 'memoire:analyser' }).Count) 'Page sans constat : pas de bouton pour regarder'
$p = Get-PageMemoireKit $c
$b = @($p.module.boutons)
Test-Ok (@($b | Where-Object { $_.id -eq 'memoire:couper' -and $_.texte -eq "Couper ce qui est sûr ($(@($c.Corrections).Count))" }).Count -eq 1) "Page : bouton « Couper ce qui est sûr » absent ou faux ($(@($b | ForEach-Object { $_.texte }) -join ', '))"
Test-Ok (@($b | Where-Object { $_.id -eq 'memoire:rapport' }).Count -eq 1) 'Page : bouton « Créer un rapport » absent'
$titres = @($p.blocs | Where-Object { $_.type -eq 'titre' } | ForEach-Object { $_.texte })
Test-Ok ($titres -contains 'À regarder' -and $titres -contains 'Par appli' -and $titres -contains 'La part cachée') "Page : titres $($titres -join ', ')"
$lignes = @($p.blocs | Where-Object { $_.type -eq 'ligne' -and $_.etiquette -in 'Windows en a besoin', 'Inutile au quotidien', 'À vérifier' })
Test-Ok ($lignes.Count -ge 5 -and $lignes[0].texte -like 'Microsoft Edge*') "Page : lignes des applis ($($lignes.Count), la première « $($lignes[0].texte) »)"
$json = $p | ConvertTo-Json -Depth 10 -Compress
Test-Ok ($json -notmatch '\\u2013|\\u2014|[\u2013\u2014]') 'Page : tiret long'
$d = New-FauxPc; $d.Services = @($d.Services | Where-Object { $_.Nom -notin 'AdobeARMservice', 'GoogleUpdaterService140.0.1.0', 'HPSysInfoCap' }); $d.Processus = @($d.Processus | Where-Object { $_.Ligne -notmatch 'no-startup-window' })
$p = Get-PageMemoireKit (Get-ConstatMemoire $d)
Test-Ok (-not @($p.module.boutons | Where-Object { $_.id -eq 'memoire:couper' }).Count) 'Page : « Couper ce qui est sûr » alors qu''il n''y a rien à couper'

# ---------------------------------------------------------------------------
# 6. La lecture en tâche de fond : le fil de fond se suffit à lui-même, le constat arrive d'abord sans le nom du pilote,
#    puis avec ; le rapport attend la fin de la lecture
# ---------------------------------------------------------------------------
# Le travail de fond dans un fil à part, sans aucun autre module (étape « pilotes » sans étiquette : rien n'est lu)
$ps = [PowerShell]::Create()
try {
    [void]$ps.AddScript($TravailMemoire.ToString()).AddArgument([IO.File]::ReadAllText($fichier, [Text.Encoding]::UTF8)).AddArgument(@{}).AddArgument(@{}).AddArgument(@()).AddArgument('pilotes')
    $sortie = @($ps.Invoke())
    $x = if ($sortie.Count) { $sortie[0] } else { $null }
    if ($x -is [psobject]) { $x = $x.psobject.BaseObject }
    Test-Ok ($x -and $x.Etape -eq 'pilotes' -and -not $x.Erreur -and $x.Pilotes -is [hashtable] -and -not $x.Pilotes.Count) "Fil de fond : résultat inattendu ($(if ($x) { "étape $($x.Etape), erreur $($x.Erreur)" } else { 'rien' }))"
    Test-Ok (-not $ps.Streams.Error.Count) "Fil de fond : erreurs $(@($ps.Streams.Error | ForEach-Object { $_.ToString() }) -join ' / ')"
} finally { $ps.Dispose() }
$appels = New-Object System.Collections.ArrayList
$script:Kit = @{}
function Start-ConstatMemoire([string]$Ensuite, [string]$Etape = 'donnees', [string[]]$Tags) {
    if ($Ensuite -and @($script:MemoireApres) -notcontains $Ensuite) { $script:MemoireApres = @(@($script:MemoireApres | Where-Object { $_ }) + $Ensuite) }
    [void]$appels.Add("lecture:${Etape}:$(@($Tags) -join ',')"); $script:Kit.MemoireEnCours = $Etape
}
function Show-MessageVue([string]$Texte) { [void]$appels.Add("message:$Texte") }
function Set-OccupeVue([string]$Texte) { [void]$appels.Add("occupe:$Texte") }
function Update-EcranNevermind { [void]$appels.Add('ecran') }
# « Créer un rapport » sans constat : la lecture est lancée, le rapport attend (aucun fichier)
$script:MemoireJob = $null; $script:MemoireApres = @()
New-RapportMemoire
Test-Ok ($appels -contains 'lecture:donnees:' -and @($script:MemoireApres) -contains 'rapport') "Rapport sans constat : la lecture n'est pas lancée ou le rapport n'attend pas ($($appels -join ' | '))"
Test-Ok (@($appels | Where-Object { $_ -like 'message:*' }).Count -eq 1) 'Rapport sans constat : pas de message pour dire d''attendre'
function New-RapportMemoire { [void]$appels.Add('rapport') }
# Les données arrivent : le constat s'affiche sans le nom du pilote, la recherche ne porte que sur l'étiquette inconnue
$appels.Clear()
$script:PilotesEtiquettes = @{ Ntfx = @() }
$d = New-FauxPc; $d.PoolNP = 1.3GB; $d.Etiquettes = @(@{ Tag = 'RTKx'; Octets = 450MB; Pilotes = @() }, @{ Tag = 'Ntfx'; Octets = 600MB; Pilotes = @() })
Receive-ResultatMemoire @{ Etape = 'donnees'; Donnees = $d; Journal = @('Mémoire : essai') }
$al = @($script:Kit.Memoire.Alertes | Where-Object { $_.Id -eq 'pilotes' })[0]
Test-Ok ($script:Kit.Memoire -and $al -and $al.Texte -notmatch 'probablement') 'Données reçues : pas de constat, ou un pilote inventé avant la recherche'
Test-Ok ($appels -contains 'lecture:pilotes:RTKx') "Données reçues : recherche du pilote mal lancée ($($appels -join ' | '))"
Test-Ok ($appels -contains 'ecran' -and $appels -notcontains 'rapport') "Données reçues : l'écran n'est pas mis à jour, ou le rapport part avant le nom du pilote ($($appels -join ' | '))"
$p = Get-PageMemoireKit $script:Kit.Memoire 'pilotes'
Test-Ok (@($p.blocs | Where-Object { $_.type -eq 'ligne' -and $_.note -match 'Je cherche encore quel pilote' }).Count -eq 1) 'Page pendant la recherche du pilote : rien ne le dit'
Test-Ok (-not @($p.actions).Count) 'Page pendant la lecture : « Actualiser » est proposé'
# Le nom du pilote arrive : il est dans le constat, gardé pour la séance, puis le rapport qui attendait est posé
$appels.Clear()
Receive-ResultatMemoire @{ Etape = 'pilotes'; Pilotes = @{ RTKx = @(@{ Fichier = 'rt640x64.sys'; Description = 'Realtek PCIe GbE Family Controller'; Societe = 'Realtek' }) }; Journal = @() }
$al = @($script:Kit.Memoire.Alertes | Where-Object { $_.Id -eq 'pilotes' })[0]
Test-Ok ($al.Texte -match 'rt640x64\.sys') "Nom du pilote reçu : pas dans le constat ($($al.Texte))"
Test-Ok ($script:PilotesEtiquettes.ContainsKey('RTKx') -and -not $script:Kit.MemoireEnCours) 'Nom du pilote reçu : pas gardé pour la séance, ou lecture toujours « en cours »'
Test-Ok (@($appels | Where-Object { $_ -eq 'rapport' }).Count -eq 1 -and -not @($script:MemoireApres).Count) "Nom du pilote reçu : le rapport qui attendait n'est pas posé une fois ($($appels -join ' | '))"
Test-Ok ($appels -contains 'occupe:' -and $appels -contains 'ecran') 'Fin de la lecture : la pastille « en cours » reste, ou l''écran n''est pas mis à jour'
# Lecture ratée : pas d'erreur, la pastille s'éteint
$appels.Clear()
try { Receive-ResultatMemoire $null; Test-Ok ($appels -contains 'occupe:') 'Lecture ratée : la pastille « en cours » reste' } catch { Test-Ok $false "Lecture ratée : erreur $($_.Exception.Message)" }
$p = Get-PageMemoireKit $null 'donnees'
Test-Ok (-not @($p.module.boutons).Count -and $p.module.titre -eq 'Je regarde ta mémoire') 'Page pendant la première lecture : un bouton en double, ou pas de message'
# ---------------------------------------------------------------------------
# Résultat
# ---------------------------------------------------------------------------
if ($echecs.Count) {
    foreach ($e in $echecs) { Write-Host "ÉCHEC  $e" -ForegroundColor Red }
    Write-Host "$($echecs.Count) échec(s) sur $nbTests vérifications." -ForegroundColor Red
    exit 1
}
Write-Host "Page RAM (mémoire) : $nbTests vérifications, tout passe." -ForegroundColor Green
exit 0
