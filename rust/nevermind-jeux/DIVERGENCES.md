# Bloc jeux : différences voulues avec la version PowerShell, et ce que le bloc attend des autres

## Différences voulues (défauts corrigés), avec la preuve

1. **Priorité haute d'un jeu à un seul programme.** `Test-GamePriority` fait `$names[0]` sur le résultat de
   `Get-GameExeNames` : avec un seul nom, PowerShell rend une chaîne, et `$names[0]` est sa première lettre (« R » au lieu
   de « RocketLeague.exe »). La priorité n'est donc jamais lue : l'interrupteur reste coupé même après l'avoir allumé.
   Preuve : juge `pages.json`, cas « fiche rocket » (registre simulé avec CpuPriorityClass = 3 : PowerShell affiche
   « coupé » et 4 points à améliorer). Rust lit la bonne clé (3 points, interrupteur allumé) ; le test le note.
2. **Onglet Lag en ligne vide.** La page `jeux` laisse `onglets[perf].onglets[lag]` à `{ id, nom }` : le bloc trafic
   donne la page `jeux-lag` {module, blocs}, que l'hôte met à cette place (consigne du coordinateur).
3. **Lancer un jeu « exe ».** La version PowerShell crée un raccourci .lnk temporaire puis l'ouvre par l'Explorateur.
   Rust lance le programme dans son dossier (`Command`, CREATE_NO_WINDOW) : même dossier de travail et mêmes
   arguments. Si l'appli Rust tourne en administrateur, le jeu hériterait de ces droits : à revoir avec l'hôte.
4. **Le panneau des restes de jeux et le détail d'une partie** (Show-Leftovers, Show-FpsSession) étaient des panneaux
   WPF traduits à la volée par ecran-panneau.ps1. Rust donne directement une page `panneau` avec les mêmes textes et
   boutons (pas de juge possible sans WPF). Identifiants : `jeux-restes-fermer`, `jeux-restes-tout`,
   `jeux-reste-ouvrir:k`, `jeux-reste-supprimer:k`, `panneau-fps-fermer`, `fps-ajouter`. Les actions proposées par le
   diagnostic d'une partie (boutons « Régler... ») ne sont pas encore branchées.
5. **Questions et messages.** Confirm-Action devient le signal commun `question` (réponse par l'action `jeux-reponse`
   {id, valeur}) ; Show-Message et Set-Status deviennent `Effet::Annonce`.
6. **Images.** Get-ImageVue donnait un jeton au hasard ; Rust donne un jeton fixe par fichier (empreinte du chemin).
   L'hôte sert le fichier avec `BlocJeux::image(jeton)`.
7. Anciennes mesures de lag (avant 1.0.119) dont le diagnostic était refait : sans objet ici (le lag est au bloc trafic).

Tri : Sort-Object n'est pas stable ; Rust reproduit exactement son algorithme (l'ancien tri rapide de .NET Framework,
vérifié sur 40 éléments à égalité) et sa comparaison sans casse, pour que les égalités sortent dans le même ordre.

## Pas porté (et pourquoi)

- Jaquettes depuis Internet (Start-CoverDownload, boutique Steam puis Wikipédia) : demande le réseau, et la consigne
  de cette nuit est hors ligne. L'interrupteur et le réglage `LibCoversOnline` sont là ; les jaquettes déjà
  téléchargées (jaquettes\index.json) sont lues.
- Organizer : la vue, la barre flottante, la veille des fenêtres Dofus et les raccourcis pris à Windows ont besoin de
  l'hôte (fenêtres, messages de Windows). Porté : la règle de visibilité (exactement Test-OrgVisible, réglage
  `OrganizerAffiche`), organizer.json, la lecture des titres, l'ordre d'initiative, suivant / précédent / n-ième perso,
  les noms et codes des touches (module `organizer`).
- New-GameCover, Get-BigIcon, Get-GameIcon (vignettes dessinées en WPF) : le kit dessine les vignettes lui-même.

## Signaux envoyés

- `jeux-liste` {jeux:[{nom, exes, source, dossier, appid, custom}]} (bloc perf, bloc divers), `jeux-bibliotheque`
  {exes} (bloc mémoire), `jeux-index` {Exes} (bloc trafic) : au démarrage et à chaque changement de la liste.
- `question` (commun), `resultat` (commun), `declarations` {titre, changements} (profil par jeu : Set-Declarations et
  Add-History au bloc qui tient la sauvegarde), `profils-changes` {jeu}, `reglage-change` {nom, valeur} (FpsMeasure,
  FpsOverlay, FpsOverlayStyle, FpsOverlayCorner, GameMode), `appliquer-reglages-windows` {ids} (bloc perf ou pc :
  Invoke-TweakFix), `diagnostic-fps` {Id} (bloc perf), `organizer-ouvrir`, `organizer` {on}.

## Signaux attendus

- `analyse` {GPUs:[{Name}], Disks:[{Letters, Disk:{MediaType}}]} ($script:AnalysisData, bloc pc) ;
  `reglages-windows` {reglages:[{Id, Titre}]} (réglages conseillés pas faits, bloc perf/pc).
- `partie-commencee` {jeu, exe, pid}, `partie-finie` {jeu, exe, pid, duree_s} (bloc perf) : « En jeu » et temps de jeu
  (jeux.json, Add-PlayTime).
- `diagnostic-fps-pret` {Id, diagnostic}, `fps-partie-enregistree`, `ajouter-jeu` (bloc perf) ;
  `jeux-restes-ouvrir`, `jeux-rechercher` (bloc divers, hôte).

## Ce dont le bloc a besoin de l'hôte

Servir les images `https://fichier.local/<jeton>` (`BlocJeux::image`), poser les questions (`question`), montrer
`resultat`, mettre la page `jeux-lag` du bloc trafic dans l'onglet Lag, et l'Organizer (fenêtres, raccourcis).
