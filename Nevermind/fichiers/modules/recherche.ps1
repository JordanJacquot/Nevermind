# Nevermind : recherche des réglages (Ctrl+K), pour la page « Rechercher » de la console (ecran-divers.ps1).
# Find-Settings trouve les réglages au fil de la frappe ; un résultat ouvre la bonne page de la console (Open-SearchEntry).
# Chargé par Nevermind.ps1 après les pages (il connaît leurs numéros) et avant evenements.ps1.

# Réglages et actions trouvables. P : page (numéro, 'jeux', 'reseau', 'trafic' ou 'overlay'), S : sous-onglet d'Optimisation gaming,
# A : texte affiché sur la page d'avant (repère du réglage), K : autres mots qui doivent le trouver,
# Do : action après l'arrivée (ouvrir une fenêtre). Les accents et majuscules ne comptent pas.
$SearchEntries = @(
    # Optimisation gaming
    @{ T = 'Réglages Windows pour les jeux'; P = 1; S = 'tweaks'; A = 'Cocher le recommandé'; K = 'tweaks optimiser optimisation fps appliquer recommande gagner performances' },
    @{ T = 'Point de restauration avant les réglages'; P = 1; S = 'tweaks'; A = 'Point de restauration avant (recommandé)'; K = 'securite sauvegarde avant' },
    @{ T = 'Mesurer mes FPS'; P = 'jeux'; Perf = 'fps'; A = 'Mesurer mes FPS quand je joue'; K = 'fps images par seconde presentmon mesure partie framerate' },
    @{ T = 'Mes parties (historique des FPS)'; P = 'jeux'; Perf = 'fps'; A = 'Mesurer mes FPS quand je joue'; K = 'historique parties fps moyenne bas avant apres comparaison' },
    @{ T = 'Mes FPS ne sont pas normaux'; P = 'jeux'; Perf = 'fps'; A = 'Mes FPS ne sont pas normaux : trouver pourquoi'; K = 'diagnostic fps rame saccades chutes freeze lent' },
    @{ T = 'Raccourci Ctrl+Maj+F (mesure manuelle)'; P = 'overlay'; A = 'Raccourci Ctrl + Maj + F'; K = 'raccourci clavier hotkey touche mesure manuelle autre jeu' },
    @{ T = 'Compteur de FPS à l''écran (overlay)'; P = 'overlay'; A = 'Afficher le compteur pendant la partie'; K = 'overlay compteur afficher ecran osd fps' },
    @{ T = 'Style du compteur : complet ou discret'; P = 'overlay'; A = 'Style du compteur'; K = 'overlay discret petit transparent semi transparence taille apparence' },
    @{ T = 'Position du compteur'; P = 'overlay'; A = 'Position du compteur'; K = 'overlay coin haut bas gauche droite emplacement deplacer' },
    @{ T = 'Lag en ligne'; P = 'jeux'; Perf = 'lag'; A = 'Mesurer ma connexion quand je joue'; K = 'lag ping latence serveur jeu connexion partie online' },
    @{ T = 'Tester ma connexion pour le jeu (30 s)'; P = 'jeux'; Perf = 'lag'; A = 'Tester ma connexion maintenant (30 s)'; K = 'test lag ping latence' },
    @{ T = 'Mode jeu : fermer des applis pendant que je joue'; P = 1; S = 'mode'; A = 'Fermer des applis pendant que je joue'; K = 'mode jeu fermer applis discord chrome navigateur' },
    @{ T = 'Profils par jeu'; P = 1; S = 'profiles'; K = 'priorite haute carte graphique puissante gpu profil jeu' },
    @{ T = 'Ajouter un jeu (non reconnu)'; P = 1; S = 'profiles'; A = 'Ajouter un jeu'; K = 'ajouter jeu manquant detecte reconnu ubisoft ea gog battlenet riot xbox autre launcher itch emulateur' },
    @{ T = 'Mes jeux reconnus'; P = 1; S = 'profiles'; A = 'Ajouter un jeu'; K = 'liste jeux installes launchers steam epic ubisoft ea gog battlenet riot xbox' },
    # Jeux
    @{ T = 'Mes jeux (bibliothèque)'; P = 'jeux'; A = 'Mes jeux'; K = 'jeux bibliotheque library steam liste installes' },
    @{ T = 'Lancer un jeu'; P = 'jeux'; A = 'Mes jeux'; K = 'lancer jouer demarrer jeu play' },
    @{ T = 'Optimiser un jeu'; P = 'jeux'; A = 'Mes jeux'; K = 'optimiser jeu tout optimiser priorite' },
    @{ T = 'Restes de jeux désinstallés (place à récupérer)'; P = 'jeux'; A = 'Mes jeux'; K = 'place disque espace jeux desinstalles restes dossiers steam liberer'; Do = { if (@(Get-BigLeftovers).Count) { Show-Leftovers } } },
    @{ T = 'Désinstaller un jeu'; P = 'jeux'; A = 'Mes jeux'; K = 'desinstaller supprimer enlever jeu' },
    @{ T = 'Temps de jeu'; P = 'jeux'; A = 'Mes jeux'; K = 'temps heures joue derniere partie' },
    @{ T = 'Organizer Dofus (passer d''un perso à l''autre)'; P = 'jeux'; Org = $true; A = 'Mes personnages'; K = 'organizer organiseur dofus multicompte multi compte perso personnages fenetres switch changer initiative ordre ankama' },
    @{ T = 'Raccourcis de l''organizer Dofus'; P = 'jeux'; Org = $true; A = 'Raccourcis'; K = 'organizer dofus raccourci touche clavier perso suivant precedent f1' },
    @{ T = 'Barre flottante de l''organizer Dofus'; P = 'jeux'; Org = $true; A = 'Barre flottante'; K = 'organizer dofus barre flottante boutons persos par dessus jeu' },
    # Tableau de bord
    @{ T = 'Score et analyse du PC'; P = 0; A = 'Relancer l''analyse'; K = 'score analyse sante composants note' },
    @{ T = 'Tout corriger'; P = 0; A = 'Tout corriger'; K = 'corriger reparer ameliorer score' },
    @{ T = 'Onduleur'; P = 0; A = 'Onduleur'; K = 'ups onduleur batterie secours coupure courant' },
    # Démarrage, Connexion, Nettoyage
    @{ T = 'Programmes au démarrage'; P = 2; A = 'Programmes au démarrage'; K = 'demarrage windows lancement boot startup lent' },
    @{ T = 'Désactiver les programmes conseillés au démarrage'; P = 2; A = 'Désactiver ce qui est conseillé'; K = 'demarrage accelerer boot' },
    @{ T = 'Tester ma connexion (ping)'; P = 3; A = 'Tester ma connexion'; K = 'ping gigue jitter connexion internet stable' },
    @{ T = 'Serveur DNS'; P = 3; A = 'Serveur DNS'; K = 'dns cloudflare google quad9' },
    @{ T = 'Vider le cache DNS'; P = 3; A = 'Vider le cache DNS'; K = 'dns cache flush' },
    @{ T = 'Redémarrer la carte réseau (vitesse de la carte)'; P = 3; A = 'Ta connexion'; K = 'carte reseau ethernet cable vitesse lente bridee 100 mbps 1 gbps gigabit relancer redemarrer veille' },
    @{ T = 'Nettoyage du disque'; P = 4; A = 'Analyser'; K = 'nettoyer nettoyage fichiers temporaires place disque espace plein' },
    # Tests
    @{ T = 'Tester la vitesse du disque'; P = 5; A = 'Tester la vitesse'; K = 'ssd hdd nvme disque vitesse lecture ecriture' },
    @{ T = 'Santé du disque'; P = 5; A = 'Santé'; K = 'ssd hdd disque usure temperature smart' },
    @{ T = 'Test du processeur'; P = 5; A = 'Test rapide (30 s)'; K = 'cpu processeur stabilite puissance' },
    @{ T = 'Test de la mémoire'; P = 5; A = 'Tester la mémoire'; K = 'ram memoire barrette erreurs' },
    @{ T = 'Carte graphique en direct'; P = 5; A = 'Surveiller en direct'; K = 'gpu temperature carte graphique nvidia amd' },
    @{ T = 'Débit Internet'; P = 5; A = 'Tester ma connexion'; K = 'debit vitesse internet speedtest telechargement' },
    @{ T = 'Lag quand la connexion est chargée'; P = 5; A = 'Lag en charge'; K = 'bufferbloat lag charge telechargement' },
    @{ T = 'Pixels morts de l''écran'; P = 5; A = 'Écrans'; K = 'pixels morts ecran moniteur couleurs' },
    @{ T = 'Batterie du portable'; P = 5; A = 'Batterie'; K = 'batterie portable autonomie usure' },
    # Sécurité
    @{ T = 'Analyse antivirus rapide'; P = 6; A = 'Analyse rapide'; K = 'virus defender antivirus scan malware' },
    @{ T = 'Analyse antivirus complète'; P = 6; A = 'Analyse complète'; K = 'virus defender antivirus scan complet' },
    @{ T = 'Analyser un dossier'; P = 6; A = 'Analyser un dossier'; K = 'virus dossier fichier scan' },
    @{ T = 'Mettre à jour l''antivirus'; P = 6; A = 'Mettre à jour la base'; K = 'defender definitions signatures antivirus' },
    @{ T = 'Niveau de protection'; P = 6; A = 'Points à vérifier'; K = 'protection securite score pare feu' },
    @{ T = 'Approuver un point à vérifier (c''est moi)'; P = 6; A = 'Points à vérifier'; K = 'approuver ok confiance faux positif ignorer autoriser tache planifiee suspecte' },
    # Sauvegarde
    @{ T = 'Tout restaurer'; P = 7; A = 'Annuler les changements de Nevermind'; K = 'tout annuler restaurer revenir arriere defaire' },
    @{ T = 'Historique des changements'; P = 7; A = 'Historique des changements'; K = 'historique annuler changement' },
    @{ T = 'Point de restauration Windows'; P = 7; A = 'Point de restauration Windows'; K = 'restauration systeme sauvegarde' },
    @{ T = 'Rapport du PC'; P = 7; A = 'Rapport de ton PC'; K = 'rapport export html partager configuration' },
    @{ T = 'Mises à jour de Nevermind'; P = 'parametres'; Tab = 'maj'; A = 'Mises à jour'; K = 'version update maj nouvelle' },
    @{ T = 'Versions bêta'; P = 'parametres'; Tab = 'maj'; A = 'Mises à jour'; K = 'beta preversion avant premiere' },
    @{ T = 'Raccourci sur le bureau'; P = 'parametres'; Tab = 'general'; A = 'Raccourci et démarrage'; K = 'raccourci bureau icone' },
    @{ T = 'Lancer Nevermind au démarrage du PC'; P = 'parametres'; Tab = 'general'; A = 'Raccourci et démarrage'; K = 'demarrage automatique boot windows lancement auto allumage' },
    # Réseau
    @{ T = 'Scanner le réseau'; P = 'reseau'; A = 'Scanner le réseau'; K = 'appareils wifi box scan connectes' },
    @{ T = 'Carte du réseau'; P = 'reseau'; A = 'Ton réseau'; K = 'carte constellation appareils map'; Do = { if (@($script:NetList).Count) { Show-NetMap } } },
    @{ T = 'Audit de sécurité du réseau'; P = 'reseau'; A = 'Audit de sécurité'; K = 'wifi wps upnp securite box audit' },
    @{ T = 'Prévenir si un nouvel appareil se connecte'; P = 'reseau'; A = 'Appareils connectés'; K = 'alerte nouvel appareil intrus surveillance notification' },
    # Trafic
    @{ T = 'Ce qui sort de ton PC'; P = 'trafic'; A = 'Ce qui sort de ton PC'; K = 'trafic internet donnees envoyees programmes connexions espion' },
    @{ T = 'Identifier les serveurs sans nom'; P = 'trafic'; A = 'Identifier les serveurs sans nom'; K = 'rdap annuaire serveur ip proprietaire' },
    @{ T = 'Pays du serveur du jeu (Lag en ligne)'; P = 'parametres'; Tab = 'reseau'; A = 'Pays du serveur dans Lag en ligne'; K = 'pays localisation serveur jeu lag ping ou se trouve geolocalisation ipinfo' },
    @{ T = 'Vérifier le pilote NVIDIA au lancement'; P = 'parametres'; Tab = 'reseau'; A = 'Vérifier le pilote NVIDIA au lancement'; K = 'pilote nvidia geforce dernier verification internet vie privee' },
    @{ T = 'Thème (couleurs de Nevermind)'; P = 'parametres'; Tab = 'theme'; K = 'theme couleur couleurs apparence design da sombre terminal arcade rubis crepuscule neon skin' },
    @{ T = 'Paramètres de Nevermind'; P = 'parametres'; Tab = 'general'; K = 'parametres reglages options configuration roue engrenage settings' },
    @{ T = 'Signaler un problème'; P = 'parametres'; Tab = 'aide'; A = 'Signaler un problème'; K = 'bug souci erreur rapport envoyer probleme aide' },
    @{ T = 'Revoir la visite guidée'; P = 'parametres'; Tab = 'aide'; A = 'Visite guidée'; K = 'visite tutoriel aide decouvrir' },
    @{ T = 'Ce que Windows envoie à Microsoft'; P = 'trafic'; A = 'Ce que Windows envoie à Microsoft'; K = 'telemetrie microsoft vie privee identifiant confidentialite diagnostic'; Do = { Show-WindowsPrivacy } },
    @{ T = 'Bloquer Internet à un programme'; P = 'trafic'; A = 'Ce qui sort de ton PC'; K = 'pare feu bloquer internet firewall programme' }
)
# Géré par Mindset : raccourci du bureau et versions bêta passent par lui (cachés dans Paramètres)
if ($Managed) { $SearchEntries = @($SearchEntries | Where-Object { $_.T -notin 'Raccourci sur le bureau', 'Versions bêta' }) }
# Géré par Mindset : le thème se choisit dans Mindset ; l'entrée mène toujours à Paramètres, Thème (le thème actuel et
# le bouton qui ouvre Mindset)
if ($Managed) { foreach ($e in $SearchEntries) { if ($e.Tab -eq 'theme') { $e.T = 'Thème (se choisit dans Mindset)'; $e.A = $ManagedThemeText; $e.K += ' mindset suite' } } }
# Suggestions quand la barre est vide
$SearchStarters = @('Compteur de FPS à l''écran (overlay)', 'Mes FPS ne sont pas normaux', 'Lag en ligne', 'Nettoyage du disque', 'Lancer Nevermind au démarrage du PC', 'Ce que Windows envoie à Microsoft')

