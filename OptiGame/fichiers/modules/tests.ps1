# OptiGame : onglet Tests (disques, processeur, mémoire, carte graphique, réseau, écrans).
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Onglet Tests
# ---------------------------------------------------------------------------
$script:TestButtons = New-Object System.Collections.ArrayList
$script:TestRunning = $false

# Travail lancé dans un fil séparé: seules les fonctions de [OGNative] y sont disponibles.
$DiskWork = {
    param($a)
    try { $r = [OGNative]::DiskTest($a.File, [long]$a.Size); if ($null -eq $r) { @{ Cancelled = $true } } else { @{ R = $r } } }
    catch { @{ Error = $_.Exception.GetBaseException().Message } }
}
$CpuWork = {
    param($a)
    try { $r = [OGNative]::CpuTest([double]$a.Single, [double]$a.Multi); if ($null -eq $r) { @{ Cancelled = $true } } else { @{ R = $r } } }
    catch { @{ Error = $_.Exception.GetBaseException().Message } }
}
$MemWork = {
    param($a)
    try { $r = [OGNative]::MemTest([long]$a.Bytes); if ($null -eq $r) { @{ Cancelled = $true } } else { @{ R = $r } } }
    catch { @{ Error = $_.Exception.GetBaseException().Message } }
}
$NetWork = {
    param($a)
    try {
        [OGNative]::Phase = 'ping'
        $ping = New-Object System.Net.NetworkInformation.Ping
        $times = @()
        for ($i = 0; $i -lt 10; $i++) {
            if ([OGNative]::Cancel) { return @{ Cancelled = $true } }
            try { $p = $ping.Send('1.1.1.1', 1000); if ($p.Status -eq 'Success') { $times += $p.RoundtripTime; [OGNative]::LiveValue = $p.RoundtripTime } } catch {}
            [OGNative]::Progress = $i + 1
            Start-Sleep -Milliseconds 100
        }
        [OGNative]::Phase = 'down'
        $servers = [string[]]@('https://speed.cloudflare.com/__down?bytes=25000000', 'https://proof.ovh.net/files/1Gb.dat', 'https://nbg1-speed.hetzner.com/1GB.bin', 'https://fsn1-speed.hetzner.com/1GB.bin')
        $down = [OGNative]::NetSpeed($servers, $false, 8, 4, 10, 55)
        if ([OGNative]::Cancel) { return @{ Cancelled = $true } }
        [OGNative]::Phase = 'up'
        $up = [OGNative]::NetSpeed([string[]]@('https://speed.cloudflare.com/__up'), $true, 8, 4, 55, 100)
        if ([OGNative]::Cancel) { return @{ Cancelled = $true } }
        $avg = if ($times.Count) { ($times | Measure-Object -Average).Average } else { -1 }
        @{ R = @($avg, $down, $up, (10 - $times.Count)) }
    } catch { @{ Error = $_.Exception.GetBaseException().Message } }
}
$RepairWork = {
    param($a)
    $out = @()
    [OGNative]::Phase = 'scan'
    $i = 0
    foreach ($l in $a.Letters) {
        try { $out += "$l|$(Repair-Volume -DriveLetter $l -Scan -ErrorAction Stop)" } catch { $out += "$l|ERR $($_.Exception.Message)" }
        $i++
        [OGNative]::Progress = 100 * $i / @($a.Letters).Count
    }
    @{ R = $out }
}

# ---------------------------------------------------------------------------
# Tuiles de l'onglet et panneau de test
# ---------------------------------------------------------------------------
function New-TestTile([string]$Tag, [string]$Title, [string]$Sub, [string]$Desc) {
    $card = New-Card
    $card.Padding = New-Thickness 18 16 18 16
    $card.Margin = New-Thickness 0 0 12 12
    $sp = New-Object System.Windows.Controls.StackPanel
    $head = New-Grid @('Auto', '*', 'Auto')
    $tagB = New-Object System.Windows.Controls.Border
    $tagB.Width = 44; $tagB.Height = 44
    $tagB.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $bg = Get-Brush $Colors.info; $bg.Opacity = 0.14
    $tagB.Background = $bg
    $tt = New-Text $Tag 12 $Colors.info -Bold
    $tt.TextWrapping = 'NoWrap'; $tt.HorizontalAlignment = 'Center'; $tt.VerticalAlignment = 'Center'
    $tagB.Child = $tt
    Add-ToGrid $head $tagB 0
    $ts = New-Object System.Windows.Controls.StackPanel
    $ts.Margin = New-Thickness 12 0 8 0
    $ts.VerticalAlignment = 'Center'
    $titleText = New-Text $Title 15 '#FFFFFF' -Semi
    $titleText.TextTrimming = 'CharacterEllipsis'; $titleText.TextWrapping = 'NoWrap'
    [void]$ts.Children.Add($titleText)
    if ($Sub) { [void]$ts.Children.Add((New-Text $Sub 12 '#9AA3B2')) }
    Add-ToGrid $head $ts 1
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 10; $dot.Height = 10; $dot.Fill = Get-Brush '#343C4C'
    $dot.VerticalAlignment = 'Top'; $dot.Margin = New-Thickness 0 6 0 0
    $dot.ToolTip = 'Pas encore testé'
    Add-ToGrid $head $dot 2
    [void]$sp.Children.Add($head)
    $d = New-Text $Desc 12.5 '#9AA3B2'
    $d.Margin = New-Thickness 0 10 0 0
    [void]$sp.Children.Add($d)
    $summary = New-Object System.Windows.Controls.WrapPanel
    $summary.Margin = New-Thickness 0 12 0 0
    $none = New-Text 'Pas encore testé' 12.5 '#5B6475'
    [void]$summary.Children.Add($none)
    [void]$sp.Children.Add($summary)
    $btns = New-Object System.Windows.Controls.WrapPanel
    $btns.Margin = New-Thickness 0 12 0 0
    [void]$sp.Children.Add($btns)
    $card.Child = $sp
    [void]$ui.TestsPanel.Children.Add($card)
    $tile = @{ Card = $card; Buttons = $btns; Summary = $summary; Dot = $dot; Tag = $Tag; Title = $Title; Sub = $Sub; Last = $null; View = $null }
    $view = New-Button 'Voir le résultat'
    $view.Margin = New-Thickness 0 0 8 6
    $view.Visibility = 'Collapsed'
    $view.Tag = $tile
    $view.Add_Click({ param($s, $e) Invoke-Safe { Show-LastResult $s.Tag } })
    $tile.View = $view
    $tile
}

function Add-TestButton($Tile, [string]$Text, [scriptblock]$OnClick, $Context, [switch]$Primary) {
    $b = New-Button $Text $(if ($Primary) { 'BtnPrimary' } else { 'BtnSecondary' })
    $b.Margin = New-Thickness 0 0 8 6
    $b.Tag = @{ T = $Tile; Ctx = $Context }
    $b.Add_Click($OnClick)
    [void]$Tile.Buttons.Children.Add($b)
    [void]$script:TestButtons.Add($b)
}

