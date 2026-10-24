# Nevermind : historique des changements, signalement d'un problème, visite guidée et nouveautés.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Historique des changements (chaque ligne peut être annulée séparément)
# ---------------------------------------------------------------------------
$HistoryFile = Join-Path $DataDir 'historique.json'

# Remet les valeurs lues dans le JSON au bon type pour le registre.
function ConvertFrom-HistoryEntry($E) {
    $h = @{}
    foreach ($p in $E.PSObject.Properties) { $h[$p.Name] = $p.Value }
    if ($h.Type -eq 'reg' -and $h.Existed) {
        switch ([string]$h.Kind) {
            'Binary'      { $h.Value = [byte[]]@(@($h.Value) | ForEach-Object { [byte]$_ }) }
            'DWord'       { $h.Value = [int]$h.Value }
            'QWord'       { $h.Value = [long]$h.Value }
            'MultiString' { $h.Value = [string[]]@($h.Value) }
            default       { $h.Value = [string]$h.Value }
        }
    }
    if ($h.Type -eq 'dns') {
        $h.Servers = @(@($h.Servers) | Where-Object { $_ } | ForEach-Object { [string]$_ })
        if ($h.ContainsKey('Nouveau')) { $h.Nouveau = @(@($h.Nouveau) | Where-Object { $_ } | ForEach-Object { [string]$_ }) }
    }
    $h
}

function Import-History {
    $script:History = New-Object System.Collections.ArrayList
    try {
        $arr = Read-JsonSur $HistoryFile
        if ($null -eq $arr) { return }
        foreach ($x in @($arr)) {
            if (-not $x) { continue }
            [void]$script:History.Add(@{
                Id = [string]$x.Id; Date = [string]$x.Date; Titre = [string]$x.Titre; Annule = [bool]$x.Annule
                Details = @(@($x.Details) | Where-Object { $_ } | ForEach-Object { [string]$_ })
                Log = @(@($x.Log) | Where-Object { $_ } | ForEach-Object { ConvertFrom-HistoryEntry $_ })
            })
        }
    } catch { Write-Log "Lecture de l'historique impossible: $_" }
}

function Save-History {
    $out = @(foreach ($h in $script:History) {
        $log = @(foreach ($e in $h.Log) {
            $c = @{}
            foreach ($k in @($e.Keys)) { $v = $e[$k]; if ($v -is [byte[]]) { $v = [int[]]$v }; $c[$k] = $v }
            $c
        })
        [ordered]@{ Id = $h.Id; Date = $h.Date; Titre = $h.Titre; Details = @($h.Details); Log = $log; Annule = $h.Annule }
    })
    try { Save-JsonSur $HistoryFile $out } catch { Write-Log "Écriture de l'historique impossible: $_" }
}

# Ajoute une ligne à l'historique (seulement si quelque chose a vraiment changé). Retourne son identifiant.
# Un changement que « Tout restaurer » n'annule pas (il protège le PC) : noté dans le journal de l'action en cours, ou
# directement dans l'historique, avec la façon de revenir en arrière à la main
function Add-ChangementDefinitif([string]$Titre, [string]$Quoi, [string]$Retour) {
    $e = @{ Type = 'definitif'; Quoi = $Quoi; Retour = $Retour }
    if ($null -ne $script:RunLog) { [void]$script:RunLog.Add($e) } else { [void](Add-History $Titre @($Quoi) @($e)) }
}
# Une entrée dont rien ne s'annule : seulement des changements définitifs
function Test-HistoriqueDefinitif($H) { $l = @($H.Log); $l.Count -and -not @($l | Where-Object { $_.Type -ne 'definitif' }).Count }
# Une entrée avec des changements définitifs et d'autres annulables : une fois annulée, les définitifs restent en place
function Test-HistoriquePartDefinitive($H) { [bool]@(@($H.Log) | Where-Object { $_.Type -eq 'definitif' }).Count }
# La façon de revenir en arrière d'une entrée (changements définitifs)
function Get-RetourDefinitif($H) { (@($H.Log) | Where-Object { $_.Type -eq 'definitif' -and $_.Retour } | ForEach-Object { [string]$_.Retour } | Select-Object -Unique) -join ' ' }

