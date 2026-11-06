# Exporte les juges du bloc « perf » de Nevermind en Rust (rust\nevermind-perf\tests\juges\*.json) depuis la version
# PowerShell, qui reste la référence : les vraies fonctions sont prises dans les vrais fichiers (Import-Def, comme
# tester-fps.ps1), sur des entrées inventées (graine fixe), sans rien toucher du PC : ni l'Aide, ni les programmes, ni les
# réglages, ni le registre, ni le disque (le disque système est remplacé par un faux).
#
#   & 'outils\exporter-juges-perf.ps1'            (depuis l'outil PowerShell, jamais depuis Bash)
#
# Fichiers écrits :
#   pages.json      Get-PagesPerf (Performances, Optimisation gaming, Démarrage, Nettoyage ; la page RAM est au bloc
#                   « memoire ») pour des états simulés ;
#   mode-jeu.json   les décisions du mode jeu : applis cochées (Get-GameModeSelection), applis fermées au début d'une
#                   partie (Start-GameSession), applis reprises d'un fichier mode-jeu.json (Resume-GameModePending),
#                   état affiché (Update-GameModeStatus) ;
#   detection.json  la détection d'une partie : index des jeux (Update-GameCache) et Test-GameRunning ;
#   parties.json    le résumé d'une partie : la vraie [FrameMon] (natif.cs) sur des images inventées, puis l'enregistrement
#                   (Save-FpsSession : Compress-Series), Test-FpsProblem et la bulle de fin de partie (Complete-FpsTarget) ;
#   diagnostic.json Get-FpsDiagData et Get-FpsDiagnosis sur des relevés inventés ;
#   temperature.json Test-TempAlert sur des suites de températures.

$ErrorActionPreference = 'Stop'
$env:NEVERMIND_TEST = '1'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
$sortie  = Join-Path $racine 'rust\nevermind-perf\tests\juges'
[void][IO.Directory]::CreateDirectory($sortie)
$ci = [Globalization.CultureInfo]::InvariantCulture

# ---------------------------------------------------------------------------
# Sécurité : rien sur le vrai PC (une fonction passe avant une commande du même nom)
# ---------------------------------------------------------------------------
foreach ($c in 'Stop-Process', 'Start-Process', 'Set-ItemProperty', 'New-ItemProperty', 'Remove-ItemProperty', 'Invoke-Aide', 'Stop-ProgrammeNom', 'taskkill', 'taskkill.exe', 'Set-Reg', 'Set-Declarations') {
    Set-Item -Path "function:script:$c" -Value ([scriptblock]::Create("throw 'Export : $c est interdit.'"))
}
$script:Journal = New-Object System.Collections.Generic.List[string]
$script:Statuts = New-Object System.Collections.Generic.List[string]
$script:Bulles = New-Object System.Collections.Generic.List[object]
$script:Reglages = @{}
function script:Write-Log([string]$Message) { $script:Journal.Add($Message) }
function script:Set-Status([string]$Text) { $script:Statuts.Add($Text) }
function script:Get-Setting([string]$Nom, $Defaut) { if ($script:Reglages.ContainsKey($Nom)) { $script:Reglages[$Nom] } else { $Defaut } }
function script:Set-Setting([string]$Nom, $Valeur) { $script:Reglages[$Nom] = $Valeur }
function script:Show-Notify([string]$Title, [string]$Text, $OnClick) { $script:Bulles.Add(@{ titre = $Title; texte = $Text }) }
function script:Get-Brush($c) { $null }
$Colors = @{ ok = '#ok'; warn = '#warn'; bad = '#bad'; info = '#info' }

# Une fonction ou une variable des vrais fichiers ; $Remplacer : @(@(texte, remplacement)) dans son code (le faux disque)
function Import-Def([string]$Fichier, [string[]]$Noms, $Remplacer = @()) {
    $tok = $null; $err = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules $Fichier), [ref]$tok, [ref]$err)
    if ($err) { throw "$Fichier n'est pas lisible : $($err[0].Message)" }
    foreach ($n in $Noms) {
        $def = if ($n.StartsWith('$')) {
            $ast.Find({ param($x) $x -is [System.Management.Automation.Language.AssignmentStatementAst] -and $x.Parent -eq $ast.EndBlock -and $x.Left.Extent.Text -eq $n }, $false)
        } else {
            $ast.Find({ param($x) $x -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $x.Name -eq $n }, $true)
        }
        if (-not $def) { throw "$n introuvable dans $Fichier" }
        $code = $def.Extent.Text
        foreach ($r in $Remplacer) {
            if (-not $code.Contains($r[0])) { continue }
            $code = $code.Replace($r[0], $r[1])
        }
        . ([scriptblock]::Create($code))
    }
}
$fauxDisque = @(, @('[IO.DriveInfo]::new($env:SystemDrive)', '$script:FauxDisque'))
. Import-Def 'ecran-perf.ps1' @('Get-DernierJeuKit', 'Get-ProfilsKit', 'Get-ReglagesKit', 'Get-TitreReglagesKit', 'Get-AppsModeJeuKit', 'Get-PagesPerf') $fauxDisque
. Import-Def 'jeu.ps1' @('$GameModeApps', '$GenericExe', 'Get-GameModeSelection', 'Update-GameModeStatus', 'Get-GameExeNames', 'Test-TempAlert', 'Compress-Series', 'Test-FpsProblem', 'Format-PlayTime', 'Get-FpsVerdict')
. Import-Def 'accueil-console.ps1' @('Format-TexteKit')
. Import-Def 'systeme.ps1' @('Format-Size')
. Import-Def 'ecran-pc.ps1' @('$TonsStatut')
. Import-Def 'diagnostic-fps.ps1' @('$SystemProcs', '$Browsers', 'Add-DiagItem', 'Get-FpsDiagnosis', 'Merge-FpsProcSample', 'Get-FpsDiagData')

function Write-Juge([string]$Nom, [string]$Sujet, $Cas) {
    $o = [ordered]@{ format = 1; sujet = $Sujet; source = 'outils\exporter-juges-perf.ps1'; cas = @($Cas) }
    $json = ConvertTo-Json -InputObject $o -Depth 40 -Compress
    [IO.File]::WriteAllText((Join-Path $sortie $Nom), $json, (New-Object Text.UTF8Encoding $false))
    Write-Host "$Nom : $(@($Cas).Count) cas"
}

