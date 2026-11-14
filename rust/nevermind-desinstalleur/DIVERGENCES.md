# nevermind-desinstalleur : ce qui diffère de la version PowerShell

Référence : `Invoke-Uninstall`, `Remove-DonneesDesinstallation`, `Get-ChoixDonneesImpose`, `Show-ChoixDesinstallation`
(systeme.ps1), la relance en administrateur (Nevermind.ps1:330-350), la voie « sur place » de l'Aide (aide-client.ps1,
`SurPlace`) et le lanceur C# compilé avec UNINSTALL (outils\lanceur.cs:149-166).

Juges : `outils\exporter-juges-desinstalleur.ps1` (« tout optimiser puis désinstaller » : 240 PC simulés, « Tout
corriger » fait par la version PowerShell, puis `Invoke-Uninstall` lu dans systeme.ps1, contre le faux PC et la fausse
Aide de exporter-juges-pc.ps1). `cargo test -p nevermind-desinstalleur` rejoue chaque cas : la même suite de gestes
**dans le même ordre** (6 855 gestes : demandes à l'Aide, registre de l'utilisateur, powercfg, écrans, curseur, souris,
tâches, programmes fermés, clé de l'Aide, raccourcis, la commande qui supprime les fichiers), la même question, le même
message de fin, le même journal, les mêmes fichiers restants dans les données, les mêmes réglages notés et le même
registre. Tout est identique, sauf ce qui suit.

## Différences voulues

1. **La question** : la version PowerShell dessinait sa fenêtre en WPF (fond sombre). Ici, la fenêtre de question de
   Windows (TaskDialog) : même titre (« Désinstaller Nevermind »), même titre principal, même texte, même case décochée
   au départ (« Supprimer aussi mes données (préférences, jeux, mesures, journal) »), même note dessous qui change quand
   on coche la case (avec une icône d'avertissement), mêmes boutons, Annuler choisi par défaut, fermer la fenêtre =
   Annuler. Pas de WebView2 ni de Tauri pour une seule question : le désinstalleur reste petit et ne dépend pas de l'appli.
   Sans les contrôles modernes de Windows (manifeste absent), une question plus simple (OK / Annuler, données gardées).
2. **La relance en administrateur** attend la fin de la copie relancée et rend son code (PowerShell rendait la main
   tout de suite) : celui qui l'a lancé (Mindset) sait quand c'est fini. Demande de Windows refusée, ou variable
   dangereuse dans le compte : le même message, et le code 1 (au lieu de 0).
3. **Nevermind ouvert depuis ce dossier** (Mindset.exe ou Nevermind.exe de cette copie, reconnus par leur chemin) est
   fermé juste après l'Aide : l'appli Rust tiendrait ses programmes et ses données ouverts, et Mindset compte sur le
   désinstalleur pour fermer l'hôte de l'appli (`Start-DesinstallAppli`, installations.ps1:866-868). Jamais
   ce programme lui-même, jamais un programme d'un autre dossier (test `nevermind_ouvert_est_ferme_avec_l_aide`). Les
   juges n'ont pas de tel programme ouvert : PowerShell ne le faisait pas.
4. **Les fichiers** : même liste que la version PowerShell, et sa commande (`ping ... & del ... & rmdir ...`) reste
   celle que les juges comparent caractère par caractère. Mais le vrai lancement (lot H, point 21) déplace d'abord tout
   dans un dossier « à effacer » neuf (Nevermind.exe disparaît donc aussitôt de sa place, et Mindset le voit), puis
   cmd.exe caché efface ce dossier 3 secondes après la fermeture, avec ce qui n'a pas pu être déplacé et les dossiers
   devenus vides.
5. **« Supprimer mes données »** ne suit jamais un lien : un point de jonction ou un lien symbolique posé dans les
   données est retiré seul, ce qu'il vise n'est pas touché (le désinstalleur tourne en administrateur). La suppression
   continue après une erreur et dit la première. Le texte d'une erreur (fichier ouvert...) est celui de Windows (sans
   « (os error N) ») et non celui de .NET : les juges n'en ont pas, les deux textes ne peuvent pas être les mêmes.
6. **Les règles du pare-feu** de Nevermind sont comptées et **les cartes réseau** retrouvées par leur identifiant
   (`PcDesinstallation`, src\os\mod.rs) ; depuis le câblage du 06/11, le `PcWindows` du bloc pc le fait aussi (lecture
   COM du pare-feu, carte par son identifiant). Dans la désinstallation, `pare-feu.debloquer-tout` est donc demandé
   quand des règles existent, et les DNS notés par carte sont remis, comme en PowerShell.
7. **powercfg.exe** est lancé par son chemin complet dans System32 (en administrateur, un « powercfg.exe » posé à côté
   du désinstalleur, dans une copie hors de Program Files, serait lancé à sa place) ; le bloc pc fait de même depuis
   le câblage du 06/11. Même commande, même résultat.
8. **Les valeurs d'origine de la machine** (`HKLM\SOFTWARE\Mindset\Nevermind\Aide\Origines`) sont effacées par le
   désinstalleur lui-même si l'Aide n'a pas pu les oublier, comme le repli de Restore-AllSettings en administrateur
   (`PcWindows`, sans droits, ne le peut pas).

## Différences imposées (même résultat)

9. **Les tâches planifiées** passent par schtasks.exe (sans fenêtre) : « Nevermind (démarrage) » et les anciennes
   « OptiGame (démarrage) », « Nexo (démarrage) » sont cherchées à la racine du Planificateur, là où Nevermind les a
   toujours créées (`Get-ScheduledTask -TaskName` cherchait dans tous les dossiers).
10. **Les programmes ouverts** sont lus comme l'Aide les lit (`chemins_processus`, le même code), et non par
   `Get-Process`. Comparaison des chemins sans tenir compte des majuscules, comme `-eq`.
11. **Le choix des données** ne vient que de la ligne de commande (comme le lanceur C#, qui effaçait toute variable
    NEVERMIND_DONNEES héritée) ; même règle pour les deux options à la fois (la question est posée).
12. **%LOCALAPPDATA%** (dossier des données, suite.json) est lu chez Windows (dossier connu), pas dans la variable, et
    à son emplacement par défaut, jamais redirigé (lot H, point 20).
13. Ce que Nevermind.ps1 faisait avant `Invoke-Uninstall` et qui ne sert pas à la désinstallation n'est pas repris
    (marque « téléchargé depuis Internet », anciens `.exe.old`, fichiers manquants du dossier modules).

## Contrat de la suite (06/11, job « Contrat » de tests.yml)

14. **Copie gérée** : aussi quand ce programme est là où Mindset installe Nevermind (`<X>:\Mindset\Programmes\Nevermind`),
    même si suite.json n'est pas dans le `%LOCALAPPDATA%` que donne Windows (point 12) : ses données sont alors
    `<X>:\Mindset\Donnees\Nevermind`, un chemin tiré du seul dossier de ce programme, jamais d'une variable. Avant, le
    contrat de la suite (qui pose suite.json dans un faux `%LOCALAPPDATA%`, comme Mindset en test) voyait une copie non
    gérée : les données cherchées dans `%LOCALAPPDATA%\OptiGame`, et « -SupprimerDonnees » n'y supprimait rien.
15. **Piloté par Mindset** (`-GarderDonnees` ou `-SupprimerDonnees`) : le message de fin n'est pas montré, il est écrit
    au journal (si les données restent). Le désinstalleur de Mindset attend la fin de celui-ci (10 minutes au plus) et
    montre son propre bilan ; une fenêtre « OK » ici bloquait tout jusqu'à un clic (le contrat attend 60 s, et
    échouait). Le texte du message reste le même (juges) ; sans option, ou avec les deux, rien ne change.
    Ce que le message aurait dit d'important est passé à Mindset, qui l'affiche dans son bilan (contrat-appli de
    Mindset) : juste avant de rendre la main, le désinstalleur écrit `<dossier des données de la suite>\Nevermind.conseils-desinstallation.txt`
    (le parent du dossier des données d'une copie gérée : `<X>:\Mindset\Donnees`, ou `%LOCALAPPDATA%\Mindset` ;
    `donnees::dossier_suite`). UTF-8 sans BOM, une ligne par conseil, au plus 10 lignes de 300 caractères : « Redémarre
    le PC... » quand des réglages ont été remis, « Certains éléments n'ont pas pu être restaurés : <élément> » (une
    ligne par élément), puis ce que la question aurait dit (« Attention : » et le problème de la sauvegarde, une ligne
    par changement de sécurité qui reste). « Nevermind est désinstallé. » et la phrase sur les données n'y sont pas
    (Mindset les dit lui-même). Rien à dire : pas de fichier (un ancien est effacé). Écrit à côté puis renommé, sans
    suivre de lien ; écrit même si les données sont supprimées. Copie non gérée, ou sans option : jamais de fichier
    (src\conseils.rs ; tests `conseils_*` de tests\juges.rs).

