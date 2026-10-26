# Nevermind : les pages de l'écran (kit) des jeux : Jeux (Bibliothèque, Perf. In Game : FPS et Lag en ligne), la fiche
# d'un jeu, Overlay et Trafic.
# Les données viennent des mêmes fonctions que l'interface d'avant (Get-LibraryGames, Get-PlayLog, Get-FpsSessions,
# Get-LagSessions, $script:Traffic...) ; le détail d'une partie, d'une mesure ou d'un programme s'ouvre dans le panneau
# (ecran-panneau.ps1).
# Chargé par Nevermind.ps1 après ecran-panneau.ps1.

# Les jaquettes des jeux (adresse pour l'écran) : gardées tant que les jeux et les jaquettes trouvées ne changent pas
function Get-JaquetteKit($Jeu) {
    if (-not $script:Kit.Jaquettes) { $script:Kit.Jaquettes = @{} }
    $f = Get-CoverFile $Jeu
    if (-not $f) { return $null }
    $u = $script:Kit.Jaquettes[$f]
    if (-not $u) { $u = Get-ImageVue $f; $script:Kit.Jaquettes[$f] = $u }
    $u
}

function Get-PagesJeux {
    $pages = [ordered]@{}
    $tous = @(if ($null -ne $script:Games) { Get-LibraryGames })
    $log = Get-PlayLog
    $last = Get-DernierJeuKit
    $lourd = $script:Vue.Ecran -in 'jeux', 'jeu'

    # ---- Bibliothèque
    $filtre = if ($script:Kit.JeuxFiltre) { $script:Kit.JeuxFiltre } else { 'Tous' }
    $q = ([string]$script:Kit.JeuxRecherche).Trim()
    $sources = @($tous | Group-Object { if ($_.Source) { $_.Source } else { 'Steam' } } | Sort-Object Count -Descending | ForEach-Object { $_.Name })
    $recents = @($tous | Where-Object { $log.ContainsKey($_.Name) }).Count
    $montres = @($tous | Where-Object {
        $src = if ($_.Source) { $_.Source } else { 'Steam' }
        ($filtre -eq 'Tous' -or ($filtre -eq 'Récents' -and $log.ContainsKey($_.Name)) -or $src -eq $filtre) -and
        (-not $q -or (ConvertTo-SearchText $_.Name).Contains((ConvertTo-SearchText $q)))
    } | Sort-Object @{ Expression = { if ($log.ContainsKey($_.Name)) { $log[$_.Name].Last } else { '' } }; Descending = $true }, @{ Expression = { $_.Name } })
    $script:Kit.JeuxMontres = $montres
    $grille = @(if ($lourd) { for ($k = 0; $k -lt [math]::Min(120, $montres.Count); $k++) {
        $g = $montres[$k]; $pl = $log[$g.Name]
        @{ id = "jeu:$k"; titre = [string]$g.Name; sous = $(if ($pl) { "Joué $(Format-LastPlayed $pl.Last)" } elseif ($g.Source) { [string]$g.Source } else { 'Steam' }); image = (Get-JaquetteKit $g); couleur = @('P', 'S', 'T')[$k % 3]
            marque = $(if ($script:GameSession -and $script:GameSession.Game -eq $g.Name) { 'En jeu' }) }
    } })
    $choixFiltre = @(@{ id = 'Tous'; texte = "Tous ($($tous.Count))" }) + @(if ($recents) { @{ id = 'Récents'; texte = "Récents ($recents)" } }) + @($sources | ForEach-Object { $n = $_; @{ id = $n; texte = "$n ($(@($tous | Where-Object { $(if ($_.Source) { $_.Source } else { 'Steam' }) -eq $n }).Count))" } })
    $blocsLib = @(
        @{ type = 'champ'; id = 'jeux-cherche'; recherche = $true; valeur = $q; indice = 'Chercher un jeu...' }
        if ($tous.Count) { @{ type = 'choix'; id = 'jeux-filtre'; valeur = $filtre; options = $choixFiltre } }
        @{ type = 'titre'; texte = 'Ta bibliothèque'; droite = "$($montres.Count) jeu$(if ($montres.Count -gt 1) { 'x' })" }
        if ($null -eq $script:Games) { @{ type = 'texte'; texte = 'Recherche de tes jeux...' } }
        elseif (-not $montres.Count) { @{ type = 'texte'; texte = $(if ($tous.Count) { 'Aucun jeu ne correspond.' } else { 'Aucun jeu trouvé sur ce PC. Clique sur « Ajouter un jeu » pour ajouter le tien.' }) } }
        elseif ($grille.Count) { @{ type = 'grille'; elements = $grille } }
        else { @{ type = 'texte'; texte = 'Chargement des jaquettes...' } }
        if ($ui.LibLeftoverBar.Visibility -eq 'Visible') { @{ type = 'ligne'; icone = 'poubelle'; couleur = 'orange'; texte = [string]$ui.LibLeftoverText.Text; bouton = @{ id = 'clic:BtnLibLeftovers'; texte = 'Voir'; style = 'second' } } }
        @{ type = 'reglages'; rangs = @(@{ texte = 'Jaquettes depuis Internet'; note = 'Les images des jeux qui n''en ont pas sont cherchées sur la boutique Steam puis Wikipédia (seul le nom du jeu est envoyé).'; bascule = @{ id = 'coche:ChkLibCovers'; oui = [bool]$ui.ChkLibCovers.IsChecked } }) }
    )

    # ---- Perf. In Game : FPS
    $sessions = @(Get-FpsSessions | Sort-Object { [datetime]$_.Date } -Descending)
    $maxFps = [math]::Max(60.0, (@($sessions | Select-Object -First 12 | ForEach-Object { [double]$_.Avg }) + 0 | Measure-Object -Maximum).Maximum)
    $blocsFps = @(
        @{ type = 'reglages'; rangs = @(
            @{ texte = 'Mesurer mes FPS quand je joue'; note = 'Automatique pour tes jeux (Steam, Epic, Ubisoft, EA, Battle.net, Riot, GOG, Xbox...). Pour un autre jeu : Ctrl + Maj + F pendant la partie.'; bascule = @{ id = 'fps-mesure'; oui = [bool](Test-FpsMeasure) } }
            @{ texte = 'Le compteur par-dessus le jeu'; note = "Compteur à l'écran : $(if (Test-FpsOverlay) { "affiché, style $(Get-FpsOverlayStyle), $($OverlayCorners[(Get-FpsOverlayCorner)].ToLower())" } else { 'masqué' })."; lien = @{ id = 'overlay'; texte = 'Régler le compteur' } }
        ) }
        if (-not (Test-Path -LiteralPath $PresentMonExe)) { @{ type = 'note'; texte = 'PresentMon est absent du dossier de Nevermind : réinstalle Nevermind.' } }
    )
    if ($sessions.Count) {
        $cmpS = @(Get-FpsSessions | Where-Object { $_.Key -eq $sessions[0].Key } | Sort-Object { [datetime]$_.Date })
        $lc = Get-LastChangeDate
        if ($lc) {
            $avant = @($cmpS | Where-Object { [datetime]$_.Date -lt $lc } | Select-Object -Last 3); $apres = @($cmpS | Where-Object { [datetime]$_.Date -ge $lc } | Select-Object -Last 3)
            if ($avant.Count -and $apres.Count) {
                $b = Measure-FpsGroup $avant; $a = Measure-FpsGroup $apres; $pct = if ($b[0] -gt 0) { 100 * ($a[0] - $b[0]) / $b[0] } else { 0 }
                $blocsFps += @{ type = 'titre'; texte = "$(Get-SessionName $sessions[0]) : avant / après tes derniers réglages" }
                $blocsFps += @{ type = 'chiffres'; elements = @(
                    @{ titre = "Avant le $($lc.ToString('dd/MM'))"; valeur = ('{0:N0}' -f $b[0]); unite = 'FPS' }
                    @{ titre = 'Après'; valeur = ('{0:N0}' -f $a[0]); unite = 'FPS' }
                    @{ titre = 'Différence'; valeur = $(if ([math]::Abs($pct) -lt 3) { 'Pareil' } else { '{0}{1:N0} %' -f $(if ($pct -gt 0) { '+' } else { '' }), $pct }); ton = $(if ([math]::Abs($pct) -lt 3) { '' } elseif ($pct -gt 0) { 'ok' } else { 'attention' }) }
                ) }
            }
        }
        $blocsFps += @{ type = 'bouton'; id = 'fps-aide'; texte = 'Mes FPS ne sont pas normaux : trouver pourquoi' }
        $blocsFps += @{ type = 'titre'; texte = 'Dernières parties'; droite = 'Clique pour le détail' }
        foreach ($s in @($sessions | Select-Object -First 12)) {
            $d = [datetime]$s.Date; $pb = Test-FpsProblem $s
            $blocsFps += @{ type = 'ligne'; id = "fps:$($s.Id)"; quand = @($d.ToString('dd/MM'), $d.ToString('HH:mm')); texte = (Get-SessionName $s); note = "$(Format-PlayTime $s.Seconds) · 1 % bas $('{0:N0}' -f $s.Low1)$(if ($pb) { ', à vérifier' })"
                barre = [int][math]::Min(100, 100 * $s.Avg / $maxFps); valeur = ('{0:N0} FPS' -f $s.Avg); ton = $(if ($s.Avg -lt 30) { 'urgent' } elseif ($s.Avg -lt 60 -or $pb) { 'attention' } else { '' }) }
        }
    } else { $blocsFps += @{ type = 'texte'; texte = 'Aucune partie mesurée pour l''instant. Joue au moins 30 secondes : tes FPS moyens, tes chutes et la courbe de la partie apparaîtront ici.' } }

    # ---- Perf. In Game : Lag en ligne
    $lags = @(Get-LagSessions | Sort-Object { [datetime]$_.Date } -Descending | Select-Object -First 15)
    $blocsLag = @(
        @{ type = 'reglages'; rangs = @(@{ texte = 'Mesurer ma connexion quand je joue'; note = 'Pour tes jeux en ligne : Nevermind mesure ta box, Internet et le serveur du jeu pendant la partie, puis t''explique d''où vient le lag.'; bascule = @{ id = 'lag-mesure'; oui = [bool](Test-LagMeasure) } }) }
        if (-not $script:LagSession) { @{ type = 'bouton'; id = 'lag-test'; texte = 'Tester ma connexion maintenant (30 s)' } }
        if ($script:LagLive -and $script:LagLive.Visibility -eq 'Visible' -and $script:LagLive.Text) { @{ type = 'pastilles'; elements = @(@{ texte = [string]$script:LagLive.Text; ton = 'ok' }) } }
        if ($lags.Count) { @{ type = 'titre'; texte = 'Dernières mesures'; droite = 'Clique pour le détail' } } else { @{ type = 'texte'; texte = 'Aucune mesure pour l''instant. Joue une partie en ligne (au moins une minute) ou lance le test rapide : le résultat apparaîtra ici.' } }
    ) + @(foreach ($s in $lags) {
        $d = [datetime]$s.Date; $niv = [string]$s.Diag.Level
        @{ type = 'ligne'; id = "lag:$($s.Id)"; quand = @($d.ToString('dd/MM'), $d.ToString('HH:mm')); texte = "$($s.Game) : $($s.Diag.Title)"; note = "$(Format-PlayTime $s.Seconds). $(Get-LagSessionLine $s)"; etiquette = $(switch ($niv) { 'ok' { 'Stable' } 'warn' { 'À voir' } 'bad' { 'Problème' } default { '' } }); ton = $TonsStatut[$niv] }
    })

    # ---- La page Jeux : le module montre le dernier jeu joué (« Reprendre »)
    $jeuM = if ($last) { $last.Game } elseif ($montres.Count) { $montres[0] } else { $null }
    $script:Kit.JeuModule = $jeuM
    $module = if ($jeuM) {
        $pl = $log[$jeuM.Name]
        @{ image = (Get-JaquetteKit $jeuM); legende = [string]$jeuM.Name; titre = $(if ($last) { 'Reprendre' } else { [string]$jeuM.Name })
            texte = $(if ($pl) { "Dernière partie $(Format-LastPlayed $pl.Last)$(if ($last -and $last.Last) { ", $('{0:N0}' -f $last.Last.Avg) FPS en moyenne" })." } else { 'Pas encore joué depuis Nevermind.' })
            boutons = @(@{ id = 'jeu-module-lancer'; texte = $(if ($script:GameSession -and $script:GameSession.Game -eq $jeuM.Name) { 'En cours de jeu' } else { 'Jouer' }) }, @{ id = 'jeu-module-fiche'; texte = 'Voir le jeu'; style = 'second' })
            resume = @(@{ nom = 'Lanceur'; valeur = $(if ($jeuM.Source) { [string]$jeuM.Source } else { 'Steam' }) }, @{ nom = 'Bibliothèque'; valeur = "$($tous.Count) jeu$(if ($tous.Count -gt 1) { 'x' })" }) }
    } else { @{ icone = 'manette'; couleur = 'S'; titre = 'Tes jeux'; texte = 'Tous tes jeux, quel que soit le lanceur (Steam, Epic...).' } }

    # ---- Les modules de Perf. In Game : la dernière partie mesurée (FPS), la dernière mesure de connexion (Lag en ligne)
    $modFps = if ($sessions.Count) {
        $s0 = $sessions[0]; $d0 = [datetime]$s0.Date
        @{ icone = 'jauge'; couleur = $(if ($s0.Avg -lt 30) { 'rouge' } elseif ($s0.Avg -lt 60) { 'orange' } else { 'vert' }); titre = "$('{0:N0}' -f $s0.Avg) FPS en moyenne"
            texte = "Dernière partie mesurée : $(Get-SessionName $s0), le $($d0.ToString('dd/MM')) à $($d0.ToString('HH:mm')), pendant $(Format-PlayTime $s0.Seconds)."
            boutons = @(@{ id = "fps:$($s0.Id)"; texte = 'Voir le détail' })
            resume = @(@{ nom = 'Jeu'; valeur = [string](Get-SessionName $s0) }, @{ nom = 'FPS moyens'; valeur = ('{0:N0}' -f $s0.Avg) }, @{ nom = '1 % bas'; valeur = ('{0:N0}' -f $s0.Low1) }, @{ nom = 'Parties mesurées'; valeur = "$($sessions.Count)" }) }
    } else { @{ icone = 'jauge'; couleur = 'doux'; titre = 'Aucune partie mesurée'; texte = 'Joue au moins 30 secondes : tes FPS moyens et tes chutes apparaîtront ici.' } }
    $modLag = if ($lags.Count) {
        $s0 = $lags[0]; $d0 = [datetime]$s0.Date; $niv = [string]$s0.Diag.Level; $ligneLag = [string](Get-LagSessionLine $s0)
        @{ icone = 'internet'; couleur = $(switch ($niv) { 'ok' { 'vert' } 'warn' { 'orange' } 'bad' { 'rouge' } default { 'T' } }); titre = [string]$s0.Diag.Title
            texte = "Dernière mesure : $($s0.Game), le $($d0.ToString('dd/MM')) à $($d0.ToString('HH:mm')).$(if ($ligneLag) { " $ligneLag." } else { '' })"
            boutons = @(@{ id = "lag:$($s0.Id)"; texte = 'Voir le détail' })
            resume = @(@{ nom = 'État'; valeur = $(switch ($niv) { 'ok' { 'Stable' } 'warn' { 'À voir' } 'bad' { 'Problème' } default { '-' } }); ton = $(if ($TonsStatut[$niv]) { [string]$TonsStatut[$niv] } else { '' }) }, @{ nom = 'Durée'; valeur = (Format-PlayTime $s0.Seconds) }) }
    } else { @{ icone = 'internet'; couleur = 'doux'; titre = 'Aucune mesure'; texte = 'Joue une partie en ligne (au moins une minute) ou lance le test rapide : Nevermind te dira d''où vient le lag.' } }
    $pages.jeux = @{ titre = 'Jeux'; sous = 'Tous tes jeux, quel que soit le lanceur (Steam, Epic...).'; icone = 'manette'; couleur = 'S'
        actions = @(@{ id = 'jeux-organizer'; texte = 'Organizer Dofus'; style = 'second' }, @{ id = 'clic:BtnLibRefresh'; texte = 'Actualiser'; icone = 'actu' }, @{ id = 'clic:BtnLibAdd'; texte = 'Ajouter un jeu'; icone = 'plus' })
        module = $module
        onglets = @(
            @{ id = 'lib'; nom = 'Bibliothèque'; module = $module; blocs = $blocsLib }
            @{ id = 'perf'; nom = 'Perf. In Game'; texte = 'Tes FPS et ta connexion pendant chaque partie : ce qui ralentit, et comment le corriger.'; onglets = @(@{ id = 'fps'; nom = 'FPS'; module = $modFps; blocs = $blocsFps }, @{ id = 'lag'; nom = 'Lag en ligne'; module = $modLag; blocs = $blocsLag }) }
        ) }

    # ---- La fiche d'un jeu
    $g = $script:Kit.JeuChoisi
    if ($g -and $lourd) {
        $pl = $log[$g.Name]
        $fsess = @($sessions | Where-Object { (Get-SessionName $_) -eq $g.Name -or @($g.Exes | ForEach-Object { [IO.Path]::GetFileNameWithoutExtension($_).ToLower() }) -contains [string]$_.Key })
        $dern = $fsess | Select-Object -First 1
        $enJeu = $script:GameSession -and $script:GameSession.Game -eq $g.Name
        $how = Get-GameLaunch $g
        $opt = @(Get-GameOptimizations $g)
        $script:Kit.JeuOptims = $opt
        $afaire = @($opt | Where-Object { -not $_.Ok -and ($_.Fix -or $_.Switch) -and -not $_.Optional })
        $hero = Get-HeroFile $g
        $lignesOpt = @(for ($k = 0; $k -lt $opt.Count; $k++) {
            $it = $opt[$k]
            $l = @{ type = 'ligne'; icone = $(if ($it.Ok) { 'ok' } else { 'alerte' }); couleur = $(if ($it.Ok) { 'vert' } elseif ($it.Optional) { 'doux' } else { 'orange' }); texte = [string]$it.Title; note = [string]$it.Text }
            if ($it.Switch) { $l.bascule = @{ id = "jeu-opt:$k"; oui = [bool]$it.Ok } }
            elseif ($it.Fix -eq 'tweaks') { $l.bouton = @{ id = "jeu-opt:$k"; texte = 'Appliquer'; style = 'second' } }
            $l
        })
        $pages.jeu = @{ titre = [string]$g.Name; parent = 'jeux'; sous = "$(if ($g.Source) { $g.Source } else { 'Steam' }) · dernière partie : $(Format-LastPlayed $(if ($pl) { $pl.Last } else { '' }))"; icone = 'manette'; couleur = 'S'
            module = @{ image = (Get-JaquetteKit $g); legende = [string]$g.Name; titre = $(if ($enJeu) { 'En cours de jeu' } else { 'Prêt à jouer' })
                texte = $(if ($how.Kind -eq 'url' -and $g.Source) { "Lancé par $($g.Source) (connexion, mises à jour et anti-triche comme d'habitude)." } else { 'Lancé directement depuis son dossier.' })
                boutons = @(if (-not $enJeu) { @{ id = 'jeu-lancer'; texte = 'Lancer' } })
                resume = @(@{ nom = 'Temps de jeu'; valeur = $(if ($pl) { Format-PlayTime $pl.Seconds } else { '-' }) }, @{ nom = 'Parties'; valeur = $(if ($pl) { [string]$pl.Count } else { '0' }) }, @{ nom = 'FPS moyens'; valeur = $(if ($dern) { '{0:N0}' -f $dern.Avg } else { '-' }) }) }
            blocs = @(
                if ($hero) { @{ type = 'banniere'; image = (Get-ImageVue $hero); titre = [string]$g.Name; sous = $(if ($pl) { "$(Format-PlayTime $pl.Seconds) de jeu" } else { '' }) } }
                if ($dern) { @{ type = 'ligne'; id = "fps:$($dern.Id)"; icone = 'jauge'; couleur = 'vert'; texte = "Dernière partie mesurée : $('{0:N0}' -f $dern.Avg) FPS"; note = "1 % bas $('{0:N0}' -f $dern.Low1). Voir le détail"; valeur = '' } }
                @{ type = 'titre'; texte = 'Optimisation du jeu'; droite = $(if ($afaire.Count) { "$($afaire.Count) point$(if ($afaire.Count -gt 1) { 's' }) à améliorer" } else { 'Ce jeu est optimisé' }) }
                if ($afaire.Count) { @{ type = 'bouton'; id = 'jeu-optimiser'; texte = 'Tout optimiser' } }
            ) + $lignesOpt + @(
                @{ type = 'titre'; texte = 'Aussi' }
                @{ type = 'bouton'; id = 'jeu-dossier'; texte = 'Ouvrir le dossier'; style = 'second' }
                @{ type = 'bouton'; id = 'optimisation'; texte = 'Profils par jeu'; style = 'second' }
                $(if ($g.Custom) { @{ type = 'bouton'; id = 'jeu-retirer'; texte = 'Retirer de la liste'; style = 'second' } } else { @{ type = 'bouton'; id = 'jeu-desinstaller'; texte = 'Désinstaller'; style = 'danger' } })
            ) }
    }

    # ---- Overlay
    $mesure = Test-FpsMeasure
    $affiche = Test-FpsOverlay
    $coins = @{ hg = 'haut-gauche'; hd = 'haut-droite'; bg = 'bas-gauche'; bd = 'bas-droite' }
    $fpsApercu = if ($sessions.Count) { [int]$sessions[0].Avg } else { 144 }
    $pages.overlay = @{ titre = 'Overlay'; sous = 'Ton compteur de FPS par-dessus le jeu : un coup d''œil suffit, sans quitter la partie.'; icone = 'jauge'; couleur = 'vert'
        module = @{ icone = 'jauge'; couleur = $(if ($affiche -and $mesure) { 'vert' } else { 'doux' }); titre = $(if (-not $mesure) { 'Mesure coupée' } elseif ($affiche) { 'Prêt' } else { 'Compteur masqué' })
            texte = $(if (-not $mesure) { 'La mesure des FPS est coupée : le compteur ne peut pas s''afficher.' } elseif ($affiche) { 'Il apparaîtra dès que tu lances un jeu.' } else { 'Active le compteur pour le voir pendant tes parties.' })
            boutons = @(if (-not $mesure) { @{ id = 'fps-mesure'; valeur = 'oui'; texte = 'Activer la mesure' } }) }
        blocs = @(
            @{ type = 'apercu'; id = 'overlay-coin'; coin = $coins[(Get-FpsOverlayCorner)]; style = $(if ((Get-FpsOverlayStyle) -eq 'discret') { 'discret' } else { 'complet' }); fps = $fpsApercu; detail = $(if ($sessions.Count) { "1 % bas $('{0:N0}' -f $sessions[0].Low1) · moy. $('{0:N0}' -f $sessions[0].Avg)" }); legende = 'Aperçu. Clique sur un coin de l''écran pour y placer le compteur.' }
            @{ type = 'reglages'; rangs = @(
                @{ texte = 'Afficher le compteur pendant la partie'; note = 'Par-dessus le jeu, en fenêtré ou plein écran fenêtré.'; bascule = @{ id = 'overlay-on'; oui = [bool]$affiche } }
                @{ texte = 'Raccourci pour le montrer ou le cacher'; note = 'Jeu non reconnu ? Appuie dessus pendant la partie pour lancer ou arrêter la mesure et le compteur.'; touches = @('Ctrl', 'Maj', 'F') }
            ) }
            @{ type = 'titre'; texte = 'Style du compteur' }
            @{ type = 'cartes'; id = 'overlay-style'; valeur = $(if ((Get-FpsOverlayStyle) -eq 'discret') { 'discret' } else { 'complet' }); options = @(@{ id = 'complet'; titre = 'Complet'; texte = 'Le chiffre, le 1 % bas (les pires moments de la partie) et la moyenne.' }, @{ id = 'discret'; titre = 'Discret'; texte = 'Juste le chiffre, petit et semi-transparent.' }) }
            @{ type = 'titre'; texte = 'Position du compteur' }
            @{ type = 'choix'; id = 'overlay-coin'; valeur = $coins[(Get-FpsOverlayCorner)]; options = @($OverlayCorners.Keys | ForEach-Object { @{ id = $coins[$_]; texte = $OverlayCorners[$_] } }) }
            @{ type = 'note'; texte = 'Il reste collé au bord, même quand le chiffre change de taille.' }
        ) }

    # ---- Trafic
    $st = $script:Traffic
    $actif = [bool]($st -and $script:TrafficTimer -and $script:TrafficTimer.IsEnabled)
    $apps = @(if ($st) { $st.Apps.Values | Where-Object { (Get-TrafficDestCount $_) -or $_.Out -gt 0 } | Sort-Object @{ Expression = { $_.Out + $_.In } } -Descending | Select-Object -First 40 })
    $script:Kit.TraficApps = $apps
    $alertes = @(if ($st) { $st.Alerts | Where-Object { $_.Level -ne 'info' } })
    $script:Kit.TraficAlertes = $alertes
    $out = 0.0; $in = 0.0; foreach ($x in $apps) { $out += $x.Out; $in += $x.In }
    $maxO = [math]::Max(1.0, (@($apps | ForEach-Object { [double]$_.Out }) + 1 | Measure-Object -Maximum).Maximum)
    $mins = if ($st) { [int]((Get-Date) - $st.Started).TotalMinutes } else { 0 }
    $blocsT = @(
        if ($actif -or $apps.Count) { @{ type = 'chiffres'; elements = @(@{ titre = 'Envoyé'; valeur = (Format-Bytes $out) }, @{ titre = 'Reçu'; valeur = (Format-Bytes $in) }, @{ titre = 'Programmes'; valeur = "$($apps.Count)" }, @{ titre = 'À vérifier'; valeur = "$($alertes.Count)"; ton = $(if ($alertes.Count) { 'attention' } else { 'ok' }) }) } }
        if ($script:TrafficCounters -and $script:TrafficCounters.Visibility -eq 'Visible') { @{ type = 'note'; texte = [string]$script:TrafficCounters.Text } }
        if ($alertes.Count) { @{ type = 'titre'; texte = 'À vérifier' } }
    )
    for ($k = 0; $k -lt $alertes.Count; $k++) {
        $al = $alertes[$k]
        $blocsT += @{ type = 'ligne'; icone = 'alerte'; couleur = $(if ($al.Level -eq 'bad') { 'rouge' } else { 'orange' }); texte = "$($al.App.Title) : $(if ($al.Level -eq 'bad') { 'comportement suspect' } else { 'à vérifier' })"; note = "Pourquoi : $($al.Why -join ' ; ')."; etiquette = $(if ($al.Level -eq 'bad') { 'Important' } else { 'À voir' }); ton = $TonsStatut[[string]$al.Level] }
        $acts = @(Get-TrafficAlertActions $al.App)
        for ($j = 0; $j -lt $acts.Count; $j++) { $blocsT += @{ type = 'bouton'; id = "trafic-al:$k`:$j"; texte = [string]$acts[$j].Label; style = $(if ($j) { 'second' }) } }
    }
    if ($actif -and -not $alertes.Count) { $blocsT += @{ type = 'pastilles'; elements = @(@{ texte = 'Rien d''anormal pour l''instant' }) } }
    $blocsT += @{ type = 'reglages'; rangs = @(@{ texte = 'Identifier les serveurs sans nom'; note = 'Page Trafic et Lag en ligne : cherche à qui appartient chaque adresse inconnue (Valve, Riot, Amazon...) dans l''annuaire public rdap.org. Seule l''adresse du serveur est envoyée.'; bascule = @{ id = 'trafic-annuaire'; oui = [bool](Test-AnnuaireServeurs) } }) }
    $blocsT += @{ type = 'ligne'; icone = 'windows'; couleur = 'doux'; texte = 'Ce que Windows envoie à Microsoft'; note = 'Les identifiants de ton PC, les réglages qui en envoient plus que le minimum, et les envois vus en direct.'; bouton = @{ id = 'trafic-microsoft'; texte = 'Voir'; style = 'second' } }
    if ($apps.Count) {
        $blocsT += @{ type = 'titre'; texte = 'Programmes qui communiquent avec Internet'; droite = "depuis $(if ($mins -lt 1) { 'moins d''une minute' } else { "$mins min" })" }
        for ($k = 0; $k -lt $apps.Count; $k++) {
            $x = $apps[$k]
            $dests = (Get-TrafficDestCount $x)
            $sig = Get-AppSigLabel $x
            $blocsT += @{ type = 'ligne'; id = "trafic-app:$k"; image = $(if ($x.Path) { Get-IconeExeKit $x.Path }); icone = 'app'; couleur = 'doux'; texte = [string]$x.Title
                note = "$($sig[0]) · $dests destination$(if ($dests -gt 1) { 's' })"; barre = [int](100 * $x.Out / $maxO); valeur = "↑ $(Format-Bytes $x.Out)"; ton = $(if ($x.Rate -gt 50KB) { 'attention' } else { '' }) }
        }
    } elseif ($actif) { $blocsT += @{ type = 'texte'; texte = 'Aucun programme ne communique avec Internet pour le moment.' } }
    $blocsT += @{ type = 'note'; texte = 'Le contenu des échanges est chiffré (HTTPS) : Nevermind voit quel programme parle à qui et combien il envoie, jamais ce qu''il y a dedans.' }
    $pages.trafic = @{ titre = 'Ce qui sort de ton PC'; sous = 'Quels programmes communiquent avec Internet, avec qui, combien ils envoient, et ce qui est anormal.'; icone = 'courbe'; couleur = 'T'
        module = @{ icone = 'courbe'; couleur = $(if ($alertes.Count) { 'orange' } elseif ($actif) { 'vert' } else { 'T' }); titre = $(if (-not $actif) { 'Surveillance arrêtée' } elseif ($alertes.Count) { "$($alertes.Count) à vérifier" } else { 'Rien d''anormal' })
            texte = $(if ($actif) { "Surveillance en cours depuis $(if ($mins -lt 1) { 'moins d''une minute' } else { "$mins min" }). $($apps.Count) programme$(if ($apps.Count -gt 1) { 's ont' } else { ' a' }) parlé avec Internet." } else { 'Démarre la surveillance pour voir ce que tes programmes envoient sur Internet.' })
            boutons = @(@{ id = 'clic:BtnTraffic'; texte = [string]$ui.BtnTraffic.Content; style = $(if ($actif) { 'second' }) }) }
        blocs = $blocsT }
    $pages
}