$rnd = New-Object System.Random 20261106
function Pile([int]$Sur = 2) { $rnd.Next($Sur) -eq 0 }
function Choisir($Tab) { $Tab[$rnd.Next(@($Tab).Count)] }
function Taille { Choisir @(0, 512, 900KB, 1MB, 5.5MB, 499MB, 500MB, 501MB, 1GB, 1.04GB, 1.05GB, 3.7GB, 1TB, 2.25TB, ($rnd.NextDouble() * 4GB)) }

# ===========================================================================
# 1. Pages
# ===========================================================================
$titres = @('Plan d''alimentation : performances', 'Mode Jeu de Windows', 'Accélération matérielle: GPU', 'Délai réseau (Nagle)', 'Effets visuels', 'Écran : 144 Hz', 'Souris : précision désactivée')
$descs = @('Le processeur reste rapide: plus de FPS.', 'Windows donne la priorité au jeu.', '', 'Un texte : avec deux points.', 'Accentué é è à ç ô')
$impacts = @('Important', 'Moyen', 'Faible')
$apps = @('OneDrive', 'Discord', 'Steam', 'Spotify', 'Adobe Updater', 'Programme très long avec un nom qui ne finit jamais vraiment', 'NVIDIA')
$kinds = @('safe', 'keep', 'choice', '')
$cats = @(@('Fichiers temporaires', 'Ceux de Windows et des programmes.'), @('Cache des miniatures', ''), @('Rapports d''erreurs', 'Envoyés à Microsoft.'), @('Ancienne mise à jour', 'Windows.old'))
$nomsJeux = @('Rocket League', 'bf6', 'Élden Ring', 'counter-strike 2', 'Apex Legends', 'Zelda', 'ARK', 'Fortnite', 'élite dangerous', 'Elite', 'Minecraft')
$exesJeux = @('C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe', 'D:\EA\bf6.exe', 'C:\Steam\eldenring.exe', 'C:\Steam\cs2.exe', 'C:\Jeux\game.exe', 'C:\Jeux\r5apex.exe', 'C:\Jeux\launcher.exe', 'C:\Fortnite\FortniteClient-Win64-Shipping.exe', 'C:\Jeux\Ark.exe', 'C:\Jeux\Minecraft.Windows.exe', 'C:\Jeux\EliteDangerous64.exe')
$procsPossibles = @('OneDrive', 'ms-teams', 'Teams', 'Skype', 'CCXProcess', 'Creative Cloud', 'Dropbox', 'GoogleDriveFS', 'wallpaper32', 'wallpaper64', 'Spotify', 'EpicGamesLauncher', 'EADesktop', 'upc', 'UbisoftConnect', 'explorer', 'onedrive', 'OneDriveSetup', 'Spotify2')
$idsApps = @('onedrive', 'teams', 'skype', 'adobe', 'dropbox', 'gdrive', 'wallpaper', 'spotify', 'epic', 'ea', 'ubisoft', 'inconnue')

function New-EntreePages {
    $e = [ordered]@{}
    $e.ecran = Choisir @('perf', 'optimisation', 'demarrage', 'nettoyage', 'accueil')
    $n = Choisir @(0, 0, 1, 2, 3, 5, 7)
    $e.reglages = @(for ($i = 0; $i -lt $n; $i++) {
        $actif = Pile 3; $actif = -not $actif
        [ordered]@{ titre = (Choisir $titres); description = (Choisir $descs); impact = (Choisir $impacts); recommande = (Choisir @($null, $true, $false, $true)); redemarrage = (Pile 3); active = $actif; coche = (Pile) }
    })
    $e.restauration = Pile
    $e.internet = if (Pile) { $null } else { [ordered]@{ moy = (Choisir @(4, 12, 35, 120, 999)); statut = (Choisir @('ok', 'warn', 'bad', 'info')) } }
    $e.bridage = Pile 5
    $e.dernier = if (Pile 3) { $null } else { [ordered]@{ jeu = (Choisir $nomsJeux); moy = (Choisir @($null, 0, 59.5, 60.5, 143.7, 1234.5)) } }
    $n = Choisir @(0, 0, 1, 2, 4, 6)
    $e.demarrage = @(for ($i = 0; $i -lt $n; $i++) {
        [ordered]@{ nom = (Choisir $apps); actif = -not (Pile 3); genre = (Choisir $kinds); etiquette = (Choisir @('Tu peux le désactiver', 'À garder', 'À toi de voir', '')); pourquoi = (Choisir @('Il sert: au démarrage.', 'Pilote de la carte.', '')); image = (Choisir @($null, $null, 'data:image/png;base64,AAAA')) }
    })
    $e.texteDemarrage = Choisir @('Lecture des programmes...', '3 programmes au démarrage', '')
    $n = Choisir @(0, 0, 1, 2, 4)
    $e.nettoyage = @(for ($i = 0; $i -lt $n; $i++) {
        $c = Choisir $cats
        $t = Taille
        [ordered]@{ titre = $c[0]; desc = $c[1]; taille = $t; active = ($t -gt 0 -and -not (Pile 4)); coche = (Pile) }
    })
    $e.disque = if (Pile 4) { $null } else { $tot = Choisir @(0, 120GB, 256GB, 512GB, 1TB, 2TB); [ordered]@{ total = $tot; libre = [double]([math]::Floor($tot * (Choisir @(0, 0.05, 0.1, 0.5, 0.95, 1.0)))) } }
    $e.texteNettoyage = Choisir @('Analyse en cours...', '1,2 Go à libérer', '')
    $e.boutonNettoyer = Choisir @('Nettoyer (1,2 Go)', 'Nettoyer')
    $e.dernierNettoyage = if (Pile) { $null } else { [ordered]@{ date = (Choisir @('2026-11-01T09:05:00', '2026-01-31T23:59:59', '')); libere = (Taille); supprimes = (Choisir @(0, 1, 2, 1500)); laisses = (Choisir @(0, 1, 3)); fichier = (Pile) } }
    $e.memoire = if (Pile) { $null } else { [ordered]@{ pourcent = (Choisir @(12, 79, 80, 95)); alertes = (Choisir @(0, 0, 1, 2)) } }
    $e.ram = if (Pile 3) { $null } else { $tot = Choisir @(8GB, 16GB, 31.9GB, 0); [ordered]@{ utilisee = [double]([math]::Floor($tot * (Choisir @(0.1, 0.5, 0.795, 0.805, 0.825, 0.995)))); totale = $tot } }
    $n = Choisir @(0, 0, 1, 3)
    $e.analyse = @(for ($i = 0; $i -lt $n + 1; $i++) {
        [ordered]@{ id = (Choisir @('tweak:power', 'tweak:gamemode', 'display:\\.\DISPLAY1', 'secu:x')); statut = (Choisir @('warn', 'ok', 'bad', 'warn')); titre = (Choisir $titres); detail = (Choisir $descs) }
    })
    $e.modeJeu = Pile
    $e.appsModeJeu = if (Pile) { $null } else { , @(@(for ($i = 0; $i -lt (Choisir @(0, 1, 3, 5)); $i++) { Choisir $idsApps }) | Select-Object -Unique) }
    $e.processus = @(for ($i = 0; $i -lt (Choisir @(0, 2, 5, 9)); $i++) { Choisir $procsPossibles })
    $e.session = if (Pile 3) { [ordered]@{ jeu = (Choisir $nomsJeux); fermees = @(for ($i = 0; $i -lt (Choisir @(0, 1, 2)); $i++) { Choisir @('OneDrive', 'Spotify', 'Dropbox', 'Microsoft Teams') }) } } else { $null }
    # (noms tous différents : Sort-Object n'est pas stable, deux jeux du même nom n'auraient pas d'ordre sûr)
    $melange = @($nomsJeux | Sort-Object { $rnd.Next() })
    $e.jeux = if (Pile 4) { $null } else {
        , @(for ($i = 0; $i -lt (Choisir @(0, 1, 2, 4, 6)); $i++) {
            $ex = @(for ($k = 0; $k -lt (Choisir @(0, 1, 1, 2)); $k++) { Choisir $exesJeux })
            [ordered]@{ nom = $melange[$i]; source = (Choisir @($null, 'Steam', 'Epic', 'Ajouté', '')); exes = $ex; prio = (Pile); gpu = (Choisir @($null, '', 'GpuPreference=2;', 'GpuPreference=1;')) }
        })
    }
    $e.gpus = @(Choisir @(@(), @('NVIDIA GeForce RTX 4070'), @('Intel(R) UHD Graphics', 'NVIDIA GeForce RTX 3060 Laptop GPU'), @('AMD Radeon RX 7800 XT', 'Microsoft Basic Display Adapter'), @('Parsec Virtual Display Adapter', 'NVIDIA GeForce GTX 1060')))
    $e.nLib = Choisir @(0, 1, 3, 10)
    $e.profilsLus = Pile
    $e
}

