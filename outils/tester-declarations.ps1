# Tests des déclarations des corrections (issue #7), sans droits administrateur et sans rien modifier sur le PC.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-declarations.ps1
#
# Chaque correction décrit à l'avance ce qu'elle change (voir Set-Declarations dans tableau-de-bord.ps1) : la fiche et la
# confirmation de « Tout optimiser » montrent cette liste avec les valeurs actuelles. Ce script contrôle :
#   1. que chaque correction qui agit (New-Fix avec -Run ou -Auto dans analyse.ps1), chaque réglage gaming ($Tweaks) et
#      chaque point de « Tout optimiser » d'un jeu a sa déclaration ;
#   2. que les déclarations ne font que lire (seulement des commandes Get-..., aucune écriture) ;
#   3. que le code des corrections n'écrit pas le registre en direct (Set-Reg, Clear-Reg) : il passe par sa déclaration ;
#   4. sur un faux registre en mémoire, que ce que chaque correction écrit vraiment (son journal, RunLog) est exactement
#      ce qu'elle déclare ;
#   5. que les détails montrés avant confirmation (Get-DetailsAvant) ont une ligne par changement, maintenant et après ;
#   6. que l'historique (Get-DetailsTechniques, page Sauvegarde) montre l'avant et l'après de chaque type de changement :
#      les vraies fonctions qui changent le PC sont lancées sur un faux PC, leur journal passe par historique.json (dans
#      %TEMP%), et les anciennes entrées sans « après » restent lisibles.
# Le code de Nevermind n'est pas chargé en entier : seules les fonctions utiles sont lues dans les modules (rien ne se
# lance au chargement), et tout ce qui pourrait toucher au PC est remplacé par une fausse version.
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
# Sécurité du test : rien ne doit pouvoir modifier le PC. Ces fonctions remplacent les commandes qui agissent (une
# fonction passe avant une commande ou un programme du même nom) ; celles de Nevermind sont remplacées plus bas.
# ---------------------------------------------------------------------------
foreach ($c in 'Set-ItemProperty', 'New-ItemProperty', 'Remove-ItemProperty', 'New-Item', 'Remove-Item', 'Set-Service', 'Start-Process',
               'Stop-Process', 'shutdown', 'shutdown.exe', 'reg', 'reg.exe', 'Invoke-Aide', 'Open-RegKey', 'Save-Backup', 'Set-Setting', 'Invoke-CleanAll', 'Invoke-CleanTargets') {
    Set-Item -Path "function:script:$c" -Value ([scriptblock]::Create("throw 'Test : $c est interdit pendant le test des déclarations.'"))
}
# powercfg : seulement la liste des plans (lecture), avec un faux résultat
function script:powercfg { if ("$args" -match '^/list') { return 'GUID du mode de gestion de l''alimentation : 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c  (Performances élevées)' }; throw "Test : powercfg $args est interdit." }

# ---------------------------------------------------------------------------
# 1. Lecture des modules : les fonctions et variables utiles, rien d'autre
# ---------------------------------------------------------------------------
$asts = @{}
foreach ($f in 'tableau-de-bord', 'analyse', 'optimisations', 'donnees', 'assistance', 'jeu', 'bibliotheque', 'memoire') {
    $tok = $null; $err = $null
    $asts[$f] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$f.ps1"), [ref]$tok, [ref]$err)
    if ($err) { Write-Host "$f.ps1 n'est pas lisible : $($err[0].Message)" -ForegroundColor Red; exit 1 }
}
function Get-FonctionAst([string]$Fichier, [string]$Nom) {
    $asts[$Fichier].Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $false)
}
$aCharger = [ordered]@{
    'tableau-de-bord' = 'New-Fix', 'Resolve-Declarations', 'Set-Declarations', 'Format-ValeurBat', 'Format-Declaration', 'Get-DetailsAvant', 'Get-ChangementsNettoyage'
    'donnees'         = 'Set-Reg', 'Clear-Reg'
    'assistance'      = 'Format-ValeurTech'
    'optimisations'   = 'Get-ChangementAlimentation', 'Get-ChangementsAccessHotkeys', 'Get-ChangementWindowedOptim', 'Get-TweakChanges', 'Invoke-TweakApply',
                        'Get-AvailableTweaks', 'Test-Tweak', 'Test-AccessHotkeys', 'Get-ChangementDemarrage', 'Set-StartupState'
    'jeu'             = 'Get-ChangementsProfil', 'Set-GameProfile', 'Get-GameExeNames'
    'bibliotheque'    = 'Get-ChangementsOptimisationJeu'
}
foreach ($f in $aCharger.Keys) {
    foreach ($n in $aCharger[$f]) {
        $def = Get-FonctionAst $f $n
        if (-not $def) { $echecs.Add("Fonction $n introuvable dans $f.ps1"); continue }
        . ([scriptblock]::Create($def.Extent.Text))
    }
}
# Variables du haut des modules (dans l'ordre des fichiers : $Tweaks utilise $MousePath et $DoPath)
$variables = @{
    'optimisations'   = 'GuidPattern', 'HighPerfGuid', 'BalancedGuid', 'MousePath', 'AccessPaths', 'DxPath', 'DoPath', 'Tweaks', 'ApprovedRoot'
    'jeu'             = 'IfeoPath', 'GenericExe'
    'analyse'         = 'SubBattery', 'BatSettings', 'BatActions'
    'tableau-de-bord' = 'BatNoms', 'BiosChanges'
}
foreach ($f in $variables.Keys) {
    foreach ($aff in @($asts[$f].EndBlock.Statements | Where-Object { $_ -is [System.Management.Automation.Language.AssignmentStatementAst] })) {
        $v = $aff.Left; if ($v -is [System.Management.Automation.Language.ConvertExpressionAst]) { $v = $v.Child }
        if ($v -is [System.Management.Automation.Language.VariableExpressionAst] -and $variables[$f] -contains $v.VariablePath.UserPath) { . ([scriptblock]::Create($aff.Extent.Text)) }
    }
}
foreach ($n in 'Tweaks', 'DxPath', 'IfeoPath', 'BatSettings', 'BiosChanges', 'MousePath') { if ($null -eq (Get-Variable $n -ValueOnly -ErrorAction SilentlyContinue)) { $echecs.Add("Variable `$$n introuvable dans les modules") } }
if ($echecs.Count) { foreach ($e in $echecs) { Write-Host "ÉCHEC  $e" -ForegroundColor Red }; exit 1 }

