# Nevermind en Rust : ce que contient son zip et d'où vient sa version, écrit à UN seul endroit pour
# outils\construire-rust.ps1, outils\verifier-zip-rust.ps1, outils\version-rust.ps1 et outils\publier.ps1 -Rust
# (chargé par « . (Join-Path $PSScriptRoot 'contenu-rust.ps1') »).
#
#   Nevermind\                                  (le dossier « dossier_zip » de l'entrée apps.json)
#     Mindset.exe                               <- le programme (rust\nevermind-appli, programme « Mindset ») : la fenêtre
#                                                  Tauri de Nevermind dans Mindset. Description « Mindset » : une seule
#                                                  ligne « Mindset » dans le Gestionnaire des tâches (contrat-appli, partie 4)
#     Nevermind.exe                             <- le relais (programme cargo « Nevermind ») : lance Mindset.exe de son
#                                                  dossier avec les mêmes arguments (anciens Mindset, raccourcis, tâches)
#     NevermindAide.exe                         <- la partie administrateur (rust\nevermind-aide, programme « NevermindAide »),
#                                                  lancée par la tâche « \Mindset\Nevermind (aide) » que crée Mindset
#     Désinstaller Nevermind.exe                <- le désinstalleur (programme cargo « NevermindDesinstaller », renommé ici :
#                                                  un nom de programme cargo ne peut avoir ni espace ni accent) ; Mindset le
#                                                  lance (champ « desinstalleur » d'apps.json, contrat-appli, partie 9)
#     version.json                              <- { "appli": "nevermind", "version": <Cargo.toml de nevermind-appli>,
#                                                    "mindset_min": "2.18.0" } (contrat-appli, partie 10)
#     LISEZMOI.txt                              <- outils\LISEZMOI-rust.txt (le LISEZMOI de Nevermind, sans ce qui ne parle
#                                                  que de la version PowerShell : lanceurs de secours, dossier modules)
#     fichiers\outils-tiers\PresentMon.exe      <- la mesure des FPS (Intel), lue par NevermindAide.exe à ce chemin
#     fichiers\outils-tiers\PresentMon-LICENCE.txt, SOURCES.md
#
# Plus aucun script PowerShell, ni NevermindPont.dll, ni bibliothèque WebView2 de WPF : les pages (Nevermind\fichiers\ui)
# sont rangées dans Mindset.exe par Tauri (docs\regles-tauri.md du dépôt Mindset : « tout est rangé dans l'exe »).
#
# Une seule version pour tout : la ligne « version = » de [package] de rust\nevermind-appli\Cargo.toml. Mindset.exe la
# prend là (tauri-build), le relais et le désinstalleur aussi (leur build.rs doit la lire là, comme mindcare-relais) ;
# NevermindAide.exe la prend dans $AppVersion de Nevermind\fichiers\Nevermind.ps1 (rust\nevermind-aide\build.rs) :
# outils\version-rust.ps1 écrit donc les deux.

$NevermindRust = @{
    # La première version de Mindset qui a le canal d'essai par appli et efface le dossier WebView2-Tauri\Nevermind d'une
    # appli désinstallée sans ses données (Mindset 2.18.0, commit 3617ffe)
    MindsetMin = '2.18.0'
    # Les programmes : nom dans le zip -> programme cargo, et ce que disent leurs informations de version
    Programmes = [ordered]@{
        'Mindset.exe'                = @{ Bin = 'Mindset'; Paquet = 'nevermind-appli'; Description = 'Mindset'; Produit = 'Mindset' }
        'Nevermind.exe'              = @{ Bin = 'Nevermind'; Paquet = ''; Description = 'Nevermind'; Produit = 'Nevermind' }
        'NevermindAide.exe'          = @{ Bin = 'NevermindAide'; Paquet = 'nevermind-aide'; Description = 'Nevermind'; Produit = 'Nevermind' }
        "D$([char]0xE9)sinstaller Nevermind.exe" = @{ Bin = 'NevermindDesinstaller'; Paquet = ''; Description = "D$([char]0xE9)sinstaller Nevermind"; Produit = "D$([char]0xE9)sinstaller Nevermind" }
    }
    # Les fichiers copiés tels quels : chemin dans le dossier Nevermind -> chemin dans le dépôt
    Copies = [ordered]@{
        'LISEZMOI.txt'                                = 'outils\LISEZMOI-rust.txt'
        'fichiers\outils-tiers\PresentMon.exe'        = 'Nevermind\fichiers\outils-tiers\PresentMon.exe'
        'fichiers\outils-tiers\PresentMon-LICENCE.txt' = 'Nevermind\fichiers\outils-tiers\PresentMon-LICENCE.txt'
        'fichiers\outils-tiers\SOURCES.md'            = 'Nevermind\fichiers\outils-tiers\SOURCES.md'
    }
    # PresentMon d'Intel : son empreinte (outils-tiers\SOURCES.md, la même que outils\construire.ps1) et son signataire
    PresentMonSha256 = 'B2A706BC6AD475749E3B7E3409263AA1E6906D45BDCF993F6DBC0F660188F1AF'
    PresentMonSignataire = 'Intel Corporation'
}

# La version de Nevermind en Rust : la ligne « version = "x.y.z" » de la section [package] de rust\nevermind-appli\Cargo.toml
function Get-VersionNevermindRust([string]$Racine) {
    $cargo = Join-Path $Racine 'rust\nevermind-appli\Cargo.toml'
    if (-not (Test-Path -LiteralPath $cargo -PathType Leaf)) { return $null }
    $section = ''
    foreach ($ligne in [IO.File]::ReadAllLines($cargo, [Text.Encoding]::UTF8)) {
        $l = $ligne.Trim()
        if ($l.StartsWith('[')) { $section = $l; continue }
        if ($section -eq '[package]' -and $l -match '^version\s*=\s*"([^"]*)"') { return $Matches[1] }
    }
    $null
}

# La version de la version PowerShell ($AppVersion de Nevermind.ps1), celle que prend aussi NevermindAide.exe
function Get-VersionNevermindPs([string]$Racine) {
    $ps1 = Join-Path $Racine 'Nevermind\fichiers\Nevermind.ps1'
    if ([IO.File]::ReadAllText($ps1, [Text.Encoding]::UTF8) -match "(?m)^\`$AppVersion = '([\d\.]+)'") { return $Matches[1] }
    $null
}

# Une version sur quatre nombres (« 2.0.0 » -> « 2.0.0.0 »), comme les informations de version des exe
function ConvertTo-Version4([string]$Version) {
    $p = @(([string]$Version).Split('.') | Where-Object { $_ -ne '' }); while ($p.Count -lt 4) { $p += '0' }
    $p -join '.'
}
