# Tests des restes de jeux Steam (page Mes jeux), sans droits administrateur et sans rien toucher sur le PC : une fausse
# bibliothèque Steam est fabriquée dans un dossier temporaire, le vrai Steam n'est jamais lu.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-bibliotheque.ps1
#
# Ce script contrôle :
#   1. libraryfolders.vdf est lu en UTF-8 (comme Steam l'écrit) : une bibliothèque « Jeux Société » est trouvée ;
#   2. un dossier qu'aucune fiche (appmanifest_*.acf) ne cite est un reste, mais pas s'il a bougé depuis moins de 7 jours ;
#   3. une fiche ouverte en exclusif (Steam en pleine mise à jour) : aucun dossier de cette bibliothèque n'est un reste,
#      et l'analyse est notée incomplète ;
#   4. juste avant de supprimer, les fiches sont relues : dossier gardé si une fiche est illisible, si une fiche le cite
#      (jeu réinstallé), ou si un fichier dedans a bougé depuis moins de 7 jours ; sinon il est bien supprimé, et la
#      suppression marche dans un fil séparé (comme dans Nevermind).
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
# Lecture des fonctions utiles dans les modules (rien ne se lance au chargement)
# ---------------------------------------------------------------------------
$asts = @{}
foreach ($f in 'optimisations', 'bibliotheque') {
    $tok = $null; $err = $null
    $asts[$f] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$f.ps1"), [ref]$tok, [ref]$err)
    if ($err) { Write-Host "$f.ps1 n'est pas lisible : $($err[0].Message)" -ForegroundColor Red; exit 1 }
}
function Import-Fonction([string]$Fichier, [string]$Nom) {
    $a = $asts[$Fichier].Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $false)
    if (-not $a) { Write-Host "Fonction $Nom introuvable dans $Fichier.ps1" -ForegroundColor Red; exit 1 }
    Invoke-Expression ($a.Extent.Text -replace "^function\s+$([regex]::Escape($Nom))", "function script:$Nom")
}
foreach ($n in 'Get-SteamLibraries', 'Read-SteamManifests', 'Get-SteamGames', 'Get-InstalledGames', 'Get-InstalledGamesWork') { Import-Fonction 'optimisations' $n }
foreach ($n in 'Measure-LeftoverDir', 'Measure-LeftoverDirs', 'Remove-LeftoverDirs') { Import-Fonction 'bibliotheque' $n }

$script:Bac = Join-Path $env:TEMP "nevermind-biblio-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
New-Item -ItemType Directory -Force -Path $script:Bac | Out-Null
$script:Bac = [IO.Path]::GetFullPath($script:Bac)
$ilYa30 = (Get-Date).AddDays(-30)

