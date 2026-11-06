# Exporte les « juges » de la page RAM (memoire.ps1, Get-PageMemoireKit) pour la version Rust (rust\nevermind-memoire) :
# des PC simulés (ceux de outils\tester-memoire.ps1, des cas limites, et des centaines de PC tirés au hasard), et pour
# chacun ce que donne la version PowerShell : le constat (regroupements, étiquettes, « fermable »), la page du kit, le
# rapport et les changements déclarés. Rien n'est lu ni modifié sur le PC : tout est calculé sur de fausses données.
#
#   & 'outils\exporter-juges-memoire.ps1' [-Dll <NevermindPont.dll>] [-Hasard 300]
#
# Écrit rust\nevermind-memoire\tests\juges\*.json.

param([string]$Dll = '', [int]$Hasard = 200, [int]$Graine = 20261106)

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
$sortie  = Join-Path $racine 'rust\nevermind-memoire\tests\juges'
$env:NEVERMIND_TEST = '1'

# Sécurité : rien ne doit pouvoir lire ou modifier le vrai PC (une fonction passe avant une commande du même nom)
foreach ($c in 'Get-CimInstance', 'Get-WmiObject', 'Get-Process', 'Get-Service', 'Set-Service', 'Stop-Service', 'Stop-Process', 'Start-Process', 'Set-ItemProperty',
               'New-ItemProperty', 'Remove-ItemProperty', 'Invoke-Aide', 'Set-Reg', 'Clear-Reg', 'Set-ServiceOff', 'Set-Declarations', 'taskkill', 'taskkill.exe') {
    Set-Item -Path "function:script:$c" -Value ([scriptblock]::Create("throw 'Export : $c est interdit.'"))
}
function script:Write-Log([string]$Message) { }

. (Join-Path $modules 'pont.ps1')
. (Join-Path $modules 'processus.ps1')
if (-not $Dll) { $Dll = 'D:\Outils\Rust\target\nevermind-memoire\debug\nevermind_pont.dll' }
$raison = Initialize-Pont $Dll
if ($raison) { throw "NevermindPont.dll : $raison" }
. (Join-Path $modules 'memoire.ps1')
$tok = $null; $err = $null
foreach ($x in @(@('systeme', 'Format-Size'), @('ecran-perf', 'Get-PageMemoireKit'))) {
    $a = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$($x[0]).ps1"), [ref]$tok, [ref]$err)
    $def = $a.Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $x[1] }, $false)
    . ([scriptblock]::Create($def.Extent.Text))
}

function ConvertTo-JsonKit($O) { (($O | ConvertTo-Json -Depth 16 -Compress) -replace '":\{\}', '":null') }
function ConvertTo-JsonTexte([string]$T) { if ($null -eq $T) { 'null' } else { ConvertTo-Json $T -Compress } }

# ---------------------------------------------------------------------------
# Les faux PC de tester-memoire.ps1
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
$moi = @{ Id = 9000; Session = 1; Compte = 'S-1-5-21-ESSAI-1001'; Eleve = 0 }
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
function Get-Proc($D, [int]$Id) { @($D.Processus | Where-Object { $_.Pid -eq $Id })[0] }

