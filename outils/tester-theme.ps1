# Tests du thème dans une copie gérée par Mindset et dans une copie autonome, sans fenêtre, sans WebView2 et sans rien
# toucher au PC.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-theme.ps1
#   (-Modules <dossier> : teste d'autres modules, par exemple ceux d'une version d'avant, pour une contre-épreuve)
#
# Décidé par Jordan : le thème de toute la suite se choisit seulement dans Mindset (Paramètres, Apparence). Ce script
# contrôle :
#   1. copie gérée, l'écran (console) : l'onglet Thème n'a aucun choix (pas de cartes) ni import ; il montre le thème
#      actuel et « Le thème se choisit dans Mindset, pour toutes les applis. » avec le bouton « Ouvrir les réglages de
#      Mindset », qui ouvre Mindset sur Paramètres, Apparence ;
#   2. copie gérée : l'action « theme » envoyée quand même par l'écran, ou Select-AppTheme appelé directement, sont sans
#      effet (thème enregistré inchangé, aucune question, suite.json jamais écrit) ;
#   3. copie gérée, la fenêtre d'avant (Paramètres, Thème) : mêmes règles (ni cartes, ni « Appliquer et relancer », ni
#      « Importer un pack ») ;
#   4. un pack « pack:<id> » installé par Mindset : repris au lancement, suivi quand Mindset le change, et son nom affiché ;
#   5. copie autonome : les cartes des thèmes et l'import sont toujours là, et choisir un thème marche (sans suite.json) ;
#   6. la recherche garde son entrée Thème ; aucun module n'écrit plus le thème dans suite.json.
# Le code de Nevermind n'est pas chargé en entier : seules les fonctions utiles sont lues dans les modules, et tout ce qui
# pourrait toucher au PC (Mindset, registre, relance) est remplacé par une fausse version. suite.json et les packs sont
# des fichiers d'essai dans %TEMP%, jamais les vrais.
# Code de sortie : 0 si tout passe, 1 sinon.

param([string]$Modules = '')
$ErrorActionPreference = 'Stop'
Remove-Item Env:\NEVERMIND_THEME, Env:\NEVERMIND_PACKS -ErrorAction SilentlyContinue
$racine  = Split-Path $PSScriptRoot -Parent
if (-not $Modules) { $Modules = Join-Path $racine 'Nevermind\fichiers\modules' }
$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0
function Test-Ok([bool]$Condition, [string]$Message) {
    $script:nbTests++
    if (-not $Condition) { $script:echecs.Add($Message) }
}
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase

# ---------------------------------------------------------------------------
# Dossier d'essai (suite.json et packs), jamais les vrais
# ---------------------------------------------------------------------------
$essai = Join-Path ([IO.Path]::GetTempPath()) "nevermind-essai-theme-$([guid]::NewGuid().ToString('N').Substring(0, 10))"
New-Item -ItemType Directory -Force -Path $essai | Out-Null
$env:NEVERMIND_TEST = '1'                 # (ce processus seulement : pas de relance après un changement de thème)
$SuiteFile = Join-Path $essai 'Mindset\suite.json'
$env:NEVERMIND_SUITE = $SuiteFile         # (une version d'avant n'écrivait que dans le suite.json de test : on le surveille)
New-Item -ItemType Directory -Force -Path (Split-Path $SuiteFile) | Out-Null
$suiteTexte = '{"theme":"pack:essai-suite","applis":{"nevermind":"C:\\essai\\Nevermind"}}'
[IO.File]::WriteAllText($SuiteFile, $suiteTexte, (New-Object Text.UTF8Encoding $false))
[IO.File]::SetLastWriteTimeUtc($SuiteFile, [datetime]'2026-01-01')
$suiteDate = [IO.File]::GetLastWriteTimeUtc($SuiteFile)
function Test-SuiteIntact([string]$Quand) {
    $ok = (Test-Path -LiteralPath $SuiteFile) -and [IO.File]::ReadAllText($SuiteFile) -ceq $suiteTexte -and [IO.File]::GetLastWriteTimeUtc($SuiteFile) -eq $suiteDate -and
        -not @(Get-ChildItem -LiteralPath (Split-Path $SuiteFile) -File | Where-Object { $_.Name -ne 'suite.json' }).Count   # (ni fichier à côté)
    Test-Ok $ok "suite.json écrit ($Quand)"
}
$DataDir = Join-Path $essai 'Nevermind'
$PacksDir = Join-Path $DataDir 'packs'
# Pack installé par Mindset (dans les packs de Nevermind, et dans ceux de Mindset, comme il le fait)
foreach ($d in (Join-Path $PacksDir 'essai-suite'), (Join-Path $essai 'Mindset\packs\essai-suite')) {
    New-Item -ItemType Directory -Force -Path $d | Out-Null
    [IO.File]::WriteAllText((Join-Path $d 'pack.json'), '{"Name":"Pack de la suite","Desc":"Pack d''essai","Base":"arcade"}', (New-Object Text.UTF8Encoding $false))
}

