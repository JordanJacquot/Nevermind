# Nevermind : les pages de l'écran (kit) des petites icônes du haut : Rechercher (Paramètres et Aide suivront).
# Chargé par OptiGame.ps1 après ecran-protection.ps1.

function Get-PagesDivers {
    $res = @($script:Kit.Resultats)
    $q = [string]$script:Kit.Recherche
    [ordered]@{
        recherche = @{ titre = 'Rechercher'; sous = 'Un réglage, un outil, un jeu.'; icone = 'loupe'; couleur = 'S'
            module = @{ icone = 'loupe'; couleur = 'S'; titre = 'Que cherches-tu ?'; texte = 'Tape un mot : Nevermind te montre où c''est et t''y emmène. Raccourci : Ctrl + K.' }
            blocs = @(
                @{ type = 'champ'; id = 'recherche'; recherche = $true; valeur = $q; indice = 'Par exemple : mode jeu' }
                if ($q) { @{ type = 'titre'; texte = $(if ($res.Count) { "$($res.Count) résultat$(if ($res.Count -gt 1) { 's' })" } else { 'Rien trouvé' }) } }
            ) + @(for ($k = 0; $k -lt $res.Count; $k++) { @{ type = 'ligne'; id = "res:$k"; icone = 'loupe'; couleur = 'S'; texte = [string]$res[$k].T; note = [string]$res[$k].Where; valeur = 'Y aller' } }) + @(
                if ($q -and -not $res.Count) { @{ type = 'texte'; texte = 'Essaie avec un autre mot.' } }
            ) }
    }
}
