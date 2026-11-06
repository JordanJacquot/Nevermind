# Bloc « trafic » : différences voulues avec la version PowerShell

Chaque différence est voulue, avec sa preuve. Tout le reste est vérifié par les juges (`tests/juges`, exportés par
`outils\exporter-juges-trafic.ps1`).

## Défauts corrigés

1. **Pics de ping et téléchargements : le bon programme nommé** (`Get-LagSpikeLinks`). En PowerShell,
   `[Array]::Sort($tf, $fond)` trie les dates mais pas les relevés (PowerShell passe une copie de `$fond`) : le
   programme nommé dans « Un téléchargement sature ta connexion » est celui du relevé au même rang dans l'ordre
   d'origine, souvent un autre. Preuve : 26 cas sur 120 du juge `Get-LagSpikeLinks` ne s'expliquent qu'ainsi ;
   `stats::liens_comme_powershell` (le défaut reproduit) donne les 120 cas exacts, et la version corrigée donne les
   mêmes nombres de pics (`Local`, `BgHit`) sur les 120 (test `petites_fonctions`).
2. **Débit d'un programme** : divisé par le vrai temps entre deux passages (PowerShell divisait toujours par 2 s).
   Identique à 2 s ; juste quand la surveillance ralentit (voir Légèreté).

## Légèreté (le but de la nuit)

3. **Rythme de la surveillance du Trafic** : PowerShell relevait toutes les 2 s tant qu'elle était démarrée, même
   fenêtre cachée et en pleine partie (noté le 29/10). Ici : 2 s quand la page Trafic (ou le détail d'un programme)
   est regardée, 5 s sur une autre page, 10 s écran caché, 30 s pendant une partie. Les octets des connexions fermées
   entre deux passages ne sont pas comptés, comme avant entre deux passages de 2 s.
4. **Pas de relevé sans surveillance** : rien ne tourne tant que l'utilisateur ne l'a pas démarrée (comme
   PowerShell dans l'écran kit) ; la mesure du lag ne tourne que pendant une partie ou le test rapide.

## Écran

5. **Le graphique du ping dans le détail d'une mesure** : PowerShell l'envoyait en image (dessin WPF rendu en PNG).
   Ici une courbe de l'écran (`type: courbe`) par étape (box, Internet, serveur), les pertes à 0 ; la légende reste
   la même. Pas de WPF en Rust.
6. **Les panneaux** (détail d'une mesure, d'un programme, « Ce que Windows envoie à Microsoft ») sont construits
   directement en blocs du kit, en suivant pas à pas la traduction de `Add-BlocsWpf` (ecran-panneau.ps1) : titres
   « ●  LE PROGRAMME », lignes de tableau « nom / note / valeur », encadrés à un seul texte en `note`, boutons
   `panneau:b:N`. Pas de juge exporté pour eux (il faudrait la fenêtre WPF) : vérifiés par les tests du bloc.
7. **Ordre à égalité** : à gravité égale, PowerShell range les alertes (et les ports d'une alerte) dans l'ordre de
   sa table de hachage, au hasard des clés ; ici l'ordre où les programmes et les destinations ont été vus. Les juges
   comparent donc ces deux listes sans tenir compte de cet ordre (test `releves_et_page_trafic`).
8. **`{}` gardé** : `$(if ($faux) { ... })` dans une table donne `{}` dans le JSON (style d'un bouton, image d'un
   programme sans emplacement). Gardé tel quel pour des pages identiques au champ près ; le kit le prend comme absent.
9. **Icône des programmes** : `Get-IconeExeKit` lit le cache d'icônes d'un autre bloc (`$script:ExeFacts`) : ici
   toujours `null` (comme PowerShell quand l'icône n'a pas encore été lue).
10. **Les questions, bulles et écritures** passent par des signaux (le noyau n'a pas d'effet pour elles) :
    `question` (signal commun), `notification` (bulle de Windows), `historique` (signal commun),
    `sauvegarde-appliquer` (Set-Reg / Set-ServiceOff notés pour « Revenir en arrière »), `analyse-demandee`
    (analyse Defender), `veille-jeu-ajouter`. Voir le rapport.

## Windows

11. **Signatures** : `WinVerifyTrust` (fichier, puis catalogue de Windows), comme `Get-AuthenticodeSignature` ;
    la vérification des applis du Microsoft Store par leur paquet (`Get-AppxPackage`) n'est pas faite : une appli
    du Store non signée elle-même reste « Non signé ».
12. **Pings IPv6** : non envoyés (cible notée perdue) ; PowerShell (`Ping.Send`) savait. Les serveurs de jeu vus
    étaient en IPv4.
13. **Trace réseau sans l'Aide** : la trace « sur place » en administrateur (`[NetFlow]`) n'existe pas ici : sans
    l'Aide, seul le serveur de connexion du jeu (TCP) est vu, comme quand Windows refuse la trace.

## Fermeture après un blocage (ajout du coordinateur)

Stop-ProgrammeChemin ferme d'abord lui-même les processus permis, puis demande « processus.fermer » {Chemin} à l'Aide pour ceux qu'il ne peut pas lire. Ici, tout passe par « processus.fermer » {Chemin} à l'Aide, qui applique la même règle commune (aide-regles).
