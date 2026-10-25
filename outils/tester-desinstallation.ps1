# Tests de la désinstallation de Nevermind (Mindset issue #2 : garder ou supprimer ses données), sans droits
# administrateur et sans rien toucher sur le PC.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-desinstallation.ps1
#
# Ce script contrôle :
#   1. le choix des données : gardées ou supprimées, venu de la ligne de commande (sans question) ou de la fenêtre ;
#   2. sur un faux dossier de données, ce qui part et ce qui reste dans chaque cas (la sauvegarde des réglages de Windows
#      part toujours ; l'historique reste seulement si des réglages n'ont pas pu être remis comme avant) ;
#   3. le message de fin, qui dit ce qui a vraiment été fait ;
#   4. la fenêtre de désinstallation (hors de l'écran, sans prendre la main) : case décochée au départ, cochée, annuler ;
#   5. le lanceur (outils\lanceur.cs, compilé ici comme « Désinstaller Nevermind.exe ») : -GarderDonnees et
#      -SupprimerDonnees arrivent au script, une valeur héritée d'ailleurs est ignorée.
# Le code de Nevermind n'est pas chargé en entier : seules les fonctions utiles sont lues dans les modules, et tout ce qui
# pourrait toucher au PC (tâches planifiées, programmes, registre, réglages de Windows) est remplacé par une fausse version.
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
# Sécurité du test : rien ne doit pouvoir modifier le PC (une fonction passe avant une commande du même nom)
# ---------------------------------------------------------------------------
$script:Lances = New-Object System.Collections.Generic.List[string]
function script:Start-Process($FilePath, $ArgumentList) { $script:Lances.Add("$FilePath $(@($ArgumentList) -join ' ')") }   # (la commande qui supprimerait les fichiers : notée, jamais lancée)
function script:Get-Process { }                                  # (jamais l'Aide ni aucun autre programme)
function script:Get-ScheduledTask { }
function script:Unregister-ScheduledTask { throw 'Test : Unregister-ScheduledTask est interdit.' }
function script:Set-AutoStart { }
function script:Test-AutoStart { $false }
function script:Test-DesktopShortcut { $false }
function script:Get-DesktopShortcutPath { Join-Path $script:Bac 'Nevermind.lnk' }
function script:Invoke-Aide { throw 'Test : Invoke-Aide est interdit.' }
$script:Journal = New-Object System.Collections.Generic.List[string]
function script:Write-Log([string]$Message) { $script:Journal.Add($Message) }
$script:Reglages = @{}
function script:Get-Setting([string]$Name, $Default = $null) { if ($script:Reglages.ContainsKey($Name)) { $script:Reglages[$Name] } else { $Default } }
function script:Set-Setting([string]$Name, $Value) { $script:Reglages[$Name] = $Value }
function script:Import-History { $script:History = @() }
function script:Import-Backup { $script:Backup = $script:FausseSauvegarde; $script:BackupProbleme = $script:FauxProbleme }
$script:ErreursRestauration = @()
$script:Restaures = 0
function script:Restore-AllSettings {
    $script:Restaures++
    # Comme la vraie : la sauvegarde part quand tout est remis ; sinon elle garde ce qui reste (la désinstallation l'enlève ensuite)
    if (-not @($script:ErreursRestauration).Count) { Remove-Item -LiteralPath $BackupFile, "$BackupFile.bak" -ErrorAction SilentlyContinue }
    , @($script:ErreursRestauration)
}
$script:Messages = New-Object System.Collections.Generic.List[string]
$script:ReponseFenetre = $null
$script:FenetreOuverte = 0
function script:Show-ChoixDesinstallation([string]$Texte) { $script:FenetreOuverte++; $script:TexteFenetre = $Texte; $script:ReponseFenetre }

