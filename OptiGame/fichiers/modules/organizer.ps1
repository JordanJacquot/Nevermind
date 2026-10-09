# Nevermind : organizer Dofus (page Jeux, bouton « Organizer Dofus »).
# Repère les fenêtres Dofus ouvertes (nom du perso lu dans le titre), les range dans l'ordre d'initiative,
# et passe d'un perso à l'autre au clavier ou avec une petite barre flottante.
# Nevermind ne touche jamais au jeu : il met seulement la bonne fenêtre devant (comme un clic dans la barre des tâches).
# Chargé par OptiGame.ps1 après bibliotheque.ps1.

$OrgFile = Join-Path $DataDir 'organizer.json'
$OrgMaxKeys = 8
$OrgHotkeyBase = 7100   # 7100 suivant, 7101 précédent, 7111 à 7118 perso 1 à 8

# Couleur de chaque classe (pastille du perso) ; une classe inconnue prend une couleur tirée de son nom
$OrgClassColors = @{
    iop = '#E5484D'; cra = '#46A758'; eniripsa = '#E93D82'; sacrieur = '#B3261E'; ecaflip = '#F5C542'; enutrof = '#C08B3A'
    sram = '#6E56CF'; xelor = '#3E63DD'; feca = '#2EB8D6'; osamodas = '#8D6E4A'; sadida = '#5BB98C'; pandawa = '#12A594'
    roublard = '#A33A3A'; zobal = '#8E4EC6'; steamer = '#0D74CE'; eliotrope = '#00B3C7'; huppermage = '#7C66DC'
    ouginak = '#D9822B'; forgelance = '#6B7F99'
}

# ---------------------------------------------------------------------------
# Réglages (organizer.json) : actif, ordre des persos, touches, barre flottante
# ---------------------------------------------------------------------------
function Get-OrgConfig {
    if ($script:OrgConfig) { return $script:OrgConfig }
    $c = @{ On = $false; Order = @(); Bar = $true; BarAlways = $false; BarX = $null; BarY = $null
        Hotkeys = @{ Next = 'Ctrl+Tab'; Prev = 'Ctrl+Maj+Tab' } }
    for ($i = 1; $i -le $OrgMaxKeys; $i++) { $c.Hotkeys["P$i"] = "F$i" }
    try {
        if (Test-Path -LiteralPath $OrgFile) {
            $j = Get-Content -LiteralPath $OrgFile -Raw -Encoding UTF8 | ConvertFrom-Json
            foreach ($k in 'On', 'Bar', 'BarAlways') { if ($null -ne $j.$k) { $c[$k] = [bool]$j.$k } }
            foreach ($k in 'BarX', 'BarY') { if ($null -ne $j.$k) { $c[$k] = [double]$j.$k } }
            $c.Order = @($j.Order | Where-Object { $_ } | ForEach-Object { [string]$_ })
            if ($j.Hotkeys) { foreach ($p in $j.Hotkeys.PSObject.Properties) { $c.Hotkeys[$p.Name] = [string]$p.Value } }
        }
    } catch { Write-Log "Organizer : réglages illisibles : $_" }
    $script:OrgConfig = $c
    $c
}

function Save-OrgConfig {
    $c = Get-OrgConfig
    try { [IO.File]::WriteAllText($OrgFile, (ConvertTo-Json -InputObject $c -Depth 4), (New-Object Text.UTF8Encoding($false))) } catch { Write-Log "Organizer : enregistrement impossible : $_" }
}

# ---------------------------------------------------------------------------
# Fenêtres Dofus
# ---------------------------------------------------------------------------
# Titre de Dofus 3 : « Perso - Classe - 3.x - Release » ; Dofus 2 : « Perso - Dofus 2.x » ; écran de connexion : « Dofus »
function ConvertFrom-DofusTitle([string]$Title) {
    $parts = @($Title -split ' - ' | ForEach-Object { $_.Trim() })
    if ($parts.Count -lt 2 -or $parts[0] -match '^Dofus\b' -or -not $parts[0]) { return @{ Name = ''; Class = '' } }
    $cls = if ($parts[1] -notmatch '^(Dofus|\d)') { $parts[1] } else { '' }
    @{ Name = $parts[0]; Class = $cls }
}

# Fenêtres ouvertes : @{ Name, Class, Pid, Hwnd, Title } (la copie de test peut en fournir de fausses)
function Get-DofusWindows {
    if ($null -ne $script:OrgFake) { return @($script:OrgFake) }
    @(Get-Process -Name 'Dofus*' -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero } | ForEach-Object {
        $t = ConvertFrom-DofusTitle $_.MainWindowTitle
        @{ Name = $t.Name; Class = $t.Class; Pid = $_.Id; Hwnd = $_.MainWindowHandle; Title = $_.MainWindowTitle }
    })
}

