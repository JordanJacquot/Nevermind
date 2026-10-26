# Tests de la page RAM (« Qu'est-ce qui prend ma mémoire ? », memoire.ps1), sur de fausses données, sans droits
# administrateur et sans rien lire ni modifier sur le PC (seule exception : les programmes d'essai de la section 8, lancés
# et fermés par ce test).
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
#   6. la lecture en tâche de fond ;
#   7. « Fermer » sur de fausses données : qui a un bouton (chaque catégorie interdite et sa contre-épreuve, le doute,
#      l'appli entière), la règle commune avec l'Aide, la page, le déroulé (confirmation, question, ne répond plus, rangée) ;
#   8. la fermeture pour de vrai de programmes d'essai compilés ici (fenêtres hors de l'écran, jamais un programme de
#      l'utilisateur) : polie, forcée, numéro repris refusé, règle relue au moment de fermer ;
#   9. « Fermer <programme> » (diagnostic des FPS), le blocage dans Trafic et le mode jeu : la même règle, le nom exact
#      (un motif comme « x* » ne ferme rien), un programme de l'antivirus refusé (audit du 25/10, constat 17).
# Code de sortie : 0 si tout passe, 1 sinon.

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
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
# La règle de fermeture, commune avec l'Aide : NevermindPont.dll, chargée comme le fait Nevermind (pont.ps1), et ce que
# Windows dit des processus (processus.ps1)
. (Join-Path $PSScriptRoot 'pont-essai.ps1')
. (Join-Path $modules 'pont.ps1')
. (Join-Path $modules 'processus.ps1')
$raisonPont = Initialize-Pont (Get-PontEssai)
if ($raisonPont) { Write-Host "NevermindPont.dll : $raisonPont" -ForegroundColor Red; exit 1 }
# L'Aide en PowerShell (aide-actions.ps1), à part dans un module : seulement pour vérifier qu'elle accepte les services
# de la liste (ses autres fonctions, dont sa propre copie de la règle, ne sont pas visibles ici)
$null = New-Module -Name AideActionsEssai -ArgumentList (Join-Path $modules 'aide-actions.ps1') -ScriptBlock {
    param($f) . $f; Export-ModuleMember -Function Invoke-AideAction -Variable AideServicesProteges, AideExesProteges, AideExesWindows }
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
Test-Ok (-not $kill.Count) 'memoire.ps1 ferme un processus autrement que par la fermeture vérifiée (Kill, CloseMainWindow)'
$texteMemoire = [IO.File]::ReadAllText($fichier)
Test-Ok ($texteMemoire -notmatch 'processus\.fermer') 'memoire.ps1 demande à l''Aide de fermer un processus'
# Seule la fermeture vérifiée ferme : TerminateProcess une fois, dans Forcer, après Verifier ; WM_CLOSE seulement dans
# FermerPoliment, après Verifier ; et Forcer ou FermerPoliment seulement dans Invoke-FermetureMemoire, après la règle
Test-Ok (([regex]::Matches($texteMemoire, 'TerminateProcess\(')).Count -eq 2 -and $texteMemoire -match '(?s)public static int Forcer\([^{]*\{\s*int v = Verifier\(pid, debut, chemin\);.*?TerminateProcess\(Poignee') 'Fermeture forcée : pas seulement après la vérification de la poignée'
Test-Ok (([regex]::Matches($texteMemoire, 'PostMessage\(w, 0x0010')).Count -eq 1 -and $texteMemoire -match '(?s)public static int FermerPoliment\([^{]*\{\s*int v = Verifier\(pid, debut, chemin\);.*?PostMessage\(w, 0x0010') 'Fermeture polie : pas seulement après la vérification de la poignée'
$appelsFermer = @($astMemoire.FindAll({ param($n) $n -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and "$($n.Member)" -in 'Forcer', 'FermerPoliment' }, $true))
$fonctionsFermer = @($appelsFermer | ForEach-Object { $x = $_.Parent; while ($x -and $x -isnot [System.Management.Automation.Language.FunctionDefinitionAst]) { $x = $x.Parent }; if ($x) { $x.Name } else { '(hors fonction)' } } | Select-Object -Unique)
Test-Ok ($appelsFermer.Count -eq 2 -and ($fonctionsFermer -join ',') -eq 'Invoke-FermetureMemoire') "Fermer appelé hors de Invoke-FermetureMemoire : $($fonctionsFermer -join ', ')"
$defInvoke = $astMemoire.Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Invoke-FermetureMemoire' }, $true).Extent.Text
Test-Ok ($defInvoke.IndexOf('Get-RefusFermeture') -gt 0 -and $defInvoke.IndexOf('Get-RefusFermeture') -lt $defInvoke.IndexOf('::Forcer(') -and $defInvoke.IndexOf('::Verifier(') -lt $defInvoke.IndexOf('Get-RefusFermeture')) 'Invoke-FermetureMemoire : la poignée et la règle ne sont pas revérifiées avant de fermer'
# Une seule règle de fermeture, en Rust (NevermindPont.dll, la même que l'Aide), utilisée par la page RAM (pas de copie
# dans Nevermind : pont.ps1 la demande à la DLL ; aide-actions.ps1 garde celle de l'Aide en PowerShell, jusqu'à son retrait)
$defsRegle = @(foreach ($m in Get-ChildItem -LiteralPath $modules -Filter '*.ps1') { if ([IO.File]::ReadAllText($m.FullName) -match 'function Get-RefusFermeture\b') { $m.Name } })
Test-Ok (($defsRegle -join ',') -eq 'aide-actions.ps1,pont.ps1') "La règle de fermeture est définie dans : $($defsRegle -join ', ')"
Test-Ok ([string]${function:Get-RefusFermeture} -match 'Get-RefusFermetureListe' -and [string]${function:Get-RefusFermetureListe} -match "action = 'fermeture\.refus'") 'La règle de fermeture de Nevermind n''est plus celle de NevermindPont.dll'
Test-Ok ($texteMemoire -notmatch 'GetProcessInformation|ProcessProtection|SecurityCenter2|TokenElevation') 'memoire.ps1 a sa propre copie de la règle (protection, antivirus, administrateur)'
$texteAideActions = [IO.File]::ReadAllText((Join-Path $modules 'aide-actions.ps1'))
Test-Ok ($texteAideActions -match '(?s)function Get-AideRefusProcessus\b[^}]*Get-RefusFermeture' -and $texteAideActions -match '(?s)''processus\.fermer''.*?Get-AideRefusProcessus') 'L''Aide (processus.fermer) n''utilise pas la règle commune'
Test-Ok ('Nevermind' -match $AideExesProteges -and 'Mindset.exe' -match $AideExesProteges -and 'csrss' -match $AideExesProteges -and 'Mindset.exe' -notmatch $AideExesWindows -and 'explorer.exe' -match $AideExesWindows) 'Les listes des programmes de Windows de l''Aide sont fausses'

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
# 7. « Fermer » : qui a un bouton (sur de fausses données). Une appli seulement si TOUS ses processus passent la règle
#    commune avec l'Aide (Get-RefusFermeture) ; chaque catégorie interdite a sa contre-épreuve (la même appli, le même
#    processus, sans ce seul détail, a son bouton)
# ---------------------------------------------------------------------------
$moi = @{ Id = 9000; Session = 1; Compte = 'S-1-5-21-ESSAI-1001'; Eleve = 0 }
# Un faux PC dont chaque processus est lisible : même compte et même session que Nevermind, pas protégé, pas en
# administrateur, poignée gardée (Debut)
function New-FauxPcFermer([array]$EnPlus) {
    $d = New-FauxPc
    $d.Processus = @($d.Processus) + @($EnPlus)
    $d.Moi = $moi.Clone(); $d.Antivirus = @('C:\Program Files\AVX\Moteur\avx.exe', 'C:\Windows\System32\avxreport.exe')
    foreach ($x in $d.Processus) {
        if (-not $x.ContainsKey('Parent')) { $x.Parent = 0 }
        $x.Session = 1; $x.Compte = $moi.Compte; $x.Protection = 0; $x.Eleve = 0; $x.Debut = [long](1000 + $x.Pid); $x.Image = $x.Chemin
    }
    $d
}
function Get-LigneFermer($C, [string]$Exe) { Find-Appli $C $Exe }
$discord = { New-Proc 800 'Discord.exe' 300 'C:\Users\x\AppData\Local\Discord\app-1.0\Discord.exe' '' 'Discord' }
$c = Get-ConstatMemoire (New-FauxPcFermer @(& $discord))
$l = Get-LigneFermer $c 'Discord.exe'
Test-Ok ($l.Fermable -and @($l.Cibles).Count -eq 1 -and $l.Cibles[0].Pid -eq 800 -and $l.Cibles[0].Debut -eq 1800 -and $l.Cibles[0].Chemin -like '*Discord.exe') "Fermer : Discord (logiciel ordinaire) n'a pas de bouton ($(@($l.Refus) -join ' / '))"
foreach ($cas in @(@('chrome.exe', $true), @('msedge.exe', $true), @('MonLogiciel.exe', $true), @('LightingService.exe', $true), @('Outil.exe', $true),
                   @('svchost.exe', $false), @('armsvc.exe', $false), @('updater.exe', $false), @('HPSysInfoCap.exe', $false), @('MsMpEng.exe', $false), @('explorer.exe', $false), @('System.exe', $false))) {
    $l = Get-LigneFermer $c $cas[0]
    Test-Ok ($l -and [bool]$l.Fermable -eq $cas[1]) "Fermer : $($cas[0]) $(if ($cas[1]) { 'n''a pas' } else { 'a' }) de bouton ($(if ($l) { @($l.Refus) -join ' / ' } else { 'ligne absente' }))"
}
# Toute l'appli : Chrome (2 processus) a les deux en cibles ; un seul d'un autre compte suffit à retirer le bouton
$l = Get-LigneFermer $c 'chrome.exe'
Test-Ok (@($l.Cibles).Count -eq 2 -and ((@($l.Cibles | ForEach-Object { $_.Pid } | Sort-Object)) -join ',') -eq '20,21') "Fermer : Chrome, pas toute l'appli en cibles ($(@($l.Cibles | ForEach-Object { $_.Pid }) -join ','))"
$d = New-FauxPcFermer @(); ($d.Processus | Where-Object { $_.Pid -eq 21 }).Compte = 'S-1-5-21-AUTRE-1002'
$l = Get-LigneFermer (Get-ConstatMemoire $d) 'chrome.exe'
Test-Ok (-not $l.Fermable -and @($l.Refus) -match 'compte') "Fermer : Chrome dont un processus est à un autre compte a un bouton ($(@($l.Refus) -join ' / '))"
# Chrome et Chrome Canary (même nom de programme, autre dossier) : deux lignes, chacune ne ferme que son programme
$c2 = Get-ConstatMemoire (New-FauxPcFermer @(New-Proc 30 'chrome.exe' 150 'C:\Users\x\AppData\Local\Google\Chrome SxS\Application\chrome.exe' '' 'Google Chrome'))
$lignesChrome = @(@($c2.Applis) | Where-Object { @($_.Exes) -contains 'chrome.exe' })
Test-Ok ($lignesChrome.Count -eq 2 -and -not @($lignesChrome | Where-Object { @($_.Cibles | ForEach-Object { [IO.Path]::GetDirectoryName($_.Chemin) } | Select-Object -Unique).Count -ne 1 }).Count) 'Fermer : Chrome et Chrome Canary dans la même ligne (fermés ensemble)'
# Chaque catégorie interdite, sur Discord : le détail qui change, et ce que la règle doit dire
$casInterdits = @(
    ,@('autre session', { param($d, $p) $p.Session = 2 }, 'session')
    ,@('session illisible', { param($d, $p) $p.Session = $null }, 'session')
    ,@('autre compte', { param($d, $p) $p.Compte = 'S-1-5-21-AUTRE-1002' }, 'compte')
    ,@('compte illisible', { param($d, $p) $p.Compte = '' }, 'compte')
    ,@('processus protégé', { param($d, $p) $p.Protection = 1 }, 'protégé')
    ,@('protection illisible', { param($d, $p) $p.Protection = -1 }, 'protégé')
    ,@('lancé en administrateur', { param($d, $p) $p.Eleve = 1 }, 'administrateur')
    ,@('administrateur illisible', { param($d, $p) $p.Eleve = -1 }, 'administrateur')
    ,@('poignée pas gardée', { param($d, $p) $p.Debut = [long]0 }, 'vérifié')
    ,@('chemin illisible', { param($d, $p) $p.Chemin = ''; $p.Image = '' }, 'illisible')
    ,@('service', { param($d, $p) $d.Services = @($d.Services) + @(New-Svc 'DiscordSvc' 'Discord Service' 800) }, 'service')
    ,@('services illisibles', { param($d, $p) $d.Services = @() }, 'service')
    ,@('dossier de Windows', { param($d, $p) $p.Chemin = 'C:\Windows\Discord\Discord.exe'; $p.Image = $p.Chemin }, 'Windows')
    ,@('nom d''un programme de Windows', { param($d, $p) $p.Nom = 'dwm.exe'; $p.Chemin = 'C:\Program Files\Discord\dwm.exe'; $p.Image = $p.Chemin }, 'Windows')
    ,@('antivirus inscrit (son dossier)', { param($d, $p) $p.Chemin = 'C:\Program Files\AVX\Moteur\avxui.exe'; $p.Image = $p.Chemin }, 'antivirus')
    ,@('Microsoft Defender', { param($d, $p) $p.Chemin = 'C:\ProgramData\Microsoft\Windows Defender\Platform\4.18.2\MpCopyAccelerator.exe'; $p.Image = $p.Chemin }, 'antivirus')
    ,@('appli du Microsoft Store', { param($d, $p) $p.Chemin = 'C:\Program Files\WindowsApps\SpotifyAB.Spotify_1.2.3\Spotify.exe'; $p.Image = $p.Chemin }, 'Store')
    ,@('suite : Program Files\Mindset', { param($d, $p) $p.Chemin = 'C:\Program Files\Mindset\Evermind\Discord.exe'; $p.Image = $p.Chemin }, 'suite')
    ,@('suite : X:\Mindset\Programmes', { param($d, $p) $p.Chemin = 'E:\Mindset\Programmes\Nevermind\Discord.exe'; $p.Image = $p.Chemin }, 'suite')
    ,@('suite : assistant de Mindcare', { param($d, $p) $p.Chemin = 'E:\Mindset\Donnees\Mindcare\ia\Discord.exe'; $p.Image = $p.Chemin }, 'suite')
    ,@('suite : assistant de Mindcare (profil)', { param($d, $p) $p.Chemin = 'C:\Users\x\AppData\Local\Mindset\Mindcare\ia\Discord.exe'; $p.Image = $p.Chemin }, 'suite')
    ,@('Nevermind lui-même', { param($d, $p) $d.Moi.Id = 800 }, 'suite')
    ,@('lancé par Nevermind (sa WebView2)', { param($d, $p) $p.Parent = 9000; $d.Processus = @($d.Processus) + @(@{ Pid = 9000; Parent = 0; Nom = 'Mindset.exe'; Chemin = 'C:\Dev\Mindset.exe'; Octets = 1MB; Session = 1; Compte = $moi.Compte; Protection = 0; Eleve = 0; Debut = [long]0 }) }, 'suite')
    ,@('lancé par une appli de la suite', { param($d, $p) $p.Parent = 650; $d.Processus = @($d.Processus) + @(@{ Pid = 650; Parent = 0; Nom = 'Mindset.exe'; Chemin = 'C:\Program Files\Mindset\Mindcare\Mindset.exe'; Octets = 1MB; Session = 1; Compte = $moi.Compte; Protection = 0; Eleve = 0; Debut = [long]1650 }) }, 'suite')
    ,@('Nevermind illisible', { param($d, $p) $d.Moi.Session = -1 }, 'inconnu')
)
foreach ($cas in $casInterdits) {
    $d = New-FauxPcFermer @(& $discord); $p = @($d.Processus | Where-Object { $_.Pid -eq 800 })[0]
    & $cas[1] $d $p
    $ligne = @(@((Get-ConstatMemoire $d).Applis) | Where-Object { @($_.Cibles | ForEach-Object { $_.Pid }) -contains 800 -or @($_.Refus) -match '^(Discord|dwm) :' })[0]
    Test-Ok ($ligne -and -not $ligne.Fermable -and (@($ligne.Refus) -join ' ') -match $cas[2]) "Fermer, $($cas[0]) : bouton proposé ou mauvaise raison ($(if ($ligne) { @($ligne.Refus) -join ' / ' } else { 'ligne absente' }))"
}
# Contre-épreuves : administrateur permis si Nevermind l'est aussi ; un Mindset.exe hors des dossiers de la suite
# n'est pas protégé par la règle de l'emplacement ; un antivirus déclaré dans Windows ne protège pas tout System32
$d = New-FauxPcFermer @(& $discord); $d.Moi.Eleve = 1; ($d.Processus | Where-Object { $_.Pid -eq 800 }).Eleve = 1
Test-Ok ((Get-LigneFermer (Get-ConstatMemoire $d) 'Discord.exe').Fermable) 'Fermer : programme en administrateur refusé alors que Nevermind l''est aussi'
$d = New-FauxPcFermer @(New-Proc 810 'Mindset.exe' 90 'C:\Copies\Mindset.exe'); $d.Processus = @($d.Processus | Where-Object { $_.Pid -notin 600, 601 })
Test-Ok ((Get-LigneFermer (Get-ConstatMemoire $d) 'Mindset.exe').Fermable) 'Fermer : un Mindset.exe hors des dossiers de la suite est protégé par son nom'
$d = New-FauxPcFermer @(); ($d.Processus | Where-Object { $_.Pid -eq 601 }).Chemin = 'D:\Mindset\Programmes\Nevermind\Mindset.exe'; ($d.Processus | Where-Object { $_.Pid -eq 601 }).Image = 'D:\Mindset\Programmes\Nevermind\Mindset.exe'
Test-Ok (-not (Get-LigneFermer (Get-ConstatMemoire $d) 'Mindset.exe').Fermable) 'Fermer : les applis de la suite (dossier Mindset\Programmes) ont un bouton'
$fakeAv = @{ Antivirus = @('C:\Windows\System32\avxreport.exe'); Windows = 'C:\Windows'; ServicesPid = [Collections.Generic.HashSet[int]]::new(); Reference = $moi }
Test-Ok (-not (Get-RefusFermeture $fakeAv @{ Id = 50; Chemin = 'C:\Program Files\Outil\outil.exe'; Session = 1; Compte = $moi.Compte; Protection = 0; Eleve = 0 })) 'Règle : un antivirus déclaré dans System32 protège n''importe quel programme'
# Les lignes que Nevermind ne ferme jamais, même si chaque processus passe la règle : Windows (étiquette), WebView2,
# un antivirus connu par son nom, un service
$d = New-FauxPcFermer @(New-Proc 820 'msedgewebview2.exe' 200 'C:\Program Files (x86)\Microsoft\EdgeWebView\Application\1.0\msedgewebview2.exe' '' 'Microsoft Edge WebView2'
                        New-Proc 830 'avastui.exe' 150 'C:\Program Files\Avast Software\Avast\avastui.exe' '' 'Avast Antivirus'
                        New-Proc 840 'nvcontainer.exe' 90 'C:\Program Files\NVIDIA Corporation\NvContainer\nvcontainer.exe' '' 'NVIDIA Container')
