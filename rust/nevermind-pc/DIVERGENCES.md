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

## Pas encore porté

- Les tests des composants eux-mêmes (vitesse des disques, processeur, mémoire, carte graphique en direct, connexion et
  « lag en charge », pixels morts, 3DMark) : la page « Tests des composants » et ses tuiles sont là ; leurs boutons disent
  que le test arrive. `mdsched.exe` et le rapport de batterie sont lancés comme avant. Le signal `test-debit` du bloc
  réseau reçoit la même réponse (pas de `debit-mesure`).
- « Voir les fichiers » du nettoyage (`nett-voir:N`, le panneau des plus gros fichiers).
- La désinstallation (`Invoke-Uninstall`, `Remove-DonneesDesinstallation`), le raccourci du bureau et la tâche de
  démarrage (systeme.ps1) : elles relèvent de l'hôte et de Mindset ; non portées ici.
- `Export-Report` (rapport HTML) et visuels.ps1 (animations WPF, inutiles avec le kit).
- Les règles du pare-feu « Nevermind : bloque ... » ne sont pas comptées (`Get-NetFirewallRule`) : « Tout restaurer »
  ne demande donc `pare-feu.debloquer-tout` que si un autre bloc le fait (à brancher).

## Correction d'un défaut du faux PC des juges (pas de Nevermind)

`ConvertTo-ValeurPs` du script rendait les valeurs binaires en `object[]` (déroulées par PowerShell) : tous les
programmes au démarrage passaient pour activés. Corrigé dans le script (`return , ([byte[]]...)`) avant l'export final.