# Liste dans l'ordre d'initiative : persos connus (connectés ou non), puis nouveaux, puis fenêtres encore à l'écran de connexion
function Get-OrgList {
    $c = Get-OrgConfig
    $wins = @(Get-DofusWindows)
    $byName = @{}
    foreach ($w in $wins) { if ($w.Name -and -not $byName.ContainsKey($w.Name)) { $byName[$w.Name] = $w } }
    $added = $false
    foreach ($w in $wins) { if ($w.Name -and $c.Order -notcontains $w.Name) { $c.Order = @($c.Order) + $w.Name; $added = $true } }
    if ($added) { Save-OrgConfig }
    $list = @(foreach ($n in $c.Order) {
        $w = $byName[$n]
        if ($w) { @{ Name = $n; Class = $w.Class; Hwnd = $w.Hwnd; Pid = $w.Pid; Online = $true } }
        else { @{ Name = $n; Class = [string]$script:OrgClassSeen[$n]; Hwnd = [IntPtr]::Zero; Pid = 0; Online = $false } }
    })
    foreach ($w in $wins) { if (-not $w.Name) { $list += @{ Name = ''; Class = ''; Hwnd = $w.Hwnd; Pid = $w.Pid; Online = $true } } }
    foreach ($w in $wins) { if ($w.Name -and $w.Class) { $script:OrgClassSeen[$w.Name] = $w.Class } }
    $list
}
if (-not $script:OrgClassSeen) { $script:OrgClassSeen = @{} }

# Persos connectés seulement, numérotés dans l'ordre (perso 1 = F1 par défaut)
function Get-OrgOnline { @(Get-OrgList | Where-Object { $_.Online -and $_.Name }) }

# Perso lâché sur un autre : il prend sa place (en descendant il passe après lui, en montant avant lui)
function Move-OrgChar([string]$Name, [string]$Target) {
    $c = Get-OrgConfig
    $l = New-Object System.Collections.Generic.List[string]
    foreach ($n in $c.Order) { $l.Add([string]$n) }
    $from = $l.IndexOf($Name); $to = $l.IndexOf($Target)
    if ($from -lt 0 -or $to -lt 0 -or $from -eq $to) { return }
    $l.RemoveAt($from)
    $l.Insert($to, $Name)
    $c.Order = @($l)
    Save-OrgConfig
}

function Remove-OrgChar([string]$Name) {
    $c = Get-OrgConfig
    $c.Order = @($c.Order | Where-Object { $_ -ne $Name })
    Save-OrgConfig
}

# Perso à afficher : suivant / précédent par rapport à la fenêtre devant, ou le n-ième
function Get-OrgTarget([string]$Action) {
    $on = @(Get-OrgOnline)
    if (-not $on.Count) { return $null }
    if ($Action -match '^P\d+$') { $n = [int]$Action.Substring(1); if ($n -le $on.Count) { return $on[$n - 1] } else { return $null } }
    $fg = if ($script:OrgFakeFg) { $script:OrgFakeFg } else { [WinFocus]::Foreground() }
    $i = -1
    for ($k = 0; $k -lt $on.Count; $k++) { if ($on[$k].Hwnd -eq $fg) { $i = $k } }
    if ($i -lt 0) { return $on[0] }
    if ($Action -eq 'Next') { $on[($i + 1) % $on.Count] } else { $on[($i - 1 + $on.Count) % $on.Count] }
}

function Show-OrgChar($Char) {
    if (-not $Char -or $Char.Hwnd -eq [IntPtr]::Zero) { return $false }
    $ok = [WinFocus]::Focus($Char.Hwnd)
    if (-not $ok) { Write-Log "Organizer : Windows a refusé d'afficher $($Char.Name)." }
    Update-OrgBar
    $ok
}

# ---------------------------------------------------------------------------
# Raccourcis : texte « Ctrl+Maj+Tab » <-> touche de Windows
# Ils ne sont pris à Windows que quand une fenêtre Dofus est devant : ailleurs, les touches restent normales.
# ---------------------------------------------------------------------------
function ConvertTo-OrgHotkey([string]$Text) {
    if (-not $Text) { return $null }
    $mods = 0; $key = $null
    foreach ($p in ($Text -split '\+')) {
        switch ($p.Trim()) {
            'Ctrl' { $mods = $mods -bor 2 } 'Alt' { $mods = $mods -bor 1 } 'Maj' { $mods = $mods -bor 4 } 'Win' { $mods = $mods -bor 8 }
            default { try { $key = [System.Windows.Input.Key]$p.Trim() } catch {} }
        }
    }
    if ($null -eq $key -or $key -eq [System.Windows.Input.Key]::None) { return $null }
    @{ Mods = $mods; Vk = [System.Windows.Input.KeyInterop]::VirtualKeyFromKey($key) }
}

