# Nevermind : constats, score, fiches de correction et retour en arrière.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Tableau de bord
# ---------------------------------------------------------------------------
function Add-Finding($List, [string]$Status, [string]$Titre, [string]$Detail, [int]$Weight, [string]$Action, [string]$ActionLabel, [string]$Id, $Fix) {
    Step-UI
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
        [switch]$Restore,
        $Changes
    )
    # -Changes : ce que la correction change, déclaré à l'avance (voir Set-Declarations). Sans -Run, la correction
    # applique exactement cette liste : ce que la fiche montre est ce qui est fait.
    if ($null -ne $Changes -and -not $Run) { $Run = { param($c) Set-Declarations $c }; $RunArgs = $Changes }
    @{
        Auto = [bool]$Auto; What = $What; Why = $Why; Steps = $Steps
        Run = $Run; Args = $RunArgs; RunLabel = $RunLabel; Confirm = $Confirm; Done = $Done; NoRescan = [bool]$NoRescan
        Open = $Open; OpenLabel = $OpenLabel; Reboot = [bool]$Reboot; Restore = [bool]$Restore; Changes = $Changes
    }
}

# ---------------------------------------------------------------------------
# Ce que change chaque correction, déclaré avant de le faire (issue #7)
# ---------------------------------------------------------------------------
# Une correction ne calcule pas ses changements en s'exécutant : elle les décrit d'abord dans une liste. La fiche
# montre cette liste avec la valeur actuelle de chaque réglage, lue sans rien modifier (Get-DetailsAvant), et la
# correction applique cette même liste (Set-Declarations). Rien n'est simulé : une lecture ne peut rien changer.
#   @{ Type = 'reg'; Path; Name; Value; Kind }      écrit une valeur du registre (Kind : DWord si absent)
#   @{ Type = 'reg'; Path; Name; Efface = $true }   efface une valeur du registre
#   @{ Type = 'pcfg'; Key; Value }                  réglage « batterie » du mode d'alimentation ($BatSettings), secteur et batterie
#   @{ Type = 'display'; Device; Hz }               fréquence d'un écran
#   @{ Type = 'power'; Cible }                      plan ou mode d'alimentation : décrit ici, fait par le code de la correction
#   @{ Type = 'texte'; Texte }                      ce qui n'est pas une simple valeur (lancer un outil, nettoyer...)
#   @{ Type = 'partiel' }                           la correction décide d'autres choses en s'exécutant (la liste le dit)
# La liste peut aussi être un scriptblock qui la calcule à partir de l'état actuel, en lecture seule.
# Vérifié par outils\tester-declarations.ps1 : chaque correction a sa liste, et pour le registre, c'est bien elle qui est écrite.
function Resolve-Declarations($Changes) {
    if ($Changes -is [scriptblock]) { return @(& $Changes | Where-Object { $_ }) }
    @(@($Changes) | Where-Object { $_ })
}

function Set-Declarations($Changes) {
    foreach ($d in (Resolve-Declarations $Changes)) {
        switch ([string]$d.Type) {
            'reg' {
                if ($d.Efface) { Clear-Reg $d.Path $d.Name }
                else { Set-Reg $d.Path $d.Name $d.Value $(if ($d.Kind) { [string]$d.Kind } else { 'DWord' }) }
            }
            'pcfg' { Set-BatterySetting $d.Key ([int]$d.Value) }
            'display' { Set-DisplayRate $d.Device ([int]$d.Hz) }
            # ('power', 'texte', 'partiel' : seulement décrits, c'est le code de la correction qui les fait)
        }
    }
    Clear-StartupItemsCache   # (le registre a pu changer : la liste des programmes au démarrage sera relue)
}

# Fréquence actuelle d'un écran ($null s'il n'est plus là)
function Get-FrequenceEcran([string]$Device) {
    $d = @(@([OGNative]::GetDisplays()) | Where-Object { ($_ -split '\|')[0] -eq $Device })[0]
    if ($d) { [int](($d -split '\|')[4]) } else { $null }
}

