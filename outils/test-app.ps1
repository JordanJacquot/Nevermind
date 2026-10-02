# Code injecté par outils\tester.ps1 dans une copie de l'app, juste avant l'ouverture de la fenêtre.
# Il remplace les boîtes de dialogue (pour ne jamais bloquer), parcourt l'app et note chaque résultat.
$script:T = @{
    Dir = '__TEST__'; Complet = $__COMPLET__; Captures = $__CAPTURES__
    Res = New-Object System.Collections.ArrayList; Msgs = New-Object System.Collections.ArrayList
    Clock = [Diagnostics.Stopwatch]::StartNew(); Last = 0.0; MaxGap = 0.0; Watch = $false
}
function Invoke-UpdateCheck { }
# Chronométrage des tâches répétées de l'app (qui tournent sur la fenêtre)
$script:T.Slow = @{}
foreach ($fn in 'Test-GameRunning', 'Update-LiveUI', 'Update-Traffic', 'Update-LagSession', 'Update-FpsTarget', 'Update-DevPing') {
    if (-not (Get-Command $fn -ErrorAction SilentlyContinue)) { continue }
    Set-Item -Path "function:$fn-Chrono" -Value (Get-Item "function:$fn").ScriptBlock
    Set-Item -Path "function:$fn" -Value ([scriptblock]::Create("`$sw = [Diagnostics.Stopwatch]::StartNew(); try { $fn-Chrono @args } finally { `$ms = `$sw.ElapsedMilliseconds; if (`$ms -gt [int]`$script:T.Slow['$fn']) { `$script:T.Slow['$fn'] = `$ms } }"))
}
function Show-Message([string]$Text, [string]$Icon = 'Information') { [void]$script:T.Msgs.Add("[$Icon] $Text") }
function Confirm-Action([string]$Text) { [void]$script:T.Msgs.Add("[Question] $Text"); $false }
function Show-Notify([string]$Title, [string]$Text, [scriptblock]$OnClick) { [void]$script:T.Msgs.Add("[Notify] $Title : $Text") }
# Fausse icône près de l'horloge : rien n'apparaît sur le PC pendant le test.
function Get-TrayIcon { if (-not $script:FakeTray) { $script:FakeTray = [pscustomobject]@{ Visible = $false } }; $script:FakeTray }

function Add-TestResult([string]$Name, [bool]$Ok, [string]$Detail = '') {
    [void]$script:T.Res.Add([pscustomobject]@{ Test = $Name; Ok = $Ok; Detail = $Detail })
}
function Wait-TestMs([int]$Ms) {
    $end = (Get-Date).AddMilliseconds($Ms)
    while ((Get-Date) -lt $end) { Update-UI; Start-Sleep -Milliseconds 15 }
}
function Save-TestShot([string]$Name) {
    if (-not $script:T.Captures) { return }
    try {
        $el = $Window.Content; $el.UpdateLayout()
        $w = [int]$el.ActualWidth; $h = [int]$el.ActualHeight
        $dv = New-Object System.Windows.Media.DrawingVisual; $dc = $dv.RenderOpen()
        $dc.DrawRectangle((Get-Brush '#0E1014'), $null, [System.Windows.Rect]::new(0, 0, $w, $h))
        $vb = New-Object System.Windows.Media.VisualBrush $el; $vb.Stretch = 'None'; $vb.AlignmentX = 'Left'; $vb.AlignmentY = 'Top'; $vb.ViewboxUnits = 'Absolute'; $vb.Viewbox = [System.Windows.Rect]::new(0, 0, $w, $h)
        $dc.DrawRectangle($vb, $null, [System.Windows.Rect]::new(0, 0, $w, $h)); $dc.Close()
        $rtb = New-Object System.Windows.Media.Imaging.RenderTargetBitmap($w, $h, 96, 96, [System.Windows.Media.PixelFormats]::Pbgra32); $rtb.Render($dv)
        $enc = New-Object System.Windows.Media.Imaging.PngBitmapEncoder; $enc.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($rtb))
        $fs = [IO.File]::Create((Join-Path $script:T.Dir "captures\$Name.png")); $enc.Save($fs); $fs.Close()
    } catch {}
}
# Exécute une étape : échoue si elle lève une erreur ou affiche un message d'avertissement.
function Test-Step([string]$Name, [scriptblock]$Body) {
    $script:T.Step = $Name
    $before = $script:T.Msgs.Count
    try {
        $detail = & $Body
        $warn = @($script:T.Msgs | Select-Object -Skip $before | Where-Object { $_ -match '^\[(Warning|Error)\]' })
        if ($warn.Count) { Add-TestResult $Name $false ($warn -join ' | ') } else { Add-TestResult $Name $true "$detail" }
    } catch { Add-TestResult $Name $false "$($_.Exception.Message) $($_.InvocationInfo.PositionMessage)" }
}
function Assert-Test([bool]$Cond, [string]$Why) { if (-not $Cond) { throw $Why } }

$Window.WindowStartupLocation = 'Manual'; $Window.Left = -5000; $Window.Top = 0; $Window.Height = 900; $Window.ShowInTaskbar = $false

# Mesure de fluidité : plus long moment où la fenêtre n'a pas répondu.
$script:T.Beat = New-Object System.Windows.Threading.DispatcherTimer
$script:T.Beat.Interval = [TimeSpan]::FromMilliseconds(50)
$script:T.Beat.Add_Tick({
    $now = $script:T.Clock.Elapsed.TotalMilliseconds
    if ($script:T.Watch -and $script:T.Last -and ($now - $script:T.Last) -gt $script:T.MaxGap) { $script:T.MaxGap = $now - $script:T.Last; $script:T.GapStep = $script:T.Step }
    $script:T.Last = $now
})
$script:T.Beat.Start()