$c3 = Get-ConstatMemoire $d
foreach ($e in 'msedgewebview2.exe', 'avastui.exe', 'nvcontainer.exe') { Test-Ok (-not (Get-LigneFermer $c3 $e).Fermable) "Fermer : $e a un bouton" }
# Données sans fiches (lecture ratée, ou ancienne lecture) : aucun bouton
Test-Ok (-not @(@((Get-ConstatMemoire (New-FauxPc)).Applis) | Where-Object { $_.Fermable }).Count) 'Fermer : un bouton alors que rien n''a pu être vérifié'
# L'Aide passe par la même règle : elle refuse aussi un programme du Store ou de la suite, mais ferme un programme de
# l'utilisateur lancé en administrateur (-ElevePermis)
$ctxA = @{ Antivirus = @(); Windows = 'C:\Windows'; ServicesPid = [Collections.Generic.HashSet[int]]::new(); Reference = $moi }
$ficheA = @{ Id = 70; Chemin = 'C:\Program Files\Outil\outil.exe'; Session = 1; Compte = $moi.Compte; Protection = 0; Eleve = 1 }
Test-Ok ((Get-RefusFermeture $ctxA $ficheA) -match 'administrateur' -and -not (Get-RefusFermeture $ctxA $ficheA -ElevePermis)) 'Règle : -ElevePermis (l''Aide) mal pris en compte'
$ficheA.Chemin = 'C:\Program Files\WindowsApps\X_1.0\x.exe'
Test-Ok ((Get-RefusFermeture $ctxA $ficheA -ElevePermis) -match 'Store') 'Règle : l''Aide fermerait une appli du Store'

