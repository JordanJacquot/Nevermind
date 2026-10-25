# Nevermind : les pages de l'écran (kit) autour de « Ton PC » : Ton PC, le Tableau de bord, et la fiche.
#
# La fiche (un point à régler, « Tout corriger », le résultat avec « Revenir en arrière », la vérification de l'écran
# après un changement de fréquence, la visite guidée) reste celle de toujours (tableau-de-bord.ps1, assistance.ps1) :
# son contenu est lu et montré dans l'écran en module déplié, et ses boutons sont ceux de la fiche (mêmes gestes).
# Chargé par Nevermind.ps1 après accueil-console.ps1.

$TonsStatut = @{ ok = 'ok'; warn = 'attention'; bad = 'urgent'; info = '' }

# Le ton d'une couleur de Nevermind pour l'écran : vert « ok », orange « attention », rouge « urgent », sinon rien
function Get-TonKit($Couleur) {
    $c = $null
    try {
        if ($Couleur -is [System.Windows.Media.SolidColorBrush]) { $c = $Couleur.Color }
        elseif ($Couleur) { $c = [System.Windows.Media.ColorConverter]::ConvertFromString([string]$Couleur) }
    } catch {}
    if ($null -eq $c) { return '' }
    $r = [int]$c.R; $g = [int]$c.G; $b = [int]$c.B
    if ($g -gt 150 -and $r -lt 120 -and $b -lt 170) { return 'ok' }
    if ($r -gt 190 -and $g -lt 110) { return 'urgent' }
    if ($r -gt 190 -and $g -ge 110 -and $g -lt 210 -and $b -lt 120) { return 'attention' }
    ''
}

# Une icône du kit pour un constat de l'analyse (d'après son identifiant)
function Get-IconeConstat([string]$Id) {
    switch -Regex ($Id) {
        '^tweak:'                 { 'manette'; break }
        '^startup'                { 'marche'; break }
        '^disk'                   { 'disque'; break }
        '^gpu'                    { 'puce'; break }
        '^ram'                    { 'memoire'; break }
        '^(ups|laptop-battery)'   { 'eclair'; break }
        '^(bsod|whea)'            { 'alerte'; break }
        '^(hvci|secureboot)'      { 'bouclier'; break }
        '^wifi'                   { 'wifi'; break }
        '^display'                { 'ecran'; break }
        default                   { 'etoile' }
    }
}
$IconesTon = @{ ok = @('ok', 'vert'); attention = @('alerte', 'orange'); urgent = @('alerte', 'rouge'); '' = @('aide', 'P') }
# Le seuil d'alerte de température de la carte graphique (le même que Test-TempAlert, jeu.ps1) : l'anneau montre la
# température sur ce seuil
$SeuilTempKit = 87

