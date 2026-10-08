# Nevermind : constats, score, fiches de correction et retour en arrière.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Tableau de bord
# ---------------------------------------------------------------------------
function Add-Finding($List, [string]$Status, [string]$Titre, [string]$Detail, [int]$Weight, [string]$Action, [string]$ActionLabel, [string]$Id, $Fix) {
    if (-not $Id) { $Id = $Titre }
    [void]$List.Add([pscustomobject]@{
        Id = $Id; Status = $Status; Titre = $Titre; Detail = $Detail; Weight = $Weight
        Action = $Action; ActionLabel = $ActionLabel; Fix = $Fix; Gain = 0
    })
}

# Décrit comment corriger un point.
#   -Auto  : l'app règle le problème toute seule avec le bouton Exécuter.
#   sinon  : l'utilisateur suit les étapes (Steps), l'app peut l'aider avec un bouton (Run / Open).
function New-Fix {
    param(
        [switch]$Auto,
        [string[]]$What,
        [string]$Why,
        [string[]]$Steps,
        [scriptblock]$Run,
        $RunArgs,
        [string]$RunLabel = 'Exécuter',
        [string]$Confirm,
        [string]$Done,
        [switch]$NoRescan,
        [string]$Open,
        [string]$OpenLabel = 'Ouvrir',
        [switch]$Reboot,
        [switch]$Restore
    )
    @{
        Auto = [bool]$Auto; What = $What; Why = $Why; Steps = $Steps
        Run = $Run; Args = $RunArgs; RunLabel = $RunLabel; Confirm = $Confirm; Done = $Done; NoRescan = [bool]$NoRescan
        Open = $Open; OpenLabel = $OpenLabel; Reboot = [bool]$Reboot; Restore = [bool]$Restore
    }
}

$BiosRun = {
    shutdown.exe /r /fw /t 10
    if ($LASTEXITCODE) { throw "Ce PC ne permet pas de redémarrer directement dans le BIOS (code $LASTEXITCODE). Redémarre et appuie sur Suppr ou F2 pendant le démarrage." }
}
$BiosConfirm = "Le PC va redémarrer directement dans le BIOS dans 10 secondes.`n`nEnregistre ton travail et ferme tes jeux avant. Continuer ?"
$BiosDone = 'Redémarrage dans le BIOS dans 10 secondes...'

function Invoke-CleanAll {
    [void](Invoke-CleanTargets $CleanTargets)
}

function Get-DriverLink([string]$Name) {
    if ($Name -match 'NVIDIA|GeForce') { return 'https://www.nvidia.com/fr-fr/drivers/' }
    if ($Name -match 'AMD|Radeon')     { return 'https://www.amd.com/fr/support/download/drivers.html' }
    if ($Name -match 'Intel')          { return 'https://www.intel.fr/content/www/fr/fr/support/detect.html' }
    'ms-settings:windowsupdate'
}

# ---------------------------------------------------------------------------
# Score et gains
# ---------------------------------------------------------------------------
# Un point vert compte entièrement, un orange à 40 %, un rouge pas du tout.
# Le gain d'un point = ce que le score gagnerait s'il passait au vert.
function Measure-Score($Findings) {
    $active = @($Findings | Where-Object { $script:Ignored -notcontains $_.Id })
    $scored = @($active | Where-Object { $_.Weight -gt 0 -and $_.Status -ne 'info' })
    $total = 0.0; $got = 0.0; $autoRaw = 0.0
    foreach ($item in $scored) {
        $credit = switch ($item.Status) { 'ok' { 1.0 } 'warn' { 0.4 } default { 0.0 } }
        $total += $item.Weight
        $got += $item.Weight * $credit
        $item.Gain = $item.Weight * (1 - $credit)
        if ($item.Fix -and $item.Fix.Auto -and $item.Status -ne 'ok') { $autoRaw += $item.Gain }
    }
    foreach ($item in $active) {
        $item.Gain = if ($total -and $scored -contains $item) { [int][math]::Round(100 * $item.Gain / $total) } else { 0 }
    }
    $score = if ($total) { [int][math]::Round(100 * $got / $total) } else { 100 }
    $potential = if ($total) { [int][math]::Round(100 * ($got + $autoRaw) / $total) } else { 100 }
    @{ Score = $score; Potential = $potential; Active = $active }
}