# L'entrée devient l'état de la version PowerShell
function Set-EtatPages($E) {
    $script:Kit = @{ Stats = @{} }
    $script:Vue = @{ Ecran = $E.ecran }
    $script:TweakRows = @(foreach ($r in $E.reglages) { @{ Tweak = @{ Titre = $r.titre; Description = $r.description; Impact = $r.impact; Recommended = $r.recommande; Reboot = $r.redemarrage }; CheckBox = @{ IsEnabled = $r.active; IsChecked = $r.coche } } })
    $script:ui = @{ ChkRestore = @{ IsChecked = $E.restauration }; StartupCount = @{ Text = $E.texteDemarrage }; CleanTotal = @{ Text = $E.texteNettoyage }; BtnClean = @{ Content = $E.boutonNettoyer } }
    $script:PingResults = @(if ($E.internet) { @{ Label = 'Box'; Avg = 1; Status = 'ok' }; @{ Label = 'Internet (Cloudflare)'; Avg = $E.internet.moy; Status = $E.internet.statut } })
    $script:FauxBridage = $E.bridage
    $script:FauxDernier = if ($E.dernier) { @{ Game = @{ Name = $E.dernier.jeu }; Last = $(if ($null -ne $E.dernier.moy) { @{ Avg = $E.dernier.moy } }) } } else { $null }
    $script:StartupEntries = @(foreach ($s in $E.demarrage) { @{ Name = $s.nom; Item = @{ Enabled = $s.actif }; Advice = @{ Kind = $s.genre; Label = $s.etiquette; Why = $s.pourquoi }; IconExe = $s.image } })
    $script:CleanRows = @(foreach ($c in $E.nettoyage) { @{ Target = @{ Titre = $c.titre; Desc = $c.desc }; Size = [double]$c.taille; CheckBox = @{ IsEnabled = $c.active; IsChecked = $c.coche } } })
    $script:FauxDisque = if ($E.disque) { [pscustomobject]@{ TotalSize = [long]$E.disque.total; AvailableFreeSpace = [long]$E.disque.libre } } else { $null }
    $script:Reglages = @{}
    if ($E.dernierNettoyage) {
        $f = if ($E.dernierNettoyage.fichier) { $script:FichierJournalNettoyage } else { 'Z:\nulle-part\nettoyage.txt' }
        $script:Reglages.LastClean = [pscustomobject]@{ Date = $E.dernierNettoyage.date; Freed = $E.dernierNettoyage.libere; Deleted = $E.dernierNettoyage.supprimes; Skipped = $E.dernierNettoyage.laisses; File = $f }
    }
    $script:Kit.Memoire = if ($E.memoire) { @{ Pourcent = $E.memoire.pourcent; Alertes = @(for ($i = 0; $i -lt $E.memoire.alertes; $i++) { @{ Id = 'x' } }) } } else { $null }
    $script:Live = if ($E.ram) { @{ Updated = (Get-Date); RamUsed = $E.ram.utilisee; RamTotal = $E.ram.totale } } else { @{} }
    $script:LastAnalysis = @{ Active = @(foreach ($a in $E.analyse) { @{ Id = $a.id; Status = $a.statut; Titre = $a.titre; Detail = $a.detail } }) }
    $script:Reglages.GameMode = $E.modeJeu
    if ($null -ne $E.appsModeJeu) { $script:Reglages.GameModeApps = @($E.appsModeJeu) }
    $script:FauxProcs = @($E.processus)
    $script:GameSession = if ($E.session) { @{ Game = $E.session.jeu; Closed = @($E.session.fermees | ForEach-Object { @{ Name = $_ } }); Pid = 1; Start = (Get-Date) } } else { $null }
    $script:FauxGpuPref = @{}
    # (« , » : une liste vide reste une liste vide, comme dans Nevermind, au lieu de devenir $null)
    $script:Games = if ($null -eq $E.jeux) { $null } else {
        , @(foreach ($g in $E.jeux) { foreach ($x in $g.exes) { $script:FauxGpuPref[$x] = $g.gpu }; @{ Name = $g.nom; Exes = @($g.exes); Source = $g.source; Prio = $g.prio } })
    }
    $script:AnalysisData = @{ GPUs = @($E.gpus | ForEach-Object { @{ Name = $_ } }) }
    $script:FauxLib = $E.nLib
    $script:GameModeStatus = [pscustomobject]@{ Text = ''; Foreground = $null }
    Update-GameModeStatus
    if ($E.profilsLus -and $null -ne $script:Games) { [void](Get-ProfilsKit) }
}
function script:Get-LastPlayedGame { $script:FauxDernier }
function script:Get-IconeExeKit($Exe) { if ($Exe) { [string]$Exe } else { $null } }
function script:Get-BridageCarte($Net) { if ($script:FauxBridage) { @{ Fois = 10 } } else { $null } }
function script:Get-LibraryGames { @(for ($i = 0; $i -lt $script:FauxLib; $i++) { @{ Name = "j$i" } }) }
function script:Get-PageMemoireKit { 'memoire' }
function script:Test-GamePriority($Game) { [bool]$Game.Prio }
function script:Get-GpuPreference([string]$Exe) { $script:FauxGpuPref[$Exe] }
function script:Get-Process { @($script:FauxProcs | ForEach-Object { [pscustomobject]@{ ProcessName = $_ } }) }

