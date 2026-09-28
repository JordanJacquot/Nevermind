# OptiGame : branchement des boutons et événements de la fenêtre.
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
    try { [OGNative]::SetDarkTitleBar((New-Object System.Windows.Interop.WindowInteropHelper $Window).Handle) } catch {}
})

$Window.Add_Closed({
    $Live.Run = $false
    if ($script:LiveTimer) { $script:LiveTimer.Stop() }
    try { if ($script:GameSession) { Stop-GameSession } } catch {}
    try { Stop-FpsTarget; Unregister-FpsHotkey } catch {}
    try { [FrameMon]::Stop() } catch {}
    try { if ($script:NotifyIcon) { $script:NotifyIcon.Visible = $false; $script:NotifyIcon.Dispose() } } catch {}
    if ($Splash) { try { $Splash.Close() } catch {} }
})

$ui.BtnFixAll.Add_Click({ Open-FixAll })
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
    if ($e.Key -eq 'Escape' -and $ui.TestOverlay.Visibility -eq 'Visible') { Hide-TestPanel; return }
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
            try { Checkpoint-Computer -Description 'OptiGame (manuel)' -RestorePointType MODIFY_SETTINGS -ErrorAction Stop; 'OK' }
            catch { $_.Exception.Message }
        }
        if ("$r" -eq 'OK') { Set-Status 'Point de restauration créé.'; Show-Message 'Point de restauration créé.' }
        else { Show-Message "Impossible de créer le point de restauration:`n`n$r`n`nLa protection du système est peut-être désactivée (Panneau de configuration > Système > Protection du système)." 'Warning' }
    }
})
$ui.BtnOpenRestore.Add_Click({ Start-Process 'rstrui.exe' })
$ui.BtnExport.Add_Click({ Invoke-Safe { Export-Report } })
$ui.BtnReportProblem.Add_Click({ Invoke-Safe { Export-ProblemReport } })
$ui.ChkNetWatch.IsChecked = [bool](Get-Setting 'NetWatch' $false)
$ui.ChkNetWatch.Add_Click({ Invoke-Safe { Set-NetWatch ([bool]$ui.ChkNetWatch.IsChecked); Set-Status $(if ($ui.ChkNetWatch.IsChecked) { 'Surveillance du réseau activée.' } else { 'Surveillance du réseau désactivée.' }) } })
$ui.ChkBeta.IsChecked = [bool](Get-Setting 'Beta' $false)
$ui.ChkBeta.Add_Click({ Invoke-Safe { Set-BetaChannel ([bool]$ui.ChkBeta.IsChecked) } })
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
    if ($ui.Tabs.SelectedIndex -eq $HubIndex -and $script:HubStats) { Invoke-Safe { Update-Hub }; return }
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
    $v = $ui.Tabs.Template.FindName('VersionText', $ui.Tabs)
    if ($v) { $v.Text = "Version $AppVersion" }
    $logo = $ui.Tabs.Template.FindName('LogoImg', $ui.Tabs)
    if ($logo -and $script:IconFrames) {
        $logo.Source = $script:IconFrames | Sort-Object PixelWidth | Where-Object { $_.PixelWidth -ge 128 } | Select-Object -First 1
    }
    $script:NavBar = $ui.Tabs.Template.FindName('NavBar', $ui.Tabs)
    $script:NavCrumb = $ui.Tabs.Template.FindName('NavCrumb', $ui.Tabs)
    $back = $ui.Tabs.Template.FindName('NavBack', $ui.Tabs)
    if ($back) { $back.Add_Click({ Show-Page $HubIndex }) }
    Build-Hub
    $ui.Tabs.SelectedIndex = $HubIndex
    Update-Hub
    Start-Live
    Invoke-Safe {
        Invoke-Analysis
        Build-GamingTab
        Update-StartupList
        Update-NetInfo
        Update-BackupSummary
        Update-HistoryList
    }
    Invoke-Safe {
        if (-not $script:SecurityBuilt) { $script:SecurityBuilt = $true; Update-SecurityTab }
        Update-Hub
        Set-Status 'Prêt.'
        Invoke-WelcomeChecks
    }
    Invoke-Safe {
        Update-GameCache
        Update-GameWatch
        Register-FpsHotkey
        if (Get-Setting 'NetWatch' $false) { Set-NetWatch $true }
    }
    try { Invoke-UpdateCheck } catch { Write-Log "Vérification de mise à jour: $_" }
})