# ---------------------------------------------------------------------------
# Lecture des fonctions utiles dans les modules (rien ne se lance au chargement)
# ---------------------------------------------------------------------------
$asts = @{}
foreach ($f in 'systeme', 'donnees') {
    $tok = $null; $err = $null
    $asts[$f] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$f.ps1"), [ref]$tok, [ref]$err)
    if ($err) { Write-Host "$f.ps1 n'est pas lisible : $($err[0].Message)" -ForegroundColor Red; exit 1 }
}
function Get-FonctionAst([string]$Fichier, [string]$Nom) {
    $asts[$Fichier].Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $false)
}
function Import-Fonction([string]$Fichier, [string]$Nom, [string]$Sous = '') {
    $a = Get-FonctionAst $Fichier $Nom
    if (-not $a) { Write-Host "Fonction $Nom introuvable dans $Fichier.ps1" -ForegroundColor Red; exit 1 }
    $nomFinal = if ($Sous) { $Sous } else { $Nom }
    # (la définition entière, paramètres compris, dans la portée du script)
    Invoke-Expression ($a.Extent.Text -replace "^function\s+$([regex]::Escape($Nom))", "function script:$nomFinal")
}
Import-Fonction 'systeme' 'Get-AppRoot'
Import-Fonction 'systeme' 'Get-ChoixDonneesImpose'
Import-Fonction 'systeme' 'Remove-DonneesDesinstallation'
Import-Fonction 'systeme' 'Invoke-Uninstall'
Import-Fonction 'systeme' 'Get-AppExes'
Import-Fonction 'systeme' 'Get-ShortcutTarget'
Import-Fonction 'systeme' 'Get-MenuShortcutPath'
Import-Fonction 'systeme' 'Remove-MenuShortcut'
Import-Fonction 'systeme' 'Show-ChoixDesinstallation' 'Show-VraieFenetre'
Import-Fonction 'donnees' 'Get-TexteProblemeSauvegarde'
function script:Show-MessageFinDesinstallation([string]$Texte) { $script:Messages.Add($Texte) }
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase

# ---------------------------------------------------------------------------
# Faux dossiers : programme et données, sous %TEMP%
# ---------------------------------------------------------------------------
$script:Bac = Join-Path $env:TEMP "nevermind-desinst-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
New-Item -ItemType Directory -Force -Path $script:Bac | Out-Null
$script:Bac = [IO.Path]::GetFullPath($script:Bac)
$AppDir = Join-Path $script:Bac 'Nevermind\fichiers'
$OldAutoStartTasks = @('OptiGame (démarrage)')
$AideMemoire = Join-Path $script:Bac 'aide-memoire'   # (à la place de la clé HKLM de l'Aide)
$script:MenuDir = Join-Path $script:Bac 'menu'        # (à la place du menu Démarrer de l'utilisateur)
$fichiersGardes = 'parametres.json', 'ignores.json', 'jeux.json', 'fps.json', 'lag.json', 'organizer.json', 'appareils.json', 'journal.txt', 'journal-ancien.txt', 'jaquettes\jeu.png', 'packs\theme\pack.json'
$fichiersPartis = 'sauvegarde.json', 'sauvegarde.json.bak', 'sauvegarde-abimee-20260101-101010.json', 'afficher.demande', 'quitter.demande'

# Un faux Nevermind installé et ses données. Renvoie le dossier des données.
function New-FauxNevermind([string]$Nom) {
    $script:DataDir = Join-Path $script:Bac "donnees-$Nom"
    $script:BackupFile = Join-Path $script:DataDir 'sauvegarde.json'
    foreach ($f in @($fichiersGardes) + @($fichiersPartis) + 'historique.json') {
        $p = Join-Path $script:DataDir $f
        New-Item -ItemType Directory -Force -Path (Split-Path $p) | Out-Null
        [IO.File]::WriteAllText($p, '{}')
    }
    New-Item -ItemType Directory -Force -Path (Join-Path $AppDir 'modules') | Out-Null
    foreach ($f in 'Nevermind.exe', 'Désinstaller Nevermind.exe') { [IO.File]::WriteAllText((Join-Path (Split-Path $AppDir) $f), 'programme') }
    [IO.File]::WriteAllText((Join-Path $AppDir 'Nevermind.ps1'), '# script')
    $script:FausseSauvegarde = @{ Registry = @{ 'a' = @{ Path = 'HKCU:\Faux'; Name = 'x' } }; Dns = @{}; Displays = @{}; PowerScheme = $null; Overlay = $null }
    $script:FauxProbleme = $null
    $script:ErreursRestauration = @()
    $script:Restaures = 0
    $script:Messages.Clear(); $script:Lances.Clear(); $script:Journal.Clear()
    $script:FenetreOuverte = 0; $script:ReponseFenetre = $null
    $script:Reglages = @{}
    Remove-Item env:NEVERMIND_DONNEES -ErrorAction SilentlyContinue
    $script:DataDir
}
function Test-Present([string]$Dossier, [string[]]$Fichiers) { @($Fichiers | Where-Object { -not (Test-Path -LiteralPath (Join-Path $Dossier $_)) }).Count -eq 0 }
function Test-Absents([string]$Dossier, [string[]]$Fichiers) { @($Fichiers | Where-Object { Test-Path -LiteralPath (Join-Path $Dossier $_) }).Count -eq 0 }

