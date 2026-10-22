# Nevermind : mesures en direct, onglets Gaming, Démarrage, Connexion, Nettoyage et Sauvegarde.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Mesures en direct (dans un fil séparé pour ne pas ralentir la fenêtre)
# ---------------------------------------------------------------------------
$Live = [hashtable]::Synchronized(@{ Run = $true; Smi = $SmiPath; BaseMHz = 0 })

$LiveScript = {
    param($sync)
    function Num($v) {
        $v = ([string]$v).Trim()
        if ($v -match '^[\d\.]+$') { [double]::Parse($v, [Globalization.CultureInfo]::InvariantCulture) } else { $null }
    }
    while ($sync.Run) {
        try {
            $pi = Get-CimInstance Win32_PerfFormattedData_Counters_ProcessorInformation -Filter "Name='_Total'" -ErrorAction Stop
            $sync.Cpu = [math]::Min(100.0, [double]$pi.PercentProcessorUtility)
            $sync.CpuPerf = [double]$pi.PercentProcessorPerformance
        } catch {}
        try {
            $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
            $sync.RamTotal = [double]$os.TotalVisibleMemorySize * 1024
            $sync.RamUsed = ([double]$os.TotalVisibleMemorySize - [double]$os.FreePhysicalMemory) * 1024
        } catch {}
        $gpuDone = $false
        if ($sync.Smi) {
            try {
                $o = & $sync.Smi '--query-gpu=utilization.gpu,temperature.gpu,memory.used,memory.total,power.draw,power.limit' '--format=csv,noheader,nounits' 2>$null | Select-Object -First 1
                if ($o) {
                    $v = $o -split ','
                    $sync.Gpu = Num $v[0]; $sync.GpuTemp = Num $v[1]
                    $sync.VramUsed = Num $v[2]; $sync.VramTotal = Num $v[3]; $sync.GpuPower = Num $v[4]; $sync.GpuPowerLimit = Num $v[5]
                    $gpuDone = $true
                }
            } catch {}
        }
        if (-not $gpuDone) {
            try {
                $eng = Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine -ErrorAction Stop | Where-Object { $_.Name -like '*engtype_3D' }
                $sync.Gpu = [math]::Min(100.0, [double](($eng | Measure-Object UtilizationPercentage -Sum).Sum))
            } catch {}
        }
        try {
            $dk = Get-CimInstance Win32_PerfFormattedData_PerfDisk_PhysicalDisk -Filter "Name='_Total'" -ErrorAction Stop
            $sync.DiskBusy = [math]::Max(0.0, [math]::Min(100.0, 100 - [double]$dk.PercentIdleTime))
        } catch {}
        $sync.Updated = Get-Date
        Start-Sleep -Milliseconds $(if ($sync.Fast) { 400 } else { 1500 })
    }
}

# Toutes les secondes : l'alerte de température (les mesures, $Live, sont lues par l'écran de la console lui-même)
function Update-LiveUI {
    if (-not $Live.Updated) { return }
    Test-TempAlert
}

function Start-Live {
    $script:LivePs = [PowerShell]::Create()
    [void]$script:LivePs.AddScript($LiveScript.ToString()).AddArgument($Live)
    [void]$script:LivePs.BeginInvoke()
    $script:LiveTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:LiveTimer.Interval = [TimeSpan]::FromSeconds(1)
    $script:LiveTimer.Add_Tick({ try { Update-LiveUI } catch {} })
    $script:LiveTimer.Start()
}

# ---------------------------------------------------------------------------
# Onglet gaming
# ---------------------------------------------------------------------------
# Pastilles de sous-onglets (Optimisation gaming, Jeux, Perf. In Game) : $OnClick reçoit le numéro de la pastille
function New-PillTabs($HostPanel, [string[]]$Labels, [scriptblock]$OnClick) {
    $tabs = @()
    for ($i = 0; $i -lt $Labels.Count; $i++) {
        $b = New-Object System.Windows.Controls.Border
        $b.CornerRadius = [System.Windows.CornerRadius]::new(16)
        $b.Padding = New-Thickness 16 7 16 7
        $b.Margin = New-Thickness 0 0 8 0
        $b.Cursor = [System.Windows.Input.Cursors]::Hand
        $t = New-Text $Labels[$i] 13 '#D3CDE3' -Semi
        $t.TextWrapping = 'NoWrap'
        $b.Child = $t
        $b.Tag = @{ I = $i; Do = $OnClick }
        $b.Add_MouseLeftButtonUp({ param($s, $e) $x = $s.Tag; Invoke-Safe { & $x.Do ([int]$x.I) } })
        [void]$HostPanel.Children.Add($b)
        $tabs += $b
    }
    , $tabs
}

# La pastille choisie prend le dégradé
function Set-PillTabs($Tabs, [int]$Index) {
    for ($i = 0; $i -lt @($Tabs).Count; $i++) {
        $b = $Tabs[$i]
        $b.Background = if ($i -eq $Index) { $Window.FindResource('AccentBg') } else { Get-Brush '#16FFFFFF' }
        $b.Child.Foreground = Get-Brush $(if ($i -eq $Index) { '#07060C' } else { '#D3CDE3' })
    }
}

# Sous-onglets d'Optimisation gaming, appelés par leur nom (Set-GamingSubPage 'mode').
# Les FPS et le lag de chaque partie sont dans la page Jeux, sous-onglet Perf. In Game (Show-PerfPage 'lag').
$GamingSubPages = @(
    @{ Id = 'tweaks'; Label = 'Réglages Windows'; Page = 'GPageTweaks' },
    @{ Id = 'mode'; Label = 'Mode jeu'; Page = 'GPageMode' },
    @{ Id = 'profiles'; Label = 'Profils par jeu'; Page = 'GPageProfiles' }
)

function Set-GamingSubPage($Page) {
    $Index = if ($Page -is [string]) { [array]::IndexOf(@($GamingSubPages | ForEach-Object { $_.Id }), $Page) } else { [int]$Page }
    if ($Index -lt 0) { $Index = 0 }
    $pages = @($GamingSubPages | ForEach-Object { $ui[$_.Page] })
    for ($i = 0; $i -lt $pages.Count; $i++) { $pages[$i].Visibility = if ($i -eq $Index) { 'Visible' } else { 'Collapsed' } }
    if ($script:GTabs) { Set-PillTabs $script:GTabs $Index }
    $script:GamingSubPage = $GamingSubPages[$Index].Id
}

function Build-GamingTabs {
    if ($script:GTabs) { return }
    $script:GTabs = New-PillTabs $ui.GamingTabs @($GamingSubPages | ForEach-Object { $_.Label }) { param($i) Set-GamingSubPage $i }
    Set-GamingSubPage 0
}

# ---------------------------------------------------------------------------
# Page Jeux : Bibliothèque (et l'Organizer Dofus) ou Perf. In Game (FPS et lag de chaque partie)
# ---------------------------------------------------------------------------
$GamesViews = @(
    @{ Id = 'lib'; Label = 'Bibliothèque' },
    @{ Id = 'perf'; Label = 'Perf. In Game' }
)
$PerfSubPages = @(
    @{ Id = 'fps'; Label = 'FPS'; Page = 'GPageFps' },
    @{ Id = 'lag'; Label = 'Lag en ligne'; Page = 'GPageLag' }
)

