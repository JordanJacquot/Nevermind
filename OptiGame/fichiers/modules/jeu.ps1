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
    $sel = Get-GameModeSelection
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
    Update-GameModeStatus
    if ($closed.Count) { Show-Notify 'Mode jeu activé' "$Game : $(($closed | ForEach-Object { $_.Name }) -join ', ') fermé$(if ($closed.Count -gt 1) {'s'}) pendant que tu joues." }
}

function Stop-GameSession {
    $s = $script:GameSession
    $script:GameSession = $null
    if (-not $s) { return }
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
        if ($on) { Start-GameWatch; Set-Status 'Mode jeu automatique activé.' } else { Stop-GameWatch; Set-Status 'Mode jeu automatique désactivé.' }
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
