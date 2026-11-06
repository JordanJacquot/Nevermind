# Suite des cas exportés par exporter-juges-jeux.ps1 : les pages Jeux et la fiche d'un jeu (Get-PagesJeux), puis les
# pages Rechercher, Paramètres et Aide (Get-PagesDivers).

# ===========================================================================
# 7. Les pages « jeux » et « jeu »
# ===========================================================================
Import-Fonction 'ecran-jeux' 'Get-PagesJeux'
Import-Fonction 'ecran-jeux' 'Get-JaquetteKit'
Import-Fonction 'ecran-perf' 'Get-DernierJeuKit'
Import-Fonction 'navigation' 'Get-LastPlayedGame'
foreach ($n in 'Get-CoverFile', 'Get-SteamArt', 'Find-SteamArt', 'Get-CoverIndex', 'Get-HeroFile', 'Get-LibraryGames', 'Get-PlayLog', 'Get-GameOptimizations', 'Test-CoversOnline', 'Test-AnnuaireServeurs') { Import-Fonction 'bibliotheque' $n }
foreach ($n in 'Get-FpsSessions', 'Get-SessionName', 'Test-FpsMeasure', 'Test-FpsOverlay', 'Get-FpsOverlayStyle', 'Get-FpsOverlayCorner', 'Get-LastChangeDate', 'Measure-FpsGroup', 'Test-FpsProblem', 'Get-GameExeNames', 'Test-GamePriority', 'Get-CustomGames') { Import-Fonction 'jeu' $n }
foreach ($n in 'Get-LagSessions', 'Test-LagMeasure', 'Test-LagConnexionOnly', 'Get-LagSessionLine', 'Format-ServerPlace', 'Format-Ms') { Import-Fonction 'lag' $n }
foreach ($n in 'Format-Country', 'Format-Bytes') { Import-Fonction 'trafic' $n }
foreach ($n in 'Import-History', 'ConvertFrom-HistoryEntry') { Import-Fonction 'assistance' $n }
Import-Fonction 'donnees' 'Read-JsonSur'
Import-Fonction 'organizer' 'Test-OrgVisible'
Import-Fonction 'organizer' 'Get-OrgConfig'
Import-Variable 'jeu' 'OverlayCorners'
Import-Variable 'jeu' 'GenericExe'
Import-Variable 'jeu' 'IfeoPath'
Import-Variable 'trafic' 'CountryNames'
Import-Variable 'ecran-pc' 'TonsStatut'
Import-Variable 'optimisations' 'DxPath'
function script:Set-LagDiag($R) { }
function script:Get-OrgConfig {
    if ($script:OrgConfig) { return $script:OrgConfig }
    $c = @{ On = $false }
    $j = Read-JsonSur $OrgFile
    if ($j -and $null -ne $j.On) { $c.On = [bool]$j.On }
    $script:OrgConfig = $c; $c
}