function Build-GamesTabs {
    if ($script:GamesTabsUi) { return }
    $script:GamesTabsUi = New-PillTabs $ui.GamesTabs @($GamesViews | ForEach-Object { $_.Label }) { param($i) Set-GamesView $GamesViews[$i].Id }
    $script:PerfTabsUi = New-PillTabs $ui.PerfTabs @($PerfSubPages | ForEach-Object { $_.Label }) { param($i) Set-PerfSubPage $PerfSubPages[$i].Id }
    # (deuxième niveau : pastilles plus petites, pour qu'on voie qu'elles sont dans Perf. In Game)
    foreach ($b in $script:PerfTabsUi) { $b.Padding = New-Thickness 13 5 13 5; $b.CornerRadius = [System.Windows.CornerRadius]::new(13); $b.Child.FontSize = 12 }
    Set-PerfSubPage 'fps'
    Set-GamesView 'lib'
}

function Set-GamesView([string]$Id) {
    Build-GamesTabs
    $i = [math]::Max(0, [array]::IndexOf(@($GamesViews | ForEach-Object { $_.Id }), $Id))
    $script:GamesView = $GamesViews[$i].Id
    Set-PillTabs $script:GamesTabsUi $i
    $perf = $script:GamesView -eq 'perf'
    $ui.PerfView.Visibility = if ($perf) { 'Visible' } else { 'Collapsed' }
    $ui.LibButtons.Visibility = if ($perf) { 'Collapsed' } else { 'Visible' }
    if ($perf) {
        foreach ($n in 'LibToolbar', 'LibBody', 'OrgScroll') { $ui[$n].Visibility = 'Collapsed' }
        $ui.LibTitle.Text = 'Perf. In Game'
        $ui.LibSub.Text = 'Tes FPS et ta connexion pendant chaque partie : ce qui ralentit, et comment le corriger.'
        if ($script:PerfSubPage -eq 'lag' -and $script:LagPanelDirty) { $script:LagPanelDirty = $false; Build-LagPanel }
    } else {
        # (la bibliothèque ou l'Organizer Dofus, comme avant le passage par Perf. In Game)
        Show-OrgView ([bool]$script:OrgViewOn)
    }
}

function Set-PerfSubPage([string]$Id) {
    Build-GamesTabs
    $i = [math]::Max(0, [array]::IndexOf(@($PerfSubPages | ForEach-Object { $_.Id }), $Id))
    $script:PerfSubPage = $PerfSubPages[$i].Id
    Set-PillTabs $script:PerfTabsUi $i
    for ($k = 0; $k -lt $PerfSubPages.Count; $k++) { $ui[$PerfSubPages[$k].Page].Visibility = if ($k -eq $i) { 'Visible' } else { 'Collapsed' } }
    if ($script:PerfSubPage -eq 'lag' -and $script:LagPanelDirty) { $script:LagPanelDirty = $false; Build-LagPanel }
}

# Ouvre Jeux > Perf. In Game > FPS ou Lag en ligne
function Show-PerfPage([string]$Id) {
    Show-Page $GamesIndex
    Set-GamesView 'perf'
    Set-PerfSubPage $Id
}

function Test-PerfShown([string]$Id) { $ui.Tabs.SelectedIndex -eq $GamesIndex -and $script:GamesView -eq 'perf' -and $script:PerfSubPage -eq $Id }

# Fin d'une partie : la page Lag n'est refaite tout de suite que si elle est à l'écran (0,5 s), sinon à sa prochaine ouverture
function Request-LagPanel {
    if ((Test-PerfShown 'lag') -and $Window.IsVisible) { $script:LagPanelDirty = $false; Build-LagPanel }
    else { $script:LagPanelDirty = $true }
}

function Build-GamingTab {
    Build-GamingTabs
    $panel = $ui.GamingPanel
    $panel.Children.Clear()
    $script:TweakRows = @()
    $tweaks = @(Get-AvailableTweaks)
    $done = 0
    foreach ($t in $tweaks) {
        Step-UI
        $ok = Test-Tweak $t
        if ($ok) { $done++ }
        $card = New-Card
        $card.Padding = New-Thickness 14 10 14 10
        $card.Margin = New-Thickness 0 0 0 6
        $g = New-Grid @('Auto', '*', 'Auto')
        $cb = New-Object System.Windows.Controls.CheckBox
        $cb.VerticalAlignment = 'Center'
        $cb.Margin = New-Thickness 0 0 12 0
        $cb.LayoutTransform = [System.Windows.Media.ScaleTransform]::new(1.2, 1.2)
        if ($ok) { $cb.IsChecked = $false; $cb.IsEnabled = $false }
        else { $cb.IsChecked = ($t.Recommended -ne $false) }
        Add-ToGrid $g $cb 0
        $sp = New-Object System.Windows.Controls.StackPanel
        $sp.VerticalAlignment = 'Center'
        $title = New-Text $t.Titre 14 $(if ($ok) { '#A6A1BC' } else { '#FFFFFF' }) -Semi
        [void]$sp.Children.Add($title)
        $desc = New-Text $t.Description 12 '#A6A1BC'
        $desc.Margin = New-Thickness 0 4 0 0
        $desc.Visibility = 'Collapsed'
        [void]$sp.Children.Add($desc)
        Add-ToGrid $g $sp 1
        $right = New-Object System.Windows.Controls.StackPanel
        $right.Orientation = 'Horizontal'; $right.VerticalAlignment = 'Center'
        if ($ok) {
            [void]$right.Children.Add((New-Badge 'Optimisé' $Colors.ok))
        } else {
            $impactColor = switch ($t.Impact) { 'Important' { $Colors.bad } 'Moyen' { $Colors.warn } default { $Colors.info } }
            [void]$right.Children.Add((New-Badge "Impact $($t.Impact.ToLower())" $impactColor))
            if ($t.Recommended -eq $false) { [void]$right.Children.Add((New-Badge 'Optionnel' '#A6A1BC')) }
        }
        if ($t.Reboot) { [void]$right.Children.Add((New-Badge 'Redémarrage' '#A6A1BC')) }
        $chev = New-Text '▾' 14 '#655E7E'
        $chev.Margin = New-Thickness 10 0 0 0; $chev.VerticalAlignment = 'Center'
        [void]$right.Children.Add($chev)
        Add-ToGrid $g $right 2
        $card.Child = $g
        $card.Cursor = [System.Windows.Input.Cursors]::Hand
        $card.ToolTip = 'Clique pour voir l''explication'
        $card.Tag = @{ Desc = $desc; Chev = $chev }
        $card.Add_MouseLeftButtonUp({
            param($s, $e)
            if ($e.OriginalSource -is [System.Windows.Controls.CheckBox] -or $e.OriginalSource.TemplatedParent -is [System.Windows.Controls.CheckBox]) { return }
            $open = $s.Tag.Desc.Visibility -ne 'Visible'
            $s.Tag.Desc.Visibility = if ($open) { 'Visible' } else { 'Collapsed' }
            $s.Tag.Chev.Text = if ($open) { '▴' } else { '▾' }
        })
        [void]$panel.Children.Add($card)
        $script:TweakRows += @{ Tweak = $t; CheckBox = $cb }
    }
    $left = $tweaks.Count - $done
    $ui.TweakSummary.Text = if ($left) { "$left réglage$(if ($left -gt 1) {'s'}) à optimiser sur $($tweaks.Count)" } else { "Tout est optimisé ($($tweaks.Count) réglages)" }
    $ui.TweakSummary.Foreground = Get-Brush $(if ($left) { '#FFFFFF' } else { $Colors.ok })
    Build-GameSections
}