# Les jeux : jamais de bouton (un clic malheureux en pleine partie) ; les lanceurs oui, sauf pendant une partie
$monJeu = { New-Proc 850 'MonJeu.exe' 900 'D:\Jeux\MonJeu\MonJeu.exe' '' 'Mon Jeu' }
$steamL = { New-Proc 860 'steam.exe' 60 'C:\Program Files (x86)\Steam\steam.exe' '' 'Steam' }
$epicL = { New-Proc 861 'EpicGamesLauncher.exe' 70 'C:\Program Files (x86)\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe' '' 'Epic Games Launcher' }
function Get-ConstatJeux([array]$Procs, $Jeux) { $d = New-FauxPcFermer $Procs; $d.Jeux = $Jeux; Get-ConstatMemoire $d }
$sansJeu = @{ Exes = @(); Noms = @(); EnCours = 0 }
$casJeux = @(
    ,@('jeu installé (bibliothèque)', @{ Exes = @('d:\jeux\monjeu\monjeu.exe'); Noms = @(); EnCours = 0 })
    ,@('jeu en cours', @{ Exes = @(); Noms = @(); EnCours = 850 })
    ,@('jeu appris par la mesure des FPS', @{ Exes = @(); Noms = @('monjeu'); EnCours = 0 })
)
foreach ($cas in $casJeux) {
    $l = Get-LigneFermer (Get-ConstatJeux @(& $monJeu) $cas[1]) 'MonJeu.exe'
    Test-Ok (-not $l.Fermable -and (@($l.Refus) -join ' ') -match 'jeu' -and $l.Note -eq 'Ton jeu : Nevermind ne le ferme pas.') "Jeux, $($cas[0]) : bouton proposé ou note fausse ($(@($l.Refus) -join ' / ') ; $($l.Note))"
}
$l = Get-LigneFermer (Get-ConstatJeux @(& $monJeu) $sansJeu) 'MonJeu.exe'
Test-Ok ($l.Fermable) "Jeux, contre-épreuve : un programme inconnu hors des dossiers de jeux n'a pas de bouton ($(@($l.Refus) -join ' / '))"
Test-Ok ((Get-LigneFermer (Get-ConstatJeux @(& $monJeu) @{ Exes = @('d:\jeux\autre\monjeu.exe'); Noms = @('autre'); EnCours = 851 }) 'MonJeu.exe').Fermable) 'Jeux, contre-épreuve : un autre jeu (autre chemin, autre nom, autre numéro) retire le bouton'
foreach ($dossier in 'C:\Steam\steamapps\common\Inconnu', 'D:\Epic Games\Inconnu', 'C:\Program Files\EA Games\Inconnu', 'C:\Riot Games\Inconnu', 'C:\XboxGames\Inconnu\Content', 'C:\GOG Games\Inconnu') {
    $l = Get-LigneFermer (Get-ConstatJeux @(New-Proc 870 'inconnu.exe' 300 "$dossier\inconnu.exe") $sansJeu) 'inconnu.exe'
    Test-Ok (-not $l.Fermable -and $l.Note -like 'Ton jeu*') "Jeux : programme inconnu sous $dossier a un bouton"
}
Test-Ok ((Get-LigneFermer (Get-ConstatJeux @(New-Proc 870 'inconnu.exe' 300 'C:\Program Files\Inconnu\inconnu.exe') $sansJeu) 'inconnu.exe').Fermable) 'Jeux, contre-épreuve : le même programme hors des dossiers de jeux n''a pas de bouton'
# Les lanceurs : bouton sans jeu (même rangé dans un dossier de jeux, comme Epic) ; pas de bouton pendant une partie
$c = Get-ConstatJeux @((& $steamL), (& $epicL)) $sansJeu
Test-Ok ((Get-LigneFermer $c 'steam.exe').Fermable -and (Get-LigneFermer $c 'EpicGamesLauncher.exe').Fermable) 'Lanceurs sans jeu en cours : pas de bouton'
foreach ($jeuxEnCours in @(@{ Exes = @(); Noms = @(); EnCours = 850 }, $sansJeu)) {
    $procs = @((& $steamL), (& $epicL), (& $monJeu))
    if (-not $jeuxEnCours.EnCours) { $procs += New-Proc 871 'jeu.exe' 500 'C:\Program Files (x86)\Steam\steamapps\common\Jeu\jeu.exe' }   # (un jeu reconnu à son dossier)
    $c = Get-ConstatJeux $procs $jeuxEnCours
    foreach ($e in 'steam.exe', 'EpicGamesLauncher.exe') {
        $l = Get-LigneFermer $c $e
        Test-Ok (-not $l.Fermable -and $l.Note -match 'Sert à ton jeu en cours') "Lanceur $e pendant une partie : bouton proposé ou note absente ($($l.Note))"
    }
}
# Au clic, un jeu lancé depuis la lecture : le lanceur n'est pas fermé
$cl = Get-ConstatJeux @(& $steamL) $sansJeu
$script:Kit = @{ Memoire = $cl }; $script:MemoireJob = $null; $script:MemoireFermeture = $null
$script:GameSession = @{ Pid = 999; Game = 'Mon Jeu' }
$messagesJeu = New-Object System.Collections.ArrayList
function Show-MessageVue([string]$Texte) { [void]$messagesJeu.Add($Texte) }
function Confirm-Action([string]$Text) { [void]$messagesJeu.Add("confirmer:$Text"); $false }
Start-FermetureMemoire ([array]::IndexOf(@($cl.Applis), (Get-LigneFermer $cl 'steam.exe')))
Test-Ok ($messagesJeu.Count -eq 1 -and $messagesJeu[0] -match 'sert à ton jeu en cours' -and -not $script:MemoireFermeture) "Lanceur, jeu lancé depuis la lecture : fermeture proposée ($($messagesJeu -join ' | '))"
$messagesJeu.Clear(); $script:GameSession = $null
Start-FermetureMemoire ([array]::IndexOf(@($cl.Applis), (Get-LigneFermer $cl 'steam.exe')))
Test-Ok ($messagesJeu.Count -eq 1 -and $messagesJeu[0] -like 'confirmer:Fermer Steam*') "Lanceur sans jeu : pas de confirmation de fermeture ($($messagesJeu -join ' | '))"
# Ce que Nevermind sait des jeux, sans rien recalculer : bibliothèque déjà lue, jeux appris, jeu en cours
$script:Games = @(@{ Name = 'Mon Jeu'; Exes = @('D:\Jeux\MonJeu\MonJeu.exe') }); $script:GameSession = @{ Pid = 850 }
function Get-FpsVeilleJeux { @('bf6.exe') }
$j = Get-JeuxMemoire
Test-Ok (@($j.Exes) -contains 'd:\jeux\monjeu\monjeu.exe' -and @($j.Noms) -contains 'bf6' -and $j.EnCours -eq 850) "Jeux connus de Nevermind mal lus ($(@($j.Exes) -join ',') / $(@($j.Noms) -join ',') / $($j.EnCours))"
$script:Games = $null; $script:GameSession = $null

