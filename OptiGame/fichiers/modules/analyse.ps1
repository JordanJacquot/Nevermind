# Nexo : santé des composants et analyse complète.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Santé des composants
# ---------------------------------------------------------------------------
# Numéro de pilote NVIDIA à partir de celui de Windows : 32.0.16.1714 donne 617.14.
function ConvertTo-NvidiaVersion([string]$WinVersion) {
    $digits = $WinVersion -replace '\D', ''
    if ($digits.Length -lt 5) { return $null }
    $d = $digits.Substring($digits.Length - 5)
    "$([int]$d.Substring(0, 3)).$($d.Substring(3))"
}
$StatusLabels = @{ ok = 'Bon état'; warn = 'À surveiller'; bad = 'Problème'; info = 'Info' }
$Muted = '#5B6475'

function Get-LoadColor([double]$Pct, [double]$Warn = 75, [double]$Bad = 90) {
    if ($Pct -ge $Bad) { $Colors.bad } elseif ($Pct -ge $Warn) { $Colors.warn } else { $Colors.ok }
}

function Get-EventCount([hashtable]$Filter) {
    try { @(Get-WinEvent -FilterHashtable $Filter -ErrorAction Stop).Count } catch { 0 }
}

function Format-Duration([TimeSpan]$Span) {
    if ($Span.TotalDays -ge 1) { return "$([int][math]::Floor($Span.TotalDays)) j $($Span.Hours) h" }
    "$($Span.Hours) h $($Span.Minutes) min"
}

function New-Component([string]$Tag, [string]$Titre, [string]$Sous) {
    @{
        Tag = $Tag; Titre = $Titre; Sous = $Sous; Status = 'ok'
        Bars = New-Object System.Collections.ArrayList
        Lines = [ordered]@{}
        Notes = New-Object System.Collections.ArrayList
        Action = $null; ActionLabel = $null
    }
}

function Set-Worse($C, [string]$Status) {
    $rank = @{ info = 0; ok = 0; warn = 1; bad = 2 }
    if ($rank[$Status] -gt $rank[$C.Status]) { $C.Status = $Status }
}

function Add-Note($C, [string]$Status, [string]$Text) {
    [void]$C.Notes.Add(@{ Status = $Status; Text = $Text })
    Set-Worse $C $Status
}

$SmiPath = @("$env:windir\System32\nvidia-smi.exe", "$env:ProgramFiles\NVIDIA Corporation\NVSMI\nvidia-smi.exe") |
    Where-Object { Test-Path $_ } | Select-Object -First 1

# Interroge nvidia-smi et renvoie les valeurs par nom de champ (null si non disponible).
function Invoke-Smi([string]$Fields) {
    if (-not $SmiPath) { return $null }
    try {
        $o = & $SmiPath "--query-gpu=$Fields" '--format=csv,noheader,nounits' 2>$null | Select-Object -First 1
        if (-not $o) { return $null }
        $vals = $o -split ','
        $names = $Fields -split ','
        $h = @{}
        for ($i = 0; $i -lt $names.Count; $i++) {
            $v = if ($i -lt $vals.Count) { $vals[$i].Trim() } else { '' }
            $h[$names[$i]] = if ($v -match '^[\d\.]+$') { [double]::Parse($v, [Globalization.CultureInfo]::InvariantCulture) } else { $null }
        }
        $h
    } catch { $null }
}

function Get-GpuVram([string]$Name) {
    $base = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}'
    foreach ($k in (Get-ChildItem $base -ErrorAction SilentlyContinue | Where-Object { $_.PSChildName -match '^\d{4}$' })) {
        $p = Get-ItemProperty $k.PSPath -ErrorAction SilentlyContinue
        if ($p -and $p.DriverDesc -eq $Name) {
            $m = $p.'HardwareInformation.qwMemorySize'
            if (-not $m) { $m = $p.'HardwareInformation.MemorySize' }
            if ($m -is [byte[]]) { $m = [BitConverter]::ToUInt32($m, 0) }
            if ($m) { return [double]$m }
        }
    }
    $null
}

function New-HealthCard($C) {
    $color = $Colors[$C.Status]
    $card = New-Card
    $card.Padding = New-Thickness 18 16 18 16
    $card.Margin = New-Thickness 0 0 0 12
    $sp = New-Object System.Windows.Controls.StackPanel

    # En-tête: pastille, titre, état
    $head = New-Grid @('Auto', '*', 'Auto')
    $tag = New-Object System.Windows.Controls.Border
    $tag.Width = 46; $tag.Height = 46
    $tag.CornerRadius = [System.Windows.CornerRadius]::new(10)
    $bg = Get-Brush $color; $bg.Opacity = 0.14
    $tag.Background = $bg
    $tt = New-Text $C.Tag 12 $color -Bold
    $tt.TextWrapping = 'NoWrap'
    $tt.HorizontalAlignment = 'Center'; $tt.VerticalAlignment = 'Center'
    $tag.Child = $tt
    Add-ToGrid $head $tag 0
    $ts = New-Object System.Windows.Controls.StackPanel
    $ts.Margin = New-Thickness 12 0 8 0
    $ts.VerticalAlignment = 'Center'
    [void]$ts.Children.Add((New-Text $C.Titre 15 '#FFFFFF' -Semi))
    if ($C.Sous) { [void]$ts.Children.Add((New-Text $C.Sous 12 '#9AA3B2')) }
    Add-ToGrid $head $ts 1
    $badge = New-Badge $StatusLabels[$C.Status] $color
    $badge.Margin = New-Thickness 0 0 0 0
    $badge.VerticalAlignment = 'Top'
    Add-ToGrid $head $badge 2
    [void]$sp.Children.Add($head)

    # Jauges
    foreach ($b in $C.Bars) {
        $row = New-Grid @('*', 'Auto')
        $row.Margin = New-Thickness 0 14 0 6
        Add-ToGrid $row (New-Text $b.Label 12.5 '#9AA3B2') 0
        Add-ToGrid $row (New-Text $b.Text 12.5 '#E6E8EE' -Semi) 1
        [void]$sp.Children.Add($row)
        $pb = New-Object System.Windows.Controls.ProgressBar
        $pb.Value = [math]::Min(100.0, [math]::Max(0.0, [double]$b.Value))
        $pb.Foreground = Get-Brush $b.Color
        [void]$sp.Children.Add($pb)
    }

    # Détails
    if ($C.Lines.Count) {
        $lines = New-Object System.Windows.Controls.StackPanel
        $lines.Margin = New-Thickness 0 12 0 0
        foreach ($k in $C.Lines.Keys) {
            $v = $C.Lines[$k]
            $txt = $v; $col = '#E6E8EE'
            if ($v -is [array]) { $txt = $v[0]; $col = $v[1] }
            $r = New-Grid @('165', '*')
            $r.Margin = New-Thickness 0 3 0 3
            Add-ToGrid $r (New-Text $k 12.5 '#9AA3B2') 0
            Add-ToGrid $r (New-Text ([string]$txt) 12.5 $col) 1
            [void]$lines.Children.Add($r)
        }
        [void]$sp.Children.Add($lines)
    }

    # Explications
    foreach ($n in $C.Notes) {
        $nb = New-Object System.Windows.Controls.Border
        $nbg = Get-Brush $Colors[$n.Status]; $nbg.Opacity = 0.10
        $nb.Background = $nbg
        $nb.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $nb.Padding = New-Thickness 12 8 12 8
        $nb.Margin = New-Thickness 0 10 0 0
        $nb.Child = New-Text $n.Text 12.5 $Colors[$n.Status]
        [void]$sp.Children.Add($nb)
    }

    if ($C.Action) {
        $btn = New-Button $C.ActionLabel
        $btn.HorizontalAlignment = 'Left'
        $btn.Margin = New-Thickness 0 12 0 0
        $btn.Tag = $C.Action
        $btn.Add_Click({ param($s, $e) Invoke-FindingAction $s.Tag })
        [void]$sp.Children.Add($btn)
    }
    $card.Child = $sp
    $card
}

# Répartit les cartes sur deux colonnes en équilibrant leur hauteur.
function Show-HealthCards($Cards) {
    $ui.HealthLeft.Children.Clear()
    $ui.HealthRight.Children.Clear()
    $hl = 0; $hr = 0
    foreach ($c in $Cards) {
        $h = 4 + 2 * $c.Bars.Count + $c.Lines.Count + 2 * $c.Notes.Count + $(if ($c.Action) { 2 } else { 0 })
        if ($hl -le $hr) { [void]$ui.HealthLeft.Children.Add((New-HealthCard $c)); $hl += $h }
        else { [void]$ui.HealthRight.Children.Add((New-HealthCard $c)); $hr += $h }
    }
    $n = @{ ok = 0; warn = 0; bad = 0 }
    foreach ($c in $Cards) { if ($n.ContainsKey($c.Status)) { $n[$c.Status]++ } else { $n.ok++ } }
    $parts = @("$($n.ok) en bon état")
    if ($n.warn) { $parts += "$($n.warn) à surveiller" }
    if ($n.bad)  { $parts += "$($n.bad) avec un problème" }
    $ui.HealthSummary.Text = $parts -join ', '
}

