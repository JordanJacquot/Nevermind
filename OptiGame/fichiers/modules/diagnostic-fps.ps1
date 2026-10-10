# Nevermind : diagnostic des FPS d'une partie. D'où vient le problème, et comment le régler
# sans rendre le jeu moche (on commence toujours par ce qui coûte le moins en qualité).
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# Programmes du système qu'on ne propose jamais de fermer.
$SystemProcs = '^(System|Idle|Registry|Memory Compression|smss|csrss|wininit|winlogon|services|lsass|svchost|dwm|explorer|fontdrvhost|sihost|ctfmon|conhost|audiodg|spoolsv|SearchHost|StartMenuExperienceHost|ShellExperienceHost|RuntimeBroker|TextInputHost|dllhost|WmiPrvSE|MsMpEng|NisSrv|SecurityHealthService|SgrmBroker|powershell|Nevermind|PresentMon|nvcontainer|NVDisplay\.Container|amdfendrsr|atiesrxx|atieclxx|steam|steamwebhelper|EpicGamesLauncher|EasyAntiCheat.*|BEService|vgc|vgk)$'
$Browsers = '^(chrome|msedge|firefox|opera|brave|vivaldi)$'

# ---------------------------------------------------------------------------
# Relevés pendant la partie
# ---------------------------------------------------------------------------
# Toutes les 5 s (jeu au premier plan) : processeur, mémoire, carte graphique, disque, batterie.
function Add-FpsSysSample($T) {
    [void]$T.Sys.Add(@{
        Cpu = $Live.Cpu; Perf = $Live.CpuPerf
        Ram = $(if ($Live.RamTotal) { 100 * $Live.RamUsed / $Live.RamTotal } else { $null })
        Gpu = $Live.Gpu; Temp = $Live.GpuTemp
        Vram = $(if ($Live.VramTotal) { 100 * $Live.VramUsed / $Live.VramTotal } else { $null })
        Power = $(if ($Live.GpuPowerLimit) { 100 * $Live.GpuPower / $Live.GpuPowerLimit } else { $null })
        Disk = $Live.DiskBusy
    })
    try { if ([string][System.Windows.Forms.SystemInformation]::PowerStatus.PowerLineStatus -eq 'Offline') { $T.OnBattery = $true } } catch {}
}

# Lecture des programmes, dans un fil séparé (ouvrir chaque programme prend du temps).
$ProcSampleWork = {
    param($skip)
    @(foreach ($p in @(Get-Process -ErrorAction SilentlyContinue)) {
        if ($skip -contains $p.Id -or $p.Id -le 4) { continue }
        try { '{0}|{1}|{2}|{3}' -f $p.Id, $p.ProcessName, $p.TotalProcessorTime.TotalSeconds.ToString([Globalization.CultureInfo]::InvariantCulture), $p.WorkingSet64 } catch {}
    })
}

# Toutes les 10 s : récupère le relevé précédent et en lance un nouveau, sans attendre.
function Add-FpsProcSample($T) {
    if ($T.ProcJob) {
        if (-not $T.ProcJob.Handle.IsCompleted) { return }
        $lines = @()
        try { $lines = @($T.ProcJob.PS.EndInvoke($T.ProcJob.Handle)) } catch {} finally { $T.ProcJob.PS.Dispose(); $T.ProcJob = $null }
        Merge-FpsProcSample $T $lines $T.ProcJobTime
    }
    $ps = [PowerShell]::Create()
    $ps.RunspacePool = $script:BgPool
    [void]$ps.AddScript($ProcSampleWork.ToString()).AddArgument(@($T.Pid, $PID))
    $T.ProcJob = @{ PS = $ps; Handle = $ps.BeginInvoke() }
    $T.ProcJobTime = Get-Date
}