# La page : « Fermer » sur la ligne fermable, avec le bon numéro ; pas sur Windows ; plus aucun pendant une fermeture
$c = Get-ConstatMemoire (New-FauxPcFermer @(& $discord))
$p = Get-PageMemoireKit $c
$bDiscord = @($p.blocs | Where-Object { $_.type -eq 'ligne' -and $_.texte -like 'Discord*' })[0].bouton
Test-Ok ($bDiscord -and $bDiscord.texte -eq 'Fermer' -and $bDiscord.id -match '^memoire-fermer:(\d+)$' -and @($c.Applis)[[int]$Matches[1]].Nom -eq 'Discord') "Page : bouton « Fermer » de Discord absent ou mal numéroté ($($bDiscord.id))"
Test-Ok (-not @($p.blocs | Where-Object { $_.type -eq 'ligne' -and $_.etiquette -eq 'Windows en a besoin' -and $_.bouton }).Count) 'Page : « Fermer » sur une appli de Windows'
$p = Get-PageMemoireKit $c '' $true
Test-Ok (-not @($p.blocs | Where-Object { $_.bouton -and $_.bouton.id -like 'memoire-fermer:*' }).Count) 'Page : « Fermer » proposé pendant qu''une appli se ferme'
Test-Ok (($p | ConvertTo-Json -Depth 10 -Compress) -notmatch '\\u2013|\\u2014|[\u2013\u2014]') 'Page : tiret long'

