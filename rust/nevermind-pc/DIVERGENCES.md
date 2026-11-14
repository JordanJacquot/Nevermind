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
4. **Historique** (câblage du 06/11) : les entrées vont dans l'historique du noyau (`Contexte::ajouter_historique`,
   commun à tous les blocs : historique.json), avec leur vrai identifiant ; « Revenir en arrière » les marque
   « annulées » (`Set-HistoryUndone`) et « Tout restaurer » aussi (`Set-HistoryAllUndone`, la remise du pare-feu n'y
   est pas suivie). Après chaque écriture de sauvegarde.json, le noyau la relit (il la garde aussi en mémoire pour les
   écritures des autres blocs) ; sur « sauvegarde-changee », ce bloc relit la sienne (rien n'est écrasé) et renvoie
   le compte de la page Sauvegarde (`sauvegarde-nombre` {n}, Get-BackupCount).
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
   Le PC est lu (WMI, registre, powercfg en lecture), comme la version PowerShell en test, pour que la construction
   d'essai montre l'analyse de Ton PC (avant le câblage du 06/11, rien n'était lu et l'analyse restait « en cours »).

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
15. **`$Live.Fast`** (mesures plus rapides pendant un test ou la carte graphique en direct) : le signal
    `mesures-rapides` {on} au bloc perf, qui mesure alors toutes les 400 ms, même écran caché (onglets.ps1:82). Les
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

## Câblage avec les autres blocs (06/11)

- **`PcWindows`** : les règles du pare-feu « Nevermind : bloque ... » (et « OptiGame : bloque ... ») sont comptées par
  l'interface COM du pare-feu (lecture sans droits, comme `Get-NetFirewallRule`) ; une carte réseau est retrouvée par
  son identifiant (`ConvertInterfaceGuidToLuid`) ; powercfg.exe, et toute commande donnée par son seul nom, est
  lancé par son chemin complet dans System32 (comme le désinstalleur). Essai `lectures_du_pc` (lecture seule).
- **Signaux pris ici** : `mesures` (bloc perf), `test-debit` (bloc réseau), `declarations` (blocs perf et jeux : le titre
  est « historique » ou « titre »), `appliquer-reglages-windows` {ids} (fiche d'un jeu : `Invoke-TweakFix`, sans point
  de restauration, « C'est déjà réglé. » si rien à faire), `ouvrir-fiche` {id} (bloc perf), `registre-ecrire`
  (« Réactiver » le contrôle des comptes, bloc protection : `Set-Reg` avec sa sauvegarde et l'historique),
  `demarrage-desactiver` {elements:[{Nom, Exe}]} (bloc protection : `Set-StartupState $i $false` pour chaque
  programme ; une ligne d'historique par programme, comme « Désactiver » de la page Démarrage, au lieu d'une seule
  ligne pour le geste de la page Sécurité), `historique-annuler` {id} (page Sauvegarde : `Undo-HistoryEntry`, avec sa
  question), `sauvegarde-changee`. L'action `fiche:annuler` {log, historique} (« Revenir en arrière » de la fiche de
  résultat d'un autre bloc, montrée par l'hôte) fait `Invoke-UndoLastRun`.
- **Signaux envoyés** : `analyse-pc`, `reglages-jeu` (avec `id` et `fait` en plus), `demarrage`,
  `demarrage-programmes`, `nettoyage`, `sauvegarde-nombre`, `mesures-rapides`, `debit-mesure`. `pc:analyse` n'est plus
  envoyé : le score et les points à régler vont à l'accueil par `_accueil` {pc: {score, a_regler}}. Les anciens
  `pc:analyser`, `pc:appliquer-reglages`, `pc:demarrage`, `pc:nettoyer`, `pc:tout-restaurer` (que personne n'envoyait)
  sont retirés : les pages passent par leurs actions.
- Table complète et vérifiée : rust\nevermind-appli\tests\signaux.rs.

## Correction d'un défaut du faux PC des juges (pas de Nevermind)

`ConvertTo-ValeurPs` du script rendait les valeurs binaires en `object[]` (déroulées par PowerShell) : tous les
programmes au démarrage passaient pour activés. Corrigé dans le script (`return , ([byte[]]...)`) avant l'export final.

## Câblage du contrat de la suite (06/11)
- Après chaque analyse (et chaque diffusion aux autres blocs), le signal « etat-mindset » `{ "pc": { Score, AFaire,
  Urgent, NbAFaire } }` part vers l'hôte, qui écrit etat.json pour Mindset (`Write-EtatMindset`, navigation.ps1:64-78 :
  le premier point de `Get-HubFindings`, important d'abord, urgent s'il est rouge).
- La première diffusion (analyse-pc, reglages-jeu, etat-mindset...) part au tic qui suit la fin de la lecture lente,
  même quand c'est `pages` (écran regardé) qui l'a vue finir : avant, elle était alors perdue jusqu'au premier geste.

## Pilote NVIDIA des vieilles cartes (second audit du 09/11)
- Le dernier pilote « Game Ready » (`NvLatest`) n'est demandé et comparé que pour une carte qu'il prend encore en charge
  (`analyse::nvidia_suivie` : RTX, GTX 16xx, MX 4xx et 5xx, T400 à T1200). En PowerShell, toute carte NVIDIA y était
  comparée : une GTX 1060 restait sur « Nouveau pilote NVIDIA disponible » vers un pilote qui ne s'installe pas sur
  elle. Pour les autres, la règle de l'âge du pilote installé, comme sans réponse de NVIDIA.

## Mes périphériques (nouveau, 13/11 ; pas dans la version PowerShell)
- Page « Ton PC », juste après « Ton PC maintenant » : « Mes périphériques », une ligne par casque, souris ou clavier
  (nom du produit, « Casque · En charge », barre et %, ou « Filaire » ; vert en charge, rouge sous 20 %, orange sous
  35 %), et dans la ligne « Ouvrir G Hub » si le logiciel de la marque est installé (Logitech G Hub ; iCUE, Synapse et
  GG sont reconnus, mais seuls les appareils Logitech sont lus). Le reste de la ligne mène à la page de détail de l'appareil
  (`perif:<sorte>-<nom>`, parent « pc » : anneau de la batterie, Batterie, Connexion, Marque, Logiciel). Aucune
  section sans appareil.
- Lecture (`Pc::peripheriques`, module Windows `windows::peripheriques`) : des questions HID seulement, identifiant de
  logiciel 0x0C (G Hub peut rester ouvert), jamais un réglage ni les notifications. Souris et claviers : HID++ 2.0
  (0x1004 batterie, 0x0005 nom et sorte), cases 1 à 6 d'un récepteur, 0xFF branché directement (fonction de batterie
  absente : « Filaire »). Casques PRO X 2 : protocole « Centurion » (pont, puis 0x0104). Dans un travail de fond,
  2 à 3 s au plus par appareil ; relue au plus une fois par minute, seulement page « Ton PC » ou détail regardés,
  jamais écran caché. « Ouvrir G Hub » : `Pc::lancer` (ShellExecute), comme les autres programmes.
- Mode test (`ctx.essai`) : des appareils d'exemple, jamais les vrais. Faux PC des tests et des juges : aucun
  appareil par défaut (la page « pc » des juges ne change pas : la section est ajoutée par le bloc après `pages_pc`).

## Plan d'alimentation disparu (14/11 ; pas dans la version PowerShell)
- Un plan noté peut disparaître du PC (`powercfg -restoredefaultschemes` du bouton de Mindcare, Windows Update, un
  autre logiciel). Nevermind s'en rend compte en lisant `powercfg /list` (rien n'est conclu si la liste est vide).
- Plan de Nevermind (`CreatedScheme`) disparu, au moment de régler l'alimentation (mode jeu, correction de Ton PC) :
  l'identifiant mort est oublié et enregistré, puis la copie est recréée comme la première fois et le nouvel
  identifiant gardé (oublié d'abord : la sauvegarde ne remplace jamais une valeur déjà notée). Aucun message.
- Plan d'origine (`PowerScheme`) disparu : au réglage, il est remplacé par Équilibré
  (381b4222-f694-41f0-9685-ff5bb260df2e) ; « Tout restaurer » (et donc le désinstalleur) et « Revenir en arrière »
  remettent Équilibré au lieu d'échouer. Une copie de Nevermind disparue : son « /delete » part comme dans la
  version PowerShell (les juges l'attendent), la réponse de powercfg est ignorée, aucune erreur.
  Toutes les écritures passent comme avant par `powercfg_changer`, avec le permis du verrou des gestes.