# ---------------------------------------------------------------------------
# Les mesures en direct (processeur, mémoire, carte graphique, température) : le fil des mesures de Nevermind ($Live)
# ---------------------------------------------------------------------------
function Get-MesuresKit {
    $info = if ($script:LastAnalysis -and $script:LastAnalysis.Info) { $script:LastAnalysis.Info } else { @{} }
    $l = New-Object System.Collections.ArrayList
    $pas = -not $Live.Updated
    $cpu = if ($pas -or $null -eq $Live.Cpu) { $null } else { [double]$Live.Cpu }
    [void]$l.Add(@{ id = 'cpu'; nom = 'Processeur'; note = [string]$info['Processeur']; valeur = $(if ($null -ne $cpu) { [int]$cpu } else { 0 }); texte = $(if ($null -ne $cpu) { "$([int]$cpu) %" } else { '...' }); ton = $(if ($cpu -ge 90) { 'attention' } else { '' }) })
    if (-not $pas -and $Live.RamTotal) {
        $p = 100 * $Live.RamUsed / $Live.RamTotal
        [void]$l.Add(@{ id = 'ram'; nom = 'Mémoire vive'; note = "$(Format-Size $Live.RamUsed) sur $(Format-Size $Live.RamTotal)"; valeur = [int]$p; texte = "$([int]$p) %"; ton = $(if ($p -ge 90) { 'attention' } else { '' }) })
    } else { [void]$l.Add(@{ id = 'ram'; nom = 'Mémoire vive'; note = ''; valeur = 0; texte = '...' }) }
    $gpu = if ($pas -or $null -eq $Live.Gpu) { $null } else { [double]$Live.Gpu }
    [void]$l.Add(@{ id = 'gpu'; nom = 'Carte graphique'; note = [string]$info['Carte graphique']; valeur = $(if ($null -ne $gpu) { [int]$gpu } else { 0 }); texte = $(if ($null -ne $gpu) { "$([int]$gpu) %" } else { '...' }) })
    if (-not $pas -and $null -ne $Live.GpuTemp) {
        $t = [double]$Live.GpuTemp
        [void]$l.Add(@{ id = 'temp'; nom = 'Température'; note = "carte graphique, alerte à $SeuilTempKit°"; valeur = [int][math]::Min(100, 100 * $t / $SeuilTempKit); texte = "$([int]$t)°"; ton = $(if ($t -ge $SeuilTempKit) { 'urgent' } elseif ($t -ge 80) { 'attention' } else { '' }) })
    }
    return @($l)
}
# Toutes les 1,5 s, sur la page Ton PC : les mesures changent sur place (sans renvoyer toute la page)
function Send-MesuresKit {
    if (-not $script:Vue.Pret -or $script:Vue.Ecran -ne 'pc' -or $script:Vue.CacheeDepuis) { return }
    $v = @{}; foreach ($m in (Get-MesuresKit)) { $v[$m.id] = @{ valeur = $m.valeur; texte = $m.texte; note = $m.note; ton = [string]$m.ton } }
    Send-OrdreVue @{ kit = 'mesures'; valeurs = $v }
}

# ---------------------------------------------------------------------------
# Ton PC et le Tableau de bord
# ---------------------------------------------------------------------------
function Get-LigneConstat($f, [string]$Id) {
    $auto = $f.Fix -and $f.Fix.Auto
    $ton = $TonsStatut[[string]$f.Status]
    @{ type = 'ligne'; id = $Id; icone = (Get-IconeConstat $f.Id); couleur = $(if ($ton -eq 'urgent') { 'rouge' } elseif ($f.Status -eq 'ok') { 'vert' } else { 'P' })
        texte = (Format-TexteKit $f.Titre); note = $(if ($f.Status -eq 'ok') { Format-TexteKit $f.Detail } elseif ($auto) { 'Nevermind le fait pour toi' } elseif ($f.Fix) { 'À faire toi-même, Nevermind explique comment' } else { Format-TexteKit $f.Detail })
        valeur = $(if ($f.Gain -gt 0) { "+$([int]$f.Gain) point$(if ($f.Gain -gt 1) { 's' })" } elseif ($f.Status -eq 'ok') { '' } else { 'Voir' }); ton = $(if ($ton -eq 'urgent') { 'urgent' } elseif ($f.Status -eq 'ok') { 'ok' } else { '' }) }
}