function Show-Score([int]$Score, [int]$Bad, [int]$Warn, [int]$Potential) {
    if ($Score -ge 85)     { $label = 'Excellent';     $color = $Colors.ok }
    elseif ($Score -ge 65) { $label = 'Bien';          $color = '#9BE15D' }
    elseif ($Score -ge 45) { $label = 'À améliorer';   $color = $Colors.warn }
    else                   { $label = 'Mal optimisé';  $color = $Colors.bad }
    $len = [math]::PI * (130 - 10) / 10
    $ui.ScoreRing.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(($len * $Score / 100), 1000))
    $ui.ScoreRing.Stroke = Get-Brush $color
    $ui.ScoreText.Text = [string]$Score
    $ui.ScoreLabel.Text = $label
    $ui.ScoreLabel.Foreground = Get-Brush $color
    $parts = @()
    if ($Bad)  { $parts += "$Bad problème$(if ($Bad -gt 1) {'s'})" }
    if ($Warn) { $parts += "$Warn à améliorer" }
    $ui.ScoreCounts.Text = if ($parts) { $parts -join ', ' } else { 'Rien à signaler' }
    $ui.ScorePotential.Text = if ($Potential -gt $Score) { "Jusqu'à $Potential en un clic" } else { '' }
    @{ Label = $label; Color = $color }
}

# ---------------------------------------------------------------------------
# « Pour gagner des points » et recommandations
# ---------------------------------------------------------------------------
function Get-FixKind($f) {
    if ($f.Fix -and $f.Fix.Auto) { return @{ Text = "L'app s'en charge"; Color = $Colors.ok } }
    @{ Text = 'À faire toi même'; Color = $Colors.info }
}

function Show-Improvements($Active) {
    $panel = $ui.ImprovePanel
    $panel.Children.Clear()
    $items = @($Active | Where-Object { $_.Status -ne 'ok' -and $_.Gain -gt 0 } |
        Sort-Object @{ Expression = { $_.Gain }; Descending = $true }, @{ Expression = { -not ($_.Fix -and $_.Fix.Auto) } })
    $auto = @($items | Where-Object { $_.Fix -and $_.Fix.Auto })

    if (-not $items.Count) {
        [void]$panel.Children.Add((New-Text "Rien à gagner de plus : ton PC est au top pour le jeu !" 13 $Colors.ok -Semi))
        $ui.ImproveSub.Text = ''
        $ui.BtnFixAll.Visibility = 'Collapsed'
        return
    }
    $ui.ImproveSub.Text = "$($items.Count) amélioration$(if ($items.Count -gt 1) {'s'}) possible$(if ($items.Count -gt 1) {'s'}), dont $($auto.Count) que l'app peut faire pour toi. Clique sur une ligne pour voir le détail."
    if ($auto.Count) {
        $sum = ($auto | Measure-Object Gain -Sum).Sum
        $ui.BtnFixAll.Content = "Tout corriger (+$sum pts)"
        $ui.BtnFixAll.Visibility = 'Visible'
    } else {
        $ui.BtnFixAll.Visibility = 'Collapsed'
    }

    foreach ($f in $items) {
        $row = New-Object System.Windows.Controls.Border
        $row.CornerRadius = [System.Windows.CornerRadius]::new(10)
        $row.Padding = New-Thickness 12 10 12 10
        $row.Margin = New-Thickness 0 0 0 6
        $row.Background = Get-Brush '#10FFFFFF'
        $row.Cursor = [System.Windows.Input.Cursors]::Hand
        $row.Tag = $f
        $g = New-Grid @('Auto', '*', 'Auto', 'Auto')

        $pill = New-Object System.Windows.Controls.Border
        $pill.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $pill.Width = 64
        $pill.Padding = New-Thickness 0 5 0 5
        $pbg = Get-Brush $Colors.ok; $pbg.Opacity = 0.16
        $pill.Background = $pbg
        $pt = New-Text "+$($f.Gain) pts" 13 $Colors.ok -Bold
        $pt.HorizontalAlignment = 'Center'; $pt.TextWrapping = 'NoWrap'
        $pill.Child = $pt
        $pill.VerticalAlignment = 'Center'
        Add-ToGrid $g $pill 0

        $title = New-Text $f.Titre 14 '#FFFFFF' -Semi
        $title.Margin = New-Thickness 14 0 10 0
        $title.VerticalAlignment = 'Center'
        Add-ToGrid $g $title 1

        $k = Get-FixKind $f
        $badge = New-Badge $k.Text $k.Color
        $badge.Margin = New-Thickness 0 0 12 0
        Add-ToGrid $g $badge 2

        $chev = New-Text '›' 22 '#A6A1BC' -Bold
        $chev.VerticalAlignment = 'Center'
        $chev.Margin = New-Thickness 0 -4 0 0
        Add-ToGrid $g $chev 3

        $row.Child = $g
        $row.Add_MouseEnter({ param($s, $e) $s.Background = Get-Brush '#252B37' })
        $row.Add_MouseLeave({ param($s, $e) $s.Background = Get-Brush '#10FFFFFF' })
        $row.Add_MouseLeftButtonUp({ param($s, $e) Open-Sheet @($s.Tag) })
        [void]$panel.Children.Add($row)
    }
}

