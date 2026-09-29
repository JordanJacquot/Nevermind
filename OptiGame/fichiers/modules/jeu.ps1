# OptiGame : mode jeu automatique, profils par jeu et alerte de température.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Mode jeu automatique : ferme des applis quand un jeu démarre, les relance à la fin
# ---------------------------------------------------------------------------
# Seulement des applis qui ne perdent rien si on les ferme (synchronisation, messageries, launchers).
$GameModeApps = @(
    @{ Id = 'onedrive'; Name = 'OneDrive'; Proc = '^OneDrive$'; On = $true; Args = '/background' },
    @{ Id = 'teams'; Name = 'Microsoft Teams'; Proc = '^(ms-teams|Teams)$'; On = $true },
    @{ Id = 'skype'; Name = 'Skype'; Proc = '^Skype$'; On = $true },
    @{ Id = 'adobe'; Name = 'Adobe Creative Cloud'; Proc = '^(Creative Cloud|CCXProcess|CCLibrary|Adobe Desktop Service)$'; On = $true },
    @{ Id = 'dropbox'; Name = 'Dropbox'; Proc = '^Dropbox$'; On = $true },
    @{ Id = 'gdrive'; Name = 'Google Drive'; Proc = '^GoogleDriveFS$'; On = $true },
    @{ Id = 'wallpaper'; Name = 'Wallpaper Engine'; Proc = '^wallpaper(32|64)$'; On = $false },
    @{ Id = 'spotify'; Name = 'Spotify'; Proc = '^Spotify$'; On = $false },
    @{ Id = 'epic'; Name = 'Epic Games Launcher'; Proc = '^EpicGamesLauncher$'; On = $false },
    @{ Id = 'ea'; Name = 'EA app'; Proc = '^EADesktop$'; On = $false },
    @{ Id = 'ubisoft'; Name = 'Ubisoft Connect'; Proc = '^(UbisoftConnect|upc)$'; On = $false }
)
# Noms d'exécutables trop courants pour identifier un jeu à coup sûr.
$GenericExe = '^(game|launcher|start|play|client|main|app|run|win64|win32|bin)$'

function Get-GameModeSelection {
    $s = Get-Setting 'GameModeApps' $null
    if ($null -eq $s) { return @($GameModeApps | Where-Object { $_.On } | ForEach-Object { $_.Id }) }
    @(@($s) | ForEach-Object { [string]$_ })
}

# Liste des jeux (Steam, Epic), calculée une fois en arrière plan.
function Update-GameCache {
    $script:Games = @(Invoke-Async ([scriptblock]::Create("function Get-InstalledGames {${function:Get-InstalledGames}}; Get-InstalledGames")))
    $script:GameIndex = @{}
    foreach ($g in $script:Games) {
        foreach ($e in @($g.Exes)) {
            $base = [IO.Path]::GetFileNameWithoutExtension($e).ToLower()
            if ($base.Length -lt 4 -or $base -match $GenericExe) { continue }
            if (-not $script:GameIndex.ContainsKey($base)) { $script:GameIndex[$base] = @{ Game = $g.Name; Exes = @() } }
            $script:GameIndex[$base].Exes += $e.ToLower()
        }
    }
    Build-GameSections
}

function Start-GameWatch {
    if (-not $script:GameTimer) {
        $script:GameTimer = New-Object System.Windows.Threading.DispatcherTimer
        $script:GameTimer.Interval = [TimeSpan]::FromSeconds(5)
        $script:GameTimer.Add_Tick({ try { Test-GameRunning } catch { Write-Log "Mode jeu: $_" } })
    }
    $script:GameTimer.Start()
    Update-GameModeStatus
}

function Stop-GameWatch {
    if ($script:GameTimer) { $script:GameTimer.Stop() }
    if ($script:GameSession) { Stop-GameSession }
    Update-GameModeStatus
}

function Test-GameRunning {
    if ($script:GameSession) {
        if (-not (Get-Process -Id $script:GameSession.Pid -ErrorAction SilentlyContinue)) { Stop-GameSession }
        return
    }
    if (-not $script:GameIndex -or -not $script:GameIndex.Count) { return }
    foreach ($p in @(Get-Process -Name @($script:GameIndex.Keys) -ErrorAction SilentlyContinue)) {
        $path = try { [string]$p.Path } catch { '' }
        $info = $script:GameIndex[$p.ProcessName.ToLower()]
        if ($path -and $info -and $info.Exes -contains $path.ToLower()) { Start-GameSession $info.Game $p; return }
    }
}

function Start-GameSession([string]$Game, $Proc) {
    $sel = if (Get-Setting 'GameMode' $false) { Get-GameModeSelection } else { @() }
    $closed = @()
    $all = @(Get-Process -ErrorAction SilentlyContinue)
    foreach ($a in @($GameModeApps | Where-Object { $sel -contains $_.Id })) {
        $procs = @($all | Where-Object { $_.ProcessName -match $a.Proc })
        if (-not $procs.Count) { continue }
        $path = @($procs | ForEach-Object { try { [string]$_.Path } catch { '' } } | Where-Object { $_ })[0]
        foreach ($p in $procs) { try { $p.Kill() } catch {} }
        $closed += @{ Name = $a.Name; Path = $path; Args = $a.Args }
    }
    $script:GameSession = @{ Game = $Game; Pid = $Proc.Id; Closed = $closed; Start = Get-Date }
    Write-Log "Mode jeu: $Game lancé, applis fermées: $(($closed | ForEach-Object { $_.Name }) -join ', ')"
    if ((Test-FpsMeasure) -and -not $script:FpsTarget) { Start-FpsTarget $Proc.Id $Game $Proc.ProcessName }
    if ((Test-LagMeasure) -and -not $script:LagSession) { try { Start-LagSession $Game $Proc.Id } catch { Write-Log "Lag: $_" } }
    Update-GameModeStatus
    if ($closed.Count) { Show-Notify 'Mode jeu activé' "$Game : $(($closed | ForEach-Object { $_.Name }) -join ', ') fermé$(if ($closed.Count -gt 1) {'s'}) pendant que tu joues." }
}