function New-RestorePoint {
    Set-Status 'Création du point de restauration (ça peut prendre une minute)...'
    # Windows refuse sinon plus d'un point de restauration par 24 h.
    Set-Reg 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore' 'SystemRestorePointCreationFrequency' 0
    $r = try { [void](Invoke-Aide 'restauration.creer' @{ Description = 'Nevermind' } 600000); 'OK' } catch { $_.Exception.Message }
    if ("$r" -eq 'OK') { Set-Status 'Point de restauration créé.'; return $true }
    Write-Log "Point de restauration: $r"
    Confirm-Action ("Impossible de créer le point de restauration.`n`nMotif : $r`n`n" +
        "La protection du système est peut-être désactivée sur ce PC. Tu peux quand même continuer : " +
        "Nevermind garde une sauvegarde de chaque réglage modifié et peut tout annuler depuis la page Sauvegarde (dans Protection).`n`nContinuer ?")
}

function Invoke-ApplyTweaks {
    $sel = @($script:TweakRows | Where-Object { $_.CheckBox.IsEnabled -and $_.CheckBox.IsChecked })
    if (-not $sel.Count) { Show-Message 'Aucune optimisation cochée.'; return }
    Set-Busy $true
    if ($ui.ChkRestore.IsChecked -and -not (New-RestorePoint)) { Set-Status 'Annulé.'; return }
    $done = @(); $failed = @(); $reboot = $false
    $before = if ($script:LastAnalysis) { $script:LastAnalysis.Score } else { $null }
    $script:RunLog = New-Object System.Collections.ArrayList
    try {
        foreach ($r in $sel) {
            Set-Status "Application : $($r.Tweak.Titre)..."
            try {
                & $r.Tweak.Apply
                $done += $r.Tweak.Titre
                if ($r.Tweak.Reboot) { $reboot = $true }
            } catch {
                $failed += "$($r.Tweak.Titre) : $($_.Exception.Message)"
                Write-Log "Échec $($r.Tweak.Id): $_"
            }
        }
    } finally {
        $log = $script:RunLog
        $script:RunLog = $null
    }
    Build-GamingTab
    Update-BackupSummary
    Invoke-Analysis
    $lines = @()
    if ($done.Count) {
        $lines += "$($done.Count) optimisation$(if ($done.Count -gt 1) {'s'}) appliquée$(if ($done.Count -gt 1) {'s'}) :"
        foreach ($d in $done) { $lines += "•  $d" }
        if ($null -ne $before) { $lines += "Score : $before → $($script:LastAnalysis.Score)" }
    }
    if ($failed) { $lines += 'Non appliqué :'; $lines += $failed }
    if ($reboot -and $done.Count) { $lines += 'Redémarre ton PC pour que tout soit pris en compte.' }
    $title = if ($done.Count) { "C'est fait !" } else { 'Rien n''a été appliqué' }
    Set-Status $title
    Show-ResultSheet $title $lines $log $null
}

# ---------------------------------------------------------------------------
# Onglet démarrage
# ---------------------------------------------------------------------------
$KeepStartup = '\b(Realtek|RtkAud|NVIDIA|AMD|Radeon|Intel|SecurityHealth|Sécurité Windows|Windows Security|Defender|Avast|AVG|Kaspersky|Bitdefender|Norton|McAfee|ESET|Malwarebytes|Synaptics|ELAN|Dolby|Nahimic|Waves|MaxxAudio|Wacom|Bluetooth)\b'
$DeviceStartup = '\b(Logitech|LGHUB|Razer|Corsair|iCUE|SteelSeries|HyperX|NGENUITY|Roccat|Glorious|Armoury|Aura|MSI Center|Mystic Light|Alienware|Stream Deck|Elgato)\b'
$HostExes = '^(rundll32|cmd|powershell|pwsh|wscript|cscript|conhost|explorer|mshta)\.exe$'

# Icône d'un programme, prête pour l'interface.
# Infos des programmes (nom, éditeur, icône) lues en arrière-plan : ouvrir un .exe réveille souvent l'antivirus,
# ce qui figeait la fenêtre jusqu'à 2 s pendant le chargement. Gardées pour toute la session.
$ExeFactsWork = {
    param($list)
    Add-Type -AssemblyName System.Drawing
    foreach ($x in $list) {
        $r = @{ Exe = $x.Exe; Company = ''; Product = ''; Desc = ''; IconExe = $x.Exe; Png = $null }
        try { $vi = [Diagnostics.FileVersionInfo]::GetVersionInfo($x.Exe); $r.Company = ([string]$vi.CompanyName).Trim(); $r.Product = ([string]$vi.ProductName).Trim(); $r.Desc = ([string]$vi.FileDescription).Trim() } catch {}
        # Applis lancées par un petit programme de mise à jour (Discord...) : l'icône de la vraie appli
        if ($x.Cmd -match '--processStart\s+"?([^"\s]+\.exe)') {
            $real = Get-ChildItem -LiteralPath (Split-Path $x.Exe -Parent) -Filter $matches[1] -Recurse -Depth 2 -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
            if ($real) { $r.IconExe = $real.FullName }
        }
        try {
            $ic = [System.Drawing.Icon]::ExtractAssociatedIcon($r.IconExe)
            if ($ic) { $bmp = $ic.ToBitmap(); $ms = New-Object IO.MemoryStream; $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png); $r.Png = $ms.ToArray(); $ms.Dispose(); $bmp.Dispose(); $ic.Dispose() }
        } catch {}
        $r
    }
}

function Update-ExeFacts($Items) {
    if (-not $script:ExeFacts) { $script:ExeFacts = @{} }
    $todo = @($Items | Where-Object { $_.Exe -and -not $script:ExeFacts.ContainsKey(([string]$_.Exe).ToLower()) } | ForEach-Object { @{ Exe = [string]$_.Exe; Cmd = [string]$_.Commande } })
    if (-not $todo.Count) { return }
    foreach ($r in @(Invoke-Async $ExeFactsWork $todo)) { if ($r -and $r.Exe) { $script:ExeFacts[([string]$r.Exe).ToLower()] = $r } }
}

function Get-ExeIcon([string]$Exe) {
    $f = if ($script:ExeFacts -and $Exe) { $script:ExeFacts[$Exe.ToLower()] } else { $null }
    if ($f -and $f.Png) {
        try {
            $bi = New-Object System.Windows.Media.Imaging.BitmapImage
            $bi.BeginInit(); $bi.StreamSource = New-Object IO.MemoryStream (, [byte[]]$f.Png); $bi.CacheOption = 'OnLoad'; $bi.EndInit(); $bi.Freeze()
            return $bi
        } catch {}
    }
    try {
        $ic = [System.Drawing.Icon]::ExtractAssociatedIcon($Exe)
        if (-not $ic) { return $null }
        $src = [System.Windows.Interop.Imaging]::CreateBitmapSourceFromHIcon($ic.Handle, [System.Windows.Int32Rect]::Empty,
            [System.Windows.Media.Imaging.BitmapSizeOptions]::FromEmptyOptions())
        $src.Freeze()
        $ic.Dispose()
        $src
    } catch { $null }
}

