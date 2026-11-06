# Bloc « perf » : différences avec la version PowerShell

Référence : `Nevermind/fichiers/modules/ecran-perf.ps1` (Get-PagesPerf sans la page RAM), la boucle des mesures en
direct d'`onglets.ps1`, `jeu.ps1` (mode jeu, partie, mesure des FPS, compteur), `diagnostic-fps.ps1`. Juges :
`outils/exporter-juges-perf.ps1` vers `tests/juges/*.json` (1816 cas), rejoués par `tests/juges.rs`.

## Différences voulues (défauts corrigés)

1. **Mode jeu : aucune appli cochée.** En PowerShell, `Get-Setting 'GameModeApps'` rend une liste vide comme `$null`
   (une fonction PowerShell déroule un tableau vide), et `Get-GameModeSelection` repart alors de la liste par défaut :
   tout décocher fermait quand même OneDrive, Teams, Skype, Adobe, Dropbox et Google Drive au début d'une partie, et la
   page les montrait de nouveau cochées. Preuve : les cas de `tests/juges/mode-jeu.json` et `pages.json` dont
   `appsModeJeu` vaut `[]` attendent les 6 applis par défaut. En Rust, `[]` veut dire « aucune » (test
   `aucune_appli_cochee`) ; ces juges sont rejoués avec ce que PowerShell lisait vraiment (absent).