# ---------------------------------------------------------------------------
# 2. Le faux PC : un registre en mémoire, et les gestes de Nevermind qui agissent notés au lieu d'être faits
# ---------------------------------------------------------------------------
$script:FauxReg = @{}
$script:Gestes = New-Object System.Collections.ArrayList
$script:Build = 26100; $script:IsLaptop = $false; $script:Backup = @{ Registry = @{} }
function Set-FauxReg([string]$Path, [string]$Name, $Value, [string]$Kind = 'DWord') { $script:FauxReg["$Path|$Name"] = @{ Value = $Value; Kind = $Kind } }
function Get-RegState([string]$Path, [string]$Name) {
    $e = $script:FauxReg["$Path|$Name"]
    if ($e) { return @{ Existed = $true; Value = $e.Value; Kind = $e.Kind } }
    @{ Existed = $false; Value = $null; Kind = $null }
}
function Get-RegValue([string]$Path, [string]$Name) { $e = $script:FauxReg["$Path|$Name"]; if ($e) { return , $e.Value }; $null }
function Save-Original([string]$Path, [string]$Name) { }
function Write-RegValue([string]$Path, [string]$Name, $Value, [string]$Kind) { Set-FauxReg $Path $Name $Value $Kind; [void]$script:Gestes.Add("reg:$Path|$Name") }
function Remove-RegValue([string]$Path, [string]$Name) { $script:FauxReg.Remove("$Path|$Name"); [void]$script:Gestes.Add("efface:$Path|$Name") }
function Get-GpuPreference([string]$Exe) { [string](Get-RegValue $DxPath $Exe) }
function Set-BatterySetting([string]$Key, [int]$Value) { [void]$script:Gestes.Add("pcfg:$Key=$Value") }
function Set-DisplayRate([string]$Device, [int]$Hz) { [void]$script:Gestes.Add("display:$Device=$Hz") }
function Set-PowerPlan { [void]$script:Gestes.Add('power') }
function Sync-Mouse { [void]$script:Gestes.Add('souris') }
function Clear-StartupItemsCache { }
function Add-History { }
function Update-BackupSummary { }
function Set-Status { }
function Get-ActiveScheme { @{ Guid = '381b4222-f694-41f0-9685-ff5bb260df2e'; Name = 'Utilisation normale' } }
function Get-BatteryPowerSettings { @{ CritAction = @(0, 0); CritLevel = @(5, 5); LowLevel = @(10, 10); LowNotify = @(0, 0); LowAction = @(0, 0) } }
function Get-FrequenceEcran([string]$Device) { 60 }
function Get-CleanAllTargets { @(@{ Titre = 'Fichiers temporaires'; Paths = @('C:\Faux\Temp') }) }

# Le faux registre de départ : les réglages ne sont pas encore faits
function Reset-FauxPc {
    $script:FauxReg = @{}; $script:Gestes.Clear()
    Set-FauxReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\GameDVR' 'HistoricalCaptureEnabled' 1
    Set-FauxReg 'HKCU:\Software\Microsoft\GameBar' 'AutoGameModeEnabled' 0
    Set-FauxReg 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' 'HwSchMode' 1
    Set-FauxReg $DxPath 'DirectXUserGlobalSettings' 'VRROptimizeEnable=0;SwapEffectUpgradeEnable=0;' 'String'
    foreach ($n in 'MouseSpeed', 'MouseThreshold1', 'MouseThreshold2') { Set-FauxReg $MousePath $n '1' 'String' }
    foreach ($p in $AccessPaths) { Set-FauxReg $p 'Flags' '510' 'String' }
    Set-FauxReg $DoPath 'DownloadMode' 3
    Set-FauxReg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'EnableTransparency' 1
}
function Format-V($V) { if ($V -is [array]) { (@($V) | ForEach-Object { [string]$_ }) -join ',' } else { [string]$V } }

# Ce qui a été écrit (journal de la correction) comparé à ce qui était déclaré : mêmes valeurs, dans le même ordre
function Compare-Ecrit([string]$Nom, $Declare, $Journal) {
    $d = @($Declare | Where-Object { $_.Type -eq 'reg' })
    $j = @($Journal | Where-Object { $_.Type -eq 'reg' })
    Test-Ok ($d.Count -eq $j.Count) "$Nom : $($d.Count) valeur(s) de registre déclarée(s), $($j.Count) écrite(s)"
    for ($i = 0; $i -lt [math]::Min($d.Count, $j.Count); $i++) {
        $a = $d[$i]; $b = $j[$i]
        $ok = $a.Path -eq $b.Path -and $a.Name -eq $b.Name -and [bool]$a.Efface -eq [bool]$b.Efface
        if ($ok -and -not $a.Efface) { $ok = (Format-V $a.Value) -eq (Format-V $b.Nouveau) -and $(if ($a.Kind) { $a.Kind } else { 'DWord' }) -eq $b.NouveauType }
        Test-Ok $ok "$Nom : déclaré $($a.Path) « $($a.Name) » = $(if ($a.Efface) { 'effacé' } else { Format-V $a.Value }), écrit $($b.Path) « $($b.Name) » = $(if ($b.Efface) { 'effacé' } else { Format-V $b.Nouveau })"
    }
}

# Les commandes appelées dans un morceau de code
function Get-Commandes($Ast) {
    @($Ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.CommandAst] }, $true) | ForEach-Object { $_.GetCommandName() } | Where-Object { $_ })
}
$Ecritures = 'Set-Reg', 'Clear-Reg', 'Write-RegValue', 'Remove-RegValue', 'Set-ItemProperty', 'New-ItemProperty', 'Remove-ItemProperty'
# Une déclaration ne fait que lire : des Get-..., le tri et le filtrage, et la liste des plans d'alimentation
function Test-LectureSeule([string]$Nom, $Ast) {
    $autres = @(Get-Commandes $Ast | Where-Object { $_ -notlike 'Get-*' -and $_ -notin 'Where-Object', 'ForEach-Object', 'Select-Object', 'Out-String', 'powercfg' } | Select-Object -Unique)
    Test-Ok (-not $autres.Count) "$Nom : la déclaration doit seulement lire, elle appelle $($autres -join ', ')"
}