function Stop-GameSession {
    $s = $script:GameSession
    $script:GameSession = $null
    if (-not $s) { return }
    if ($script:FpsTarget -and $script:FpsTarget.Pid -eq $s.Pid) { Stop-FpsTarget }
    if ($script:LagSession -and $script:LagSession.Pid -eq $s.Pid) { try { Stop-LagSession } catch { Write-Log "Lag: $_" } }
    $failed = @()
    foreach ($c in $s.Closed) {
        if (-not $c.Path) { $failed += $c.Name; continue }
        $name = [IO.Path]::GetFileNameWithoutExtension($c.Path)
        if (Get-Process -Name $name -ErrorAction SilentlyContinue) { continue }
        try {
            Start-Unelevated $c.Path $c.Args
        } catch { $failed += $c.Name }
    }
    $mins = [int]((Get-Date) - $s.Start).TotalMinutes
    Write-Log "Mode jeu: fin de $($s.Game) après $mins min$(if ($failed) { ", à relancer à la main: $($failed -join ', ')" })"
    Update-GameModeStatus
    if ($s.Closed.Count) {
        Show-Notify 'Fin du mode jeu' $(if ($failed) { "Relance toi même : $($failed -join ', ')." } else { 'Les applis fermées ont été relancées.' })
    }
}

function Update-GameModeStatus {
    if (-not $script:GameModeStatus) { return }
    $t = $script:GameModeStatus
    if (-not (Get-Setting 'GameMode' $false)) {
        $t.Text = 'Désactivé.'; $t.Foreground = Get-Brush '#5B6475'
    } elseif ($script:GameSession) {
        $n = @($script:GameSession.Closed).Count
        $t.Text = "En jeu : $($script:GameSession.Game). $n appli$(if ($n -gt 1) {'s'}) fermée$(if ($n -gt 1) {'s'})."
        $t.Foreground = Get-Brush $Colors.ok
    } else {
        $n = if ($script:Games) { @($script:Games).Count } else { 0 }
        $t.Text = "Actif : en attente d'un jeu ($n jeu$(if ($n -gt 1) {'x'}) surveillé$(if ($n -gt 1) {'s'}))."
        $t.Foreground = Get-Brush $Colors.info
    }
}

function Build-GameModeCard {
    $panel = $ui.GameModePanel
    $panel.Children.Clear()
    $card = New-Card
    $sp = New-Object System.Windows.Controls.StackPanel
    $row = New-SwitchRow 'Fermer des applis pendant que je joue' 'Quand un jeu Steam ou Epic démarre, les applis cochées sont fermées, puis relancées quand tu quittes le jeu.' ([bool](Get-Setting 'GameMode' $false)) {
        param($s, $e)
        $on = [bool]$s.IsChecked
        Set-Setting 'GameMode' $on
        Update-GameWatch
        Update-GameModeStatus
        Set-Status $(if ($on) { 'Mode jeu automatique activé.' } else { 'Mode jeu automatique désactivé.' })
    }
    $row.Margin = New-Thickness 0
    [void]$sp.Children.Add($row)
    $script:GameModeStatus = New-Text '' 12.5 '#5B6475' -Semi
    $script:GameModeStatus.Margin = New-Thickness 0 6 0 0
    [void]$sp.Children.Add($script:GameModeStatus)
    $wrap = New-Object System.Windows.Controls.WrapPanel
    $wrap.Margin = New-Thickness 0 14 0 0
    $sel = Get-GameModeSelection
    foreach ($a in $GameModeApps) {
        $cb = New-Object System.Windows.Controls.CheckBox
        $cb.Content = $a.Name
        $cb.Foreground = Get-Brush '#E6E8EE'
        $cb.Margin = New-Thickness 0 0 20 8
        $cb.IsChecked = $sel -contains $a.Id
        $cb.Tag = $a.Id
        $cb.Add_Click({
            param($s, $e)
            $cur = @(Get-GameModeSelection | Where-Object { $_ -ne $s.Tag })
            if ($s.IsChecked) { $cur += $s.Tag }
            Set-Setting 'GameModeApps' $cur
        })
        [void]$wrap.Children.Add($cb)
    }
    [void]$sp.Children.Add($wrap)
    $n = New-Text 'Ne coche pas le launcher du jeu auquel tu joues (Epic, EA, Ubisoft). Marche tant qu''OptiGame est ouvert, même réduit.' 12 '#5B6475'
    $n.Margin = New-Thickness 0 4 0 0
    [void]$sp.Children.Add($n)
    $card.Child = $sp
    [void]$panel.Children.Add($card)
    Update-GameModeStatus
}

# ---------------------------------------------------------------------------
# Profils par jeu : réglages appliqués par Windows à chaque lancement du jeu
# ---------------------------------------------------------------------------
$IfeoPath = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options'

function Get-GameExeNames($Game) {
    @(@($Game.Exes) | ForEach-Object { [IO.Path]::GetFileName($_) } | Where-Object { [IO.Path]::GetFileNameWithoutExtension($_) -notmatch $GenericExe -and $_.Length -ge 8 } | Select-Object -Unique)
}

function Test-GamePriority($Game) {
    $names = Get-GameExeNames $Game
    if (-not $names.Count) { return $false }
    (Get-RegValue "$IfeoPath\$($names[0])\PerfOptions" 'CpuPriorityClass') -eq 3
}

function Set-GameProfile($Game, [string]$What, [bool]$On) {
    $script:RunLog = New-Object System.Collections.ArrayList
    try {
        if ($What -eq 'priority') {
            foreach ($n in (Get-GameExeNames $Game)) {
                if ($On) { Set-Reg "$IfeoPath\$n\PerfOptions" 'CpuPriorityClass' 3 } else { Clear-Reg "$IfeoPath\$n\PerfOptions" 'CpuPriorityClass' }
            }
        } else {
            foreach ($e in @($Game.Exes)) {
                if ($On) { Set-Reg $DxPath $e 'GpuPreference=2;' 'String' } else { Clear-Reg $DxPath $e }
            }
        }
    } finally { $log = $script:RunLog; $script:RunLog = $null }
    $label = if ($What -eq 'priority') { 'priorité haute' } else { 'carte graphique puissante' }
    [void](Add-History "Profil de $($Game.Name) : $label $(if ($On) { 'activée' } else { 'désactivée' })" @() $log)
    Update-BackupSummary
    Set-Status "$($Game.Name) : $label $(if ($On) { 'activée' } else { 'désactivée' })."
}

