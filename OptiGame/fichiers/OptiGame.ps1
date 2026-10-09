#Requires -Version 5.1
<#
    Nevermind 1.0.64
    Analyse et optimisation gaming pour Windows 10 et 11.

    Chaque réglage modifié est sauvegardé dans %LOCALAPPDATA%\OptiGame\sauvegarde.json
    et peut être annulé depuis l'onglet Sauvegarde.

    OptiGame.ps1 -Demarrage  s'ouvre réduit près de l'horloge (lancement avec Windows).
    OptiGame.ps1 -Uninstall  remet les réglages comme avant et supprime l'application.
#>
param([switch]$Uninstall, [switch]$Demarrage)   # -Demarrage : lancé avec Windows, réduit près de l'horloge

$AppVersion = '1.0.64'
$UpdateRepo = 'JordanJacquot/Nevermind'   # dépôt GitHub où sont publiées les mises à jour

# ---------------------------------------------------------------------------
# Droits administrateur
# ---------------------------------------------------------------------------
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase, System.Drawing, System.Windows.Forms

# ---------------------------------------------------------------------------
# Une seule fenêtre : si Nevermind tourne déjà (même caché près de l'horloge), on le ramène devant
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
    if ($alreadyRunning -and $Demarrage) { exit }   # déjà ouvert : le lancement avec Windows n'a rien à faire
    if ($alreadyRunning) {
        Send-ShowRequest
        # Relance après une mise à jour : l'ancienne version est en train de se fermer. On attend un peu :
        # si elle disparaît sans avoir pris la demande, c'est à ce lancement-ci d'ouvrir l'app.
        $gone = $false
        for ($i = 0; $i -lt 25 -and [IO.File]::Exists($ShowRequest); $i++) {
            Start-Sleep -Milliseconds 200
            try { $probe = [System.Threading.Mutex]::OpenExisting('Local\OptiGame-Instance'); $probe.Dispose() }
            catch {
                $inner = $_.Exception; while ($inner.InnerException) { $inner = $inner.InnerException }
                if ($inner -isnot [System.UnauthorizedAccessException]) { $gone = $true; break }
            }
        }
        if (-not $gone) { exit }
        try { [IO.File]::Delete($ShowRequest) } catch {}
    }
}

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    try {
        Start-Process -FilePath 'powershell.exe' -Verb RunAs -ErrorAction Stop -ArgumentList @(
            @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', "`"$PSCommandPath`"") + @(if ($Uninstall) { '-Uninstall' }))
    } catch {
        [System.Windows.MessageBox]::Show(
            "Nevermind a besoin des droits administrateur pour modifier les réglages de Windows.`n`nRelance l'application et clique sur « Oui » quand Windows le demande.",
            'Nevermind', 'OK', 'Warning') | Out-Null
    }
    exit
}

# Instance unique (lancement direct en administrateur, ou deux clics très rapprochés)
if (-not $Uninstall -and -not $env:OPTIGAME_TEST) {
    $mutexNew = $false
    $script:InstanceMutex = New-Object System.Threading.Mutex($true, 'Local\OptiGame-Instance', [ref]$mutexNew)
    if (-not $mutexNew) {
        # Ancienne version encore en train de se fermer (relance après une mise à jour) : elle libère la place sous peu
        $got = $false
        try { $got = $script:InstanceMutex.WaitOne(3000) } catch [System.Threading.AbandonedMutexException] { $got = $true }
        if (-not $got) { if (-not $Demarrage) { Send-ShowRequest }; exit }
    }
    try { if (Test-Path -LiteralPath $ShowRequest) { [IO.File]::Delete($ShowRequest) } } catch {}
}

# Retire la marque « téléchargé depuis Internet » des fichiers de Nevermind, pour que
# Windows n'affiche plus d'avertissement aux lancements suivants.
try {
    $appRoot = if ((Split-Path $PSScriptRoot -Leaf) -eq 'fichiers') { Split-Path $PSScriptRoot -Parent } else { $PSScriptRoot }
    @(Get-ChildItem -LiteralPath $PSScriptRoot -File -Recurse -ErrorAction SilentlyContinue) +
    @(Get-ChildItem -LiteralPath $appRoot -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '^(Nevermind\.exe|Désinstaller Nevermind\.exe|OptiGame\.exe|Désinstaller OptiGame\.exe|LISEZMOI\.txt)$' }) |
        Unblock-File -ErrorAction SilentlyContinue
} catch {}

