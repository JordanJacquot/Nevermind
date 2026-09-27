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

- **Tableau de bord** : mesures en direct, santé de chaque composant, score et liste de ce qui peut être amélioré.
- **Fiches de correction** : ce qui a été trouvé, ce que l'app va faire, bouton Exécuter, puis « Revenir en arrière » si besoin.
- **Optimisation gaming** : plan d'alimentation, mode jeu, Game Bar, planification GPU, accélération de la souris...
- **Tests des composants** : vitesse et santé de chaque disque, processeur (puissance, stabilité), mémoire, capteurs de la carte graphique, débit Internet, pixels morts.
- **Démarrage**, **Réseau** (ping, gigue, DNS), **Nettoyage**, **Sauvegarde** (tout annuler, point de restauration, rapport HTML).
- **PC portables** : mode « Meilleures performances », jeux forcés sur la carte graphique dédiée, santé de la batterie.

Aucune modification n'est faite sans clic de l'utilisateur, et chaque réglage modifié est sauvegardé avant d'être changé.

## Développement

```
OptiGame/            l'application telle qu'elle est distribuée
  fichiers/OptiGame.ps1   le code de l'application (PowerShell + WPF)
outils/
  construire.ps1     génère l'icône, compile OptiGame.exe et le désinstalleur, crée OptiGame.zip
  publier.ps1        publie une nouvelle version sur GitHub
  icone.ps1          dessine l'icône
  lanceur.cs         code du lanceur OptiGame.exe
```

Publier une mise à jour :

```
.\outils\publier.ps1 -Version 1.1 -Notes "Ce qui change"
```