function Build-GameProfiles {
    $panel = $ui.GameProfilesPanel
    $panel.Children.Clear()
    if ($null -eq $script:Games) { [void]$panel.Children.Add((New-Text 'Recherche des jeux installés...' 13 '#5B6475')); return }
    $games = @($script:Games | Where-Object { (Get-GameExeNames $_).Count } | Sort-Object { $_.Name })
    if (-not $games.Count) { [void]$panel.Children.Add((New-Text 'Aucun jeu Steam ou Epic trouvé sur ce PC.' 13 '#5B6475')); return }
    $intro = New-Text 'Appliqués à chaque lancement du jeu, même OptiGame fermé. « Priorité haute » : le jeu passe avant les autres programmes.' 12.5 '#9AA3B2'
    $intro.Margin = New-Thickness 0 0 0 10
    [void]$panel.Children.Add($intro)
    $gpuNames = @($script:AnalysisData.GPUs | ForEach-Object { [string]$_.Name } | Where-Object { $_ -notmatch 'Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta|Microsoft Basic' })
    $dual = $gpuNames.Count -ge 2
    foreach ($g in $games) {
        $card = New-Card
        $card.Padding = New-Thickness 16 10 16 10
        $card.Margin = New-Thickness 0 0 0 6
        $row = New-Grid @('*', 'Auto', 'Auto')
        $sp = New-Object System.Windows.Controls.StackPanel
        $sp.VerticalAlignment = 'Center'
        $nm = New-Text $g.Name 14 '#FFFFFF' -Semi
        $nm.TextTrimming = 'CharacterEllipsis'; $nm.TextWrapping = 'NoWrap'
        [void]$sp.Children.Add($nm)
        $exeNames = Get-GameExeNames $g
        $sub = New-Text ($exeNames -join ', ') 11.5 '#5B6475'
        $sub.TextTrimming = 'CharacterEllipsis'; $sub.TextWrapping = 'NoWrap'; $sub.ToolTip = (@($g.Exes) -join "`n")
        [void]$sp.Children.Add($sub)
        Add-ToGrid $row $sp 0
        $opts = @(@('priority', 'Priorité haute', (Test-GamePriority $g)))
        if ($dual) { $opts += , @('gpu', 'Carte puissante', ((Get-GpuPreference $g.Exes[0]) -match 'GpuPreference=2')) }
        $col = 1
        foreach ($o in $opts) {
            $box = New-Object System.Windows.Controls.StackPanel
            $box.Orientation = 'Horizontal'
            $box.Margin = New-Thickness 18 0 0 0
            $box.VerticalAlignment = 'Center'
            $sw = New-Object System.Windows.Controls.CheckBox
            $sw.Style = $Window.FindResource('Switch')
            $sw.IsChecked = [bool]$o[2]
            $sw.Tag = @{ Game = $g; What = $o[0] }
            $sw.Add_Click({ param($s, $e) $x = $s.Tag; $on = [bool]$s.IsChecked; Invoke-Safe { Set-GameProfile $x.Game $x.What $on } })
            [void]$box.Children.Add($sw)
            $lbl = New-Text $o[1] 12.5 '#9AA3B2'
            $lbl.Margin = New-Thickness 8 0 0 0; $lbl.VerticalAlignment = 'Center'
            [void]$box.Children.Add($lbl)
            Add-ToGrid $row $box $col
            $col++
        }
        $card.Child = $row
        [void]$panel.Children.Add($card)
    }
}

function Build-GameSections {
    Build-GameModeCard
    Build-FpsPanel
    Build-GameProfiles
    Build-LagPanel
}

# ---------------------------------------------------------------------------
# Alerte de température (carte graphique NVIDIA : seule mesure fiable sans outil externe)
# ---------------------------------------------------------------------------
function Test-TempAlert {
    $t = $Live.GpuTemp
    if ($null -eq $t) { return }
    if ($t -lt 87) { $script:HotSince = $null; return }
    if (-not $script:HotSince) { $script:HotSince = Get-Date; return }
    if (((Get-Date) - $script:HotSince).TotalSeconds -lt 15) { return }
    if ($script:HotAlerted -and ((Get-Date) - $script:HotAlerted).TotalMinutes -lt 10) { return }
    $script:HotAlerted = Get-Date
    Write-Log "Alerte température: carte graphique à $([int]$t) °C"
    Show-Notify 'Carte graphique très chaude' "Elle est à $([int]$t) °C depuis un moment. Vérifie que les ventilateurs tournent et que le PC respire (poussière, grilles bouchées)."
}

# ---------------------------------------------------------------------------
# Mesure des FPS (PresentMon, outil gratuit d'Intel) : stats de chaque partie et overlay optionnel
# ---------------------------------------------------------------------------
$PresentMonExe = Join-Path $AppDir 'outils-tiers\PresentMon.exe'
$FpsFile = Join-Path $DataDir 'fps.json'
$FpsHotkeyId = 7001

# « Mesurer mes FPS » (les versions 1.0.14 et 1.0.15 n'avaient qu'un réglage, celui de l'overlay).
function Test-FpsMeasure { [bool](Get-Setting 'FpsMeasure' ([bool](Get-Setting 'FpsOverlay' $false))) }
function Test-FpsOverlay { [bool](Get-Setting 'FpsOverlay' $false) }

function Test-GameWatchNeeded { ([bool](Get-Setting 'GameMode' $false)) -or (Test-FpsMeasure) -or (Test-LagMeasure) }

function Update-GameWatch {
    if (Test-GameWatchNeeded) { Start-GameWatch } else { Stop-GameWatch }
}

