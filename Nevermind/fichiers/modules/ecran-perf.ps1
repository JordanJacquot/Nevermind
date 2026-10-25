# Nevermind : les pages de l'écran (kit) de « Performances » : Performances, Optimisation gaming (Réglages Windows, Mode
# jeu, Profils par jeu), Démarrage et Nettoyage.
# Les réglages, programmes et catégories viennent de l'interface d'avant, toujours chargée (cases à cocher et
# interrupteurs cachés) : l'écran les montre et les règle, puis appelle les mêmes fonctions (Invoke-ApplyTweaks,
# Set-StartupToggle, Invoke-Clean...).
# Chargé par Nevermind.ps1 après ecran-pc.ps1.

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

# Les réglages pour les jeux : une seule façon de les compter, pour la tuile de l'accueil, la page Performances et
# l'Optimisation gaming. « À faire » : les réglages conseillés pas encore faits, ceux que le Tableau de bord classe à faire.
# Les facultatifs pas faits ne comptent pas (le Tableau les range « Pour info »).
function Get-ReglagesKit {
    $rows = @($script:TweakRows | Where-Object { $_ })
    $conseilles = @($rows | Where-Object { $_.Tweak.Recommended -ne $false })
    $faits = @($conseilles | Where-Object { -not $_.CheckBox.IsEnabled }).Count
    $facultatifs = @($rows | Where-Object { $_.Tweak.Recommended -eq $false -and $_.CheckBox.IsEnabled }).Count
    @{ Lignes = $rows; Total = $conseilles.Count; Faits = $faits; Reste = $conseilles.Count - $faits; Facultatifs = $facultatifs }
}
# Le résumé des réglages en quelques mots (tuile de l'accueil, page Performances)
function Get-TitreReglagesKit($R) {
    if (-not $R.Lignes.Count) { 'Réglages pour les jeux' } elseif ($R.Reste) { "$($R.Reste) réglage$(if ($R.Reste -gt 1) { 's' }) à faire" } else { 'Tout est optimisé' }
}

# Les applis du Mode jeu présentes en ce moment (ouvertes, ou fermées par le Mode jeu pendant la partie en cours) :
# Nevermind sait seulement voir les applis ouvertes, pas celles qui sont installées. Gardé 20 s.
function Get-AppsModeJeuKit {
    $c = $script:Kit.AppsModeJeu
    if ($c -and ((Get-Date) - $c.At).TotalSeconds -lt 20) { return $c.Ids }
    $noms = @(Get-Process -ErrorAction SilentlyContinue | ForEach-Object { [string]$_.ProcessName })
    $fermees = @(if ($script:GameSession) { @($script:GameSession.Closed) | ForEach-Object { [string]$_.Name } })
    $ids = @(foreach ($x in $GameModeApps) { if (@($noms -match $x.Proc).Count -or $fermees -contains $x.Name) { [string]$x.Id } })
    $script:Kit.AppsModeJeu = @{ At = Get-Date; Ids = $ids }
    $ids
}

# Une icône de programme (démarrage) en image intégrée pour l'écran, si Nevermind l'a déjà lue
function Get-IconeExeKit([string]$Exe) {
    $f = if ($script:ExeFacts -and $Exe) { $script:ExeFacts[$Exe.ToLower()] } else { $null }
    if ($f -and $f.Png -and $f.Png.Length -lt 60000) { 'data:image/png;base64,' + [Convert]::ToBase64String([byte[]]$f.Png) } else { $null }
}

