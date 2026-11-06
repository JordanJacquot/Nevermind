# Les juges du bloc « pc » de Nevermind en Rust (rust/nevermind-pc/tests/juges) : des PC simulés passés dans la version
# PowerShell (la référence), et ce qu'elle en sort.
#
#   & 'outils\exporter-juges-pc.ps1' [-Nombre 300] [-Graine 6112026]
#
# Comme outils\tester-declarations.ps1 : le code de Nevermind n'est pas chargé en entier, seules les fonctions utiles sont
# lues dans les modules (rien ne se lance au chargement), et tout ce qui lirait ou toucherait le PC est remplacé par une
# fausse version qui lit le PC simulé et NOTE chaque geste (demande à l'Aide, écriture du registre de l'utilisateur,
# powercfg, fréquence d'écran, programme lancé) au lieu de le faire. Rien ne touche le vrai PC, aucune fenêtre ne s'ouvre.
#
# Sorties :
#   analyse.json      : pour chaque PC simulé, l'analyse (constats, cartes, score), les pages Ton PC et Tableau de bord,
#                       les fiches (un point à régler, « Tout corriger ») et la page Tests des composants.
#   corrections.json  : pour des PC simulés à régler, « Tout corriger » puis « Revenir en arrière », et « Tout corriger »
#                       puis « Tout restaurer » : la suite exacte des gestes, le journal (RunLog), la sauvegarde
#                       (sauvegarde.json) et les réglages notés (SvcOriginal, PcfgOriginal) à chaque étape.
param([int]$Nombre = 300, [int]$Graine = 6112026, [int]$NombreCorrections = 80)

$ErrorActionPreference = 'Stop'
$env:NEVERMIND_TEST = '1'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
$sortie  = Join-Path $racine 'rust\nevermind-pc\tests\juges'
New-Item -ItemType Directory -Force -Path $sortie | Out-Null
Add-Type -AssemblyName PresentationCore, PresentationFramework, WindowsBase   # (couleurs et fiche : objets WPF en mémoire, jamais montrés)

# ---------------------------------------------------------------------------
# Sécurité : les commandes qui agissent sont interdites (une fonction passe avant une commande du même nom)
# ---------------------------------------------------------------------------
foreach ($c in 'Set-ItemProperty', 'New-ItemProperty', 'Remove-ItemProperty', 'Set-Service', 'Stop-Process', 'shutdown.exe', 'reg', 'reg.exe',
               'Register-ScheduledTask', 'Unregister-ScheduledTask', 'Get-WinEvent', 'Get-PhysicalDisk', 'Get-NetAdapter', 'Invoke-RestMethod', 'Get-Service') {
    Set-Item -Path "function:script:$c" -Value ([scriptblock]::Create("throw 'Juges : $c est interdit.'"))
}

# ---------------------------------------------------------------------------
# Les fonctions et variables des modules
# ---------------------------------------------------------------------------
$asts = @{}
foreach ($f in 'tableau-de-bord', 'analyse', 'optimisations', 'donnees', 'assistance', 'jeu', 'navigation', 'ecran-pc', 'accueil-console', 'interface', 'systeme', 'microsoft', 'onglets', 'tests', 'ecran-divers') {
    $tok = $null; $err = $null
    $asts[$f] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$f.ps1"), [ref]$tok, [ref]$err)
    if ($err) { throw "$f.ps1 n'est pas lisible : $($err[0].Message)" }
}
function Get-FonctionAst([string]$Fichier, [string]$Nom) {
    $asts[$Fichier].Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $false)
}
# Restore-AllSettings efface les copies d'origine gardées dans le registre : remplacé par un geste noté (jamais le vrai registre)
$RemplacementsRegistre = @(
    @('try { $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($OriginesUtilisateur, $true); if ($k) { try { $k.DeleteValue($cle, $false) } finally { $k.Close() } } } catch {}', "Add-Geste 'origine.utilisateur.oublier' `$cle"),
    @('try { [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($OriginesUtilisateur, $false) } catch {}', "Add-Geste 'origines.utilisateur.effacer' `$null"),
    @('try { [Microsoft.Win32.Registry]::LocalMachine.DeleteSubKeyTree($OriginesMachine, $false); $originesOubliees = $true } catch {}', "Add-Geste 'origines.machine.effacer' `$null; `$originesOubliees = `$true")
)
$aCharger = [ordered]@{
    'interface'       = 'Invoke-FindingAction', 'Invoke-Safe', 'Get-MessageErreur'
    'accueil-console' = 'Format-TexteKit'
    'assistance'      = 'Format-ValeurTech', 'Format-DemarrageService', 'Get-HistoryItems', 'Get-HistoryTitle'
    'navigation'      = 'Get-HubFindings'
    'analyse'         = 'ConvertTo-NvidiaVersion', 'Get-LoadColor', 'Format-Duration', 'New-Component', 'Set-Worse', 'Add-Note', 'Show-HealthCards', 'Invoke-Analysis',
                        'Get-BatteryPowerSettings', 'Test-HibernateOn', 'Set-BatterySetting', 'Get-UpsConfigAdvice', 'Format-BatSetting', 'Get-BatChemistry', 'Add-UpsCards'
    'tableau-de-bord' = 'Add-Finding', 'New-Fix', 'Resolve-Declarations', 'Set-Declarations', 'Format-ValeurBat', 'Format-Declaration', 'Get-DetailsAvant', 'Invoke-CleanAll',
                        'Get-CleanAllTargets', 'Get-ChangementsNettoyage', 'Get-DriverLink', 'Get-FrequenceEcran', 'Measure-Score', 'Get-ScoreLook', 'Get-FixKind', 'Show-Findings',
                        'Add-SheetSection', 'Add-SheetInfo', 'Add-SheetDetails', 'Open-Sheet', 'Close-Sheet', 'Open-FixAll', 'Invoke-SheetRun', 'Undo-RunLog', 'Show-ResultSheet',
                        'Invoke-UndoLastRun', 'Set-IgnoreFinding'
    'optimisations'   = 'Get-ActiveScheme', 'Test-PowerPlan', 'Set-BestPerfOverlay', 'Set-PowerPlan', 'Get-ChangementAlimentation', 'Test-AccessHotkeys', 'Get-ChangementsAccessHotkeys',
                        'Get-ChangementWindowedOptim', 'Test-IsUps', 'Test-IsDesktop', 'Test-IsLaptop', 'Get-TweakChanges', 'Invoke-TweakApply', 'Get-AvailableTweaks', 'Test-Tweak',
                        'Get-TweakWeight', 'Get-ChangementDemarrage', 'Set-StartupState', 'Test-StartupEnabled', 'Resolve-StartupExe'
    'donnees'         = 'Set-Reg', 'Clear-Reg', 'Get-CleRegNormale', 'Test-RegUtilisateur', 'Write-RegValue', 'Remove-RegValue', 'Set-DisplayRate'
    'jeu'             = 'Get-ChangementsProfil'
    'systeme'         = 'Format-Size', 'Restore-AllSettings', 'Invoke-CleanTargets'
    'microsoft'       = 'Set-ServiceOff'
    'onglets'         = 'New-RestorePoint', 'Get-BackupCount'
    'ecran-pc'        = 'Get-TonKit', 'Get-IconeConstat', 'Get-MesuresKit', 'Get-LigneConstat', 'Get-PagesPc', 'Get-IdOutilKit', 'Get-FicheKit'
}
foreach ($f in $aCharger.Keys) {
    foreach ($n in $aCharger[$f]) {
        $def = Get-FonctionAst $f $n
        if (-not $def) { throw "Fonction $n introuvable dans $f.ps1" }
        $texte = $def.Extent.Text
        foreach ($r in $RemplacementsRegistre) { $texte = $texte.Replace($r[0], $r[1]) }
        if ($texte -match '\[Microsoft\.Win32\.Registry\]') { throw "Juges : $n écrit encore le vrai registre" }
        . ([scriptblock]::Create($texte))
    }
}
$variables = [ordered]@{
    'interface'       = 'Colors'
    'analyse'         = 'StatusLabels', 'Muted', 'SubBattery', 'BatSettings', 'BatActions'
    'optimisations'   = 'GuidPattern', 'HighPerfGuid', 'BalancedGuid', 'BestPerfOverlay', 'UltimateGuid', 'MousePath', 'AccessPaths', 'DxPath', 'DoPath', 'Tweaks', 'UpsVendors', 'ApprovedRoot', 'SafeStartup'
    'jeu'             = 'IfeoPath'
    'tableau-de-bord' = 'BatNoms', 'BiosRun', 'BiosChanges', 'BiosConfirm', 'BiosDone'
    'ecran-pc'        = 'TonsStatut', 'IconesTon', 'SeuilTempKit'
}
foreach ($f in $variables.Keys) {
    foreach ($aff in @($asts[$f].EndBlock.Statements | Where-Object { $_ -is [System.Management.Automation.Language.AssignmentStatementAst] })) {
        $v = $aff.Left; if ($v -is [System.Management.Automation.Language.ConvertExpressionAst]) { $v = $v.Child }
        if ($v -is [System.Management.Automation.Language.VariableExpressionAst] -and $variables[$f] -contains $v.VariablePath.UserPath) { . ([scriptblock]::Create($aff.Extent.Text)) }
    }
}
foreach ($n in 'Tweaks', 'Colors', 'StatusLabels', 'TonsStatut', 'BatSettings', 'BiosChanges', 'IfeoPath', 'UpsVendors') { if ($null -eq (Get-Variable $n -ValueOnly -ErrorAction SilentlyContinue)) { throw "Variable `$$n introuvable" } }