# Minuscules, sans accents ni ponctuation
function ConvertTo-SearchText([string]$Text) {
    $d = $Text.ToLowerInvariant().Normalize([Text.NormalizationForm]::FormD)
    $sb = New-Object System.Text.StringBuilder
    foreach ($ch in $d.ToCharArray()) {
        if ([Globalization.CharUnicodeInfo]::GetUnicodeCategory($ch) -ne 'NonSpacingMark') { [void]$sb.Append($(if ([char]::IsLetterOrDigit($ch)) { $ch } else { ' ' })) }
    }
    ($sb.ToString() -replace '\s+', ' ').Trim()
}

# Une faute de frappe (lettre en trop, en moins, changée ou inversée)
function Test-OneTypo([string]$A, [string]$B) {
    if ($A -eq $B) { return $true }
    if ([math]::Abs($A.Length - $B.Length) -gt 1) { return $false }
    $i = 0
    while ($i -lt $A.Length -and $i -lt $B.Length -and $A[$i] -eq $B[$i]) { $i++ }
    $ta = $A.Substring($i); $tb = $B.Substring($i)
    if ($ta.Length -eq $tb.Length) {
        if ($ta.Substring(1) -eq $tb.Substring(1)) { return $true }                        # lettre changée
        return ($ta.Length -ge 2 -and $ta[0] -eq $tb[1] -and $ta[1] -eq $tb[0] -and $ta.Substring(2) -eq $tb.Substring(2))   # lettres inversées
    }
    if ($ta.Length -gt $tb.Length) { return $ta.Substring(1) -eq $tb }                    # lettre en trop
    $ta -eq $tb.Substring(1)                                                               # lettre en moins
}
# Liste complète : réglages fixes, plus les réglages Windows et les jeux de cette machine
function Get-SearchIndex {
    $all = New-Object System.Collections.ArrayList
    $org = Test-OrgVisible
    foreach ($e in $SearchEntries) { if ($org -or -not $e.Org) { [void]$all.Add($e) } }
    foreach ($r in @($script:TweakRows)) {
        $tw = $r.Tweak
        if ($tw -and $tw.Titre) { [void]$all.Add(@{ T = [string]$tw.Titre; P = 1; S = 'tweaks'; A = [string]$tw.Titre; K = 'reglage windows optimisation' }) }
    }
    if ($script:Games) {
        foreach ($g in @($script:Games | Where-Object { $_.Name } | Select-Object -First 80)) {
            [void]$all.Add(@{ T = [string]$g.Name; P = 'jeux'; Game = [string]$g.Name; K = "jeu lancer jouer $($g.Source)" })
            [void]$all.Add(@{ T = "Profil de $($g.Name)"; P = 1; S = 'profiles'; A = [string]$g.Name; K = 'jeu profil priorite' })
        }
    }
    foreach ($e in $all) {
        if (-not $e.Where) { $e.Where = Get-SearchWhere $e }
        if (-not $e.Words) {
            $e.TitleN = ConvertTo-SearchText $e.T
            $e.TitleWords = @($e.TitleN -split ' ')
            $e.Words = @((ConvertTo-SearchText "$($e.T) $($e.K) $($e.Where)") -split ' ' | Select-Object -Unique)
        }
    }
    $all
}

