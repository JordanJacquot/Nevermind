# Nevermind : mode jeu automatique, profils par jeu et alerte de température.
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

# Liste des jeux (tous les launchers, plus ceux ajoutés à la main), calculée une fois en arrière plan.
function Update-GameCache {
    $all = @(Invoke-Async ([scriptblock]::Create("function Get-InstalledGames {${function:Get-InstalledGames}}; Get-InstalledGames")))
    # Dossiers laissés par des jeux désinstallés : pas des jeux, proposés au nettoyage dans la bibliothèque
    $script:Leftovers = @($all | Where-Object { $_.Leftover })
    $found = @($all | Where-Object { -not $_.Leftover })
    $script:Games = @($found) + @(Get-CustomGames | Where-Object { $p = $_.Exes[0]; -not @($found | Where-Object { @($_.Exes) -contains $p }).Count })
    $script:GameIndex = @{}
    foreach ($g in $script:Games) {
        foreach ($e in @($g.Exes)) {
            $base = [IO.Path]::GetFileNameWithoutExtension($e).ToLower()
            if ($base.Length -lt 4 -or $base -match $GenericExe) { continue }
            if (-not $script:GameIndex.ContainsKey($base)) { $script:GameIndex[$base] = @{ Game = $g.Name; Exes = @(); Source = $g.Source; ByPath = @{} } }
            $script:GameIndex[$base].Exes += $e.ToLower()
            # Deux jeux avec le même nom d'exécutable (Dofus et Dofus 2) : reconnus par leur dossier
            $script:GameIndex[$base].ByPath[$e.ToLower()] = $g.Name
        }
    }
    Build-GameSections
    if ($script:LibBuilt) { Update-LibraryView }
}

# Jeux ajoutés à la main (jeu autonome, itch.io, émulateur...) : { Name, Exes, Source = 'Ajouté' }
function Get-CustomGames {
    @(@(Get-Setting 'CustomGames' @()) | Where-Object { $_ -and $_.Exe -and (Test-Path -LiteralPath ([string]$_.Exe)) } |
        ForEach-Object { @{ Name = [string]$_.Name; Exes = @([string]$_.Exe); Source = 'Ajouté'; Custom = $true } })
}

function Add-CustomGame([string]$Exe, [string]$Name) {
    if (-not $Exe) {
        $dlg = New-Object Microsoft.Win32.OpenFileDialog
        $dlg.Title = 'Choisis le programme du jeu (.exe)'
        $dlg.Filter = 'Programme du jeu (*.exe)|*.exe'
        if (-not $dlg.ShowDialog($Window)) { return }
        $Exe = $dlg.FileName
    }
    if (-not $Name) {
        $vi = try { [Diagnostics.FileVersionInfo]::GetVersionInfo($Exe) } catch { $null }
        $Name = @([string]$vi.ProductName, [string]$vi.FileDescription, [IO.Path]::GetFileNameWithoutExtension($Exe)) | Where-Object { $_ -and $_.Trim() -and $_.Length -lt 60 } | Select-Object -First 1
    }
    $base = [IO.Path]::GetFileNameWithoutExtension($Exe).ToLower()
    if ($base.Length -lt 4 -or $base -match $GenericExe) {
        Show-Message "Le nom « $([IO.Path]::GetFileName($Exe)) » est trop courant pour reconnaître le jeu à coup sûr.`n`nChoisis plutôt le programme qui porte le nom du jeu (souvent dans un sous dossier comme Binaries\Win64)." 'Warning'
        return
    }
    $list = @(@(Get-Setting 'CustomGames' @()) | Where-Object { $_ -and [string]$_.Exe -ne $Exe })
    Set-Setting 'CustomGames' @($list + @{ Name = $Name; Exe = $Exe })
    Update-GameCache
    Set-Status "$Name ajouté à tes jeux : Nevermind le reconnaîtra à son lancement."
}

