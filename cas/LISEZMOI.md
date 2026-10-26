# Le corpus de cas

Les cas de l'Aide de Nevermind, écrits en données (JSON) plutôt que dans le code des tests. La même liste sera passée à
la version PowerShell d'aujourd'hui (les tests `outils\tester-aide*.ps1` lisent ces fichiers) et à la version Rust
(`cargo test`, au travers d'une couche « Windows » remplaçable). Les deux versions doivent répondre pareil à chaque cas
(plan Rust, point 3.5).

Un cas se change dans le fichier, jamais dans le test. Un cas ajouté ici est essayé par les deux versions.

## Le format (commun avec `cas\installateur` du dépôt Mindset)

Un fichier = un sujet, en UTF-8 sans BOM, un objet JSON :

- `format` : 1 (changé seulement si un champ change de sens) ;
- `sujet` : ce que le fichier vérifie, en une phrase ;
- `regle` : où est la règle dans le code PowerShell d'aujourd'hui (fichier : fonction et ses arguments) ;
- `attendu` : le sens de chaque champ de `attendu` dans les cas ;
- `valeurs` (facultatif) : les valeurs écrites `{nom}` dans les cas (minuscules, tirets simples), avec ce qu'elles
  valent sur le PC d'essai. Elles sont remplacées dans les cas seulement : un texte égal à `{nom}` prend la valeur telle
  quelle (un nombre reste un nombre, par exemple `{carte}`), un texte qui la contient la reçoit en texte (par exemple
  `{windows}\System32\svchost.exe`). Une valeur que le test ne donne pas l'arrête ;
- `champs`, `note`, `calcul` (facultatifs) : explications ;
- une base commune (facultatif, ici `contexte` et `fiche` dans `fermeture.json`) et `fusion` : chaque cas part de la
  base, et un champ cité dans le cas remplace la valeur entière de la base. Un champ mis à `null` vaut « absent »
  (illisible, inconnu) ;
- `cas` : la liste des cas, un par ligne.

Un cas :

- `nom` : texte court (il apparaît dans le compte rendu du test). Dans `demandes.json`, c'est le nom du groupe de
  demandes, partagé par plusieurs cas ;
- `entree` : tout ce qui est donné à la règle (demande, paramètres, état du faux PC, client) ;
- `attendu` : le résultat ;
- `besoin` (facultatif) : ce qu'il faut sur le PC d'essai (`carte` : une vraie carte réseau). Sans lui, le cas est
  sauté et compté à part.

Règles :

- aucun chemin du vrai PC : seulement des `{valeurs}` ou des chemins inventés ;
- chaque test qui lit le corpus donne le nombre de cas qu'il attend au moins : un fichier vidé ou coupé fait échouer
  le test (`outils\lire-cas.ps1`, `Read-Cas ... -Minimum`) ;
- ce qui ne s'exprime pas en données reste dans le test : lecture du code lui-même, vrais programmes lancés, ce qui
  dépend de ce PC (antivirus inscrit, services, DNS de la box).

## Les fichiers de `cas\aide`

| Fichier | Règle | Lu par |
|---|---|---|
| `demandes.json` | Les demandes à l'Aide : acceptées ou refusées (`Invoke-AideAction ... -Verifier`) | `tester-aide.ps1` |
| `fermeture.json` | La règle « fermable » (`Get-RefusFermeture`) : services, antivirus, protégés, Windows, Store, suite par emplacement, élevés, autre compte ou session, doute | `tester-aide.ps1` |
| `canal.json` | Le nom du canal et du verrou de l'Aide (`Get-AideSuffixe`), dont un dossier avec un accent et une espace | `tester-aide.ps1` |
| `client.json` | Qui peut parler à l'Aide (`Test-AideClient`) | `tester-aide.ps1` |
| `restauration.json` | Un point de restauration par 24 h au plus | `tester-aide.ps1` |
| `taches-compte.json` | Les tâches planifiées montrées : celles du compte et du système | `tester-aide.ps1` |
| `demarrage-securite.json` | Sécurité Windows et les antivirus jamais coupés au démarrage | `tester-aide.ps1` |
| `tache.json` | Nevermind ne lance que la tâche SYSTEM de son utilisateur (`Get-AideEtatTache`) | `tester-aide-systeme.ps1` |
| `serveur.json` | Nevermind reconnaît l'Aide au propriétaire du canal (`AideClient.ServeurAccepte`) | `tester-aide-systeme.ps1` |
| `variables.json` | Les variables lues par .NET au démarrage qui bloquent la demande de Windows | `tester-aide-systeme.ps1` |
| `environnement.json` | De vraies demandes à une vraie Aide de test, dans un environnement piégé | `tester-aide-environnement.ps1` |

## Points à garder en tête pour la version Rust

- `canal.json` : la mise en minuscules est celle de .NET (`ToLowerInvariant`) : `İ` (I pointé) reste `İ`, alors que
  `to_lowercase` de Rust le change en `i` suivi d'un point. Aucune normalisation des accents : `é` écrit en un ou en
  deux caractères donne deux noms différents. Les cas le fixent.
- `fermeture.json` : la raison du refus est comparée au texte exact (il est montré à l'utilisateur).
- `variables.json` : les noms sont comparés sans tenir compte des majuscules, comme Windows.
- `taches-compte.json` : le compte `SYSTEM`, écrit par son nom, est traduit en SID par Windows.