# Les détails techniques d'une entrée de l'historique : ce qui a été changé exactement, avant et après (issue #7)
function Format-ValeurTech($V) {
    if ($null -eq $V) { return 'vide' }
    if ($V -is [array]) { $t = (@($V) | Select-Object -First 16) -join ','; if (@($V).Count -gt 16) { $t += '...' }; return $t }
    $t = [string]$V; if ($t.Length -gt 120) { $t = $t.Substring(0, 117) + '...' }; $t
}
# La nouvelle valeur (« Nouveau ») est notée au moment du changement ; les entrées plus anciennes n'ont que l'avant.
function Test-ChampHisto($E, [string]$Nom) { if ($E -is [hashtable]) { return $E.ContainsKey($Nom) }; $null -ne $E.PSObject.Properties[$Nom] }
function Format-DemarrageService([string]$T) {
    switch ($T) { 'Automatic' { 'automatique' } 'Manual' { 'manuel' } 'Disabled' { 'désactivé' } default { $T } }
}
function Format-PlanAlim([string]$Guid, [string]$Nom) {
    if (-not $Nom) {
        $Nom = switch ($Guid) { $HighPerfGuid { 'Performances élevées' } $BalancedGuid { 'Utilisation normale' } $UltimateGuid { 'Performances optimales' } default { '' } }
        if (-not $Nom -and $Guid -and $Guid -eq $script:Backup.CreatedScheme) { $Nom = 'copie de Performances élevées' }
    }
    if ($Nom) { "« $Nom » ($Guid)" } else { $Guid }
}
function Format-CurseurAlim([string]$Guid) { if ($Guid -eq $BestPerfOverlay) { "« Meilleures performances » ($Guid)" } else { $Guid } }
function Format-ServeursDns($S) { $l = @(@($S) | Where-Object { $_ }); if ($l.Count) { $l -join ', ' } else { 'automatique (box)' } }
function Get-DetailsTechniques($H) {
    @(foreach ($e in @($H.Log)) {
        $apres = Test-ChampHisto $e 'Nouveau'
        switch ([string]$e.Type) {
            'reg' {
                $avant = if ($e.Existed) { Format-ValeurTech $e.Value } else { 'absente' }
                $ap = if ($e.Efface) { 'effacée' } elseif ($apres) { Format-ValeurTech $e.Nouveau } else { 'modifiée' }
                "Registre $($e.Path) : valeur « $($e.Name) », avant $avant, après $ap"
            }
            'svc'     { "Service $($e.Name) : démarrage avant $(Format-DemarrageService $e.StartType)$(if ($e.Running) { ', il tournait' })$(if ($apres) { ", après $(Format-DemarrageService $e.Nouveau), arrêté" })" }
            'power'   { "Plan d'alimentation : avant $(Format-PlanAlim $e.Guid $e.Nom)$(if ($apres) { ", après $(Format-PlanAlim $e.Nouveau)" })" }
            'pcfg' {
                # (le nom du réglage : noté avec la valeur, ou retrouvé par son identifiant pour les anciennes entrées)
                $k = if ($e.Key) { [string]$e.Key } else { @(@($BatSettings.Keys) | Where-Object { $BatSettings[$_] -eq $e.Guid })[0] }
                if ($k -and $BatNoms[$k]) {
                    "Réglage d'alimentation, $($BatNoms[$k]) : avant secteur $(Format-ValeurBat $k $e.Ac), batterie $(Format-ValeurBat $k $e.Dc)$(if ($apres) { ", après $(Format-ValeurBat $k $e.Nouveau) (secteur et batterie)" })"
                } else {
                    "Réglage d'alimentation $($e.Guid) : avant secteur $($e.Ac), batterie $($e.Dc)$(if ($apres) { ", après $($e.Nouveau) (secteur et batterie)" })"
                }
            }
            'display' { "Écran $($e.Device) : fréquence avant $($e.Hz) Hz$(if ($apres) { ", après $($e.Nouveau) Hz" })" }
            'overlay' { "Curseur du mode d'alimentation de Windows : avant $(Format-CurseurAlim $e.Guid)$(if ($apres) { ", après $(Format-CurseurAlim $e.Nouveau)" })" }
            'dns'     { "DNS de la carte réseau : avant $(Format-ServeursDns $e.Servers)$(if ($apres) { ", après $(Format-ServeursDns $e.Nouveau)" })" }
            # (une règle n'est ajoutée que pour bloquer un programme : avant, il n'y en avait pas)
            'fw'      { "Pare-feu : règle « $($e.Name) », avant aucune règle, après Internet bloqué pour ce programme" }
            'definitif' { "$($e.Quoi) (définitif)" }
        }
    })
}

function Add-History([string]$Titre, [string[]]$Details, $Log) {
    if (-not $Log -or -not @($Log).Count) { return $null }
    if ($null -eq $script:History) { Import-History }
    $id = [guid]::NewGuid().ToString('N').Substring(0, 12)
    $script:History.Insert(0, @{ Id = $id; Date = (Get-Date).ToString('dd/MM/yyyy à HH:mm'); Titre = $Titre; Details = @($Details | Where-Object { $_ }); Log = @($Log); Annule = $false })
    while ($script:History.Count -gt 100) { $script:History.RemoveAt($script:History.Count - 1) }
    Save-History
    Update-HistoryList
    $id
}

# Titre lisible d'une fiche de résultat : les lignes « •  ... » décrivent ce qui a été fait.
function Get-HistoryItems([string[]]$Lines) { @($Lines | Where-Object { $_ -like '•*' } | ForEach-Object { ($_ -replace '^•\s*', '').Trim() }) }
function Get-HistoryTitle([string]$Title, [string[]]$Lines) {
    $items = @(Get-HistoryItems $Lines)
    if ($items.Count -eq 1) { return $items[0] }
    if ($items.Count -gt 1) { return "$($items.Count) changements : $(($items | Select-Object -First 3) -join ', ')$(if ($items.Count -gt 3) { '...' })" }
    $Title
}

