# Nevermind : les pages de l'écran (kit) de « Protection » : Protection, Sécurité, Connexion et Sauvegarde.
# Les données viennent des mêmes fonctions que l'interface d'avant (Update-SecurityTab garde sa dernière vérification
# dans $script:SecDonnees ; Invoke-NetTest, Get-ActiveNet, l'historique des changements) ; les gestes aussi.
# Chargé par Nevermind.ps1 après ecran-perf.ps1.

function Get-EtatSecuriteKit {
    $d = $script:SecDonnees
    $l = @()
    if (-not $d) { return }
    $S = $d.S; $mp = $S.Mp
    if ($mp -and $mp.AMRunningMode -eq 'Normal') {
        $l += @{ nom = 'Antivirus'; valeur = 'Microsoft Defender'; ton = 'ok' }
        $l += @{ nom = 'Protection en direct'; valeur = $(if ($mp.RealTimeProtectionEnabled) { 'Activée' } else { 'Désactivée' }); ton = $(if ($mp.RealTimeProtectionEnabled) { 'ok' } else { 'urgent' }) }
        $age = [int]$mp.AntivirusSignatureAge
        $l += @{ nom = 'Base de virus'; valeur = $(if ($age -le 0) { 'À jour' } else { "Il y a $age jour$(if ($age -gt 1) { 's' })" }); ton = $(if ($age -le 3) { 'ok' } else { 'attention' }) }
        $l += @{ nom = 'Dernière analyse'; valeur = $(if ($mp.QuickScanEndTime) { $mp.QuickScanEndTime.ToString('dd/MM à HH:mm') } else { 'Jamais' }); ton = $(if ($mp.QuickScanAge -le 14) { 'ok' } else { 'attention' }) }
    } elseif ($S.OtherAv.Count) {
        $l += @{ nom = 'Antivirus'; valeur = ($S.OtherAv -join ', '); ton = 'ok' }
    } else { $l += @{ nom = 'Antivirus'; valeur = 'Aucun'; ton = 'urgent' } }
    $l += @{ nom = 'Pare-feu'; valeur = $(if ($S.Firewall) { 'Activé' } else { 'Désactivé' }); ton = $(if ($S.Firewall) { 'ok' } else { 'urgent' }) }
    $l += @{ nom = 'Contrôle des comptes'; valeur = $(if ($S.Uac) { 'Activé' } else { 'Désactivé' }); ton = $(if ($S.Uac) { 'ok' } else { 'urgent' }) }
    $l
}

