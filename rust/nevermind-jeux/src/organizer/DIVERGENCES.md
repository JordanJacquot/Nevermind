# Organizer : différences voulues avec la version PowerShell (organizer.ps1, natif.cs)

À reporter dans `rust/nevermind-jeux/DIVERGENCES.md` par le coordinateur (ce fichier est à part pour ne pas toucher
au fichier commun pendant que l'agent « câblage » y travaille).

## Porté à l'identique (et vérifié par les juges)

Get-OrgConfig, Save-OrgConfig, ConvertFrom-DofusTitle, Get-DofusWindows (même règle que MainWindowHandle de .NET : la
première fenêtre visible sans propriétaire de chaque programme, programmes rangés par nom puis par numéro comme
Get-Process), Get-OrgList, Get-OrgOnline, Move-OrgChar, Remove-OrgChar, Get-OrgTarget (avec ses bizarreries : « P0 »
donne le dernier perso), Show-OrgChar, ConvertTo-OrgHotkey (le KeyConverter de WPF puis Enum.Parse, table complète
des 202 noms de touche), ConvertTo-OrgMouse, Format-OrgKey, Register-OrgHotkeys, Unregister-OrgHotkeys,
Start-OrgWatch, Stop-OrgWatch, Update-OrgWatch, Test-OrgVisible, Set-OrgVisible, Set-OrgOn, Get-OrgBreedId,
Get-OrgColor, Receive-OrgKey, Receive-OrgMouse, Add-OrgModifiers, Set-OrgCapturedKey, Show-OrgBar, Update-OrgBar,
Hide-OrgBar, Set-OrgBarOnTop, Reset-OrgBarPosition, Test-OrgBarOnScreen, Get-OrgBarRadius, Build-OrgPanel (textes),
New-OrgKeyRow, New-OrgKeyButton ; WinFocus (Focus, Foreground, Pid, KeepOnTop, NoActivate), OGNative.AddHotKey
(MOD_NOREPEAT), MouseHook (écoute sur son propre fil, bouton avalé et son relâchement aussi).

## Différences voulues

1. **La vue est une fenêtre à elle.** Dans la version PowerShell, la vue remplaçait la bibliothèque dans la fenêtre
   WPF. L'appli Rust n'a plus de fenêtre WPF (l'écran est le kit) : la vue s'ouvre dans sa propre fenêtre « Organizer
   Dofus », avec les mêmes cartes, textes, interrupteurs et boutons. « Retour à mes jeux » devient la croix de la
   fenêtre. Elle s'ouvre par l'action `jeux-organizer` (page Jeux) et le signal `organizer-ouvrir` (recherche).
2. **Les logos des classes** (téléchargés depuis DofusDB par Get-OrgClassIcon) ne sont ni téléchargés (consigne hors
   ligne de cette nuit) ni montrés : la pastille garde l'initiale sur la couleur de la classe (le repli de
   New-OrgBadge). À faire : lire `organizer\classe-N.png` s'il existe déjà, puis le téléchargement.
3. **Interrupteurs de la barre, Organizer coupé.** « Afficher la barre » et « Toujours visible » appelaient
   `Update-OrgWatch -Full` même Organizer coupé (Build-OrgPanel, lignes 805-806) : avec « Toujours visible », la barre
   apparaissait et restait affichée, Organizer coupé, puisque la minuterie ne tournait pas pour la cacher. Rust ne
   refait la passe que si l'Organizer est allumé (`Veille::regler_barre`). Les juges tirent ces gestes Organizer
   allumé.
4. **Thème.** Les couleurs de la marque (cyan, violet, magenta) prennent celles du thème, et le thème Terminal garde
   ses coins carrés et sa police de code, comme avant. Pas portés : la teinte des gris propre à chaque thème
   (NH, NS), et les polices des packs de thème (Set-OverlayFont, police pixel).
5. **Dessin.** La barre est une fenêtre « en couches » dessinée pixel par pixel (mêmes couleurs, rayons, marges et
   tailles que le XAML) ; l'animation du bord de plasma est la même (un tour en 9 s, 20 images par seconde, arrêtée
   barre cachée) ; la lueur du perso devant est calculée une fois par changement, pas à chaque image. Les
   infobulles sont celles de Windows.
6. **Glisser une ligne** se fait avec la souris tenue sur la poignée (pas par le glisser-déposer OLE de WPF) ; la
   ligne visée s'allume en cyan comme avant.
7. **Messages.** Set-Status devient une bulle du kit (`Effet::Annonce`), rendue au tic du bloc (une seconde au plus
   tant que le fil tourne). Write-Log : journal.txt, par la même file.
8. **Une erreur dans un geste** (fenêtre de la barre ou de la vue) est notée au journal (« Organizer : erreur ») et
   le geste abandonné : elle ne ferme jamais Nevermind (comme Invoke-Safe).
9. **La veille est plus légère.** Get-Process -Name 'Dofus*' toutes les 2 s (8,8 ms mesurées sur le PC de Jordan,
   sans jeu ouvert) devient une liste des fenêtres de Windows (EnumWindows) et le nom d'un programme cherché une
   seule fois par fenêtre (0,005 ms par recherche sur le bureau d'essai). Le titre des fenêtres vues est noté au
   journal à partir de la même recherche (la version PowerShell refaisait Get-Process pour cela : même résultat).
10. **Le fil.** Il démarre au premier tic du bloc si l'Organizer était allumé (version PowerShell : au repos de la
    fenêtre, juste après le démarrage), et s'arrête dès que l'Organizer est coupé et sa vue fermée : rien ne tourne
    alors. Mode test (`ctx.essai`) : il ne démarre jamais.
11. **Les touches simulées** (12/11, nouveau, pas dans la version PowerShell). Trois lignes « Bouton 1 à 3 » : un
    raccourci (un bouton de souris le plus souvent : S1 à S3 dans « Hotkeys », identifiants 7131 à 7133) et la touche
    qu'il fait dans le jeu (« Sends », choisie au clavier seulement). Le raccourci n'est pris que jeu devant, comme les
    autres ; la touche part dans la fenêtre devant seulement (SendInput, appuyée 60 ms, avec le code de la touche
    physique), jamais vers plusieurs fenêtres. Une touche qui est déjà un raccourci de l'Organizer n'est pas envoyée.
    « Sends » n'est écrit dans organizer.json que s'il y en a. Essai (`p.essai`) : la touche est notée, jamais envoyée.
12. **La vue sur un seul écran** (12/11, maquette « E1 » choisie par Jordan). Trois colonnes : les persos CONNECTÉS    seulement (plus de persos déconnectés ni de bouton « Retirer » ; l'ordre d'initiative reste gardé sans être montré),    chacun avec son logo de classe et sa touche (la liste « Perso 1 à 8 » n'existe plus) ; raccourcis et barre    flottante ; boutons de souris en pastilles (20 au plus, S1 à S20) créés en deux grandes cases (le bouton de souris,    puis la touche du jeu). Les logos des classes reviennent comme dans la version PowerShell (DofusDB, gardés dans    `<données>organizerasse-<n>.png`, lus par WIC ; jamais téléchargés pendant un essai).

## Signaux

- Attendu : `organizer-ouvrir` (bloc divers, recherche) : la vue s'ouvre.
- Envoyés (inchangés) : `organizer-ouvrir` (action `jeux-organizer`), `organizer` {on: false} (Organizer caché depuis
  Paramètres quand le fil ne tourne pas ; s'il tourne, c'est lui qui coupe l'Organizer et le dit).
