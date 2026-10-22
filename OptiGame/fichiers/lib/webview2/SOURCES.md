# WebView2 : d'où viennent ces fichiers

| Fichier | Version | SHA-256 |
|---|---|---|
| `Microsoft.Web.WebView2.Core.dll` | 1.0.4258.31 | `D60E6E94245078CB56E44EBD5F360DC04BF18E460136BE93BFC4CFCA43B98505` |
| `Microsoft.Web.WebView2.Wpf.dll` | 1.0.4258.31 | `F05A06BBC03AFCB83DC98168B8D343D9015B2A533EDBFAC90C52D88894B653E4` |
| `WebView2Loader.dll` | 1.0.4258.31 | `3426DCC55FDFB8B5E7AC623CF33B4CCF283FBF68A0C5A65BDDFDB4D749A8ABEE` |

- Origine : le paquet NuGet officiel de Microsoft, `Microsoft.Web.WebView2` version **1.0.4258.31**
  (https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.4258.31) : `lib\net462\` pour les deux DLL .NET,
  `runtimes\win-x64\native\` pour `WebView2Loader.dll`.
- Les trois fichiers sont signés par **Microsoft Corporation** (signature Authenticode valide).
- Licence : `LICENCE-Microsoft-WebView2.txt`.
- `outils\construire.ps1` vérifie l'empreinte et la signature de chaque fichier avant de construire : si l'un ne
  correspond plus, la construction s'arrête. Pour changer de version : remplacer les fichiers depuis le paquet NuGet,
  puis mettre à jour ce tableau et les empreintes dans `outils\construire.ps1`.
