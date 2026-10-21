# Nevermind : les pages de l'écran (kit) de « Performances » : Performances, Optimisation gaming (Réglages Windows, Mode
# jeu, Profils par jeu), Démarrage et Nettoyage.
# Les réglages, programmes et catégories viennent de l'interface d'avant, toujours chargée (cases à cocher et
# interrupteurs cachés) : l'écran les montre et les règle, puis appelle les mêmes fonctions (Invoke-ApplyTweaks,
# Set-StartupToggle, Invoke-Clean...).
# Chargé par OptiGame.ps1 après ecran-pc.ps1.

# Le dernier jeu joué (lu dans les mesures de parties) : gardé 20 s, l'écran est préparé souvent
function Get-DernierJeuKit {
    $c = $script:Kit.DernierJeu
    if ($c -and ((Get-Date) - $c.At).TotalSeconds -lt 20) { return $c.Jeu }
    $j = try { Get-LastPlayedGame } catch { $null }
    $script:Kit.DernierJeu = @{ At = Get-Date; Jeu = $j }
    $j
}
# Les profils par jeu (lus dans le registre, un jeu après l'autre) : gardés jusqu'au prochain changement de profil ou
# de liste de jeux
function Get-ProfilsKit {
    $c = $script:Kit.Profils
    if ($c -and [object]::ReferenceEquals($c.Jeux, $script:Games)) { return $c }
    $jeux = @($script:Games | Where-Object { (Get-GameExeNames $_).Count } | Sort-Object { $_.Name })
    $gpus = @($script:AnalysisData.GPUs | ForEach-Object { [string]$_.Name } | Where-Object { $_ -notmatch 'Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta|Microsoft Basic' })
    $deux = $gpus.Count -ge 2
    $liste = @(foreach ($g in $jeux) {
        @{ Jeu = $g; Nom = [string]$g.Name; Note = "$(if ($g.Source) { $g.Source } else { 'Steam' }) · $((Get-GameExeNames $g) -join ', ')"
            Prio = [bool](Test-GamePriority $g); Gpu = $(if ($deux) { [bool]((Get-GpuPreference $g.Exes[0]) -match 'GpuPreference=2') } else { $null }) }
    })
    $script:Kit.Profils = @{ Jeux = $script:Games; Liste = $liste; Deux = $deux }
    $script:Kit.Profils
}

# Une icône de programme (démarrage) en image intégrée pour l'écran, si Nevermind l'a déjà lue
function Get-IconeExeKit([string]$Exe) {
    $f = if ($script:ExeFacts -and $Exe) { $script:ExeFacts[$Exe.ToLower()] } else { $null }
    if ($f -and $f.Png -and $f.Png.Length -lt 60000) { 'data:image/png;base64,' + [Convert]::ToBase64String([byte[]]$f.Png) } else { $null }
}