# ---------------------------------------------------------------------------
# Un juge : les données, puis ce que donne la version PowerShell
# ---------------------------------------------------------------------------
$programmes = [Environment]::GetFolderPath('ProgramFiles')
function Get-Juge([string]$Nom, $D, [switch]$PeuDePages) {
    $c = Get-ConstatMemoire $D
    $entree = @{}
    foreach ($k in @($D.Keys)) { $entree[$k] = $D[$k] }
    if ($D.Date) { $entree.Date = ([datetime]$D.Date).ToString('s') }
    $entree.Programmes = $programmes
    $applis = @(foreach ($g in @($c.Applis)) {
        [ordered]@{ Cle = [string]$g.Cle; Nom = [string]$g.Nom; Kind = [string]$g.Kind; Note = [string]$g.Note; Etiquette = [string]$g.Etiquette
            Octets = [double]$g.Octets; Nb = [int]$g.Nb; Exes = @($g.Exes); Services = @($g.Services); Corrections = @($g.Corrections)
            Cibles = @(foreach ($x in @($g.Cibles)) { [ordered]@{ Pid = [long]$x.Pid; Debut = [long]$x.Debut; Chemin = [string]$x.Chemin; Octets = [double]$x.Octets } })
            Refus = @($g.Refus); Fermable = [bool]$g.Fermable; Jeu = [bool]$g.Jeu; Lanceur = [bool]$g.Lanceur; AuDemarrage = [bool]$g.AuDemarrage; ParWindows = [bool]$g.ParWindows }
    })
    $constat = [ordered]@{ Installee = [double]$c.Installee; InstalleeLue = [bool]$c.InstalleeLue; Visible = [double]$c.Visible; Reservee = [double]$c.Reservee
        Utilisee = [double]$c.Utilisee; Disponible = [double]$c.Disponible; Cache = [double]$c.Cache; PoolNP = [double]$c.PoolNP; PoolP = [double]$c.PoolP
        Compressee = [double]$c.Compressee; Pourcent = [int]$c.Pourcent; TotalApplis = [double]$c.TotalApplis; Reste = [double]$c.Reste
        Applis = $applis
        Alertes = @(foreach ($a in @($c.Alertes)) { [ordered]@{ Id = $a.Id; Ton = $a.Ton; Titre = $a.Titre; Texte = $a.Texte; Quoi = $a.Quoi } })
        Corrections = @(foreach ($x in @($c.Corrections)) { [ordered]@{ Id = $x.Id; Nom = $x.Nom } })
        Changements = @(foreach ($x in @(Get-ChangementsMemoire $c)) { [ordered]@{ Type = $x.Type; Name = $x.Name; Demarrage = $x.Demarrage } }) }
    $pages = [ordered]@{}
    $variantes = if ($PeuDePages) { @(@('', $false), @('', $true)) } else { @(@('', $false), @('pilotes', $false), @('donnees', $false), @('', $true)) }
    foreach ($v in $variantes) {
        $pages["$($v[0])|$($v[1])"] = Get-PageMemoireKit $c $v[0] $v[1]
    }
    $p = @(foreach ($k in $pages.Keys) { "$(ConvertTo-JsonTexte $k):$(ConvertTo-JsonKit $pages[$k])" }) -join ','
    "{`"nom`":$(ConvertTo-JsonTexte $Nom),`"donnees`":$(ConvertTo-JsonKit $entree),`"constat`":$(ConvertTo-JsonKit $constat),`"pages`":{$p},`"rapport`":$(ConvertTo-JsonTexte (Format-RapportMemoire $c)),`"fichier`":$(ConvertTo-JsonTexte ([IO.Path]::GetFileName((Join-Path 'x' "Nevermind, rapport mémoire du $($c.Date.ToString('dd-MM-yyyy')) à $($c.Date.ToString('HH\hmm')).txt"))))}"
}

$juges = New-Object System.Collections.Generic.List[string]
function Add-Juge([string]$Nom, $D, [switch]$PeuDePages) { $script:juges.Add((Get-Juge $Nom $D -PeuDePages:$PeuDePages)) }

# 1. Les cas de tester-memoire.ps1
Add-Juge 'faux PC' (New-FauxPc)
$d = New-FauxPc
$d.Processus = @($d.Processus) + @(
    New-Proc 800 'Discord.exe' 300 'C:\Users\x\AppData\Local\Discord\app-1.0\Discord.exe' '' 'Discord'
    New-Proc 810 'steam.exe' 50 'C:\Steam\steam.exe' '' 'Steam'
    New-Proc 811 'steamwebhelper.exe' 200 'C:\Steam\bin\steamwebhelper.exe' '' 'Steam Client WebHelper'
    New-Proc 820 'LEDKeeper2.exe' 100 'C:\Program Files (x86)\MSI\LEDKeeper2.exe' '' 'LEDKeeper2'
    New-Proc 830 'Code.exe' 400 'C:\Users\x\AppData\Local\Programs\Microsoft VS Code\Code.exe' '' 'Visual Studio Code')
