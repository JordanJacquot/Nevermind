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
4. **Les fichiers** : ceux qui peuvent partir tout de suite partent tout de suite (Nevermind.exe disparaît donc avant la
   fin de ce programme, et Mindset le voit aussitôt), puis la même commande que la version PowerShell (cmd.exe caché,
   `ping ... & del ... & rmdir ...`) refait le tout 3 secondes après la fermeture. Même liste, même commande (comparée
   aux juges caractère par caractère).
5. **« Supprimer mes données »** ne suit jamais un lien : un point de jonction ou un lien symbolique posé dans les
   données est retiré seul, ce qu'il vise n'est pas touché (le désinstalleur tourne en administrateur). La suppression
   continue après une erreur et dit la première. Le texte d'une erreur (fichier ouvert...) est celui de Windows (sans
   « (os error N) ») et non celui de .NET : les juges n'en ont pas, les deux textes ne peuvent pas être les mêmes.
6. **Les règles du pare-feu** de Nevermind sont comptées et **les cartes réseau** retrouvées par leur identifiant
   (`PcDesinstallation`, src\os\mod.rs) : le `PcWindows` du bloc pc ne le fait pas encore (règles toujours 0, carte
   jamais trouvée : nevermind-pc\DIVERGENCES.md). Dans la désinstallation, `pare-feu.debloquer-tout` est donc demandé
   quand des règles existent, et les DNS notés par carte sont remis, comme en PowerShell.
7. **powercfg.exe** est lancé par son chemin complet dans System32 (le `PcWindows` du bloc pc le cherche par son nom :
   en administrateur, un « powercfg.exe » posé à côté du désinstalleur, dans une copie hors de Program Files, serait
   lancé à sa place). Même commande, même résultat.
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
12. **%LOCALAPPDATA%** (dossier des données, suite.json) est lu chez Windows (dossier connu), pas dans la variable.
13. Ce que Nevermind.ps1 faisait avant `Invoke-Uninstall` et qui ne sert pas à la désinstallation n'est pas repris
    (marque « téléchargé depuis Internet », anciens `.exe.old`, fichiers manquants du dossier modules).

## La voie « sur place » (architecture, pas une différence)

En administrateur, la version PowerShell faisait elle-même les demandes de la liste de l'Aide (aide-actions.ps1). Ici,
le même code que NevermindAide.exe : ses modules (`actions`, `serveur`, `systeme`, `mesures`, `windows` de
rust\nevermind-aide\src) sont lus par src\lib.rs à leur place (`#[path]`), sans être copiés ni changés, et chaque demande
passe par `serveur::repondre` (vérifiée par `aide_regles::demandes::verifier`, faite par `actions::faire`). L'Aide
n'écoute que Mindset.exe de ce dossier : elle refuserait le désinstalleur, et la réveiller la laisserait tenir
NevermindAide.exe. Si nevermind-aide devient un jour une bibliothèque, ces cinq lignes `#[path]` deviennent une
dépendance.

## Fiche de version

Description et produit « Désinstaller Nevermind » (demandé pour la version Rust ; le lanceur C# disait « Nevermind »).
outils\contenu-rust.ps1 attend encore « Nevermind » pour ce programme : à changer avec la chaîne.