# Nom lisible d'une touche (D1 -> 1, NumPad1 -> Pavé 1)
function Format-OrgKey([string]$Text) {
    if (-not $Text) { return 'Aucune' }
    (@($Text -split '\+' | ForEach-Object {
        $p = $_.Trim()
        if ($p -match '^D(\d)$') { $Matches[1] } elseif ($p -match '^NumPad(\d)$') { "Pavé $($Matches[1])" }
        else { switch ($p) { 'Space' { 'Espace' } 'Return' { 'Entrée' } 'Escape' { 'Échap' } 'Back' { 'Retour' } 'Oem7' { '²' } 'Oem3' { 'ù' } default { $p } } }
    })) -join ' + '
}

function Register-OrgHotkeys {
    if ($script:OrgKeysOn) { return }
    $c = Get-OrgConfig
    try {
        $h = (New-Object System.Windows.Interop.WindowInteropHelper $Window).EnsureHandle()
        if (-not $script:OrgHook) {
            $script:OrgHook = [System.Windows.Interop.HwndSourceHook] {
                param([IntPtr]$hwnd, [int]$msg, [IntPtr]$wParam, [IntPtr]$lParam, [ref]$handled)
                if ($msg -eq 0x0312) {
                    $id = $wParam.ToInt32()
                    if ($script:OrgKeyIds -and $script:OrgKeyIds.ContainsKey($id)) {
                        try { [void](Show-OrgChar (Get-OrgTarget $script:OrgKeyIds[$id])) } catch { Write-Log "Organizer : $_" }
                        $handled.Value = $true
                    }
                }
                [IntPtr]::Zero
            }
            [System.Windows.Interop.HwndSource]::FromHwnd($h).AddHook($script:OrgHook)
        }
        $script:OrgKeyIds = @{}
        $actions = @('Next', 'Prev') + @(1..$OrgMaxKeys | ForEach-Object { "P$_" })
        foreach ($a in $actions) {
            $hk = ConvertTo-OrgHotkey $c.Hotkeys[$a]
            if (-not $hk) { continue }
            $id = if ($a -eq 'Next') { $OrgHotkeyBase } elseif ($a -eq 'Prev') { $OrgHotkeyBase + 1 } else { $OrgHotkeyBase + 10 + [int]$a.Substring(1) }
            if ([OGNative]::AddHotKey($h, $id, [uint32]$hk.Mods, [uint32]$hk.Vk)) { $script:OrgKeyIds[$id] = $a }
        }
        $script:OrgKeysHandle = $h
        $script:OrgKeysOn = $true
    } catch { Write-Log "Organizer, raccourcis : $_" }
}

function Unregister-OrgHotkeys {
    if (-not $script:OrgKeysOn) { return }
    foreach ($id in @($script:OrgKeyIds.Keys)) { try { [OGNative]::RemoveHotKey($script:OrgKeysHandle, $id) } catch {} }
    $script:OrgKeyIds = @{}
    $script:OrgKeysOn = $false
}

# ---------------------------------------------------------------------------
# Surveillance : fenêtres toutes les 2 s, fenêtre devant toutes les 250 ms
# ---------------------------------------------------------------------------
function Start-OrgWatch {
    if (-not $script:OrgTimer) {
        $script:OrgTimer = New-Object System.Windows.Threading.DispatcherTimer
        $script:OrgTimer.Interval = [TimeSpan]::FromMilliseconds(250)
        $script:OrgTimer.Add_Tick({ try { Update-OrgWatch } catch { Write-Log "Organizer : $_" } })
    }
    $script:OrgTick = 0
    $script:OrgTimer.Start()
    Update-OrgWatch -Full
}

function Stop-OrgWatch {
    if ($script:OrgTimer) { $script:OrgTimer.Stop() }
    Unregister-OrgHotkeys
    Hide-OrgBar
}

function Update-OrgWatch([switch]$Full) {
    $script:OrgTick++
    if ($Full -or $script:OrgTick % 8 -eq 0 -or -not $script:OrgLast) {
        $list = @(Get-OrgList)
        $sig = ($list | ForEach-Object { "$($_.Name)|$($_.Hwnd)|$($_.Online)" }) -join ';'
        $script:OrgHwnds = @{}
        foreach ($x in $list) { if ($x.Online) { $script:OrgHwnds[[int64]$x.Hwnd] = $true } }
        if ($sig -ne $script:OrgSig) {
            $script:OrgSig = $sig
            if ($script:OrgViewOn -and -not $script:OrgDragging) { Build-OrgPanel }
            $script:OrgBarSig = $null
        }
        $script:OrgLast = Get-Date
    }
    $fg = [WinFocus]::Foreground()
    $inGame = $script:OrgHwnds -and $script:OrgHwnds.ContainsKey([int64]$fg)
    if ($inGame) { Register-OrgHotkeys } else { Unregister-OrgHotkeys }
    $c = Get-OrgConfig
    $want = $c.Bar -and $script:OrgHwnds.Count -gt 0 -and ($inGame -or $c.BarAlways -or ($script:OrgBar -and $script:OrgBar.Win.IsMouseOver))
    if ($want) { Show-OrgBar; if ($fg -ne $script:OrgBarFg) { $script:OrgBarFg = $fg; Update-OrgBar } } else { Hide-OrgBar }
}