function Merge-FpsProcSample($T, $Lines, [datetime]$When) {
    $now = $When
    $cur = @{}
    foreach ($l in $Lines) {
        $x = ([string]$l) -split '\|'
        if ($x.Count -lt 4) { continue }
        $cur[[int]$x[0]] = @($x[1], [double]::Parse($x[2], [Globalization.CultureInfo]::InvariantCulture), [double]$x[3])
    }
    if ($T.PrevProc) {
        foreach ($id in @($cur.Keys)) {
            if (-not $T.PrevProc.ContainsKey($id)) { continue }
            $d = $cur[$id][1] - $T.PrevProc[$id][1]
            if ($d -le 0) { continue }
            $n = $cur[$id][0]
            if (-not $T.ProcCpu.ContainsKey($n)) { $T.ProcCpu[$n] = 0.0 }
            $T.ProcCpu[$n] += $d
        }
        $T.ProcSeconds += ($now - $T.PrevProcTime).TotalSeconds
    }
    $mem = @{}
    foreach ($v in $cur.Values) { if (-not $mem.ContainsKey($v[0])) { $mem[$v[0]] = [double]0 }; $mem[$v[0]] += $v[2] }
    foreach ($n in @($mem.Keys)) { if (-not $T.ProcMem.ContainsKey($n) -or $mem[$n] -gt $T.ProcMem[$n]) { $T.ProcMem[$n] = $mem[$n] } }
    $T.PrevProc = $cur
    $T.PrevProcTime = $now
}

# Résumé des relevés, enregistré avec la partie.
function Get-FpsDiagData($T, $Busy) {
    $sys = @($T.Sys)
    $stat = {
        param($k, $how)
        $v = @($sys | ForEach-Object { $_[$k] } | Where-Object { $null -ne $_ })
        if (-not $v.Count) { return $null }
        $m = $v | Measure-Object -Average -Maximum -Minimum
        [math]::Round($(switch ($how) { 'max' { $m.Maximum } 'min' { $m.Minimum } default { $m.Average } }), 1)
    }
    $cores = [Environment]::ProcessorCount
    $secs = [math]::Max(1.0, $T.ProcSeconds)
    $top = @($T.ProcCpu.GetEnumerator() | ForEach-Object { [pscustomobject]@{ Name = $_.Key; Pct = [math]::Round(100 * $_.Value / $secs / $cores, 1) } } |
        Where-Object { $_.Pct -ge 1 } | Sort-Object Pct -Descending | Select-Object -First 6)
    $topMem = @($T.ProcMem.GetEnumerator() | Sort-Object Value -Descending | Select-Object -First 6 | ForEach-Object { [pscustomobject]@{ Name = $_.Key; MB = [int]($_.Value / 1MB) } })
    $media = ''
    $drive = if ($T.Path -and $T.Path.Length -gt 1) { $T.Path.Substring(0, 1).ToUpper() } else { '' }
    foreach ($dd in @($script:AnalysisData.Disks)) {
        if ($drive -and @($dd.Letters) -contains $drive) {
            $media = if ([string]$dd.Disk.BusType -eq 'NVMe' -or [string]$dd.Disk.MediaType -eq 'SSD') { 'SSD' } elseif ([string]$dd.Disk.MediaType -eq 'HDD') { 'HDD' } else { [string]$dd.Disk.MediaType }
        }
    }
    $hz = 0; $hzMax = 0; $display = ''
    try {
        $prim = @([OGNative]::GetDisplays() | Where-Object { ($_ -split '\|')[6] -eq '1' })[0]
        if ($prim) { $x = $prim -split '\|'; $hz = [int]$x[4]; $hzMax = [int]$x[5]; $display = $x[0] }
    } catch {}
    $gpuNames = @($script:AnalysisData.GPUs | ForEach-Object { [string]$_.Name } | Where-Object { $_ -notmatch 'Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta|Microsoft Basic' })
    [pscustomobject]@{
        CpuRatio = [math]::Round($Busy[0], 3); GpuRatio = [math]::Round($Busy[1], 3); Stutters = [int]$Busy[2]
        Cpu = (& $stat 'Cpu' 'avg'); CpuMax = (& $stat 'Cpu' 'max'); Perf = (& $stat 'Perf' 'avg')
        Ram = (& $stat 'Ram' 'max'); Gpu = (& $stat 'Gpu' 'avg'); Temp = (& $stat 'Temp' 'max'); Vram = (& $stat 'Vram' 'max')
        Power = (& $stat 'Power' 'avg'); Disk = (& $stat 'Disk' 'max'); DiskAvg = (& $stat 'Disk' 'avg')
        OnBattery = [bool]$T.OnBattery; Top = $top; TopMem = $topMem; Path = [string]$T.Path; Drive = $drive; Media = $media
        Hz = $hz; HzMax = $hzMax; Display = $display; Laptop = [bool]$script:IsLaptop; Dual = ($gpuNames.Count -ge 2); Nvidia = [bool]($gpuNames -match 'NVIDIA|GeForce')
        Rtx = [bool]($gpuNames -match 'RTX'); Amd = [bool]($gpuNames -match 'Radeon|AMD'); GameMode = [bool](Get-Setting 'GameMode' $false); Ups = [bool]$script:HasUps
    }
}