# ---------------------------------------------------------------------------
# 3. Les corrections du tableau de bord (analyse.ps1) : chacune déclare, et son code n'écrit pas le registre en direct
# ---------------------------------------------------------------------------
$nbFix = 0
foreach ($cmd in @($asts['analyse'].FindAll({ param($n) $n -is [System.Management.Automation.Language.CommandAst] -and $n.GetCommandName() -eq 'New-Fix' }, $true))) {
    $params = @{}
    $els = @($cmd.CommandElements)
    for ($i = 1; $i -lt $els.Count; $i++) {
        if ($els[$i] -is [System.Management.Automation.Language.CommandParameterAst]) {
            $val = if ($els[$i].Argument) { $els[$i].Argument } elseif ($i + 1 -lt $els.Count -and $els[$i + 1] -isnot [System.Management.Automation.Language.CommandParameterAst]) { $els[$i + 1] } else { $null }
            $params[$els[$i].ParameterName] = $val
        }
    }
    if (-not ($params.ContainsKey('Run') -or $params.ContainsKey('Auto') -or $params.ContainsKey('Changes'))) { continue }   # (conseils seulement : rien à déclarer)
    $nbFix++
    $ligne = "analyse.ps1 ligne $($cmd.Extent.StartLineNumber)"
    Test-Ok ($params.ContainsKey('Changes') -and $params.Changes) "$ligne : correction sans déclaration (-Changes)"
    if ($params.Changes) { Test-LectureSeule $ligne $params.Changes }
    if ($params.Run) {
        $direct = @(Get-Commandes $params.Run | Where-Object { $Ecritures -contains $_ })
        Test-Ok (-not $direct.Count) "$ligne : le code de la correction écrit le registre en direct ($($direct -join ', ')) au lieu de passer par sa déclaration"
    }
}
Test-Ok ($nbFix -ge 8) "Seulement $nbFix corrections trouvées dans analyse.ps1 (au moins 8 attendues)"
# $BiosRun, partagé par deux corrections : décrit, et sans écriture
Test-Ok (@($BiosChanges | Where-Object { $_.Type -eq 'texte' -and $_.Texte }).Count -eq 1) '$BiosChanges : la description du redémarrage dans le BIOS manque'

# Les fonctions qui font les changements des corrections : jamais d'écriture du registre en direct
foreach ($x in @(@('optimisations', 'Set-StartupState'), @('optimisations', 'Invoke-TweakApply'), @('jeu', 'Set-GameProfile'))) {
    $direct = @(Get-Commandes (Get-FonctionAst $x[0] $x[1]) | Where-Object { $Ecritures -contains $_ })
    Test-Ok (-not $direct.Count) "$($x[1]) écrit le registre en direct ($($direct -join ', ')) au lieu de passer par sa déclaration"
}
# Les fonctions qui déclarent : lecture seule
foreach ($x in @(@('optimisations', 'Get-ChangementAlimentation'), @('optimisations', 'Get-ChangementsAccessHotkeys'), @('optimisations', 'Get-ChangementWindowedOptim'),
                 @('optimisations', 'Get-ChangementDemarrage'), @('jeu', 'Get-ChangementsProfil'), @('bibliotheque', 'Get-ChangementsOptimisationJeu'), @('tableau-de-bord', 'Get-ChangementsNettoyage'),
                 @('memoire', 'Get-ChangementsMemoire'))) {
    Test-LectureSeule $x[1] (Get-FonctionAst $x[0] $x[1]).Body
}

# ---------------------------------------------------------------------------
# 4. Les réglages gaming : chacun déclare, et ce qu'il écrit sur le faux registre est ce qu'il déclare
# ---------------------------------------------------------------------------
$tweaksAst = $asts['optimisations'].Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and "$($n.Left)" -eq '$Tweaks' }, $false)
foreach ($ht in @($tweaksAst.FindAll({ param($n) $n -is [System.Management.Automation.Language.HashtableAst] -and @($n.KeyValuePairs | Where-Object { "$($_.Item1)" -eq 'Titre' }).Count }, $true))) {
    $cles = @{}; foreach ($kv in $ht.KeyValuePairs) { $cles["$($kv.Item1)"] = $kv.Item2 }
    $id = "$($cles['Id'])".Trim("'")
    Test-Ok ($cles.ContainsKey('Changes')) "Réglage $id : pas de déclaration (Changes)"
    Test-Ok (-not $cles.ContainsKey('Apply')) "Réglage $id : « Apply » n'existe plus, ses changements vont dans Changes (et le reste dans Ensuite)"
    if ($cles['Changes']) { Test-LectureSeule "Réglage $id" $cles['Changes'] }
    if ($cles['Ensuite']) {
        $autres = @(Get-Commandes $cles['Ensuite'] | Where-Object { $_ -notin 'Set-PowerPlan', 'Sync-Mouse' })
        Test-Ok (-not $autres.Count) "Réglage $id : Ensuite fait autre chose que le plan d'alimentation ou la souris ($($autres -join ', ')) : à déclarer dans Changes"
    }
}
Test-Ok (@($Tweaks).Count -ge 9) "Seulement $(@($Tweaks).Count) réglages dans `$Tweaks"
foreach ($t in @($Tweaks)) {
    Reset-FauxPc
    $declare = @(Get-TweakChanges $t)
    Test-Ok ($declare.Count -ge 1) "Réglage $($t.Id) : déclaration vide"
    $inconnus = @($declare | Where-Object { $_.Type -notin 'reg', 'power', 'texte', 'pcfg', 'display', 'partiel' })
    Test-Ok (-not $inconnus.Count) "Réglage $($t.Id) : type de changement inconnu ($(@($inconnus | ForEach-Object { $_.Type }) -join ', '))"
    $lignes = @(Get-DetailsAvant $t.Changes)
    Test-Ok ($lignes.Count -eq $declare.Count) "Réglage $($t.Id) : $($lignes.Count) ligne(s) de détails pour $($declare.Count) changement(s)"
    foreach ($l in @($lignes | Where-Object { $_ -like 'Registre *' })) { Test-Ok ($l -match ', maintenant .+, après .+') "Réglage $($t.Id) : ligne sans valeur actuelle ou cible : $l" }
    $script:RunLog = New-Object System.Collections.ArrayList
    try { Invoke-TweakApply $t } catch { Test-Ok $false "Réglage $($t.Id) : erreur sur le faux registre : $($_.Exception.Message)" }
    $journal = @($script:RunLog); $script:RunLog = $null
    Compare-Ecrit "Réglage $($t.Id)" $declare $journal
    $power = @($declare | Where-Object { $_.Type -eq 'power' }).Count
    Test-Ok ($power -eq @($script:Gestes | Where-Object { $_ -eq 'power' }).Count) "Réglage $($t.Id) : plan d'alimentation déclaré $power fois, changé $(@($script:Gestes | Where-Object { $_ -eq 'power' }).Count) fois"
    # Appliqué, le réglage est fait (sauf le plan d'alimentation : sa vérification lit le vrai PC)
    if (-not $power) { Test-Ok (Test-Tweak $t) "Réglage $($t.Id) : toujours pas fait après ses changements déclarés (Test ne passe pas)" }
}