# Nom lisible, éditeur et conseil pour une entrée de démarrage.
function Get-StartupInfo($Item) {
    $name = $Item.Nom; $company = ''
    $isHost = [IO.Path]::GetFileName($Item.Exe) -match $HostExes
    $facts = if ($script:ExeFacts -and $Item.Exe) { $script:ExeFacts[([string]$Item.Exe).ToLower()] } else { $null }
    try {
        if ($facts) { $company = $facts.Company; $prod = $facts.Product; $desc = $facts.Desc }
        else { $vi = [Diagnostics.FileVersionInfo]::GetVersionInfo($Item.Exe); $company = ([string]$vi.CompanyName).Trim(); $prod = ([string]$vi.ProductName).Trim(); $desc = ([string]$vi.FileDescription).Trim() }
        if (-not $isHost) {
            if ($prod -and $prod.Length -le 40 -and $prod -notmatch 'Windows.*(Operating System|Système)') { $name = $prod }
            elseif ($desc -and $desc.Length -le 50) { $name = $desc }
        }
    } catch {}
    $file = [IO.Path]::GetFileName($Item.Exe)
    # Noms trop vagues (« Update », « Launcher »...) : le nom de l'entrée est plus parlant.
    if ($name -match '^(Update|Updater|Launcher|Helper|Service|Tray|App|Client|Setup)$' -and $Item.Nom -match '[A-Za-z]{3}') { $name = $Item.Nom }
    if ($file -match '^EpicGamesLauncher') { $name = 'Epic Games Launcher' }
    # Nom, éditeur, entrée et nom du fichier (pas le dossier complet : un programme rangé
    # dans le dossier de Steam n'est pas Steam).
    $text = "$name $company $($Item.Nom) $file"
    if ($text -match $SafeStartup) {
        $adv = @{ Kind = 'safe'; Label = 'Tu peux le désactiver'; Color = $Colors.ok; Why = 'Il se lance quand tu l''ouvres, pas besoin qu''il démarre avec Windows.' }
    } elseif ($text -match $KeepStartup) {
        $adv = @{ Kind = 'keep'; Label = 'À garder'; Color = $Colors.warn; Why = 'Pilote ou protection de ton PC : laisse-le activé.' }
    } elseif ($text -match $DeviceStartup) {
        $adv = @{ Kind = 'choice'; Label = 'À toi de voir'; Color = '#A6A1BC'; Why = 'Garde-le si tu utilises les réglages de ta souris, ton clavier ou tes lumières.' }
    } else {
        $adv = @{ Kind = 'choice'; Label = 'À toi de voir'; Color = '#A6A1BC'; Why = 'Désactive-le si tu ne t''en sers pas dès que tu allumes ton PC.' }
    }
    # Applis lancées par un petit programme de mise à jour (Discord...) : on prend l'icône de la vraie appli.
    $iconExe = $Item.Exe
    if ($facts) { $iconExe = $facts.IconExe }
    elseif ($Item.Commande -match '--processStart\s+"?([^"\s]+\.exe)') {
        $real = Get-ChildItem -LiteralPath (Split-Path $Item.Exe -Parent) -Filter $matches[1] -Recurse -Depth 2 -File -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if ($real) { $iconExe = $real.FullName }
    }
    @{ Item = $Item; Name = $name; Company = $company; Advice = $adv; IconExe = $iconExe }
}

function Update-StartupCount {
    $on = @($script:StartupEntries | Where-Object { $_.Item.Enabled }).Count
    $ui.StartupCount.Text = if ($on) {
        "$on programme$(if ($on -gt 1) {'s se lancent'} else {' se lance'}) quand tu allumes ton PC. Moins il y en a, plus il démarre vite et plus il reste de mémoire pour tes jeux. Rien n'est supprimé : tu peux changer d'avis quand tu veux."
    } else { 'Aucun programme ne se lance quand tu allumes ton PC.' }
    $safe = @($script:StartupEntries | Where-Object { $_.Advice.Kind -eq 'safe' -and $_.Item.Enabled }).Count
    $ui.BtnDisableStartup.Content = "Désactiver ce qui est conseillé ($safe)"
    $ui.BtnDisableStartup.Visibility = if ($safe) { 'Visible' } else { 'Collapsed' }
}

function Update-StartupList {
    $order = @{ safe = 0; choice = 1; keep = 2 }
    $startItems = @(Get-StartupItems)
    Update-ExeFacts $startItems
    $script:StartupEntries = @($startItems | ForEach-Object { Get-StartupInfo $_ } |
        Sort-Object @{ Expression = { -not $_.Item.Enabled } }, @{ Expression = { $order[$_.Advice.Kind] } }, @{ Expression = { $_.Name } })
    $panel = $ui.StartupPanel
    $panel.Children.Clear()
    foreach ($s in $script:StartupEntries) {
        Step-UI
        $card = New-Card
        $card.Padding = New-Thickness 14 12 16 12
        $g = New-Grid @('Auto', '*', 'Auto')

        $icon = Get-ExeIcon $s.IconExe
        if ($icon) {
            $img = New-Object System.Windows.Controls.Image
            $img.Source = $icon; $img.Width = 32; $img.Height = 32
            [System.Windows.Media.RenderOptions]::SetBitmapScalingMode($img, 'HighQuality')
            $iconEl = $img
        } else {
            $iconEl = New-Object System.Windows.Controls.Border
            $iconEl.Width = 32; $iconEl.Height = 32
            $iconEl.CornerRadius = [System.Windows.CornerRadius]::new(8)
            $iconEl.Background = Get-Brush '#22FFFFFF'
        }
        $iconEl.Margin = New-Thickness 0 0 14 0
        $iconEl.VerticalAlignment = 'Center'
        Add-ToGrid $g $iconEl 0

        $sp = New-Object System.Windows.Controls.StackPanel
        $sp.VerticalAlignment = 'Center'
        $head = New-Object System.Windows.Controls.WrapPanel
        [void]$head.Children.Add((New-Text $s.Name 14.5 '#FFFFFF' -Semi))
        [void]$head.Children.Add((New-Badge $s.Advice.Label $s.Advice.Color))
        [void]$sp.Children.Add($head)
        $sub = New-Text $s.Advice.Why 12.5 '#A6A1BC'
        $sub.Margin = New-Thickness 0 3 0 0
        [void]$sp.Children.Add($sub)
        Add-ToGrid $g $sp 1

        $sw = New-Object System.Windows.Controls.CheckBox
        $sw.Style = $Window.FindResource('Switch')
        $sw.IsChecked = [bool]$s.Item.Enabled
        $sw.VerticalAlignment = 'Center'
        $sw.Margin = New-Thickness 16 0 0 0
        $sw.ToolTip = 'Activé = se lance quand tu allumes ton PC'
        $s.Card = $card
        $sw.Tag = $s
        $sw.Add_Click({ param($sender, $e) Invoke-Safe { Set-StartupToggle $sender } })
        Add-ToGrid $g $sw 2

        $card.Child = $g
        if (-not $s.Item.Enabled) { $card.Opacity = 0.6 }
        [void]$panel.Children.Add($card)
    }
    if (-not $script:StartupEntries.Count) {
        [void]$panel.Children.Add((New-Text 'Aucun programme ne se lance avec Windows.' 14 '#A6A1BC'))
    }
    Update-StartupCount
}

