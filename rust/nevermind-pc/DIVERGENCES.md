# nevermind-pc : ce qui diffère de la version PowerShell

Juges : `outils\exporter-juges-pc.ps1` (300 PC simulés pour l'analyse, les pages Ton PC / Tableau de bord et 1 216 fiches ;
80 PC simulés pour « Tout corriger », « Revenir en arrière » et « Tout restaurer », 4 429 gestes contre une fausse Aide).
Tout est identique, sauf ce qui suit.

## Différences voulues ou imposées

1. **Ordre de « Tout restaurer » dans le registre** : PowerShell parcourt `$script:Backup.Registry` (une table de hachage
   de .NET, sans ordre voulu) ; Rust parcourt les réglages dans l'ordre où ils ont été notés. Mêmes demandes, même
   résultat ; seul l'ordre des remises de registre peut différer (le juge compare cette étape sans l'ordre).
2. **Questions** : en PowerShell, `Confirm-Action` attend la réponse sur place. En Rust, la question part à l'hôte par le
   signal commun `question` (action de retour `pc:reponse`) et la suite reprend avec la réponse. Même texte, même suite.
3. **« Tes écrans s'affichent bien ? »** : le compte à rebours de 15 s est tenu par `tic` (au lieu d'une boucle qui
   bloque la fenêtre). Sans réponse : retour à l'ancienne fréquence, comme avant.
4. **Historique** : les entrées partent au bloc « protection » par le signal `historique` (au lieu d'écrire
   historique.json ici). « Revenir en arrière » depuis la fiche de résultat marche ; marquer l'entrée « annulée » dans
   l'historique (`Set-HistoryUndone`, `Set-HistoryAllUndone`) n'a pas de signal commun : à ajouter par l'intégration.
5. **Culture** : les nombres sont toujours écrits en français (fr-FR de Windows 11 : groupes séparés par U+202F, virgule),
   comme sur le PC de Jordan ; PowerShell suivait la culture de chaque PC. `LastClean.Date` est écrit sans décalage horaire.
6. **Dates WMI** (pilote, BIOS) : lues sans conversion en heure locale (PowerShell les convertit) : le jour affiché peut
   différer d'un jour seulement pour une date écrite près de minuit.
7. **Programmes au démarrage** : le nom montré est celui de l'entrée (PowerShell prenait le nom du produit lu dans
   l'exécutable, `FileVersionInfo`) ; la cible des raccourcis .lnk du dossier Démarrage n'est pas lue (le raccourci
   lui-même sert d'exécutable). Les icônes (`image`) ne sont pas envoyées.
8. **Dernier pilote NVIDIA** (`NvLatest`, geforce.com) : pas lu (pas de client HTTP dans le bloc) : l'analyse se base sur
   l'âge du pilote, comme quand l'option « Vérifier le pilote NVIDIA » est coupée.
9. **Mode test** (`ctx.essai`) : sur le vrai PC, aucune écriture (registre, powercfg, écrans, Aide sauf lectures).

## Tests des composants (tests.ps1, src/tests)

Juges : `outils\exporter-juges-tests.ps1` (les 300 PC simulés d'analyse.json, 3 ou 4 suites de gestes chacun, des
mesures inventées) : le panneau tel que l'écran le reçoit (Get-PanneauKit, pendant et après chaque test), la page Tests
des composants, les messages, ce qui est demandé au moteur ([OGNative]) et les gestes sur le PC. Tout est identique ;
ce qui suit diffère.

10. **« Voir le résultat » pendant un test** : grisé comme les autres boutons des tuiles. En PowerShell il restait actif
    (il n'est pas dans `$script:TestButtons`) et remplaçait le panneau du test en cours par l'ancien résultat.
11. **Santé d'un disque** : les compteurs de fiabilité sont ceux que l'Aide a donnés à l'analyse (`Get-AideFiabiliteDisque`) ;
    `Get-StorageReliabilityCounter` n'est pas appelé (il demande les droits d'administrateur : sans eux, PowerShell
    tombait déjà sur l'Aide). Le disque lui-même est relu par WMI (MSFT_PhysicalDisk, les mêmes champs).
12. **Recherche d'erreurs** : le même `Repair-Volume -Scan`, lancé dans un powershell.exe sans fenêtre (CREATE_NO_WINDOW)
    au lieu du fil de Nevermind.
13. **Mesures** : les mêmes calculs (celui du processeur est identique au bit près, voir tests_mesures.rs), les mêmes
    durées, étapes et progressions. Le contenu écrit et les positions lues du test du disque viennent d'un autre tirage
    au hasard (sans effet sur la mesure). L'envoi du test de débit annonce sa taille (400 morceaux de 64 Ko par demande,
    comme natif.cs en écrit au plus) au lieu d'un envoi « chunked » : les octets comptés sont les mêmes. Les points du
    processeur sont ceux du code compilé de Nevermind en Rust : ils ne sont pas comparables au point près à ceux de
    l'ancienne version (même calcul, vitesse d'exécution différente).
14. **Couleurs plein écran** : une fenêtre Windows simple (au lieu de WPF), mêmes couleurs, même texte, mêmes touches.
15. **`$Live.Fast`** (mesures plus rapides pendant un test) : pas de signal commun ; le bloc perf garde son rythme. Les
    fréquences du processeur sont relevées à chaque tour de 100 ms sur la dernière mesure reçue, comme avant.
16. **Tour du bloc** : `tic` ne rend jamais « plus besoin » (un tour vide par demi-seconde hors test), car l'hôte ne
    rappelle pas un bloc qui l'a dit, et un test peut être lancé à tout moment par un bouton.
17. **Rapport de batterie** : le chemin du fichier était faux (« \r » pris pour un retour chariot) ; corrigé.
18. **Mode test** (`ctx.essai`) : rien n'est mesuré ni lu ; un test lancé finit en « Échec » (« mode test »).
19. **Thème** : les tons (vert, orange, rouge) sont lus sur les couleurs Néon, comme dans les juges ; PowerShell les
    lisait sur les couleurs du thème en cours (`ConvertTo-ThemeHex`). Avec un autre thème, un ton pourrait différer :
    pas vérifié.

## Pas encore porté

- « Voir les fichiers » du nettoyage (`nett-voir:N`, le panneau des plus gros fichiers).
- La désinstallation (`Invoke-Uninstall`, `Remove-DonneesDesinstallation`), le raccourci du bureau et la tâche de
  démarrage (systeme.ps1) : elles relèvent de l'hôte et de Mindset ; non portées ici.
- `Export-Report` (rapport HTML) et visuels.ps1 (animations WPF, inutiles avec le kit).
- Les règles du pare-feu « Nevermind : bloque ... » ne sont pas comptées (`Get-NetFirewallRule`) : « Tout restaurer »
  ne demande donc `pare-feu.debloquer-tout` que si un autre bloc le fait (à brancher).

## Correction d'un défaut du faux PC des juges (pas de Nevermind)

`ConvertTo-ValeurPs` du script rendait les valeurs binaires en `object[]` (déroulées par PowerShell) : tous les
programmes au démarrage passaient pour activés. Corrigé dans le script (`return , ([byte[]]...)`) avant l'export final.