function Remove-CustomGame([string]$Exe) {
    Set-Setting 'CustomGames' @(@(Get-Setting 'CustomGames' @()) | Where-Object { $_ -and [string]$_.Exe -ne $Exe })
    Update-GameCache
    Set-Status 'Jeu retiré de la liste.'
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
    # Une seule lecture des programmes ouverts (demander chaque jeu par son nom coûtait jusqu'à 0,8 s toutes les 5 s)
    $procs = @([Diagnostics.Process]::GetProcesses() | Where-Object { $script:GameIndex.ContainsKey($_.ProcessName.ToLower()) })
    foreach ($p in $procs) {
        $path = try { [string]$p.Path } catch { '' }
        $info = $script:GameIndex[$p.ProcessName.ToLower()]
        if ($path -and $info -and $info.Exes -contains $path.ToLower()) {
            $name = if ($info.ByPath -and $info.ByPath[$path.ToLower()]) { $info.ByPath[$path.ToLower()] } else { $info.Game }
            Start-GameSession $name $p; return
        }
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
    if ($script:LibBuilt) { Update-LibraryView }
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
    try { Add-PlayTime $s.Game $s.Start } catch { Write-Log "Temps de jeu: $_" }
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
        $t.Text = 'Désactivé.'; $t.Foreground = Get-Brush '#655E7E'
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
    $row = New-SwitchRow 'Fermer des applis pendant que je joue' 'Quand un de tes jeux démarre (Steam, Epic, Ubisoft, EA, Battle.net, Riot, GOG, Xbox...), les applis cochées sont fermées, puis relancées quand tu quittes le jeu.' ([bool](Get-Setting 'GameMode' $false)) {
        param($s, $e)
        $on = [bool]$s.IsChecked
        Set-Setting 'GameMode' $on
        Update-GameWatch
        Update-GameModeStatus
        Set-Status $(if ($on) { 'Mode jeu automatique activé.' } else { 'Mode jeu automatique désactivé.' })
    }
    $row.Margin = New-Thickness 0
    [void]$sp.Children.Add($row)
    $script:GameModeStatus = New-Text '' 12.5 '#655E7E' -Semi
    $script:GameModeStatus.Margin = New-Thickness 0 6 0 0
    [void]$sp.Children.Add($script:GameModeStatus)
    $wrap = New-Object System.Windows.Controls.WrapPanel
    $wrap.Margin = New-Thickness 0 14 0 0
    $sel = Get-GameModeSelection
    foreach ($a in $GameModeApps) {
        $cb = New-Object System.Windows.Controls.CheckBox
        $cb.Content = $a.Name
        $cb.Foreground = Get-Brush '#EEEBF7'
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
    $n = New-Text 'Ne coche pas le launcher du jeu auquel tu joues (Epic, EA, Ubisoft). Marche tant que Nevermind est ouvert, même réduit.' 12 '#655E7E'
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
    if ($null -eq $script:Games) { [void]$panel.Children.Add((New-Text 'Recherche des jeux installés...' 13 '#655E7E')); return }
    $games = @($script:Games | Where-Object { (Get-GameExeNames $_).Count } | Sort-Object { $_.Name })
    # En tête : d'où viennent les jeux, et ajout d'un jeu que les launchers ne déclarent pas
    $hd = New-Grid @('*', 'Auto')
    $hd.Margin = New-Thickness 0 0 0 10
    $srcs = @($games | Group-Object { if ($_.Source) { $_.Source } else { 'Steam' } } | Sort-Object Count -Descending | ForEach-Object { "$($_.Name) $($_.Count)" })
    $hl = New-Object System.Windows.Controls.StackPanel
    [void]$hl.Children.Add((New-Text $(if ($games.Count) { "$($games.Count) jeu$(if ($games.Count -gt 1) {'x'}) reconnu$(if ($games.Count -gt 1) {'s'}) : $($srcs -join ', ')." } else { 'Aucun jeu trouvé sur ce PC.' }) 13 '#FFFFFF' -Semi))
    $intro = New-Text 'Un jeu manque (jeu autonome, itch.io, émulateur...) ? Ajoute le : Nevermind le reconnaîtra à son lancement (mode jeu, FPS, lag). Les réglages ci dessous sont appliqués à chaque lancement, même Nevermind fermé ; « Priorité haute » : le jeu passe avant les autres programmes.' 12 '#A6A1BC'
    $intro.Margin = New-Thickness 0 2 0 0
    [void]$hl.Children.Add($intro)
    Add-ToGrid $hd $hl 0
    $add = New-Button 'Ajouter un jeu' 'BtnPrimary'
    $add.Margin = New-Thickness 16 0 0 0; $add.VerticalAlignment = 'Center'
    $add.Add_Click({ Invoke-Safe { Add-CustomGame } })
    Add-ToGrid $hd $add 1
    [void]$panel.Children.Add($hd)
    $gpuNames = @($script:AnalysisData.GPUs | ForEach-Object { [string]$_.Name } | Where-Object { $_ -notmatch 'Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta|Microsoft Basic' })
    $dual = $gpuNames.Count -ge 2
    foreach ($g in $games) {
        Step-UI
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
        $src = if ($g.Source) { $g.Source } else { 'Steam' }
        $sub = New-Text "$src  ·  $($exeNames -join ', ')" 11.5 '#655E7E'
        $sub.TextTrimming = 'CharacterEllipsis'; $sub.TextWrapping = 'NoWrap'; $sub.ToolTip = (@($g.Exes) -join "`n")
        [void]$sp.Children.Add($sub)
        if ($g.Custom) {
            $rm = New-Object System.Windows.Controls.TextBlock
            $rm.Text = 'Retirer de la liste'; $rm.FontSize = 11.5; $rm.Foreground = Get-Brush '#A6A1BC'; $rm.TextDecorations = [System.Windows.TextDecorations]::Underline
            $rm.Cursor = [System.Windows.Input.Cursors]::Hand; $rm.Margin = New-Thickness 0 2 0 0; $rm.HorizontalAlignment = 'Left'
            $rm.Tag = [string]$g.Exes[0]
            $rm.Add_MouseLeftButtonUp({ param($s, $e) $x = [string]$s.Tag; Invoke-Safe { Remove-CustomGame $x } })
            [void]$sp.Children.Add($rm)
        }
        Add-ToGrid $row $sp 0
        # La virgule garde une liste de listes même avec une seule option (sinon PowerShell l'aplatit en lettres)
        $opts = @(, @('priority', 'Priorité haute', (Test-GamePriority $g)))
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
            $lbl = New-Text $o[1] 12.5 '#A6A1BC'
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
    Build-GameModeCard; Step-UI
    Build-FpsPanel; Step-UI
    Build-OverlayPanel; Step-UI
    Build-GameProfiles; Step-UI
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

# Toujours actif : la bibliothèque note le temps de jeu de chaque partie (une lecture des programmes toutes les 5 s)
function Test-GameWatchNeeded { $true }

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
# Style du compteur : « complet » (chiffre, 1 % bas et moyenne sur un fond) ou « discret » (juste « 144 FPS », petit et semi transparent)
function Get-FpsOverlayStyle { if ([string](Get-Setting 'FpsOverlayStyle' 'complet') -eq 'discret') { 'discret' } else { 'complet' } }

# Contenu du compteur, partagé par l'overlay et l'aperçu de la page « Mes parties »
function New-FpsOverlayContent([string]$Style, [string]$Value = '...') {
    if ($Style -eq 'discret') {
        $row = New-Object System.Windows.Controls.StackPanel
        $row.Orientation = 'Horizontal'
        $row.Opacity = 0.7
        # Petite ombre : le chiffre reste lisible sur un décor clair sans fond derrière
        $sh = New-Object System.Windows.Media.Effects.DropShadowEffect
        $sh.Color = [System.Windows.Media.Colors]::Black; $sh.ShadowDepth = 1; $sh.BlurRadius = 3; $sh.Opacity = 0.9
        $row.Effect = $sh
        $fps = New-Text $Value 15 '#FFFFFF' -Semi
        $fps.TextWrapping = 'NoWrap'
        $fps.FontFamily = New-Object System.Windows.Media.FontFamily $MonoFont
        [void]$row.Children.Add($fps)
        $unit = New-Text 'FPS' 10 '#FFFFFF' -Semi
        $unit.VerticalAlignment = 'Bottom'; $unit.Margin = New-Thickness 3 0 0 2
        [void]$row.Children.Add($unit)
        $b = New-Object System.Windows.Controls.Border
        $b.Padding = New-Thickness 4 2 4 2
        $b.Child = $row
        return @{ Root = $b; Fps = $fps; Sub = $null; Discreet = $true }
    }
    # Verre sombre et liseré cyan vers magenta (DA Nevermind) ; le chiffre garde sa couleur (vert = fluide)
    $b = New-Object System.Windows.Controls.Border
    $b.Background = Get-Brush '#D90B0820'
    $b.BorderBrush = New-LinearBrush @('#B000E5FF', '#B0FF2EB5') 0 0 1 1
    $b.BorderThickness = New-Thickness 1 1 1 1
    $b.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $b.Padding = New-Thickness 12 5 14 7
    $sp = New-Object System.Windows.Controls.StackPanel
    $row = New-Object System.Windows.Controls.StackPanel
    $row.Orientation = 'Horizontal'
    $fps = New-Text $Value 26 $Colors.ok -Bold
    $fps.TextWrapping = 'NoWrap'
    $fps.FontFamily = New-Object System.Windows.Media.FontFamily $MonoFont
    [void]$row.Children.Add($fps)
    $unit = New-Text 'FPS' 12 '#A6A1BC' -Semi
    $unit.VerticalAlignment = 'Bottom'; $unit.Margin = New-Thickness 6 0 0 5
    [void]$row.Children.Add($unit)
    [void]$sp.Children.Add($row)
    $sub = New-Text 'Mesure en cours...' 11.5 '#D3CDE3'
    $sub.TextWrapping = 'NoWrap'
    [void]$sp.Children.Add($sub)
    $b.Child = $sp
    @{ Root = $b; Fps = $fps; Sub = $sub; Discreet = $false }
}

function Show-FpsOverlay {
    if ($script:Overlay) { return }
    $w = New-Object System.Windows.Window
    $w.WindowStyle = 'None'; $w.AllowsTransparency = $true
    $w.Background = [System.Windows.Media.Brushes]::Transparent
    $w.Topmost = $true; $w.ShowInTaskbar = $false; $w.ShowActivated = $false; $w.Focusable = $false
    $w.SizeToContent = 'WidthAndHeight'; $w.ResizeMode = 'NoResize'; $w.IsHitTestVisible = $false
    $w.Title = 'Nevermind FPS'
    $c = New-FpsOverlayContent (Get-FpsOverlayStyle)
    $w.Content = $c.Root
    $w.Add_SourceInitialized({ param($s, $e) try { [OGNative]::MakeOverlay((New-Object System.Windows.Interop.WindowInteropHelper $s).Handle) } catch {} })
    # Coin droit ou bas : la taille du compteur change avec le chiffre, il reste collé au bord
    $w.Add_SizeChanged({ if ($script:Overlay -and $script:Overlay.Pid) { Set-OverlayPosition $script:Overlay.Pid } })
    $script:Overlay = @{ Win = $w; Fps = $c.Fps; Sub = $c.Sub; Discreet = $c.Discreet; Pid = 0 }
    $script:OverlayTicks = 0
}

# Changement de style pendant une partie : le compteur est recréé tout de suite
function Set-FpsOverlayStyle([string]$Style) {
    Set-Setting 'FpsOverlayStyle' $Style
    if ($script:Overlay) { Hide-FpsOverlay; if ($script:FpsTarget -and (Test-FpsOverlay)) { Show-FpsOverlay } }
    Build-OverlayPanel
    Set-Status "Compteur de FPS : style $Style."
}

# Coin de l'écran : hg, hd, bg, bd (haut / bas, gauche / droite)
$OverlayCorners = [ordered]@{ hg = 'En haut à gauche'; hd = 'En haut à droite'; bg = 'En bas à gauche'; bd = 'En bas à droite' }
function Get-FpsOverlayCorner { $c = [string](Get-Setting 'FpsOverlayCorner' 'hg'); if ($OverlayCorners.Contains($c)) { $c } else { 'hg' } }
function Set-FpsOverlayCorner([string]$Corner) {
    Set-Setting 'FpsOverlayCorner' $Corner
    if ($script:Overlay -and $script:Overlay.Pid) { Set-OverlayPosition $script:Overlay.Pid }
    Build-OverlayPanel
    Set-Status "Compteur de FPS : $($OverlayCorners[$Corner].ToLower())."
}
function Hide-FpsOverlay {
    if (-not $script:Overlay) { return }
    try { $script:Overlay.Win.Close() } catch {}
    $script:Overlay = $null
}

# Dans le coin choisi de l'écran où se trouve le jeu.
function Set-OverlayPosition([int]$ProcId) {
    $o = $script:Overlay
    if (-not $o) { return }
    $o.Pid = $ProcId
    $h = [IntPtr]::Zero
    try { $h = (Get-Process -Id $ProcId -ErrorAction Stop).MainWindowHandle } catch {}
    $scr = if ($h -ne [IntPtr]::Zero) { [System.Windows.Forms.Screen]::FromHandle($h) } else { [System.Windows.Forms.Screen]::PrimaryScreen }
    # Échelle de l'écran : celle du compteur s'il est affiché (la fenêtre de Nevermind peut être cachée près de l'horloge)
    $src = [System.Windows.PresentationSource]::FromVisual($o.Win)
    if (-not $src) { $src = [System.Windows.PresentationSource]::FromVisual($Window) }
    $k = if ($src) { $src.CompositionTarget.TransformToDevice.M11 } else { 1.0 }
    $b = $scr.Bounds
    $corner = Get-FpsOverlayCorner
    $w = [double]$o.Win.ActualWidth; $hh = [double]$o.Win.ActualHeight
    $o.Win.Left = if ($corner -like '?d') { ($b.X + $b.Width - 16) / $k - $w } else { ($b.X + 16) / $k }
    $o.Win.Top = if ($corner -like 'b?') { ($b.Y + $b.Height - 16) / $k - $hh } else { ($b.Y + 16) / $k }
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
    # Menus et chargements bloqués (30, 60 FPS...) : mis à part, ils ne sont pas des chutes
    $t.Plateau = try { [FrameMon]::Plateau() } catch { @(0, 0) }
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
    $open = { Show-Page 1; Set-GamingSubPage 'fps'; Show-FpsSession $saved.Id }.GetNewClosure()
    if ($saved -and (Test-FpsProblem $saved)) {
        Show-Notify "Partie terminée : $($t.Name)" ('{0:N0} FPS en moyenne, avec des chutes. Nevermind a regardé d''où ça vient : clique ici pour voir et corriger.' -f $s[0]) $open
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
        if (((Get-Date) - $t.Start).TotalSeconds -gt 8) { $o.Fps.Text = '?'; if ($o.Sub) { $o.Sub.Text = 'Aucune image reçue pour le moment.' } }
        return
    }
    $o.Fps.Text = '{0:N0}' -f $l[0]
    if ($o.Sub) {
        $o.Fps.Foreground = Get-Brush (Get-FpsColor $l[0])
        $o.Sub.Text = '1 % bas {0:N0}    moyenne {1:N0}' -f $l[1], $l[2]
    }
    $script:OverlayTicks++
    if ($script:OverlayTicks % 6 -eq 0) { $o.Win.Topmost = $false; $o.Win.Topmost = $true }
}

# Ctrl+Maj+F : lance ou arrête la mesure sur le jeu au premier plan, quel que soit son launcher.
function Switch-FpsManual {
    if (-not (Test-FpsMeasure)) {
        Show-Notify 'Mesure des FPS désactivée' 'Active la dans Nevermind, page Optimisation gaming, onglet Mes parties.'
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
        $h = (New-Object System.Windows.Interop.WindowInteropHelper $Window).EnsureHandle()
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
        MenuSec = $(if ($T.Plateau) { [int]$T.Plateau[0] } else { 0 }); MenuFps = $(if ($T.Plateau) { [int]$T.Plateau[1] } else { 0 })
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

# Date du dernier changement fait par Nevermind (non annulé).
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
        [void]$sp.Children.Add((New-Text $x[1] 11.5 '#A6A1BC'))
        [void]$sp.Children.Add((New-Text ('{0:N0} FPS' -f $x[2][0]) 18 '#FFFFFF' -Bold))
        [void]$sp.Children.Add((New-Text ('1 % bas {0:N0}, {1} partie{2}' -f $x[2][1], $x[3], $(if ($x[3] -gt 1) { 's' })) 11.5 '#A6A1BC'))
        Add-ToGrid $row $sp $x[0]
    }
    $col = if ([math]::Abs($pct) -lt 3) { '#A6A1BC' } elseif ($pct -gt 0) { $Colors.ok } else { $Colors.warn }
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
    if ([int]$s.MenuSec -ge 5) {
        $mn = New-Text "Menus, chargements ou cinématiques bloqués à $([int]$s.MenuFps) FPS pendant $(Format-PlayTime $s.MenuSec) : mis à part, ils ne comptent ni dans ces chiffres ni comme des chutes (les creux de la courbe)." 12 '#A6A1BC'
        $mn.Margin = New-Thickness 2 6 0 0
        [void]$body.Children.Add($mn)
    }
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
    # Jeu mesuré avec Ctrl+Maj+F et inconnu de Nevermind : proposer de l'ajouter pour la prochaine fois
    $exePath = if ($s.Diag) { [string]$s.Diag.Path } else { '' }
    if ($exePath -and (Test-Path -LiteralPath $exePath) -and -not @($script:Games | Where-Object { @($_.Exes) -contains $exePath }).Count) {
        $ag = New-Grid @('*', 'Auto')
        $ag.Margin = New-Thickness 0 12 0 0
        $at = New-Text "Nevermind ne connaît pas encore ce jeu : ajoute le pour qu'il soit reconnu tout seul la prochaine fois (mode jeu, FPS, lag)." 12.5 '#D3CDE3'
        $at.VerticalAlignment = 'Center'
        Add-ToGrid $ag $at 0
        $ab = New-Button 'Ajouter à mes jeux' 'BtnPrimary'
        $ab.Margin = New-Thickness 16 0 0 0
        $ab.Tag = @{ Exe = $exePath; Name = (Get-SessionName $s) }
        $ab.Add_Click({ param($x, $y) $g = $x.Tag; Invoke-Safe { Add-CustomGame $g.Exe $g.Name; $x.IsEnabled = $false; $x.Content = 'Ajouté' } })
        Add-ToGrid $ag $ab 1
        [void]$body.Children.Add($ag)
    }
    [void]$body.Children.Add((New-Details @(
        @('Durée mesurée', "$(Format-PlayTime $s.Seconds) de jeu$(if ([int]$s.MenuSec -ge 5) { " + $(Format-PlayTime $s.MenuSec) de menus bloqués à $([int]$s.MenuFps) FPS" })"),
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
    [void]$left.Children.Add((New-Text "$($d.ToString('dd/MM')) à $($d.ToString('HH:mm')), $(Format-PlayTime $S.Seconds)" 11.5 '#A6A1BC'))
    Add-ToGrid $row $left 0
    $mid = New-Object System.Windows.Controls.StackPanel
    $mid.HorizontalAlignment = 'Right'; $mid.VerticalAlignment = 'Center'; $mid.Margin = New-Thickness 12 0 12 0
    $big = New-Text ('{0:N0} FPS' -f $S.Avg) 16 (Get-FpsColor $S.Avg) -Bold
    $big.HorizontalAlignment = 'Right'
    [void]$mid.Children.Add($big)
    $sm = New-Text $(if (Test-FpsProblem $S) { '1 % bas {0:N0}, à vérifier' -f $S.Low1 } else { '1 % bas {0:N0}' -f $S.Low1 }) 11.5 $(if (Test-FpsProblem $S) { $Colors.warn } else { '#A6A1BC' })
    $sm.HorizontalAlignment = 'Right'
    [void]$mid.Children.Add($sm)
    Add-ToGrid $row $mid 1
    $chev = New-Text '›' 22 '#655E7E'
    $chev.VerticalAlignment = 'Center'
    Add-ToGrid $row $chev 2
    $card.Child = $row
    $card.Tag = $S.Id
    $card.Add_MouseEnter({ param($s, $e) $s.Background = Get-Brush 'card-hover' })
    $card.Add_MouseLeave({ param($s, $e) $s.Background = Get-Brush 'card' })
    $card.Add_MouseLeftButtonUp({ param($s, $e) Invoke-Safe { Show-FpsSession $s.Tag } })
    $card
}

# Une ligne « interrupteur + titre + une phrase ».
function New-SwitchRow([string]$Title, [string]$Text, [bool]$On, [scriptblock]$OnClick) {
    $row = New-Grid @('*', 'Auto')
    $row.Margin = New-Thickness 0 0 0 10
    $sp = New-Object System.Windows.Controls.StackPanel
    [void]$sp.Children.Add((New-Text $Title 14 '#FFFFFF' -Semi))
    $t = New-Text $Text 12 '#A6A1BC'
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
        '3.  Si Nevermind ne reconnaît pas le jeu, appuie sur Ctrl + Maj + F en jeu (ou ajoute le dans Profils par jeu).',
        '4.  Quitte le jeu : Nevermind t''explique d''où vient le problème et ce qu''il peut régler pour toi.') $null 'Nevermind regarde qui freine (carte graphique ou processeur), la température, la mémoire, le disque et les programmes en arrière plan.'
}

# ---------------------------------------------------------------------------
# Onglet « Overlay » (à côté de Trafic) : tout le compteur de FPS au même endroit.
# Grand aperçu sur une scène de jeu (coins cliquables), état en direct, style, position, raccourci.
# ---------------------------------------------------------------------------
$OverlayIndex = 12

# Scène de jeu factice (coucher de soleil néon) : assez claire en haut pour juger la lisibilité du compteur
function New-OverlayScene([double]$W, [double]$H, [double]$Radius = 14) {
    $b = New-Object System.Windows.Controls.Border
    $b.Width = $W; $b.Height = $H
    $b.CornerRadius = [System.Windows.CornerRadius]::new($Radius)
    $b.ClipToBounds = $true
    $b.Background = New-LinearBrush @('#3B2A7A', '#B0508F', '#F59E6B') 0 0 0 0.62
    $cv = New-Object System.Windows.Controls.Canvas
    $cv.Width = $W; $cv.Height = $H
    # Soleil
    $sun = New-Object System.Windows.Shapes.Ellipse
    $sun.Width = $H * 0.42; $sun.Height = $sun.Width
    $sun.Fill = New-LinearBrush @('#FFE38A', '#FF6FA8') 0 0 0 1
    $sun.Effect = New-Glow '#FF9F6B' ($H * 0.12) 0.8
    [System.Windows.Controls.Canvas]::SetLeft($sun, $W * 0.5 - $sun.Width / 2); [System.Windows.Controls.Canvas]::SetTop($sun, $H * 0.62 - $sun.Height * 0.78)
    [void]$cv.Children.Add($sun)
    # Montagnes
    foreach ($m in @(@('#4A2B6E', @(0, 0.62, 0.18, 0.42, 0.34, 0.62)), @('#3A2160', @(0.22, 0.62, 0.42, 0.36, 0.6, 0.62)), @('#4A2B6E', @(0.55, 0.62, 0.78, 0.4, 1, 0.62)))) {
        $pg = New-Object System.Windows.Shapes.Polygon
        $pts = New-Object System.Windows.Media.PointCollection
        $c = $m[1]
        for ($i = 0; $i -lt $c.Count; $i += 2) { [void]$pts.Add([System.Windows.Point]::new($c[$i] * $W, $c[$i + 1] * $H)) }
        $pg.Points = $pts; $pg.Fill = Get-Brush $m[0]
        [void]$cv.Children.Add($pg)
    }
    # Sol en grille néon
    $ground = New-Object System.Windows.Shapes.Rectangle
    $ground.Width = $W; $ground.Height = $H * 0.38
    $ground.Fill = New-LinearBrush @('#1A0F33', '#0B0820') 0 0 0 1
    [System.Windows.Controls.Canvas]::SetTop($ground, $H * 0.62)
    [void]$cv.Children.Add($ground)
    $gridBrush = New-AlphaBrush '#FF2EB5' 110
    for ($i = 0; $i -le 12; $i++) {
        $x = $W * $i / 12
        $ln = New-Object System.Windows.Shapes.Line
        $ln.X1 = $W / 2 + ($x - $W / 2) * 0.15; $ln.Y1 = $H * 0.62; $ln.X2 = $W / 2 + ($x - $W / 2) * 1.6; $ln.Y2 = $H
        $ln.Stroke = $gridBrush; $ln.StrokeThickness = 1
        [void]$cv.Children.Add($ln)
    }
    foreach ($f in 0.66, 0.72, 0.8, 0.9) {
        $ln = New-Object System.Windows.Shapes.Line
        $ln.X1 = 0; $ln.X2 = $W; $ln.Y1 = $H * $f; $ln.Y2 = $H * $f
        $ln.Stroke = $gridBrush; $ln.StrokeThickness = 1
        [void]$cv.Children.Add($ln)
    }
    $b.Child = $cv
    $b
}

# Compteur posé dans un coin d'une scène (aperçu)
function Add-OverlayPreview($Parent, [string]$Style, [string]$Corner, [string]$Value, [double]$Scale = 1.0) {
    $c = New-FpsOverlayContent $Style $Value
    if ($c.Sub) { $c.Sub.Text = '1 % bas 118    moyenne 141' }
    if ($Scale -ne 1.0) { $c.Root.LayoutTransform = New-Object System.Windows.Media.ScaleTransform $Scale, $Scale }
    $c.Root.HorizontalAlignment = if ($Corner -like '?d') { 'Right' } else { 'Left' }
    $c.Root.VerticalAlignment = if ($Corner -like 'b?') { 'Bottom' } else { 'Top' }
    $m = 12 * $Scale
    $c.Root.Margin = New-Thickness $m $m $m $m
    [void]$Parent.Children.Add($c.Root)
    $c
}

function New-OverlayPanelCard([string]$Title, [string]$Text, [string]$Color) {
    $card = New-Card
    $card.Margin = New-Thickness 0 0 0 0
    $sp = New-Object System.Windows.Controls.StackPanel
    $head = New-Object System.Windows.Controls.StackPanel
    $head.Orientation = 'Horizontal'
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 9; $dot.Height = 9; $dot.Fill = Get-Brush $Color; $dot.Effect = New-Glow $Color 10 0.9
    $dot.VerticalAlignment = 'Center'; $dot.Margin = New-Thickness 0 1 10 0
    [void]$head.Children.Add($dot)
    [void]$head.Children.Add((New-Text $Title 15 '#FFFFFF' -Bold))
    [void]$sp.Children.Add($head)
    if ($Text) {
        $d = New-Text $Text 12 '#8E88A8'
        $d.Margin = New-Thickness 19 2 0 14
        [void]$sp.Children.Add($d)
    }
    $card.Child = $sp
    @{ Card = $card; Body = $sp }
}

function Set-FpsOverlayOn([bool]$On) {
    Set-Setting 'FpsOverlay' $On
    if (-not $On) { Hide-FpsOverlay } elseif ($script:FpsTarget) { Show-FpsOverlay }
    Build-FpsPanel
    Build-OverlayPanel
    Set-Status $(if ($On) { 'Compteur de FPS affiché pendant les parties.' } else { 'Compteur de FPS masqué.' })
}

# Mesure des FPS : même effet depuis Mes parties, Overlay ou les Paramètres
function Set-FpsMeasure([bool]$On) {
    Set-Setting 'FpsMeasure' $On
    if (-not $On -and $script:FpsTarget) { Stop-FpsTarget }
    Update-GameWatch; Update-FpsHotkey
    Build-FpsPanel; Build-OverlayPanel
    Set-Status $(if ($On) { 'Mesure des FPS activée : lance un jeu.' } else { 'Mesure des FPS désactivée.' })
}
function Enable-FpsMeasure { Set-FpsMeasure $true }

function Build-OverlayPanel {
    $panel = $ui.OverlayPanel
    if (-not $panel) { return }
    $panel.Children.Clear()
    $style = Get-FpsOverlayStyle
    $corner = Get-FpsOverlayCorner
    $on = Test-FpsOverlay
    $measure = Test-FpsMeasure

    # Ligne 1 : grand aperçu (coins cliquables) et état
    $top = New-Grid @('*', '340')
    $top.Margin = New-Thickness 0 0 0 16
    $hero = New-Object System.Windows.Controls.Border
    $hero.CornerRadius = [System.Windows.CornerRadius]::new(22)
    $hero.Background = Get-Brush 'card'
    $hero.BorderBrush = New-LinearBrush @('#9900E5FF', '#22FFFFFF', '#99FF2EB5') 0 0 1 1
    $hero.BorderThickness = New-Thickness 1 1 1 1
    $hero.Padding = New-Thickness 16 16 16 12
    $hero.Margin = New-Thickness 0 0 16 0
    $hs = New-Object System.Windows.Controls.StackPanel
    $sceneHost = New-Object System.Windows.Controls.Grid
    $sceneHost.Width = 560; $sceneHost.Height = 315
    $sceneHost.HorizontalAlignment = 'Center'
    [void]$sceneHost.Children.Add((New-OverlayScene 560 315 16))
    $pv = Add-OverlayPreview $sceneHost $style $corner '144'
    if (-not $on) { $pv.Root.Opacity = 0.25 }
    $script:OverlayPreview = $pv
    # Coins cliquables : survol en pointillés, clic pour y placer le compteur
    foreach ($k in @($OverlayCorners.Keys)) {
        if ($k -eq $corner) { continue }
        $hot = New-Object System.Windows.Controls.Grid
        $hot.Width = 150; $hot.Height = 74
        $hot.HorizontalAlignment = if ($k -like '?d') { 'Right' } else { 'Left' }
        $hot.VerticalAlignment = if ($k -like 'b?') { 'Bottom' } else { 'Top' }
        $hot.Margin = New-Thickness 8 8 8 8
        $hot.Background = Get-Brush '#01FFFFFF'
        $hot.Cursor = [System.Windows.Input.Cursors]::Hand
        $hot.ToolTip = "Placer le compteur $($OverlayCorners[$k].ToLower())"
        $r = New-Object System.Windows.Shapes.Rectangle
        $r.RadiusX = 10; $r.RadiusY = 10
        $r.Stroke = Get-Brush '#CCFFFFFF'; $r.StrokeThickness = 1.5
        $r.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(4, 3))
        $r.Fill = Get-Brush '#22FFFFFF'
        $r.Opacity = 0
        [void]$hot.Children.Add($r)
        $lbl = New-Text 'Placer ici' 12 '#FFFFFF' -Semi
        $lbl.HorizontalAlignment = 'Center'; $lbl.VerticalAlignment = 'Center'; $lbl.Opacity = 0
        [void]$hot.Children.Add($lbl)
        $hot.Tag = @{ Corner = $k; R = $r; L = $lbl }
        $hot.Add_MouseEnter({ param($s, $e) $s.Tag.R.Opacity = 1; $s.Tag.L.Opacity = 1 })
        $hot.Add_MouseLeave({ param($s, $e) $s.Tag.R.Opacity = 0; $s.Tag.L.Opacity = 0 })
        $hot.Add_MouseLeftButtonUp({ param($s, $e) $x = [string]$s.Tag.Corner; Invoke-Safe { Set-FpsOverlayCorner $x; Build-FpsPanel } })
        [void]$sceneHost.Children.Add($hot)
    }
    [void]$hs.Children.Add($sceneHost)
    $cap = New-Text 'Aperçu en direct. Clique sur un coin de l''écran pour y placer le compteur.' 12 '#8E88A8'
    $cap.HorizontalAlignment = 'Center'; $cap.Margin = New-Thickness 0 10 0 0
    [void]$hs.Children.Add($cap)
    $hero.Child = $hs
    Add-ToGrid $top $hero 0

    # État : interrupteur, ce qui se passe maintenant, raccourci
    $st = New-OverlayPanelCard 'Compteur à l''écran' '' '#00E5FF'
    $st.Card.Padding = New-Thickness 20 18 20 18
    $row = New-SwitchRow 'Afficher le compteur pendant la partie' 'Par dessus le jeu, en fenêtré ou plein écran fenêtré.' $on { param($s, $e) $v = [bool]$s.IsChecked; Invoke-Safe { Set-FpsOverlayOn $v } }
    $row.Margin = New-Thickness 0 14 0 0
    [void]$st.Body.Children.Add($row)
    $state = New-Object System.Windows.Controls.Border
    $state.CornerRadius = [System.Windows.CornerRadius]::new(14)
    $state.Padding = New-Thickness 14 12 14 12
    $state.Margin = New-Thickness 0 6 0 0
    $ss = New-Object System.Windows.Controls.StackPanel
    if (-not $measure) {
        $state.Background = New-AlphaBrush $Colors.warn 30
        $t = New-Text 'La mesure des FPS est coupée : le compteur ne peut pas s''afficher.' 12.5 $Colors.warn -Semi
        $t.TextWrapping = 'Wrap'
        [void]$ss.Children.Add($t)
        $b = New-Button 'Activer la mesure' 'BtnPrimary'
        $b.Margin = New-Thickness 0 10 0 0; $b.HorizontalAlignment = 'Left'
        $b.Add_Click({ Invoke-Safe { Enable-FpsMeasure } })
        [void]$ss.Children.Add($b)
    } else {
        $state.Background = Get-Brush '#12FFFFFF'
        $live = [bool]$script:FpsTarget
        $line = New-Object System.Windows.Controls.StackPanel
        $line.Orientation = 'Horizontal'
        $dot = New-Object System.Windows.Shapes.Ellipse
        $dot.Width = 8; $dot.Height = 8; $dot.VerticalAlignment = 'Center'; $dot.Margin = New-Thickness 0 1 8 0
        $dot.Fill = Get-Brush $(if ($live -and $on) { $Colors.ok } elseif ($on) { '#00E5FF' } else { '#655E7E' })
        if ($live -and $on) { $dot.Effect = New-Glow $Colors.ok 10 0.9 }
        [void]$line.Children.Add($dot)
        $txt = if (-not $on) { 'Compteur masqué' } elseif ($live) { "Affiché sur $($script:FpsTarget.Name)" } else { 'Prêt : il apparaîtra dès que tu lances un jeu' }
        $lt = New-Text $txt 12.5 '#EEEBF7' -Semi
        $lt.TextWrapping = 'Wrap'; $lt.MaxWidth = 230
        [void]$line.Children.Add($lt)
        [void]$ss.Children.Add($line)
    }
    $state.Child = $ss
    [void]$st.Body.Children.Add($state)
    # Raccourci clavier
    $kt = New-Text 'Raccourci Ctrl + Maj + F' 13.5 '#FFFFFF' -Semi
    $kt.Margin = New-Thickness 0 18 0 6
    [void]$st.Body.Children.Add($kt)
    $keys = New-Object System.Windows.Controls.StackPanel
    $keys.Orientation = 'Horizontal'
    $first = $true
    foreach ($k in 'Ctrl', 'Maj', 'F') {
        if (-not $first) { $plus = New-Text '+' 12 '#8E88A8'; $plus.Margin = New-Thickness 6 0 6 0; $plus.VerticalAlignment = 'Center'; [void]$keys.Children.Add($plus) }
        $first = $false
        $kc = New-Object System.Windows.Controls.Border
        $kc.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $kc.Background = Get-Brush '#1AFFFFFF'; $kc.BorderBrush = Get-Brush '#33FFFFFF'; $kc.BorderThickness = New-Thickness 1 1 1 3
        $kc.Padding = New-Thickness 10 4 10 4
        $kx = New-Text $k 12 '#FFFFFF' -Bold
        $kx.FontFamily = New-Object System.Windows.Media.FontFamily $MonoFont
        $kc.Child = $kx
        [void]$keys.Children.Add($kc)
    }
    [void]$st.Body.Children.Add($keys)
    $kd = New-Text 'Jeu non reconnu ? Appuie dessus pendant la partie pour lancer ou arrêter la mesure et le compteur.' 12 '#8E88A8'
    $kd.Margin = New-Thickness 0 8 0 0
    [void]$st.Body.Children.Add($kd)
    Add-ToGrid $top $st.Card 1
    [void]$panel.Children.Add($top)

    # Ligne 2 : style et position
    $row2 = New-Grid @('*', '*')
    $row2.Margin = New-Thickness 0 0 0 16
    $sc = New-OverlayPanelCard 'Style du compteur' 'Choisis ce qui s''affiche par dessus ton jeu.' '#B04BFF'
    $sc.Card.Margin = New-Thickness 0 0 8 0
    $tiles = New-Grid @('*', '*')
    $col = 0
    foreach ($o in @(@('complet', 'Complet', 'Le chiffre, le 1 % bas et la moyenne.'), @('discret', 'Discret', 'Juste le chiffre, petit et semi transparent.'))) {
        $sel = $style -eq $o[0]
        $tile = New-Object System.Windows.Controls.Border
        $tile.CornerRadius = [System.Windows.CornerRadius]::new(16)
        $tile.Padding = New-Thickness 10 10 10 12
        $tile.Margin = New-Thickness $(if ($col) { 6 } else { 0 }) 0 $(if ($col) { 0 } else { 6 }) 0
        $tile.Background = Get-Brush $(if ($sel) { '#18FFFFFF' } else { '#0AFFFFFF' })
        $tile.BorderThickness = New-Thickness $(if ($sel) { 2 } else { 1 }) $(if ($sel) { 2 } else { 1 }) $(if ($sel) { 2 } else { 1 }) $(if ($sel) { 2 } else { 1 })
        $tile.BorderBrush = if ($sel) { New-LinearBrush @('#00E5FF', '#B04BFF') 0 0 1 1 } else { Get-Brush '#1CFFFFFF' }
        if ($sel) { $tile.Effect = New-Glow '#00E5FF' 16 0.35 }
        $tile.Cursor = [System.Windows.Input.Cursors]::Hand
        $tsp = New-Object System.Windows.Controls.StackPanel
        $mini = New-Object System.Windows.Controls.Grid
        $mini.Height = 92; $mini.ClipToBounds = $true
        [void]$mini.Children.Add((New-OverlayScene 220 92 10))
        [void](Add-OverlayPreview $mini $o[0] 'hg' '144' 0.75)
        [void]$tsp.Children.Add($mini)
        $nm = New-Object System.Windows.Controls.StackPanel
        $nm.Orientation = 'Horizontal'; $nm.Margin = New-Thickness 2 10 0 0
        [void]$nm.Children.Add((New-Text $o[1] 14 '#FFFFFF' -Semi))
        if ($sel) { $ck = New-Text ([string][char]0xE73E) 12 '#00E5FF'; $ck.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'; $ck.Margin = New-Thickness 8 3 0 0; [void]$nm.Children.Add($ck) }
        [void]$tsp.Children.Add($nm)
        $ds = New-Text $o[2] 11.5 '#8E88A8'
        $ds.Margin = New-Thickness 2 2 0 0
        [void]$tsp.Children.Add($ds)
        $tile.Child = $tsp
        $tile.Tag = $o[0]
        $tile.Add_MouseLeftButtonUp({ param($s, $e) $x = [string]$s.Tag; Invoke-Safe { Set-FpsOverlayStyle $x; Build-FpsPanel } })
        Add-ToGrid $tiles $tile $col
        $col++
    }
    [void]$sc.Body.Children.Add($tiles)
    Add-ToGrid $row2 $sc.Card 0

    $pc = New-OverlayPanelCard 'Position du compteur' 'Le coin de l''écran du jeu où il s''affiche.' '#FF2EB5'
    $pc.Card.Margin = New-Thickness 8 0 0 0
    $pg = New-Grid @('Auto', '*')
    # Petit écran avec un bouton par coin
    $scr = New-Object System.Windows.Controls.Grid
    $scr.Width = 196; $scr.Height = 110
    $frame = New-Object System.Windows.Controls.Border
    $frame.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $frame.Background = Get-Brush '#0AFFFFFF'; $frame.BorderBrush = Get-Brush '#33FFFFFF'; $frame.BorderThickness = New-Thickness 2 2 2 2
    [void]$scr.Children.Add($frame)
    foreach ($k in @($OverlayCorners.Keys)) {
        $sel = $k -eq $corner
        $cb = New-Object System.Windows.Controls.Border
        $cb.Width = 46; $cb.Height = 26
        $cb.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $cb.HorizontalAlignment = if ($k -like '?d') { 'Right' } else { 'Left' }
        $cb.VerticalAlignment = if ($k -like 'b?') { 'Bottom' } else { 'Top' }
        $cb.Margin = New-Thickness 8 8 8 8
        $cb.Background = if ($sel) { New-LinearBrush @('#00E5FF', '#B04BFF') 0 0 1 0 } else { Get-Brush '#16FFFFFF' }
        if ($sel) { $cb.Effect = New-Glow '#00E5FF' 12 0.6 }
        $cb.Cursor = [System.Windows.Input.Cursors]::Hand
        $cb.ToolTip = $OverlayCorners[$k]
        $ct = New-Text 'FPS' 10 $(if ($sel) { '#08060F' } else { '#8E88A8' }) -Bold
        $ct.HorizontalAlignment = 'Center'; $ct.VerticalAlignment = 'Center'
        $cb.Child = $ct
        $cb.Tag = $k
        $cb.Add_MouseEnter({ param($s, $e) if ($s.Tag -ne (Get-FpsOverlayCorner)) { $s.Background = Get-Brush '#2AFFFFFF' } })
        $cb.Add_MouseLeave({ param($s, $e) if ($s.Tag -ne (Get-FpsOverlayCorner)) { $s.Background = Get-Brush '#16FFFFFF' } })
        $cb.Add_MouseLeftButtonUp({ param($s, $e) $x = [string]$s.Tag; Invoke-Safe { Set-FpsOverlayCorner $x; Build-FpsPanel } })
        [void]$scr.Children.Add($cb)
    }
    Add-ToGrid $pg $scr 0
    $pinfo = New-Object System.Windows.Controls.StackPanel
    $pinfo.VerticalAlignment = 'Center'; $pinfo.Margin = New-Thickness 18 0 0 0
    [void]$pinfo.Children.Add((New-Text $OverlayCorners[$corner] 16 '#FFFFFF' -Bold))
    $pt = New-Text 'Il reste collé au bord, même quand le chiffre change de taille.' 12 '#8E88A8'
    $pt.Margin = New-Thickness 0 4 0 0
    [void]$pinfo.Children.Add($pt)
    Add-ToGrid $pg $pinfo 1
    [void]$pc.Body.Children.Add($pg)
    Add-ToGrid $row2 $pc.Card 1
    [void]$panel.Children.Add($row2)

    # Bon à savoir
    $tip = New-OverlayPanelCard 'Bon à savoir' '' '#4EA8FF'
    $tt = New-Text "En plein écran exclusif, Windows ne laisse rien s'afficher par dessus le jeu : choisis « plein écran fenêtré » ou « sans bordure » dans les options du jeu. La mesure, elle, continue quand même.`nTes FPS de chaque partie (moyenne, chutes, courbe) sont gardés dans Optimisation gaming, onglet Mes parties." 12.5 '#B9B3CC'
    $tt.Margin = New-Thickness 19 8 0 0
    [void]$tip.Body.Children.Add($tt)
    [void]$panel.Children.Add($tip.Card)
    Start-OverlayPreviewAnim
}

# Le chiffre de l'aperçu bouge comme en jeu, tant que l'onglet est affiché
function Start-OverlayPreviewAnim {
    if (-not $script:OverlayPreviewTimer) {
        $script:OverlayPreviewTimer = New-Object System.Windows.Threading.DispatcherTimer
        $script:OverlayPreviewTimer.Interval = [TimeSpan]::FromMilliseconds(450)
        $script:OverlayPreviewTimer.Add_Tick({
            if ($ui.Tabs.SelectedIndex -ne $OverlayIndex -or -not $Window.IsVisible -or -not $script:OverlayPreview) { $script:OverlayPreviewTimer.Stop(); return }
            $v = Get-Random -Minimum 136 -Maximum 149
            $script:OverlayPreview.Fps.Text = [string]$v
        })
    }
    if ($ui.Tabs.SelectedIndex -eq $OverlayIndex) { $script:OverlayPreviewTimer.Start() }
}

# Onglet « Mes parties »
function Build-FpsPanel {
    $panel = $ui.FpsPanel
    if (-not $panel) { return }
    $panel.Children.Clear()
    $card = New-Card
    $card.Margin = New-Thickness 0 0 0 16
    $sp = New-Object System.Windows.Controls.StackPanel
    [void]$sp.Children.Add((New-SwitchRow 'Mesurer mes FPS quand je joue' 'Automatique pour tes jeux (Steam, Epic, Ubisoft, EA, Battle.net, Riot, GOG, Xbox...). Pour un autre jeu : Ctrl + Maj + F pendant la partie.' (Test-FpsMeasure) {
        param($s, $e)
        $on = [bool]$s.IsChecked
        Invoke-Safe { Set-FpsMeasure $on }
    }))
    # Le compteur à l'écran a son propre onglet
    $og = New-Grid @('*', 'Auto')
    $og.Margin = New-Thickness 0 4 0 0
    $ot = New-Text "Compteur à l'écran : $(if (Test-FpsOverlay) { "affiché, style $(Get-FpsOverlayStyle), $($OverlayCorners[(Get-FpsOverlayCorner)].ToLower())" } else { 'masqué' })." 12.5 '#A6A1BC'
    $ot.VerticalAlignment = 'Center'
    Add-ToGrid $og $ot 0
    $ob = New-Button 'Régler le compteur'
    $ob.Margin = New-Thickness 16 0 0 0
    $ob.Add_Click({ Show-Page $OverlayIndex })
    Add-ToGrid $og $ob 1
    [void]$sp.Children.Add($og)
    if (-not (Test-Path -LiteralPath $PresentMonExe)) { [void]$sp.Children.Add((New-Text 'PresentMon est absent du dossier de l''app : réinstalle Nevermind.' 12.5 $Colors.warn -Semi)) }
    $card.Child = $sp
    [void]$panel.Children.Add($card)

    $all = @(Get-FpsSessions)
    if (-not $all.Count) {
        $e = New-Text 'Aucune partie mesurée pour l''instant. Joue au moins 30 secondes : tes FPS moyens, tes chutes et la courbe de la partie apparaîtront ici.' 13 '#655E7E'
        [void]$panel.Children.Add($e)
        return
    }
    $recent = @($all | Sort-Object { [datetime]$_.Date } -Descending)
    $cmp = New-FpsCompare @($all | Where-Object { $_.Key -eq $recent[0].Key })
    if ($cmp) {
        [void]$panel.Children.Add((New-Text "$(Get-SessionName $recent[0]) : avant / après tes derniers réglages" 13 '#A6A1BC' -Semi))
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
    $h = New-Text 'Dernières parties (clique pour le détail)' 13 '#A6A1BC' -Semi
    $h.Margin = New-Thickness 0 0 0 6
    [void]$panel.Children.Add($h)
    foreach ($s in @($recent | Select-Object -First 12)) { Step-UI; [void]$panel.Children.Add((New-FpsRow $s)) }
}