function Get-PagesPerf {
    $rows = @($script:TweakRows | Where-Object { $_ })
    $total = $rows.Count
    $okN = @($rows | Where-Object { -not $_.CheckBox.IsEnabled }).Count
    $reste = $total - $okN
    $last = Get-DernierJeuKit
    $resume = @()
    if ($last -and $last.Last -and $last.Last.Avg) { $resume += @{ nom = 'Dernière partie'; valeur = ('{0:N0} FPS' -f $last.Last.Avg) } }
    if ($last) { $resume += @{ nom = 'Jeu'; valeur = [string]$last.Game.Name } }
    $anneau = if ($total) { @{ valeur = $okN; sur = $total; petit = "/$total" } } else { $null }
    $titreReg = if (-not $total) { 'Réglages pour les jeux' } elseif ($reste) { "$reste réglage$(if ($reste -gt 1) { 's' }) à faire" } else { 'Tout est optimisé' }
    $texteReg = if ($total) { "$okN réglage$(if ($okN -gt 1) { 's' }) sur $total déjà au mieux pour jouer." } else { 'Les réglages se chargent.' }

    # Démarrage
    $entrees = @($script:StartupEntries | Where-Object { $_ })
    $on = @($entrees | Where-Object { $_.Item.Enabled }).Count
    $conseil = @($entrees | Where-Object { $_.Advice.Kind -eq 'safe' -and $_.Item.Enabled }).Count
    # Nettoyage
    $cRows = @($script:CleanRows | Where-Object { $_ })
    $tout = 0.0; $choisi = 0.0
    foreach ($r in $cRows) { $tout += $r.Size; if ($r.CheckBox.IsChecked) { $choisi += $r.Size } }

    # Conseillé pour toi : les réglages de jeu encore à faire (la fiche de chaque réglage, avec « Exécuter »)
    $a = $script:LastAnalysis
    $script:Kit.Conseils = @(if ($a) { $a.Active | Where-Object { $_.Id -like 'tweak:*' -and $_.Status -eq 'warn' } })
    $conseils = @(for ($k = 0; $k -lt $script:Kit.Conseils.Count; $k++) { $f = $script:Kit.Conseils[$k]; @{ type = 'ligne'; id = "conseil:$k"; icone = 'manette'; texte = [string]$f.Titre; note = [string]$f.Detail; etiquette = 'Conseillé'; ton = 'attention' } })

    $pages = [ordered]@{}
    $pages.perf = @{ titre = 'Performances'; sous = 'Pour gagner des FPS : les réglages de Windows, le démarrage, la place sur le disque.'; icone = 'eclair'; couleur = 'P'
        module = $(if ($anneau) { @{ anneau = $anneau; titre = $titreReg; texte = $texteReg; boutons = @(@{ id = 'optimisation'; texte = 'Voir les réglages' }); resume = $resume } } else { @{ icone = 'eclair'; titre = $titreReg; texte = $texteReg; resume = $resume } })
        blocs = @(
            @{ type = 'titre'; texte = 'Pour gagner des FPS' }
            @{ type = 'outils'; elements = @(
                @{ id = 'optimisation'; nom = 'Optimisation gaming'; note = 'Réglages, Mode jeu, profils par jeu'; icone = 'manette'; couleur = 'P'; etiquette = $(if ($reste) { "$reste à faire" } elseif ($total) { 'Optimisé' }); ton = $(if ($reste) { 'attention' } else { 'ok' }) }
                @{ id = 'demarrage'; nom = 'Démarrage'; note = 'Ce qui se lance avec le PC'; icone = 'marche'; couleur = 'S'; etiquette = $(if ($entrees.Count) { "$on programme$(if ($on -gt 1) { 's' })" }); ton = $(if ($conseil) { 'attention' } else { '' }) }
                @{ id = 'nettoyage'; nom = 'Nettoyage'; note = 'Les fichiers inutiles'; icone = 'balai'; couleur = 'T'; etiquette = $(if ($cRows.Count) { Format-Size $tout }); ton = $(if ($tout -gt 1GB) { 'attention' } elseif ($cRows.Count) { 'ok' } else { '' }) }
            ) }
            if ($conseils.Count) { @{ type = 'titre'; texte = 'Conseillé pour toi' } }
        ) + $conseils
    }

    # ---- Optimisation gaming : Réglages Windows, Mode jeu, Profils par jeu
    $lignesReg = @(for ($k = 0; $k -lt $rows.Count; $k++) {
        $t = $rows[$k].Tweak; $cb = $rows[$k].CheckBox
        $fait = -not $cb.IsEnabled
        $etq = if ($fait) { 'Optimisé' } else { "Impact $(([string]$t.Impact).ToLower())" }
        $l = @{ type = 'ligne'; texte = [string]$t.Titre; note = [string]$t.Description; etiquette = $etq; ton = $(if ($fait) { 'ok' } elseif ($t.Impact -eq 'Important') { 'urgent' } elseif ($t.Impact -eq 'Moyen') { 'attention' } else { '' }) }
        if (-not $fait) { $l.case = @{ id = 'tweak'; valeur = "$k"; oui = [bool]$cb.IsChecked } }
        if ($t.Reboot -and -not $fait) { $l.note = "$($l.note) Il faut redémarrer le PC ensuite." }
        $l
    })
    $sel = Get-GameModeSelection
    $modeJeu = @(
        @{ type = 'reglages'; rangs = @(@{ texte = 'Fermer des applis pendant que je joue'; note = 'Quand un de tes jeux démarre, les applis cochées sont fermées, puis relancées quand tu quittes le jeu.'; bascule = @{ id = 'modejeu'; oui = [bool](Get-Setting 'GameMode' $false) } }) }
        if ($script:GameModeStatus -and $script:GameModeStatus.Text) { @{ type = 'texte'; texte = [string]$script:GameModeStatus.Text } }
        @{ type = 'titre'; texte = 'Applis à fermer' }
    ) + @(foreach ($x in $GameModeApps) { @{ type = 'case'; id = 'modejeu-app'; valeur = [string]$x.Id; texte = [string]$x.Name; oui = ($sel -contains $x.Id) } }) + @(
        @{ type = 'note'; texte = 'Ne coche pas le launcher du jeu auquel tu joues (Epic, EA, Ubisoft). Marche tant que Nevermind est ouvert, même réduit.' }
    )
    $profils = New-Object System.Collections.ArrayList
    $script:Kit.JeuxProfils = @()
    if ($null -eq $script:Games) { [void]$profils.Add(@{ type = 'texte'; texte = 'Recherche des jeux installés...' }) }
    elseif ($script:Vue.Ecran -ne 'optimisation' -and -not $script:Kit.Profils) { [void]$profils.Add(@{ type = 'texte'; texte = 'Lecture des profils...' }) }   # (lus en arrivant sur la page)
    else {
        $pr = Get-ProfilsKit
        $script:Kit.JeuxProfils = @($pr.Liste | ForEach-Object { $_.Jeu })
        [void]$profils.Add(@{ type = 'texte'; texte = 'Ces réglages sont appliqués par Windows à chaque lancement du jeu, même Nevermind fermé. « Priorité haute » : le jeu passe avant les autres programmes.' })
        [void]$profils.Add(@{ type = 'bouton'; id = 'ajouterjeu'; texte = 'Ajouter un jeu'; style = 'second'; icone = 'plus' })
        [void]$profils.Add(@{ type = 'titre'; texte = "$(@($pr.Liste).Count) jeu$(if (@($pr.Liste).Count -gt 1) { 'x' }) reconnu$(if (@($pr.Liste).Count -gt 1) { 's' })" })
        $k = 0
        foreach ($x in $pr.Liste) {
            $b = @(@{ id = "profil:$k`:priority"; texte = 'Priorité haute'; oui = $x.Prio })
            if ($pr.Deux) { $b += @{ id = "profil:$k`:gpu"; texte = 'Carte puissante'; oui = [bool]$x.Gpu } }
            [void]$profils.Add(@{ type = 'ligne'; icone = 'manette'; couleur = 'S'; texte = $x.Nom; note = $x.Note; bascules = $b })
            $k++
        }
    }
    $pages.optimisation = @{ titre = 'Optimisation gaming'; parent = 'perf'; sous = 'Règle Windows pour tes jeux : réglages, Mode jeu et profils par jeu.'; icone = 'manette'; couleur = 'P'
        module = @{ anneau = $(if ($anneau) { $anneau } else { @{ valeur = 0; sur = 1; texte = '...'; petit = '' } }); titre = $titreReg; texte = $texteReg
            boutons = @(@{ id = 'clic:BtnApply'; texte = 'Appliquer' }, @{ id = 'opti:recommande'; texte = 'Cocher le recommandé'; style = 'second' })
            case = @{ id = 'coche:ChkRestore'; texte = 'Point de restauration avant (recommandé)'; oui = [bool]$ui.ChkRestore.IsChecked } }
        onglets = @(
            @{ id = 'reglages'; nom = 'Réglages Windows'; blocs = @(@{ type = 'titre'; texte = 'Réglages Windows'; droite = 'Tout est annulable' }) + $lignesReg + @(@{ type = 'note'; texte = 'Seulement des réglages à l''effet reconnu : pas de « tweaks miracles ». Coche ceux que tu veux, puis « Appliquer ». Tout est annulable depuis la page Sauvegarde.' }) }
            @{ id = 'mode'; nom = 'Mode jeu'; blocs = $modeJeu }
            @{ id = 'profils'; nom = 'Profils par jeu'; blocs = @($profils) }
        ) }

    # ---- Démarrage
    $lignesDem = @(for ($k = 0; $k -lt $entrees.Count; $k++) {
        $s = $entrees[$k]
        @{ type = 'ligne'; image = (Get-IconeExeKit $s.IconExe); icone = 'app'; couleur = 'doux'; texte = [string]$s.Name; note = [string]$s.Advice.Why
            etiquette = [string]$s.Advice.Label; ton = $(switch ($s.Advice.Kind) { 'safe' { 'ok' } 'keep' { 'attention' } default { '' } })
            bascule = @{ id = "dem:$k"; oui = [bool]$s.Item.Enabled }; fait = -not $s.Item.Enabled }
    })
    $pages.demarrage = @{ titre = 'Démarrage'; parent = 'perf'; sous = 'Les programmes qui se lancent tout seuls quand tu allumes ton PC.'; icone = 'marche'; couleur = 'S'
        actions = @(@{ id = 'clic:BtnRefreshStartup'; texte = 'Actualiser'; icone = 'actu' })
        module = $(if ($entrees.Count -and $on -and $conseil) { @{ anneau = @{ valeur = $conseil; sur = $on; petit = "/$on" }; titre = $(if ($conseil) { "$conseil à couper" } else { 'Rien à couper' }); texte = "Sur $on programme$(if ($on -gt 1) { 's' }) au démarrage, $(if ($conseil) { "$conseil ne servent pas dès l'allumage. Les couper rend l'allumage plus rapide, sans rien désinstaller." } else { 'aucun n''est à couper : ce qui reste est utile.' })"; boutons = @(if ($conseil) { @{ id = 'clic:BtnDisableStartup'; texte = "Couper ce qui est conseillé ($conseil)" } }) } }
            else { @{ icone = 'marche'; couleur = 'S'; titre = $(if ($on) { 'Rien à couper' } elseif ($entrees.Count) { 'Rien ne se lance' } else { 'Lecture en cours' }); texte = $(if ($on) { "$on programme$(if ($on -gt 1) { 's se lancent' } else { ' se lance' }) avec le PC, et tous sont utiles. Tu peux quand même en couper un, rien n'est supprimé." } else { [string]$ui.StartupCount.Text }) } })
        blocs = @(@{ type = 'titre'; texte = 'Au démarrage'; droite = 'Activé : se lance avec le PC' }) + $(if ($lignesDem.Count) { $lignesDem } else { @(@{ type = 'texte'; texte = 'Aucun programme ne se lance avec Windows.' }) }) + @(@{ type = 'note'; texte = 'Rien n''est supprimé : un programme coupé s''ouvre toujours quand tu cliques dessus, et tu peux le remettre quand tu veux.' }) }

    # ---- Nettoyage
    $disque = try { [IO.DriveInfo]::new($env:SystemDrive) } catch { $null }
    $lignesNet = @(for ($k = 0; $k -lt $cRows.Count; $k++) {
        $r = $cRows[$k]; $c = $r.Target; $vide = -not $r.CheckBox.IsEnabled
        $l = @{ type = 'ligne'; texte = [string]$c.Titre; note = [string]$c.Desc; valeur = $(if ($vide) { '0' } else { Format-Size $r.Size }); ton = $(if ($r.Size -gt 500MB) { 'attention' } else { '' }) }
        if ($vide) { $l.etiquette = 'Déjà propre'; $l.ton = 'ok'; $l.fait = $true } else {
            $l.case = @{ id = 'nett'; valeur = "$k"; oui = [bool]$r.CheckBox.IsChecked }
            if ($c.Warn) { $l.etiquette = 'À garder si tu utilises Claude'; $l.ton = 'attention' }
            $l.bouton = @{ id = "nett-voir:$k"; texte = 'Voir'; style = 'second' }
        }
        $l
    })
    $dern = Get-Setting 'LastClean' $null
    $pages.nettoyage = @{ titre = 'Nettoyage'; parent = 'perf'; sous = 'Libère de la place en supprimant les fichiers inutiles.'; icone = 'balai'; couleur = 'T'
        actions = @(@{ id = 'clic:BtnCleanScan'; texte = $(if ($cRows.Count) { 'Analyser de nouveau' } else { 'Analyser' }); icone = 'actu' })
        module = $(if ($disque -and $disque.TotalSize) {
            $p = [int](100 * ($disque.TotalSize - $disque.AvailableFreeSpace) / $disque.TotalSize)
            @{ anneau = @{ valeur = $p; sur = 100; petit = '%'; ton = $(if ($p -ge 90) { 'attention' }) }; titre = "Disque $($env:SystemDrive.TrimEnd(':')) rempli à $p %"
                texte = "$(Format-Size $disque.AvailableFreeSpace) libres sur $(Format-Size $disque.TotalSize).$(if ($cRows.Count) { " $(Format-Size $choisi) choisis pour le nettoyage." } else { ' Clique sur Analyser pour voir ce qui peut partir.' })"
                boutons = @(if ($cRows.Count) { @{ id = 'clic:BtnClean'; texte = [string]$ui.BtnClean.Content } } else { @{ id = 'clic:BtnCleanScan'; texte = 'Analyser' } }) }
          } else { @{ icone = 'balai'; couleur = 'T'; titre = 'Nettoyage'; texte = [string]$ui.CleanTotal.Text; boutons = @(@{ id = 'clic:BtnCleanScan'; texte = 'Analyser' }) } })
        blocs = @(
            @{ type = 'titre'; texte = 'Ce qui peut partir'; droite = 'Ceux de moins de 24 h sont gardés' }
            if (-not $cRows.Count) { @{ type = 'texte'; texte = 'Clique sur « Analyser » pour voir ce qui peut être libéré.' } }
        ) + $lignesNet + @(
            if ($dern -and $dern.Date) { @{ type = 'ligne'; icone = 'horloge'; texte = "Dernier nettoyage le $(([datetime]$dern.Date).ToString('dd/MM à HH:mm'))"; note = "$(Format-Size ([double]$dern.Freed)) libérés, $($dern.Deleted) fichier(s) supprimé(s), $($dern.Skipped) laissé(s) car utilisé(s)"; bouton = $(if ($dern.File -and (Test-Path -LiteralPath ([string]$dern.File))) { @{ id = 'nett-journal'; texte = 'Ouvrir le journal'; style = 'second' } }) } }
            @{ type = 'note'; texte = 'Nevermind ne touche jamais à tes documents, à la corbeille ni aux caches de shaders des jeux : les vider provoquerait des saccades le temps qu''ils se reconstruisent.' }
        ) }
    $pages
}

