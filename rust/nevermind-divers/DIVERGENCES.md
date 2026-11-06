# Bloc divers : différences voulues avec la version PowerShell, et ce que le bloc attend des autres

## Différences voulues (défauts corrigés), avec la preuve

1. **Onglet Thème des Paramètres, géré par Mindset.** `$theme = if ($Managed) { @(@{...}) } else {...}` : PowerShell
   déplie la liste d'un seul élément et envoie `blocs` en objet au lieu d'une liste. Rust envoie la liste. Preuve :
   juge `pages.json`, cas 1 et 4 (geree = vrai) ; le test le note.
2. **Questions.** Le choix d'un thème (copie autonome) pose le signal commun `question` (réponse par l'action
   `divers-reponse` {id, valeur}) ; « oui » envoie `theme-choisi` {id} à l'hôte (Select-AppTheme et relance).
3. **Arrivée sur un réglage** (Open-SearchEntry) : la page du kit s'ouvre (`Effet::OuvrirEcran`), le texte de Set-Status
   devient une annonce ; le sous-onglet d'Optimisation gaming et la sélection d'un jeu visaient la fenêtre d'avant
   (cachée), ils n'ont pas d'effet à l'écran ni ici. Les gestes qui suivent deviennent des signaux :
   `jeux-restes-ouvrir` (bloc jeux), `reseau-carte` (bloc réseau), `trafic-microsoft` (bloc trafic), et
   `organizer-ouvrir` pour les réglages de l'Organizer.

Tri des résultats : même algorithme que Sort-Object (voir le bloc jeux, `tri.rs` est une copie : les blocs ne dépendent
pas les uns des autres).

## Actions prises

`recherche` (Register-ActionsNevermind d'accueil-console.ps1, mais ses données sont celles de la page Rechercher),
`res:<k>`, `visite` (signal `visite` à l'hôte), `theme-mindset`, `pilote-nvidia`, `maj-auto`, `nouveautes-en-ligne`,
`theme`, `divers-reponse`. Pas prises (autres blocs ou hôte) : `coche:ChkNetWatch` (réseau), `modejeu` (perf),
`fps-mesure`, `org-affiche`, `coche:ChkLibCovers` (jeux), `lag-mesure`, `lag-pays`, `trafic-annuaire`,
`trafic-microsoft` (trafic), `clic:BtnShortcut`, `coche:ChkAutoStart`, `coche:ChkBeta`, `clic:BtnCheckUpdate`,
`clic:BtnReportProblem` (hôte).

## Signaux attendus

- `etat-hote` {version, maj_texte, maj_bouton, raccourci_texte, raccourci_bouton, demarrage_ligne, demarrage_coche,
  theme_cle, theme_nom, themes:[{id, titre, texte}]} : ce que montrait la fenêtre d'avant ($AppVersion,
  $ui.UpdateStatus, $ui.ShortcutStatus, $ui.AutoStartRow, thèmes et packs). Sans lui : thème lu dans le réglage
  « Theme » parmi les 5 thèmes de base, la version vide.
- `jeux-liste` (bloc jeux) : les 80 premiers jeux dans la recherche, et l'Organizer proposé si Dofus est installé.
- `reglages-windows-liste` {titres:[...]} (bloc perf/pc) : les réglages Windows dans la recherche ($script:TweakRows).

Les pages d'assistance (`pages_assistance` du noyau) ne sont pas encore là : la ligne « Signaler un problème » de l'Aide
garde l'action `clic:BtnReportProblem` pour l'hôte.