function Format-PlayTime([double]$Seconds) {
    if ($Seconds -lt 90) { return "$([int]$Seconds) s" }
    $m = [int]($Seconds / 60)
    if ($m -lt 60) { return "$m min" }
    "$([int]($m / 60)) h $('{0:D2}' -f ($m % 60))"
}

# Nom affiché d'une partie : le vrai nom du jeu s'il est connu (les anciennes parties gardaient le nom du dossier).
function Get-SessionName($S) {
    if ($script:GameIndex -and $S.Key -and $script:GameIndex[[string]$S.Key]) { return $script:GameIndex[[string]$S.Key].Game }
    [string]$S.Game
}

function Get-FpsColor([double]$Fps) { if ($Fps -ge 60) { $Colors.ok } elseif ($Fps -ge 30) { $Colors.warn } else { $Colors.bad } }

# ---------------------------------------------------------------------------
# Overlay (visible en fenêtré ou en plein écran fenêtré)
# ---------------------------------------------------------------------------
function Show-FpsOverlay {
    if ($script:Overlay) { return }
    $w = New-Object System.Windows.Window
    $w.WindowStyle = 'None'; $w.AllowsTransparency = $true
    $w.Background = [System.Windows.Media.Brushes]::Transparent
    $w.Topmost = $true; $w.ShowInTaskbar = $false; $w.ShowActivated = $false; $w.Focusable = $false
    $w.SizeToContent = 'WidthAndHeight'; $w.ResizeMode = 'NoResize'; $w.IsHitTestVisible = $false
    $w.Title = 'OptiGame FPS'
    $b = New-Object System.Windows.Controls.Border
    $bg = Get-Brush '#0E1014'; $bg.Opacity = 0.8
    $b.Background = $bg
    $b.CornerRadius = [System.Windows.CornerRadius]::new(10)
    $b.Padding = New-Thickness 12 5 14 7
    $sp = New-Object System.Windows.Controls.StackPanel
    $row = New-Object System.Windows.Controls.StackPanel
    $row.Orientation = 'Horizontal'
    $fps = New-Text '...' 26 $Colors.ok -Bold
    $fps.TextWrapping = 'NoWrap'
    [void]$row.Children.Add($fps)
    $unit = New-Text 'FPS' 12 '#9AA3B2' -Semi
    $unit.VerticalAlignment = 'Bottom'; $unit.Margin = New-Thickness 6 0 0 5
    [void]$row.Children.Add($unit)
    [void]$sp.Children.Add($row)
    $sub = New-Text 'Mesure en cours...' 11.5 '#C9CED8'
    $sub.TextWrapping = 'NoWrap'
    [void]$sp.Children.Add($sub)
    $b.Child = $sp
    $w.Content = $b
    $w.Add_SourceInitialized({ param($s, $e) try { [OGNative]::MakeOverlay((New-Object System.Windows.Interop.WindowInteropHelper $s).Handle) } catch {} })
    $script:Overlay = @{ Win = $w; Fps = $fps; Sub = $sub }
    $script:OverlayTicks = 0
}

function Hide-FpsOverlay {
    if (-not $script:Overlay) { return }
    try { $script:Overlay.Win.Close() } catch {}
    $script:Overlay = $null
}

# En haut à gauche de l'écran où se trouve le jeu.
function Set-OverlayPosition([int]$ProcId) {
    $o = $script:Overlay
    if (-not $o) { return }
    $h = [IntPtr]::Zero
    try { $h = (Get-Process -Id $ProcId -ErrorAction Stop).MainWindowHandle } catch {}
    $scr = if ($h -ne [IntPtr]::Zero) { [System.Windows.Forms.Screen]::FromHandle($h) } else { [System.Windows.Forms.Screen]::PrimaryScreen }
    $src = [System.Windows.PresentationSource]::FromVisual($Window)
    $k = if ($src) { $src.CompositionTarget.TransformToDevice.M11 } else { 1.0 }
    $o.Win.Left = ($scr.Bounds.X + 16) / $k
    $o.Win.Top = ($scr.Bounds.Y + 16) / $k
}

# ---------------------------------------------------------------------------
# Mesure d'une partie
# ---------------------------------------------------------------------------
function Start-FpsTarget([int]$ProcId, [string]$Name, [string]$Exe) {
    if (-not (Test-Path -LiteralPath $PresentMonExe)) { Set-Status 'Mesure des FPS indisponible : PresentMon est absent du dossier de l''app.'; return }
    if ($script:FpsTarget) { Stop-FpsTarget }
    if (-not [FrameMon]::Start($PresentMonExe, $ProcId)) { Write-Log "Mesure des FPS: $([FrameMon]::LastError)"; return }
    $path = try { [string](Get-Process -Id $ProcId -ErrorAction Stop).Path } catch { '' }
    $script:FpsTarget = @{ Pid = $ProcId; Name = $Name; Key = $Exe.ToLower(); Start = Get-Date; Series = (New-Object System.Collections.ArrayList); Ticks = 0; Exclusive = $false; Warned = $false
        Path = $path; Sys = (New-Object System.Collections.ArrayList); ProcCpu = @{}; ProcMem = @{}; ProcSeconds = 0.0; PrevProc = $null; PrevProcTime = $null; OnBattery = $false }
    if (Test-FpsOverlay) { Show-FpsOverlay }
    if (-not $script:FpsTimer) {
        $script:FpsTimer = New-Object System.Windows.Threading.DispatcherTimer
        $script:FpsTimer.Interval = [TimeSpan]::FromMilliseconds(500)
        $script:FpsTimer.Add_Tick({ try { Update-FpsTarget } catch { Write-Log "Mesure des FPS: $_" } })
    }
    $script:FpsTimer.Start()
    Write-Log "Mesure des FPS: $Name"
}