function Set-HistoryUndone([string]$Id) {
    if (-not $Id -or $null -eq $script:History) { return }
    foreach ($h in $script:History) { if ($h.Id -eq $Id) { $h.Annule = $true } }
    Save-History
    Update-HistoryList
}

# (« Tout restaurer » n'annule pas les changements définitifs : une entrée qui n'a que ça reste « Définitif ». Une entrée
# dont un réglage n'a pas pu être remis, toujours dans la sauvegarde, reste « à annuler » : Test-HistoriqueRemis)
function Set-HistoryAllUndone {
    if ($null -eq $script:History) { Import-History }
    foreach ($h in $script:History) { if (-not $h.Annule -and -not (Test-HistoriqueDefinitif $h) -and (Test-HistoriqueRemis $h)) { $h.Annule = $true } }
    Save-History
    Update-HistoryList
}

function Update-HistoryList {
    if ($null -eq $script:History) { Import-History }
    $panel = $ui.HistoryPanel
    $panel.Children.Clear()
    if (-not $script:History.Count) {
        [void]$panel.Children.Add((New-Text 'Aucun changement pour le moment.' 13 '#655E7E'))
        return
    }
    foreach ($h in @($script:History | Select-Object -First 15)) {
        $row = New-Grid @('*', 'Auto')
        $row.Margin = New-Thickness 0 0 0 8
        $sp = New-Object System.Windows.Controls.StackPanel
        $t = New-Text $h.Titre 13.5 $(if ($h.Annule) { '#655E7E' } else { '#FFFFFF' }) -Semi
        $t.TextTrimming = 'CharacterEllipsis'; $t.TextWrapping = 'NoWrap'; $t.ToolTip = (@($h.Titre) + @($h.Details) + @(Get-DetailsTechniques $h)) -join "`n"
        [void]$sp.Children.Add($t)
        [void]$sp.Children.Add((New-Text $h.Date 12 '#A6A1BC'))
        Add-ToGrid $row $sp 0
        if ($h.Annule -or (Test-HistoriqueDefinitif $h)) {
            $partiel = $h.Annule -and (Test-HistoriquePartDefinitive $h)
            $b = New-Badge $(if ($partiel) { 'Annulé en partie' } elseif ($h.Annule) { 'Annulé' } else { 'Définitif' }) '#A6A1BC'
            $b.VerticalAlignment = 'Center'
            if ($partiel) { $b.ToolTip = "Les protections du PC restent en place. Pour revenir en arrière : $(Get-RetourDefinitif $h)" }
            elseif (-not $h.Annule) { $b.ToolTip = "Pour revenir en arrière : $(Get-RetourDefinitif $h)" }
        } else {
            $b = New-Button 'Annuler'
            $b.VerticalAlignment = 'Center'
            $b.Tag = $h.Id
            $b.Add_Click({ param($s, $e) Invoke-Safe { Undo-HistoryEntry $s.Tag } })
        }
        $b.Margin = New-Thickness 12 0 0 0
        Add-ToGrid $row $b 1
        [void]$panel.Children.Add($row)
    }
    if ($script:History.Count -gt 15) { [void]$panel.Children.Add((New-Text "et $($script:History.Count - 15) changement(s) plus ancien(s)." 12 '#655E7E')) }
}

function Undo-HistoryEntry([string]$Id) {
    $h = @($script:History | Where-Object { $_.Id -eq $Id }) | Select-Object -First 1
    if (-not $h -or $h.Annule -or (Test-HistoriqueDefinitif $h)) { return }
    if (-not (Confirm-Action "Annuler ce changement ?`n`n« $($h.Titre) »`n$($h.Date)`n`nLes réglages concernés reviennent à ce qu'ils étaient juste avant.")) { return }
    Set-Busy $true
    Set-Status 'Retour en arrière...'
    $errors = Undo-RunLog $h.Log
    $h.Annule = $true
    Save-History
    Build-GamingTab
    Update-StartupList
    Update-BackupSummary
    Update-NetInfo
    Update-HistoryList
    Invoke-Analysis
    $lines = @("« $($h.Titre) » est annulé.")
    if ($errors) { $lines += 'Pas pu être restauré :'; $lines += $errors }
    Set-Status 'Changement annulé.'
    Show-ResultSheet 'Changement annulé' $lines $null $null
}