# ---------------------------------------------------------------------------
# Écran de chargement, affiché pendant que l'app se prépare
# ---------------------------------------------------------------------------
$Splash = $null
if (-not $env:OPTIGAME_TEST -and -not $Uninstall -and -not $Demarrage) {
    try {
        $Splash = New-Object System.Windows.Window
        $Splash.WindowStyle = 'None'; $Splash.ResizeMode = 'NoResize'; $Splash.WindowStartupLocation = 'CenterScreen'
        $Splash.Width = 360; $Splash.Height = 190; $Splash.Title = 'Nevermind'
        $Splash.Background = [System.Windows.Media.BrushConverter]::new().ConvertFromString('#110F19')
        $Splash.BorderBrush = [System.Windows.Media.BrushConverter]::new().ConvertFromString('#2E2843'); $Splash.BorderThickness = 1
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
        foreach ($t in @(@('Nevermind', 22, '#FFFFFF', 'Bold'), @('Préparation de ton tableau de bord...', 13, '#A6A1BC', 'Normal'))) {
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
$missing = @('natif.cs', 'interface.xaml', 'donnees.ps1', 'optimisations.ps1', 'systeme.ps1', 'themes.ps1', 'interface.ps1', 'tableau-de-bord.ps1', 'analyse.ps1', 'onglets.ps1', 'visuels.ps1', 'tests.ps1', 'securite.ps1', 'navigation.ps1', 'reseau.ps1', 'reseau-avance.ps1', 'carte-reseau.ps1', 'audit-reseau.ps1', 'mises-a-jour.ps1', 'assistance.ps1', 'jeu.ps1', 'diagnostic-fps.ps1', 'trafic.ps1', 'microsoft.ps1', 'lag.ps1', 'bibliotheque.ps1', 'parametres.ps1', 'recherche.ps1', 'evenements.ps1' | Where-Object { -not (Test-Path -LiteralPath (Join-Path $ModulesDir $_)) })
if ($missing) {
    [System.Windows.MessageBox]::Show("Des fichiers de Nevermind sont manquants :`n`n$($missing -join ', ')`n`nRetélécharge Nevermind et remplace tout le dossier.", 'OptiGame', 'OK', 'Error') | Out-Null
    exit
}

# Fonctions natives (écrans, souris, barre de titre sombre)
Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $ModulesDir 'natif.cs'), [Text.Encoding]::UTF8))

foreach ($ogModule in 'donnees', 'optimisations', 'systeme', 'themes') { . (Join-Path $ModulesDir "$ogModule.ps1") }

if ($Uninstall) {
    Invoke-Uninstall
    exit
}

foreach ($ogModule in 'interface', 'tableau-de-bord', 'analyse', 'onglets', 'visuels', 'tests', 'securite', 'navigation', 'reseau', 'reseau-avance', 'carte-reseau', 'audit-reseau', 'mises-a-jour', 'assistance', 'jeu', 'diagnostic-fps', 'trafic', 'microsoft', 'lag', 'bibliotheque', 'parametres', 'recherche', 'evenements') { . (Join-Path $ModulesDir "$ogModule.ps1") }

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
# Lancé avec Windows : la fenêtre se prépare sans s'afficher, puis reste près de l'horloge
$script:StartHidden = [bool]$Demarrage
if ($script:StartHidden) { $Window.ShowInTaskbar = $false; $Window.ShowActivated = $false; $Window.WindowState = 'Minimized' }
Write-Log "Démarrage Nevermind $AppVersion (Windows build $($script:Build), langue $((Get-UICulture).Name), PowerShell $($PSVersionTable.PSVersion))$(if ($Demarrage) { ', lancé avec Windows' })"
$Window.Show()
[System.Windows.Threading.Dispatcher]::Run()
if ($script:Relaunch -and (Test-Path -LiteralPath $script:Relaunch)) {
    # Libère la place avant de relancer, sinon la nouvelle version croirait que Nevermind est déjà ouvert
    if ($script:InstanceMutex) { try { $script:InstanceMutex.ReleaseMutex() } catch {}; $script:InstanceMutex.Dispose(); $script:InstanceMutex = $null }
    Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', "`"$script:Relaunch`"")
}