# ---------------------------------------------------------------------------
# Diagnostic
# ---------------------------------------------------------------------------
# Jeu d'une partie (pour les profils) : le jeu connu, sinon l'exécutable mesuré.
function Get-SessionGame($S) {
    $key = [string]$S.Key
    if ($script:GameIndex -and $script:GameIndex[$key]) {
        $name = $script:GameIndex[$key].Game
        $g = @($script:Games | Where-Object { $_.Name -eq $name })[0]
        if ($g) { return $g }
    }
    if ($S.Diag -and $S.Diag.Path) { return @{ Name = (Get-SessionName $S); Exes = @([string]$S.Diag.Path) } }
    $null
}

function Add-DiagItem($List, [string]$Status, [string]$Title, [string]$Detail, $Items, $Actions) {
    [void]$List.Add(@{ Status = $Status; Title = $Title; Detail = $Detail; Items = @(@($Items) | Where-Object { $_ }); Actions = @(@($Actions) | Where-Object { $_ }); ShowAll = $true })
}

function Get-FpsDiagnosis($S) {
    $d = $S.Diag
    $items = New-Object System.Collections.ArrayList
    if (-not $d) {
        return @{ Limit = 'unknown'; Problem = $false; Status = 'info'; Headline = 'Pas de diagnostic pour cette partie'
                  Text = 'Elle a été mesurée par une ancienne version de Nevermind. Rejoue une partie pour avoir le diagnostic complet.'; Items = @() }
    }
    $avg = [double]$S.Avg; $low = [double]$S.Low1
    $perMin = $d.Stutters / [math]::Max(1.0, $S.Seconds / 60)
    $lowFps = $avg -lt 60
    $stutter = ($low -lt 0.5 * $avg) -or ($perMin -gt 6)
    $problem = $lowFps -or $stutter
    $game = Get-SessionGame $S
    $gameName = Get-SessionName $S

    # Qui freine ?
    # FPS bloqués : moyenne sur une valeur ronde (ou la fréquence de l'écran) et courbe presque plate.
    $series = @($S.Series | Where-Object { $null -ne $_ } | ForEach-Object { [double]$_ })
    $flat = $low -ge 0.75 * $avg
    if ($series.Count -ge 10) {
        $mean = ($series | Measure-Object -Average).Average
        $var = 0.0; foreach ($x in $series) { $var += ($x - $mean) * ($x - $mean) }
        $flat = $flat -and ($mean -gt 0) -and ([math]::Sqrt($var / $series.Count) / $mean -lt 0.02)
    }
    $cap = $null
    foreach ($c in @(@(30, 60, 75, 90, 100, 120, 144, 165, 170, 180, 200, 240, 280, 360) + @($d.Hz))) {
        if ($flat -and $c -and [math]::Abs($avg - $c) -le [math]::Max(1.5, $c * 0.03)) { $cap = [int]$c; break }
    }
    $gpuBound = ($d.GpuRatio -ge 0.85) -or ($d.GpuRatio -lt 0 -and $d.Gpu -ge 93)
    $igpu = $d.Dual -and $d.Nvidia -and $null -ne $d.Gpu -and $d.Gpu -lt 10 -and $avg -lt 60
    $cpuBound = -not $gpuBound -and (($d.CpuRatio -ge 0.85) -or ($d.GpuRatio -ge 0 -and $d.GpuRatio -lt 0.7) -or ($d.GpuRatio -lt 0 -and $null -ne $d.Gpu -and $d.Gpu -lt 75))
    $limit = if ($igpu) { 'igpu' } elseif ($cap -and -not $gpuBound) { 'cap' } elseif ($gpuBound) { 'gpu' } elseif ($cpuBound) { 'cpu' } else { 'mixed' }

    $gpuLabel = if ($null -ne $d.Gpu) { " (utilisée à $([int]$d.Gpu) %)" } else { '' }
    switch ($limit) {
        'igpu' { $head = 'Le jeu tourne sur la petite puce graphique'; $txt = 'Ton PC a deux cartes graphiques, et le jeu utilise la moins puissante (celle intégrée au processeur). La grosse carte est restée presque au repos.' }
        'cap' { $head = "Tes FPS sont bloqués à $cap"; $txt = if ($d.HzMax -and $cap -ge $d.HzMax) { "C'est la fréquence de ton écran ($($d.HzMax) Hz) : afficher plus d'images ne servirait à rien. C'est idéal." } else { 'Une limite de FPS ou la synchronisation verticale (V-Sync) bloque les images. Ce n''est pas ton PC qui peine.' } }
        'gpu' { $head = "C'est ta carte graphique qui décide des FPS$gpuLabel"; $txt = 'Elle travaille à fond pour dessiner chaque image. Pour gagner des FPS, il faut lui demander un peu moins de travail dans les options graphiques du jeu.' }
        'cpu' { $head = 'C''est ton processeur qui limite les FPS'; $txt = 'La carte graphique attend que le processeur lui prépare les images. Des programmes en arrière plan, un mode d''économie d''énergie ou certains réglages du jeu peuvent en être la cause.' }
        default { $head = 'Ni la carte graphique ni le processeur ne sont à fond'; $txt = 'Les FPS sont limités par autre chose : le jeu lui même, une limite de FPS, ou des petits blocages (disque, mémoire).' }
    }
    if (-not $problem -and $limit -ne 'cap') { $txt = 'Ta partie était fluide. ' + $txt }

    # Causes et corrections, de la plus probable à la moins probable
    if ($d.OnBattery -and $d.Ups -and -not $d.Laptop) {
        Add-DiagItem $items 'bad' 'Coupure de courant pendant la partie' 'Ton PC tournait sur l''onduleur : Windows peut alors brider le PC, et l''onduleur ne tient que quelques minutes.' @('Quand le courant est coupé, enregistre et quitte ta partie.') $null
    } elseif ($d.OnBattery) {
        Add-DiagItem $items 'bad' 'Le PC était sur batterie' 'Sur batterie, Windows bride le processeur et la carte graphique pour tenir plus longtemps : les FPS chutent.' @('Branche le chargeur quand tu joues.') $null
    }
    if ($limit -eq 'igpu' -and $game) {
        Add-DiagItem $items 'bad' "Forcer la grosse carte graphique pour $gameName" 'Windows choisit parfois la puce intégrée pour économiser la batterie. Nevermind peut obliger le jeu à utiliser la carte puissante.' @('Relance le jeu après le réglage.') @(
            @{ Label = 'Utiliser la carte puissante'; NoRefresh = $true; Arg = $game; Script = { param($g) Hide-TestPanel; Set-GameProfile $g 'gpu' $true; Show-Message "C'est réglé. Relance $($g.Name) pour que ce soit pris en compte." } })
    }
    $powerOk = $true
    try { $powerOk = Test-PowerPlan } catch {}
    if (-not $powerOk -and ($limit -eq 'cpu' -or $stutter -or $lowFps)) {
        Add-DiagItem $items 'warn' 'Windows est en mode économie d''énergie' 'Le processeur ralentit dès qu''il peut, ce qui donne des chutes de FPS et des micro saccades.' $null @(
            @{ Label = 'Mettre en mode performances'; NoRefresh = $true; Script = { Hide-TestPanel; Invoke-TweakFix @('power') } })
    } elseif ($null -ne $d.Perf -and $d.Perf -lt 85 -and $d.Cpu -ge 50) {
        Add-DiagItem $items 'warn' 'Le processeur a ralenti pendant la partie' "Il tournait en moyenne à $([int]$d.Perf) % de sa vitesse normale alors qu'il était très occupé : souvent une surchauffe." @(
            'Dépoussière les grilles et les ventilateurs du PC.', 'Vérifie que les ventilateurs du processeur tournent.', 'Sur un portable, pose le sur une surface dure (pas sur un lit ou un coussin).') $null
    }
    if ($null -ne $d.Temp -and $d.Temp -ge 83) {
        Add-DiagItem $items 'warn' "Ta carte graphique a chauffé ($([int]$d.Temp) °C)" 'Au delà de 83 °C environ, elle baisse sa vitesse pour se protéger, et les FPS avec.' @(
            'Dépoussière la carte et les entrées d''air du boîtier.', 'Laisse de l''espace autour du PC pour qu''il respire.', 'Si le boîtier a peu de ventilateurs, en ajouter un aide beaucoup.') $null
    }
    if ($null -ne $d.Vram -and $d.Vram -ge 95) {
        Add-DiagItem $items 'warn' 'La mémoire de ta carte graphique était pleine' "Elle était remplie à $([int]$d.Vram) % : le jeu doit alors aller chercher ses textures plus loin, ce qui donne des saccades." @(
            'Dans le jeu, baisse la qualité des textures d''un cran (ex : Ultra vers Élevé). La différence se voit à peine.',
            'Ferme les vidéos et les navigateurs pendant que tu joues : ils utilisent aussi cette mémoire.') $null
    }
    if ($null -ne $d.Ram -and $d.Ram -ge 90) {
        $heavy = @($d.TopMem | Where-Object { $_.Name -notmatch $SystemProcs -and $_.Name -ne $S.Key } | Select-Object -First 4)
        $acts = @($heavy | Select-Object -First 2 | ForEach-Object {
            @{ Label = "Fermer $($_.Name)"; NoRefresh = $true; Arg = $_.Name
               Confirm = "Fermer tous les $($_.Name) ?$(if ($_.Name -match $Browsers) { "`n`nTes onglets seront proposés à la réouverture du navigateur." }) Si tu as du travail pas enregistré dedans, ferme le toi même."
               Script = { param($n) Get-Process -Name $n -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue; Set-Status "$n fermé." } } })
        Add-DiagItem $items 'warn' "La mémoire vive était presque pleine ($([int]$d.Ram) %)" 'Quand elle est pleine, Windows déplace des données sur le disque et le jeu saccade.' @($heavy | ForEach-Object { "$($_.Name) : $([math]::Round($_.MB / 1024, 1)) Go" }) $acts
    }
    $bg = @($d.Top | Where-Object { $_.Name -notmatch '^(System|Idle|dwm|audiodg|MsMpEng|csrss)$' -and $_.Pct -ge 4 })
    if ($bg.Count -and ($limit -eq 'cpu' -or $stutter)) {
        $acts = @()
        foreach ($p in @($bg | Where-Object { $_.Name -notmatch $SystemProcs } | Select-Object -First 2)) {
            $acts += @{ Label = "Fermer $($p.Name)"; NoRefresh = $true; Arg = $p.Name
                Confirm = "Fermer tous les $($p.Name) ? Si tu as du travail pas enregistré dedans, ferme le toi même."
                Script = { param($n) Get-Process -Name $n -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue; Set-Status "$n fermé." } }
        }
        if (-not $d.GameMode) {
            $acts += @{ Label = 'Activer le mode jeu'; NoRefresh = $true; Script = { Set-Setting 'GameMode' $true; Update-GameWatch; Build-GameModeCard; Set-Status 'Mode jeu automatique activé : les applis cochées seront fermées pendant tes parties.' } }
        }
        $lines = @($bg | ForEach-Object { "$($_.Name) : $([math]::Round($_.Pct, 0)) % du processeur" })
        if ($d.Top | Where-Object { $_.Name -eq 'MsMpEng' -and $_.Pct -ge 4 }) { $lines += 'MsMpEng, c''est l''antivirus : une analyse tournait pendant la partie. Lance les analyses quand tu ne joues pas.' }
        Add-DiagItem $items 'warn' 'Des programmes en arrière plan utilisaient ton processeur' 'Pendant que tu jouais, ces programmes prenaient du temps de calcul au jeu.' $lines $acts
    }
    if ($stutter -and ($d.Media -eq 'HDD' -or ($null -ne $d.Disk -and $d.Disk -ge 90))) {
        $lines = if ($d.Media -eq 'HDD') { @("Le jeu est installé sur un disque dur classique ($($d.Drive):), bien plus lent qu'un SSD.", 'Steam : clic droit sur le jeu > Propriétés > Fichiers installés > Déplacer le dossier d''installation, vers un SSD.', 'Epic : désinstalle puis réinstalle le jeu sur un SSD.') } else { @('Le disque était occupé à 100 % par moments pendant la partie.', 'Évite les téléchargements et les mises à jour (Steam, Windows) pendant que tu joues.') }
        Add-DiagItem $items 'warn' 'Le disque ralentit le jeu' 'Les saccades arrivent quand le jeu doit charger des données et que le disque n''arrive pas à suivre.' $lines $null
    }
    if ($limit -eq 'cap' -and $d.HzMax -gt $cap) {
        # Seulement l'écran où tourne le jeu (un second écran volontairement en 60 Hz ne compte pas).
        $disp = @($script:LastAnalysis.Active | Where-Object { $d.Display -and $_.Id -eq "display:$($d.Display)" -and $_.Status -ne 'ok' })
        if ($cap -le 60 -and $d.Hz -le 60 -and $d.HzMax -gt 60 -and $disp.Count) {
            Add-DiagItem $items 'warn' "Ton écran est réglé en $($d.Hz) Hz alors qu'il peut faire $($d.HzMax) Hz" 'L''écran n''affiche que 60 images par seconde : le jeu se cale dessus. En le passant à sa vraie fréquence, tu gagnes en fluidité sans rien perdre.' $null @(
                @{ Label = 'Régler l''écran'; NoRefresh = $true; Arg = $disp[0]; Script = { param($f) Hide-TestPanel; Open-Sheet @($f) } })
        } else {
            Add-DiagItem $items 'info' "Une limite bloque le jeu à $cap FPS" "Ton écran peut afficher $($d.HzMax) images par seconde." @(
                'Dans les options graphiques du jeu, désactive la synchronisation verticale (V-Sync), ou monte la limite de FPS.',
                "Idéal : limite de FPS à $($d.HzMax) (la fréquence de ton écran), avec G-Sync / FreeSync si ton écran le gère.") $null
        }
    }
    if ($limit -eq 'gpu' -and $problem) {
        $up = if ($d.Rtx) { 'Active le DLSS en mode « Qualité » (ta carte NVIDIA RTX le gère) : souvent +30 à 50 % de FPS, image presque identique.' } elseif ($d.Amd) { 'Active le FSR en mode « Qualité » : souvent +30 à 40 % de FPS, image presque identique.' } else { 'Active le DLSS, le FSR ou le XeSS en mode « Qualité » s''ils sont proposés : souvent +30 % de FPS, image presque identique.' }
        Add-DiagItem $items 'info' 'Les réglages du jeu qui font gagner le plus, pour le moins de perte' 'À changer dans les options graphiques du jeu, dans cet ordre. Rejoue après chaque étape : arrête toi dès que c''est fluide.' @(
            $up,
            'Ombres : de Ultra à Moyen (+10 à 20 %, se voit peu en jeu).',
            'Effets volumétriques, nuages, brouillard : Bas (+10 %).',
            'Réflexions et ray tracing : désactivés ou Bas (le ray tracing peut coûter 30 à 50 %).',
            'Occlusion ambiante : Moyen.',
            'Garde les textures au maximum si la mémoire de ta carte suffit : elles coûtent très peu de FPS.',
            'En dernier recours seulement : résolution de rendu à 85 ou 90 %.') $null
    }
    if ($limit -eq 'cpu' -and $problem) {
        $acts = @()
        if ($game -and -not (Test-GamePriority $game)) {
            $acts += @{ Label = 'Donner la priorité au jeu'; NoRefresh = $true; Arg = $game; Script = { param($g) Hide-TestPanel; Set-GameProfile $g 'priority' $true; Show-Message "C'est réglé : $($g.Name) passera avant les autres programmes à son prochain lancement." } }
        }
        Add-DiagItem $items 'info' 'Les réglages du jeu qui soulagent le processeur' 'Ta carte graphique a de la marge : tu peux même monter les effets visuels sans perdre de FPS. Ce qui compte ici :' @(
            'Distance d''affichage (ou de vue) : d''un cran plus bas.',
            'Densité de foule, de végétation ou de personnages : Moyen.',
            'Qualité de la physique ou des particules : Moyen.',
            $(if ($d.Nvidia) { 'Active NVIDIA Reflex s''il est proposé (mode « Activé »).' } elseif ($d.Amd) { 'Active AMD Anti-Lag dans le logiciel AMD.' } else { $null })) $acts
    }
    if ($stutter -and -not @($items | Where-Object { $_.Status -in 'bad', 'warn' }).Count) {
        $prev = @(Get-FpsSessions | Where-Object { $_.Key -eq $S.Key -and $_.Id -ne $S.Id })
        if (-not $prev.Count) {
            Add-DiagItem $items 'info' 'Première partie mesurée sur ce jeu' 'Les premières minutes d''un jeu (et après une mise à jour du pilote graphique) saccadent souvent : le jeu prépare ses effets visuels. Rejoue une partie pour comparer.' $null $null
        }
    }
    $nv = $script:AnalysisData.NvLatest
    if ($problem -and $d.Nvidia -and $nv) {
        $g0 = @($script:AnalysisData.GPUs | Where-Object { $_.Name -match 'NVIDIA|GeForce' })[0]
        $inst = if ($g0) { ConvertTo-NvidiaVersion $g0.DriverVersion } else { $null }
        $old = $false; try { $old = $inst -and [version]$inst -lt [version]$nv.Version } catch {}
        if ($old) {
            Add-DiagItem $items 'warn' "Pilote NVIDIA pas à jour ($inst, le dernier est le $($nv.Version))" 'Les nouveaux pilotes corrigent souvent des chutes de FPS dans les jeux récents.' $null @(
                @{ Label = 'Page du pilote'; NoRefresh = $true; Arg = $nv.Url; Script = { param($u) Open-Url $u } })
        }
    }
    if (-not $problem) {
        Add-DiagItem $items 'ok' 'Aucun problème de FPS sur cette partie' $(if ($limit -eq 'gpu') { 'Si tu veux encore plus de FPS un jour, commence par le DLSS ou le FSR en mode Qualité.' } else { 'Rien à régler.' }) $null $null
    }
    $status = if (@($items | Where-Object { $_.Status -eq 'bad' }).Count) { 'bad' } elseif ($problem) { 'warn' } else { 'ok' }
    @{ Limit = $limit; Problem = $problem; Status = $status; Headline = $head; Text = $txt; Items = @($items); Cap = $cap }
}