$BatNoms = @{ CritAction = 'action à batterie critique'; CritLevel = 'niveau de batterie critique'; LowLevel = 'niveau de batterie faible'; LowNotify = 'alerte de batterie faible'; LowAction = 'action à batterie faible' }
function Format-ValeurBat([string]$Key, $V) {
    if ($null -eq $V) { return 'inconnu' }
    if ($Key -like '*Action') { $a = $BatActions[[int]$V]; if ($a) { return $a }; return "$V" }
    if ($Key -eq 'LowNotify') { if ([int]$V -eq 1) { return 'activée' }; return 'coupée' }
    "$V %"
}

# Une ligne lisible : le réglage, sa valeur actuelle (lue sans rien changer) et sa valeur après la correction
function Format-Declaration($D, $Cache) {
    switch ([string]$D.Type) {
        'reg' {
            # (certaines clés du PC ne se lisent qu'en administrateur : c'est l'Aide qui les écrit)
            $st = $null
            try { $ErrorActionPreference = 'Stop'; $st = Get-RegState $D.Path $D.Name } catch {} finally { $ErrorActionPreference = 'Continue' }
            $maint = if (-not $st) { 'illisible sans droits administrateur' } elseif ($st.Existed) { Format-ValeurTech $st.Value } else { 'absente' }
            $apres = if ($D.Efface) { 'effacée' } else { Format-ValeurTech $D.Value }
            $pareil = $st -and $(if ($D.Efface) { -not $st.Existed } else { $st.Existed -and $maint -eq $apres })
            return "Registre $($D.Path -replace '^Registry::', '') : valeur « $($D.Name) », maintenant $maint, après $apres$(if ($pareil) { ' (déjà comme ça)' })"
        }
        'pcfg' {
            if (-not $Cache.ContainsKey('pcfg')) { $Cache.pcfg = Get-BatteryPowerSettings }
            $cur = $Cache.pcfg[[string]$D.Key]
            $maint = if ($cur) { "secteur $(Format-ValeurBat $D.Key $cur[0]), batterie $(Format-ValeurBat $D.Key $cur[1])" } else { 'inconnu' }
            return "Alimentation, $($BatNoms[[string]$D.Key]) (réglage $($BatSettings[[string]$D.Key])) : maintenant $maint, après $(Format-ValeurBat $D.Key $D.Value) (secteur et batterie)"
        }
        'display' {
            $hz = Get-FrequenceEcran $D.Device
            return "Écran $($D.Device) : fréquence maintenant $(if ($null -ne $hz) { "$hz Hz" } else { 'inconnue' }), après $($D.Hz) Hz (même résolution)"
        }
        'power' {
            $s = Get-ActiveScheme
            return "Plan d'alimentation : maintenant « $(if ($s) { $s.Name } else { 'inconnu' }) », après « $($D.Cible) »$(if ($s -and $D.Guid -and $s.Guid -eq $D.Guid) { ' (déjà comme ça)' })"
        }
        'texte' { return [string]$D.Texte }
        'partiel' { return 'Et d''autres changements décidés pendant l''exécution, visibles ensuite dans l''historique (page Sauvegarde).' }
    }
    "Changement non décrit ($($D.Type))"
}

# Les détails techniques d'une correction avant de la faire : « réglage : maintenant ..., après ... » (lecture seule)
function Get-DetailsAvant($Changes) {
    $cache = @{}
    try { $liste = @(Resolve-Declarations $Changes) } catch { return @("Détails impossibles à lire : $($_.Exception.Message)") }
    @(foreach ($d in $liste) {
        try { Format-Declaration $d $cache } catch { "$(if ($d.Path) { "$($d.Path) « $($d.Name) »" } else { [string]$d.Type }) : valeur actuelle illisible ($($_.Exception.Message))" }
    })
}

