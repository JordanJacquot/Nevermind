# Les cas exportés par exporter-juges-jeux.ps1 (chargé par lui, dans son contexte : ne se lance pas seul).

# ===========================================================================
# 1. Les petits outils de texte
# ===========================================================================
Import-Fonction 'recherche' 'ConvertTo-SearchText'
Import-Fonction 'recherche' 'Test-OneTypo'
Import-Fonction 'jeu' 'Format-PlayTime'
Import-Fonction 'bibliotheque' 'Format-LastPlayed'
Import-Fonction 'systeme' 'Format-Size'

$mots = @('', '   ', 'Mode jeu', 'Démarrage', 'DÉMARRAGE', 'Éléphant-Rose  2', 'Rocket League®', "l'été", 'Œuvre', 'ça va ?', 'Ctrl+Maj+F',
    'Assassin''s Creed® Valhalla', 'Counter-Strike 2', 'Tom Clancy''s Rainbow Six® Siege', 'Café crème', 'naïve façade', 'straße', 'İstanbul',
    'Ångström', 'ﬁn', '²³', 'Ⅻ', 'a-b_c.d/e\f', 'Café ☕ noir', 'Wörld Of Wärcraft', 'ÀÉÎÕÜ àéîõü ç Ç ñ Ñ', 'μ Ω', 'Привет мир', '日本語のゲーム',
    'tab	ici', "ligne`nsuite", '   espaces   multiples   ', '100 % FPS', 'v1.2.3', 'Ⓐⓑ', 'ʼapostrophe', 'Ǆemal', ('x' * 300))
$texte = [ordered]@{
    recherche = @(foreach ($m in $mots) { @{ e = $m; s = (ConvertTo-SearchText $m) } })
    duree = @(foreach ($s in 0, 1, 29.5, 30.5, 45, 89, 89.4, 89.5, 89.6, 90, 91, 149, 150, 151, 600, 2970, 3000, 3570, 3599, 3600, 3629, 3631, 5400, 5430, 7199, 7200, 9000, 86399, 360000, -5) { @{ e = [double]$s; s = (Format-PlayTime $s) } })
    derniere = @(foreach ($d in '', '2026-11-06T09:05:00', '2026-11-06T23:59:59', '2026-11-05T23:59:59', '2026-11-05T00:00:00', '2026-11-04T12:00:00', '2026-11-01T10:00:00', '2026-10-31T10:00:00', '2026-10-30T10:00:00', '2025-12-25T08:00:00', '2026-11-07T10:00:00', '2026-11-06T15:00:00') { @{ e = $d; s = (Format-LastPlayed $d) } })
    n0 = @(foreach ($x in 0, 0.4, 0.5, 1.5, 2.5, 3.5, -0.4, -0.5, -2.5, 59.5, 59.49, 999.5, 1234.5, 12345.678, 1234567, -1234.5, 144, 0.1 + 0.2, 1e15, 60.04999999) { @{ e = [double]$x; s = ('{0:N0}' -f [double]$x); s1 = ('{0:N1}' -f [double]$x) } })
    taille = @(foreach ($x in 0, 500, 1024, 1536, 1048575, 1048576, 52428800, 1073741823, 1073741824, 1610612736, 1099511627776, 3.3e12) { @{ e = [double]$x; s = (Format-Size $x) } })
    entier = @(foreach ($x in 0.5, 1.5, 2.5, -0.5, -1.5, 99.5, 100.4999) { @{ e = [double]$x; s = [int]$x } })
    titre = @(foreach ($m in 'waven', 'wAKFU online', 'DOFUS touch', 'dofus-retro', 'l''arène', 'king of hearts 2') { @{ e = $m; s = (Get-Culture).TextInfo.ToTitleCase($m) } })
    tri = @(foreach ($l in @(@('b', 'A', 'a', 'B', 'é', 'e', 'E', 'f'), @('Rocket League', 'rocket league', 'Rocket-League', 'RocketLeague', 'Rocket  League'), @('Zelda', 'Éclipse', 'eclipse', 'Age of Empires', 'age of empires II', '7 Days to Die', '#Tag', ' Espace'), @('Dofus 2', 'Dofus', 'Dofus Retro', 'DOFUS', 'dofus 3'), @('Counter-Strike 2', 'Counter Strike', 'CounterStrike', 'Celeste'))) {
        @{ e = $l; s = @($l | Sort-Object { $_ }) } })
}
$typos = @(@('overlay', 'overlya'), @('overlay', 'overly'), @('overlay', 'overlays'), @('overlay', 'averlay'), @('overlay', 'ovrly'), @('dns', 'dsn'), @('demarrage', 'demarage'), @('a', 'b'), @('ab', 'ba'), @('abc', 'abc'), @('abcd', 'abdc'), @('abcd', 'bacd'), @('abcd', 'ab'), @('nettoyage', 'netoyage'), @('', 'a'))
$texte.fautes = @(foreach ($p in $typos) { @{ a = $p[0]; b = $p[1]; s = [bool](Test-OneTypo $p[0] $p[1]) } })
Save-Juge $sortieJeux 'texte' @($texte)
Save-Juge $sortieDivers 'texte' @(@{ recherche = $texte.recherche; fautes = $texte.fautes })