# ---------------------------------------------------------------------------
# 5. Profils de jeu, programmes au démarrage, « Tout optimiser » d'un jeu, et les autres types de changements
# ---------------------------------------------------------------------------
$jeu = @{ Name = 'Jeu d''essai'; Exes = @('C:\Faux\JeuEssai\JeuEssaiPrincipal.exe', 'C:\Faux\JeuEssai\JeuEssaiOutil.exe') }
foreach ($quoi in 'priority', 'gpu') {
    foreach ($on in $true, $false) {
        Reset-FauxPc
        if (-not $on) { foreach ($d in @(Get-ChangementsProfil $jeu $quoi $true)) { Set-FauxReg $d.Path $d.Name $d.Value $(if ($d.Kind) { $d.Kind } else { 'DWord' }) } }
        $declare = @(Get-ChangementsProfil $jeu $quoi $on)
        Test-Ok ($declare.Count -eq 2) "Profil $quoi $on : $($declare.Count) changement(s) déclaré(s) pour 2 exécutables"
        # Set-GameProfile garde son journal pour l'historique : on le récupère par Add-History
        $script:JournalProfil = $null
        function Add-History($T, $D, $Log) { $script:JournalProfil = @($Log) }
        Set-GameProfile $jeu $quoi $on
        Compare-Ecrit "Profil $quoi $(if ($on) { 'activé' } else { 'retiré' })" $declare $script:JournalProfil
    }
}
function Add-History { }

Reset-FauxPc
$prog = [pscustomobject]@{ Nom = 'Programme d''essai'; Approved = "HKCU:\$ApprovedRoot\Run"; ValueName = 'ProgrammeEssai' }
$script:RunLog = New-Object System.Collections.ArrayList
Set-StartupState $prog $false
Compare-Ecrit 'Programme au démarrage' @(Get-ChangementDemarrage $prog $false) @($script:RunLog)
$script:RunLog = $null
Test-Ok ((Format-V (Get-RegValue "HKCU:\$ApprovedRoot\Run" 'ProgrammeEssai')) -eq '3,0,0,0,0,0,0,0,0,0,0,0') 'Programme au démarrage : valeur StartupApproved inattendue'

# « Tout optimiser » : chaque point qui agit a sa déclaration (lu dans Get-GameOptimizations)
$optims = Get-FonctionAst 'bibliotheque' 'Get-GameOptimizations'
$branches = @((Get-FonctionAst 'bibliotheque' 'Get-ChangementsOptimisationJeu').Body.Find({ param($n) $n -is [System.Management.Automation.Language.SwitchStatementAst] }, $true).Clauses | ForEach-Object { "$($_.Item1.Value)" })
$points = 0
foreach ($ht in @($optims.FindAll({ param($n) $n -is [System.Management.Automation.Language.HashtableAst] }, $true))) {
    $cles = @($ht.KeyValuePairs | ForEach-Object { "$($_.Item1)" })
    if ($cles -notcontains 'Id' -or $cles -contains 'Optional' -or -not ($cles -contains 'Switch' -or $cles -contains 'Fix')) { continue }
    $id = "$(@($ht.KeyValuePairs | Where-Object { "$($_.Item1)" -eq 'Id' })[0].Item2)".Trim("'")
    $points++
    Test-Ok ($branches -contains $id) "« Tout optimiser » : le point « $id » n'a pas de déclaration dans Get-ChangementsOptimisationJeu"
}
Test-Ok ($points -ge 4) "« Tout optimiser » : seulement $points points trouvés dans Get-GameOptimizations"
Reset-FauxPc
foreach ($id in $branches) {
    $it = @{ Id = $id; Ids = @(@($Tweaks) | ForEach-Object { $_.Id }) }
    Test-Ok (@(Get-ChangementsOptimisationJeu $jeu $it).Count -ge 1) "« Tout optimiser » : déclaration vide pour « $id »"
}