$donneesBase = @(
    @{ C = 'donnees\jeux.json'; T = '{"Rocket League":{"Last":"2026-11-06T10:15:00","Seconds":5430,"Count":12},"Dofus":{"Last":"2026-11-02T21:00:00","Seconds":89.5,"Count":1},"Counter-Strike 2":{"Last":"2026-10-01T08:00:00","Seconds":360000,"Count":300},"Jeu Disparu":{"Last":"2026-11-06T12:00:00","Seconds":10,"Count":1},"stardew valley":{"Last":"","Seconds":0,"Count":0}}' }
    @{ C = 'donnees\fps.json'; T = '[{"Id":"a1","Date":"2026-11-06T10:15:00","Game":"rocketleague","Key":"rocketleague","Avg":143.6,"Low1":61.2,"Seconds":5430,"Diag":{"Stutters":10}},{"Id":"a2","Date":"2026-11-05T20:00:00","Game":"Rocket League","Key":"rocketleague","Avg":120.4,"Low1":50,"Seconds":1200},{"Id":"a3","Date":"2026-10-28T20:00:00","Game":"Rocket League","Key":"rocketleague","Avg":100.5,"Low1":80,"Seconds":600},{"Id":"","Date":"2026-10-20T20:00:00","Game":"cs2","Key":"cs2","Avg":45.5,"Low1":20.5,"Seconds":65},{"Id":"b2","Date":"2026-10-21T20:00:00","Game":"Inconnu","Key":"inconnu","Avg":25,"Low1":10,"Seconds":3700,"Diag":{"Stutters":500}},{"Id":"b3","Date":"2026-09-01T09:00:00","Game":"Dofus","Key":"dofus","Avg":59.5,"Low1":40,"Seconds":30}]' }
    @{ C = 'donnees\lag.json'; T = '[{"Id":"l1","Date":"2026-11-06T09:00:00","Game":"Rocket League","Seconds":1800,"Diag":{"Level":"ok","Title":"Ta connexion est stable"},"Stats":{"gw":{"Med":2.4},"ref":{"Med":18.5},"srv":{"Med":35}},"Server":{"Proto":"udp","Pays":"DE","Ville":"Francfort"}},{"Id":"l2","Date":"2026-11-04T09:00:00","Game":"Test rapide","Seconds":30,"Diag":{"Level":"warn","Title":"Le Wi-Fi perd des paquets"},"Stats":{"gw":{"Med":-1},"ref":{"Dead":true,"Med":20}},"Server":{"Proto":"tcp","Pays":"united states of america"}},{"Id":"l3","Date":"2026-11-03T09:00:00","Game":"Valorant","Seconds":95,"Diag":{"Level":"bad","Title":"Le serveur est loin"},"Stats":{"srv":{"Med":120}},"Server":{"Proto":"tcp","Pays":"fr"},"Etw":true},{"Id":"l4","Date":"2026-11-02T09:00:00","Game":"X","Seconds":60,"Diag":{"Level":"","Title":"Mesure"}}]' }
    @{ C = 'donnees\historique.json'; T = '[{"Id":"h1","Date":"02/11/2026 à 18:30","Titre":"Annulé","Annule":true},{"Id":"h0","Date":"pas une date","Titre":"x"},{"Id":"h2","Date":"01/11/2026 à 12:00","Titre":"Réglages"}]' }
    @{ C = 'Steam\appcache\librarycache\252950\library_600x900.jpg'; Taille = 10 }
    @{ C = 'Steam\appcache\librarycache\252950\library_hero.jpg'; Taille = 10 }
    @{ C = 'Steam\appcache\librarycache\730\abcdef\library_600x900.jpg'; Taille = 10 }
    @{ C = 'Steam\appcache\librarycache\730\header.jpg'; Taille = 10 }
    @{ C = 'donnees\jaquettes\witcher.jpg'; Taille = 10 }
    @{ C = 'donnees\jaquettes\index.json'; T = '{"The Witcher 3: Wild Hunt":{"Cover":"{BAC2}\\donnees\\jaquettes\\witcher.jpg","Hero":"{BAC2}\\donnees\\jaquettes\\absent.jpg","Logo":"","Date":"2026-10-01T00:00:00"}}' }
    @{ C = 'appli\outils-tiers\PresentMon.exe'; Taille = 10 }
)
$scenarios = @(
    @{ nom = 'pas encore cherche'; jeuxNull = $true; ecran = 'jeux'; vide = $true }
    @{ nom = 'jeux sans donnees'; ecran = 'jeux'; vide = $true }
    @{ nom = 'tout'; ecran = 'jeux' }
    @{ nom = 'accueil'; ecran = 'accueil' }
    @{ nom = 'filtre steam'; ecran = 'jeux'; filtre = 'Steam'; recherche = '  counter '; parametres = '{"FpsOverlay":true,"FpsOverlayStyle":"discret","FpsOverlayCorner":"bd","LagMeasure":false,"LibCoversOnline":true}' }
    @{ nom = 'recents'; ecran = 'jeu'; filtre = 'Récents' }
    @{ nom = 'rien'; ecran = 'jeux'; recherche = 'zzz' }
    @{ nom = 'fiche rocket'; ecran = 'jeu'; choisi = 'Rocket League'; enJeu = 'Rocket League'; gpus = @('NVIDIA GeForce RTX 4070', 'Intel(R) UHD Graphics 770', 'Parsec Virtual Display'); disque = 'SSD'
        reglages = @(@{ Id = 'power'; Titre = 'Plan d''alimentation' }, @{ Id = 'dvr'; Titre = 'Enregistrement en arrière-plan' }, @{ Id = 'gm'; Titre = 'Mode jeu' }, @{ Id = 'hags'; Titre = 'HAGS' }) }
    @{ nom = 'fiche dofus'; ecran = 'jeu'; choisi = 'Dofus'; parametres = '{"FpsMeasure":false,"FpsOverlay":true,"GameMode":true,"OrganizerAffiche":true}'; gpus = @('AMD Radeon RX 7800 XT'); disque = 'HDD'
        reglages = @(@{ Id = 'power'; Titre = 'Plan d''alimentation' }) }
    @{ nom = 'fiche ajoute'; ecran = 'jeu'; choisi = 'Mon Super Jeu'; parametres = '{"CustomGames":[{"Name":"Mon Super Jeu","Exe":"{BAC2}\\Perso\\super.exe"},{"Name":"Absent","Exe":"{BAC2}\\Perso\\absent.exe"}],"OrganizerAffiche":false}' }
    @{ nom = 'fiche witcher'; ecran = 'jeu'; choisi = 'The Witcher 3: Wild Hunt'; gpus = @('A', 'B') }
    @{ nom = 'fiche hors ecran'; ecran = 'jeux'; choisi = 'Overwatch'; enJeu = 'Overwatch' }
    @{ nom = 'fiche xbox'; ecran = 'jeu'; choisi = 'Forza Horizon 5' }
)
$casPages = @()
foreach ($s in $scenarios) {
    $pc = @{ Fichiers = @($pc1.Fichiers) + $(if ($s.vide) { @() } else { $donneesBase }) + @(Exe 'Perso\super.exe' 300); Registre = @($pc1.Registre); Disques = $pc1.Disques; Env = $pc1.Env }
    if ($s.parametres) { $pc.Fichiers += @{ C = 'donnees\parametres.json'; T = $s.parametres } }
    $pc.Registre += @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\RocketLeague.exe\PerfOptions'; Valeurs = @{ CpuPriorityClass = 3 } }
    $pc.Registre += @{ Ruche = 'HKCU'; Cle = 'Software\Microsoft\DirectX\UserGpuPreferences'; Valeurs = @{ '{BAC}\Ankama\Dofus3\Dofus.exe' = 'GpuPreference=2;' } }
    $ici = New-FauxPc $pc
    $DataDir = Join-Path $ici 'donnees'
    $PlayFile = Join-Path $DataDir 'jeux.json'; $FpsFile = Join-Path $DataDir 'fps.json'; $LagFile = Join-Path $DataDir 'lag.json'
    $HistoryFile = Join-Path $DataDir 'historique.json'; $SettingsFile = Join-Path $DataDir 'parametres.json'; $OrgFile = Join-Path $DataDir 'organizer.json'
    $CoverDir = Join-Path $DataDir 'jaquettes'; $CoverIndexFile = Join-Path $CoverDir 'index.json'
    $PresentMonExe = Join-Path $ici 'appli\outils-tiers\PresentMon.exe'
    foreach ($v in 'PlayLog', 'Settings', 'LagSessions', 'History', 'CoverIndex', 'SteamCache', 'LibArt', 'OrgConfig', 'GameSession', 'LagSession', 'LagLive', 'Traffic', 'TrafficTimer', 'TrafficCounters', 'JsonBloques') { Set-Variable -Scope Script -Name $v -Value $null }
    if ($s.jeuxNull) { $script:Games = $null }
    else {
        $found = @(Get-InstalledGames | Where-Object { -not $_.Leftover })
        $script:Games = @($found) + @(Get-CustomGames | Where-Object { $p = $_.Exes[0]; -not @($found | Where-Object { @($_.Exes) -contains $p }).Count })
    }
    $script:GameIndex = @{}
    foreach ($g in @($script:Games)) { foreach ($e in @($g.Exes)) { $base = [IO.Path]::GetFileNameWithoutExtension($e).ToLower(); if ($base.Length -lt 3 -or $base -match $GenericExe) { continue }; if (-not $script:GameIndex.ContainsKey($base)) { $script:GameIndex[$base] = @{ Game = $g.Name } } } }
    $lettre = $ici.Substring(0, 1).ToUpper()
    $script:AnalysisData = @{ GPUs = @($s.gpus | Where-Object { $_ } | ForEach-Object { @{ Name = $_ } }); Disks = @(if ($s.disque) { @{ Letters = @($lettre); Disk = @{ MediaType = $s.disque } } }) }
    $script:LibTweaks = @{ At = (Get-Date); List = @($s.reglages | Where-Object { $_ }) }
    if ($s.enJeu) { $script:GameSession = @{ Game = $s.enJeu } }
    $script:Vue = @{ Ecran = $s.ecran }
    $script:Kit = @{ JeuxFiltre = $s.filtre; JeuxRecherche = $s.recherche }
    if ($s.choisi) { $script:Kit.JeuChoisi = @(Get-LibraryGames | Where-Object { $_.Name -eq $s.choisi })[0] }
    $script:ui = @{ LibLeftoverBar = @{ Visibility = 'Collapsed' }; ChkLibCovers = @{ IsChecked = (Test-CoversOnline) }; BtnTraffic = @{ Content = 'Démarrer' } }
    $p = Get-PagesJeux
    $sortie = [ordered]@{ jeux = $p.jeux }
    if ($p.Contains('jeu')) { $sortie.jeu = $p.jeu }
    $sortie.overlay = $p.overlay
    $casPages += (ConvertTo-Juge ([ordered]@{ nom = $s.nom; pc = $pc; ecran = $s.ecran; jeuxNull = [bool]$s.jeuxNull; filtre = [string]$s.filtre; recherche = [string]$s.recherche
        choisi = [string]$s.choisi; enJeu = [string]$s.enJeu; analyse = $script:AnalysisData; reglages = @($s.reglages | Where-Object { $_ }); applis = @($script:FauxApplis | ForEach-Object { @{ nom = $_.Name; id = $_.AppID } }); pages = $sortie }))
}
Save-Juge $sortieJeux 'pages' $casPages