function Set-OrgOn([bool]$On) {
    $c = Get-OrgConfig
    $c.On = $On
    Save-OrgConfig
    if ($On) { Start-OrgWatch } else { Stop-OrgWatch }
    if ($script:OrgViewOn) { Build-OrgPanel }
    Set-Status $(if ($On) { 'Organizer Dofus activé.' } else { 'Organizer Dofus coupé.' })
}

# ---------------------------------------------------------------------------
# Pastille d'un perso : initiale sur la couleur de sa classe
# ---------------------------------------------------------------------------
function Get-OrgColor($Char) {
    $k = ConvertTo-SearchText ([string]$Char.Class)
    if ($k -and $OrgClassColors.ContainsKey($k)) { return $OrgClassColors[$k] }
    Get-GameHue $(if ($Char.Name) { $Char.Name } else { 'Dofus' })
}

function New-OrgBadge($Char, [double]$Size) {
    $g = New-Object System.Windows.Controls.Grid
    $g.Width = $Size; $g.Height = $Size
    $e = New-Object System.Windows.Shapes.Ellipse
    $col = Get-OrgColor $Char
    $e.Fill = New-RawGradient @($col, '#33000000') 0 1
    $e.Stroke = New-RawBrush '#55FFFFFF'; $e.StrokeThickness = 1
    [void]$g.Children.Add($e)
    $l = New-Text $(if ($Char.Name) { $Char.Name.Substring(0, 1).ToUpper() } else { '?' }) ($Size * 0.45) '#FFFFFF' -Bold
    $l.HorizontalAlignment = 'Center'; $l.VerticalAlignment = 'Center'; $l.TextWrapping = 'NoWrap'
    [void]$g.Children.Add($l)
    $g
}

# ---------------------------------------------------------------------------
# Barre flottante : un bouton par perso connecté, par dessus le jeu, sans jamais lui prendre le focus
# ---------------------------------------------------------------------------
function Show-OrgBar {
    if ($script:OrgBar) {
        if (-not $script:OrgBar.Win.IsVisible) { $script:OrgBar.Win.Show() }
        if ($script:OrgBarSig -ne $script:OrgSig) { Update-OrgBar }
        return
    }
    $w = New-Object System.Windows.Window
    $w.WindowStyle = 'None'; $w.AllowsTransparency = $true
    $w.Background = [System.Windows.Media.Brushes]::Transparent
    $w.Topmost = $true; $w.ShowInTaskbar = $false; $w.ShowActivated = $false
    $w.SizeToContent = 'WidthAndHeight'; $w.ResizeMode = 'NoResize'
    $w.Title = 'Nevermind Organizer'
    $w.Add_SourceInitialized({ param($s, $e) try { [WinFocus]::NoActivate((New-Object System.Windows.Interop.WindowInteropHelper $s).Handle) } catch {} })
    $root = New-Object System.Windows.Controls.Border
    $root.Background = Get-Brush '#E60B0820'
    $root.BorderBrush = New-LinearBrush @('#B000E5FF', '#B0FF2EB5') 0 0 1 1
    $root.BorderThickness = New-Thickness 1 1 1 1
    $root.CornerRadius = [System.Windows.CornerRadius]::new($(if (($PackFont -and $ThemePack.FontPixel) -or $Theme.Decor -eq 'terminal') { 3 } else { 14 }))
    $root.Padding = New-Thickness 4 4 6 4
    $row = New-Object System.Windows.Controls.StackPanel
    $row.Orientation = 'Horizontal'
    $root.Child = $row
    $w.Content = $root
    $c = Get-OrgConfig
    $w.WindowStartupLocation = 'Manual'
    $wa = [System.Windows.SystemParameters]::WorkArea
    $w.Left = if ($null -ne $c.BarX) { $c.BarX } else { $wa.Left + $wa.Width / 2 - 150 }
    $w.Top = if ($null -ne $c.BarY) { $c.BarY } else { $wa.Top + 6 }
    $script:OrgBar = @{ Win = $w; Row = $row; Items = @{} }
    Update-OrgBar
    $w.Show()
    # Hors de l'écran (écran débranché) : retour en haut au centre
    if ($w.Left -gt [System.Windows.SystemParameters]::VirtualScreenWidth - 40 -or $w.Top -gt [System.Windows.SystemParameters]::VirtualScreenHeight - 20) { Reset-OrgBarPosition }
}