try {
    # -----------------------------------------------------------------------
    # 1. D'où vient le choix
    # -----------------------------------------------------------------------
    Remove-Item env:NEVERMIND_DONNEES -ErrorAction SilentlyContinue
    Test-Ok ((Get-ChoixDonneesImpose) -eq '') 'Sans option, le choix n''est pas imposé (la question est posée)'
    $env:NEVERMIND_DONNEES = 'garder';    Test-Ok ((Get-ChoixDonneesImpose) -eq 'garder') 'NEVERMIND_DONNEES=garder : données gardées'
    $env:NEVERMIND_DONNEES = 'supprimer'; Test-Ok ((Get-ChoixDonneesImpose) -eq 'supprimer') 'NEVERMIND_DONNEES=supprimer : données supprimées'
    $env:NEVERMIND_DONNEES = 'n''importe quoi'; Test-Ok ((Get-ChoixDonneesImpose) -eq '') 'Une valeur inconnue n''impose rien'
    Remove-Item env:NEVERMIND_DONNEES
    & { $GarderDonnees = [switch]$true; Test-Ok ((Get-ChoixDonneesImpose) -eq 'garder') 'Paramètre -GarderDonnees de Nevermind.ps1 : données gardées' }
    & { $SupprimerDonnees = [switch]$true; Test-Ok ((Get-ChoixDonneesImpose) -eq 'supprimer') 'Paramètre -SupprimerDonnees de Nevermind.ps1 : données supprimées' }
    & { $GarderDonnees = [switch]$true; $env:NEVERMIND_DONNEES = 'supprimer'; Test-Ok ((Get-ChoixDonneesImpose) -eq '') 'Garder et supprimer à la fois : la question est posée'; Remove-Item env:NEVERMIND_DONNEES }

    # -----------------------------------------------------------------------
    # 2 et 3. Désinstallations sur un faux dossier de données
    # -----------------------------------------------------------------------
    $d = New-FauxNevermind 'garder'
    $env:NEVERMIND_DONNEES = 'garder'
    Invoke-Uninstall
    Test-Ok ($script:FenetreOuverte -eq 0) 'Choix imposé (garder) : aucune question'
    Test-Ok ($script:Restaures -eq 1) 'Garder : les réglages de Windows sont remis comme avant'
    Test-Ok (Test-Present $d $fichiersGardes) "Garder : préférences, jeux, mesures, thèmes et journal restent ($d)"
    Test-Ok (Test-Absents $d $fichiersPartis) 'Garder : la sauvegarde (et sa copie, les sauvegardes abîmées) et les demandes partent'
    Test-Ok (Test-Absents $d @('historique.json')) 'Garder, tout remis comme avant : l''historique part'
    Test-Ok ($script:Messages.Count -eq 1 -and $script:Messages[0] -match 'Tes données sont gardées' -and $script:Messages[0] -match [regex]::Escape($d)) 'Garder : le message dit où sont les données gardées'
    Test-Ok ($script:Lances.Count -eq 1 -and $script:Lances[0] -match 'Nevermind\.exe' -and $script:Lances[0] -notmatch [regex]::Escape($d)) 'Garder : seuls les fichiers du programme sont supprimés ensuite'
    Test-Ok ($script:Journal -match 'données gardées').Count 'Garder : noté dans le journal (gardé)'

    $d = New-FauxNevermind 'supprimer'
    $env:NEVERMIND_DONNEES = 'supprimer'
    Invoke-Uninstall
    Test-Ok ($script:FenetreOuverte -eq 0) 'Choix imposé (supprimer) : aucune question'
    Test-Ok ($script:Restaures -eq 1) 'Supprimer : les réglages de Windows sont remis comme avant'
    Test-Ok (-not (Test-Path -LiteralPath $d)) 'Supprimer : tout le dossier des données part'
    Test-Ok ($script:Messages.Count -eq 1 -and $script:Messages[0] -match 'Ses données sont supprimées\.' -and $script:Messages[0] -notmatch 'gardées') "Supprimer : le message le dit ($($script:Messages[0]))"

    $d = New-FauxNevermind 'erreur'
    $env:NEVERMIND_DONNEES = 'garder'
    $script:ErreursRestauration = @('HKCU:\Faux\x: accès refusé')
    Invoke-Uninstall
    Test-Ok (Test-Absents $d @('sauvegarde.json', 'sauvegarde.json.bak')) 'Réglages pas tous remis : la sauvegarde part quand même'
    Test-Ok (Test-Present $d @('historique.json')) 'Réglages pas tous remis : l''historique est gardé (il montre ce qui reste)'
    Test-Ok ($script:Messages[0] -match 'accès refusé') 'Réglages pas tous remis : le message le dit'

    $d = New-FauxNevermind 'probleme'
    $env:NEVERMIND_DONNEES = 'garder'
    $script:FauxProbleme = 'abimee'
    $script:FausseSauvegarde = @{ Registry = @{}; Dns = @{}; Displays = @{}; PowerScheme = $null; Overlay = $null }
    Invoke-Uninstall
    Test-Ok ($script:Restaures -eq 0) 'Rien à remettre : la restauration n''est pas lancée'
    Test-Ok (Test-Present $d @('historique.json')) 'Sauvegarde abîmée : l''historique est gardé'
    Test-Ok (Test-Absents $d $fichiersPartis) 'Sauvegarde abîmée : les sauvegardes partent quand même'
    Test-Ok ($script:Messages[0] -match 'abîmé') 'Choix imposé : l''avertissement de la question est dit à la fin'

    $d = New-FauxNevermind 'services'
    $env:NEVERMIND_DONNEES = 'garder'
    $script:FausseSauvegarde = @{ Registry = @{}; Dns = @{}; Displays = @{}; PowerScheme = $null; Overlay = $null }
    $script:Reglages['SvcOriginal'] = @('DiagTrack|Automatic|True')
    Invoke-Uninstall
    Test-Ok ($script:Restaures -eq 1) 'Seulement des services changés : ils sont remis comme avant aussi'

    $d = New-FauxNevermind 'definitif'
    $env:NEVERMIND_DONNEES = 'garder'
    $script:History = @(@{ Annule = $true; Log = @(@{ Type = 'definitif'; Quoi = 'Protection test'; Retour = 'page Protection' }) })
    Invoke-Uninstall
    $script:History = $null
    Test-Ok ($script:Messages[0] -match 'Protection test') 'Un changement définitif d''une entrée annulée est toujours dit (il reste en place)'
    Remove-Item env:NEVERMIND_DONNEES -ErrorAction SilentlyContinue

    $d = New-FauxNevermind 'annule'
    $script:ReponseFenetre = $null
    Invoke-Uninstall
    Test-Ok ($script:FenetreOuverte -eq 1) 'Sans option : la fenêtre pose la question'
    Test-Ok ($script:TexteFenetre -notmatch 'supprimer ses données') 'La fenêtre ne dit plus que les données sont supprimées d''office'
    Test-Ok ($script:Restaures -eq 0 -and $script:Lances.Count -eq 0 -and $script:Messages.Count -eq 0 -and (Test-Present $d (@($fichiersGardes) + @($fichiersPartis)))) 'Annuler : rien n''est touché'

    $d = New-FauxNevermind 'fenetre-garder'
    $script:ReponseFenetre = 'garder'
    $script:FauxProbleme = 'abimee'
    Invoke-Uninstall
    Test-Ok ((Test-Present $d $fichiersGardes) -and (Test-Absents $d $fichiersPartis) -and $script:TexteFenetre -match 'abîmé' -and $script:Messages[0] -match 'gardées') 'Fenêtre, case décochée : données gardées'
    Test-Ok ($script:Messages[0] -notmatch 'abîmé') 'Fenêtre : les avertissements ne sont pas répétés à la fin'

    $d = New-FauxNevermind 'fenetre-supprimer'
    $script:ReponseFenetre = 'supprimer'
    Invoke-Uninstall
    Test-Ok (-not (Test-Path -LiteralPath $d) -and $script:Messages[0] -match 'supprimées') 'Fenêtre, case cochée : données supprimées'

    # Un fichier ouvert empêche de tout supprimer : le message le dit (pas « supprimées » tout court)
    $d = New-FauxNevermind 'verrou'
    $env:NEVERMIND_DONNEES = 'supprimer'
    $verrou = [IO.File]::Open((Join-Path $d 'journal.txt'), 'Open', 'Read', 'None')
    try { Invoke-Uninstall } finally { $verrou.Dispose() }
    Test-Ok ($script:Messages[0] -match 'sauf ce qui est indiqué' -and $script:Messages[0] -match 'Données') 'Supprimer, un fichier ouvert : le message dit ce qui reste'
    Remove-Item env:NEVERMIND_DONNEES -ErrorAction SilentlyContinue

    # Le raccourci du menu Démarrer écrit par Mindset (Mindset\Nevermind.lnk, le nom de Nevermind dans la barre des tâches)
    Test-Ok ((Get-MenuShortcutPath) -eq (Join-Path $script:Bac 'menu\Mindset\Nevermind.lnk')) 'Raccourci du menu Démarrer : cherché dans le faux menu du test'
    $sh = New-Object -ComObject WScript.Shell
    $lienMenu = Get-MenuShortcutPath
    $d = New-FauxNevermind 'menu'
    $env:NEVERMIND_DONNEES = 'garder'
    New-Item -ItemType Directory -Force -Path (Split-Path $lienMenu) | Out-Null
    [IO.File]::WriteAllText((Join-Path (Split-Path $AppDir) 'Mindset.exe'), 'programme')
    $s = $sh.CreateShortcut($lienMenu); $s.TargetPath = Join-Path (Split-Path $AppDir) 'Mindset.exe'; $s.Save()
    Invoke-Uninstall
    Test-Ok (-not (Test-Path -LiteralPath $lienMenu) -and -not (Test-Path -LiteralPath (Split-Path $lienMenu))) 'Le raccourci du menu Démarrer vers ce Nevermind est retiré, avec le dossier « Mindset » devenu vide'
    $d = New-FauxNevermind 'menu-autre'
    New-Item -ItemType Directory -Force -Path (Split-Path $lienMenu) | Out-Null
    $s = $sh.CreateShortcut($lienMenu); $s.TargetPath = Join-Path $script:Bac 'autre\Mindset.exe'; $s.Save()
    $autre = Join-Path (Split-Path $lienMenu) 'Evermind.lnk'
    $s = $sh.CreateShortcut($autre); $s.TargetPath = Join-Path $script:Bac 'autre\Mindset.exe'; $s.Save()
    Invoke-Uninstall
    Test-Ok ((Test-Path -LiteralPath $lienMenu) -and (Test-Path -LiteralPath $autre)) 'Un raccourci vers une autre copie, ou d''une autre appli, n''est pas touché'
    Remove-Item env:NEVERMIND_DONNEES -ErrorAction SilentlyContinue

    # -----------------------------------------------------------------------
    # 4. La vraie fenêtre, hors de l'écran
    # -----------------------------------------------------------------------
    $image = Join-Path $script:Bac 'fenetre.png'
    Test-Ok ((Show-VraieFenetre "Nevermind va :`n  - supprimer les fichiers de Nevermind de ce dossier" 0 $image) -eq 'garder') 'Fenêtre : case laissée telle quelle (décochée au départ) = garder'
    Test-Ok ((Test-Path -LiteralPath $image) -and (Get-Item -LiteralPath $image).Length -gt 1000) 'Fenêtre : elle s''affiche (image enregistrée)'
    if ($env:NEVERMIND_TEST_IMAGE) { Copy-Item -LiteralPath $image -Destination $env:NEVERMIND_TEST_IMAGE -Force }
    Test-Ok ((Show-VraieFenetre 'Texte' 1) -eq 'supprimer') 'Fenêtre : case cochée = supprimer'
    Test-Ok ($null -eq (Show-VraieFenetre 'Texte' 2)) 'Fenêtre : Annuler = rien'

    # -----------------------------------------------------------------------
    # 5. Le lanceur transmet l'option
    # -----------------------------------------------------------------------
    $csc = Join-Path $env:windir 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
    $sma = @(Get-ChildItem (Join-Path $env:windir 'Microsoft.NET\assembly\GAC_MSIL\System.Management.Automation') -Recurse -Filter 'System.Management.Automation.dll' | Select-Object -First 1)[0].FullName
    $lanceur = Join-Path $script:Bac 'lanceur'
    New-Item -ItemType Directory -Force -Path (Join-Path $lanceur 'fichiers') | Out-Null
    $exe = Join-Path $lanceur 'Désinstaller Nevermind.exe'
    $null = & $csc /nologo /target:winexe /reference:System.Windows.Forms.dll /reference:System.Web.Extensions.dll "/reference:$sma" /define:UNINSTALL "/out:$exe" (Join-Path $PSScriptRoot 'lanceur.cs')
    Test-Ok ($LASTEXITCODE -eq 0) 'Le lanceur se compile (Désinstaller Nevermind.exe)'
    # Un faux Nevermind.ps1 : il note ce qu'il reçoit, rien d'autre
    $sortie = Join-Path $script:Bac 'recu.txt'
    $faux = "param([switch]`$Uninstall, [switch]`$Demarrage)`r`n[IO.File]::WriteAllText('$sortie', `"`$Uninstall|`$env:NEVERMIND_DONNEES`")"
    [IO.File]::WriteAllText((Join-Path $lanceur 'fichiers\Nevermind.ps1'), $faux, (New-Object Text.UTF8Encoding $true))
    foreach ($c in @(@('-GarderDonnees', 'True|garder'), @('-SupprimerDonnees', 'True|supprimer'), @('', 'True|'), @('-GarderDonnees -SupprimerDonnees', 'True|'))) {
        Remove-Item -LiteralPath $sortie -ErrorAction SilentlyContinue
        $env:NEVERMIND_DONNEES = 'supprimer'   # (valeur héritée : le lanceur l'ignore, seule sa ligne de commande compte)
        $p = if ($c[0]) { & (Get-Command Start-Process -CommandType Cmdlet) -FilePath $exe -ArgumentList $c[0] -PassThru } else { & (Get-Command Start-Process -CommandType Cmdlet) -FilePath $exe -PassThru }
        [void]$p.WaitForExit(30000)
        Remove-Item env:NEVERMIND_DONNEES -ErrorAction SilentlyContinue
        $recu = if (Test-Path -LiteralPath $sortie) { [IO.File]::ReadAllText($sortie) } else { '(rien)' }
        Test-Ok ($recu -eq $c[1]) "Lanceur « $($c[0]) » : le script reçoit « $($c[1]) » (reçu : $recu)"
    }
}
catch {
    $echecs.Add("Arrêt inattendu : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))")
}
finally {
    Remove-Item env:NEVERMIND_DONNEES -ErrorAction SilentlyContinue
    $null = & "$env:SystemRoot\System32\cmd.exe" /c rd /s /q "$($script:Bac)" 2>&1
}

if ($echecs.Count) {
    Write-Host "$($echecs.Count) échec(s) sur $nbTests contrôles :" -ForegroundColor Red
    $echecs | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
    exit 1
}
Write-Host "Désinstallation : les $nbTests contrôles sont bons." -ForegroundColor Green
exit 0
