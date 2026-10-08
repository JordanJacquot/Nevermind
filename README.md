# OptiGame

Analyse et optimisation gaming pour Windows 10 et 11.

OptiGame vérifie la santé de ton PC (processeur, carte graphique, mémoire, disques, écrans, stabilité), lui donne un score sur 100 et t'indique ce qui freine tes jeux. Quand c'est possible, il corrige lui même en un clic, et tout peut être annulé. Il surveille aussi ton réseau et tout ce qui sort de ton PC.

## Télécharger

1. Va dans [Releases](../../releases/latest) et télécharge **OptiGame.zip**.
2. Clic droit sur le zip > **Extraire tout**.
3. Double clique sur **OptiGame.exe** et accepte la demande d'autorisation de Windows.

La première fois, Windows peut afficher « Windows a protégé votre ordinateur » : clique sur « Informations complémentaires » puis « Exécuter quand même ».

Dans la page **Sauvegarde**, « Raccourci et démarrage » crée un raccourci sur le bureau et peut lancer OptiGame à chaque démarrage du PC (réduit près de l'horloge, sans demande d'autorisation de Windows).

Les mises à jour sont ensuite proposées directement dans l'application. Relancer OptiGame alors qu'il est déjà ouvert (même caché près de l'horloge) ramène simplement sa fenêtre.

## Ce que fait l'application

Quatre sections dans le menu de gauche : **Ordinateur**, **Jeux**, **Réseau** et **Trafic**. En haut de chaque page : une **barre de recherche** (Ctrl + K) qui propose les réglages au fil de la frappe, fautes de frappe comprises, et emmène directement au bon endroit ; et un bouton « Signaler un problème ».

### Ordinateur

L'accueil, avec le score d'optimisation, le niveau de protection et une carte par fonction.

- **Tableau de bord** : mesures en direct, santé de chaque composant, score et liste de ce qui peut être amélioré. Chaque point ouvre une fiche : ce qui a été trouvé, ce que l'app va faire, bouton Exécuter, puis « Revenir en arrière » si besoin.
- **Onduleur** : fabricant, modèle, charge, autonomie, usure de la batterie, alerte en cas de coupure, et réglage en un clic d'un arrêt propre du PC avant que la batterie soit vide. Un onduleur invisible pour Windows est repéré par sa prise USB ou par son logiciel.
- **Optimisation gaming** :
  - réglages de Windows à l'effet reconnu (plan d'alimentation, mode jeu, Game Bar, planification GPU, accélération de la souris...) ;
  - **Mes parties** : FPS mesurés pendant tes parties (via PresentMon d'Intel), comparaison avant / après, et diagnostic quand une partie rame (carte graphique ou processeur à fond, surchauffe, mémoire, disque, programmes en arrière plan...) avec corrections guidées ;
  - **Overlay** : compteur de FPS par dessus le jeu, style complet ou discret, dans le coin de l'écran choisi, avec un aperçu ;
  - **Lag en ligne** : pendant tes parties ou en test de 30 secondes, mesure chaque étape du chemin (PC vers box, box vers fournisseur, Internet, serveur du jeu) et explique d'où vient le lag (Wi-Fi, téléchargement en arrière plan, box, serveur loin, VPN) ;
  - **Mode jeu** (ferme des applis pendant que tu joues) et **profils par jeu** (priorité haute, carte graphique puissante).
  - **Jeux reconnus** : Steam, Epic, Ubisoft Connect, EA app, GOG Galaxy, Battle.net, Riot, Rockstar, Amazon Games, Xbox / Game Pass, plus « Ajouter un jeu » pour tout le reste.
- **Tests des composants** : vitesse et santé de chaque disque, processeur (puissance, stabilité), mémoire, capteurs de la carte graphique, débit Internet, lag en charge (bufferbloat), pixels morts.
- **Sécurité** : niveau de protection, analyses Microsoft Defender, recherche de fichiers déguisés, programmes cachés, tâches planifiées suspectes, exclusions d'antivirus, hosts et proxy.
- **Démarrage**, **Connexion** (ping, gigue, DNS), **Nettoyage** (liste des fichiers avant de nettoyer, journal fichier par fichier), **Sauvegarde** (tout annuler, historique, point de restauration, rapport HTML).
- **PC portables** : mode « Meilleures performances », jeux forcés sur la carte graphique dédiée, santé de la batterie.

### Jeux

La bibliothèque de tous tes jeux, comme celle de Steam mais tous launchers confondus (Steam, Epic, Ubisoft Connect, EA app, GOG, Battle.net, Riot, Ankama, Rockstar, Amazon, Xbox, plus les jeux ajoutés à la main).

- **Jaquettes** : celles que Steam garde sur le PC, sinon cherchées sur la boutique Steam puis Wikipédia (seul le nom du jeu est envoyé, option désactivable), sinon le logo ou l'icône du jeu, recherche et filtre par launcher, les derniers jeux joués en premier.
- **Double clic ou « Lancer »** : le jeu démarre par son launcher (connexion, mises à jour, anti triche comme d'habitude), sans les droits administrateur d'OptiGame.
- **Fiche du jeu** : temps de jeu et nombre de parties (notés par OptiGame), FPS de la dernière partie mesurée.
- **Optimisation du jeu** : priorité haute, carte graphique puissante, réglages Windows, mesure des FPS, mode jeu, disque du jeu, avec un bouton « Tout optimiser » annulable.
- **Désinstaller** : ouvre le désinstalleur du jeu ou de son launcher (Steam, Ubisoft, Battle.net, Riot...), la liste se met à jour toute seule.

### Réseau

- **Scan** de tous les appareils connectés (téléphones, consoles, TV, box...), avec leur fabricant, et signalement des nouveaux appareils d'un scan à l'autre (option : être prévenu toutes les 10 minutes).
- **Recherche approfondie** : appareils discrets (qui ne répondent pas au ping, ou visibles seulement en IPv6), nom, marque et modèle annoncés, services proposés, caméras possibles. Les autres cartes réseau de ce PC (Wi-Fi, même déconnectée) sont reconnues comme « ce PC ».
- **Filtres** : tous, discrets, nouveaux, caméras possibles.
- **Carte du réseau** en constellation animée : la box au centre, les appareils en orbes lumineux rangés par familles, des impulsions sur les liaisons d'autant plus rapides que l'appareil répond vite. Un clic ouvre la fiche de l'appareil (ping en direct, services ouverts).
- **Audit de sécurité** : Wi-Fi, box (WPS, UPnP), DNS, appareils et ce PC, note sur 100, corrections et rapport.

### Trafic

Ce qui sort de ton PC, en direct. Le contenu des échanges, chiffré, n'est jamais lu : OptiGame voit quel programme parle à qui et combien il envoie.

- **Programmes connectés** : pour chacun, les serveurs contactés, les volumes envoyés et reçus, la signature de l'éditeur. Les services Windows cachés derrière « svchost » sont affichés par leur vrai nom.
- **Alertes** : programme non signé dans un dossier à risque, outil de Windows détourné, port utilisé par les virus ou Tor, gros envois inhabituels, prise en main à distance. Pour chaque alerte : « C'est normal, je lui fais confiance », analyse antivirus, blocage d'Internet (annulable), ouvrir l'emplacement.
- **Type de données envoyées** : la fiche de chaque programme dit ce qu'il envoie probablement (messages et appels, jeu en ligne, statistiques d'utilisation, publicité et suivi, fichiers synchronisés, assistant IA, mises à jour...) et ce qu'il n'envoie pas, d'après le nom des serveurs et les volumes.
- **Serveurs sans nom** identifiés automatiquement : nom officiel de l'adresse et entreprise propriétaire avec son pays, via l'annuaire public des adresses Internet (rdap.org). Seule l'adresse du serveur est envoyée, et l'option se coupe sur la page.
- **Ce que Windows envoie à Microsoft** : identifiants du PC (appareil, publicité, compte), réglages qui envoient plus que le minimum (télémétrie, pubs, recherche Bing, saisie, voix...) avec un bouton « Couper » annulable, et services Windows qui parlent à Microsoft en direct.

Les compteurs d'octets et le repérage des serveurs en UDP demandent les droits administrateur (OptiGame les demande au lancement).

### Engagements

- Aucune modification n'est faite sans clic de l'utilisateur, et chaque réglage modifié est sauvegardé avant d'être changé.
- Tout est annulable, un par un depuis l'historique ou en une fois (« Tout restaurer »).
- Aucune donnée personnelle n'est envoyée. Le code est lisible dans `OptiGame/fichiers/modules`.

## Développement

```
OptiGame/            l'application telle qu'elle est distribuée
  fichiers/OptiGame.ps1   point d'entrée : instance unique, droits admin, version, chargement des modules, lancement
  fichiers/modules/       le code, découpé par partie (chargé dans cet ordre)
    natif.cs              fonctions natives C# (écrans, tests, scan réseau, trafic, lag...)
    interface.xaml        la fenêtre, le thème et tous les onglets
    donnees.ps1           sauvegarde, journal, accès au registre
    optimisations.ps1     réglages gaming, programmes au démarrage, jeux installés, onduleurs
    systeme.ps1           connexion active, nettoyage, restauration, désinstallation
    interface.ps1         chargement de la fenêtre, aides, travail en arrière plan, écran de chargement
    tableau-de-bord.ps1   constats, score, fiches de correction, retour en arrière
    analyse.ps1           santé des composants, onduleur et analyse complète
    onglets.ps1           mesures en direct, Gaming, Démarrage, Connexion, Nettoyage, Sauvegarde
    visuels.ps1           animations, jauges, courbes, compteur du chargement
    tests.ps1             onglet Tests
    securite.ps1          onglet Sécurité
    navigation.ps1        accueil « Ordinateur » et navigation
    reseau.ps1            section Réseau : scan, filtres et fiche appareil
    reseau-avance.ps1     recherche approfondie : appareils discrets, noms et modèles annoncés, caméras
    carte-reseau.ps1      carte du réseau en constellation animée
    audit-reseau.ps1      audit de sécurité du réseau
    mises-a-jour.ps1      mises à jour depuis GitHub
    assistance.ps1        historique, signaler un problème, notifications, visite guidée
    jeu.ps1               mode jeu automatique, profils par jeu, compteur de FPS, alerte de température
    diagnostic-fps.ps1    d'où viennent les problèmes de FPS et comment les régler
    trafic.ps1            ce qui sort du PC : connexions par programme, volumes, types de données, alertes, annuaire des serveurs
    microsoft.ps1         ce que Windows envoie à Microsoft : identifiants, réglages, services
    lag.ps1               lag en ligne : mesure du chemin et diagnostic
    bibliotheque.ps1      section Jeux : bibliothèque, lancement, temps de jeu, optimisation par jeu
    recherche.ps1         barre de recherche des réglages, suggestions et accès direct
    evenements.ps1        branchement des boutons et du démarrage
  fichiers/outils-tiers/  PresentMon.exe (Intel, licence MIT) pour mesurer les FPS
outils/
  construire.ps1     génère l'icône, compile OptiGame.exe et le désinstalleur, crée OptiGame.zip
  publier.ps1        publie une nouvelle version sur GitHub
  tester.ps1         lance le test automatique sur une copie isolée de l'app
  test-app.ps1       les étapes du test automatique
  icone.ps1          dessine l'icône
  lanceur.cs         code du lanceur OptiGame.exe
```

Tester (copie isolée de l'app, hors écran, rien n'est modifié sur le PC) :

```
.\outils\tester.ps1              # toutes les pages
.\outils\tester.ps1 -Complet     # + réseau, audit, lag en charge
.\outils\tester.ps1 -Captures    # + captures d'écran dans _test\captures
```

Publier une mise à jour (vérifie la syntaxe, les tirets et lance le test avant) :

```
.\outils\publier.ps1 -Version 1.1 -Notes "Ce qui change"
.\outils\publier.ps1 -Version 1.2 -Notes "..." -Beta   # seulement pour ceux qui ont activé les bêtas
```
