# Nevermind : les pages de l'écran (kit) des petites icônes du haut (Rechercher, Paramètres, Aide) et la page Tests des
# composants (dans Ton PC).
# Paramètres reprend les réglages de la fenêtre d'avant (parametres.ps1), branchés sur les mêmes fonctions ; Tests lit
# les tuiles de composants de Nevermind (tests.ps1) et appuie sur leurs vrais boutons ; un test en cours s'affiche dans
# le panneau (ecran-panneau.ps1).
# Chargé par Nevermind.ps1 après ecran-reseau.ps1.

$IconesTests = @{ SSD = 'disque'; HDD = 'disque'; USB = 'disque'; CPU = 'puce'; RAM = 'memoire'; GPU = 'ecran'; NET = 'ondes'; HZ = 'ecran'; BAT = 'eclair' }

# Les tuiles de la page Tests de Nevermind, lues pour l'écran
function Get-TuilesTestsKit {
    $script:Kit.TestsBoutons = New-Object System.Collections.ArrayList
    $blocs = New-Object System.Collections.ArrayList
    $faits = 0; $total = 0
    foreach ($carte in @($ui.TestsPanel.Children)) {
        $sp = $carte.Child
        if (-not ($sp -is [System.Windows.Controls.StackPanel]) -or $sp.Children.Count -lt 4) { continue }
        $tete = $sp.Children[0]
        $tag = [string]$tete.Children[0].Child.Text
        $titres = @($tete.Children[1].Children | Where-Object { $_ -is [System.Windows.Controls.TextBlock] } | ForEach-Object { [string]$_.Text })
        $point = $tete.Children[2]
        $ton = Get-TonKit $point.Fill
        $total++; if ($ton) { $faits++ }
        $ligne = @{ type = 'ligne'; icone = $(if ($IconesTests[$tag]) { $IconesTests[$tag] } else { 'test' }); couleur = $(switch ($ton) { 'ok' { 'vert' } 'attention' { 'orange' } 'urgent' { 'rouge' } default { 'P' } })
            # (« SSD, 1 To  /  C: D: » : « SSD, 1 To, disque C: D: », sans point collé à la lettre du disque)
            texte = $titres[0]; note = (@($(if ($titres.Count -gt 1) { $titres[1] -replace '\s+/\s+', ', disque ' }), [string]$sp.Children[1].Text) | Where-Object { $_ }) -join ' · '
            etiquette = $(switch ($ton) { 'ok' { 'Bon' } 'attention' { 'À surveiller' } 'urgent' { 'Problème' } default { 'Pas encore testé' } }); ton = $ton }
        # Les boutons du test, dans la carte du composant
        $premier = $true
        $ligne.boutons = @(foreach ($b in @($sp.Children[3].Children)) {
            if (-not ($b -is [System.Windows.Controls.Primitives.ButtonBase]) -or $b.Visibility -ne 'Visible') { continue }
            [void]$script:Kit.TestsBoutons.Add($b)
            @{ id = "test-b:$($script:Kit.TestsBoutons.Count - 1)"; texte = (Get-ContenuWpf $b); style = $(if (-not $premier) { 'second' } else { $null }) }
            $premier = $false
        })
        [void]$blocs.Add($ligne)
        # Le résumé du dernier test (les petites cases chiffrées)
        $cases = @(foreach ($c in @($sp.Children[2].Children)) { if ($c -is [System.Windows.Controls.Border] -and $c.Child -is [System.Windows.Controls.StackPanel] -and $c.Child.Children.Count -ge 2) { @{ titre = [string]$c.Child.Children[1].Text; valeur = [string]$c.Child.Children[0].Text } } })
        if ($cases.Count) { [void]$blocs.Add(@{ type = 'chiffres'; elements = $cases }) }
    }
    @{ Blocs = @($blocs); Faits = $faits; Total = $total }
}

# Où se trouve un résultat de recherche : Get-SearchWhere (recherche.ps1) donne déjà les noms de la console
function Get-OuKit($E) { [string]$E.Where }

