# Nevermind en Rust : construit ses programmes en Rust et assemble le dossier de Nevermind prêt à zipper (le contenu
# est décrit dans outils\contenu-rust.ps1). Lancé par la CI de GitHub (.github\workflows\tests.yml et release.yml) ; sur
# le PC, seulement pour un essai : le zip publié vient toujours de GitHub (outils\zip-rust-github.ps1).
#
#   & '.\outils\construire-rust.ps1' -Sortie <dossier> [-HorsLigne] [-SansConstruire]
#
#   <dossier>\Nevermind\   Mindset.exe, Nevermind.exe, NevermindAide.exe, Désinstaller Nevermind.exe, version.json,
#                          LISEZMOI.txt, fichiers\outils-tiers\ (PresentMon)
#
# Les programmes sont trouvés par leur nom de programme cargo (cargo metadata), quelle que soit la crate qui les
# construit : « Mindset » (rust\nevermind-appli), « Nevermind » (le relais), « NevermindAide » (rust\nevermind-aide),
# « NevermindDesinstaller » (le désinstalleur). Un programme qui manque : erreur claire, rien n'est assemblé.
# Construction de publication seulement (cargo build --release --locked, sans aucune option : jamais l'option « essai »,
# règle 5 de docs\regles-tauri.md du dépôt Mindset). Le dossier <dossier>\Nevermind est vidé d'abord.
# -HorsLigne : cargo sans réseau (--offline), pour un essai sur le PC. -SansConstruire : seulement l'assemblage, avec
# les exe déjà construits.
param(
    [Parameter(Mandatory = $true)][string]$Sortie,
    [switch]$HorsLigne,
    [switch]$SansConstruire
)

$ErrorActionPreference = 'Stop'
$racine = Split-Path $PSScriptRoot -Parent
$rust = Join-Path $racine 'rust'
. (Join-Path $PSScriptRoot 'contenu-rust.ps1')

$version = Get-VersionNevermindRust $racine
if ($version -notmatch '^\d+(\.\d+){1,3}$') { throw "Version de nevermind-appli introuvable ou mal écrite : « $version » (rust\nevermind-appli\Cargo.toml)." }
$versionPs = Get-VersionNevermindPs $racine
if ($versionPs -ne $version) {
    # (normal entre deux publications : outils\publier.ps1 -Rust écrit la même version aux deux endroits ; la
    # vérification du zip à publier, elle, la refuse)
    Write-Host "Attention : NevermindAide.exe aura la version de Nevermind.ps1 ($versionPs), pas celle de nevermind-appli ($version). outils\version-rust.ps1 les aligne." -ForegroundColor Yellow
}

# cargo écrit ses messages normaux sur la sortie d'erreur : seul son code de sortie compte
function Invoke-Cargo([string[]]$Arguments, [switch]$Capturer) {
    $avant = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    Push-Location $rust
    try {
        if ($HorsLigne) { $Arguments += '--offline' }
        if ($Capturer) { $s = @(& cargo @Arguments 2>$null) } else { & cargo @Arguments 2>&1 | ForEach-Object { "$_" } | Out-Host }
        $code = $LASTEXITCODE
    } finally {
        Pop-Location
        $ErrorActionPreference = $avant
    }
    if ($code) { throw "cargo $($Arguments -join ' ') a échoué (code $code)." }
    if ($Capturer) { $s -join "`n" }
}