# Les autres types : onduleur, écran, texte, effacement, et ce que la fiche en montre
Reset-FauxPc
$script:RunLog = New-Object System.Collections.ArrayList
$liste = @(
    @{ Type = 'pcfg'; Key = 'CritAction'; Value = 3 }
    @{ Type = 'display'; Device = '\\.\DISPLAY1'; Hz = 144 }
    @{ Type = 'reg'; Path = 'HKCU:\Software\Microsoft\GameBar'; Name = 'AutoGameModeEnabled'; Efface = $true }
    @{ Type = 'texte'; Texte = 'Lance un outil de Windows.' }
    @{ Type = 'partiel' }
)
$lignes = @(Get-DetailsAvant $liste)
Test-Ok ($lignes.Count -eq 5) "Détails : $($lignes.Count) lignes pour 5 changements"
Test-Ok ($lignes[0] -match 'maintenant secteur ne rien faire, batterie ne rien faire, après arrêt du PC') "Détails onduleur : $($lignes[0])"
Test-Ok ($lignes[1] -match 'maintenant 60 Hz, après 144 Hz') "Détails écran : $($lignes[1])"
Test-Ok ($lignes[2] -match 'maintenant 0, après effacée') "Détails effacement : $($lignes[2])"
Test-Ok ($lignes[3] -eq 'Lance un outil de Windows.') "Détails texte : $($lignes[3])"
Test-Ok ($lignes[4] -match 'historique') "Détails partiels : $($lignes[4])"
Test-Ok (-not @($lignes | Where-Object { $_ -match '[\u2013\u2014]' }).Count) 'Détails : tiret long dans le texte'
Set-Declarations $liste
Test-Ok ((@($script:Gestes) -join ' ') -eq 'pcfg:CritAction=3 display:\\.\DISPLAY1=144 efface:HKCU:\Software\Microsoft\GameBar|AutoGameModeEnabled') "Set-Declarations : gestes faits « $(@($script:Gestes) -join ' ') »"
$script:RunLog = $null
# Une correction sans -Run applique sa déclaration, et rien d'autre
$fix = New-Fix -Auto -What 'Essai' -Changes @(@{ Type = 'reg'; Path = 'HKCU:\Faux'; Name = 'Essai'; Value = 1 })
Reset-FauxPc
$script:RunLog = New-Object System.Collections.ArrayList
& $fix.Run $fix.Args
Compare-Ecrit 'New-Fix sans -Run' $fix.Changes @($script:RunLog)
$script:RunLog = $null

# ---------------------------------------------------------------------------
# 6. L'historique : avant et après pour chaque type de changement noté (issue #7)
# ---------------------------------------------------------------------------
# Les vraies fonctions qui changent le PC et notent le changement, à la place des fausses utilisées plus haut
foreach ($f in 'microsoft', 'onglets', 'systeme') {
    $tok = $null; $err = $null
    $asts[$f] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$f.ps1"), [ref]$tok, [ref]$err)
    if ($err) { $echecs.Add("$f.ps1 n'est pas lisible : $($err[0].Message)") }
}
$ecrivains = [ordered]@{
    'microsoft'     = 'Set-ServiceOff'
    'optimisations' = 'Set-PowerPlan', 'Set-BestPerfOverlay'
    'analyse'       = 'Set-BatterySetting'
    'donnees'       = 'Set-DisplayRate', 'Save-TexteSur', 'Save-JsonSur', 'Read-JsonSur', 'Get-CleRegNormale'
    'systeme'       = 'Test-HistoriqueRemis'
    'onglets'       = 'Set-Dns'
    'assistance'    = 'ConvertFrom-HistoryEntry', 'Import-History', 'Save-History', 'Test-ChampHisto', 'Format-DemarrageService', 'Format-PlanAlim',
                      'Format-CurseurAlim', 'Format-ServeursDns', 'Get-DetailsTechniques', 'Set-HistoryAllUndone', 'Test-HistoriqueDefinitif', 'Test-HistoriqueNote', 'Test-HistoriquePartDefinitive'
}
foreach ($f in $ecrivains.Keys) {
    foreach ($n in $ecrivains[$f]) {
        $def = Get-FonctionAst $f $n
        if (-not $def) { $echecs.Add("Fonction $n introuvable dans $f.ps1"); continue }
        . ([scriptblock]::Create($def.Extent.Text))
    }
}
foreach ($f in 'optimisations', 'systeme') {
    foreach ($aff in @($asts[$f].EndBlock.Statements | Where-Object { $_ -is [System.Management.Automation.Language.AssignmentStatementAst] })) {
        $v = $aff.Left
        if ($v -is [System.Management.Automation.Language.VariableExpressionAst] -and $v.VariablePath.UserPath -in 'BestPerfOverlay', 'UltimateGuid', 'DnsChoices') { . ([scriptblock]::Create($aff.Extent.Text)) }
    }
}

# Le faux PC, suite : les gestes notés au lieu d'être faits
Add-Type -TypeDefinition @'
public static class OGNative {
    public static System.Collections.Generic.List<string> Gestes = new System.Collections.Generic.List<string>();
    public static string Curseur = "00000000-0000-0000-0000-000000000000";
    public static string[] GetDisplays() { return new string[] { @"\\.\DISPLAY1|1920|1080|32|60" }; }
    public static int SetRefreshRate(string device, int hz) { Gestes.Add("display:" + device + "=" + hz); return 0; }
    public static string GetOverlay() { return Curseur; }
    public static uint SetOverlay(string scheme) { Gestes.Add("overlay:" + scheme); Curseur = scheme; return 0; }
}
'@
function script:powercfg { [void]$script:Gestes.Add("powercfg $args"); if ("$args" -match '^/list') { return 'GUID du mode de gestion de l''alimentation : 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c  (Performances élevées)' } }
$script:Reglages = @{}
function Get-Setting([string]$Name, $Default) { if ($script:Reglages.ContainsKey($Name)) { return , $script:Reglages[$Name] }; , $Default }
function Set-Setting([string]$Name, $Value) { $script:Reglages[$Name] = $Value }
function Save-Backup { }
function Invoke-Aide([string]$Action, $P) { [void]$script:Gestes.Add("aide:$Action") }
function Get-Service { [pscustomobject]@{ Name = 'ServiceEssai'; StartType = 'Manual'; Status = 'Running' } }
function Get-BatteryPowerSettings { @{ CritAction = @(0, 1); CritLevel = @(5, 5); LowLevel = @(10, 10); LowNotify = @(0, 0); LowAction = @(0, 0) } }
function Get-ActiveNet { $script:Net }
function Update-NetInfo { }
function Write-Log { }
function Update-HistoryList { }
$script:IsLaptop = $false
$script:Backup = @{ Registry = @{}; Displays = @{}; Dns = @{} }
$script:Net = @{ IfIndex = 7; Guid = '{11111111-2222-3333-4444-555555555555}' }