function Add-FindingCard($Panel, $f, [switch]$Ignored) {
    $card = New-Card
    $g = New-Grid @('Auto', '*', 'Auto')
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 12; $dot.Height = 12
    $dot.Fill = Get-Brush $(if ($Ignored) { $Muted } else { $Colors[$f.Status] })
    $dot.VerticalAlignment = 'Top'
    $dot.Margin = New-Thickness 0 4 14 0
    Add-ToGrid $g $dot 0

    $sp = New-Object System.Windows.Controls.StackPanel
    $head = New-Object System.Windows.Controls.WrapPanel
    [void]$head.Children.Add((New-Text $f.Titre 14 '#FFFFFF' -Semi))
    if ($f.Gain -gt 0 -and -not $Ignored) { [void]$head.Children.Add((New-Badge "+$($f.Gain) pts" $Colors.ok)) }
    [void]$sp.Children.Add($head)
    if ($f.Detail) {
        $d = New-Text $f.Detail 12.5 '#A6A1BC'
        $d.Margin = New-Thickness 0 3 0 0
        [void]$sp.Children.Add($d)
    }
    Add-ToGrid $g $sp 1

    $btn = $null
    if ($Ignored) {
        $btn = New-Button 'Ne plus ignorer'
        $btn.Tag = $f
        $btn.Add_Click({ param($s, $e) Invoke-Safe { Set-IgnoreFinding $s.Tag $false } })
    } elseif ($f.Status -ne 'ok' -and $f.Fix) {
        $btn = if ($f.Fix.Auto) { New-Button 'Corriger' 'BtnPrimary' } else { New-Button 'Comment faire' }
        $btn.Tag = $f
        $btn.Add_Click({ param($s, $e) Open-Sheet @($s.Tag) })
    } elseif ($f.Action) {
        $btn = New-Button $f.ActionLabel
        $btn.Tag = $f.Action
        $btn.Add_Click({ param($s, $e) Invoke-FindingAction $s.Tag })
    }
    if ($btn) {
        $btn.Margin = New-Thickness 14 0 0 0
        Add-ToGrid $g $btn 2
    }
    $card.Child = $g
    if ($Ignored) { $card.Opacity = 0.75 }
    [void]$Panel.Children.Add($card)
}

