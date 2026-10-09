# Nevermind : branchement des boutons et événements de la fenêtre.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Événements
# ---------------------------------------------------------------------------
$IconPath = Join-Path $AppDir 'OptiGame.ico'
if (Test-Path $IconPath) {
    try {
        # Chargée en mémoire pour ne pas bloquer le fichier (il doit pouvoir être remplacé par une mise à jour).
        $iconStream = New-Object IO.MemoryStream (, [IO.File]::ReadAllBytes($IconPath))
        $script:IconFrames = [System.Windows.Media.Imaging.BitmapDecoder]::Create($iconStream, 'None', 'OnLoad').Frames
        $Window.Icon = $script:IconFrames | Sort-Object PixelWidth | Where-Object { $_.PixelWidth -ge 32 } | Select-Object -First 1
    } catch { Write-Log "Icône: $_" }
}

$Window.Add_SourceInitialized({
    $h = (New-Object System.Windows.Interop.WindowInteropHelper $Window).Handle
    try { [OGNative]::SetDarkTitleBar($h) } catch {}
    # Windows 11 : fond « Mica » (le fond d'écran, flouté et teinté, transparaît derrière l'app)
    if ($script:Build -ge 22621 -and (Get-Setting 'Mica' $true)) {
        try {
            if ([OGNative]::EnableMica($h)) {
                [System.Windows.Interop.HwndSource]::FromHwnd($h).CompositionTarget.BackgroundColor = [System.Windows.Media.Colors]::Transparent
                $Window.Background = [System.Windows.Media.Brushes]::Transparent
                $ui.BackdropBase.Opacity = 0.82
            }
        } catch { Write-Log "Mica: $_" }
    }
})
foreach ($ov in $ui.TestOverlay, $ui.Overlay, $ui.NetMapOverlay) { $ov.Add_IsVisibleChanged({ try { Update-BackdropBlur } catch {} }) }

$Window.Add_StateChanged({
    if ($script:StartHidden) { return }   # lancement avec Windows : la fenêtre se prépare d'abord, réduite
    if ($Window.WindowState -eq 'Minimized') { try { Hide-ToTray } catch { Write-Log "Réduction: $_" } }
    else { $script:StateBeforeTray = [string]$Window.WindowState }
})
# Un nouveau lancement de Nevermind demande d'afficher cette fenêtre (elle peut être cachée près de l'horloge)
if (-not $env:OPTIGAME_TEST) {
    $script:ShowWatch = New-Object System.Windows.Threading.DispatcherTimer
    $script:ShowWatch.Interval = [TimeSpan]::FromMilliseconds(600)
    $script:ShowWatch.Add_Tick({
        if ([IO.File]::Exists($ShowRequest)) {
            try { [IO.File]::Delete($ShowRequest) } catch {}
            try { Show-MainWindow } catch { Write-Log "Affichage: $_" }
        }
    })
    $script:ShowWatch.Start()
}

$Window.Add_Closed({
    $script:Closing = $true
    try { if ($script:ShowWatch) { $script:ShowWatch.Stop() } } catch {}
    $Live.Run = $false
    if ($script:LiveTimer) { $script:LiveTimer.Stop() }
    try { Stop-FpsTarget } catch { Write-Log "Fermeture, mesure des FPS: $_" }
    try { if ($script:GameSession) { Stop-GameSession } } catch { Write-Log "Fermeture, mode jeu: $_" }
    try { Unregister-FpsHotkey } catch {}
    try { if ($script:TrafficTimer) { $script:TrafficTimer.Stop() } } catch {}
    try { if ($script:LogoTimer) { $script:LogoTimer.Stop() } } catch {}
    try { [FrameMon]::Stop() } catch {}
    try { if ($script:LagSession) { $script:LagSession = $null; if ($script:LagTimer) { $script:LagTimer.Stop() }; [LagMon]::Stop() } } catch {}
    try { if ([NetFlow]::Running) { [NetFlow]::Stop() } } catch {}
    try { if ($script:NotifyIcon) { $script:NotifyIcon.Visible = $false; $script:NotifyIcon.Dispose() } } catch {}
    if ($Splash) { try { $Splash.Close() } catch {} }
    # En dernier : fin de la boucle de l'app.
    [System.Windows.Threading.Dispatcher]::CurrentDispatcher.InvokeShutdown()
})