foreach ($x in $d.Processus) { $x.Parent = switch ([int]$x.Pid) { 811 { 810 } 820 { 101 } default { 0 } } }
$d.Demarrage = @('update.exe', 'discord.exe', 'steam.exe', 'onedrive.exe')
Add-Juge 'origines (page Démarrage)' $d
$d.Demarrage = @(); Add-Juge 'origines sans page Démarrage' $d
$d = New-FauxPc; $d.Processus = @($d.Processus) + @(New-Proc 22 'chrome.exe' 50 'C:\Program Files\Google\Chrome\Application\chrome.exe' '"chrome.exe" --no-startup-window' 'Google Chrome')
Add-Juge 'Chrome en arrière-plan' $d
$d = New-FauxPc; $d.Processus = @($d.Processus | Where-Object { $_.Ligne -notmatch 'no-startup-window' }); Add-Juge 'sans lancement en avance' $d
foreach ($x in @(@(32GB, 1.3GB), @(32GB, 1.7GB), @(32GB, 0.7GB), @(16GB, 1.3GB), @(16GB, 0.9GB), @(16GB, 0.5GB), @(8GB, 1.1GB), @(8GB, 0.8GB), @(64GB, 1.9GB), @(64GB, 2.1GB))) {
    $d = New-FauxPc; $d.Installee = $x[0]; $d.Visible = $x[0] - 0.2GB; $d.PoolNP = $x[1]
    Add-Juge "pilotes $($x[1]) sur $($x[0])" $d
}
$et = @(
    @{ Tag = 'Ntfx'; Octets = 600MB; Pilotes = @(@{ Fichier = 'ntfs.sys'; Description = 'Pilote NT File System'; Societe = 'Microsoft Corporation' }) }
    @{ Tag = 'RTKx'; Octets = 450MB; Pilotes = @(@{ Fichier = 'rt640x64.sys'; Description = 'Realtek PCIe GbE Family Controller'; Societe = 'Realtek' }) }
)
foreach ($cas in @(@('deux étiquettes', $et), @('pilote de Windows seul', @($et[0])), @('étiquette sans pilote', @(@{ Tag = 'Xxxx'; Octets = 600MB; Pilotes = @() })),
                   @('pilote sans description', @(@{ Tag = 'Kil1'; Octets = 700MB; Pilotes = @(@{ Fichier = 'e2xw10x64.sys'; Description = ''; Societe = '' }) })))) {
    $d = New-FauxPc; $d.Installee = 16GB; $d.Visible = 15.8GB; $d.PoolNP = 1.3GB; $d.Etiquettes = $cas[1]; Add-Juge "alerte : $($cas[0])" $d
}
$d = New-FauxPc; $d.Installee = 16GB; $d.Visible = 14.5GB; Add-Juge 'réservé au matériel' $d
$d = New-FauxPc; $d.Installee = 0; Add-Juge 'mémoire installée illisible' $d
Add-Juge 'données vides' @{ Visible = 8GB; Disponible = 2GB; Date = [datetime]'2026-01-02 03:04' }
Add-Juge 'rien du tout' @{ Date = [datetime]'2026-01-02 03:04' }
$d = New-FauxPc; $d.Services = @($d.Services | Where-Object { $_.Nom -notin 'AdobeARMservice', 'GoogleUpdaterService140.0.1.0', 'HPSysInfoCap' }); $d.Processus = @($d.Processus | Where-Object { $_.Ligne -notmatch 'no-startup-window' })
Add-Juge 'rien à couper' $d

