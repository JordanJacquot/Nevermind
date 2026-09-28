# OptiGame

Analyse et optimisation gaming pour Windows 10 et 11.

OptiGame vérifie la santé de ton PC (processeur, carte graphique, mémoire, disques, écrans, stabilité), lui donne un score sur 100 et t'indique ce qui freine tes jeux. Quand c'est possible, il corrige lui même en un clic, et tout peut être annulé.

## Télécharger

1. Va dans [Releases](../../releases/latest) et télécharge **OptiGame.zip**.
2. Clic droit sur le zip > **Extraire tout**.
3. Double clique sur **OptiGame.exe** et accepte la demande d'autorisation de Windows.

La première fois, Windows peut afficher « Windows a protégé votre ordinateur » : clique sur « Informations complémentaires » puis « Exécuter quand même ».

Les mises à jour sont ensuite proposées directement dans l'application.

## Ce que fait l'application

Deux sections : **Ordinateur** (accueil avec une carte par fonction) et **Réseau** (scan des appareils connectés, fabricant, détection des nouveaux appareils, fiche détaillée par appareil, audit de sécurité avec note, corrections et rapport HTML).

- **Tableau de bord** : mesures en direct, santé de chaque composant, score et liste de ce qui peut être amélioré.
- **Fiches de correction** : ce qui a été trouvé, ce que l'app va faire, bouton Exécuter, puis « Revenir en arrière » si besoin.
- **Optimisation gaming** : plan d'alimentation, mode jeu, Game Bar, planification GPU, accélération de la souris... Mode jeu automatique (ferme des applis pendant que tu joues) et profils par jeu (priorité haute, carte graphique).
- **Tests des composants** : vitesse et santé de chaque disque, processeur (puissance, stabilité), mémoire, capteurs de la carte graphique, débit Internet, lag en charge (bufferbloat), pixels morts.
- **Sécurité** : niveau de protection, analyses Microsoft Defender, recherche de fichiers déguisés, programmes cachés, tâches planifiées suspectes, exclusions d'antivirus, hosts et proxy.
- **Démarrage**, **Connexion** (ping, gigue, DNS), **Nettoyage**, **Sauvegarde** (tout annuler, point de restauration, rapport HTML).
- **PC portables** : mode « Meilleures performances », jeux forcés sur la carte graphique dédiée, santé de la batterie.

Aucune modification n'est faite sans clic de l'utilisateur, et chaque réglage modifié est sauvegardé avant d'être changé.

## Développement

```
OptiGame/            l'application telle qu'elle est distribuée
  fichiers/OptiGame.ps1   point d'entrée : droits admin, version, chargement des modules, lancement
  fichiers/modules/       le code, découpé par partie (chargé dans cet ordre)
    natif.cs              fonctions natives C# (écrans, tests, scan réseau...)
    interface.xaml        la fenêtre et tous les onglets
    donnees.ps1           sauvegarde, journal, accès au registre
    optimisations.ps1     réglages gaming, programmes au démarrage, jeux installés
    systeme.ps1           connexion active, nettoyage, restauration, désinstallation
    interface.ps1         chargement de la fenêtre, aides, travail en arrière plan
    tableau-de-bord.ps1   constats, score, fiches de correction, retour en arrière
    analyse.ps1           santé des composants et analyse complète
    onglets.ps1           mesures en direct, Gaming, Démarrage, Connexion, Nettoyage, Sauvegarde
    visuels.ps1           animations, jauges, courbes
    tests.ps1             onglet Tests
    securite.ps1          onglet Sécurité
    navigation.ps1        accueil « Ordinateur » et navigation
    reseau.ps1            section Réseau : scan et fiche appareil
    audit-reseau.ps1      audit de sécurité du réseau
    mises-a-jour.ps1      mises à jour depuis GitHub
    assistance.ps1        historique, signaler un problème, notifications, visite guidée
    jeu.ps1               mode jeu automatique, profils par jeu, alerte de température
    evenements.ps1        branchement des boutons
outils/
  construire.ps1     génère l'icône, compile OptiGame.exe et le désinstalleur, crée OptiGame.zip
  publier.ps1        publie une nouvelle version sur GitHub
  icone.ps1          dessine l'icône
  lanceur.cs         code du lanceur OptiGame.exe
```

Tester (copie isolée de l'app, hors écran, rien n'est modifié sur le PC) :

```
.\outils\tester.ps1              # toutes les pages
.\outils\tester.ps1 -Complet     # + réseau, audit, lag en charge
```

Publier une mise à jour (vérifie la syntaxe, les tirets et lance le test avant) :

```
.\outils\publier.ps1 -Version 1.1 -Notes "Ce qui change"
.\outils\publier.ps1 -Version 1.2 -Notes "..." -Beta   # seulement pour ceux qui ont activé les bêtas
```