function Get-SearchWhere($E) {
    $sub = if ($E.S) { ' › ' + (@($GamingSubPages | Where-Object { $_.Id -eq $E.S })[0]).Label }
           elseif ($E.Perf) { ' › Perf. In Game › ' + (@($PerfSubPages | Where-Object { $_.Id -eq $E.Perf })[0]).Label } else { '' }
    switch ($E.P) {
        'reseau' { 'Réseau' }
        'jeux' { 'Jeux' }
        'trafic' { 'Trafic' }
        'overlay' { 'Overlay' }
        'parametres' { if ($E.Tab -eq 'aide') { 'Aide' } else { 'Paramètres › ' + (@($SettingsTabs | Where-Object { $_.Id -eq $E.Tab })[0]).Label } }
        default {
            # Rangement de la console : Ton PC, Performances, Protection (table locale : $KitPagesIndex n'existe pas encore au chargement)
            $i = [int]$E.P
            $famille = @{ 1 = 'Performances'; 2 = 'Performances'; 4 = 'Performances'; 3 = 'Protection'; 6 = 'Protection'; 7 = 'Protection' }
            if ($i -eq 0) { "Ton PC$sub" }
            elseif ($i -eq 5) { "Ton PC › Tests des composants$sub" }
            else { "$($famille[$i]) › $($PageNames[$i])$sub" }
        }
    }
}

