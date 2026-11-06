# Bloc « protection » : ce qui diffère de la version PowerShell

Les pages Protection, Sécurité et Sauvegarde sont les mêmes au champ près : 622 juges exportés par
`outils\exporter-juges-protection.ps1` (et 1 522 + 1 222 cas tirés avec d'autres graines, jamais vus pendant l'écriture)
passent, après la remise en liste que fait `kit.js` (`normaliser`) et le `{}` remis à `null` de `vue.ps1`. Seul ce qui
tourne derrière change :

## Voulu (plus léger, ou le travail laissé à son propriétaire)
1. **La lecture tourne dans un fil à part**, l'écran n'attend pas. La version PowerShell attendait la lecture pendant
   `Update-SecurityTab` (Invoke-Async, avec la boucle de messages). Aucune boucle de mesure : une lecture au démarrage,
   puis seulement sur « Revérifier » ou après un geste. Le `tic` revient toutes les 60 s sans rien faire (0,5 s quand
   un travail de fond attend sa fin).
2. **« C'est moi » et « Retirer » rejugent la dernière lecture** au lieu de tout relire (fichiers, signatures,
   Defender) : seuls les éléments approuvés ont changé. Le résultat est le même, sans la seconde lecture lente.
3. **Les confirmations** (`Confirm-Action`) passent par le signal commun « question » ; une réponse autre que « oui »
   n'est pas redemandée.
4. **Les analyses de Defender** lancent `MpCmdRun.exe -Scan -ScanType 1|2|3` (sans fenêtre) au lieu de `Start-MpScan`,
   et la base `MpCmdRun -SignatureUpdate` au lieu de `Update-MpSignature`. Codes 0 et 2 (menaces trouvées) : analyse
   faite. L'arrêt tue notre MpCmdRun puis lance `MpCmdRun -Cancel` (comme le bouton Arrêter). Le panneau (câblage du
   06/11) est donné dans les pages (« panneau », src\panneau.rs) comme celui d'un test du bloc pc après la traduction
   de `Get-PanneauKit` : le temps qui passe, le texte d'aide, puis l'état et les verdicts, « Arrêter le test »
   (`panneau:stop`), « Fermer » (`panneau:close`), « Supprimer les menaces » (`panneau:b:0`). Le radar animé n'est pas
   dessiné (il ne l'était pas non plus dans l'écran façon console : « SansImageKit »).
   Le verdict reste celui de `Get-ScanThreats` (jamais « Aucune menace » sans résultat lu).
5. **Délégués aux autres blocs par signaux** (leur moteur n'est pas dans ce bloc) : l'écriture de l'historique
   (« historique », signal commun, fait par l'hôte), le contrôle des comptes (« registre-ecrire » : Set-Reg avec sa
   sauvegarde, bloc pc), « Désactiver au démarrage » (« demarrage-desactiver », bloc pc), l'annulation d'une ligne
   (« historique-annuler » : Undo-HistoryEntry, bloc pc). Le choix d'un dossier (`Invoke-FolderScan`) : la fenêtre de
   Windows (IFileOpenDialog, dossiers seulement) ouverte par ce bloc, au lieu de FolderBrowserDialog, puis l'analyse.
   La note va à la tuile de l'accueil par `_accueil` {protection: {score}} (plus de signal « protection-note »).
   La page Connexion et ses actions (dns, carte-relancer, debit, clic:BtnPing, clic:BtnDnsFlush) sont au bloc réseau ;
   `clic:BtnUndo`, `BtnRestorePoint`, `BtnOpenRestore`, `BtnExport` ne sont pas pris par ce bloc.
6. **La page Sauvegarde lit le disque** : historique.json (et .bak) et sauvegarde.json (et .bak), relus seulement quand
   leur date change, plus SvcOriginal et PcfgOriginal des préférences. Un autre bloc qui garde un réglage d'origine
   (Backup.Dns du DNS, Save-Original du Bureau à distance...) l'écrit dans sauvegarde.json comme la version
   PowerShell puis envoie « sauvegarde-changee » ; s'il garde des origines ailleurs (le registre de l'Aide,
   Read-OriginesRegistre), il envoie « sauvegarde-nombre » {n} avec le compte complet de Get-BackupCount.

## Petites différences de lecture (sans effet sur l'écran dans les essais)
7. Les sous-dossiers qui sont des liens (jonctions, liens symboliques) ne sont pas parcourus pendant la recherche des
   fichiers piégés (Get-ChildItem -Recurse de Windows PowerShell les suivait).
8. Signature : WinVerifyTrust sur le fichier, puis par catalogue (SHA256 puis SHA1), comme Get-AuthenticodeSignature.
9. Tâches planifiées lues sur place : la tâche de démarrage de Nevermind est reconnue avec le script
   `<dossier de l'exe>\fichiers\Nevermind.ps1` (Get-AppScript) ; la version Rust n'a pas ce script, sa propre tâche
   n'est de toute façon pas suspecte (ni commande cachée, ni dossier temporaire).
10. L'Aide (`ctx.aide`) est appelée depuis le `tic` (fil de l'hôte), comme Get-AideLecture l'était depuis la fenêtre ;
    jusqu'à 60 s si l'Aide tarde. Un client de l'Aide utilisable depuis un autre fil (noyau) permettrait de le sortir.
11. **Analyse demandée par le bloc trafic** (Invoke-TrafficScan) : signal reçu « analyse-demandee » {Sorte, Chemins,
    Titre, Marque}, même déroulé que Invoke-DefenderScan ; à la fin, signal « analyse-finie » {Marque, Resultat}
    où Resultat vaut `$script:LastScanResult` (clean, threats, unknown, error, cancel). Le bloc trafic pose sa marque
    « analysé » seulement pour « clean ».