# ---------------------------------------------------------------------------
# Fausses versions (rien ne sort de ce processus)
# ---------------------------------------------------------------------------
$script:Journal = New-Object System.Collections.Generic.List[string]
$script:Messages = New-Object System.Collections.Generic.List[string]
$script:Questions = New-Object System.Collections.Generic.List[string]
$script:Erreurs = New-Object System.Collections.Generic.List[string]
$script:PagesMindset = New-Object System.Collections.Generic.List[string]
$script:Actions = @{}
$script:Reglages = @{}
function Write-Log([string]$Text) { $script:Journal.Add($Text) }
function Show-Message([string]$Text, [string]$Kind = '') { $script:Messages.Add($Text) }
function Set-Status([string]$Text) { }
function Confirm-Action([string]$Text) { $script:Questions.Add($Text); $true }
function Invoke-Safe([scriptblock]$Action) { try { & $Action } catch { $script:Erreurs.Add("$_") } }
function Open-MindsetSettings([string]$Page = 'apparence') { $script:PagesMindset.Add($Page) }
function Open-Url([string]$Url) { $script:Erreurs.Add("Open-Url appelé : $Url") }
function Get-Setting([string]$Name, $Default) { if ($script:Reglages.Contains($Name)) { $script:Reglages[$Name] } else { $Default } }
function Set-Setting([string]$Name, $Value) { $script:Reglages[$Name] = $Value }
function Register-ActionVue([string]$Id, [scriptblock]$Action) { $script:Actions[$Id] = $Action }
function Get-TuilesTestsKit { @{ Blocs = @(); Faits = 0; Total = 0 } }
function Get-OuKit($E) { '' }
foreach ($f in 'Test-FpsMeasure', 'Test-LagMeasure', 'Test-CoversOnline', 'Test-AnnuaireServeurs', 'Test-PaysServeur', 'Test-PiloteNvidia', 'Test-MajAuto', 'Test-NouveautesEnLigne', 'Build-TestsTab') {
    Set-Item -Path "function:$f" -Value { $false }
}
# Petits éléments de la fenêtre d'avant, sans ses styles
function New-Thickness($l, $t, $r, $b) { [System.Windows.Thickness]::new($l, $t, $r, $b) }
function Get-Brush([string]$Hex) { [System.Windows.Media.BrushConverter]::new().ConvertFromString($(if ($Hex.StartsWith('#')) { $Hex } else { '#000000' })) }
function New-RawBrush([string]$Hex) { [System.Windows.Media.BrushConverter]::new().ConvertFromString($Hex) }
function New-RawGradient([string[]]$Hex, [double]$X2 = 1, [double]$Y2 = 1) { New-RawBrush $Hex[0] }
function New-Text([string]$Text, [double]$Size = 13, [string]$Color = '#EEEBF7', [switch]$Bold, [switch]$Semi) { $t = New-Object System.Windows.Controls.TextBlock; $t.Text = $Text; $t }
function New-Card { New-Object System.Windows.Controls.Border }
function New-Button([string]$Text, [string]$Style = 'BtnSecondary') { $b = New-Object System.Windows.Controls.Button; $b.Content = $Text; $b }
function New-Grid([string[]]$Cols) { New-Object System.Windows.Controls.Grid }
function Add-ToGrid($Grid, $Element, [int]$Column) { [void]$Grid.Children.Add($Element) }
function New-ThemePreview([string]$Id) { New-Object System.Windows.Controls.Grid }
$Window = $null
$AppVersion = '0.0.0'
$SuiteThemes = @('neon', 'crepuscule', 'terminal', 'arcade', 'rubis')
$script:Vue = @{ Ecran = 'parametres' }
$script:Kit = @{ Resultats = @(); Recherche = '' }
$ui = @{ SetTheme = (New-Object System.Windows.Controls.StackPanel) }

