# nevermind-appli : différences voulues avec la version PowerShell

Chaque différence est voulue ; la preuve ou la raison est donnée. Tout le reste suit Nevermind.ps1, vue.ps1,
accueil-console.ps1, navigation.ps1, ecran-panneau.ps1, themes.ps1 et evenements.ps1 (juges :
tests\juges\hote-accueil.json, 300 cas, plus 500 cas d'une autre graine qui passent aussi).

## Adresses des images
- Images d'un pack : `http://pack.localhost/<fichier>` au lieu de `https://theme.local/<fichier>` (règle 6 de
  regles-tauri.md : un protocole Tauri, la lecture sûre de suite-commun). kit.js accepte cette adresse (`urlSure`,
  une seule ligne changée dans ui\kit.js).
- Images du PC déclarées par les blocs : `http://fichier.localhost/<jeton>` au lieu de `https://fichier.local/<jeton>`.
  Le jeton est toujours au hasard ; un fichier n'est servi que si son extension ET ses premiers octets disent image,
  et s'il fait moins de 15 Mo (vue.ps1 regardait seulement l'extension).
- Les juges ramènent ces adresses à une seule forme avant de comparer.

## Fenêtre
- « Fermer » de Mindset (WM_CLOSE), la croix et Alt + F4 : Nevermind quitte sans question (contrat-appli : « Fermer »
  de Mindset doit faire quitter l'appli ; Mindcare en Rust fait pareil). La version PowerShell posait la question
  Quitter / Réduire.
- Le rond « Fermer Nevermind » de l'accueil (dans Mindset) pose toujours la question, mais dans l'écran (le kit) au lieu
  d'une petite fenêtre WPF : mêmes textes ; « Quitter » est le premier bouton (Entrée), Échap donne « Réduire » (le kit
  rend toujours le dernier bouton sur Échap ; la fenêtre WPF ne faisait rien sur Échap).
- Pas d'écran de chargement (splash) : la fenêtre est prête cachée, avec un fond sombre dès l'allumage.
- Pas d'icône près de l'horloge pour une copie de travail (non gérée) : « Réduire » la réduit dans la barre des tâches.
  Une copie gérée se cache, comme avant (`Hide-ToTray` d'une copie gérée : `$Window.Hide()`).
- Pas de propriétés de relance par fenêtre (`AppIdentity.SetWindow`) : seule l'identité du processus est posée
  (`JordanJacquot.Nevermind`), comme Mindcare en Rust. Une copie gérée est épinglée par le raccourci de Mindset.
- « Nevermind s'utilise maintenant avec Mindset » : une boîte de Windows (Oui / Non) au lieu de la fenêtre WPF dessinée.
- Écran impossible (WebView2 absente) : un message simple puis la fermeture, sans proposer le fichier « Signaler un
  problème » (c'est le bloc divers qui le fait ; à brancher par l'intégration).
- Pas de mise à l'échelle dans un panneau de Mindset plus petit que la taille minimale : Nevermind PowerShell n'en
  avait pas (le kit se met en page à la taille donnée), seul Mindcare en avait une.
- La page en veille profonde (`TrySuspend`) au bout d'une minute cachée, comme vue.ps1 ; en plus, les ordres au kit
  attendent dès que la page est cachée (vue.ps1 ne les gardait qu'en veille profonde) : rien ne réveille la page en jeu.

## Thème
- Les couleurs d'un pack sont lues par suite-commun, qui refuse en plus des couleurs illisibles sur le fond
  (contraste W3C) : un tel pack garde son thème de base, avec la raison au journal.
- `LogoGlow` d'un pack mal écrit (pas « #RRGGBB » ni « #AARRGGBB ») : pas changé ici (texte tel quel, comme vue.ps1).
- La phrase d'accueil : le format .NET n'est suivi que pour `{0}`, `{{` et `}}` ; toute autre accolade donne une phrase
  vide (.NET lève une erreur, PowerShell rendait aussi une phrase vide). `{0,5}` ou `{0:x}` donnent vide aussi.

## Navigation
- `focus` de l'état n'est jamais posé : il ne servait qu'au retour depuis une page d'avant (WPF), qui n'existe plus.
- `page:<n>` mène à la page du kit (comme `Show-Page`) ; l'Organizer et la carte du réseau d'avant n'existent plus.

## Ce que l'hôte fait en plus (pas de PowerShell)
- Un bloc qui plante (pages, action, tic, signal, fermeture) est noté au journal et n'arrête rien (`Invoke-Safe`) ;
  un tic qui plante est rappelé au plus tôt 10 s après.
- Un bloc qui a rendu « plus besoin de tic » (`None`) est rappelé à la minuterie suivante dès qu'il prend un geste ou
  reçoit un signal (le geste a pu lancer un travail à suivre).

## Câblage des blocs (06/11)
- La table des signaux (qui envoie, qui écoute, sous quel nom) est dans tests\signaux.rs, vérifiée sur les sources de
  toutes les crates ; un signal envoyé sans récepteur ni raison écrite fait échouer le test.
- Au démarrage, l'hôte envoie « etat-hote » au bloc divers (la page Paramètres : version, mises à jour et démarrage
  par Mindset, raccourci « se règle dans Mindset »). « clic:BtnShortcut » (copie gérée) ouvre la page
  « parametres:general » de Mindset (afficher-page.txt et son signal, comme `Open-MindsetSettings`, sans relancer
  Mindset.exe : il est ouvert, Nevermind est dans son onglet). Une copie de travail n'a ni bouton ni réglage de
  mise à jour : Nevermind en Rust ne se met à jour que par Mindset.
- « theme-choisi » {id} (bloc divers, copie autonome) : le thème enregistré, puis la relance (`Set-AppTheme`).
- `Mindset.exe -Uninstall` lance « Désinstaller Nevermind.exe » du même dossier avec les mêmes options
  (`-GarderDonnees`, `-SupprimerDonnees`), l'attend et rend son code. La variable NEVERMIND_DONNEES n'est pas lue
  (le lanceur C# l'effaçait aussi) ; jamais dans la construction d'essai.
- Les images de tous les blocs : « fichier:<chemin> » dans les pages, remplacé ici (le bloc jeux aussi, plus de
  `https://fichier.local`).
- Les tuiles de l'accueil : chaque bloc donne ses faits dans `_accueil` (pc {score, a_regler}, perf {sous},
  protection {score}, jeux {nombre, dernier, fps, image}, reseau {appareils, scan}).
- Construction d'essai seulement : `{ "script": "..." }` dans actions.demande joue un geste dans la page (le clic sur
  un onglet du kit, pour les captures).