# ===========================================================================
# 2. La recherche des réglages (Find-Settings)
# ===========================================================================
foreach ($f in 'Get-SearchIndex', 'Get-SearchWhere', 'Find-Settings') { Import-Fonction 'recherche' $f }
Import-Variable 'onglets' 'GamingSubPages'
Import-Variable 'onglets' 'PerfSubPages'
Import-Variable 'parametres' 'SettingsTabs'
Import-Variable 'navigation' 'PageNames'
Import-Variable 'parametres' 'ManagedThemeText'
function script:Test-OrgVisible { [bool]$script:OrgFaux }
$requetes = @('mode jeu', 'fps', 'FPS', 'compteur', 'overlya', 'overlay', 'dns', 'Dofus', 'organizer', 'organiser', 'demarrage', 'démarrage', 'DÉMARRAGE',
    'netoyage', 'nettoyge', 'ram', 'gpu', 'thème', 'theme', 'mindset', 'raccourci', 'beta', 'bêta', 'lag', 'ping', 'restes', 'place disque', 'virus',
    'antivirus rapide', 'restaurer', 'pilote nvidia', 'rocket', 'profil rocket', 'rocket league', 'Counter', 'jeu', 'jouer', 'a', 'zz', '', '   ', '!!!', 'é',
    'mode', 'mod', 'mo', 'jeux', 'tests', 'test', 'tst', 'carte', 'carte reseau', 'carte réseau', 'wifi', 'box', 'bufferbloat', 'tout', 'tout corriger',
    'corigger', 'sauvegarde', 'historique', 'microsoft', 'telemetrie', 'télémétrie', 'pare feu', 'pare-feu', 'firewall', 'internet', 'débit', 'debit',
    'vitesse', 'ssd', 'hdd', 'disque', 'batterie', 'portable', 'écran', 'pixels', 'temperature', 'température', 'onduleur', 'ups', 'score', 'analyse',
    'paramètres', 'parametres', 'settings', 'aide', 'bug', 'visite', 'tutoriel', 'pays', 'serveur', 'rdap', 'dofus raccourci', 'barre', 'flottante',
    'priorite', 'priorité haute', 'profil', 'ajouter', 'ajouter un jeu', 'itch', 'emulateur', 'émulateur', 'steam', 'epic', 'ubisoft', 'xbox',
    'reglage windows', 'plan', 'alimentation', 'xbox game bar', 'enregistrement', 'mesure', 'mesurer', 'mesurer fps', 'fps bas', 'saccades', 'freeze',
    'rame', 'lent', 'lag en ligne', 'connexion', 'conexion', 'connection', 'ctrl', 'hotkey', 'raccourci clavier', 'coin', 'position', 'discret',
    'style', 'compteur fps', 'osd', 'frame', 'framerate', 'images par seconde', 'mise a jour', 'mise à jour', 'maj', 'version', 'update', 'nouvelle version',
    'lancement auto', 'automatique', 'boot', 'allumage', 'bureau', 'icone', 'icône', 'skin', 'neon', 'néon', 'arcade', 'rubis', 'crepuscule', 'terminal',
    'appareils', 'intrus', 'notification', 'alerte', 'scanner', 'scan', 'audit', 'wps', 'upnp', 'espion', 'données envoyées', 'bloquer', 'bloquer internet',
    'point de restauration', 'restauration', 'rapport', 'html', 'partager', 'approuver', 'faux positif', 'tache planifiee', 'tâche planifiée',
    'definitions', 'signatures', 'dossier', 'analyser un dossier', 'complet', 'complete', 'complète', 'nettoyer', 'temporaires', 'espace', 'plein',
    'redemarrer', 'redémarrer', 'gigabit', '100 mbps', 'cable', 'câble', 'jitter', 'gigue', 'cloudflare', 'quad9', 'flush', 'cache', 'cache dns',
    'temps de jeu', 'heures', 'derniere partie', 'désinstaller', 'desinstaller', 'supprimer', 'optimiser', 'optimisation', 'tweaks', 'recommande',
    'liste', 'installes', 'battlenet', 'riot', 'gog', 'reconnu', 'manquant', 'discord', 'chrome', 'navigateur', 'fermer applis', 'comparaison', 'avant apres',
    'presentmon', 'zzz fps', 'fps zzz', 'mode jeu fps', 'jeu mode', 'MODE   JEU', 'mOdE-JeU', 'mode_jeu', 'mode.jeu', 'lAg', 'pIng!', '¿dns?',
    'demarrag', 'demarrge', 'demarrae', 'emarrage', 'xdemarrage', 'overlayy', 'organizr', 'orgnaizer', 'raccourc', 'racourci', 'raccourcci',
    'wallpaper', 'counter strike', 'cs2', 'valorant', 'league', 'dofus retro', 'retro', 'wakfu', 'forza', 'profil de dofus', 'profil de')
