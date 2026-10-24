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
    # Barre des tâches : bouton Nevermind avec son icône, et une épingle qui relance Mindset.exe de ce dossier (le programme
    # qui fait tourner Nevermind), pas PowerShell ; Nevermind.exe (son relais) s'il manque
    try {
        $exe = @((Join-Path (Split-Path $AppDir -Parent) 'Mindset.exe'), (Join-Path (Split-Path $AppDir -Parent) 'Nevermind.exe') | Where-Object { Test-Path -LiteralPath $_ })[0]
        $relaunch = if ($exe) { "`"$exe`"" } else { '' }
        [void][AppIdentity]::SetWindow($h, $AppUserModelId, $relaunch, "$IconPath,0", 'Nevermind')
    } catch { Write-Log "Barre des tâches : $_" }
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

$Window.Add_StateChanged({
    if ($script:StartHidden) { return }   # lancement avec Windows : la fenêtre se prépare d'abord, réduite
    if ($Window.WindowState -eq 'Minimized') { try { Hide-ToTray } catch { Write-Log "Réduction: $_" } }
    else { $script:StateBeforeTray = [string]$Window.WindowState }
})
# Mindset quitté : il demande la fermeture (quitter.demande) et attend 6 s. Nevermind quitte comme par « Quitter »
# près de l'horloge (overlay arrêté, minuteries, icône retirée), sans la question de la croix.
# Une action en cours (optimisation, nettoyage...) a 3,5 s pour finir ; au delà, on ferme quand même : la sauvegarde
# est écrite avant chaque réglage modifié, elle reste juste (« Tout restaurer » remet tout comme avant, sauf les changements définitifs qui protègent le PC).
function Invoke-RequestedQuit {
    if ($script:AllowClose) { return }
    $busy = [bool]($script:Starting -or $script:WaitFrames.Count -or $Window.Cursor)
    if ($busy -and ((Get-Date) - $script:QuitAsked).TotalSeconds -lt 3.5) { return }
    if ($busy) { Write-Log 'Fermeture demandée par Mindset : action encore en cours, fermeture quand même' }
    try { if ($script:QuitDialog) { $script:QuitDialog.Close() } } catch {}
    $script:AllowClose = $true
    $Window.Close()
}

# Affiché dans Mindset : Mindset accroche la fenêtre dans sa zone centrale (fenêtre enfant) et la décroche à tout
# moment. Accrochée, elle n'a plus de taille minimale (Mindset choisit sa taille) ; décrochée, tout redevient comme avant.
# Lu toutes les 600 ms (Watch-Requests) : un crochet sur les messages de la fenêtre ferait passer chaque message par PowerShell.
function Update-EmbedState {
    $h = (New-Object System.Windows.Interop.WindowInteropHelper $Window).Handle
    $on = [EmbedInfo]::IsEmbedded($h)
    if ($on -eq [bool]$script:Embedded) { return }
    $script:Embedded = $on
    # La barre de Mindset cède la place à celle de la console (Menu et Fermer y sont), et les icônes du haut changent
    try { [EmbedInfo]::BarrePropre($h, ($on -and $script:Kit -and $script:Kit.Actif)) } catch {}
    if ($on -and $script:Kit -and $script:Kit.Actif) { try { Update-EcranNevermind } catch {} }
    if ($on) {
        $script:WindowMin = @($Window.MinWidth, $Window.MinHeight)
        $Window.MinWidth = 0; $Window.MinHeight = 0
        # Lancée cachée près de l'horloge (-Demarrage) puis montrée par Mindset de l'extérieur : pour WPF elle est toujours
        # cachée et sa zone resterait noire. On la montre pour de bon, sans l'activer (elle reste dans Mindset).
        if ($Window.WindowState -eq 'Minimized') { $Window.WindowState = 'Normal' }
        if (-not $Window.IsVisible) {
            $act = $Window.ShowActivated
            $Window.ShowActivated = $false
            $Window.Show()
            $Window.ShowActivated = $act
        }
    } else {
        if ($script:WindowMin) { $Window.MinWidth = $script:WindowMin[0]; $Window.MinHeight = $script:WindowMin[1] }
        # Décrochée, Mindset la cache lui-même : WPF l'apprend ici, et l'icône près de l'horloge permet de la rouvrir
        if ($Window.IsVisible -and -not [EmbedInfo]::IsShown($h)) { try { Hide-ToTray } catch { Write-Log "Réduction: $_" } }
    }
    Write-Log $(if ($on) { 'Fenêtre affichée dans Mindset' } else { 'Fenêtre décrochée de Mindset' })
}

# Demandes déposées par un autre programme : afficher la fenêtre (nouveau lancement de Nevermind) ou fermer (Mindset)
function Watch-Requests {
    try { Update-EmbedState } catch {}
    if ([IO.File]::Exists($QuitRequest)) {
        try { [IO.File]::Delete($QuitRequest) } catch {}
        if (-not $script:QuitAsked) { $script:QuitAsked = Get-Date; Write-Log 'Fermeture demandée par Mindset' }
    }
    if ($script:QuitAsked) { Invoke-RequestedQuit; return }
    if ([IO.File]::Exists($ShowRequest)) {
        $demande = ''
        try { $demande = ([IO.File]::ReadAllText($ShowRequest)).Trim() } catch {}
        try { [IO.File]::Delete($ShowRequest) } catch {}
        # Clic sur une bulle que Mindset a montrée pour Nevermind : ce qu'elle annonçait. Seul un numéro donné par
        # Show-Notify est accepté : rien de ce qui est écrit dans ce fichier n'est exécuté.
        if ($demande -match '^notif:([0-9a-f]{12})$' -and $script:NotifyActions -and $script:NotifyActions.ContainsKey($Matches[1])) {
            $aFaire = $script:NotifyActions[$Matches[1]]   # (pas « $action » : c'est le paramètre d'Invoke-Safe, qui se rappellerait lui-même)
            $script:NotifyActions.Remove($Matches[1])
            Invoke-Safe { & $aFaire }
        }
        # Affichée dans Mindset : c'est lui qui la montre, on ne touche pas à son état (elle sortirait de Mindset)
        elseif (-not $script:Embedded) { try { Show-MainWindow } catch { Write-Log "Affichage: $_" } }
    }
}
# Un nouveau lancement de Nevermind demande d'afficher cette fenêtre (elle peut être cachée près de l'horloge)
if (-not $env:OPTIGAME_TEST) {
    $script:ShowWatch = New-Object System.Windows.Threading.DispatcherTimer
    $script:ShowWatch.Interval = [TimeSpan]::FromMilliseconds(600)
    # (protégée de l'intérieur : une minuterie dont le passage lève une erreur n'est plus relancée par WPF, et Nevermind
    # n'entendrait plus les ordres de Mindset : fermer, afficher une page, clic sur une bulle)
    $script:ShowWatch.Add_Tick({ try { Watch-Requests } catch { Write-Log "Demandes de Mindset : $_" } })
    $script:ShowWatch.Start()
}
# Partie en cours : le jeu n'est pas au premier plan et Nevermind est à l'écran (seul ou dans Mindset). On regarde
# alors Nevermind ou Mindset, pas le jeu : un thème qui vient d'y être choisi peut s'appliquer tout de suite.
function Test-ThemeSansGener {
    try {
        $fg = [WinFocus]::Pid([WinFocus]::Foreground())
        if (-not $fg -or ($script:GameSession -and $fg -eq $script:GameSession.Pid)) { return $false }
        if ($fg -eq $PID) { return $true }
        $h = (New-Object System.Windows.Interop.WindowInteropHelper $Window).Handle
        [bool]($script:Embedded -and [EmbedInfo]::IsShown($h))
    } catch { $false }
}

# Géré par Mindset : le thème de la suite changé ailleurs (Mindset, Evermind) est suivi. Toutes les 2,5 s, seule la
# date du fichier est lue ; il n'est relu que si elle a changé. Nouveau thème (un des 5, ou un pack de la suite
# installé par Mindset dans les packs de Nevermind) : enregistré, puis Nevermind se relance
# (comme un changement de thème ici). Pas de boucle : après la relance, son thème est celui de la suite.
# (Une copie de test qui simule Mindset, OPTIGAME_SUITE, le suit aussi : sans relance, voir Set-AppTheme.)
if ($Managed -and (-not $env:OPTIGAME_TEST -or $env:OPTIGAME_SUITE)) {
    $script:SuiteWatch = New-Object System.Windows.Threading.DispatcherTimer
    $script:SuiteWatch.Interval = [TimeSpan]::FromMilliseconds(2500)
    $script:SuiteWatch.Add_Tick({ try { Watch-SuiteTheme } catch { Write-Log "Thème de la suite : $_" } })   # (protégée de l'intérieur)
    $script:SuiteWatch.Start()
}
function Watch-SuiteTheme {
    $stamp = try { [IO.File]::GetLastWriteTimeUtc($SuiteFile) } catch { $script:SuiteStamp }
    if ($stamp -eq $script:SuiteStamp -or $script:AllowClose) { return }
    # Chargement ou action en cours : on attendra le prochain passage (date non retenue)
    if ($script:Starting -or $script:WaitFrames.Count) { return }
    $t = (Read-Suite).Theme
    if (-not $t -or $t -ceq (Get-CurrentThemeKey)) { $script:SuiteStamp = $stamp; return }
    # Partie en cours : pas de relance pendant qu'on joue. Elle attend la fin de la partie, ou que Nevermind ou
    # Mindset soit regardé (le thème vient d'y être choisi, le jeu est en arrière-plan). Noté une fois au journal.
    if (($script:GameSession -or $script:LagSession) -and -not (Test-ThemeSansGener)) {
        if ($script:SuiteThemeAttente -cne $t) { $script:SuiteThemeAttente = $t; Write-Log "Thème de la suite changé dans Mindset : $t, appliqué après la partie en cours (ou dès que Nevermind ou Mindset est à l'écran)" }
        return
    }
    $script:SuiteStamp = $stamp
    if ($t) {
        # Pack de la suite pas (encore) installé ou illisible : thème actuel gardé, noté une fois au journal, et
        # suite.json relu au passage suivant (le pack peut arriver juste après)
        if ($t -like 'pack:*' -and -not (Get-ThemePack $t.Substring(5))) {
            if ($script:SuitePackMissing -cne $t) { $script:SuitePackMissing = $t; Write-Log "Thème de la suite changé dans Mindset : $t absent ou illisible dans $PacksDir, thème actuel gardé" }
            $script:SuiteStamp = [datetime]::MinValue
            return
        }
        Write-Log "Thème de la suite changé dans Mindset : $t"
        Invoke-Safe { Set-AppTheme $t }
    }
}

# Croix (ou Alt + F4) : on demande avant de quitter. Les fermetures voulues (Quitter près de l'horloge,
# mise à jour, changement de thème) et l'arrêt du PC passent sans question.
[Microsoft.Win32.SystemEvents]::add_SessionEnding({ $script:AllowClose = $true })
$Window.Add_Closing({
    param($s, $e)
    if ($script:AllowClose -or $env:OPTIGAME_TEST) { return }
    $e.Cancel = $true
    $choice = ''
    try { $choice = Show-QuitDialog } catch { Write-Log "Fenêtre Quitter : $_"; $choice = 'quit' }
    if ($choice -eq 'quit') { $script:AllowClose = $true; $null = $Window.Dispatcher.BeginInvoke([Action]{ $Window.Close() }) }
    elseif ($choice -eq 'tray') { try { Hide-ToTray } catch { Write-Log "Réduction: $_" } }
})

$Window.Add_Closed({
    $script:Closing = $true
    try { if ($script:ShowWatch) { $script:ShowWatch.Stop() } } catch {}
    try { if ($script:SuiteWatch) { $script:SuiteWatch.Stop() } } catch {}
    $Live.Run = $false
    if ($script:LiveTimer) { $script:LiveTimer.Stop() }
    try { Stop-FpsTarget } catch { Write-Log "Fermeture, mesure des FPS: $_" }
    try { if ($script:GameSession) { Stop-GameSession } } catch { Write-Log "Fermeture, mode jeu: $_" }
    try { Unregister-FpsHotkey } catch {}
    try { Stop-OrgWatch; if ($script:OrgBar) { $script:OrgBar.Win.Close() } } catch {}
    try { if ($script:TrafficTimer) { $script:TrafficTimer.Stop() } } catch {}
    try { [FrameMon]::Stop() } catch {}
    try { if ($script:LagSession) { $script:LagSession = $null; if ($script:LagTimer) { $script:LagTimer.Stop() }; [LagMon]::Stop() } } catch {}
    try { if ([NetFlow]::Running) { [NetFlow]::Stop() } } catch {}
    try { if ($script:NotifyIcon) { $script:NotifyIcon.Visible = $false; $script:NotifyIcon.Dispose() } } catch {}
    if ($Splash) { try { $Splash.Close() } catch {} }
    # En dernier : fin de la boucle de l'app.
    [System.Windows.Threading.Dispatcher]::CurrentDispatcher.InvokeShutdown()
})

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
    # (Ctrl+K, la recherche : accueil-console.ps1)
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
$ui.BtnRefreshStartup.Add_Click({ Invoke-Safe { Clear-StartupItemsCache; Update-StartupList } })
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
        $r = try { [void](Invoke-Aide 'restauration.creer' @{ Description = 'Nevermind (manuel)' } 600000); 'OK' } catch { $_.Exception.Message }
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
# Filet contre les plantages : une erreur imprévue sur la fenêtre (clic, minuterie) ne la ferme plus, voir Write-ErreurNonGeree.
# Une erreur sur un autre fil (rare : code natif) ferme l'appli quoi qu'on fasse : sa cause est au moins notée au journal.
$Window.Dispatcher.Add_UnhandledException({
    param($s, $e)
    $e.Handled = $true
    try { Write-ErreurNonGeree $e.Exception } catch {}
})
try { [FiletPlantage]::Installer($LogFile) } catch {}
$ui.Tabs.Add_SelectionChanged({
    param($s, $e)
    if ($e.OriginalSource -ne $ui.Tabs) { return }
    if ($ui.Tabs.SelectedIndex -eq $HubIndex -and $script:HubPret) { Invoke-Safe { Update-Hub }; return }
    if ($ui.Tabs.SelectedIndex -eq $GamesIndex) {
        Invoke-Safe {
            Build-GamesTabs
            if (-not $script:LibBuilt) { Build-Library } elseif ($script:GamesView -ne 'perf') { Update-LibraryView }
            if ((Test-PerfShown 'lag') -and $script:LagPanelDirty) { $script:LagPanelDirty = $false; Build-LagPanel }
        }
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
Initialize-Suite
$ui.BtnCheckUpdate.Add_Click({
    if ($script:PendingUpdate) { Invoke-Safe { Install-Update } } else { Invoke-Safe { Invoke-UpdateCheck -Manual } }
})

$Window.Add_ContentRendered({
    if ($Splash) { try { $Splash.Close() } catch {}; $script:Splash = $null }
    if ($ui.VersionText) { $ui.VersionText.Text = "Nevermind $AppVersion" }
    # L'en-tête des pages d'avant encore montrées (l'Organizer) : habillé par la console (Initialize-HabillageConsole)
    $script:NavBar = $ui.Tabs.Template.FindName('NavBar', $ui.Tabs)
    $script:NavCrumb = $ui.Tabs.Template.FindName('NavCrumb', $ui.Tabs)
    try { Initialize-AccueilConsole } catch { Show-EcranImpossible "accueil : $($_.Exception.Message)" }
    if ($script:EcranImpossible) { return }   # (pas d'écran : Nevermind se ferme, rien d'autre ne démarre)
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
        $script:HubPret = $true   # (l'accueil peut se mettre à jour : Update-Hub)
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
            # (déjà accrochée dans Mindset pendant le chargement : elle y reste affichée)
            if (-not $script:Embedded) {
                try { Hide-ToTray } catch { Write-Log "Réduction: $_" }
                # Préparée hors de l'écran (OptiGame.ps1) : une fois cachée, elle reprend sa place au centre de l'écran
                if ($Window.Left -le -30000) {
                    $wa = [System.Windows.SystemParameters]::WorkArea
                    $Window.Left = $wa.Left + [math]::Max(0.0, ($wa.Width - $Window.Width) / 2)
                    $Window.Top = $wa.Top + [math]::Max(0.0, ($wa.Height - $Window.Height) / 2)
                }
            }
            & $t
        } else { Hide-StartupOverlay }
    }
})
