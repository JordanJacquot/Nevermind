# OptiGame : historique des changements, signalement d'un problème, visite guidée et nouveautés.
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
    if ($h.Type -eq 'dns') { $h.Servers = @(@($h.Servers) | Where-Object { $_ } | ForEach-Object { [string]$_ }) }
    $h
}

function Import-History {
    $script:History = New-Object System.Collections.ArrayList
    if (-not (Test-Path -LiteralPath $HistoryFile)) { return }
    try {
        $arr = ConvertFrom-Json (Get-Content -LiteralPath $HistoryFile -Raw -Encoding UTF8)
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
    try { ConvertTo-Json -InputObject $out -Depth 6 | Set-Content -LiteralPath $HistoryFile -Encoding UTF8 } catch { Write-Log "Écriture de l'historique impossible: $_" }
}

# Ajoute une ligne à l'historique (seulement si quelque chose a vraiment changé). Retourne son identifiant.
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

function Set-HistoryAllUndone {
    if ($null -eq $script:History) { Import-History }
    foreach ($h in $script:History) { $h.Annule = $true }
    Save-History
    Update-HistoryList
}

function Update-HistoryList {
    if ($null -eq $script:History) { Import-History }
    $panel = $ui.HistoryPanel
    $panel.Children.Clear()
    if (-not $script:History.Count) {
        [void]$panel.Children.Add((New-Text 'Aucun changement pour le moment.' 13 '#5B6475'))
        return
    }
    foreach ($h in @($script:History | Select-Object -First 15)) {
        $row = New-Grid @('*', 'Auto')
        $row.Margin = New-Thickness 0 0 0 8
        $sp = New-Object System.Windows.Controls.StackPanel
        $t = New-Text $h.Titre 13.5 $(if ($h.Annule) { '#5B6475' } else { '#FFFFFF' }) -Semi
        $t.TextTrimming = 'CharacterEllipsis'; $t.TextWrapping = 'NoWrap'; $t.ToolTip = (@($h.Titre) + @($h.Details)) -join "`n"
        [void]$sp.Children.Add($t)
        [void]$sp.Children.Add((New-Text $h.Date 12 '#9AA3B2'))
        Add-ToGrid $row $sp 0
        if ($h.Annule) {
            $b = New-Badge 'Annulé' '#9AA3B2'
            $b.VerticalAlignment = 'Center'
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
    if ($script:History.Count -gt 15) { [void]$panel.Children.Add((New-Text "et $($script:History.Count - 15) changement(s) plus ancien(s)." 12 '#5B6475')) }
}

function Undo-HistoryEntry([string]$Id) {
    $h = @($script:History | Where-Object { $_.Id -eq $Id }) | Select-Object -First 1
    if (-not $h -or $h.Annule) { return }
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
# OptiGame tourne en administrateur : tout ce qu'il lance hériterait de ces droits. En passant
# par l'Explorateur Windows (déjà ouvert en utilisateur normal), le programme démarre sans eux.
function Open-Url([string]$Target) {
    try { Start-Process -FilePath (Join-Path $env:windir 'explorer.exe') -ArgumentList "`"$Target`"" -ErrorAction Stop }
    catch { Start-Process $Target }
}

# Même chose pour un programme avec des arguments : via un raccourci temporaire.
function Start-Unelevated([string]$Path, [string]$Arguments) {
    if (-not $Arguments) { Open-Url $Path; return }
    $lnk = Join-Path $env:TEMP "OptiGame-$([IO.Path]::GetFileNameWithoutExtension($Path)).lnk"
    $sh = New-Object -ComObject WScript.Shell
    $sc = $sh.CreateShortcut($lnk)
    $sc.TargetPath = $Path; $sc.Arguments = $Arguments; $sc.WorkingDirectory = Split-Path $Path -Parent
    $sc.Save()
    Open-Url $lnk
}

# Réaffiche la fenêtre (cachée quand elle a été réduite).
function Show-MainWindow {
    try {
        if (-not $Window.IsVisible) { $Window.Show() }
        $Window.WindowState = if ($script:StateBeforeTray -eq 'Maximized') { 'Maximized' } else { 'Normal' }
        [void]$Window.Activate()
    } catch {}
}

function Get-TrayIcon {
    if (-not $script:NotifyIcon) {
        $ni = New-Object System.Windows.Forms.NotifyIcon
        $ico = Join-Path $AppDir 'OptiGame.ico'
        $ni.Icon = if (Test-Path -LiteralPath $ico) { New-Object System.Drawing.Icon (New-Object IO.MemoryStream (, [IO.File]::ReadAllBytes($ico))) } else { [System.Drawing.SystemIcons]::Information }
        $ni.Text = 'OptiGame'
        $menu = New-Object System.Windows.Forms.ContextMenuStrip
        [void]$menu.Items.Add('Ouvrir OptiGame', $null, { Show-MainWindow })
        [void]$menu.Items.Add('Quitter', $null, { $Window.Close() })
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

# Fenêtre réduite : elle quitte la barre des tâches et reste près de l'horloge.
function Hide-ToTray {
    $ni = Get-TrayIcon
    $ni.Visible = $true
    $Window.Hide()
    if (-not (Get-Setting 'TrayHintShown' $false)) {
        Set-Setting 'TrayHintShown' $true
        Show-Notify 'OptiGame reste ouvert' 'Il continue en arrière plan (mode jeu, mesure des FPS). Clique sur son icône près de l''horloge pour le rouvrir.'
    }
}

function Show-Notify([string]$Title, [string]$Text, [scriptblock]$OnClick) {
    if ($script:Closing) { Write-Log "$Title : $Text"; return }   # l'app se ferme : la bulle disparaîtrait aussitôt
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
        if ($env:USERNAME) { $t = $t -replace [regex]::Escape($env:USERNAME), '<utilisateur>' }
        if ($env:COMPUTERNAME) { $t = $t -replace [regex]::Escape($env:COMPUTERNAME), '<pc>' }
        $t
    }
    $tmp = Join-Path $env:TEMP "OptiGame-probleme-$(Get-Random)"
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    try {
        $os = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' -ErrorAction SilentlyContinue
        $info = @(
            "OptiGame $AppVersion$(if (Get-Setting 'Beta' $false) { ' (versions bêta activées)' })",
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
            if ($script:ReportPage -eq $HubIndex) { $page = 'Ordinateur' } elseif ($script:ReportPage -eq $NetIndex) { $page = 'Réseau' } elseif ($script:ReportPage -eq $TrafficIndex) { $page = 'Trafic' }
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
        foreach ($n in 'sauvegarde.json', 'historique.json', 'parametres.json', 'ignores.json') {
            $p = Join-Path $DataDir $n
            if (Test-Path -LiteralPath $p) { Set-Content -LiteralPath (Join-Path $tmp $n) -Value (& $mask (Get-Content -LiteralPath $p -Raw -Encoding UTF8)) -Encoding UTF8 }
        }
        $folder = if ($Dest) { $Dest } else { [Environment]::GetFolderPath('Desktop') }
        $zip = Join-Path $folder "OptiGame problème $(Get-Date -Format 'yyyy-MM-dd HH.mm').zip"
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
    Show-Message "Le fichier « $(Split-Path $zip -Leaf) » est sur ton bureau.`n`nEnvoie-le à la personne qui t'a donné OptiGame (par Discord par exemple), avec une phrase qui explique ce qui ne va pas.`n`nIl ne contient ni tes fichiers, ni ton nom, ni tes mots de passe."
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
    [void]$body.Children.Add((New-Text 'Explique en une ou deux phrases ce qui ne va pas (ce que tu faisais, ce qui s''est passé) :' 13 '#E6E8EE'))
    $tb = New-Object System.Windows.Controls.TextBox
    $tb.Height = 90; $tb.Margin = New-Thickness 0 8 0 0
    $tb.AcceptsReturn = $true; $tb.TextWrapping = 'Wrap'; $tb.VerticalScrollBarVisibility = 'Auto'
    $tb.FontSize = 13; $tb.Padding = New-Thickness 8 6 8 6
    $tb.Background = Get-Brush '#0E1116'; $tb.Foreground = Get-Brush '#FFFFFF'; $tb.BorderBrush = Get-Brush '#2C3342'; $tb.CaretBrush = Get-Brush '#FFFFFF'
    [void]$body.Children.Add($tb)
    $n = New-Text 'OptiGame crée un fichier .zip sur ton bureau avec ta phrase, les infos du PC (Windows, composants, score) et le journal de l''app. Il ne contient ni tes fichiers, ni ton nom, ni tes mots de passe. Envoie-le à la personne qui t''a donné OptiGame (Discord par exemple).' 12 '#9AA3B2'
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
    @{ Title = 'Bienvenue dans OptiGame'; Lines = @(
        'OptiGame analyse ton PC et le règle pour que tes jeux tournent au mieux.',
        'Rien n''est modifié sans ton accord : chaque changement passe par un bouton sur lequel tu cliques.') },
    @{ Title = 'Ton score'; Lines = @(
        'Sur l''accueil « Ordinateur », la note Optimisation montre ce qui freine tes jeux.',
        'Ouvre le Tableau de bord pour voir chaque point : clique dessus, lis la fiche, puis « Exécuter » pour que l''app le corrige.',
        'La section Réseau scanne les appareils de ta maison et vérifie leur sécurité.') },
    @{ Title = 'Tout est annulable'; Lines = @(
        'Chaque changement est sauvegardé avant d''être fait.',
        'Après une correction, « Revenir en arrière » annule tout de suite. Plus tard, la page Sauvegarde garde l''historique : tu peux annuler n''importe quel changement.',
        'Un souci ? Le bouton « Signaler un problème », en haut à droite, crée un fichier à envoyer.') }
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
        $t = New-Text $l 14 '#E6E8EE'
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
        try { $starts = @(Select-String -LiteralPath $LogFile -Pattern 'Démarrage OptiGame' -SimpleMatch).Count } catch {}
        if ($starts -gt 1) { Set-Setting 'TourDone' $true; $tourDone = $true; $last = 'précédente' }
    }
    if (-not $tourDone) {
        Set-Setting 'LastVersion' $AppVersion
        Show-Tour
        return
    }
    Show-WhatsNew $last
}