$script:FichierJournalNettoyage = Join-Path $env:TEMP "nevermind-juges-perf-$PID.txt"
[IO.File]::WriteAllText($script:FichierJournalNettoyage, 'journal')
try {
    $cas = @(for ($k = 0; $k -lt 500; $k++) {
        $e = New-EntreePages
        Set-EtatPages $e
        $p = Get-PagesPerf
        $p.Remove('memoire')
        [ordered]@{ nom = "pages $k ($($e.ecran))"; entree = $e; attendu = $p }
    })
} finally { Remove-Item -LiteralPath $script:FichierJournalNettoyage -Force -ErrorAction SilentlyContinue }
Write-Juge 'pages.json' 'Get-PagesPerf (sans la page RAM) pour des états simulés' $cas

# ===========================================================================
# 2. Mode jeu : applis cochées, fermées, reprises, état affiché
# ===========================================================================
. Import-Def 'jeu.ps1' @('Start-GameSession', 'Resume-GameModePending', 'Restart-GameModeApps', 'Save-GameModePending', 'Clear-GameModePending')
function script:Get-AideContexte { @{} }
function script:Get-AideReference($p) { 0 }
function script:Test-FpsMeasure { $false }
function script:Test-LagMeasure { $false }
function script:Update-LibraryIfShown {}
function script:Save-JsonSur($Path, $Objet) { $script:Ecrit = $Objet }
function script:Read-JsonSur($Path) { $script:ALire }
# La règle commune de fermeture : ici, un processus « refusé » par son nom (protege), les autres fermés
function script:Stop-ProcessusRegle($Processus, [string]$Chemin, $Contexte) {
    $r = @{ Fermes = 0; Aide = 0; Refus = @() }
    foreach ($p in @($Processus)) { if ($script:Refuses -contains $p.ProcessName) { $r.Refus += 'protégé' } else { $r.Fermes++ } }
    $r
}
function script:Start-Unelevated([string]$Path, [string]$Arguments) { $script:Relances.Add("$Path|$Arguments"); if ($script:EchecsRelance -contains $Path) { throw 'non' } }
$GameModeFile = Join-Path $env:TEMP "nevermind-juges-perf-mode-$PID.json"
$cas = New-Object System.Collections.ArrayList
for ($k = 0; $k -lt 300; $k++) {
    $e = [ordered]@{}
    $e.modeJeu = -not (Pile 4)
    $e.appsModeJeu = if (Pile 3) { $null } else { , @(@(for ($i = 0; $i -lt (Choisir @(0, 1, 3, 6)); $i++) { Choisir $idsApps }) | Select-Object -Unique) }
    $e.processus = @(for ($i = 0; $i -lt (Choisir @(0, 2, 5, 9, 14)); $i++) { [ordered]@{ nom = (Choisir $procsPossibles); chemin = (Choisir @('', 'C:\Users\x\AppData\Local\Microsoft\OneDrive\OneDrive.exe', 'C:\Program Files\Spotify\Spotify.exe')) } })
    $e.refuses = @(if (Pile 3) { Choisir $procsPossibles })
    $e.jeu = Choisir $nomsJeux
    $script:Reglages = @{ GameMode = $e.modeJeu }
    if ($null -ne $e.appsModeJeu) { $script:Reglages.GameModeApps = @($e.appsModeJeu) }
    $script:FauxProcs2 = @($e.processus)
    $script:Refuses = @($e.refuses)
    $script:Bulles.Clear(); $script:Ecrit = $null; $script:GameSession = $null
    function script:Get-Process { @($script:FauxProcs2 | ForEach-Object { [pscustomobject]@{ ProcessName = $_.nom; Path = $_.chemin; Id = 9 } }) }
    $sel = @(Get-GameModeSelection)
    Start-GameSession $e.jeu ([pscustomobject]@{ Id = 4242; ProcessName = 'jeu' })
    $s = $script:GameSession
    $script:GameModeStatus = [pscustomobject]@{ Text = ''; Foreground = $null }
    $script:Games = @(for ($i = 0; $i -lt (Choisir @(0, 1, 2, 5)); $i++) { @{ Name = "j$i" } })
    $e.nJeux = @($script:Games).Count
    Update-GameModeStatus
    $enJeu = $script:GameModeStatus.Text
    $script:GameSession = $null
    Update-GameModeStatus
    [void]$cas.Add([ordered]@{ nom = "début de partie $k"; entree = $e; attendu = [ordered]@{
        selection = $sel
        fermees = @($s.Closed | ForEach-Object { [ordered]@{ id = $_.Id; nom = $_.Name; chemin = $_.Path; args = $_.Args } })
        fichier = $(if ($script:Ecrit) { [ordered]@{ Game = $script:Ecrit.Game; Pid = $script:Ecrit.Pid; Proc = $script:Ecrit.Proc; Closed = @($script:Ecrit.Closed | ForEach-Object { [ordered]@{ Id = $_.Id; Name = $_.Name; Path = $_.Path } }) } } else { $null })
        bulles = $script:Bulles.ToArray()
        etatEnJeu = $enJeu; etatAttente = $script:GameModeStatus.Text
    } })
}
# Reprise d'un fichier mode-jeu.json (Nevermind arrêté pendant une partie)
$fichiers = @(
    @{ Closed = @(@{ Id = 'onedrive'; Name = 'OneDrive'; Path = 'C:\Users\x\AppData\Local\Microsoft\OneDrive\OneDrive.exe' }) }
    @{ Closed = @(@{ Id = 'onedrive'; Name = 'Faux'; Path = 'C:\Windows\System32\cmd.exe' }) }
    @{ Closed = @(@{ Id = 'pirate'; Name = 'Pirate'; Path = 'C:\pirate.exe' }) }
    @{ Closed = @(@{ Id = 'spotify'; Name = 'Spotify'; Path = 'Spotify.exe' }, @{ Id = 'teams'; Name = 'Teams'; Path = 'C:\T\ms-teams.exe' }) }
    @{ Closed = @(@{ Id = 'spotify'; Name = 'Spotify'; Path = '' }) }
    @{ Closed = @() }
    @{ Closed = @(@{ Id = 'wallpaper'; Name = 'WE'; Path = 'C:\WE\wallpaper64.exe' }, @{ Id = 'adobe'; Name = 'CC'; Path = 'C:\Adobe\Creative Cloud.exe' }, @{ Id = 'ubisoft'; Name = 'U'; Path = 'C:\U\upc.EXE' }) }
    @{ Closed = @(@{ Id = 'dropbox'; Name = 'Dropbox'; Path = 'C:\D\Dropbox.exe.bak' }) }
)
foreach ($f in $fichiers) {
    foreach ($ouvert in @($false, $true)) {
        $script:ALire = [pscustomobject]@{ Game = 'Rocket League'; Pid = 77; Proc = 'RocketLeague'; Start = '2026-11-05T20:00:00.0000000+01:00'; Closed = @($f.Closed | ForEach-Object { [pscustomobject]$_ }) }
        $script:GameModeRepris = $false; $script:GameSession = $null; $script:Bulles.Clear()
        $script:Relances = New-Object System.Collections.Generic.List[string]
        $script:EchecsRelance = @('C:\T\ms-teams.exe')
        $script:FauxProcs2 = @()
        function script:Get-Process { param([int]$Id, [string]$Name, $ErrorAction) if ($Id) { if ($script:JeuOuvert) { [pscustomobject]@{ Id = $Id; ProcessName = 'RocketLeague' } } else { throw 'absent' } } else { $null } }
        $script:JeuOuvert = $ouvert
        Resume-GameModePending
        [void]$cas.Add([ordered]@{ nom = "reprise $($cas.Count) (jeu $(if ($ouvert) { 'ouvert' } else { 'fermé' }))"; entree = [ordered]@{ reprise = $script:ALire; jeuOuvert = $ouvert; echecsRelance = $script:EchecsRelance }; attendu = [ordered]@{
            session = $(if ($script:GameSession) { [ordered]@{ jeu = $script:GameSession.Game; pid = $script:GameSession.Pid; fermees = @($script:GameSession.Closed | ForEach-Object { [ordered]@{ id = $_.Id; nom = $_.Name; chemin = $_.Path; args = $_.Args } }) } } else { $null })
            relances = $script:Relances.ToArray(); bulles = $script:Bulles.ToArray()
        } })
    }
}
Remove-Item -LiteralPath $GameModeFile -Force -ErrorAction SilentlyContinue
Write-Juge 'mode-jeu.json' 'Mode jeu : applis cochées, fermées au début d''une partie, reprises au lancement, état affiché' $cas