function Hide-OrgBar {
    if ($script:OrgBar -and $script:OrgBar.Win.IsVisible) { $script:OrgBar.Win.Hide() }
}

function Reset-OrgBarPosition {
    $c = Get-OrgConfig
    $c.BarX = $null; $c.BarY = $null
    Save-OrgConfig
    if ($script:OrgBar) {
        $wa = [System.Windows.SystemParameters]::WorkArea
        $script:OrgBar.Win.Left = $wa.Left + ($wa.Width - $script:OrgBar.Win.ActualWidth) / 2
        $script:OrgBar.Win.Top = $wa.Top + 6
    }
}

function Update-OrgBar {
    $b = $script:OrgBar
    if (-not $b) { return }
    $b.Row.Children.Clear()
    $b.Items = @{}
    $script:OrgBarSig = $script:OrgSig
    # Poignée : glisser pour déplacer la barre
    $grip = New-Object System.Windows.Controls.Border
    $grip.Background = Get-Brush '#01FFFFFF'; $grip.Padding = New-Thickness 4 0 4 0
    $grip.Cursor = [System.Windows.Input.Cursors]::SizeAll
    $grip.ToolTip = 'Glisse pour déplacer la barre'
    $gi = New-Text ([string][char]0xE76F) 12 '#8E88A8'
    $gi.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'; $gi.VerticalAlignment = 'Center'
    $grip.Child = $gi
    $grip.Add_MouseLeftButtonDown({
        param($s, $e)
        try {
            $script:OrgBar.Win.DragMove()
            $c = Get-OrgConfig; $c.BarX = $script:OrgBar.Win.Left; $c.BarY = $script:OrgBar.Win.Top; Save-OrgConfig
        } catch {}
    })
    [void]$b.Row.Children.Add($grip)
    $fg = [WinFocus]::Foreground()
    $n = 0
    foreach ($ch in @(Get-OrgOnline)) {
        $n++
        $on = $ch.Hwnd -eq $fg
        $pill = New-Object System.Windows.Controls.Border
        $pill.CornerRadius = [System.Windows.CornerRadius]::new($(if (($PackFont -and $ThemePack.FontPixel) -or $Theme.Decor -eq 'terminal') { 2 } else { 10 }))
        $pill.Padding = New-Thickness 5 3 9 3; $pill.Margin = New-Thickness 2 0 2 0
        $pill.Cursor = [System.Windows.Input.Cursors]::Hand
        $pill.Background = if ($on) { New-LinearBrush @('#6600E5FF', '#66B04BFF') 0 0 1 0 } else { Get-Brush '#10FFFFFF' }
        $pill.BorderBrush = Get-Brush $(if ($on) { '#00E5FF' } else { '#00FFFFFF' }); $pill.BorderThickness = New-Thickness 1 1 1 1
        $sp = New-Object System.Windows.Controls.StackPanel
        $sp.Orientation = 'Horizontal'
        $bd = New-OrgBadge $ch 22
        $bd.Margin = New-Thickness 0 0 6 0
        [void]$sp.Children.Add($bd)
        $t = New-Text $ch.Name 12 $(if ($on) { '#FFFFFF' } else { '#D3CDE3' }) -Semi
        $t.TextWrapping = 'NoWrap'; $t.VerticalAlignment = 'Center'; $t.MaxWidth = 110; $t.TextTrimming = 'CharacterEllipsis'
        Set-OverlayFont $t 8
        [void]$sp.Children.Add($t)
        $pill.Child = $sp
        $key = (Get-OrgConfig).Hotkeys["P$n"]
        $pill.ToolTip = "$($ch.Name)$(if ($ch.Class) { " ($($ch.Class))" })$(if ($key) { " : $(Format-OrgKey $key)" })"
        $pill.Tag = $ch.Name
        $pill.Add_MouseLeftButtonUp({ param($s, $e) $nm = [string]$s.Tag; Invoke-Safe { [void](Show-OrgChar (@(Get-OrgOnline | Where-Object { $_.Name -eq $nm })[0])) } })
        [void]$b.Row.Children.Add($pill)
        $b.Items[$ch.Name] = $pill
    }
    if (-not $n) {
        $t = New-Text 'Aucun perso connecté' 12 '#8E88A8'
        $t.Margin = New-Thickness 6 4 6 4; $t.TextWrapping = 'NoWrap'
        [void]$b.Row.Children.Add($t)
    }
}