function Set-StartupToggle($Switch) {
    $s = $Switch.Tag
    $on = [bool]$Switch.IsChecked
    $script:RunLog = New-Object System.Collections.ArrayList
    try { Set-StartupState $s.Item $on } finally { $log = $script:RunLog; $script:RunLog = $null }
    [void](Add-History $(if ($on) { "Démarrage : « $($s.Name) » réactivé" } else { "Démarrage : « $($s.Name) » ne se lance plus" }) @() $log)
    $s.Item.Enabled = $on
    $s.Card.Opacity = if ($on) { 1 } else { 0.6 }
    Update-StartupCount
    Update-BackupSummary
    Set-Status $(if ($on) { "« $($s.Name) » se lancera de nouveau quand tu allumes ton PC." } else { "« $($s.Name) » ne se lancera plus quand tu allumes ton PC." })
}

function Disable-RecommendedStartup {
    $todo = @($script:StartupEntries | Where-Object { $_.Advice.Kind -eq 'safe' -and $_.Item.Enabled })
    if (-not $todo.Count) { return }
    $names = ($todo | ForEach-Object { $_.Name }) -join ', '
    if (-not (Confirm-Action "Ces programmes ne se lanceront plus quand tu allumes ton PC :`n`n$names`n`nIls restent installés et s'ouvrent normalement quand tu cliques dessus. Continuer ?")) { return }
    Set-Busy $true
    $script:RunLog = New-Object System.Collections.ArrayList
    try { foreach ($s in $todo) { Set-StartupState $s.Item $false } }
    finally { $log = $script:RunLog; $script:RunLog = $null }
    Update-StartupList
    Update-BackupSummary
    $lines = @("$($todo.Count) programme$(if ($todo.Count -gt 1) {'s'}) ne se lancer$(if ($todo.Count -gt 1) {'ont'} else {'a'}) plus au démarrage :")
    foreach ($s in $todo) { $lines += "•  $($s.Name)" }
    Set-Status 'Programmes désactivés au démarrage.'
    Show-ResultSheet "C'est fait !" $lines $log $null
}

# ---------------------------------------------------------------------------
# Onglet réseau
# ---------------------------------------------------------------------------
function Update-NetInfo {
    $panel = $ui.NetInfoPanel
    $panel.Children.Clear()
    if (-not $script:Net) { $script:Net = Get-ActiveNet }
    [void]$panel.Children.Add((New-Text 'Ta connexion' 16 '#FFFFFF' -Semi))
    if (-not $script:Net) {
        [void]$panel.Children.Add((New-Text 'Aucune connexion Internet détectée.' 13 $Colors.bad))
        $ui.DnsCurrent.Text = ''
        return
    }
    $n = $script:Net
    $rows = [ordered]@{
        'Type'    = if ($n.Wifi) { 'Wi-Fi' } else { 'Ethernet (câble)' }
        'Carte'   = $n.Desc
        'Vitesse' = $n.Speed
        'Box'     = $n.Gateway
    }
    foreach ($k in $rows.Keys) {
        $g = New-Grid @('100', '*')
        $g.Margin = New-Thickness 0 6 0 0
        Add-ToGrid $g (New-Text $k 13 '#A6A1BC') 0
        Add-ToGrid $g (New-Text ([string]$rows[$k]) 13) 1
        [void]$panel.Children.Add($g)
    }
    try {
        $dns = [string]$n.Dns
        if (-not $dns) { throw 'DNS inconnu' }
        $ui.DnsCurrent.Text = "DNS actuel : $dns"
    } catch { $ui.DnsCurrent.Text = '' }
}

function Measure-Latency([string]$Target, [string]$Label, [int]$Count = 20) {
    $ping = New-Object System.Net.NetworkInformation.Ping
    $times = New-Object System.Collections.Generic.List[double]
    $lost = 0
    for ($i = 1; $i -le $Count; $i++) {
        Set-Status "Test de $Label... ($i sur $Count)"
        try {
            $r = $ping.Send($Target, 1000)
            if ($r.Status -eq 'Success') { $times.Add($r.RoundtripTime) } else { $lost++ }
        } catch { $lost++ }
        Start-Sleep -Milliseconds 80
    }
    $ping.Dispose()
    $avg = 0; $jitter = 0
    if ($times.Count) {
        $avg = ($times | Measure-Object -Average).Average
        if ($times.Count -gt 1) {
            $diffs = for ($i = 1; $i -lt $times.Count; $i++) { [math]::Abs($times[$i] - $times[$i - 1]) }
            $jitter = ($diffs | Measure-Object -Average).Average
        }
    }
    @{ Avg = [math]::Round($avg, 1); Jitter = [math]::Round($jitter, 1); Loss = [math]::Round(100 * $lost / $Count) }
}

function Invoke-NetTest {
    Set-Busy $true
    $script:Net = Get-ActiveNet
    Update-NetInfo
    $panel = $ui.PingPanel
    $panel.Children.Clear()
    $script:PingResults = @()
    if (-not $script:Net) { Set-Status 'Pas de connexion.'; return }

    $targets = @()
    if ($script:Net.Gateway -and $script:Net.Gateway -ne '0.0.0.0') { $targets += @{ Label = 'Ta box (réseau local)'; Host = $script:Net.Gateway; Local = $true } }
    $targets += @{ Label = 'Internet : Cloudflare'; Host = '1.1.1.1'; Local = $false }
    $targets += @{ Label = 'Internet : Google'; Host = '8.8.8.8'; Local = $false }

    foreach ($t in $targets) {
        $m = Measure-Latency $t.Host $t.Label
        if ($t.Local) {
            if ($m.Loss -gt 0 -or $m.Avg -gt 5 -or $m.Jitter -gt 3) {
                $st = 'warn'; $verdict = "Réseau local instable. C'est typique du Wi-Fi (murs, distance, voisins) : rapproche-toi de la box ou passe en câble."
            } else { $st = 'ok'; $verdict = 'Liaison avec ta box rapide et stable.' }
        } else {
            if ($m.Loss -ge 5)       { $st = 'bad';  $verdict = 'Pertes de paquets : tu risques de la téléportation et des tirs qui ne comptent pas.' }
            elseif ($m.Loss -gt 0)   { $st = 'warn'; $verdict = 'Quelques pertes de paquets.' }
            elseif ($m.Jitter -gt 15){ $st = 'warn'; $verdict = 'Ping instable (gigue élevée) : sensations de lag irrégulières.' }
            elseif ($m.Avg -lt 30)   { $st = 'ok';   $verdict = 'Excellent pour jouer en ligne.' }
            elseif ($m.Avg -lt 60)   { $st = 'ok';   $verdict = 'Bon pour jouer en ligne.' }
            elseif ($m.Avg -lt 100)  { $st = 'warn'; $verdict = 'Moyen : jouable, mais tu seras désavantagé dans les jeux compétitifs.' }
            else                     { $st = 'bad';  $verdict = 'Ping élevé.' }
        }
        if ($m.Loss -eq 100) { $st = 'bad'; $verdict = 'Aucune réponse (ce serveur bloque peut-être le ping).' }

        $card = New-Card
        $g = New-Grid @('Auto', '*', 'Auto')
        $dot = New-Object System.Windows.Shapes.Ellipse
        $dot.Width = 12; $dot.Height = 12; $dot.Fill = Get-Brush $Colors[$st]
        $dot.VerticalAlignment = 'Top'; $dot.Margin = New-Thickness 0 4 14 0
        Add-ToGrid $g $dot 0
        $sp = New-Object System.Windows.Controls.StackPanel
        [void]$sp.Children.Add((New-Text "$($t.Label) ($($t.Host))" 14 '#FFFFFF' -Semi))
        $v = New-Text $verdict 12.5 '#A6A1BC'; $v.Margin = New-Thickness 0 3 0 0
        [void]$sp.Children.Add($v)
        Add-ToGrid $g $sp 1
        $stats = New-Text "$($m.Avg) ms   gigue $($m.Jitter) ms   pertes $($m.Loss) %" 13 $Colors[$st] -Semi
        $stats.VerticalAlignment = 'Center'; $stats.Margin = New-Thickness 14 0 0 0
        Add-ToGrid $g $stats 2
        $card.Child = $g
        [void]$panel.Children.Add($card)
        $script:PingResults += [pscustomobject]@{ Label = "$($t.Label) ($($t.Host))"; Avg = $m.Avg; Jitter = $m.Jitter; Loss = $m.Loss; Status = $st; Verdict = $verdict }
    }
    Set-Status 'Test réseau terminé.'
}