# ---------------------------------------------------------------------------
# Notifications Windows (bulle près de l'horloge)
# ---------------------------------------------------------------------------
# Nevermind tourne en administrateur : tout ce qu'il lance hériterait de ces droits. En passant
# par l'Explorateur Windows (déjà ouvert en utilisateur normal), le programme démarre sans eux.
function Open-Url([string]$Target) {
    try { Start-Process -FilePath (Join-Path $env:windir 'explorer.exe') -ArgumentList "`"$Target`"" -ErrorAction Stop }
    catch { Start-Process $Target }
}

# Même chose pour un programme avec des arguments : via un raccourci temporaire.
function Start-Unelevated([string]$Path, [string]$Arguments) {
    if (-not $Arguments) { Open-Url $Path; return }
    $lnk = Join-Path $env:TEMP "Nevermind-$([IO.Path]::GetFileNameWithoutExtension($Path)).lnk"
    $sh = New-Object -ComObject WScript.Shell
    $sc = $sh.CreateShortcut($lnk)
    $sc.TargetPath = $Path; $sc.Arguments = $Arguments; $sc.WorkingDirectory = Split-Path $Path -Parent
    $sc.Save()
    Open-Url $lnk
}

# Réaffiche la fenêtre (cachée quand elle a été réduite).
function Show-MainWindow {
    # Géré par Mindset et pas (encore) accroché : c'est Mindset qui l'affiche, dans son onglet
    if ($Managed -and -not $script:Embedded -and -not $env:OPTIGAME_TEST -and (Open-DansMindset)) { return }
    try {
        if (-not $Window.IsVisible) { $Window.Show() }
        if ($script:Embedded) { return }   # affichée dans Mindset : la montrer dans sa zone suffit, sans changer son état
        if ($Window.WindowState -eq 'Minimized') { $Window.WindowState = if ($script:StateBeforeTray -eq 'Maximized') { 'Maximized' } else { 'Normal' } }
        # Passe devant toutes les fenêtres, puis redevient une fenêtre normale
        $Window.Topmost = $true; $Window.Topmost = $false
        [void]$Window.Activate()
    } catch {}
}

function Get-TrayIcon {
    if (-not $script:NotifyIcon) {
        $ni = New-Object System.Windows.Forms.NotifyIcon
        $ico = Join-Path $AppDir 'OptiGame.ico'
        $ni.Icon = if (Test-Path -LiteralPath $ico) { New-Object System.Drawing.Icon (New-Object IO.MemoryStream (, [IO.File]::ReadAllBytes($ico))) } else { [System.Drawing.SystemIcons]::Information }
        $ni.Text = 'Nevermind'
        $menu = New-Object System.Windows.Forms.ContextMenuStrip
        [void]$menu.Items.Add('Ouvrir Nevermind', $null, { Show-MainWindow })
        [void]$menu.Items.Add('Quitter', $null, { $script:AllowClose = $true; $Window.Close() })
        $ni.ContextMenuStrip = $menu
        $ni.Add_MouseClick({ param($s, $e) if ([string]$e.Button -eq 'Left') { Show-MainWindow } })
        $ni.Add_BalloonTipClicked({
            Show-MainWindow
            $a = $script:NotifyAction; $script:NotifyAction = $null
            if ($a) { Invoke-Safe { & $a } }
        })
        $script:NotifyIcon = $ni
    }
    $script:NotifyIcon
}

# Croix de la fenêtre : « Voulez-vous vraiment quitter ? », Quitter ou Réduire près de l'horloge.
# Renvoie 'quit', 'tray' ou '' (Échap ou clic à côté : on ne fait rien).
function Show-QuitDialog {
    $d = New-Object System.Windows.Window
    $d.WindowStyle = 'None'; $d.AllowsTransparency = $true; $d.ResizeMode = 'NoResize'
    $d.Background = [System.Windows.Media.Brushes]::Transparent
    $d.SizeToContent = 'WidthAndHeight'; $d.ShowInTaskbar = $false; $d.Title = 'Nevermind'
    if ($Window.IsVisible) { $d.Owner = $Window; $d.WindowStartupLocation = 'CenterOwner' } else { $d.WindowStartupLocation = 'CenterScreen' }
    $d.Resources = $Window.Resources
    $d.FontFamily = $Window.FontFamily
    $card = New-Object System.Windows.Controls.Border
    $card.Width = 380
    $card.CornerRadius = [System.Windows.CornerRadius]::new($(if (($PackFont -and $ThemePack.FontPixel) -or $Theme.Decor -eq 'terminal') { 4 } else { 20 }))
    $card.Background = Get-Brush '#F20B0820'
    $card.BorderBrush = New-LinearBrush @('#9900E5FF', '#22FFFFFF', '#99FF2EB5') 0 0 1 1
    $card.BorderThickness = New-Thickness 1 1 1 1
    $card.Padding = New-Thickness 26 22 26 22
    $card.Margin = New-Thickness 14 14 14 14
    $card.Effect = New-Glow '#00E5FF' 24 0.35
    $sp = New-Object System.Windows.Controls.StackPanel
    $t = New-Text 'Quitter Nevermind ?' 18 '#FFFFFF' -Bold
    Set-PackFont $t
    [void]$sp.Children.Add($t)
    $q = New-Text 'Voulez-vous vraiment quitter ?' 13.5 '#EEEBF7'
    $q.Margin = New-Thickness 0 10 0 0
    [void]$sp.Children.Add($q)
    $h = New-Text 'En réduisant, Nevermind reste ouvert près de l''horloge : mode jeu, compteur de FPS et organizer continuent.' 12 '#8E88A8'
    $h.Margin = New-Thickness 0 6 0 0
    [void]$sp.Children.Add($h)
    $row = New-Object System.Windows.Controls.StackPanel
    $row.Orientation = 'Horizontal'; $row.HorizontalAlignment = 'Right'; $row.Margin = New-Thickness 0 20 0 0
    $bTray = New-Button 'Réduire' 'BtnSecondary'
    $bTray.Margin = New-Thickness 0 0 10 0
    $bQuit = New-Button 'Quitter' 'BtnPrimary'
    $script:QuitChoice = ''
    $bTray.Add_Click({ $script:QuitChoice = 'tray'; $script:QuitDialog.Close() })
    $bQuit.Add_Click({ $script:QuitChoice = 'quit'; $script:QuitDialog.Close() })
    [void]$row.Children.Add($bTray); [void]$row.Children.Add($bQuit)
    [void]$sp.Children.Add($row)
    $card.Child = $sp
    $d.Content = $card
    $d.Add_KeyDown({ param($s, $e) if ($e.Key -eq 'Escape') { $s.Close() } elseif ($e.Key -eq 'Return') { $script:QuitChoice = 'quit'; $s.Close() } })
    $d.Add_MouseLeftButtonDown({ param($s, $e) try { $s.DragMove() } catch {} })
    $script:QuitDialog = $d
    $d.Add_ContentRendered({ $script:QuitButtons = @($bTray, $bQuit); [void]$bQuit.Focus() })
    [void]$d.ShowDialog()
    $script:QuitDialog = $null
    $script:QuitChoice
}

# Fenêtre réduite : elle quitte la barre des tâches et reste près de l'horloge.
function Hide-ToTray {
    if ($Managed) { $Window.Hide(); return }   # (géré par Mindset : pas d'icône à Nevermind près de l'horloge, c'est celle de Mindset)
    $ni = Get-TrayIcon
    $ni.Visible = $true
    $Window.Hide()
    if (-not (Get-Setting 'TrayHintShown' $false)) {
        Set-Setting 'TrayHintShown' $true
        Show-Notify 'Nevermind reste ouvert' 'Il continue en arrière-plan (mode jeu, mesure des FPS). Clique sur son icône près de l''horloge pour le rouvrir.'
    }
}

# ---------------------------------------------------------------------------
# Raccourci sur le bureau et lancement au démarrage (page Sauvegarde)
# ---------------------------------------------------------------------------
function Update-ShortcutCard {
    $has = Test-DesktopShortcut
    $ui.BtnShortcut.Content = if ($has) { 'Recréer le raccourci' } else { 'Créer le raccourci' }
    $ui.ShortcutStatus.Text = if ($has) { 'Le raccourci Nevermind est sur ton bureau.' } else { 'Mets Nevermind sur ton bureau pour l''ouvrir en un double clic.' }
    # Copie gérée : raccourci et démarrage avec Windows se règlent dans Mindset (interrupteur « S'ouvre avec Mindset »)
    if (Test-DemarrageParMindset) {
        $ui.ShortcutStatus.Text = 'Le raccourci et le démarrage avec Windows se règlent dans Mindset : interrupteur « S''ouvre avec Mindset » sur la carte de Nevermind, dans l''accueil de Mindset.'
        $ui.BtnShortcut.Content = 'Réglages de Mindset'
        return
    }
    if ($Managed) { $ui.ShortcutStatus.Text = 'Le raccourci sur le bureau est géré par Mindset.' }   # bouton caché (Initialize-Suite)
    # Lire les tâches planifiées prend environ 0,3 s : pendant le chargement, en arrière plan pour ne pas figer l'animation
    $ui.ChkAutoStart.IsChecked = if ($script:Starting) {
        [bool](@(Invoke-Async { param($n) [bool](Get-ScheduledTask -TaskName $n -ErrorAction SilentlyContinue) } $AutoStartTask)[-1])
    } else { Test-AutoStart }
}

function Invoke-CreateShortcut {
    if (Test-DemarrageParMindset) { Open-MindsetSettings 'general'; return }   # (bouton « Réglages de Mindset »)
    if ($Managed) { return }   # Mindset crée le raccourci de la copie qu'il gère
    New-DesktopShortcut
    Update-ShortcutCard
    Set-Status 'Raccourci Nevermind créé sur le bureau.'
}

function Set-AutoStartFromUi([bool]$On) {
    try { Set-AutoStart $On }
    catch {
        $ui.ChkAutoStart.IsChecked = -not $On
        Write-Log "Démarrage automatique: $_"
        Show-Message "Impossible de $(if ($On) { 'programmer' } else { 'retirer' }) le lancement au démarrage :`n`n$($_.Exception.Message)" 'Warning'
        return
    }
    Write-Log "Démarrage automatique $(if ($On) { 'activé' } else { 'désactivé' })"
    Set-Status $(if ($On) { 'Nevermind se lancera au démarrage du PC, réduit près de l''horloge.' } else { 'Nevermind ne se lancera plus au démarrage du PC.' })
}

function Show-Notify([string]$Title, [string]$Text, [scriptblock]$OnClick) {
    if ($script:Closing) { Write-Log "$Title : $Text"; return }   # l'app se ferme : la bulle disparaîtrait aussitôt
    if ($Managed) {
        # Géré par Mindset : la bulle est montrée par Mindset, sur son icône (notification.demande). Un clic dessus revient
        # ici par afficher.demande (« notif:<numéro> ») pour faire ce qu'elle annonçait (Watch-Requests).
        $id = [guid]::NewGuid().ToString('N').Substring(0, 12)
        if (-not $script:NotifyActions) { $script:NotifyActions = @{} }
        if ($OnClick) { $script:NotifyActions[$id] = $OnClick }
        try { [IO.File]::WriteAllText((Join-Path $DataDir 'notification.demande'), (ConvertTo-Json -InputObject @{ titre = $Title; texte = $Text; id = $id } -Compress), (New-Object Text.UTF8Encoding $false)) }
        catch { Write-Log "Notification impossible: $_" }
        Set-Status "$Title : $Text"
        return
    }
    $script:NotifyAction = $OnClick
    try {
        [void](Get-TrayIcon)
        $script:NotifyIcon.Visible = $true
        $script:NotifyIcon.ShowBalloonTip(8000, $Title, $Text, [System.Windows.Forms.ToolTipIcon]::Info)
    } catch { Write-Log "Notification impossible: $_" }
    Set-Status "$Title : $Text"
}

# ---------------------------------------------------------------------------
# Signaler un problème : un zip sur le bureau, sans données personnelles
# ---------------------------------------------------------------------------
function Export-ProblemReport([string]$Dest, [string]$Description) {
    Set-Busy $true
    Set-Status 'Préparation du fichier...'
    $mask = {
        param($t)
        $t = [string]$t
        # Le dossier du profil peut porter un autre nom que le compte (compte Microsoft : 5 premières lettres de l'e-mail...)
        $prof = if ($env:USERPROFILE) { Split-Path $env:USERPROFILE -Leaf } else { '' }
        if ($prof -and $prof.Length -ge 3) { $t = $t -replace "(?i)\\Users\\$([regex]::Escape($prof))(?=\\|\b|$)", '\Users\<utilisateur>' }
        if ($env:USERNAME) { $t = $t -replace [regex]::Escape($env:USERNAME), '<utilisateur>' }
        if ($env:COMPUTERNAME) { $t = $t -replace [regex]::Escape($env:COMPUTERNAME), '<pc>' }
        $t
    }
    $tmp = Join-Path $env:TEMP "Nevermind-probleme-$(Get-Random)"
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    try {
        $os = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' -ErrorAction SilentlyContinue
        $info = @(
            "Nevermind $AppVersion$(if (Get-Setting 'Beta' $false) { ' (versions bêta activées)' })",
            "Date : $(Get-Date -Format 'dd/MM/yyyy HH:mm')",
            "Windows : $($os.ProductName) $($os.DisplayVersion) (build $($os.CurrentBuildNumber).$($os.UBR))",
            "Langue : $((Get-Culture).Name), interface $((Get-UICulture).Name)",
            "PowerShell : $($PSVersionTable.PSVersion)",
            "Écran principal : $([System.Windows.SystemParameters]::PrimaryScreenWidth) x $([System.Windows.SystemParameters]::PrimaryScreenHeight)",
            "Dossier de l'app : $AppDir"
        )
        $a = $script:LastAnalysis
        if ($a) {
            $info += ''
            $info += "Score d'optimisation : $($a.Score)"
            foreach ($k in $a.Info.Keys) { $info += "$k : $($a.Info[$k])" }
            $info += ''
            $info += 'Points analysés :'
            foreach ($f in $a.Findings) { $info += "[$($f.Status)] $($f.Titre)" }
        }
        if ($null -ne $script:SecurityScore) { $info += ''; $info += "Protection : $($script:SecurityScore) sur 100" }
        Set-Content -LiteralPath (Join-Path $tmp 'infos.txt') -Value (& $mask ($info -join "`r`n")) -Encoding UTF8
        if ($Description.Trim()) {
            $page = if ($script:ReportPage -ge 0) { [string]$PageNames[$script:ReportPage] } else { '' }
            if ($script:ReportPage -eq $HubIndex) { $page = 'Ordinateur' } elseif ($script:ReportPage -eq $NetIndex) { $page = 'Réseau' } elseif ($script:ReportPage -eq $TrafficIndex) { $page = 'Trafic' } elseif ($script:ReportPage -eq $OverlayIndex) { $page = 'Overlay' }
            Set-Content -LiteralPath (Join-Path $tmp 'description.txt') -Value (& $mask "Ce qui ne va pas :`r`n$($Description.Trim())`r`n`r`nPage ouverte : $page") -Encoding UTF8
        }
        $errs = @(foreach ($e in @($global:Error | Select-Object -First 80)) {
            $msg = if ($e.Exception) { $e.Exception.Message } else { "$e" }
            $pos = if ($e.InvocationInfo) { (($e.InvocationInfo.PositionMessage -split "`r?`n") | Select-Object -First 1) } else { '' }
            "$msg`r`n    $pos"
        })
        Set-Content -LiteralPath (Join-Path $tmp 'erreurs.txt') -Value (& $mask ($errs -join "`r`n")) -Encoding UTF8
        if (Test-Path -LiteralPath $LogFile) {
            Set-Content -LiteralPath (Join-Path $tmp 'journal.txt') -Value (& $mask ((Get-Content -LiteralPath $LogFile -Encoding UTF8 -Tail 400) -join "`r`n")) -Encoding UTF8
        }
        # Journal de l'Aide (gardé en mémoire par elle, 200 lignes) : seulement si elle tourne, sans la réveiller pour ça
        if ([AideClient]::Joignable(1)) {
            try {
                $lignesAide = @(Invoke-Aide 'aide.journal' @{} 10000 | ForEach-Object { [string]$_ })
                Set-Content -LiteralPath (Join-Path $tmp 'journal-aide.txt') -Value (& $mask ($lignesAide -join "`r`n")) -Encoding UTF8
            } catch {}
        }
        foreach ($n in 'sauvegarde.json', 'historique.json', 'parametres.json', 'ignores.json') {
            $p = Join-Path $DataDir $n
            if (Test-Path -LiteralPath $p) { Set-Content -LiteralPath (Join-Path $tmp $n) -Value (& $mask (Get-Content -LiteralPath $p -Raw -Encoding UTF8)) -Encoding UTF8 }
        }
        $folder = if ($Dest) { $Dest } else { [Environment]::GetFolderPath('Desktop') }
        $zip = Join-Path $folder "Nevermind problème $(Get-Date -Format 'yyyy-MM-dd HH.mm').zip"
        # Archive créée en arrière plan : la fenêtre ne se fige pas
        [void](Invoke-Async {
            param($a)
            Add-Type -AssemblyName System.IO.Compression.FileSystem
            if (Test-Path -LiteralPath $a.Zip) { [IO.File]::Delete($a.Zip) }
            [IO.Compression.ZipFile]::CreateFromDirectory($a.Dir, $a.Zip)
        } @{ Dir = $tmp; Zip = $zip })
        if (-not (Test-Path -LiteralPath $zip)) { throw 'Le fichier n''a pas pu être créé.' }
    } finally {
        try { [IO.Directory]::Delete($tmp, $true) } catch {}
    }
    Set-Status 'Fichier créé sur le bureau.'
    if ($Dest) { return $zip }
    Start-Process explorer.exe -ArgumentList "/select,`"$zip`""
    Show-Message "Le fichier « $(Split-Path $zip -Leaf) » est sur ton bureau.`n`nEnvoie-le à la personne qui t'a donné Nevermind (par Discord par exemple), avec une phrase qui explique ce qui ne va pas.`n`nIl ne contient ni tes fichiers, ni ton nom, ni tes mots de passe."
}

function Show-ReportPanel {
    if ($script:TestRunning) { return }
    $script:ReportPage = $ui.Tabs.SelectedIndex
    Show-TestPanel @{ Tag = '!'; Title = 'Signaler un problème'; Sub = 'Un fichier à envoyer, sans données personnelles' }
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    Set-TestState 'info' 'Rapport'
    $body = $ui.TestBody
    [void]$body.Children.Add((New-Text 'Explique en une ou deux phrases ce qui ne va pas (ce que tu faisais, ce qui s''est passé) :' 13 '#EEEBF7'))
    $tb = New-Object System.Windows.Controls.TextBox
    $tb.Height = 90; $tb.Margin = New-Thickness 0 8 0 0
    $tb.AcceptsReturn = $true; $tb.TextWrapping = 'Wrap'; $tb.VerticalScrollBarVisibility = 'Auto'
    $tb.FontSize = Get-UiFontSize 13; $tb.Padding = New-Thickness 8 6 8 6
    $tb.Background = Get-Brush '#0E0C16'; $tb.Foreground = Get-Brush '#FFFFFF'; $tb.BorderBrush = Get-Brush '#2AFFFFFF'; $tb.CaretBrush = Get-Brush '#FFFFFF'
    [void]$body.Children.Add($tb)
    $n = New-Text 'Nevermind crée un fichier .zip sur ton bureau avec ta phrase, les infos du PC (Windows, composants, score) et le journal de Nevermind. Il ne contient ni tes fichiers, ni ton nom, ni tes mots de passe. Envoie-le à la personne qui t''a donné Nevermind (Discord par exemple).' 12 '#A6A1BC'
    $n.Margin = New-Thickness 0 10 0 0
    [void]$body.Children.Add($n)
    $wp = New-Object System.Windows.Controls.WrapPanel
    $wp.Margin = New-Thickness 0 12 0 0
    $b = New-Button 'Créer le fichier' 'BtnPrimary'
    $b.Margin = New-Thickness 0 0 10 0
    $b.Tag = $tb
    $b.Add_Click({ param($s, $e) $d = [string]$s.Tag.Text; Invoke-Safe { Hide-TestPanel; Export-ProblemReport '' $d } })
    [void]$wp.Children.Add($b)
    [void]$body.Children.Add($wp)
    $script:ReportBox = $tb
    [void]$tb.Focus()
}

# ---------------------------------------------------------------------------
# Visite guidée (première ouverture) et nouveautés (après une mise à jour)
# ---------------------------------------------------------------------------
$TourSteps = @(
    @{ Title = 'Bienvenue dans Nevermind'; Lines = @(
        'Nevermind analyse ton PC et le règle pour que tes jeux tournent au mieux.',
        'Rien n''est modifié sans ton accord : chaque changement passe par un bouton sur lequel tu cliques.') },
    @{ Title = 'Les onglets'; Lines = @(
        'En haut, les onglets : Ordinateur, Jeux, Réseau, Trafic et Overlay.',
        'Ordinateur range les outils en trois familles : Performances, Santé du PC et Protection. Le tableau de bord montre ce qui freine tes jeux : clique sur un point, lis la fiche, puis « Exécuter » pour que Nevermind le corrige.',
        'Jeux réunit tous tes jeux : tu les lances et tu optimises chacun depuis sa fiche.',
        'Réseau scanne les appareils de ta maison et vérifie leur sécurité.') },
    @{ Title = 'Tu peux revenir en arrière'; Lines = @(
        'Chaque changement est sauvegardé avant d''être fait.',
        'Après une correction, « Revenir en arrière » annule tout de suite. Plus tard, la page Sauvegarde (dans Ordinateur, famille Protection) garde l''historique : tu peux annuler chaque changement, sauf quatre protections du PC qui restent en place (marquées « Définitif »).',
        'En haut, la barre de recherche trouve un réglage (ou Ctrl + K) et la roue crantée ouvre les Paramètres. « Signaler un problème » est dans l''onglet Aide des Paramètres.') }
)

function Show-Tour {
    Set-Setting 'TourDone' $true
    Show-TourStep 0
}

function Show-TourStep([int]$Index) {
    if ($Index -ge $TourSteps.Count) { Close-Sheet; $script:SheetMode = 'fix'; return }
    $script:SheetMode = 'tour'
    $script:TourStep = $Index
    $st = $TourSteps[$Index]
    $body = $ui.SheetBody
    $body.Children.Clear()
    [void]$body.Children.Add((New-Text "Étape $($Index + 1) sur $($TourSteps.Count)" 12.5 $Colors.ok -Semi))
    $h = New-Text $st.Title 22 '#FFFFFF' -Bold
    $h.Margin = New-Thickness 0 4 0 6
    [void]$body.Children.Add($h)
    foreach ($l in $st.Lines) {
        $t = New-Text $l 14 '#EEEBF7'
        $t.Margin = New-Thickness 0 8 0 0
        [void]$body.Children.Add($t)
    }
    $ui.SheetRun.Content = if ($Index -eq $TourSteps.Count - 1) { 'C''est parti' } else { 'Suivant' }
    $ui.SheetRun.Visibility = 'Visible'
    $ui.SheetOpen.Visibility = 'Collapsed'
    $ui.SheetIgnore.Visibility = 'Collapsed'
    $ui.SheetClose.Content = 'Passer'
    $ui.SheetClose.Visibility = if ($Index -eq $TourSteps.Count - 1) { 'Collapsed' } else { 'Visible' }
    $ui.Overlay.Visibility = 'Visible'
}

# Au lancement : visite guidée la première fois, « Quoi de neuf » après une mise à jour.
function Invoke-WelcomeChecks {
    $last = [string](Get-Setting 'LastVersion' '')
    $tourDone = [bool](Get-Setting 'TourDone' $false)
    if (-not $last -and -not $tourDone) {
        # Déjà utilisateur avant l'arrivée de la visite guidée : pas de visite, mais les nouveautés.
        $starts = 0
        try { $starts = @(Select-String -LiteralPath $LogFile -Pattern 'Démarrage (OptiGame|Nexo|Nevermind)').Count } catch {}
        if ($starts -gt 1) { Set-Setting 'TourDone' $true; $tourDone = $true; $last = 'précédente' }
    }
    if (-not $tourDone) {
        Set-Setting 'LastVersion' $AppVersion
        Show-Tour
        return
    }
    Show-WhatsNew $last
}