# Le déroulé (sans rien fermer : le travail de fond et l'état des processus sont simulés) : confirmation en mots
# simples, puis selon ce qui reste au bout de 5 s
Add-Type -AssemblyName WindowsBase
$vus = New-Object System.Collections.ArrayList
$script:ReponseQuestion = 'non'; $script:ReponseConfirmer = $true; $script:Vivants = @(); $script:EtatSimule = 'question'
function Confirm-Action([string]$Text) { [void]$vus.Add("confirmer:$Text"); $script:ReponseConfirmer }
function Read-QuestionVue([string]$Texte, [string]$Titre, [array]$Boutons) { [void]$vus.Add("question:$Texte|$(@($Boutons | ForEach-Object { $_.texte }) -join ',')"); $script:ReponseQuestion }
function Show-Message([string]$Text) { [void]$vus.Add("info:$Text") }
function Show-MessageVue([string]$Texte) { [void]$vus.Add("message:$Texte") }
function Start-TravailFermeture($Cibles, [string]$Mode) { [void]$vus.Add("travail:${Mode}:$(@($Cibles | ForEach-Object { $_.Pid }) -join ',')"); @{ Handle = @{ IsCompleted = $true } } }
function Get-JobResult($Job) { , @($script:ResultatTravail) }
function Test-VivantFermeture($Cible) { $script:Vivants -contains [int]$Cible.Pid }
function Get-EtatFermeture($Restants) { $script:EtatSimule }
function Add-History([string]$Titre, [string[]]$Details, $Log) { [void]$vus.Add(@{ Titre = $Titre; Details = @($Details); Log = @($Log) }) }
function Start-ConstatMemoire { [void]$vus.Add('relecture') }
$script:MemoireJob = $null; $script:MemoireFermeture = $null; $script:MemoireCode = 'code'
$d = New-FauxPcFermer @(); $script:Kit.Memoire = Get-ConstatMemoire $d
$iChrome = [array]::IndexOf(@($script:Kit.Memoire.Applis), (Find-Appli $script:Kit.Memoire 'chrome.exe'))
function Invoke-Deroule([string]$Etat, [int[]]$Vivants, [string]$Reponse) {
    $vus.Clear(); $script:EtatSimule = $Etat; $script:ReponseQuestion = $Reponse; $script:Vivants = @(20, 21)
    Start-FermetureMemoire $iChrome
    $c = @($script:Kit.Memoire.Applis)[$iChrome].Cibles
    $script:ResultatTravail = @{ Mode = 'poli'; Faits = @($c); Deja = @(); Refuses = @(); Journal = @() }
    Update-FermetureMemoire                                   # (le travail de fond est fini : on attend)
    $script:Vivants = $Vivants
    if ($script:MemoireFermeture) { $script:MemoireFermeture.Debut = (Get-Date).AddSeconds(-6) }   # (5 s sont passées)
    Update-FermetureMemoire
    if ($script:MemoireFermeture -and $script:MemoireFermeture.Mode -eq 'force') {   # (forcé : le travail, puis l'attente)
        $script:ResultatTravail = @{ Mode = 'force'; Faits = @($c | Where-Object { $Vivants -contains $_.Pid }); Deja = @(); Refuses = @(); Journal = @() }
        Update-FermetureMemoire
        $script:Vivants = @(); Update-FermetureMemoire
    }
    try { $script:FermetureTimer.Stop() } catch {}
}
# Fermée poliment : confirmation, historique (nom, chemin, polie, mémoire, jamais un titre de fenêtre), relue
Invoke-Deroule 'question' @() 'non'
$conf = @($vus | Where-Object { $_ -is [string] -and $_ -like 'confirmer:*' })
Test-Ok ($conf.Count -eq 1 -and $conf[0] -match '^confirmer:Fermer Google Chrome \? Environ 400 Mo libérés\.' -and $conf[0] -match 'croix') "Déroulé : confirmation « $conf »"
Test-Ok (@($vus | Where-Object { $_ -is [string] -and $_ -like 'travail:poli:*' })[0] -eq 'travail:poli:20,21') "Déroulé : pas toute l'appli fermée poliment ($($vus -join ' | '))"
$h = @($vus | Where-Object { $_ -is [hashtable] })[0]
Test-Ok ($h -and $h.Titre -eq 'Fermé : Google Chrome' -and @($h.Details) -contains 'C:\Program Files\Google\Chrome\Application\chrome.exe' -and (@($h.Details) -join ' ') -match 'polie' -and (@($h.Details) -join ' ') -match '400 Mo.*estimation' -and @($h.Log)[0].Type -eq 'note') "Déroulé : historique ($(if ($h) { "$($h.Titre) / $(@($h.Details) -join ' / ')" }))"
Test-Ok ($vus -contains 'relecture' -and @($vus | Where-Object { $_ -is [string] -and $_ -like 'message:*fermé*estimation*' }).Count -eq 1 -and -not $script:MemoireFermeture) "Déroulé : pas de message, pas de relecture, ou fermeture pas finie ($($vus -join ' | '))"
# Il pose une question (« Enregistrer ? ») : le dire, sans pousser à forcer
Invoke-Deroule 'question' @(20) 'forcer'
Test-Ok (@($vus | Where-Object { $_ -is [string] -and $_ -like 'info:Google Chrome te pose une question*' }).Count -eq 1 -and -not @($vus | Where-Object { $_ -is [string] -and $_ -like 'question:*' -or $_ -like 'travail:force*' }).Count) "Déroulé, question : « Forcer » proposé ou pas de message ($($vus -join ' | '))"
# Il ne répond plus : « Forcer la fermeture » proposé, avec l'avertissement ; accepté, toute l'appli restante est forcée
Invoke-Deroule 'bloquee' @(20, 21) 'forcer'
$q = @($vus | Where-Object { $_ -is [string] -and $_ -like 'question:*' })
Test-Ok ($q.Count -eq 1 -and $q[0] -match 'ne répond plus' -and $q[0] -match 'sera perdu' -and $q[0] -match 'Forcer la fermeture,Laisser ouvert') "Déroulé, bloquée : question « $q »"
Test-Ok (@($vus | Where-Object { $_ -is [string] -and $_ -eq 'travail:force:20,21' }).Count -eq 1) "Déroulé, bloquée : pas forcée ($($vus -join ' | '))"
$h = @($vus | Where-Object { $_ -is [hashtable] })[0]
Test-Ok ($h -and (@($h.Details) -join ' ') -match 'forcée') 'Déroulé, bloquée : l''historique ne dit pas « forcée »'
# Refusé : rien n'est forcé ni noté
Invoke-Deroule 'bloquee' @(20, 21) 'non'
Test-Ok (-not @($vus | Where-Object { $_ -is [string] -and $_ -like 'travail:force*' }).Count -and -not @($vus | Where-Object { $_ -is [hashtable] }).Count -and $vus -contains 'relecture') "Déroulé, « Laisser ouvert » : forcé ou noté ($($vus -join ' | '))"
# Rangée près de l'horloge : dit en mots simples, « Forcer » proposé
Invoke-Deroule 'rangee' @(20, 21) 'non'
Test-Ok (@($vus | Where-Object { $_ -is [string] -and $_ -like "question:*rangé près de l'horloge*Forcer la fermeture*" }).Count -eq 1) "Déroulé, rangée : message ($($vus -join ' | '))"
# Fermée en partie (un processus reste après « Laisser ouvert ») : noté « en partie »
Invoke-Deroule 'rangee' @(21) 'non'
$h = @($vus | Where-Object { $_ -is [hashtable] })[0]
Test-Ok ($h -and $h.Titre -eq 'Fermé en partie : Google Chrome') "Déroulé, en partie : historique « $(if ($h) { $h.Titre }) »"
# Confirmation refusée : rien
$vus.Clear(); $script:ReponseConfirmer = $false; Start-FermetureMemoire $iChrome; $script:ReponseConfirmer = $true
Test-Ok (-not @($vus | Where-Object { $_ -is [string] -and $_ -like 'travail:*' }).Count -and -not $script:MemoireFermeture) 'Déroulé : fermé sans confirmation'
# Une ligne sans bouton : rien, même appelée directement
$vus.Clear(); Start-FermetureMemoire ([array]::IndexOf(@($script:Kit.Memoire.Applis), (Find-Appli $script:Kit.Memoire 'explorer.exe')))
Test-Ok (-not $vus.Count) 'Déroulé : une appli de Windows peut être fermée en appelant le geste directement'
Test-Ok ((Get-MessageFermeture 'Discord' 'rangee').Forcer -and -not (Get-MessageFermeture 'Discord' 'question').Forcer -and (Get-MessageFermeture 'Discord' 'bloquee').Forcer) 'Messages : « Forcer » proposé au mauvais moment'
foreach ($e in 'rangee', 'question', 'bloquee') { Test-Ok ((Get-MessageFermeture 'Discord' $e).Texte -notmatch '[\u2013\u2014]') "Message $e : tiret long" }