$BiosRun = {
    shutdown.exe /r /fw /t 10
    if ($LASTEXITCODE) { throw "Ce PC ne permet pas de redémarrer directement dans le BIOS (code $LASTEXITCODE). Redémarre et appuie sur Suppr ou F2 pendant le démarrage." }
}
$BiosChanges = @(@{ Type = 'texte'; Texte = 'Redémarre le PC dans 10 secondes, directement dans le BIOS (shutdown.exe /r /fw /t 10). Aucun réglage n''est changé : c''est toi qui choisis dans le BIOS.' })
$BiosConfirm = "Le PC va redémarrer directement dans le BIOS dans 10 secondes.`n`nEnregistre ton travail et ferme tes jeux avant. Continuer ?"
$BiosDone = 'Redémarrage dans le BIOS dans 10 secondes...'

function Invoke-CleanAll {
    [void](Invoke-CleanTargets @(Get-CleanAllTargets))
}
function Get-CleanAllTargets { @($CleanTargets | Where-Object { $_.Default -ne $false }) }   # les catégories à garder (Claude) ne partent jamais d'office

# Ce que fait le nettoyage en un clic (déclaration de la correction « disque plein »)
function Get-ChangementsNettoyage {
    foreach ($t in (Get-CleanAllTargets)) { @{ Type = 'texte'; Texte = "Supprime « $($t.Titre) » : $((@($t.Paths) | Where-Object { $_ }) -join ', ')" } }
    @{ Type = 'texte'; Texte = 'Les fichiers exacts sont choisis pendant le nettoyage (ceux de moins de 24 h ou encore utilisés sont gardés) : leur liste est notée dans le journal du nettoyage. Ils ne peuvent pas être récupérés.' }
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
    $score = if ($total) { [int][math]::Round(100 * $got / $total) } else { 100 }
    $potential = if ($total) { [int][math]::Round(100 * ($got + $autoRaw) / $total) } else { 100 }
    foreach ($item in $active) {
        $raw = if ($total -and $scored -contains $item) { [double]$item.Gain } else { 0.0 }
        $g = if ($total) { [int][math]::Round(100 * $raw / $total) } else { 0 }
        # (le gain montré : ce que le score gagnerait vraiment, arrondis compris, comme « Jusqu'à N en un clic » ; au moins 1)
        if ($g -gt 0) { $g = [math]::Max(1, [int][math]::Round(100 * ($got + $raw) / $total) - $score) }
        $item.Gain = $g
    }
    @{ Score = $score; Potential = $potential; Active = $active }
}

# L'appréciation de la note (le mot et sa couleur), gardée avec l'analyse
function Get-ScoreLook([int]$Score) {
    if ($Score -ge 85)     { $label = 'Excellent';     $color = $Colors.ok }
    elseif ($Score -ge 65) { $label = 'Bien';          $color = '#9BE15D' }
    elseif ($Score -ge 45) { $label = 'À améliorer';   $color = $Colors.warn }
    else                   { $label = 'Mal optimisé';  $color = $Colors.bad }
    @{ Label = $label; Color = $color }
}

# ---------------------------------------------------------------------------
# Le genre de correction (badge de la fiche) et les points ignorés
# ---------------------------------------------------------------------------
function Get-FixKind($f) {
    if ($f.Fix -and $f.Fix.Auto) { return @{ Text = "L'app s'en charge"; Color = $Colors.ok } }
    @{ Text = 'À faire toi-même'; Color = $Colors.info }
}

# Les points ignorés (à régler mais mis de côté), pour l'écran : page Ton PC (ecran-pc.ps1)
function Show-Findings($All, $Active) {
    $script:IgnoredFindings = @($All | Where-Object { $script:Ignored -contains $_.Id -and $_.Status -ne 'ok' })
}

# ---------------------------------------------------------------------------
# Fiche détaillée (fenêtre par-dessus l'application)
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

