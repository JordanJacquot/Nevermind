#Requires -Version 5.1
<#
    Nevermind 1.0.120
    Analyse et optimisation gaming pour Windows 10 et 11.

    Chaque réglage modifié est sauvegardé dans %LOCALAPPDATA%\OptiGame\sauvegarde.json
    (%LOCALAPPDATA%\Mindset\Nevermind quand Nevermind est géré par Mindset)
    et peut être annulé depuis l'onglet Sauvegarde.

    OptiGame.ps1 -Demarrage  s'ouvre réduit près de l'horloge (lancement avec Windows).
    OptiGame.ps1 -Uninstall  remet les réglages comme avant et supprime l'application.
#>
param([switch]$Uninstall, [switch]$Demarrage)   # -Demarrage : lancé avec Windows, réduit près de l'horloge

$AppVersion = '1.0.120'
$UpdateRepo = 'JordanJacquot/Nevermind'   # dépôt GitHub où sont publiées les mises à jour

# ---------------------------------------------------------------------------
# Mindset (le portail de la suite) et dossier des données
# ---------------------------------------------------------------------------
# Mindset écrit %LOCALAPPDATA%\Mindset\suite.json : le thème de la suite et le dossier de chaque appli qu'il gère.
# Nevermind est « géré par Mindset » quand le dossier noté pour lui est celui de cette copie : ses données vont
# alors dans %LOCALAPPDATA%\Mindset\Nevermind, il suit le thème de la suite, et Mindset s'occupe des mises à jour,
# du raccourci et du lancement au démarrage. Sinon, rien ne change.
# Sécurité : ce fichier est modifiable par n'importe quel programme de
# l'utilisateur. On n'y accepte qu'un nom de thème connu, et le dossier n'y sert qu'à comparer avec le nôtre.
$SuiteFile = Join-Path $env:LOCALAPPDATA 'Mindset\suite.json'
if ($env:OPTIGAME_TEST -and $env:OPTIGAME_SUITE) { $SuiteFile = $env:OPTIGAME_SUITE }   # la copie de test peut simuler Mindset
$SuiteThemes = @('neon', 'crepuscule', 'terminal', 'arcade', 'rubis')