# Applique des réglages Windows précis (ceux de l'onglet Réglages), avec historique et retour en arrière.
function Invoke-TweakFix([string[]]$Ids) {
    $sel = @(Get-AvailableTweaks | Where-Object { $Ids -contains $_.Id -and -not (Test-Tweak $_) })
    if (-not $sel.Count) { Show-Message 'C''est déjà réglé.'; return }
    $done = @()
    $script:RunLog = New-Object System.Collections.ArrayList
    try { foreach ($t in $sel) { & $t.Apply; $done += $t.Titre } } finally { $log = $script:RunLog; $script:RunLog = $null }
    Build-GamingTab
    Update-BackupSummary
    Show-ResultSheet 'C''est fait !' (@('Réglage appliqué :') + @($done | ForEach-Object { "•  $_" }) + @('Rejoue une partie : Nevermind comparera tes FPS avant / après.')) $log $null
}

# Section « d'où ça vient » dans la fiche d'une partie.
function Add-FpsDiagnosisView($S, $Body) {
    $dg = Get-FpsDiagnosis $S
    [void]$Body.Children.Add((New-SectionTitle 'D''OÙ ÇA VIENT'))
    $card = New-Object System.Windows.Controls.Border
    $bg = Get-Brush $Colors[$dg.Status]; $bg.Opacity = 0.1
    $card.Background = $bg
    $card.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $card.Padding = New-Thickness 16 12 16 12
    $card.Margin = New-Thickness 0 4 0 8
    $sp = New-Object System.Windows.Controls.StackPanel
    [void]$sp.Children.Add((New-Text $dg.Headline 16 '#FFFFFF' -Bold))
    $t = New-Text $dg.Text 13 '#D3CDE3'
    $t.Margin = New-Thickness 0 4 0 0
    [void]$sp.Children.Add($t)
    $d = $S.Diag
    if ($d -and $d.GpuRatio -ge 0 -and $dg.Limit -in 'gpu', 'cpu', 'mixed') {
        $bars = New-Object System.Windows.Controls.StackPanel
        $bars.Margin = New-Thickness 0 10 0 0
        foreach ($b in @(@('Carte graphique', $d.GpuRatio), @('Processeur', $d.CpuRatio))) {
            $row = New-Grid @('130', '*', '50')
            $row.Margin = New-Thickness 0 2 0 2
            Add-ToGrid $row (New-Text $b[0] 12 '#A6A1BC') 0
            $pb = New-Object System.Windows.Controls.ProgressBar
            $pb.Height = 8; $pb.Minimum = 0; $pb.Maximum = 100; $pb.Value = [math]::Min(100.0, 100 * $b[1])
            $pb.Foreground = Get-Brush $(if ($b[1] -ge 0.85) { $Colors.warn } else { $Colors.info })
            $pb.Background = Get-Brush '#22FFFFFF'; $pb.BorderThickness = New-Thickness 0 0 0 0; $pb.VerticalAlignment = 'Center'
            Add-ToGrid $row $pb 1
            $v = New-Text ('{0:N0} %' -f (100 * $b[1])) 12 '#EEEBF7' -Semi
            $v.HorizontalAlignment = 'Right'
            Add-ToGrid $row $v 2
            [void]$bars.Children.Add($row)
        }
        [void]$bars.Children.Add((New-Text 'Part du temps où chacun travaillait pour dessiner une image. Celui qui est proche de 100 % limite les FPS.' 11 '#655E7E'))
        [void]$sp.Children.Add($bars)
    }
    $card.Child = $sp
    [void]$Body.Children.Add($card)
    foreach ($it in $dg.Items) {
        $c = New-SecurityCard $it
        $c.Margin = New-Thickness 0 4 0 4
        [void]$Body.Children.Add($c)
    }
    $dg
}