# Réglages qui correspondent : chaque mot tapé doit se retrouver (début de mot, dedans, ou à une faute près)
function Find-Settings([string]$Query, [int]$Max = 8) {
    $q = @((ConvertTo-SearchText $Query) -split ' ' | Where-Object { $_ })
    if (-not $q.Count) { return @() }
    $qn = $q -join ' '
    $hits = foreach ($e in (Get-SearchIndex)) {
        $score = 0.0; $ok = $true
        foreach ($w in $q) {
            $best = 0.0
            foreach ($x in $e.Words) {
                $s = if ($x.StartsWith($w)) { 3.0 } elseif ($w.Length -ge 3 -and $x.Contains($w)) { 1.5 } elseif ($w.Length -ge 4 -and ((Test-OneTypo $w $x) -or ($x.Length -gt $w.Length -and (Test-OneTypo $w $x.Substring(0, $w.Length))))) { 1.0 } else { 0.0 }
                if ($s -gt 0 -and $e.TitleWords -contains $x) { $s += 1.5 }
                if ($s -gt $best) { $best = $s }
            }
            if ($best -eq 0) { $ok = $false; break }
            $score += $best
        }
        if (-not $ok) { continue }
        if ($e.TitleN.StartsWith($qn)) { $score += 5 } elseif ($e.TitleN.Contains($qn)) { $score += 2 }
        @{ E = $e; Score = $score }
    }
    @($hits | Sort-Object @{ Expression = { $_.Score }; Descending = $true }, @{ Expression = { $_.E.T.Length } } | Select-Object -First $Max | ForEach-Object { $_.E })
}