# ===========================================================================
# 3. Détection d'une partie : index des jeux et Test-GameRunning
# ===========================================================================
. Import-Def 'jeu.ps1' @('Test-GameRunning')
function script:Update-FpsVeille {}
$cas = New-Object System.Collections.ArrayList
$exesDet = @('C:\Jeux\RocketLeague\Binaries\Win64\RocketLeague.exe', 'D:\EA\bf6.exe', 'C:\Jeux\Dofus\Dofus.exe', 'C:\Jeux\Dofus 2\Dofus.exe', 'C:\Jeux\game.exe', 'C:\Jeux\ab.exe', 'C:\Jeux\Win64.exe', 'C:\Steam\cs2.exe', 'C:\Steam\CS2.EXE', 'C:\Jeux\Élan\Élan.exe')
for ($k = 0; $k -lt 300; $k++) {
    $jeux = @(for ($i = 0; $i -lt (Choisir @(0, 1, 3, 5)); $i++) { [ordered]@{ nom = (Choisir @('Rocket League', 'Battlefield 6', 'Dofus', 'Dofus 2', 'Jeu', 'Counter-Strike 2', 'Élan')); exes = @(for ($j = 0; $j -lt (Choisir @(1, 1, 2)); $j++) { Choisir $exesDet }) } })
    $procs = @(for ($i = 0; $i -lt (Choisir @(0, 1, 3, 6)); $i++) { $x = if (@($jeux).Count -and -not (Pile 3)) { Choisir @((Choisir $jeux).exes) } else { Choisir $exesDet }; [ordered]@{ pid = 100 + $i; nom = [IO.Path]::GetFileNameWithoutExtension($x); chemin = (Choisir @($x, $x, $x.ToUpper(), '', 'C:\Autre\' + [IO.Path]::GetFileName($x))) } })
    # (Update-GameCache, sans la bibliothèque : l'index des jeux)
    $script:Games = @($jeux | ForEach-Object { @{ Name = $_.nom; Exes = @($_.exes) } })
    $script:GameIndex = @{}
    foreach ($g in $script:Games) {
        foreach ($e in @($g.Exes)) {
            $base = [IO.Path]::GetFileNameWithoutExtension($e).ToLower()
            if ($base.Length -lt 3 -or $base -match $GenericExe) { continue }
            if (-not $script:GameIndex.ContainsKey($base)) { $script:GameIndex[$base] = @{ Game = $g.Name; Exes = @(); Source = $g.Source; ByPath = @{} } }
            $script:GameIndex[$base].Exes += $e.ToLower()
            $script:GameIndex[$base].ByPath[$e.ToLower()] = $g.Name
        }
    }
    $script:Lance = $null
    $script:FauxDet = $procs
    function script:Start-GameSession([string]$Game, $Proc) { $script:Lance = @{ jeu = $Game; pid = $Proc.Id } }
    $script:GameSession = $null
    # (Test-GameRunning lit [Diagnostics.Process]::GetProcesses() : remplacé par la fausse liste, comme le disque)
    $code = (Get-Command Test-GameRunning).Definition.Replace('[Diagnostics.Process]::GetProcesses()', '$script:FauxProcsDet').Replace('[TrafficMon]::GetProcessPath($p.Id)', '$p.Chemin2')
    Set-Item function:script:Test-GameRunning2 -Value ([scriptblock]::Create($code))
    $script:FauxProcsDet = @($procs | ForEach-Object { [pscustomobject]@{ Id = $_.pid; ProcessName = $_.nom; Path = $_.chemin; Chemin2 = $_.chemin } })
    Test-GameRunning2
    [void]$cas.Add([ordered]@{ nom = "détection $k"; entree = [ordered]@{ jeux = $jeux; processus = $procs }; attendu = $script:Lance })
}
Write-Juge 'detection.json' 'Détection d''une partie : index des jeux (Update-GameCache) et Test-GameRunning' $cas

# ===========================================================================
# 4. Résumé d'une partie : la vraie FrameMon, l'enregistrement et la bulle de fin
# ===========================================================================
if (-not ('FrameMon' -as [type])) { Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $modules 'natif.cs'), [Text.Encoding]::UTF8)) }
$cas = New-Object System.Collections.ArrayList
$entetes = @('Application,ProcessID,MsBetweenPresents,PresentMode,MsCPUBusy,MsGPUBusy', 'Application,ProcessID,FrameTime,PresentMode', 'Application,MsBetweenAppStart,MsCPUBusy,MsGPUBusy,PresentMode', 'Application,ProcessID,Autre')
for ($k = 0; $k -lt 100; $k++) {
    $ent = Choisir $entetes
    $cols = $ent.Split(',')
    $mode = Choisir @('Hardware: Independent Flip', 'Hardware: Legacy Flip', 'Composed: Flip')
    # Les morceaux de la partie : { durée d'image en ms, nombre, écart (0 = régulier), saccade tous les n }
    $morceaux = @(for ($i = 0; $i -lt (Choisir @(1, 2, 3, 4)); $i++) {
        [ordered]@{ ms = (Choisir @(4.17, 6.94, 8.33, 16.667, 33.333, 10, 25, 12.5)); nb = (Choisir @(0, 50, 200, 1000, 3000)); ecart = (Choisir @(0, 0.05, 0.2)); saccade = (Choisir @(0, 0, 97, 500)) }
    })
    $pause = Choisir @(-1, -1, 0, 100)
    [FrameMon]::Reset(); [FrameMon]::Paused = $false; [FrameMon]::MaxImages = 2000000
    [FrameMon]::Feed($ent)
    $lignes = New-Object System.Collections.Generic.List[string]
    $n = 0; $echantillons = New-Object System.Collections.ArrayList
    foreach ($m in $morceaux) {
        for ($i = 0; $i -lt $m.nb; $i++) {
            $ft = $m.ms
            if ($m.ecart) { $ft = $ft * (1 + $m.ecart * [math]::Sin($n * 0.7)) }
            if ($m.saccade -and ($n % $m.saccade) -eq 3) { $ft = 80 + ($n % 7) }
            $ft = [math]::Round($ft, 3)
            $v = @(foreach ($c in $cols) { switch ($c) { 'Application' { 'jeu.exe' } 'ProcessID' { '42' } 'PresentMode' { $mode } 'MsCPUBusy' { ($ft * 0.6).ToString($ci) } 'MsGPUBusy' { ($ft * 0.93).ToString($ci) } 'Autre' { 'x' } default { $ft.ToString($ci) } } })
            $l = $v -join ','
            if ($pause -ge 0 -and $n -ge $pause -and $n -lt $pause + 300) { [FrameMon]::Paused = $true } else { [FrameMon]::Paused = $false }
            [FrameMon]::Feed($l)
            $n++
            if ($n % 500 -eq 0) { [void]$echantillons.Add([math]::Round([FrameMon]::Sample(), 6)) }
        }
    }
    [FrameMon]::Paused = $false
    $b = [FrameMon]::Bilan()
    $live = [FrameMon]::Live()
    $s = $b[0]
    $t = @{ Name = (Choisir $nomsJeux); Key = 'jeu'; Start = [datetime]'2026-11-05T21:30:00'; Exclusive = ([string][FrameMon]::LastMode -match 'Legacy'); Series = @($echantillons | Where-Object { $_ -gt 0 } | ForEach-Object { [math]::Round($_, 1) }); Plateau = $b[1]; Refus = (Pile 5) }
    # (Complete-FpsTarget, sans écran : la décision et la bulle)
    $enreg = $null; $bulle = $null
    if ($s[3] -eq 0) {
        if ($t.Refus) { $bulle = [ordered]@{ titre = "Partie terminée : $($t.Name)"; texte = 'L''anti-triche de ce jeu bloque la mesure des FPS. À la prochaine partie, ouvre Nevermind avant de lancer le jeu : la mesure démarrera avant lui.' } }
    } elseif (-not ($s[4] -lt 30 -or $s[3] -lt 300)) {
        $diag = [pscustomobject]@{ Stutters = [int]$b[2][2] }
        $enreg = [ordered]@{ Date = $t.Start.ToString('s'); Game = $t.Name; Key = $t.Key
            Avg = [math]::Round($s[0], 1); Low1 = [math]::Round($s[1], 1); Low01 = [math]::Round($s[2], 1); Seconds = [int]$s[4]; Frames = [int]$s[3]
            Exclusive = [bool]$t.Exclusive; Series = @(Compress-Series $t.Series)
            MenuSec = $(if ($t.Plateau) { [int]$t.Plateau[0] } else { 0 }); MenuFps = $(if ($t.Plateau) { [int]$t.Plateau[1] } else { 0 }) }
        $pb = Test-FpsProblem ([pscustomobject]@{ Diag = $diag; Seconds = $enreg.Seconds; Avg = $enreg.Avg; Low1 = $enreg.Low1 })
        $bulle = if ($pb) { [ordered]@{ titre = "Partie terminée : $($t.Name)"; texte = ('{0:N0} FPS en moyenne, avec des chutes. Nevermind a regardé d''où ça vient : clique ici pour voir et corriger.' -f $s[0]) } }
                 else { [ordered]@{ titre = "Partie terminée : $($t.Name)"; texte = ('{0} de jeu, {1:N0} FPS en moyenne (1 % bas {2:N0}). Tout était fluide.' -f (Format-PlayTime $s[4]), $s[0], $s[1]) } }
        $enreg.Probleme = $pb
        $enreg.Verdict = @(Get-FpsVerdict ([pscustomobject]$enreg))
    }
    [void]$cas.Add([ordered]@{ nom = "partie $k"; entree = [ordered]@{ entete = $ent; mode = $mode; morceaux = $morceaux; pause = $pause; jeu = $t.Name; refus = $t.Refus }; attendu = [ordered]@{
        resume = @($s | ForEach-Object { [math]::Round($_, 6) }); palier = @($b[1] | ForEach-Object { [math]::Round($_, 6) }); occupe = @($b[2] | ForEach-Object { [math]::Round($_, 6) })
        direct = @($live | ForEach-Object { [math]::Round($_, 6) }); echantillons = @($echantillons); images = [FrameMon]::Frames; exclusif = [bool]$t.Exclusive
        enregistrement = $enreg; bulle = $bulle } })
}
Write-Juge 'parties.json' 'Résumé d''une partie : FrameMon (natif.cs), Save-FpsSession, Test-FpsProblem, bulle de fin' $cas

# ===========================================================================
# 5. Diagnostic : relevés de la partie (Get-FpsDiagData) et ce qu'il en dit (Get-FpsDiagnosis)
# ===========================================================================
function script:Test-PowerPlan { $script:FauxPlan }
function script:Get-SessionName($S) { [string]$S.Game }
function script:Get-FpsSessions { @($script:FauxSessions) }
function script:Test-GamePriority($Game) { [bool]$script:FauxPrio }
function script:ConvertTo-NvidiaVersion($v) { $script:FauxNvInst }
function script:Get-SessionGame($S) { if ($script:FauxJeuConnu) { @{ Name = [string]$S.Game; Exes = @('C:\Jeux\jeu.exe') } } else { $null } }
$cas = New-Object System.Collections.ArrayList
for ($k = 0; $k -lt 400; $k++) {
    $e = [ordered]@{}
    $e.avg = Choisir @(25, 45, 59.9, 60, 61, 90, 120, 143.8, 144.2, 165, 239)
    $e.low1 = [math]::Round($e.avg * (Choisir @(0.3, 0.49, 0.5, 0.7, 0.76, 0.9)), 1)
    $e.seconds = Choisir @(30, 60, 600, 3600)
    $e.series = @(if (Pile 3) { @() } else { $plat = Pile; for ($i = 0; $i -lt (Choisir @(5, 10, 40)); $i++) { if ($plat) { $e.avg } else { [math]::Round($e.avg * (1 + 0.1 * [math]::Sin($i)), 1) } } })
    $e.sansDiag = Pile 12
    $e.diag = [ordered]@{
        CpuRatio = (Choisir @(-1, 0.3, 0.69, 0.7, 0.86, 0.95)); GpuRatio = (Choisir @(-1, 0.2, 0.69, 0.7, 0.84, 0.85, 0.99)); Stutters = (Choisir @(0, 3, 40, 400))
        Cpu = (Choisir @($null, 20, 50, 90)); CpuMax = 95; Perf = (Choisir @($null, 60, 84.9, 85, 100)); Ram = (Choisir @($null, 50, 89.9, 90, 97))
        Gpu = (Choisir @($null, 5, 9.9, 50, 74.9, 75, 93, 99)); Temp = (Choisir @($null, 60, 82.9, 83, 90)); Vram = (Choisir @($null, 50, 94.9, 95, 100))
        Power = 80; Disk = (Choisir @($null, 20, 89.9, 90, 100)); DiskAvg = 10
        OnBattery = (Pile 4); Ups = (Pile 4); Laptop = (Pile 3); Dual = (Pile 3); Nvidia = (Pile); Rtx = (Pile); Amd = (Pile 3); GameMode = (Pile)
        Top = @(for ($i = 0; $i -lt (Choisir @(0, 1, 3)); $i++) { [ordered]@{ Name = (Choisir @('chrome', 'MsMpEng', 'dwm', 'Discord', 'msedgewebview2', 'EAAntiCheat.GameService', 'jeu')); Pct = (Choisir @(1, 3.9, 4, 12.5)) } })
        TopMem = @(for ($i = 0; $i -lt (Choisir @(0, 2, 5)); $i++) { [ordered]@{ Name = (Choisir @('chrome', 'firefox', 'svchost', 'Discord', 'msedgewebview2', 'jeu', 'Spotify')); MB = (Choisir @(150, 1024, 3500)) } })
        Path = 'C:\Jeux\jeu.exe'; Drive = 'C'; Media = (Choisir @('SSD', 'HDD', '', 'Unspecified')); Hz = (Choisir @(0, 60, 144)); HzMax = (Choisir @(0, 60, 144, 240)); Display = (Choisir @('', '\\.\DISPLAY1'))
    }
    $e.plan = -not (Pile 4)
    $e.prio = Pile
    $e.jeuConnu = -not (Pile 3)
    $e.autresParties = Choisir @(0, 0, 2)
    $e.ecranAnalyse = Pile
    $e.nvidia = if (Pile 3) { [ordered]@{ installe = (Choisir @('560.94', '566.36', $null)); dernier = '566.36'; url = 'https://www.nvidia.com/x' } } else { $null }
    $script:FauxPlan = $e.plan; $script:FauxPrio = $e.prio; $script:FauxJeuConnu = $e.jeuConnu
    $script:FauxSessions = @(for ($i = 0; $i -lt $e.autresParties; $i++) { [pscustomobject]@{ Key = 'jeu'; Id = "autre$i" } })
    $script:LastAnalysis = @{ Active = @(if ($e.ecranAnalyse) { @{ Id = 'display:\\.\DISPLAY1'; Status = 'warn' } }) }
    $script:AnalysisData = @{ NvLatest = $(if ($e.nvidia) { @{ Version = $e.nvidia.dernier; Url = $e.nvidia.url } }); GPUs = @(@{ Name = 'NVIDIA GeForce RTX 4070'; DriverVersion = 'x' }) }
    $script:FauxNvInst = if ($e.nvidia) { $e.nvidia.installe } else { $null }
    $d = if ($e.sansDiag) { $null } else { [pscustomobject]$e.diag }
    if ($d) { $d.Top = @($e.diag.Top | ForEach-Object { [pscustomobject]$_ }); $d.TopMem = @($e.diag.TopMem | ForEach-Object { [pscustomobject]$_ }) }
    $s = [pscustomobject]@{ Id = 'cette'; Game = 'Mon jeu'; Key = 'jeu'; Avg = $e.avg; Low1 = $e.low1; Seconds = $e.seconds; Series = $e.series; Diag = $d }
    $g = Get-FpsDiagnosis $s
    $items = @($g.Items | ForEach-Object { [ordered]@{ statut = $_.Status; titre = $_.Title; detail = $_.Detail; lignes = @($_.Items); actions = @($_.Actions | ForEach-Object { [ordered]@{ texte = $_.Label; arg = $(if ($_.Arg -is [string]) { $_.Arg } elseif ($_.Arg) { 'objet' } else { $null }); confirmer = $_.Confirm } }) } })
    [void]$cas.Add([ordered]@{ nom = "diagnostic $k"; entree = $e; attendu = [ordered]@{ limite = $g.Limit; probleme = $g.Problem; statut = $g.Status; titre = $g.Headline; texte = $g.Text; palier = $g.Cap; elements = $items } })
}
# Relevés pendant la partie : processus vus deux fois (Merge-FpsProcSample), puis le résumé (Get-FpsDiagData)
for ($k = 0; $k -lt 120; $k++) {
    $sys = @(for ($i = 0; $i -lt (Choisir @(0, 1, 4, 20)); $i++) { [ordered]@{ Cpu = (Choisir @($null, 12.34, 55.5, 99.9)); Perf = (Choisir @($null, 80, 101.5)); Ram = (Choisir @($null, 40.04, 91.26)); Gpu = (Choisir @($null, 30, 97.45)); Temp = (Choisir @($null, 60, 84)); Vram = (Choisir @($null, 50, 96)); Power = (Choisir @($null, 70.25)); Disk = (Choisir @($null, 0, 100)) } })
    $releves = @(for ($i = 0; $i -lt (Choisir @(0, 1, 2, 4)); $i++) {
        [ordered]@{ secondes = (10 * ($i + 1)); lignes = @(for ($j = 0; $j -lt (Choisir @(0, 2, 6)); $j++) { $id = Choisir @(10, 11, 12, 13); '{0}|{1}|{2}|{3}' -f $id, (Choisir @('chrome', 'Discord', 'svchost')), ((Choisir @(1.5, 30, 31.25, 100)) + 10 * $i).ToString($ci), (Choisir @(1048576, 300000000, 5000000000)) }) }
    })
    $coeurs = Choisir @(4, 8, 16)
    $T = @{ Sys = @($sys | ForEach-Object { $h = @{}; foreach ($x in $_.Keys) { $h[$x] = $_[$x] }; $h }); ProcCpu = @{}; ProcMem = @{}; ProcSeconds = 0.0; PrevProc = $null; PrevProcTime = $null; OnBattery = (Pile 4); Path = (Choisir @('', 'C:\Jeux\jeu.exe', 'd:\x.exe', 'X')) }
    foreach ($r in $releves) { Merge-FpsProcSample $T $r.lignes ([datetime]'2026-11-05T20:00:00').AddSeconds($r.secondes) }
    $busy = @((Choisir @(-1, 0.31234)), (Choisir @(-1, 0.9876)), (Choisir @(0, 12)))
    $script:AnalysisData = @{ Disks = @(@{ Letters = @('C'); Disk = @{ BusType = 'NVMe'; MediaType = 'SSD' } }, @{ Letters = @('D'); Disk = @{ BusType = 'SATA'; MediaType = 'HDD' } }); GPUs = @(Choisir @(@(), @(@{ Name = 'NVIDIA GeForce RTX 4070' }), @(@{ Name = 'AMD Radeon RX 6600' }, @{ Name = 'Intel UHD' }))) }
    $script:IsLaptop = Pile; $script:HasUps = Pile 3
    $script:Reglages = @{ GameMode = (Pile) }
    # (le nombre de cœurs et l'écran viennent du PC : remplacés)
    $code = (Get-Command Get-FpsDiagData).Definition.Replace('[Environment]::ProcessorCount', '$script:FauxCoeurs').Replace('[OGNative]::GetDisplays()', '$script:FauxEcrans')
    Set-Item function:script:Get-FpsDiagData2 -Value ([scriptblock]::Create($code))
    $script:FauxCoeurs = $coeurs
    $script:FauxEcrans = @(Choisir @(@(), @('\\.\DISPLAY1|carte|1920|1080|60|144|1'), @('\\.\DISPLAY2|carte|1920|1080|60|60|0', '\\.\DISPLAY1|carte|2560|1440|144|165|1')))
    $dd = Get-FpsDiagData2 $T $busy
    [void]$cas.Add([ordered]@{ nom = "relevés $k"; entree = [ordered]@{ releves = $sys; processus = $releves; coeurs = $coeurs; batterie = $T.OnBattery; chemin = $T.Path; occupe = $busy; gpus = @($script:AnalysisData.GPUs | ForEach-Object { $_.Name }); portable = $script:IsLaptop; onduleur = $script:HasUps; modeJeu = $script:Reglages.GameMode; ecrans = $script:FauxEcrans }; attendu = $dd })
}
Write-Juge 'diagnostic.json' 'Diagnostic des FPS : Get-FpsDiagnosis et Get-FpsDiagData' $cas

# ===========================================================================
# 6. Alerte de température (Test-TempAlert) : une suite de mesures, une par seconde
# ===========================================================================
$cas = New-Object System.Collections.ArrayList
for ($k = 0; $k -lt 80; $k++) {
    # (par paliers de 1 à 40 s : l'alerte demande 15 s d'affilée à 87 °C ou plus)
    $long = Choisir @(5, 20, 40, 900, 900)
    $suite = New-Object System.Collections.ArrayList
    while ($suite.Count -lt $long) { $v = Choisir @($null, 60, 86.9, 87, 90, 95, 87, 88); for ($j = 0; $j -lt (Choisir @(1, 3, 14, 15, 16, 40)) -and $suite.Count -lt $long; $j++) { [void]$suite.Add($v) } }
    $suite = @($suite)
    $script:HotSince = $null; $script:HotAlerted = $null; $script:Bulles.Clear()
    $t0 = [datetime]'2026-11-05T20:00:00'
    $alertes = New-Object System.Collections.ArrayList
    # (Get-Date remplacé par l'horloge du test)
    $code = (Get-Command Test-TempAlert).Definition.Replace('Get-Date', '$script:Maintenant')
    Set-Item function:script:Test-TempAlert2 -Value ([scriptblock]::Create($code))
    for ($i = 0; $i -lt $suite.Count; $i++) {
        $script:Maintenant = $t0.AddSeconds($i)
        $Live = @{ GpuTemp = $suite[$i] }
        $avant = $script:Bulles.Count
        Test-TempAlert2
        if ($script:Bulles.Count -gt $avant) { [void]$alertes.Add([ordered]@{ seconde = $i; texte = $script:Bulles[$script:Bulles.Count - 1].texte }) }
    }
    [void]$cas.Add([ordered]@{ nom = "température $k"; entree = [ordered]@{ temperatures = $suite }; attendu = @($alertes) })
}
Write-Juge 'temperature.json' 'Alerte de température (Test-TempAlert), une mesure par seconde' $cas