$jeuxRecherche = @(@{ Name = 'Rocket League'; Source = 'Epic Games' }, @{ Name = 'Counter-Strike 2'; Source = 'Steam' }, @{ Name = 'Dofus'; Source = 'Ankama' },
    @{ Name = 'Dofus Retro'; Source = 'Ankama' }, @{ Name = 'Forza Horizon 5'; Source = 'Xbox' }, @{ Name = ''; Source = 'Steam' }, @{ Name = 'VALORANT'; Source = 'Riot' })
$reglagesRecherche = @('Plan d''alimentation', 'Enregistrement en arrière-plan (Xbox Game Bar)', 'Mode jeu de Windows', 'Optimisations pour les jeux en fenêtre')
$casRecherche = @()
foreach ($conf in @(@{ Geree = $false; Org = $false; Jeux = $false }, @{ Geree = $true; Org = $false; Jeux = $false }, @{ Geree = $false; Org = $true; Jeux = $true }, @{ Geree = $true; Org = $true; Jeux = $true })) {
    $script:Managed = $conf.Geree
    $script:OrgFaux = $conf.Org
    Import-Variable 'recherche' 'SearchEntries'
    foreach ($st in (Get-Ast 'recherche').EndBlock.Statements) { if ($st -is [System.Management.Automation.Language.IfStatementAst] -and $st.Extent.Text -match '^if \(\$Managed\)') { Invoke-Expression ($st.Extent.Text -replace '\$SearchEntries', '$script:SearchEntries') } }
    $script:Games = if ($conf.Jeux) { @($jeuxRecherche) } else { $null }
    $script:TweakRows = if ($conf.Jeux) { @($reglagesRecherche | ForEach-Object { @{ Tweak = @{ Titre = $_ } } }) } else { @() }
    $res = @(foreach ($q in $requetes) { @{ q = $q; r = @(Find-Settings $q 8 | ForEach-Object { @{ t = [string]$_.T; ou = [string]$_.Where; p = [string]$_.P; tab = [string]$_.Tab; org = [bool]$_.Org; jeu = [string]$_.Game } }) } })
    $casRecherche += @{ geree = $conf.Geree; organizer = $conf.Org; jeux = $(if ($conf.Jeux) { $jeuxRecherche } else { @() }); reglages = $(if ($conf.Jeux) { $reglagesRecherche } else { @() }); requetes = $res }
}
Save-Juge $sortieDivers 'recherche' $casRecherche

