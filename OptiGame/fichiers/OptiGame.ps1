#Requires -Version 5.1
<#
    OptiGame 1.0.11
    Analyse et optimisation gaming pour Windows 10 et 11.

    Chaque réglage modifié est sauvegardé dans %LOCALAPPDATA%\OptiGame\sauvegarde.json
    et peut être annulé depuis l'onglet Sauvegarde.

    OptiGame.ps1 -Uninstall  remet les réglages comme avant et supprime l'application.
#>
param([switch]$Uninstall)

$AppVersion = '1.0.11'
$UpdateRepo = 'JordanJacquot/OptiGame'   # dépôt GitHub où sont publiées les mises à jour

# ---------------------------------------------------------------------------
# Droits administrateur
# ---------------------------------------------------------------------------
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase, System.Drawing, System.Windows.Forms

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    try {
        Start-Process -FilePath 'powershell.exe' -Verb RunAs -ErrorAction Stop -ArgumentList @(
            @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', "`"$PSCommandPath`"") + @(if ($Uninstall) { '-Uninstall' }))
    } catch {
        [System.Windows.MessageBox]::Show(
            "OptiGame a besoin des droits administrateur pour modifier les réglages de Windows.`n`nRelance l'application et clique sur « Oui » quand Windows le demande.",
            'OptiGame', 'OK', 'Warning') | Out-Null
    }
    exit
}

# Retire la marque « téléchargé depuis Internet » des fichiers d'OptiGame, pour que
# Windows n'affiche plus d'avertissement aux lancements suivants.
try {
    $appRoot = if ((Split-Path $PSScriptRoot -Leaf) -eq 'fichiers') { Split-Path $PSScriptRoot -Parent } else { $PSScriptRoot }
    @(Get-ChildItem -LiteralPath $PSScriptRoot -File -Recurse -ErrorAction SilentlyContinue) +
    @(Get-ChildItem -LiteralPath $appRoot -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '^(OptiGame\.exe|Désinstaller OptiGame\.exe|LISEZMOI\.txt)$' }) |
        Unblock-File -ErrorAction SilentlyContinue
} catch {}

# ---------------------------------------------------------------------------
# Chargement des modules (dossier « modules » à côté de ce fichier)
# ---------------------------------------------------------------------------
$AppDir = $PSScriptRoot
$ModulesDir = Join-Path $AppDir 'modules'
$missing = @('natif.cs', 'interface.xaml', 'donnees.ps1', 'optimisations.ps1', 'systeme.ps1', 'interface.ps1', 'tableau-de-bord.ps1', 'analyse.ps1', 'onglets.ps1', 'visuels.ps1', 'tests.ps1', 'securite.ps1', 'navigation.ps1', 'reseau.ps1', 'audit-reseau.ps1', 'mises-a-jour.ps1', 'evenements.ps1' | Where-Object { -not (Test-Path -LiteralPath (Join-Path $ModulesDir $_)) })
if ($missing) {
    [System.Windows.MessageBox]::Show("Des fichiers d'OptiGame sont manquants :`n`n$($missing -join ', ')`n`nRetélécharge OptiGame et remplace tout le dossier.", 'OptiGame', 'OK', 'Error') | Out-Null
    exit
}

# Fonctions natives (écrans, souris, barre de titre sombre)
Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $ModulesDir 'natif.cs'), [Text.Encoding]::UTF8))

foreach ($ogModule in 'donnees', 'optimisations', 'systeme') { . (Join-Path $ModulesDir "$ogModule.ps1") }

if ($Uninstall) {
    Invoke-Uninstall
    exit
}

foreach ($ogModule in 'interface', 'tableau-de-bord', 'analyse', 'onglets', 'visuels', 'tests', 'securite', 'navigation', 'reseau', 'audit-reseau', 'mises-a-jour', 'evenements') { . (Join-Path $ModulesDir "$ogModule.ps1") }

# ---------------------------------------------------------------------------
# Lancement
# ---------------------------------------------------------------------------
Import-Backup
Import-Ignored
$script:KnownDevices = @{}
$script:RestoreDone = $false
$script:Build = [int](Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' -ErrorAction SilentlyContinue).CurrentBuildNumber
$script:Pool = [RunspaceFactory]::CreateRunspacePool(1, 4)
$script:Pool.Open()
$pfPs = [PowerShell]::Create()
$pfPs.RunspacePool = $script:Pool
[void]$pfPs.AddScript($AnalysisDataWork.ToString()).AddArgument($env:SystemDrive)
$script:Prefetch = @{ PS = $pfPs; Handle = $pfPs.BeginInvoke() }
$script:TweakRows = @()
$script:CleanRows = @()
$script:PingResults = @()
Write-Log "Démarrage OptiGame $AppVersion"
[void]$Window.ShowDialog()
if ($script:Relaunch -and (Test-Path -LiteralPath $script:Relaunch)) {
    Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', "`"$script:Relaunch`"")
}