function Get-PagesProtection {
    $d = $script:SecDonnees
    $sec = $script:SecurityScore
    $etat = @(Get-EtatSecuriteKit)
    $checks = New-Object System.Collections.ArrayList
    if ($d) {
        if ($d.S.Active.Count) {
            [void]$checks.Add(@{ Status = 'bad'; Title = "$($d.S.Active.Count) menace$(if ($d.S.Active.Count -gt 1) { 's' }) toujours active$(if ($d.S.Active.Count -gt 1) { 's' })"; Detail = 'Microsoft Defender a trouvé des virus qui ne sont pas encore supprimés.'
                Items = @($d.S.Active | ForEach-Object { $_.ThreatName }); Actions = @(@{ Label = 'Supprimer les menaces'; Confirm = 'Supprimer toutes les menaces actives trouvées par Microsoft Defender ?'; Script = { Remove-MpThreat -ErrorAction Stop } }) })
        }
        foreach ($c in @($d.Checks | Sort-Object @{ Expression = { if ($_.Status -eq 'bad') { 0 } else { 1 } } })) { [void]$checks.Add($c) }
    }
    $script:Kit.SecChecks = @($checks)
    $defenderOk = $d -and $d.S.Mp -and $d.S.Mp.AMRunningMode -eq 'Normal'
    $titreSec = if ($null -eq $sec) { 'Vérification en cours' } elseif ($sec -ge 80) { 'Bien protégé' } elseif ($sec -ge 50) { 'Quelques points à voir' } else { 'Vérifie ta protection' }
    $texteSec = if ($checks.Count) { "$($checks.Count) point$(if ($checks.Count -gt 1) { 's' }) à vérifier." } elseif ($d) { 'Rien de suspect sur ton PC.' } else { 'Antivirus, programmes cachés, réseau, sauvegarde.' }
    $analyses = @(if ($defenderOk) { @{ id = 'clic:BtnScanQuick'; texte = 'Analyse rapide' }; @{ id = 'clic:BtnScanFull'; texte = 'Analyse complète' } })
    # (la carte résume le niveau ; le détail de chaque protection est à droite, sans doublon)
    $module = if ($null -ne $sec) { @{ anneau = @{ valeur = [int]$sec; sur = 100; ton = $(if ($sec -lt 50) { 'urgent' } elseif ($sec -lt 80) { 'attention' }) }; titre = $titreSec; texte = $texteSec; boutons = $analyses } }
        else { @{ icone = 'bouclier'; couleur = 'T'; titre = $titreSec; texte = 'Nevermind vérifie ta protection, ça prend quelques secondes.' } }

    # Connexion
    $net = $script:Net
    $pings = @($script:PingResults | Where-Object { $_ })
    $inter = @($pings | Where-Object { $_.Label -like 'Internet*' } | Select-Object -First 1)
    # Sauvegarde
    $nb = Get-BackupCount
    if ($null -eq $script:History) { try { Import-History } catch {} }
    $hist = @($script:History | Where-Object { $_ } | Select-Object -First 20)

    $pages = [ordered]@{}
    $pages.protection = @{ titre = 'Protection'; sous = 'Antivirus, programmes cachés, réseau, sauvegarde.'; icone = 'bouclier'; couleur = 'T'
        actions = @(@{ id = 'clic:BtnSecRefresh'; texte = 'Revérifier'; icone = 'actu' }); module = $module
        blocs = @(
            @{ type = 'outils'; elements = @(
                @{ id = 'securite'; nom = 'Sécurité'; note = 'Virus et programmes dangereux'; icone = 'bouclier'; couleur = 'T'; etiquette = $(if ($checks.Count) { "$($checks.Count) point$(if ($checks.Count -gt 1) { 's' })" } elseif ($d) { 'Tout est propre' }); ton = $(if (@($checks | Where-Object { $_.Status -eq 'bad' }).Count) { 'urgent' } elseif ($checks.Count) { 'attention' } else { 'ok' }) }
                @{ id = 'connexion'; nom = 'Connexion'; note = 'Temps de réponse (ping), stabilité, DNS'; icone = 'ondes'; couleur = 'P'; etiquette = $(if ($inter.Count) { "$($inter[0].Avg) ms" } else { 'À tester' }); ton = $(if ($inter.Count) { $TonsStatut[[string]$inter[0].Status] } else { '' }) }
                @{ id = 'sauvegarde'; nom = 'Sauvegarde'; note = 'Revenir en arrière à tout moment'; icone = 'sauve'; couleur = 'S'; etiquette = $(if ($nb) { "$nb réglage$(if ($nb -gt 1) { 's' })" } else { 'Rien de modifié' }); ton = 'ok' }
            ) }
            if ($checks.Count) { @{ type = 'titre'; texte = 'Points à vérifier' } }
        ) + @(foreach ($c in @($checks | Select-Object -First 4)) { @{ type = 'ligne'; id = 'securite'; icone = 'alerte'; couleur = $(if ($c.Status -eq 'bad') { 'rouge' } else { 'orange' }); texte = [string]$c.Title; note = [string]$c.Detail; etiquette = $(if ($c.Status -eq 'bad') { 'Important' } else { 'À voir' }); ton = $TonsStatut[[string]$c.Status] } }) + @(
            if ($etat.Count) { @{ type = 'titre'; texte = 'Ta protection' } }
        ) + @(foreach ($e in $etat) { @{ type = 'ligne'; id = 'securite'; icone = $(if ($e.ton -eq 'ok') { 'ok' } else { 'alerte' }); couleur = $(switch ([string]$e.ton) { 'ok' { 'vert' } 'urgent' { 'rouge' } 'attention' { 'orange' } default { 'P' } }); texte = [string]$e.nom; valeur = [string]$e.valeur; ton = [string]$e.ton } }) }

    # ---- Sécurité
    $lignesSec = New-Object System.Collections.ArrayList
    for ($k = 0; $k -lt $checks.Count; $k++) {
        $c = $checks[$k]
        [void]$lignesSec.Add(@{ type = 'ligne'; icone = 'alerte'; couleur = $(if ($c.Status -eq 'bad') { 'rouge' } else { 'orange' }); texte = [string]$c.Title; note = [string]$c.Detail; etiquette = $(if ($c.Status -eq 'bad') { 'Important' } else { 'À voir' }); ton = $TonsStatut[[string]$c.Status] })
        if ($c.Items) {
            $it = @($c.Items | Select-Object -First 6 | ForEach-Object { [string]$_ })
            if (@($c.Items).Count -gt 6) { $reste = @($c.Items).Count - 6; $it += "et $reste autre$(if ($reste -gt 1) { 's' })..." }
            [void]$lignesSec.Add(@{ type = 'liste'; elements = $it })
        }
        for ($j = 0; $j -lt @($c.Actions).Count; $j++) { [void]$lignesSec.Add(@{ type = 'bouton'; id = "sec:$k`:$j"; texte = [string]$c.Actions[$j].Label; style = $(if ($j) { 'second' }) }) }
        if (@($c.Trust).Count) { [void]$lignesSec.Add(@{ type = 'bouton'; id = "sec-moi:$k"; texte = $(if (@($c.Trust).Count -gt 1) { 'C''est moi, tout est OK' } else { 'C''est moi, c''est OK' }); style = 'second' }) }
    }
    $approuves = @(Get-SecTrusted)
    $script:Kit.SecApprouves = $approuves
    $menaces = @(if ($d) { $d.Hist })
    $ongletsSec = @(
        @{ id = 'points'; nom = "Points à vérifier ($($checks.Count))"; blocs = $(if ($lignesSec.Count) { @($lignesSec) + @(@{ type = 'note'; texte = 'Les points à vérifier sont des indices, pas des preuves : un programme non signé n''est pas forcément un virus. En cas de doute, lance une analyse.' }) } else { @(@{ type = 'pastilles'; elements = @(@{ texte = 'Rien de suspect'; ton = 'ok' }) }, @{ type = 'texte'; texte = 'Pas de fichier déguisé, pas de programme caché, pas de tâche douteuse, pas de redirection.' }) }) }
        @{ id = 'etat'; nom = 'État'; blocs = @(@{ type = 'reglages'; rangs = @($etat | ForEach-Object { @{ texte = $_.nom; valeur = $_.valeur } }) }) + @(if ($defenderOk) { @{ type = 'bouton'; id = 'clic:BtnScanFolder'; texte = 'Analyser un dossier'; style = 'second' }; @{ type = 'bouton'; id = 'clic:BtnScanUpdate'; texte = 'Mettre à jour la base de virus'; style = 'second' } }) + @(@{ type = 'note'; texte = [string]$ui.SecScanNote.Text }) }
        @{ id = 'menaces'; nom = "Menaces récentes ($($menaces.Count))"; blocs = $(if ($menaces.Count) { @(foreach ($h in $menaces) { @{ type = 'ligne'; icone = 'alerte'; couleur = $(if ($h.Active) { 'rouge' } else { 'doux' }); texte = [string]$h.Name; note = "$(try { $h.When.ToString('dd/MM/yyyy HH:mm') } catch { '' })   $($h.File)"; etiquette = [string]$h.State; ton = $(if ($h.Active) { 'urgent' } else { 'ok' }) } }) } else { @(@{ type = 'texte'; texte = 'Aucune menace trouvée sur ce PC récemment.' }) }) }
    )
    if ($approuves.Count) {
        $ongletsSec += @{ id = 'approuves'; nom = "Approuvés par toi ($($approuves.Count))"; blocs = @(@{ type = 'texte'; texte = 'Tu as indiqué que c''est toi qui les as mis en place : ils ne baissent plus le niveau de protection. Retire-les si tu changes d''avis.' }) + @(for ($k = 0; $k -lt $approuves.Count; $k++) { $t = $approuves[$k]; @{ type = 'ligne'; icone = 'ok'; couleur = 'vert'; texte = [string]$t.Text; note = [string]$t.Kind; bouton = @{ id = "sec-retirer:$k"; texte = 'Retirer'; style = 'second' } } }) }
    }
    $pages.securite = @{ titre = 'Sécurité'; parent = 'protection'; sous = 'Recherche des virus et de tout ce qui pourrait être dangereux sur ton PC.'; icone = 'bouclier'; couleur = 'T'
        actions = @(@{ id = 'clic:BtnSecRefresh'; texte = 'Revérifier'; icone = 'actu' })
        module = $(if ($null -ne $sec) { @{ anneau = $module.anneau; titre = $titreSec; texte = $texteSec; boutons = @($analyses) } } else { $module })
        onglets = $ongletsSec }

    # ---- Connexion
    $choixDns = 0
    if ($net -and $net.Dns) { $dns = [string]$net.Dns; for ($k = 1; $k -lt $DnsChoices.Count; $k++) { if ($dns -match [regex]::Escape($DnsChoices[$k][0])) { $choixDns = $k } } }
    $infos = @(if ($net) {
        @{ texte = 'Type'; valeur = $(if ($net.Wifi) { 'Wi-Fi' } else { 'Câble (Ethernet)' }) }
        @{ texte = 'Carte'; valeur = [string]$net.Desc }
        @{ texte = 'Vitesse'; valeur = [string]$net.Speed }
        @{ texte = 'Box'; valeur = [string]$net.Gateway }
        if ($net.Dns) { @{ texte = 'DNS actuel'; valeur = [string]$net.Dns } }
    })
    $pages.connexion = @{ titre = 'Connexion'; parent = 'protection'; sous = 'Mesure ton temps de réponse (ping), sa stabilité (gigue) et les messages perdus en route.'; icone = 'ondes'; couleur = 'P'
        module = $(if ($inter.Count) { $p = $inter[0]
                @{ icone = 'ondes'; couleur = $(switch ([string]$p.Status) { 'ok' { 'vert' } 'warn' { 'orange' } 'bad' { 'rouge' } default { 'P' } }); titre = "Ping $($p.Avg) ms"; texte = [string]$p.Verdict; boutons = @(@{ id = 'clic:BtnPing'; texte = 'Tester ma connexion' }); resume = @(@{ nom = 'Gigue'; valeur = "$($p.Jitter) ms" }, @{ nom = 'Pertes'; valeur = "$($p.Loss) %"; ton = $(if ($p.Loss -gt 0) { 'attention' } else { 'ok' }) }) } }
            elseif (-not $net) { @{ icone = 'ondes'; couleur = 'rouge'; titre = 'Pas de connexion'; texte = 'Aucune connexion Internet détectée.'; boutons = @(@{ id = 'clic:BtnPing'; texte = 'Réessayer' }) } }
            else { @{ icone = 'ondes'; couleur = 'P'; titre = 'Pas encore testée'; texte = 'Le test envoie quelques petits messages à ta box et à Internet pendant une dizaine de secondes.'; boutons = @(@{ id = 'clic:BtnPing'; texte = 'Tester ma connexion' }) } })
        blocs = @(
            if ($pings.Count) { @{ type = 'titre'; texte = 'Dernier test' } }
        ) + @(foreach ($p in $pings) { @{ type = 'ligne'; icone = $(if ($p.Label -like 'Ta box*') { 'box' } else { 'internet' }); couleur = $(switch ([string]$p.Status) { 'ok' { 'vert' } 'warn' { 'orange' } 'bad' { 'rouge' } default { 'P' } }); texte = [string]$p.Label; note = [string]$p.Verdict; valeur = "$($p.Avg) ms"; etiquette = "gigue $($p.Jitter) ms · pertes $($p.Loss) %"; ton = $TonsStatut[[string]$p.Status] } }) + @(
            if ($infos.Count) { @{ type = 'titre'; texte = 'Ta connexion' }; @{ type = 'reglages'; rangs = $infos } }
            @{ type = 'titre'; texte = 'Serveur DNS' }
            @{ type = 'choix'; id = 'dns'; valeur = "$choixDns"; options = @(@{ id = '0'; texte = 'Automatique (celui de ta box)' }, @{ id = '1'; texte = 'Cloudflare' }, @{ id = '2'; texte = 'Google' }, @{ id = '3'; texte = 'Quad9' }) }
            @{ type = 'bouton'; id = 'clic:BtnDnsFlush'; texte = 'Vider le cache DNS'; style = 'second' }
            @{ type = 'note'; texte = 'Bon à savoir : le DNS ne change pas ton ping en jeu, il sert seulement à trouver l''adresse des serveurs. Un DNS rapide accélère un peu la navigation et la connexion des lanceurs de jeux (Steam, Epic...).' }
        ) }

    # ---- Sauvegarde
    $lignesH = @(foreach ($h in $hist) {
        $dt = [string]$h.Date; $jour = $dt; $heure = ''
        if ($dt -match '^(\d{2}/\d{2})/\d{4} à (\d{2}:\d{2})$') { $jour = $Matches[1]; $heure = $Matches[2] }
        $l = @{ type = 'ligne'; quand = @($jour, $heure); texte = [string]$h.Titre; note = (@($h.Details) -join ', '); fait = [bool]$h.Annule }
        if ($h.Annule) { $l.etiquette = 'Annulé' }
        elseif (Test-HistoriqueNote $h) { $l.etiquette = 'Noté' }   # (une appli fermée : rien à remettre)
        elseif (Test-HistoriqueDefinitif $h) { $l.etiquette = 'Définitif'; $l.note = (@(@($h.Details) -join ', ') + "Pour revenir en arrière : $(Get-RetourDefinitif $h)" | Where-Object { $_ }) -join '. ' }
        else { $l.bouton = @{ id = "hist:$($h.Id)"; texte = 'Annuler'; style = 'second' } }
        $l
        $tech = @(Get-DetailsTechniques $h)
        if ($tech.Count) { @{ type = 'faq'; elements = @(@{ question = 'Détails techniques'; reponse = ($tech -join ' ; '); icone = 'liste' }) } }
    })
    $pages.sauvegarde = @{ titre = 'Sauvegarde et rapport'; parent = 'protection'; sous = 'Reviens en arrière à tout moment, ou partage l''état de ton PC.'; icone = 'sauve'; couleur = 'S'
        module = @{ icone = 'sauve'; couleur = 'S'; titre = $(if ($nb) { "$nb réglage$(if ($nb -gt 1) { 's' }) modifié$(if ($nb -gt 1) { 's' })" } else { 'Rien de modifié' }); texte = $(if ($nb) { 'Tout ce que Nevermind a changé est sauvegardé avant d''être fait. « Tout restaurer » remet tout comme avant, sauf les protections du PC marquées « Définitif » dans l''historique, qui restent en place.' } else { 'Nevermind n''a encore rien modifié sur ce PC.' }); boutons = @(if ($nb) { @{ id = 'clic:BtnUndo'; texte = 'Tout restaurer' } }) }
        blocs = @(@{ type = 'titre'; texte = 'Historique des changements'; droite = 'Du plus récent au plus ancien' }) + $(if ($lignesH.Count) { $lignesH } else { @(@{ type = 'texte'; texte = 'Aucun changement pour le moment.' }) }) + @(
            @{ type = 'titre'; texte = 'Aussi' }
            @{ type = 'ligne'; icone = 'bouclier'; couleur = 'T'; texte = 'Point de restauration Windows'; note = 'Une photo complète des réglages de Windows. En cas de souci, la restauration du système te ramène à cet état.'; bouton = @{ id = 'clic:BtnRestorePoint'; texte = 'Créer'; style = 'second' } }
            @{ type = 'ligne'; icone = 'sauve'; couleur = 'S'; texte = 'Restauration du système'; note = 'Revenir à un point de restauration (outil de Windows).'; bouton = @{ id = 'clic:BtnOpenRestore'; texte = 'Ouvrir'; style = 'second' } }
            @{ type = 'ligne'; icone = 'rapport'; couleur = 'P'; texte = 'Rapport de ton PC'; note = 'Une page web avec ta configuration, ton score et les points à améliorer.'; bouton = @{ id = 'clic:BtnExport'; texte = 'Exporter'; style = 'second' } }
        ) }
    $pages
}

