# Nevermind : la page Réseau de l'écran (kit) : le système solaire (ta box au centre, chaque appareil en orbite d'autant
# plus proche qu'il répond vite), l'audit de sécurité, les appareils à vérifier et la liste des appareils.
# Les appareils viennent du scan de Nevermind ($script:NetList) ; la fiche d'un appareil s'ouvre dans le panneau
# (ecran-panneau.ps1).
# Chargé par OptiGame.ps1 après ecran-jeux.ps1.

# L'icône et la famille (couleur) d'un appareil dans le système solaire, d'après sa sorte
$SortesReseau = @{
    'Ce PC' = @('pc', 'P'); 'Box Internet' = @('box', 'P'); 'Caméra' = @('camera', 'orange'); 'Imprimante' = @('imprimante', 'P')
    'Routeur ou répéteur Wi-Fi' = @('repeteur', 'vert'); 'Ordinateur' = @('pc', 'P'); 'Appareil Apple' = @('tel', 'S'); 'TV ou multimédia' = @('tv', 'T')
    'Enceinte ou audio' = @('enceinte', 'T'); 'Objet connecté' = @('ampoule', 'vert'); 'Téléphone ou tablette' = @('tel', 'S'); 'Console de jeu' = @('console', 'T')
    'Box ou décodeur TV' = @('box', 'T'); 'Téléphone probable' = @('tel', 'S'); 'Appareil' = @('inconnu', 'doux')
}
function Get-SorteReseau($D) { $s = $SortesReseau[[string]$D.KindInfo.Kind]; if ($s) { $s } else { @('inconnu', 'doux') } }

