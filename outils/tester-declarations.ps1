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
#   5. que les détails montrés avant confirmation (Get-DetailsAvant) ont une ligne par changement, maintenant et après.
# Le code de Nevermind n'est pas chargé en entier : seules les fonctions utiles sont lues dans les modules (rien ne se
# lance au chargement), et tout ce qui pourrait toucher au PC est remplacé par une fausse version.
# Code de sortie : 0 si tout passe, 1 sinon.

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'OptiGame\fichiers\modules'
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
foreach ($f in 'tableau-de-bord', 'analyse', 'optimisations', 'donnees', 'assistance', 'jeu', 'bibliotheque') {
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
                 @('optimisations', 'Get-ChangementDemarrage'), @('jeu', 'Get-ChangementsProfil'), @('bibliotheque', 'Get-ChangementsOptimisationJeu'), @('tableau-de-bord', 'Get-ChangementsNettoyage'))) {
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
# Résultat
# ---------------------------------------------------------------------------
if ($echecs.Count) {
    foreach ($e in $echecs) { Write-Host "ÉCHEC  $e" -ForegroundColor Red }
    Write-Host "$($echecs.Count) échec(s) sur $nbTests vérifications." -ForegroundColor Red
    exit 1
}
Write-Host "Déclarations des corrections : $nbTests vérifications, tout passe ($nbFix corrections du tableau de bord, $(@($Tweaks).Count) réglages, $points points de « Tout optimiser »)." -ForegroundColor Green
exit 0