# ---------------------------------------------------------------------------
# Analyse complète
# ---------------------------------------------------------------------------
function Invoke-Analysis {
    Set-Busy $true
    Set-Status 'Analyse de ton PC en cours...'
    $F = New-Object System.Collections.ArrayList
    $cards = New-Object System.Collections.ArrayList
    $info = [ordered]@{}
    $since = (Get-Date).AddDays(-30)

    # Système (lecture en arrière plan, lancée dès l'ouverture de l'app)
    Set-Status 'Lecture des informations du PC...'
    if ($script:Prefetch) {
        $pf = $script:Prefetch; $script:Prefetch = $null
        Wait-Handle $pf.Handle
        try { $data = @($pf.PS.EndInvoke($pf.Handle))[0] } finally { $pf.PS.Dispose() }
    } else {
        $data = Invoke-Async $AnalysisDataWork $env:SystemDrive | Select-Object -First 1
    }
    $script:AnalysisData = $data
    $os = $data.OS
    $script:Build = [int]$os.BuildNumber
    # Sur un PC fixe, toute « batterie » est un onduleur, même si son nom ne dit rien (ex : CP1500EPFCLCD)
    $desk = Test-IsDesktop $data
    $ups = @($data.Battery | Where-Object { $desk -or (Test-IsUps $_) })
    $battery = @($data.Battery | Where-Object { -not ($desk -or (Test-IsUps $_)) })
    $hints = @($data.UpsHints | Where-Object { $_ })
    $script:HasUps = [bool]($ups.Count -or $hints.Count)
    $script:IsLaptop = Test-IsLaptop $battery $data
    if ($script:IsLaptop -and ($battery | Where-Object { $_.BatteryStatus -eq 1 })) {
        Add-Finding $F 'warn' 'Portable sur batterie' 'Sur batterie, Windows bride le processeur et la carte graphique. Branche le chargeur pour jouer.' 2 -Id 'laptop-battery' -Fix (New-Fix `
            -Why 'Sur batterie, le processeur et la carte graphique tournent au ralenti pour économiser l''énergie: tu peux perdre la moitié de tes FPS.' `
            -Steps @('Branche le chargeur de ton portable avant de jouer.', 'Dans Paramètres > Système > Alimentation, choisis le mode « Meilleures performances ».', 'Relance l''analyse.') `
            -Open 'ms-settings:powersleep' -OpenLabel 'Paramètres d''alimentation')
    }

    # --- Processeur
    Set-Status 'Analyse du processeur...'
    $cpu = $script:AnalysisData.CPU
    $cpuName = ($cpu.Name -replace '\s+', ' ').Trim()
    $Live.BaseMHz = [int]$cpu.MaxClockSpeed
    $info['Processeur'] = "$cpuName ($($cpu.NumberOfCores) cœurs, $($cpu.NumberOfLogicalProcessors) threads)"
    $c = New-Component 'CPU' 'Processeur' $cpuName
    $c.Lines['Cœurs'] = "$($cpu.NumberOfCores) cœurs, $($cpu.NumberOfLogicalProcessors) threads"
    $c.Lines['Fréquence de base'] = '{0:N1} GHz' -f ($cpu.MaxClockSpeed / 1000)
    if ($cpu.L3CacheSize) { $c.Lines['Cache L3'] = "$([math]::Round($cpu.L3CacheSize / 1024)) Mo" }
    $c.Lines['Température'] = @('Non fournie par Windows (utilise HWiNFO)', $Muted)
    [void]$cards.Add($c)
    Update-UI

    # --- Carte graphique
    Set-Status 'Analyse de la carte graphique...'
    $gpus = @($data.GPUs | Where-Object { $_.Name -notmatch 'Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta' })
    $info['Carte graphique'] = ($gpus | ForEach-Object { $_.Name }) -join ' + '
    $dedicatedPattern = 'NVIDIA|GeForce|Radeon RX|Radeon Pro|Arc'
    $hasDedicated = [bool]($gpus | Where-Object { $_.Name -match $dedicatedPattern })
    foreach ($g in $gpus) {
        $c = New-Component 'GPU' 'Carte graphique' $g.Name
        if ($g.Name -match 'Microsoft Basic') {
            Add-Note $c 'bad' 'Aucun vrai pilote installé: les jeux tournent très mal.'
            $c.Action = 'ms-settings:windowsupdate'; $c.ActionLabel = 'Mettre à jour'
            Add-Finding $F 'bad' 'Pilote graphique manquant' "Windows utilise un pilote générique: les jeux tournent très mal. Installe le pilote de ta carte graphique." 3 -Id 'gpu-nodriver' -Fix (New-Fix `
                -Why 'Sans le vrai pilote, la carte graphique ne sert presque à rien: pas d''accélération 3D correcte.' `
                -Steps @('Clique sur « Windows Update » et installe toutes les mises à jour, y compris les mises à jour facultatives de pilotes.', 'Si ça ne suffit pas, télécharge le pilote sur le site du fabricant de ta carte (NVIDIA, AMD ou Intel).', 'Redémarre puis relance l''analyse.') `
                -Open 'ms-settings:windowsupdate' -OpenLabel 'Windows Update')
            [void]$cards.Add($c)
            continue
        }
        $vram = Get-GpuVram $g.Name
        if ($vram) { $c.Lines['Mémoire vidéo'] = Format-Size $vram }
        $w = if ($hasDedicated -and $g.Name -notmatch $dedicatedPattern) { 1 } else { 2 }
        $nvInstalled = if ($g.Name -match 'NVIDIA|GeForce') { ConvertTo-NvidiaVersion $g.DriverVersion } else { $null }
        $nvLatest = $data.NvLatest
        if ($nvInstalled -and $nvLatest) {
            $isOld = $false
            try { $isOld = [version]$nvInstalled -lt [version]$nvLatest.Version } catch {}
            $c.Lines['Pilote'] = @("$nvInstalled$(if ($isOld) { " (la $($nvLatest.Version) est sortie)" } else { ' (le plus récent)' })", $(if ($isOld) { $Colors.warn } else { '#E6E8EE' }))
            if ($isOld) {
                Add-Note $c 'warn' "Nouveau pilote NVIDIA disponible : $($nvLatest.Version)."
                $c.Action = $nvLatest.Url; $c.ActionLabel = 'Télécharger le pilote'
                Add-Finding $F 'warn' "Nouveau pilote NVIDIA disponible ($($nvLatest.Version))" "Tu as la version $nvInstalled. La dernière version « Game Ready » est la $($nvLatest.Version)." $w -Id "gpu-driver:$($g.Name)" -Fix (New-Fix `
                    -Why 'Chaque pilote « Game Ready » apporte des optimisations pour les jeux récents et corrige des bugs (plantages, textures qui clignotent...).' `
                    -Steps @('Ouvre l''application NVIDIA si tu l''as (onglet Pilotes) et clique sur Télécharger, ou clique sur « Page du pilote ».', 'Lance l''installation (installation rapide). L''écran peut clignoter, c''est normal.', 'Relance l''analyse de Nexo.') `
                    -Open $nvLatest.Url -OpenLabel 'Page du pilote')
            } else {
                Add-Finding $F 'ok' "Pilote graphique à jour ($($g.Name))" "Tu as le dernier pilote NVIDIA « Game Ready » ($nvInstalled)." $w -Id "gpu-driver:$($g.Name)"
            }
        } elseif ($g.DriverDate) {
            $age = ((Get-Date) - $g.DriverDate).Days
            $c.Lines['Pilote'] = @("$($g.DriverVersion) du $($g.DriverDate.ToString('dd/MM/yyyy'))", $(if ($age -gt 180) { $Colors.warn } else { '#E6E8EE' }))
            if ($age -gt 180) {
                $months = [math]::Floor($age / 30)
                Add-Note $c 'warn' "Pilote vieux d'environ $months mois: mets le à jour pour de meilleures performances."
                $c.Action = Get-DriverLink $g.Name; $c.ActionLabel = 'Télécharger le pilote'
                $steps = if ($g.Name -match 'NVIDIA|GeForce') {
                    @('Ouvre l''application NVIDIA si tu l''as (onglet Pilotes), ou clique sur « Site du pilote ».', 'Télécharge le dernier pilote « Game Ready » pour ta carte.', 'Lance l''installation (installation rapide). L''écran peut clignoter, c''est normal.', 'Relance l''analyse de Nexo.')
                } elseif ($g.Name -match 'AMD|Radeon') {
                    @('Ouvre AMD Software (clic droit sur le bureau) et va dans « Pilotes et logiciels », ou clique sur « Site du pilote ».', 'Installe la dernière version recommandée.', 'Redémarre si l''installation le demande, puis relance l''analyse.')
                } else {
                    @('Clique sur « Site du pilote » et laisse l''assistant détecter ta carte.', 'Installe le pilote proposé.', 'Redémarre si besoin, puis relance l''analyse.')
                }
                Add-Finding $F 'warn' "Pilote graphique ancien ($($g.Name))" "Ton pilote date d'il y a environ $months mois (version $($g.DriverVersion))." $w -Id "gpu-driver:$($g.Name)" -Fix (New-Fix `
                    -Why 'Chaque nouveau pilote apporte des optimisations pour les jeux récents et corrige des bugs (plantages, textures qui clignotent...).' `
                    -Steps $steps -Open (Get-DriverLink $g.Name) -OpenLabel 'Site du pilote')
            } else {
                Add-Finding $F 'ok' "Pilote graphique à jour ($($g.Name))" "Pilote de moins de 6 mois (version $($g.DriverVersion))." $w -Id "gpu-driver:$($g.Name)"
            }
        }
        if ($g.Name -match 'NVIDIA|GeForce') {
            $s = Invoke-Smi 'temperature.gpu,fan.speed,power.draw,power.limit,pcie.link.width.current,pcie.link.width.max'
            if ($s) {
                if ($null -ne $s['temperature.gpu']) {
                    $t = [int]$s['temperature.gpu']
                    $c.Lines['Température'] = @("$t °C", (Get-LoadColor $t 80 87))
                    if ($t -ge 80) { Add-Note $c 'warn' "Carte graphique chaude ($t °C) alors que le PC ne joue pas forcément: dépoussière le PC et vérifie les ventilateurs." }
                }
                if ($null -ne $s['fan.speed']) { $c.Lines['Ventilateurs'] = "$([int]$s['fan.speed']) %" }
                if ($null -ne $s['power.draw']) {
                    $c.Lines['Consommation'] = ('{0:N0} W' -f $s['power.draw']) + $(if ($s['power.limit']) { ' sur {0:N0} W max' -f $s['power.limit'] } else { '' })
                }
                if ($s['pcie.link.width.current'] -and $s['pcie.link.width.max']) {
                    $wc = [int]$s['pcie.link.width.current']; $wm = [int]$s['pcie.link.width.max']
                    $c.Lines['Liaison PCIe'] = @("x$wc (max x$wm)", $(if ($wc -lt $wm) { $Colors.warn } else { '#E6E8EE' }))
                    if (-not $script:IsLaptop -and $wc -lt $wm -and $wm -ge 16) {
                        Add-Note $c 'warn' "La carte fonctionne en x$wc au lieu de x${wm}: vérifie qu'elle est branchée sur le premier port PCIe (le plus proche du processeur)."
                        Add-Finding $F 'warn' "Carte graphique en PCIe x$wc" "Elle devrait être en x$wm. Souvent elle est branchée sur le mauvais port de la carte mère: tu peux perdre des FPS." 2 -Id 'gpu-pcie' -Fix (New-Fix `
                            -Why 'Avec moins de lignes PCIe, la carte graphique reçoit ses données moins vite, surtout quand sa mémoire vidéo est pleine.' `
                            -Steps @('Éteins le PC et débranche le câble d''alimentation.', 'Ouvre le boîtier: la carte graphique doit être sur le port PCIe le plus haut (le plus proche du processeur), souvent renforcé en métal.', 'Vérifie qu''elle est bien enfoncée et que le clip de verrouillage est fermé.', 'Si elle est déjà sur le bon port, un SSD branché sur certains emplacements M.2 peut partager ses lignes: regarde le manuel de ta carte mère.'))
                    }
                }
            }
        }
        [void]$cards.Add($c)
    }

    # --- Portable à deux cartes graphiques: les jeux doivent utiliser la carte dédiée
    $dedicated = @($gpus | Where-Object { $_.Name -match $dedicatedPattern } | Select-Object -First 1)
    $integrated = @($gpus | Where-Object { $_.Name -notmatch $dedicatedPattern -and $_.Name -notmatch 'Microsoft Basic' })
    if ($script:IsLaptop -and $dedicated.Count -and $integrated.Count) {
        Set-Status 'Recherche de tes jeux...'
        $dgpu = $dedicated[0].Name
        $games = @(Invoke-Async ([scriptblock]::Create("function Get-InstalledGames {${function:Get-InstalledGames}}; Get-InstalledGames")) | Where-Object { -not $_.Leftover })
        $todo = @($games | Where-Object { $g = $_; @($g.Exes | Where-Object { (Get-GpuPreference $_) -notmatch 'GpuPreference=2' }).Count })
        if (-not $games.Count) {
            Add-Finding $F 'info' 'Jeux sur la carte graphique dédiée' "Aucun jeu trouvé. Pour tes autres jeux, choisis « Hautes performances » dans Paramètres > Écran > Graphiques." 0 -Id 'hybrid-gpu' -Fix (New-Fix `
                -Why "Ton portable a deux cartes graphiques. Windows peut lancer un jeu sur la puce intégrée, beaucoup moins puissante que ta $dgpu." `
                -Steps @('Ouvre Paramètres > Système > Écran > Graphiques.', 'Ajoute ton jeu (bouton « Parcourir ») s''il n''est pas dans la liste.', 'Clique dessus, puis « Options » et choisis « Hautes performances ».') `
                -Open 'ms-settings:display-advancedgraphics' -OpenLabel 'Paramètres graphiques')
        } elseif ($todo.Count) {
            $names = @($todo | ForEach-Object { $_.Name })
            $list = if ($names.Count -gt 6) { (($names | Select-Object -First 6) -join ', ') + " et $($names.Count - 6) autre$(if ($names.Count -gt 7) {'s'})" } else { $names -join ', ' }
            Add-Finding $F 'warn' "$($todo.Count) jeu$(if ($todo.Count -gt 1) {'x'}) sans carte graphique imposée" "Windows peut lancer ces jeux sur la puce intégrée au lieu de ta $dgpu : $list." 2 -Id 'hybrid-gpu' -Fix (New-Fix -Auto `
                -What @("Régler $($todo.Count) jeu$(if ($todo.Count -gt 1) {'x'}) Steam / Epic sur « Hautes performances » pour qu'ils utilisent toujours la $dgpu : $list.", 'C''est le même réglage que Paramètres > Écran > Graphiques, en une seule fois.') `
                -Why "Ton portable a deux cartes graphiques. Sur la puce intégrée, un jeu peut tourner 3 à 5 fois moins vite. Pense aussi à brancher le chargeur: sur batterie, la carte dédiée est bridée." `
                -Run { param($a) foreach ($g in $a.Games) { foreach ($e in $g.Exes) { if ((Get-GpuPreference $e) -notmatch 'GpuPreference=2') { Set-Reg $DxPath $e 'GpuPreference=2;' 'String' } } } } `
                -RunArgs @{ Games = $todo } -Open 'ms-settings:display-advancedgraphics' -OpenLabel 'Paramètres graphiques')
        } else {
            Add-Finding $F 'ok' 'Jeux sur la carte graphique dédiée' "Tes $($games.Count) jeux Steam / Epic utilisent la $dgpu." 2 -Id 'hybrid-gpu'
        }
    }

    # --- Écrans
    $displays = @()
    try { $displays = @([OGNative]::GetDisplays()) } catch { Write-Log "Écrans: $_" }
    if ($displays.Count) {
        $c = New-Component 'HZ' 'Écrans' "$($displays.Count) écran$(if ($displays.Count -gt 1) {'s'}) connecté$(if ($displays.Count -gt 1) {'s'})"
        $dispTexts = @()
        $i = 0
        foreach ($d in $displays) {
            $i++
            $p = $d -split '\|'
            $w = [int]$p[2]; $h = [int]$p[3]; $cur = [int]$p[4]; $max = [int]$p[5]
            $dispTexts += "${w}x$h à $cur Hz"
            $dev = $p[0]
            $dispId = "display:$dev"
            # Seul l'écran principal (celui où l'on joue) compte dans la note.
            $primary = ($displays.Count -eq 1) -or ($p.Count -gt 6 -and $p[6] -eq '1')
            $role = if ($displays.Count -gt 1) { if ($primary) { ' (principal)' } else { ' (secondaire)' } } else { '' }
            $bridled = $max -gt 60 -and ($max - $cur) -ge 5
            $ignored = $bridled -and ($script:Ignored -contains $dispId)
            $sev = if ($primary) { 'bad' } else { 'warn' }
            $lineColor = if ($ignored) { '#9AA3B2' } elseif ($bridled) { $Colors[$sev] } else { '#E6E8EE' }
            $lineText = "${w}x$h à $cur Hz" + $(if ($ignored) { ' (volontaire)' } elseif ($bridled) { " (peut faire $max Hz)" } else { '' })
            $c.Lines["Écran $i$role"] = @($lineText, $lineColor)
            if ($bridled) {
                if (-not $ignored) {
                    $noteTxt = "L'écran $i tourne à $cur Hz au lieu de $max Hz." + $(if (-not $primary) { " C'est un écran secondaire : ça ne compte pas dans la note." } else { '' })
                    Add-Note $c $sev $noteTxt
                }
                $detail = "Ton écran $i ($w x $h) peut monter à $max Hz mais Windows l'utilise à $cur Hz." +
                    $(if (-not $primary) { " Écran secondaire : simple avertissement, il ne fait pas baisser ta note." } else { '' })
                Add-Finding $F $sev "Écran $i$role bridé à $cur Hz" $detail $(if ($primary) { 3 } else { 0 }) -Id $dispId -Fix (New-Fix -Auto `
                    -What @("Passer l'écran $i ($w x $h) de $cur Hz à $max Hz, sans changer sa résolution.") `
                    -Why "Plus de Hz, c'est une image plus fluide et moins de latence: c'est le réglage qui se voit le plus en jeu. Si tu utilises cet écran seulement pour des vidéos, clique sur « Ignorer (c'est voulu) »: il ne comptera plus dans ton score. Si l'écran devient noir après le changement, ne touche à rien: l'app revient toute seule à l'ancien réglage au bout de 15 secondes." `
                    -Run { param($a) Set-DisplayRate $a.Device $a.Hz } -RunArgs @{ Device = $dev; Hz = $max } `
                    -Open 'ms-settings:display-advanced' -OpenLabel 'Affichage avancé')
            } elseif ($cur -gt 60) {
                Add-Finding $F 'ok' "Écran $i$role à $cur Hz" 'Ton écran tourne à sa fréquence maximale.' $(if ($primary) { 3 } else { 0 }) -Id $dispId
            } else {
                Add-Finding $F 'info' "Écran $i$role en 60 Hz" "Ton écran est à sa fréquence maximale (60 Hz). Un écran 144 Hz est l'une des meilleures améliorations pour jouer." 0 -Id $dispId
            }
        }
        $info['Écran'] = $dispTexts -join ' + '
        if ($c.Status -ne 'ok') { $c.Action = 'ms-settings:display-advanced'; $c.ActionLabel = 'Affichage avancé' }
        [void]$cards.Add($c)
    }
    Update-UI

    # --- Mémoire vive
    Set-Status 'Analyse de la mémoire...'
    $mem = @($script:AnalysisData.Mem)
    if ($mem.Count) {
        $first = $mem[0]
        $totalGB = [math]::Round((($mem | Measure-Object Capacity -Sum).Sum) / 1GB)
        $speed = if ($first.ConfiguredClockSpeed) { [int]$first.ConfiguredClockSpeed } else { [int]$first.Speed }
        $memType = switch ([int]$first.SMBIOSMemoryType) { 24 { 'DDR3' } 26 { 'DDR4' } 30 { 'LPDDR4' } 34 { 'DDR5' } 35 { 'LPDDR5' } default { '' } }
        $info['Mémoire'] = "$totalGB Go $memType à $speed MT/s ($($mem.Count) barrette$(if ($mem.Count -gt 1) {'s'}))"

        $c = New-Component 'RAM' 'Mémoire vive' "$totalGB Go $memType à $speed MT/s"
        $totB = [double]$os.TotalVisibleMemorySize * 1KB
        $usedB = $totB - [double]$os.FreePhysicalMemory * 1KB
        $usedPct = 100 * $usedB / $totB
        [void]$c.Bars.Add(@{ Label = 'Utilisée en ce moment'; Value = $usedPct; Text = "$(Format-Size $usedB) sur $(Format-Size $totB)"; Color = (Get-LoadColor $usedPct 80 90) })
        if ($usedPct -ge 85) { Add-Note $c 'warn' 'Mémoire presque pleine en ce moment: ferme ton navigateur et les programmes inutiles avant de jouer.' }
        $c.Lines['Mode'] = if ($mem.Count -ge 2) { @('Double canal', $Colors.ok) } else { @('Simple canal', $Colors.warn) }
        $n = 0
        foreach ($m in $mem) {
            $n++
            $brand = ([string]$m.Manufacturer).Trim()
            if ($brand -match '^(Unknown|Undefined|0+)$') { $brand = '' }
            $c.Lines["Barrette $n"] = (@("$([math]::Round($m.Capacity / 1GB)) Go", $brand, ([string]$m.PartNumber).Trim()) | Where-Object { $_ }) -join ' '
        }
        $md = $data.MemDiag
        if ($md) {
            $when = $md.TimeCreated.ToString('dd/MM/yyyy')
            if ($md.Id -in 1101, 1201) { $c.Lines['Test mémoire Windows'] = @("Aucune erreur ($when)", $Colors.ok) }
            elseif ($md.Id -in 1102, 1202) {
                $c.Lines['Test mémoire Windows'] = @("Erreurs détectées ($when)", $Colors.bad)
                Add-Note $c 'bad' "Le test mémoire de Windows a trouvé des erreurs. Désactive l'XMP / EXPO pour tester, et si ça continue, une barrette est défectueuse."
                Add-Finding $F 'bad' 'Mémoire défectueuse ou instable' "Le dernier test mémoire de Windows a trouvé des erreurs: plantages et écrans bleus possibles." 3 -Id 'ram-errors' -Fix (New-Fix `
                    -Why 'Une mémoire qui fait des erreurs provoque plantages, écrans bleus et fichiers corrompus.' `
                    -Steps @('Entre dans le BIOS et remets la mémoire en réglage par défaut (désactive XMP / EXPO).', 'Relance le test mémoire de Windows avec le bouton ci dessous.', 'Si les erreurs continuent, teste les barrettes une par une: celle qui provoque des erreurs est défectueuse.') `
                    -What @('Lancer le test mémoire de Windows (le PC redémarre, le test dure environ 15 minutes).') `
                    -Run { Start-Process 'mdsched.exe' } -RunLabel 'Lancer le test mémoire' -NoRescan -Done 'Le test mémoire de Windows est lancé: choisis « Redémarrer maintenant ».')
            }
        } else {
            $c.Lines['Test mémoire Windows'] = @('Jamais lancé', $Muted)
        }
        $c.Action = 'run:mdsched.exe'; $c.ActionLabel = 'Tester la mémoire (redémarre le PC)'
        [void]$cards.Add($c)

        $ramFix = New-Fix `
            -Why 'Quand la mémoire est pleine, Windows utilise le disque à la place: grosses saccades garanties.' `
            -Steps @('En attendant: ferme ton navigateur, Discord et les programmes inutiles avant de jouer.', 'Regarde combien d''emplacements libres a ta carte mère (ou si ton portable accepte plus de mémoire).', 'Achète un kit identique à ta mémoire actuelle (même type et même vitesse), ou un kit de 2 x 8 Go / 2 x 16 Go.')
        if ($totalGB -lt 8)      { Add-Finding $F 'bad'  "Seulement $totalGB Go de mémoire" "Trop peu pour les jeux actuels: 16 Go est le minimum conseillé aujourd'hui." 2 -Id 'ram-amount' -Fix $ramFix }
        elseif ($totalGB -lt 16) { Add-Finding $F 'warn' "$totalGB Go de mémoire" 'Ça passe, mais beaucoup de jeux récents conseillent 16 Go.' 2 -Id 'ram-amount' -Fix $ramFix }
        else                     { Add-Finding $F 'ok'   "$totalGB Go de mémoire" 'Suffisant pour les jeux actuels.' 2 -Id 'ram-amount' }

        $channelFix = New-Fix `
            -Why 'En double canal, la mémoire a deux fois plus de débit. Les jeux qui dépendent du processeur y gagnent beaucoup.' `
            -Steps @('Ajoute une 2e barrette identique (même capacité, même vitesse), ou remplace par un kit de 2 barrettes.', 'Sur une carte mère à 4 emplacements, mets les 2 barrettes sur les ports A2 et B2 (en général le 2e et le 4e en partant du processeur, voir le manuel).', 'Relance l''analyse: la ligne « Mode » doit indiquer « Double canal ».')
        if ($mem.Count -eq 1) {
            if ($script:IsLaptop) {
                Add-Finding $F 'warn' 'Une seule barrette de mémoire' "La mémoire fonctionne sans doute en simple canal, ce qui peut coûter beaucoup de FPS (surtout avec une puce graphique intégrée). Vérifie si ton portable accepte une 2e barrette." 1 -Id 'ram-channel' -Fix $channelFix
            } else {
                Add-Finding $F 'bad' 'Une seule barrette de mémoire' "La mémoire fonctionne en simple canal: jusqu'à 10 à 30 % de FPS en moins dans certains jeux. Ajoute une 2e barrette identique (ou prends un kit de 2)." 3 -Id 'ram-channel' -Fix $channelFix
            }
        } else {
            Add-Finding $F 'ok' 'Mémoire en double canal' "$($mem.Count) barrettes installées." 2 -Id 'ram-channel'
        }

        if (-not $script:IsLaptop -and $memType -in 'DDR4', 'DDR5') {
            $low = ($memType -eq 'DDR4' -and $speed -le 2666) -or ($memType -eq 'DDR5' -and $speed -le 4800)
            if ($low) {
                Add-Finding $F 'warn' "Mémoire à $speed MT/s: profil XMP / EXPO à vérifier" "La plupart des barrettes gaming sont vendues pour une vitesse plus élevée, mais il faut activer le profil XMP (Intel) ou EXPO / DOCP (AMD) dans le BIOS. Si tes barrettes sont vendues pour $speed MT/s, tout va bien." 2 -Id 'ram-xmp' -Fix (New-Fix `
                    -Why 'Sans profil XMP / EXPO, la mémoire tourne à une vitesse de sécurité: souvent 10 à 20 % de FPS minimum en moins dans les jeux gourmands en processeur.' `
                    -Steps @('Clique sur « Redémarrer dans le BIOS » (enregistre ton travail avant).', 'Dans le BIOS, cherche XMP (Intel), EXPO ou DOCP (AMD). Souvent dans le menu Ai Tweaker, OC ou Extreme Tweaker.', 'Choisis le Profil 1, puis enregistre et quitte avec F10.', 'Si le PC devient instable, retourne dans le BIOS et remets ce réglage sur Auto.') `
                    -What @('Redémarrer le PC directement dans le BIOS, pour que tu n''aies pas à chercher la bonne touche au démarrage.') `
                    -Run $BiosRun -RunLabel 'Redémarrer dans le BIOS' -Confirm $BiosConfirm -NoRescan -Done $BiosDone)
            } else {
                Add-Finding $F 'ok' "Mémoire à $speed MT/s" 'Le profil XMP / EXPO semble actif.' 2 -Id 'ram-xmp'
            }
        }
    }
    Update-UI

    # --- Disques
    Set-Status 'Analyse des disques...'
    $sysDisk = $null
    $sysDisk = [string]$data.SysDisk
    foreach ($dd in @($data.Disks)) {
        $d = $dd.Disk
        $media = [string]$d.MediaType; $bus = [string]$d.BusType
        $kind = if ($bus -eq 'NVMe') { 'SSD NVMe' } elseif ($media -eq 'SSD') { 'SSD' } elseif ($media -eq 'HDD') { 'Disque dur' } else { 'Disque' }
        $tag = if ($bus -eq 'USB') { 'USB' } elseif ($media -eq 'HDD') { 'HDD' } else { 'SSD' }
        $isSys = [string]$d.DeviceId -eq $sysDisk
        $sous = "$kind, $(Format-Size $d.Size)" + $(if ($bus -eq 'USB') { ', externe' } else { '' }) + $(if ($isSys) { ', Windows' } else { '' })
        $c = New-Component $tag (([string]$d.FriendlyName).Trim()) $sous

        $parts = @()
        $parts = @($dd.Vols)
        foreach ($pt in $parts) {
            $v = $pt.Vol
            if (-not $v -or -not $v.Size) { continue }
            $pct = 100 * ($v.Size - $v.SizeRemaining) / $v.Size
            $label = "Lecteur $($pt.DriveLetter):" + $(if ($v.FileSystemLabel) { " $($v.FileSystemLabel)" } else { '' })
            [void]$c.Bars.Add(@{ Label = $label; Value = $pct; Text = "$(Format-Size $v.SizeRemaining) libres sur $(Format-Size $v.Size)"; Color = (Get-LoadColor $pct 80 90) })
            if ($pct -ge 90) { Add-Note $c 'warn' "Le lecteur $($pt.DriveLetter): est presque plein." }
        }

        switch ([string]$d.HealthStatus) {
            'Healthy'   { $c.Lines['État SMART'] = @('Bon', $Colors.ok) }
            'Warning'   { $c.Lines['État SMART'] = @('Avertissement', $Colors.warn); Add-Note $c 'warn' 'Le disque signale un problème: sauvegarde tes fichiers importants.' }
            'Unhealthy' { $c.Lines['État SMART'] = @('Défaillant', $Colors.bad); Add-Note $c 'bad' 'Le disque est en train de lâcher: sauvegarde tes fichiers MAINTENANT et prévois de le remplacer.' }
            default     { $c.Lines['État SMART'] = @('Inconnu', $Muted) }
        }
        $rel = $null
        $rel = $dd.Rel
        if ($rel) {
            if ($null -ne $rel.Wear -and $tag -ne 'HDD') {
                $wear = [int]$rel.Wear
                $c.Lines['Usure'] = @("$wear %", (Get-LoadColor $wear 70 90))
                if ($wear -ge 90) { Add-Note $c 'bad' "SSD usé à $wear %: il approche de sa fin de vie, prévois son remplacement." }
                elseif ($wear -ge 70) { Add-Note $c 'warn' "SSD usé à $wear %: surveille le et sauvegarde tes fichiers." }
            }
            if ($rel.Temperature -gt 0) {
                $t = [int]$rel.Temperature
                $warnT = if ($tag -eq 'HDD') { 50 } else { 70 }
                $c.Lines['Température'] = @("$t °C", (Get-LoadColor $t $warnT ($warnT + 10)))
                if ($t -ge $warnT) { Add-Note $c 'warn' "Disque chaud ($t °C): vérifie la ventilation du boîtier (un dissipateur aide beaucoup sur un SSD NVMe)." }
            }
            if ($rel.PowerOnHours -gt 0) { $c.Lines["Heures d'utilisation"] = '{0:N0} h' -f $rel.PowerOnHours }
            if ($null -ne $rel.ReadErrorsUncorrected) {
                $re = [long]$rel.ReadErrorsUncorrected
                $c.Lines['Erreurs de lecture'] = @("$re", $(if ($re -gt 0) { $Colors.warn } else { $Colors.ok }))
                if ($re -gt 0) { Add-Note $c 'warn' "$re erreur(s) de lecture non corrigée(s): sauvegarde tes fichiers importants." }
            }
        } else {
            $c.Lines['Usure et température'] = @('Non fournies par ce disque', $Muted)
        }

        $diskFix = New-Fix `
            -Why (($c.Notes | ForEach-Object { $_.Text }) -join ' ') `
            -Steps @('Copie tes fichiers importants (photos, documents, sauvegardes de jeux) sur un autre disque ou dans le cloud.', 'Regarde le détail dans la carte du disque (usure, température, erreurs, remplissage).', 'Si le disque est défaillant ou très usé, remplace le rapidement: les jeux se réinstallent, tes fichiers perso non.')
        if ($c.Status -eq 'bad') {
            Add-Finding $F 'bad' "Disque en mauvaise santé: $($d.FriendlyName)" 'Sauvegarde tes fichiers importants dès maintenant.' 3 -Id "disk:$($d.FriendlyName)" -Fix $diskFix
        } elseif ($c.Status -eq 'warn') {
            Add-Finding $F 'warn' "Disque à surveiller: $($d.FriendlyName)" 'Détails dans la carte du disque, dans « Santé des composants ».' 2 -Id "disk:$($d.FriendlyName)" -Fix $diskFix
        }
        if ($isSys) {
            if ($media -eq 'HDD') {
                Add-Finding $F 'bad' 'Windows est sur un disque dur mécanique' "Démarrage lent, chargements très longs et saccades dans les jeux récents. Passer à un SSD est l'amélioration la plus visible possible." 3 -Id 'disk-hdd' -Fix (New-Fix `
                    -Why 'Un SSD est 5 à 50 fois plus rapide qu''un disque dur: Windows démarre en quelques secondes et les jeux chargent beaucoup plus vite.' `
                    -Steps @('Achète un SSD (un NVMe si ta carte mère a un emplacement M.2, sinon un SSD SATA 2,5 pouces).', 'Clone ton disque actuel vers le SSD avec le logiciel gratuit du fabricant (Samsung Magician, Crucial Acronis, WD...), ou réinstalle Windows dessus.', 'Garde l''ancien disque dur pour stocker tes fichiers.'))
            } elseif ($kind -like 'SSD*') {
                Add-Finding $F 'ok' "Windows est sur un $kind" 'Chargements rapides.' 3 -Id 'disk-hdd'
            }
            $info['Disque système'] = "$kind $($d.FriendlyName)"
        }
        [void]$cards.Add($c)
    }
    $ld = $data.LogicalC
    if ($ld -and $ld.Size) {
        $freeGB = [math]::Round($ld.FreeSpace / 1GB)
        $pct = [math]::Round(100 * $ld.FreeSpace / $ld.Size)
        $spaceFix = New-Fix -Auto `
            -What @('Supprimer les fichiers temporaires de Windows et de ton compte.', 'Supprimer les fichiers de mises à jour Windows déjà installées et les anciens rapports d''erreurs.', 'Tes documents, tes jeux et la corbeille ne sont pas touchés.') `
            -Why 'Windows et les jeux ralentissent quand le disque système est plein, et les mises à jour de jeux peuvent échouer. Si ça ne suffit pas, désinstalle les jeux auxquels tu ne joues plus.' `
            -Run { Invoke-CleanAll } -Open 'tab:4' -OpenLabel 'Onglet Nettoyage'
        if ($pct -lt 10)     { Add-Finding $F 'bad'  "Disque presque plein ($freeGB Go libres)" 'Windows et les jeux ralentissent quand le disque est plein.' 2 -Id 'disk-space' -Fix $spaceFix }
        elseif ($pct -lt 20) { Add-Finding $F 'warn' "Espace disque limité ($freeGB Go libres)" 'Garde au moins 20 % de libre pour de bonnes performances.' 2 -Id 'disk-space' -Fix $spaceFix }
        else                 { Add-Finding $F 'ok'   "Espace disque suffisant ($freeGB Go libres)" '' 2 -Id 'disk-space' }
    }
    Update-UI

    # --- Carte mère et BIOS
    Set-Status 'Analyse de la carte mère...'
    $bb = $data.BaseBoard
    $bios = $data.BIOS
    $c = New-Component 'BIOS' 'Carte mère et BIOS' ("$($bb.Manufacturer) $($bb.Product)".Trim())
    if ($bios) {
        $c.Lines['Version du BIOS'] = [string]$bios.SMBIOSBIOSVersion
        if ($bios.ReleaseDate) {
            $years = [math]::Floor(((Get-Date) - $bios.ReleaseDate).TotalDays / 365)
            $ageTxt = if ($years -ge 1) { " (il y a $years an$(if ($years -gt 1) {'s'}))" } else { '' }
            $c.Lines['Date du BIOS'] = @("$($bios.ReleaseDate.ToString('dd/MM/yyyy'))$ageTxt", $(if ($years -ge 3) { $Colors.warn } else { '#E6E8EE' }))
            if ($years -ge 3 -and -not $script:IsLaptop) {
                Add-Note $c 'info' "BIOS de plus de 3 ans: une mise à jour depuis le site du fabricant peut améliorer la stabilité et la compatibilité mémoire. À faire avec prudence (ne jamais couper le courant pendant la mise à jour)."
            }
        }
    }
    $c.Lines['Démarrage'] = if ($env:firmware_type -eq 'UEFI') { 'UEFI' } else { @('Legacy (ancien BIOS)', $Colors.warn) }
    try {
        if ($null -eq $data.SecureBoot) { throw 'indisponible' }
        $sb = [bool]$data.SecureBoot
        if ($sb) { $c.Lines['Secure Boot'] = @('Activé', $Colors.ok) }
        else {
            $c.Lines['Secure Boot'] = @('Désactivé', $Colors.warn)
            Add-Note $c 'warn' "Certains jeux récents (Valorant, Battlefield 6, Call of Duty) exigent le Secure Boot pour leur anti triche. Active le dans le BIOS si un jeu refuse de se lancer."
            Add-Finding $F 'warn' 'Secure Boot désactivé' "Des jeux comme Valorant, Battlefield 6 ou les derniers Call of Duty peuvent refuser de se lancer." 1 -Id 'secureboot' -Fix (New-Fix `
                -Why 'Les anti triche récents vérifient que le Secure Boot est actif. Sans lui, certains jeux refusent de démarrer.' `
                -Steps @('Clique sur « Redémarrer dans le BIOS » (enregistre ton travail avant).', 'Va dans le menu Boot (ou Sécurité) > Secure Boot et mets le sur « Enabled ». Sur certaines cartes, il faut d''abord mettre « OS Type » sur « Windows UEFI mode ».', 'Enregistre avec F10 et redémarre.') `
                -What @('Redémarrer le PC directement dans le BIOS.') `
                -Run $BiosRun -RunLabel 'Redémarrer dans le BIOS' -Confirm $BiosConfirm -NoRescan -Done $BiosDone)
        }
    } catch { $c.Lines['Secure Boot'] = @('Non disponible', $Muted) }
    try {
        if (-not $data.TpmOk) { throw 'indisponible' }
        $tpm = $data.Tpm
        if ($tpm) { $c.Lines['Puce TPM'] = @("Version $(([string]$tpm.SpecVersion -split ',')[0].Trim())", $Colors.ok) }
        else { $c.Lines['Puce TPM'] = @('Non détectée', $Colors.warn) }
    } catch {}
    [void]$cards.Add($c)

    # --- Stabilité
    Set-Status 'Lecture du journal des erreurs...'
    $bsod = [int]$data.Bsod
    $crash = [int]$data.Crash
    $wheaErr = [int]$data.WheaErr
    $wheaWarn = [int]$data.WheaWarn
    $uptime = (Get-Date) - $os.LastBootUpTime
    $c = New-Component 'SYS' 'Stabilité du système' 'Sur les 30 derniers jours'
    $c.Lines['Écrans bleus'] = @("$bsod", $(if ($bsod -ge 2) { $Colors.bad } elseif ($bsod) { $Colors.warn } else { $Colors.ok }))
    $c.Lines['Arrêts brutaux'] = @("$crash", $(if ($crash) { $Colors.warn } else { $Colors.ok }))
    $c.Lines['Erreurs matérielles'] = @("$wheaErr", $(if ($wheaErr) { $Colors.bad } else { $Colors.ok }))
    $c.Lines['Erreurs corrigées'] = @("$wheaWarn", $(if ($wheaWarn -ge 20) { $Colors.warn } else { $Colors.ok }))
    $c.Lines['Allumé depuis'] = Format-Duration $uptime
    $crashFix = New-Fix `
        -Why 'Les plantages viennent le plus souvent d''un pilote défectueux, d''un réglage XMP / overclock trop agressif ou d''une surchauffe.' `
        -Steps @('Clique sur « Historique » pour voir quand le PC a planté et quel programme ou pilote est en cause.', 'Mets à jour Windows et le pilote de ta carte graphique.', 'Si tu as activé XMP / EXPO, un overclock ou un undervolt, remets les réglages par défaut dans le BIOS pour tester.', 'Surveille les températures en jeu avec HWiNFO: au delà de 90 °C, nettoie le PC ou améliore le refroidissement.') `
        -Open 'run:perfmon.exe /rel' -OpenLabel 'Historique'
    if ($bsod -ge 2) {
        Add-Note $c 'bad' "$bsod écrans bleus ce mois ci. Causes fréquentes: pilote défectueux, XMP / overclock instable, surchauffe."
        Add-Finding $F 'bad' "$bsod écrans bleus en 30 jours" 'Ton PC plante régulièrement.' 3 -Id 'bsod' -Fix $crashFix
    } elseif ($bsod -eq 1) {
        Add-Note $c 'warn' "Un écran bleu ce mois ci. Si ça se reproduit, regarde l'historique de fiabilité."
        Add-Finding $F 'warn' 'Un écran bleu en 30 jours' "Un plantage isolé n'est pas grave, mais surveille si ça se reproduit." 2 -Id 'bsod' -Fix $crashFix
    }
    if ($wheaErr) {
        Add-Note $c 'bad' "$wheaErr erreur(s) matérielle(s) grave(s). Souvent un overclock, un undervolt ou un profil XMP instable, parfois une surchauffe."
        Add-Finding $F 'bad' 'Erreurs matérielles détectées' "Windows a enregistré $wheaErr erreur(s) matérielle(s) grave(s) ce mois ci. Souvent un overclock / undervolt ou un profil XMP instable." 2 -Id 'whea' -Fix $crashFix
    } elseif ($wheaWarn -ge 20) {
        Add-Note $c 'warn' "$wheaWarn erreurs matérielles corrigées automatiquement. Pas critique, mais souvent signe d'un réglage limite (overclock, undervolt, XMP)."
    }
    if ($crash -gt $bsod) {
        Add-Note $c 'info' "Arrêts brutaux: coupure de courant, bouton d'alimentation maintenu, ou PC qui s'éteint seul (alimentation, surchauffe) si ça arrive en jeu."
    }
    if ($uptime.TotalDays -ge 7) {
        Add-Note $c 'info' "Pense à redémarrer de temps en temps: avec le démarrage rapide de Windows, « Arrêter » ne remet pas tout à zéro."
    }
    $c.Action = 'run:perfmon.exe /rel'; $c.ActionLabel = 'Historique de fiabilité'
    [void]$cards.Add($c)

    # --- Batterie
    if ($script:IsLaptop) {
        $b = $battery[0]
        $c = New-Component 'BAT' 'Batterie' ([string]$b.Name).Trim()
        $design = (Get-CimInstance -Namespace root\wmi -ClassName BatteryStaticData -ErrorAction SilentlyContinue | Select-Object -First 1).DesignedCapacity
        $full = (Get-CimInstance -Namespace root\wmi -ClassName BatteryFullChargedCapacity -ErrorAction SilentlyContinue | Select-Object -First 1).FullChargedCapacity
        if ($design -and $full) {
            $health = [math]::Min(100.0, 100 * $full / $design)
            [void]$c.Bars.Add(@{ Label = 'Santé de la batterie'; Value = $health; Text = "$([int]$health) %"; Color = $(if ($health -lt 60) { $Colors.bad } elseif ($health -lt 80) { $Colors.warn } else { $Colors.ok }) })
            $c.Lines["Capacité d'origine"] = '{0:N0} mWh' -f $design
            $c.Lines['Capacité actuelle'] = '{0:N0} mWh' -f $full
            if ($health -lt 60) { Add-Note $c 'bad' 'La batterie a perdu beaucoup de capacité: elle mérite d''être remplacée.' }
            elseif ($health -lt 80) { Add-Note $c 'warn' 'La batterie commence à s''user.' }
        }
        $cycles = (Get-CimInstance -Namespace root\wmi -ClassName BatteryCycleCount -ErrorAction SilentlyContinue | Select-Object -First 1).CycleCount
        if ($cycles) { $c.Lines['Cycles de charge'] = "$cycles" }
        if ($null -ne $b.EstimatedChargeRemaining) { $c.Lines['Charge'] = "$($b.EstimatedChargeRemaining) %$(if ($b.BatteryStatus -eq 2) { ', sur secteur' } else { ', sur batterie' })" }
        [void]$cards.Add($c)
    }

    # --- Onduleur
    Add-UpsCards $ups $hints $data $cards $F

    # --- Réseau
    Set-Status 'Analyse du réseau...'
    $script:Net = $data.Net
    if (-not $script:Net) { $script:Net = Get-ActiveNet }
    if ($script:Net) {
        $info['Réseau'] = "$(if ($script:Net.Wifi) { 'Wi-Fi' } else { 'Ethernet (câble)' }), $($script:Net.Speed)"
        $c = New-Component 'NET' 'Réseau' $script:Net.Desc
        $c.Lines['Connexion'] = if ($script:Net.Wifi) { @('Wi-Fi', $Colors.warn) } else { @('Câble Ethernet', $Colors.ok) }
        $c.Lines['Vitesse de la carte'] = [string]$script:Net.Speed
        $c.Action = 'tab:3'; $c.ActionLabel = 'Tester le ping'
        if ($script:Net.Wifi) {
            Add-Note $c 'info' 'Le Wi-Fi ajoute de la latence et des pertes de paquets. Pour jouer en ligne, le câble reste le plus stable.'
            Add-Finding $F 'warn' 'Connexion en Wi-Fi' 'Le Wi-Fi ajoute de la latence et des pertes de paquets, surtout loin de la box.' 1 -Id 'wifi' -Fix (New-Fix `
                -Why 'En Wi-Fi, le ping varie et des paquets se perdent: ça se traduit par du lag et des tirs qui ne comptent pas.' `
                -Steps @('Si possible, branche un câble Ethernet entre le PC et la box (un long câble plat se cache facilement).', 'Sinon, un adaptateur CPL (réseau par les prises électriques) est une bonne alternative.', 'En Wi-Fi, connecte toi au réseau 5 GHz ou 6 GHz de ta box plutôt qu''au 2,4 GHz, et rapproche le PC de la box.') `
                -Open 'tab:3' -OpenLabel 'Tester le ping')
        } else {
            Add-Finding $F 'ok' 'Connexion par câble' 'La connexion la plus stable pour jouer en ligne.' 1 -Id 'wifi'
        }
        [void]$cards.Add($c)
    }
    $info['Windows'] = "$($os.Caption -replace 'Microsoft ', '') (build $($os.BuildNumber))"
    $info['Type'] = if ($script:IsLaptop) { 'PC portable' } else { 'PC fixe' }

    # --- Démarrage
    $startItems = @(Get-StartupItems | Where-Object { $_.Enabled })
    $enabled = $startItems.Count
    if ($enabled -gt 12) {
        $safe = @($startItems | Where-Object { $_.Nom -match $SafeStartup -or $_.Commande -match $SafeStartup })
        $startFix = if ($safe.Count) {
            New-Fix -Auto `
                -What @("Désactiver le lancement automatique de : $(($safe | ForEach-Object { $_.Nom }) -join ', ').", 'Ces programmes restent installés et se lancent normalement quand tu les ouvres.') `
                -Why 'Chaque programme au démarrage ralentit l''allumage du PC et occupe de la mémoire pendant tes parties. Pour les autres, choisis toi même dans l''onglet Démarrage.' `
                -Run { param($a) foreach ($i in $a.Items) { Set-StartupState $i $false } } -RunArgs @{ Items = $safe } `
                -Open 'tab:2' -OpenLabel 'Onglet Démarrage'
        } else {
            New-Fix -Why 'Chaque programme au démarrage ralentit l''allumage du PC et occupe de la mémoire pendant tes parties.' `
                -Steps @('Ouvre l''onglet Démarrage.', 'Désactive les programmes dont tu n''as pas besoin dès l''allumage (launchers, messageries, outils de mise à jour).', 'Garde l''antivirus et les pilotes (audio, carte graphique, souris).') `
                -Open 'tab:2' -OpenLabel 'Onglet Démarrage'
        }
        Add-Finding $F 'warn' "$enabled programmes se lancent au démarrage" "Ils ralentissent l'allumage du PC et occupent de la mémoire pendant tes parties." 1 -Id 'startup' -Fix $startFix
    } else {
        Add-Finding $F 'ok' "$enabled programmes au démarrage" 'Nombre raisonnable.' 1 'tab:2' 'Voir' -Id 'startup'
    }

    # --- Sécurité (information seulement)
    $hvci = Get-RegValue 'HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity' 'Enabled'
    if ($hvci -eq 1) {
        Add-Finding $F 'info' 'Intégrité de la mémoire activée' "Cette protection de Windows bloque certains logiciels malveillants mais peut coûter quelques pourcents de FPS. Microsoft conseille de la laisser activée: Nexo n'y touche pas, c'est à toi de décider." 0 -Id 'hvci' -Fix (New-Fix `
            -Why 'C''est un compromis entre sécurité et performances. Microsoft recommande de la laisser activée, et certains anti triche l''exigent.' `
            -Steps @('Si tu veux la désactiver: ouvre Sécurité Windows > Sécurité des appareils > Isolation du noyau.', 'Coupe « Intégrité de la mémoire » et redémarre.', 'Si un jeu ou un anti triche la réclame, réactive la au même endroit.') `
            -Open 'windowsdefender://coreisolation' -OpenLabel 'Isolation du noyau')
    }

    # --- Réglages gaming
    foreach ($t in (Get-AvailableTweaks)) {
        if (Test-Tweak $t) {
            Add-Finding $F 'ok' $t.Titre $t.Ok (Get-TweakWeight $t) -Id "tweak:$($t.Id)"
        } else {
            $st = if ($t.Recommended -eq $false) { 'info' } else { 'warn' }
            Add-Finding $F $st $t.Titre $t.Ko (Get-TweakWeight $t) -Id "tweak:$($t.Id)" -Fix (New-Fix -Auto `
                -What @($t.What) -Why $t.Description -Run $t.Apply -Reboot:([bool]$t.Reboot) -Restore -Open 'tab:1' -OpenLabel 'Onglet Gaming')
        }
    }

    $m = Measure-Score $F
    $score = $m.Score
    $order = @{ bad = 0; warn = 1; info = 2; ok = 3 }
    $sorted = @($F | Sort-Object @{ Expression = { $order[$_.Status] } }, @{ Expression = { -$_.Gain } }, @{ Expression = { -$_.Weight } })
    $active = @($sorted | Where-Object { $script:Ignored -notcontains $_.Id })
    $nbBad = @($active | Where-Object { $_.Status -eq 'bad' }).Count
    $nbWarn = @($active | Where-Object { $_.Status -eq 'warn' }).Count

    Show-HealthCards $cards
    Show-Improvements $active
    Show-Findings $sorted $active
    $s = Show-Score $score $nbBad $nbWarn $m.Potential
    $script:LastAnalysis = @{ Info = $info; Cards = $cards; Findings = $active; Active = $active; Score = $score; Potential = $m.Potential; Label = $s.Label; Color = $s.Color; Date = Get-Date }
    Set-Status "Analyse terminée: score de $score sur 100."
}

# ---------------------------------------------------------------------------
# Onduleur : infos, réglages de Windows (batterie faible / critique) et conseils
# ---------------------------------------------------------------------------
$SubBattery = 'e73a048d-bf27-4f12-9731-8b2076e8891f'
$BatSettings = [ordered]@{
    CritAction = '637ea02f-bbcb-4015-8e2c-a1c7b9c0b546'; CritLevel = '9a66d8d7-4ff7-4ef9-b5a2-5a326ca2a469'
    LowLevel = '8183ba9a-e910-48da-8769-14ae6dc1170a'; LowNotify = 'bcded951-187b-4d05-bccc-f7e51960c258'; LowAction = 'd8742dcb-3e6a-4b3c-b3fe-374623cdcf06'
}
$BatActions = @('ne rien faire', 'mise en veille', 'mise en veille prolongée', 'arrêt du PC')

# Réglages « batterie » du mode d'alimentation actif : @{ CritAction = @(secteur, batterie) ... }
function Get-BatteryPowerSettings {
    $r = @{}; $cur = $null
    foreach ($l in @(powercfg /q SCHEME_CURRENT $SubBattery 2>$null)) {
        if ($l -match '([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})') {
            $g = $Matches[1]; $cur = $null
            foreach ($k in $BatSettings.Keys) { if ($BatSettings[$k] -eq $g) { $cur = $k; $r[$k] = @($null, $null) } }
            continue
        }
        if (-not $cur) { continue }
        if ($l -match '(alternatif|\bAC\b).*:\s*0x([0-9a-fA-F]+)') { $r[$cur][0] = [Convert]::ToInt32($Matches[2], 16) }
        elseif ($l -match '(continu|\bDC\b).*:\s*0x([0-9a-fA-F]+)') { $r[$cur][1] = [Convert]::ToInt32($Matches[2], 16) }
    }
    $r
}

function Test-HibernateOn {
    $v = Get-RegValue 'HKLM:\SYSTEM\CurrentControlSet\Control\Power' 'HibernateEnabled'
    if ($null -ne $v -and $v -isnot [array]) { return [int]$v -eq 1 }
    Test-Path -LiteralPath "$env:SystemDrive\hiberfil.sys"
}

# Réglage « batterie » changé sur secteur et sur batterie, noté pour « Revenir en arrière » et « Tout restaurer »
function Set-BatterySetting([string]$Key, [int]$Value) {
    $g = $BatSettings[$Key]
    $cur = (Get-BatteryPowerSettings)[$Key]
    if ($cur) {
        if ($null -ne $script:RunLog) { [void]$script:RunLog.Add(@{ Type = 'pcfg'; Guid = $g; Ac = $cur[0]; Dc = $cur[1] }) }
        $orig = @(Get-Setting 'PcfgOriginal' @())
        if (-not @($orig | Where-Object { ([string]$_ -split '\|')[0] -eq $g }).Count) { Set-Setting 'PcfgOriginal' @($orig + "$g|$($cur[0])|$($cur[1])") }
    }
    powercfg /setacvalueindex SCHEME_CURRENT $SubBattery $g $Value | Out-Null
    powercfg /setdcvalueindex SCHEME_CURRENT $SubBattery $g $Value | Out-Null
    if ($LASTEXITCODE) { throw "Windows a refusé le réglage (code $LASTEXITCODE)." }
    powercfg /setactive SCHEME_CURRENT | Out-Null
}

# Ce qui ne va pas dans les réglages de l'onduleur : @{ Problems = @(...); Fix = @{ réglage = valeur } }
function Get-UpsConfigAdvice($P, [bool]$Hibernate) {
    $pr = @(); $fix = [ordered]@{}
    $act = if ($P.CritAction) { $P.CritAction[1] } else { $null }
    $crit = if ($P.CritLevel) { $P.CritLevel[1] } else { $null }
    $low = if ($P.LowLevel) { $P.LowLevel[1] } else { $null }
    $notify = if ($P.LowNotify) { $P.LowNotify[1] } else { $null }
    $okAct = $act -eq 3 -or ($act -eq 2 -and $Hibernate)
    if ($null -ne $act -and -not $okAct) {
        $pr += $(switch ($act) {
            0 { 'quand la batterie de l''onduleur sera presque vide, Windows ne fera rien : le PC s''éteindra d''un coup' }
            1 { 'quand la batterie sera presque vide, Windows mettra le PC en veille : la veille a besoin de courant, le PC s''éteindra d''un coup' }
            2 { 'Windows doit passer en veille prolongée, mais elle est désactivée sur ce PC : rien ne se passera' }
            default { 'l''action sur batterie critique est inconnue' }
        })
        $fix.CritAction = 3
    }
    if ($null -ne $crit -and $crit -lt 20) {
        $pr += "l'arrêt est prévu à $crit % : sur un onduleur, ça ne laisse que quelques secondes$(if ($act -eq 2) { ', trop peu pour écrire la mémoire sur le disque' })"
        $fix.CritLevel = 25
    }
    if ($null -ne $low -and $low -lt 40) { $pr += "l'alerte « batterie faible » arrive à $low %, trop tard pour enregistrer ta partie"; $fix.LowLevel = 50 }
    if ($null -ne $notify -and $notify -eq 0) { $pr += 'aucune alerte ne prévient quand la batterie baisse'; $fix.LowNotify = 1 }
    if ($fix.Count -and $null -ne $P.LowAction -and $P.LowAction[1] -ne 0) { $fix.LowAction = 0 }
    @{ Problems = $pr; Fix = $fix }
}

function Format-BatSetting($P, [string]$Level, [string]$Action, [string]$Notify) {
    $l = if ($P[$Level]) { "$($P[$Level][1]) %" } else { '?' }
    $a = if ($P[$Action] -and $null -ne $P[$Action][1]) { $BatActions[[int]$P[$Action][1]] } else { '?' }
    $n = if ($Notify -and $P[$Notify]) { if ($P[$Notify][1] -eq 1) { ', avec une alerte' } else { ', sans alerte' } } else { '' }
    "à $l : $a$n"
}

# Chimie de la batterie (code sur 4 lettres du pilote, ex : « PbAc »)
function Get-BatChemistry($Code, $Win32) {
    $t = ''
    if ($Code) { try { $t = [Text.Encoding]::ASCII.GetString([BitConverter]::GetBytes([uint32]$Code)).Trim([char]0).Trim() } catch {} }
    if ($t -match '(?i)^pb' -or [int]$Win32 -eq 3) { return 'Plomb (durée de vie 3 à 5 ans)' }
    if ($t -match '(?i)li') { return 'Lithium' }
    if ($t -match '(?i)ni') { return 'Nickel' }
    $t
}

function Add-UpsCards($Ups, $Hints, $Data, $Cards, $F) {
    $P = if ($null -ne $Data.BatPower) { $Data.BatPower } elseif (@($Ups).Count) { Get-BatteryPowerSettings } else { $null }
    $hib = if ($null -ne $Data.Hibernate) { [bool]$Data.Hibernate } else { Test-HibernateOn }
    $adv = if ($P -and $P.Count) { Get-UpsConfigAdvice $P $hib } else { $null }
    foreach ($u in $Ups) {
        $name = ([string]$u.Name).Trim()
        $w = $Data.BatWmi
        $c = New-Component 'UPS' 'Onduleur' $(if ($name) { $name } else { 'Onduleur' })
        $onBattery = [int]$u.BatteryStatus -eq 1
        $c.Lines['Alimentation'] = if ($onBattery) { @('Sur batterie (coupure de courant)', $Colors.bad) } else { @('Sur secteur', '#E6E8EE') }
        if ($w -and $w.Maker) { $c.Lines['Fabricant'] = $w.Maker }
        if ($w -and $w.Serial) { $c.Lines['Numéro de série'] = $w.Serial }
        $chem = Get-BatChemistry $(if ($w) { $w.Chem } else { $null }) $u.Chemistry
        if ($chem) { $c.Lines['Batterie'] = $chem }
        if ($null -ne $u.EstimatedChargeRemaining) {
            $charge = [int]$u.EstimatedChargeRemaining
            [void]$c.Bars.Add(@{ Label = 'Charge de la batterie'; Value = $charge; Text = "$charge %"; Color = $(if ($charge -lt 30) { $Colors.bad } elseif ($charge -lt 70) { $Colors.warn } else { $Colors.ok }) })
        }
        # Usure : capacité actuelle comparée à celle d'origine
        $health = $null
        if ($w -and $w.Design -gt 0 -and $w.Full -gt 0) {
            $health = [math]::Min(100, [math]::Round(100.0 * $w.Full / $w.Design))
            [void]$c.Bars.Add(@{ Label = 'Santé de la batterie'; Value = $health; Text = "$health %"; Color = $(if ($health -lt 40) { $Colors.bad } elseif ($health -lt 60) { $Colors.warn } else { $Colors.ok }) })
        }
        $run = if ($w -and $w.Runtime -gt 0 -and $w.Runtime -lt 360000) { [math]::Round($w.Runtime / 60) } else { [long]$u.EstimatedRunTime }
        if ($run -gt 0 -and $run -lt 10000) { $c.Lines['Autonomie estimée'] = "$run min" }
        if ($w -and $w.Volt -gt 0) { $c.Lines['Tension'] = '{0:N1} V' -f ($w.Volt / 1000) }
        if ($P -and $P.Count) {
            $c.Lines['Batterie faible'] = Format-BatSetting $P 'LowLevel' 'LowAction' 'LowNotify'
            $c.Lines['Batterie critique'] = @((Format-BatSetting $P 'CritLevel' 'CritAction' ''), $(if ($adv -and $adv.Fix.Contains('CritAction')) { $Colors.bad } else { '#E6E8EE' }))
        }
        if ($onBattery) {
            Add-Note $c 'bad' 'Coupure de courant : ton PC tourne sur l''onduleur. Enregistre ton travail et quitte ta partie.'
            Add-Finding $F 'bad' 'Coupure de courant' "Ton PC tourne sur l'onduleur ($name)$(if ($run -gt 0 -and $run -lt 10000) { ", environ $run minutes d'autonomie" })." 1 -Id 'ups-battery' -Fix (New-Fix `
                -Why 'Quand la batterie de l''onduleur sera vide, le PC s''éteindra d''un coup : les parties et documents non enregistrés seront perdus.' `
                -Steps @('Enregistre ce qui est ouvert et quitte ta partie.', 'Éteins le PC proprement si le courant ne revient pas vite.'))
        }
        # Réglages : arrêt propre avant que la batterie soit vide
        if ($adv -and $adv.Problems.Count) {
            $bad = $adv.Fix.Contains('CritAction')
            Add-Note $c $(if ($bad) { 'bad' } else { 'warn' }) "Réglages à revoir : $($adv.Problems[0])."
            $what = @()
            if ($adv.Fix.Contains('CritAction')) { $what += 'À batterie critique : arrêt propre du PC (tes fichiers sont enregistrés par Windows, rien n''est corrompu).' }
            if ($adv.Fix.Contains('CritLevel')) { $what += 'Arrêt à 25 % de batterie, pour qu''il ait le temps de se faire.' }
            if ($adv.Fix.Contains('LowLevel') -or $adv.Fix.Contains('LowNotify')) { $what += 'Alerte à 50 % de batterie, pour que tu aies le temps d''enregistrer et de quitter ta partie.' }
            Add-Finding $F $(if ($bad) { 'bad' } else { 'warn' }) 'Onduleur : arrêt automatique mal réglé' "$(($adv.Problems | ForEach-Object { $_.Substring(0, 1).ToUpper() + $_.Substring(1) }) -join '. ')." $(if ($bad) { 2 } else { 1 }) -Id 'ups-config' -Fix (New-Fix -Auto `
                -What $what `
                -Why 'Pendant une coupure, l''onduleur ne tient que quelques minutes. Si Windows ne s''éteint pas proprement avant, le PC se coupe d''un coup : partie perdue, fichiers en cours d''écriture abîmés.' `
                -Run { param($x) foreach ($k in $x.Keys) { Set-BatterySetting $k $x[$k] } } -RunArgs $adv.Fix `
                -Done 'L''onduleur éteindra le PC proprement avant d''être vide.')
        }
        # Usure et autonomie
        if ($null -ne $health -and $health -lt 60) {
            Add-Note $c $(if ($health -lt 40) { 'bad' } else { 'warn' }) "Batterie usée ($health % de sa capacité d'origine) : pense à la remplacer."
            Add-Finding $F $(if ($health -lt 40) { 'bad' } else { 'warn' }) 'Batterie de l''onduleur usée' "Elle ne garde plus que $health % de sa capacité d'origine : l'autonomie pendant une coupure est bien plus courte." 1 -Id 'ups-health' -Fix (New-Fix `
                -Why 'Les batteries au plomb des onduleurs s''usent en 3 à 5 ans, même sans coupure. Une batterie usée peut lâcher dès le début d''une coupure.' `
                -Steps @('Regarde la référence de la batterie sur l''étiquette de l''onduleur ou dans sa notice (chez APC par exemple : « RBC » suivi d''un numéro).', 'Une batterie de remplacement coûte bien moins cher qu''un onduleur neuf, et se change souvent sans outil.', 'Rapporte l''ancienne dans un point de collecte (magasin de bricolage, déchetterie).'))
        } elseif ($run -gt 0 -and $run -lt 5 -and -not $onBattery) {
            Add-Note $c 'warn' "Autonomie courte ($run min) : l'onduleur est très chargé ou sa batterie fatigue."
            Add-Finding $F 'warn' 'Onduleur : autonomie très courte' "Pendant une coupure, il ne tiendrait qu'environ $run minutes." 1 -Id 'ups-runtime' -Fix (New-Fix `
                -Why 'Plus on branche d''appareils sur l''onduleur, moins il tient. Un PC de jeu avec une grosse carte graphique consomme beaucoup.' `
                -Steps @('Ne branche sur les prises « batterie » de l''onduleur que le PC, l''écran et la box : pas l''imprimante, les enceintes ou un radiateur.', 'Si ça ne change rien, sa batterie est peut-être usée : regarde sa date d''achat (3 à 5 ans de vie).'))
        }
        if (-not $onBattery -and -not ($adv -and $adv.Problems.Count) -and -not ($null -ne $health -and $health -lt 60)) {
            Add-Note $c 'ok' 'Il protège ton PC des coupures et des surtensions, et Windows l''éteindra proprement avant qu''il soit vide.'
        }
        [void]$Cards.Add($c)
    }
    # Onduleur repéré (prise USB du fabricant, logiciel) mais que Windows ne lit pas comme une batterie
    if (-not @($Ups).Count -and @($Hints).Count) {
        $usb = @($Hints | Where-Object { $_.Kind -eq 'usb' })
        $soft = @($Hints | Where-Object { $_.Kind -ne 'usb' } | ForEach-Object { $_.Name } | Select-Object -Unique)
        $brand = @($usb | ForEach-Object { if ($UpsVendors[$_.Vid]) { $UpsVendors[$_.Vid] } } | Select-Object -First 1)[0]
        $title = if ($brand -and $brand -notmatch 'générique') { "Onduleur $brand" } elseif (@($usb | Where-Object { $_.Name -match '(?i)ups|onduleur' }).Count) { @($usb | Where-Object { $_.Name -match '(?i)ups|onduleur' })[0].Name } else { 'Onduleur' }
        $c = New-Component 'UPS' 'Onduleur' $title
        $found = @()
        if ($usb.Count) { $found += "câble USB$(if ($brand) { " ($brand)" })" }
        if ($soft.Count) { $found += "logiciel $(($soft | Select-Object -First 2) -join ', ')" }
        $c.Lines['Repéré grâce à'] = $found -join ', '
        $c.Lines['État de la batterie'] = @('Non transmis à Windows', '#9AA3B2')
        Add-Note $c 'ok' "$(if ($usb.Count) { 'Il est branché au PC, mais il ne donne pas son état à Windows' } else { "$($soft[0]) est installé : un onduleur est sûrement relié à ce PC, mais il ne donne pas son état à Windows" }) (charge, coupure de courant)."
        Add-Note $c 'warn' "$(if ($soft.Count) { "C'est $($soft[0]) qui doit éteindre le PC pendant une coupure : ouvre-le et vérifie que l'arrêt automatique est activé (vers 25 % de batterie)." } else { 'Pour que le PC s''éteigne proprement pendant une coupure, installe le logiciel de la marque de l''onduleur et active son arrêt automatique.' })"
        [void]$Cards.Add($c)
    }
}
