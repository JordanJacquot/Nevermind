# Suite des cas exportés par exporter-juges-jeux.ps1 : bibliothèques simulées, restes, pages Jeux et fiche d'un jeu,
# pages Rechercher, Paramètres et Aide.

# ===========================================================================
# 5. Les jeux installés (Get-InstalledGames) sur de faux PC
# ===========================================================================
foreach ($n in 'Get-SteamLibraries', 'Read-SteamManifests', 'Get-SteamGames') { Import-Fonction 'optimisations' $n }
Import-Fonction 'optimisations' 'Get-InstalledGames' { param($t) $t.Replace('@([IO.DriveInfo]::GetDrives() | Where-Object { $_.DriveType -eq ''Fixed'' -and $_.IsReady })', '@(Get-FauxDisques)') }

function script:Acf([string]$Id, [string]$Nom, [string]$Dossier) { "`"AppState`"`n{`n`t`"appid`"`t`t`"$Id`"`n`t`"name`"`t`t`"$Nom`"`n`t`"installdir`"`t`t`"$Dossier`"`n}`n" }
function script:Exe([string]$C, [int]$Ko) { @{ C = $C; Taille = $Ko * 1024 } }
$portrait = New-ImageB64 20 30
$paysage = New-ImageB64 40 20

$pc1 = @{
    Fichiers = @(
        @{ C = 'Steam\steamapps\libraryfolders.vdf'; T = "`"libraryfolders`"`n{`n`t`"0`"`n`t{`n`t`t`"path`"`t`t`"{BAC2}\\Steam`"`n`t}`n`t`"1`"`n`t{`n`t`t`"path`"`t`t`"{BAC2}\\Jeux Société`"`n`t}`n`t`"2`"`n`t{`n`t`t`"path`"`t`t`"{BAC2}\\Steam\\`"`n`t}`n}`n" }
        @{ C = 'Steam\steamapps\appmanifest_252950.acf'; T = (Acf '252950' 'Rocket League' 'rocketleague') }
        @{ C = 'Steam\steamapps\appmanifest_730.acf'; T = (Acf '730' 'Counter-Strike 2' 'Counter-Strike Global Offensive') }
        @{ C = 'Steam\steamapps\appmanifest_99.acf.bak'; T = (Acf '99' 'Pas une fiche' 'pasunefiche') }
        (Exe 'Steam\steamapps\common\rocketleague\Binaries\Win64\RocketLeague.exe' 900)
        (Exe 'Steam\steamapps\common\rocketleague\Binaries\Win64\UnityCrashHandler64.exe' 900)
        (Exe 'Steam\steamapps\common\rocketleague\Launcher.exe' 300)
        (Exe 'Steam\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\cs2.exe' 1200)
        (Exe 'Steam\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\setup.exe' 1200)
        (Exe 'Steam\steamapps\common\Counter-Strike Global Offensive\a\b\c\d\trop-profond.exe' 5000)
        (Exe 'Steam\steamapps\common\Counter-Strike Global Offensive\outil.exe' 100)
        (Exe 'Steam\steamapps\common\SteamVR\bin\vrmonitor.exe' 900)
        (Exe 'Steam\steamapps\common\VieuxReste\x.exe' 900)
        @{ C = 'Steam\steamapps\common\VieuxReste'; Dossier = $true; Age = 30 }
        (Exe 'Steam\steamapps\common\ResteRecent\x.exe' 900)
        @{ C = 'Steam\appcache\x'; Dossier = $true }
        @{ C = 'Jeux Société\steamapps\appmanifest_413150.acf'; T = (Acf '413150' 'Stardew Valley' 'Stardew Valley') }
        @{ C = 'Jeux Société\steamapps\appmanifest_1.acf'; T = "`"AppState`"`n{`n`t`"appid`"`t`"1`"`n}" }
        (Exe 'Jeux Société\steamapps\common\Stardew Valley\Stardew Valley.exe' 60)
        (Exe 'Jeux Société\steamapps\common\Stardew Valley\StardewModdingAPI.exe' 55)
        (Exe 'Jeux Société\steamapps\common\Stardew Valley\petit.exe' 10)
        @{ C = 'Jeux Société\steamapps\common\Reste Incomplet'; Dossier = $true; Age = 30 }
        @{ C = 'ProgramData\Epic\EpicGamesLauncher\Data\Manifests\A1.item'; T = '{ "DisplayName": "Rocket League", "InstallLocation": "{BAC2}\\Epic\\rocketleague", "LaunchExecutable": "Binaries/Win64/RocketLeague.exe", "CatalogNamespace": "9773aa1aa54f4f7b80e44bef04986cea", "CatalogItemId": "530145df28a24424923f5828cc9031a1", "AppName": "Sugar" }' }
        @{ C = 'ProgramData\Epic\EpicGamesLauncher\Data\Manifests\B2.item'; T = '{ "DisplayName": "Absent", "InstallLocation": "{BAC2}\\Epic\\absent" }' }
        @{ C = 'ProgramData\Epic\EpicGamesLauncher\Data\Manifests\C3.item'; T = '{ "DisplayName": "Fortnite", "InstallLocation": "{BAC2}\\Epic\\Fortnite" }' }
        @{ C = 'ProgramData\Epic\EpicGamesLauncher\Data\Manifests\D4.item'; T = 'pas du json' }
        (Exe 'Epic\rocketleague\Binaries\Win64\RocketLeague.exe' 900)
        (Exe 'Epic\Fortnite\FortniteGame\Binaries\Win64\FortniteClient-Win64-Shipping.exe' 2000)
        (Exe 'Epic\Fortnite\FortniteGame\Binaries\Win64\FortniteLauncher.exe' 2000)
        (Exe 'GOG\Witcher 3\bin\x64\witcher3.exe' 2000)
        (Exe 'Ubi\Rayman Legends\Rayman Legends.exe' 800)
        (Exe 'Ubi\Anno 1800\Bin\Win64\Anno1800.exe' 800)
        (Exe 'Riot\VALORANT\live\VALORANT.exe' 400)
        (Exe 'Riot\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe' 3000)
        (Exe 'Blizzard\Overwatch\Overwatch.exe' 3000)
        (Exe 'Blizzard\Battle.net\Battle.net.exe' 3000)
        (Exe 'EA\Apex\r5apex.exe' 3000)
        (Exe 'EA\Apex\EasyAntiCheat_launcher.exe' 3000)
        (Exe 'Steam\steamapps\common\EA Sur Steam\jeu.exe' 3000)
        @{ C = 'AppData\zaap\repositories\production\dofus\dofus3\release.json'; T = '{ "location": "{BAC2}\\Ankama\\Dofus3", "gameUid": "dofus", "name": "dofus3", "gameName": "dofus" }' }
        @{ C = 'AppData\zaap\repositories\production\dofus\dofus3\data\home_cover.jpg'; B64 = $portrait }
        @{ C = 'AppData\zaap\repositories\production\dofus\dofus3\data\logo.png'; Taille = 10 }
        @{ C = 'AppData\zaap\repositories\production\dofus\main\release.json'; T = '{ "location": "{BAC2}\\Ankama\\Dofus2", "gameUid": "dofus", "name": "main", "gameName": "dofus" }' }
        @{ C = 'AppData\zaap\repositories\production\dofus\main\data\home_cover.jpg'; B64 = $paysage }
        @{ C = 'AppData\zaap\repositories\production\dofus\main\data\cover.jpg'; Taille = 10 }
        @{ C = 'AppData\zaap\repositories\production\dofus\main\data\icon.png'; Taille = 10 }
        @{ C = 'AppData\zaap\repositories\production\retro\main\release.json'; T = '{ "location": "{BAC2}\\Ankama\\Retro", "gameUid": "retro", "name": "main", "gameName": "retro" }' }
        @{ C = 'AppData\zaap\repositories\production\waven\beta\release.json'; T = '{ "location": "{BAC2}\\Ankama\\Waven", "gameUid": "waven", "name": "beta", "gameName": "waven" }' }
        @{ C = 'AppData\zaap\repositories\production\wakfu\main\release.json'; T = '{ "location": null, "gameUid": "wakfu", "name": "main", "gameName": "wakfu" }' }
        (Exe 'Ankama\Dofus3\Dofus.exe' 900)
        (Exe 'Ankama\Dofus2\Dofus.exe' 184)
        (Exe 'Ankama\Retro\Dofus Retro.exe' 900)
        (Exe 'Ankama\Waven\Waven.exe' 900)
        (Exe 'D1\XboxGames\Forza Horizon 5\Content\ForzaHorizon5.exe' 3000)
        (Exe 'D1\XboxGames\Halo\Halo.exe' 3000)
        (Exe 'D1\XboxGames\GameSave\x.exe' 3000)
        (Exe 'D1\XboxGames\Vide\petit.exe' 1)
        (Exe 'Programmes\Mon Jeu\MonJeu.exe' 500)
    )
    Registre = @(
        @{ Ruche = 'HKCU'; Cle = 'Software\Valve\Steam'; Valeurs = @{ SteamPath = '{BAC}/Steam' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\WOW6432Node\GOG.com\Games\1207664663'; Valeurs = @{ gameName = 'The Witcher 3: Wild Hunt'; path = '{BAC}\GOG\Witcher 3\'; exe = '"{BAC}\GOG\Witcher 3\bin\x64\witcher3.exe"' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\WOW6432Node\GOG.com\Games\2'; Valeurs = @{ gameName = 'Absent'; path = '{BAC}\GOG\absent' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\635'; Valeurs = @{ InstallDir = '{BAC}/Ubi/Rayman Legends/' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\4553'; Valeurs = @{ InstallDir = '{BAC}/Ubi/Anno 1800/' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Uplay Install 4553'; Valeurs = @{ DisplayName = 'Anno 1800 '; InstallLocation = '{BAC}\Ubi\Anno 1800'; Publisher = 'Ubisoft'; UninstallString = '"C:\Ubisoft\upc.exe" uplay://uninstall/4553'; DisplayIcon = '{BAC}\Ubi\Anno 1800\Bin\Win64\Anno1800.exe,0' } }
        @{ Ruche = 'HKCU'; Cle = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\Riot Game valorant.live'; Valeurs = @{ DisplayName = 'VALORANT'; InstallLocation = '{BAC}\Riot\VALORANT'; Publisher = 'Riot Games, Inc'; UninstallString = '"C:\Riot Games\Riot Client\RiotClientServices.exe" --uninstall-product=valorant --uninstall-patchline=live'; DisplayIcon = 'C:\ProgramData\Riot Games\Metadata\valorant.live\valorant.live.ico' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Overwatch'; Valeurs = @{ DisplayName = 'Overwatch'; InstallLocation = '{BAC}\Blizzard\Overwatch'; Publisher = 'Blizzard Entertainment'; UninstallString = '"C:\Battle.net\Battle.net Uninstaller.exe" --lang=frFR --uid=prometheus --displayname="Overwatch"'; DisplayIcon = '{BAC}\Blizzard\Overwatch\Overwatch.exe' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Battle.net'; Valeurs = @{ DisplayName = 'Battle.net'; InstallLocation = '{BAC}\Blizzard\Battle.net'; Publisher = 'Blizzard Entertainment'; UninstallString = 'x' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{1234-APEX}'; Valeurs = @{ DisplayName = 'Apex Legends'; InstallLocation = '"{BAC}\EA\Apex\"'; Publisher = 'Electronic Arts'; UninstallString = '{BAC}\EA\Apex\__Installer\Cleanup.exe uninstall_game -autologging'; DisplayIcon = '{BAC}\EA\Apex\r5apex.exe,-101' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\EASteam'; Valeurs = @{ DisplayName = 'EA Sur Steam'; InstallLocation = '{BAC}\Steam\steamapps\common\EA Sur Steam'; Publisher = 'Electronic Arts' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\MonJeu'; Valeurs = @{ DisplayName = 'Mon Jeu'; InstallLocation = '{BAC}\Programmes\Mon Jeu'; Publisher = 'Moi'; UninstallString = '"{BAC}\Programmes\Mon Jeu\unins000.exe"'; DisplayIcon = '{BAC}\Programmes\Mon Jeu\MonJeu.ico' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Steam App 252950'; Valeurs = @{ DisplayName = 'Rocket League'; InstallLocation = '{BAC}\Steam\steamapps\common\rocketleague'; Publisher = 'Psyonix'; UninstallString = 'steam://uninstall/252950'; DisplayIcon = '{BAC}\Steam\steamapps\common\rocketleague\Binaries\Win64\RocketLeague.exe' } }
        @{ Ruche = 'HKCR'; Cle = 'zaap\shell\open\command'; Valeurs = @{ '' = '"{BAC}\Ankama\Ankama Launcher.exe" "%1"' } }
    )
    Disques = @('{BAC}\D1', '{BAC}\D2')
    Env = @{ ProgramData = '{BAC}\ProgramData'; APPDATA = '{BAC}\AppData' }
}
$pc2 = @{ Fichiers = @(); Registre = @(); Disques = @() }
$pc3 = @{
    Fichiers = @(
        @{ C = 'S\steamapps\appmanifest_10.acf'; T = (Acf '10' 'Jeu Dix' 'dix') }
        @{ C = 'S\steamapps\appmanifest_11.acf'; T = '' }
        (Exe 'S\steamapps\common\dix\dix.exe' 300)
        @{ C = 'S\steamapps\common\VieuxSansFiche'; Dossier = $true; Age = 40 }
        (Exe 'U\Assassin''s Creed Valhalla\ACValhalla.exe' 3000)
    )
    Registre = @(
        @{ Ruche = 'HKCU'; Cle = 'Software\Valve\Steam'; Valeurs = @{ SteamPath = '{BAC}\S' } }
        @{ Ruche = 'HKLM'; Cle = 'SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\13504'; Valeurs = @{ InstallDir = '{BAC}/U/Assassin''s Creed Valhalla/' } }
        @{ Ruche = 'HKCR'; Cle = 'zaap\shell\open\command'; Valeurs = @{ '' = 'C:\pas entre guillemets.exe' } }
    )
    Disques = @()
}
function script:ConvertTo-JeuJuge($g) {
    @{ Name = [string]$g.Name; Exes = @($g.Exes | ForEach-Object { [string]$_ }); Source = [string]$g.Source; Dir = [string]$g.Dir; AppId = [string]$g.AppId; Launch = [string]$g.Launch
        Uninstall = [string]$g.Uninstall; Icon = [string]$g.Icon; Leftover = [bool]$g.Leftover; Incomplet = [bool]$g.Incomplet
        Art = $(if ($g.Art) { @{ Cover = [string]$g.Art.Cover; Hero = [string]$g.Art.Hero; Logo = [string]$g.Art.Logo } }) }
}
$casBiblio = @(foreach ($pc in $pc1, $pc2, $pc3) {
    $null = New-FauxPc $pc
    $r = @(Get-InstalledGames)
    (ConvertTo-Juge @{ pc = $pc; jeux = @($r | ForEach-Object { ConvertTo-JeuJuge $_ }) })
})
Save-Juge $sortieJeux 'biblio' $casBiblio

# ===========================================================================
# 6. Les restes de jeux désinstallés : la barre (Update-LeftoverBar)
# ===========================================================================
foreach ($n in 'Update-LeftoverBar', 'Get-LeftoverIncompletText', 'Get-BigLeftovers') { Import-Fonction 'bibliotheque' $n }
$null = New-FauxPc @{ Fichiers = @(@{ C = 'a'; Dossier = $true }, @{ C = 'b'; Dossier = $true }, @{ C = 'c'; Dossier = $true }, @{ C = 'd'; Dossier = $true }, @{ C = 'e'; Dossier = $true }) }
$MB = 1048576
$casRestes = @()
foreach ($c in @(
    @(), @(, @('A', 'a', 60, $false)), @(, @('A', 'a', 49, $false)), @(, @('A', 'a', 60, $true)), @(, @('A', 'x', 600, $false)),
    @(@('A', 'a', 60, $false), @('B', 'b', 5000, $false)), @(@('A', 'a', 60, $false), @('B', 'b', 70, $false), @('C', 'c', 2048, $false), @('D', 'd', 51, $false)),
    @(@('A', 'a', (1024 * 1024 * 1.5), $false), @('E', 'e', 10, $false)))) {
    foreach ($inc in $false, $true) {
        $script:Leftovers = @(foreach ($x in $c) { @{ Name = $x[0]; Dir = (Join-Path $script:Ici $x[1]) } })
        $script:LeftoverSizes = @{}; $script:LeftoverRecents = @{}
        foreach ($x in $c) { $script:LeftoverSizes[(Join-Path $script:Ici $x[1])] = [double]$x[2] * $MB; $script:LeftoverRecents[(Join-Path $script:Ici $x[1])] = $x[3] }
        $script:LeftoversIncomplets = @(if ($inc) { 'X' })
        $script:LeftoverJob = $null
        $script:ui = @{ LibLeftoverBar = @{ Visibility = 'Collapsed' }; LibLeftoverText = @{ Text = '' }; TestOverlay = @{ Visibility = 'Collapsed' } }
        Update-LeftoverBar
        $casRestes += (ConvertTo-Juge @{ restes = @(foreach ($x in $c) { @{ nom = $x[0]; dossier = "{BAC}\$($x[1])"; mo = [double]$x[2]; recent = $x[3] } }); incomplet = $inc
            texte = $(if ($script:ui.LibLeftoverBar.Visibility -eq 'Visible') { $script:ui.LibLeftoverText.Text } else { $null }) })
    }
}
Save-Juge $sortieJeux 'restes' $casRestes

if (Test-Path (Join-Path $PSScriptRoot 'exporter-juges-jeux.cas3.ps1')) { . (Join-Path $PSScriptRoot 'exporter-juges-jeux.cas3.ps1') }