function Register-ActionsProtection {
    Register-ActionVue 'dns' { param($Valeur, $Ecran) $k = [int]$Valeur; if ($k -ge 0 -and $k -lt $DnsChoices.Count) { Invoke-Safe { Set-Dns $k } } }
}

# Les gestes à numéro (sécurité, historique) : appelés par l'action « * » de accueil-console.ps1
function Invoke-ActionProtection([string]$Id, $Valeur) {
    if ($Id -match '^sec:(\d+):(\d+)$') {
        $c = @($script:Kit.SecChecks)[[int]$Matches[1]]; if (-not $c) { return $true }
        $act = @($c.Actions)[[int]$Matches[2]]; if (-not $act) { return $true }
        if ($act.Confirm -and -not (Confirm-Action $act.Confirm)) { return $true }
        $ttl = [string]$c.Title
        Invoke-Safe {
            $script:RunLog = New-Object System.Collections.ArrayList
            try { & $act.Script $act.Arg } finally { $log = $script:RunLog; $script:RunLog = $null }
            [void](Add-History "$($act.Label) ($ttl)" @() $log)
            if ($act.After) { & $act.After } elseif (-not $act.NoRefresh -and -not $script:ScanRunning) { Update-SecurityTab }
        }
        return $true
    }
    if ($Id -match '^sec-moi:(\d+)$') {
        $c = @($script:Kit.SecChecks)[[int]$Matches[1]]; if (-not $c -or -not @($c.Trust).Count) { return $true }
        $n = @($c.Trust).Count
        if (-not (Confirm-Action $(if ($n -gt 1) { "Marquer ces $n éléments comme sûrs ? Ils ne compteront plus dans le niveau de protection. S'ils changent (autre commande, autre fichier), Nevermind les signalera de nouveau." } else { 'Marquer cet élément comme sûr ? Il ne comptera plus dans le niveau de protection. S''il change (autre commande, autre fichier), Nevermind le signalera de nouveau.' }))) { return $true }
        Invoke-Safe { Add-SecTrusted @($c.Trust); if (-not $script:ScanRunning) { Update-SecurityTab } }
        return $true
    }
    if ($Id -match '^sec-retirer:(\d+)$') {
        $t = @($script:Kit.SecApprouves)[[int]$Matches[1]]
        if ($t) { $cle = [string]$t.Key; Invoke-Safe { Remove-SecTrusted $cle; if (-not $script:ScanRunning) { Update-SecurityTab } } }
        return $true
    }
    if ($Id -match '^hist:([0-9a-f]{6,40})$') { $h = $Matches[1]; Invoke-Safe { Undo-HistoryEntry $h }; return $true }
    $false
}