function Register-ActionsJeux {
    Register-ActionVue 'jeux-cherche' { param($Valeur, $Ecran) $script:Kit.JeuxRecherche = [string]$Valeur }
    Register-ActionVue 'jeux-filtre' { param($Valeur, $Ecran) $script:Kit.JeuxFiltre = [string]$Valeur }
    Register-ActionVue 'jeux-organizer' { param($Valeur, $Ecran) & $script:KitShowPage $GamesIndex; Invoke-Safe { Show-OrgView $true } }
    Register-ActionVue 'jeu-module-lancer' { param($Valeur, $Ecran) $g = $script:Kit.JeuModule; if ($g) { Invoke-Safe { Start-LibraryGame $g } } }
    Register-ActionVue 'jeu-module-fiche' { param($Valeur, $Ecran) $g = $script:Kit.JeuModule; if ($g) { $script:Kit.JeuChoisi = $g; Open-EcranVue 'jeu' } }
    Register-ActionVue 'jeu-lancer' { param($Valeur, $Ecran) $g = $script:Kit.JeuChoisi; if ($g) { Invoke-Safe { Start-LibraryGame $g } } }
    Register-ActionVue 'jeu-optimiser' { param($Valeur, $Ecran) $g = $script:Kit.JeuChoisi; if ($g) { Invoke-Safe { Invoke-GameOptimize $g } } }
    Register-ActionVue 'jeu-dossier' { param($Valeur, $Ecran) $g = $script:Kit.JeuChoisi; if ($g) { $d = if ($g.Dir) { $g.Dir } else { Split-Path @($g.Exes)[0] -Parent }; Invoke-Safe { Open-Url $d } } }
    Register-ActionVue 'jeu-retirer' { param($Valeur, $Ecran) $g = $script:Kit.JeuChoisi; if ($g) { Invoke-Safe { Remove-CustomGame @($g.Exes)[0]; Update-LibraryView }; $script:Kit.JeuChoisi = $null; Open-EcranVue 'jeux' } }
    Register-ActionVue 'jeu-desinstaller' { param($Valeur, $Ecran) $g = $script:Kit.JeuChoisi; if ($g) { Invoke-Safe { Uninstall-LibraryGame $g } } }
    Register-ActionVue 'fps-mesure' { param($Valeur, $Ecran) $on = $(if ($Valeur -is [bool]) { $Valeur } else { $true }); Invoke-Safe { Set-FpsMeasure $on } }
    Register-ActionVue 'fps-aide' { param($Valeur, $Ecran) Invoke-Safe { Invoke-FpsHelp } }
    Register-ActionVue 'lag-mesure' { param($Valeur, $Ecran)
        Set-Setting 'LagMeasure' ([bool]$Valeur)
        if (-not $Valeur -and $script:LagSession -and -not $script:LagSession.Seconds) { Stop-LagSession }
        Update-GameWatch
    }
    Register-ActionVue 'lag-test' { param($Valeur, $Ecran) if (-not $script:LagSession) { Invoke-Safe { Start-LagSession 'Test rapide' 0 30 } } }
    Register-ActionVue 'overlay-on' { param($Valeur, $Ecran) Invoke-Safe { Set-FpsOverlayOn ([bool]$Valeur) } }
    Register-ActionVue 'overlay-style' { param($Valeur, $Ecran) Invoke-Safe { Set-FpsOverlayStyle ([string]$Valeur) } }
    Register-ActionVue 'overlay-coin' { param($Valeur, $Ecran)
        $c = @{ 'haut-gauche' = 'hg'; 'haut-droite' = 'hd'; 'bas-gauche' = 'bg'; 'bas-droite' = 'bd' }[[string]$Valeur]
        if ($c) { Invoke-Safe { Set-FpsOverlayCorner $c } }
    }
    Register-ActionVue 'lag-pays' { param($Valeur, $Ecran) Set-Setting 'LagPays' ([bool]$Valeur); if ($script:LagPaysSwitch) { $script:LagPaysSwitch.IsChecked = [bool]$Valeur } }
    Register-ActionVue 'trafic-annuaire' { param($Valeur, $Ecran) Set-Setting 'TrafficLookup' ([bool]$Valeur); if ($script:TrafficLookupSwitch) { $script:TrafficLookupSwitch.IsChecked = [bool]$Valeur } }
    Register-ActionVue 'trafic-microsoft' { param($Valeur, $Ecran) Invoke-Safe { Show-WindowsPrivacy } }
}