# ---------------------------------------------------------------------------
# Page : bouton « Organizer Dofus » de la page Jeux
# ---------------------------------------------------------------------------
function Show-OrgView([bool]$On) {
    $script:OrgViewOn = $On
    $lib = if ($On) { 'Collapsed' } else { 'Visible' }
    foreach ($n in 'LibToolbar', 'LibBody', 'BtnLibRefresh', 'BtnLibAdd') { if ($ui[$n]) { $ui[$n].Visibility = $lib } }
    if ($ui.OrgScroll) { $ui.OrgScroll.Visibility = if ($On) { 'Visible' } else { 'Collapsed' } }
    $ui.LibTitle.Text = if ($On) { 'Organizer Dofus' } else { 'Mes jeux' }
    $ui.BtnLibOrganizer.Content = if ($On) { 'Retour à mes jeux' } else { 'Organizer Dofus' }
    if ($On) {
        $ui.LibSub.Text = 'Passe d''un perso à l''autre au clavier ou avec la barre flottante.'
        Build-OrgPanel
    } elseif ($script:LibBuilt) { Update-LibraryView }
}

function New-OrgKeyButton([string]$Action) {
    $c = Get-OrgConfig
    $b = New-Object System.Windows.Controls.Border
    $b.CornerRadius = [System.Windows.CornerRadius]::new(8)
    $b.Padding = New-Thickness 10 4 10 4
    $b.MinWidth = 70
    $b.Cursor = [System.Windows.Input.Cursors]::Hand
    $capturing = $script:OrgCapture -eq $Action
    $b.Background = Get-Brush $(if ($capturing) { '#3300E5FF' } else { '#1AFFFFFF' })
    $b.BorderBrush = Get-Brush $(if ($capturing) { '#00E5FF' } else { '#33FFFFFF' }); $b.BorderThickness = New-Thickness 1 1 1 3
    $t = New-Text $(if ($capturing) { 'Appuie sur une touche...' } else { Format-OrgKey $c.Hotkeys[$Action] }) 12 $(if ($c.Hotkeys[$Action] -or $capturing) { '#FFFFFF' } else { '#8E88A8' }) -Bold
    $t.TextWrapping = 'NoWrap'; $t.HorizontalAlignment = 'Center'
    $t.FontFamily = New-Object System.Windows.Media.FontFamily $MonoFont
    $b.Child = $t
    $b.ToolTip = 'Clique puis appuie sur la touche voulue (Échap : annuler, Retour arrière : aucune touche)'
    $b.Tag = $Action
    $b.Add_MouseLeftButtonUp({ param($s, $e) $script:OrgCapture = [string]$s.Tag; Build-OrgPanel; $Window.Focus() })
    $b
}

# Touche capturée dans la fenêtre de Nevermind (Window.PreviewKeyDown)
function Receive-OrgKey($E) {
    if (-not $script:OrgCapture) { return }
    $E.Handled = $true
    $key = if ($E.Key -eq [System.Windows.Input.Key]::System) { $E.SystemKey } else { $E.Key }
    if ($key -in 'LeftCtrl', 'RightCtrl', 'LeftShift', 'RightShift', 'LeftAlt', 'RightAlt', 'LWin', 'RWin') { return }
    $action = $script:OrgCapture
    $script:OrgCapture = $null
    $c = Get-OrgConfig
    if ($key -eq 'Escape') { Build-OrgPanel; return }
    if ($key -eq 'Back' -or $key -eq 'Delete') { $c.Hotkeys[$action] = '' }
    else {
        $m = [int][System.Windows.Input.Keyboard]::Modifiers
        $parts = @()
        if ($m -band 2) { $parts += 'Ctrl' }
        if ($m -band 1) { $parts += 'Alt' }
        if ($m -band 4) { $parts += 'Maj' }
        if ($m -band 8) { $parts += 'Win' }
        $parts += [string]$key
        $txt = $parts -join '+'
        # Une même touche ne sert qu'à une action
        foreach ($k in @($c.Hotkeys.Keys)) { if ($k -ne $action -and $c.Hotkeys[$k] -eq $txt) { $c.Hotkeys[$k] = '' } }
        $c.Hotkeys[$action] = $txt
    }
    Save-OrgConfig
    if ($script:OrgKeysOn) { Unregister-OrgHotkeys }
    Build-OrgPanel
    Set-Status "Organizer : $(Format-OrgKey $c.Hotkeys[$action])."
}

function New-OrgKeyRow([string]$Label, [string]$Action) {
    $g = New-Grid @('*', 'Auto')
    $g.Margin = New-Thickness 0 0 0 8
    $l = New-Text $Label 13 '#EEEBF7' -Semi
    $l.VerticalAlignment = 'Center'
    Add-ToGrid $g $l 0
    Add-ToGrid $g (New-OrgKeyButton $Action) 1
    $g
}