# ===========================================================================
# 3. L'Organizer : titres des fenêtres, touches, visibilité
# ===========================================================================
Import-Fonction 'organizer' 'ConvertFrom-DofusTitle'
Import-Fonction 'organizer' 'Format-OrgKey'
$titres = @('Bob - Iop - 3.0.12 - Release', 'Bob - Dofus 2.70', 'Dofus', 'Dofus - 3.0', ' - Iop', 'Bob', 'Bob - ', 'Bob -  Cra  - 3.1', 'Élise - Eniripsa - 3.2.1.5 - Beta',
    'dofus retro', 'Bob - 3.0 - Release', 'Bob-Iop', 'A - B - C', 'DofusBot - Iop', 'Dofus3 - Iop', '  Bob  - Sram')
$touches = @('', 'Ctrl+Tab', 'Ctrl+Maj+Tab', 'D1', 'NumPad3', 'Ctrl+D5', 'Souris4', 'Alt+Souris5', 'Molette', 'Space', 'Return', 'Escape', 'Back', 'Oem7', 'Oem3', 'F12', 'Ctrl + Alt + X', 'D12', 'NumPad10', 'space')
$org = [ordered]@{
    titres = @(foreach ($t in $titres) { $r = ConvertFrom-DofusTitle $t; @{ e = $t; nom = [string]$r.Name; classe = [string]$r.Class } })
    touches = @(foreach ($t in $touches) { @{ e = $t; s = (Format-OrgKey $t) } })
}
# Test-OrgVisible : réglage, Organizer allumé, Dofus dans la bibliothèque
Import-Fonction 'organizer' 'Test-OrgVisible'
Import-Fonction 'donnees' 'Get-Setting'
function script:Get-OrgConfig { $script:OrgConfig }
$vis = @()
foreach ($reg in @('absent', $true, $false)) {
    foreach ($on in $false, $true) {
        foreach ($jeux in @(@(), @('Dofus'), @('Dofus Retro'), @('dofusbot'), @('Wakfu', 'Super Dofus!'), @('DOFUS 3'))) {
            $script:Settings = @{}; if ($reg -ne 'absent') { $script:Settings['OrganizerAffiche'] = $reg }
            $script:OrgConfig = @{ On = $on }
            $script:Games = @($jeux | ForEach-Object { @{ Name = $_ } })
            $vis += @{ reglage = $(if ($reg -eq 'absent') { $null } else { $reg }); on = $on; jeux = @($jeux); s = [bool](Test-OrgVisible) }
        }
    }
}
$org.visible = $vis
Save-Juge $sortieJeux 'organizer' @($org)
function script:Test-OrgVisible { [bool]$script:OrgFaux }