# Bloc repliable « Détails techniques » (fermé au départ) : montré dans l'écran comme celui de l'historique (Get-FicheKit)
function Add-SheetDetails([string]$Title, [string[]]$Lines) {
    $Lines = @($Lines | Where-Object { $_ })
    if (-not $Lines.Count) { return }
    $x = New-Object System.Windows.Controls.Expander
    $x.Header = $Title; $x.IsExpanded = $false
    $x.Foreground = Get-Brush '#A6A1BC'
    $x.Margin = New-Thickness 0 14 0 0
    $t = New-Text ($Lines -join "`n") 12.5 '#A6A1BC'
    $t.Margin = New-Thickness 4 6 0 0
    $x.Content = $t
    $x.Tag = @{ Lignes = $Lines }
    [void]$ui.SheetBody.Children.Add($x)
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
            Add-SheetSection "Ce que Nevermind va faire quand tu cliques sur Exécuter" $fix.What -Bullets
            if ($fix.Run) { Add-SheetDetails 'Détails techniques' (Get-DetailsAvant $fix.Changes) }
            $safe = 'Tu peux revenir en arrière à tout moment depuis la page Sauvegarde (dans Protection).'
            if ($fix.Restore) { $safe = 'Un point de restauration Windows est créé avant. ' + $safe }
            Add-SheetInfo $safe $Colors.ok
        } elseif ($fix) {
            if ($fix.Steps) { Add-SheetSection 'Ce que tu dois faire' $fix.Steps -Numbered }
            if ($fix.What) { Add-SheetSection "Ce que Nevermind peut faire pour t'aider" $fix.What -Bullets }
            if ($fix.Run) { Add-SheetDetails 'Détails techniques' (Get-DetailsAvant $fix.Changes) }
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
        foreach ($f in $auto) { Add-SheetDetails "Détails techniques : $($f.Titre)" (Get-DetailsAvant $f.Fix.Changes) }
        $safe = 'Un point de restauration Windows est créé avant. Tu peux annuler ces corrections depuis la page Sauvegarde (dans Protection).'
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
        Step-UI
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
    $t = New-Text "La fréquence de l'écran vient d'être changée. Si un écran est resté noir ou affiche un message d'erreur, ne touche à rien : Nevermind revient tout seul à l'ancien réglage." 14 '#EEEBF7'
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
                'fw' { [void](Invoke-Aide 'pare-feu.debloquer' @{ Nom = [string]$e.Name }) }
                'definitif' { }   # (protège le PC : jamais annulé, voir Add-ChangementDefinitif)
                'pcfg' {
                    if ($null -ne $e.Ac) { powercfg /setacvalueindex SCHEME_CURRENT $SubBattery $e.Guid ([int]$e.Ac) | Out-Null }
                    if ($null -ne $e.Dc) { powercfg /setdcvalueindex SCHEME_CURRENT $SubBattery $e.Guid ([int]$e.Dc) | Out-Null }
                    powercfg /setactive SCHEME_CURRENT | Out-Null
                }
                'svc' {
                    [void](Invoke-Aide 'service.regler' @{ Nom = [string]$e.Name; Demarrage = [string]$e.StartType; Lancer = [bool]$e.Running })
                }
                'dns' {
                    # Le numéro de la carte réseau peut changer (redémarrage, câble changé de port) : on la retrouve par son identifiant.
                    $idx = $e.IfIndex
                    if ($e.Guid) {
                        $ad = @(Get-NetAdapter -IncludeHidden -ErrorAction SilentlyContinue | Where-Object { [string]$_.InterfaceGuid -eq [string]$e.Guid })[0]
                        if (-not $ad) { throw 'La carte réseau de ce réglage n''existe plus sur ce PC.' }
                        $idx = $ad.ifIndex
                    }
                    [void](Invoke-Aide 'dns.regler' @{ Index = [int]$idx; Serveurs = @($e.Servers | Where-Object { $_ }) })
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