function Set-TileSummary($Tile, [array]$Chips, [string]$Status) {
    $Tile.Summary.Children.Clear()
    foreach ($c2 in $Chips) {
        $b = New-Object System.Windows.Controls.Border
        $b.Background = Get-Brush '#1A1F29'
        $b.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $b.Padding = New-Thickness 10 5 10 6
        $b.Margin = New-Thickness 0 0 6 6
        $sp = New-Object System.Windows.Controls.StackPanel
        [void]$sp.Children.Add((New-Text $c2[1] 14 '#FFFFFF' -Bold))
        [void]$sp.Children.Add((New-Text $c2[0] 11 '#9AA3B2'))
        $b.Child = $sp
        [void]$Tile.Summary.Children.Add($b)
    }
    $Tile.Dot.Fill = Get-Brush $Colors[$Status]
    $Tile.Dot.ToolTip = "Testé à $((Get-Date).ToString('HH:mm'))"
    if (-not $Tile.Buttons.Children.Contains($Tile.View)) { [void]$Tile.Buttons.Children.Add($Tile.View) }
    $Tile.View.Visibility = 'Visible'
}

function Set-TestState([string]$State, [string]$Text) {
    $map = @{ run = $Colors.info; live = $Colors.ok; ok = $Colors.ok; warn = $Colors.warn; bad = $Colors.bad; info = '#9AA3B2' }
    $ui.TestStateDot.Fill = Get-Brush $map[$State]
    $ui.TestStateText.Text = $Text
    $ui.TestStateText.Foreground = Get-Brush $map[$State]
    if ($State -in 'run', 'live') { Start-Pulse $ui.TestStateDot } else { Stop-Pulse $ui.TestStateDot }
}

function Show-TestPanel($Tile) {
    $ui.TestTag.Text = $Tile.Tag
    $ui.TestTitle.Text = $Tile.Title
    $ui.TestSub.Text = $Tile.Sub
    $ui.TestBody.Children.Clear()
    $ui.TestProgress.Value = 0
    $ui.TestPct.Text = ''
    $ui.TestOverlay.Visibility = 'Visible'
    $ui.TestCard.Opacity = 0
    Start-WpfAnim $ui.TestCard ([System.Windows.UIElement]::OpacityProperty) 1 250
}

function Hide-TestPanel {
    if ($script:TestRunning) { return }
    $script:DevPing = $null
    if ($script:MonitorTimer) { $script:MonitorTimer.Stop(); $script:MonitorTimer = $null }
    $Live.Fast = $false
    $ui.TestOverlay.Visibility = 'Collapsed'
}

function Set-TestButtons([string]$Mode) {
    $ui.BtnTestStop.Visibility = if ($Mode -eq 'run') { 'Visible' } else { 'Collapsed' }
    $ui.BtnTestAgain.Visibility = if ($Mode -eq 'done') { 'Visible' } else { 'Collapsed' }
    $ui.BtnTestClose.IsEnabled = $Mode -ne 'run'
    $ui.BtnTestX.IsEnabled = $Mode -ne 'run'
}

$script:TestTimer = New-Object System.Windows.Threading.DispatcherTimer
$script:TestTimer.Interval = [TimeSpan]::FromMilliseconds(100)
$script:TestTimer.Add_Tick({
    $cur = $script:CurTest
    if (-not $cur) { return }
    $prog = [math]::Min(100.0, [OGNative]::Progress)
    $ui.TestProgress.Value = $prog
    $ui.TestPct.Text = '{0:N0} %' -f $prog
    $ph = [OGNative]::Phase
    Update-Stepper $cur.Stepper $ph
    if ($ph -and $Live.CpuPerf) {
        if (-not $cur.Freq.ContainsKey($ph)) { $cur.Freq[$ph] = New-Object System.Collections.ArrayList }
        [void]$cur.Freq[$ph].Add([double]$Live.CpuPerf)
    }
    if ($cur.Chart -and $ph -and $cur.Def.Chart.Phases -contains $ph) {
        if ($cur.Def.Chart.Source -eq 'cpu') {
            if ($Live.Updated -eq $cur.LastSample) { return }
            $cur.LastSample = $Live.Updated
            $v = if ($Live.CpuPerf -and $Live.BaseMHz) { $Live.BaseMHz * $Live.CpuPerf / 100 / 1000 } else { 0 }
        } else { $v = [OGNative]::LiveValue }
        Add-ChartPoint $cur.Chart $v
        $cur.LiveVal.Text = $cur.Def.Chart.Fmt -f $v
        $cur.LiveLabel.Text = $cur.Def.Steps[$ph]
    }
})

# Lance un test: panneau animé pendant le test, puis résultat en jauges.
function Invoke-ComponentTest($Tile, $Def, $Ctx, [string]$Rerun) {
    if ($script:TestRunning) { Show-Message 'Un test est déjà en cours : attends qu''il se termine.'; return }
    $script:TestRunning = $true
    $script:LastRun = @{ Fn = $Rerun; Tile = $Tile; Ctx = $Ctx }
    foreach ($b in $script:TestButtons) { $b.IsEnabled = $false }
    Show-TestPanel $Tile
    Set-TestState 'run' 'Test en cours'
    Set-TestButtons 'run'
    $Live.Fast = $true
    $body = $ui.TestBody
    $stepper = New-Stepper $Def.Steps
    [void]$body.Children.Add($stepper.El)
    $cur = @{ Def = $Def; Stepper = $stepper; Chart = $null; LiveVal = $null; LiveLabel = $null; Freq = @{}; LastSample = $null }
    if ($Def.Chart) {
        $head = New-Object System.Windows.Controls.StackPanel
        $head.Orientation = 'Horizontal'
        $cur.LiveVal = New-Text '0' 44 '#FFFFFF' -Bold
        $cur.LiveVal.TextWrapping = 'NoWrap'
        $unit = New-Text $Def.Chart.Unit 16 '#9AA3B2' -Semi
        $unit.VerticalAlignment = 'Bottom'; $unit.Margin = New-Thickness 8 0 0 10
        [void]$head.Children.Add($cur.LiveVal)
        [void]$head.Children.Add($unit)
        $cur.LiveLabel = New-Text 'Préparation...' 13 '#9AA3B2'
        [void]$body.Children.Add($cur.LiveLabel)
        [void]$body.Children.Add($head)
        $cur.Head = $head
        $cur.Chart = New-LiveChart $Def.Chart.Color $Def.Chart.Unit $Def.Chart.Fmt
        if ($Def.Chart.Ref) { $cur.Chart.RefValue = $Def.Chart.Ref; $cur.Chart.RefText.Text = $Def.Chart.RefLabel }
        [void]$body.Children.Add($cur.Chart.El)
    }
    [OGNative]::Cancel = $false
    [OGNative]::Progress = 0
    [OGNative]::Phase = ''
    [OGNative]::LiveValue = 0
    $script:CurTest = $cur
    $script:TestTimer.Start()
    try {
        $r = Invoke-Async $Def.Work $Def.Arg | Select-Object -First 1
    } finally {
        $script:TestTimer.Stop()
        $script:CurTest = $null
        $script:TestRunning = $false
        $Live.Fast = $false
        foreach ($b in $script:TestButtons) { $b.IsEnabled = $true }
        Set-TestButtons 'done'
    }
    if (-not $r -or $r.Error) {
        if ($r.Error) { Write-Log "Test: $($r.Error)" }
        Set-TestState 'bad' 'Échec'
        [void]$body.Children.Add((New-Verdict 'bad' "Le test n'a pas pu aller au bout : $(if ($r.Error) { $r.Error } else { 'erreur inconnue' })"))
        return
    }
    if ($r.Cancelled) {
        Set-TestState 'info' 'Arrêté'
        [void]$body.Children.Add((New-Verdict 'info' 'Test arrêté.'))
        return
    }
    Update-Stepper $stepper '' -AllDone
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = '100 %'
    if ($cur.LiveLabel) { $cur.LiveLabel.Text = 'Courbe du test'; $cur.Head.Visibility = 'Collapsed' }
    $res = @{ R = @($r.R); Freq = $cur.Freq; ChartValues = $(if ($cur.Chart) { @($cur.Chart.Values) } else { @() }) }
    $Tile.Last = @{ Def = $Def; Res = $res; Ctx = $Ctx }
    $out = & $Def.Render $res $Ctx $body
    Set-TestState $out.Status $(switch ($out.Status) { 'ok' { 'Terminé' } 'warn' { 'Terminé : à surveiller' } 'bad' { 'Problème détecté' } default { 'Terminé' } })
    Show-ResultTop
    Set-TileSummary $Tile $out.Chips $out.Status
    Set-Status "$($Tile.Title) : test terminé."
}