# Chaque type de changement, fait par la vraie fonction de Nevermind
$script:JournalHisto = New-Object System.Collections.ArrayList
function Add-History($T, $D, $Log) { foreach ($e in @($Log)) { [void]$script:JournalHisto.Add($e) } }
$cas = [ordered]@{
    'reg'               = { Set-Reg 'HKCU:\Software\Microsoft\GameBar' 'AutoGameModeEnabled' 1 }
    'reg (effacement)'  = { Clear-Reg 'HKCU:\Software\Microsoft\GameBar' 'AutoGameModeEnabled' }
    'svc'               = { Set-ServiceOff 'ServiceEssai' }
    'svc (manuel)'      = { Set-ServiceOff 'ServiceEssai' 'Manual' }
    'power'             = { Set-PowerPlan }
    'overlay'           = { $script:IsLaptop = $true; try { Test-Ok (Set-BestPerfOverlay) 'Curseur du mode d''alimentation : Set-BestPerfOverlay n''a rien fait' } finally { $script:IsLaptop = $false } }
    'pcfg'              = { Set-BatterySetting 'CritAction' 3 }
    'display'           = { Set-DisplayRate '\\.\DISPLAY1' 144 }
    'dns'               = { Set-Dns 1 }
    'dns (automatique)' = { Set-Dns 0 }
}
$entrees = [ordered]@{}
foreach ($nom in $cas.Keys) {
    Reset-FauxPc
    Set-FauxReg "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$($script:Net.Guid)" 'NameServer' '192.168.1.1' 'String'
    $script:JournalHisto.Clear()
    $script:RunLog = New-Object System.Collections.ArrayList
    $global:LASTEXITCODE = 0
    try { & $cas[$nom] } catch { Test-Ok $false "Historique, $nom : erreur sur le faux PC : $($_.Exception.Message)" }
    $journal = @(@($script:RunLog) + @($script:JournalHisto)); $script:RunLog = $null
    Test-Ok ($journal.Count -eq 1) "Historique, $nom : $($journal.Count) entrée(s) notée(s) au lieu d'une"
    if ($journal.Count) { $entrees[$nom] = $journal[0] }
}

# La page RAM (memoire.ps1) : sur un faux PC où Edge et Chrome se lancent en avance (jamais touchés) et où un service de mise à jour
# démarre avec Windows, ce que « Couper ce qui est sûr » écrit est exactement ce qu'il déclare, et chaque changement est
# montré avant (une ligne par changement) puis noté pour l'historique
foreach ($n in 'Get-ConstatMemoire', 'Get-AppliMemoire', 'Get-SeuilsPilotes', 'Get-GenrePilote', 'Get-ChangementsMemoire', 'Get-OrigineMemoire', 'Test-JeuMemoire') {
    $def = Get-FonctionAst 'memoire' $n
    if (-not $def) { $echecs.Add("Fonction $n introuvable dans memoire.ps1"); continue }
    . ([scriptblock]::Create($def.Extent.Text))
}
foreach ($aff in @($asts['memoire'].EndBlock.Statements | Where-Object { $_ -is [System.Management.Automation.Language.AssignmentStatementAst] })) { . ([scriptblock]::Create($aff.Extent.Text)) }
function Format-Size([double]$Bytes) { "$([math]::Round($Bytes / 1MB)) Mo" }
$fauxMem = @{ Installee = 16GB; Visible = 16GB; Disponible = 8GB; PoolNP = 0.2GB
    Processus = @(@{ Pid = 10; Nom = 'msedge.exe'; Octets = 100MB; Ligne = 'msedge.exe --no-startup-window' }, @{ Pid = 11; Nom = 'chrome.exe'; Octets = 100MB; Ligne = 'chrome.exe --no-startup-window' }, @{ Pid = 12; Nom = 'updater.exe'; Octets = 5MB })
    Services = @(@{ Nom = 'ServiceEssai'; Titre = 'Essai'; Pid = 12; Demarrage = 'Auto' }, @{ Nom = 'edgeupdate'; Titre = 'Microsoft Edge Update'; Pid = 0; Demarrage = 'Auto' }, @{ Nom = 'WSearch'; Titre = 'Windows Search'; Pid = 0; Demarrage = 'Auto' })
    Demarrage = @() }
$cMem = Get-ConstatMemoire $fauxMem
$declMem = @(Get-ChangementsMemoire $cMem)
Test-Ok (($cMem.Corrections | ForEach-Object { $_.Id } | Sort-Object) -join ',' -eq 'svc:edgeupdate') "Page RAM : corrections $(@($cMem.Corrections | ForEach-Object { $_.Id }) -join ', ')"
$lignesMem = @(Get-DetailsAvant $declMem)
Test-Ok ($lignesMem.Count -eq $declMem.Count) "Page RAM : $($lignesMem.Count) ligne(s) de détails pour $($declMem.Count) changement(s)"
Test-Ok (-not @($lignesMem | Where-Object { $_ -like 'Changement non décrit*' }).Count) "Page RAM : un changement non décrit avant confirmation : $($lignesMem -join ' / ')"
Test-Ok (@($lignesMem | Where-Object { $_ -match '^Service edgeupdate : démarrage maintenant .+, après manuel' }).Count -eq 1) "Page RAM : ligne du service mal décrite : $($lignesMem -join ' / ')"
Reset-FauxPc
$script:JournalHisto.Clear()
$script:RunLog = New-Object System.Collections.ArrayList
try { foreach ($x in @($cMem.Corrections)) { Set-Declarations $x.Changes } } catch { Test-Ok $false "Page RAM : erreur sur le faux PC : $($_.Exception.Message)" }
$journalMem = @($script:RunLog); $script:RunLog = $null
Compare-Ecrit 'Page RAM' $declMem $journalMem
$svcMem = @($journalMem | Where-Object { $_.Type -eq 'svc' })
Test-Ok ($svcMem.Count -eq 1 -and $svcMem[0].Name -eq 'edgeupdate' -and $svcMem[0].Nouveau -eq 'Manual') "Page RAM : service noté $(@($svcMem | ForEach-Object { "$($_.Name)=$($_.Nouveau)" }) -join ', ')"
Test-Ok (@($script:Gestes | Where-Object { $_ -eq 'aide:service.regler' }).Count -eq 1) "Page RAM : le service n'est pas passé par l'Aide une fois ($(@($script:Gestes) -join ' '))"
Test-Ok ([bool]@(@(Get-Setting 'SvcOriginal' @()) | ForEach-Object { $_ } | Where-Object { [string]$_ -like 'edgeupdate|*' }).Count) 'Page RAM : le service n''est pas gardé pour « Tout restaurer » (SvcOriginal)'
foreach ($e in $journalMem) { $l = @(Get-DetailsTechniques @{ Log = @($e) }); Test-Ok ($l.Count -eq 1 -and $l[0] -match 'avant .+, après .+') "Page RAM, historique : ligne sans avant ou après : $($l -join ' / ')" }

