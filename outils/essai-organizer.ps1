# Essai de la vraie barre flottante, de la vue et des raccourcis de l'Organizer de Nevermind en Rust, sur un BUREAU
# WINDOWS À PART (jamais affiché) : rien ne s'ouvre sur l'écran, aucun raccourci n'est pris sur le bureau affiché.
# Les fenêtres « du jeu » sont celles du programme d'essai (titres semblables à ceux du jeu), jamais le vrai jeu.
#
#   & 'outils\essai-organizer.ps1'            (depuis l'outil PowerShell ; construire avant, depuis Bash :
#                                              cargo build --offline -p nevermind-jeux --example organizer_essai)
#
# Écrit dans le dossier -Sortie : resultats.txt, et les images de la barre et de la vue (PNG) à chaque étape.
param(
    [string]$Exe = 'D:\Outils\Rust\target\nevermind-organizer\debug\examples\organizer_essai.exe',
    [string]$Sortie = (Join-Path $env:TEMP 'essai-organizer'),
    [switch]$Dedans
)
$ErrorActionPreference = 'Stop'

if ($Dedans) {
    # (sur le bureau à part : le programme d'essai, sans console, attendu jusqu'au bout)
    $p = Start-Process -FilePath $Exe -ArgumentList "`"$Sortie`"" -WindowStyle Hidden -Wait -PassThru
    "code de sortie : $($p.ExitCode)"
    exit $p.ExitCode
}

if (-not (Test-Path -LiteralPath $Exe)) { throw "Programme d'essai absent : $Exe" }
if (Test-Path -LiteralPath $Sortie) { Remove-Item -LiteralPath $Sortie -Recurse -Force }
New-Item -ItemType Directory -Force -Path $Sortie | Out-Null
$surBureau = Join-Path (Split-Path $PSScriptRoot -Parent) 'rust\nevermind-appli\outils\sur-bureau.ps1'
& $surBureau -Script $PSCommandPath -Arguments "-Dedans -Exe '$Exe' -Sortie '$Sortie'" -Secondes 180 -Sortie (Join-Path $Sortie 'sur-bureau.txt')

# Les fenêtres d'essai restées ouvertes (essai interrompu) : arrêtées par leur numéro, seulement les nôtres
$copie = Join-Path $Sortie 'organizer_essai_fenetre.exe'
foreach ($x in @(Get-CimInstance Win32_Process -Filter "Name = 'organizer_essai_fenetre.exe'")) {
    if ($x.ExecutablePath -and ([IO.Path]::GetFullPath($x.ExecutablePath) -eq [IO.Path]::GetFullPath($copie))) {
        try { Stop-Process -Id $x.ProcessId -Force -ErrorAction Stop; "Fenêtre d'essai arrêtée : $($x.ProcessId)" } catch {}
    }
}

# Les images en PNG (pour les regarder)
Add-Type -AssemblyName System.Drawing
foreach ($f in Get-ChildItem -LiteralPath $Sortie -Recurse -Filter *.bmp) {
    $img = [System.Drawing.Image]::FromFile($f.FullName)
    $img.Save([IO.Path]::ChangeExtension($f.FullName, '.png'), [System.Drawing.Imaging.ImageFormat]::Png)
    $img.Dispose()
}
$r = Join-Path $Sortie 'resultats.txt'
if (Test-Path -LiteralPath $r) { Get-Content -LiteralPath $r -Encoding UTF8 } else { 'Pas de résultats.' }