function Set-Dns([int]$Choice) {
    if (-not $script:Net) { Show-Message 'Aucune connexion détectée.'; return }
    $idx = $script:Net.IfIndex
    $key = [string]$idx
    if (-not $script:Backup.Dns.ContainsKey($key)) {
        $static = Get-RegValue "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$($script:Net.Guid)" 'NameServer'
        $script:Backup.Dns[$key] = [string]$static
        Save-Backup
    }
    $prev = [string](Get-RegValue "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$($script:Net.Guid)" 'NameServer')
    $dnsLog = @(@{ Type = 'dns'; IfIndex = $idx; Guid = [string]$script:Net.Guid; Servers = @($prev -split '[,\s]+' | Where-Object { $_ }) })
    $servers = $DnsChoices[$Choice]
    [void](Invoke-Aide 'dns.regler' @{ Index = $idx; Serveurs = @($servers) })
    $script:Net = Get-ActiveNet
    Update-NetInfo
    Update-BackupSummary
    [void](Add-History "Serveur DNS : $(if ($servers.Count) { $servers -join ', ' } else { 'automatique' })" @() $dnsLog)
    Set-Status 'DNS modifié.'
}

# ---------------------------------------------------------------------------
# Onglet nettoyage
# ---------------------------------------------------------------------------
function Invoke-CleanScan {
    Set-Busy $true
    $panel = $ui.CleanPanel
    $panel.Children.Clear()
    $script:CleanRows = @()
    if (-not $script:CleanChoice) { $script:CleanChoice = @{} }
    foreach ($c in $CleanTargets) {
        if ($c.Optional -and -not (Test-Path -LiteralPath $c.Paths[0])) { continue }
        Set-Status "Calcul : $($c.Titre)..."
        $info = Get-CleanInfo $c 300
        $size = if ($info) { [double]$info.Size } else { 0.0 }
        $count = if ($info) { [int]$info.Count } else { 0 }
        $empty = -not $count
        $id = if ($c.Id) { $c.Id } else { $c.Titre }
        $on = if ($script:CleanChoice.ContainsKey($id)) { $script:CleanChoice[$id] } else { $c.Default -ne $false }

        # Une carte par catégorie : titre, ce que c'est, taille, interrupteur. Toute la carte est cliquable.
        $card = New-Card
        $card.Padding = New-Thickness 18 14 18 14
        $card.Cursor = if ($empty) { [System.Windows.Input.Cursors]::Arrow } else { [System.Windows.Input.Cursors]::Hand }
        if ($empty) { $card.Opacity = 0.55 }
        $g = New-Grid @('*', 'Auto', 'Auto')
        $left = New-Object System.Windows.Controls.StackPanel
        $head = New-Object System.Windows.Controls.WrapPanel
        $tt = New-Text $c.Titre 15 '#FFFFFF' -Semi
        $tt.Margin = New-Thickness 0 0 10 0; $tt.VerticalAlignment = 'Center'
        Set-PackFont $tt
        [void]$head.Children.Add($tt)
        $badge = if ($empty) { New-Badge 'Déjà propre' $Colors.ok } else { New-Badge 'Sans risque' $Colors.ok }
        $badge.VerticalAlignment = 'Center'
        [void]$head.Children.Add($badge)
        [void]$left.Children.Add($head)
        $d = New-Text $c.Desc 12.5 '#A6A1BC'
        $d.Margin = New-Thickness 0 4 0 0
        [void]$left.Children.Add($d)
        if ($count) {
            $vb = New-Button 'Voir les fichiers'
            $vb.HorizontalAlignment = 'Left'; $vb.Margin = New-Thickness 0 10 0 0
            $vb.Tag = @{ Target = $c; Info = $info }
            $vb.Add_Click({ param($s, $e) $x = $s.Tag; Invoke-Safe { Show-CleanFiles $x.Target $x.Info } })
            [void]$left.Children.Add($vb)
        }
        Add-ToGrid $g $left 0
        $right = New-Object System.Windows.Controls.StackPanel
        $right.VerticalAlignment = 'Center'; $right.Margin = New-Thickness 20 0 18 0
        $sz = New-Text $(if ($empty) { '0' } else { Format-Size $size }) 18 $(if ($size -gt 500MB) { $Colors.warn } else { '#FFFFFF' }) -Bold
        $sz.HorizontalAlignment = 'Right'
        [void]$right.Children.Add($sz)
        $nb = New-Text "$count fichier$(if ($count -gt 1) {'s'})" 11.5 '#8E88A8'
        $nb.HorizontalAlignment = 'Right'
        [void]$right.Children.Add($nb)
        Add-ToGrid $g $right 1
        $sw = New-Object System.Windows.Controls.CheckBox
        $sw.Style = $Window.FindResource('Switch')
        $sw.VerticalAlignment = 'Center'
        $sw.IsChecked = [bool]($on -and -not $empty)
        $sw.IsEnabled = -not $empty
        $sw.ToolTip = 'Activé = sera nettoyé'
        Add-ToGrid $g $sw 2
        $card.Child = $g
        $row = @{ Id = $id; Target = $c; CheckBox = $sw; Size = $size; Card = $card }
        $sw.Tag = $row
        $sw.Add_Click({ param($s, $e) $r = $s.Tag; $script:CleanChoice[$r.Id] = [bool]$s.IsChecked; Update-CleanSelection })
        $card.Tag = $row
        if (-not $empty) {
            $card.Add_MouseLeftButtonUp({
                param($s, $e)
                # Clic sur la carte (pas sur l'interrupteur ni sur un bouton) : coche ou décoche
                $x = $e.OriginalSource
                while ($x -and $x -ne $s) { if ($x -is [System.Windows.Controls.Primitives.ButtonBase]) { return }; $x = if ($x -is [System.Windows.Media.Visual]) { [System.Windows.Media.VisualTreeHelper]::GetParent($x) } else { $x.Parent } }
                $r = $s.Tag; $r.CheckBox.IsChecked = -not [bool]$r.CheckBox.IsChecked
                $script:CleanChoice[$r.Id] = [bool]$r.CheckBox.IsChecked
                Update-CleanSelection
            })
        }
        [void]$panel.Children.Add($card)
        $script:CleanRows += $row
    }
    Add-LastCleanInfo
    Update-CleanSelection
    Set-Status 'Analyse du nettoyage terminée.'
}

