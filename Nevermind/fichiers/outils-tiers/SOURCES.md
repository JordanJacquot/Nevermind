# Outils d'autres auteurs : d'où ils viennent

| Fichier | Version | SHA-256 |
|---|---|---|
| `PresentMon.exe` | 2.6.0 (x64) | `B2A706BC6AD475749E3B7E3409263AA1E6906D45BDCF993F6DBC0F660188F1AF` |

- Origine : la version officielle d'Intel, https://github.com/GameTechDev/PresentMon/releases/tag/v2.6.0
  (fichier `PresentMon-2.6.0-x64.exe`, même empreinte que celle publiée par GitHub pour cette version).
- Signé par **Intel Corporation** (signature Authenticode valide).
- Licence : MIT, `PresentMon-LICENCE.txt`.
- Lancé par Nevermind Aide (administrateur), ou par Nevermind lui-même quand l'Aide ne répond pas
  (`modules\aide-client.ps1`). Les deux vérifient avant chaque lancement que la signature est valide et vient d'Intel ;
  Nevermind garde aussi le fichier ouvert en lecture seule pendant ce contrôle et toute la mesure, pour que personne ne
  puisse le remplacer entre-temps. `outils\construire.ps1` vérifie aussi l'empreinte avant de construire.
- Pour changer de version : prendre le fichier `-x64.exe` de la version voulue sur la page des versions d'Intel,
  vérifier son empreinte avec celle affichée par GitHub, puis mettre à jour ce tableau et `outils\construire.ps1`.