# ---------------------------------------------------------------------------
# Aller au réglage : la bonne page de la console (et le bon sous-onglet de la page d'avant, cachée)
# ---------------------------------------------------------------------------
function Open-SearchEntry($E) {
    if (-not $E) { return }
    if ($ui.TestOverlay.Visibility -eq 'Visible') { Hide-TestPanel }
    if ($ui.NetMapOverlay.Visibility -eq 'Visible') { Hide-NetMap }
    if ($ui.Overlay.Visibility -eq 'Visible' -and $script:SheetMode -ne 'display') { Close-Sheet }
    if ($E.P -ne 'parametres') { Hide-Settings }
    if ($E.P -eq 'parametres') {
        Show-Settings $E.Tab
        Set-Status "$($E.T) : $($E.Where)"
        return
    }
    $page = switch ($E.P) { 'reseau' { $NetIndex } 'trafic' { $TrafficIndex } 'overlay' { $OverlayIndex } 'jeux' { $GamesIndex } default { [int]$E.P } }
    Show-Page $page
    if ($E.S) { Build-GamingTabs; Set-GamingSubPage $E.S }
    if ($E.Perf) { Show-PerfPage $E.Perf }
    elseif ($page -eq $GamesIndex) { Set-GamesView 'lib'; Show-OrgView ([bool]$E.Org) }
    if ($E.Game) { $ui.LibSearch.Text = ''; $script:LibFilter = 'Tous'; Update-LibraryView; Set-LibrarySelection $E.Game }
    Set-Status "$($E.T) : $($E.Where)"
    if (-not $E.Do) { return }
    # L'action qui suit l'arrivée (ouvrir un panneau), une fois la page affichée. (Pas de GetNewClosure : le bloc
    # perdrait les fonctions de Nevermind quand il tourne dans Nevermind.exe ; la demande attend dans $script:SearchPending.)
    $script:SearchPending = $E.Do
    $null = $Window.Dispatcher.BeginInvoke([System.Windows.Threading.DispatcherPriority]::Loaded, [Action]{
        $d = $script:SearchPending
        $script:SearchPending = $null
        if (-not $d) { return }
        try { Invoke-Safe $d } catch { Write-Log "Recherche: $_" }
    })
}