# Affiche d'abord ce qui est à corriger ; les points OK et ignorés sont repliés.
function Show-Findings($All, $Active) {
    $panel = $ui.FindingsPanel
    $panel.Children.Clear()
    $issues = @($Active | Where-Object { $_.Status -ne 'ok' })
    $script:OkFindings = @($Active | Where-Object { $_.Status -eq 'ok' })
    $script:IgnoredFindings = @($All | Where-Object { $script:Ignored -contains $_.Id -and $_.Status -ne 'ok' })
    foreach ($f in $issues) { Add-FindingCard $panel $f }
    if (-not $issues.Count) {
        Add-FindingCard $panel ([pscustomobject]@{ Status = 'ok'; Titre = 'Rien à corriger'; Detail = 'Ton PC est bien réglé pour le jeu.'; Gain = 0; Fix = $null; Action = $null })
    }
    $more = New-Object System.Windows.Controls.WrapPanel
    $more.Margin = New-Thickness 0 4 0 0
    if ($script:OkFindings.Count) {
        $b = New-Button $(if ($script:OkFindings.Count -gt 1) { "Voir les $($script:OkFindings.Count) points déjà OK" } else { 'Voir le point déjà OK' })
        $b.Margin = New-Thickness 0 0 10 0
        $b.Add_Click({
            param($s, $e)
            $s.Visibility = 'Collapsed'
            foreach ($f in $script:OkFindings) { Add-FindingCard $ui.FindingsPanel $f }
        })
        [void]$more.Children.Add($b)
    }
    if ($script:IgnoredFindings.Count) {
        $b = New-Button $(if ($script:IgnoredFindings.Count -gt 1) { "Voir les $($script:IgnoredFindings.Count) points ignorés" } else { 'Voir le point ignoré' })
        $b.Add_Click({
            param($s, $e)
            $s.Visibility = 'Collapsed'
            foreach ($f in $script:IgnoredFindings) { Add-FindingCard $ui.FindingsPanel $f -Ignored }
        })
        [void]$more.Children.Add($b)
    }
    [void]$panel.Children.Add($more)
    $ui.FindingsSummary.Text = "$($issues.Count) point$(if ($issues.Count -gt 1) {'s'}) à regarder" +
        $(if ($script:IgnoredFindings.Count) { ", $($script:IgnoredFindings.Count) ignoré$(if ($script:IgnoredFindings.Count -gt 1) {'s'})" } else { '' })
}

# ---------------------------------------------------------------------------
# Fiche détaillée (fenêtre par dessus l'application)
# ---------------------------------------------------------------------------
function Add-SheetSection([string]$Title, [string[]]$Lines, [switch]$Numbered, [switch]$Bullets) {
    $h = New-Text $Title 13 '#A6A1BC' -Semi
    $h.Margin = New-Thickness 0 18 0 6
    [void]$ui.SheetBody.Children.Add($h)
    $i = 0
    foreach ($l in $Lines) {
        $i++
        $prefix = if ($Numbered) { "$i.  " } elseif ($Bullets) { '•  ' } else { '' }
        $t = New-Text "$prefix$l" 14 '#EEEBF7'
        $t.Margin = New-Thickness $(if ($prefix) { 4 } else { 0 }) 2 0 4
        [void]$ui.SheetBody.Children.Add($t)
    }
}

function Add-SheetInfo([string]$Text, [string]$Color) {
    $b = New-Object System.Windows.Controls.Border
    $bg = Get-Brush $Color; $bg.Opacity = 0.10
    $b.Background = $bg
    $b.CornerRadius = [System.Windows.CornerRadius]::new(8)
    $b.Padding = New-Thickness 12 9 12 9
    $b.Margin = New-Thickness 0 16 0 0
    $b.Child = New-Text $Text 13 $Color
    [void]$ui.SheetBody.Children.Add($b)
}