2. **Mesures en direct bien plus légères** (le but de la nuit). Avant : chaque seconde 5 requêtes WMI (dont
   GPUEngine, 630 compteurs par lecture sur le PC de Jordan) et `nvidia-smi.exe` lancé à chaque tour ; écran caché,
   4 requêtes et nvidia-smi toutes les 10 s, même en partie. Après : GetSystemTimes, GlobalMemoryStatusEx,
   D3DKMTQueryStatistics (temps de travail de chaque moteur de carte, le moteur le plus occupé comme avant), nvml.dll de
   System32 chargée une fois, deux compteurs PDH ouverts une fois (noms anglais). Écran caché : la température seule
   (l'alerte), toutes les 10 s, toutes les 5 s quand la carte dépasse 87 °C ; plus de processeur, mémoire ni disque.
   Pendant une partie mesurée, jeu devant : un relevé complet frais toutes les 5 s pour le diagnostic (avant : la
   dernière valeur de `$Live`, vieille jusqu'à 10 s) ; jeu derrière : rien. Pendant une partie, l'écran compte comme
   caché tant que Nevermind n'est pas au premier plan (Test-Regardee).
3. **Alerte de température** : écran caché, la mesure est toutes les 5 s quand la carte chauffe, donc l'alerte arrive
   entre 15 et 20 s au lieu de 15 s pile (le reste est identique, juge `temperature.json`).

## Différences de forme (même résultat pour l'utilisateur)

- **Fermeture des applis du mode jeu** : par l'Aide (`processus.fermer` avec le nom exact, la même règle commune que
  la page RAM), au lieu de `Kill()` sur place puis l'Aide pour les programmes administrateur. Mêmes refus.
- **Mesure des FPS** : seulement par l'Aide (`fps.demarrer`, `fps.lire` toutes les 250 ms, `fps.arreter`). Le repli
  sur place de PowerShell (PresentMon lancé par Nevermind, contrôle de signature) n'est pas porté : sans droits il est
  refusé par Windows, et c'est lui qui lançait un programme.
- **Veille avant les jeux à anti-triche** (`fps.veiller`) : l'Aide n'est pas réveillée pour elle (PowerShell regardait
  seulement si elle répondait) : appel si l'Aide a déjà répondu, sinon un essai toutes les 10 minutes. La trace réseau
  (`flux.veiller`, lag.ps1) est au bloc réseau.
- **Bilan de fin de partie** : calculé dans un autre fil (comme BilanAsync), relevé au passage suivant.
- **Compteur par-dessus le jeu** : fenêtre Win32 native (UpdateLayeredWindow, transparente pixel par pixel, souris qui
  passe au travers, jamais le focus, hors Alt+Tab), dans son propre fil créé à la première demande. Police Consolas
  (au lieu de « Cascadia Code, Consolas ») ; la police et le logo d'un pack de thème ne sont pas repris. Placé à 16 px
  du coin de l'écran de la fenêtre au premier plan, remis devant toutes les 3 s, caché en plein écran exclusif et quand
  le jeu n'est pas devant, comme avant. **Pas essayé à l'écran** (aucun bureau à part n'était prêt pour cet essai).
- **Bulles** : Show-Notify devient le signal `notification` {titre, texte, action} (l'hôte montre la bulle ou la passe
  à Mindset) ; Set-Status devient `Effet::Annonce`.
- **mode-jeu.json** : `Start` au format de `horodatage()` du noyau (sans le décalage horaire de `ToString('o')`) ; il
  n'est relu que pour savoir si la partie continue.
- **Nombres** : la culture fr-FR est fixe (espace fine insécable U+202F, lue sur le PC de Jordan), comme les juges.
- **Pages** : comme `vue.ps1` avant d'envoyer au kit (`-replace '":\{\}', '":null'`), un « rien » de PowerShell est
  `null`.

## Entrée des pages (`pages::pages`, la même forme que les juges)

Ce que le bloc sait lui-même : `ecran`, `dernier` {jeu, moy} (jeux.json et fps.json), `disque` {total, libre} et
`lecteur`, `dernierNettoyage` (réglage LastClean), `ram` {utilisee, totale} (mesures en direct), `modeJeu`,
`appsModeJeu`, `processus` (noms, gardés 20 s), `session` {jeu, fermees}, `jeux` (avec `prio` et `gpu` lus dans le
registre), `gpus`, `nLib`, `profilsLus`.

Ce que les autres blocs lui envoient par signal (le champ `donnees` du signal) :

| Signal | Contenu | Envoyé par |
|---|---|---|
| `jeux-liste` | `[{nom, exes:[chemins], source, custom}]` (ou `{jeux:[...]}`) : la liste des jeux (Update-GameCache) | jeux |
| `reglages-jeu` | `{lignes:[{titre, description, impact, recommande (true/false/null), redemarrage, active (pas encore fait), coche}], restauration}` | pc |
| `demarrage` | `{entrees:[{nom, actif, genre (safe/keep/choice), etiquette, pourquoi, image}], texte}` | pc |
| `nettoyage` | `{lignes:[{titre, desc, taille, active, coche}], texte, bouton}` | pc |
| `connexion` | `{internet:{moy, statut}|null, bridage}` | réseau |
| `analyse-pc` | `{analyse:[{id, statut, titre, detail}], gpus:[noms], portable, onduleur, disques:[{lettres, bus, media}], nvidia:{installe, dernier, url}, planOk}` | pc |
| `memoire-constat` | `{pourcent, alertes}` | mémoire |
| `diagnostic-fps` | `{Id}` : la partie dont il faut le diagnostic ; réponse `diagnostic-fps-pret` {Id, diagnostic} | jeux |

## Signaux envoyés

- `partie-commencee` {jeu, exe, pid} et `partie-finie` {jeu, exe, pid, duree_s} (règle commune du coordinateur) :
  temps de jeu, « En jeu » de la bibliothèque, lag, mémoire.
- `mesures` (nom aligné sur le bloc pc, câblage du 06/11 ; reçu : `mesures-rapides` {on} du bloc pc pendant un test
  des composants, `$Live.Fast` : toutes les 400 ms, même écran caché ; `profils-changes` du bloc jeux : profils relus ;
  la tuile de l'accueil : `_accueil` {perf: {sous}}, le même compte que la page) {Cpu, CpuPerf, RamTotal, RamUsed, Gpu, GpuTemp, VramUsed, VramTotal, GpuPower, GpuPowerLimit,
  DiskBusy, Updated} : chaque seconde, seulement écran regardé (les tuiles de Ton PC, les tests).
- `fps-partie-enregistree` : la partie ajoutée à fps.json (bloc jeux : Mes parties).
- `notification` {titre, texte, action} ; `ajouter-jeu` {} (Add-CustomGame, bloc jeux) ; `ouvrir-fiche` (la fiche de
  l'analyse d'un conseil) ; `declarations` {changements, historique} (profil par jeu : Set-Declarations et
  Add-History, au bloc qui les tient).

## Pas porté ici (appartient à un autre bloc)

Réglages Windows (Build-GamingTab, Invoke-ApplyTweaks, `tweak`, `opti:recommande`, `clic:BtnApply`), démarrage
(Update-StartupList, `dem:k`, lecture des programmes au démarrage : `demarrage-programmes` n'est pas envoyé par ce
bloc), nettoyage (`nett`, `nett-voir`, `nett-journal`), connexion, sauvegarde : bloc pc / réseau. Carte réseau
(Watch-CarteReseau dans Update-LiveUI) : bloc réseau. Pages FPS, compteur (réglages FpsMeasure, FpsOverlay,
FpsOverlayStyle, FpsOverlayCorner) et fiche d'une partie : bloc jeux ; ce bloc relit ces réglages à chaque passage.