function Get-PagesDivers {
    $pages = [ordered]@{}

    # ---- Tests des composants
    if ($script:Vue.Ecran -eq 'tests' -and -not $ui.TestsPanel.Children.Count) { try { Build-TestsTab } catch { Write-Log "Écran, tests : $_" } }
    $tt = Get-TuilesTestsKit
    $pages.tests = @{ titre = 'Tests des composants'; parent = 'pc'; sous = 'Vérifie que chaque pièce de ton PC fonctionne bien et à la bonne vitesse.'; icone = 'test'; couleur = 'S'
        module = $(if ($tt.Total -and $tt.Faits) { @{ anneau = @{ valeur = $tt.Faits; sur = $tt.Total; petit = "/$($tt.Total)" }; titre = "$($tt.Faits) composant$(if ($tt.Faits -gt 1) { 's' }) testé$(if ($tt.Faits -gt 1) { 's' })"; texte = 'Ferme tes jeux avant de lancer un test : il prend toute la puissance du composant.' } }
            else { @{ icone = 'test'; couleur = 'S'; titre = 'Aucun test pour le moment'; texte = 'Choisis un composant : chaque test dure de quelques secondes à quelques minutes. Ferme tes jeux avant.' } })
        blocs = $(if ($tt.Blocs.Count) { @(@{ type = 'titre'; texte = 'Tes composants' }) + $tt.Blocs } else { @(@{ type = 'texte'; texte = 'Lecture de tes composants...' }) }) }

    # ---- Rechercher
    $res = @($script:Kit.Resultats | Where-Object { $_ })
    $q = [string]$script:Kit.Recherche
    $pages.recherche = @{ titre = 'Rechercher'; sous = 'Un réglage, un outil, un jeu.'; icone = 'loupe'; couleur = 'S'
        module = @{ icone = 'loupe'; couleur = 'S'; titre = 'Que cherches-tu ?'; texte = 'Tape un mot : Nevermind te montre où c''est et t''y emmène. Raccourci : Ctrl + K.' }
        blocs = @(
            @{ type = 'champ'; id = 'recherche'; recherche = $true; valeur = $q; indice = 'Par exemple : mode jeu' }
            if ($q) { @{ type = 'titre'; texte = $(if ($res.Count) { "$($res.Count) résultat$(if ($res.Count -gt 1) { 's' })" } else { 'Rien trouvé' }) } }
        ) + @(for ($k = 0; $k -lt $res.Count; $k++) { @{ type = 'ligne'; id = "res:$k"; icone = 'loupe'; couleur = 'S'; texte = [string]$res[$k].T; note = (Get-OuKit $res[$k]); valeur = 'Y aller' } }) + @(
            if ($q -and -not $res.Count) { @{ type = 'texte'; texte = 'Essaie avec un autre mot.' } }
            if (-not $q) { @{ type = 'raccourcis'; elements = @(@{ id = 'optimisation'; nom = 'Mode jeu'; icone = 'manette'; couleur = 'P' }, @{ id = 'connexion'; nom = 'Serveur DNS'; icone = 'ondes'; couleur = 'P' }, @{ id = 'overlay'; nom = 'Compteur de FPS'; icone = 'jauge'; couleur = 'vert' }, @{ id = 'sauvegarde'; nom = 'Tout restaurer'; icone = 'sauve'; couleur = 'S' }) } }
        ) }

    # ---- Paramètres
    # (géré par Mindset : le thème se choisit seulement dans Mindset, pour toute la suite ; ici, le thème actuel et le
    #  chemin vers Mindset, sans aucun choix ni import)
    $theme = if ($Managed) {
        @(@{ type = 'reglages'; rangs = @(
            @{ texte = 'Thème actuel'; note = $ManagedThemeText; valeur = (Get-CurrentThemeName) }
            @{ texte = 'Changer de thème'; note = 'Mindset s''ouvre sur Paramètres, Apparence. Le thème choisi là-bas va sur Nevermind et sur les autres applis.'; bouton = @{ id = 'theme-mindset'; texte = 'Ouvrir les réglages de Mindset'; style = 'second' } }
          ) })
    } else {
        @(@{ type = 'texte'; texte = 'Change les couleurs et l''ambiance de Nevermind. Nevermind redémarre pour appliquer le thème.' }
          @{ type = 'cartes'; id = 'theme'; valeur = (Get-CurrentThemeKey); options = @(Get-ThemeChoices | ForEach-Object { @{ id = [string]$_.Key; titre = [string]$_.Name; texte = [string]$_.Desc } }) })
    }
    $raccourci = $ui.BtnShortcut.Visibility -eq 'Visible'
    $demarrage = $ui.AutoStartRow.Visibility -eq 'Visible'
    $pages.parametres = @{ titre = 'Paramètres'; sous = 'Les réglages de Nevermind lui-même.'; icone = 'reglages'; couleur = 'doux'
        module = @{ icone = 'reglages'; couleur = 'P'; titre = "Nevermind $AppVersion"; texte = [string]$ui.UpdateStatus.Text; boutons = @(if ($ui.BtnCheckUpdate.Visibility -eq 'Visible') { @{ id = 'clic:BtnCheckUpdate'; texte = 'Vérifier les mises à jour'; style = 'second' } }) }
        onglets = @(
            @{ id = 'general'; nom = 'Général'; blocs = @(
                @{ type = 'reglages'; rangs = @(
                    @{ texte = 'Raccourci sur le bureau'; note = [string]$ui.ShortcutStatus.Text; bouton = $(if ($raccourci) { @{ id = 'clic:BtnShortcut'; texte = 'Créer le raccourci'; style = 'second' } }) }
                    if ($demarrage) { @{ texte = 'Lancer Nevermind au démarrage du PC'; note = 'Il reste réduit près de l''horloge, sans demande d''autorisation, pour le mode jeu et la mesure des FPS.'; bascule = @{ id = 'coche:ChkAutoStart'; oui = [bool]$ui.ChkAutoStart.IsChecked } } }
                    @{ texte = 'Visite guidée'; note = 'Revoir en 3 étapes comment marche Nevermind.'; bouton = @{ id = 'visite'; texte = 'Revoir la visite'; style = 'second' } }
                ) }
            ) }
            @{ id = 'jeux'; nom = 'Jeux'; blocs = @(
                @{ type = 'reglages'; rangs = @(
                    @{ texte = 'Mesurer mes FPS quand je joue'; note = 'Moyenne, chutes et courbe de chaque partie, gardées dans Jeux, Perf. In Game.'; bascule = @{ id = 'fps-mesure'; oui = [bool](Test-FpsMeasure) } }
                    @{ texte = 'Compteur de FPS à l''écran'; note = 'Affichage, style et position du compteur par-dessus le jeu.'; lien = @{ id = 'overlay'; texte = 'Ouvrir Overlay' } }
                    @{ texte = 'Mesurer ma connexion quand je joue'; note = 'Box, Internet et serveur du jeu pendant la partie, pour savoir d''où vient le lag.'; bascule = @{ id = 'lag-mesure'; oui = [bool](Test-LagMeasure) } }
                    @{ texte = 'Mode jeu : fermer des applis pendant que je joue'; note = 'Les applis choisies (Optimisation gaming, Mode jeu) sont fermées puis relancées après la partie.'; bascule = @{ id = 'modejeu'; oui = [bool](Get-Setting 'GameMode' $false) } }
                    @{ texte = 'Organizer Dofus'; note = 'Pour passer d''un perso à l''autre quand tu joues à plusieurs comptes. Coupé : le bouton disparaît de la page Jeux.'; bascule = @{ id = 'org-affiche'; oui = [bool](Test-OrgVisible) } }
                    @{ texte = 'Jaquettes depuis Internet'; note = 'Cherche les images des jeux qui n''en ont pas sur la boutique Steam puis Wikipédia (seul le nom du jeu est envoyé).'; bascule = @{ id = 'coche:ChkLibCovers'; oui = [bool](Test-CoversOnline) } }
                ) }
            ) }
            @{ id = 'reseau'; nom = 'Réseau et vie privée'; blocs = @(
                @{ type = 'reglages'; rangs = @(
                    @{ texte = 'Me prévenir quand un nouvel appareil se connecte'; note = 'Vérifié toutes les 10 minutes sur ton réseau, tant que Nevermind est ouvert.'; bascule = @{ id = 'coche:ChkNetWatch'; oui = [bool](Get-Setting 'NetWatch' $false) } }
                    @{ texte = 'Identifier les serveurs sans nom'; note = 'Page Trafic et Lag en ligne : cherche à qui appartient une adresse inconnue (Valve, Riot, Amazon...) dans l''annuaire public rdap.org. Seule l''adresse du serveur est envoyée.'; bascule = @{ id = 'trafic-annuaire'; oui = [bool](Test-AnnuaireServeurs) } }
                    @{ texte = 'Pays du serveur dans Lag en ligne'; note = 'Lag en ligne : cherche dans quel pays se trouve le serveur du jeu (ipinfo.io). Seule l''adresse du serveur est envoyée.'; bascule = @{ id = 'lag-pays'; oui = [bool](Test-PaysServeur) } }
                    @{ texte = 'Vérifier le pilote NVIDIA au lancement'; note = 'Si le PC a une carte NVIDIA : demande à NVIDIA (geforce.com) le numéro du dernier pilote. Rien sur ton PC n''est envoyé. Coupé : Nevermind regarde seulement l''âge de ton pilote.'; bascule = @{ id = 'pilote-nvidia'; oui = [bool](Test-PiloteNvidia) } }
                    @{ texte = 'Ce que Windows envoie à Microsoft'; note = 'Identifiants, réglages qui envoient plus que le minimum et envois vus en direct.'; bouton = @{ id = 'trafic-microsoft'; texte = 'Voir'; style = 'second' } }
                ) }
                @{ type = 'note'; texte = 'Nevermind ne déchiffre jamais tes connexions et n''envoie rien sur toi : il regarde seulement qui parle à qui depuis ton PC.' }
            ) }
            @{ id = 'theme'; nom = 'Thème'; blocs = $theme }
            @{ id = 'maj'; nom = 'Mises à jour'; blocs = @(
                @{ type = 'reglages'; rangs = @(
                    @{ texte = 'Version'; note = [string]$ui.UpdateStatus.Text; valeur = "Nevermind $AppVersion" }   # (« Vérifier les mises à jour » : le bouton de la carte)
                    @{ texte = 'Recevoir les versions bêta'; note = 'Les nouveautés en avant-première ; elles peuvent contenir des erreurs.'; bascule = @{ id = 'coche:ChkBeta'; oui = [bool]$ui.ChkBeta.IsChecked } }
                    if (-not $Managed) { @{ texte = 'Chercher une nouvelle version au lancement'; note = 'Demande à GitHub (api.github.com) le numéro de la dernière version. Coupé : seulement quand tu cliques sur « Vérifier les mises à jour ».'; bascule = @{ id = 'maj-auto'; oui = [bool](Test-MajAuto) } } }
                    @{ texte = 'Lire les nouveautés sur Internet'; note = 'Après une mise à jour : la liste des nouveautés est lue sur GitHub (api.github.com). Coupé : Nevermind dit seulement qu''il a été mis à jour.'; bascule = @{ id = 'nouveautes-en-ligne'; oui = [bool](Test-NouveautesEnLigne) } }
                ) }
            ) }
        ) }

    # ---- Aide
    $pages.aide = @{ titre = 'Aide'; sous = 'Les réponses aux questions qu''on se pose souvent.'; icone = 'aide'; couleur = 'T'
        module = @{ icone = 'aide'; couleur = 'T'; titre = 'Bienvenue dans Nevermind'; texte = 'Nevermind analyse ton PC et le règle pour que tes jeux tournent au mieux.'; boutons = @(@{ id = 'visite'; texte = 'Revoir la visite' }) }
        blocs = @(
            @{ type = 'faq'; elements = @(
                @{ question = 'Est-ce que Nevermind peut abîmer mon PC ?'; icone = 'bouclier'; couleur = 'T'; reponse = 'Non. Rien n''est modifié sans ton accord, et chaque changement est sauvegardé avant d''être fait : tu peux l''annuler à tout moment depuis la page Sauvegarde (dans Protection). Nevermind ne touche jamais à tes documents.' }
                @{ question = 'Comment revenir à l''accueil ?'; icone = 'maison'; couleur = 'P'; reponse = 'Clique sur le logo ou le nom Nevermind en haut à gauche, ou appuie sur Échap (ou le bouton Retour en bas à droite) jusqu''à revenir à l''accueil.' }
                @{ question = 'Que se passe-t-il quand je clique sur la croix ?'; icone = 'horloge'; couleur = 'P'; reponse = 'Nevermind te demande si tu veux quitter ou réduire. « Quitter » le ferme complètement. « Réduire » le garde ouvert près de l''horloge : il continue en arrière-plan pour le mode jeu, le compteur de FPS et la mesure des FPS.' }
                @{ question = 'Mes FPS ne sont pas mesurés dans un jeu'; icone = 'manette'; couleur = 'S'; reponse = 'Vérifie dans Paramètres, Jeux, que « Mesurer mes FPS quand je joue » est activé. Si Nevermind ne reconnaît pas le jeu, appuie sur Ctrl + Maj + F pendant la partie, ou ajoute le jeu dans la page Jeux.' }
                @{ question = 'À quoi sert la page Réseau ?'; icone = 'radar'; couleur = 'S'; reponse = 'Elle trouve les appareils de ta maison (téléphones, consoles, TV, objets connectés) et vérifie leur sécurité. Un nouvel appareil inconnu est signalé.' }
            ) }
            @{ type = 'titre'; texte = 'Toujours bloqué ?' }
            @{ type = 'ligne'; icone = 'rapport'; couleur = 'P'; texte = 'Signaler un problème'; note = 'Crée un fichier sur ton bureau avec le journal de Nevermind et la configuration de ton PC, sans tes fichiers ni ton nom. Envoie-le à la personne qui t''a donné Nevermind.'; bouton = @{ id = 'clic:BtnReportProblem'; texte = 'Créer le fichier'; style = 'second' } }
            @{ type = 'raccourcis'; elements = @(@{ id = 'recherche'; nom = 'Rechercher un réglage'; icone = 'loupe'; couleur = 'S' }) }
        ) }
    $pages
}