function Get-PagesPc {
    $a = $script:LastAnalysis
    $fd = Get-HubFindings $a
    $aRegler = @(@($fd.Important) + @($fd.Manual) | Where-Object { $_ })
    $todo = $aRegler.Count
    $script:Kit.Fiches = @($aRegler | Select-Object -First 8)
    $auto = @(if ($a) { $a.Active | Where-Object { $_.Status -ne 'ok' -and $_.Gain -gt 0 -and $_.Fix -and $_.Fix.Auto } })
    # (le gain de « Tout corriger » : le même calcul que « Jusqu'à N en un clic », Measure-Score)
    $gainAuto = if ($a) { [math]::Max(0, [int]$a.Potential - [int]$a.Score) } else { 0 }
    $boutonsTout = @(if ($auto.Count) { @{ id = 'toutcorriger'; texte = $(if ($gainAuto) { "Tout corriger (+$gainAuto point$(if ($gainAuto -gt 1) { 's' }))" } else { 'Tout corriger' }) } })
    $analyser = @{ id = 'analyser'; texte = 'Relancer l''analyse'; icone = 'actu' }

    $module = if ($a) {
        @{ anneau = @{ valeur = [int]$a.Score; sur = 100; ton = $(if ($todo) { 'attention' } else { '' }) }
            titre = [string]$a.Label
            texte = $(if ($todo) { "$(if ($todo -eq 1) { '1 point' } else { "$todo points" }) à régler pour gagner des points$(if ($auto.Count -eq $todo) { ', Nevermind s''en charge.' } elseif ($auto.Count) { ", dont $($auto.Count) que Nevermind fait pour toi." } else { '.' })" } else { 'Ton PC est au top pour jouer. Rien à corriger.' })
            boutons = $boutonsTout }
    } else { @{ icone = 'pc'; titre = 'Analyse en cours'; texte = 'Je regarde ton PC, ça prend quelques secondes.' } }

    $lignes = @(for ($k = 0; $k -lt $script:Kit.Fiches.Count; $k++) { Get-LigneConstat $script:Kit.Fiches[$k] "fiche:$k" })
    $cartes = @(if ($a -and $a.Cards) { $a.Cards })
    $blocs = @(
        @{ type = 'titre'; texte = 'Ton PC maintenant'; direct = $true }
        @{ type = 'mesures'; elements = @(Get-MesuresKit) }
        if ($lignes.Count) { @{ type = 'titre'; texte = 'Pour gagner des points'; droite = $(if ($todo -gt $lignes.Count) { "les $($lignes.Count) premiers sur $todo" } else { "$todo à régler" }) } }
    ) + $lignes + @(
        @{ type = 'titre'; texte = 'Outils' }
        @{ type = 'raccourcis'; elements = @(@{ id = 'tableau'; nom = 'Tableau de bord'; icone = 'jauge'; couleur = 'P' }, @{ id = (Get-IdOutilKit 'tests' 5); nom = 'Tests des composants'; icone = 'test'; couleur = 'S' }) }
        if ($cartes.Count) {
            @{ type = 'titre'; texte = 'Santé des composants'; droite = [string]$ui.HealthSummary.Text }
            @{ type = 'chiffres'; elements = @(foreach ($c in $cartes) { @{ titre = [string]$c.Titre; valeur = [string]$StatusLabels[[string]$c.Status]; ton = $TonsStatut[[string]$c.Status] } }) }
        }
    )

    # Le Tableau de bord : tout le détail de l'analyse
    $tous = @(if ($a) { $a.Active })
    $pourInfo = @($tous | Where-Object { $_.Status -ne 'ok' -and $aRegler -notcontains $_ })
    $dejaOk = @($tous | Where-Object { $_.Status -eq 'ok' })
    $ignores = @($script:IgnoredFindings | Where-Object { $_ })
    $script:Kit.Constats = @($aRegler) + @($pourInfo) + @($dejaOk) + @($ignores)
    $k = 0
    $lr = @(foreach ($f in $aRegler) { $l = Get-LigneConstat $f "constat:$k"; $l.etiquette = [string]$StatusLabels[[string]$f.Status]; $l.ton = $TonsStatut[[string]$f.Status]; $l; $k++ })
    $lp = @(foreach ($f in $pourInfo) { $l = Get-LigneConstat $f "constat:$k"; $l.etiquette = 'Pour info'; $l.ton = ''; $l.valeur = 'Voir'; $l; $k++ })
    $lo = @(foreach ($f in $dejaOk) { $l = Get-LigneConstat $f "constat:$k"; $l; $k++ })
    $li = @(foreach ($f in $ignores) { $l = Get-LigneConstat $f "constat:$k"; $l.etiquette = 'Ignoré'; $l.note = 'Ne compte plus dans ton score'; $l; $k++ })
    $onglets = @(
        @{ id = 'regarder'; nom = "À régler ($($aRegler.Count))"; blocs = $(if ($lr.Count) { @(@{ type = 'texte'; texte = 'Les points qui font gagner des points. Clique sur un point pour voir ce qu''il faut faire.' }) + $lr } else { @(@{ type = 'texte'; texte = 'Rien à corriger : ton PC est bien réglé pour le jeu.' }) }) }
    )
    if ($lp.Count) { $onglets += @{ id = 'info'; nom = "Pour info ($($lp.Count))"; blocs = @(@{ type = 'texte'; texte = 'Ces points ne comptent pas dans ton score : à toi de voir s''ils te concernent.' }) + $lp } }
    $onglets += @{ id = 'ok'; nom = "Déjà OK ($($dejaOk.Count))"; blocs = $lo }
    if ($li.Count) { $onglets += @{ id = 'ignores'; nom = "Ignorés ($($li.Count))"; blocs = $li } }
    $comp = New-Object System.Collections.ArrayList
    $script:Kit.ActionsComposants = @()
    foreach ($c in $cartes) {
        [void]$comp.Add(@{ type = 'titre'; texte = [string]$c.Titre; droite = [string]$StatusLabels[[string]$c.Status] })
        foreach ($b in $c.Bars) { [void]$comp.Add(@{ type = 'ligne'; texte = [string]$b.Label; barre = [math]::Max(0, [math]::Min(100, [double]$b.Value)); valeur = [string]$b.Text; ton = (Get-TonKit $b.Color) }) }
        $lignesC = @(foreach ($kk in $c.Lines.Keys) { $v = $c.Lines[$kk]; $t = $v; if ($v -is [array]) { $t = $v[0] }; @{ texte = [string]$kk; valeur = [string]$t } })
        if ($lignesC.Count) { [void]$comp.Add(@{ type = 'reglages'; rangs = $lignesC }) }
        foreach ($n in $c.Notes) { [void]$comp.Add(@{ type = 'note'; texte = [string]$n.Text }) }
        if ($c.Action) {
            $script:Kit.ActionsComposants += [string]$c.Action
            [void]$comp.Add(@{ type = 'bouton'; id = "composant:$($script:Kit.ActionsComposants.Count - 1)"; texte = [string]$c.ActionLabel; style = 'second' })
        }
    }
    if ($comp.Count) { $onglets += @{ id = 'composants'; nom = 'Composants'; blocs = @($comp) } }

    [ordered]@{
        pc = @{ titre = 'Ton PC'; sous = 'La santé de chaque composant de ton PC et ce qui freine tes jeux.'; icone = 'pc'; couleur = 'P'; actions = @($analyser); module = $module; blocs = $blocs }
        tableau = @{ titre = 'Tableau de bord'; parent = 'pc'; sous = 'Tout le détail de l''analyse : ce qui freine tes jeux, et la santé de chaque composant.'; icone = 'jauge'; couleur = 'P'; actions = @($analyser)
            module = $(if ($a) { @{ anneau = @{ valeur = [int]$a.Score; sur = 100; ton = $(if ($todo) { 'attention' } else { '' }) }; titre = [string]$a.Label; texte = $(if ($gainAuto -and $auto.Count) { "Jusqu'à $($a.Potential) en un clic." } elseif ($todo) { "$(if ($todo -eq 1) { '1 point' } else { "$todo points" }) à régler." } else { 'Rien à corriger.' }); boutons = $boutonsTout } } else { $module })
            onglets = $onglets }
    }
}