function Build-OrgPanel {
    $panel = $ui.OrgPanel
    if (-not $panel) { return }
    $panel.Children.Clear()
    $c = Get-OrgConfig
    $list = @(Get-OrgList)
    $script:OrgRows = @{}

    $top = New-Grid @('*', '330')
    # Persos
    $pc = New-OverlayPanelCard 'Mes personnages' 'Range les dans ton ordre d''initiative : glisse une ligne pour la déplacer.' '#00E5FF'
    $pc.Card.Margin = New-Thickness 0 0 16 0
    $pc.Card.VerticalAlignment = 'Top'
    $pc.Card.Padding = New-Thickness 20 18 20 18
    $sw = New-SwitchRow 'Activer l''organizer' 'Raccourcis clavier et barre flottante pendant que tu joues.' $c.On { param($s, $e) $v = [bool]$s.IsChecked; Invoke-Safe { Set-OrgOn $v } }
    [void]$pc.Body.Children.Add($sw)
    $named = @($list | Where-Object { $_.Name })
    if (-not $list.Count) {
        $e = New-Object System.Windows.Controls.Border
        $e.CornerRadius = [System.Windows.CornerRadius]::new(14); $e.Background = Get-Brush '#0AFFFFFF'; $e.Padding = New-Thickness 16 18 16 18
        $et = New-Text 'Aucune fenêtre Dofus ouverte. Lance tes comptes : tes persos apparaîtront ici tout seuls, dès qu''ils sont connectés.' 12.5 '#A6A1BC'
        $e.Child = $et
        [void]$pc.Body.Children.Add($e)
    }
    $num = 0
    foreach ($ch in $list) {
        $row = New-Object System.Windows.Controls.Border
        $row.CornerRadius = [System.Windows.CornerRadius]::new(12)
        $row.Padding = New-Thickness 10 8 10 8; $row.Margin = New-Thickness 0 0 0 6
        $row.Background = Get-Brush $(if ($ch.Online) { '#12FFFFFF' } else { '#06FFFFFF' })
        $row.BorderBrush = Get-Brush '#14FFFFFF'; $row.BorderThickness = New-Thickness 1 1 1 1
        $g = New-Grid @('Auto', 'Auto', 'Auto', '*', 'Auto')
        # Poignée
        $grip = New-Text ([string][char]0xE76F) 13 '#655E7E'
        $grip.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
        $grip.VerticalAlignment = 'Center'; $grip.Margin = New-Thickness 0 0 10 0
        if ($ch.Name) { $grip.Cursor = [System.Windows.Input.Cursors]::SizeAll } else { $grip.Opacity = 0 }
        Add-ToGrid $g $grip 0
        # Numéro (connectés seulement : c'est celui des touches F1, F2...)
        $nb = New-Text $(if ($ch.Online -and $ch.Name) { $num++; [string]$num } else { '' }) 13 '#00E5FF' -Bold
        $nb.Width = 18; $nb.VerticalAlignment = 'Center'
        Add-ToGrid $g $nb 1
        $bd = New-OrgBadge $ch 30
        $bd.Margin = New-Thickness 0 0 10 0
        if (-not $ch.Online) { $bd.Opacity = 0.4 }
        Add-ToGrid $g $bd 2
        $info = New-Object System.Windows.Controls.StackPanel
        $info.VerticalAlignment = 'Center'
        $nt = New-Text $(if ($ch.Name) { $ch.Name } else { 'Écran de connexion' }) 13.5 $(if ($ch.Online) { '#FFFFFF' } else { '#8E88A8' }) -Semi
        $nt.TextWrapping = 'NoWrap'; $nt.TextTrimming = 'CharacterEllipsis'
        [void]$info.Children.Add($nt)
        $st = if (-not $ch.Name) { 'Pas encore de perso choisi' } elseif ($ch.Online) { "$(if ($ch.Class) { "$($ch.Class)  ·  " })Connecté" } else { 'Pas connecté' }
        [void]$info.Children.Add((New-Text $st 11.5 $(if ($ch.Online -and $ch.Name) { $Colors.ok } else { '#8E88A8' })))
        Add-ToGrid $g $info 3
        $acts = New-Object System.Windows.Controls.StackPanel
        $acts.Orientation = 'Horizontal'; $acts.VerticalAlignment = 'Center'
        if ($ch.Online) {
            $b = New-Button 'Afficher'
            $b.Tag = [int64]$ch.Hwnd
            $b.Add_Click({ param($s, $e) $h = [IntPtr][int64]$s.Tag; Invoke-Safe { [void](Show-OrgChar @{ Name = ''; Hwnd = $h }) } })
            [void]$acts.Children.Add($b)
        } elseif ($ch.Name) {
            $b = New-Button 'Retirer'
            $b.ToolTip = 'Enlever ce perso de la liste (il reviendra s''il se reconnecte)'
            $b.Tag = $ch.Name
            $b.Add_Click({ param($s, $e) $nm = [string]$s.Tag; Invoke-Safe { Remove-OrgChar $nm; Build-OrgPanel } })
            [void]$acts.Children.Add($b)
        }
        Add-ToGrid $g $acts 4
        $row.Child = $g
        if ($ch.Name) {
            $row.Tag = $ch.Name
            $row.AllowDrop = $true
            $grip.Tag = $ch.Name
            $grip.Add_MouseLeftButtonDown({ param($s, $e) $script:OrgDragging = $true; try { [void][System.Windows.DragDrop]::DoDragDrop($s, ('org:' + [string]$s.Tag), 'Move') } catch {} finally { $script:OrgDragging = $false } })
            $row.Add_DragOver({ param($s, $e) $e.Effects = 'Move'; $s.BorderBrush = Get-Brush '#00E5FF'; $e.Handled = $true })
            $row.Add_DragLeave({ param($s, $e) $s.BorderBrush = Get-Brush '#14FFFFFF' })
            $row.Add_Drop({
                param($s, $e)
                $d = [string]$e.Data.GetData([string])
                if ($d -like 'org:*') { $from = $d.Substring(4); $to = [string]$s.Tag; Invoke-Safe { Move-OrgChar $from $to; Build-OrgPanel; Update-OrgBar } }
            })
            $script:OrgRows[$ch.Name] = $row
        }
        [void]$pc.Body.Children.Add($row)
    }
    Add-ToGrid $top $pc.Card 0

    # Raccourcis et barre
    $side = New-Object System.Windows.Controls.StackPanel
    $kc = New-OverlayPanelCard 'Raccourcis' 'Ils ne marchent que quand une fenêtre Dofus est devant : ailleurs, tes touches restent normales.' '#B04BFF'
    $kc.Card.Padding = New-Thickness 20 18 20 18
    [void]$kc.Body.Children.Add((New-OrgKeyRow 'Perso suivant' 'Next'))
    [void]$kc.Body.Children.Add((New-OrgKeyRow 'Perso précédent' 'Prev'))
    $sub = New-Text 'Une touche par perso' 12 '#8E88A8' -Semi
    $sub.Margin = New-Thickness 0 8 0 8
    [void]$kc.Body.Children.Add($sub)
    $on = @($list | Where-Object { $_.Online -and $_.Name })
    for ($i = 1; $i -le $OrgMaxKeys; $i++) {
        $lbl = if ($i -le $on.Count) { "$i.  $($on[$i - 1].Name)" } else { "Perso $i" }
        [void]$kc.Body.Children.Add((New-OrgKeyRow $lbl "P$i"))
    }
    [void]$side.Children.Add($kc.Card)

    $bc = New-OverlayPanelCard 'Barre flottante' 'Un bouton par perso connecté, par dessus le jeu.' '#FF2EB5'
    $bc.Card.Padding = New-Thickness 20 18 20 18
    $bc.Card.Margin = New-Thickness 0 16 0 0
    [void]$bc.Body.Children.Add((New-SwitchRow 'Afficher la barre' 'Visible quand une fenêtre Dofus est devant.' $c.Bar { param($s, $e) $v = [bool]$s.IsChecked; Invoke-Safe { $cc = Get-OrgConfig; $cc.Bar = $v; Save-OrgConfig; Update-OrgWatch -Full } }))
    [void]$bc.Body.Children.Add((New-SwitchRow 'Toujours visible' 'Même quand tu es sur une autre fenêtre que Dofus.' $c.BarAlways { param($s, $e) $v = [bool]$s.IsChecked; Invoke-Safe { $cc = Get-OrgConfig; $cc.BarAlways = $v; Save-OrgConfig; Update-OrgWatch -Full } }))
    $rb = New-Button 'Remettre la barre en haut au centre'
    $rb.HorizontalAlignment = 'Left'; $rb.Margin = New-Thickness 0 4 0 0
    $rb.Add_Click({ Invoke-Safe { Reset-OrgBarPosition; Set-Status 'Barre flottante remise en haut au centre.' } })
    [void]$bc.Body.Children.Add($rb)
    [void]$side.Children.Add($bc.Card)
    Add-ToGrid $top $side 1
    [void]$panel.Children.Add($top)

    $note = New-Text 'Nevermind ne fait que changer de fenêtre : il n''envoie aucune touche ni aucun clic au jeu et ne lit rien dedans, à part le titre de la fenêtre.' 12 '#655E7E'
    $note.Margin = New-Thickness 4 16 0 8
    [void]$panel.Children.Add($note)
}

# Au démarrage : l'organizer reprend s'il était actif
if ((Get-OrgConfig).On) { $null = $Window.Dispatcher.BeginInvoke([System.Windows.Threading.DispatcherPriority]::ApplicationIdle, [Action]{ try { Start-OrgWatch } catch { Write-Log "Organizer : $_" } }) }
$ui.BtnLibOrganizer.Add_Click({ Invoke-Safe { Show-OrgView (-not $script:OrgViewOn) } })
$Window.Add_PreviewKeyDown({ param($s, $e) if ($script:OrgCapture) { try { Receive-OrgKey $e } catch { Write-Log "Organizer : $_" } } })