# ===========================================================================
# 4. Lancer et désinstaller un jeu
# ===========================================================================
Import-Fonction 'bibliotheque' 'Get-GameLaunch' { param($t) $t -replace 'Get-StartApps -ErrorAction Stop', '$script:FauxApplis' }
Import-Fonction 'bibliotheque' 'Split-Command'
Import-Fonction 'bibliotheque' 'Get-GameUninstall'
$script:FauxApplis = @([pscustomobject]@{ Name = 'Forza Horizon 5'; AppID = 'Microsoft.624F8B84B80_8wekyb3d8bbwe!ForzaHorizon5' }, [pscustomobject]@{ Name = 'Halo Infinite Campaign'; AppID = 'Microsoft.Halo_8wekyb3d8bbwe!App' })
$jl = @(
    @{ Name = 'A'; Exes = @('C:\Jeux\A\a.exe'); Launch = ''; Uninstall = ''; Source = 'Steam' },
    @{ Name = 'B'; Exes = @('C:\Jeux\B\b.exe'); Launch = 'steam://rungameid/10'; Uninstall = 'steam://uninstall/10'; Source = 'Steam' },
    @{ Name = 'C'; Exes = @('C:\Jeux\C\c.exe'); Launch = 'exe|C:\Riot\RiotClientServices.exe|--launch-product=valorant --launch-patchline=live'; Uninstall = '"C:\Riot\RiotClientServices.exe" --uninstall-product=valorant'; Source = 'Riot' },
    @{ Name = 'D'; Exes = @('C:\Jeux\D\d.exe'); Launch = 'exe|C:\Ankama\Ankama Launcher.exe|'; Uninstall = 'launcher|C:\Ankama\Ankama Launcher.exe'; Source = 'Ankama' },
    @{ Name = 'E'; Exes = @('C:\X\Forza\fh5.exe'); Launch = 'xbox:Forza Horizon 5'; Uninstall = 'launcher|ms-settings:appsfeatures'; Source = 'Xbox' },
    @{ Name = 'F'; Exes = @('C:\X\Halo\halo.exe'); Launch = 'xbox:halo infinite'; Uninstall = ''; Source = 'Xbox' },
    @{ Name = 'G'; Exes = @('C:\X\Inconnu\i.exe'); Launch = 'xbox:Inconnu'; Uninstall = ''; Source = '' },
    @{ Name = 'H'; Exes = @('C:\EA\h.exe'); Launch = ''; Uninstall = 'C:\Program Files\EA Games\H\uninstall.exe /S'; Source = 'EA app' },
    @{ Name = 'I'; Exes = @('C:\EA\i.exe'); Launch = 'EXE|C:\a.exe'; Uninstall = 'MsiExec.exe /X{12345678-1234}'; Source = 'GOG' },
    @{ Name = 'J'; Exes = @('C:\J\j.exe'); Launch = 'uplay://launch/635/0'; Uninstall = 'LAUNCHER|uplay://'; Source = 'Ubisoft Connect' },
    @{ Name = 'K'; Exes = @(); Launch = ''; Uninstall = 'battlenet://Pro'; Source = 'Battle.net' },
    @{ Name = 'L'; Exes = @('C:\L\l.exe'); Launch = ''; Uninstall = '  "C:\L\unins000.exe"   /SILENT  '; Source = 'Ajouté' },
    @{ Name = 'M'; Exes = @('C:\M\m.exe'); Launch = ''; Uninstall = 'C:\M\remove'; Source = 'GOG' }
)
$lanc = @(foreach ($g in $jl) {
    $l = Get-GameLaunch $g; $u = Get-GameUninstall $g
    @{ jeu = $g; lancer = @{ kind = [string]$l.Kind; path = [string]$l.Path; args = [string]$l.Args }; desinstaller = @{ kind = [string]$u.Kind; path = [string]$u.Path; args = [string]$u.Args } }
})
Save-Juge $sortieJeux 'lancement' @(@{ applis = @($script:FauxApplis | ForEach-Object { @{ nom = $_.Name; id = $_.AppID } }); cas = $lanc })

. (Join-Path $PSScriptRoot 'exporter-juges-jeux.cas2.ps1')