function Get-PagesPerf {
    $rg = Get-ReglagesKit
    $rows = $rg.Lignes
    $total = $rg.Total; $okN = $rg.Faits; $reste = $rg.Reste
    $last = Get-DernierJeuKit
    $resume = @()
    if ($last -and $last.Last -and $last.Last.Avg) { $resume += @{ nom = 'Dernière partie'; valeur = ('{0:N0} FPS' -f $last.Last.Avg) } }
    if ($last) { $resume += @{ nom = 'Jeu'; valeur = [string]$last.Game.Name } }
    $anneau = if ($total) { @{ valeur = $okN; sur = $total; petit = "/$total" } } else { $null }
    $titreReg = Get-TitreReglagesKit $rg
    $texteReg = if ($total) { "$okN réglage$(if ($okN -gt 1) { 's' }) conseillé$(if ($okN -gt 1) { 's' }) sur $total déjà fait$(if ($okN -gt 1) { 's' }).$(if ($rg.Facultatifs) { " $($rg.Facultatifs) réglage$(if ($rg.Facultatifs -gt 1) { 's' }) facultatif$(if ($rg.Facultatifs -gt 1) { 's' }) : à toi de voir." })" } else { 'Les réglages se chargent.' }

    # Démarrage
    $entrees = @($script:StartupEntries | Where-Object { $_ })
    $on = @($entrees | Where-Object { $_.Item.Enabled }).Count
    $conseil = @($entrees | Where-Object { $_.Advice.Kind -eq 'safe' -and $_.Item.Enabled }).Count
    $aVoir = @($entrees | Where-Object { $_.Advice.Kind -eq 'choice' -and $_.Item.Enabled }).Count
    # Nettoyage
    $cRows = @($script:CleanRows | Where-Object { $_ })
    $tout = 0.0; $choisi = 0.0
    foreach ($r in $cRows) { $tout += $r.Size; if ($r.CheckBox.IsChecked) { $choisi += $r.Size } }
    # RAM : le constat s'il est fait, sinon la mesure en direct
    $mem = $script:Kit.Memoire
    $ramP = if ($mem) { [int]$mem.Pourcent } elseif ($Live.Updated -and $Live.RamTotal) { [int](100 * $Live.RamUsed / $Live.RamTotal) } else { $null }
    $ramAlerte = $mem -and @($mem.Alertes).Count

    # Conseillé pour toi : les réglages de jeu encore à faire (la fiche de chaque réglage, avec « Exécuter »)
    $a = $script:LastAnalysis
    $script:Kit.Conseils = @(if ($a) { $a.Active | Where-Object { $_.Id -like 'tweak:*' -and $_.Status -eq 'warn' } })
    $conseils = @(for ($k = 0; $k -lt $script:Kit.Conseils.Count; $k++) { $f = $script:Kit.Conseils[$k]; @{ type = 'ligne'; id = "conseil:$k"; icone = 'manette'; texte = (Format-TexteKit $f.Titre); note = (Format-TexteKit $f.Detail); etiquette = 'Conseillé'; ton = 'attention' } })

    $pages = [ordered]@{}
    $pages.perf = @{ titre = 'Performances'; sous = 'Pour gagner des FPS (images par seconde : plus il y en a, plus le jeu est fluide). Les réglages de Windows, le démarrage, la place sur le disque.'; icone = 'eclair'; couleur = 'P'
        module = $(if ($anneau) { @{ anneau = $anneau; titre = $titreReg; texte = $texteReg; boutons = @(@{ id = 'optimisation'; texte = 'Voir les réglages' }); resume = $resume } } else { @{ icone = 'eclair'; titre = $titreReg; texte = $texteReg; resume = $resume } })
        blocs = @(
            @{ type = 'titre'; texte = 'Pour gagner des FPS' }
            @{ type = 'outils'; elements = @(
                @{ id = 'optimisation'; nom = 'Optimisation gaming'; note = 'Réglages, Mode jeu, profils par jeu'; icone = 'manette'; couleur = 'P'; etiquette = $(if ($reste) { "$reste à faire" } elseif ($total) { 'Optimisé' }); ton = $(if ($reste) { 'attention' } elseif ($total) { 'ok' } else { '' }) }
                @{ id = 'demarrage'; nom = 'Démarrage'; note = 'Ce qui se lance avec le PC'; icone = 'marche'; couleur = 'S'; etiquette = $(if ($entrees.Count) { "$on programme$(if ($on -gt 1) { 's' })" }); ton = $(if ($conseil) { 'attention' } else { '' }) }
                @{ id = 'nettoyage'; nom = 'Nettoyage'; note = 'Les fichiers inutiles'; icone = 'balai'; couleur = 'T'; etiquette = $(if ($cRows.Count) { Format-Size $tout }); ton = $(if ($tout -gt 1GB) { 'attention' } elseif ($cRows.Count) { 'ok' } else { '' }) }
                @{ id = 'memoire'; nom = 'RAM'; note = 'Ce qui prend la mémoire'; icone = 'memoire'; couleur = 'P'; etiquette = $(if ($null -ne $ramP) { "$ramP %" }); ton = $(if ($ramAlerte -or $ramP -ge 80) { 'attention' } else { '' }) }
            ) }
            if ($conseils.Count) { @{ type = 'titre'; texte = 'Conseillé pour toi' } }
        ) + $conseils
    }

    # ---- Optimisation gaming : Réglages Windows, Mode jeu, Profils par jeu
    $lignesReg = @(for ($k = 0; $k -lt $rows.Count; $k++) {
        $t = $rows[$k].Tweak; $cb = $rows[$k].CheckBox
        $fait = -not $cb.IsEnabled
        $facultatif = $t.Recommended -eq $false
        $etq = if ($fait) { 'Optimisé' } elseif ($facultatif) { 'Facultatif' } else { "Impact $(([string]$t.Impact).ToLower())" }
        $l = @{ type = 'ligne'; texte = (Format-TexteKit $t.Titre); note = (Format-TexteKit $t.Description); etiquette = $etq; ton = $(if ($fait) { 'ok' } elseif ($facultatif) { '' } elseif ($t.Impact -eq 'Important') { 'urgent' } elseif ($t.Impact -eq 'Moyen') { 'attention' } else { '' }) }
        if (-not $fait) { $l.case = @{ id = 'tweak'; valeur = "$k"; oui = [bool]$cb.IsChecked } }
        if ($t.Reboot -and -not $fait) { $l.note = "$($l.note) Il faut redémarrer le PC ensuite." }
        $l
    })
    # Le module de l'onglet Réglages Windows : les réglages conseillés faits sur le total, avec Appliquer
    # (Appliquer seulement quand quelque chose est coché ; « Cocher le recommandé » seulement s'il reste un réglage conseillé)
    $coches = @($rows | Where-Object { $_.CheckBox.IsEnabled -and $_.CheckBox.IsChecked }).Count
    $modReg = @{ titre = $titreReg; texte = $texteReg }
    if ($anneau) { $modReg.anneau = @{ valeur = $okN; sur = $total; petit = "/$total"; ton = $(if ($reste) { 'attention' } else { '' }) } } else { $modReg.icone = 'reglages'; $modReg.couleur = 'P' }
    $modReg.boutons = @(
        if ($coches) { @{ id = 'clic:BtnApply'; texte = $(if ($coches -gt 1) { "Appliquer ($coches)" } else { 'Appliquer' }) } }
        if ($reste) { @{ id = 'opti:recommande'; texte = 'Cocher le recommandé'; style = $(if ($coches) { 'second' } else { $null }) } }
    )
    if ($coches) { $modReg.case = @{ id = 'coche:ChkRestore'; texte = 'Créer un point de restauration avant (conseillé)'; oui = [bool]$ui.ChkRestore.IsChecked } }
    $sel = Get-GameModeSelection
    $modeOn = [bool](Get-Setting 'GameMode' $false)
    $etatMode = if ($modeOn -and $script:GameModeStatus -and $script:GameModeStatus.Text) { " $([string]$script:GameModeStatus.Text)" } else { '' }
    $presentes = @(Get-AppsModeJeuKit)
    $appsLa = @($GameModeApps | Where-Object { $presentes -contains $_.Id })
    $autres = @($GameModeApps | Where-Object { $presentes -notcontains $_.Id } | ForEach-Object { [string]$_.Name })
    # Le module de l'onglet Mode jeu : son état et son interrupteur (la case du module)
    $nCoches = @($sel).Count
    $modMode = @{ icone = 'manette'; couleur = $(if ($modeOn) { 'vert' } else { 'doux' }); titre = "Mode jeu $(if ($modeOn) { 'activé' } else { 'désactivé' })"
        texte = "Quand un de tes jeux démarre, les applis cochées sont fermées, puis relancées quand tu quittes le jeu.$etatMode"
        case = @{ id = 'modejeu'; texte = 'Fermer des applis pendant que je joue'; oui = $modeOn }
        resume = @(
            @{ nom = 'Applis cochées'; valeur = "$nCoches" }
            @{ nom = 'Ouvertes en ce moment'; valeur = "$($appsLa.Count)" }
        ) }
    $modeJeu = @(
        @{ type = 'titre'; texte = 'Applis à fermer'; droite = 'Ouvertes en ce moment' }
        if (-not $appsLa.Count) { @{ type = 'texte'; texte = 'Aucune des applis que Nevermind sait fermer n''est ouverte en ce moment.' } }
    ) + @(foreach ($x in $appsLa) { @{ type = 'case'; id = 'modejeu-app'; valeur = [string]$x.Id; texte = [string]$x.Name; oui = ($sel -contains $x.Id) } }) + @(
        if ($autres.Count) { @{ type = 'note'; texte = "Nevermind sait aussi fermer $($autres -join ', ') : elles ne sont pas ouvertes en ce moment, elles apparaîtront ici quand elles le seront." } }
        @{ type = 'note'; texte = 'Ne coche pas le lanceur du jeu auquel tu joues (Epic, EA, Ubisoft). Marche tant que Nevermind est ouvert, même réduit.' }
    )
    $profils = New-Object System.Collections.ArrayList
    $script:Kit.JeuxProfils = @()
    $boutonAjout = @{ id = 'ajouterjeu'; texte = 'Ajouter un jeu' }
    $modProfils = @{ icone = 'manette'; couleur = 'S'; titre = 'Profils par jeu'; texte = 'Lecture des profils de tes jeux...'; boutons = @($boutonAjout) }
    if ($null -eq $script:Games) { [void]$profils.Add(@{ type = 'texte'; texte = 'Recherche des jeux installés...' }); $modProfils.texte = 'Recherche des jeux installés...' }
    elseif ($script:Vue.Ecran -ne 'optimisation' -and -not $script:Kit.Profils) { [void]$profils.Add(@{ type = 'texte'; texte = 'Lecture des profils...' }) }   # (lus en arrivant sur la page)
    else {
        $pr = Get-ProfilsKit
        $script:Kit.JeuxProfils = @($pr.Liste | ForEach-Object { $_.Jeu })
        [void]$profils.Add(@{ type = 'texte'; texte = 'Ces réglages sont appliqués par Windows à chaque lancement du jeu, même Nevermind fermé. « Priorité haute » : le jeu passe avant les autres programmes.' })
        $nP = @($pr.Liste).Count
        # Le module de l'onglet : les jeux en priorité haute sur les jeux dont Nevermind connaît le programme
        $nPrio = @($pr.Liste | Where-Object { $_.Prio }).Count
        $modProfils = if ($nP) {
            @{ anneau = @{ valeur = $nPrio; sur = $nP; petit = "/$nP" }; titre = "$nPrio jeu$(if ($nPrio -gt 1) { 'x' }) en priorité haute"
                texte = "Sur $nP jeu$(if ($nP -gt 1) { 'x' }) dont Nevermind connaît le programme."
                boutons = @($boutonAjout)
                resume = @(@{ nom = 'Jeux reconnus'; valeur = "$nP" }, @{ nom = 'Priorité haute'; valeur = "$nPrio" }) + @(if ($pr.Deux) { @{ nom = 'Carte puissante'; valeur = "$(@($pr.Liste | Where-Object { $_.Gpu }).Count)" } }) }
        } else { @{ icone = 'manette'; couleur = 'S'; titre = 'Aucun jeu reconnu'; texte = 'Nevermind ne connaît le programme d''aucun de tes jeux pour l''instant. Ajoute ton jeu pour lui faire un profil.'; boutons = @($boutonAjout) } }
        [void]$profils.Add(@{ type = 'titre'; texte = "$nP jeu$(if ($nP -gt 1) { 'x' }) dont Nevermind connaît le programme" })
        $nLib = @(try { Get-LibraryGames } catch { }).Count
        if ($nLib -gt $nP) { [void]$profils.Add(@{ type = 'texte'; texte = "Ta bibliothèque compte $nLib jeux : pour $(if ($nLib - $nP -gt 1) { "$($nLib - $nP) d'entre eux" } else { "l'un d'eux" }), Nevermind ne connaît pas le programme du jeu, il ne peut donc pas lui faire de profil." }) }
        $k = 0
        foreach ($x in $pr.Liste) {
            $b = @(@{ id = "profil:$k`:priority"; texte = 'Priorité haute'; oui = $x.Prio })
            if ($pr.Deux) { $b += @{ id = "profil:$k`:gpu"; texte = 'Carte puissante'; oui = [bool]$x.Gpu } }
            [void]$profils.Add(@{ type = 'ligne'; icone = 'manette'; couleur = 'S'; texte = $x.Nom; note = $x.Note; bascules = $b })
            $k++
        }
    }
    $pages.optimisation = @{ titre = 'Optimisation gaming'; parent = 'perf'; sous = 'Règle Windows pour tes jeux : réglages, Mode jeu et profils par jeu.'; icone = 'manette'; couleur = 'P'
        module = @{ icone = 'manette'; couleur = 'P'; titre = 'Pour tes jeux'; texte = 'Trois façons de régler Windows pour tes jeux : les réglages, le Mode jeu et les profils par jeu.'
            resume = @(
                @{ nom = 'Réglages conseillés'; valeur = $(if ($total) { "$okN sur $total faits" } else { '...' }); ton = $(if ($reste) { 'attention' } elseif ($total) { 'ok' } else { '' }) }
                @{ nom = 'Mode jeu'; valeur = $(if ($modeOn) { 'Activé' } else { 'Désactivé' }); ton = $(if ($modeOn) { 'ok' } else { '' }) }
                @{ nom = 'Profils par jeu'; valeur = $(if ($script:Kit.Profils) { $np = @($script:Kit.Profils.Liste | Where-Object { $_.Prio }).Count; "$np en priorité haute" } else { '...' }) }
            ) }
        onglets = @(
            @{ id = 'reglages'; nom = 'Réglages Windows'; module = $modReg; blocs = @(
                @{ type = 'titre'; texte = 'Les réglages'; droite = 'Chacun s''annule' }
            ) + $lignesReg + @(@{ type = 'note'; texte = 'Seulement des réglages à l''effet reconnu : pas de « réglages miracles ». Coche ceux que tu veux, puis « Appliquer ». Chacun s''annule depuis la page Sauvegarde.' }) }
            @{ id = 'mode'; nom = 'Mode jeu'; module = $modMode; blocs = $modeJeu }
            @{ id = 'profils'; nom = 'Profils par jeu'; module = $modProfils; blocs = @($profils) }
        ) }

    # ---- Démarrage
    $lignesDem = @(for ($k = 0; $k -lt $entrees.Count; $k++) {
        $s = $entrees[$k]
        $actif = [bool]$s.Item.Enabled
        # (le conseil suit l'état : un programme déjà coupé n'a plus besoin du conseil « Tu peux le désactiver »)
        $etq = [string]$s.Advice.Label; $why = Format-TexteKit $s.Advice.Why; $ton = $(switch ($s.Advice.Kind) { 'safe' { 'ok' } 'keep' { 'attention' } default { '' } })
        if (-not $actif) {
            if ($s.Advice.Kind -eq 'keep') { $etq = 'Coupé'; $why = 'Pilote ou protection de ton PC : mieux vaut le remettre.'; $ton = 'attention' }
            else { $etq = 'Coupé au démarrage'; $why = 'Il ne se lance plus avec le PC. Il s''ouvre toujours quand tu cliques dessus.'; $ton = '' }
        }
        @{ type = 'ligne'; image = (Get-IconeExeKit $s.IconExe); icone = 'app'; couleur = 'doux'; texte = [string]$s.Name; note = $why
            etiquette = $etq; ton = $ton
            bascule = @{ id = "dem:$k"; oui = $actif }; fait = -not $actif }
    })
    $pages.demarrage = @{ titre = 'Démarrage'; parent = 'perf'; sous = 'Les programmes qui se lancent tout seuls quand tu allumes ton PC.'; icone = 'marche'; couleur = 'S'
        actions = @(@{ id = 'clic:BtnRefreshStartup'; texte = 'Actualiser'; icone = 'actu' })
        module = $(if ($entrees.Count -and $on -and $conseil) { @{ anneau = @{ valeur = $conseil; sur = $on; petit = "/$on" }; titre = $(if ($conseil) { "$conseil à couper" } else { 'Rien à couper' }); texte = "Sur $on programme$(if ($on -gt 1) { 's' }) au démarrage, $(if ($conseil -eq 1) { "1 ne sert pas dès l'allumage. Le couper rend l'allumage plus rapide, sans rien désinstaller." } elseif ($conseil) { "$conseil ne servent pas dès l'allumage. Les couper rend l'allumage plus rapide, sans rien désinstaller." } else { 'aucun n''est à couper : ce qui reste est utile.' })"; boutons = @(if ($conseil) { @{ id = 'clic:BtnDisableStartup'; texte = "Couper ce qui est conseillé ($conseil)" } }) } }
            elseif ($on -and $aVoir) { @{ icone = 'marche'; couleur = 'S'; titre = 'À toi de voir'; texte = "Sur $on programme$(if ($on -gt 1) { 's' }) au démarrage, aucun n'est inutile à coup sûr. $(if ($aVoir -eq 1) { '1 dépend' } else { "$aVoir dépendent" }) de ton usage : regarde ceux marqués « À toi de voir ». Rien n'est supprimé si tu en coupes un." } }
            else { @{ icone = 'marche'; couleur = 'S'; titre = $(if ($on) { 'Rien à couper' } elseif ($entrees.Count) { 'Rien ne se lance' } else { 'Lecture en cours' }); texte = $(if ($on -eq 1) { "1 programme se lance avec le PC, et il est utile. Tu peux quand même le couper, rien n'est supprimé." } elseif ($on) { "$on programmes se lancent avec le PC, et tous sont utiles. Tu peux quand même en couper un, rien n'est supprimé." } else { [string]$ui.StartupCount.Text }) } })
        blocs = @(@{ type = 'titre'; texte = 'Au démarrage'; droite = 'Activé : se lance avec le PC' }) + $(if ($lignesDem.Count) { $lignesDem } else { @(@{ type = 'texte'; texte = 'Aucun programme ne se lance avec Windows.' }) }) + @(@{ type = 'note'; texte = 'Rien n''est supprimé : un programme coupé s''ouvre toujours quand tu cliques dessus, et tu peux le remettre quand tu veux.' }) }

    # ---- Nettoyage
    $disque = try { [IO.DriveInfo]::new($env:SystemDrive) } catch { $null }
    $lignesNet = @(for ($k = 0; $k -lt $cRows.Count; $k++) {
        $r = $cRows[$k]; $c = $r.Target; $vide = -not $r.CheckBox.IsEnabled
        $l = @{ type = 'ligne'; texte = [string]$c.Titre; note = [string]$c.Desc; valeur = $(if ($vide) { '0' } else { Format-Size $r.Size }); ton = $(if ($r.Size -gt 500MB) { 'attention' } else { '' }) }
        if ($vide) { $l.etiquette = 'Déjà propre'; $l.ton = 'ok'; $l.fait = $true } else {
            $l.case = @{ id = 'nett'; valeur = "$k"; oui = [bool]$r.CheckBox.IsChecked }
            $l.bouton = @{ id = "nett-voir:$k"; texte = 'Voir'; style = 'second' }
        }
        $l
    })
    $dern = Get-Setting 'LastClean' $null
    $pages.nettoyage = @{ titre = 'Nettoyage'; parent = 'perf'; sous = 'Libère de la place en supprimant les fichiers inutiles.'; icone = 'balai'; couleur = 'T'
        actions = @(if ($cRows.Count) { @{ id = 'clic:BtnCleanScan'; texte = 'Analyser de nouveau'; icone = 'actu' } })   # (avant la première analyse : le bouton de la carte seulement)
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
            if ($dern -and $dern.Date) { @{ type = 'ligne'; icone = 'horloge'; texte = "Dernier nettoyage le $(([datetime]$dern.Date).ToString('dd/MM à HH:mm'))"; note = "$(Format-Size ([double]$dern.Freed)) libérés, $($dern.Deleted) fichier$(if ([int]$dern.Deleted -gt 1) { 's' }) supprimé$(if ([int]$dern.Deleted -gt 1) { 's' }), $($dern.Skipped) laissé$(if ([int]$dern.Skipped -gt 1) { 's' }) car utilisé$(if ([int]$dern.Skipped -gt 1) { 's' })"; bouton = $(if ($dern.File -and (Test-Path -LiteralPath ([string]$dern.File))) { @{ id = 'nett-journal'; texte = 'Ouvrir le journal'; style = 'second' } }) } }
            @{ type = 'note'; texte = 'Nevermind ne touche jamais à tes documents, à la corbeille ni aux fichiers que les jeux préparent pour l''affichage (caches de shaders) : les vider provoquerait des saccades le temps qu''ils se reconstruisent.' }
        ) }

    # ---- RAM : qu'est-ce qui prend ma mémoire ?
    $pages.memoire = Get-PageMemoireKit $mem ([string]$script:Kit.MemoireEnCours)
    $pages
}

# La page RAM (memoire.ps1 fait le constat en tâche de fond : en arrivant sur la page, puis avec « Actualiser »).
# $EnCours : 'donnees' (lecture du PC) ou 'pilotes' (recherche du pilote qui garde la mémoire), vide sinon.
function Get-PageMemoireKit($C, [string]$EnCours) {
    $page = @{ titre = 'RAM'; parent = 'perf'; sous = 'Ce qui prend la mémoire de ton PC, et ce qui peut être coupé sans risque.'; icone = 'memoire'; couleur = 'P'
        actions = @(if (-not $EnCours) { @{ id = 'memoire:analyser'; texte = 'Actualiser'; icone = 'actu' } }) }
    if (-not $C) {
        $page.module = @{ icone = 'memoire'; couleur = 'P'; titre = 'Je regarde ta mémoire'; texte = 'Quelques secondes : je fais le tour de ce qui tourne sur ton PC.'
            boutons = @(if (-not $EnCours) { @{ id = 'memoire:analyser'; texte = 'Regarder' } }) }
        $page.blocs = @(@{ type = 'texte'; texte = 'Nevermind regroupe la mémoire par appli et te dit ce qui est utile, ce qui ne sert pas au quotidien, et ce qui est à vérifier.' })
        return $page
    }
    $n = @($C.Corrections).Count
    $page.module = @{ anneau = @{ valeur = [int]$C.Pourcent; sur = 100; petit = '%'; ton = $(if ($C.Pourcent -ge 80) { 'attention' } else { '' }) }
        titre = "$(Format-Size $C.Utilisee) utilisés sur $(Format-Size $C.Visible)"
        texte = $(if ($n -eq 1) { '1 chose ne sert pas au quotidien et peut être coupée sans risque.' } elseif ($n) { "$n choses ne servent pas au quotidien et peuvent être coupées sans risque." } else { 'Rien à couper sans risque : ce qui reste se règle à la main (page Démarrage), ou est utile.' })
        boutons = @(
            if ($n) { @{ id = 'memoire:couper'; texte = "Couper ce qui est sûr ($n)" } }
            @{ id = 'memoire:rapport'; texte = 'Créer un rapport'; style = $(if ($n) { 'second' } else { $null }) }
        )
        resume = @(
            @{ nom = 'Installée'; valeur = (Format-Size $C.Installee) }
            @{ nom = 'En cache (ne compte pas)'; valeur = (Format-Size $C.Cache) }
            @{ nom = 'Libre tout de suite'; valeur = (Format-Size $C.Disponible) }
        ) }
    $alertes = @(foreach ($a in @($C.Alertes)) {
        $suite = if ($a.Id -eq 'pilotes' -and $EnCours -eq 'pilotes') { ' Je cherche encore quel pilote en est la cause...' } else { '' }
        @{ type = 'ligne'; icone = 'alerte'; couleur = $(if ($a.Ton -eq 'urgent') { 'rouge' } else { 'orange' }); texte = [string]$a.Titre; note = "$($a.Texte)$suite Que faire : $($a.Quoi)"; etiquette = $(if ($a.Ton -eq 'urgent') { 'Important' } else { 'À surveiller' }); ton = [string]$a.Ton }
    })
    $max = [math]::Max(1, [double]@($C.Applis)[0].Octets)
    $grosses = @(@($C.Applis) | Where-Object { $_.Octets -ge 20MB } | Select-Object -First 30)
    $petites = @(@($C.Applis) | Where-Object { $grosses -notcontains $_ })
    $petitesOctets = 0.0; foreach ($g in $petites) { $petitesOctets += $g.Octets }
    $lignes = @(foreach ($g in $grosses) {
        @{ type = 'ligne'; icone = $(switch ($g.Kind) { 'windows' { 'pc' } 'inutile' { 'balai' } default { 'app' } }); couleur = $(switch ($g.Kind) { 'windows' { 'doux' } 'inutile' { 'orange' } default { 'P' } })
            texte = "$($g.Nom)$(if ($g.Nb -gt 1) { " ($($g.Nb) processus)" })"; note = [string]$g.Note
            barre = [int](100 * $g.Octets / $max); valeur = (Format-Size $g.Octets); etiquette = [string]$g.Etiquette
            ton = $(switch ($g.Kind) { 'windows' { 'ok' } 'inutile' { 'attention' } default { '' } }) }
    })
    if ($petites.Count) { $lignes += @{ type = 'ligne'; icone = 'liste'; couleur = 'doux'; texte = "$($petites.Count) petits programmes"; note = 'Chacun prend moins de 20 Mo.'; valeur = (Format-Size $petitesOctets) } }
    $cachee = @(foreach ($x in (Get-PartCacheeMemoire $C)) {
        $l = @{ type = 'ligne'; icone = 'memoire'; couleur = 'doux'; texte = [string]$x.Nom; note = [string]$x.Note; valeur = (Format-Size $x.Octets) }
        if ($x.Nom -like 'En cache*') { $l.ton = 'ok'; $l.etiquette = 'Ne compte pas' }
        $l
    })
    $page.blocs = @(
        if ($alertes.Count) { @{ type = 'titre'; texte = 'À regarder' } }
    ) + $alertes + @(
        @{ type = 'titre'; texte = 'Par appli'; droite = 'Du plus gros au plus petit' }
    ) + $lignes + @(
        @{ type = 'titre'; texte = 'La part cachée'; droite = 'Ce qui n''est à aucune appli' }
    ) + $cachee + @(
        @{ type = 'note'; texte = 'Nevermind ne ferme jamais une appli à ta place. « Couper ce qui est sûr » ne touche qu''aux mises à jour et aux logiciels du fabricant, rien de Windows, et se remet comme avant depuis la page Sauvegarde.' }
    )
    $page
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
    Register-ActionVue 'memoire:analyser' { param($Valeur, $Ecran) Invoke-Safe { Start-ConstatMemoire } }
    Register-ActionVue 'memoire:couper' { param($Valeur, $Ecran) Invoke-Safe { Invoke-CorrectionMemoire } }
    Register-ActionVue 'memoire:rapport' { param($Valeur, $Ecran) Invoke-Safe { New-RapportMemoire } }
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
