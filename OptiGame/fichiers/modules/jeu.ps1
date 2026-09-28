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
    if ((Get-Setting 'FpsOverlay' $false) -and -not $script:FpsTarget) { Start-FpsTarget $Proc.Id $Game $Proc.ProcessName }
    Update-GameModeStatus
    if ($closed.Count) { Show-Notify 'Mode jeu activé' "$Game : $(($closed | ForEach-Object { $_.Name }) -join ', ') fermé$(if ($closed.Count -gt 1) {'s'}) pendant que tu joues." }
}

function Stop-GameSession {
    $s = $script:GameSession
    $script:GameSession = $null
    if (-not $s) { return }
    if ($script:FpsTarget -and $script:FpsTarget.Pid -eq $s.Pid) { Stop-FpsTarget }
    $failed = @()
    foreach ($c in $s.Closed) {
        if (-not $c.Path) { $failed += $c.Name; continue }
        $name = [IO.Path]::GetFileNameWithoutExtension($c.Path)
        if (Get-Process -Name $name -ErrorAction SilentlyContinue) { continue }
        try {
            if ($c.Args) { Start-Process -FilePath $c.Path -ArgumentList $c.Args -ErrorAction Stop } else { Start-Process -FilePath $c.Path -ErrorAction Stop }
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
    $head = New-Grid @('*', 'Auto')
    $left = New-Object System.Windows.Controls.StackPanel
    [void]$left.Children.Add((New-Text 'Ferme les applis inutiles quand tu lances un jeu' 14.5 '#FFFFFF' -Semi))
    $d = New-Text 'Quand un jeu Steam ou Epic démarre, OptiGame ferme les applis cochées ci dessous, puis les relance quand tu quittes le jeu. Ça marche tant qu''OptiGame est ouvert (tu peux le réduire).' 12.5 '#9AA3B2'
    $d.Margin = New-Thickness 0 4 0 0
    [void]$left.Children.Add($d)
    $script:GameModeStatus = New-Text '' 12.5 '#5B6475' -Semi
    $script:GameModeStatus.Margin = New-Thickness 0 8 0 0
    [void]$left.Children.Add($script:GameModeStatus)
    Add-ToGrid $head $left 0
    $sw = New-Object System.Windows.Controls.CheckBox
    $sw.Style = $Window.FindResource('Switch')
    $sw.IsChecked = [bool](Get-Setting 'GameMode' $false)
    $sw.VerticalAlignment = 'Top'
    $sw.Margin = New-Thickness 16 2 0 0
    $sw.Add_Click({
        param($s, $e)
        $on = [bool]$s.IsChecked
        Set-Setting 'GameMode' $on
        Update-GameWatch
        Update-GameModeStatus
        Set-Status $(if ($on) { 'Mode jeu automatique activé.' } else { 'Mode jeu automatique désactivé.' })
    })
    Add-ToGrid $head $sw 1
    [void]$sp.Children.Add($head)
    $wrap = New-Object System.Windows.Controls.WrapPanel
    $wrap.Margin = New-Thickness 0 12 0 0
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
    $n = New-Text 'Ne coche pas le launcher d''un jeu auquel tu joues (Epic, EA, Ubisoft) : certains jeux en ont besoin pour fonctionner.' 12 '#5B6475'
    $n.Margin = New-Thickness 0 4 0 0
    [void]$sp.Children.Add($n)
    $card.Child = $sp
    $card.Margin = New-Thickness 0 0 0 10
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
    $intro = New-Text 'Réglages appliqués par Windows à chaque lancement du jeu, même quand OptiGame est fermé. « Priorité haute » fait passer le jeu avant les autres programmes quand le processeur est très occupé (Discord, navigateur, enregistrement...).' 12.5 '#9AA3B2'
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
# Compteur de FPS (PresentMon, outil gratuit d'Intel) : overlay par dessus le jeu et avant / après
# ---------------------------------------------------------------------------
$PresentMonExe = Join-Path $AppDir 'outils-tiers\PresentMon.exe'
$FpsFile = Join-Path $DataDir 'fps.json'
$FpsHotkeyId = 7001

function Test-GameWatchNeeded { ([bool](Get-Setting 'GameMode' $false)) -or ([bool](Get-Setting 'FpsOverlay' $false)) }

function Update-GameWatch {
    if (Test-GameWatchNeeded) { Start-GameWatch } else { Stop-GameWatch }
}

function Format-PlayTime([double]$Seconds) {
    if ($Seconds -lt 90) { return "$([int]$Seconds) s" }
    $m = [int]($Seconds / 60)
    if ($m -lt 60) { return "$m min" }
    "$([int]($m / 60)) h $('{0:D2}' -f ($m % 60))"
}

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
    $brand = New-Text 'OptiGame   Ctrl+Maj+F pour arrêter' 9.5 '#5B6475'
    $brand.TextWrapping = 'NoWrap'; $brand.Margin = New-Thickness 0 2 0 0
    [void]$sp.Children.Add($brand)
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

function Start-FpsTarget([int]$ProcId, [string]$Name, [string]$Exe) {
    if (-not (Test-Path -LiteralPath $PresentMonExe)) { Set-Status 'Compteur de FPS indisponible : PresentMon est absent du dossier de l''app.'; return }
    if ($script:FpsTarget) { Stop-FpsTarget }
    if (-not [FrameMon]::Start($PresentMonExe, $ProcId)) { Write-Log "Compteur de FPS: $([FrameMon]::LastError)"; return }
    $script:FpsTarget = @{ Pid = $ProcId; Name = $Name; Key = $Exe.ToLower(); Start = Get-Date }
    Show-FpsOverlay
    if (-not $script:FpsTimer) {
        $script:FpsTimer = New-Object System.Windows.Threading.DispatcherTimer
        $script:FpsTimer.Interval = [TimeSpan]::FromMilliseconds(500)
        $script:FpsTimer.Add_Tick({ try { Update-FpsOverlay } catch { Write-Log "Compteur de FPS: $_" } })
    }
    $script:FpsTimer.Start()
    Write-Log "Compteur de FPS: mesure de $Name"
}

function Stop-FpsTarget {
    $t = $script:FpsTarget
    $script:FpsTarget = $null
    if ($script:FpsTimer) { $script:FpsTimer.Stop() }
    Hide-FpsOverlay
    if (-not $t) { return }
    $s = [FrameMon]::Summary()
    $err = [FrameMon]::LastError
    [FrameMon]::Stop()
    [FrameMon]::Paused = $false
    if ($s[3] -eq 0) { Write-Log "Compteur de FPS: aucune image reçue pour $($t.Name). $err"; return }
    if ($s[4] -lt 30 -or $s[3] -lt 300) { return }
    Save-FpsSession $t $s
    Show-Notify "Partie terminée : $($t.Name)" ('{0:N0} FPS en moyenne, 1 % bas {1:N0} ({2} mesurées)' -f $s[0], $s[1], (Format-PlayTime $s[4]))
    Build-FpsPanel
}

function Update-FpsOverlay {
    $t = $script:FpsTarget
    if (-not $t) { return }
    if (-not (Get-Process -Id $t.Pid -ErrorAction SilentlyContinue)) { Stop-FpsTarget; return }
    $front = [OGNative]::GetForegroundPid() -eq $t.Pid
    [FrameMon]::Paused = -not $front
    $o = $script:Overlay
    if (-not $o) { return }
    if (-not $front) { if ($o.Win.IsVisible) { $o.Win.Hide() }; return }
    if (-not $o.Win.IsVisible) { Set-OverlayPosition $t.Pid; $o.Win.Show() }
    $l = [FrameMon]::Live()
    if ([FrameMon]::Frames -eq 0) {
        if (((Get-Date) - $t.Start).TotalSeconds -gt 8) {
            $o.Fps.Text = '?'
            $o.Sub.Text = 'Aucune image reçue : mets le jeu en « plein écran fenêtré ».'
        }
        return
    }
    $o.Fps.Text = '{0:N0}' -f $l[0]
    $o.Fps.Foreground = Get-Brush $(if ($l[0] -ge 60) { $Colors.ok } elseif ($l[0] -ge 30) { $Colors.warn } else { $Colors.bad })
    $o.Sub.Text = '1 % bas {0:N0}    moyenne {1:N0}' -f $l[1], $l[2]
    $script:OverlayTicks++
    if ($script:OverlayTicks % 6 -eq 0) { $o.Win.Topmost = $false; $o.Win.Topmost = $true }
}

# Ctrl+Maj+F : lance ou arrête le compteur sur le jeu au premier plan, quel que soit son launcher.
function Switch-FpsManual {
    if (-not (Get-Setting 'FpsOverlay' $false)) {
        Show-Notify 'Compteur de FPS désactivé' 'Active le dans OptiGame, page Optimisation gaming.'
        return
    }
    if ($script:FpsTarget) { Stop-FpsTarget; return }
    $fg = [OGNative]::GetForegroundPid()
    if ($fg -eq 0 -or $fg -eq $PID) { return }
    $p = Get-Process -Id $fg -ErrorAction SilentlyContinue
    if (-not $p) { return }
    $known = if ($script:GameIndex) { $script:GameIndex[$p.ProcessName.ToLower()] } else { $null }
    $name = if ($known) { $known.Game } elseif ($p.MainWindowTitle) { $p.MainWindowTitle } else { $p.ProcessName }
    Start-FpsTarget $fg $name $p.ProcessName
}

function Register-FpsHotkey {
    try {
        $h = (New-Object System.Windows.Interop.WindowInteropHelper $Window).Handle
        if (-not [OGNative]::AddHotKey($h, $FpsHotkeyId, 0x0006, 0x46)) { Write-Log 'Raccourci Ctrl+Maj+F déjà pris par un autre programme.'; return }
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
    if ($script:HotkeyHandle) { try { [OGNative]::RemoveHotKey($script:HotkeyHandle, $FpsHotkeyId) } catch {} }
}

# ---------------------------------------------------------------------------
# Parties mesurées et comparaison avant / après les derniers réglages
# ---------------------------------------------------------------------------
function Get-FpsSessions {
    if (-not (Test-Path -LiteralPath $FpsFile)) { return @() }
    try { $a = ConvertFrom-Json (Get-Content -LiteralPath $FpsFile -Raw -Encoding UTF8); @(@($a) | Where-Object { $_ }) } catch { @() }
}

function Save-FpsSession($T, $S) {
    $new = [pscustomobject]@{ Date = (Get-Date).ToString('s'); Game = $T.Name; Key = $T.Key
        Avg = [math]::Round($S[0], 1); Low1 = [math]::Round($S[1], 1); Low01 = [math]::Round($S[2], 1); Seconds = [int]$S[4]; Frames = [int]$S[3] }
    $list = @(@(Get-FpsSessions) + $new | Select-Object -Last 300)
    try { ConvertTo-Json -InputObject $list -Depth 3 | Set-Content -LiteralPath $FpsFile -Encoding UTF8 } catch { Write-Log "Écriture des FPS impossible: $_" }
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

function Build-FpsPanel {
    $panel = $ui.FpsPanel
    if (-not $panel) { return }
    $panel.Children.Clear()
    $card = New-Card
    $card.Margin = New-Thickness 0 0 0 10
    $sp = New-Object System.Windows.Controls.StackPanel
    $head = New-Grid @('*', 'Auto')
    $left = New-Object System.Windows.Controls.StackPanel
    [void]$left.Children.Add((New-Text 'Compteur de FPS par dessus le jeu' 14.5 '#FFFFFF' -Semi))
    $d = New-Text 'Affiche tes FPS en haut à gauche pendant que tu joues (sans gêner le jeu) et enregistre chaque partie pour comparer avant / après tes réglages. Il se lance tout seul avec les jeux Steam et Epic. Pour un autre jeu, appuie sur Ctrl + Maj + F pendant que tu joues.' 12.5 '#9AA3B2'
    $d.Margin = New-Thickness 0 4 0 0
    [void]$left.Children.Add($d)
    $n = New-Text 'Si le compteur n''apparaît pas, mets le jeu en « plein écran fenêtré » (ou « sans bordure ») dans ses options graphiques. Mesure faite par PresentMon, l''outil gratuit d''Intel.' 12 '#5B6475'
    $n.Margin = New-Thickness 0 6 0 0
    [void]$left.Children.Add($n)
    if (-not (Test-Path -LiteralPath $PresentMonExe)) {
        [void]$left.Children.Add((New-Text 'PresentMon est absent du dossier de l''app : réinstalle OptiGame.' 12.5 $Colors.warn -Semi))
    }
    Add-ToGrid $head $left 0
    $sw = New-Object System.Windows.Controls.CheckBox
    $sw.Style = $Window.FindResource('Switch')
    $sw.IsChecked = [bool](Get-Setting 'FpsOverlay' $false)
    $sw.VerticalAlignment = 'Top'
    $sw.Margin = New-Thickness 16 2 0 0
    $sw.Add_Click({
        param($s, $e)
        $on = [bool]$s.IsChecked
        Set-Setting 'FpsOverlay' $on
        if (-not $on -and $script:FpsTarget) { Stop-FpsTarget }
        Update-GameWatch
        Set-Status $(if ($on) { 'Compteur de FPS activé : lance un jeu (ou Ctrl+Maj+F dans le jeu).' } else { 'Compteur de FPS désactivé.' })
    })
    Add-ToGrid $head $sw 1
    [void]$sp.Children.Add($head)
    $card.Child = $sp
    [void]$panel.Children.Add($card)

    $all = @(Get-FpsSessions)
    if (-not $all.Count) {
        [void]$panel.Children.Add((New-Text 'Aucune partie mesurée pour l''instant. Active le compteur et joue au moins 30 secondes.' 13 '#5B6475'))
        return
    }
    $lc = Get-LastChangeDate
    $groups = @($all | Group-Object Key | Sort-Object { ($_.Group | ForEach-Object { [datetime]$_.Date } | Measure-Object -Maximum).Maximum } -Descending | Select-Object -First 6)
    foreach ($g in $groups) {
        $ss = @($g.Group | Sort-Object { [datetime]$_.Date })
        $last = $ss[-1]
        $c2 = New-Card
        $c2.Padding = New-Thickness 16 12 16 12
        $c2.Margin = New-Thickness 0 0 0 6
        $st = New-Object System.Windows.Controls.StackPanel
        $title = New-Text "$($last.Game)" 14 '#FFFFFF' -Semi
        $title.TextTrimming = 'CharacterEllipsis'; $title.TextWrapping = 'NoWrap'
        [void]$st.Children.Add($title)
        $before = if ($lc) { @($ss | Where-Object { [datetime]$_.Date -lt $lc } | Select-Object -Last 3) } else { @() }
        $after = if ($lc) { @($ss | Where-Object { [datetime]$_.Date -ge $lc } | Select-Object -Last 3) } else { @() }
        if ($before.Count -and $after.Count) {
            $b = Measure-FpsGroup $before; $a = Measure-FpsGroup $after
            $pct = if ($b[0] -gt 0) { 100 * ($a[0] - $b[0]) / $b[0] } else { 0 }
            $row = New-Grid @('*', '*', 'Auto')
            $row.Margin = New-Thickness 0 8 0 0
            $bx = New-Object System.Windows.Controls.StackPanel
            [void]$bx.Children.Add((New-Text "Avant tes réglages du $($lc.ToString('dd/MM'))" 11.5 '#9AA3B2'))
            [void]$bx.Children.Add((New-Text ('{0:N0} FPS' -f $b[0]) 18 '#FFFFFF' -Bold))
            [void]$bx.Children.Add((New-Text ('1 % bas {0:N0}   ({1} parties, {2})' -f $b[1], $before.Count, (Format-PlayTime $b[2])) 11.5 '#9AA3B2'))
            Add-ToGrid $row $bx 0
            $ax = New-Object System.Windows.Controls.StackPanel
            [void]$ax.Children.Add((New-Text 'Après' 11.5 '#9AA3B2'))
            [void]$ax.Children.Add((New-Text ('{0:N0} FPS' -f $a[0]) 18 '#FFFFFF' -Bold))
            [void]$ax.Children.Add((New-Text ('1 % bas {0:N0}   ({1} parties, {2})' -f $a[1], $after.Count, (Format-PlayTime $a[2])) 11.5 '#9AA3B2'))
            Add-ToGrid $row $ax 1
            $col = if ([math]::Abs($pct) -lt 3) { '#9AA3B2' } elseif ($pct -gt 0) { $Colors.ok } else { $Colors.warn }
            $delta = New-Text $(if ([math]::Abs($pct) -lt 3) { 'Pareil' } else { '{0}{1:N0} %' -f $(if ($pct -gt 0) { '+' } else { '' }), $pct }) 20 $col -Bold
            $delta.VerticalAlignment = 'Center'
            Add-ToGrid $row $delta 2
            [void]$st.Children.Add($row)
        } else {
            $l = New-Text ('Dernière partie ({0}, {1}) : {2:N0} FPS en moyenne, 1 % bas {3:N0}' -f ([datetime]$last.Date).ToString('dd/MM à HH:mm'), (Format-PlayTime $last.Seconds), $last.Avg, $last.Low1) 12.5 '#E6E8EE'
            $l.Margin = New-Thickness 0 4 0 0
            [void]$st.Children.Add($l)
            $hint = if (-not $lc) { 'Fais une optimisation dans OptiGame puis rejoue : l''app comparera avant / après.' }
                    elseif (-not $after.Count) { "Rejoue pour voir l'effet de tes réglages du $($lc.ToString('dd/MM'))." }
                    else { 'Pas encore de partie mesurée avant tes derniers réglages : la comparaison arrivera au prochain changement.' }
            [void]$st.Children.Add((New-Text $hint 11.5 '#5B6475'))
        }
        $c2.Child = $st
        [void]$panel.Children.Add($c2)
    }
    $foot = New-Text 'Les FPS changent selon la scène (effets, nombre de joueurs, carte). La comparaison fait la moyenne de tes 3 dernières parties de chaque côté : plus tu joues, plus elle est fiable.' 11.5 '#5B6475'
    $foot.Margin = New-Thickness 0 4 0 0
    [void]$panel.Children.Add($foot)
}
