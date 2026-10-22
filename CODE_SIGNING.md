# Code signing policy

*Version française plus bas : [Politique de signature du code](#politique-de-signature-du-code).*

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

> **Status:** the application to SignPath Foundation is in progress. Until it is accepted, the releases are not
> Authenticode signed, and the notes of each [Release](../../releases) give the SHA-256 of `OptiGame.zip`.

## What is signed

- `Nevermind.exe`, `Désinstaller Nevermind.exe`, `NevermindAide.exe` and the PowerShell scripts (`.ps1`) of
  Nevermind, all built from this repository.
- Files from other authors are never signed with this certificate: the Microsoft WebView2 libraries
  (`fichiers\lib\webview2`, already signed by Microsoft Corporation), PresentMon (`fichiers\outils-tiers`, already
  signed by Intel Corporation, MIT license) and Three.js (JavaScript, MIT license).

## How a release is built and signed

1. The version is tagged in this repository (`v<version>`).
2. The [Release workflow](.github/workflows/release.yml) builds the three executables on a GitHub hosted runner, from
   the tagged source only, and uploads the `OptiGame` folder as a workflow artifact (the folder and the zip keep the
   former name of the app, which installed copies look for).
3. The workflow sends a signing request to SignPath. SignPath checks that the artifact comes from this workflow and
   this repository. **Every signing request is approved manually** by an approver (see below).
4. The maintainer downloads the signed `OptiGame.zip` produced by the workflow, adds the detached RSA signature
   `OptiGame.zip.sig` (Mindset and Nevermind use it to verify updates; it does not change any file inside the zip)
   and publishes the Release.

## Team and roles

| Role | Members |
|---|---|
| Committers and reviewers | [Jordan Jacquot](https://github.com/JordanJacquot) |
| Approvers | [Jordan Jacquot](https://github.com/JordanJacquot) |

- Committers can change the code of this repository. Changes proposed by anyone else (pull requests) are reviewed
  by a committer before they are merged.
- Every signing request is approved by an approver.
- All team members use multi-factor authentication on GitHub and on SignPath.

## Privacy policy

Nevermind has no account, no advertising, no telemetry and no statistics. It never reads the content of your
connections and it does not send any personal data. The table below lists every connection that Nevermind makes
to systems on the Internet, and how to turn it off. Every automatic connection has its own switch in Paramètres
(the settings of Nevermind); the others only happen when you press a button.

### Automatic (on by default)

| What | When | What is sent | How to turn it off |
|---|---|---|---|
| Latest NVIDIA "Game Ready" driver version: `gfwsl.geforce.com` | When Nevermind starts, only if an NVIDIA graphics card is found | A fixed request, the same for every PC (no information about your PC) | Paramètres > Réseau et vie privée > "Vérifier le pilote NVIDIA au lancement" (when off, Nevermind only looks at the age of the installed driver) |
| Connection measurement while you play: pings to your box, to the first server of your provider, to Cloudflare (`1.1.1.1`, with a route trace) and to the game server your game is already connected to | During each game (the game server only when the game is online) | Ping packets only | Paramètres > Jeux > "Mesurer ma connexion quand je joue" |
| Your public IP address: `api.ipify.org` | When you open the Réseau page | A standard HTTPS request | Paramètres > Réseau et vie privée > "Afficher mon adresse Internet" (when off, the page says that the address is not looked up) |
| What's new after an update: `api.github.com` | Once, after Nevermind has been updated | A standard HTTPS request for the notes of the new version | Paramètres > Mises à jour > "Lire les nouveautés sur Internet" |
| Updates of a copy that is not managed by Mindset: `api.github.com` (and `github.com` to download an update you accept) | When this copy starts. A copy managed by Mindset never checks: Mindset does it (see Mindset's own policy) | A standard HTTPS request (user agent `OptiGame`) | Paramètres > Mises à jour > "Chercher une nouvelle version au lancement" (when off, only when you click "Vérifier les mises à jour"), or use Nevermind through Mindset |

### Off by default (only if you turn them on)

| What | What is sent | Setting |
|---|---|---|
| Game covers from the Internet: Steam store search (`store.steampowered.com`) and Steam images (`steamstatic.com`), then Wikipedia (`en.wikipedia.org`, `fr.wikipedia.org`); for three games, the publisher's official images (`cmsassets.rgpub.io`, `ddragon.leagueoflegends.com`) | The name of the game | Paramètres > Jeux > "Jaquettes depuis Internet" |
| Owner of unknown server addresses (Trafic and Lag en ligne pages): `rdap.org`, and a reverse DNS lookup through your usual DNS | The server's IP address | Paramètres > Réseau et vie privée > "Identifier les serveurs sans nom" |
| Country of the game server (Lag en ligne): `ipinfo.io` | The server's IP address | Paramètres > Réseau et vie privée > "Pays du serveur dans Lag en ligne" |
| Warning when a new device joins your network: scan of your local network every 10 minutes (see the network scan below) | Nothing leaves your local network, except the one time download of the manufacturer list | Paramètres > Réseau et vie privée > "Me prévenir quand un nouvel appareil se connecte" |

### Only when you ask for it (a button)

| What | What is sent |
|---|---|
| Internet speed test and "Lag en charge": download from `speed.cloudflare.com`, `proof.ovh.net`, `nbg1-speed.hetzner.com`, `fsn1-speed.hetzner.com`; upload of random data to `speed.cloudflare.com`; pings to `1.1.1.1` | Test data only |
| Ping test: your box, `1.1.1.1` (Cloudflare) and `8.8.8.8` (Google) | Ping packets only |
| Scan of your local network: only the devices of your own network (ping, ARP, mDNS, SSDP, NetBIOS, WS-Discovery, a few TCP ports, title of their web page). The first time, the list of network card manufacturers is downloaded from `standards-oui.ieee.org` | Nothing about you; a standard HTTPS request for the list |
| Network security check: your box (UPnP), your devices, the DNS answers for `one.one.one.one`, `dns.google` and a random name that does not exist (to detect a hijacked DNS), and your public IP address (`api.ipify.org`) | Standard requests, nothing about you |
| Window organizer for a game (Jeux page): the class icons of the characters, from the game's public database | The number of the class |
| Links you click (your box's page, driver pages of NVIDIA, AMD and Intel, Mindset's download page) | They open in your web browser |

Third party components: Nevermind shows its window with Microsoft Edge WebView2, a component of Windows; it only
displays its own local pages in it. The WebView2 Runtime is covered by the Microsoft privacy statement:
<https://privacy.microsoft.com/privacystatement>. PresentMon (Intel) runs on your PC only, to measure FPS.

## System changes and uninstallation

- Every Windows setting that Nevermind changes is written in its history. Most changes can be undone one by one;
  the few that are kept on purpose (because they protect the PC) are marked as such, with how to undo them by hand.
  The administrator part (`NevermindAide`) only runs the actions of a strict list that refuses
  anything that would weaken the security of the PC.
- `Désinstaller Nevermind.exe` uninstalls Nevermind and puts the Windows settings back as they were. When Nevermind
  is installed by Mindset, it can also be uninstalled from Mindset.

---

# Politique de signature du code

Signature gratuite du code fournie par [SignPath.io](https://about.signpath.io), certificat de la
[SignPath Foundation](https://signpath.org).

> **Où on en est :** la demande auprès de la SignPath Foundation est en cours. En attendant, les versions ne sont pas
> signées (Authenticode), et les notes de chaque [version publiée](../../releases) donnent l'empreinte SHA-256 de
> `OptiGame.zip`.

## Ce qui est signé

- `Nevermind.exe`, `Désinstaller Nevermind.exe`, `NevermindAide.exe` et les scripts PowerShell (`.ps1`) de
  Nevermind, tous construits depuis ce dépôt.
- Les fichiers d'autres auteurs ne sont jamais signés avec ce certificat : les bibliothèques WebView2 de Microsoft
  (`fichiers\lib\webview2`, déjà signées par Microsoft Corporation), PresentMon (`fichiers\outils-tiers`, déjà signé
  par Intel Corporation, licence MIT) et Three.js (JavaScript, licence MIT).

## Comment une version est construite et signée

1. La version reçoit son étiquette dans ce dépôt (`v<version>`).
2. Le [workflow Release](.github/workflows/release.yml) construit les trois programmes sur une machine de GitHub,
   seulement depuis le code de cette étiquette, et dépose le dossier `OptiGame` comme artefact (le dossier et le zip
   gardent l'ancien nom de l'appli, que les copies déjà installées cherchent).
3. Le workflow envoie une demande de signature à SignPath, qui vérifie que l'artefact vient bien de ce workflow et de
   ce dépôt. **Chaque demande est approuvée à la main** par un approbateur (voir plus bas).
4. Le responsable récupère le `OptiGame.zip` signé construit par le workflow, ajoute la signature RSA séparée
   `OptiGame.zip.sig` (Mindset et Nevermind s'en servent pour vérifier les mises à jour ; elle ne change aucun
   fichier du zip) et publie la version.

## Équipe et rôles

| Rôle | Membres |
|---|---|
| Auteurs et relecteurs | [Jordan Jacquot](https://github.com/JordanJacquot) |
| Approbateurs | [Jordan Jacquot](https://github.com/JordanJacquot) |

- Les auteurs peuvent modifier le code de ce dépôt. Les changements proposés par quelqu'un d'autre (pull requests)
  sont relus par un auteur avant d'être acceptés.
- Chaque demande de signature est approuvée par un approbateur.
- Tous les membres de l'équipe utilisent la double authentification sur GitHub et sur SignPath.

## Confidentialité

Nevermind n'a ni compte, ni publicité, ni télémétrie, ni statistiques. Il ne lit jamais le contenu de tes connexions
et n'envoie aucune donnée personnelle. Voici toutes les connexions que Nevermind fait vers Internet, et comment les
couper. Chaque connexion automatique a son interrupteur dans Paramètres (les réglages de Nevermind) ; les autres ne
partent que quand tu appuies sur un bouton.

### Automatiques (activées au départ)

| Quoi | Quand | Ce qui est envoyé | Comment le couper |
|---|---|---|---|
| Dernière version du pilote NVIDIA « Game Ready » : `gfwsl.geforce.com` | Au lancement de Nevermind, seulement si une carte graphique NVIDIA est trouvée | Une demande fixe, la même pour tous les PC (rien sur ton PC) | Paramètres > Réseau et vie privée > « Vérifier le pilote NVIDIA au lancement » (coupé, Nevermind regarde seulement l'âge du pilote installé) |
| Mesure de la connexion pendant que tu joues : pings vers ta box, le premier serveur de ton fournisseur, Cloudflare (`1.1.1.1`, avec un tracé du chemin) et le serveur auquel ton jeu est déjà connecté | Pendant chaque partie (le serveur du jeu seulement si le jeu est en ligne) | Seulement des paquets de ping | Paramètres > Jeux > « Mesurer ma connexion quand je joue » |
| Ton adresse Internet (IP publique) : `api.ipify.org` | Quand tu ouvres la page Réseau | Une demande HTTPS ordinaire | Paramètres > Réseau et vie privée > « Afficher mon adresse Internet » (coupé, la page dit que l'adresse n'est pas cherchée) |
| Les nouveautés après une mise à jour : `api.github.com` | Une fois, après une mise à jour de Nevermind | Une demande HTTPS ordinaire pour les notes de la nouvelle version | Paramètres > Mises à jour > « Lire les nouveautés sur Internet » |
| Mises à jour d'une copie qui n'est pas gérée par Mindset : `api.github.com` (et `github.com` pour télécharger une mise à jour que tu acceptes) | Au lancement de cette copie. Une copie gérée par Mindset ne cherche jamais : c'est Mindset qui le fait (voir sa propre politique) | Une demande HTTPS ordinaire (nom du programme `OptiGame`) | Paramètres > Mises à jour > « Chercher une nouvelle version au lancement » (coupé, seulement quand tu cliques sur « Vérifier les mises à jour »), ou utiliser Nevermind avec Mindset |

### Coupées au départ (seulement si tu les actives)

| Quoi | Ce qui est envoyé | Réglage |
|---|---|---|
| Jaquettes depuis Internet : recherche dans la boutique Steam (`store.steampowered.com`) et images de Steam (`steamstatic.com`), puis Wikipédia (`en.wikipedia.org`, `fr.wikipedia.org`) ; pour trois jeux, les images officielles de l'éditeur (`cmsassets.rgpub.io`, `ddragon.leagueoflegends.com`) | Le nom du jeu | Paramètres > Jeux > « Jaquettes depuis Internet » |
| Propriétaire des adresses de serveurs inconnues (pages Trafic et Lag en ligne) : `rdap.org`, et une recherche du nom de l'adresse par ton DNS habituel | L'adresse IP du serveur | Paramètres > Réseau et vie privée > « Identifier les serveurs sans nom » |
| Pays du serveur du jeu (Lag en ligne) : `ipinfo.io` | L'adresse IP du serveur | Paramètres > Réseau et vie privée > « Pays du serveur dans Lag en ligne » |
| Prévenir quand un nouvel appareil se connecte : recherche des appareils de ton réseau toutes les 10 minutes (voir plus bas) | Rien ne sort de ton réseau, sauf le téléchargement unique de la liste des fabricants | Paramètres > Réseau et vie privée > « Me prévenir quand un nouvel appareil se connecte » |

### Seulement quand tu le demandes (un bouton)

| Quoi | Ce qui est envoyé |
|---|---|
| Test de débit et « Lag en charge » : téléchargement depuis `speed.cloudflare.com`, `proof.ovh.net`, `nbg1-speed.hetzner.com`, `fsn1-speed.hetzner.com` ; envoi de données sans contenu vers `speed.cloudflare.com` ; pings vers `1.1.1.1` | Seulement des données de test |
| Test de ping : ta box, `1.1.1.1` (Cloudflare) et `8.8.8.8` (Google) | Seulement des paquets de ping |
| Recherche des appareils de ton réseau : seulement les appareils de ton propre réseau (ping, ARP, mDNS, SSDP, NetBIOS, WS-Discovery, quelques ports TCP, titre de leur page web). La première fois, la liste des fabricants de cartes réseau est téléchargée depuis `standards-oui.ieee.org` | Rien sur toi ; une demande HTTPS ordinaire pour la liste |
| Vérification de la sécurité du réseau : ta box (UPnP), tes appareils, les réponses du DNS pour `one.one.one.one`, `dns.google` et un nom au hasard qui n'existe pas (pour repérer un DNS détourné), et ton adresse Internet (`api.ipify.org`) | Des demandes ordinaires, rien sur toi |
| Organiseur de fenêtres d'un jeu (page Jeux) : les icônes des classes de personnages, depuis l'annuaire public du jeu | Le numéro de la classe |
| Les liens sur lesquels tu cliques (page de ta box, pages des pilotes NVIDIA, AMD et Intel, page de téléchargement de Mindset) | Ils s'ouvrent dans ton navigateur |

Composants d'autres auteurs : Nevermind affiche sa fenêtre avec Microsoft Edge WebView2, un composant de Windows ; il
n'y montre que ses propres pages, enregistrées sur le PC. WebView2 relève de la déclaration de confidentialité de
Microsoft : <https://privacy.microsoft.com/privacystatement>. PresentMon (Intel) tourne seulement sur ton PC, pour
mesurer les FPS.

## Ce qui change sur le PC, et la désinstallation

- Chaque réglage de Windows que Nevermind modifie est noté dans son historique. La plupart s'annulent un par un ;
  les quelques changements gardés exprès (parce qu'ils protègent le PC) sont signalés, avec la façon de revenir en
  arrière à la main. La partie administrateur (`NevermindAide`) ne fait que les actions d'une liste stricte, qui
  refuse tout ce qui affaiblirait la sécurité du PC.
- `Désinstaller Nevermind.exe` désinstalle Nevermind et remet les réglages de Windows comme avant. Quand Nevermind
  est installé par Mindset, il se désinstalle aussi depuis Mindset.