function Open-Sheet($Items) {
    $script:SheetMode = 'fix'
    $script:SheetItems = @($Items)
    $ui.SheetClose.Content = 'Fermer'
    $ui.SheetClose.Visibility = 'Visible'
    $body = $ui.SheetBody
    $body.Children.Clear()

    if ($script:SheetItems.Count -eq 1) {
        $f = $script:SheetItems[0]
        $fix = $f.Fix
        $isIgnored = $script:Ignored -contains $f.Id

        $head = New-Grid @('Auto', '*')
        $dot = New-Object System.Windows.Shapes.Ellipse
        $dot.Width = 14; $dot.Height = 14
        $dot.Fill = Get-Brush $Colors[$f.Status]
        $dot.Margin = New-Thickness 0 8 14 0
        $dot.VerticalAlignment = 'Top'
        Add-ToGrid $head $dot 0
        Add-ToGrid $head (New-Text $f.Titre 21 '#FFFFFF' -Bold) 1
        [void]$body.Children.Add($head)

        $badges = New-Object System.Windows.Controls.WrapPanel
        $badges.Margin = New-Thickness 18 8 0 0
        if ($f.Gain -gt 0) { [void]$badges.Children.Add((New-Badge "+$($f.Gain) points au score" $Colors.ok)) }
        if ($fix) { $k = Get-FixKind $f; [void]$badges.Children.Add((New-Badge $k.Text $k.Color)) }
        if ($fix -and $fix.Reboot) { [void]$badges.Children.Add((New-Badge 'Redémarrage requis' '#A6A1BC')) }
        foreach ($c in $badges.Children) { $c.Margin = New-Thickness 0 0 8 0 }
        [void]$body.Children.Add($badges)

        if ($f.Detail) { Add-SheetSection "Ce qu'on a trouvé" @($f.Detail) }
        if ($fix -and $fix.Why) { Add-SheetSection 'Pourquoi ça compte' @($fix.Why) }
        if ($fix -and $fix.Auto) {
            Add-SheetSection "Ce que l'app va faire quand tu cliques sur Exécuter" $fix.What -Bullets
            $safe = 'Tu peux revenir en arrière à tout moment depuis l''onglet Sauvegarde.'
            if ($fix.Restore) { $safe = 'Un point de restauration Windows est créé avant. ' + $safe }
            Add-SheetInfo $safe $Colors.ok
        } elseif ($fix) {
            if ($fix.Steps) { Add-SheetSection 'Ce que tu dois faire' $fix.Steps -Numbered }
            if ($fix.What) { Add-SheetSection "Ce que l'app peut faire pour t'aider" $fix.What -Bullets }
        }
        if ($isIgnored) { Add-SheetInfo 'Ce point est ignoré : il ne compte plus dans ton score.' $Colors.info }

        $ui.SheetRun.Visibility = if ($fix -and $fix.Run -and -not $isIgnored) { 'Visible' } else { 'Collapsed' }
        if ($fix) { $ui.SheetRun.Content = $fix.RunLabel }
        $open = if ($fix -and $fix.Open) { $fix.Open } else { $f.Action }
        $ui.SheetOpen.Visibility = if ($open) { 'Visible' } else { 'Collapsed' }
        $ui.SheetOpen.Tag = $open
        $ui.SheetOpen.Content = if ($fix -and $fix.Open) { $fix.OpenLabel } elseif ($f.ActionLabel) { $f.ActionLabel } else { 'Ouvrir' }
        $ui.SheetIgnore.Visibility = if ($f.Status -ne 'ok') { 'Visible' } else { 'Collapsed' }
        $ui.SheetIgnore.Content = if ($isIgnored) { 'Ne plus ignorer' } else { "Ignorer (c'est voulu)" }
    } else {
        $auto = @($script:SheetItems)
        $sum = ($auto | Measure-Object Gain -Sum).Sum
        [void]$body.Children.Add((New-Text 'Tout corriger en un clic' 21 '#FFFFFF' -Bold))
        $s = New-Text "L'app va appliquer $($auto.Count) correction$(if ($auto.Count -gt 1) {'s'}), pour environ +$sum points :" 14 '#A6A1BC'
        $s.Margin = New-Thickness 0 6 0 0
        [void]$body.Children.Add($s)
        foreach ($f in $auto) { Add-SheetSection "$($f.Titre)   (+$($f.Gain) pts)" $f.Fix.What -Bullets }
        $safe = 'Un point de restauration Windows est créé avant. Tu peux tout annuler depuis l''onglet Sauvegarde.'
        if ($auto | Where-Object { $_.Fix.Reboot }) { $safe += ' Certains réglages demandent un redémarrage.' }
        Add-SheetInfo $safe $Colors.ok
        $ui.SheetRun.Visibility = 'Visible'
        $ui.SheetRun.Content = "Exécuter les $($auto.Count) corrections"
        $ui.SheetOpen.Visibility = 'Collapsed'
        $ui.SheetIgnore.Visibility = 'Collapsed'
    }
    $ui.Overlay.Visibility = 'Visible'
}