$ui.BtnFixAll.Add_Click({ Open-FixAll })
$ui.BtnNetMapClose.Add_Click({ Hide-NetMap })
$ui.BtnNetMapScan.Add_Click({ Invoke-Safe { Hide-NetMap; Invoke-NetworkScan; if (@($script:NetList).Count) { Show-NetMap } } })
$script:SheetMode = 'fix'
$ui.SheetClose.Add_Click({ Close-Sheet })
$ui.OverlayBackdrop.Add_MouseLeftButtonUp({ if ($script:SheetMode -ne 'display') { Close-Sheet } })
$ui.SheetRun.Add_Click({
    if ($script:SheetMode -eq 'display') { $script:DisplayChoice = 'keep'; return }
    if ($script:SheetMode -eq 'tour') { Show-TourStep ($script:TourStep + 1); return }
    Invoke-Safe { Invoke-SheetRun }
})
$ui.SheetOpen.Add_Click({ if ($ui.SheetOpen.Tag) { Close-Sheet; Invoke-FindingAction $ui.SheetOpen.Tag } })
$ui.SheetIgnore.Add_Click({
    switch ($script:SheetMode) {
        'display' { $script:DisplayChoice = 'revert' }
        'result'  { Invoke-Safe { Invoke-UndoLastRun } }
        default {
            $f = $script:SheetItems[0]
            Invoke-Safe { Set-IgnoreFinding $f (-not ($script:Ignored -contains $f.Id)) }
        }
    }
})
$Window.Add_SizeChanged({ $ui.TestScroll.MaxHeight = [math]::Max(300.0, $Window.ActualHeight - 300) })
$ui.BtnTestStop.Add_Click({
    [OGNative]::Cancel = $true
    Set-TestState 'info' 'Arrêt en cours...'
    if ($script:ScanRunning -and (Test-Path $MpCmd)) { Start-Process -FilePath $MpCmd -ArgumentList '-Cancel' -WindowStyle Hidden }
})
foreach ($n in 'BtnScanQuick', 'BtnScanFull', 'BtnScanFolder', 'BtnScanUpdate') { [void]$script:SecButtons.Add($ui[$n]) }
$ui.BtnScanQuick.Add_Click({ Invoke-Safe { Invoke-DefenderScan 'QuickScan' } })
$ui.BtnScanFull.Add_Click({
    if (-not (Confirm-Action "L'analyse complète vérifie tous les fichiers du PC : elle peut durer une heure ou plus. Tu peux continuer à utiliser ton PC pendant ce temps. Lancer l'analyse ?")) { return }
    Invoke-Safe { Invoke-DefenderScan 'FullScan' }
})
$ui.BtnScanFolder.Add_Click({ Invoke-Safe { Invoke-FolderScan } })
$ui.BtnScanUpdate.Add_Click({ Invoke-Safe { Update-Definitions } })
$ui.BtnNetScan.Add_Click({ Invoke-Safe { Invoke-NetworkScan } })
$ui.BtnTraffic.Add_Click({
    Invoke-Safe {
        if (-not $script:TrafficBuilt) { Build-TrafficPage }
        if ($script:TrafficTimer -and $script:TrafficTimer.IsEnabled) { Stop-TrafficWatch } else { Start-TrafficWatch }
    }
})
$ui.BtnNetAudit.Add_Click({ Invoke-Safe { Invoke-NetAudit } })
$ui.BtnNetAuditView.Add_Click({ Invoke-Safe { Show-NetAuditReport } })
$ui.BtnSecRefresh.Add_Click({ Invoke-Safe { Update-SecurityTab } })
$ui.BtnTestClose.Add_Click({ Hide-TestPanel })
$ui.BtnTestX.Add_Click({ Hide-TestPanel })
$ui.TestBackdrop.Add_MouseLeftButtonUp({ Hide-TestPanel })
$ui.BtnTestAgain.Add_Click({
    $run = $script:LastRun
    if ($run -and $run.Fn) { Invoke-Safe { & $run.Fn $run.Tile $run.Ctx } }
})
$Window.Add_KeyDown({
    param($s, $e)
    # Ctrl+K ou Ctrl+F : recherche d'un réglage
    if (($e.Key -eq 'K' -or $e.Key -eq 'F') -and [System.Windows.Input.Keyboard]::Modifiers -eq 'Control') { Focus-Search; $e.Handled = $true; return }
    if ($e.Key -eq 'Escape' -and $ui.TestOverlay.Visibility -eq 'Visible') { Hide-TestPanel; return }
    if ($e.Key -eq 'Escape' -and $ui.SettingsOverlay.Visibility -eq 'Visible' -and $ui.Overlay.Visibility -ne 'Visible') { Hide-Settings; return }
    if ($e.Key -eq 'Escape' -and $ui.NetMapOverlay.Visibility -eq 'Visible' -and $ui.Overlay.Visibility -ne 'Visible') { Hide-NetMap; return }
    if ($e.Key -ne 'Escape' -or $ui.Overlay.Visibility -ne 'Visible') { return }
    if ($script:SheetMode -eq 'display') { $script:DisplayChoice = 'revert' } else { Close-Sheet }
})