# Un dossier de jeu avec un fichier de 1 Ko ; -Vieux : tout daté d'il y a 30 jours
function New-DossierJeu([string]$Dir, [switch]$Vieux) {
    New-Item -ItemType Directory -Force -Path (Join-Path $Dir 'Sauvegardes') | Out-Null
    $f = Join-Path $Dir 'Sauvegardes\partie.sav'
    [IO.File]::WriteAllBytes($f, (New-Object byte[] 1024))
    if ($Vieux) {
        [IO.File]::SetLastWriteTime($f, $ilYa30)
        [IO.Directory]::SetLastWriteTime((Join-Path $Dir 'Sauvegardes'), $ilYa30)
        [IO.Directory]::SetLastWriteTime($Dir, $ilYa30)
    }
}
function Set-Fiche([string]$SteamApps, [string]$Id, [string]$Dossier) {
    [IO.File]::WriteAllText((Join-Path $SteamApps "appmanifest_$Id.acf"), "`"AppState`"`n{`n`t`"appid`"`t`t`"$Id`"`n`t`"name`"`t`t`"Jeu $Id`"`n`t`"installdir`"`t`t`"$Dossier`"`n}`n", (New-Object Text.UTF8Encoding($false)))
}

try {
    # Steam (dossier principal, sans jeux) et une seconde bibliothèque au nom accentué
    $steam = Join-Path $script:Bac 'Steam'
    New-Item -ItemType Directory -Force -Path (Join-Path $steam 'steamapps') | Out-Null
    $lib = Join-Path $script:Bac 'Jeux Société'
    $sa = Join-Path $lib 'steamapps'
    $common = Join-Path $sa 'common'
    New-Item -ItemType Directory -Force -Path $common | Out-Null
    $vdf = "`"libraryfolders`"`n{`n`t`"0`"`n`t{`n`t`t`"path`"`t`t`"$($steam.Replace('\', '\\'))`"`n`t}`n`t`"1`"`n`t{`n`t`t`"path`"`t`t`"$($lib.Replace('\', '\\'))`"`n`t}`n}`n"
    [IO.File]::WriteAllText((Join-Path $steam 'steamapps\libraryfolders.vdf'), $vdf, (New-Object Text.UTF8Encoding($false)))   # comme Steam : UTF-8 sans BOM

    # -----------------------------------------------------------------------
    # 1. libraryfolders.vdf en UTF-8
    # -----------------------------------------------------------------------
    $libs = @(Get-SteamLibraries $steam)
    Test-Ok ($libs -contains $lib) "Bibliothèque « Jeux Société » trouvée dans libraryfolders.vdf (lu : $($libs -join ' ; '))"

    # -----------------------------------------------------------------------
    # 2. Restes : dossier sans fiche, sauf s'il a bougé depuis moins de 7 jours
    # -----------------------------------------------------------------------
    Set-Fiche $sa '100' 'JeuInstalle'
    New-DossierJeu (Join-Path $common 'JeuInstalle') -Vieux
    $reste = Join-Path $common 'VieuxReste'
    New-DossierJeu $reste -Vieux
    $recent = Join-Path $common 'ResteRecent'
    New-DossierJeu $recent
    [IO.Directory]::SetLastWriteTime($common, $ilYa30)
    $s = Get-SteamGames $steam
    $restes = @($s.Restes | Where-Object { -not $_.Incomplet } | ForEach-Object { $_.Dir })
    Test-Ok (@($s.Dirs | Where-Object { $_.Dir -eq (Join-Path $common 'JeuInstalle') -and $_.AppId -eq '100' }).Count -eq 1) 'Jeu installé (cité par sa fiche) : reconnu comme jeu, avec son numéro Steam'
    Test-Ok ($restes -contains $reste) 'Dossier sans fiche, vieux de 30 jours : proposé comme reste'
    Test-Ok ($restes -notcontains $recent) 'Dossier sans fiche modifié aujourd''hui : pas proposé (Steam y travaille peut-être)'
    Test-Ok (-not @($s.Restes | Where-Object { $_.Incomplet }).Count) 'Toutes les fiches lues : analyse complète'
    $m = Measure-LeftoverDir $reste
    Test-Ok ($m.Taille -eq 1024 -and -not $m.Recent) "Taille d'un reste et ancienneté ($($m.Taille) octets, récent : $($m.Recent))"
    $l = @(Measure-LeftoverDirs @($reste, $recent))
    Test-Ok ($l -contains "$reste|1024|0" -and $l -contains "$recent|1024|1") "Calcul de fond des tailles : « dossier|taille|récent » ($($l -join ' ; '))"
    # Un vieux dossier dont un fichier au fond vient de bouger : récent
    $profond = Join-Path $common 'ResteProfond'
    New-DossierJeu $profond -Vieux
    New-Item -ItemType Directory -Force -Path (Join-Path $profond 'a\b') | Out-Null
    [IO.File]::WriteAllText((Join-Path $profond 'a\b\neuf.txt'), 'x')
    [IO.Directory]::SetLastWriteTime((Join-Path $profond 'a\b'), $ilYa30); [IO.Directory]::SetLastWriteTime((Join-Path $profond 'a'), $ilYa30); [IO.Directory]::SetLastWriteTime($profond, $ilYa30)
    Test-Ok ((Measure-LeftoverDir $profond).Recent) 'Un fichier modifié aujourd''hui au fond du dossier : le dossier compte comme récent'

    # -----------------------------------------------------------------------
    # 3. Fiche ouverte en exclusif pendant l'analyse
    # -----------------------------------------------------------------------
    Set-Fiche $sa '200' 'AutreJeu'
    $verrou = [IO.File]::Open((Join-Path $sa 'appmanifest_200.acf'), 'Open', 'ReadWrite', 'None')
    try {
        $f = Read-SteamManifests $sa
        Test-Ok (-not $f.Complet) 'Fiche verrouillée : lecture des fiches notée incomplète'
        $s = Get-SteamGames $steam
        $restes = @($s.Restes | Where-Object { -not $_.Incomplet } | ForEach-Object { $_.Dir })
        Test-Ok (-not $restes.Count) "Fiche verrouillée : aucun dossier de la bibliothèque proposé comme reste ($($restes -join ' ; '))"
        Test-Ok (@($s.Restes | Where-Object { $_.Incomplet -and $_.Leftover -and $_.Dir -eq $lib }).Count -eq 1) 'Fiche verrouillée : bibliothèque signalée « analyse incomplète » (et jamais prise pour un jeu)'

        # 4a. Suppression pendant que la fiche est verrouillée : gardé
        $st = [hashtable]::Synchronized(@{ Done = 0; Current = ''; Results = [Collections.ArrayList]::Synchronized((New-Object Collections.ArrayList)) })
        Remove-LeftoverDirs @($reste) $st
        Test-Ok ((Test-Path -LiteralPath $reste) -and [string]$st.Results[0] -match '\|gardé par prudence, une fiche de Steam') "Suppression avec une fiche illisible : dossier gardé ($($st.Results[0]))"
    } finally { $verrou.Dispose() }

    # 4b. Une fiche le cite de nouveau (jeu réinstallé depuis l'analyse) : gardé et plus proposé
    Set-Fiche $sa '300' 'VieuxReste'
    [IO.Directory]::SetLastWriteTime($reste, $ilYa30)
    $st = [hashtable]::Synchronized(@{ Done = 0; Current = ''; Results = [Collections.ArrayList]::Synchronized((New-Object Collections.ArrayList)) })
    Remove-LeftoverDirs @($reste) $st
    Test-Ok ((Test-Path -LiteralPath $reste) -and [string]$st.Results[0] -match 'Steam s''en sert de nouveau.*\|garde$') "Suppression d'un dossier qu'une fiche cite : gardé ($($st.Results[0]))"
    Remove-Item -LiteralPath (Join-Path $sa 'appmanifest_300.acf')

    # 4c. Modifié depuis moins de 7 jours : gardé
    $st = [hashtable]::Synchronized(@{ Done = 0; Current = ''; Results = [Collections.ArrayList]::Synchronized((New-Object Collections.ArrayList)) })
    Remove-LeftoverDirs @($recent, $profond) $st
    Test-Ok ((Test-Path -LiteralPath $recent) -and (Test-Path -LiteralPath $profond) -and @($st.Results | Where-Object { $_ -match 'moins de 7 jours\|garde$' }).Count -eq 2) "Suppression de dossiers modifiés il y a moins de 7 jours : gardés ($($st.Results -join ' ; '))"

    # 4d. Hors de « steamapps\common » : refusé
    $st = [hashtable]::Synchronized(@{ Done = 0; Current = ''; Results = [Collections.ArrayList]::Synchronized((New-Object Collections.ArrayList)) })
    Remove-LeftoverDirs @($sa) $st
    Test-Ok ((Test-Path -LiteralPath $sa) -and [string]$st.Results[0] -match 'pas un dossier de jeu') 'Suppression hors de « steamapps\common » : refusée'

    # 4e. Vrai reste, dans un fil séparé comme dans Nevermind : supprimé
    [IO.Directory]::SetLastWriteTime($reste, $ilYa30)
    $st = [hashtable]::Synchronized(@{ Done = 0; Current = ''; Results = [Collections.ArrayList]::Synchronized((New-Object Collections.ArrayList)) })
    $ps = [PowerShell]::Create()
    try {
        [void]$ps.AddScript("function Read-SteamManifests {${function:Read-SteamManifests}}`nfunction Measure-LeftoverDir {${function:Measure-LeftoverDir}}`nfunction Remove-LeftoverDirs {${function:Remove-LeftoverDirs}}`nRemove-LeftoverDirs `$args[0] `$args[1]").AddArgument(@($reste)).AddArgument($st)
        [void]$ps.Invoke()
        $erreurs = @($ps.Streams.Error | ForEach-Object { [string]$_ })
    } finally { $ps.Dispose() }
    Test-Ok (-not (Test-Path -LiteralPath $reste) -and [string]$st.Results[0] -eq "$reste||" -and -not $erreurs.Count) "Vrai reste supprimé dans un fil séparé ($($st.Results[0]) $($erreurs -join ' ; '))"
    Test-Ok (Test-Path -LiteralPath (Join-Path $common 'JeuInstalle\Sauvegardes\partie.sav')) 'Le jeu installé n''a pas été touché'

    # Le fil de la liste des jeux contient bien les fonctions Steam (sinon la liste des jeux serait vide)
    $tok = $null; $err = $null
    $txt = (Get-InstalledGamesWork).ToString()
    [void][System.Management.Automation.Language.Parser]::ParseInput($txt, [ref]$tok, [ref]$err)
    Test-Ok (-not $err -and $txt -match 'function Get-SteamGames' -and $txt -match 'function Read-SteamManifests' -and $txt -match 'function Get-SteamLibraries') 'Fil de la liste des jeux : lisible, avec les fonctions Steam'
    # (le même texte, lancé dans un fil neuf sur la fausse bibliothèque au lieu du vrai Steam)
    $ps = [PowerShell]::Create()
    try {
        [void]$ps.AddScript(($txt -replace 'Get-InstalledGames\s*$', 'Get-SteamGames $args[0]')).AddArgument($steam)
        $s = @($ps.Invoke())[0]
    } finally { $ps.Dispose() }
    Test-Ok ($s -and @($s.Dirs | Where-Object { $_.AppId -eq '100' }).Count -eq 1) 'Fil de la liste des jeux : les fonctions Steam marchent seules dans un fil neuf'
    # Les appels de Nevermind passent par ce fil
    foreach ($f in 'jeu', 'analyse') {
        $code = [IO.File]::ReadAllText((Join-Path $modules "$f.ps1"))
        Test-Ok ($code -match 'Invoke-Async \(Get-InstalledGamesWork\)' -and $code -notmatch 'function Get-InstalledGames \{') "$f.ps1 : la liste des jeux passe par Get-InstalledGamesWork"
    }
}
catch {
    $echecs.Add("Arrêt inattendu : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))")
}
finally {
    $null = & "$env:SystemRoot\System32\cmd.exe" /c rd /s /q "$($script:Bac)" 2>&1
}

if ($echecs.Count) {
    Write-Host "$($echecs.Count) échec(s) sur $nbTests contrôles :" -ForegroundColor Red
    $echecs | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
    exit 1
}
Write-Host "Restes de jeux Steam : les $nbTests contrôles sont bons." -ForegroundColor Green
exit 0