function Close-Sheet { $ui.Overlay.Visibility = 'Collapsed' }

function Open-FixAll {
    if (-not $script:LastAnalysis) { return }
    $auto = @($script:LastAnalysis.Active | Where-Object { $_.Status -ne 'ok' -and $_.Gain -gt 0 -and $_.Fix -and $_.Fix.Auto } |
        Sort-Object Gain -Descending)
    if ($auto.Count) { Open-Sheet $auto }
}

function Invoke-SheetRun {
    $items = @($script:SheetItems | Where-Object { $_.Fix -and $_.Fix.Run })
    if (-not $items.Count) { return }
    if ($items.Count -eq 1 -and $items[0].Fix.Confirm -and -not (Confirm-Action $items[0].Fix.Confirm)) { return }
    Close-Sheet
    Set-Busy $true
    $before = if ($script:LastAnalysis) { $script:LastAnalysis.Score } else { $null }

    if (($items | Where-Object { $_.Fix.Restore }) -and -not $script:RestoreDone) {
        if (-not (New-RestorePoint)) { Set-Status 'Annulé.'; return }
        $script:RestoreDone = $true
    }
    $done = @(); $failed = @(); $reboot = $false
    $script:RunLog = New-Object System.Collections.ArrayList
    try {
        foreach ($f in $items) {
            Set-Status "En cours : $($f.Titre)..."
            try {
                & $f.Fix.Run $f.Fix.Args
                $done += $f
                if ($f.Fix.Reboot) { $reboot = $true }
            } catch {
                $failed += "$($f.Titre) : $($_.Exception.Message)"
                Write-Log "Échec correction $($f.Id): $_"
            }
        }
    } finally {
        $log = $script:RunLog
        $script:RunLog = $null
    }

    if ($done.Count -eq 1 -and $done[0].Fix.NoRescan) {
        $msg = if ($done[0].Fix.Done) { $done[0].Fix.Done } else { 'C''est lancé.' }
        Set-Status $msg
        Show-Message $msg
        return
    }

    # Un écran a changé de fréquence: on vérifie qu'il affiche toujours quelque chose.
    $reverted = @(Confirm-DisplayChange $log)

    Build-GamingTab
    Update-StartupList
    Update-BackupSummary
    Invoke-Analysis
    $after = $script:LastAnalysis.Score

    $kept = @($done | Where-Object { -not ($_.Id -like 'display:*' -and $reverted -contains $_.Id.Substring(8)) })
    $lines = @()
    if ($kept.Count) {
        $lines += "$($kept.Count) correction$(if ($kept.Count -gt 1) {'s'}) appliquée$(if ($kept.Count -gt 1) {'s'}) :"
        foreach ($f in $kept) { $lines += "•  $($f.Titre)" }
    }
    if ($reverted.Count) {
        $lines += "L'écran est revenu à son ancienne fréquence car la nouvelle ne s'affichait pas. Ce point est maintenant ignoré : il ne compte plus dans ton score."
    }
    if ($null -ne $before -and $kept.Count) { $lines += "Score : $before → $after" }
    if ($failed) { $lines += 'Non appliqué :'; $lines += $failed }
    if ($reboot -and $kept.Count) { $lines += 'Redémarre ton PC pour que tout soit pris en compte.' }
    $note = if ($kept | Where-Object { $_.Id -eq 'disk-space' }) { 'Les fichiers supprimés par le nettoyage ne peuvent pas être récupérés. Tout le reste peut être annulé.' } else { $null }
    $title = if ($kept.Count) { "C'est fait !" } elseif ($reverted.Count) { 'Retour à l''ancien réglage' } else { 'Rien n''a été appliqué' }
    Set-Status $title
    Show-ResultSheet $title $lines $log $note
}

