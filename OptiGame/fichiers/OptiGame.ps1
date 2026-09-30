#Requires -Version 5.1
<#
    OptiGame 1.0.42
    Analyse et optimisation gaming pour Windows 10 et 11.

    Chaque réglage modifié est sauvegardé dans %LOCALAPPDATA%\OptiGame\sauvegarde.json
    et peut être annulé depuis l'onglet Sauvegarde.

    OptiGame.ps1 -Uninstall  remet les réglages comme avant et supprime l'application.
#>
param([switch]$Uninstall)

$AppVersion = '1.0.42'
$UpdateRepo = 'JordanJacquot/OptiGame'   # dépôt GitHub où sont publiées les mises à jour

# ---------------------------------------------------------------------------
# Droits administrateur
# ---------------------------------------------------------------------------
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase, System.Drawing, System.Windows.Forms

# ---------------------------------------------------------------------------
# Une seule fenêtre : si OptiGame tourne déjà (même caché près de l'horloge), on le ramène devant
# ---------------------------------------------------------------------------
# La fenêtre ouverte a les droits administrateur : ce lancement-ci ne peut pas lui envoyer de signal
# direct, il dépose donc une « demande d'affichage » qu'elle surveille.
$ShowRequest = Join-Path $env:LOCALAPPDATA 'OptiGame\afficher.demande'
function Send-ShowRequest {
    try {
        New-Item -ItemType Directory -Force -Path (Split-Path $ShowRequest) | Out-Null
        [IO.File]::WriteAllText($ShowRequest, [string]$PID)
        # Autorise la fenêtre ouverte à passer devant (ce lancement vient d'un clic, il en a le droit)
        Add-Type -Namespace OG -Name Fg -MemberDefinition '[DllImport("user32.dll")] public static extern bool AllowSetForegroundWindow(int pid);'
        [void][OG.Fg]::AllowSetForegroundWindow(-1)
    } catch {}
}
if (-not $Uninstall -and -not $env:OPTIGAME_TEST) {
    $alreadyRunning = $false
    try { $probe = [System.Threading.Mutex]::OpenExisting('Local\OptiGame-Instance'); $probe.Dispose(); $alreadyRunning = $true }
    catch {
        # Accès refusé = elle existe, mais appartient à la fenêtre lancée en administrateur
        $inner = $_.Exception; while ($inner.InnerException) { $inner = $inner.InnerException }
        if ($inner -is [System.UnauthorizedAccessException]) { $alreadyRunning = $true }
    }
    if ($alreadyRunning) { Send-ShowRequest; exit }
}

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