# ---------------------------------------------------------------------------
# Le code de Nevermind : fonctions et quelques instructions lues dans les modules (rien d'autre ne se lance)
# ---------------------------------------------------------------------------
function Get-Ast([string]$Fichier) {
    $err = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $Modules $Fichier), [ref]$null, [ref]$err)
    if ($err) { throw "$Fichier illisible : $($err[0].Message)" }
    $ast
}
function Get-Fonctions([string]$Fichier, [string[]]$Noms) {
    $ast = Get-Ast $Fichier
    @($ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $Noms -contains $n.Name }, $true) | ForEach-Object { $_.Extent.Text }) -join "`r`n"
}
# Instructions du haut du fichier dont le texte commence par l'un de ces débuts
function Get-Instructions([string]$Fichier, [string[]]$Debuts) {
    $ast = Get-Ast $Fichier
    @($ast.EndBlock.Statements | Where-Object { $t = $_.Extent.Text; @($Debuts | Where-Object { $t.StartsWith($_) }).Count } | ForEach-Object { $_.Extent.Text }) -join "`r`n"
}
. ([scriptblock]::Create((Get-Instructions 'themes.ps1' @('$AppThemes = '))))
. ([scriptblock]::Create((Get-Fonctions 'themes.ps1' @('Test-CouleursPack', 'Get-ThemePack', 'Get-ThemePackConsole', 'Get-ThemePacks', 'Test-PackSuite', 'Write-SuiteTheme'))))
$demarrageSuite = [scriptblock]::Create((Get-Instructions 'themes.ps1' @('if ($Managed -and $Suite.Theme')))
. ([scriptblock]::Create((Get-Instructions 'parametres.ps1' @('$SettingsTabs = ', '$ManagedThemeText = '))))
. ([scriptblock]::Create((Get-Fonctions 'parametres.ps1' @('New-SettingAction', 'Get-CurrentThemeKey', 'Get-CurrentThemeName', 'Get-ThemeChoices', 'Build-ThemePanel', 'Select-AppTheme', 'Set-AppTheme'))))
. ([scriptblock]::Create((Get-Fonctions 'ecran-divers.ps1' @('Get-PagesDivers', 'Register-ActionsDivers'))))
$rechercheCode = [scriptblock]::Create((Get-Instructions 'recherche.ps1' @('$SearchEntries = ', 'if ($Managed)')))
Register-ActionsDivers
$texteGere = 'Le thème se choisit dans Mindset, pour toutes les applis.'

# Le thème chargé : comme themes.ps1 au lancement (après la reprise du thème de la suite)
function Set-ThemeCharge([string]$Reglage) {
    $script:ThemeSetting = $Reglage
    $script:ThemePack = $null
    $script:ThemeId = if ($AppThemes.Contains($Reglage)) { $Reglage } else { 'neon' }
    if ($Reglage -like 'pack:*') { $script:ThemePack = Get-ThemePack $Reglage.Substring(5); if ($script:ThemePack) { $script:ThemeId = $script:ThemePack.Base } }
}
# Tout ce que montre l'onglet Thème de l'écran, à plat : blocs, rangs, boutons, cartes
function Get-OngletTheme {
    $o = @((Get-PagesDivers).parametres.onglets | Where-Object { $_.id -eq 'theme' })[0]
    $blocs = @($o.blocs)
    $rangs = @($blocs | Where-Object { $_.type -eq 'reglages' } | ForEach-Object { @($_.rangs) })
    @{ Blocs = $blocs; Rangs = $rangs
       Textes = @(@($blocs | ForEach-Object { [string]$_.texte }) + @($rangs | ForEach-Object { [string]$_.texte; [string]$_.note; [string]$_.valeur }) | Where-Object { $_ })
       Boutons = @($rangs | Where-Object { $_.bouton } | ForEach-Object { $_.bouton }) + @($blocs | Where-Object { $_.boutons } | ForEach-Object { @($_.boutons) })
       Cartes = @($blocs | Where-Object { $_.type -eq 'cartes' -or $_.id -eq 'theme' -or $_.options }) }
}
# Tous les textes et boutons de l'onglet Thème de la fenêtre d'avant
function Get-PanneauTheme {
    $script:ThemePick = $null
    Build-ThemePanel
    $textes = New-Object System.Collections.Generic.List[string]; $boutons = @(); $cartes = 0
    $pile = New-Object System.Collections.Stack; $pile.Push($ui.SetTheme)
    while ($pile.Count) {
        $x = $pile.Pop()
        if ($x -is [System.Windows.Controls.TextBlock]) { $textes.Add($x.Text) }
        if ($x -is [System.Windows.Controls.Button]) { $boutons += $x }
        if ($x -is [System.Windows.Controls.Border] -and $x.Tag) { $cartes++ }
        foreach ($k in @([System.Windows.LogicalTreeHelper]::GetChildren($x))) { if ($k -is [System.Windows.DependencyObject]) { $pile.Push($k) } }
    }
    @{ Textes = @($textes); Boutons = $boutons; NomsBoutons = @($boutons | ForEach-Object { [string]$_.Content }); Cartes = $cartes }
}