# Chemin complet sans « \ » final, pour comparer deux dossiers (vide s'il n'est pas valable)
function Get-FullDir([string]$Path) {
    try { if ($Path -and [IO.Path]::IsPathRooted($Path)) { return [IO.Path]::GetFullPath($Path).TrimEnd('\') } } catch {}
    ''
}

# Ce qui nous concerne dans suite.json : le thème (un des 5 thèmes, ou « pack:<id> » : un pack de la suite que Mindset
# installe dans les packs de Nevermind ; l'id ne sert qu'à nommer ce dossier, d'où la liste blanche stricte) et le
# dossier de Nevermind
function Read-Suite {
    $s = @{ Theme = $null; Dir = '' }
    try {
        if (-not [IO.File]::Exists($SuiteFile)) { return $s }
        $j = [IO.File]::ReadAllText($SuiteFile, [Text.Encoding]::UTF8) | ConvertFrom-Json
        if ($j.theme -is [string] -and ($j.theme -cin $SuiteThemes -or $j.theme -cmatch '\Apack:[a-z0-9-]{1,40}\z')) { $s.Theme = [string]$j.theme }
        if ($j.applis -and $j.applis.nevermind -is [string]) { $s.Dir = Get-FullDir $j.applis.nevermind }
    } catch {}
    $s
}

$SuiteStamp = try { [IO.File]::GetLastWriteTimeUtc($SuiteFile) } catch { [datetime]::MinValue }   # surveillé ensuite (evenements.ps1)
$Suite = Read-Suite
$Managed = [bool]($Suite.Dir -and $Suite.Dir -eq (Get-FullDir (Split-Path $PSScriptRoot -Parent)))   # -eq ignore la casse

# Géré par Mindset, Nevermind ne s'affiche que dans Mindset. Lancé à la main (Mindset, lui, le lance caché avec -Demarrage
# puis l'accroche dans son onglet), ou une fenêtre à montrer : Mindset s'ouvre sur l'onglet de Nevermind.
# Mindset vient du registre (clé de désinstallation, écrite par un administrateur), jamais d'un fichier modifiable, et il
# est lancé par l'Explorateur, sans les droits de Nevermind. $false si Mindset est introuvable (Nevermind s'ouvre seul).
function Open-DansMindset {
    $dir = ''
    try { $dir = ([string](Get-ItemProperty -LiteralPath 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Mindset' -Name InstallLocation -ErrorAction Stop).InstallLocation).Trim().Trim('"') } catch {}
    $exe = if ($dir -and [IO.Path]::IsPathRooted($dir)) { Join-Path $dir 'Mindset.exe' } else { '' }
    if (-not $exe -or -not (Test-Path -LiteralPath $exe -PathType Leaf)) { return $false }
    try {
        [void][IO.Directory]::CreateDirectory((Split-Path $SuiteFile))
        [IO.File]::WriteAllText((Join-Path (Split-Path $SuiteFile) 'afficher-page.txt'), 'appli:nevermind', (New-Object Text.UTF8Encoding $false))
        Start-Process -FilePath (Join-Path $env:windir 'explorer.exe') -ArgumentList "`"$exe`"" -ErrorAction Stop
        $true
    } catch { $false }
}
if ($Managed -and -not $Demarrage -and -not $Uninstall -and -not $env:OPTIGAME_TEST) { if (Open-DansMindset) { exit } }

# Nevermind déjà ouvert ? (accès refusé au verrou = il existe, mais appartient à la fenêtre lancée en administrateur)
# Ligne du journal écrite avant le chargement des modules (Write-Log n'existe pas encore) : un lancement abandonné
# doit laisser une trace, sinon Mindset affiche seulement « n'a pas pu s'ouvrir »
function Write-LaunchNote([string]$Text) {
    try { [IO.File]::AppendAllText((Join-Path $DataDir 'journal.txt'), "$(Get-Date -Format s) $Text (processus $PID)`r`n", (New-Object Text.UTF8Encoding($false))) } catch {}
}

function Test-InstanceRunning {
    try { $probe = [System.Threading.Mutex]::OpenExisting('Local\OptiGame-Instance'); $probe.Dispose(); return $true }
    catch {
        $inner = $_.Exception; while ($inner.InnerException) { $inner = $inner.InnerException }
        return ($inner -is [System.UnauthorizedAccessException])
    }
}

# Dossier des données. Copie gérée : déduit de l'emplacement du programme (dossier protégé, jamais d'un fichier
# modifiable) : <X>:\Mindset\Programmes\Nevermind donne <X>:\Mindset\Donnees\Nevermind (Mindset installé sur un autre
# disque), sinon %LOCALAPPDATA%\Mindset\Nevermind. Même règle dans le lanceur (DossierDonnees, lanceur.cs).
# Au premier lancement, l'ancien dossier y est renommé d'un coup (même disque : rien n'est copié, rien ne peut rester
# à moitié déplacé). S'il est utilisé (fichier ouvert, autre Nevermind encore ouvert dessus), ou sur un autre disque
# (c'est alors Mindset qui copie les données avant de lancer Nevermind), on garde l'ancien pour cette fois.
$OldDataDir = Join-Path $env:LOCALAPPDATA 'OptiGame'
$DataDirNote = $null   # noté au journal dès qu'il est prêt (après donnees.ps1)
function Get-ManagedDataDir {
    $p = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent   # parent du dossier de Nevermind.exe
    $m = Split-Path $p -Parent
    if ($m -and (Split-Path $p -Leaf) -eq 'Programmes' -and (Split-Path $m -Leaf) -eq 'Mindset') { return [IO.Path]::Combine($m, 'Donnees\Nevermind') }
    Join-Path $env:LOCALAPPDATA 'Mindset\Nevermind'
}
function Resolve-DataDir {
    if (-not $Managed) { return $OldDataDir }
    $new = Get-ManagedDataDir
    if ([IO.Directory]::Exists($new) -or -not [IO.Directory]::Exists($OldDataDir)) { return $new }
    if ($Uninstall -or (Test-InstanceRunning)) { return $OldDataDir }
    if ([IO.Path]::GetPathRoot($new) -ne [IO.Path]::GetPathRoot($OldDataDir)) {
        $script:DataDirNote = "Données : $OldDataDir est sur un autre disque que $new, rien n'est copié (Mindset s'en charge), l'ancien dossier est gardé pour cette fois"
        return $OldDataDir
    }
    try {
        New-Item -ItemType Directory -Force -Path (Split-Path $new) -ErrorAction Stop | Out-Null
        # Directory.Move et pas Move-Item : quand un fichier est ouvert, Move-Item crée quand même le nouveau dossier
        # (vide), qui serait pris au lancement suivant pour le bon. Directory.Move renomme tout, ou rien.
        [IO.Directory]::Move($OldDataDir, $new)
        $script:DataDirNote = "Données déplacées : $OldDataDir -> $new (Nevermind est géré par Mindset)"
        return $new
    } catch {
        $script:DataDirNote = "Données : $OldDataDir n'a pas pu être renommé en $new, l'ancien dossier est gardé pour cette fois ($($_.Exception.Message))"
        return $OldDataDir
    }
}
$DataDir = Resolve-DataDir

# ---------------------------------------------------------------------------
# Droits administrateur
# ---------------------------------------------------------------------------
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase, System.Drawing, System.Windows.Forms

# ---------------------------------------------------------------------------
# Nevermind s'utilise avec Mindset
# ---------------------------------------------------------------------------
# Une copie qui n'est pas gérée par Mindset ne s'ouvre plus : une petite fenêtre propose d'installer (ou d'ouvrir)
# Mindset, qui reprend cette copie avec ses réglages et ses données. Contrôlé avant tout le reste : pas de demande
# des droits administrateur, pas de verrou, rien ne démarre (ni minuteries, ni icône près de l'horloge, ni overlay).
# Restent possibles : la désinstallation (elle remet les réglages de Windows comme avant) et les dossiers de travail
# (dossier .git à la racine de la copie ou juste au-dessus). La copie de test n'est bloquée qu'avec OPTIGAME_TEST_BLOCAGE
# (s'il finit par .png, la fenêtre s'y enregistre en image puis se ferme : capture pour vérifier son allure).
$MindsetUrl = 'https://github.com/JordanJacquot/Mindset/releases/latest'
$MindsetExe = Join-Path $(if ($env:ProgramW6432) { $env:ProgramW6432 } else { $env:ProgramFiles }) 'Mindset\Mindset\Mindset.exe'

function Test-WorkCopy {
    $root = Split-Path $PSScriptRoot -Parent
    foreach ($d in @($root, (Split-Path $root -Parent))) { if ($d -and (Test-Path -LiteralPath (Join-Path $d '.git'))) { return $true } }
    $false
}

function Show-MindsetRequired {
    $hasMindset = Test-Path -LiteralPath $MindsetExe
    $xaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation" xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="Nevermind" Width="480" SizeToContent="Height" WindowStyle="None" ResizeMode="NoResize" WindowStartupLocation="CenterScreen"
        Background="#110F19" BorderBrush="#2E2843" BorderThickness="1" FontFamily="Segoe UI Variable Text, Segoe UI">
  <Window.Resources>
    <Style x:Key="Btn" TargetType="Button">
      <Setter Property="Foreground" Value="#E6E1F5"/><Setter Property="FontSize" Value="13"/><Setter Property="FontWeight" Value="SemiBold"/>
      <Setter Property="Padding" Value="18,9"/><Setter Property="Cursor" Value="Hand"/><Setter Property="Background" Value="#14FFFFFF"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Border x:Name="b" CornerRadius="12" Padding="{TemplateBinding Padding}" Background="{TemplateBinding Background}" BorderBrush="#2EFFFFFF" BorderThickness="1">
              <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/>
            </Border>
            <ControlTemplate.Triggers><Trigger Property="IsMouseOver" Value="True"><Setter TargetName="b" Property="Opacity" Value="0.85"/></Trigger></ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>
  </Window.Resources>
  <StackPanel Margin="28,26,28,24">
    <DockPanel>
      <Image x:Name="Logo" DockPanel.Dock="Left" Width="44" Height="44" Margin="0,0,16,0" VerticalAlignment="Top"/>
      <StackPanel>
        <TextBlock Text="Nevermind s'utilise maintenant avec Mindset" Foreground="White" FontSize="18" FontWeight="Bold" TextWrapping="Wrap"
                   FontFamily="Segoe UI Variable Display, Segoe UI"/>
        <TextBlock Text="Installe Mindset, le portail de la suite : il reprend cette copie avec tes réglages et tes données, rien n'est perdu."
                   Foreground="#A6A1BC" FontSize="13" TextWrapping="Wrap" Margin="0,8,0,0" LineHeight="19"/>
      </StackPanel>
    </DockPanel>
    <StackPanel Orientation="Horizontal" HorizontalAlignment="Right" Margin="0,24,0,0">
      <Button x:Name="BtnClose" Style="{StaticResource Btn}" Content="Fermer" Margin="0,0,10,0" IsCancel="True"/>
      <Button x:Name="BtnMain" Style="{StaticResource Btn}" Foreground="#08060F" IsDefault="True">
        <Button.Background>
          <LinearGradientBrush StartPoint="0,0" EndPoint="1,0"><GradientStop Color="#00E5FF" Offset="0"/><GradientStop Color="#B04BFF" Offset="1"/></LinearGradientBrush>
        </Button.Background>
      </Button>
    </StackPanel>
  </StackPanel>
</Window>
'@
    $w = [Windows.Markup.XamlReader]::Parse($xaml)
    $main = $w.FindName('BtnMain')
    $main.Content = if ($hasMindset) { 'Ouvrir Mindset' } else { 'Télécharger Mindset' }
    $ico = Join-Path $PSScriptRoot 'OptiGame.ico'
    if (Test-Path -LiteralPath $ico) {
        try {
            $frames = [System.Windows.Media.Imaging.BitmapDecoder]::Create((New-Object IO.MemoryStream (, [IO.File]::ReadAllBytes($ico))), 'None', 'OnLoad').Frames
            $w.FindName('Logo').Source = $frames | Sort-Object PixelWidth | Where-Object { $_.PixelWidth -ge 48 } | Select-Object -First 1
            $w.Icon = $frames | Sort-Object PixelWidth | Where-Object { $_.PixelWidth -ge 32 } | Select-Object -First 1
        } catch {}
    }
    $w.Add_MouseLeftButtonDown({ try { $w.DragMove() } catch {} })
    $w.FindName('BtnClose').Add_Click({ $w.Close() })
    $main.Add_Click({
        # Par l'Explorateur : Mindset (ou le navigateur) s'ouvre sans les droits administrateur de ce lancement
        $target = if ($hasMindset) { $MindsetExe } else { $MindsetUrl }
        try { Start-Process -FilePath (Join-Path $env:windir 'explorer.exe') -ArgumentList "`"$target`"" -ErrorAction Stop } catch {}
        $w.Close()
    })
    if ("$env:OPTIGAME_TEST_BLOCAGE" -like '*.png') {
        $w.Add_ContentRendered({
            $bmp = New-Object System.Windows.Media.Imaging.RenderTargetBitmap ([int]$w.ActualWidth), ([int]$w.ActualHeight), 96, 96, 'Pbgra32'
            $bmp.Render($w)
            $enc = New-Object System.Windows.Media.Imaging.PngBitmapEncoder
            $enc.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($bmp))
            $fs = [IO.File]::Create($env:OPTIGAME_TEST_BLOCAGE); try { $enc.Save($fs) } finally { $fs.Dispose() }
            $w.Close()
        })
    }
    [void]$w.ShowDialog()
}

if (-not $Managed -and -not $Uninstall -and -not (Test-WorkCopy) -and (-not $env:OPTIGAME_TEST -or $env:OPTIGAME_TEST_BLOCAGE)) {
    Show-MindsetRequired
    exit
}

# ---------------------------------------------------------------------------
# Une seule fenêtre : si Nevermind tourne déjà (même caché près de l'horloge), on le ramène devant
# ---------------------------------------------------------------------------
# La fenêtre ouverte a les droits administrateur : ce lancement-ci ne peut pas lui envoyer de signal
# direct, il dépose donc une « demande d'affichage » qu'elle surveille.
$ShowRequest = Join-Path $DataDir 'afficher.demande'
# Même principe dans l'autre sens : Mindset, quand on le quitte, dépose une « demande de fermeture » (evenements.ps1)
$QuitRequest = Join-Path $DataDir 'quitter.demande'
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
    if ($alreadyRunning -and $Demarrage) {
        # Lancé caché (Mindset lance toujours Nevermind ainsi pour l'afficher dans son onglet) alors qu'un Nevermind est
        # encore là : souvent l'ancien qui finit de se fermer. On lui laisse 5 s pour libérer la place et on continue s'il
        # disparaît. Avant, ce lancement s'arrêtait aussitôt et Mindset attendait une fenêtre qui ne venait jamais.
        $gone = $false
        for ($i = 0; $i -lt 25; $i++) { Start-Sleep -Milliseconds 200; if (-not (Test-InstanceRunning)) { $gone = $true; break } }
        if (-not $gone) { Write-LaunchNote 'Lancement caché abandonné : un autre Nevermind est encore ouvert après 5 s'; exit }
        $alreadyRunning = $false
    }
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

# Nevermind.exe : le programme qui fait tourner l'app (le Gestionnaire des tâches affiche « Nevermind », pas PowerShell)
$AppExe = Join-Path (Split-Path $PSScriptRoot -Parent) 'Nevermind.exe'
# Nevermind tourne sans les droits administrateur : ce qui en a besoin passe par son Aide (NevermindAide.exe, lancée par la
# tâche que Mindset crée, sans demande de Windows ; liste stricte dans modules\aide-actions.ps1).
# Seule la désinstallation les demande (une fois) : elle remet tous les réglages de Windows comme avant et retire l'Aide.
$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if ($Uninstall -and -not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    try {
        Start-Process -FilePath 'powershell.exe' -Verb RunAs -ErrorAction Stop -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', "`"$PSCommandPath`"", '-Uninstall')
    } catch {
        [System.Windows.MessageBox]::Show(
            "La désinstallation de Nevermind a besoin des droits administrateur pour remettre les réglages de Windows comme avant.`n`nRelance-la et clique sur « Oui » quand Windows le demande.",
            'Nevermind', 'OK', 'Warning') | Out-Null
    }
    exit
}

# Instance unique (lancement direct en administrateur, ou deux clics très rapprochés)
if (-not $Uninstall -and -not $env:OPTIGAME_TEST) {
    $mutexNew = $false
    # (une ancienne version encore ouverte en administrateur garde un verrou que ce lancement ne peut pas ouvrir : elle est déjà là)
    try { $script:InstanceMutex = New-Object System.Threading.Mutex($true, 'Local\OptiGame-Instance', [ref]$mutexNew) }
    catch { if (-not $Demarrage) { Send-ShowRequest }; exit }
    if (-not $mutexNew) {
        # Ancienne version encore en train de se fermer (relance après une mise à jour) : elle libère la place sous peu
        $got = $false
        try { $got = $script:InstanceMutex.WaitOne(5000) } catch [System.Threading.AbandonedMutexException] { $got = $true }
        if (-not $got) {
            if (-not $Demarrage) { Send-ShowRequest } else { Write-LaunchNote 'Lancement caché abandonné : la place n''a pas été libérée en 5 s' }
            exit
        }
    }
    try { if (Test-Path -LiteralPath $ShowRequest) { [IO.File]::Delete($ShowRequest) } } catch {}
    # Vieille demande de fermeture (Mindset quitté pendant que Nevermind était fermé) : elle ne vaut pas pour ce lancement
    try { if (Test-Path -LiteralPath $QuitRequest) { [IO.File]::Delete($QuitRequest) } } catch {}
}

# Anciens programmes mis de côté par la dernière mise à jour (ils tournaient pendant qu'on les remplaçait)
try { foreach ($o in @(Get-ChildItem -LiteralPath (Split-Path $PSScriptRoot -Parent) -Filter '*.exe.old' -File -ErrorAction SilentlyContinue)) { try { [IO.File]::Delete($o.FullName) } catch {} } } catch {}

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
        $Splash.ShowInTaskbar = $false   # pas de bouton PowerShell dans la barre des tâches pendant le chargement
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
$missing = @('natif.cs', 'interface.xaml', 'donnees.ps1', 'aide-actions.ps1', 'aide-client.ps1', 'optimisations.ps1', 'systeme.ps1', 'themes.ps1', 'interface.ps1', 'tableau-de-bord.ps1', 'analyse.ps1', 'onglets.ps1', 'visuels.ps1', 'tests.ps1', 'securite.ps1', 'navigation.ps1', 'reseau.ps1', 'reseau-avance.ps1', 'carte-reseau.ps1', 'audit-reseau.ps1', 'mises-a-jour.ps1', 'assistance.ps1', 'jeu.ps1', 'diagnostic-fps.ps1', 'trafic.ps1', 'microsoft.ps1', 'lag.ps1', 'bibliotheque.ps1', 'organizer.ps1', 'parametres.ps1', 'recherche.ps1', 'evenements.ps1' | Where-Object { -not (Test-Path -LiteralPath (Join-Path $ModulesDir $_)) })
if ($missing) {
    [System.Windows.MessageBox]::Show("Des fichiers de Nevermind sont manquants :`n`n$($missing -join ', ')`n`nRetélécharge Nevermind et remplace tout le dossier.", 'OptiGame', 'OK', 'Error') | Out-Null
    exit
}

# Fonctions natives (écrans, souris, barre de titre sombre)
Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $ModulesDir 'natif.cs'), [Text.Encoding]::UTF8))
# Identité propre dans la barre des tâches (sinon l'app est rangée avec PowerShell, avec son icône)
$AppUserModelId = 'JordanJacquot.Nevermind'
try { [void][AppIdentity]::SetProcessAppId($AppUserModelId) } catch {}

foreach ($ogModule in 'donnees', 'aide-actions', 'optimisations', 'systeme', 'aide-client', 'themes') { . (Join-Path $ModulesDir "$ogModule.ps1") }
if ($DataDirNote) { Write-Log $DataDirNote }

if ($Uninstall) {
    Invoke-Uninstall
    exit
}

foreach ($ogModule in 'interface', 'tableau-de-bord', 'analyse', 'onglets', 'visuels', 'tests', 'securite', 'navigation', 'reseau', 'reseau-avance', 'carte-reseau', 'audit-reseau', 'mises-a-jour', 'assistance', 'jeu', 'diagnostic-fps', 'trafic', 'microsoft', 'lag', 'bibliotheque', 'organizer', 'parametres', 'recherche', 'evenements') { . (Join-Path $ModulesDir "$ogModule.ps1") }

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
try { Start-ProtectionPrefetch } catch { Write-Log "Sécurité, lecture de fond : $_" }
$script:TweakRows = @()
$script:CleanRows = @()
$script:PingResults = @()
# Lancé avec Windows : la fenêtre se prépare sans s'afficher, puis reste près de l'horloge
$script:StartHidden = [bool]$Demarrage
# Lancé caché (par Mindset, ou avec Windows) : la fenêtre se prépare hors de l'écran, sans bouton dans la barre des tâches.
# (Réduite, Windows la montrait en petit bandeau en bas à gauche de l'écran, dont la croix ouvrait la question de fermeture
# derrière Mindset et bloquait le chargement.)
if ($script:StartHidden) { $Window.ShowInTaskbar = $false; $Window.ShowActivated = $false; $Window.WindowStartupLocation = 'Manual'; $Window.Left = -32000; $Window.Top = -32000 }
try { Start-AideFond } catch { Write-Log "Aide : $_" }
Write-Log "Démarrage Nevermind $AppVersion (Windows build $($script:Build), langue $((Get-UICulture).Name), PowerShell $($PSVersionTable.PSVersion))$(if ($Demarrage) { ', lancé avec Windows' })"
$Window.Show()
[System.Windows.Threading.Dispatcher]::Run()
if ($script:Relaunch -and (Test-Path -LiteralPath $script:Relaunch)) {
    # Libère la place avant de relancer, sinon la nouvelle version croirait que Nevermind est déjà ouvert
    if ($script:InstanceMutex) { try { $script:InstanceMutex.ReleaseMutex() } catch {}; $script:InstanceMutex.Dispose(); $script:InstanceMutex = $null }
    $relExe = Join-Path (Split-Path (Split-Path $script:Relaunch -Parent) -Parent) 'Nevermind.exe'
    # Relancé pendant qu'il était caché près de l'horloge (thème changé dans Mindset) : il y retourne
    $relArgs = @(if ($script:RelaunchHidden -or $Managed) { '-Demarrage' })   # (géré par Mindset : relancé caché, Mindset le raccroche)
    if (Test-Path -LiteralPath $relExe) { if ($relArgs) { Start-Process -FilePath $relExe -ArgumentList $relArgs } else { Start-Process -FilePath $relExe } }
    else { Start-Process -FilePath 'powershell.exe' -ArgumentList (@('-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', "`"$script:Relaunch`"") + $relArgs) }
}
# Fin de Nevermind : le verrou est rendu tout de suite (Mindset peut rouvrir Nevermind sans attendre la fin du processus),
# et les groupes de fils de fond sont fermés : ouverts, ils gardaient Nevermind.exe en vie sans fenêtre après la fermeture.
# (Le lanceur termine ensuite le processus quoi qu'il arrive, Environment.Exit dans lanceur.cs.)
if ($script:InstanceMutex) { try { $script:InstanceMutex.ReleaseMutex() } catch {}; try { $script:InstanceMutex.Dispose() } catch {}; $script:InstanceMutex = $null }
foreach ($fils in @($script:Pool, $script:BgPool)) { if ($fils) { try { [void]$fils.BeginClose($null, $null) } catch {} } }