function Register-ActionsDivers {
    Register-ActionVue 'visite' { param($Valeur, $Ecran) Invoke-Safe { Show-Tour } }
    Register-ActionVue 'theme-mindset' { param($Valeur, $Ecran) Invoke-Safe { Open-MindsetSettings 'apparence' } }
    Register-ActionVue 'pilote-nvidia' { param($Valeur, $Ecran) Set-Setting 'PiloteNvidia' ([bool]$Valeur) }
    Register-ActionVue 'maj-auto' { param($Valeur, $Ecran) Set-Setting 'MajAuto' ([bool]$Valeur) }
    Register-ActionVue 'nouveautes-en-ligne' { param($Valeur, $Ecran) Set-Setting 'NouveautesEnLigne' ([bool]$Valeur) }
    # Choix d'un thème : copie autonome seulement. Géré par Mindset, refusé sans effet (le thème se choisit dans Mindset),
    # même si l'écran l'envoie quand même.
    Register-ActionVue 'theme' { param($Valeur, $Ecran)
        $id = [string]$Valeur
        if ($Managed) { Write-Log "Thème « $id » demandé depuis l'écran : refusé (copie gérée, le thème se choisit dans Mindset)"; return }
        if ($id -eq (Get-CurrentThemeKey)) { return }
        if (Confirm-Action 'Changer de thème ? Nevermind va redémarrer pour l''appliquer.') { Invoke-Safe { Select-AppTheme $id } }
    }
}
function Invoke-ActionDivers([string]$Id, $Valeur) {
    if ($Id -match '^test-b:(\d+)$') {
        $b = @($script:Kit.TestsBoutons)[[int]$Matches[1]]
        if ($b -and $b.IsEnabled) { $b.RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Primitives.ButtonBase]::ClickEvent))) }
        return $true
    }
    $false
}