function Stop-FpsTarget {
    $t = $script:FpsTarget
    $script:FpsTarget = $null
    if ($script:FpsTimer) { $script:FpsTimer.Stop() }
    Hide-FpsOverlay
    if (-not $t) { return }
    $s = [FrameMon]::Summary()
    $busy = [FrameMon]::Busy()
    $err = [FrameMon]::LastError
    [FrameMon]::Stop()
    [FrameMon]::Paused = $false
    if ($s[3] -eq 0) { Write-Log "Mesure des FPS: aucune image reçue pour $($t.Name). $err"; return }
    if ($s[4] -lt 30 -or $s[3] -lt 300) { return }
    $diag = $null
    try { $diag = Get-FpsDiagData $t $busy } catch { Write-Log "Diagnostic FPS: $_" }
    $saved = Save-FpsSession $t $s $diag
    Build-FpsPanel
    $open = { Show-Page 1; Set-GamingSubPage 1; Show-FpsSession $saved.Id }.GetNewClosure()
    if ($saved -and (Test-FpsProblem $saved)) {
        Show-Notify "Partie terminée : $($t.Name)" ('{0:N0} FPS en moyenne, avec des chutes. OptiGame a regardé d''où ça vient : clique ici pour voir et corriger.' -f $s[0]) $open
    } else {
        Show-Notify "Partie terminée : $($t.Name)" ('{0} de jeu, {1:N0} FPS en moyenne (1 % bas {2:N0}). Tout était fluide.' -f (Format-PlayTime $s[4]), $s[0], $s[1]) $open
    }
}

# Toutes les 500 ms : jeu au premier plan ou non, overlay, un point de courbe toutes les 5 s.
function Update-FpsTarget {
    $t = $script:FpsTarget
    if (-not $t) { return }
    if (-not (Get-Process -Id $t.Pid -ErrorAction SilentlyContinue)) { Stop-FpsTarget; return }
    $front = [OGNative]::GetForegroundPid() -eq $t.Pid
    [FrameMon]::Paused = -not $front
    $t.Ticks++
    if ($t.Ticks % 10 -eq 0) {
        $v = [FrameMon]::Sample()
        if ($v -gt 0) { [void]$t.Series.Add([math]::Round($v, 1)) }
        if ($front) { Add-FpsSysSample $t }
    }
    if ($t.Ticks % 20 -eq 0) { Add-FpsProcSample $t }
    if ([FrameMon]::LastMode -match 'Legacy') { $t.Exclusive = $true }
    $o = $script:Overlay
    if (-not $o) { return }
    if ($t.Exclusive -and -not $t.Warned) {
        $t.Warned = $true
        Write-Log "Mesure des FPS: $($t.Name) est en plein écran exclusif, l'overlay ne peut pas s'afficher."
        Set-Status "$($t.Name) est en plein écran : le compteur ne peut pas s'afficher par dessus, mais la mesure continue."
    }
    if (-not $front) { if ($o.Win.IsVisible) { $o.Win.Hide() }; return }
    if (-not $o.Win.IsVisible) { Set-OverlayPosition $t.Pid; $o.Win.Show() }
    $l = [FrameMon]::Live()
    if ([FrameMon]::Frames -eq 0) {
        if (((Get-Date) - $t.Start).TotalSeconds -gt 8) { $o.Fps.Text = '?'; $o.Sub.Text = 'Aucune image reçue pour le moment.' }
        return
    }
    $o.Fps.Text = '{0:N0}' -f $l[0]
    $o.Fps.Foreground = Get-Brush (Get-FpsColor $l[0])
    $o.Sub.Text = '1 % bas {0:N0}    moyenne {1:N0}' -f $l[1], $l[2]
    $script:OverlayTicks++
    if ($script:OverlayTicks % 6 -eq 0) { $o.Win.Topmost = $false; $o.Win.Topmost = $true }
}

# Ctrl+Maj+F : lance ou arrête la mesure sur le jeu au premier plan, quel que soit son launcher.
function Switch-FpsManual {
    if (-not (Test-FpsMeasure)) {
        Show-Notify 'Mesure des FPS désactivée' 'Active la dans OptiGame, page Optimisation gaming, onglet Mes parties.'
        return
    }
    if ($script:FpsTarget) {
        $n = $script:FpsTarget.Name
        Stop-FpsTarget
        Set-Status "Mesure des FPS arrêtée ($n)."
        return
    }
    $fg = [OGNative]::GetForegroundPid()
    if ($fg -eq 0 -or $fg -eq $PID) { return }
    $p = Get-Process -Id $fg -ErrorAction SilentlyContinue
    if (-not $p) { return }
    $known = if ($script:GameIndex) { $script:GameIndex[$p.ProcessName.ToLower()] } else { $null }
    $name = if ($known) { $known.Game } elseif ($p.MainWindowTitle) { $p.MainWindowTitle } else { $p.ProcessName }
    Start-FpsTarget $fg $name $p.ProcessName
    Show-Notify 'Mesure des FPS lancée' "$name : appuie de nouveau sur Ctrl + Maj + F pour arrêter."
}

# Le raccourci n'est pris à Windows que si la mesure est activée (sinon Ctrl+Maj+F reste libre pour les autres logiciels).
function Update-FpsHotkey {
    if (Test-FpsMeasure) { Register-FpsHotkey } else { Unregister-FpsHotkey }
}

function Register-FpsHotkey {
    if ($script:HotkeyRegistered) { return }
    try {
        $h = (New-Object System.Windows.Interop.WindowInteropHelper $Window).Handle
        if (-not [OGNative]::AddHotKey($h, $FpsHotkeyId, 0x0006, 0x46)) { Write-Log 'Raccourci Ctrl+Maj+F déjà pris par un autre programme.'; return }
        $script:HotkeyRegistered = $true
        $script:HotkeyHandle = $h
        if ($script:HotkeyHook) { return }
        $script:HotkeyHook = [System.Windows.Interop.HwndSourceHook] {
            param([IntPtr]$hwnd, [int]$msg, [IntPtr]$wParam, [IntPtr]$lParam, [ref]$handled)
            if ($msg -eq 0x0312 -and $wParam.ToInt32() -eq $FpsHotkeyId) {
                try { Switch-FpsManual } catch { Write-Log "Raccourci FPS: $_" }
                $handled.Value = $true
            }
            [IntPtr]::Zero
        }
        [System.Windows.Interop.HwndSource]::FromHwnd($h).AddHook($script:HotkeyHook)
        $script:HotkeyHandle = $h
    } catch { Write-Log "Raccourci FPS: $_" }
}

