# OptiGame : mesures en direct, onglets Gaming, Démarrage, Connexion, Nettoyage et Sauvegarde.
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
                $o = & $sync.Smi '--query-gpu=utilization.gpu,temperature.gpu,memory.used,memory.total,power.draw' '--format=csv,noheader,nounits' 2>$null | Select-Object -First 1
                if ($o) {
                    $v = $o -split ','
                    $sync.Gpu = Num $v[0]; $sync.GpuTemp = Num $v[1]
                    $sync.VramUsed = Num $v[2]; $sync.VramTotal = Num $v[3]; $sync.GpuPower = Num $v[4]
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
        $sync.Updated = Get-Date
        Start-Sleep -Milliseconds $(if ($sync.Fast) { 400 } else { 1500 })
    }
}

function Set-Gauge($Val, $Bar, $Sub, $Value, [string]$Text, [string]$SubText, [double]$Warn = 75, [double]$Bad = 90) {
    if ($null -eq $Value) { $Val.Text = 'N/D'; $Bar.Value = 0; $Sub.Text = $SubText; return }
    $Val.Text = $Text
    $Bar.Value = [math]::Min(100.0, [math]::Max(0.0, [double]$Value))
    $Bar.Foreground = Get-Brush (Get-LoadColor $Value $Warn $Bad)
    $Sub.Text = $SubText
}