$ui.BtnAnalyze.Add_Click({ Invoke-Safe { Invoke-Analysis } })
$ui.BtnSelectAll.Add_Click({
    foreach ($r in $script:TweakRows) { if ($r.CheckBox.IsEnabled) { $r.CheckBox.IsChecked = ($r.Tweak.Recommended -ne $false) } }
})
$ui.BtnApply.Add_Click({ Invoke-Safe { Invoke-ApplyTweaks } })
$ui.BtnRefreshStartup.Add_Click({ Invoke-Safe { Update-StartupList } })
$ui.BtnDisableStartup.Add_Click({ Invoke-Safe { Disable-RecommendedStartup } })
$ui.BtnPing.Add_Click({ Invoke-Safe { Invoke-NetTest } })
$ui.BtnDnsApply.Add_Click({ Invoke-Safe { Set-Dns $ui.DnsCombo.SelectedIndex } })
$ui.BtnDnsFlush.Add_Click({ Invoke-Safe { Clear-DnsClientCache; Set-Status 'Cache DNS vidé.' } })
$ui.BtnCleanScan.Add_Click({ Invoke-Safe { Invoke-CleanScan } })
$ui.BtnClean.Add_Click({ Invoke-Safe { Invoke-Clean } })
$ui.BtnUndo.Add_Click({ Invoke-Safe { Invoke-UndoAll } })
$ui.BtnRestorePoint.Add_Click({
    Invoke-Safe {
        Set-Busy $true
        $r = Invoke-Async {
            try { Checkpoint-Computer -Description 'Nevermind (manuel)' -RestorePointType MODIFY_SETTINGS -ErrorAction Stop; 'OK' }
            catch { $_.Exception.Message }
        }
        if ("$r" -eq 'OK') { Set-Status 'Point de restauration créé.'; Show-Message 'Point de restauration créé.' }
        else { Show-Message "Impossible de créer le point de restauration:`n`n$r`n`nLa protection du système est peut-être désactivée (Panneau de configuration > Système > Protection du système)." 'Warning' }
    }
})
$ui.BtnOpenRestore.Add_Click({ Start-Process 'rstrui.exe' })
$ui.BtnExport.Add_Click({ Invoke-Safe { Export-Report } })
$ui.BtnReportProblem.Add_Click({ Invoke-Safe { Show-ReportPanel } })
$ui.ChkNetWatch.IsChecked = [bool](Get-Setting 'NetWatch' $false)
$ui.ChkNetWatch.Add_Click({ Invoke-Safe { Set-NetWatch ([bool]$ui.ChkNetWatch.IsChecked); Set-Status $(if ($ui.ChkNetWatch.IsChecked) { 'Surveillance du réseau activée.' } else { 'Surveillance du réseau désactivée.' }) } })
$ui.ChkBeta.IsChecked = [bool](Get-Setting 'Beta' $false)
$ui.ChkBeta.Add_Click({ Invoke-Safe { Set-BetaChannel ([bool]$ui.ChkBeta.IsChecked) } })
Initialize-Library
$ui.BtnShortcut.Add_Click({ Invoke-Safe { Invoke-CreateShortcut } })
$ui.ChkAutoStart.Add_Click({ Invoke-Safe { Set-AutoStartFromUi ([bool]$ui.ChkAutoStart.IsChecked) } })
$Window.Dispatcher.Add_UnhandledException({
    param($s, $e)
    Write-Log "ERREUR non gérée: $($e.Exception.Message)"
    $e.Handled = $true
    try { Set-Status 'Une erreur est survenue (elle est notée dans le journal).' } catch {}
})
$ui.Tabs.Add_SelectionChanged({
    param($s, $e)
    if ($e.OriginalSource -ne $ui.Tabs) { return }
    Update-NavBar
    try { Start-PageTransition } catch {}
    if ($ui.Tabs.SelectedIndex -eq $HubIndex -and $script:HubStats) { Invoke-Safe { Update-Hub }; return }
    if ($ui.Tabs.SelectedIndex -eq $GamesIndex) {
        Invoke-Safe { if (-not $script:LibBuilt) { Build-Library } else { Update-LibraryView } }
        return
    }
    if ($ui.Tabs.SelectedIndex -eq $OverlayIndex) { Invoke-Safe { Build-OverlayPanel }; return }
    if ($ui.Tabs.SelectedIndex -eq $TrafficIndex) {
        Invoke-Safe {
            if (-not $script:TrafficBuilt) { Build-TrafficPage; Start-TrafficWatch } else { Update-TrafficView }
        }
        return
    }
    if ($ui.Tabs.SelectedIndex -eq $NetIndex -and -not $script:NetBuilt) {
        $script:NetBuilt = $true
        Invoke-Safe { Show-NetHeroIdle; Update-NetAuditCard; Update-NetScanInfo }
        return
    }
    if ($ui.Tabs.SelectedIndex -eq 6 -and -not $script:SecurityBuilt) {
        $script:SecurityBuilt = $true
        Invoke-Safe { Update-SecurityTab }
        return
    }
    if ($ui.Tabs.SelectedIndex -eq 5 -and -not $script:TestsBuilt) {
        $script:TestsBuilt = $true
        Set-Status 'Préparation des tests...'
        Invoke-Safe { Build-TestsTab }
        Set-Status 'Choisis un composant à tester.'
    }
})
$ui.BtnUpdate.Add_Click({ Invoke-Safe { Install-Update } })
$ui.BtnUpdateLater.Add_Click({ $ui.UpdateBanner.Visibility = 'Collapsed' })
$ui.BtnCheckUpdate.Add_Click({
    if ($script:PendingUpdate) { Invoke-Safe { Install-Update } } else { Invoke-Safe { Invoke-UpdateCheck -Manual } }
})