## Second audit, lot C (10/11) : en administrateur, rien par un simple chemin dans les dossiers du compte

16. **Les données** : le compte peut remplacer son dossier des données (il a « Modification » sur `<X>:\Mindset\Donnees`)
    par une jonction à tout moment. En administrateur, le dossier n'est plus créé, le journal n'est plus raccourci et une
    sauvegarde abîmée n'est plus mise de côté (elle est seulement dite : « abimee » ; elle part de toute façon avec les
    données). La sauvegarde, les réglages et l'historique sont lus dans le dossier ouvert et vérifié, chaque fichier
    ouvert par rapport à lui sans suivre de lien (`os::contexte_administrateur`) ; un dossier qui est un lien, ou qui
    passe par un lien, n'est pas lu (la sauvegarde est dite abîmée) ; « Tout restaurer » ne change la sauvegarde et
    l'historique qu'en mémoire. Les lignes du journal sont gardées en mémoire, puis ajoutées à la fin, comme les
    réglages changés (`SvcOriginal`, `PcfgOriginal`), de la même façon (`os::ecrire_donnees`). Les conseils pour Mindset
    aussi. Lancé sans les droits, le désinstalleur met d'abord le journal au propre sous le compte (raccourci s'il
    dépasse 1 Mo), avant la demande de Windows.