# Texte d'un TextBlock, même fait de plusieurs morceaux colorés (son .Text est alors vide)
function Get-TextBlockText($Tb) {
    if ($Tb.Text) { return $Tb.Text }
    -join @($Tb.Inlines | ForEach-Object { if ($_ -is [System.Windows.Documents.Run]) { $_.Text } })
}

# Premier élément visible de la page qui affiche ce texte (titre, bouton, case)
function Find-PageElement($Root, [string]$Text) {
    $stack = New-Object System.Collections.Stack
    $stack.Push($Root)
    $partial = $null
    while ($stack.Count) {
        $x = $stack.Pop()
        if ($x -is [System.Windows.UIElement] -and $x.Visibility -ne 'Visible') { continue }
        $label = if ($x -is [System.Windows.Controls.TextBlock]) { Get-TextBlockText $x } elseif ($x -is [System.Windows.Controls.ContentControl] -and $x.Content -is [string]) { [string]$x.Content } else { $null }
        if ($label) {
            $label = $label -replace '^(// |●  )', ''   # titres de section « ● TITRE »
            if ($label -eq $Text) { return $x }
            if (-not $partial -and $label.StartsWith($Text, [StringComparison]::OrdinalIgnoreCase)) { $partial = $x }
        }
        $kids = @([System.Windows.LogicalTreeHelper]::GetChildren($x) | Where-Object { $_ -is [System.Windows.DependencyObject] })
        for ($i = $kids.Count - 1; $i -ge 0; $i--) { $stack.Push($kids[$i]) }
    }
    $partial
}