# Total coché, affiché en direct dans le titre et sur le bouton
function Update-CleanSelection {
    $all = 0.0; $sel = 0.0
    foreach ($r in @($script:CleanRows)) { $all += $r.Size; if ($r.CheckBox.IsChecked) { $sel += $r.Size } }
    $ui.CleanTotal.Text = if (-not $all) { 'Rien à nettoyer : ton PC est déjà propre.' } else { "$(Format-Size $sel) à libérer sur $(Format-Size $all) possibles." }
    $ui.BtnClean.Content = if ($sel -gt 0) { "Nettoyer $(Format-Size $sel)" } else { 'Nettoyer la sélection' }
}

# Dernier nettoyage : résumé et bouton pour ouvrir son journal
function Add-LastCleanInfo {
    $l = Get-Setting 'LastClean' $null
    if (-not $l -or -not $l.Date) { return }
    $g = New-Grid @('*', 'Auto')
    $g.Margin = New-Thickness 4 10 0 0
    $t = New-Text "Dernier nettoyage le $(([datetime]$l.Date).ToString('dd/MM à HH:mm')) : $(Format-Size ([double]$l.Freed)) libérés, $($l.Deleted) fichier$(if ([int]$l.Deleted -gt 1) { 's' }) supprimé$(if ([int]$l.Deleted -gt 1) { 's' }), $($l.Skipped) laissé$(if ([int]$l.Skipped -gt 1) { 's' }) car utilisé$(if ([int]$l.Skipped -gt 1) { 's' }), $($l.Errors) refusé$(if ([int]$l.Errors -gt 1) { 's' })." 12.5 '#A6A1BC'
    $t.VerticalAlignment = 'Center'
    Add-ToGrid $g $t 0
    if ($l.File -and (Test-Path -LiteralPath ([string]$l.File))) {
        $b = New-Button 'Ouvrir le journal'
        $b.Margin = New-Thickness 16 0 0 0
        $b.Tag = [string]$l.File
        $b.Add_Click({ param($s, $e) $f = [string]$s.Tag; Invoke-Safe { Start-Process -FilePath 'notepad.exe' -ArgumentList "`"$f`"" } })
        Add-ToGrid $g $b 1
    }
    [void]$ui.CleanPanel.Children.Add($g)
}

# Les plus gros fichiers d'une catégorie, avant de nettoyer
function Show-CleanFiles($Target, $Info) {
    if ($script:TestRunning) { return }
    Show-TestPanel @{ Tag = 'TMP'; Title = $Target.Titre; Sub = "$($Info.Count) fichier$(if ($Info.Count -gt 1) {'s'}), $(Format-Size ([double]$Info.Size))" }
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    Set-TestState 'info' 'Avant nettoyage'
    $body = $ui.TestBody
    $top = @($Info.Top)
    $intro = New-Text "$(if ($Info.Count -gt $top.Count) { "Les $($top.Count) plus gros fichiers sur $($Info.Count)." } else { 'Tous les fichiers, du plus gros au plus petit.' }) Tout ce qui est dans ce$(if ($Target.Paths.Count -gt 1) {'s'}) dossier$(if ($Target.Paths.Count -gt 1) {'s'}) sera supprimé ; un fichier utilisé par un programme ou créé il y a moins de 24 h est laissé en place. Après le nettoyage, un journal liste chaque fichier." 12.5 '#A6A1BC'
    $intro.Margin = New-Thickness 0 0 0 8
    [void]$body.Children.Add($intro)
    $wp = New-Object System.Windows.Controls.WrapPanel
    $wp.Margin = New-Thickness 0 0 0 10
    foreach ($p in $Target.Paths) {
        if (-not (Test-Path -LiteralPath $p)) { continue }
        $b = New-Button "Ouvrir $(Split-Path $p -Leaf)"
        $b.Margin = New-Thickness 0 0 8 0
        $b.ToolTip = $p; $b.Tag = $p
        $b.Add_Click({ param($s, $e) $x = [string]$s.Tag; Invoke-Safe { Start-Process 'explorer.exe' -ArgumentList "`"$x`"" } })
        [void]$wp.Children.Add($b)
    }
    [void]$body.Children.Add($wp)
    $hd = New-Grid @('*', '110', '80')
    Add-ToGrid $hd (New-Text 'FICHIER' 11 '#655E7E' -Semi) 0
    Add-ToGrid $hd (New-Text 'MODIFIÉ LE' 11 '#655E7E' -Semi) 1
    $hs = New-Text 'TAILLE' 11 '#655E7E' -Semi; $hs.HorizontalAlignment = 'Right'
    Add-ToGrid $hd $hs 2
    [void]$body.Children.Add($hd)
    foreach ($f in $top) {
        $row = New-Grid @('*', '110', '80')
        $row.Margin = New-Thickness 0 3 0 3
        $path = [string]$f[0]
        $short = $path
        foreach ($p in $Target.Paths) { if ($path.StartsWith($p, [StringComparison]::OrdinalIgnoreCase)) { $short = $path.Substring($p.Length).TrimStart('\') } }
        $t = New-Text $short 12 '#EEEBF7'
        $t.TextTrimming = 'CharacterEllipsis'; $t.TextWrapping = 'NoWrap'; $t.ToolTip = $path
        Add-ToGrid $row $t 0
        Add-ToGrid $row (New-Text $(try { ([datetime]$f[2]).ToString('dd/MM/yyyy') } catch { '' }) 12 '#A6A1BC') 1
        $s = New-Text (Format-Size ([double]$f[1])) 12 '#D3CDE3'
        $s.HorizontalAlignment = 'Right'
        Add-ToGrid $row $s 2
        [void]$body.Children.Add($row)
    }
}

function Invoke-Clean {
    $sel = @($script:CleanRows | Where-Object { $_.CheckBox.IsChecked })
    if (-not $sel.Count) { Show-Message "Clique d'abord sur Analyser, puis active ce que tu veux nettoyer."; return }
    Set-Busy $true
    $r = Invoke-CleanTargets @($sel | ForEach-Object { $_.Target })
    Invoke-CleanScan
    $msg = "$(Format-Size $r.Freed) libérés : $($r.Deleted) fichier$(if ($r.Deleted -gt 1) {'s'}) supprimé$(if ($r.Deleted -gt 1) {'s'})$(if ($r.Skipped) { ", $($r.Skipped) laissé$(if ($r.Skipped -gt 1) {'s'}) car utilisé$(if ($r.Skipped -gt 1) {'s'}) par un programme (c'est normal)" })$(if ($r.Errors) { ", $($r.Errors) refusé$(if ($r.Errors -gt 1) {'s'}) par Windows" }).`n`nLe détail fichier par fichier est dans le journal (bouton « Ouvrir le journal » sous la liste)."
    Set-Status "$(Format-Size $r.Freed) libérés."
    Show-Message $msg
}

# ---------------------------------------------------------------------------
# Onglet sauvegarde
# ---------------------------------------------------------------------------
function Get-BackupCount {
    $script:Backup.Registry.Count + $script:Backup.Dns.Count + $script:Backup.Displays.Count + $(if ($script:Backup.PowerScheme) { 1 } else { 0 }) + $(if ($script:Backup.Overlay) { 1 } else { 0 }) +
        @(Get-Setting 'SvcOriginal' @()).Count + @(Get-Setting 'PcfgOriginal' @()).Count
}

function Update-BackupSummary {
    $n = Get-BackupCount
    $ui.BackupSummary.Text = if ($n) {
        "$n réglage$(if ($n -gt 1) {'s'}) modifié$(if ($n -gt 1) {'s'}) par Nevermind. Un clic remet tout comme avant."
    } else {
        "Nevermind n'a encore rien modifié sur ce PC."
    }
}

function Invoke-UndoAll {
    if (-not (Get-BackupCount)) { Show-Message $(if ($script:BackupProbleme) { Get-TexteProblemeSauvegarde } else { "Il n'y a aucun changement à annuler." }); return }
    if (-not (Confirm-Action "Remettre tous les réglages modifiés par Nevermind comme ils étaient avant ?$(if ($script:BackupProbleme) { "`n`nAttention : $(Get-TexteProblemeSauvegarde)" })")) { return }
    Set-Busy $true
    Set-Status 'Restauration des réglages...'
    $errors = Restore-AllSettings
    Set-HistoryAllUndone
    Update-BackupSummary
    Build-GamingTab
    Update-StartupList
    Update-NetInfo
    $msg = 'Tous les réglages ont été remis comme avant. Redémarre le PC pour que tout soit pris en compte.'
    if ($errors) { $msg += "`n`nCertains éléments n'ont pas pu être restaurés :`n" + ($errors -join "`n") }
    Show-Message $msg
    Invoke-Analysis
}

function Export-Report {
    if (-not $script:LastAnalysis) { Invoke-Analysis }
    $a = $script:LastAnalysis
    $dlg = New-Object Microsoft.Win32.SaveFileDialog
    $dlg.Filter = 'Page web (*.html)|*.html'
    $dlg.FileName = "Rapport Nevermind $env:COMPUTERNAME $(Get-Date -Format 'yyyy-MM-dd').html"
    $dlg.InitialDirectory = [Environment]::GetFolderPath('Desktop')
    if ($dlg.ShowDialog($Window) -ne $true) { return }

    $enc = { param($s) [System.Net.WebUtility]::HtmlEncode([string]$s) }
    $infoRows = ($a.Info.Keys | ForEach-Object { "<tr><th>$(& $enc $_)</th><td>$(& $enc $a.Info[$_])</td></tr>" }) -join "`n"
    $findRows = ($a.Findings | ForEach-Object {
        "<div class='f'><span class='dot' style='background:$($Colors[$_.Status])'></span><div><b>$(& $enc $_.Titre)</b><p>$(& $enc $_.Detail)</p></div></div>"
    }) -join "`n"
    $compHtml = ($a.Cards | ForEach-Object {
        $c = $_
        $rows = @()
        foreach ($b in $c.Bars) { $rows += "<tr><th>$(& $enc $b.Label)</th><td>$(& $enc $b.Text) ($([int]$b.Value) % utilisé)</td></tr>" }
        foreach ($k in $c.Lines.Keys) {
            $v = $c.Lines[$k]
            $t = if ($v -is [array]) { $v[0] } else { $v }
            $rows += "<tr><th>$(& $enc $k)</th><td>$(& $enc $t)</td></tr>"
        }
        foreach ($n in $c.Notes) { $rows += "<tr><td colspan='2' style='color:$($Colors[$n.Status])'>$(& $enc $n.Text)</td></tr>" }
        "<div class='c'><div class='ch'><b>$(& $enc $c.Titre)</b><span style='color:$($Colors[$c.Status])'>$(& $enc $StatusLabels[$c.Status])</span></div><div class='sub cs'>$(& $enc $c.Sous)</div><table>$($rows -join '')</table></div>"
    }) -join "`n"
    $pingHtml = ''
    if ($script:PingResults) {
        $pingRows = ($script:PingResults | ForEach-Object {
            "<tr><th>$(& $enc $_.Label)</th><td style='color:$($Colors[$_.Status])'>$($_.Avg) ms, gigue $($_.Jitter) ms, pertes $($_.Loss) %</td></tr>"
        }) -join "`n"
        $pingHtml = "<h2>Réseau</h2><table>$pingRows</table>"
    }

    $html = @"
<!DOCTYPE html>
<html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Rapport Nevermind</title>
<style>
body{margin:0;background:#0D0B14;color:#EEEBF7;font:15px/1.5 'Segoe UI',system-ui,sans-serif}
main{max-width:860px;margin:0 auto;padding:32px 16px}
h1{margin:0;font-size:28px}h1 span{color:#00D9F5}
h2{margin:32px 0 12px;font-size:18px}
.sub{color:#A6A1BC}
.score{display:flex;align-items:center;gap:20px;background:#18151F;border:1px solid #231E33;border-radius:12px;padding:20px;margin-top:24px}
.score b{font-size:48px;color:$($a.Color)}
table{width:100%;border-collapse:collapse;background:#18151F;border:1px solid #231E33;border-radius:12px;overflow:hidden}
th,td{text-align:left;padding:10px 14px;border-bottom:1px solid #231E33;vertical-align:top}
th{color:#A6A1BC;font-weight:normal;width:180px}
.f{display:flex;gap:14px;background:#18151F;border:1px solid #231E33;border-radius:12px;padding:12px 16px;margin-bottom:8px}
.f p{margin:2px 0 0;color:#A6A1BC;font-size:14px}
.dot{flex:none;width:12px;height:12px;border-radius:50%;margin-top:6px}
.c{margin-bottom:14px}
.ch{display:flex;justify-content:space-between;gap:12px;font-size:16px}
.cs{margin:0 0 8px;font-size:13px}
</style></head><body><main>
<h1>Opti<span>Game</span></h1>
<div class="sub">Rapport de $(& $enc $env:COMPUTERNAME), le $($a.Date.ToString('dd/MM/yyyy à HH:mm'))</div>
<div class="score"><b>$($a.Score)</b><div><div style="font-size:20px;font-weight:600">$(& $enc $a.Label)</div><div class="sub">Score d'optimisation gaming sur 100</div></div></div>
<h2>Configuration</h2><table>$infoRows</table>
<h2>Santé des composants</h2>
$compHtml
$pingHtml
<h2>Détail de l'analyse</h2>
$findRows
</main></body></html>
"@
    Set-Content -Path $dlg.FileName -Value $html -Encoding UTF8
    Open-Url $dlg.FileName
    Set-Status 'Rapport exporté.'
}
