# Bloc « memoire » : différences avec la version PowerShell

Tout le reste est identique au champ près (juges de `outils\exporter-juges-memoire.ps1`, `tests\juges.rs`).

## 1. Le chemin des programmes que WMI ne donne pas (voulu)
`Win32_Process.ExecutablePath` est vide pour certains processus (lancés en administrateur, d'un autre compte ou
protégés). La version Rust lit le chemin avec `QueryFullProcessImageName` (le même appel que `Read-AideFiche`), qui
le donne aussi pour eux. Ces applis ont donc leur vrai nom (celui de leurs informations de version) au lieu du nom du
programme.
Preuve (PC de Jordan, 06/11/2026, lecture seule) : 10 processus sur 352 seulement. Par exemple TabTip.exe devient
« Clavier tactile et volet d'écriture manuscrite », DefenderSessionHelper.exe devient « Microsoft Defender Session
Helper », et LEDKeeper2.exe ou audiodg.exe ont leur chemin. La règle « fermable » n'est pas touchée : elle se servait
déjà de ce chemin-là (`Image`).

## 2. L'ordre de deux applis qui prennent exactement la même mémoire (sans effet visible)
PowerShell les range dans l'ordre de sa table de hachage, qui ne veut rien dire. Rust garde l'ordre de la lecture. Les
juges rangent ces égalités de la même façon des deux côtés avant de comparer (les numéros des boutons « Fermer » sont
comparés par appli, pas par place).

## 3. Des données sans champ « Etiquettes » (cas qui n'arrive jamais)
Avec PowerShell, `@($null)` compte un élément : un rapport fait sur des données sans ce champ montre une section
« ETIQUETTES » avec une ligne vide. `Get-DonneesMemoire` met toujours ce champ, donc cela n'arrive jamais pour de vrai.
Rust n'invente pas cette ligne. Seuls les juges « données vides » et « rien du tout » le montrent (le rapport n'y est
pas comparé).

## 4. Ce que lit la lecture, et comment (plus léger, même résultat)
- Les processus et leur mémoire propre : un seul appel `NtQuerySystemInformation` au lieu de `Win32_Process` et
  `Win32_PerfRawData_PerfProc_Process`. Comparé sur le vrai PC au même moment : 357 processus sur 358 dans
  l'encadrement des deux lectures WMI (avant, après), total 11 404 Mo contre 11 390 Mo.
- La ligne de commande n'est lue que pour Edge et Chrome (le seul endroit où elle sert : « --no-startup-window »).
- Le type de démarrage d'un service n'est lu que pour les services de la liste « inutile » (le seul endroit où il
  sert : les corrections). Les autres ont `Demarrage` vide dans les données, sans effet sur le constat ni la page.
- Les compteurs de la mémoire (cache, pilotes) : PDH au lieu de WMI, mêmes compteurs.
- Mémoire installée : `GetPhysicallyInstalledSystemMemory` (même valeur que `Win32_PhysicalMemory` sur le PC de Jordan :
  34 359 738 368 octets).

## 5. Le rapport en mode essai
En mode essai (`ctx.essai`) sans `ESSAI_DOSSIER`, le rapport va dans le dossier des données d'essai, jamais sur le
vrai Bureau (la version PowerShell l'aurait posé sur le Bureau).

## 6. Un geste refusé par Windows pendant « Couper ce qui est sûr »
Le message d'un service absent est écrit par Nevermind (« Impossible de trouver un service assorti du nom « x ». »),
comme celui de `Get-Service` en français, au lieu d'être pris dans PowerShell.

## 7. Les programmes au démarrage (câblage du 06/11)
Le signal « demarrage-programmes » du bloc pc donne la liste de la page Démarrage (`{ Nom, Commande, Exe, Enabled,
Source }`) ; ce bloc en tire les noms comme memoire.ps1:671 (activés avec un .exe : le nom du .exe et ceux que nomme
leur commande, en minuscules, sans doublon : `programmes_demarrage`).