17. **Les raccourcis** : le bureau et le menu Démarrer viennent de `HKCU\...\User Shell Folders`, que le compte peut
    changer. En administrateur, chaque raccourci est ouvert par rapport à son dossier vérifié, relu, puis effacé par la
    même poignée ; un bureau qui est une jonction, ou qui passe par une, n'est pas touché. Lancé sans les droits, le
    désinstalleur repasse ensuite sous le compte et efface lui-même ceux qui lancent encore cette copie.
    (14/11) Un fichier ou un dossier gardé par OneDrive (« Fichiers à la demande » : point d'analyse à l'étiquette
    « nuage », IO_REPARSE_TAG_CLOUD) n'est pas un lien : un Bureau dans OneDrive et son raccourci sont traités comme
    un dossier et un fichier ordinaires, comme en PowerShell (`effacement::attributs_juges`). Avant, en administrateur,
    ils pouvaient être refusés comme des liens et le raccourci restait. Jonctions et liens symboliques : inchangé.
18. **Les programmes fermés** (l'Aide, Nevermind) : ouverts une fois, leur chemin relu par cette poignée juste avant
    de les fermer par elle (comme l'Aide, M6) ; un numéro redonné à un autre programme depuis la liste n'est pas fermé.

## Second audit, lot H (11/11) : le désinstalleur

19. **Relu après la fermeture (M2)** : la sauvegarde, les réglages et l'historique lus au début servent à la question.
    Une fois l'Aide et Nevermind fermés, juste avant « Tout restaurer », ils sont relus de la même façon (sans suivre de
    lien, avec la copie de secours et les valeurs d'origine gardées au registre : `Systeme::relire_donnees`), et c'est
    cette relecture qui est remise comme avant : un réglage fait dans Nevermind pendant la question est remis lui aussi.
    Si le nombre de réglages diffère de celui de la question, le message de fin le dit. À la fin, parametres.json est
    relu et seuls `SvcOriginal` et `PcfgOriginal` y changent (s'ils ont changé depuis la relecture) : une préférence
    écrite entre-temps par Nevermind est gardée. Mindset ne ferme pas Nevermind avant de lancer ce désinstalleur et le
    lance sans option : la question est posée, rien n'est à changer de son côté.
20. **Local AppData** : le compte peut le rediriger sans droits (`HKCU\...\User Shell Folders`). En administrateur,
    « Supprimer mes données » aurait effacé un vrai dossier choisi ainsi. Il est donc pris à son emplacement par défaut
    (`KF_FLAG_DEFAULT_PATH`), vérifié dans le profil que Windows note pour le compte (`GetUserProfileDirectoryW`, dans
    HKLM) ; puis ouvert par une poignée vérifiée, comme les autres données (point 16). Des données vraiment redirigées
    ailleurs (rare) ne sont alors pas trouvées : elles restent, rien n'est effacé.
21. **La commande de fin** : avant, elle effaçait les fichiers par leur nom 3 secondes après la fermeture, et donc une
    réinstallation faite pendant ces 3 secondes. Maintenant chaque fichier et dossier à effacer est ouvert sans suivre
    de lien, vérifié, puis déplacé par sa poignée dans `a-effacer-<numéro>-<heure>`, créé neuf dans le dossier des
    programmes (même disque : un simple renommage, possible même pour ce programme en cours). La commande n'efface plus
    que ce dossier, puis les dossiers vides (`rmdir` sans `/s` : un dossier réinstallé, non vide, reste). Ce qui ne peut
    pas être déplacé (un dossier tenu ouvert par un autre programme, par exemple) est effacé par son nom comme avant, et
    c'est noté au journal.

## La voie « sur place » (architecture, pas une différence)

En administrateur, la version PowerShell faisait elle-même les demandes de la liste de l'Aide (aide-actions.ps1). Ici,
le même code que NevermindAide.exe : ses modules (`actions`, `serveur`, `systeme`, `mesures`, `windows` de
rust\nevermind-aide\src) sont lus par src\lib.rs à leur place (`#[path]`), sans être copiés ni changés, et chaque demande
passe par `serveur::repondre` (vérifiée par `aide_regles::demandes::verifier`, faite par `actions::faire`). L'Aide
n'écoute que Mindset.exe de ce dossier : elle refuserait le désinstalleur, et la réveiller la laisserait tenir
NevermindAide.exe. Si nevermind-aide devient un jour une bibliothèque, ces cinq lignes `#[path]` deviennent une
dépendance.

## Fiche de version

Description et produit « Nevermind », comme le lanceur C# et le relais : le contrat de la suite (tester-contrat.ps1,
partie 4) veut le nom de l'appli pour le relais et pour le désinstalleur. outils\contenu-rust.ps1 attend la même chose.
