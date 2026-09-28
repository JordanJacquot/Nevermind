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
    $ready = $script:LastAnalysis -and $ui.StatusText.Text -eq 'Prêt.'
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