# ---------------------------------------------------------------------------
# Le faux PC : tout ce que Nevermind lit vient du PC simulé, tout ce qu'il fait est noté dans $script:Gestes
# ---------------------------------------------------------------------------
$script:Maintenant = [datetime]'2026-11-06T12:00:00'
function Get-Date {
    if (-not $args.Count) { return $script:Maintenant }
    if ($args.Count -eq 2 -and $args[0] -eq '-Format') { return $script:Maintenant.ToString($(if ($args[1] -eq 's') { 'yyyy-MM-ddTHH:mm:ss' } else { $args[1] })) }
    throw "Juges : Get-Date $args"
}
$script:Gestes = New-Object System.Collections.ArrayList
function Add-Geste([string]$Quoi, $Donnees) { [void]$script:Gestes.Add([ordered]@{ geste = $Quoi; donnees = $Donnees }) }

function ConvertTo-ValeurPs($e) {
    if ($null -eq $e) { return $null }
    switch ([string]$e.Kind) {
        'DWord' { return [int]$e.Value }
        'QWord' { return [long]$e.Value }
        'Binary' { return , ([byte[]]@($e.Value)) }
        'MultiString' { return , ([string[]]@($e.Value)) }
        default { return [string]$e.Value }
    }
}
function Get-RegState([string]$Path, [string]$Name) {
    $e = $script:FauxReg["$Path|$Name"]
    if ($e) { $v = ConvertTo-ValeurPs $e; return @{ Existed = $true; Value = $v; Kind = [string]$e.Kind } }
    @{ Existed = $false; Value = $null; Kind = $null }
}
function Get-RegValue([string]$Path, [string]$Name) { $e = $script:FauxReg["$Path|$Name"]; if ($e) { return , (ConvertTo-ValeurPs $e) }; $null }
function Get-GpuPreference([string]$Exe) { $v = Get-RegValue $DxPath $Exe; if ($null -eq $v) { return '' }; [string]$v }
function Open-RegKey { throw 'Juges : Open-RegKey est interdit.' }
# Ce qu'écrit Nevermind dans le registre de l'utilisateur (Write-RegValue) : le faux registre change, et c'est noté
function script:Write-RegHkcu([string]$Path, [string]$Name, $Value, [string]$Kind) {
    $v = if ($Value -is [byte[]]) { @([int[]]$Value) } else { $Value }
    $script:FauxReg["$Path|$Name"] = @{ Value = $v; Kind = $Kind }
    Add-Geste 'reg.utilisateur.ecrire' ([ordered]@{ Cle = $Path; Nom = $Name; Valeur = $v; Type = $Kind })
}
function script:Remove-RegHkcu([string]$Path, [string]$Name) {
    $script:FauxReg.Remove("$Path|$Name")
    Add-Geste 'reg.utilisateur.effacer' ([ordered]@{ Cle = $Path; Nom = $Name })
}
# Write-RegValue / Remove-RegValue de donnees.ps1, avec la partie « utilisateur » branchée sur le faux registre
function Write-RegValue([string]$Path, [string]$Name, $Value, [string]$Kind) {
    if (-not (Test-RegUtilisateur $Path)) {
        $v = if ($Value -is [byte[]]) { [int[]]$Value } else { $Value }
        [void](Invoke-Aide 'reg.ecrire' @{ Cle = $Path; Nom = $Name; Valeur = $v; Type = $Kind })
        return
    }
    Write-RegHkcu $Path $Name $Value $Kind
}
function Remove-RegValue([string]$Path, [string]$Name) {
    if (-not (Test-RegUtilisateur $Path)) { [void](Invoke-Aide 'reg.effacer' @{ Cle = $Path; Nom = $Name }); return }
    Remove-RegHkcu $Path $Name
}
# Save-Original de donnees.ps1 sur le faux registre (sa copie HKCU notée comme un geste)
function Save-Original([string]$Path, [string]$Name) {
    $key = "$Path|$Name"
    if ($script:Backup.Registry.ContainsKey($key)) { return }
    $st = Get-RegState $Path $Name
    $val = $st.Value; if ($val -is [byte[]]) { $val = [int[]]$val }
    $script:Backup.Registry[$key] = @{ Path = $Path; Name = $Name; Existed = $st.Existed; Value = $val; Kind = $st.Kind }
    Save-Backup
    if (Test-RegUtilisateur $Path) { Add-Geste 'origine.utilisateur' ([ordered]@{ Cle = $key; Json = (ConvertTo-Json -InputObject $script:Backup.Registry[$key] -Compress -Depth 3) }) }
}
function Save-Backup { $script:SauvegardesEcrites++ }
function Save-BackupOuOublier([scriptblock]$Oublier) { Save-Backup }
function Import-Backup { }
# L'Aide : chaque demande est notée ; les réponses viennent du PC simulé
function Invoke-Aide([string]$Action, $Parametres = @{}, [int]$Delai = 120000, [switch]$SansDemande) {
    $p = [ordered]@{}; foreach ($k in @($Parametres.Keys | Sort-Object)) { $p[$k] = $Parametres[$k] }
    Add-Geste "aide:$Action" $p
    if ($script:AideRefuse -contains $Action) { throw "L'Aide a refusé ($Action)" }
    switch ($Action) {
        'reg.ecrire' { $script:FauxReg["$($Parametres.Cle)|$($Parametres.Nom)"] = @{ Value = $Parametres.Valeur; Kind = $Parametres.Type } }
        'reg.effacer' { $script:FauxReg.Remove("$($Parametres.Cle)|$($Parametres.Nom)") }
        'service.regler' { $script:FauxServices[[string]$Parametres.Nom] = @{ StartType = [string]$Parametres.Demarrage; Running = [bool]$Parametres.Lancer } }
        'restauration.creer' { return $script:ReponseRestauration }
        'nettoyage.lancer' { return @{ Freed = 1048576.0; Deleted = 3; Skipped = 1; Errors = 0; Log = @("[SUPPRIMÉ] C:\Faux\$($Parametres.Id)\a.tmp - 1,0 Mo") } }
    }
    'OK'
}
function Get-Service([string]$Name, $ErrorAction) {
    $s = $script:FauxServices[$Name]
    if (-not $s) { if ($ErrorAction -eq 'Stop') { throw "Service $Name introuvable" }; return $null }
    [pscustomobject]@{ Name = $Name; StartType = $s.StartType; Status = $(if ($s.Running) { 'Running' } else { 'Stopped' }) }
}
# powercfg : la liste des plans, le plan actif, les réglages « batterie » ; les changements sont notés (et le faux PC suit)
function powercfg {
    $a = @($args | ForEach-Object { [string]$_ })
    $global:LASTEXITCODE = 0
    switch -Regex ($a[0]) {
        '^/getactivescheme$' { $pl = $script:Plans | Where-Object { $_.Guid -eq $script:PlanActif } | Select-Object -First 1; if ($pl) { return "GUID du mode de gestion de l'alimentation : $($pl.Guid)  ($($pl.Nom))" }; return '' }
        '^/list$' { return @('Modes de gestion de l''alimentation existants (* Actif)', '-----------------------------------') + @($script:Plans | ForEach-Object { "GUID du mode de gestion de l'alimentation : $($_.Guid)  ($($_.Nom))$(if ($_.Guid -eq $script:PlanActif) { ' *' })" }) }
        '^/q$' { return @(Get-TexteBatterie) }
    }
    Add-Geste 'powercfg' @($a)
    switch -Regex ($a[0]) {
        '^-duplicatescheme$' { $g = 'c0ffee00-1111-2222-3333-444455556666'; $script:Plans += @{ Guid = $g; Nom = 'Performances élevées' }; return "GUID du mode de gestion de l'alimentation : $g  (Performances élevées)" }
        '^/setactive$' { if ($a[1] -ne 'SCHEME_CURRENT') { if (@($script:Plans | Where-Object { $_.Guid -eq $a[1] }).Count) { $script:PlanActif = $a[1] } else { $global:LASTEXITCODE = 1 } } }
        '^/delete$' { $script:Plans = @($script:Plans | Where-Object { $_.Guid -ne $a[1] }) }
        '^/set(ac|dc)valueindex$' {
            foreach ($k in @($script:BatEtat.Keys)) { if ($BatSettings[$k] -eq $a[3]) { $script:BatEtat[$k][$(if ($a[0] -match 'ac') { 0 } else { 1 })] = [int]$a[4] } }
        }
    }
}
# Les réglages « batterie » du plan actif, comme les écrit powercfg /q (le sous-groupe et ses réglages)
function Get-TexteBatterie {
    if (-not $script:BatEtat -or -not $script:BatEtat.Count) { return @() }
    "GUID du mode de gestion de l'alimentation : $($script:PlanActif)  (Plan actif)"
    "  GUID du sous-groupe : $SubBattery  (Batterie)"
    foreach ($k in @($BatSettings.Keys)) {
        if (-not $script:BatEtat.ContainsKey($k)) { continue }
        "    GUID du paramètre d'alimentation : $($BatSettings[$k])  ($k)"
        "      Index de paramètre de courant alternatif actuel : 0x$('{0:x8}' -f [int]$script:BatEtat[$k][0])"
        "      Index de paramètre de courant continu actuel : 0x$('{0:x8}' -f [int]$script:BatEtat[$k][1])"
    }
}
function Test-Path {
    param([Parameter(Position = 0)][string]$Path, [string]$LiteralPath, $PathType)
    $p = if ($LiteralPath) { $LiteralPath } else { $Path }
    if ($p -like '*hiberfil.sys') { return [bool]$script:Hiberfil }
    if ($p -like "$DataDir*") { return [IO.File]::Exists($p) -or [IO.Directory]::Exists($p) }
    $false
}
function Remove-Item { param([string[]]$LiteralPath, $ErrorAction) Add-Geste 'fichier.effacer' @($LiteralPath | ForEach-Object { [IO.Path]::GetFileName($_) }) }
# Le curseur du mode d'alimentation et les écrans : la classe native, remplacée
class OGNative {
    static [string] $Overlay = ''
    static [bool] $OverlayRefuse = $false
    static [string[]] $Ecrans = @()
    static [string] GetOverlay() { if (-not [OGNative]::Overlay) { throw 'pas de curseur' }; return [OGNative]::Overlay }
    static [int] SetOverlay([string]$g) { [void]$script:Gestes.Add([ordered]@{ geste = 'curseur'; donnees = $g }); if ([OGNative]::OverlayRefuse) { return 5 }; [OGNative]::Overlay = $g; return 0 }
    static [string[]] GetDisplays() { return [OGNative]::Ecrans }
    static [int] SetRefreshRate([string]$d, [int]$hz) {
        [void]$script:Gestes.Add([ordered]@{ geste = 'ecran.frequence'; donnees = [ordered]@{ Ecran = $d; Hz = $hz } })
        [OGNative]::Ecrans = @(foreach ($e in [OGNative]::Ecrans) { $p = $e -split '\|'; if ($p[0] -eq $d) { $p[4] = "$hz" }; $p -join '|' })
        return 0
    }
    static [int] SystemParametersInfo([int]$a, [int]$b, [int[]]$c, [int]$d) { [void]$script:Gestes.Add([ordered]@{ geste = 'souris.relire'; donnees = @($c) }); return 1 }
}
function Sync-Mouse {
    $speed = [int](Get-RegValue $MousePath 'MouseSpeed'); $t1 = [int](Get-RegValue $MousePath 'MouseThreshold1'); $t2 = [int](Get-RegValue $MousePath 'MouseThreshold2')
    [void][OGNative]::SystemParametersInfo(4, 0, [int[]]@($t1, $t2, $speed), 3)
}
function Start-Process { Add-Geste 'lancer' (@($args | ForEach-Object { [string]$_ })) }
function Open-Url([string]$u) { Add-Geste 'ouvrir' $u }
function Get-NetFirewallRule { @() }
function Get-CimInstance { param([string]$ClassName, $Namespace, $ErrorAction) $script:Cim[$ClassName] }
function Invoke-Smi([string]$Fields) { $script:Smi }
function Get-GpuVram([string]$Name) { $v = $script:Vram[$Name]; if ($v) { [double]$v } else { $null } }
# (activé ou pas : lu dans StartupApproved du faux registre, comme Read-StartupItems)
function Get-StartupItems {
    @(foreach ($d in $script:Demarrage) {
        $on = Test-StartupEnabled $d.Approved $d.ValueName
        [pscustomobject]@{ Nom = $d.Nom; Etat = $(if ($on) { 'Activé' } else { 'Désactivé' }); Source = $d.Source; Commande = $d.Commande; Approved = $d.Approved; ValueName = $d.ValueName; Enabled = $on; Exe = $d.Exe }
    })
}
function Clear-StartupItemsCache { }
function Get-InstalledGamesWork { { 'jeux' } }
$AnalysisDataWork = { 'donnees' }
function Invoke-Async($Script, $Argument, $Argument2) {
    if ("$Script" -match "'jeux'") { return @($script:Jeux) }
    if ("$Script" -match "'donnees'") { return $script:Data }
    throw "Juges : Invoke-Async ($Script)"
}
function Get-ActiveNet { $null }
function Add-AideLecturePc($Data) { }
function Test-PiloteNvidia { $false }
function Get-Setting([string]$Name, $Default = $null) { if ($script:Settings.ContainsKey($Name)) { $script:Settings[$Name] } else { $Default } }
function Set-Setting([string]$Name, $Value, [switch]$Exiger) {
    if ($Exiger -and $script:ParametresBloques) { throw "Nevermind n'a pas pu noter l'état d'origine (son fichier de préférences est pris par un autre programme) : rien n'a été changé. Réessaie dans un instant." }
    $script:Settings[$Name] = $Value
    Add-Geste 'reglage' ([ordered]@{ Nom = $Name; Valeur = $Value })
}
function Save-Ignored { Add-Geste 'ignores' @($script:Ignored) }
function Write-Log([string]$m) { }
function Step-UI { } ; function Update-UI { } ; function Set-Busy { } ; function Set-Status([string]$t) { $script:DernierStatut = $t }
function Show-Message([string]$t) { Add-Geste 'message' $t }
function Confirm-Action([string]$t) { Add-Geste 'question' $t; $script:ReponseQuestion }
function Build-GamingTab { } ; function Update-StartupList { } ; function Update-BackupSummary { } ; function Update-NetInfo { } ; function Update-HistoryList { }
function Add-History([string]$Titre, [string[]]$Details, $Log) { if (-not $Log -or -not @($Log).Count) { return $null }; Add-Geste 'historique' ([ordered]@{ Titre = $Titre; Details = @($Details | Where-Object { $_ }); Nombre = @($Log).Count }); 'h1' }
function Set-HistoryUndone([string]$Id) { Add-Geste 'historique.annule' $Id }
function Get-DossiersNettoyage { @{ wu = @('C:\Windows\SoftwareDistribution\Download'); do = @('C:\Windows\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache'); dump = @('C:\Windows\Minidump') } }
function Invoke-CleanOne($Target) { if ($Target.Aide) { return (Invoke-Aide 'nettoyage.lancer' @{ Id = $Target.Id } 900000) }; Add-Geste 'nettoyage.dossier' $Target.Id; @{ Freed = 2048.0; Deleted = 1; Skipped = 0; Errors = 0; Log = @("[SUPPRIMÉ] $($Target.Paths[0])\b.tmp - 2 Ko") } }
function Test-RegUtilisateur([string]$Path) { ($Path -replace '^Registry::', '') -match '^(HKCU:|HKEY_CURRENT_USER)' }
$OriginesUtilisateur = 'Software\MindsetTest\Origines'
$OriginesMachine = 'SOFTWARE\Mindset\Nevermind\Aide\Origines'
$BackupFile = Join-Path $env:TEMP 'nevermind-juges-pc\sauvegarde.json'
$DataDir = Join-Path $env:TEMP 'nevermind-juges-pc'
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
$DossiersNettoyageAide = Get-DossiersNettoyage
foreach ($aff in @($asts['systeme'].EndBlock.Statements | Where-Object { $_ -is [System.Management.Automation.Language.AssignmentStatementAst] -and "$($_.Left)" -eq '$CleanTargets' })) { . ([scriptblock]::Create($aff.Extent.Text)) }
$env:TEMP = 'C:\Faux\Temp'   # (après $DataDir : les chemins du nettoyage, jamais le vrai dossier)
foreach ($aff in @($asts['systeme'].EndBlock.Statements | Where-Object { $_ -is [System.Management.Automation.Language.AssignmentStatementAst] -and "$($_.Left)" -eq '$CleanTargets' })) { . ([scriptblock]::Create($aff.Extent.Text)) }
$env:TEMP = [IO.Path]::GetTempPath().TrimEnd('\')

# L'interface : seulement ce que lisent les fonctions chargées (la fiche est faite de vrais éléments WPF, jamais montrés)
function New-Thickness($a, $b, $c, $d) { [System.Windows.Thickness]::new($a, $b, $c, $d) }
function Get-Brush([string]$Hex) { [System.Windows.Media.SolidColorBrush]::new([System.Windows.Media.ColorConverter]::ConvertFromString($Hex)) }
function New-Text([string]$Text, [double]$Size = 13, [string]$Color = '#EEEBF7', [switch]$Bold, [switch]$Semi) {
    $t = New-Object System.Windows.Controls.TextBlock
    $t.Text = $Text; $t.Foreground = Get-Brush $Color
    if ($Bold) { $t.FontWeight = [System.Windows.FontWeights]::Bold } elseif ($Semi) { $t.FontWeight = [System.Windows.FontWeights]::SemiBold }
    $t
}
function New-Grid([string[]]$Cols) { New-Object System.Windows.Controls.Grid }
function Add-ToGrid($Grid, $El, [int]$Col) { [void]$Grid.Children.Add($El) }
function New-Badge([string]$Text, [string]$Color) { $b = New-Object System.Windows.Controls.Border; $b.Child = New-Text $Text 11 $Color -Semi; $b }
function New-Ui {
    $u = @{ HealthSummary = [pscustomobject]@{ Text = '' } }
    $u.SheetBody = New-Object System.Windows.Controls.StackPanel
    $u.Overlay = [pscustomobject]@{ Visibility = 'Collapsed' }
    foreach ($n in 'SheetRun', 'SheetOpen', 'SheetIgnore', 'SheetClose') { $u[$n] = New-Object System.Windows.Controls.Button }
    $u
}
$Window = $null

# ---------------------------------------------------------------------------
# Les PC simulés
# ---------------------------------------------------------------------------
$script:Rnd = New-Object System.Random $Graine
function Get-Hasard([int]$Max) { $script:Rnd.Next($Max) }
function Get-Un($Liste) { $l = @($Liste); $l[$script:Rnd.Next($l.Count)] }
function Test-Chance([double]$P) { $script:Rnd.NextDouble() -lt $P }
function Get-Date-Iso([datetime]$D) { $D.ToString('yyyy-MM-ddTHH:mm:ss') }

$GpuPool = @(
    @{ Name = 'NVIDIA GeForce RTX 3070'; DriverVersion = '32.0.15.6094' }, @{ Name = 'NVIDIA GeForce GTX 1660 SUPER'; DriverVersion = '31.0.15.3640' },
    @{ Name = 'NVIDIA GeForce RTX 4060 Laptop GPU'; DriverVersion = '32.0.15.8157' }, @{ Name = 'AMD Radeon RX 6700 XT'; DriverVersion = '31.0.24002.92' },
    @{ Name = 'AMD Radeon(TM) Graphics'; DriverVersion = '31.0.21910.5' }, @{ Name = 'Intel(R) UHD Graphics 630'; DriverVersion = '31.0.101.2125' },
    @{ Name = 'Intel(R) Arc(TM) A770 Graphics'; DriverVersion = '32.0.101.6083' }, @{ Name = 'Microsoft Basic Display Adapter'; DriverVersion = '10.0.26100.1' },
    @{ Name = 'Parsec Virtual Display Adapter'; DriverVersion = '0.45.0.0' }, @{ Name = 'Intel(R) Iris(R) Xe Graphics'; DriverVersion = '31.0.101.4502' }
)
$CpuPool = @('AMD Ryzen 7 5800X3D 8-Core Processor', 'Intel(R) Core(TM) i5-10400F CPU @ 2.90GHz  ', '12th Gen Intel(R) Core(TM) i7-12700H', 'AMD Ryzen 5 3600 6-Core Processor', 'Intel(R) Core(TM) i3-8100 CPU @ 3.60GHz')
$DemarragePool = @(
    @('Discord', '"C:\Users\Essai\AppData\Local\Discord\Update.exe" --processStart Discord.exe'), @('Steam', '"C:\Program Files (x86)\Steam\steam.exe" -silent'),
    @('RtkAudUService', '"C:\Windows\System32\RtkAudUService64.exe" -background'), @('SecurityHealth', '%windir%\system32\SecurityHealthSystray.exe'),
    @('Spotify', 'C:\Users\Essai\AppData\Roaming\Spotify\Spotify.exe /minimized'), @('OneDrive', '"C:\Program Files\Microsoft OneDrive\OneDrive.exe" /background'),
    @('EpicGamesLauncher', '"C:\Program Files (x86)\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe" -silent'), @('LGHUB', '"C:\Program Files\LGHUB\lghub.exe" --background'),
    @('MicrosoftEdgeAutoLaunch_123', '"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --no-startup-window'), @('Outil maison', 'C:\Outils\outil.exe'),
    @('Battle.net', '"C:\Program Files (x86)\Battle.net\Battle.net Launcher.exe" --autostarted'), @('Teams', '"C:\Users\Essai\AppData\Local\Microsoft\Teams\Update.exe" --processStart Teams.exe'),
    @('NvBackend', '"C:\Program Files\NVIDIA Corporation\NvBackend.exe"'), @('Ubisoft Connect', '"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\UbisoftConnect.exe" -autostart'),
    @('Razer Synapse', '"C:\Program Files (x86)\Razer\Synapse3\Razer Synapse 3.exe"'), @('Overwolf', '"C:\Program Files (x86)\Overwolf\OverwolfLauncher.exe" -launchapp'),
    @('Zoom', '"C:\Users\Essai\AppData\Roaming\Zoom\bin\Zoom.exe" --autostart'), @('WhatsApp', 'C:\Users\Essai\AppData\Local\WhatsApp\WhatsApp.exe'), @('Clavier élan', 'C:\Program Files\Elan\ETDCtrl.exe'),
    @('Mise à jour', 'C:\Program Files\Updater\update.exe')
)

function New-PcSimule([int]$N) {
    $portable = Test-Chance 0.3
    $ups = (-not $portable) -and (Test-Chance 0.25)
    $boot = $script:Maintenant.AddHours(-(1 + (Get-Hasard 400)))
    $totMem = 0
    $mem = @(for ($i = 0; $i -lt (1 + (Get-Hasard 4)); $i++) {
        $cap = Get-Un @(4GB, 8GB, 8GB, 16GB, 16GB, 32GB)
        $totMem += $cap
        [ordered]@{ Capacity = [double]$cap; ConfiguredClockSpeed = $(Get-Un @(0, 2133, 2666, 3200, 3600, 4800, 6000)); Speed = (Get-Un @(2400, 3200, 5600))
            SMBIOSMemoryType = (Get-Un @(24, 26, 26, 34, 30, 0)); Manufacturer = (Get-Un @('Kingston', 'Corsair', 'Unknown', '0000', 'G.Skill  ', '')); PartNumber = (Get-Un @('KHX3200C16D4/8GX   ', 'CMK16GX4M2B3200C16', '', 'F4-3600C16-8GVKC')) }
    })
    $nbGpu = 1 + (Get-Hasard 2)
    $gpus = @(for ($i = 0; $i -lt $nbGpu; $i++) {
        $g = Get-Un $GpuPool
        $d = if (Test-Chance 0.1) { $null } else { Get-Date-Iso ($script:Maintenant.AddDays(-(Get-Hasard 700))) }
        [ordered]@{ Name = $g.Name; DriverVersion = $g.DriverVersion; DriverDate = $d }
    })
    $disques = @(for ($i = 0; $i -lt (1 + (Get-Hasard 3)); $i++) {
        $media = Get-Un @('SSD', 'SSD', 'HDD', 'Unspecified')
        $bus = if ($media -eq 'HDD') { Get-Un @('SATA', 'USB') } else { Get-Un @('NVMe', 'NVMe', 'SATA', 'USB') }
        $taille = [double](Get-Un @(256GB, 512GB, 1TB, 2TB, 4TB))
        $vols = @(foreach ($l in @(if ($i -eq 0) { 'C' } else { Get-Un @('D', 'E', 'F') })) {
            $libre = [double]([math]::Floor($taille * (Get-Un @(0.03, 0.08, 0.15, 0.4, 0.7))))
            [ordered]@{ DriveLetter = $l; Vol = $(if (Test-Chance 0.05) { $null } else { [ordered]@{ Size = $taille; SizeRemaining = $libre; FileSystemLabel = (Get-Un @('', 'Jeux', 'Données', 'Windows')) } }) }
        })
        $rel = if (Test-Chance 0.25) { $null } else {
            [ordered]@{ Wear = $(if (Test-Chance 0.2) { $null } else { Get-Un @(0, 3, 12, 71, 92) }); Temperature = (Get-Un @(0, 34, 45, 52, 73, 81)); PowerOnHours = (Get-Un @(0, 1250, 18432, 40001))
                ReadErrorsUncorrected = $(if (Test-Chance 0.3) { $null } else { Get-Un @(0, 0, 0, 4) }) }
        }
        [ordered]@{ Disk = [ordered]@{ DeviceId = "$i"; FriendlyName = (Get-Un @('Samsung SSD 980 PRO 1TB', 'WDC WD10EZEX-08WN4A0 ', 'Crucial CT500MX500SSD1', 'KINGSTON SNV2S1000G', 'Seagate Expansion')); MediaType = $media; BusType = $bus
                HealthStatus = (Get-Un @('Healthy', 'Healthy', 'Healthy', 'Warning', 'Unhealthy', '')); Size = $taille }
            Vols = $vols; Rel = $rel; Letters = @($vols | ForEach-Object { $_.DriveLetter }) }
    })
    $batterie = @()
    if ($portable) { $batterie = @([ordered]@{ Name = 'DELL 7FHHV31 '; DeviceID = '1234DELL'; Description = 'Batterie interne'; Chemistry = 6; BatteryStatus = (Get-Un @(1, 2)); EstimatedChargeRemaining = (Get-Un @(12, 54, 100)); EstimatedRunTime = 71582788 }) }
    elseif ($ups) { $batterie = @([ordered]@{ Name = (Get-Un @('Back-UPS BX700U FW:', 'CP1500EPFCLCD', 'Onduleur')); DeviceID = 'UPS1'; Description = ''; Chemistry = (Get-Un @(3, 2)); BatteryStatus = (Get-Un @(2, 2, 1)); EstimatedChargeRemaining = (Get-Un @(100, 64, 20)); EstimatedRunTime = (Get-Un @(3, 12, 45)) }) }
    $chassis = if ($portable) { Get-Un @(@(10), @(9), @()) } else { Get-Un @(@(3), @(7), @(), @(35)) }
    $pcType = if ($portable) { Get-Un @(2, 2, 0) } else { Get-Un @(1, 0) }
    $batPower = $null
    if ($ups -or (Test-Chance 0.1)) {
        $batPower = [ordered]@{ CritAction = @((Get-Un @(0, 1, 2, 3)), (Get-Un @(0, 1, 2, 3))); CritLevel = @(5, (Get-Un @(5, 10, 25))); LowLevel = @(10, (Get-Un @(10, 30, 50))); LowNotify = @(1, (Get-Un @(0, 1))); LowAction = @(0, (Get-Un @(0, 1))) }
        if (Test-Chance 0.2) { $batPower.Remove('LowAction') }
    }
    $hints = @()
    if (-not $ups -and (Test-Chance 0.1)) { $hints = @(Get-Un @(@([ordered]@{ Kind = 'usb'; Name = 'American Power Conversion USB UPS'; Vid = '051D'; Ok = $true }), @([ordered]@{ Kind = 'soft'; Name = 'PowerChute Personal Edition'; Ok = $true }), @([ordered]@{ Kind = 'usb'; Name = 'Périphérique USB'; Vid = '0665'; Ok = $true }, [ordered]@{ Kind = 'app'; Name = 'ViewPower'; Ok = $true }))) }
    $net = if (Test-Chance 0.1) { $null } else { [ordered]@{ Wifi = (Test-Chance 0.4); Speed = (Get-Un @('1 Gbps', '866.7 Mbps', '100 Mbps', '2.5 Gbps')); Desc = (Get-Un @('Realtek PCIe GbE Family Controller', 'Intel(R) Wi-Fi 6 AX201 160MHz')) } }
    $build = Get-Un @(19045, 22631, 26100, 26200)
    $data = [ordered]@{
        OS = [ordered]@{ Caption = $(if ($build -ge 22000) { 'Microsoft Windows 11 Famille' } else { 'Microsoft Windows 10 Professionnel' }); BuildNumber = "$build"
            TotalVisibleMemorySize = [double]([math]::Floor($totMem / 1KB * 0.98)); FreePhysicalMemory = [double]([math]::Floor($totMem / 1KB * (Get-Un @(0.05, 0.12, 0.4, 0.7)))); LastBootUpTime = (Get-Date-Iso $boot) }
        Battery = $batterie; BatWmi = $(if ($ups) { [ordered]@{ Maker = (Get-Un @('APC', '', 'CPS')); Serial = (Get-Un @('3B2041X12345', '')); Chem = (Get-Un @(1665230416, 0, 1852402508)); Design = (Get-Un @(0, 12000)); Full = (Get-Un @(0, 4000, 9000, 11800)); Volt = (Get-Un @(0, 13600)); Runtime = (Get-Un @(0, 240, 1500, 400000)) } } else { $null })
        UpsHints = $hints; Chassis = $chassis; PCType = $pcType
        CPU = [ordered]@{ Name = (Get-Un $CpuPool); NumberOfCores = (Get-Un @(4, 6, 8, 14)); NumberOfLogicalProcessors = (Get-Un @(4, 12, 16, 20)); MaxClockSpeed = (Get-Un @(2496, 2900, 3401, 3600)); L3CacheSize = (Get-Un @(0, 6144, 32768, 98304)) }
        GPUs = $gpus
        NvLatest = $(if (Test-Chance 0.4) { [ordered]@{ Version = (Get-Un @('581.57', '560.94', '615.14')); Date = 'Mon Oct 20, 2025'; Url = 'https://www.nvidia.com/fr-fr/drivers/details/999999/' } } else { $null })
        Mem = $mem
        MemDiag = $(if (Test-Chance 0.5) { $null } else { [ordered]@{ Id = (Get-Un @(1101, 1102, 1201, 1202, 1001)); TimeCreated = (Get-Date-Iso ($script:Maintenant.AddDays(-(Get-Hasard 300)))) } })
        SysDisk = '0'; Disks = $disques
        LogicalC = $(if (Test-Chance 0.05) { $null } else { $s = [double](Get-Un @(250GB, 500GB, 1TB)); [ordered]@{ Size = $s; FreeSpace = [double]([math]::Floor($s * (Get-Un @(0.04, 0.09, 0.15, 0.19, 0.25, 0.6)))) } })
        BaseBoard = [ordered]@{ Manufacturer = (Get-Un @('ASUSTeK COMPUTER INC.', 'Micro-Star International Co., Ltd.', '')); Product = (Get-Un @('ROG STRIX B550-F GAMING', 'MAG B660M', '')) }
        BIOS = [ordered]@{ SMBIOSBIOSVersion = (Get-Un @('2803', '1.A0', 'F12')); ReleaseDate = (Get-Date-Iso ($script:Maintenant.AddDays(-(Get-Hasard 2500)))) }
        SecureBoot = (Get-Un @($true, $false, $null)); TpmOk = (Test-Chance 0.7); Tpm = $(if (Test-Chance 0.8) { [ordered]@{ SpecVersion = '2.0, 0, 1.59' } } else { $null })
        Bsod = (Get-Un @(0, 0, 0, 1, 3)); Crash = (Get-Un @(0, 0, 2, 5)); WheaErr = (Get-Un @(0, 0, 0, 2)); WheaWarn = (Get-Un @(0, 0, 25))
        Net = $net
    }
    # Le registre simulé : chaque réglage gaming fait ou pas, et ses valeurs
    $reg = [ordered]@{}
    $ajoute = { param($p, $n, $v, $k) if ($null -ne $v) { $reg["$p|$n"] = [ordered]@{ Value = $v; Kind = $k } } }
    & $ajoute 'HKCU:\Software\Microsoft\Windows\CurrentVersion\GameDVR' 'HistoricalCaptureEnabled' (Get-Un @(0, 1, $null)) 'DWord'
    & $ajoute 'HKCU:\Software\Microsoft\GameBar' 'AutoGameModeEnabled' (Get-Un @(0, 1, $null)) 'DWord'
    & $ajoute 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' 'HwSchMode' (Get-Un @(1, 2, $null)) 'DWord'
    & $ajoute $DxPath 'DirectXUserGlobalSettings' (Get-Un @('VRROptimizeEnable=0;SwapEffectUpgradeEnable=1;', 'SwapEffectUpgradeEnable=0;', 'AutoHDREnable=1;', $null)) 'String'
    $vitesse = Get-Un @('0', '1', $null)
    & $ajoute $MousePath 'MouseSpeed' $vitesse 'String'
    if ($null -ne $vitesse) { & $ajoute $MousePath 'MouseThreshold1' (Get-Un @('0', '6')) 'String'; & $ajoute $MousePath 'MouseThreshold2' (Get-Un @('0', '10')) 'String' }
    foreach ($p in $AccessPaths) { & $ajoute $p 'Flags' (Get-Un @('510', '506', '126', '122', $null)) 'String' }
    & $ajoute $DoPath 'DownloadMode' (Get-Un @(0, 1, 3, $null)) 'DWord'
    & $ajoute 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'EnableTransparency' (Get-Un @(0, 1, $null)) 'DWord'
    & $ajoute 'HKLM:\SYSTEM\CurrentControlSet\Control\Power' 'HibernateEnabled' (Get-Un @(0, 1, $null)) 'DWord'
    & $ajoute 'HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity' 'Enabled' (Get-Un @(0, 1, $null)) 'DWord'
    # Écrans : « appareil|nom|largeur|hauteur|actuelle|max|principal »
    $ecrans = @(for ($i = 1; $i -le (Get-Hasard 4); $i++) {
        $max = Get-Un @(60, 75, 144, 165, 240); $cur = Get-Un @(60, $max, $max, 120, 144)
        if ($cur -gt $max) { $cur = $max }
        "\\.\DISPLAY$i|Écran générique|$(Get-Un @(1920, 2560, 3840))|$(Get-Un @(1080, 1440, 2160))|$cur|$max|$(if ($i -eq 1) { '1' } else { '0' })"
    })
    # Programmes au démarrage
    $nbDem = Get-Un @(0, 3, 8, 13, 15, 20)
    $dem = @(for ($i = 0; $i -lt $nbDem; $i++) {
        $d = $DemarragePool[$i % $DemarragePool.Count]
        $src = Get-Un @(@{ A = "HKCU:\$ApprovedRoot\Run"; L = 'Utilisateur' }, @{ A = "HKLM:\$ApprovedRoot\Run"; L = 'Tous les utilisateurs' }, @{ A = "HKCU:\$ApprovedRoot\StartupFolder"; L = 'Dossier Démarrage' })
        $etat = Get-Un @('rien', 'rien', 'actif', 'coupe', 'coupe', 'actif7')
        switch ($etat) { 'actif' { $reg["$($src.A)|$($d[0])"] = [ordered]@{ Value = @(2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0); Kind = 'Binary' } } 'coupe' { $reg["$($src.A)|$($d[0])"] = [ordered]@{ Value = @(3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0); Kind = 'Binary' } } 'actif7' { $reg["$($src.A)|$($d[0])"] = [ordered]@{ Value = @(6, 0, 0, 0); Kind = 'Binary' } } }
        [ordered]@{ Nom = $d[0]; Source = $src.L; Commande = $d[1]; Approved = $src.A; ValueName = $d[0]; Exe = (($d[1] -replace '^"([^"]+)".*$', '$1') -replace ' /.*$| -.*$', '') }
    })
    # Les jeux (portable à deux cartes) et leur préférence de carte graphique
    $jeux = @(for ($i = 0; $i -lt (Get-Un @(0, 1, 3, 8)); $i++) {
        $exes = @("C:\Jeux\Jeu$i\Jeu$i.exe") + @(if (Test-Chance 0.3) { "C:\Jeux\Jeu$i\bin\Jeu$($i)Outil.exe" })
        foreach ($e in $exes) { if (Test-Chance 0.4) { $reg["$DxPath|$e"] = [ordered]@{ Value = (Get-Un @('GpuPreference=2;', 'GpuPreference=1;')); Kind = 'String' } } }
        [ordered]@{ Name = (Get-Un @('Rocket League', 'Hollow Knight', "Jeu d'essai $i", 'Counter-Strike 2', 'Hadès')); Exes = $exes }
    })
    $plans = @(@{ Guid = $BalancedGuid; Nom = 'Utilisation normale' })
    if (Test-Chance 0.6) { $plans += @{ Guid = $HighPerfGuid; Nom = 'Performances élevées' } }
    if (Test-Chance 0.1) { $plans += @{ Guid = $UltimateGuid; Nom = 'Performances optimales' } }
    if (Test-Chance 0.1) { $plans += @{ Guid = '11111111-2222-3333-4444-555555555555'; Nom = 'Mode gaming du fabricant' } }
    $actif = (Get-Un $plans).Guid
    $ignores = @(Get-Un @(@(), @(), @('display:\\.\DISPLAY1'), @('tweak:transparency', 'ram-xmp'), @('wifi', 'startup', 'disk-space'), @('tweak:power', 'display:\\.\DISPLAY2')))
    [ordered]@{
        numero = $N; data = $data; alimBatterie = $batPower; hiberfil = (Test-Chance 0.5); registre = $reg; ecrans = $ecrans; demarrage = $dem; jeux = $jeux; ignores = $ignores
        plans = @($plans | ForEach-Object { [ordered]@{ Guid = $_.Guid; Nom = $_.Nom } }); planActif = $actif
        curseur = $(Get-Un @('', '', $BestPerfOverlay, '961cc777-2547-4f9d-8174-7d86181b8a7a'))
        uefi = (Test-Chance 0.85)
        smi = $(if (Test-Chance 0.5) { [ordered]@{ 'temperature.gpu' = (Get-Un @(41.0, 82.0, $null)); 'fan.speed' = (Get-Un @(30.0, $null)); 'power.draw' = (Get-Un @(23.45, $null)); 'power.limit' = (Get-Un @(220.0, $null, 0.0)); 'pcie.link.width.current' = (Get-Un @(16.0, 8.0, $null)); 'pcie.link.width.max' = (Get-Un @(16.0, 8.0)) } } else { $null })
        vram = $(if (Test-Chance 0.7) { $h = [ordered]@{}; foreach ($g in $gpus) { $h[$g.Name] = (Get-Un @(4GB, 8GB, 12GB)) }; $h } else { [ordered]@{} })
        cim = [ordered]@{ BatteryStaticData = $(if (Test-Chance 0.7) { [ordered]@{ DesignedCapacity = 54000 } } else { $null }); BatteryFullChargedCapacity = [ordered]@{ FullChargedCapacity = (Get-Un @(30000, 42000, 53000)) }; BatteryCycleCount = $(if (Test-Chance 0.5) { [ordered]@{ CycleCount = 312 } } else { $null }) }
        creeParNevermind = $(if (Test-Chance 0.1) { 'c0ffee00-1111-2222-3333-444455556666' } else { $null })
        mesures = $(if (Test-Chance 0.3) { [ordered]@{ Updated = $false } } else { [ordered]@{ Updated = $true; Cpu = (Get-Un @(3.2, 55.5, 93.0, $null)); RamUsed = 6.2GB; RamTotal = $(Get-Un @(16GB, 0)); Gpu = (Get-Un @(12.5, $null, 99.4)); GpuTemp = (Get-Un @(45.0, 81.4, 88.0, $null)) } })
    }
}

# Un PC simulé chargé dans le faux PC (les dates passent en [datetime], comme les objets de WMI)
function Use-PcSimule($Sc) {
    $j = $Sc | ConvertTo-Json -Depth 12 | ConvertFrom-Json
    $d = $j.data
    $d.OS.LastBootUpTime = [datetime]$d.OS.LastBootUpTime
    foreach ($g in @($d.GPUs)) { if ($g.DriverDate) { $g.DriverDate = [datetime]$g.DriverDate } }
    $d.BIOS.ReleaseDate = [datetime]$d.BIOS.ReleaseDate
    if ($d.MemDiag) { $d.MemDiag.TimeCreated = [datetime]$d.MemDiag.TimeCreated }
    $d.Battery = @($d.Battery); $d.UpsHints = @($d.UpsHints); $d.Chassis = @($d.Chassis); $d.GPUs = @($d.GPUs); $d.Mem = @($d.Mem); $d.Disks = @($d.Disks)
    $script:Data = $d
    $script:Prefetch = $null
    $script:FauxReg = @{}; foreach ($p in $j.registre.PSObject.Properties) { $script:FauxReg[$p.Name] = @{ Value = $p.Value.Value; Kind = $p.Value.Kind } }
    [OGNative]::Ecrans = [string[]]@($j.ecrans); [OGNative]::Overlay = [string]$j.curseur; [OGNative]::OverlayRefuse = $false
    $script:Demarrage = @($j.demarrage | ForEach-Object { $_ })
    $script:Jeux = @($j.jeux | ForEach-Object { [pscustomobject]@{ Name = $_.Name; Exes = @($_.Exes); Leftover = $false } })
    $script:Ignored = @($j.ignores | Where-Object { $_ })
    $script:Plans = @($j.plans | ForEach-Object { @{ Guid = $_.Guid; Nom = $_.Nom } }); $script:PlanActif = $j.planActif
    $script:BatEtat = @{}; if ($j.alimBatterie) { foreach ($p in $j.alimBatterie.PSObject.Properties) { $script:BatEtat[$p.Name] = @([int]$p.Value[0], [int]$p.Value[1]) } }
    $script:Hiberfil = [bool]$j.hiberfil
    $env:firmware_type = if ($j.uefi) { 'UEFI' } else { 'Legacy' }
    $script:Smi = if ($j.smi) { $h = @{}; foreach ($p in $j.smi.PSObject.Properties) { $h[$p.Name] = $(if ($null -ne $p.Value) { [double]$p.Value } else { $null }) }; $h } else { $null }
    $script:Vram = @{}; foreach ($p in $j.vram.PSObject.Properties) { $script:Vram[$p.Name] = $p.Value }
    $script:Cim = @{}; foreach ($p in $j.cim.PSObject.Properties) { $script:Cim[$p.Name] = $p.Value }
    $script:Backup = @{ Registry = @{}; PowerScheme = $null; CreatedScheme = $j.creeParNevermind; Overlay = $null; Dns = @{}; Displays = @{}; Oublies = @() }
    $script:Settings = @{}
    $script:FauxServices = @{}
    $m = $j.mesures
    $script:Live = @{ Updated = $m.Updated; Cpu = $m.Cpu; RamUsed = $m.RamUsed; RamTotal = $m.RamTotal; Gpu = $m.Gpu; GpuTemp = $m.GpuTemp }
    $script:Kit = @{ Fiches = @(); Constats = @(); PagesKit = @('pc', 'tableau', 'perf', 'optimisation', 'demarrage', 'nettoyage', 'memoire', 'protection', 'securite', 'connexion', 'sauvegarde', 'jeux', 'jeu', 'overlay', 'trafic', 'reseau', 'tests', 'recherche', 'parametres', 'aide') }
    $script:LastAnalysis = $null; $script:RestoreDone = $false; $script:RunLog = $null
    $script:ui = New-Ui
    $script:Gestes.Clear()
    $script:ReponseQuestion = $true; $script:ReponseRestauration = 'OK'; $script:AideRefuse = @(); $script:ParametresBloques = $false
}

# Après un changement de fréquence : la question « Tes écrans s'affichent bien ? » (sans attendre 15 s). La réponse vient
# du cas ($script:ChoixEcran : keep, revert, timeout) ; le retour en arrière est celui de Confirm-DisplayChange.
function Confirm-DisplayChange($Log) {
    $disp = @($Log | Where-Object { $_.Type -eq 'display' })
    if (-not $disp.Count) { return @() }
    Add-Geste 'ecran.question' $script:ChoixEcran
    $choice = $script:ChoixEcran
    if ($choice -eq 'keep') { return @() }
    $devices = @()
    foreach ($d in $disp) {
        [void][OGNative]::SetRefreshRate($d.Device, $d.Hz)
        [void]$Log.Remove($d)
        $devices += $d.Device
        if ($script:Ignored -notcontains "display:$($d.Device)") { $script:Ignored += "display:$($d.Device)" }
    }
    Save-Ignored
    $devices
}

# ---------------------------------------------------------------------------
# Ce qui est gardé dans les juges
# ---------------------------------------------------------------------------
function Get-SorteRun($Run) {
    if (-not $Run) { return '' }
    $t = "$Run"
    if ($t -match 'Set-Declarations') { 'declarations' } elseif ($t -match 'Invoke-TweakApply') { 'tweak' } elseif ($t -match 'Invoke-CleanAll') { 'nettoyage' }
    elseif ($t -match 'mdsched') { 'mdsched' } elseif ($t -match 'shutdown') { 'bios' } else { "autre : $t" }
}
function ConvertTo-JugeValeur($V) {
    if ($V -is [byte[]]) { return @([int[]]$V) }
    if ($V -is [array]) { return @($V | ForEach-Object { ConvertTo-JugeValeur $_ }) }
    if ($V -is [hashtable] -or $V -is [System.Collections.Specialized.OrderedDictionary]) { $o = [ordered]@{}; foreach ($k in @($V.Keys | Sort-Object)) { $o[[string]$k] = ConvertTo-JugeValeur $V[$k] }; return $o }
    if ($V -is [scriptblock]) { return "{$V}" }
    $V
}
function ConvertTo-JugeConstat($F) {
    $fix = $F.Fix
    [ordered]@{ Id = $F.Id; Status = $F.Status; Titre = $F.Titre; Detail = $F.Detail; Weight = $F.Weight; Action = $F.Action; ActionLabel = $F.ActionLabel; Gain = $F.Gain
        Fix = $(if ($fix) {
            [ordered]@{ Auto = $fix.Auto; What = @($fix.What | Where-Object { $null -ne $_ }); Why = $fix.Why; Steps = @($fix.Steps | Where-Object { $null -ne $_ }); Run = (Get-SorteRun $fix.Run); RunLabel = $fix.RunLabel
                Confirm = $fix.Confirm; Done = $fix.Done; NoRescan = $fix.NoRescan; Open = $fix.Open; OpenLabel = $fix.OpenLabel; Reboot = $fix.Reboot; Restore = $fix.Restore
                Changes = @(Resolve-Declarations $fix.Changes | ForEach-Object { ConvertTo-JugeValeur $_ }); Tweak = $(if ($fix.Args -is [hashtable] -and $fix.Args.Id) { [string]$fix.Args.Id } else { '' }) }
        } else { $null }) }
}
function ConvertTo-JugeCarte($C) {
    [ordered]@{ Tag = $C.Tag; Titre = $C.Titre; Sous = $C.Sous; Status = $C.Status
        Bars = @($C.Bars | ForEach-Object { [ordered]@{ Label = $_.Label; Value = [double]$_.Value; Text = $_.Text; Color = $_.Color } })
        Lines = @(foreach ($k in $C.Lines.Keys) { $v = $C.Lines[$k]; if ($v -is [array]) { [ordered]@{ Cle = [string]$k; Texte = [string]$v[0]; Couleur = [string]$v[1] } } else { [ordered]@{ Cle = [string]$k; Texte = [string]$v; Couleur = $null } } })
        Notes = @($C.Notes | ForEach-Object { [ordered]@{ Status = $_.Status; Text = $_.Text } }); Action = $C.Action; ActionLabel = $C.ActionLabel }
}
function ConvertTo-JugeAnalyse($A) {
    [ordered]@{ Info = $A.Info; Cards = @($A.Cards | ForEach-Object { ConvertTo-JugeCarte $_ }); Active = @($A.Active | ForEach-Object { ConvertTo-JugeConstat $_ })
        Score = $A.Score; Potential = $A.Potential; Label = $A.Label; Color = $A.Color; Sante = [string]$ui.HealthSummary.Text
        Ignores = @($script:IgnoredFindings | ForEach-Object { $_.Id }); Portable = [bool]$script:IsLaptop; Onduleur = [bool]$script:HasUps; Build = $script:Build }
}
function Get-Copie($O) { if ($null -eq $O) { return $null }; ConvertTo-JugeValeur $O }
function Get-EtatFaux {
    [ordered]@{ Backup = (Get-Copie $script:Backup); Settings = (Get-Copie $script:Settings); Ignores = @($script:Ignored); Registre = (Get-Copie $script:FauxReg); PlanActif = $script:PlanActif; Curseur = [OGNative]::Overlay; Ecrans = @([OGNative]::Ecrans); Batterie = (Get-Copie $script:BatEtat); Services = (Get-Copie $script:FauxServices) }
}
function Get-Gestes { $g = @($script:Gestes | ForEach-Object { [ordered]@{ geste = $_.geste; donnees = (ConvertTo-JugeValeur $_.donnees) } }); $script:Gestes.Clear(); , $g }

# ---------------------------------------------------------------------------
# 1. Les analyses, les pages et les fiches
# ---------------------------------------------------------------------------
$cas = New-Object System.Collections.ArrayList
for ($n = 0; $n -lt $Nombre; $n++) {
    $sc = New-PcSimule $n
    Use-PcSimule $sc
    Invoke-Analysis
    $a = $script:LastAnalysis
    $pages = Get-PagesPc
    $hub = Get-HubFindings $a
    $fiches = New-Object System.Collections.ArrayList
    if ($n -lt 60) {
        for ($k = 0; $k -lt @($script:Kit.Constats).Count; $k++) { Open-Sheet @($script:Kit.Constats[$k]); [void]$fiches.Add((Get-FicheKit)) }
    }
    $script:ui.Overlay.Visibility = 'Collapsed'
    Open-FixAll
    $tout = if ($script:ui.Overlay.Visibility -eq 'Visible') { Get-FicheKit } else { $null }
    $gestesCas = Get-Gestes
    [void]$cas.Add([ordered]@{ entree = $sc; sortie = [ordered]@{ analyse = (ConvertTo-JugeAnalyse $a); pages = $pages
        hub = [ordered]@{ Important = @($hub.Important | ForEach-Object { $_.Id }); Manual = @($hub.Manual | ForEach-Object { $_.Id }) }
        fiches = @($fiches); toutCorriger = $tout; gestes = $gestesCas } })
}
$json = ConvertTo-Json -InputObject @($cas) -Depth 40 -Compress
[IO.File]::WriteAllText((Join-Path $sortie 'analyse.json'), $json, (New-Object Text.UTF8Encoding($false)))
Write-Host "analyse.json : $($cas.Count) PC simulés, $([math]::Round($json.Length / 1MB, 1)) Mo"

# ---------------------------------------------------------------------------
# 2. « Tout corriger », « Revenir en arrière » et « Tout restaurer » : les gestes exacts, sur le faux PC
# ---------------------------------------------------------------------------
# Les variantes : ce que répond le faux PC
function Set-Variante([int]$V) {
    $script:ChoixEcran = @('keep', 'revert', 'timeout')[$V % 3]
    switch ($V % 7) {
        1 { $script:ReponseRestauration = 'recent' }
        2 { $script:AideRefuse = @('restauration.creer'); $script:ReponseQuestion = $false }
        3 { $script:AideRefuse = @('restauration.creer'); $script:ReponseQuestion = $true }
        4 { $script:AideRefuse = @('reg.ecrire') }
        5 { [OGNative]::OverlayRefuse = $true }
        6 { $script:ParametresBloques = $true }
    }
}
$corr = New-Object System.Collections.ArrayList
for ($n = 0; $n -lt $NombreCorrections; $n++) {
    $sc = New-PcSimule (5000 + $n)
    # (à régler : l'enregistrement de la Game Bar toujours actif, souvent un écran bridé, un onduleur mal réglé, un disque plein)
    foreach ($k in @($sc.registre.Keys)) { if ($k -like '*GameDVR*') { $sc.registre[$k].Value = 1 } }
    if ($n % 2) { $sc.ecrans = @('\\.\DISPLAY1|Écran générique|2560|1440|60|165|1') + @($sc.ecrans | Select-Object -Skip 1) }
    if ($n % 3 -eq 0) { $sc.alimBatterie = [ordered]@{ CritAction = @(0, 1); CritLevel = @(5, 5); LowLevel = @(10, 10); LowNotify = @(1, 0); LowAction = @(0, 1) }; $sc.data.Battery = @([ordered]@{ Name = 'Back-UPS BX700U FW:'; DeviceID = 'UPS1'; Description = ''; Chemistry = 3; BatteryStatus = 2; EstimatedChargeRemaining = 100; EstimatedRunTime = 12 }); $sc.data.Chassis = @(3); $sc.data.PCType = 1 }
    if ($n % 4 -eq 1) { $sc.data.LogicalC = [ordered]@{ Size = [double]500GB; FreeSpace = [double]20GB } }
    $etapes = [ordered]@{}
    # a. Tout corriger, puis Revenir en arrière
    Use-PcSimule $sc; Set-Variante $n
    Invoke-Analysis
    [void](Get-Gestes)
    $script:ui.Overlay.Visibility = 'Collapsed'
    Open-FixAll
    $etapes.ficheTout = if ($script:ui.Overlay.Visibility -eq 'Visible') { Get-FicheKit } else { $null }
    $etapes.items = @($script:SheetItems | ForEach-Object { $_.Id })
    if ($script:ui.Overlay.Visibility -eq 'Visible') {
        Invoke-Safe { Invoke-SheetRun }
        $etapes.corriger = [ordered]@{ gestes = (Get-Gestes); journal = @(Get-Copie $script:ResultLog); etat = (Get-EtatFaux); score = $script:LastAnalysis.Score; fiche = $(if ($script:ui.Overlay.Visibility -eq 'Visible') { Get-FicheKit } else { $null }); statut = $script:DernierStatut }
        if ($script:SheetMode -eq 'result') {
            Invoke-Safe { Invoke-UndoLastRun }
            $etapes.annuler = [ordered]@{ gestes = (Get-Gestes); etat = (Get-EtatFaux); score = $script:LastAnalysis.Score; fiche = $(if ($script:ui.Overlay.Visibility -eq 'Visible') { Get-FicheKit } else { $null }) }
        }
    }
    # b. Tout corriger, puis Tout restaurer
    Use-PcSimule $sc; Set-Variante $n
    Invoke-Analysis
    $script:ui.Overlay.Visibility = 'Collapsed'
    Open-FixAll
    if ($script:ui.Overlay.Visibility -eq 'Visible') {
        Invoke-Safe { Invoke-SheetRun }
        [void](Get-Gestes)
        $script:AideRefuse = @(if ($n % 5 -eq 2) { 'service.regler' } elseif ($n % 5 -eq 3) { 'origines.oublier' })
        $err = @(Restore-AllSettings)
        $etapes.restaurer = [ordered]@{ gestes = (Get-Gestes); erreurs = @($err); etat = (Get-EtatFaux); compte = (Get-BackupCount) }
    }
    [void]$corr.Add([ordered]@{ entree = $sc; variante = $n; sortie = $etapes })
}
$json = ConvertTo-Json -InputObject @($corr) -Depth 40 -Compress
[IO.File]::WriteAllText((Join-Path $sortie 'corrections.json'), $json, (New-Object Text.UTF8Encoding($false)))
Write-Host "corrections.json : $($corr.Count) PC simulés, $([math]::Round($json.Length / 1MB, 1)) Mo"