# Quelle crate construit chaque programme, et le dossier où cargo range ce qu'il construit (CARGO_TARGET_DIR compris)
$meta = Invoke-Cargo @('metadata', '--no-deps', '--format-version', '1', '--locked') -Capturer | ConvertFrom-Json
$paquets = @{}
$manque = @()
foreach ($nom in $NevermindRust.Programmes.Keys) {
    $p = $NevermindRust.Programmes[$nom]
    $trouves = @($meta.packages | Where-Object { @($_.targets | Where-Object { $_.kind -contains 'bin' -and $_.name -ceq $p.Bin }).Count })
    if ($trouves.Count -ne 1) { $manque += "$nom (programme cargo « $($p.Bin) »$(if ($p.Paquet) { " de $($p.Paquet)" }) : $(if ($trouves.Count) { "dans $($trouves.Count) crates" } else { 'aucune crate ne le construit' }))"; continue }
    if ($p.Paquet -and $trouves[0].name -cne $p.Paquet) { $manque += "$nom (programme « $($p.Bin) » construit par $($trouves[0].name) au lieu de $($p.Paquet))"; continue }
    # Jamais la construction d'essai : l'option « essai » ne doit pas être dans les options par défaut
    $defaut = @($trouves[0].features.default)
    if ($defaut -contains 'essai') { $manque += "$nom : l'option « essai » de $($trouves[0].name) est dans ses options par défaut" ; continue }
    $paquets[$nom] = $trouves[0].name
}
if ($manque.Count) { throw "Nevermind en Rust incomplet :`n  $($manque -join "`n  ")" }
$construits = Join-Path $meta.target_directory 'release'

if (-not $SansConstruire) {
    $construction = @('build', '--release', '--locked')
    foreach ($n in @($paquets.Values | Sort-Object -Unique)) { $construction += @('-p', $n) }
    Invoke-Cargo $construction
}

$dossier = Join-Path ([IO.Path]::GetFullPath($Sortie)) 'Nevermind'
if (Test-Path -LiteralPath $dossier) { Remove-Item -LiteralPath $dossier -Recurse -Force }
New-Item -ItemType Directory -Force -Path $dossier | Out-Null
foreach ($nom in $NevermindRust.Programmes.Keys) {
    $chemin = Join-Path $construits "$($NevermindRust.Programmes[$nom].Bin).exe"
    if (-not (Test-Path -LiteralPath $chemin -PathType Leaf)) { throw "$($NevermindRust.Programmes[$nom].Bin).exe introuvable dans $construits : la construction ne l'a pas fait." }
    Copy-Item -LiteralPath $chemin -Destination (Join-Path $dossier $nom)
}

# version.json en UTF-8 sans marque, comme le lit Mindset (Read-VersionJson) et suite-commun
$json = [ordered]@{ appli = 'nevermind'; version = $version; mindset_min = $NevermindRust.MindsetMin } | ConvertTo-Json -Compress
[IO.File]::WriteAllText((Join-Path $dossier 'version.json'), $json, (New-Object Text.UTF8Encoding($false)))

foreach ($rel in $NevermindRust.Copies.Keys) {
    $source = Join-Path $racine $NevermindRust.Copies[$rel]
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Fichier à livrer absent : $($NevermindRust.Copies[$rel])" }
    $dest = Join-Path $dossier $rel
    New-Item -ItemType Directory -Force -Path (Split-Path $dest -Parent) | Out-Null
    Copy-Item -LiteralPath $source -Destination $dest
}
# PresentMon (Intel) : l'empreinte notée dans SOURCES.md et la signature d'Intel, sinon rien n'est livré
$pm = Join-Path $dossier 'fichiers\outils-tiers\PresentMon.exe'
$h = (Get-FileHash -LiteralPath $pm -Algorithm SHA256).Hash
if ($h -ne $NevermindRust.PresentMonSha256) { throw "PresentMon.exe modifié (empreinte $h, attendue $($NevermindRust.PresentMonSha256) : voir son SOURCES.md)" }
$s = Get-AuthenticodeSignature -LiteralPath $pm
if ($s.Status -ne 'Valid' -or $s.SignerCertificate.Subject -notmatch "CN=$([regex]::Escape($NevermindRust.PresentMonSignataire))(,|$)") { throw "PresentMon.exe mal signé ($($s.Status), $($s.SignerCertificate.Subject))" }

"OK : $dossier (Nevermind $version en Rust)"
Get-ChildItem -LiteralPath $dossier -Recurse -File | ForEach-Object { "  $($_.FullName.Substring($dossier.Length + 1)) ($($_.Length) octets)" }