# Instance unique (lancement direct en administrateur, ou deux clics très rapprochés)
if (-not $Uninstall -and -not $env:OPTIGAME_TEST) {
    $mutexNew = $false
    $script:InstanceMutex = New-Object System.Threading.Mutex($true, 'Local\OptiGame-Instance', [ref]$mutexNew)
    if (-not $mutexNew) { Send-ShowRequest; exit }
    try { if (Test-Path -LiteralPath $ShowRequest) { [IO.File]::Delete($ShowRequest) } } catch {}
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
# Écran de chargement, affiché pendant que l'app se prépare
# ---------------------------------------------------------------------------
$Splash = $null
if (-not $env:OPTIGAME_TEST -and -not $Uninstall) {
    try {
        $Splash = New-Object System.Windows.Window
        $Splash.WindowStyle = 'None'; $Splash.ResizeMode = 'NoResize'; $Splash.WindowStartupLocation = 'CenterScreen'
        $Splash.Width = 360; $Splash.Height = 190; $Splash.Title = 'OptiGame'
        $Splash.Background = [System.Windows.Media.BrushConverter]::new().ConvertFromString('#12151B')
        $Splash.BorderBrush = [System.Windows.Media.BrushConverter]::new().ConvertFromString('#2C3342'); $Splash.BorderThickness = 1
        $sp = New-Object System.Windows.Controls.StackPanel
        $sp.VerticalAlignment = 'Center'; $sp.HorizontalAlignment = 'Center'
        $ico = Join-Path $PSScriptRoot 'OptiGame.ico'
        if (Test-Path -LiteralPath $ico) {
            $frames = [System.Windows.Media.Imaging.BitmapDecoder]::Create((New-Object IO.MemoryStream (, [IO.File]::ReadAllBytes($ico))), 'None', 'OnLoad').Frames
            $img = New-Object System.Windows.Controls.Image
            $img.Source = $frames | Sort-Object PixelWidth | Where-Object { $_.PixelWidth -ge 64 } | Select-Object -First 1
            $img.Width = 56; $img.Height = 56; $img.Margin = '0,0,0,12'
            [void]$sp.Children.Add($img)
        }
        foreach ($t in @(@('OptiGame', 22, '#FFFFFF', 'Bold'), @('Préparation de ton tableau de bord...', 13, '#9AA3B2', 'Normal'))) {
            $tb = New-Object System.Windows.Controls.TextBlock
            $tb.Text = $t[0]; $tb.FontSize = $t[1]; $tb.FontWeight = $t[3]; $tb.HorizontalAlignment = 'Center'
            $tb.Foreground = [System.Windows.Media.BrushConverter]::new().ConvertFromString($t[2])
            [void]$sp.Children.Add($tb)
        }
        $Splash.Content = $sp
        $Splash.Show()
        [System.Windows.Threading.Dispatcher]::CurrentDispatcher.Invoke([action]{}, 'Background')
    } catch { $Splash = $null }
}

# ---------------------------------------------------------------------------
# Chargement des modules (dossier « modules » à côté de ce fichier)
# ---------------------------------------------------------------------------
$AppDir = $PSScriptRoot
$ModulesDir = Join-Path $AppDir 'modules'
$missing = @('natif.cs', 'interface.xaml', 'donnees.ps1', 'optimisations.ps1', 'systeme.ps1', 'interface.ps1', 'tableau-de-bord.ps1', 'analyse.ps1', 'onglets.ps1', 'visuels.ps1', 'tests.ps1', 'securite.ps1', 'navigation.ps1', 'reseau.ps1', 'reseau-avance.ps1', 'carte-reseau.ps1', 'audit-reseau.ps1', 'mises-a-jour.ps1', 'assistance.ps1', 'jeu.ps1', 'diagnostic-fps.ps1', 'trafic.ps1', 'microsoft.ps1', 'lag.ps1', 'evenements.ps1' | Where-Object { -not (Test-Path -LiteralPath (Join-Path $ModulesDir $_)) })
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

foreach ($ogModule in 'interface', 'tableau-de-bord', 'analyse', 'onglets', 'visuels', 'tests', 'securite', 'navigation', 'reseau', 'reseau-avance', 'carte-reseau', 'audit-reseau', 'mises-a-jour', 'assistance', 'jeu', 'diagnostic-fps', 'trafic', 'microsoft', 'lag', 'evenements') { . (Join-Path $ModulesDir "$ogModule.ps1") }

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
# Tâches de fond longues (annuaire des serveurs, tracé du chemin, signatures) : à part, pour ne pas faire attendre la fenêtre
$script:BgPool = [RunspaceFactory]::CreateRunspacePool(1, 8)
$script:BgPool.Open()
$pfPs = [PowerShell]::Create()
$pfPs.RunspacePool = $script:Pool
[void]$pfPs.AddScript($AnalysisDataWork.ToString()).AddArgument($env:SystemDrive)
$script:Prefetch = @{ PS = $pfPs; Handle = $pfPs.BeginInvoke() }
$script:TweakRows = @()
$script:CleanRows = @()
$script:PingResults = @()
Write-Log "Démarrage OptiGame $AppVersion (Windows build $($script:Build), langue $((Get-UICulture).Name), PowerShell $($PSVersionTable.PSVersion))"
$Window.Show()
[System.Windows.Threading.Dispatcher]::Run()
if ($script:Relaunch -and (Test-Path -LiteralPath $script:Relaunch)) {
    Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', "`"$script:Relaunch`"")
}