# Réaffiche le dernier résultat (les jauges se réaniment).
function Show-LastResult($Tile) {
    $last = $Tile.Last
    if (-not $last) { return }
    if ($last.Live) { & $last.Live $Tile $last.Ctx; return }
    Show-TestPanel $Tile
    Set-TestButtons 'done'
    $script:LastRun = @{ Fn = $last.Fn; Tile = $Tile; Ctx = $last.Ctx }
    $body = $ui.TestBody
    $stepper = New-Stepper $last.Def.Steps
    [void]$body.Children.Add($stepper.El)
    Update-Stepper $stepper '' -AllDone
    if ($last.Def.Chart -and $last.Res.ChartValues.Count) {
        [void]$body.Children.Add((New-Text 'Courbe du test' 13 '#9AA3B2'))
        $ch = New-LiveChart $last.Def.Chart.Color $last.Def.Chart.Unit $last.Def.Chart.Fmt
        if ($last.Def.Chart.Ref) { $ch.RefValue = $last.Def.Chart.Ref; $ch.RefText.Text = $last.Def.Chart.RefLabel }
        foreach ($v in $last.Res.ChartValues) { [void]$ch.Values.Add([double]$v) }
        [void]$body.Children.Add($ch.El)
        Update-Chart $ch
    }
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    $out = & $last.Def.Render $last.Res $last.Ctx $body
    Set-TestState $out.Status $(if ($out.Status -eq 'ok') { 'Terminé' } elseif ($out.Status -eq 'bad') { 'Problème détecté' } else { 'Terminé : à surveiller' })
    Show-ResultTop
}

# Fait défiler le panneau jusqu'au résultat.
function Show-ResultTop {
    $ui.TestBody.UpdateLayout()
    $target = $ui.TestBody.Children | Where-Object { $_ -is [System.Windows.Controls.TextBlock] -and $_.Text -eq 'RÉSULTAT' } | Select-Object -First 1
    if ($target) {
        $y = $target.TranslatePoint([System.Windows.Point]::new(0, 0), $ui.TestBody).Y
        $ui.TestScroll.ScrollToVerticalOffset([math]::Max(0.0, $y - 8))
    }
}

function Get-GHz($Samples, [switch]$Max) {
    if (-not $Samples -or -not $Samples.Count -or -not $Live.BaseMHz) { return $null }
    $v = if ($Max) { ($Samples | Measure-Object -Maximum).Maximum } else { ($Samples | Measure-Object -Average).Average }
    $Live.BaseMHz * $v / 100 / 1000
}