function Register-ActionsPerf {
    # « Conseillé pour toi » : la fiche du réglage (avec « Exécuter »)
    Register-ActionVue 'opti:recommande' { param($Valeur, $Ecran) foreach ($r in @($script:TweakRows | Where-Object { $_ })) { if ($r.CheckBox.IsEnabled) { $r.CheckBox.IsChecked = ($r.Tweak.Recommended -ne $false) } } }
    Register-ActionVue 'tweak' { param($Valeur, $Ecran) $r = @($script:TweakRows | Where-Object { $_ })[[int]$Valeur.valeur]; if ($r -and $r.CheckBox.IsEnabled) { $r.CheckBox.IsChecked = [bool]$Valeur.oui } }
    Register-ActionVue 'modejeu' { param($Valeur, $Ecran)
        $on = [bool]$Valeur
        Set-Setting 'GameMode' $on; Update-GameWatch; Update-GameModeStatus
        Set-Status $(if ($on) { 'Mode jeu automatique activé.' } else { 'Mode jeu automatique désactivé.' })
        try { Build-GameModeCard } catch {}
    }
    Register-ActionVue 'modejeu-app' { param($Valeur, $Ecran)
        $id = [string]$Valeur.valeur
        $cur = @(Get-GameModeSelection | Where-Object { $_ -ne $id })
        if ($Valeur.oui) { $cur += $id }
        Set-Setting 'GameModeApps' $cur
        try { Build-GameModeCard } catch {}
    }
    Register-ActionVue 'ajouterjeu' { param($Valeur, $Ecran) Invoke-Safe { Add-CustomGame } }
    Register-ActionVue 'nett' { param($Valeur, $Ecran)
        $r = @($script:CleanRows | Where-Object { $_ })[[int]$Valeur.valeur]
        if ($r -and $r.CheckBox.IsEnabled) { $r.CheckBox.IsChecked = [bool]$Valeur.oui; $script:CleanChoice[$r.Id] = [bool]$Valeur.oui; Update-CleanSelection }
    }
    Register-ActionVue 'nett-journal' { param($Valeur, $Ecran) $l = Get-Setting 'LastClean' $null; if ($l -and $l.File) { $f = [string]$l.File; Invoke-Safe { Start-Process -FilePath 'notepad.exe' -ArgumentList "`"$f`"" } } }
}

# Les gestes à numéro (une ligne d'une liste) : appelés par l'action « * » de accueil-console.ps1
function Invoke-ActionPerf([string]$Id, $Valeur) {
    if ($Id -match '^conseil:(\d+)$') { $f = $script:Kit.Conseils[[int]$Matches[1]]; if ($f) { Invoke-Safe { Open-Sheet @($f) } }; return $true }
    if ($Id -match '^dem:(\d+)$') {
        $s = @($script:StartupEntries | Where-Object { $_ })[[int]$Matches[1]]
        if ($s) { Invoke-Safe { Set-StartupToggle ([pscustomobject]@{ Tag = $s; IsChecked = [bool]$Valeur }) } }
        return $true
    }
    if ($Id -match '^profil:(\d+):(priority|gpu)$') {
        $g = @($script:Kit.JeuxProfils)[[int]$Matches[1]]; $quoi = $Matches[2]
        if ($g) { Invoke-Safe { Set-GameProfile $g $quoi ([bool]$Valeur) }; $script:Kit.Profils = $null }
        return $true
    }
    if ($Id -match '^nett-voir:(\d+)$') {
        $r = @($script:CleanRows | Where-Object { $_ })[[int]$Matches[1]]
        if ($r) { $info = Get-CleanInfo $r.Target 300; Invoke-Safe { Show-CleanFiles $r.Target $info } }
        return $true
    }
    $false
}