# Après un changement de fréquence: demande si l'affichage est correct, sinon revient
# automatiquement en arrière au bout de 15 secondes (comme Windows).
function Confirm-DisplayChange($Log) {
    $disp = @($Log | Where-Object { $_.Type -eq 'display' })
    if (-not $disp.Count) { return @() }
    $script:SheetMode = 'display'
    $script:DisplayChoice = $null

    $body = $ui.SheetBody
    $body.Children.Clear()
    [void]$body.Children.Add((New-Text 'Tes écrans s''affichent bien ?' 21 '#FFFFFF' -Bold))
    $t = New-Text "La fréquence de l'écran vient d'être changée. Si un écran est resté noir ou affiche un message d'erreur, ne touche à rien : l'app revient toute seule à l'ancien réglage." 14 '#EEEBF7'
    $t.Margin = New-Thickness 0 12 0 0
    [void]$body.Children.Add($t)
    Add-SheetInfo 'Sans réponse, retour automatique à l''ancien réglage.' $Colors.warn
    $ui.SheetRun.Content = 'Oui, garder'
    $ui.SheetRun.Visibility = 'Visible'
    $ui.SheetIgnore.Visibility = 'Visible'
    $ui.SheetOpen.Visibility = 'Collapsed'
    $ui.SheetClose.Visibility = 'Collapsed'
    $ui.Overlay.Visibility = 'Visible'
    try { [void]$Window.Activate() } catch {}

    $end = (Get-Date).AddSeconds(15)
    while (-not $script:DisplayChoice) {
        $left = [math]::Ceiling(($end - (Get-Date)).TotalSeconds)
        if ($left -le 0) { $script:DisplayChoice = 'timeout'; break }
        $ui.SheetIgnore.Content = "Revenir en arrière ($left)"
        Update-UI
        Start-Sleep -Milliseconds 100
    }
    $choice = $script:DisplayChoice
    Close-Sheet
    $script:SheetMode = 'fix'
    $ui.SheetClose.Visibility = 'Visible'
    if ($choice -eq 'keep') { return @() }

    $devices = @()
    foreach ($d in $disp) {
        [void][OGNative]::SetRefreshRate($d.Device, $d.Hz)
        [void]$Log.Remove($d)
        $devices += $d.Device
        if ($script:Ignored -notcontains "display:$($d.Device)") { $script:Ignored += "display:$($d.Device)" }
    }
    Save-Ignored
    Write-Log "Fréquence annulée ($choice) pour: $($devices -join ', ')"
    $devices
}

# Annule exactement les changements notés dans le journal, du plus récent au plus ancien.
function Undo-RunLog($Log) {
    $errors = @()
    for ($i = $Log.Count - 1; $i -ge 0; $i--) {
        $e = $Log[$i]
        try {
            switch ($e.Type) {
                'reg' {
                    if ($e.Existed) {
                        Write-RegValue $e.Path $e.Name $e.Value $e.Kind
                    } else {
                        Remove-RegValue $e.Path $e.Name
                    }
                }
                'power' { powercfg /setactive $e.Guid | Out-Null }
                'overlay' { [void][OGNative]::SetOverlay($e.Guid) }
                'display' {
                    $r = [OGNative]::SetRefreshRate($e.Device, $e.Hz)
                    if ($r -ne 0) { throw "Écran $($e.Device): fréquence non restaurée (code $r)" }
                }
                'fw' { Remove-NetFirewallRule -DisplayName $e.Name -ErrorAction Stop }
                'pcfg' {
                    if ($null -ne $e.Ac) { powercfg /setacvalueindex SCHEME_CURRENT $SubBattery $e.Guid ([int]$e.Ac) | Out-Null }
                    if ($null -ne $e.Dc) { powercfg /setdcvalueindex SCHEME_CURRENT $SubBattery $e.Guid ([int]$e.Dc) | Out-Null }
                    powercfg /setactive SCHEME_CURRENT | Out-Null
                }
                'svc' {
                    Set-Service -Name $e.Name -StartupType $e.StartType -ErrorAction Stop
                    if ($e.Running) { Start-Service -Name $e.Name -ErrorAction Stop }
                }
                'dns' {
                    # Le numéro de la carte réseau peut changer (redémarrage, câble changé de port) : on la retrouve par son identifiant.
                    $idx = $e.IfIndex
                    if ($e.Guid) {
                        $ad = @(Get-NetAdapter -IncludeHidden -ErrorAction SilentlyContinue | Where-Object { [string]$_.InterfaceGuid -eq [string]$e.Guid })[0]
                        if (-not $ad) { throw 'La carte réseau de ce réglage n''existe plus sur ce PC.' }
                        $idx = $ad.ifIndex
                    }
                    if (@($e.Servers).Count) { Set-DnsClientServerAddress -InterfaceIndex $idx -ServerAddresses @($e.Servers) -ErrorAction Stop }
                    else { Set-DnsClientServerAddress -InterfaceIndex $idx -ResetServerAddresses -ErrorAction Stop }
                    Clear-DnsClientCache
                    $script:Net = Get-ActiveNet
                }
            }
        } catch { $errors += $_.Exception.Message }
    }
    Sync-Mouse
    , $errors
}