$Window.Add_ContentRendered({
    if ($Splash) { try { $Splash.Close() } catch {}; $script:Splash = $null }
    if ($ui.VersionText) { $ui.VersionText.Text = "Nevermind $AppVersion" }
    # Logo Nevermind animé (le N et le mot « glitchent » au survol et de temps en temps)
    try { Initialize-NexoLogo $ui.Tabs.Template.FindName('LogoMarkHost', $ui.Tabs) $ui.Tabs.Template.FindName('LogoWordHost', $ui.Tabs) } catch { Write-Log "Logo: $_" }
    $script:NavBar = $ui.Tabs.Template.FindName('NavBar', $ui.Tabs)
    $script:NavCrumb = $ui.Tabs.Template.FindName('NavCrumb', $ui.Tabs)
    $back = $ui.Tabs.Template.FindName('NavBack', $ui.Tabs)
    if ($back) { $back.Add_Click({ Show-Page $HubIndex }) }
    # Roue crantée : Paramètres (Signaler un problème est dans l'onglet Aide)
    $script:TopSettings = $ui.Tabs.Template.FindName('TopSettings', $ui.Tabs)
    $script:SettingsGear = $ui.Tabs.Template.FindName('TopSettingsIcon', $ui.Tabs)
    if ($script:SettingsGear) { $script:SettingsGear.RenderTransform = New-Object System.Windows.Media.RotateTransform }   # celle du modèle est figée
    if ($script:TopSettings) { $script:TopSettings.Add_Click({ Invoke-Safe { Show-Settings } }) }
    try { Initialize-TopBarFit } catch { Write-Log "Barre du haut: $_" }
    try { Initialize-Search } catch { Write-Log "Recherche: $_" }
    try { Add-ThemeDecor } catch { Write-Log "Décor du thème: $_" }
    try { Start-StartupLoader } catch { Write-Log "Chargement: $_" }
    # Premières tâches derrière l'écran de chargement : l'app n'apparaît qu'une fois prête
    $t0 = Get-Date
    # Détecteur de blocages du chargement : note dans le journal chaque gel de la fenêtre (animation figée) et ce qui tournait
    $script:StartGaps = @{ Sw = [Diagnostics.Stopwatch]::StartNew(); Last = 0.0; Seen = (New-Object System.Collections.ArrayList) }
    $gapT = New-Object System.Windows.Threading.DispatcherTimer
    $gapT.Interval = [TimeSpan]::FromMilliseconds(15)
    $gapT.Add_Tick({
        $g = $script:StartGaps
        if (-not $g) { return }
        $now = $g.Sw.Elapsed.TotalMilliseconds
        if ($g.Last -and ($now - $g.Last) -gt 120) { Write-Log ("Chargement : fenêtre figée {0:N0} ms ({1})" -f ($now - $g.Last), ((@($g.Seen) | Select-Object -Unique) -join ', ')) }
        if ($g.Seen.Count -gt 1) { $keep = $g.Seen[-1]; $g.Seen.Clear(); [void]$g.Seen.Add($keep) }
        $g.Last = $now
    })
    $gapT.Start()
    $script:Starting = $true
    $script:StartGapTimer = $gapT
    try {
        Set-StartupStep 'Préparation de l''interface...' 5
        Step-UI; [void]$script:StartGaps.Seen.Add('Build-Hub'); Build-Hub
        $ui.Tabs.SelectedIndex = $HubIndex
        Step-UI; [void]$script:StartGaps.Seen.Add('Update-Hub'); Update-Hub
        Step-UI; [void]$script:StartGaps.Seen.Add('Start-Live'); Start-Live
        Set-StartupStep 'Analyse de ton PC...' 12
        Invoke-Safe { Step-UI; [void]$script:StartGaps.Seen.Add('Invoke-Analysis'); Invoke-Analysis }
        Set-StartupStep 'Réglages gaming et programmes au démarrage...' 50
        Invoke-Safe {
            Step-UI; [void]$script:StartGaps.Seen.Add('Build-GamingTab'); Build-GamingTab
            Step-UI; [void]$script:StartGaps.Seen.Add('Update-StartupList'); Update-StartupList
        }
        Set-StartupStep 'Connexion et sauvegardes...' 62
        Invoke-Safe {
            Step-UI; [void]$script:StartGaps.Seen.Add('Update-NetInfo'); Update-NetInfo
            Step-UI; [void]$script:StartGaps.Seen.Add('Update-BackupSummary'); Update-BackupSummary
            Step-UI; [void]$script:StartGaps.Seen.Add('Update-HistoryList'); Update-HistoryList
        }
        Invoke-Safe {
            if (-not $env:OPTIGAME_TEST) {
                Step-UI; [void]$script:StartGaps.Seen.Add('Invoke-NameMigration'); Invoke-NameMigration
                Step-UI; [void]$script:StartGaps.Seen.Add('Update-AutoStartPath'); Update-AutoStartPath
            }
            Step-UI; [void]$script:StartGaps.Seen.Add('Update-ShortcutCard'); Update-ShortcutCard
        }
        Set-StartupStep 'Protection du PC...' 72
        Invoke-Safe {
            if (-not $script:SecurityBuilt) { $script:SecurityBuilt = $true; Step-UI; [void]$script:StartGaps.Seen.Add('Update-SecurityTab'); Update-SecurityTab }
            Step-UI; [void]$script:StartGaps.Seen.Add('Update-Hub'); Update-Hub
        }
        Set-StartupStep 'Recherche de tes jeux...' 86
        Invoke-Safe {
            Step-UI; [void]$script:StartGaps.Seen.Add('Update-GameCache'); Update-GameCache
            Step-UI; [void]$script:StartGaps.Seen.Add('Update-Hub'); Update-Hub   # la carte « Ta dernière partie » a besoin de la liste des jeux
            Step-UI; [void]$script:StartGaps.Seen.Add('Update-GameWatch'); Update-GameWatch
            Step-UI; [void]$script:StartGaps.Seen.Add('Update-FpsHotkey'); Update-FpsHotkey
            if (Get-Setting 'NetWatch' $false) { Set-NetWatch $true }
        }
        Set-StartupStep (Get-ThemeText 'Ready') 100
        $script:StartGapTimer.Stop(); $script:StartGaps = $null; $script:Starting = $false
        Write-Log "Démarrage terminé en $([math]::Round(((Get-Date) - $t0).TotalSeconds, 1)) s"
    } finally {
        $script:Starting = $false
        if ($script:StartGapTimer) { $script:StartGapTimer.Stop() }
        $script:StartupThen = {
            Set-Status 'Prêt.'
            Invoke-Safe { Invoke-WelcomeChecks }
            try { Invoke-UpdateCheck } catch { Write-Log "Vérification de mise à jour: $_" }
        }
        if ($script:StartHidden) {
            # Lancé avec Windows : pas d'animation (une fenêtre cachée ne l'avancerait pas), direction l'horloge
            $script:StartHidden = $false
            $ui.StartupOverlay.Visibility = 'Collapsed'
            try { Stop-StartupLoader } catch {}
            $t = $script:StartupThen; $script:StartupThen = $null
            $Window.ShowInTaskbar = $true
            try { Hide-ToTray } catch { Write-Log "Réduction: $_" }
            & $t
        } else { Hide-StartupOverlay }
    }
})
