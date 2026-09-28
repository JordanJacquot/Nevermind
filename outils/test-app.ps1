# Code injecté par outils\tester.ps1 dans une copie de l'app, juste avant l'ouverture de la fenêtre.
# Il remplace les boîtes de dialogue (pour ne jamais bloquer), parcourt l'app et note chaque résultat.
$script:T = @{
    Dir = '__TEST__'; Complet = $__COMPLET__; Captures = $__CAPTURES__
    Res = New-Object System.Collections.ArrayList; Msgs = New-Object System.Collections.ArrayList
    Clock = [Diagnostics.Stopwatch]::StartNew(); Last = 0.0; MaxGap = 0.0; Watch = $false
}
function Invoke-UpdateCheck { }
function Show-Message([string]$Text, [string]$Icon = 'Information') { [void]$script:T.Msgs.Add("[$Icon] $Text") }
function Confirm-Action([string]$Text) { [void]$script:T.Msgs.Add("[Question] $Text"); $false }
function Show-Notify([string]$Title, [string]$Text, [scriptblock]$OnClick) { [void]$script:T.Msgs.Add("[Notify] $Title : $Text") }

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
        $dc.DrawRectangle((New-Object System.Windows.Media.VisualBrush $el), $null, [System.Windows.Rect]::new(0, 0, $w, $h)); $dc.Close()
        $rtb = New-Object System.Windows.Media.Imaging.RenderTargetBitmap($w, $h, 96, 96, [System.Windows.Media.PixelFormats]::Pbgra32); $rtb.Render($dv)
        $enc = New-Object System.Windows.Media.Imaging.PngBitmapEncoder; $enc.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($rtb))
        $fs = [IO.File]::Create((Join-Path $script:T.Dir "captures\$Name.png")); $enc.Save($fs); $fs.Close()
    } catch {}
}
# Exécute une étape : échoue si elle lève une erreur ou affiche un message d'avertissement.
function Test-Step([string]$Name, [scriptblock]$Body) {
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
    if ($script:T.Watch -and $script:T.Last) { $script:T.MaxGap = [math]::Max($script:T.MaxGap, $now - $script:T.Last) }
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
                Wait-TestMs 500; Save-TestShot 'gaming-reglages'; Set-GamingSubPage 2; Wait-TestMs 300; Save-TestShot 'gaming-mode-jeu'; Set-GamingSubPage 3; Wait-TestMs 300; Save-TestShot 'gaming-profils'
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
                $ui.Tabs.SelectedIndex = 1; Set-GamingSubPage 1; Wait-TestMs 500; Save-TestShot 'mes-parties'
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
                Set-GamingSubPage 0
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
                    "$($l.Count) appareils"
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
            Add-TestResult 'Fluidité' ($gap -lt 3000) "plus long blocage de la fenêtre : $gap ms"
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
[void]$Window.ShowDialog()
[Environment]::Exit(0)
