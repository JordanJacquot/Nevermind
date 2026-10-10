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
                Assert-Test ($ui.HubCards.Children.Count -eq 3) "$($ui.HubCards.Children.Count) familles au lieu de 3"
                Assert-Test ($script:HubStats.Count -eq 8) "seulement $($script:HubStats.Count) outils"
                Assert-Test ($ui.HubHello.Text -like ((Get-ThemeText 'Hello') -f '*')) "bonjour : $($ui.HubHello.Text)"
                Assert-Test ($ui.HubTodoPanel.Children.Count -ge 2) 'carte « À faire » vide'
                "3 familles, 8 outils, « $($ui.HubHello.Text) »"
            }
            Test-Step 'Accueil : carte mise en avant (3 cas)' {
                if ($null -eq $script:Games) { Update-GameCache }
                $tagOf = { (Get-TextBlockText $ui.HubTodoPanel.Children[0]) }
                $manualF = @{ Id = 'test:manuel'; Status = 'warn'; Titre = 'Pilote à installer (test)'; Detail = 'x'; Gain = 4; Fix = @{ Auto = $false } }
                $autoF = @{ Id = 'test:auto'; Status = 'warn'; Titre = 'Réglage en un clic (test)'; Detail = 'x'; Gain = 2; Fix = @{ Auto = $true } }
                $realLog = $script:PlayLog
                $game = @(Get-LibraryGames)[0]
                try {
                    # 1. Une correction en un clic : « À faire », le pilote en petite ligne
                    Update-HubTodo @{ Active = @($manualF, $autoF); Score = 90 }
                    Assert-Test ((& $tagOf) -eq 'À FAIRE') "étiquette « $(& $tagOf) » au lieu de À FAIRE"
                    Assert-Test ((Get-TextBlockText $ui.HubTodoPanel.Children[1]) -like 'Réglage en un clic*') 'la correction en un clic n''est pas en avant'
                    Assert-Test ($ui.HubTodoPanel.Children[-1] -is [System.Windows.Controls.Border]) 'pas de petite ligne pour le pilote'
                    # 2. Seulement du manuel et un jeu lancé récemment : « Ta dernière partie »
                    $s1 = 'pas de jeu installé'
                    if ($game) {
                        $script:PlayLog = @{ $game.Name = @{ Last = (Get-Date).ToString('s'); Seconds = 4000; Count = 3 } }
                        Update-HubTodo @{ Active = @($manualF); Score = 96 }
                        Assert-Test ((& $tagOf) -eq 'TA DERNIÈRE PARTIE') "étiquette « $(& $tagOf) » au lieu de TA DERNIÈRE PARTIE"
                        Assert-Test ((Get-TextBlockText $ui.HubTodoPanel.Children[1]) -eq $game.Name) 'mauvais jeu affiché'
                        Assert-Test ($ui.HubTodoPanel.Children[-1] -is [System.Windows.Controls.Border]) 'le pilote a disparu'
                        Wait-TestMs 300; Save-TestShot 'accueil-derniere-partie'
                        $s1 = "dernière partie : $($game.Name)$(if ($ui.HubTodoArt.Background) { ' (avec image)' })"
                    }
                    # 3. Rien joué, rien à faire : « Prêt à jouer »
                    $script:PlayLog = @{}
                    Update-HubTodo @{ Active = @(); Score = 100 }
                    Assert-Test ((& $tagOf) -eq 'PRÊT À JOUER') "étiquette « $(& $tagOf) » au lieu de PRÊT À JOUER"
                    Assert-Test (-not $ui.HubTodoArt.Background) 'image de jeu restée'
                } finally { $script:PlayLog = $realLog; Update-Hub }
                "à faire, $s1, prêt à jouer"
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
                Show-Page $OverlayIndex; Wait-TestMs 300; Save-TestShot 'overlay-page-discret'
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
            Test-Step 'Onglet Overlay' {
                $oldOn = Test-FpsOverlay; $oldStyle = Get-FpsOverlayStyle; $oldCorner = Get-FpsOverlayCorner; $oldMeasure = Test-FpsMeasure
                try {
                    Set-Setting 'FpsMeasure' $true; Set-Setting 'FpsOverlay' $true
                    Assert-Test ($ui.Tabs.Items[$OverlayIndex].Visibility -eq 'Visible') 'onglet Overlay absent de la barre'
                    Show-Page $OverlayIndex; Wait-TestMs 500
                    Assert-Test ($ui.Tabs.SelectedIndex -eq $OverlayIndex -and $ui.OverlayPanel.Children.Count -ge 3) 'page Overlay vide'
                    foreach ($a in 'Afficher le compteur pendant la partie', 'Style du compteur', 'Position du compteur', 'Raccourci Ctrl + Maj + F') {
                        Assert-Test ([bool](Find-PageElement $ui.Tabs.Items[$OverlayIndex].Content $a)) "« $a » absent de la page"
                    }
                    Assert-Test (-not ($GamingSubPages | Where-Object { $_.Id -eq 'overlay' })) 'sous-onglet Overlay encore dans Optimisation gaming'
                    # L'aperçu bouge tout seul
                    $v1 = $script:OverlayPreview.Fps.Text; Wait-TestMs 1400; $v2 = $script:OverlayPreview.Fps.Text
                    Assert-Test ($script:OverlayPreviewTimer.IsEnabled) 'aperçu figé'
                    Save-TestShot 'overlay-onglet'
                    # Coin et style changés depuis la page : réglages enregistrés, aperçu à jour
                    Set-FpsOverlayCorner 'bd'; Wait-TestMs 200
                    Assert-Test ((Get-FpsOverlayCorner) -eq 'bd' -and $script:OverlayPreview.Root.HorizontalAlignment -eq 'Right' -and $script:OverlayPreview.Root.VerticalAlignment -eq 'Bottom') 'coin non appliqué à l''aperçu'
                    Set-FpsOverlayStyle 'discret'; Wait-TestMs 200
                    Assert-Test ($script:OverlayPreview.Discreet) 'style discret non appliqué à l''aperçu'
                    Save-TestShot 'overlay-onglet-discret'
                    # Mesure coupée : avertissement et bouton pour la réactiver
                    Set-Setting 'FpsMeasure' $false; Build-OverlayPanel
                    Assert-Test ([bool](Find-PageElement $ui.OverlayPanel 'Activer la mesure')) 'pas de bouton pour réactiver la mesure'
                    # En quittant l'onglet, l'aperçu s'arrête
                    Show-Page $HubIndex; Wait-TestMs 700
                    Assert-Test (-not $script:OverlayPreviewTimer.IsEnabled) 'aperçu toujours animé hors de l''onglet'
                } finally {
                    Set-Setting 'FpsMeasure' $oldMeasure; Set-Setting 'FpsOverlay' $oldOn; Set-Setting 'FpsOverlayStyle' $oldStyle; Set-Setting 'FpsOverlayCorner' $oldCorner
                    Build-OverlayPanel
                }
                "page complète, aperçu animé ($v1 puis $v2), coin et style appliqués, mesure coupée signalée"
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
            Test-Step 'FPS : menus bloqués à 60 pas pris pour des chutes' {
                # Parties simulées, image par image, comme PresentMon les envoie
                $rnd = New-Object Random 7
                $feed = {
                    param($parts)
                    [FrameMon]::Reset(); [FrameMon]::Paused = $false
                    [FrameMon]::Feed('Application,ProcessID,MsBetweenPresents')
                    foreach ($p in $parts) {
                        $ms = 0.0
                        while ($ms -lt $p.Sec * 1000) {
                            $ft = switch ($p.Kind) { 'jeu' { 5.0 + ($rnd.NextDouble() - 0.5) * 1.6 } 'menu' { 16.667 + ($rnd.NextDouble() - 0.5) * 0.1 } 'chute' { 18 + $rnd.NextDouble() * 30 } }
                            [FrameMon]::Feed("jeu.exe,1,$($ft.ToString([Globalization.CultureInfo]::InvariantCulture))")
                            $ms += $ft
                        }
                    }
                }
                # 1. Jeu à 200 FPS avec 40 s de menu bloqué à 60 : pas de chute
                & $feed @(@{ Kind = 'jeu'; Sec = 50 }, @{ Kind = 'menu'; Sec = 40 }, @{ Kind = 'jeu'; Sec = 50 })
                $s = [FrameMon]::Summary(); $pl = [FrameMon]::Plateau(); $b = [FrameMon]::Busy()
                $sess = @{ Avg = $s[0]; Low1 = $s[1]; Seconds = $s[4]; Diag = @{ Stutters = $b[2] } }
                Assert-Test ([math]::Abs($pl[0] - 40) -lt 3 -and $pl[1] -eq 60) "palier vu : $([int]$pl[0]) s à $($pl[1]) FPS (attendu 40 s à 60)"
                Assert-Test (-not (Test-FpsProblem $sess)) "menu pris pour une chute : moyenne $([int]$s[0]), 1 % bas $([int]$s[1])"
                $menu = "menu de 40 s à 60 mis à part (moyenne $([int]$s[0]), 1 % bas $([int]$s[1]))"
                # 2. Vraie chute en jeu (images irrégulières) : toujours signalée
                & $feed @(@{ Kind = 'jeu'; Sec = 50 }, @{ Kind = 'chute'; Sec = 10 }, @{ Kind = 'jeu'; Sec = 50 })
                $s = [FrameMon]::Summary(); $b = [FrameMon]::Busy()
                Assert-Test (Test-FpsProblem @{ Avg = $s[0]; Low1 = $s[1]; Seconds = $s[4]; Diag = @{ Stutters = $b[2] } }) "vraie chute non vue : moyenne $([int]$s[0]), 1 % bas $([int]$s[1])"
                # 3. Jeu bloqué à 60 du début à la fin : rien n'est retiré, moyenne 60
                & $feed @(@{ Kind = 'menu'; Sec = 60 })
                $s = [FrameMon]::Summary(); $pl = [FrameMon]::Plateau()
                Assert-Test ([math]::Abs($s[0] - 60) -lt 1 -and $pl[0] -eq 0) "partie bloquée à 60 : moyenne $([int]$s[0]), palier $([int]$pl[0]) s"
                [FrameMon]::Reset()
                "$menu ; vraie chute toujours signalée ; partie entière à 60 gardée"
            }
            Test-Step 'Jeux de tous les launchers et jeu ajouté' {
                if ($null -eq $script:Games) { Update-GameCache }
                $by = @($script:Games | Group-Object { if ($_.Source) { $_.Source } else { 'Steam' } } | ForEach-Object { "$($_.Name) $($_.Count)" })
                # Aucun launcher ne doit passer pour un jeu
                $lnch = @($script:Games | Where-Object { $_.Name -match '^(Battle\.net|Ubisoft Connect|EA app|Riot Client|Riot Vanguard|GOG GALAXY|Rockstar Games Launcher)$' })
                Assert-Test (-not $lnch.Count) "launcher pris pour un jeu : $(($lnch | ForEach-Object { $_.Name }) -join ', ')"
                $badExe = @($script:Games | ForEach-Object { @($_.Exes) } | Where-Object { [IO.Path]::GetFileNameWithoutExtension($_) -match 'launcher|uninst|crash' })
                Assert-Test (-not $badExe.Count) "exécutable non jeu gardé : $(($badExe | Select-Object -First 3) -join ', ')"
                # Jeu ajouté à la main : reconnu quand il tourne (copie de powershell sous un nom de jeu)
                $dir = Join-Path $DataDir 'jeu-essai'
                New-Item -ItemType Directory -Force -Path $dir | Out-Null
                $exe = Join-Path $dir 'MonJeuEssai.exe'
                Copy-Item "$PSHOME\powershell.exe" $exe -Force
                Add-CustomGame $exe 'Mon jeu d''essai'
                Assert-Test (@($script:Games | Where-Object { $_.Name -eq 'Mon jeu d''essai' -and $_.Custom }).Count -and $script:GameIndex.ContainsKey('monjeuessai')) 'jeu ajouté absent de la liste'
                Set-GamingSubPage 'profiles'; Wait-TestMs 300; Save-TestShot 'jeux-launchers'
                $p = Start-Process $exe -ArgumentList '-NoProfile', '-Command', 'Start-Sleep 20' -WindowStyle Hidden -PassThru
                try {
                    Wait-TestMs 800
                    $old = $script:GameSession; $script:GameSession = $null
                    # Seulement le jeu d'essai : une vraie partie en cours sur le PC ne doit pas passer devant
                    $idx = $script:GameIndex; $script:GameIndex = @{ monjeuessai = $idx['monjeuessai'] }
                    $fm = Get-Setting 'FpsMeasure' $false; $lm = Get-Setting 'LagMeasure' $true
                    Set-Setting 'FpsMeasure' $false; Set-Setting 'LagMeasure' $false
                    Test-GameRunning
                    $seen = $script:GameSession -and $script:GameSession.Game -eq 'Mon jeu d''essai'
                    if ($script:GameSession) { $script:GameSession.Closed = @(); Stop-GameSession }
                    $script:GameIndex = $idx; $script:GameSession = $old
                    Set-Setting 'FpsMeasure' $fm; Set-Setting 'LagMeasure' $lm
                } finally { try { $p.Kill() } catch {} }
                Assert-Test $seen 'jeu ajouté non reconnu à son lancement'
                Remove-CustomGame $exe
                Assert-Test (-not @($script:Games | Where-Object { $_.Name -eq 'Mon jeu d''essai' }).Count) 'jeu ajouté non retiré'
                "$(@($script:Games).Count) jeux ($($by -join ', ')), aucun launcher pris pour un jeu ; jeu ajouté reconnu à son lancement puis retiré"
            }
            Test-Step 'Organizer Dofus' {
                $oldCfg = $script:OrgConfig
                try {
                    # Titres de Dofus 3, Dofus 2 et de l'écran de connexion
                    $t3 = ConvertFrom-DofusTitle 'Brakmar-Iop - Iop - 3.1.12.5 - Release'
                    $t2 = ConvertFrom-DofusTitle 'Vieux-Cra - Dofus 2.71.4.10'
                    $t0 = ConvertFrom-DofusTitle 'Dofus'
                    Assert-Test ($t3.Name -eq 'Brakmar-Iop' -and $t3.Class -eq 'Iop' -and $t2.Name -eq 'Vieux-Cra' -and -not $t2.Class -and -not $t0.Name) "titres mal lus : $($t3.Name)/$($t3.Class), $($t2.Name)/$($t2.Class), $($t0.Name)"
                    # Trois persos connectés et une fenêtre à l'écran de connexion (fenêtres factices)
                    if (Test-Path -LiteralPath $OrgFile) { [IO.File]::Delete($OrgFile) }
                    $script:OrgConfig = $null
                    $script:OrgFake = @(
                        @{ Name = 'Brakmar-Iop'; Class = 'Iop'; Pid = 11; Hwnd = [IntPtr]1001; Title = '' },
                        @{ Name = 'Soin-Eni'; Class = 'Eniripsa'; Pid = 12; Hwnd = [IntPtr]1002; Title = '' },
                        @{ Name = 'Vieux-Cra'; Class = ''; Pid = 13; Hwnd = [IntPtr]1003; Title = '' },
                        @{ Name = ''; Class = ''; Pid = 14; Hwnd = [IntPtr]1004; Title = 'Dofus' })
                    $l = @(Get-OrgList)
                    Assert-Test ($l.Count -eq 4 -and (Get-OrgConfig).Order.Count -eq 3) "liste : $($l.Count) lignes, $((Get-OrgConfig).Order.Count) persos retenus"
                    # Ordre d'initiative : le Crâ passe en premier, retenu dans organizer.json
                    Move-OrgChar 'Vieux-Cra' 'Brakmar-Iop'
                    $script:OrgConfig = $null
                    Assert-Test (((Get-OrgConfig).Order -join ',') -eq 'Vieux-Cra,Brakmar-Iop,Soin-Eni') "ordre : $((Get-OrgConfig).Order -join ',')"
                    Move-OrgChar 'Vieux-Cra' 'Soin-Eni'
                    Assert-Test (((Get-OrgConfig).Order -join ',') -eq 'Brakmar-Iop,Soin-Eni,Vieux-Cra') "ordre en descendant : $((Get-OrgConfig).Order -join ',')"
                    # Suivant, précédent et touche par perso, depuis la fenêtre du Iop
                    $script:OrgFakeFg = [IntPtr]1001
                    Assert-Test ((Get-OrgTarget 'Next').Name -eq 'Soin-Eni' -and (Get-OrgTarget 'Prev').Name -eq 'Vieux-Cra' -and (Get-OrgTarget 'P3').Name -eq 'Vieux-Cra' -and -not (Get-OrgTarget 'P5')) 'mauvais perso choisi'
                    $script:OrgFakeFg = [IntPtr]1003
                    Assert-Test ((Get-OrgTarget 'Next').Name -eq 'Brakmar-Iop') 'le suivant du dernier n''est pas le premier'
                    # Logo de classe : retrouvé depuis le nom (accents compris), pris dans le dossier de Nevermind
                    Assert-Test ((Get-OrgBreedId 'Crâ') -eq 9 -and (Get-OrgBreedId 'Xélor') -eq 5 -and (Get-OrgBreedId 'Forgelance') -eq 20 -and -not (Get-OrgBreedId 'Inconnu')) 'classes mal reconnues'
                    $iconFile = Join-Path $OrgIconDir 'classe-8.png'
                    if (-not (Test-Path -LiteralPath $OrgIconDir)) { New-Item -ItemType Directory -Force -Path $OrgIconDir | Out-Null }
                    Add-Type -AssemblyName System.Drawing; $bmp = New-Object System.Drawing.Bitmap 16, 16; $bmp.SetPixel(8, 8, [System.Drawing.Color]::Red); $bmp.Save($iconFile, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
                    $bIop = New-OrgBadge @{ Name = 'Brakmar-Iop'; Class = 'Iop' } 30; $bEni = New-OrgBadge @{ Name = 'Soin-Eni'; Class = 'Eniripsa' } 30
                    Assert-Test ($bIop.Children[0] -is [System.Windows.Controls.Image] -and $bEni.Children[0] -is [System.Windows.Shapes.Ellipse]) 'logo de classe non affiché (ou affiché sans fichier)'
                    [IO.File]::Delete($iconFile)
                    # Touches
                    $hk = ConvertTo-OrgHotkey 'Ctrl+Maj+Tab'; $f1 = ConvertTo-OrgHotkey 'F1'
                    Assert-Test ($hk.Mods -eq 6 -and $hk.Vk -eq 9 -and $f1.Mods -eq 0 -and $f1.Vk -eq 0x70 -and -not (ConvertTo-OrgHotkey '')) 'touches mal converties'
                    Assert-Test ((Format-OrgKey 'Ctrl+D1') -eq 'Ctrl + 1') "nom de touche : $(Format-OrgKey 'Ctrl+D1')"
                    # Page : bouton de la page Jeux, persos, raccourcis, barre
                    Show-Page $GamesIndex; Wait-TestMs 300
                    $ui.BtnLibOrganizer.RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Button]::ClickEvent))); Wait-TestMs 300
                    Assert-Test ($ui.OrgScroll.Visibility -eq 'Visible' -and $ui.LibBody.Visibility -eq 'Collapsed' -and $ui.LibTitle.Text -eq 'Organizer Dofus') 'vue Organizer non affichée'
                    foreach ($a in 'Mes personnages', 'Activer l''organizer', 'Raccourcis', 'Perso suivant', 'Barre flottante') { Assert-Test ([bool](Find-PageElement $ui.OrgPanel $a)) "« $a » absent" }
                    Assert-Test ($script:OrgRows.Count -eq 3) "$($script:OrgRows.Count) lignes de persos"
                    Save-TestShot 'organizer'
                    # Touche choisie au clavier : F7 pour « suivant », retirée du perso 7
                    $script:OrgCapture = 'Next'; Build-OrgPanel
                    $ke = New-Object System.Windows.Input.KeyEventArgs ([System.Windows.Input.Keyboard]::PrimaryDevice, [System.Windows.PresentationSource]::FromVisual($Window), 0, [System.Windows.Input.Key]::F7); $ke.RoutedEvent = [System.Windows.Input.Keyboard]::PreviewKeyDownEvent
                    Receive-OrgKey $ke
                    Assert-Test ((Get-OrgConfig).Hotkeys.Next -eq 'F7' -and -not (Get-OrgConfig).Hotkeys.P7 -and -not $script:OrgCapture) "touche capturée : $((Get-OrgConfig).Hotkeys.Next), perso 7 : $((Get-OrgConfig).Hotkeys.P7)"
                    # Bouton latéral de la souris comme raccourci : capturé, compris, écouté seulement quand c'est utile
                    $m4 = ConvertTo-OrgMouse 'Ctrl+Souris4'
                    Assert-Test ($m4.Mods -eq 2 -and $m4.Button -eq 4 -and (ConvertTo-OrgMouse 'Molette').Button -eq 3 -and -not (ConvertTo-OrgMouse 'F1') -and -not (ConvertTo-OrgHotkey 'Souris4')) 'boutons de souris mal compris'
                    $script:OrgCapture = 'Prev'
                    $me = New-Object System.Windows.Input.MouseButtonEventArgs ([System.Windows.Input.Mouse]::PrimaryDevice, 0, [System.Windows.Input.MouseButton]::XButton2); $me.RoutedEvent = [System.Windows.Input.Mouse]::PreviewMouseDownEvent
                    Receive-OrgMouse $me
                    Assert-Test ((Get-OrgConfig).Hotkeys.Prev -eq 'Souris5' -and (Format-OrgKey 'Souris5') -eq 'Souris 5 (avant)') "bouton capturé : $((Get-OrgConfig).Hotkeys.Prev)"
                    Register-OrgHotkeys
                    Assert-Test ([MouseHook]::Running) 'écoute de la souris non lancée'
                    Unregister-OrgHotkeys
                    Assert-Test (-not [MouseHook]::Running) 'écoute de la souris toujours active'
                    # Touche déjà réservée par un autre programme : signalée dans la page au lieu d'échouer en silence
                    $hMain = (New-Object System.Windows.Interop.WindowInteropHelper $Window).EnsureHandle()
                    [void][OGNative]::AddHotKey($hMain, 7999, 3, 0x7A)
                    try {
                        (Get-OrgConfig).Hotkeys.P8 = 'Ctrl+Alt+F11'
                        Register-OrgHotkeys; Unregister-OrgHotkeys; Build-OrgPanel
                        Assert-Test ($script:OrgKeyFailed.ContainsKey('P8') -and [bool](Find-PageElement $ui.OrgPanel 'Déjà prise par un autre programme : choisis en une autre.')) 'touche déjà prise non signalée'
                    } finally { [OGNative]::RemoveHotKey($hMain, 7999); (Get-OrgConfig).Hotkeys.P8 = 'F8'; $script:OrgKeyFailed = @{} }
                    # Barre flottante : un bouton par perso connecté
                    Show-OrgBar; Wait-TestMs 300
                    Assert-Test ($script:OrgBar.Items.Count -eq 3 -and $script:OrgBar.Win.IsVisible) "barre : $($script:OrgBar.Items.Count) boutons"
                    $bw = $script:OrgBar.Win; $bw.UpdateLayout()
                    $rtb = New-Object System.Windows.Media.Imaging.RenderTargetBitmap([int]$bw.ActualWidth, [int]$bw.ActualHeight, 96, 96, [System.Windows.Media.PixelFormats]::Pbgra32)
                    $rtb.Render($bw.Content)
                    $enc = New-Object System.Windows.Media.Imaging.PngBitmapEncoder; $enc.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($rtb))
                    $fs = [IO.File]::Create((Join-Path $script:T.Dir 'captures\organizer-barre.png')); $enc.Save($fs); $fs.Close()
                    $nBar = $script:OrgBar.Items.Count
                    # Barre remise au dessus de tout, et ramenée si sa position est sur un écran débranché
                    Set-OrgBarOnTop
                    Assert-Test ((Test-OrgBarOnScreen 200 100) -and -not (Test-OrgBarOnScreen 50000 100) -and -not (Test-OrgBarOnScreen -50000 100) -and -not (Test-OrgBarOnScreen 200 50000)) 'position hors écran mal repérée'
                    # Activation : surveillance lancée puis arrêtée, barre cachée
                    Set-OrgOn $true; Wait-TestMs 400
                    Assert-Test ($script:OrgTimer.IsEnabled -and -not $script:OrgKeysOn) 'surveillance non lancée (ou raccourcis pris hors de Dofus)'
                    Set-OrgOn $false
                    Assert-Test (-not $script:OrgTimer.IsEnabled -and -not $script:OrgBar.Win.IsVisible) 'organizer pas arrêté'
                    # Retour à la bibliothèque
                    $ui.BtnLibOrganizer.RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Button]::ClickEvent))); Wait-TestMs 200
                    Assert-Test ($ui.LibBody.Visibility -eq 'Visible' -and $ui.LibTitle.Text -eq 'Mes jeux') 'bibliothèque non revenue'
                    "titres lus, logos de classe, ordre retenu, suivant / précédent / touche par perso, touche et bouton de souris capturés, touche déjà prise signalée, barre de $nBar persos"
                } finally {
                    $script:OrgFake = $null; $script:OrgFakeFg = $null; $script:OrgCapture = $null
                    try { Stop-OrgWatch } catch {}
                    if (Test-Path -LiteralPath $OrgFile) { [IO.File]::Delete($OrgFile) }
                    $script:OrgConfig = $oldCfg
                    if ($script:OrgViewOn) { Show-OrgView $false }
                }
            }
            Test-Step 'Bibliothèque de jeux' {
                $sw = [Diagnostics.Stopwatch]::StartNew(); Show-Page $GamesIndex; $tBuild = $sw.ElapsedMilliseconds; Wait-TestMs 600
                $sw.Restart(); Update-LibraryView; $tAgain = $sw.ElapsedMilliseconds
                Assert-Test ($tAgain -lt 1500) "rafraîchir la bibliothèque prend $tAgain ms"
                $games = @(Get-LibraryGames)
                Assert-Test ($games.Count -ge [math]::Min(2, @($script:Games).Count)) "bibliothèque : $($games.Count) jeux pour $(@($script:Games).Count) trouvés"
                Assert-Test ($script:LibBuilt -and $script:LibTiles.Count -eq $games.Count) "vignettes : $($script:LibTiles.Count) pour $($games.Count) jeux"
                # Comment chaque jeu sera lancé (sans rien lancer)
                $kinds = @{}
                foreach ($g in $games) {
                    $h = Get-GameLaunch $g
                    Assert-Test ([bool]$h.Path) "aucun moyen de lancer $($g.Name)"
                    if ($g.AppId) { Assert-Test ($h.Path -like 'steam://rungameid/*') "$($g.Name) : $($h.Path)" }
                    $kinds[$(if ($h.Kind -eq 'url') { ($h.Path -split ':')[0] } else { 'exe' })] = $true
                }
                # Jeu choisi : panneau avec Lancer et l'optimisation
                $busy = if ($script:GameSession) { $script:GameSession.Game } else { '' }
                $pick = @($games | Where-Object { $_.AppId -and $_.Name -ne $busy } | Select-Object -First 1)[0]
                if (-not $pick) { $pick = $games[0] }
                if ($pick) {
                    Set-LibrarySelection $pick.Name; Wait-TestMs 400
                    $el = Find-PageElement $ui.LibDetailPanel 'Lancer'
                    Assert-Test ($el -and (Find-PageElement $ui.LibDetailPanel 'OPTIMISATION DU JEU')) 'panneau du jeu incomplet'
                    Save-TestShot 'bibliotheque'
                    # Recherche d'un jeu par son nom : ouvre la bibliothèque sur lui
                    $r = @(Find-Settings $pick.Name)
                    Assert-Test ($r.Count -and $r[0].Game -eq $pick.Name) "recherche du jeu : $(($r | Select-Object -First 2 | ForEach-Object { $_.T }) -join ' | ')"
                    Show-Page $HubIndex; Open-SearchEntry $r[0]; Wait-TestMs 400
                    Assert-Test ($ui.Tabs.SelectedIndex -eq $GamesIndex -and $script:LibSelected -eq $pick.Name) 'la recherche n''ouvre pas le jeu'
                }
                # Désinstallation : chaque jeu a un moyen (sans rien lancer)
                $unk = @($games | Where-Object { -not $_.Custom -and (Get-GameUninstall $_).Kind -eq 'none' } | ForEach-Object { $_.Name })
                $ukinds = @($games | ForEach-Object { (Get-GameUninstall $_).Kind } | Select-Object -Unique)
                foreach ($g in @($games | Where-Object { $_.AppId })) { Assert-Test ((Get-GameUninstall $g).Path -eq "steam://uninstall/$($g.AppId)") "désinstallation Steam de $($g.Name)" }
                $sc = Split-Command '"C:\Riot Games\Riot Client\RiotClientServices.exe" --uninstall-product=valorant --uninstall-patchline=live'
                Assert-Test ($sc.Exe -eq 'C:\Riot Games\Riot Client\RiotClientServices.exe' -and $sc.Args -like '--uninstall-product=valorant*') "commande mal lue : $($sc.Exe) / $($sc.Args)"
                # Ankama (Dofus) : présent si l'Ankama Launcher a installé des jeux sur ce PC
                $ank = @(Get-ChildItem "$env:APPDATA\zaap\repositories\production\*\*\release.json" -ErrorAction SilentlyContinue | Where-Object { (Get-Content $_.FullName -Raw) -match '"location":"[A-Z]' })
                if ($ank.Count) { Assert-Test (@($games | Where-Object { $_.Source -eq 'Ankama' }).Count) 'jeux Ankama (Dofus) absents' }
                # Jaquette d'un jeu du même nom sur un autre launcher, et jaquette téléchargée
                $noCover = @($games | Where-Object { -not (Get-CoverFile $_) })
                $withCover = @($games | Where-Object { Get-CoverFile $_ })
                if ($noCover.Count -and $withCover.Count) {
                    $script:CoverIndex = @{ ($noCover[0].Name) = @{ Cover = (Get-CoverFile $withCover[0]); Hero = ''; Logo = ''; Date = (Get-Date).ToString('s') } }
                    Assert-Test ((Get-CoverFile $noCover[0]) -eq (Get-CoverFile $withCover[0])) 'jaquette téléchargée non utilisée'
                    $script:CoverIndex = $null
                }                # Jeux Steam désinstallés dont le dossier est resté : pas des jeux, signalés comme place à récupérer
                $lo = @($script:Leftovers)
                Assert-Test (-not @($games | Where-Object { $_.Leftover -or ($_.Source -eq 'Steam' -and -not $_.AppId) }).Count) 'jeu Steam sans fiche d''installation affiché'
                # Tailles calculées en arrière plan (jusqu'à 60 s pour des centaines de milliers de fichiers)
                $waited = 0
                while ($lo.Count -and $script:LeftoverJob -and $waited -lt 60000) { Wait-TestMs 500; $waited += 500 }
                $big = @(Get-BigLeftovers)
                if ($big.Count) { Assert-Test ($ui.LibLeftoverBar.Visibility -eq 'Visible' -and $ui.LibLeftoverText.Text -match 'récupérer') "bandeau des restes : $($ui.LibLeftoverText.Text)"; Save-TestShot 'bibliotheque-restes' }
                $loText = if ($big.Count) { "$($lo.Count) dossier(s) de jeux désinstallés, $($big.Count) signalé(s) : $($ui.LibLeftoverText.Text)" } else { "$($lo.Count) petit(s) reste(s) de jeux désinstallés, rien à signaler" }
                # Suppression définitive : sur un faux reste créé pour l'essai (les vrais dossiers ne sont jamais touchés)
                $fakeRoot = Join-Path $DataDir 'essai-steam\steamapps\common'
                $fake = Join-Path $fakeRoot 'Jeu desinstalle'
                New-Item -ItemType Directory -Force -Path "$fake\data\sous" | Out-Null
                foreach ($i in 1..30) { [IO.File]::WriteAllBytes("$fake\data\sous\f$i.bin", (New-Object byte[] 2000)) }
                [IO.File]::WriteAllText("$fake\lecture-seule.txt", 'x'); [IO.File]::SetAttributes("$fake\lecture-seule.txt", 'ReadOnly')
                $keep = Join-Path $fakeRoot 'Jeu installe'
                New-Item -ItemType Directory -Force -Path $keep | Out-Null
                $realLo = $script:Leftovers; $realSizes = $script:LeftoverSizes
                $script:Leftovers = @(@{ Name = 'Jeu desinstalle'; Dir = $fake; Leftover = $true })
                $script:LeftoverSizes = @{ $fake = 60MB }
                # Un dossier qui n'est pas signalé comme reste (ou hors de steamapps\common) est refusé
                $before = $script:T.Msgs.Count
                Remove-Leftovers @(@{ Name = 'Jeu installe'; Dir = $keep }) -Force
                Assert-Test ((Test-Path $keep) -and -not $script:LeftoverDelete) 'un dossier non signalé a été supprimé'
                $script:T.Msgs.RemoveRange($before, $script:T.Msgs.Count - $before)
                Remove-Leftovers @($script:Leftovers[0]) -Force
                $w = 0; while ($script:LeftoverDelete -and $w -lt 20000) { Wait-TestMs 300; $w += 300 }
                if ($ui.Overlay.Visibility -eq 'Visible') { Close-Sheet }
                Assert-Test (-not (Test-Path $fake) -and (Test-Path $keep)) "faux reste non supprimé (ou mauvais dossier touché)"
                Assert-Test (-not @($script:Leftovers).Count -and $script:LastLeftoverFreed -eq 60MB) 'liste des restes non mise à jour'
                $script:Leftovers = $realLo; $script:LeftoverSizes = $realSizes
                [IO.Directory]::Delete((Join-Path $DataDir 'essai-steam'), $true)
                # Jaquettes des jeux Ankama : fournies par leur launcher
                foreach ($g in @($games | Where-Object { $_.Source -eq 'Ankama' })) { Assert-Test ([bool](Get-CoverFile $g)) "pas de jaquette pour $($g.Name)" }
                # Filtre et recherche dans la bibliothèque
                $ui.LibSearch.Text = 'zzzz'; Wait-TestMs 500
                Assert-Test ($script:LibTiles.Count -eq 0) 'filtre de recherche sans effet'
                $ui.LibSearch.Text = ''
                # Temps de jeu noté à la fin d'une partie
                $script:PlayLog = @{}
                Add-PlayTime 'Jeu d''essai' (Get-Date).AddMinutes(-42)
                Assert-Test ([int]((Get-PlayLog)['Jeu d''essai'].Seconds / 60) -eq 42) 'temps de jeu mal compté'
                "$($games.Count) jeux ($(@($games | Where-Object { Get-CoverFile $_ }).Count) avec jaquette) en vignettes, désinstallation par $($ukinds -join '/')$(if ($unk.Count) { " (sans désinstalleur : $($unk -join ', '))" }) (page prête en $tBuild ms, rafraîchie en $tAgain ms), lancement par $(@($kinds.Keys | Sort-Object) -join ', '), panneau et optimisation du jeu, recherche d'un jeu, temps de jeu ; $loText"
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
            Test-Step 'Tâches planifiées lues en arrière plan' {
                # Invoke-NameMigration / Update-AutoStartPath ne tournent pas dans la copie de test : on vérifie leur lecture des tâches
                $info = @(Get-TaskInfo @($AutoStartTask, 'Nevermind tâche qui n''existe pas'))
                $real = Test-AutoStart
                Assert-Test ($info.Count -eq [int]$real) "$($info.Count) tâche(s) lue(s), attendu $([int]$real)"
                if ($real) { Assert-Test ($info[0].Args -like '*-Demarrage*') "arguments lus : $($info[0].Args)" }
                $sw = [Diagnostics.Stopwatch]::StartNew(); $script:Starting = $true
                try { Update-ShortcutCard } finally { $script:Starting = $false }
                Assert-Test ([bool]$ui.ChkAutoStart.IsChecked -eq $real) 'case « démarrage » fausse en lecture en arrière plan'
                "tâche de démarrage $(if ($real) { 'trouvée' } else { 'absente' }), lue en $($sw.ElapsedMilliseconds) ms sans figer la fenêtre"
            }
            Test-Step 'Raccourci sur le bureau et démarrage' {
                # Bureau simulé : le vrai bureau n'est pas touché
                $script:DesktopDir = Join-Path $DataDir 'bureau-essai'
                New-Item -ItemType Directory -Force -Path $script:DesktopDir | Out-Null
                $exe = Get-AppExe
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
            Test-Step 'Identité Nevermind (nom, logo animé, couleurs)' {
                Assert-Test ($Window.Title -eq 'Nevermind') "titre de la fenêtre : $($Window.Title)"
                Assert-Test ($script:LogoMark -and $script:LogoWord -and $script:LogoWord.Text.Text -eq 'Nevermind') 'logo de la barre de gauche absent'
                if ($script:LogoMark.Pack -and $script:LogoMark.Timer) {
                    # Logo animé d'un pack : il joue un tour complet puis revient à sa première image
                    Start-PackLogoShake; Wait-TestMs 150
                    $moved = $script:LogoMark.Timer.IsEnabled
                    Wait-TestMs ([int]($script:LogoMark.Frames.Count * $script:LogoMark.Timer.Interval.TotalMilliseconds) + 800)
                    $rest = -not $script:LogoMark.Timer.IsEnabled -and $script:LogoMark.Frame -eq 0
                    Assert-Test ($moved -and $rest) "logo animé du pack : lancé $moved, retour au repos $rest"
                } elseif ($script:LogoMark.Pack -and $script:LogoMark.Glow) {
                    # Logo d'un pack en Dofus : il lévite et brille, puis revient au repos
                    # Relevé pendant l'animation (un seul instant peut tomber pile au mauvais moment)
                    Start-PackLogoShake; $moved = $false
                    for ($k = 0; $k -lt 15 -and -not $moved; $k++) { Wait-TestMs 40; $moved = $null -ne $script:LogoMark.Img.Effect -or [math]::Abs([double]$script:LogoMark.Move.Y) -gt 0.5 }
                    Wait-TestMs 3200
                    $rest = $null -eq $script:LogoMark.Img.Effect -and $script:LogoMark.Move.Y -eq 0
                    Assert-Test ($moved -and $rest) "logo du pack : lévitation $moved, retour au repos $rest"
                } elseif ($script:LogoMark.Pack) {
                    # Logo d'un pack : la balle se secoue puis revient droite
                    # Angle relevé plusieurs fois : la balle peut repasser par zéro pile au moment d'un seul relevé
                    Start-PackLogoShake; $moved = $false
                    for ($k = 0; $k -lt 15 -and -not $moved; $k++) { Wait-TestMs 25; $moved = [math]::Abs($script:LogoMark.Rot.Angle) -gt 1 }
                    Wait-TestMs 700
                    $rest = [math]::Abs($script:LogoMark.Rot.Angle) -lt 0.01
                    Assert-Test ($moved -and $rest) "logo du pack : secoué $moved, retour au repos $rest"
                } else {
                    # Un saut de glitch déplace les calques puis les remet au repos
                    while ($script:LogoMark.Busy) { Wait-TestMs 100 }   # un saut automatique en cours
                    Start-NexoGlitch $script:LogoMark $script:LogoWord 1500
                    $moved = $script:LogoMark.Busy
                    Wait-TestMs 1900
                    $rest = [math]::Abs($script:LogoMark.Cyan.RenderTransform.X - $script:LogoMark.Rest[0] * $script:LogoMark.Size) -lt 0.01 -and -not $script:LogoMark.Busy
                    Assert-Test ($moved -and $rest) "glitch : en cours $moved, retour au repos $rest"
                }
                # Plus aucun « OptiGame » visible dans la fenêtre (hors chemins de fichiers)
                $seen = @()
                foreach ($i in 0..($ui.Tabs.Items.Count - 1)) {
                    $stack = New-Object System.Collections.Stack; $stack.Push($ui.Tabs.Items[$i])
                    while ($stack.Count) {
                        $x = $stack.Pop()
                        $txt = if ($x -is [System.Windows.Controls.TextBlock]) { Get-TextBlockText $x } elseif ($x -is [System.Windows.Controls.ContentControl] -and $x.Content -is [string]) { $x.Content } else { '' }
                        if ($txt -match 'OptiGame|\bNexo\b' -and $txt -notmatch '\\OptiGame|OptiGame\\') { $seen += $txt }
                        foreach ($ch in [System.Windows.LogicalTreeHelper]::GetChildren($x)) { if ($ch -is [System.Windows.DependencyObject]) { $stack.Push($ch) } }
                    }
                }
                Assert-Test (-not $seen.Count) "« OptiGame » encore affiché : $(($seen | Select-Object -First 3) -join ' | ')"
                Save-TestShot 'nexo-accueil'
                'titre, logo animé (saut puis retour au repos), aucun ancien nom affiché'
            }            Test-Step 'Recherche des réglages' {
                Assert-Test ($null -ne $script:Search) 'barre de recherche non branchée'
                $cases = @(
                    @('compteur discret', 'Style du compteur*'), @('netoyage', 'Nettoyage*'), @('demarage pc', 'Lancer Nevermind au démarrage*'),
                    @('raccourci bureau', 'Raccourci sur le bureau'), @('telemetrie', 'Ce que Windows envoie*'), @('ping', '*'), @('position overlay', 'Position du compteur'), @('organiseur dofus', 'Organizer Dofus*')
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
                Assert-Test ($ui.Tabs.SelectedIndex -eq $OverlayIndex) "page $($ui.Tabs.SelectedIndex) au lieu de l'onglet Overlay"
                Assert-Test (-not $script:Search.Popup.IsOpen -and -not $script:Search.Input.Text) 'barre non refermée'
                Assert-Test ($script:SearchLastHit -and $script:SearchLastHit.Effect) 'réglage non mis en évidence'
                Assert-Test (-not $script:Search.Input.IsKeyboardFocused) 'la barre garde le focus'
                Save-TestShot 'recherche-arrivee'
                Open-SearchEntry (@(Find-Settings 'raccourci bureau')[0]); Wait-TestMs 400
                $el = Find-PageElement $ui.SettingsScroll 'Raccourci et démarrage'
                Assert-Test ($ui.SettingsOverlay.Visibility -eq 'Visible' -and $script:SettingsTab -eq 'general' -and $el) 'réglage « Raccourci et démarrage » non trouvé dans les Paramètres'
                Hide-Settings
                Wait-TestMs 2500
                "$($cases.Count) recherches justes (fautes de frappe comprises), suggestions affichées, arrivée sur Overlay et dans les Paramètres"
            }
            Test-Step 'Paramètres (roue crantée)' {
                Assert-Test ($null -ne $script:TopSettings -and $script:TopSettings.IsVisible) 'roue crantée absente en haut'
                Assert-Test ($null -eq $ui.Tabs.Template.FindName('TopReport', $ui.Tabs)) 'bouton Signaler encore dans la barre'
                Show-Page 6; Wait-TestMs 300
                Assert-Test $script:TopSettings.IsVisible 'roue absente sur une page intérieure'
                $script:TopSettings.RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Primitives.ButtonBase]::ClickEvent))); Wait-TestMs 400
                Assert-Test ($ui.SettingsOverlay.Visibility -eq 'Visible') 'la roue n''ouvre pas les Paramètres'
                Save-TestShot 'parametres-general'
                $seen = @()
                foreach ($t in $SettingsTabs) {
                    Set-SettingsTab $t.Id; Wait-TestMs 150
                    Assert-Test ($ui[$t.Panel].Visibility -eq 'Visible' -and $ui[$t.Panel].Children.Count) "onglet $($t.Label) vide"
                    $seen += $t.Label
                    if ($t.Id -in 'jeux', 'aide', 'theme') { Save-TestShot "parametres-$($t.Id)" }
                }
                # Un réglage changé ici est à jour sur sa page (jaquettes : sans effet de bord)
                $old = Test-CoversOnline
                Set-SettingsTab 'jeux'
                $sw = @($ui.SetGames.Children | ForEach-Object { $_.Child } | Where-Object { $_ -is [System.Windows.Controls.Grid] -and (Get-TextBlockText $_.Children[0].Children[0]) -eq 'Jaquettes depuis Internet' })[0]
                Assert-Test ($null -ne $sw) 'interrupteur Jaquettes introuvable'
                $chk = $sw.Children[1]; $chk.IsChecked = -not $old
                $chk.RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Primitives.ButtonBase]::ClickEvent)))
                Assert-Test ((Test-CoversOnline) -ne $old -and [bool]$ui.ChkLibCovers.IsChecked -ne $old) 'réglage non repris sur la page Jeux'
                Set-Setting 'LibCoversOnline' $old; $ui.ChkLibCovers.IsChecked = $old
                # Signaler un problème : dans Aide
                Set-SettingsTab 'aide'
                Assert-Test ($ui.BtnReportProblem.IsVisible) 'Signaler un problème absent de l''onglet Aide'
                # Échap ferme
                $ev = New-Object System.Windows.Input.KeyEventArgs ([System.Windows.Input.Keyboard]::PrimaryDevice, [System.Windows.PresentationSource]::FromVisual($Window), 0, [System.Windows.Input.Key]::Escape)
                $ev.RoutedEvent = [System.Windows.UIElement]::KeyDownEvent; $Window.RaiseEvent($ev)
                Assert-Test ($ui.SettingsOverlay.Visibility -ne 'Visible') 'Échap ne ferme pas les Paramètres'
                # Fenêtre au plus étroit : les 5 onglets ne passent pas sous la recherche
                $oldW = $Window.Width; $wasMax = $Window.WindowState
                $Window.WindowState = 'Normal'; $Window.Width = $Window.MinWidth; Wait-TestMs 400; Update-TopBarFit; $Window.UpdateLayout()
                $tb = $script:TopBar
                $tabsRight = $tb.Tabs.TranslatePoint([System.Windows.Point]::new($tb.Tabs.ActualWidth, 0), $Window).X
                $searchLeft = $tb.Search.TranslatePoint([System.Windows.Point]::new(0, 0), $Window).X
                Save-TestShot 'barre-etroite'
                $Window.Width = $oldW; $Window.WindowState = $wasMax; Wait-TestMs 300
                Assert-Test ($tabsRight -le $searchLeft) "onglets jusqu'à $([int]$tabsRight) px, recherche à $([int]$searchLeft) px"
                Show-Page $HubIndex
                "$($seen.Count) onglets ($($seen -join ', ')), réglage synchronisé avec sa page, Échap ferme, barre du haut tenue à $([int]$Window.MinWidth) px"
            }
            Test-Step 'Thèmes (5 choix)' {
                $base = @($AppThemes.Keys | Where-Object { -not $AppThemes[$_].Pack })
                Assert-Test ($base.Count -eq 5) "$($base.Count) thèmes"
                $raw = [IO.File]::ReadAllText((Join-Path $ModulesDir 'interface.xaml'), [Text.Encoding]::UTF8)
                foreach ($id in $base) {
                    $th = $AppThemes[$id]
                    Assert-Test ((ConvertTo-ThemeHex '#00E5FF' $id) -eq $th.P) "$id : cyan traduit en $(ConvertTo-ThemeHex '#00E5FF' $id)"
                    foreach ($st in '#22D37A', '#F5A524', '#F04438', '#4EA8FF', '#FFFFFF') { Assert-Test ((ConvertTo-ThemeHex $st $id) -eq $st) "$id : couleur d'état $st changée" }
                    # La fenêtre complète se charge dans chaque thème
                    $w = [Windows.Markup.XamlReader]::Load((New-Object System.Xml.XmlNodeReader ([xml](Convert-ThemeXaml $raw $id))))
                    Assert-Test ($null -ne $w) "$id : fenêtre non chargée"
                    $w.Close()
                }
                # Onglet Thème : 5 cartes, choisir une autre affiche « Appliquer et relancer »
                Show-Settings 'theme'; Wait-TestMs 300
                $cards = @($ui.SetTheme.Children[0].Children)
                Assert-Test ($cards.Count -eq @(Get-ThemeChoices).Count -and $cards.Count -ge 5) "$($cards.Count) cartes de thème"
                $other = @($AppThemes.Keys | Where-Object { $_ -ne $ThemeId })[2]
                $script:ThemePick = $other; Build-ThemePanel
                Assert-Test ([bool](Find-PageElement $ui.SetTheme 'Appliquer et relancer')) 'pas de bouton pour appliquer'
                Save-TestShot 'parametres-theme-choix'
                $old = Get-Setting 'Theme' 'neon'
                Set-AppTheme $other
                Assert-Test ((Get-Setting 'Theme' '') -eq $other -and $ui.SettingsOverlay.Visibility -eq 'Visible') 'thème non enregistré (ou la copie de test s''est fermée)'
                Set-Setting 'Theme' $old; $script:ThemePick = $null; Hide-Settings
                "$(($base | ForEach-Object { $AppThemes[$_].Name }) -join ', ') : fenêtre chargée dans chacun, couleurs d'état intactes"
            }
            Test-Step 'Packs de thème (image perso, chargement)' {
                Add-Type -AssemblyName System.Drawing
                $work = Join-Path $DataDir 'essai-pack'; New-Item -ItemType Directory -Force -Path $work | Out-Null
                # GIF sur fond blanc : un rond jaune bordé de noir, avec un point blanc au milieu (comme le reflet d'un oeil)
                $bmp = New-Object System.Drawing.Bitmap 120, 90
                $g = [System.Drawing.Graphics]::FromImage($bmp); $g.Clear([System.Drawing.Color]::White)
                $g.FillEllipse([System.Drawing.Brushes]::Black, 30, 15, 60, 60); $g.FillEllipse([System.Drawing.Brushes]::Gold, 34, 19, 52, 52); $g.FillRectangle([System.Drawing.Brushes]::Black, 55, 40, 12, 12); $g.FillRectangle([System.Drawing.Brushes]::White, 58, 43, 5, 5); $g.Dispose()
                $gif = Join-Path $work 'perso.gif'; $bmp.Save($gif, [System.Drawing.Imaging.ImageFormat]::Gif); $bmp.Dispose()
                $src = Join-Path $work 'src'; New-Item -ItemType Directory -Force -Path $src | Out-Null
                $info = Convert-GifToSheet $gif (Join-Path $src 'chargement.png') 100
                $sheet = New-Object System.Drawing.Bitmap (Join-Path $src 'chargement.png')
                $corner = $sheet.GetPixel(0, 0).A; $mid = $sheet.GetPixel([int]($sheet.Width / 2), [int]($sheet.Height / 2)); $sw = $sheet.Width; $sheet.Dispose()
                Assert-Test ($info.Frames -eq 1 -and $info.Height -eq 100) "planche : $($info.Frames) image(s), hauteur $($info.Height)"
                Assert-Test ($corner -eq 0) 'le fond blanc n''a pas été retiré'
                Assert-Test ($mid.A -eq 255 -and $mid.R -gt 200 -and $mid.G -gt 200 -and $mid.B -gt 200) "le point blanc entouré de noir a été effacé ($($mid.A), $($mid.R))"
                Assert-Test ($sw -lt 120) "image non recadrée sur le personnage ($sw px)"
                [IO.File]::WriteAllText((Join-Path $src 'pack.json'), (@{ Name = 'Pack d''essai'; Desc = 'Test'; Base = 'arcade'; Hello = 'Coucou {0}'; Loader = @{ File = 'chargement.png'; Frames = 1; Delay = 80 } } | ConvertTo-Json -Depth 4), (New-Object Text.UTF8Encoding($false)))
                $zip = Join-Path $work 'essai.zip'
                Add-Type -AssemblyName System.IO.Compression.FileSystem
                if (Test-Path -LiteralPath $zip) { [IO.File]::Delete($zip) }
                [IO.Compression.ZipFile]::CreateFromDirectory($src, $zip)
                if ($PacksDir -notlike "$DataDir*") { $script:PacksDir = Join-Path $DataDir 'packs'; $PacksDir = $script:PacksDir }   # jamais dans les vrais packs
                $id = Import-ThemePack $zip
                $pk = Get-ThemePack $id
                Assert-Test ($pk -and $pk.Name -eq 'Pack d''essai' -and $pk.Base -eq 'arcade' -and $pk.Loader) 'pack importé illisible'
                # Onglet Thème : le pack apparaît après les 5 thèmes, avec son badge
                Show-Settings 'theme'; Wait-TestMs 300
                $cards = @($ui.SetTheme.Children[0].Children)
                Assert-Test (@($cards | Where-Object { $_.Tag -eq "pack:$id" }).Count -eq 1 -and $cards[4].Tag -eq 'rubis') "$($cards.Count) cartes, pack d'essai absent"
                $ui.SettingsScroll.ScrollToEnd(); Wait-TestMs 200; Save-TestShot 'parametres-theme-pack'
                Set-AppTheme "pack:$id"
                Assert-Test ((Get-Setting 'Theme' '') -eq "pack:$id") 'pack non choisi'
                Set-Setting 'Theme' 'neon'; $script:ThemePick = $null; Hide-Settings
                # Écran de chargement du pack : le personnage court sur la barre et avance avec elle
                $oldPack = $ThemePack; $script:ThemePack = $pk; $ThemePack = $pk
                try {
                    $ui.StartupOverlay.BeginAnimation([System.Windows.UIElement]::OpacityProperty, $null); $ui.StartupOverlay.Opacity = 1; $ui.StartupOverlay.Visibility = 'Visible'
                    Start-StartupLoader
                    Assert-Test ($script:Loader.Kind -eq 'sprite') 'écran de chargement du pack non utilisé'
                    $x0 = [System.Windows.Controls.Canvas]::GetLeft($script:Loader.Img)
                    Set-LoaderProgress 80; Wait-TestMs 900
                    $x1 = [System.Windows.Controls.Canvas]::GetLeft($script:Loader.Img)
                    Assert-Test ($x1 -gt $x0 + 100 -and $script:Loader.Text.Text -eq '80 %') "personnage de $x0 à $x1, texte $($script:Loader.Text.Text)"
                    Save-TestShot 'chargement-pack'
                } finally {
                    Stop-StartupLoader; $ui.StartupOverlay.Visibility = 'Collapsed'
                    $script:ThemePack = $oldPack; $ThemePack = $oldPack
                }
                # Dessin animé au centre de la barre du bas
                $oldPack3 = $ThemePack
                try {
                    $script:ThemePack = @{ Footer = @{ File = $pk.Loader.File; Frames = 1; Delay = 100; Height = 40 } }; $ThemePack = $script:ThemePack
                    Initialize-PackFooter; Wait-TestMs 300
                    Assert-Test ($ui.FooterArt.Visibility -eq 'Visible' -and $ui.FooterArt.Child -and $script:Footer.Timer.IsEnabled) 'dessin de la barre du bas absent'
                    Assert-Test ($ui.StatusText.MaxWidth -lt $ui.StatusBar.ActualWidth / 2) "le texte d'état peut passer sous le dessin ($($ui.StatusText.MaxWidth) px)"
                } finally {
                    if ($script:Footer) { $script:Footer.Timer.Stop(); $script:Footer = $null }
                    $ui.FooterArt.Child = $null; $ui.FooterArt.Visibility = 'Collapsed'; $ui.StatusText.MaxWidth = [double]::PositiveInfinity
                    $script:ThemePack = $oldPack3; $ThemePack = $oldPack3
                }
                # Icône animée d'onglet : immobile au repos, animée au survol, de retour à la 1re image ensuite
                $oldPack4 = $ThemePack; $hdr = $ui.Tabs.Items[$GamesIndex].Header; $oldIcon = $hdr.Children[0]
                try {
                    $script:ThemePack = @{ TabIcons = @{ jeux = @{ File = $pk.Loader.File; Frames = 1; Delay = 60 } } }; $ThemePack = $script:ThemePack
                    Initialize-PackTabIcons
                    $st = $script:TabIcons['jeux']
                    Assert-Test ($st -and $hdr.Children[0] -is [System.Windows.Controls.Image] -and -not $st.Timer.IsEnabled) 'icône de l''onglet Jeux non remplacée (ou animée au repos)'
                    $ev = New-Object System.Windows.Input.MouseEventArgs ([System.Windows.Input.Mouse]::PrimaryDevice, 0); $ev.RoutedEvent = [System.Windows.Input.Mouse]::MouseEnterEvent
                    $ui.Tabs.Items[$GamesIndex].RaiseEvent($ev)
                    Assert-Test ($st.Timer.IsEnabled -or ($st.Hop -and $st.Hop.HasAnimatedProperties)) 'icône non animée au survol'
                    $ev = New-Object System.Windows.Input.MouseEventArgs ([System.Windows.Input.Mouse]::PrimaryDevice, 0); $ev.RoutedEvent = [System.Windows.Input.Mouse]::MouseLeaveEvent
                    $ui.Tabs.Items[$GamesIndex].RaiseEvent($ev)
                    Assert-Test (-not $st.Timer.IsEnabled -and $st.Frame -eq 0 -and (-not $st.Hop -or -not $st.Hop.HasAnimatedProperties)) 'icône toujours animée après le survol'
                } finally {
                    if ($script:TabIcons) { foreach ($x in $script:TabIcons.Values) { $x.Timer.Stop() }; $script:TabIcons = $null }
                    if ($hdr.Children[0] -ne $oldIcon) { $hdr.Children.RemoveAt(0); $hdr.Children.Insert(0, $oldIcon) }
                    $script:ThemePack = $oldPack4; $ThemePack = $oldPack4
                }
                # Police de pack : la fenêtre entière se charge avec (titres, onglets, boutons), et en mode « tout »
                $oldFont = $PackFont; $oldPack2 = $ThemePack
                try {
                    $script:PackFont = 'file:///C:/Windows/Fonts/#Consolas'; $PackFont = $script:PackFont
                    foreach ($scope in 'titres', 'tout') {
                        $script:ThemePack = @{ FontScope = $scope }; $ThemePack = $script:ThemePack
                        $raw = [IO.File]::ReadAllText((Join-Path $ModulesDir 'interface.xaml'), [Text.Encoding]::UTF8)
                        $w = [Windows.Markup.XamlReader]::Load((New-Object System.Xml.XmlNodeReader ([xml](Convert-ThemeXaml $raw $ThemeId))))
                        Assert-Test ($null -ne $w) "fenêtre non chargée avec la police du pack ($scope)"
                        $w.Close()
                    }
                    $tb = New-Text 'Test' 14; Set-PackFont $tb
                    Assert-Test ([string]$tb.FontFamily -like '*Consolas*') "police non posée : $($tb.FontFamily)"
                } finally { $script:PackFont = $oldFont; $PackFont = $oldFont; $script:ThemePack = $oldPack2; $ThemePack = $oldPack2 }
                "GIF détouré (reflet gardé), zip importé, carte « Pack » dans Thème, personnage qui avance avec la barre, dessin dans la barre du bas, icône d'onglet animée au survol, police du pack chargée"
            }
            Test-Step 'Signaler un problème (Paramètres, Aide)' {
                Show-ReportPanel; $script:ReportBox.Text = 'Le jeu rame depuis la mise à jour'; Wait-TestMs 400; Save-TestShot 'signaler'; Hide-TestPanel
                $dir = Join-Path $DataDir 'essai-rapport'; New-Item -ItemType Directory -Force -Path $dir | Out-Null
                $zip = Export-ProblemReport $dir 'Le jeu rame depuis la mise à jour'
                Add-Type -AssemblyName System.IO.Compression.FileSystem
                $z = [IO.Compression.ZipFile]::OpenRead($zip)
                try { $names = @($z.Entries | ForEach-Object { $_.Name }) } finally { $z.Dispose() }
                Assert-Test ($names -contains 'description.txt' -and $names -contains 'infos.txt') "contenu du fichier : $($names -join ', ')"
                Show-Page $HubIndex
                "fichier avec la description ($($names.Count) fichiers)"
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
                if ($ThemePack -and $ThemePack.Loader) { Assert-Test ($script:Loader.Kind -eq 'sprite' -and $script:Loader.GlitchTimer.IsEnabled) 'chargement du pack absent' }
                else { Assert-Test ($ui.StartupLoaderHost.Children.Count -eq 1 -and $script:Loader.Loops.Count -ge 4 -and $script:Loader.Mark -and $script:Loader.GlitchTimer.IsEnabled) 'chargement animé (logo Nevermind) absent' }
                Set-StartupStep 'Recherche de tes jeux...' 72; $ui.StartupDetail.Text = 'Calcul: Fichiers temporaires (utilisateur)...'; Wait-TestMs 1200; Save-TestShot 'chargement'
                $gt = $script:Loader.GlitchTimer
                Stop-StartupLoader
                Assert-Test ($null -eq $script:Loader -and -not $gt.IsEnabled) 'animations du chargement non arrêtées'
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
                # Une vraie partie en cours sur le PC (mesure lancée toute seule par la copie de test) : on repart de zéro
                if ($script:LagSession) { $script:LagSession = $null; if ($script:LagTimer) { $script:LagTimer.Stop() }; [LagMon]::Stop() }
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
            Test-Step 'Fenêtre Quitter (croix)' {
                # La fenêtre est modale : un minuteur la regarde, la capture puis clique à la place de l'utilisateur
                $script:QuitSeen = $null
                $qt = New-Object System.Windows.Threading.DispatcherTimer
                $qt.Interval = [TimeSpan]::FromMilliseconds(500)
                $qt.Add_Tick({
                    param($s, $e)
                    $s.Stop()
                    $d = $script:QuitDialog
                    if (-not $d) { return }
                    try {
                        $c = $d.Content; $c.UpdateLayout()
                        $script:QuitSeen = @{ Ask = [bool](Find-PageElement $c 'Voulez-vous vraiment quitter ?'); Quit = [bool](Find-PageElement $c 'Quitter'); Tray = [bool](Find-PageElement $c 'Réduire') }
                        $qw = [int]$c.ActualWidth + 28; $qh = [int]$c.ActualHeight + 28
                        $rtb = New-Object System.Windows.Media.Imaging.RenderTargetBitmap($qw, $qh, 96, 96, [System.Windows.Media.PixelFormats]::Pbgra32)
                        $rtb.Render($c)
                        $enc = New-Object System.Windows.Media.Imaging.PngBitmapEncoder; $enc.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($rtb))
                        $fs = [IO.File]::Create((Join-Path $script:T.Dir 'captures\quitter.png')); $enc.Save($fs); $fs.Close()
                        $b = Find-PageElement $c 'Réduire'
                        $b.RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Button]::ClickEvent)))
                    } catch { Write-Log "Test fenêtre Quitter : $_" }
                    finally { if ($script:QuitDialog) { $script:QuitDialog.Close() } }
                })
                $qt.Start()
                $r = Show-QuitDialog
                Assert-Test ($script:QuitSeen -and $script:QuitSeen.Ask -and $script:QuitSeen.Quit -and $script:QuitSeen.Tray) "fenêtre : $(if ($script:QuitSeen) { ($script:QuitSeen.GetEnumerator() | ForEach-Object { "$($_.Key) $($_.Value)" }) -join ', ' } else { 'pas affichée' })"
                Assert-Test ($r -eq 'tray') "choix : $r au lieu de Réduire"
                # Échap : rien ne se passe
                $qt2 = New-Object System.Windows.Threading.DispatcherTimer
                $qt2.Interval = [TimeSpan]::FromMilliseconds(400)
                $qt2.Add_Tick({ param($s, $e) $s.Stop(); if ($script:QuitDialog) { $script:QuitDialog.Close() } })
                $qt2.Start()
                $r2 = Show-QuitDialog
                Assert-Test (-not $r2) "Échap ou fermeture : $r2"
                'question posée, Quitter et Réduire proposés, Réduire choisi, fermeture sans choix sans effet'
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