try {
    # ===================================================================
    # 4 (d'abord). Copie gérée : le pack de la suite (suite.json) repris au lancement
    # ===================================================================
    $Managed = $true
    $Suite = @{ Theme = 'pack:essai-suite'; Dir = 'C:\essai\Nevermind' }
    $ThemeSetting = 'neon'
    . $demarrageSuite
    Test-Ok ($ThemeSetting -eq 'pack:essai-suite' -and (Get-Setting 'Theme' '') -eq 'pack:essai-suite') "pack de Mindset pas repris au lancement : $ThemeSetting"
    Set-ThemeCharge $ThemeSetting
    Test-Ok ($ThemePack -and $ThemePack.Name -eq 'Pack de la suite' -and $ThemeId -eq 'arcade') 'pack de Mindset illisible'
    Test-SuiteIntact 'lancement géré'

    # ===================================================================
    # 1. Copie gérée, l'écran : ni choix ni import, le thème actuel (nom du pack) et le chemin vers Mindset
    # ===================================================================
    $t = Get-OngletTheme
    Test-Ok ($t.Cartes.Count -eq 0) "géré, écran : $($t.Cartes.Count) bloc(s) de choix de thème encore proposé(s)"
    Test-Ok (-not @($t.Textes | Where-Object { $_ -match 'import' }).Count -and -not @($t.Boutons | Where-Object { $_.texte -match 'import|Appliquer' }).Count) "géré, écran : import ou choix proposé ($($t.Textes -join ' | '))"
    Test-Ok ($t.Textes -contains $texteGere) "géré, écran : « $texteGere » absent ($($t.Textes -join ' | '))"
    Test-Ok (@($t.Textes | Where-Object { $_ -match 'Pack de la suite' }).Count -gt 0) "géré, écran : nom du pack de la suite absent ($($t.Textes -join ' | '))"
    $bm = @($t.Boutons | Where-Object { $_.id -eq 'theme-mindset' })
    Test-Ok ($bm.Count -eq 1 -and $bm[0].texte -eq 'Ouvrir les réglages de Mindset') "géré, écran : bouton « Ouvrir les réglages de Mindset » absent ($(@($t.Boutons | ForEach-Object { $_.texte }) -join ', '))"
    Test-Ok (@($t.Boutons).Count -eq 1) "géré, écran : $(@($t.Boutons).Count) boutons dans l'onglet Thème (un seul attendu)"
    $script:PagesMindset.Clear()
    & $script:Actions['theme-mindset'] $null $null
    Test-Ok ($script:PagesMindset.Count -eq 1 -and $script:PagesMindset[0] -eq 'apparence') "géré : le bouton n'ouvre pas Mindset sur Paramètres, Apparence ($($script:PagesMindset -join ', '))"
    # Un des 5 thèmes : son nom
    Set-ThemeCharge 'rubis'
    $t = Get-OngletTheme
    Test-Ok (@($t.Textes | Where-Object { $_ -ceq 'Rubis' }).Count -gt 0 -and $t.Cartes.Count -eq 0) "géré, écran, thème Rubis : nom absent ou choix proposé ($($t.Textes -join ' | '))"

    # ===================================================================
    # 2. Copie gérée : l'action « theme » et Select-AppTheme sans effet, suite.json jamais écrit
    # ===================================================================
    Set-Setting 'Theme' 'rubis'
    $script:Questions.Clear(); $script:Messages.Clear()
    foreach ($id in 'terminal', 'neon', 'pack:essai-suite', 'pack:inconnu') {
        & $script:Actions['theme'] $id $null
        Test-Ok ((Get-Setting 'Theme' '') -eq 'rubis') "géré : l'action « theme » ($id) a changé le thème en $(Get-Setting 'Theme' '')"
        Test-SuiteIntact "géré, action « theme » $id"
        Select-AppTheme $id
        Test-Ok ((Get-Setting 'Theme' '') -eq 'rubis') "géré : Select-AppTheme $id a changé le thème en $(Get-Setting 'Theme' '')"
        Test-SuiteIntact "géré, Select-AppTheme $id"
    }
    Test-Ok ($script:Questions.Count -eq 0) "géré : l'action « theme » pose encore une question ($($script:Questions -join ' | '))"
    Test-Ok (-not @($script:Messages | Where-Object { $_ -match 'pas pu être enregistré' }).Count) "géré : message d'enregistrement du thème de la suite ($($script:Messages -join ' | '))"

    # ===================================================================
    # 3. Copie gérée, la fenêtre d'avant (Paramètres, Thème)
    # ===================================================================
    Set-ThemeCharge 'pack:essai-suite'
    $w = Get-PanneauTheme
    Test-Ok ($w.Cartes -eq 0) "géré, fenêtre : $($w.Cartes) cartes de thème proposées"
    foreach ($n in 'Appliquer et relancer', 'Importer un pack', 'Ouvrir le dossier') { Test-Ok ($w.NomsBoutons -notcontains $n) "géré, fenêtre : bouton « $n » proposé" }
    Test-Ok ($w.Textes -contains $texteGere) "géré, fenêtre : « $texteGere » absent ($($w.Textes -join ' | '))"
    Test-Ok (@($w.Textes | Where-Object { $_ -match 'Pack de la suite' }).Count -gt 0) "géré, fenêtre : nom du pack de la suite absent ($($w.Textes -join ' | '))"
    $bw = @($w.Boutons | Where-Object { [string]$_.Content -eq 'Ouvrir les réglages de Mindset' })
    Test-Ok ($bw.Count -eq 1 -and $w.Boutons.Count -eq 1) "géré, fenêtre : boutons $($w.NomsBoutons -join ', ')"
    if ($bw.Count) {
        $script:PagesMindset.Clear()
        $bw[0].RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Primitives.ButtonBase]::ClickEvent)))
        Test-Ok ($script:PagesMindset.Count -eq 1 -and $script:PagesMindset[0] -eq 'apparence') "géré, fenêtre : le bouton n'ouvre pas Mindset sur Paramètres, Apparence ($($script:PagesMindset -join ', '))"
    }
    Test-Ok (@($SettingsTabs | Where-Object { $_.Id -eq 'theme' }).Count -eq 1) 'onglet Thème absent des Paramètres'
    Test-SuiteIntact 'fenêtre gérée'

    # ===================================================================
    # 4. Copie gérée : un changement de thème fait dans Mindset est toujours suivi (pack compris), un pack absent non
    # ===================================================================
    Set-Setting 'Theme' 'neon'
    Set-AppTheme 'pack:essai-suite'
    Test-Ok ((Get-Setting 'Theme' '') -eq 'pack:essai-suite') 'géré : le pack choisi dans Mindset n''est plus suivi'
    Set-AppTheme 'pack:absent'
    Test-Ok ((Get-Setting 'Theme' '') -eq 'pack:essai-suite') 'géré : un pack absent a été pris'
    Set-AppTheme 'terminal'
    Test-Ok ((Get-Setting 'Theme' '') -eq 'terminal') 'géré : un thème choisi dans Mindset n''est plus suivi'
    Test-SuiteIntact 'thème suivi'

    # ===================================================================
    # 6. La recherche garde son entrée Thème (copie gérée et autonome)
    # ===================================================================
    foreach ($g in $true, $false) {
        $Managed = $g
        . $rechercheCode
        $e = @($SearchEntries | Where-Object { $_.Tab -eq 'theme' })
        Test-Ok ($e.Count -eq 1 -and $e[0].P -eq 'parametres' -and $e[0].T -match '^Thème') "recherche ($(if ($g) { 'géré' } else { 'autonome' })) : entrée Thème absente"
        if ($g) { Test-Ok ($e.Count -eq 1 -and $e[0].T -match 'Mindset' -and $e[0].A -eq $texteGere) "recherche (géré) : l'entrée Thème ne dit pas qu'il se choisit dans Mindset ($($e[0].T))" }
    }

    # ===================================================================
    # 5. Copie autonome : choix et import toujours là, choisir un thème marche, sans suite.json
    # ===================================================================
    $Managed = $false
    Set-ThemeCharge 'neon'; Set-Setting 'Theme' 'neon'
    $t = Get-OngletTheme
    $cartes = @($t.Blocs | Where-Object { $_.type -eq 'cartes' -and $_.id -eq 'theme' })
    Test-Ok ($cartes.Count -eq 1 -and @($cartes[0].options).Count -ge 6 -and $cartes[0].valeur -eq 'neon') "autonome, écran : choix des thèmes absent ou incomplet"
    Test-Ok (@($cartes[0].options | Where-Object { $_.id -eq 'pack:essai-suite' -and $_.titre -eq 'Pack de la suite' }).Count -eq 1) 'autonome, écran : pack installé absent des choix'
    $script:Questions.Clear()
    & $script:Actions['theme'] 'arcade' $null
    Test-Ok ($script:Questions.Count -eq 1 -and (Get-Setting 'Theme' '') -eq 'arcade') "autonome : l'action « theme » ne change plus le thème ($(Get-Setting 'Theme' ''))"
    & $script:Actions['theme'] 'pack:essai-suite' $null
    Test-Ok ((Get-Setting 'Theme' '') -eq 'pack:essai-suite') 'autonome : un pack ne se choisit plus'
    Test-SuiteIntact 'autonome'
    $w = Get-PanneauTheme
    Test-Ok ($w.Cartes -ge 6) "autonome, fenêtre : $($w.Cartes) cartes de thème"
    foreach ($n in 'Importer un pack', 'Ouvrir le dossier') { Test-Ok ($w.NomsBoutons -contains $n) "autonome, fenêtre : bouton « $n » absent" }
    Test-Ok ($w.NomsBoutons -notcontains 'Ouvrir les réglages de Mindset') 'autonome, fenêtre : renvoie à Mindset'

    # ===================================================================
    # 6. Aucun module n'écrit le thème dans suite.json (seul afficher-page.txt est écrit à côté, pour Mindset)
    # ===================================================================
    $ecritures = @(Get-ChildItem -LiteralPath $Modules -Filter *.ps1 | ForEach-Object {
        $f = $_.Name
        $n = 0
        foreach ($l in [IO.File]::ReadAllLines($_.FullName, [Text.Encoding]::UTF8)) {
            $n++
            if ($l -match '\$SuiteFile\b' -and $l -match 'WriteAll|Move-Item|Copy-Item|Set-Content|Out-File|Add-Content|Rename-Item|Replace\(' -and $l -notmatch 'Split-Path \$SuiteFile') { "$f ligne $n" }
            if ($l -match 'Write-SuiteTheme|pas pu être enregistré') { "$f ligne $n" }
        }
    })
    Test-Ok ($ecritures.Count -eq 0) "suite.json encore écrit par Nevermind : $($ecritures -join ', ')"
} catch {
    $echecs.Add("Erreur pendant le test : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))")
} finally {
    Remove-Item -LiteralPath $essai -Recurse -Force -ErrorAction SilentlyContinue
}
if ($script:Erreurs.Count) { $echecs.Add("Erreurs dans le code testé : $($script:Erreurs -join ' | ')") }

if ($echecs.Count) {
    Write-Host "$($echecs.Count) échec(s) sur $nbTests vérifications :" -ForegroundColor Red
    foreach ($e in $echecs) { Write-Host "  - $e" -ForegroundColor Red }
    exit 1
}
Write-Host "Les $nbTests vérifications du thème (copie gérée et autonome) passent." -ForegroundColor Green
exit 0