# Fiche de résultat avec le bouton « Revenir en arrière ».
function Show-ResultSheet([string]$Title, [string[]]$Lines, $Log, [string]$Note) {
    $script:SheetMode = 'result'
    $script:ResultLog = $Log
    $script:ResultHistoryId = if ($Log -and $Log.Count) { Add-History (Get-HistoryTitle $Title $Lines) (Get-HistoryItems $Lines) $Log } else { $null }
    $body = $ui.SheetBody
    $body.Children.Clear()
    $head = New-Grid @('Auto', '*')
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 14; $dot.Height = 14
    $dot.Fill = Get-Brush $Colors.ok
    $dot.Margin = New-Thickness 0 8 14 0
    $dot.VerticalAlignment = 'Top'
    Add-ToGrid $head $dot 0
    Add-ToGrid $head (New-Text $Title 21 '#FFFFFF' -Bold) 1
    [void]$body.Children.Add($head)
    foreach ($l in $Lines) {
        $t = New-Text $l 14 '#EEEBF7'
        $t.Margin = New-Thickness 0 8 0 0
        [void]$body.Children.Add($t)
    }
    if ($Note) { Add-SheetInfo $Note $Colors.info }
    $canUndo = $Log -and $Log.Count
    if ($canUndo) { Add-SheetInfo 'Si quelque chose ne va pas, « Revenir en arrière » annule exactement ces changements.' $Colors.ok }
    $ui.SheetRun.Visibility = 'Collapsed'
    $ui.SheetOpen.Visibility = 'Collapsed'
    $ui.SheetIgnore.Visibility = if ($canUndo) { 'Visible' } else { 'Collapsed' }
    $ui.SheetIgnore.Content = 'Revenir en arrière'
    $ui.SheetClose.Content = 'OK'
    $ui.SheetClose.Visibility = 'Visible'
    $ui.Overlay.Visibility = 'Visible'
}

function Invoke-UndoLastRun {
    $log = $script:ResultLog
    Close-Sheet
    if (-not $log -or -not $log.Count) { return }
    Set-Busy $true
    Set-Status 'Retour en arrière...'
    $before = if ($script:LastAnalysis) { $script:LastAnalysis.Score } else { $null }
    $errors = Undo-RunLog $log
    $script:ResultLog = $null
    Set-HistoryUndone $script:ResultHistoryId
    Update-NetInfo
    Build-GamingTab
    Update-StartupList
    Update-BackupSummary
    Invoke-Analysis
    $lines = @('Les réglages sont revenus exactement comme avant.')
    if ($null -ne $before) { $lines += "Score : $before → $($script:LastAnalysis.Score)" }
    if ($errors) { $lines += 'Pas pu être restauré :'; $lines += $errors }
    Set-Status 'Retour en arrière effectué.'
    Show-ResultSheet 'Retour en arrière effectué' $lines $null $null
}

function Set-IgnoreFinding($f, [bool]$Ignore) {
    if ($Ignore) { if ($script:Ignored -notcontains $f.Id) { $script:Ignored += $f.Id } }
    else { $script:Ignored = @($script:Ignored | Where-Object { $_ -ne $f.Id }) }
    Save-Ignored
    Close-Sheet
    Invoke-Analysis
    Set-Status $(if ($Ignore) { "« $($f.Titre) » est ignoré et ne compte plus dans le score." } else { "« $($f.Titre) » compte de nouveau dans le score." })
}
