# Nevermind : barre de recherche des réglages (en haut de chaque page, Ctrl+K).
# Suggestions au fil de la frappe ; un clic ouvre la bonne page et fait clignoter le réglage.
# Chargé par OptiGame.ps1 après les pages (il connaît leurs numéros) et avant evenements.ps1.

# Réglages et actions trouvables. P : page (numéro ou 'trafic'), S : sous-onglet d'Optimisation gaming,
# A : texte affiché sur la page, mis en évidence à l'arrivée, K : autres mots qui doivent le trouver,
# Do : action après l'arrivée (ouvrir une fenêtre). Les accents et majuscules ne comptent pas.
$SearchEntries = @(
    # Optimisation gaming
    @{ T = 'Réglages Windows pour les jeux'; P = 1; S = 'tweaks'; A = 'Cocher le recommandé'; K = 'tweaks optimiser optimisation fps appliquer recommande gagner performances' },
    @{ T = 'Point de restauration avant les réglages'; P = 1; S = 'tweaks'; A = 'Point de restauration avant (recommandé)'; K = 'securite sauvegarde avant' },
    @{ T = 'Mesurer mes FPS'; P = 1; S = 'fps'; A = 'Mesurer mes FPS quand je joue'; K = 'fps images par seconde presentmon mesure partie framerate' },
    @{ T = 'Mes parties (historique des FPS)'; P = 1; S = 'fps'; A = 'Mesurer mes FPS quand je joue'; K = 'historique parties fps moyenne bas avant apres comparaison' },
    @{ T = 'Mes FPS ne sont pas normaux'; P = 1; S = 'fps'; A = 'Mes FPS ne sont pas normaux : trouver pourquoi'; K = 'diagnostic fps rame saccades chutes freeze lent' },
    @{ T = 'Raccourci Ctrl+Maj+F (mesure manuelle)'; P = 1; S = 'overlay'; A = 'Afficher le compteur pendant la partie'; K = 'raccourci clavier hotkey touche mesure manuelle autre jeu' },
    @{ T = 'Compteur de FPS à l''écran (overlay)'; P = 1; S = 'overlay'; A = 'Afficher le compteur pendant la partie'; K = 'overlay compteur afficher ecran osd fps' },
    @{ T = 'Style du compteur : complet ou discret'; P = 1; S = 'overlay'; A = 'Style du compteur'; K = 'overlay discret petit transparent semi transparence taille apparence' },
    @{ T = 'Position du compteur'; P = 1; S = 'overlay'; A = 'Position du compteur'; K = 'overlay coin haut bas gauche droite emplacement deplacer' },
    @{ T = 'Lag en ligne'; P = 1; S = 'lag'; A = 'Mesurer ma connexion quand je joue'; K = 'lag ping latence serveur jeu connexion partie online' },
    @{ T = 'Tester ma connexion pour le jeu (30 s)'; P = 1; S = 'lag'; A = 'Tester ma connexion maintenant (30 s)'; K = 'test lag ping latence' },
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
    @{ T = 'Nettoyage du disque'; P = 4; A = 'Analyser'; K = 'nettoyer nettoyage fichiers temporaires place disque espace plein' },
    # Tests
    @{ T = 'Tester la vitesse du disque'; P = 5; A = 'Tester la vitesse'; K = 'ssd hdd nvme disque vitesse lecture ecriture' },
    @{ T = 'Santé du disque'; P = 5; A = 'Santé'; K = 'ssd hdd disque usure temperature smart' },
    @{ T = 'Test du processeur'; P = 5; A = 'Test rapide (30 s)'; K = 'cpu processeur stabilite puissance' },
    @{ T = 'Test de la mémoire'; P = 5; A = 'Tester la mémoire'; K = 'ram memoire barrette erreurs' },
    @{ T = 'Carte graphique en direct'; P = 5; A = 'Surveiller en direct'; K = 'gpu temperature carte graphique nvidia amd' },
    @{ T = 'Débit Internet'; P = 5; A = 'Tester ma connexion'; K = 'debit vitesse internet speedtest telechargement' },
    @{ T = 'Lag en charge (bufferbloat)'; P = 5; A = 'Lag en charge'; K = 'bufferbloat lag charge telechargement' },
    @{ T = 'Pixels morts de l''écran'; P = 5; A = 'Écrans'; K = 'pixels morts ecran moniteur couleurs' },
    @{ T = 'Batterie du portable'; P = 5; A = 'Batterie'; K = 'batterie portable autonomie usure' },
    # Sécurité
    @{ T = 'Analyse antivirus rapide'; P = 6; A = 'Analyse rapide'; K = 'virus defender antivirus scan malware' },
    @{ T = 'Analyse antivirus complète'; P = 6; A = 'Analyse complète'; K = 'virus defender antivirus scan complet' },
    @{ T = 'Analyser un dossier'; P = 6; A = 'Analyser un dossier'; K = 'virus dossier fichier scan' },
    @{ T = 'Mettre à jour l''antivirus'; P = 6; A = 'Mettre à jour la base'; K = 'defender definitions signatures antivirus' },
    @{ T = 'Niveau de protection'; P = 6; A = 'Points à vérifier'; K = 'protection securite score pare feu' },
    # Sauvegarde
    @{ T = 'Tout annuler'; P = 7; A = 'Annuler les changements de Nevermind'; K = 'annuler restaurer revenir arriere defaire' },
    @{ T = 'Historique des changements'; P = 7; A = 'Historique des changements'; K = 'historique annuler changement' },
    @{ T = 'Point de restauration Windows'; P = 7; A = 'Point de restauration Windows'; K = 'restauration systeme sauvegarde' },
    @{ T = 'Rapport du PC'; P = 7; A = 'Rapport de ton PC'; K = 'rapport export html partager configuration' },
    @{ T = 'Mises à jour de Nevermind'; P = 7; A = 'Mises à jour'; K = 'version update maj nouvelle' },
    @{ T = 'Versions bêta'; P = 7; A = 'Mises à jour'; K = 'beta preversion avant premiere' },
    @{ T = 'Raccourci sur le bureau'; P = 7; A = 'Raccourci et démarrage'; K = 'raccourci bureau icone' },
    @{ T = 'Lancer Nevermind au démarrage du PC'; P = 7; A = 'Raccourci et démarrage'; K = 'demarrage automatique boot windows lancement auto allumage' },
    @{ T = 'Signaler un problème'; P = 7; A = 'Signaler un problème'; K = 'bug erreur rapport aide support' },
    # Réseau
    @{ T = 'Scanner le réseau'; P = 'reseau'; A = 'Scanner le réseau'; K = 'appareils wifi box scan connectes' },
    @{ T = 'Carte du réseau'; P = 'reseau'; A = 'Ton réseau'; K = 'carte constellation appareils map'; Do = { if (@($script:NetList).Count) { Show-NetMap } } },
    @{ T = 'Audit de sécurité du réseau'; P = 'reseau'; A = 'Audit de sécurité'; K = 'wifi wps upnp securite box audit' },
    @{ T = 'Prévenir si un nouvel appareil se connecte'; P = 'reseau'; A = 'Appareils connectés'; K = 'alerte nouvel appareil intrus surveillance notification' },
    # Trafic
    @{ T = 'Ce qui sort de ton PC'; P = 'trafic'; A = 'Ce qui sort de ton PC'; K = 'trafic internet donnees envoyees programmes connexions espion' },
    @{ T = 'Identifier les serveurs sans nom'; P = 'trafic'; A = 'Identifier les serveurs sans nom'; K = 'rdap annuaire serveur ip proprietaire' },
    @{ T = 'Ce que Windows envoie à Microsoft'; P = 'trafic'; A = 'Ce que Windows envoie à Microsoft'; K = 'telemetrie microsoft vie privee identifiant confidentialite diagnostic'; Do = { Show-WindowsPrivacy } },
    @{ T = 'Bloquer Internet à un programme'; P = 'trafic'; A = 'Ce qui sort de ton PC'; K = 'pare feu bloquer internet firewall programme' }
)
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
    foreach ($e in $SearchEntries) { [void]$all.Add($e) }
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
    $sub = if ($E.S) { ' › ' + (@($GamingSubPages | Where-Object { $_.Id -eq $E.S })[0]).Label } else { '' }
    switch ($E.P) {
        'reseau' { 'Réseau' }
        'jeux' { 'Jeux' }
        'trafic' { 'Trafic' }
        default { "Ordinateur › $($PageNames[[int]$E.P])$sub" }
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
# Liste des suggestions sous la barre
# ---------------------------------------------------------------------------
function Update-SearchResults {
    $box = $script:Search.Results
    $box.Children.Clear()
    $text = [string]$script:Search.Input.Text
    $script:Search.Hint.Visibility = if ($text) { 'Collapsed' } else { 'Visible' }
    $script:Search.Key.Visibility = if ($text) { 'Collapsed' } else { 'Visible' }
    if ($text.Trim()) {
        $list = @(Find-Settings $text)
        $head = $null
    } else {
        $idx = Get-SearchIndex
        $list = @(foreach ($s in $SearchStarters) { @($idx | Where-Object { $_.T -eq $s })[0] })
        $head = 'SUGGESTIONS'
    }
    $script:Search.List = $list
    $script:Search.Sel = 0
    if ($head) {
        $h = New-Text $head 10.5 '#655E7E' -Semi
        $h.Margin = New-Thickness 10 4 0 4
        [void]$box.Children.Add($h)
    }
    if (-not $list.Count) {
        $n = New-Text "Aucun réglage trouvé pour « $($text.Trim()) ». Essaie un autre mot : fps, ping, démarrage, nettoyage..." 12.5 '#A6A1BC'
        $n.Margin = New-Thickness 10 8 10 8
        [void]$box.Children.Add($n)
    }
    $script:Search.Rows = @()
    for ($i = 0; $i -lt $list.Count; $i++) {
        $e = $list[$i]
        $row = New-Object System.Windows.Controls.Border
        $row.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $row.Padding = New-Thickness 10 7 10 7
        $row.Cursor = [System.Windows.Input.Cursors]::Hand
        $row.Background = [System.Windows.Media.Brushes]::Transparent
        $sp = New-Object System.Windows.Controls.StackPanel
        $t = New-Text $e.T 13 '#FFFFFF' -Semi
        $t.TextTrimming = 'CharacterEllipsis'; $t.TextWrapping = 'NoWrap'
        [void]$sp.Children.Add($t)
        $w = New-Text $e.Where 11.5 '#A6A1BC'
        $w.TextTrimming = 'CharacterEllipsis'; $w.TextWrapping = 'NoWrap'
        [void]$sp.Children.Add($w)
        $row.Child = $sp
        $row.Tag = $i
        $row.Add_MouseEnter({ param($s, $e) Set-SearchSelection ([int]$s.Tag) })
        $row.Add_MouseLeftButtonDown({ param($s, $e) $e.Handled = $true; $x = $script:Search.List[[int]$s.Tag]; Invoke-Safe { Open-SearchEntry $x } })
        [void]$box.Children.Add($row)
        $script:Search.Rows += $row
    }
    Set-SearchSelection 0
    $script:Search.Popup.IsOpen = $true
}

function Set-SearchSelection([int]$Index) {
    $rows = @($script:Search.Rows)
    if (-not $rows.Count) { return }
    $Index = [math]::Max(0, [math]::Min($rows.Count - 1, $Index))
    $script:Search.Sel = $Index
    for ($i = 0; $i -lt $rows.Count; $i++) { $rows[$i].Background = if ($i -eq $Index) { Get-Brush '#262D3A' } else { [System.Windows.Media.Brushes]::Transparent } }
}

function Update-SearchHint {
    $empty = -not [string]$script:Search.Input.Text
    $script:Search.Hint.Visibility = if ($empty) { 'Visible' } else { 'Collapsed' }
    $script:Search.Key.Visibility = if ($empty) { 'Visible' } else { 'Collapsed' }
}

function Close-Search([switch]$Clear) {
    if (-not $script:Search) { return }
    # Texte vidé d'abord : sinon la liste se rouvrirait sur le changement de texte
    if ($Clear) { $script:Search.Input.Text = '' }
    $script:Search.Popup.IsOpen = $false
}

# ---------------------------------------------------------------------------
# Aller au réglage : bonne page, bon sous-onglet, puis le réglage clignote
# ---------------------------------------------------------------------------
function Open-SearchEntry($E) {
    if (-not $E) { return }
    Close-Search -Clear
    [System.Windows.Input.Keyboard]::ClearFocus()
    if ($ui.TestOverlay.Visibility -eq 'Visible') { Hide-TestPanel }
    if ($ui.NetMapOverlay.Visibility -eq 'Visible') { Hide-NetMap }
    if ($ui.Overlay.Visibility -eq 'Visible' -and $script:SheetMode -ne 'display') { Close-Sheet }
    $page = switch ($E.P) { 'reseau' { $NetIndex } 'trafic' { $TrafficIndex } 'jeux' { $GamesIndex } default { [int]$E.P } }
    Show-Page $page
    if ($E.S) { Build-GamingTabs; Set-GamingSubPage $E.S }
    if ($E.Game) { $ui.LibSearch.Text = ''; $script:LibFilter = 'Tous'; Update-LibraryView; Set-LibrarySelection $E.Game }
    Set-Status "$($E.T) : $($E.Where)"
    $anchor = [string]$E.A
    $after = $E.Do
    # Une fois la page affichée et mise en page
    $null = $Window.Dispatcher.BeginInvoke([System.Windows.Threading.DispatcherPriority]::Loaded, [Action]{
        try {
            if ($anchor) {
                $el = Find-PageElement $ui.Tabs.Items[$page].Content $anchor
                if ($el) { Show-Highlight $el }
            }
            if ($after) { Invoke-Safe $after }
        } catch { Write-Log "Recherche: $_" }
    }.GetNewClosure())
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

# Fait défiler jusqu'au réglage et entoure sa carte d'un halo cyan pendant 2 secondes
function Show-Highlight($El) {
    $card = $El
    $p = $El
    while ($p) {
        if ($p -is [System.Windows.Controls.Border] -and $p.CornerRadius.TopLeft -ge 8 -and $p.ActualWidth -gt 200) { $card = $p; break }
        $p = [System.Windows.LogicalTreeHelper]::GetParent($p)
        if (-not $p) { break }
    }
    $card.BringIntoView()
    $script:SearchLastHit = $card
    $script:SearchFlashCount = 0
    if ($card -is [System.Windows.Controls.Border]) {
        $oldBrush = $card.ReadLocalValue([System.Windows.Controls.Border]::BorderBrushProperty)
        $oldThick = $card.ReadLocalValue([System.Windows.Controls.Border]::BorderThicknessProperty)
        $card.BorderThickness = New-Thickness 2 2 2 2
        $br = Get-Brush $Colors.accent
        $card.BorderBrush = $br
        $fade = New-Object System.Windows.Media.Animation.ColorAnimation
        $fade.To = [System.Windows.Media.Color]::FromArgb(0, 0x00, 0xD9, 0xF5)
        $fade.BeginTime = [TimeSpan]::FromMilliseconds(1200)
        $fade.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds(900))
        $br.BeginAnimation([System.Windows.Media.SolidColorBrush]::ColorProperty, $fade)
    }
    $oldEffect = $card.Effect
    $card.Effect = New-Glow $Colors.accent 22 0.9
    $t = New-Object System.Windows.Threading.DispatcherTimer
    $t.Interval = [TimeSpan]::FromMilliseconds(2200)
    $state = @{ Card = $card; Effect = $oldEffect; Brush = $oldBrush; Thick = $oldThick; Timer = $t }
    $t.Tag = $state
    $t.Add_Tick({
        param($s, $e)
        $x = $s.Tag
        $s.Stop()
        $c = $x.Card
        $c.Effect = $x.Effect
        if ($c -is [System.Windows.Controls.Border]) {
            if ($x.Brush -eq [System.Windows.DependencyProperty]::UnsetValue) { $c.ClearValue([System.Windows.Controls.Border]::BorderBrushProperty) } else { $c.BorderBrush = $x.Brush }
            if ($x.Thick -eq [System.Windows.DependencyProperty]::UnsetValue) { $c.ClearValue([System.Windows.Controls.Border]::BorderThicknessProperty) } else { $c.BorderThickness = $x.Thick }
        }
    })
    $t.Start()
}

# ---------------------------------------------------------------------------
# Branchement (la barre fait partie du modèle de la fenêtre : appelé une fois la fenêtre affichée)
# ---------------------------------------------------------------------------
function Initialize-Search {
    $tpl = $ui.Tabs.Template
    $s = @{
        Box = $tpl.FindName('SearchBox', $ui.Tabs); Input = $tpl.FindName('SearchInput', $ui.Tabs); Hint = $tpl.FindName('SearchHint', $ui.Tabs)
        Key = $tpl.FindName('SearchKey', $ui.Tabs); Popup = $tpl.FindName('SearchPopup', $ui.Tabs); Results = $tpl.FindName('SearchResults', $ui.Tabs)
        List = @(); Rows = @(); Sel = 0
    }
    if (-not $s.Input -or -not $s.Popup) { Write-Log 'Recherche: barre introuvable'; return }
    $script:Search = $s
    $s.Input.Add_TextChanged({ if ($script:Search.Input.IsKeyboardFocused) { try { Update-SearchResults } catch { Write-Log "Recherche: $_" } } else { Update-SearchHint } })
    $s.Input.Add_GotKeyboardFocus({
        $script:Search.Box.BorderBrush = Get-Brush $Colors.accent
        try { Update-SearchResults } catch { Write-Log "Recherche: $_" }
    })
    $s.Input.Add_LostKeyboardFocus({
        $script:Search.Box.ClearValue([System.Windows.Controls.Border]::BorderBrushProperty)
        # Laisse le temps à un clic dans la liste d'arriver
        $t = New-Object System.Windows.Threading.DispatcherTimer
        $t.Interval = [TimeSpan]::FromMilliseconds(180)
        $t.Add_Tick({ param($x, $y) $x.Stop(); if (-not $script:Search.Input.IsKeyboardFocused) { Close-Search } })
        $t.Start()
    })
    $s.Input.Add_PreviewKeyDown({
        param($src, $e)
        switch ([string]$e.Key) {
            'Down' { Set-SearchSelection ($script:Search.Sel + 1); $e.Handled = $true }
            'Up' { Set-SearchSelection ($script:Search.Sel - 1); $e.Handled = $true }
            'Return' { $x = @($script:Search.List)[$script:Search.Sel]; if ($x) { Invoke-Safe { Open-SearchEntry $x } }; $e.Handled = $true }
            'Escape' { Close-Search -Clear; [System.Windows.Input.Keyboard]::ClearFocus(); $e.Handled = $true }
        }
    })
    $Window.Add_Deactivated({ Close-Search })
    $Window.Add_LocationChanged({ Close-Search })
}

# Ctrl+K ou Ctrl+F : va dans la barre de recherche
function Focus-Search {
    if (-not $script:Search) { return }
    [void]$script:Search.Input.Focus()
    $script:Search.Input.SelectAll()
}
