# Bloc Réseau : différences avec la version PowerShell

La référence est la version PowerShell (reseau.ps1, reseau-avance.ps1, audit-reseau.ps1, ecran-reseau.ps1, la partie
Connexion d'ecran-protection.ps1, onglets.ps1, systeme.ps1). Les juges (`tests/juges`, exportés par
`outils\exporter-juges-reseau.ps1`) montrent que les pages, l'audit, le scan, la carte bridée, la surveillance et le
test du ping donnent la même chose, au champ près. Ce qui suit est différent, et pourquoi.

## Reproduit tel quel (défauts de la version PowerShell, à corriger des deux côtés plus tard)

1. **« Trouvé grâce à » incomplet.** Dans Merge-NetDeep, `@($(if ...), $(if ...), $(if ...), $(if ...)) | Where-Object { $_ }`
   s'arrête au deuxième `$(if ...)` vide (PowerShell 5.1). Preuve : la table des 16 cas mesurée le 06/11
   (`0011 -> rien`, `0101 -> u`, `1101 -> m,u,w`...). Un appareil vu seulement par NetBIOS garde « réponse au ping ».
   Reproduit dans `scan::fusionner` pour garder les mêmes fiches.
2. **Panneau d'un port sans bouton « Ouvrir ».** La traduction WPF vers l'écran (Add-BlocsWpf) ne garde que le premier
   texte de plus de 3 lettres de l'encadré : le numéro du port s'il a 4 chiffres, sinon le nom du service. Reproduit.

## Différences voulues

3. **Ordre des éléments de quatre points de l'audit** (Telnet, FTP, prise de contrôle, flux vidéo). PowerShell les
   prend dans l'ordre d'une table (`$open.Keys`), qui n'est pas fixé ; Rust les donne dans l'ordre où l'audit les a
   trouvés. Les juges comparent ces listes triées.
4. **Appareils vus seulement en IPv6** : à égalité dans le tri (`[version]` impossible), PowerShell les range dans un
   ordre non fixé (Sort-Object) ; Rust garde leur ordre d'arrivée. Les juges les rangent par adresse des deux côtés.
5. **Panneaux (fiche d'un appareil, audit) écrits directement** au lieu d'être traduits depuis WPF : ils suivent les
   règles d'Add-BlocsWpf (titres « ●  IDENTITÉ », lignes nom/valeur, verdict en ligne colorée, jauge en chiffre,
   courbe) mais ne sont pas jugés (il faudrait dessiner le WPF). Le pas à pas de l'audit en cours est remplacé par une
   ligne de progression. Les cartes des points de l'audit sont une ligne, la liste des éléments (5 au plus) et leurs
   boutons.
6. **Ping en direct de la fiche** : seulement quand l'écran est regardé (`cache = false`) ; PowerShell continuait
   derrière un jeu. Plus léger, rien de visible ne change.
7. **Surveillance (toutes les 10 minutes)** : rien pendant une partie ni quand l'écran est caché (règle du
   coordinateur du 06/11). PowerShell la lançait quand même.
8. **« Exporter le rapport »** : pas de fenêtre « Enregistrer sous » (pas de WPF) ; le rapport est écrit sur le Bureau
   (« Audit réseau Nevermind <date>.html ») puis ouvert.
9. **audit.json et appareils.json** : même contenu, mais écrits par serde_json (indentation différente de
   ConvertTo-Json). Les deux versions relisent les fichiers de l'autre.
10. **Recherche approfondie** : mDNS (Bonjour), WS-Discovery et titres des pages en https ne sont pas encore portés
    (la vraie sonde rend des listes vides pour eux). La logique de fusion, elle, est portée et jugée avec ces données.
11. **Audit, ce PC** : SMBv1 serveur lu dans le registre (valeur `SMB1`) au lieu de Get-SmbServerConfiguration ;
    catégorie du réseau (privé, public) et droits « Tout le monde » des partages pas encore lus (vides). Adresse
    Internet publique (ipify, https) pas lue : elle ne servait qu'à la carte WPF.
12. **« Passer en public »** : Set-NetConnectionProfile lancé dans un PowerShell sans fenêtre, comme la version
    PowerShell le faisait dans son propre processus (Windows peut refuser sans droits administrateur).