$script:T.Run = New-Object System.Windows.Threading.DispatcherTimer
$script:T.Run.Interval = [TimeSpan]::FromSeconds(1)
$script:T.Run.Add_Tick({
    # Prête = analyse faite, statut « Prêt. » et vérifications d'accueil passées (elles notent la version vue).
    $ready = $script:LastAnalysis -and $ui.StatusText.Text -eq 'Prêt.' -and (Get-Setting 'LastVersion' '')
    if (-not $ready -and $script:T.Clock.Elapsed.TotalSeconds -lt 120) { return }
    $script:T.Run.Stop()
    try {
        $sec = [int]$script:T.Clock.Elapsed.TotalSeconds
        Add-TestResult 'Démarrage' ([bool]$ready) $(if ($ready) { "prête en $sec s" } else { "pas prête après $sec s ($($ui.StatusText.Text))" })
        if ($ready) {
            Test-Step 'Visite guidée (première ouverture)' {
                $end = (Get-Date).AddSeconds(5)
                while ($script:SheetMode -ne 'tour' -and (Get-Date) -lt $end) { Wait-TestMs 100 }
                Assert-Test ($ui.Overlay.Visibility -eq 'Visible' -and $script:SheetMode -eq 'tour') 'la visite ne s''affiche pas'
                Save-TestShot 'visite'
                for ($k = 1; $k -le $TourSteps.Count; $k++) { Show-TourStep $k; Wait-TestMs 100 }
                Assert-Test ($ui.Overlay.Visibility -ne 'Visible') 'la visite ne se ferme pas'
                Assert-Test ([bool](Get-Setting 'TourDone' $false)) 'visite non mémorisée'
                "$($TourSteps.Count) étapes"
            }
            $script:T.Watch = $true
            Save-TestShot 'accueil'
            Test-Step 'Accueil Ordinateur' {
                Assert-Test ($ui.HubCards.Children.Count -ge 8) "seulement $($ui.HubCards.Children.Count) cartes"
                "$($ui.HubCards.Children.Count) cartes"
            }
            Test-Step 'Tableau de bord' {
                $ui.Tabs.SelectedIndex = 0; Wait-TestMs 800; Save-TestShot 'tableau-de-bord'
                $a = $script:LastAnalysis
                Assert-Test ($a.Score -ge 0 -and $a.Score -le 100) "score invalide : $($a.Score)"
                Assert-Test (@($a.Findings).Count -ge 5) "seulement $(@($a.Findings).Count) points analysés"
                "score $($a.Score), $(@($a.Findings).Count) points"
            }
            foreach ($pg in @(@(1, 'Gaming'), @(2, 'Démarrage'), @(3, 'Connexion'), @(4, 'Nettoyage'), @(7, 'Sauvegarde'))) {
                $i = $pg[0]
                Test-Step "Page $($pg[1])" {
                    $ui.Tabs.SelectedIndex = $i; Wait-TestMs 800
                    Assert-Test ($ui.Tabs.SelectedIndex -eq $i) 'la page ne s''ouvre pas'
                    Save-TestShot $pg[1].ToLower()
                    $ui.StatusText.Text
                }
            }
            Test-Step 'Pilote graphique' {
                $f = @($script:LastAnalysis.Findings | Where-Object { $_.Id -like 'gpu-driver:*' })
                Assert-Test ($f.Count -ge 1) 'aucun point sur le pilote'
                ($f | ForEach-Object { "$($_.Titre) : $($_.Detail)" }) -join ' | '
            }
            Test-Step 'Mode jeu et profils par jeu' {
                $ui.Tabs.SelectedIndex = 1
                # Au démarrage, la liste est calculée juste après « Prêt. » : le test passe avant, on la calcule ici.
                if ($null -eq $script:Games) { Update-GameCache }
                Wait-TestMs 500; Save-TestShot 'gaming-reglages'; Set-GamingSubPage 'mode'; Wait-TestMs 300; Save-TestShot 'gaming-mode-jeu'; Set-GamingSubPage 'profiles'; Wait-TestMs 300; Save-TestShot 'gaming-profils'
                Assert-Test ($null -ne $script:Games) 'liste des jeux jamais calculée'
                Assert-Test ($ui.GameModePanel.Children.Count -ge 1) 'carte du mode jeu absente'
                # Session de jeu simulée : aucune appli cochée, donc rien n'est fermé sur ce PC
                Set-Setting 'GameModeApps' @()
                Start-GameSession 'Jeu d''essai' (Get-Process -Id $PID)
                Assert-Test ($null -ne $script:GameSession) 'session non démarrée'
                Stop-GameSession
                Assert-Test ($null -eq $script:GameSession) 'session non terminée'
                $admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
                $detail = "$(@($script:Games).Count) jeux, $($ui.GameProfilesPanel.Children.Count) lignes de profils"
                # Profil « carte puissante » sur un faux jeu (clé de l'utilisateur, pas besoin d'être administrateur)
                $fake = @{ Name = 'Jeu d''essai'; Exes = @('C:\OptiGameTest\OptiGameTestJeu.exe') }
                Set-GameProfile $fake 'gpu' $true
                $gOn = (Get-GpuPreference $fake.Exes[0]) -match 'GpuPreference=2'
                Set-GameProfile $fake 'gpu' $false
                $gOff = -not (Get-GpuPreference $fake.Exes[0])
                Assert-Test ($gOn -and $gOff) "carte puissante : activée=$gOn, retirée=$gOff"
                $detail += ', profil carte puissante activé puis retiré'
                if ($admin) {
                    $g = @{ Name = 'Jeu d''essai'; Exes = @('C:\OptiGameTest\OptiGameTestJeu.exe') }
                    $key = "$IfeoPath\OptiGameTestJeu.exe"
                    Set-GameProfile $g 'priority' $true
                    $on = Test-GamePriority $g
                    Set-GameProfile $g 'priority' $false
                    $off = Test-GamePriority $g
                    Remove-Item -LiteralPath $key -Recurse -Force -ErrorAction SilentlyContinue
                    Assert-Test ($on -and -not $off) "priorité : activée=$on, désactivée=$(-not $off)"
                    $detail += ', priorité haute activée puis retirée'
                } else { $detail += ' (profil non testé : pas administrateur)' }
                $detail
            }
            Test-Step 'Compteur de FPS (overlay, raccourci, avant / après)' {
                Assert-Test (Test-Path -LiteralPath $PresentMonExe) 'PresentMon absent'
                # Raccourci Ctrl+Maj+F : message simulé, compteur désactivé donc une notification l'explique
                Set-Setting 'FpsMeasure' $false
                $before = @($script:T.Msgs | Where-Object { $_ -like '`[Notify`] Mesure des FPS*' }).Count
                if (-not $script:HotkeyHandle) { Register-FpsHotkey }
                # Si OptiGame est déjà ouvert sur le PC, il garde le raccourci : on appelle alors l'action directement.
                if ($script:HotkeyHandle) { [void][OGNative]::SendMessage($script:HotkeyHandle, 0x0312, [IntPtr]$FpsHotkeyId, [IntPtr]::Zero); $hk = 'raccourci' }
                else { Switch-FpsManual; $hk = 'raccourci pris par l''app déjà ouverte, action testée directement' }
                $after = @($script:T.Msgs | Where-Object { $_ -like '`[Notify`] Mesure des FPS*' }).Count
                Assert-Test ($after -gt $before) 'le raccourci ne réagit pas'
                # Overlay affiché puis fermé
                Show-FpsOverlay
                $script:Overlay.Fps.Text = '144'; $script:Overlay.Sub.Text = '1 % bas 98    moyenne 131'
                $script:Overlay.Win.Show(); Wait-TestMs 300
                $ov = $script:Overlay.Win
                $ov.UpdateLayout()
                $rtb = New-Object System.Windows.Media.Imaging.RenderTargetBitmap([int]$ov.ActualWidth, [int]$ov.ActualHeight, 96, 96, [System.Windows.Media.PixelFormats]::Pbgra32)
                $rtb.Render($ov.Content)
                $enc = New-Object System.Windows.Media.Imaging.PngBitmapEncoder; $enc.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($rtb))
                $fs = [IO.File]::Create((Join-Path $script:T.Dir 'captures\overlay.png')); $enc.Save($fs); $fs.Close()
                Hide-FpsOverlay
                # Style discret : juste le chiffre, sans sous-ligne, semi transparent ; mise à jour en direct sans erreur
                Set-FpsOverlayStyle 'discret'
                Show-FpsOverlay
                Assert-Test ($script:Overlay.Discreet -and $null -eq $script:Overlay.Sub) 'style discret non appliqué'
                $script:Overlay.Fps.Text = '144'
                $script:Overlay.Win.Show(); Wait-TestMs 300
                $ov = $script:Overlay.Win; $ov.UpdateLayout()
                Assert-Test ($ov.ActualHeight -lt 40) "compteur discret trop grand : $([int]$ov.ActualWidth) x $([int]$ov.ActualHeight)"
                $rtb = New-Object System.Windows.Media.Imaging.RenderTargetBitmap([int]$ov.ActualWidth, [int]$ov.ActualHeight, 96, 96, [System.Windows.Media.PixelFormats]::Pbgra32)
                $rtb.Render($ov.Content)
                $enc = New-Object System.Windows.Media.Imaging.PngBitmapEncoder; $enc.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($rtb))
                $fs = [IO.File]::Create((Join-Path $script:T.Dir 'captures\overlay-discret.png')); $enc.Save($fs); $fs.Close()
                $script:FpsTarget = @{ Pid = $PID; Start = (Get-Date).AddSeconds(-20); Ticks = 0; Exclusive = $false; Warned = $false; Series = (New-Object System.Collections.ArrayList); Sys = (New-Object System.Collections.ArrayList); ProcCpu = @{}; ProcMem = @{} }
                Update-FpsTarget
                $script:FpsTarget = $null
                Show-Page 1; Set-GamingSubPage 'overlay'; Wait-TestMs 300; Save-TestShot 'overlay-page'
                Hide-FpsOverlay
                Set-FpsOverlayStyle 'complet'
                # Mesure lancée puis arrêtée sur un programme (sans droits admin, PresentMon refuse : l'app ne doit pas planter)
                Start-FpsTarget $PID 'Programme d''essai' 'powershell'
                Update-FpsTarget
                Stop-FpsTarget
                Assert-Test ($null -eq $script:FpsTarget -and $null -eq $script:Overlay) 'mesure non arrêtée'
                # Avant / après sur des parties simulées autour d'un changement
                [void](Add-History 'Réglage d''essai FPS' @() @(@{ Type = 'reg'; Path = 'HKCU:\Software\OptiGameTest'; Name = 'X'; Existed = $false }))
                $lc = Get-LastChangeDate
                $mk = { param($d, $avg, $low) [pscustomobject]@{ Id = [guid]::NewGuid().ToString('N').Substring(0, 10); Date = $d.ToString('s'); Game = 'Jeu d''essai'; Key = 'jeuessai'; Avg = $avg; Low1 = $low; Low01 = $low - 10; Seconds = 600; Frames = 60000; Exclusive = $false
                    Series = @(1..120 | ForEach-Object { [math]::Round($avg + 15 * [math]::Sin($_ / 7) - $(if ($_ % 29 -eq 0) { 40 } else { 0 }), 1) }) } }
                $sess = @((& $mk $lc.AddDays(-2) 110 70), (& $mk $lc.AddDays(-1) 114 74), (& $mk $lc.AddMinutes(5) 121 88), (& $mk $lc.AddMinutes(50) 125 90))
                ConvertTo-Json -InputObject $sess | Set-Content -LiteralPath $FpsFile -Encoding UTF8
                Build-FpsPanel
                $ui.Tabs.SelectedIndex = 1; Set-GamingSubPage 'fps'; Wait-TestMs 500; Save-TestShot 'mes-parties'
                Assert-Test ($ui.FpsPanel.Children.Count -ge 5) "panneau incomplet ($($ui.FpsPanel.Children.Count) éléments)"
                # Fiche d'une partie (clic sur la ligne la plus récente)
                $rowCard = @($ui.FpsPanel.Children | Where-Object { $_.Tag -is [string] })[0]
                Assert-Test ($null -ne $rowCard) 'aucune ligne de partie cliquable'
                $ev = New-Object System.Windows.Input.MouseButtonEventArgs ([System.Windows.Input.Mouse]::PrimaryDevice, 0, [System.Windows.Input.MouseButton]::Left)
                $ev.RoutedEvent = [System.Windows.UIElement]::MouseLeftButtonUpEvent
                $rowCard.RaiseEvent($ev)
                Wait-TestMs 1500; Save-TestShot 'fiche-partie'
                Assert-Test ($ui.TestOverlay.Visibility -eq 'Visible') 'la fiche de la partie ne s''ouvre pas'
                $ui.TestScroll.ScrollToEnd(); Wait-TestMs 300; Save-TestShot 'fiche-partie-bas'
                Hide-TestPanel
                Set-GamingSubPage 'tweaks'
                "$hk, overlay, arrêt propre et comparaison OK"
            }
            Test-Step 'Diagnostic des FPS (5 situations)' {
                $base = @{ CpuRatio = 0.5; GpuRatio = 0.6; Stutters = 0; Cpu = 30; CpuMax = 50; Perf = 105; Ram = 55; Gpu = 60; Temp = 65; Vram = 50; Power = 60; Disk = 10; DiskAvg = 3
                    OnBattery = $false; Top = @(); TopMem = @(); Path = 'C:\Jeux\Essai.exe'; Drive = 'C'; Media = 'SSD'; Hz = 144; HzMax = 144; Laptop = $false; Dual = $false; Nvidia = $true; Rtx = $true; Amd = $false; GameMode = $false }
                $mkS = {
                    param($avg, $low, [hashtable]$over)
                    $amp = if ($over.ContainsKey('Amp')) { $over.Amp } else { 8 }; $dd = $base.Clone(); foreach ($k in $over.Keys) { if ($k -ne 'Amp') { $dd[$k] = $over[$k] } }
                    [pscustomobject]@{ Id = [guid]::NewGuid().ToString('N').Substring(0, 10); Date = (Get-Date).ToString('s'); Game = 'Jeu d''essai'; Key = 'essai'; Avg = $avg; Low1 = $low; Low01 = $low * 0.7
                        Seconds = 900; Frames = 50000; Exclusive = $false; Series = @(1..60 | ForEach-Object { $avg + $amp * [math]::Sin($_ / 5) }); Diag = [pscustomobject]$dd }
                }
                $cases = @(
                    @('gpu', (& $mkS 45 35 @{ GpuRatio = 0.97; CpuRatio = 0.4; Gpu = 99; Vram = 97 }), 'Les réglages du jeu qui font gagner'),
                    @('cpu', (& $mkS 50 22 @{ GpuRatio = 0.45; CpuRatio = 0.93; Gpu = 50; Cpu = 85; Stutters = 200; Top = @([pscustomobject]@{ Name = 'chrome'; Pct = 18 }, [pscustomobject]@{ Name = 'MsMpEng'; Pct = 9 }) }), 'Des programmes en arrière plan'),
                    @('cap', (& $mkS 60 57 @{ GpuRatio = 0.4; CpuRatio = 0.3; Gpu = 40; Hz = 60; HzMax = 144; Amp = 0.4 }), 'Une limite bloque le jeu'),
                    @('igpu', (& $mkS 35 25 @{ GpuRatio = -1; CpuRatio = -1; Gpu = 3; Dual = $true; Laptop = $true }), 'Forcer la grosse carte'),
                    @('cpu', (& $mkS 200 150 @{}), 'Aucun problème')
                )
                $out = @()
                foreach ($cs in $cases) {
                    $dg = Get-FpsDiagnosis $cs[1]
                    $titles = @($dg.Items | ForEach-Object { $_.Title }) -join ' | '
                    Assert-Test ($dg.Limit -eq $cs[0]) "attendu $($cs[0]), obtenu $($dg.Limit) ($($dg.Headline))"
                    Assert-Test ($titles -like "*$($cs[2])*") "$($cs[0]) : conseil « $($cs[2]) » absent ($titles)"
                    $out += "$($cs[0]) : $($dg.Items.Count) conseil(s)"
                }
                # Fiche complète d'une partie qui rame (limitée par la carte graphique)
                ConvertTo-Json -InputObject @($cases[1][1], $cases[0][1]) -Depth 6 | Set-Content -LiteralPath $FpsFile -Encoding UTF8
                Show-FpsSession $cases[0][1].Id
                Wait-TestMs 1500; Save-TestShot 'diagnostic-gpu'
                $ui.TestScroll.ScrollToVerticalOffset(520); Wait-TestMs 300; Save-TestShot 'diagnostic-gpu-conseils'
                Hide-TestPanel
                Show-FpsSession $cases[1][1].Id
                Wait-TestMs 1200; $ui.TestScroll.ScrollToVerticalOffset(420); Wait-TestMs 300; Save-TestShot 'diagnostic-cpu'
                Hide-TestPanel
                $out -join ', '
            }
            Test-Step 'Correctifs de la revue de code' {
                # Ctrl+Maj+F n'est pas pris quand la mesure est désactivée
                Set-Setting 'FpsMeasure' $false; Update-FpsHotkey
                Assert-Test (-not $script:HotkeyRegistered) 'raccourci pris alors que la mesure est désactivée'
                # Relevé des programmes en arrière plan : deux relevés, le second complète le premier
                $t = @{ Pid = 0; ProcCpu = @{}; ProcMem = @{}; ProcSeconds = 0.0; PrevProc = $null; PrevProcTime = $null; ProcJob = $null }
                Add-FpsProcSample $t
                $end = (Get-Date).AddSeconds(15); while (-not $t.ProcJob.Handle.IsCompleted -and (Get-Date) -lt $end) { Wait-TestMs 100 }
                Add-FpsProcSample $t
                $end = (Get-Date).AddSeconds(15); while (-not $t.ProcJob.Handle.IsCompleted -and (Get-Date) -lt $end) { Wait-TestMs 100 }
                Wait-TestMs 1500
                Add-FpsProcSample $t
                Assert-Test ($t.ProcMem.Count -ge 10) "relevé mémoire incomplet ($($t.ProcMem.Count) programmes)"
                Assert-Test ($t.ProcSeconds -gt 0) 'aucune durée entre deux relevés'
                if ($t.ProcJob) { try { $t.ProcJob.PS.Dispose() } catch {} }
                # Annulation d'un DNS dont la carte réseau n'existe plus : message clair, rien n'est modifié
                $errs = Undo-RunLog @(@{ Type = 'dns'; IfIndex = 9999; Guid = '{00000000-0000-0000-0000-000000000000}'; Servers = @('1.1.1.1') })
                Assert-Test ([bool](@($errs) -match 'carte réseau')) "annulation DNS : $($errs -join ' ')"
                # Lien ouvert sans droits admin : raccourci temporaire correct (sans le lancer)
                $lnk = Join-Path $env:TEMP 'OptiGame-test-raccourci.lnk'
                $sh = New-Object -ComObject WScript.Shell; $sc = $sh.CreateShortcut($lnk); $sc.TargetPath = "$env:windir\notepad.exe"; $sc.Arguments = '/background'; $sc.Save()
                $chk = $sh.CreateShortcut($lnk)
                Assert-Test ($chk.Arguments -eq '/background') 'raccourci temporaire incorrect'
                [IO.File]::Delete($lnk)
                "raccourci libre, $($t.ProcMem.Count) programmes relevés en arrière plan, DNS protégé"
            }
            Test-Step 'Profils par jeu : libellés (issue 2)' {
                if ($null -eq $script:Games) { Update-GameCache }
                Build-GameProfiles
                $texts = @(); $switches = @()
                $stack = New-Object System.Collections.Stack; $stack.Push($ui.GameProfilesPanel)
                while ($stack.Count) {
                    $x = $stack.Pop()
                    if ($x -is [System.Windows.Controls.TextBlock]) { $texts += $x.Text }
                    if ($x -is [System.Windows.Controls.CheckBox] -and $x.Tag) { $switches += $x.Tag.What }
                    foreach ($ch in [System.Windows.LogicalTreeHelper]::GetChildren($x)) { if ($ch -is [System.Windows.DependencyObject]) { $stack.Push($ch) } }
                }
                if (-not $switches.Count) { return 'aucun jeu installé : rien à vérifier' }
                Assert-Test (-not @($texts | Where-Object { $_.Length -eq 1 }).Count) "libellés d'une lettre : $((@($texts | Where-Object { $_.Length -eq 1 }) | Select-Object -Unique) -join ', ')"
                Assert-Test (@($texts | Where-Object { $_ -eq 'Priorité haute' }).Count -ge 1) 'libellé « Priorité haute » absent'
                Assert-Test (-not @($switches | Where-Object { $_ -notin 'priority', 'gpu' }).Count) "réglage inconnu derrière un interrupteur : $(($switches | Select-Object -Unique) -join ', ')"
                "$($switches.Count) interrupteurs, libellés complets"
            }
            Test-Step 'Nettoyage : fichiers et journal (issue 1)' {
                Wait-TestMs 100
                $d = Join-Path $DataDir 'essai-nettoyage'
                $out = Join-Path $DataDir 'essai-hors-nettoyage'
                foreach ($x in $d, $out) { New-Item -ItemType Directory -Force -Path $x | Out-Null }
                New-Item -ItemType Directory -Force -Path "$d\sous" | Out-Null
                [IO.File]::WriteAllBytes("$d\gros.tmp", (New-Object byte[] 300000))
                [IO.File]::WriteAllBytes("$d\sous\petit.tmp", (New-Object byte[] 2000))
                [IO.File]::WriteAllBytes("$d\utilise.tmp", (New-Object byte[] 5000))
                [IO.File]::WriteAllBytes("$out\a-garder.txt", (New-Object byte[] 100))
                [IO.File]::WriteAllBytes("$d\recent.tmp", (New-Object byte[] 700))
                # Fichiers vieux de 3 jours ; recent.tmp (à l'instant) doit être laissé
                foreach ($x in "$d\gros.tmp", "$d\sous\petit.tmp", "$d\utilise.tmp", "$out\a-garder.txt") { [IO.File]::SetLastWriteTime($x, (Get-Date).AddDays(-3)) }
                # Un lien dans le dossier vers un autre dossier : il ne doit pas être suivi
                New-Item -ItemType Junction -Path "$d\lien" -Target $out | Out-Null
                $target = @{ Titre = 'Dossier d''essai'; Paths = @($d) }
                $script:T.Step = 'nettoyage : liste'
                $info = Invoke-Async $CleanListScript @{ Paths = $target.Paths; Top = 300 } | Select-Object -First 1
                Assert-Test ($info.Count -eq 3 -and [string]$info.Top[0][0] -like '*gros.tmp') "analyse : $($info.Count) fichiers (attendu 3, le lien ignoré) ; sur le disque : $(@(Get-ChildItem -LiteralPath $d -Recurse -Force -File -ErrorAction SilentlyContinue).Count) ; résultat : $(if ($info) { ($info.Keys -join '/') + ' taille ' + $info.Size } else { 'aucun' })"
                $script:T.Step = 'nettoyage : fenêtre fichiers'; Show-CleanFiles $target $info; Wait-TestMs 500; Save-TestShot 'nettoyage-fichiers'; Hide-TestPanel
                $lock = [IO.File]::Open("$d\utilise.tmp", 'Open', 'Read', 'None')
                $script:T.Step = 'nettoyage : suppression'; try { $r = Invoke-CleanTargets @($target) } finally { $lock.Dispose() }
                Assert-Test ($r.Deleted -eq 2 -and $r.Skipped -eq 1) "nettoyage : $($r.Deleted) supprimés, $($r.Skipped) laissés (attendu 2 et 1)"
                Assert-Test (Test-Path "$out\a-garder.txt") 'un fichier hors du dossier a été supprimé en suivant un lien'
                Assert-Test (Test-Path "$d\recent.tmp") 'un fichier de moins de 24 h a été supprimé'
                Assert-Test (-not (Test-Path "$d\sous")) 'dossier vide non supprimé'
                $log = Get-Content -LiteralPath $r.File -Raw -Encoding UTF8
                Assert-Test ($log -match '\[SUPPRIMÉ\].*gros\.tmp' -and $log -match '\[LAISSÉ\].*utilise\.tmp') 'journal incomplet'
                [IO.Directory]::Delete("$d\lien")
                # Vraie analyse de la page (lecture seule)
                $script:T.Step = 'nettoyage : page'; Show-Page 4; $script:T.Step = 'nettoyage : analyse page'; Invoke-CleanScan; Set-Busy $false; Wait-TestMs 500; Save-TestShot 'nettoyage-analyse'
                "3 fichiers vus (lien et fichier récent ignorés), 2 supprimés, 1 laissé car utilisé, journal $(Split-Path $r.File -Leaf)"
            }
            Test-Step 'Raccourci sur le bureau et démarrage' {
                # Bureau simulé : le vrai bureau n'est pas touché
                $script:DesktopDir = Join-Path $DataDir 'bureau-essai'
                New-Item -ItemType Directory -Force -Path $script:DesktopDir | Out-Null
                $exe = Join-Path (Get-AppRoot) 'OptiGame.exe'
                if (-not (Test-Path -LiteralPath $exe)) { [IO.File]::WriteAllBytes($exe, [byte[]](77, 90)) }
                Assert-Test (-not (Test-DesktopShortcut)) 'raccourci vu avant sa création'
                Invoke-CreateShortcut
                Assert-Test (Test-DesktopShortcut) "raccourci absent ou mauvaise cible : $(Get-ShortcutTarget (Get-DesktopShortcutPath))"
                Assert-Test ($ui.BtnShortcut.Content -eq 'Recréer le raccourci') "bouton : $($ui.BtnShortcut.Content)"
                $auto = Test-AutoStart
                Assert-Test ($ui.ChkAutoStart.IsChecked -eq $auto) 'interrupteur du démarrage différent de la tâche planifiée'
                $script:DesktopDir = $null
                "raccourci créé vers $(Split-Path $exe -Leaf), démarrage automatique $(if ($auto) { 'activé' } else { 'désactivé' }) sur ce PC"
            }
            Test-Step 'Recherche des réglages' {
                Assert-Test ($null -ne $script:Search) 'barre de recherche non branchée'
                $cases = @(
                    @('compteur discret', 'Style du compteur*'), @('netoyage', 'Nettoyage*'), @('demarage pc', 'Lancer OptiGame au démarrage*'),
                    @('raccourci bureau', 'Raccourci sur le bureau'), @('telemetrie', 'Ce que Windows envoie*'), @('ping', '*'), @('position overlay', 'Position du compteur')
                )
                foreach ($c in $cases) {
                    $r = @(Find-Settings $c[0])
                    Assert-Test ($r.Count -and $r[0].T -like $c[1]) "« $($c[0]) » donne : $(($r | Select-Object -First 3 | ForEach-Object { $_.T }) -join ' | ')"
                }
                Assert-Test (-not @(Find-Settings 'zzzqqq').Count) 'résultats pour un mot inconnu'
                # Suggestions affichées en tapant
                Focus-Search; $script:Search.Input.Text = 'fps'; Wait-TestMs 200
                Assert-Test ($script:Search.Popup.IsOpen -and @($script:Search.Rows).Count -ge 3) "suggestions : $(@($script:Search.Rows).Count)"
                $pc = $script:Search.Popup.Child; $pc.UpdateLayout()
                $rtb = New-Object System.Windows.Media.Imaging.RenderTargetBitmap([int]$pc.ActualWidth, [int]$pc.ActualHeight, 96, 96, [System.Windows.Media.PixelFormats]::Pbgra32)
                $rtb.Render($pc)
                $enc = New-Object System.Windows.Media.Imaging.PngBitmapEncoder; $enc.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($rtb))
                $fs = [IO.File]::Create((Join-Path $script:T.Dir 'captures\recherche-suggestions.png')); $enc.Save($fs); $fs.Close()
                # Aller au réglage : bonne page, bon sous-onglet, réglage mis en évidence
                Open-SearchEntry (@(Find-Settings 'position compteur')[0]); Wait-TestMs 400
                Assert-Test ($ui.Tabs.SelectedIndex -eq 1 -and $script:GamingSubPage -eq 'overlay') "page $($ui.Tabs.SelectedIndex), sous-onglet $($script:GamingSubPage)"
                Assert-Test (-not $script:Search.Popup.IsOpen -and -not $script:Search.Input.Text) 'barre non refermée'
                Assert-Test ($script:SearchLastHit -and $script:SearchLastHit.Effect) 'réglage non mis en évidence'
                Assert-Test (-not $script:Search.Input.IsKeyboardFocused) 'la barre garde le focus'
                Save-TestShot 'recherche-arrivee'
                Open-SearchEntry (@(Find-Settings 'raccourci bureau')[0]); Wait-TestMs 400
                $el = Find-PageElement $ui.Tabs.Items[7].Content 'Raccourci et démarrage'
                Assert-Test ($ui.Tabs.SelectedIndex -eq 7 -and $el) 'réglage « Raccourci et démarrage » non trouvé sur la page'
                Wait-TestMs 2500
                "$($cases.Count) recherches justes (fautes de frappe comprises), suggestions affichées, arrivée sur Overlay et Sauvegarde"
            }
            Test-Step 'Signaler un problème (bouton en haut)' {
                Assert-Test ($null -ne $script:TopReport -and $script:TopReport.IsVisible) 'bouton « Signaler un problème » absent en haut'
                Show-Page 6; Wait-TestMs 300
                Assert-Test $script:TopReport.IsVisible 'bouton absent sur une page intérieure'
                Show-ReportPanel; $script:ReportBox.Text = 'Le jeu rame depuis la mise à jour'; Wait-TestMs 400; Save-TestShot 'signaler'; Hide-TestPanel
                $dir = Join-Path $DataDir 'essai-rapport'; New-Item -ItemType Directory -Force -Path $dir | Out-Null
                $zip = Export-ProblemReport $dir 'Le jeu rame depuis la mise à jour'
                Add-Type -AssemblyName System.IO.Compression.FileSystem
                $z = [IO.Compression.ZipFile]::OpenRead($zip)
                try { $names = @($z.Entries | ForEach-Object { $_.Name }) } finally { $z.Dispose() }
                Assert-Test ($names -contains 'description.txt' -and $names -contains 'infos.txt') "contenu du fichier : $($names -join ', ')"
                Show-Page $HubIndex
                "bouton visible partout, fichier avec la description ($($names.Count) fichiers)"
            }
            Test-Step 'Écran de chargement au démarrage' {
                Wait-TestMs 500
                Assert-Test ($ui.StartupOverlay.Visibility -eq 'Collapsed') 'l''écran de chargement reste affiché après le démarrage'
                $l = @(Select-String -LiteralPath $LogFile -Pattern 'Démarrage terminé en' -SimpleMatch | Select-Object -Last 1)
                Assert-Test ($l.Count -eq 1) 'durée du démarrage non notée'
                # Capture de l'écran de chargement (réaffiché un instant)
                $ui.StartupOverlay.BeginAnimation([System.Windows.UIElement]::OpacityProperty, $null); $ui.StartupOverlay.Opacity = 1; $ui.StartupOverlay.Visibility = 'Visible'
                foreach ($zp in [System.Windows.Media.ScaleTransform]::ScaleXProperty, [System.Windows.Media.ScaleTransform]::ScaleYProperty) { $ui.StartupZoom.BeginAnimation($zp, $null) }
                Start-StartupLoader
                Assert-Test ($ui.StartupLoaderHost.Children.Count -eq 1 -and $script:Loader.Loops.Count -ge 6) 'compteur animé absent'
                Set-StartupStep 'Recherche de tes jeux...' 72; $ui.StartupDetail.Text = 'Calcul: Fichiers temporaires (utilisateur)...'; Wait-TestMs 1200; Save-TestShot 'chargement'
                Stop-StartupLoader
                Assert-Test ($null -eq $script:Loader) 'animations du chargement non arrêtées'
                $ui.StartupOverlay.Visibility = 'Collapsed'
                ($l[0].Line -replace '^.*Démarrage terminé', 'premières tâches terminées')
            }
            Test-Step 'Carte du réseau' {
                $saved = $script:NetList
                $kinds = @('Ce PC', 'Ordinateur', 'Routeur ou répéteur Wi-Fi', 'TV ou multimédia', 'Téléphone ou tablette', 'Imprimante', 'Caméra', 'Objet connecté', 'Console de jeu', 'Box ou décodeur TV', 'Enceinte ou audio', 'Appareil')
                $script:NetList = @(@{ Ip = '192.168.1.1'; Ms = 1; Mac = 'AA-00'; Gateway = $true; Self = $false; Title = 'Livebox'; Vendor = 'Sagemcom'; KindInfo = @{ Kind = 'Box Internet'; Glyph = 0xE80F; Color = $Colors.ok } })
                for ($i = 0; $i -lt 13; $i++) {
                    $k = $kinds[$i % $kinds.Count]
                    $script:NetList += @{ Ip = "192.168.1.$(10 + $i)"; Ms = $(if ($i % 4 -eq 3) { $null } else { $i }); Mac = "AA-$i"; Gateway = $false; Self = ($i -eq 0); Hidden = ($i % 4 -eq 3); New = ($i -eq 5); Camera = ($k -eq 'Caméra')
                        Title = "Appareil $i"; Vendor = 'Test'; KindInfo = @{ Kind = $k; Glyph = 0xE774; Color = '#4EA8FF' } }
                }
                Show-NetMap; Wait-TestMs 2600
                Assert-Test ($ui.NetMapSub.Text -match 'Appareil 3 \(192\.168\.1\.13\)') "appareils discrets non nommés en haut de la carte : $($ui.NetMapSub.Text)"
                Set-NetFilter 'hidden'
                Assert-Test ($ui.NetDevices.Children.Count -eq 3) "filtre Discrets : $($ui.NetDevices.Children.Count) appareils (attendu 3)"
                Set-NetFilter 'all'
                $nodes = @($ui.NetMapCanvas.Children | Where-Object { $_ -is [System.Windows.Controls.StackPanel] })
                Assert-Test ($ui.NetMapOverlay.Visibility -eq 'Visible' -and $nodes.Count -eq 15) "carte : $($nodes.Count) éléments (attendu 15 : Internet, la box et 13 appareils)"
                # Changer le filtre de la liste ne redessine pas la carte : les appareils restent affichés
                Assert-Test (@($nodes | Where-Object { $_.RenderTransform.ScaleX -lt 0.99 }).Count -eq 0) 'la carte a été redessinée par un changement de filtre'
                $pulse = @($ui.NetMapCanvas.Children | Where-Object { $_ -is [System.Windows.Shapes.Path] -and $_.Data -is [System.Windows.Media.EllipseGeometry] })[0]
                Assert-Test ($pulse -and $pulse.Data.Center -ne [System.Windows.Point]::new(600, 430)) 'les impulsions ne bougent pas'
                Save-TestShot 'carte-reseau'
                $nodes = $nodes.Count
                for ($i = 14; $i -lt 34; $i++) { $script:NetList += @{ Ip = "192.168.1.$(10 + $i)"; Ms = 2; Mac = "AA-$i"; Gateway = $false; Self = $false; Title = "Objet $i"; KindInfo = @{ Kind = 'Objet connecté'; Glyph = 0xE80F; Color = '#4EA8FF' } } }
                Show-NetMap; Wait-TestMs 3000; Save-TestShot 'carte-reseau-33'
                Hide-NetMap
                $script:NetList = $saved
                "$nodes éléments animés (orbes, impulsions), carte non redessinée par les filtres"
            }
            Test-Step 'Deuxième carte réseau de ce PC' {
                $loc = Get-LocalInterfaces
                Assert-Test ($loc.Count -ge 1) 'cartes réseau de ce PC non lues'
                # De préférence une carte déconnectée qui garde son adresse (le cas de « pcjordan-1 »)
                $ip = @(@($loc.Keys | Where-Object { $_ -notlike '169.254.*' -and $_ -ne '127.0.0.1' -and -not $loc[$_].Up }) + @($loc.Keys | Where-Object { $_ -notlike '169.254.*' -and $_ -ne '127.0.0.1' }))[0]
                $d = @{ Ip = $ip; Ms = $null; Mac = $null; Self = $false; Gateway = $false; Host = ''; Vendor = ''; Title = 'pcjordan-1'; New = $true; Hidden = $true }
                Set-LocalDevice $d $loc
                Assert-Test ($d.Self -and $d.Mac -and $d.Title -like "$env:COMPUTERNAME (ce PC*" -and -not $d.Hidden -and -not $d.New) "carte $ip : $($d.Title), mac $($d.Mac)"
                "$($loc.Count) adresse(s) de ce PC ; $ip reconnue comme « $($d.Title) », adresse physique $($d.Mac)$(if ($d.Vendor) { ", $($d.Vendor)" })"
            }
            Test-Step 'Réseau approfondi (situation simulée)' {
                $script:NetList = @(
                    @{ Ip = '10.0.0.1'; Ms = 1; Mac = 'AA-BB-CC-00-00-01'; Ttl = 64; Self = $false; Gateway = $true; Host = ''; Vendor = ''; Title = 'Box Internet'; New = $false },
                    @{ Ip = '10.0.0.20'; Ms = 3; Mac = 'AA-BB-CC-00-00-20'; Ttl = 64; Self = $false; Gateway = $false; Host = ''; Vendor = 'LG Innotek'; Title = 'LG Innotek'; New = $false }
                )
                $fake = @{
                    Arp = @('10.0.0.1|AA-BB-CC-00-00-01', '10.0.0.30|AA-BB-CC-00-00-30')
                    Mdns = @('10.0.0.20|ptr|_airplay._tcp.local|[LG] webOS TV OLED65._airplay._tcp.local', '10.0.0.20|txt|[LG] webOS TV OLED65._airplay._tcp.local|model=OLED65C54LA',
                             '10.0.0.20|txt|[LG] webOS TV OLED65._airplay._tcp.local|manufacturer=LG', '10.0.0.20|txt|[LG] webOS TV OLED65._airplay._tcp.local|serialNumber=SECRET123',
                             '10.0.0.20|a|LGwebOSTV.local|10.0.0.20')
                    Wsd = @('10.0.0.30|dn:NetworkVideoTransmitter tds:Device|onvif://www.onvif.org/type/video_encoder onvif://www.onvif.org/name/Cam%20Salon onvif://www.onvif.org/hardware/DS-2CD2143|http://10.0.0.30/onvif/device_service')
                    Upnp = @(); NetBios = @(); Ports = @('10.0.0.30|554', '10.0.0.30|80'); Titles = @('http://10.0.0.30:80|Web Viewer|App-webs/')
                    V6 = @('fe80::1234%12|AA-BB-CC-00-00-20', '2a01::99|AA-BB-CC-00-00-99')
                }
                $added = @(Merge-NetDeep $fake @{ Ip = '10.0.0.5' } @(1..254 | ForEach-Object { "10.0.0.$_" }))
                $tv = @($script:NetList | Where-Object { $_.Ip -eq '10.0.0.20' })[0]
                $cam = @($script:NetList | Where-Object { $_.Ip -eq '10.0.0.30' })[0]
                $v6 = @($script:NetList | Where-Object { $_.Only6 })[0]
                Assert-Test ($added.Count -eq 2) "appareils ajoutés : $($added.Count) (attendu 2)"
                Assert-Test ($tv.Title -eq '[LG] webOS TV OLED65' -and $tv.Model -eq 'OLED65C54LA' -and $tv.KindInfo.Kind -eq 'TV ou multimédia') "TV : $($tv.Title) / $($tv.Model) / $($tv.KindInfo.Kind)"
                Assert-Test (-not $tv.Txt.ContainsKey('serialNumber')) 'le numéro de série a été gardé'
                Assert-Test (@($tv.Ipv6) -contains 'fe80::1234%12') 'adresse IPv6 de la TV absente'
                Assert-Test ($cam.Camera -and $cam.Hidden -and $cam.KindInfo.Kind -eq 'Caméra' -and $cam.Title -eq 'Cam Salon') "caméra : $($cam.Title) / $($cam.KindInfo.Kind) / caméra=$($cam.Camera) discret=$($cam.Hidden)"
                Assert-Test ($null -ne $v6 -and $v6.Hidden) 'appareil visible seulement en IPv6 absent'
                # Une box ou un décodeur avec seulement un flux vidéo n'est pas une caméra
                $dec = @{ Ip = '10.0.0.40'; Ms = 2; Mac = ''; Self = $false; Gateway = $false; Host = ''; Vendor = 'Sagemcom'; Services = @(); Ports = @(554); Ipv6 = @(); FoundBy = @(); Txt = @{} }
                Update-DeviceIdentity $dec
                Assert-Test (-not $dec.Camera) 'un décodeur TV est pris pour une caméra'
                $script:NetFirstScan = $false
                $ui.Tabs.SelectedIndex = $NetIndex; Show-NetDevices; Wait-TestMs 1200; Save-TestShot 'reseau-approfondi'
                Show-DeviceDetail $cam; Wait-TestMs 1200; Save-TestShot 'fiche-camera'; Hide-TestPanel
                $script:NetList = $null
                $ui.NetDevices.Children.Clear()
                'TV nommée et typée, caméra ONVIF cachée repérée, appareil IPv6 trouvé, série non gardée'
            }
            Test-Step 'Onduleur' {
                $upsA = [pscustomobject]@{ Name = 'Back-UPS ES 700G FW:871.O2'; DeviceID = 'APCBack-UPS'; Chemistry = 3; BatteryStatus = 2; EstimatedChargeRemaining = 100; EstimatedRunTime = 25 }
                $upsB = [pscustomobject]@{ Name = 'Eaton 3S'; DeviceID = 'EATON'; Chemistry = 2; BatteryStatus = 1 }
                $lap = [pscustomobject]@{ Name = 'DELL 1VX1H'; DeviceID = '1VX1H'; Chemistry = 6; BatteryStatus = 2 }
                Assert-Test ((Test-IsUps $upsA) -and (Test-IsUps $upsB) -and -not (Test-IsUps $lap)) 'reconnaissance des onduleurs'
                # PC fixe dont le boîtier se déclare « inconnu », avec un onduleur : pas un portable
                Assert-Test (-not (Test-IsLaptop @($upsA) @{ Chassis = @(2); PCType = 1 })) 'onduleur pris pour une batterie de portable'
                Assert-Test (Test-IsLaptop @($lap) @{ Chassis = @(2); PCType = 1 }) 'vrai portable non reconnu'
                # Modèle nommé par sa seule référence sur un PC fixe : onduleur quand même
                $upsC = [pscustomobject]@{ Name = 'CP1500EPFCLCD'; DeviceID = 'CPS'; Chemistry = 2; BatteryStatus = 2 }
                Assert-Test (Test-IsUps $upsC) 'référence CyberPower non reconnue'
                Assert-Test (Test-IsDesktop @{ Chassis = @(3); PCType = 1 }) 'tour non reconnue comme PC fixe'
                Assert-Test (-not (Test-IsDesktop @{ Chassis = @(10); PCType = 2 })) 'portable pris pour un PC fixe'
                Assert-Test ($UpsVendors['051D'] -eq 'APC') 'marques USB'
                # Réglages de Windows pour la batterie, lus sur ce PC
                $pw = Get-BatteryPowerSettings
                Assert-Test ($null -ne $pw.CritAction -and $null -ne $pw.CritLevel[1]) 'réglages batterie non lus'
                # Réglages par défaut (veille prolongée à 5 %, veille prolongée coupée) : à corriger
                $bad = @{ CritAction = @(2, 2); CritLevel = @(5, 5); LowLevel = @(10, 10); LowNotify = @(1, 1); LowAction = @(0, 0) }
                $adv = Get-UpsConfigAdvice $bad $false
                Assert-Test ($adv.Fix.CritAction -eq 3 -and $adv.Fix.CritLevel -eq 25 -and $adv.Fix.LowLevel -eq 50) "conseils : $($adv.Fix.Keys -join ', ')"
                $good = @{ CritAction = @(3, 3); CritLevel = @(25, 25); LowLevel = @(50, 50); LowNotify = @(1, 1); LowAction = @(0, 0) }
                Assert-Test (-not (Get-UpsConfigAdvice $good $true).Problems.Count) 'bons réglages signalés à tort'
                # Carte complète d'un onduleur simulé : infos, réglages, usure
                $fd = New-Object System.Collections.ArrayList; $cd = New-Object System.Collections.ArrayList
                $wmi = @{ Maker = 'American Power Conversion'; Serial = '3B2212X12345'; Chem = [uint32]0x63416250; Design = 100; Full = 45; Volt = 13600; Runtime = 1260 }
                Add-UpsCards @($upsA) @() @{ BatPower = $bad; Hibernate = $false; BatWmi = $wmi } $cd $fd
                $card = $cd[0]
                Assert-Test ($card.Lines['Fabricant'] -eq 'American Power Conversion' -and $card.Lines['Batterie'] -like 'Plomb*' -and $card.Lines['Autonomie estimée'] -eq '21 min') "infos de la carte : $(($card.Lines.Keys) -join ', ')"
                Assert-Test (@($fd | Where-Object { $_.Id -in 'ups-config', 'ups-health' }).Count -eq 2) "conseils : $(($fd | ForEach-Object { $_.Id }) -join ', ')"
                Show-TestPanel @{ Tag = 'UPS'; Title = 'Onduleur simulé'; Sub = 'Carte du tableau de bord' }
                [void]$ui.TestBody.Children.Add((New-HealthCard $card)); Wait-TestMs 500; Save-TestShot 'onduleur'; Hide-TestPanel
                $h = @($script:AnalysisData.UpsHints | Where-Object { $_ })
                "onduleurs reconnus (plomb, marque, référence, PC fixe), portable toujours reconnu ; indices sur ce PC : $($h.Count)"
            }
            Test-Step 'Ce que Windows envoie à Microsoft' {
                $items = @(Get-PrivacyItems)
                Assert-Test ($items.Count -ge 8) "réglages lus : $($items.Count)"
                Assert-Test (@(Get-PcIdentifiers).Count -eq 5) 'identifiants non lus'
                Assert-Test ((Get-MsService 'v10.events.data.microsoft.com').Label -eq 'Télémétrie de Windows') 'serveur de télémétrie non reconnu'
                Assert-Test ((Get-MsService 'arc.msn.com').Label -eq 'Pubs et suggestions de Windows') 'serveur de pubs non reconnu'
                $dtPid = (Get-CimInstance Win32_Service -Filter "Name='DiagTrack'").ProcessId
                if ($dtPid) { Assert-Test (@(Get-SvcNames $dtPid | Where-Object { $_.Name -eq 'DiagTrack' }).Count -eq 1) 'service DiagTrack non retrouvé dans son svchost' }
                # Couper puis annuler un réglage : il revient exactement comme avant
                $k = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Privacy'
                $before = Get-RegState $k 'TailoredExperiencesWithDiagnosticDataEnabled'
                $script:RunLog = New-Object System.Collections.ArrayList
                & (@($items | Where-Object { $_.Id -eq 'tailored' })[0].Off)
                $log = $script:RunLog; $script:RunLog = $null
                Assert-Test ((Get-RegNum $k 'TailoredExperiencesWithDiagnosticDataEnabled') -eq 0) 'réglage non coupé'
                $errs = Undo-RunLog $log
                $after = Get-RegState $k 'TailoredExperiencesWithDiagnosticDataEnabled'
                Assert-Test (-not $errs.Count -and $after.Existed -eq $before.Existed -and "$($after.Value)" -eq "$($before.Value)") 'annulation incomplète'
                Show-WindowsPrivacy; Wait-TestMs 800; Save-TestShot 'microsoft'; Hide-TestPanel
                "$(@($items | Where-Object { $_.On }).Count) réglage(s) sur $($items.Count) envoient plus que le minimum"
            }
            Test-Step 'Lag en ligne (5 situations et vraie mesure)' {
                $a = @(); for ($i = 0; $i -lt 60; $i++) { $a += $i * 0.5; $a += $(if ($i % 20 -eq 5) { -1 } else { 10 + ($i % 3) }) }
                $ps = Get-PingStats ([double[]]$a)
                Assert-Test ($ps.Loss -eq 5 -and $ps.Med -ge 10 -and $ps.Med -le 12 -and $ps.Spikes.Count -eq 3) "statistiques de ping fausses (perte $($ps.Loss), médiane $($ps.Med), pics $($ps.Spikes.Count))"
                $good = @{ Med = 2; P95 = 4; Jit = 0.5; Loss = 0; Dead = $false }
                $ref = @{ Med = 12; P95 = 20; Jit = 2; Loss = 0; Dead = $false }
                $badRef = @{ Med = 15; P95 = 180; Jit = 25; Loss = 2; Dead = $false }
                $mk = {
                    param($gw, $rf, $srv, $extra)
                    $r = @{ Game = 'Test'; Quick = $false; Seconds = 600; Wifi = $true; Vpn = $false; Signal = 45; SignalAvg = 55; Band = '2,4 GHz'; Channel = '6'; Etw = $true
                        Server = @{ Ip = '1.2.3.4'; Port = 7000; Proto = 'udp'; Owner = 'Valve Corporation (États-Unis)' }; Isp = 'Orange (France)'
                        Stats = @{ gw = $gw; ref = $rf; srv = $srv }; Spikes = 10; LocalSpikes = 0; BgSpikes = 0; BgApps = @(); Gaps = 0; MaxGap = 0 }
                    if ($extra) { foreach ($k in $extra.Keys) { $r[$k] = $extra[$k] } }
                    Get-LagDiagnosis $r
                }
                $cases = @(
                    @('Wi-Fi', (& $mk @{ Med = 4; P95 = 80; Jit = 15; Loss = 3; Dead = $false } $ref $null), 'Le Wi-Fi fait laguer', 'bad'),
                    @('téléchargement', (& $mk $good $badRef $null @{ BgSpikes = 8; BgApps = @(@{ Name = 'Steam'; Rate = 5MB }) }), 'Un téléchargement sature ta connexion', 'bad'),
                    @('box', (& $mk $good $badRef $null), 'Ta connexion Internet sature ou décroche', 'bad'),
                    @('serveur loin', (& $mk $good $ref @{ Med = 140; P95 = 150; Jit = 2; Loss = 0; Dead = $false }), 'Le serveur du jeu est loin', 'warn'),
                    @('stable', (& $mk $good $ref @{ Med = 25; P95 = 30; Jit = 1; Loss = 0; Dead = $false }), 'Ta connexion était stable', 'ok')
                )
                foreach ($k in $cases) {
                    $titles = @($k[1].Findings | ForEach-Object { $_.Title })
                    Assert-Test ($titles -contains $k[2] -and $k[1].Level -eq $k[3]) "$($k[0]) : trouvé « $($titles -join ' / ') » ($($k[1].Level))"
                }
                # Jeu hors ligne (aucun serveur) avec une connexion stable : rien n'est gardé
                $nOff = @(Get-LagSessions).Count
                Start-LagSession 'Jeu solo' 0 0; Wait-TestMs 1500
                $script:LagSession.Start = (Get-Date).AddMinutes(-2); Stop-LagSession
                Assert-Test (@(Get-LagSessions).Count -eq $nOff -or @(Get-LagSessions)[-1].Diag.Level -ne 'ok') 'mesure d''un jeu hors ligne gardée'
                # Vraie mesure de 8 secondes
                $n0 = @(Get-LagSessions).Count
                # Un vrai jeu lancé pendant le test a déjà démarré une mesure : on l'arrête d'abord
                if ($script:LagSession) { Stop-LagSession }
                Start-LagSession 'Test rapide' 0 8
                for ($i = 0; $i -lt 150 -and $script:LagSession; $i++) { Wait-TestMs 200 }
                Assert-Test (-not $script:LagSession) 'la mesure ne s''arrête pas'
                $last = @(Get-LagSessions)[-1]
                Assert-Test (@(Get-LagSessions).Count -eq $n0 + 1 -and $last.Stats.ref.Med -gt 0) 'mesure réelle non enregistrée'
                Wait-TestMs 800; Save-TestShot 'lag-mesure'; Hide-TestPanel
                Show-Page 1; Set-GamingSubPage 'lag'; Wait-TestMs 500; Save-TestShot 'lag'
                "5 situations reconnues ; vraie mesure : box $(Format-Ms $last.Stats.gw.Med), fournisseur $(if ($last.Isp) { $last.Isp } elseif ($last.Stats.isp) { Format-Ms $last.Stats.isp.Med } else { 'non trouvé' }), Internet $(Format-Ms $last.Stats.ref.Med)"
            }
            Test-Step 'Trafic : ce qui sort du PC' {
                $ui.Tabs.SelectedIndex = $TrafficIndex
                Wait-TestMs 6000
                $st = $script:Traffic
                Assert-Test ($null -ne $st -and $script:TrafficTimer.IsEnabled) 'surveillance non démarrée'
                $real = @($st.Apps.Values | Where-Object { @($_.Dest.Values | Where-Object { -not $_.Private }).Count })
                Assert-Test ($real.Count -ge 1) 'aucun programme connecté à Internet trouvé'
                Save-TestShot 'trafic'
                # Programmes simulés : un suspect, un légitime qui envoie beaucoup, et OptiGame lui même
                $mk = {
                    param($key, $name, $path, $sig, $port, $out, $in, $self)
                    $st.Apps[$key] = @{ Key = $key; Name = $name; Path = $path; Title = $name; OutClosed = 0; InClosed = 0; Out = [double]$out; In = [double]$in; Rate = 0; LastOut = 0
                        Dest = @{ "203.0.113.9|$port" = @{ Remote = '203.0.113.9'; Port = $port; Out = [double]$out; In = [double]$in; OutClosed = 0; InClosed = 0; Live = $true; Private = $false } }
                        Ports = @{}; Udp = $false; Pids = @{}; Sig = $sig; Publisher = ''; Icon = $null; IsSelf = $self; Live = 1 }
                }
                & $mk 'test:virus' 'svch0st' 'C:\Users\x\AppData\Local\Temp\svch0st.exe' 'NotSigned' 4444 5MB 1KB $false
                & $mk 'test:onedrive' 'OneDrive' 'C:\Program Files\Microsoft OneDrive\OneDrive.exe' 'Valid' 443 900MB 10MB $false
                & $mk 'test:self' 'powershell' 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' 'Valid' 443 1MB 1MB $true
                & $mk 'test:upload' 'inconnu' 'C:\Program Files\Inconnu\inconnu.exe' 'Valid' 443 600MB 5MB $false
                $alerts = @(Get-TrafficAlerts)
                $v = @($alerts | Where-Object { $_.App.Key -eq 'test:virus' })[0]
                $u = @($alerts | Where-Object { $_.App.Key -eq 'test:upload' })[0]
                Assert-Test ($v -and $v.Level -eq 'bad') "programme suspect non signalé ($($v.Level))"
                Assert-Test ($u -and $u.Level -eq 'warn') 'gros envoi d''un programme inconnu non signalé'
                Assert-Test (-not @($alerts | Where-Object { $_.App.Key -in 'test:onedrive', 'test:self' }).Count) 'OneDrive ou OptiGame signalé à tort'
                # Jeu signé qui parle à un serveur sur le port 5555, jeu non signé dans Steam : rien à signaler
                & $mk 'test:jeu' 'Jeu' 'D:\Jeux\Jeu\Binaries\Win64\Jeu-Win64-Shipping.exe' 'Valid' 5555 1MB 50MB $false
                & $mk 'test:jeu2' 'Jeu2' 'D:\steam\steamapps\common\Jeu2\jeu2.exe' 'NotSigned' 443 1MB 50MB $false
                $alerts = @(Get-TrafficAlerts)
                Assert-Test (-not @($alerts | Where-Object { $_.App.Key -in 'test:jeu', 'test:jeu2' }).Count) 'un jeu est signalé à tort'
                # « C'est normal » : l'alerte disparaît ; analysé sans virus : « non signé » ne compte plus
                & $mk 'test:remote' 'AnyDesk' 'C:\Program Files\AnyDesk\AnyDesk.exe' 'Valid' 443 1MB 1MB $false
                & $mk 'test:nonsigne' 'outil' 'C:\Outils\outil.exe' 'NotSigned' 443 1MB 1MB $false
                Assert-Test (@(Get-TrafficAlerts | Where-Object { $_.App.Key -in 'test:remote', 'test:nonsigne' }).Count -eq 2) 'alertes de départ absentes'
                Set-TrafficMark 'TrafficTrusted' 'test:remote' $true
                Set-TrafficMark 'TrafficScanned' 'test:nonsigne' $true
                Assert-Test (-not @(Get-TrafficAlerts | Where-Object { $_.App.Key -in 'test:remote', 'test:nonsigne' }).Count) 'les alertes restent après « C''est normal » ou une analyse propre'
                Set-TrafficMark 'TrafficTrusted' 'test:remote' $false
                Assert-Test (@(Get-TrafficAlerts | Where-Object { $_.App.Key -eq 'test:remote' }).Count -eq 1) 'retirer la confiance ne remet pas l''alerte'
                Set-TrafficMark 'TrafficScanned' 'test:nonsigne' $false
                # PowerShell lancé par un programme signé (Claude Code) : information ; lancé par un inconnu : à vérifier
                & $mk 'test:ps1' 'powershell' 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' 'Valid' 443 1MB 1MB $false
                $st.Apps['test:ps1'].Parent = 'claude.exe'; $st.Apps['test:ps1'].ParentSig = 'Valid'; $st.Apps['test:ps1'].ParentPub = 'Anthropic, PBC'
                $lv = @(Get-TrafficAlerts | Where-Object { $_.App.Key -eq 'test:ps1' })[0].Level
                Assert-Test ($lv -eq 'info') "PowerShell lancé par Claude Code : niveau $lv (attendu info)"
                $st.Apps['test:ps1'].Parent = 'bizarre.exe'; $st.Apps['test:ps1'].ParentSig = 'NotSigned'
                $lv = @(Get-TrafficAlerts | Where-Object { $_.App.Key -eq 'test:ps1' })[0].Level
                Assert-Test ($lv -eq 'warn') "PowerShell lancé par un inconnu : niveau $lv (attendu warn)"
                # Vérification ratée : pas « non signé »
                & $mk 'test:err' 'gros' 'C:\Program Files\Gros\gros.exe' 'Unknown' 443 1MB 1MB $false
                Assert-Test (-not @(Get-TrafficAlerts | Where-Object { $_.App.Key -eq 'test:err' }).Count) 'vérification ratée prise pour « non signé »'
                $st.Apps.Remove('test:ps1'); $st.Apps.Remove('test:err')
                $signedReal = @($st.Apps.Values | Where-Object { $_.Sig -eq 'Valid' }).Count
                Assert-Test ($signedReal -ge 1) 'aucune signature lue en arrière plan'
                foreach ($k in 'test:jeu', 'test:jeu2', 'test:remote', 'test:nonsigne') { $st.Apps.Remove($k) }
                $alerts = @(Get-TrafficAlerts)
                $st.Alerts = $alerts
                $script:TrafficAlertKeys = $null
                Update-TrafficView; Wait-TestMs 400; Save-TestShot 'trafic-alertes'
                Show-TrafficApp 'test:virus'; Wait-TestMs 800; Save-TestShot 'trafic-fiche'; Hide-TestPanel
                # Type de données d'après le serveur
                foreach ($cas in @(@('vortex.data.microsoft.com', 'telemetry'), @('securepubads.g.doubleclick.net', 'ads'), @('gateway.discord.gg', 'chat'), @('ocsp.digicert.com', 'cert'),
                                   @('api.anthropic.com', 'ai'), @('login.live.com', 'auth'), @('rr3---sn-25ge7nsd.googlevideo.com', 'stream'), @('api.steampowered.com', 'game'),
                                   @('my.microsoftpersonalcontent.com', 'update'), @('onedrive.live.com', 'sync'), @('ec2-3-1-2-3.compute.amazonaws.com', 'cloud'))) {
                    $got = (Get-DestType $cas[0] 443 1KB 1KB $null).Id
                    Assert-Test ($got -eq $cas[1]) "$($cas[0]) classé $got (attendu $($cas[1]))"
                }
                Assert-Test ((Get-DestType '' 443 900MB 1MB $null).Id -eq 'upload') 'gros envoi sans nom non repéré'
                Assert-Test ((Get-DestType '' 53 1KB 1KB $null).Id -eq 'dns') 'DNS non repéré'
                Assert-Test ((Get-DestType '' 443 1KB 1KB @{ Name = 'AnyDesk' }).Id -eq 'remote') 'AnyDesk sans nom de serveur non reconnu'
                $st.Dns['198.51.100.7'] = 'vortex.data.microsoft.com'
                $st.Dns['198.51.100.8'] = 'gateway.discord.gg'
                $st.Apps['test:types'] = @{ Key = 'test:types'; Name = 'Discord'; Path = 'C:\x\Discord.exe'; Title = 'Discord'; Out = [double]3MB; In = [double]20MB; Rate = 0; Sig = 'Valid'; Publisher = 'Discord Inc.'; Icon = $null; IsSelf = $false; Live = 2; Udp = $true; Ports = @{}; Pids = @{}
                    Dest = @{ a = @{ Remote = '198.51.100.7'; Port = 443; Out = [double]1MB; In = [double]1KB; Live = $true; Private = $false }; b = @{ Remote = '198.51.100.8'; Port = 443; Out = [double]2MB; In = [double]20MB; Live = $true; Private = $false }
                              c = @{ Remote = '192.168.1.1'; Port = 80; Out = [double]1KB; In = [double]1KB; Live = $false; Private = $true } } }
                $sum = @(Get-AppDataTypes $st $st.Apps['test:types'])
                Assert-Test ($sum.Count -eq 2 -and $sum[0].Type.Id -eq 'chat' -and $sum[1].Type.Id -eq 'telemetry') "résumé des types faux ($(($sum | ForEach-Object { $_.Type.Id }) -join ','))"
                Show-TrafficApp 'test:types'; Wait-TestMs 800; Save-TestShot 'trafic-types'; Hide-TestPanel
                $st.Apps.Remove('test:types')
                # Serveurs sans nom : propriétaire trouvé dans l'annuaire, puis mémorisé par plage d'adresses
                $lp = [PowerShell]::Create(); $lp.RunspacePool = $script:Pool; [void]$lp.AddScript($ServerLookupWork.ToString()).AddArgument(@('155.133.248.34')); $lh = $lp.BeginInvoke()
                for ($i = 0; $i -lt 75 -and -not $lh.IsCompleted; $i++) { Wait-TestMs 200 }
                $one = if ($lh.IsCompleted) { @($lp.EndInvoke($lh))[0] } else { @{ Ok = $false } }; $lp.Dispose()
                if ($one.Ok) { Assert-Test ($one.O -match 'Valve' -and $one.S -and $one.E) "annuaire : propriétaire lu « $($one.O) »" }
                [void](Get-ServerCache).Add(@{ S = (ConvertTo-IpHex '198.51.100.0'); E = (ConvertTo-IpHex '198.51.100.255'); O = 'Valve Corporation'; C = 'US'; N = 'VALVE'; D = (Get-Date).ToString('yyyy-MM-dd') })
                $ow = Get-ServerOwner '198.51.100.42'
                Assert-Test ($ow -and (Get-OwnerLabel $ow) -eq 'Valve Corporation (États-Unis)') "propriétaire mal retrouvé ($(if ($ow) { Get-OwnerLabel $ow }))"
                Assert-Test ((Get-DestType '' 27015 1KB 1KB $null $ow).Id -eq 'game') 'serveur Valve non classé en jeu'
                Assert-Test ((Get-DestType '' 443 1KB 1KB $null @{ O = 'Google LLC'; N = 'GOOGLE' }).Id -eq 'owned') 'serveur Google sans nom mal classé'
                Assert-Test (-not (Get-ServerOwner '203.0.113.200')) 'propriétaire inventé'
                Save-ServerCache; $script:ServerCache = $null
                Assert-Test ([bool](Get-ServerOwner '198.51.100.9')) 'annuaire non gardé sur le disque'
                $lookup = "annuaire : $(if ($one.Ok) { $one.O } else { 'injoignable' })"
                foreach ($k in 'test:virus', 'test:onedrive', 'test:self', 'test:upload') { $st.Apps.Remove($k) }
                $errs = Undo-RunLog @(@{ Type = 'fw'; Name = 'OptiGame : bloque règle inexistante (test)' })
                Stop-TrafficWatch
                "$($real.Count) programmes connectés vus en vrai, alertes simulées correctes, $lookup, comptage des octets : $(if ([TrafficMon]::CountersOk) { 'actif' } else { 'indisponible sans droits admin' })"
            }
            Test-Step 'Page Tests' {
                $ui.Tabs.SelectedIndex = 5; Wait-TestMs 1500; Save-TestShot 'tests'
                Assert-Test ($ui.TestsPanel.Children.Count -ge 4) "seulement $($ui.TestsPanel.Children.Count) tuiles"
                "$($ui.TestsPanel.Children.Count) tuiles"
            }
            Test-Step 'Page Sécurité' {
                $ui.Tabs.SelectedIndex = 6; Wait-TestMs 800; Save-TestShot 'securite'
                Assert-Test ($null -ne $script:SecurityScore) 'pas de note de protection'
                "protection $($script:SecurityScore) sur 100"
            }
            Test-Step 'Page Réseau (sans scan automatique)' {
                $ui.Tabs.SelectedIndex = $NetIndex; Wait-TestMs 1000; Save-TestShot 'reseau'
                Assert-Test ($ui.NetDevices.Children.Count -eq 0) 'un scan s''est lancé tout seul'
                'aucun scan avant le clic'
            }
            Test-Step 'Historique et annulation' {
                # Réglage d'essai sans effet, dans une clé créée pour l'occasion
                $key = 'HKCU:\Software\OptiGameTest'
                $script:RunLog = New-Object System.Collections.ArrayList
                try { Set-Reg $key 'Essai' 1 } finally { $log = $script:RunLog; $script:RunLog = $null }
                $id = Add-History 'Réglage d''essai' @() $log
                Import-History
                $h = @($script:History | Where-Object { $_.Id -eq $id })[0]
                Assert-Test ($null -ne $h) 'ligne absente après relecture'
                $errs = Undo-RunLog $h.Log
                $left = (Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue).Essai
                Remove-Item -LiteralPath $key -Recurse -Force -ErrorAction SilentlyContinue
                Assert-Test (-not @($errs).Count) "erreurs : $($errs -join ', ')"
                Assert-Test ($null -eq $left) 'la valeur n''a pas été annulée'
                Set-HistoryUndone $id
                $ui.Tabs.SelectedIndex = 7; Wait-TestMs 600; Save-TestShot 'sauvegarde-historique'
                Assert-Test ($ui.HistoryPanel.Children.Count -ge 1) 'historique vide à l''écran'
                'écrit, relu et annulé'
            }
            Test-Step 'Signaler un problème' {
                $zip = Export-ProblemReport $script:T.Dir
                Assert-Test (Test-Path -LiteralPath $zip) 'fichier non créé'
                Add-Type -AssemblyName System.IO.Compression.FileSystem
                $z = [IO.Compression.ZipFile]::OpenRead($zip)
                try {
                    $names = @($z.Entries | ForEach-Object { $_.Name })
                    $rd = New-Object IO.StreamReader ($z.GetEntry('infos.txt').Open())
                    $txt = $rd.ReadToEnd(); $rd.Close()
                } finally { $z.Dispose() }
                Assert-Test ($names -contains 'journal.txt' -and $names -contains 'erreurs.txt') "contenu : $($names -join ', ')"
                Assert-Test ($txt -notmatch [regex]::Escape($env:USERNAME)) 'le nom d''utilisateur apparaît'
                "$($names.Count) fichiers"
            }
            Test-Step 'Réduire dans la zone de notification' {
                $Window.WindowState = 'Minimized'; Wait-TestMs 500
                Assert-Test (-not $Window.IsVisible) 'la fenêtre reste dans la barre des tâches'
                Assert-Test ($script:FakeTray.Visible) 'pas d''icône près de l''horloge'
                Show-MainWindow; Wait-TestMs 500
                Assert-Test ($Window.IsVisible -and $Window.WindowState -eq 'Normal') 'la fenêtre ne revient pas'
                # Agrandie avant d'être réduite : elle doit revenir agrandie
                $Window.WindowState = 'Maximized'; Wait-TestMs 300
                $Window.WindowState = 'Minimized'; Wait-TestMs 400
                Show-MainWindow; Wait-TestMs 400
                $maxOk = $Window.WindowState -eq 'Maximized'
                $Window.WindowState = 'Normal'; Wait-TestMs 300
                Assert-Test $maxOk 'agrandie avant, mais revenue en taille normale'
                'réduite près de l''horloge puis rouverte (agrandie comme avant)'
            }
            Test-Step 'Retour à l''accueil' {
                Show-Page $HubIndex; Wait-TestMs 500
                Assert-Test ($ui.Tabs.SelectedIndex -eq $HubIndex) 'accueil non affiché'
            }
            if ($script:T.Complet) {
                Test-Step 'Lag en charge (bufferbloat)' {
                    $ui.Tabs.SelectedIndex = 5; Wait-TestMs 300
                    $b = @($script:TestButtons | Where-Object { $_.Content -eq 'Lag en charge' })[0]
                    Assert-Test ($null -ne $b) 'bouton absent'
                    Test-Bufferbloat $b.Tag.T $b.Tag.Ctx
                    Wait-TestMs 1500; Save-TestShot 'lag-en-charge'
                    Hide-TestPanel
                    $r = $b.Tag.T.Last.Res.R
                    Assert-Test ($r -and $r[0] -ge 0) 'pas de résultat'
                    '{0:N0} ms au repos, {1:N0} ms en téléchargement, {2:N0} ms en envoi' -f $r[0], $r[1], $r[2]
                }
                $ui.Tabs.SelectedIndex = $NetIndex; Wait-TestMs 300
                Test-Step 'Scan du réseau' {
                    Invoke-NetworkScan
                    $l = @($script:NetList)
                    Assert-Test ($l.Count -ge 2) "seulement $($l.Count) appareil(s)"
                    Assert-Test ([bool]($l | Where-Object { $_.Self })) 'ce PC absent de la liste'
                    Assert-Test ([bool]($l | Where-Object { $_.Gateway })) 'box absente de la liste'
                    Save-TestShot 'reseau-scan'
                    $info = @($l | Where-Object { -not $_.Self } | ForEach-Object { "$($_.Title) [$($_.KindInfo.Kind)$(if ($_.Model) { ', ' + $_.Model })$(if ($_.Hidden) { ', discret' })$(if ($_.Camera) { ', CAMÉRA' })]" })
                    "$($l.Count) appareils : $($info -join ' ; ')"
                }
                Test-Step 'Surveillance des nouveaux appareils' {
                    $before = @($script:T.Msgs | Where-Object { $_ -like '`[Notify`]*' }).Count
                    Invoke-NetWatch
                    $n = @($script:T.Msgs | Where-Object { $_ -like '`[Notify`]*' }).Count - $before
                    "scan discret fait, $n notification(s)"
                }
                Test-Step 'Fiche d''un appareil' {
                    $gw = @($script:NetList | Where-Object { $_.Gateway })[0]
                    Show-DeviceDetail $gw
                    Wait-TestMs 3000; Save-TestShot 'fiche-appareil'
                    $sent = if ($script:DevPing) { $script:DevPing.Sent } else { 0 }
                    Hide-TestPanel
                    Assert-Test ($sent -ge 3) "ping en direct : $sent envoi(s)"
                    "$sent pings"
                }
                Test-Step 'Audit de sécurité' {
                    Invoke-NetAudit
                    Wait-TestMs 800; Save-TestShot 'audit'
                    Hide-TestPanel
                    $a = $script:NetAudit
                    Assert-Test ($a -and $a.Score -ge 0 -and $a.Score -le 100) 'pas de note'
                    Assert-Test (@($a.Checks).Count -ge 8) "seulement $(@($a.Checks).Count) points"
                    "note $($a.Score), $(@($a.Checks).Count) points"
                }
            }
            $script:T.Watch = $false
            $gap = [int]$script:T.MaxGap
            Add-TestResult 'Fluidité' ($gap -lt 3000) "plus long blocage de la fenêtre : $gap ms (étape « $($script:T.GapStep) ») ; tâches répétées les plus lentes : $(($script:T.Slow.GetEnumerator() | Sort-Object Value -Descending | ForEach-Object { "$($_.Key) $($_.Value) ms" }) -join ', ')"
        }
        $errs = @()
        if (Test-Path -LiteralPath $LogFile) { $errs = @(Get-Content -LiteralPath $LogFile -Encoding UTF8 | Where-Object { $_ -match 'ERREUR|Échec' }) }
        Add-TestResult 'Journal sans erreur' (-not $errs.Count) ($errs -join ' | ')
    } catch {
        Add-TestResult 'Déroulement du test' $false "$($_.Exception.Message) $($_.InvocationInfo.PositionMessage)"
    }
    @{ Resultats = @($script:T.Res); Messages = @($script:T.Msgs) } | ConvertTo-Json -Depth 4 |
        Set-Content -LiteralPath (Join-Path $script:T.Dir 'resultats.json') -Encoding UTF8
    $Window.Close()
})
$script:T.Run.Start()
$Window.Show()
[System.Windows.Threading.Dispatcher]::Run()
[Environment]::Exit(0)