# Les gestes à numéro (un jeu, une partie, une mesure, un programme) : appelés par l'action « * »
function Invoke-ActionJeux([string]$Id, $Valeur) {
    if ($Id -match '^jeu:(\d+)$') { $g = @($script:Kit.JeuxMontres)[[int]$Matches[1]]; if ($g) { $script:Kit.JeuChoisi = $g; Update-EcranNevermind; Open-EcranVue 'jeu' }; return $true }
    if ($Id -match '^fps:([\w.:-]{1,40})$') { $x = $Matches[1]; Invoke-Safe { Show-FpsSession $x }; return $true }
    if ($Id -match '^lag:([\w.:-]{1,40})$') { $x = $Matches[1]; Invoke-Safe { Show-LagSession $x }; return $true }
    if ($Id -match '^jeu-opt:(\d+)$') {
        $it = @($script:Kit.JeuOptims)[[int]$Matches[1]]; $g = $script:Kit.JeuChoisi
        if ($it -and $g) {
            if ($it.Switch) { Invoke-Safe { Set-GameOptimization $g.Name $it.Id ([bool]$Valeur) } }
            elseif ($it.Fix -eq 'tweaks') { $ids = @($it.Ids); Invoke-Safe { Invoke-TweakFix $ids; $script:LibTweaks = $null } }
            $script:Kit.Profils = $null
        }
        return $true
    }
    if ($Id -match '^trafic-app:(\d+)$') { $a = @($script:Kit.TraficApps)[[int]$Matches[1]]; if ($a) { $k = $a.Key; Invoke-Safe { Show-TrafficApp $k } }; return $true }
    if ($Id -match '^trafic-al:(\d+):(\d+)$') {
        $al = @($script:Kit.TraficAlertes)[[int]$Matches[1]]
        if ($al) {
            $act = @(Get-TrafficAlertActions $al.App)[[int]$Matches[2]]
            if ($act) { if ($act.Confirm -and -not (Confirm-Action $act.Confirm)) { return $true }; Invoke-Safe { & $act.Script $act.Arg } }
        }
        return $true
    }
    $false
}