# ---------------------------------------------------------------------------
# Disques
# ---------------------------------------------------------------------------
function Test-DiskSpeed($Tile, $Ctx) {
    $vol = Get-VolInfo $Ctx.Letter
    if (-not $vol -or $vol.SizeRemaining -lt 1GB) { Show-Message "Il faut au moins 1 Go de libre sur le lecteur $($Ctx.Letter): pour faire ce test."; return }
    $size = if ($vol.SizeRemaining -gt 40GB) { 8GB } elseif ($vol.SizeRemaining -gt 10GB) { 2GB } else { 256MB }
    $file = if ("$($Ctx.Letter):" -eq $env:SystemDrive) { Join-Path $env:TEMP 'OptiGame-test-disque.tmp' } else { "$($Ctx.Letter):\OptiGame-test-disque.tmp" }
    $steps = [ordered]@{ write = 'Écriture'; read = 'Lecture'; random = 'Petits fichiers' }
    $def = @{
        Steps = $steps; Work = $DiskWork; Arg = @{ File = $file; Size = [long]$size }
        Chart = @{ Unit = 'Mo/s'; Fmt = '{0:N0}'; Color = $Colors.ok; Source = 'engine'; Phases = @('write', 'read') }
        Render = {
            param($res, $ctx, $body)
            $r = $res.R
            $scale = switch ($ctx.Kind) { 'NVMe' { 7000 } 'SSD' { 600 } 'HDD' { 250 } default { 1000 } }
            [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
            [void]$body.Children.Add((New-GaugeRow @(
                (New-Gauge 'Lecture' $r[1] $scale '{0:N0}' 'Mo/s' $Colors.ok 0),
                (New-Gauge 'Écriture' $r[0] $scale '{0:N0}' 'Mo/s' $Colors.info 150),
                (New-Gauge 'Petits fichiers' $r[3] 25000 '{0:N0}' 'par seconde' '#B18CFF' 300)
            )))
            [void]$body.Children.Add((New-SectionTitle 'COMPARAISON (LECTURE)'))
            [void]$body.Children.Add((New-CompareBars @(
                @{ Label = 'Ton disque'; Value = $r[1]; Mine = $true; Color = $Colors.ok },
                @{ Label = 'Disque dur'; Value = 150 },
                @{ Label = 'SSD classique'; Value = 550 },
                @{ Label = 'SSD NVMe récent'; Value = 5000 }
            ) 'Mo/s'))
            $min = switch ($ctx.Kind) { 'NVMe' { 1200 } 'SSD' { 350 } 'HDD' { 80 } default { 0 } }
            if ($min -and $r[1] -lt $min) {
                $status = 'warn'
                $txt = 'Plus lent que la normale pour ce type de disque : disque presque plein, qui chauffe, ou SSD branché sur un port lent.'
            } else {
                $status = 'ok'
                $txt = 'Vitesse normale pour ce type de disque.'
            }
            [void]$body.Children.Add((New-Verdict $status $txt))
            @{ Status = $status; Chips = @(@('Lecture', ('{0:N0} Mo/s' -f $r[1])), @('Écriture', ('{0:N0} Mo/s' -f $r[0]))) }
        }
    }
    $def.Chart.Phases = @('write', 'read')
    Set-Status "Test de vitesse : $($Ctx.Name)..."
    Invoke-ComponentTest $Tile $def $Ctx 'Test-DiskSpeed'
}

function Show-DiskHealth($Tile, $Ctx) {
    $pd = Invoke-Async $DiskInfoWork $Ctx.Id | Select-Object -First 1
    $d = $pd.D
    if (-not $d) { Show-Message 'Disque introuvable.'; return }
    $script:LastRun = @{ Fn = 'Show-DiskHealth'; Tile = $Tile; Ctx = $Ctx }
    Show-TestPanel $Tile
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    $body = $ui.TestBody
    $rel = $null
    $rel = $pd.Rel
    $status = 'ok'; $notes = @()
    switch ([string]$d.HealthStatus) {
        'Warning'   { $status = 'warn'; $notes += 'Le disque signale lui même un problème.' }
        'Unhealthy' { $status = 'bad'; $notes += 'Le disque annonce une panne proche.' }
    }
    $gauges = @()
    $i = 0
    if ($rel -and $null -ne $rel.Wear -and $Ctx.Kind -ne 'HDD') {
        $life = 100 - [int]$rel.Wear
        $gauges += New-Gauge 'Durée de vie restante' $life 100 '{0:N0}' '%' $(if ($life -le 10) { $Colors.bad } elseif ($life -le 30) { $Colors.warn } else { $Colors.ok }) 0
        if ($life -le 10) { $status = 'bad'; $notes += "Il reste environ $life % de durée de vie." } elseif ($life -le 30) { if ($status -eq 'ok') { $status = 'warn' }; $notes += "Il reste environ $life % de durée de vie." }
    } else {
        $sh = switch ([string]$d.HealthStatus) { 'Healthy' { 100 } 'Warning' { 50 } 'Unhealthy' { 10 } default { 0 } }
        $gauges += New-Gauge 'État SMART' $sh 100 '{0:N0}' '%' $(if ($sh -ge 100) { $Colors.ok } elseif ($sh -ge 50) { $Colors.warn } else { $Colors.bad }) 0
    }
    if ($rel -and $rel.Temperature -gt 0) {
        $warnT = if ($Ctx.Kind -eq 'HDD') { 50 } else { 70 }
        $gauges += New-Gauge 'Température' $rel.Temperature 90 '{0:N0}' '°C' (Get-LoadColor $rel.Temperature $warnT ($warnT + 10)) 150
        if ($rel.Temperature -ge $warnT) { if ($status -eq 'ok') { $status = 'warn' }; $notes += 'Le disque chauffe : vérifie la ventilation.' }
    }
    $fillPct = $null
    foreach ($l in $Ctx.Letters) {
        $v = Get-VolInfo $l
        if ($v -and $v.Size) { $fillPct = 100 * ($v.Size - $v.SizeRemaining) / $v.Size; break }
    }
    if ($null -ne $fillPct) {
        $gauges += New-Gauge 'Rempli' $fillPct 100 '{0:N0}' '%' (Get-LoadColor $fillPct 80 90) 300
        if ($fillPct -ge 90) { if ($status -eq 'ok') { $status = 'warn' }; $notes += 'Le disque est presque plein.' }
    }
    [void]$body.Children.Add((New-SectionTitle 'SANTÉ DU DISQUE'))
    [void]$body.Children.Add((New-GaugeRow $gauges))
    $tiles = @()
    if ($rel -and $rel.PowerOnHours -gt 0) { $tiles += New-StatTile 'heures allumé au total' ([double]$rel.PowerOnHours) '{0:N0}' '#FFFFFF' 200 }
    if ($rel -and $null -ne $rel.ReadErrorsUncorrected) {
        $errs = [double]$rel.ReadErrorsUncorrected + $(if ($null -ne $rel.WriteErrorsUncorrected) { [double]$rel.WriteErrorsUncorrected } else { 0 })
        $tiles += New-StatTile 'erreurs non réparées' $errs '{0:N0}' $(if ($errs) { $Colors.warn } else { $Colors.ok }) 350
        if ($errs) { if ($status -eq 'ok') { $status = 'warn' }; $notes += "$errs erreur(s) de lecture ou d'écriture." }
    }
    if ($rel -and $rel.StartStopCycleCount -gt 0) { $tiles += New-StatTile 'démarrages' ([double]$rel.StartStopCycleCount) '{0:N0}' '#FFFFFF' 500 }
    if ($tiles) { [void]$body.Children.Add((New-StatRow $tiles)) }
    $rows = @(
        @('Modèle', ([string]$d.FriendlyName).Trim()),
        @('Numéro de série', ([string]$d.SerialNumber).Trim().TrimEnd('.')),
        @('Version du micrologiciel', [string]$d.FirmwareVersion),
        @('Type', "$($Ctx.KindLabel), branché en $([string]$d.BusType)"),
        @('Capacité', (Format-Size $d.Size)),
        @('État SMART', $(switch ([string]$d.HealthStatus) { 'Healthy' { 'Bon' } 'Warning' { 'Avertissement' } 'Unhealthy' { 'Défaillant' } default { 'Inconnu' } }))
    )
    if ($rel) {
        if ($null -ne $rel.Wear) { $rows += , @('Usure', "$([int]$rel.Wear) %") }
        if ($rel.TemperatureMax -gt 0) { $rows += , @('Température limite du fabricant', "$([int]$rel.TemperatureMax) °C") }
        if ($null -ne $rel.ReadErrorsCorrected) { $rows += , @('Erreurs de lecture réparées', ('{0:N0}' -f $rel.ReadErrorsCorrected)) }
        if ($null -ne $rel.ReadErrorsUncorrected) { $rows += , @('Erreurs de lecture non réparées', ('{0:N0}' -f $rel.ReadErrorsUncorrected)) }
        if ($null -ne $rel.WriteErrorsUncorrected) { $rows += , @('Erreurs d''écriture non réparées', ('{0:N0}' -f $rel.WriteErrorsUncorrected)) }
        if ($rel.ReadLatencyMax -gt 0) { $rows += , @('Temps de réponse max en lecture', "$($rel.ReadLatencyMax) ms") }
        if ($rel.WriteLatencyMax -gt 0) { $rows += , @('Temps de réponse max en écriture', "$($rel.WriteLatencyMax) ms") }
    } else {
        $rows += , @('Détails SMART', 'Non fournis par ce disque', '#9AA3B2')
    }
    foreach ($l in $Ctx.Letters) {
        $v = Get-VolInfo $l
        if ($v -and $v.Size) { $rows += , @("Lecteur $($l):", "$(Format-Size $v.SizeRemaining) libres sur $(Format-Size $v.Size)") }
    }
    [void]$body.Children.Add((New-Details $rows))
    $txt = switch ($status) {
        'ok'   { 'Ce disque est en bonne santé.' }
        'warn' { 'À surveiller : ' + ($notes -join ' ') + ' Pense à sauvegarder tes fichiers importants.' }
        'bad'  { 'Attention : ' + ($notes -join ' ') + ' Sauvegarde tes fichiers maintenant et prévois de remplacer ce disque.' }
    }
    [void]$body.Children.Add((New-Verdict $status $txt))
    Set-TestState $status $(switch ($status) { 'ok' { 'Bonne santé' } 'warn' { 'À surveiller' } default { 'Problème détecté' } })
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    $chips = @(, @('Santé', $(switch ($status) { 'ok' { 'Bonne' } 'warn' { 'À surveiller' } default { 'Problème' } })))
    if ($rel -and $rel.Temperature -gt 0) { $chips += , @('Température', "$([int]$rel.Temperature) °C") }
    if (-not $Tile.Last) { $Tile.Last = @{ Live = { param($t2, $c2) Show-DiskHealth $t2 $c2 }; Ctx = $Ctx } }
    Set-TileSummary $Tile $chips $status
}

function Test-DiskErrors($Tile, $Ctx) {
    $def = @{
        Steps = [ordered]@{ scan = 'Recherche d''erreurs (quelques minutes)' }
        Work = $RepairWork; Arg = @{ Letters = @($Ctx.Letters) }
        Chart = $null
        Render = {
            param($res, $ctx, $body)
            [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
            $bad = $false
            $tiles = @()
            $i = 0
            foreach ($line in $res.R) {
                $l, $v = ([string]$line) -split '\|', 2
                $ok = $v -eq 'NoErrorsFound'
                if (-not $ok -and $v -notlike 'ERR*') { $bad = $true }
                $label = if ($ok) { 'aucune erreur' } elseif ($v -like 'ERR*') { 'vérification impossible' } else { 'erreurs trouvées' }
                $tiles += New-StatTile "Lecteur $($l): $label" $(if ($ok) { 0 } else { 1 }) $(if ($ok) { '✓' } else { '!' }) $(if ($ok) { $Colors.ok } else { $Colors.warn }) (150 * $i)
                $i++
            }
            [void]$body.Children.Add((New-StatRow $tiles))
            if ($bad) {
                [void]$body.Children.Add((New-Verdict 'warn' 'Windows a trouvé des erreurs dans le système de fichiers. Redémarre le PC : Windows les répare souvent tout seul.'))
                @{ Status = 'warn'; Chips = @(, @('Erreurs', 'Trouvées')) }
            } else {
                [void]$body.Children.Add((New-Verdict 'ok' 'Aucune erreur trouvée sur ce disque.'))
                @{ Status = 'ok'; Chips = @(, @('Erreurs', 'Aucune')) }
            }
        }
    }
    Set-Status "Recherche d'erreurs : $($Ctx.Name)..."
    Invoke-ComponentTest $Tile $def $Ctx 'Test-DiskErrors'
}

# ---------------------------------------------------------------------------
# Processeur
# ---------------------------------------------------------------------------
function Test-Cpu($Tile, $Ctx) {
    $base = $Live.BaseMHz / 1000
    $def = @{
        Steps = [ordered]@{ single = 'Un seul cœur'; multi = 'Tous les cœurs' }
        Work = $CpuWork; Arg = @{ Single = $Ctx.Single; Multi = $Ctx.Multi }
        Chart = @{ Unit = 'GHz'; Fmt = '{0:N1}'; Color = $Colors.info; Source = 'cpu'; Phases = @('single', 'multi'); Ref = $base; RefLabel = ('fréquence de base {0:N1} GHz' -f $base) }
        Render = {
            param($res, $ctx, $body)
            $r = $res.R
            $maxSingle = Get-GHz $res.Freq['single'] -Max
            $avgMulti = Get-GHz $res.Freq['multi']
            $base2 = $Live.BaseMHz / 1000
            $top = [math]::Max(6.0, [math]::Ceiling(([double]$maxSingle) + 0.5))
            [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
            [void]$body.Children.Add((New-StatRow @(
                (New-StatTile 'points sur un cœur' $r[0] '{0:N0}' '#FFFFFF' 0),
                (New-StatTile "points sur les $([int]$r[3]) cœurs" $r[1] '{0:N0}' '#FFFFFF' 150),
                (New-StatTile 'erreurs de calcul' $r[2] '{0:N0}' $(if ($r[2]) { $Colors.bad } else { $Colors.ok }) 300)
            )))
            $g = @()
            if ($maxSingle) { $g += New-Gauge 'Fréquence max' $maxSingle $top '{0:N1}' 'GHz' $Colors.info 200 }
            if ($avgMulti) { $g += New-Gauge 'En pleine charge' $avgMulti $top '{0:N1}' 'GHz' $(if ($avgMulti -lt $base2 * 0.95) { $Colors.warn } else { $Colors.ok }) 350 }
            $g += New-Gauge 'Tous les cœurs vs un seul' ($r[1] / [math]::Max(1.0, $r[0])) ([math]::Max(1.0, $r[3])) '{0:N1}' 'fois plus' '#B18CFF' 500
            [void]$body.Children.Add((New-GaugeRow $g))
            if ($r[2] -gt 0) {
                $status = 'bad'; $txt = 'Le processeur a fait des erreurs de calcul : il est instable (overclock ou undervolt trop poussé, XMP instable, surchauffe). Remets les réglages du BIOS par défaut et refais le test.'
            } elseif ($avgMulti -and $avgMulti -lt $base2 * 0.95) {
                $status = 'warn'; $txt = 'Sous forte charge, le processeur passe sous sa fréquence de base : il chauffe trop ou manque d''alimentation. Vérifie le ventirad et la pâte thermique.'
            } else {
                $status = 'ok'; $txt = $(if ($ctx.Multi -ge 120) { 'Aucune erreur pendant 5 minutes à pleine charge : ton processeur est stable.' } else { 'Tout est normal : aucune erreur et le processeur garde bien sa vitesse.' })
            }
            [void]$body.Children.Add((New-Verdict $status $txt))
            @{ Status = $status; Chips = @(@('1 cœur', ('{0:N0} pts' -f $r[0])), @('Tous les cœurs', ('{0:N0} pts' -f $r[1])), @('En charge', $(if ($avgMulti) { '{0:N1} GHz' -f $avgMulti } else { '?' }))) }
        }
    }
    Set-Status 'Test du processeur...'
    Invoke-ComponentTest $Tile $def $Ctx 'Test-Cpu'
}

# ---------------------------------------------------------------------------
# Mémoire vive
# ---------------------------------------------------------------------------
function Test-Memory($Tile, $Ctx) {
    $free = if ($Live.RamTotal) { [double]$Live.RamTotal - [double]$Live.RamUsed } else { [double]2GB }
    $bytes = [long][math]::Min([double]2GB, [math]::Max([double]256MB, $free * 0.5))
    $def = @{
        Steps = [ordered]@{ alloc = 'Réservation'; write = 'Écriture'; read = 'Vérification'; pattern = '2e passage'; copy = 'Copie' }
        Work = $MemWork; Arg = @{ Bytes = $bytes }
        Chart = @{ Unit = 'Go/s'; Fmt = '{0:N1}'; Color = '#B18CFF'; Source = 'engine'; Phases = @('write', 'read', 'pattern') }
        Render = {
            param($res, $ctx, $body)
            $r = $res.R
            [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
            [void]$body.Children.Add((New-GaugeRow @(
                (New-Gauge 'Lecture' $r[1] 100 '{0:N1}' 'Go/s' $Colors.ok 0),
                (New-Gauge 'Écriture' $r[0] 100 '{0:N1}' 'Go/s' $Colors.info 150),
                (New-Gauge 'Copie' $r[2] 100 '{0:N1}' 'Go/s' '#B18CFF' 300)
            )))
            [void]$body.Children.Add((New-StatRow @(
                (New-StatTile 'Go vérifiés' $r[4] '{0:N1}' '#FFFFFF' 300),
                (New-StatTile 'erreurs trouvées' $r[3] '{0:N0}' $(if ($r[3]) { $Colors.bad } else { $Colors.ok }) 450)
            )))
            if ($r[3] -gt 0) {
                $status = 'bad'; $txt = 'Des erreurs ont été trouvées. Désactive le profil XMP / EXPO dans le BIOS et refais le test. Si ça continue, une barrette est défectueuse : lance le test complet de Windows.'
            } else {
                $status = 'ok'; $txt = 'Aucune erreur. Ce test rapide ne vérifie qu''une partie de la mémoire : en cas de plantages, lance le test complet de Windows.'
            }
            [void]$body.Children.Add((New-Verdict $status $txt))
            @{ Status = $status; Chips = @(@('Lecture', ('{0:N1} Go/s' -f $r[1])), @('Erreurs', ('{0:N0}' -f $r[3]))) }
        }
    }
    Set-Status 'Test de la mémoire...'
    Invoke-ComponentTest $Tile $def $Ctx 'Test-Memory'
}

# ---------------------------------------------------------------------------
# Carte graphique: surveillance en direct
# ---------------------------------------------------------------------------
function Show-GpuMonitor($Tile, $Ctx) {
    $g = $Ctx.Gpu
    $script:LastRun = @{ Fn = 'Show-GpuMonitor'; Tile = $Tile; Ctx = $Ctx }
    Show-TestPanel $Tile
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    Set-TestState 'live' 'En direct'
    $Live.Fast = $true
    $body = $ui.TestBody
    $nvidia = $g.Name -match 'NVIDIA|GeForce' -and $SmiPath
    $info = $null
    if ($nvidia) {
        $fields = 'driver_version,vbios_version,pstate,clocks.gr,clocks.max.gr,clocks.mem,clocks.max.mem,temperature.gpu,fan.speed,power.draw,power.limit,pcie.link.gen.current,pcie.link.gen.max,pcie.link.width.current,pcie.link.width.max,utilization.gpu,memory.used,memory.total,clocks_throttle_reasons.active'
        $o = & $SmiPath "--query-gpu=$fields" '--format=csv,noheader,nounits' 2>$null | Select-Object -First 1
        if ($o) { $info = @($o -split ',' | ForEach-Object { $_.Trim() }) }
    }
    $num = { param($s) if ($s -match '^[\d\.]+$') { [double]::Parse($s, [Globalization.CultureInfo]::InvariantCulture) } else { 0 } }
    $plimit = if ($info) { [math]::Max(50.0, (& $num $info[10])) } else { 300 }
    [void]$body.Children.Add((New-Text 'Lance un jeu, puis reviens ici avec Alt + Tab : tout se met à jour en direct.' 13 '#9AA3B2'))
    $gUse = New-Gauge 'Utilisation' ([double]$Live.Gpu) 100 '{0:N0}' '%' $Colors.info 0
    $gauges = @($gUse)
    $gTemp = $null; $gPow = $null
    if ($nvidia) {
        $gTemp = New-Gauge 'Température' ([double]$Live.GpuTemp) 100 '{0:N0}' '°C' $Colors.ok 150
        $gPow = New-Gauge 'Consommation' ([double]$Live.GpuPower) $plimit '{0:N0}' 'W' $Colors.warn 300
        $gauges += $gTemp; $gauges += $gPow
    }
    [void]$body.Children.Add((New-GaugeRow $gauges))
    [void]$body.Children.Add((New-Text 'Utilisation de la carte graphique' 13 '#9AA3B2'))
    $chart = New-LiveChart $Colors.info '%' '{0:N0}'
    [void]$body.Children.Add($chart.El)
    $status = 'ok'
    if ($info) {
        $bits = 0
        try { $bits = [Convert]::ToUInt64(($info[18] -replace '^0x', ''), 16) } catch {}
        $why = @()
        if ($bits -band 0x4)  { $why += 'limite de consommation (normal en pleine charge)' }
        if ($bits -band 0x20) { $why += 'chauffe' }
        if ($bits -band 0x40) { $why += 'surchauffe' }
        if ($bits -band 0x8)  { $why += 'ralentissement matériel' }
        if ($bits -band 0x80) { $why += 'alimentation insuffisante' }
        $rows = @(
            @('Fréquence du GPU', "$($info[3]) MHz (max $($info[4]) MHz)"),
            @('Fréquence de la mémoire', "$($info[5]) MHz (max $($info[6]) MHz)"),
            @('Mémoire vidéo utilisée', ('{0:N1} Go sur {1:N0} Go' -f ((& $num $info[16]) / 1024), ((& $num $info[17]) / 1024))),
            @('Ventilateurs', "$($info[8]) %"),
            @('Liaison PCIe', "Gen $($info[11]) x$($info[13])  (max Gen $($info[12]) x$($info[14]))"),
            @('Ralentissements', $(if ($why) { $why -join ', ' } else { 'Aucun' })),
            @('Pilote', $info[0]),
            @('BIOS de la carte', $info[1])
        )
        [void]$body.Children.Add((New-Details $rows))
        if ($bits -band 0xE8) { $status = 'warn'; [void]$body.Children.Add((New-Verdict 'warn' 'La carte ralentit à cause de la chaleur ou de l''alimentation : dépoussière le PC et vérifie la ventilation du boîtier.')) }
        else { [void]$body.Children.Add((New-Verdict 'ok' 'Aucun ralentissement. À vide, la carte baisse sa vitesse pour économiser l''énergie : c''est normal.')) }
    } else {
        $rows = @(@('Modèle', $g.Name), @('Pilote', [string]$g.DriverVersion), @('Résolution', "$($g.CurrentHorizontalResolution) x $($g.CurrentVerticalResolution)"))
        [void]$body.Children.Add((New-Details $rows))
        [void]$body.Children.Add((New-Verdict 'info' 'Température et consommation ne sont lisibles que sur les cartes NVIDIA. Pour une carte AMD : AMD Software > Performances.'))
    }
    $script:GpuLive = @{ Use = $gUse; Temp = $gTemp; Pow = $gPow; Chart = $chart; Last = $null }
    $script:MonitorTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:MonitorTimer.Interval = [TimeSpan]::FromMilliseconds(500)
    $script:MonitorTimer.Add_Tick({
        $m = $script:GpuLive
        if (-not $m -or $Live.Updated -eq $m.Last) { return }
        $m.Last = $Live.Updated
        Set-GaugeLive $m.Use ([double]$Live.Gpu)
        if ($m.Temp) { Set-GaugeLive $m.Temp ([double]$Live.GpuTemp) }
        if ($m.Pow) { Set-GaugeLive $m.Pow ([double]$Live.GpuPower) }
        Add-ChartPoint $m.Chart ([double]$Live.Gpu)
    })
    $script:MonitorTimer.Start()
    $ui.TestProgress.Value = 0; $ui.TestPct.Text = ''
    $chips = @(, @('Température', $(if ($Live.GpuTemp) { "$([int]$Live.GpuTemp) °C" } else { '?' })))
    if ($info) { $chips += , @('Consommation', "$([int](& $num $info[9])) W") }
    $Tile.Last = @{ Live = { param($t2, $c2) Show-GpuMonitor $t2 $c2 }; Ctx = $Ctx }
    Set-TileSummary $Tile $chips $status
}

# ---------------------------------------------------------------------------
# Réseau
# ---------------------------------------------------------------------------
function Test-NetSpeed($Tile, $Ctx) {
    $def = @{
        Steps = [ordered]@{ ping = 'Ping'; down = 'Téléchargement'; up = 'Envoi' }
        Work = $NetWork; Arg = @{}
        Chart = @{ Unit = 'Mb/s'; Fmt = '{0:N0}'; Color = $Colors.ok; Source = 'engine'; Phases = @('down', 'up') }
        Render = {
            param($res, $ctx, $body)
            $r = $res.R
            $scale = if ([math]::Max($r[1], $r[2]) -gt 1000) { 2500 } else { 1000 }
            [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
            $g = @()
            if ($r[1] -ge 0) { $g += New-Gauge 'Téléchargement' $r[1] $scale '{0:N0}' 'Mb/s' $Colors.ok 0 }
            if ($r[2] -ge 0) { $g += New-Gauge 'Envoi' $r[2] $scale '{0:N0}' 'Mb/s' $Colors.info 150 }
            if ($r[0] -ge 0) { $g += New-Gauge 'Ping' $r[0] 100 '{0:N0}' 'ms' $(if ($r[0] -gt 60) { $Colors.warn } else { $Colors.ok }) 300 }
            [void]$body.Children.Add((New-GaugeRow $g))
            if ($r[1] -gt 0) {
                $min = 50 * 8000 / $r[1] / 60
                [void]$body.Children.Add((New-StatRow @((New-StatTile 'minutes pour télécharger un jeu de 50 Go' $min '{0:N0}' '#FFFFFF' 400))))
            }
            if ($r[1] -lt 0 -or $r[2] -lt 0) {
                $status = 'info'; $txt = 'Les serveurs de test n''ont pas répondu (trop de tests d''affilée ou pas de connexion). Réessaie dans quelques minutes.'
            } elseif ($r[1] -lt 10) {
                $status = 'warn'; $txt = 'Connexion lente : les téléchargements seront longs. Pour jouer, c''est surtout le ping qui compte.'
            } elseif ($r[0] -gt 60) {
                $status = 'warn'; $txt = 'Le débit est correct mais le ping est élevé : en Wi-Fi, rapproche toi de la box ou branche un câble.'
            } else {
                $status = 'ok'; $txt = 'Bonne connexion pour jouer et télécharger.'
            }
            [void]$body.Children.Add((New-Verdict $status $txt))
            @{ Status = $status; Chips = @(@('Téléchargement', $(if ($r[1] -ge 0) { '{0:N0} Mb/s' -f $r[1] } else { '?' })), @('Ping', $(if ($r[0] -ge 0) { '{0:N0} ms' -f $r[0] } else { '?' }))) }
        }
    }
    Set-Status 'Test de la connexion...'
    Invoke-ComponentTest $Tile $def $Ctx 'Test-NetSpeed'
}

# ---------------------------------------------------------------------------
# Écrans: pixels morts
# ---------------------------------------------------------------------------
function Start-PixelTest([int]$Index) {
    $screens = [System.Windows.Forms.Screen]::AllScreens
    if ($Index -ge $screens.Count) { return }
    $b = $screens[$Index].Bounds
    $src = [System.Windows.PresentationSource]::FromVisual($Window)
    $scale = if ($src) { $src.CompositionTarget.TransformToDevice.M11 } else { 1 }
    $w = New-Object System.Windows.Window
    $w.WindowStyle = 'None'; $w.ResizeMode = 'NoResize'; $w.Topmost = $true; $w.ShowInTaskbar = $false
    $w.WindowStartupLocation = 'Manual'
    $w.Width = 100; $w.Height = 100
    $w.Left = ($b.X + $b.Width / 2) / $scale - 50
    $w.Top = ($b.Y + $b.Height / 2) / $scale - 50
    $w.Cursor = [System.Windows.Input.Cursors]::None
    $script:PixColors = @('#000000', '#FFFFFF', '#FF0000', '#00FF00', '#0000FF', '#808080')
    $script:PixIndex = 0
    $w.Background = Get-Brush $script:PixColors[0]
    $hint = New-Text "Cherche les points qui ne sont pas de la bonne couleur.`n`nClic ou Espace : couleur suivante        Échap : quitter" 20 '#9AA3B2'
    $hint.HorizontalAlignment = 'Center'; $hint.VerticalAlignment = 'Center'; $hint.TextAlignment = 'Center'
    $w.Content = $hint
    $w.Add_SourceInitialized({ param($s, $e) $s.WindowState = 'Maximized' })
    $w.Add_MouseDown({
        param($s, $e)
        $script:PixIndex++
        if ($script:PixIndex -ge $script:PixColors.Count) { $s.Close(); return }
        $s.Content = $null
        $s.Background = Get-Brush $script:PixColors[$script:PixIndex]
    })
    $w.Add_KeyDown({
        param($s, $e)
        if ($e.Key -eq 'Escape') { $s.Close(); return }
        if ($e.Key -in 'Space', 'Enter', 'Right') {
            $script:PixIndex++
            if ($script:PixIndex -ge $script:PixColors.Count) { $s.Close(); return }
            $s.Content = $null
            $s.Background = Get-Brush $script:PixColors[$script:PixIndex]
        }
    })
    [void]$w.ShowDialog()
}

# ---------------------------------------------------------------------------
# Construction de l'onglet
# ---------------------------------------------------------------------------
function Test-3DMark {
    $steam = [string](Get-ItemProperty 'HKCU:\Software\Valve\Steam' -ErrorAction SilentlyContinue).SteamPath
    if (-not $steam) { return $false }
    $libs = @($steam -replace '/', '\')
    $vdf = Join-Path $libs[0] 'steamapps\libraryfolders.vdf'
    if (Test-Path -LiteralPath $vdf) { foreach ($m in [regex]::Matches((Get-Content -LiteralPath $vdf -Raw), '"path"\s+"([^"]+)"')) { $libs += ($m.Groups[1].Value -replace '\\\\', '\') } }
    [bool]($libs | Where-Object { Test-Path -LiteralPath (Join-Path $_ 'steamapps\common\3DMark') })
}

function Build-TestsTab {
    $ui.TestsPanel.Children.Clear()
    $script:TestButtons.Clear()
    if (-not $script:AnalysisData) { $script:AnalysisData = Invoke-Async $AnalysisDataWork $env:SystemDrive | Select-Object -First 1 }

    foreach ($dd in @($script:AnalysisData.Disks)) {
        $d = $dd.Disk
        $media = [string]$d.MediaType; $bus = [string]$d.BusType
        $kind = if ($bus -eq 'NVMe') { 'NVMe' } elseif ($media -eq 'SSD') { 'SSD' } elseif ($media -eq 'HDD') { 'HDD' } elseif ($bus -eq 'USB') { 'USB' } else { 'SSD' }
        $kindLabel = switch ($kind) { 'NVMe' { 'SSD NVMe' } 'SSD' { 'SSD' } 'HDD' { 'Disque dur' } default { 'Disque externe' } }
        $letters = @()
        $letters = @($dd.Letters)
        $sub = "$kindLabel, $(Format-Size $d.Size)" + $(if ($letters) { "  /  $(($letters | ForEach-Object { "$($_):" }) -join ' ')" } else { '' })
        $tag = switch ($kind) { 'HDD' { 'HDD' } 'USB' { 'USB' } default { 'SSD' } }
        $tile = New-TestTile $tag (([string]$d.FriendlyName).Trim()) $sub 'Vitesse réelle et santé du disque (usure, température, erreurs).'
        $ctx = @{ Id = [string]$d.DeviceId; Name = ([string]$d.FriendlyName).Trim(); Kind = $kind; KindLabel = $kindLabel; Letters = $letters; Letter = $(if ($letters) { $letters[0] } else { $null }) }
        if ($ctx.Letter) { Add-TestButton $tile 'Tester la vitesse' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-DiskSpeed $x.T $x.Ctx } } $ctx -Primary }
        Add-TestButton $tile 'Santé' { param($s, $e) $x = $s.Tag; Invoke-Safe { Show-DiskHealth $x.T $x.Ctx } } $ctx
        if ($ctx.Letter) { Add-TestButton $tile 'Erreurs' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-DiskErrors $x.T $x.Ctx } } $ctx }
    }

    $cpu = $script:AnalysisData.CPU
    $tile = New-TestTile 'CPU' (($cpu.Name -replace '\s+', ' ').Trim()) "$($cpu.NumberOfCores) cœurs, $($cpu.NumberOfLogicalProcessors) threads" 'Puissance, vitesse tenue quand il chauffe et stabilité. Ferme tes jeux avant.'
    Add-TestButton $tile 'Test rapide (30 s)' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-Cpu $x.T $x.Ctx } } @{ Single = 8; Multi = 22 } -Primary
    Add-TestButton $tile 'Stabilité (5 min)' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-Cpu $x.T $x.Ctx } } @{ Single = 5; Multi = 295 }

    $mem = @($script:AnalysisData.Mem)
    $totalGB = [math]::Round((($mem | Measure-Object Capacity -Sum).Sum) / 1GB)
    $tile = New-TestTile 'RAM' 'Mémoire vive' "$totalGB Go, $($mem.Count) barrette$(if ($mem.Count -gt 1) {'s'})" 'Vitesse et recherche d''erreurs en quelques secondes.'
    Add-TestButton $tile 'Tester la mémoire' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-Memory $x.T $x.Ctx } } @{} -Primary
    Add-TestButton $tile 'Test complet Windows' { param($s, $e) Start-Process 'mdsched.exe' } @{}

    foreach ($g in @($script:AnalysisData.GPUs | Where-Object { $_.Name -notmatch 'Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta|Microsoft Basic' })) {
        $tile = New-TestTile 'GPU' $g.Name 'Carte graphique' 'Température, utilisation et consommation en direct, et ralentissements éventuels.'
        Add-TestButton $tile 'Surveiller en direct' { param($s, $e) $x = $s.Tag; Invoke-Safe { Show-GpuMonitor $x.T $x.Ctx } } @{ Gpu = $g } -Primary
        if (Test-3DMark) {
            Add-TestButton $tile '3DMark' { param($s, $e) Start-Process 'steam://rungameid/223850' } @{}
        }
    }

    $tile = New-TestTile 'NET' 'Connexion Internet' 'Ping, téléchargement, envoi' 'Réactivité et vitesse de ta connexion, en 20 secondes.'
    Add-TestButton $tile 'Tester ma connexion' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-NetSpeed $x.T $x.Ctx } } @{} -Primary

    $screens = [System.Windows.Forms.Screen]::AllScreens
    $tile = New-TestTile 'HZ' 'Écrans' "$($screens.Count) écran$(if ($screens.Count -gt 1) {'s'})" 'Couleurs unies en plein écran pour repérer les pixels morts. Échap pour quitter.'
    for ($i = 0; $i -lt $screens.Count; $i++) {
        Add-TestButton $tile "Écran $($i + 1)$(if ($screens[$i].Primary -and $screens.Count -gt 1) { ' (principal)' })" { param($s, $e) $x = $s.Tag; Start-PixelTest $x.Ctx.Index } @{ Index = $i } -Primary:($i -eq 0)
    }

    if ($script:IsLaptop -and @($script:AnalysisData.Battery).Count) {
        $tile = New-TestTile 'BAT' 'Batterie' 'Rapport de Windows' 'Capacité d''origine, capacité actuelle et autonomie estimée.'
        Add-TestButton $tile 'Voir le rapport' {
            param($s, $e)
            $out = Join-Path $env:TEMP 'rapport-batterie.html'
            Start-Process -FilePath 'powercfg.exe' -ArgumentList '/batteryreport', '/output', "`"$out`"" -Wait -WindowStyle Hidden
            if (Test-Path $out) { Start-Process $out }
        } @{} -Primary
    }
}
