# Noyau de Nevermind en Rust : différences avec la version PowerShell

La version PowerShell (Nevermind\fichiers\modules\donnees.ps1, aide-client.ps1, pont.ps1, mises-a-jour.ps1,
assistance.ps1) fait foi. Les juges (tests\juges, exportés par outils\exporter-juges-noyau.ps1) vérifient tout le
reste au caractère près. Voici les seules différences, chacune voulue ou sans effet pour l'utilisateur.

## Fichiers de données (Save-JsonSur, Read-JsonSur, ConvertTo-Json)

1. **Ordre des clés d'une table.** Une table PowerShell (`@{}`, par exemple `$script:Settings` ou `$script:Backup`)
   s'écrit dans l'ordre de ses cases de hachage ; le Rust garde l'ordre du fichier lu, puis celui des ajouts. Le
   contenu et la forme (retraits, `":  "`, échappements) sont les mêmes : juge `reglages.json` (comparé sans l'ordre)
   et juge `json.json` (313 cas, plus 2013 cas jamais vus d'une autre graine, comparés au caractère près).
2. **Nombres écrits en décimal par l'utilisateur.** `ConvertFrom-Json` lit « 0.000012 » ou « 1.50 » comme un
   `decimal` et les réécrit tels quels ; le Rust les lit comme des nombres à virgule et les écrit comme un `double` de
   .NET (« 1.2E-05 », « 1.5 »). Un nombre écrit par Nevermind lui-même (toujours un `double`) ressort identique. Seul un
   fichier modifié à la main peut changer d'écriture, jamais de valeur.
3. **Messages d'erreur.** Le texte de l'erreur de lecture du JSON ou de Windows (« Objet non valide passé... »,
   « Exception lors de l'appel de... ») vient de .NET en PowerShell, de Rust ici. Le reste de la ligne du journal est le
   même (les juges comparent ce qui entoure le message). `LogError` garde le même début
   (« <date> MethodInvocationException: ») avec le message de Rust.
4. **Profondeur.** `Save-JsonSur -Depth 6` écrivait le nom du type au-delà de 6 niveaux ; le Rust écrit tout.
   Aucun fichier de Nevermind n'a plus de 6 niveaux.
5. **Préférences qui ne sont pas un objet.** Un parametres.json qui contient une liste ou un texte donnait en
   PowerShell des réglages « Length », « Count »... ; le Rust repart de réglages vides (le fichier n'est pas touché).

## Registre

6. **Mode test.** `Save-Original` écrivait en test une copie d'origine dans `HKCU\Software\MindsetTest\Origines` ; le
   Rust n'écrit jamais dans le vrai registre en mode test (faux registre du `Contexte`, valeurs d'origine factices).
   Hors test, même comportement.

## Aide

7. **Plus de DLL.** Les règles de NevermindPont.dll (nom du canal, règle « fermable », programmes des commandes,
   dossiers du nettoyage) sont appelées directement dans aide-regles : le cas « DLL absente » (rien de fermable, canal
   inconnu) n'existe plus. Un dossier qui n'est pas un chemin « X:\ » donne encore le canal « aucune », comme avant.
8. **Délai de connexion.** Le message d'une connexion au canal qui n'aboutit pas est « Le délai de connexion à l'Aide
   est dépassé. » (en PowerShell, le message anglais de .NET « The operation has timed out. »).
9. **Désinstallation.** La voie « sur place » d'Invoke-Aide (désinstallation en administrateur, actions de l'Aide
   faites dans le même processus) n'est pas dans le noyau : elle appartient à la désinstallation.
9 bis. **Démarrage de l'Aide (nevermind-aide, 14/11).** En PowerShell, l'Aide ouvrait son canal, puis renommait les
   anciennes règles du pare-feu (« OptiGame : bloque », Get-NetFirewallRule, souvent plusieurs secondes) avant
   d'écouter : la toute première demande de Nevermind, déjà connectée, attendait la fin. Le Rust faisait pareil (toutes
   les règles lues par COM) et sa sonde de 1,5 s concluait à une Aide muette. Maintenant le renommage se fait dans un
   fil à part, pendant que l'Aide écoute déjà (les demandes du pare-feu cherchent de toute façon les deux noms).

## Mises à jour

10. **Date des fichiers dézippés.** `Expand-ZipVerifie` reprenait la date de modification du zip ; le Rust laisse la
    date de l'installation. Les contrôles (chiffré, taille, CRC, deflate, chemin hors du dossier) sont les mêmes.
11. **Comparaison des fichiers.** `Get-FileHash` (SHA-256) est remplacé par la comparaison des octets : même décision.
12. **Le zip publié.** L'installation cherche encore `fichiers\Nevermind.ps1` (ou `OptiGame.ps1`) dans le zip, comme
    aujourd'hui. Quand le zip publié sera celui de la version Rust, l'hôte devra dire quel programme relancer.

## Assistance

13. **« Signaler un problème ».** La ligne « PowerShell : 5.1... » de infos.txt devient « Moteur : Nevermind en Rust » ;
    « Écran principal » vient de Windows (GetSystemMetrics) au lieu de WPF ; `$global:Error` devient les erreurs notées
    par `Contexte::noter_erreur`.
14. **Défaut gardé tel quel (à décider).** Dans l'écran façon console, le panneau « Signaler un problème » ne montre
    pas la zone de texte de la fenêtre d'avant (Get-PanneauKit ne lit pas les TextBox) : le fichier est créé sans la
    phrase de l'utilisateur. Le Rust fait pareil (juge `pages.json`). Pour le corriger, il suffirait d'ajouter un bloc
    `champ` à `page_signalement` et de passer sa valeur à `signaler_probleme`.