# ---------------------------------------------------------------------------
# 8. Fermer pour de vrai, seulement les programmes d'essai de ce test : un petit programme compilé ici, lancé caché ou
#    avec une fenêtre hors de l'écran (jamais activée), dans un dossier temporaire. Même énumération des fenêtres, même
#    poignée gardée et même règle que la page RAM. Jamais un programme de l'utilisateur.
# ---------------------------------------------------------------------------
$codeEssai = @'
using System;
using System.Drawing;
using System.Threading;
using System.Windows.Forms;
public class FenetreEssai : Form {
    string mode; bool demande;
    public FenetreEssai(string m, string titre) {
        mode = m; Text = titre; StartPosition = FormStartPosition.Manual; Location = new Point(-32000, -32000); Size = new Size(160, 60);
        ShowInTaskbar = false; FormBorderStyle = FormBorderStyle.FixedToolWindow;
    }
    protected override bool ShowWithoutActivation { get { return true; } }
    protected override void OnShown(EventArgs e) {
        base.OnShown(e);
        if (mode == "bloque") BeginInvoke(new MethodInvoker(delegate { Thread.Sleep(Timeout.Infinite); }));
    }
    protected override void OnFormClosing(FormClosingEventArgs e) {
        if (mode == "question") {   // (WM_CLOSE venu d'un autre programme : CloseReason.None)
            e.Cancel = true;
            if (!demande) { demande = true; new FenetreEssai("rien", "Enregistrer ?").Show(); }
            return;
        }
        if (mode == "range") { e.Cancel = true; Hide(); return; }
        base.OnFormClosing(e);
    }
    [STAThread]
    public static void Main(string[] a) {
        var garde = new Thread(delegate() { Thread.Sleep(120000); Environment.Exit(0); });   // (jamais oublié plus de 2 minutes)
        garde.IsBackground = true; garde.Start();
        Application.Run(new FenetreEssai(a.Length > 0 ? a[0] : "fenetre", "Essai Nevermind"));
    }
}
'@
. $fichier   # (les vraies fonctions, à la place de celles simulées plus haut)
$tmpFermer = Join-Path $env:TEMP "nevermind-test-memoire-fermer-$PID"
$essais = @{}
try {
    [void][IO.Directory]::CreateDirectory($tmpFermer)
    $exeEssai = Join-Path $tmpFermer 'nm-essai-fermer.exe'
    Add-Type -TypeDefinition $codeEssai -ReferencedAssemblies System.Windows.Forms, System.Drawing -OutputAssembly $exeEssai -OutputType WindowsApplication
    $exeEssai = (Get-Item -LiteralPath $exeEssai).FullName
    foreach ($m in 'bloque', 'fenetre', 'question', 'range', 'perime', 'service') {
        $psi = New-Object Diagnostics.ProcessStartInfo($exeEssai, $(if ($m -in 'perime', 'service') { 'fenetre' } else { $m }))
        $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true
        $essais[$m] = [Diagnostics.Process]::Start($psi)
    }
    $debutEssais = Get-Date
    Initialize-AideProcessus; Initialize-FermetureMemoire
    # (les fenêtres d'essai s'ouvrent)
    $limite = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $limite -and @($essais.Values | Where-Object { -not @([NmFermeture]::Fenetres($_.Id, $true)).Count }).Count) { Start-Sleep -Milliseconds 200 }
    $ctxVrai = @{ Antivirus = @(); Windows = $env:windir; ServicesPid = [Collections.Generic.HashSet[int]]::new(); Reference = (Get-AideReference $PID) }
    $cibles = @{}
    foreach ($m in @($essais.Keys)) { $cibles[$m] = @{ Pid = $essais[$m].Id; Debut = [NmFermeture]::Garder($essais[$m].Id); Chemin = $exeEssai; Octets = 1MB } }
    Test-Ok (-not @($cibles.Values | Where-Object { -not $_.Debut }).Count) 'Essai : la poignée d''un programme d''essai n''a pas pu être gardée'
    Test-Ok (-not (Get-RefusFermeture $ctxVrai (Read-AideFiche $essais.fenetre.Id $exeEssai))) "Essai : la règle refuse un programme d'essai ordinaire ($(Get-RefusFermeture $ctxVrai (Read-AideFiche $essais.fenetre.Id $exeEssai)))"
    Test-Ok (@([NmFermeture]::Fenetres($essais.fenetre.Id, $false)).Count -ge 1) 'Essai : la fenêtre du programme d''essai n''est pas trouvée par l''énumération'
    # Numéro repris : une fiche périmée (même numéro, autre heure de démarrage) est refusée, polie comme forcée
    $perimee = $cibles.perime.Clone(); $perimee.Debut = [long]$perimee.Debut - 1
    $r = Invoke-FermetureMemoire @($perimee) $ctxVrai 'poli'
    $r2 = Invoke-FermetureMemoire @($perimee) $ctxVrai 'force'
    $autre = $cibles.perime.Clone(); $autre.Chemin = Join-Path $tmpFermer 'autre.exe'
    $r3 = Invoke-FermetureMemoire @($autre) $ctxVrai 'force'
    $sansPoignee = @{ Pid = $PID; Debut = [long]1; Chemin = $exeEssai }
    $r4 = Invoke-FermetureMemoire @($sansPoignee) $ctxVrai 'force'
    Start-Sleep -Milliseconds 800
    Test-Ok (-not $r.Faits.Count -and $r.Refuses.Count -eq 1 -and -not $r2.Faits.Count -and $r2.Refuses.Count -eq 1 -and -not $essais.perime.HasExited) "Essai : une fiche périmée (numéro repris) n'est pas refusée ($($r.Faits.Count) / $($r2.Faits.Count), fermé : $($essais.perime.HasExited))"
    Test-Ok (-not $r3.Faits.Count -and -not $essais.perime.HasExited) 'Essai : un autre chemin de programme n''est pas refusé'
    Test-Ok (-not $r4.Faits.Count -and $r4.Refuses.Count -eq 1) 'Essai : un processus sans poignée gardée n''est pas refusé'
    # La règle relue au moment de fermer : devenu « service », il n'est plus fermé
    $ctxService = $ctxVrai.Clone(); $ctxService.ServicesPid = [Collections.Generic.HashSet[int]]::new(); [void]$ctxService.ServicesPid.Add($essais.service.Id)
    $r = Invoke-FermetureMemoire @($cibles.service) $ctxService 'force'
    Start-Sleep -Milliseconds 500
    Test-Ok (-not $r.Faits.Count -and ($r.Refuses[0].Raison -match 'service') -and -not $essais.service.HasExited) 'Essai : la règle n''est pas relue juste avant de fermer (service fermé)'
    # Contre-épreuve : la bonne fiche est fermée (forcée)
    $r = Invoke-FermetureMemoire @($cibles.perime) $ctxVrai 'force'
    Test-Ok ($r.Faits.Count -eq 1 -and $essais.perime.WaitForExit(3000)) 'Essai : la bonne fiche n''est pas fermée de force'
    $r = Invoke-FermetureMemoire @($cibles.perime) $ctxVrai 'poli'
    Test-Ok ($r.Deja.Count -eq 1 -and -not $r.Faits.Count) 'Essai : un programme déjà fermé n''est pas vu comme tel'
    # Fermeture polie de tous les autres (le programme d'essai qui ne répond plus attend 6 s : Windows dit « ne répond
    # plus » au bout de 5 s)
    while (((Get-Date) - $debutEssais).TotalSeconds -lt 1) { Start-Sleep -Milliseconds 100 }
    $polis = @($cibles.fenetre, $cibles.question, $cibles.range, $cibles.bloque)
    $r = Invoke-FermetureMemoire $polis $ctxVrai 'poli'
    Test-Ok ($r.Faits.Count -eq 4) "Essai : fermeture polie refusée ($(@($r.Refuses | ForEach-Object { $_.Raison }) -join ' / '))"
    Test-Ok ($essais.fenetre.WaitForExit(5000)) 'Essai : le programme d''essai ordinaire ne s''est pas fermé poliment'
    $limite = $debutEssais.AddSeconds(7.5); while ((Get-Date) -lt $limite) { Start-Sleep -Milliseconds 200 }
    Test-Ok (-not (Test-VivantFermeture $cibles.fenetre) -and (Test-VivantFermeture $cibles.question)) 'Essai : Test-VivantFermeture se trompe'
    $etats = @{}; foreach ($m in 'question', 'range', 'bloque') { $etats[$m] = Get-EtatFermeture @($cibles[$m]) }
    Test-Ok ($etats.question -eq 'question') "Essai « Enregistrer ? » : état « $($etats.question) » au lieu de « question »"
    Test-Ok ($etats.range -eq 'rangee') "Essai rangé près de l'horloge : état « $($etats.range) » au lieu de « rangee »"
    Test-Ok ($etats.bloque -eq 'bloquee') "Essai qui ne répond plus : état « $($etats.bloque) » au lieu de « bloquee »"
    $mq = Get-MessageFermeture 'Essai' $etats.question; $mr = Get-MessageFermeture 'Essai' $etats.range
    Test-Ok (-not $mq.Forcer -and $mq.Texte -match 'te pose une question' -and $mr.Forcer -and $mr.Texte -match "rangé près de l'horloge") 'Essai : message « question » ou « rangé » faux'
    # Forcée : ce qui restait se ferme
    $r = Invoke-FermetureMemoire @($cibles.range, $cibles.bloque, $cibles.question) $ctxVrai 'force'
    Test-Ok ($r.Faits.Count -eq 3 -and $essais.range.WaitForExit(3000) -and $essais.bloque.WaitForExit(3000) -and $essais.question.WaitForExit(3000)) 'Essai : la fermeture forcée n''a pas tout fermé'
} catch {
    Test-Ok $false "Essai de fermeture : erreur $($_.Exception.Message)"
} finally {
    foreach ($e in $essais.Values) { try { if (-not $e.HasExited) { $e.Kill() } } catch {}; try { [void]$e.WaitForExit(3000); $e.Dispose() } catch {} }   # (seulement nos programmes d'essai)
    try { [NmFermeture]::Relacher() } catch {}
    foreach ($x in @(try { [IO.Directory]::GetFiles($tmpFermer) } catch { })) { try { [IO.File]::Delete($x) } catch {} }
    try { [IO.Directory]::Delete($tmpFermer) } catch {}
}