# 2. « Fermer » (tester-memoire.ps1, section 7)
$discord = { New-Proc 800 'Discord.exe' 300 'C:\Users\x\AppData\Local\Discord\app-1.0\Discord.exe' '' 'Discord' }
Add-Juge 'fermer : Discord' (New-FauxPcFermer @(& $discord))
$d = New-FauxPcFermer @(); (Get-Proc $d 21).Compte = 'S-1-5-21-AUTRE-1002'; Add-Juge 'fermer : Chrome dont un processus est à un autre compte' $d
Add-Juge 'fermer : Chrome et Chrome Canary' (New-FauxPcFermer @(New-Proc 30 'chrome.exe' 150 'C:\Users\x\AppData\Local\Google\Chrome SxS\Application\chrome.exe' '' 'Google Chrome'))
$casInterdits = @(
    ,@('autre session', { param($d, $p) $p.Session = 2 })
    ,@('session illisible', { param($d, $p) $p.Session = $null })
    ,@('autre compte', { param($d, $p) $p.Compte = 'S-1-5-21-AUTRE-1002' })
    ,@('compte illisible', { param($d, $p) $p.Compte = '' })
    ,@('processus protégé', { param($d, $p) $p.Protection = 1 })
    ,@('protection illisible', { param($d, $p) $p.Protection = -1 })
    ,@('lancé en administrateur', { param($d, $p) $p.Eleve = 1 })
    ,@('administrateur illisible', { param($d, $p) $p.Eleve = -1 })
    ,@('poignée pas gardée', { param($d, $p) $p.Debut = [long]0 })
    ,@('chemin illisible', { param($d, $p) $p.Chemin = ''; $p.Image = '' })
    ,@('service', { param($d, $p) $d.Services = @($d.Services) + @(New-Svc 'DiscordSvc' 'Discord Service' 800) })
    ,@('services illisibles', { param($d, $p) $d.Services = @() })
    ,@('dossier de Windows', { param($d, $p) $p.Chemin = 'C:\Windows\Discord\Discord.exe'; $p.Image = $p.Chemin })
    ,@('nom d''un programme de Windows', { param($d, $p) $p.Nom = 'dwm.exe'; $p.Chemin = 'C:\Program Files\Discord\dwm.exe'; $p.Image = $p.Chemin })
    ,@('antivirus inscrit (son dossier)', { param($d, $p) $p.Chemin = 'C:\Program Files\AVX\Moteur\avxui.exe'; $p.Image = $p.Chemin })
    ,@('Microsoft Defender', { param($d, $p) $p.Chemin = 'C:\ProgramData\Microsoft\Windows Defender\Platform\4.18.2\MpCopyAccelerator.exe'; $p.Image = $p.Chemin })
    ,@('appli du Microsoft Store', { param($d, $p) $p.Chemin = 'C:\Program Files\WindowsApps\SpotifyAB.Spotify_1.2.3\Spotify.exe'; $p.Image = $p.Chemin })
    ,@('suite : Program Files\Mindset', { param($d, $p) $p.Chemin = 'C:\Program Files\Mindset\Evermind\Discord.exe'; $p.Image = $p.Chemin })
    ,@('suite : X:\Mindset\Programmes', { param($d, $p) $p.Chemin = 'E:\Mindset\Programmes\Nevermind\Discord.exe'; $p.Image = $p.Chemin })
    ,@('suite : assistant de Mindcare', { param($d, $p) $p.Chemin = 'E:\Mindset\Donnees\Mindcare\ia\Discord.exe'; $p.Image = $p.Chemin })
    ,@('suite : assistant de Mindcare (profil)', { param($d, $p) $p.Chemin = 'C:\Users\x\AppData\Local\Mindset\Mindcare\ia\Discord.exe'; $p.Image = $p.Chemin })
    ,@('Nevermind lui-même', { param($d, $p) $d.Moi.Id = 800 })
    ,@('lancé par Nevermind (sa WebView2)', { param($d, $p) $p.Parent = 9000; $d.Processus = @($d.Processus) + @(@{ Pid = 9000; Parent = 0; Nom = 'Mindset.exe'; Chemin = 'C:\Dev\Mindset.exe'; Octets = 1MB; Session = 1; Compte = $moi.Compte; Protection = 0; Eleve = 0; Debut = [long]0 }) })
    ,@('lancé par une appli de la suite', { param($d, $p) $p.Parent = 650; $d.Processus = @($d.Processus) + @(@{ Pid = 650; Parent = 0; Nom = 'Mindset.exe'; Chemin = 'C:\Program Files\Mindset\Mindcare\Mindset.exe'; Octets = 1MB; Session = 1; Compte = $moi.Compte; Protection = 0; Eleve = 0; Debut = [long]1650 }) })
    ,@('Nevermind illisible', { param($d, $p) $d.Moi.Session = -1 })
    ,@('Nevermind sans compte', { param($d, $p) $d.Moi.Compte = '' })
    ,@('Nevermind en administrateur', { param($d, $p) $d.Moi.Eleve = 1; $p.Eleve = 1 })
    ,@('sans Nevermind', { param($d, $p) $d.Remove('Moi') })
)
foreach ($cas in $casInterdits) {
    $d = New-FauxPcFermer @(& $discord); $p = Get-Proc $d 800
    & $cas[1] $d $p
    Add-Juge "fermer : $($cas[0])" $d
}
$d = New-FauxPcFermer @(New-Proc 810 'Mindset.exe' 90 'C:\Copies\Mindset.exe'); $d.Processus = @($d.Processus | Where-Object { $_.Pid -notin 600, 601 }); Add-Juge 'fermer : Mindset.exe hors de la suite' $d
$d = New-FauxPcFermer @(); (Get-Proc $d 601).Chemin = 'D:\Mindset\Programmes\Nevermind\Mindset.exe'; (Get-Proc $d 601).Image = (Get-Proc $d 601).Chemin; Add-Juge 'fermer : suite dans Mindset\Programmes' $d
Add-Juge 'fermer : jamais (WebView2, antivirus, NVIDIA)' (New-FauxPcFermer @(New-Proc 820 'msedgewebview2.exe' 200 'C:\Program Files (x86)\Microsoft\EdgeWebView\Application\1.0\msedgewebview2.exe' '' 'Microsoft Edge WebView2'
                        New-Proc 830 'avastui.exe' 150 'C:\Program Files\Avast Software\Avast\avastui.exe' '' 'Avast Antivirus'
                        New-Proc 840 'nvcontainer.exe' 90 'C:\Program Files\NVIDIA Corporation\NvContainer\nvcontainer.exe' '' 'NVIDIA Container'))