function Update-LiveUI {
    if (-not $Live.Updated) { return }
    $cpuSub = if ($Live.CpuPerf -and $Live.BaseMHz) { 'Fréquence {0:N1} GHz' -f ($Live.BaseMHz * $Live.CpuPerf / 100 / 1000) } else { '' }
    Set-Gauge $ui.LiveCpuVal $ui.LiveCpuBar $ui.LiveCpuSub $Live.Cpu "$([int]$Live.Cpu) %" $cpuSub

    if ($Live.RamTotal) {
        $pct = 100 * $Live.RamUsed / $Live.RamTotal
        Set-Gauge $ui.LiveRamVal $ui.LiveRamBar $ui.LiveRamSub $pct "$([int]$pct) %" "$(Format-Size $Live.RamUsed) sur $(Format-Size $Live.RamTotal)" 80 90
    }

    $gpuSub = if ($Live.VramTotal) { "VRAM {0:N1} / {1:N0} Go" -f ($Live.VramUsed / 1024), ($Live.VramTotal / 1024) } else { '' }
    $gpuText = if ($null -ne $Live.Gpu) { "$([int]$Live.Gpu) %" } else { '' }
    Set-Gauge $ui.LiveGpuVal $ui.LiveGpuBar $ui.LiveGpuSub $Live.Gpu $gpuText $gpuSub 101 101

    if ($null -ne $Live.GpuTemp) {
        $ui.LiveTempBox.Visibility = 'Visible'; $ui.LiveGrid.Columns = 4
        $powerSub = if ($null -ne $Live.GpuPower) { 'Consommation {0:N0} W' -f $Live.GpuPower } else { '' }
        Set-Gauge $ui.LiveTempVal $ui.LiveTempBar $ui.LiveTempSub $Live.GpuTemp "$([int]$Live.GpuTemp) °C" $powerSub 80 87
    } else {
        $ui.LiveTempBox.Visibility = 'Collapsed'; $ui.LiveGrid.Columns = 3
    }
    $ui.LiveStamp.Text = "Actualisé à $($Live.Updated.ToString('HH:mm:ss'))"
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
function Set-GamingSubPage([int]$Index) {
    $pages = @($ui.GPageTweaks, $ui.GPageFps, $ui.GPageMode, $ui.GPageProfiles)
    for ($i = 0; $i -lt $pages.Count; $i++) {
        $pages[$i].Visibility = if ($i -eq $Index) { 'Visible' } else { 'Collapsed' }
        $b = $script:GTabs[$i]
        if ($b) {
            $b.Background = Get-Brush $(if ($i -eq $Index) { '#22D37A' } else { '#1A1F29' })
            $b.Child.Foreground = Get-Brush $(if ($i -eq $Index) { '#0B0D10' } else { '#C9CED8' })
        }
    }
    $script:GamingSubPage = $Index
}

function Build-GamingTabs {
    if ($script:GTabs) { return }
    $script:GTabs = @()
    $i = 0
    foreach ($label in 'Réglages Windows', 'Mes parties', 'Mode jeu', 'Profils par jeu') {
        $b = New-Object System.Windows.Controls.Border
        $b.CornerRadius = [System.Windows.CornerRadius]::new(16)
        $b.Padding = New-Thickness 16 7 16 7
        $b.Margin = New-Thickness 0 0 8 0
        $b.Cursor = [System.Windows.Input.Cursors]::Hand
        $t = New-Text $label 13 '#C9CED8' -Semi
        $t.TextWrapping = 'NoWrap'
        $b.Child = $t
        $b.Tag = $i
        $b.Add_MouseLeftButtonUp({ param($s, $e) Set-GamingSubPage ([int]$s.Tag) })
        [void]$ui.GamingTabs.Children.Add($b)
        $script:GTabs += $b
        $i++
    }
    Set-GamingSubPage 0
}

function Build-GamingTab {
    Build-GamingTabs
    $panel = $ui.GamingPanel
    $panel.Children.Clear()
    $script:TweakRows = @()
    $tweaks = @(Get-AvailableTweaks)
    $done = 0
    foreach ($t in $tweaks) {
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
        $title = New-Text $t.Titre 14 $(if ($ok) { '#9AA3B2' } else { '#FFFFFF' }) -Semi
        [void]$sp.Children.Add($title)
        $desc = New-Text $t.Description 12 '#9AA3B2'
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
            if ($t.Recommended -eq $false) { [void]$right.Children.Add((New-Badge 'Optionnel' '#9AA3B2')) }
        }
        if ($t.Reboot) { [void]$right.Children.Add((New-Badge 'Redémarrage' '#9AA3B2')) }
        $chev = New-Text '▾' 14 '#5B6475'
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
    $r = Invoke-Async {
        try { Checkpoint-Computer -Description 'OptiGame' -RestorePointType MODIFY_SETTINGS -ErrorAction Stop; 'OK' }
        catch { $_.Exception.Message }
    }
    if ("$r" -eq 'OK') { Set-Status 'Point de restauration créé.'; return $true }
    Write-Log "Point de restauration: $r"
    Confirm-Action ("Impossible de créer le point de restauration.`n`nMotif: $r`n`n" +
        "La protection du système est peut-être désactivée sur ce PC. Tu peux quand même continuer: " +
        "OptiGame garde une sauvegarde de chaque réglage modifié et peut tout annuler depuis l'onglet Sauvegarde.`n`nContinuer ?")
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
            Set-Status "Application: $($r.Tweak.Titre)..."
            try {
                & $r.Tweak.Apply
                $done += $r.Tweak.Titre
                if ($r.Tweak.Reboot) { $reboot = $true }
            } catch {
                $failed += "$($r.Tweak.Titre): $($_.Exception.Message)"
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
function Get-ExeIcon([string]$Exe) {
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
    try {
        $vi = [Diagnostics.FileVersionInfo]::GetVersionInfo($Item.Exe)
        $company = ([string]$vi.CompanyName).Trim()
        if (-not $isHost) {
            $prod = ([string]$vi.ProductName).Trim(); $desc = ([string]$vi.FileDescription).Trim()
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
        $adv = @{ Kind = 'keep'; Label = 'À garder'; Color = $Colors.warn; Why = 'Pilote ou protection de ton PC : laisse le activé.' }
    } elseif ($text -match $DeviceStartup) {
        $adv = @{ Kind = 'choice'; Label = 'À toi de voir'; Color = '#9AA3B2'; Why = 'Garde le si tu utilises les réglages de ta souris, ton clavier ou tes lumières.' }
    } else {
        $adv = @{ Kind = 'choice'; Label = 'À toi de voir'; Color = '#9AA3B2'; Why = 'Désactive le si tu ne t''en sers pas dès que tu allumes ton PC.' }
    }
    # Applis lancées par un petit programme de mise à jour (Discord...) : on prend l'icône de la vraie appli.
    $iconExe = $Item.Exe
    if ($Item.Commande -match '--processStart\s+"?([^"\s]+\.exe)') {
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
    $script:StartupEntries = @(Get-StartupItems | ForEach-Object { Get-StartupInfo $_ } |
        Sort-Object @{ Expression = { -not $_.Item.Enabled } }, @{ Expression = { $order[$_.Advice.Kind] } }, @{ Expression = { $_.Name } })
    $panel = $ui.StartupPanel
    $panel.Children.Clear()
    foreach ($s in $script:StartupEntries) {
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
            $iconEl.Background = Get-Brush '#262C38'
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
        $sub = New-Text $s.Advice.Why 12.5 '#9AA3B2'
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
        [void]$panel.Children.Add((New-Text 'Aucun programme ne se lance avec Windows.' 14 '#9AA3B2'))
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
        Add-ToGrid $g (New-Text $k 13 '#9AA3B2') 0
        Add-ToGrid $g (New-Text ([string]$rows[$k]) 13) 1
        [void]$panel.Children.Add($g)
    }
    try {
        $dns = [string]$n.Dns
        if (-not $dns) { throw 'DNS inconnu' }
        $ui.DnsCurrent.Text = "DNS actuel: $dns"
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
    $targets += @{ Label = 'Internet: Cloudflare'; Host = '1.1.1.1'; Local = $false }
    $targets += @{ Label = 'Internet: Google'; Host = '8.8.8.8'; Local = $false }

    foreach ($t in $targets) {
        $m = Measure-Latency $t.Host $t.Label
        if ($t.Local) {
            if ($m.Loss -gt 0 -or $m.Avg -gt 5 -or $m.Jitter -gt 3) {
                $st = 'warn'; $verdict = "Réseau local instable. C'est typique du Wi-Fi (murs, distance, voisins): rapproche toi de la box ou passe en câble."
            } else { $st = 'ok'; $verdict = 'Liaison avec ta box rapide et stable.' }
        } else {
            if ($m.Loss -ge 5)       { $st = 'bad';  $verdict = 'Pertes de paquets: tu risques de la téléportation et des tirs qui ne comptent pas.' }
            elseif ($m.Loss -gt 0)   { $st = 'warn'; $verdict = 'Quelques pertes de paquets.' }
            elseif ($m.Jitter -gt 15){ $st = 'warn'; $verdict = 'Ping instable (gigue élevée): sensations de lag irrégulières.' }
            elseif ($m.Avg -lt 30)   { $st = 'ok';   $verdict = 'Excellent pour jouer en ligne.' }
            elseif ($m.Avg -lt 60)   { $st = 'ok';   $verdict = 'Bon pour jouer en ligne.' }
            elseif ($m.Avg -lt 100)  { $st = 'warn'; $verdict = 'Moyen: jouable, mais tu seras désavantagé dans les jeux compétitifs.' }
            else                     { $st = 'bad';  $verdict = 'Ping élevé.' }
        }
        if ($m.Loss -eq 100) { $st = 'bad'; $verdict = 'Aucune réponse (ce serveur bloque peut être le ping).' }

        $card = New-Card
        $g = New-Grid @('Auto', '*', 'Auto')
        $dot = New-Object System.Windows.Shapes.Ellipse
        $dot.Width = 12; $dot.Height = 12; $dot.Fill = Get-Brush $Colors[$st]
        $dot.VerticalAlignment = 'Top'; $dot.Margin = New-Thickness 0 4 14 0
        Add-ToGrid $g $dot 0
        $sp = New-Object System.Windows.Controls.StackPanel
        [void]$sp.Children.Add((New-Text "$($t.Label) ($($t.Host))" 14 '#FFFFFF' -Semi))
        $v = New-Text $verdict 12.5 '#9AA3B2'; $v.Margin = New-Thickness 0 3 0 0
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
    $dnsLog = @(@{ Type = 'dns'; IfIndex = $idx; Servers = @($prev -split '[,\s]+' | Where-Object { $_ }) })
    $servers = $DnsChoices[$Choice]
    if ($servers.Count) { Set-DnsClientServerAddress -InterfaceIndex $idx -ServerAddresses $servers -ErrorAction Stop }
    else { Set-DnsClientServerAddress -InterfaceIndex $idx -ResetServerAddresses -ErrorAction Stop }
    Clear-DnsClientCache
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
    $total = 0
    foreach ($c in $CleanTargets) {
        Set-Status "Calcul: $($c.Titre)..."
        $size = [double](Invoke-Async $SizeScript $c.Paths)
        $total += $size
        $card = New-Card
        $g = New-Grid @('*', 'Auto')
        $cb = New-Object System.Windows.Controls.CheckBox
        $cb.Content = $c.Titre
        $cb.FontSize = 14
        $cb.IsChecked = $size -gt 0
        $cb.VerticalContentAlignment = 'Center'
        Add-ToGrid $g $cb 0
        Add-ToGrid $g (New-Text (Format-Size $size) 14 $(if ($size -gt 500MB) { $Colors.warn } else { '#9AA3B2' }) -Semi) 1
        $card.Child = $g
        [void]$panel.Children.Add($card)
        $script:CleanRows += @{ Target = $c; CheckBox = $cb; Size = $size }
    }
    $ui.CleanTotal.Text = "$(Format-Size $total) peuvent être libérés."
    Set-Status 'Analyse du nettoyage terminée.'
}

function Invoke-Clean {
    $sel = @($script:CleanRows | Where-Object { $_.CheckBox.IsChecked })
    if (-not $sel.Count) { Show-Message "Clique d'abord sur Analyser, puis coche ce que tu veux nettoyer."; return }
    Set-Busy $true
    $freed = 0
    foreach ($r in $sel) {
        Set-Status "Nettoyage: $($r.Target.Titre)..."
        [void](Invoke-Async $CleanScript $r.Target.Paths)
        $after = [double](Invoke-Async $SizeScript $r.Target.Paths)
        $freed += [math]::Max(0.0, $r.Size - $after)
    }
    Invoke-CleanScan
    $msg = "$(Format-Size $freed) libérés. Certains fichiers en cours d'utilisation ont pu être laissés en place, c'est normal."
    Set-Status $msg
    Show-Message $msg
}

# ---------------------------------------------------------------------------
# Onglet sauvegarde
# ---------------------------------------------------------------------------
function Get-BackupCount {
    $script:Backup.Registry.Count + $script:Backup.Dns.Count + $script:Backup.Displays.Count + $(if ($script:Backup.PowerScheme) { 1 } else { 0 }) + $(if ($script:Backup.Overlay) { 1 } else { 0 })
}

function Update-BackupSummary {
    $n = Get-BackupCount
    $ui.BackupSummary.Text = if ($n) {
        "$n réglage$(if ($n -gt 1) {'s'}) modifié$(if ($n -gt 1) {'s'}) par OptiGame. Un clic remet tout comme avant."
    } else {
        "OptiGame n'a encore rien modifié sur ce PC."
    }
}

function Invoke-UndoAll {
    if (-not (Get-BackupCount)) { Show-Message "Il n'y a aucun changement à annuler."; return }
    if (-not (Confirm-Action "Remettre tous les réglages modifiés par OptiGame comme ils étaient avant ?")) { return }
    Set-Busy $true
    Set-Status 'Restauration des réglages...'
    $errors = Restore-AllSettings
    Set-HistoryAllUndone
    Update-BackupSummary
    Build-GamingTab
    Update-StartupList
    Update-NetInfo
    $msg = 'Tous les réglages ont été remis comme avant. Redémarre le PC pour que tout soit pris en compte.'
    if ($errors) { $msg += "`n`nCertains éléments n'ont pas pu être restaurés:`n" + ($errors -join "`n") }
    Show-Message $msg
    Invoke-Analysis
}

function Export-Report {
    if (-not $script:LastAnalysis) { Invoke-Analysis }
    $a = $script:LastAnalysis
    $dlg = New-Object Microsoft.Win32.SaveFileDialog
    $dlg.Filter = 'Page web (*.html)|*.html'
    $dlg.FileName = "Rapport OptiGame $env:COMPUTERNAME $(Get-Date -Format 'yyyy-MM-dd').html"
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
<title>Rapport OptiGame</title>
<style>
body{margin:0;background:#0E1014;color:#E6E8EE;font:15px/1.5 'Segoe UI',system-ui,sans-serif}
main{max-width:860px;margin:0 auto;padding:32px 16px}
h1{margin:0;font-size:28px}h1 span{color:#22D37A}
h2{margin:32px 0 12px;font-size:18px}
.sub{color:#9AA3B2}
.score{display:flex;align-items:center;gap:20px;background:#181C24;border:1px solid #232937;border-radius:12px;padding:20px;margin-top:24px}
.score b{font-size:48px;color:$($a.Color)}
table{width:100%;border-collapse:collapse;background:#181C24;border:1px solid #232937;border-radius:12px;overflow:hidden}
th,td{text-align:left;padding:10px 14px;border-bottom:1px solid #232937;vertical-align:top}
th{color:#9AA3B2;font-weight:normal;width:180px}
.f{display:flex;gap:14px;background:#181C24;border:1px solid #232937;border-radius:12px;padding:12px 16px;margin-bottom:8px}
.f p{margin:2px 0 0;color:#9AA3B2;font-size:14px}
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
    Start-Process $dlg.FileName
    Set-Status 'Rapport exporté.'
}