# L'id d'un outil dans l'écran : sa page du kit si elle existe, sinon l'ancienne page de Nevermind (« page:N »)
function Get-IdOutilKit([string]$Page, [int]$Index) { if ($script:Kit.PagesKit -contains $Page) { $Page } else { "page:$Index" } }

# ---------------------------------------------------------------------------
# La fiche : le panneau de Nevermind ($ui.Overlay) lu et montré dans l'écran
# ---------------------------------------------------------------------------
function Get-FicheKit {
    $titre = ''; $avant = ''; $ton = ''; $etiq = @()
    $blocs = New-Object System.Collections.ArrayList
    $liste = $null; $sorte = ''
    $finir = { if ($liste -and $liste.Count) { [void]$blocs.Add($(if ($sorte -eq 'etapes') { @{ type = 'etapes'; elements = @($liste | ForEach-Object { @{ texte = $_ } }) } } else { @{ type = 'liste'; elements = @($liste) } })) }}
    foreach ($el in @($ui.SheetBody.Children)) {
        if ($el.Visibility -ne 'Visible') { continue }
        # « Détails techniques » (Add-SheetDetails) : un bloc qui s'ouvre, comme dans l'historique ; ceux qui se suivent
        # vont dans le même bloc
        if ($el -is [System.Windows.Controls.Expander]) {
            if ($liste) { & $finir; $liste = $null }
            $lg = @($el.Tag.Lignes)
            $q = @{ question = [string]$el.Header; lignes = $lg; reponse = ($lg -join ' ; '); icone = 'liste' }
            $der = if ($blocs.Count) { $blocs[$blocs.Count - 1] } else { $null }
            if ($der -and $der.type -eq 'faq') { $der.elements = @($der.elements) + @($q) } else { [void]$blocs.Add(@{ type = 'faq'; elements = @($q) }) }
            continue
        }
        if ($el -is [System.Windows.Controls.Grid]) {
            foreach ($x in $el.Children) {
                if ($x -is [System.Windows.Controls.TextBlock] -and -not $titre) { $titre = $x.Text }
                elseif ($x -is [System.Windows.Shapes.Ellipse]) { $ton = Get-TonKit $x.Fill }
            }
            continue
        }
        if ($el -is [System.Windows.Controls.WrapPanel]) {
            foreach ($x in $el.Children) { if ($x -is [System.Windows.Controls.Border] -and $x.Child -is [System.Windows.Controls.TextBlock]) { $etiq += @{ texte = $x.Child.Text; ton = (Get-TonKit $x.Child.Foreground) } } }
            continue
        }
        if ($el -is [System.Windows.Controls.Border] -and $el.Child -is [System.Windows.Controls.TextBlock]) {
            if ($liste) { & $finir; $liste = $null }
            [void]$blocs.Add(@{ type = 'note'; texte = $el.Child.Text }); continue
        }
        if ($el -is [System.Windows.Controls.TextBlock]) {
            $t = [string]$el.Text
            if (-not $titre) {
                if ($el.FontWeight -eq [System.Windows.FontWeights]::Bold) { $titre = $t } else { $avant = $t }
                continue
            }
            if ($t -match '^\d+\.\s+(.*)$') { if ($sorte -ne 'etapes' -and $liste) { & $finir; $liste = $null }; if (-not $liste) { $liste = New-Object System.Collections.ArrayList; $sorte = 'etapes' }; [void]$liste.Add($Matches[1]); continue }
            if ($t -match '^•\s+(.*)$') { if ($sorte -ne 'liste' -and $liste) { & $finir; $liste = $null }; if (-not $liste) { $liste = New-Object System.Collections.ArrayList; $sorte = 'liste' }; [void]$liste.Add($Matches[1]); continue }
            if ($liste) { & $finir; $liste = $null }
            if ($el.FontWeight -eq [System.Windows.FontWeights]::SemiBold -or $el.FontWeight -eq [System.Windows.FontWeights]::Bold) { [void]$blocs.Add(@{ type = 'titre'; texte = $t }) }
            else { [void]$blocs.Add(@{ type = 'texte'; texte = $t }) }
            continue
        }
        if ($el -is [System.Windows.Controls.Panel]) {   # (autre chose : ses textes, à la suite)
            foreach ($x in $el.Children) { if ($x -is [System.Windows.Controls.TextBlock] -and $x.Text) { [void]$blocs.Add(@{ type = 'texte'; texte = [string]$x.Text }) } }
        }
    }
    if ($liste) { & $finir }
    $b = @{}
    foreach ($n in 'SheetRun', 'SheetOpen', 'SheetIgnore', 'SheetClose') { if ($ui[$n].Visibility -eq 'Visible' -and $ui[$n].IsEnabled) { $b[$n] = ([string]$ui[$n].Content) -replace '^Onglet ', 'Ouvrir ' } }
    $ic = $IconesTon[$ton]; if ($script:SheetMode -eq 'tour') { $ic = @('etoile', 'P') } elseif ($script:SheetMode -eq 'display') { $ic = @('ecran', 'orange') }
    $boutonsM = @(
        if ($b.SheetRun) { @{ id = 'fiche:run'; texte = $b.SheetRun } }
        if ($b.SheetOpen) { @{ id = 'fiche:open'; texte = $b.SheetOpen; style = 'second' } }
    )
    $bas = @(
        if ($b.SheetIgnore) { @{ type = 'bouton'; id = 'fiche:ignore'; texte = $b.SheetIgnore; style = 'second' } }
        if ($b.SheetClose) { @{ type = 'bouton'; id = 'fiche:close'; texte = $b.SheetClose; style = 'second' } }
    )
    # En tête du module : la première étiquette (« +4 points au score »), puis les autres
    $mt = if ($etiq.Count) { $etiq[0].texte } else { '' }
    $mx = (@($etiq | Select-Object -Skip 1 | ForEach-Object { $_.texte }) -join ' · ')
    @{ titre = $(if ($titre) { $titre } else { 'Nevermind' }); sous = $avant; icone = $ic[0]; couleur = $ic[1]; retour = 'appli'
        parent = $(if ($script:Kit.FicheDe -and $script:Kit.FicheDe -ne 'accueil' -and $script:Kit.FicheDe -ne 'fiche') { $script:Kit.FicheDe })
        module = @{ icone = $ic[0]; couleur = $ic[1]; titre = $mt; texte = $mx; boutons = $boutonsM }
        blocs = @($blocs) + $bas }
}