# ===========================================================================
# 8. Les pages « recherche », « parametres » et « aide »
# ===========================================================================
Import-Fonction 'ecran-divers' 'Get-PagesDivers'
Import-Fonction 'ecran-divers' 'Get-TuilesTestsKit'
Import-Fonction 'ecran-divers' 'Get-OuKit'
Import-Variable 'ecran-divers' 'IconesTests'
foreach ($n in 'Get-CurrentThemeKey', 'Get-CurrentThemeName', 'Get-ThemeChoices') { Import-Fonction 'parametres' $n }
foreach ($n in 'Test-PiloteNvidia', 'Test-NouveautesEnLigne', 'Test-MajAuto', 'Test-PaysServeur') { Import-Fonction 'bibliotheque' $n }
Import-Variable 'themes' 'AppThemes'
function script:Get-ThemePacks { @() }
function script:Test-OrgVisible { [bool]$script:OrgFaux }
$casDivers = @()
foreach ($c in @(
    @{ geree = $false; q = ''; theme = 'neon'; parametres = '{}' }
    @{ geree = $true; q = 'mode jeu'; theme = 'arcade'; parametres = '{"Beta":true,"FpsOverlay":true,"LagMeasure":false,"GameMode":true,"LibCoversOnline":true,"NetWatch":true,"TrafficLookup":true,"LagPays":true,"PiloteNvidia":false,"MajAuto":false,"NouveautesEnLigne":false,"Theme":"arcade"}'; org = $true; raccourci = $false; demarrage = $false; majBouton = $false }
    @{ geree = $false; q = 'zzzz'; theme = 'rubis'; parametres = '{"FpsMeasure":true,"Theme":"rubis"}'; org = $true }
    @{ geree = $false; q = 'dofus'; theme = 'terminal'; parametres = '{"Theme":"terminal"}'; org = $true; coche = $true }
    @{ geree = $true; q = 'thème'; theme = 'neon'; parametres = '{}' }
)) {
    $ici = New-FauxPc @{ Fichiers = @(@{ C = 'donnees\parametres.json'; T = $c.parametres }) }
    $DataDir = Join-Path $ici 'donnees'; $SettingsFile = Join-Path $DataDir 'parametres.json'
    $script:Settings = $null; $script:JsonBloques = $null
    $script:Managed = $c.geree; $script:OrgFaux = [bool]$c.org
    $script:ThemeId = $c.theme; $script:ThemePack = $null
    $script:AppVersion = '1.15.6'
    Import-Variable 'recherche' 'SearchEntries'
    foreach ($st in (Get-Ast 'recherche').EndBlock.Statements) { if ($st -is [System.Management.Automation.Language.IfStatementAst] -and $st.Extent.Text -match '^if \(\$Managed\)') { Invoke-Expression ($st.Extent.Text -replace '\$SearchEntries', '$script:SearchEntries') } }
    $script:Games = $null; $script:TweakRows = @()
    $etat = @{ version = '1.15.6'; maj_texte = 'Version 1.15.6 : tu as la dernière version.'; maj_bouton = ($c.majBouton -ne $false); raccourci_texte = 'Mets Nevermind sur ton bureau pour l''ouvrir en un double clic.'
        raccourci_bouton = ($c.raccourci -ne $false); demarrage_ligne = ($c.demarrage -ne $false); demarrage_coche = [bool]$c.coche }
    $script:ui = @{ TestsPanel = @{ Children = @() }; BtnShortcut = @{ Visibility = $(if ($etat.raccourci_bouton) { 'Visible' } else { 'Collapsed' }) }; AutoStartRow = @{ Visibility = $(if ($etat.demarrage_ligne) { 'Visible' } else { 'Collapsed' }) }
        ChkAutoStart = @{ IsChecked = $etat.demarrage_coche }; UpdateStatus = @{ Text = $etat.maj_texte }; BtnCheckUpdate = @{ Visibility = $(if ($etat.maj_bouton) { 'Visible' } else { 'Collapsed' }) }; ShortcutStatus = @{ Text = $etat.raccourci_texte }
        ChkBeta = @{ IsChecked = [bool](Get-Setting 'Beta' $false) } }
    $script:Vue = @{ Ecran = 'parametres' }
    $script:Kit = @{ Recherche = $c.q; Resultats = @(if ($c.q) { Find-Settings $c.q 8 }) }
    $p = Get-PagesDivers
    $casDivers += (ConvertTo-Juge ([ordered]@{ geree = $c.geree; organizer = [bool]$c.org; q = $c.q; parametres = $c.parametres; etat = $etat; pages = [ordered]@{ recherche = $p.recherche; parametres = $p.parametres; aide = $p.aide } }))
}
Save-Juge $sortieDivers 'pages' $casDivers