function Unregister-FpsHotkey {
    if ($script:HotkeyHandle -and $script:HotkeyRegistered) { try { [OGNative]::RemoveHotKey($script:HotkeyHandle, $FpsHotkeyId) } catch {} }
    $script:HotkeyRegistered = $false
}

# ---------------------------------------------------------------------------
# Parties enregistrées
# ---------------------------------------------------------------------------
function Get-FpsSessions {
    if (-not (Test-Path -LiteralPath $FpsFile)) { return @() }
    try {
        $a = ConvertFrom-Json (Get-Content -LiteralPath $FpsFile -Raw -Encoding UTF8)
        @(@($a) | Where-Object { $_ } | ForEach-Object { if (-not $_.Id) { $_ | Add-Member -NotePropertyName Id -NotePropertyValue ([string]$_.Date) -Force }; $_ })
    } catch { @() }
}

# Courbe ramenée à 160 points au plus (largeur du graphique).
function Compress-Series($Values) {
    $v = @($Values)
    if ($v.Count -le 160) { return $v }
    $out = @()
    for ($i = 0; $i -lt 160; $i++) {
        $a = [int][math]::Floor($i * $v.Count / 160); $b = [int][math]::Floor(($i + 1) * $v.Count / 160) - 1
        $out += [math]::Round((($v[$a..$b] | Measure-Object -Average).Average), 1)
    }
    $out
}

function Save-FpsSession($T, $S, $Diag) {
    $new = [pscustomobject]@{
        Id = [guid]::NewGuid().ToString('N').Substring(0, 10); Date = $T.Start.ToString('s'); Game = $T.Name; Key = $T.Key
        Avg = [math]::Round($S[0], 1); Low1 = [math]::Round($S[1], 1); Low01 = [math]::Round($S[2], 1); Seconds = [int]$S[4]; Frames = [int]$S[3]
        Exclusive = [bool]$T.Exclusive; Series = @(Compress-Series $T.Series); Diag = $Diag
    }
    $list = @(@(Get-FpsSessions) + $new | Select-Object -Last 200)
    try { ConvertTo-Json -InputObject $list -Depth 6 -Compress | Set-Content -LiteralPath $FpsFile -Encoding UTF8 } catch { Write-Log "Écriture des FPS impossible: $_" }
    $new
}

# Partie avec un souci de FPS : moins de 60 en moyenne, ou des chutes fortes et fréquentes.
function Test-FpsProblem($S) {
    $perMin = if ($S.Diag) { $S.Diag.Stutters / [math]::Max(1.0, $S.Seconds / 60) } else { 0 }
    ($S.Avg -lt 60) -or ($S.Low1 -lt 0.5 * $S.Avg) -or ($perMin -gt 6)
}

# Date du dernier changement fait par OptiGame (non annulé).
function Get-LastChangeDate {
    if ($null -eq $script:History) { Import-History }
    foreach ($h in $script:History) {
        if ($h.Annule) { continue }
        try { return [datetime]::ParseExact($h.Date, "dd/MM/yyyy 'à' HH:mm", [Globalization.CultureInfo]::InvariantCulture) } catch {}
    }
    $null
}

# Moyenne pondérée par la durée de jeu : { FPS moyen, 1 % bas, secondes }
function Measure-FpsGroup($Sessions) {
    $sec = ($Sessions | Measure-Object Seconds -Sum).Sum
    if (-not $sec) { return $null }
    $avg = 0.0; $low = 0.0
    foreach ($s in $Sessions) { $avg += $s.Avg * $s.Seconds; $low += $s.Low1 * $s.Seconds }
    @(($avg / $sec), ($low / $sec), $sec)
}

function Get-FpsVerdict($S) {
    $ratio = if ($S.Avg -gt 0) { $S.Low1 / $S.Avg } else { 0 }
    if ($S.Avg -lt 30) { return @('bad', 'Moins de 30 FPS en moyenne : le jeu saccade. Baisse la qualité graphique ou la résolution.') }
    if ($ratio -lt 0.5) { return @('warn', 'Moyenne correcte, mais des chutes fréquentes : ce sont elles que tu ressens comme des saccades. Souvent des programmes en arrière plan, un disque plein ou une surchauffe.') }
    if ($S.Avg -lt 60) { return @('warn', 'Jouable, mais en dessous de 60 FPS la fluidité se ressent dans les jeux rapides.') }
    if ($ratio -lt 0.7) { return @('ok', 'Bonne moyenne, avec quelques petites chutes par moments.') }
    @('ok', 'Fluide et régulier : très bonne partie.')
}

# Bloc « avant / après les derniers réglages » pour un jeu (ou $null s'il manque des parties).
function New-FpsCompare($Sessions) {
    $lc = Get-LastChangeDate
    if (-not $lc) { return $null }
    $ss = @($Sessions | Sort-Object { [datetime]$_.Date })
    $before = @($ss | Where-Object { [datetime]$_.Date -lt $lc } | Select-Object -Last 3)
    $after = @($ss | Where-Object { [datetime]$_.Date -ge $lc } | Select-Object -Last 3)
    if (-not $before.Count -or -not $after.Count) { return $null }
    $b = Measure-FpsGroup $before; $a = Measure-FpsGroup $after
    $pct = if ($b[0] -gt 0) { 100 * ($a[0] - $b[0]) / $b[0] } else { 0 }
    $row = New-Grid @('*', '*', 'Auto')
    foreach ($x in @(@(0, "Avant le $($lc.ToString('dd/MM'))", $b, $before.Count), @(1, 'Après', $a, $after.Count))) {
        $sp = New-Object System.Windows.Controls.StackPanel
        [void]$sp.Children.Add((New-Text $x[1] 11.5 '#9AA3B2'))
        [void]$sp.Children.Add((New-Text ('{0:N0} FPS' -f $x[2][0]) 18 '#FFFFFF' -Bold))
        [void]$sp.Children.Add((New-Text ('1 % bas {0:N0}, {1} partie{2}' -f $x[2][1], $x[3], $(if ($x[3] -gt 1) { 's' })) 11.5 '#9AA3B2'))
        Add-ToGrid $row $sp $x[0]
    }
    $col = if ([math]::Abs($pct) -lt 3) { '#9AA3B2' } elseif ($pct -gt 0) { $Colors.ok } else { $Colors.warn }
    $delta = New-Text $(if ([math]::Abs($pct) -lt 3) { 'Pareil' } else { '{0}{1:N0} %' -f $(if ($pct -gt 0) { '+' } else { '' }), $pct }) 20 $col -Bold
    $delta.VerticalAlignment = 'Center'
    Add-ToGrid $row $delta 2
    $row
}