$monJeu = { New-Proc 850 'MonJeu.exe' 900 'D:\Jeux\MonJeu\MonJeu.exe' '' 'Mon Jeu' }
$steamL = { New-Proc 860 'steam.exe' 60 'C:\Program Files (x86)\Steam\steam.exe' '' 'Steam' }
$epicL = { New-Proc 861 'EpicGamesLauncher.exe' 70 'C:\Program Files (x86)\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe' '' 'Epic Games Launcher' }
function Add-JugeJeux([string]$Nom, [array]$Procs, $Jeux) { $d = New-FauxPcFermer $Procs; $d.Jeux = $Jeux; Add-Juge "jeux : $Nom" $d }
$sansJeu = @{ Exes = @(); Noms = @(); EnCours = 0 }
Add-JugeJeux 'bibliothèque' @(& $monJeu) @{ Exes = @('d:\jeux\monjeu\monjeu.exe'); Noms = @(); EnCours = 0 }
Add-JugeJeux 'en cours' @(& $monJeu) @{ Exes = @(); Noms = @(); EnCours = 850 }
Add-JugeJeux 'appris' @(& $monJeu) @{ Exes = @(); Noms = @('monjeu'); EnCours = 0 }
Add-JugeJeux 'sans jeu' @(& $monJeu) $sansJeu
Add-JugeJeux 'un autre jeu' @(& $monJeu) @{ Exes = @('d:\jeux\autre\monjeu.exe'); Noms = @('autre'); EnCours = 851 }
foreach ($dossier in 'C:\Steam\steamapps\common\Inconnu', 'D:\Epic Games\Inconnu', 'C:\Program Files\EA Games\Inconnu', 'C:\Riot Games\Inconnu', 'C:\XboxGames\Inconnu\Content', 'C:\GOG Games\Inconnu', 'C:\Program Files\Inconnu') {
    Add-JugeJeux "dossier $dossier" @(New-Proc 870 'inconnu.exe' 300 "$dossier\inconnu.exe") $sansJeu
}
Add-JugeJeux 'lanceurs sans jeu' @((& $steamL), (& $epicL)) $sansJeu
Add-JugeJeux 'lanceurs pendant une partie' @((& $steamL), (& $epicL), (& $monJeu)) @{ Exes = @(); Noms = @(); EnCours = 850 }
Add-JugeJeux 'lanceurs et jeu dans steamapps' @((& $steamL), (& $epicL), (& $monJeu), (New-Proc 871 'jeu.exe' 500 'C:\Program Files (x86)\Steam\steamapps\common\Jeu\jeu.exe')) $sansJeu