# Une ligne lisible par entrée, avec l'avant et l'après ; la même après un passage par historique.json (dans %TEMP%)
$HistoryFile = Join-Path $env:TEMP "nevermind-test-historique-$PID.json"
try {
    $script:History = New-Object System.Collections.ArrayList
    [void]$script:History.Add(@{ Id = 'essai'; Date = '01/01/2026 à 12:00'; Titre = 'Essai'; Details = @(); Log = @($entrees.Values); Annule = $false })
    Save-History
    $script:History = $null
    Import-History
    $relues = @($script:History[0].Log)
    Test-Ok ($relues.Count -eq $entrees.Count) "Historique : $($relues.Count) entrée(s) relue(s) dans historique.json pour $($entrees.Count)"
    $i = 0
    foreach ($nom in @($entrees.Keys)) {
        $e = $entrees[$nom]
        Test-Ok ((Test-ChampHisto $e 'Nouveau') -or $e.Efface) "Historique, $nom : la nouvelle valeur (Nouveau) n'est pas notée au moment du changement"
        $l = @(Get-DetailsTechniques @{ Log = @($e) })
        Test-Ok ($l.Count -eq 1 -and $l[0] -match 'avant .+, après .+' -and $l[0] -notmatch 'après modifiée') "Historique, $nom : ligne sans avant ou sans après : $($l -join ' / ')"
        if ($i -lt $relues.Count) {
            $l2 = @(Get-DetailsTechniques @{ Log = @($relues[$i]) })
            Test-Ok (($l -join '|') -eq ($l2 -join '|')) "Historique, $nom : ligne différente après historique.json : « $($l -join '') » puis « $($l2 -join '') »"
        }
        $i++
    }
    # Les valeurs attendues : l'après montré est bien celui qui a été fait
    $attendu = [ordered]@{
        'reg'               = 'avant 0, après 1$'
        'reg (effacement)'  = 'avant 0, après effacée$'
        'svc'               = 'démarrage avant manuel, il tournait, après désactivé, arrêté$'
        'svc (manuel)'      = 'démarrage avant manuel, il tournait, après manuel, arrêté$'
        'power'             = 'avant « Utilisation normale » \(381b4222-f694-41f0-9685-ff5bb260df2e\), après « Performances élevées » \(8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c\)$'
        'overlay'           = "avant 00000000-0000-0000-0000-000000000000, après « Meilleures performances » \($BestPerfOverlay\)$"
        'pcfg'              = 'action à batterie critique : avant secteur ne rien faire, batterie mise en veille, après arrêt du PC \(secteur et batterie\)$'
        'display'           = 'fréquence avant 60 Hz, après 144 Hz$'
        'dns'               = 'avant 192\.168\.1\.1, après 1\.1\.1\.1, 1\.0\.0\.1$'
        'dns (automatique)' = 'avant 192\.168\.1\.1, après automatique \(box\)$'
    }
    foreach ($nom in $attendu.Keys) {
        if (-not $entrees.Contains($nom)) { continue }
        $l = [string]@(Get-DetailsTechniques @{ Log = @($entrees[$nom]) })[0]
        Test-Ok ($l -match $attendu[$nom]) "Historique, $nom : « $l » ne correspond pas à « $($attendu[$nom]) »"
    }

    # Les anciennes entrées (d'avant la nouvelle valeur notée) : lisibles, avec seulement l'avant
    $anciennes = @(foreach ($nom in @($entrees.Keys)) {
        $c = @{}; foreach ($k in @($entrees[$nom].Keys)) { if ($k -notin 'Nouveau', 'NouveauType', 'Key', 'Nom') { $c[$k] = $entrees[$nom][$k] } }; $c
    })
    $script:History = New-Object System.Collections.ArrayList
    [void]$script:History.Add(@{ Id = 'ancienne'; Date = '01/01/2025 à 12:00'; Titre = 'Ancienne'; Details = @(); Log = $anciennes; Annule = $false })
    Save-History
    $script:History = $null
    Import-History
    foreach ($e in @($script:History[0].Log)) {
        $l = @(); try { $l = @(Get-DetailsTechniques @{ Log = @($e) }) } catch { Test-Ok $false "Ancienne entrée $($e.Type) : erreur $($_.Exception.Message)" }
        Test-Ok ($l.Count -eq 1 -and $l[0] -match 'avant') "Ancienne entrée $($e.Type) : pas de ligne avec l'avant : $($l -join ' / ')"
        if ($e.Type -ne 'reg') { Test-Ok ($l[0] -notmatch 'après') "Ancienne entrée $($e.Type) : un « après » inventé : $($l[0])" }
    }
    Test-Ok ([string]@(Get-DetailsTechniques @{ Log = @(@{ Type = 'pcfg'; Guid = $BatSettings.CritAction; Ac = 0; Dc = 1 }) })[0] -match 'action à batterie critique') 'Ancienne entrée pcfg : le nom du réglage n''est pas retrouvé'

    # « Tout restaurer » : une entrée qui n'a que des changements définitifs reste « Définitif » ; les autres sont annulées
    $def = @{ Type = 'definitif'; Quoi = 'SMBv1 désactivé'; Retour = 'à la main' }
    $script:History = New-Object System.Collections.ArrayList
    [void]$script:History.Add(@{ Id = 'def'; Titre = 'Définitif'; Details = @(); Log = @($def); Annule = $false })
    [void]$script:History.Add(@{ Id = 'mixte'; Titre = 'Mixte'; Details = @(); Log = @($def, $entrees['reg']); Annule = $false })
    [void]$script:History.Add(@{ Id = 'reg'; Titre = 'Registre'; Details = @(); Log = @($entrees['reg']); Annule = $false })
    [void]$script:History.Add(@{ Id = 'note'; Titre = 'Fermé : Discord'; Details = @(); Log = @(@{ Type = 'note'; Quoi = 'Discord fermé'; Chemins = @('C:\x\Discord.exe') }); Annule = $false })   # (une appli fermée depuis la page RAM : rien à remettre)
    Set-HistoryAllUndone
    Test-Ok (-not $script:History[0].Annule -and (Test-HistoriqueDefinitif $script:History[0])) 'Tout restaurer : une entrée définitive est marquée « Annulé »'
    Test-Ok ($script:History[1].Annule -and (Test-HistoriquePartDefinitive $script:History[1]) -and -not (Test-HistoriqueDefinitif $script:History[1])) 'Tout restaurer : entrée mixte (définitive en partie) mal marquée'
    Test-Ok ($script:History[2].Annule -and -not (Test-HistoriquePartDefinitive $script:History[2])) 'Tout restaurer : une entrée annulable n''est pas marquée « Annulé »'
    Test-Ok (-not $script:History[3].Annule -and (Test-HistoriqueNote $script:History[3]) -and -not (Test-HistoriqueNote $script:History[2]) -and [string]@(Get-DetailsTechniques $script:History[3])[0] -match 'Discord fermé : C:') 'Tout restaurer : une appli fermée (seulement notée) est marquée « Annulé », ou mal décrite'
} finally { foreach ($f in $HistoryFile, "$HistoryFile.bak", "$HistoryFile.tmp") { if ([IO.File]::Exists($f)) { [IO.File]::Delete($f) } } }   # (Remove-Item est interdit dans ce test ; la copie de secours part aussi)
Test-Ok ([string]@(Get-DetailsTechniques @{ Log = @(@{ Type = 'fw'; Name = 'Nevermind essai' }) })[0] -match 'avant .+, après .+') 'Historique, pare-feu : pas d''avant ou d''après'