# Fiche d'une partie : jauges, courbe, verdict, comparaison.
function Show-FpsSession([string]$Id) {
    if ($script:TestRunning) { return }
    $all = @(Get-FpsSessions)
    $s = @($all | Where-Object { $_.Id -eq $Id })[0]
    if (-not $s) { return }
    $d = [datetime]$s.Date
    Show-TestPanel @{ Tag = 'FPS'; Title = (Get-SessionName $s); Sub = "Partie du $($d.ToString('dd/MM')) à $($d.ToString('HH:mm')), $(Format-PlayTime $s.Seconds) mesurées" }
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    $v = Get-FpsVerdict $s
    Set-TestState $v[0] $(if ($v[0] -eq 'ok') { 'Bonne partie' } elseif ($v[0] -eq 'warn') { 'À surveiller' } else { 'Saccades' })
    $body = $ui.TestBody
    [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
    $max = [math]::Max(60.0, [math]::Ceiling($s.Avg * 1.25 / 30) * 30)
    [void]$body.Children.Add((New-GaugeRow @(
        (New-Gauge 'FPS moyen' $s.Avg $max '{0:N0}' 'FPS' (Get-FpsColor $s.Avg) 0),
        (New-Gauge '1 % bas' $s.Low1 $max '{0:N0}' 'FPS' (Get-FpsColor $s.Low1) 150),
        (New-Gauge '0,1 % bas' $s.Low01 $max '{0:N0}' 'FPS' (Get-FpsColor $s.Low01) 300)
    )))
    [void]$body.Children.Add((New-Verdict $v[0] $v[1]))
    $dg = Add-FpsDiagnosisView $s $body
    if ($dg.Problem) { Set-TestState $dg.Status $(if ($dg.Status -eq 'bad') { 'Problème trouvé' } else { 'À améliorer' }) }
    $series = @($s.Series | Where-Object { $null -ne $_ })
    if ($series.Count -ge 2) {
        [void]$body.Children.Add((New-SectionTitle 'FPS PENDANT LA PARTIE'))
        $ch = New-LiveChart $Colors.info 'FPS' '{0:N0}'
        $ch.RefValue = $s.Avg; $ch.RefText.Text = ('moyenne {0:N0}' -f $s.Avg)
        # Étirée sur toute la largeur du graphique (160 points), même pour une partie courte.
        for ($k = 0; $k -lt 160; $k++) { [void]$ch.Values.Add([double]$series[[int][math]::Floor($k * $series.Count / 160)]) }
        [void]$body.Children.Add($ch.El)
        Update-Chart $ch
    }
    $cmp = New-FpsCompare @($all | Where-Object { $_.Key -eq $s.Key })
    if ($cmp) {
        [void]$body.Children.Add((New-SectionTitle 'AVANT / APRÈS TES DERNIERS RÉGLAGES'))
        [void]$body.Children.Add($cmp)
    }
    [void]$body.Children.Add((New-Details @(
        @('Durée mesurée', (Format-PlayTime $s.Seconds)),
        @('Images affichées', ('{0:N0}' -f $s.Frames)),
        @('Mode d''affichage', $(if ($s.Exclusive) { 'Plein écran (le compteur ne peut pas s''afficher par dessus)' } else { 'Fenêtré ou plein écran fenêtré' })),
        @('Mesure', 'PresentMon (Intel). Les moments où le jeu n''était pas au premier plan ne comptent pas.')
    )))
}

function New-FpsRow($S) {
    $card = New-Card
    $card.Padding = New-Thickness 16 10 16 10
    $card.Margin = New-Thickness 0 0 0 6
    $card.Cursor = [System.Windows.Input.Cursors]::Hand
    $row = New-Grid @('*', 'Auto', 'Auto')
    $left = New-Object System.Windows.Controls.StackPanel
    $left.VerticalAlignment = 'Center'
    $nm = New-Text (Get-SessionName $S) 14 '#FFFFFF' -Semi
    $nm.TextTrimming = 'CharacterEllipsis'; $nm.TextWrapping = 'NoWrap'
    [void]$left.Children.Add($nm)
    $d = [datetime]$S.Date
    [void]$left.Children.Add((New-Text "$($d.ToString('dd/MM')) à $($d.ToString('HH:mm')), $(Format-PlayTime $S.Seconds)" 11.5 '#9AA3B2'))
    Add-ToGrid $row $left 0
    $mid = New-Object System.Windows.Controls.StackPanel
    $mid.HorizontalAlignment = 'Right'; $mid.VerticalAlignment = 'Center'; $mid.Margin = New-Thickness 12 0 12 0
    $big = New-Text ('{0:N0} FPS' -f $S.Avg) 16 (Get-FpsColor $S.Avg) -Bold
    $big.HorizontalAlignment = 'Right'
    [void]$mid.Children.Add($big)
    $sm = New-Text $(if (Test-FpsProblem $S) { '1 % bas {0:N0}, à vérifier' -f $S.Low1 } else { '1 % bas {0:N0}' -f $S.Low1 }) 11.5 $(if (Test-FpsProblem $S) { $Colors.warn } else { '#9AA3B2' })
    $sm.HorizontalAlignment = 'Right'
    [void]$mid.Children.Add($sm)
    Add-ToGrid $row $mid 1
    $chev = New-Text '›' 22 '#5B6475'
    $chev.VerticalAlignment = 'Center'
    Add-ToGrid $row $chev 2
    $card.Child = $row
    $card.Tag = $S.Id
    $card.Add_MouseEnter({ param($s, $e) $s.Background = Get-Brush '#1C212B' })
    $card.Add_MouseLeave({ param($s, $e) $s.Background = Get-Brush '#181C24' })
    $card.Add_MouseLeftButtonUp({ param($s, $e) Invoke-Safe { Show-FpsSession $s.Tag } })
    $card
}

# Une ligne « interrupteur + titre + une phrase ».
function New-SwitchRow([string]$Title, [string]$Text, [bool]$On, [scriptblock]$OnClick) {
    $row = New-Grid @('*', 'Auto')
    $row.Margin = New-Thickness 0 0 0 10
    $sp = New-Object System.Windows.Controls.StackPanel
    [void]$sp.Children.Add((New-Text $Title 14 '#FFFFFF' -Semi))
    $t = New-Text $Text 12 '#9AA3B2'
    $t.Margin = New-Thickness 0 2 0 0
    [void]$sp.Children.Add($t)
    Add-ToGrid $row $sp 0
    $sw = New-Object System.Windows.Controls.CheckBox
    $sw.Style = $Window.FindResource('Switch')
    $sw.IsChecked = $On
    $sw.VerticalAlignment = 'Center'
    $sw.Margin = New-Thickness 16 0 0 0
    $sw.Add_Click($OnClick)
    Add-ToGrid $row $sw 1
    $row
}

# « Mes FPS ne sont pas normaux » : ouvre le diagnostic de la dernière partie, ou explique comment en mesurer une.
function Invoke-FpsHelp {
    $last = @(Get-FpsSessions | Sort-Object { [datetime]$_.Date } -Descending)[0]
    if ($last -and $last.Diag) { Show-FpsSession $last.Id; return }
    if (-not (Test-FpsMeasure)) { Set-Setting 'FpsMeasure' $true; Update-GameWatch; Update-FpsHotkey; Build-FpsPanel }
    Show-ResultSheet 'Trouvons d''où viennent tes problèmes de FPS' @(
        '1.  La mesure des FPS est activée.',
        '2.  Lance ton jeu et joue au moins 5 minutes, de préférence là où ça rame.',
        '3.  Si ce n''est pas un jeu Steam ou Epic, appuie sur Ctrl + Maj + F en jeu pour lancer la mesure.',
        '4.  Quitte le jeu : OptiGame t''explique d''où vient le problème et ce qu''il peut régler pour toi.') $null 'OptiGame regarde qui freine (carte graphique ou processeur), la température, la mémoire, le disque et les programmes en arrière plan.'
}

# Onglet « Mes parties »
function Build-FpsPanel {
    $panel = $ui.FpsPanel
    if (-not $panel) { return }
    $panel.Children.Clear()
    $card = New-Card
    $card.Margin = New-Thickness 0 0 0 16
    $sp = New-Object System.Windows.Controls.StackPanel
    [void]$sp.Children.Add((New-SwitchRow 'Mesurer mes FPS quand je joue' 'Automatique pour les jeux Steam et Epic. Pour un autre jeu : Ctrl + Maj + F pendant la partie.' (Test-FpsMeasure) {
        param($s, $e)
        $on = [bool]$s.IsChecked
        Set-Setting 'FpsMeasure' $on
        if (-not $on -and $script:FpsTarget) { Stop-FpsTarget }
        Update-GameWatch
        Update-FpsHotkey
        Set-Status $(if ($on) { 'Mesure des FPS activée : lance un jeu.' } else { 'Mesure des FPS désactivée.' })
    }))
    $last = New-SwitchRow 'Afficher le compteur pendant la partie' 'En haut à gauche. Visible en fenêtré ou en plein écran fenêtré, pas en plein écran.' (Test-FpsOverlay) {
        param($s, $e)
        Set-Setting 'FpsOverlay' ([bool]$s.IsChecked)
        if (-not $s.IsChecked) { Hide-FpsOverlay } elseif ($script:FpsTarget) { Show-FpsOverlay }
    }
    $last.Margin = New-Thickness 0
    [void]$sp.Children.Add($last)
    if (-not (Test-Path -LiteralPath $PresentMonExe)) { [void]$sp.Children.Add((New-Text 'PresentMon est absent du dossier de l''app : réinstalle OptiGame.' 12.5 $Colors.warn -Semi)) }
    $card.Child = $sp
    [void]$panel.Children.Add($card)

    $all = @(Get-FpsSessions)
    if (-not $all.Count) {
        $e = New-Text 'Aucune partie mesurée pour l''instant. Joue au moins 30 secondes : tes FPS moyens, tes chutes et la courbe de la partie apparaîtront ici.' 13 '#5B6475'
        [void]$panel.Children.Add($e)
        return
    }
    $recent = @($all | Sort-Object { [datetime]$_.Date } -Descending)
    $cmp = New-FpsCompare @($all | Where-Object { $_.Key -eq $recent[0].Key })
    if ($cmp) {
        [void]$panel.Children.Add((New-Text "$(Get-SessionName $recent[0]) : avant / après tes derniers réglages" 13 '#9AA3B2' -Semi))
        $cc = New-Card
        $cc.Margin = New-Thickness 0 6 0 16
        $cc.Child = $cmp
        [void]$panel.Children.Add($cc)
    }
    $help = New-Button 'Mes FPS ne sont pas normaux : trouver pourquoi' 'BtnPrimary'
    $help.HorizontalAlignment = 'Left'
    $help.Margin = New-Thickness 0 0 0 16
    $help.Add_Click({ Invoke-Safe { Invoke-FpsHelp } })
    [void]$panel.Children.Add($help)
    $h = New-Text 'Dernières parties (clique pour le détail)' 13 '#9AA3B2' -Semi
    $h.Margin = New-Thickness 0 0 0 6
    [void]$panel.Children.Add($h)
    foreach ($s in @($recent | Select-Object -First 12)) { [void]$panel.Children.Add((New-FpsRow $s)) }
}