function Get-PagesReseau {
    $liste = @($script:NetList | Where-Object { $_ })
    $script:Kit.Appareils = $liste
    $aVoir = @($liste | Where-Object { $_.New -or $_.Camera })
    $box = @($liste | Where-Object { $_.Gateway })[0]
    $scan = [bool]$script:NetScanning
    $audit = $script:NetAuditSaved
    $filtre = if ($script:Kit.ReseauFiltre) { $script:Kit.ReseauFiltre } else { 'all' }
    $test = @($NetFilters | Where-Object { $_.Id -eq $filtre })[0].Test; if (-not $test) { $test = { $true }; $filtre = 'all' }
    $montres = @($liste | Where-Object $test)
    $planetes = @(foreach ($d in $liste) {
        if ($d.Gateway) { continue }
        $s = Get-SorteReseau $d
        @{ nom = [string]$d.Title; icone = $s[0]; couleur = $s[1]; ping = $(if ($null -ne $d.Ms) { [double]$d.Ms } else { 50 }); attention = [bool]($d.Camera); nouveau = [bool]$d.New }
    })
    $ligneApp = {
        param($d, [int]$k)
        $s = Get-SorteReseau $d
        $badges = @(if ($d.Self) { 'Ce PC' }; if ($d.Gateway) { 'Ta box' }; if ($d.New) { 'Nouveau' }; if ($d.Camera) { 'Caméra ?' }; if ($d.Hidden) { 'Discret' })
        @{ type = 'ligne'; id = "net-app:$k"; icone = $s[0]; couleur = $(if ($d.New -or $d.Camera) { 'orange' } else { $s[1] }); texte = [string]$d.Title; note = "$($d.KindInfo.Kind)$(if ($d.Ip) { " · $($d.Ip)" })"
            etiquette = ($badges -join ' · '); ton = $(if ($d.New -or $d.Camera) { 'attention' } elseif ($d.Self -or $d.Gateway) { 'ok' } else { '' })
            valeur = $(if ($null -ne $d.Ms) { if ($d.Ms -lt 1) { '< 1 ms' } else { "$($d.Ms) ms" } }) }
    }
    $module = if ($liste.Count) {
        $ok = $liste.Count - $aVoir.Count
        @{ anneau = @{ valeur = $ok; sur = $liste.Count; petit = "/$($liste.Count)"; ton = $(if ($aVoir.Count) { 'attention' }) }
            titre = $(if ($aVoir.Count) { "$($aVoir.Count) appareil$(if ($aVoir.Count -gt 1) { 's' }) à vérifier" } else { 'Tout est en ordre' })
            texte = "$ok appareil$(if ($ok -gt 1) { 's' }) sur $($liste.Count) $(if ($ok -gt 1) { 'sont connus et répondent' } else { 'est connu et répond' }) normalement."
            boutons = @(@{ id = 'clic:BtnNetAudit'; texte = [string]$ui.BtnNetAudit.Content }) + @(if ($ui.BtnNetAuditView.Visibility -eq 'Visible') { @{ id = 'clic:BtnNetAuditView'; texte = 'Voir le rapport de l''audit'; style = 'second' } })
            resume = @(@{ nom = 'Box'; valeur = $(if ($box) { [string]$box.Title } else { '?' }) }, @{ nom = 'Nouveaux'; valeur = "$(@($liste | Where-Object { $_.New }).Count)"; ton = $(if (@($liste | Where-Object { $_.New }).Count) { 'attention' } else { 'ok' }) }) }
    } else { @{ icone = 'radar'; couleur = 'S'; titre = $(if ($scan) { 'Scan en cours' } else { 'Pas encore scanné' }); texte = 'Découvre tous les appareils connectés à ton réseau : téléphones, consoles, TV, box, objets connectés...'; boutons = @(if (-not $scan) { @{ id = 'clic:BtnNetScan'; texte = 'Scanner le réseau' } }) } }
    $blocs = @(
        if ($liste.Count) { @{ type = 'reseau'; centre = $(if ($box) { [string]$box.Title } else { 'Ta box' }); appareils = $planetes; legende = 'Plus un appareil tourne près de la box, plus il répond vite. Orange : à vérifier.' } }
        elseif ($scan) { @{ type = 'texte'; texte = 'Nevermind cherche les appareils de ton réseau, ça prend une à deux minutes.' } }
        @{ type = 'ligne'; icone = 'bouclier'; couleur = 'T'; texte = 'Audit de sécurité du réseau'; note = [string]$ui.NetAuditText.Text; etiquette = $(if ($audit) { "$(@($audit.Score)[0]) sur 100" }); ton = $(if ($audit) { $(if ([int]@($audit.Score)[0] -ge 85) { 'ok' } elseif ([int]@($audit.Score)[0] -ge 60) { 'attention' } else { 'urgent' }) }); bouton = @{ id = 'clic:BtnNetAudit'; texte = [string]$ui.BtnNetAudit.Content; style = 'second' } }
        if ($aVoir.Count) { @{ type = 'titre'; texte = 'À vérifier' } }
    )
    $k = 0
    $index = @{}; foreach ($d in $liste) { $index[[string]$d.Ip + '|' + [string]$d.Mac] = $k; $k++ }
    foreach ($d in $aVoir) { $blocs += & $ligneApp $d $index[[string]$d.Ip + '|' + [string]$d.Mac] }
    if ($liste.Count) {
        $blocs += @{ type = 'titre'; texte = 'Appareils'; droite = [string]$ui.NetDevSummary.Text }
        $choix = @(foreach ($f in $NetFilters) { $n = @($liste | Where-Object $f.Test).Count; if ($f.Id -eq 'all' -or $n) { @{ id = $f.Id; texte = "$($f.Label) ($n)" } } })
        if ($choix.Count -gt 1) { $blocs += @{ type = 'choix'; id = 'net-filtre'; valeur = $filtre; options = $choix } }
        foreach ($d in $montres) { $blocs += & $ligneApp $d $index[[string]$d.Ip + '|' + [string]$d.Mac] }
    }
    $blocs += @{ type = 'reglages'; rangs = @(@{ texte = 'Me prévenir quand un nouvel appareil se connecte'; note = 'Vérifié toutes les 10 minutes, tant que Nevermind est ouvert.'; bascule = @{ id = 'coche:ChkNetWatch'; oui = [bool]$ui.ChkNetWatch.IsChecked } }) }
    if ($liste.Count -and $ui.NetDevHint.Text) { $blocs += @{ type = 'note'; texte = [string]$ui.NetDevHint.Text } }
    [ordered]@{
        reseau = @{ titre = 'Réseau'; sous = 'Tous les appareils connectés chez toi : téléphones, consoles, TV, box, objets connectés.'; icone = 'radar'; couleur = 'S'
            actions = @(if ($liste.Count) { @{ id = 'clic:BtnNetScan'; texte = $(if ($scan) { 'Scan en cours...' } else { 'Scanner de nouveau' }); icone = 'actu' } })   # (avant le premier scan : le bouton de la carte seulement)
            module = $module; blocs = $blocs }
    }
}

function Register-ActionsReseau {
    Register-ActionVue 'net-filtre' { param($Valeur, $Ecran) $script:Kit.ReseauFiltre = [string]$Valeur }
}
function Invoke-ActionReseau([string]$Id, $Valeur) {
    if ($Id -match '^net-app:(\d+)$') { $d = @($script:Kit.Appareils)[[int]$Matches[1]]; if ($d) { Invoke-Safe { Show-DeviceDetail $d } }; return $true }
    $false
}