# ---------------------------------------------------------------------------
# 9. Audit du 25/10 (constat 17) : « Fermer <programme> » (diagnostic des FPS), le blocage dans Trafic et le mode jeu
#    passent par la règle commune, avec le nom EXACT du programme. Programmes d'essai compilés ici (rien d'autre n'est
#    fermé : ils ont des noms qu'aucun programme du PC ne porte).
# ---------------------------------------------------------------------------
$astClient = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules 'aide-client.ps1'), [ref]$null, [ref]$null)
foreach ($n in 'Stop-ProcessusRegle', 'Stop-ProgrammeNom', 'Stop-ProgrammeChemin') {
    $def = $astClient.Find({ param($x) $x -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $x.Name -eq $n }, $true)
    if ($def) { . ([scriptblock]::Create($def.Extent.Text)) } else { Test-Ok $false "aide-client.ps1 : $n introuvable" }
}
# (seulement nos programmes d'essai : la liste des processus est filtrée sur leurs noms ; l'Aide n'est jamais appelée)
function script:Get-Process { [Diagnostics.Process]::GetProcesses() | Where-Object { $_.ProcessName -like 'nmfauxfermer*' } }
$script:AppelsAideFermer = New-Object System.Collections.Generic.List[string]
function script:Invoke-Aide([string]$Action, $P) { $script:AppelsAideFermer.Add("$Action $(ConvertTo-Json $P -Compress)") }
function script:Get-AideContexte { @{ Antivirus = @(); Windows = $env:windir; ServicesPid = [Collections.Generic.HashSet[int]]::new() } }
$tmpNom = Join-Path $env:TEMP "nmfn-$PID"
$fauxFermer = @{}
try {
    foreach ($d in 'Windows Defender', 'autre', 'a', 'b') { [void][IO.Directory]::CreateDirectory((Join-Path $tmpNom $d)) }
    $dort = Join-Path $tmpNom 'dort.exe'
    Add-Type -TypeDefinition 'public static class NmDort { public static void Main() { System.Threading.Thread.Sleep(90000); } }' -OutputAssembly $dort -OutputType WindowsApplication
    $places = [ordered]@{ av = 'Windows Defender\nmfauxfermer1.exe'; ord = 'autre\nmfauxfermer2.exe'; a = 'a\nmfauxfermer3.exe'; b = 'b\nmfauxfermer3.exe' }
    foreach ($k in $places.Keys) {
        $exe = Join-Path $tmpNom $places[$k]; [IO.File]::Copy($dort, $exe, $true)
        $psi = New-Object Diagnostics.ProcessStartInfo($exe); $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true
        $fauxFermer[$k] = [Diagnostics.Process]::Start($psi)
    }
    Start-Sleep -Milliseconds 500
    # Un motif n'est pas un nom : rien n'est fermé
    Stop-ProgrammeNom 'nmfauxfermer*'
    Start-Sleep -Milliseconds 300
    Test-Ok (-not @($fauxFermer.Values | Where-Object { $_.HasExited }).Count) 'Fermer par nom : « nmfauxfermer* » est pris pour un motif (des programmes sont fermés)'
    # Le programme dans un dossier « Windows Defender » : refusé par la règle commune
    $refus = ''
    try { Stop-ProgrammeNom 'nmfauxfermer1' } catch { $refus = $_.Exception.Message }
    Start-Sleep -Milliseconds 300
    Test-Ok (-not $fauxFermer.av.HasExited -and $refus -match 'antivirus') "Fermer par nom : un programme de l'antivirus n'est pas refusé (fermé : $($fauxFermer.av.HasExited), message : $refus)"
    # Contre-épreuve : un programme ordinaire, par son nom exact
    Stop-ProgrammeNom 'nmfauxfermer2'
    Test-Ok ($fauxFermer.ord.WaitForExit(3000)) 'Fermer par nom : un programme ordinaire n''est pas fermé'
    # Par chemin (blocage dans Trafic) : seulement ce programme-là, pas son homonyme d'un autre dossier
    Stop-ProgrammeChemin (Join-Path $tmpNom $places.a)
    Test-Ok ($fauxFermer.a.WaitForExit(3000) -and -not $fauxFermer.b.HasExited) 'Fermer par chemin : l''homonyme d''un autre dossier est fermé aussi (ou le bon ne l''est pas)'
    Test-Ok (-not $script:AppelsAideFermer.Count) "Fermer : l'Aide est appelée pour des programmes de l'utilisateur ($($script:AppelsAideFermer -join ' | '))"
    # Le mode jeu et les deux boutons n'ont plus de Kill() sans la règle
    $texteJeu = [IO.File]::ReadAllText((Join-Path $modules 'jeu.ps1'))
    Test-Ok ($texteJeu -match '(?s)function Start-GameSession\b.*?Stop-ProcessusRegle' -and $texteJeu -notmatch '(?s)function Start-GameSession\b[^}]*\.Kill\(\)') 'Mode jeu : les applis sont fermées sans la règle commune'
} catch {
    Test-Ok $false "Essai de fermeture par nom : erreur $($_.Exception.Message)"
} finally {
    foreach ($e in $fauxFermer.Values) { try { if (-not $e.HasExited) { $e.Kill() } } catch {}; try { [void]$e.WaitForExit(3000); $e.Dispose() } catch {} }   # (seulement nos programmes d'essai)
    Start-Sleep -Milliseconds 300
    try { [IO.Directory]::Delete($tmpNom, $true) } catch {}
}
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