# Appelé toutes les 200 ms (Update-EcranKitVisible) : la fiche ouverte est montrée, et suivie (compte à rebours...)
function Update-FicheKit {
    $ouverte = $ui.Overlay.Visibility -eq 'Visible'
    if ($ouverte) {
        # (relue au plus une fois toutes les 900 ms, sauf dans les 2 s après un geste : la fiche répond tout de suite)
        $maintenant = Get-Date
        if ($script:Kit.FicheOuverte -and $script:Kit.FicheVu -and ($maintenant - $script:Kit.FicheVu).TotalMilliseconds -lt 900 -and ($maintenant - $script:Kit.Geste).TotalSeconds -gt 2) { return }
        $script:Kit.FicheVu = $maintenant
        $f = Get-FicheKit
        $cle = $f | ConvertTo-Json -Depth 8 -Compress
        if ($cle -eq $script:Kit.FicheCle) { return }
        $script:Kit.FicheCle = $cle; $script:Kit.Fiche = $f
        $premiere = -not $script:Kit.FicheOuverte
        if ($premiere) { $script:Kit.FicheOuverte = $true; $script:Kit.FicheDe = $(if ($script:Vue.Ecran) { $script:Vue.Ecran } else { 'accueil' }); $f.parent = $(if ($script:Kit.FicheDe -notin 'accueil', 'fiche') { $script:Kit.FicheDe }) }
        Update-EcranNevermind
        if ($premiere) { Open-EcranVue 'fiche' }
    } elseif ($script:Kit.FicheOuverte) {
        $script:Kit.FicheOuverte = $false; $script:Kit.Fiche = $null; $script:Kit.FicheCle = ''; $script:Kit.FicheVu = $null
        $de = $script:Kit.FicheDe
        if ($script:Vue.Ecran -eq 'fiche') { Open-EcranVue $(if ($de -and $de -ne 'fiche') { $de } else { 'accueil' }) }
        Update-EcranNevermind
    }
}

function Register-ActionsPc {
    Register-ActionVue 'toutcorriger' { param($Valeur, $Ecran) Invoke-Safe { Open-FixAll } }
    # Les boutons de la fiche : exactement ceux de la fiche de Nevermind
    foreach ($n in @(@('fiche:run', 'SheetRun'), @('fiche:open', 'SheetOpen'), @('fiche:ignore', 'SheetIgnore'), @('fiche:close', 'SheetClose'))) {
        $bouton = $n[1]
        Register-ActionVue $n[0] ([scriptblock]::Create("param(`$Valeur, `$Ecran) `$b = `$ui['$bouton']; if (`$b.Visibility -eq 'Visible' -and `$b.IsEnabled) { `$b.RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Primitives.ButtonBase]::ClickEvent))) }"))
    }
}