# Tous les types que l'historique peut contenir (ceux que « Annuler » sait défaire) sont vérifiés ci-dessus
$undo = (Get-FonctionAst 'tableau-de-bord' 'Undo-RunLog').Body.Find({ param($n) $n -is [System.Management.Automation.Language.SwitchStatementAst] }, $true)
$typesHisto = @($undo.Clauses | ForEach-Object { "$($_.Item1.Value)" })
Test-Ok ($typesHisto.Count -ge 8) "Historique : seulement $($typesHisto.Count) types trouvés dans Undo-RunLog"
$couverts = @(@($entrees.Keys) | ForEach-Object { ($_ -split ' ')[0] }) + 'fw'   # (pare-feu : une règle ajoutée seulement pour bloquer)
foreach ($t in $typesHisto) {
    if ($t -eq 'definitif') { continue }   # (une protection du PC décrite par une phrase, avec la façon de revenir en arrière)
    Test-Ok ($couverts -contains $t) "Historique : le type « $t » n'est pas vérifié (avant et après)"
}
# Dans tous les modules, chaque changement noté pour l'historique (RunLog.Add, Add-History, journal $...Log) note sa
# nouvelle valeur : une nouvelle fonction qui oublierait l'après est signalée ici
$nbNotes = 0
foreach ($fichier in Get-ChildItem -LiteralPath $modules -Filter '*.ps1') {
    $tok = $null; $err = $null
    $a = [System.Management.Automation.Language.Parser]::ParseFile($fichier.FullName, [ref]$tok, [ref]$err)
    foreach ($ht in @($a.FindAll({ param($n) $n -is [System.Management.Automation.Language.HashtableAst] -and @($n.KeyValuePairs | Where-Object { "$($_.Item1)" -eq 'Type' }).Count }, $true))) {
        $p = $ht.Parent; $journal = $false
        while ($p -and -not $journal) {
            if ($p -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and "$($p.Member)" -eq 'Add' -and "$($p.Expression)" -match 'RunLog') { $journal = $true }
            elseif ($p -is [System.Management.Automation.Language.CommandAst] -and $p.GetCommandName() -eq 'Add-History') { $journal = $true }
            elseif ($p -is [System.Management.Automation.Language.AssignmentStatementAst] -and "$($p.Left)" -match 'Log$') { $journal = $true }
            if ($p -is [System.Management.Automation.Language.FunctionDefinitionAst]) { break }
            $p = $p.Parent
        }
        if (-not $journal) { continue }
        $nbNotes++
        $cles = @($ht.KeyValuePairs | ForEach-Object { "$($_.Item1)" })
        $type = "$(@($ht.KeyValuePairs | Where-Object { "$($_.Item1)" -eq 'Type' })[0].Item2)".Trim("'", '"', ' ')
        if ($type -in 'fw', 'definitif', 'note' -or $cles -contains 'Efface' -or $cles -contains 'Nouveau') { continue }
        Test-Ok $false "$($fichier.Name) ligne $($ht.Extent.StartLineNumber) : changement « $type » noté pour l'historique sans sa nouvelle valeur (Nouveau)"
    }
}
Test-Ok ($nbNotes -ge 9) "Historique : seulement $nbNotes endroits qui notent un changement trouvés dans les modules"

# ---------------------------------------------------------------------------
# Résultat
# ---------------------------------------------------------------------------
if ($echecs.Count) {
    foreach ($e in $echecs) { Write-Host "ÉCHEC  $e" -ForegroundColor Red }
    Write-Host "$($echecs.Count) échec(s) sur $nbTests vérifications." -ForegroundColor Red
    exit 1
}
Write-Host "Déclarations des corrections : $nbTests vérifications, tout passe ($nbFix corrections du tableau de bord, $(@($Tweaks).Count) réglages, $points points de « Tout optimiser »)." -ForegroundColor Green
exit 0
