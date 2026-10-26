# Construit NevermindPont.dll (rust\nevermind-pont : les règles de l'Aide en Rust, appelées par Nevermind) et la pose
# à côté des programmes de Nevermind : Nevermind\NevermindPont.dll. C'est là que Nevermind la charge (pont.ps1, par son
# chemin complet, dans le dossier protégé de l'appli), là que les tests la trouvent, et là que zips.ps1 la prend.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\construire-pont.ps1
#   -Essai : la DLL d'essai à la place (avec la demande « essai.panique », pour tester-pont.ps1), posée dans
#            _test\pont-essai\NevermindPont.dll. Jamais livrée.
#
# Le runtime C est mis DANS la DLL (crt-static) : sans cela, elle aurait besoin de VCRUNTIME140.dll, absent de beaucoup
# de PC (Nevermind ne proposerait alors plus « Fermer » et ne trouverait plus son Aide).
# Sur le PC de Jordan : Rust de D:\Outils\Rust, constructions dans D:\Outils\Rust\target\nevermind-pont, jamais pendant
# un jeu. Ailleurs (GitHub) : le cargo du PATH.
# Rend le chemin de la DLL posée.
param([switch]$Essai)
$ErrorActionPreference = 'Stop'
$racine = Split-Path $PSScriptRoot -Parent
$rust = Join-Path $racine 'rust'

# Jamais pendant un jeu (règle de Jordan) : on attend qu'il ait fini
$jeux = { @(Get-Process -Name 'bf6', 'dofus*', 'RocketLeague', 'FC*', 'FIFA*' -ErrorAction SilentlyContinue) }
$annonce = $false
while ((& $jeux).Count) {
    if (-not $annonce) { Write-Host "Un jeu est ouvert ($((& $jeux)[0].Name)) : la construction attend qu'il soit fermé." -ForegroundColor Yellow; $annonce = $true }
    Start-Sleep -Seconds 30
}

$outils = 'D:\Outils\Rust'
if (Test-Path -LiteralPath (Join-Path $outils 'cargo\bin\cargo.exe')) {
    if (-not $env:RUSTUP_HOME) { $env:RUSTUP_HOME = Join-Path $outils 'rustup' }
    if (-not $env:CARGO_HOME) { $env:CARGO_HOME = Join-Path $outils 'cargo' }
    $env:Path = (Join-Path $outils 'cargo\bin') + ';' + $env:Path
    if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path $outils 'target\nevermind-pont' }
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'cargo introuvable : Rust n''est pas installé (voir le plan Rust, annexe B)' }
$cible = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $rust 'target' }

$avant = $env:RUSTFLAGS
$env:RUSTFLAGS = '-C target-feature=+crt-static'
try {
    Push-Location $rust
    try {
        # (la DLL d'essai : profil « dev », dossier debug, pour ne jamais écraser la DLL livrée du dossier release)
        if ($Essai) { & cargo build --locked -p nevermind-pont --features essai-panique }
        else { & cargo build --locked --release -p nevermind-pont }
        if ($LASTEXITCODE) { throw "cargo build a échoué (code $LASTEXITCODE)" }
    } finally { Pop-Location }
} finally { $env:RUSTFLAGS = $avant }

$construite = Join-Path $cible $(if ($Essai) { 'debug\nevermind_pont.dll' } else { 'release\nevermind_pont.dll' })
if (-not (Test-Path -LiteralPath $construite -PathType Leaf)) { throw "DLL construite introuvable : $construite" }
# (contrôle du runtime C : le nom VCRUNTIME ne doit pas figurer dans la liste des DLL dont elle a besoin)
$texte = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($construite))
if ($texte -match '(?i)vcruntime\d+\.dll') { throw "La DLL a besoin de $($Matches[0]) : construite sans crt-static" }

$dest = if ($Essai) { Join-Path $racine '_test\pont-essai\NevermindPont.dll' } else { Join-Path $racine 'Nevermind\NevermindPont.dll' }
[void][IO.Directory]::CreateDirectory((Split-Path $dest -Parent))
Copy-Item -LiteralPath $construite -Destination $dest -Force
$dest