# 3. Cas limites : accents, très longs, majuscules, Windows dans un autre dossier, plus de 30 applis, petites applis
$d = New-FauxPcFermer @(
    New-Proc 900 'Éditeur Vidéo.EXE' 333.3 'C:\Program Files\Éditeur\Éditeur Vidéo.EXE' '' 'Éditeur « vidéo » d''été'
    New-Proc 901 ('TrèsLong' * 40 + '.exe') 21 ('C:\Program Files\' + ('Dossier' * 30) + '\x.exe') '' ('Produit ' * 50)
    New-Proc 902 'CHROME.EXE' 44 'C:\PROGRAM FILES\GOOGLE\CHROME\APPLICATION\CHROME.EXE' '"CHROME.EXE" --NO-STARTUP-WINDOW' 'Google Chrome'
    New-Proc 903 'Spotify.exe' 1023.7 'C:\Users\x\AppData\Roaming\Spotify\Spotify.exe' '' ''
    New-Proc 904 'sans.exe' 19.99 '' '' '' 'Sans chemin'
    New-Proc 905 'svchost.exe' 12 'C:\Windows\System32\svchost.exe')
$d.Windir = 'C:\WINDOWS\'
$d.Services = @($d.Services) + @(New-Svc 'b-svc' 'B' 905; New-Svc 'A_svc' 'a' 905; New-Svc 'a2' '' 905; New-Svc 'Zeta' 'Zêta' 905; New-Svc 'éclair' 'Éclair' 905)
Add-Juge 'accents, longueurs et majuscules' $d
$d = New-FauxPc
$d.Processus = @($d.Processus) + @(for ($i = 0; $i -lt 40; $i++) { New-Proc (2000 + $i) "Appli$i.exe" (21 + $i * 7.3) "C:\Program Files\Appli$i\Appli$i.exe" '' "Appli $i" })
Add-Juge 'plus de 30 grosses applis' $d
$d = New-FauxPc; $d.Visible = 15.7GB; $d.Disponible = 1GB; Add-Juge 'mémoire presque pleine' $d
$d = New-FauxPc; $d.Disponible = 20GB; Add-Juge 'disponible plus grand que visible' $d

# 4. Des PC tirés au hasard (graine fixe)
$r = New-Object System.Random $Graine
function Get-Au([array]$L) { $L[$script:r.Next($L.Count)] }
$nomsHasard = @('chrome.exe', 'msedge.exe', 'firefox.exe', 'Discord.exe', 'steam.exe', 'steamwebhelper.exe', 'EpicGamesLauncher.exe', 'svchost.exe', 'svchost.exe', 'svchost.exe',
    'explorer.exe', 'dwm.exe', 'MsMpEng.exe', 'Memory Compression', 'Registry', 'csrss.exe', 'OneDrive.exe', 'Spotify.exe', 'Teams.exe', 'iCUE.exe', 'LightingService.exe',
    'armsvc.exe', 'GoogleUpdate.exe', 'msedgewebview2.exe', 'Mindset.exe', 'Nevermind.exe', 'avastui.exe', 'vmmem', 'Code.exe', 'jeu.exe', 'MonJeu.exe', 'bf6.exe',
    'HPSysInfoCap.exe', 'updater.exe', 'Outil.exe', 'Éditeur.exe', 'NVIDIA Share.exe', 'nvcontainer.exe', 'Widgets.exe', 'PhoneExperienceHost.exe', 'RiotClientServices.exe',
    'conhost.exe', 'RuntimeBroker.exe', 'SearchHost.exe', 'ctfmon.exe', 'powershell.exe', 'audiodg.exe', 'Battle.net.exe', 'upc.exe', 'mysticlight2.exe')
$dossiersHasard = @('C:\Windows\System32', 'C:\Windows', 'C:\Program Files\Google\Chrome\Application', 'C:\Program Files (x86)\Steam', 'C:\Program Files (x86)\Steam\steamapps\common\Jeu',
    'C:\Users\x\AppData\Local\Discord\app-1.0', 'C:\Program Files\WindowsApps\App_1.0', 'D:\Mindset\Programmes\Nevermind', 'C:\Program Files\Mindset\Mindset', 'C:\Outils',
    'D:\Jeux\MonJeu', 'C:\Program Files\AVX\Moteur', 'C:\ProgramData\Microsoft\Windows Defender\Platform\4.18', 'C:\Program Files\Éditeur', '', 'C:\Riot Games\Riot Client')
$produitsHasard = @('', '', 'Google Chrome', 'Microsoft® Windows® Operating System', 'Système d''exploitation Microsoft® Windows®', 'Discord', 'Steam', 'Mon Produit', 'Éditions Été')
$servicesHasard = @('Dnscache', 'nsi', 'WSearch', 'SysMain', 'AdobeARMservice', 'gupdate', 'gupdatem', 'GoogleUpdaterService141.0.2.3', 'edgeupdate', 'edgeupdatem', 'HPSysInfoCap',
    'MozillaMaintenance', 'NvTelemetryContainer', 'ESRV_SVC_QUEENCREEK', 'SupportAssistAgent', 'ASUSLinkNear', 'WinDefend', 'Audiosrv', 'BITS', 'wuauserv', 'Spooler', 'xedgeupdate', 'CDPSvc', 'cbdhsvc_1a2b')
for ($k = 0; $k -lt $Hasard; $k++) {
    $inst = Get-Au @(4GB, 8GB, 16GB, 32GB, 64GB, 0)
    $vis = if ($inst) { $inst - (Get-Au @(0, 0.1GB, 0.3GB, 1.5GB, 2GB)) } else { Get-Au @(8GB, 15.8GB) }
    if ($k % 17 -eq 3) { $vis = $inst + 1GB }   # (installée plus petite que visible : illisible)
    $d = @{ Pc = "PC-$k"; Date = ([datetime]'2026-03-01').AddMinutes($r.Next(0, 400000)); Windir = (Get-Au @('C:\Windows', 'C:\Windows', 'D:\WINDOWS'))
        Installee = [double]$inst; Visible = [double]$vis; Disponible = [double]$vis * $r.NextDouble(); Cache = 6GB * $r.NextDouble(); Modifiee = 0.2GB * $r.NextDouble()
        PoolNP = (Get-Au @(0.2GB, 0.5GB, 0.75GB, 0.8GB, 0.9GB, 1.2GB, 1.6GB, 2.3GB)) * (0.9 + 0.2 * $r.NextDouble()); PoolP = 1GB * $r.NextDouble()
        Processus = @(); Services = @(); Demarrage = @(); Etiquettes = @() }
    $n = $r.Next(0, 60)
    $pids = @(for ($i = 0; $i -lt $n; $i++) { 8 + 4 * $i + $r.Next(0, 3) })
    $fiches = $r.Next(0, 4) -ne 0
    $d.Processus = @((New-Proc 0 'System Idle Process' 0), (New-Proc 4 'System' 1.5)) + @(foreach ($id in $pids) {
        $nom = Get-Au $nomsHasard
        $dos = Get-Au $dossiersHasard
        $chemin = if ($dos) { "$dos\$nom" } else { '' }
        $p = New-Proc $id $nom ([math]::Round(400 * $r.NextDouble() * $r.NextDouble(), 3) + 0.001 * $r.Next(1, 999)) $chemin (Get-Au @('', '', '--type=renderer', '"x.exe" --no-startup-window')) (Get-Au $produitsHasard) (Get-Au @('', 'Une description', 'Hôte de la fenêtre', ''))
        $p.Societe = Get-Au @('', 'Microsoft Corporation', 'Valve')
        $p.Parent = if ($r.Next(0, 3)) { Get-Au (@(0, 4) + $pids) } else { 0 }
        if ($fiches -and $id -gt 4) {
            $p.Session = Get-Au @(1, 1, 1, 1, 0, 2, $null); $p.Compte = Get-Au @($moi.Compte, $moi.Compte, $moi.Compte, 'S-1-5-18', ''); $p.Protection = Get-Au @(0, 0, 0, 0, 1, -1)
            $p.Eleve = Get-Au @(0, 0, 0, 0, 1, -1); $p.Debut = if ($r.Next(0, 6)) { [long](130000000000000000 + $id) } else { [long]0 }; $p.Image = if ($r.Next(0, 8)) { $chemin } else { '' }
        }
        $p
    })
    $d.Services = @(for ($i = 0; $i -lt $r.Next(0, 14); $i++) {
        $sid = if ($pids.Count -and $r.Next(0, 4)) { Get-Au $pids } else { 0 }
        New-Svc (Get-Au $servicesHasard) (Get-Au @('', 'Un service', 'Service Été')) $sid (Get-Au @('Auto', 'Manual', 'Disabled', 'Automatic', ''))
    })
    $d.Demarrage = @(for ($i = 0; $i -lt $r.Next(0, 4); $i++) { (Get-Au $nomsHasard).ToLower() })
    if ($r.Next(0, 3) -eq 0) {
        $d.Etiquettes = @(for ($i = 0; $i -lt $r.Next(1, 4); $i++) {
            @{ Tag = (Get-Au @('Ntfx', 'RTKx', 'Kil1', 'AvP ', 'Ab12')); Octets = (50MB + 900MB * $r.NextDouble()); Pilotes = @(for ($j = 0; $j -lt $r.Next(0, 3); $j++) {
                @{ Fichier = (Get-Au @('rt640x64.sys', 'ntfs.sys', 'e2xw10x64.sys', 'aswSP.sys', 'iaStorAC.sys')); Description = (Get-Au @('', 'Realtek PCIe GbE Family Controller', 'Killer Ethernet', 'Avast Antivirus', 'Intel RST', 'Pilote')); Societe = (Get-Au @('', 'Microsoft Corporation', 'Realtek', 'Rivet Networks', 'AVAST Software')) } }) } })
    }
    if ($fiches) { $d.Moi = @{ Id = (Get-Au (@(9000) + $pids)); Session = (Get-Au @(1, 1, 1, -1)); Compte = $moi.Compte; Eleve = (Get-Au @(0, 0, 1)) }; $d.Antivirus = @(Get-Au @(@(), @('C:\Program Files\AVX\Moteur\avx.exe'))) }
    if ($r.Next(0, 2)) { $d.Jeux = @{ Exes = @(Get-Au @(@(), @('d:\jeux\monjeu\monjeu.exe'))); Noms = @(Get-Au @(@(), @('bf6'), @('jeu'))); EnCours = $(if ($r.Next(0, 3) -eq 0 -and $pids.Count) { Get-Au $pids } else { 0 }) } }
    Add-Juge "hasard $k" $d -PeuDePages
}

# ---------------------------------------------------------------------------
# Les petites fonctions : Format-Size, Get-GenrePilote, Get-MessageFermeture, Get-OrigineMemoire, la page sans constat
# ---------------------------------------------------------------------------
$tailles = @(0, 1, 511, 512, 1023, 1024, 1536, 1048575, 1048576, 1572864, 524288000, 1048051712, 1072693248, 1073217536, 1073741823, 1073741824, 1395864371, 1.25GB, 1.35GB, 1.45GB, 2.05GB, 15.7GB, 1023.96GB, 1TB, 1.5TB)
for ($i = 0; $i -lt 300; $i++) { $tailles += [math]::Floor([math]::Pow(10, 13 * $r.NextDouble())) }
$t = @(foreach ($x in $tailles) { "[$(ConvertTo-Json ([double]$x) -Compress),$(ConvertTo-JsonTexte (Format-Size $x))]" }) -join ','
$g = @(foreach ($x in @('Killer E3100G 2.5 Gigabit Ethernet', 'Realtek PCIe GbE Family Controller', 'Intel(R) Wi-Fi 6 AX201', 'Avast Antivirus', 'ASUS Aura lighting', 'rt640x64.sys  Realtek', 'NVIDIA nvlddmkm', 'Intel RST', 'Nahimic', 'ntfs.sys Pilote NT File System Microsoft Corporation', '', 'LAN x', 'Rivet', 'AMD Radeon')) { "[$(ConvertTo-JsonTexte $x),$(ConvertTo-JsonTexte (Get-GenrePilote $x))]" }) -join ','
$m = @(foreach ($e in 'bloquee', 'question', 'rangee', 'autre') { $x = Get-MessageFermeture 'Discord « été »' $e; "[$(ConvertTo-JsonTexte $e),$(([bool]$x.Forcer).ToString().ToLower()),$(ConvertTo-JsonTexte $x.Texte)]" }) -join ','
$o = @(foreach ($a in $true, $false) { foreach ($b in $true, $false) { "[$($a.ToString().ToLower()),$($b.ToString().ToLower()),$(ConvertTo-JsonTexte (Get-OrigineMemoire @{ AuDemarrage = $a; ParWindows = $b }))]" } }) -join ','
$sans = @(foreach ($e in '', 'donnees', 'pilotes') { "$(ConvertTo-JsonTexte $e):$(ConvertTo-JsonKit (Get-PageMemoireKit $null $e))" }) -join ','
$petites = "{`"tailles`":[$t],`"genres`":[$g],`"messages`":[$m],`"origines`":[$o],`"pagesSansConstat`":{$sans}}"

[void][IO.Directory]::CreateDirectory($sortie)
foreach ($f in [IO.Directory]::GetFiles($sortie, '*.json')) { [IO.File]::Delete($f) }
$utf8 = New-Object Text.UTF8Encoding($false)
[IO.File]::WriteAllText((Join-Path $sortie 'petites.json'), $petites, $utf8)
$parFichier = 50
for ($i = 0; $i -lt $juges.Count; $i += $parFichier) {
    $morceau = @($juges | Select-Object -Skip $i -First $parFichier) -join ",`n"
    [IO.File]::WriteAllText((Join-Path $sortie ('constats-{0:D2}.json' -f ($i / $parFichier))), "[`n$morceau`n]", $utf8)
}
Write-Host "$($juges.Count) juges de constat et $($tailles.Count) tailles exportés dans $sortie"
