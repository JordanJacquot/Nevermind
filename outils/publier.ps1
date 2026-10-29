# Publie une nouvelle version de Nevermind sur GitHub.
# Mindset la propose ensuite et l'installe (Nevermind.zip et sa signature Nevermind.zip.sig, vérifiée avant installation).
# Le zip de compatibilité (OptiGame.zip et sa signature), publié de 1.12.0 à 1.15.4 pendant le changement de nom, ne
# l'est plus (voir outils\zips.ps1).
#
#   .\outils\publier.ps1 -Version 1.1 -Notes "Ce qui change dans cette version"
#   .\outils\publier.ps1 -Version 1.2 -Notes "..." -Beta    version bêta (marquée « préversion » sur GitHub)
#   .\outils\publier.ps1 -Essai                            essai : vérifications, construction et le zip, signé
#                                                          avec une clé jetable ; rien n'est modifié ni publié
#
# Avant de publier, le script vérifie la syntaxe de tous les fichiers, l'absence de tirets
# cadratins et lance le test automatique (outils\tester.ps1). -SansTest saute le test (urgence).

param(
    [string]$Version = '',
    [string]$Notes = '',
    [switch]$Beta,
    [switch]$SansTest,
    [switch]$Essai
)
$ErrorActionPreference = 'Stop'
$racine = Split-Path $PSScriptRoot -Parent
$app    = Join-Path $racine 'Nevermind'
$ps1    = Join-Path $app 'fichiers\Nevermind.ps1'
$lisez  = Join-Path $app 'LISEZMOI.txt'
$utf8   = New-Object Text.UTF8Encoding($true)

if (-not $Essai -and $Version -notmatch '^\d+\.\d+(\.\d+)?$') { throw "Numéro de version invalide : $Version (exemple : 1.1)" }

# Un numéro déjà publié ne doit jamais être réutilisé : GitHub garde l'ancien zip en cache.
if (-not $Essai) {
    Push-Location $racine
    try {
        $ErrorActionPreference = 'Continue'   # « introuvable » est la réponse attendue ici, pas une erreur
        git rev-parse -q --verify "refs/tags/v$Version" 2>$null | Out-Null
        $tagExiste = ($LASTEXITCODE -eq 0)
        gh release view "v$Version" 2>$null | Out-Null
        $releaseExiste = ($LASTEXITCODE -eq 0)
    } finally { Pop-Location; $ErrorActionPreference = 'Stop' }
    if ($tagExiste -or $releaseExiste) { throw "La version $Version existe déjà. Choisis un numéro plus grand (exemple : 1.0.2 ou 1.1)." }
}

# Vérifications : syntaxe, fenêtre, tirets cadratins, puis test automatique de l'app
$problemes = @()
foreach ($f in Get-ChildItem -LiteralPath (Join-Path $app 'fichiers') -Recurse -Filter '*.ps1') {
    $err = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile($f.FullName, [ref]$null, [ref]$err)
    foreach ($e in $err) { $problemes += "Syntaxe : $($f.Name) ligne $($e.Extent.StartLineNumber) : $($e.Message)" }
}
try { [void][xml][IO.File]::ReadAllText((Join-Path $app 'fichiers\modules\interface.xaml'), [Text.Encoding]::UTF8) } catch { $problemes += "interface.xaml : $($_.Exception.Message)" }
$tirets = Get-ChildItem -LiteralPath $racine -Recurse -File -Include '*.ps1', '*.cs', '*.xaml', '*.txt', '*.md', '*.bat' |
    Where-Object { $_.FullName -notmatch '\\(\.git|_test)\\' } | Select-String -Pattern '[\u2013\u2014]'
foreach ($t in $tirets) { $problemes += "Tiret cadratin : $($t.Path) ligne $($t.LineNumber)" }
# Clés publiques livrées avec Nevermind : exactement la liste de Mindset (mêmes fichiers, même contenu), sinon les copies
# autonomes refuseraient une version signée avec une nouvelle clé, ou accepteraient encore une clé retirée
$cles = Join-Path $app 'fichiers\cles-publiques'
$clesMindset = Join-Path (Split-Path $racine -Parent) 'Mindset\Mindset\fichiers\cles-publiques'
function Get-EmpreinteCles([string]$Dossier) { @(Get-ChildItem -LiteralPath $Dossier -File -Force -ErrorAction SilentlyContinue | Sort-Object Name | ForEach-Object { "$($_.Name)=$((Get-FileHash -LiteralPath $_.FullName).Hash)" }) -join ';' }
$empreinte = Get-EmpreinteCles $cles
if (-not $empreinte -or $empreinte -ne (Get-EmpreinteCles $clesMindset)) {
    $problemes += "Clés publiques : Nevermind\fichiers\cles-publiques absentes ou différentes de celles de Mindset ($clesMindset)"
}
if ($problemes) {
    $problemes | ForEach-Object { Write-Host $_ -ForegroundColor Red }
    throw 'Publication annulée : corrige les points ci dessus.'
}
if (-not $SansTest -and -not $Essai) {
    & (Join-Path $PSScriptRoot 'tester.ps1')
    if ($LASTEXITCODE) { throw 'Publication annulée : le test automatique a échoué.' }
}

# Un programme (git, gh) : seul son code de sortie compte. Ses messages normaux (« To https://github.com/... ») passent
# par la sortie d'erreur : sans « Continue », PowerShell les prenait pour des erreurs et arrêtait la publication.
function Invoke-Native([string]$Exe, [string[]]$Arguments) {
    $ErrorActionPreference = 'Continue'
    & $Exe @Arguments 2>&1 | ForEach-Object { "$_" }
    if ($LASTEXITCODE) { throw "Échec : $Exe $($Arguments -join ' ')" }
}

# Les fichiers de Nevermind suivis par git (« Nevermind/... ») : ce sont eux, et seulement eux, qui partent dans le zip
# (outils\zips.ps1). Tout est donc commité avant : sinon un changement pas commité partirait dans la version publiée
# sans être dans git (et l'ancien « git add -A » commitait aussi un fichier oublié).
function Get-FichiersGitNevermind {
    $avant = [Console]::OutputEncoding
    try {
        [Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
        $s = & git -C $racine -c core.quotepath=false ls-files -z -- Nevermind 2>$null
        if ($LASTEXITCODE) { throw 'Publication annulée : git ne donne pas la liste des fichiers.' }
    } finally { [Console]::OutputEncoding = $avant }
    @(($s -join '') -split "`0" | Where-Object { $_ -like 'Nevermind/*' })
}
if (-not $Essai) {
    $avant = [Console]::OutputEncoding
    try { [Console]::OutputEncoding = New-Object Text.UTF8Encoding($false); $sale = @(& git -C $racine -c core.quotepath=false status --porcelain 2>$null | Where-Object { $_ }) }
    finally { [Console]::OutputEncoding = $avant }
    # (une publication arrêtée en route, par exemple un mot de passe mal tapé, laisse seulement le numéro de version écrit
    # dans ces deux fichiers : on les remet comme dans git, ils sont réécrits juste après)
    $siens = @('Nevermind/fichiers/Nevermind.ps1', 'Nevermind/LISEZMOI.txt')
    if ($sale.Count -and -not @($sale | Where-Object { $_ -notmatch '^(M |MM| M) ' -or $siens -notcontains $_.Substring(3).Trim('"') }).Count) {
        Invoke-Native 'git' @('-C', $racine, 'reset', '-q', '--', $siens[0], $siens[1]) | Out-Null   # (préparés pour le commit par une tentative arrêtée)
        Invoke-Native 'git' @('-C', $racine, 'checkout', '--', $siens[0], $siens[1]) | Out-Null
        Write-Host 'Le numéro de version laissé par une publication arrêtée en route est remis comme dans git.'
        $sale = @()
    }
    if ($sale.Count) { throw "Publication annulée : des fichiers ne sont pas commités ($(($sale | Select-Object -First 5) -join ' ; ')). Commite-les d'abord : la version publiée est celle de git." }
}

# 1. Numéro de version dans l'app et le LISEZMOI (pas en essai : rien n'est modifié)
if (-not $Essai) {
    $c = [IO.File]::ReadAllText($ps1, [Text.Encoding]::UTF8)
    $c = [regex]::Replace($c, "(?m)^\`$AppVersion = '[^']*'", "`$AppVersion = '$Version'")
    $c = [regex]::Replace($c, '(?m)^    Nevermind [\d\.]+\r?$', "    Nevermind $Version`r")
    [IO.File]::WriteAllText($ps1, $c, $utf8)
    $l = [IO.File]::ReadAllText($lisez, [Text.Encoding]::UTF8)
    $l = [regex]::Replace($l, '^Nevermind [\d\.]+', "Nevermind $Version")
    [IO.File]::WriteAllText($lisez, $l, $utf8)
    # Le numéro est commité tout de suite : les programmes en Rust sont construits par GitHub à partir de ce commit, et
    # leur fiche de version (Propriétés > Détails) le prend dans Nevermind.ps1. Une tentative arrêtée plus loin laisse ce
    # commit ; la suivante avec le même numéro n'a rien de nouveau à commiter et reprend la même construction.
    & git -C $racine diff --quiet -- $ps1 $lisez
    if ($LASTEXITCODE) {
        Invoke-Native git @('-C', $racine, 'add', '--', $ps1, $lisez)
        Invoke-Native git @('-C', $racine, 'commit', '-m', "Nevermind $Version", '-m', 'Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>')
    }
}

# 2. Construction des .exe et du zip (Nevermind.zip), puis sa signature (Mindset et Nevermind refusent un zip sans
# signature valide). signer.ps1 s'arrête sur une erreur s'il ne peut pas signer : la publication est alors annulée
# avant tout envoi. En essai, une clé jetable signe à sa place.
& (Join-Path $PSScriptRoot 'construire.ps1')
# Les programmes en Rust (passage en Rust, étape 1a) : NevermindAide.exe quand rust\nevermind-aide existe,
# NevermindPont.dll quand rust\nevermind-pont existe. Ils ne sont jamais construits sur ce PC : ils viennent de la
# construction GitHub (release.yml) de ce commit, et leur attestation est vérifiée (bon dépôt, bon fichier de
# construction, ce commit) avant de prendre leur place dans le dossier ; sinon, rien n'est publié (outils\aide-rust.ps1).
# Sans rust\nevermind-aide ni rust\nevermind-pont, rien de tout ça : la publication reste la même.
$pont = Test-Path -LiteralPath (Join-Path $racine 'rust\nevermind-pont\Cargo.toml')
$programmes = @('Mindset.exe', 'Nevermind.exe', 'NevermindAide.exe', 'Désinstaller Nevermind.exe') + @(if ($pont) { 'NevermindPont.dll' })
if ($pont -or (Test-Path -LiteralPath (Join-Path $racine 'rust\nevermind-aide\Cargo.toml'))) {
    $rust = & (Join-Path $PSScriptRoot 'aide-rust.ps1') -Essai:$Essai
    if ($rust) {
        foreach ($n in @($rust.Keys)) { Copy-Item -LiteralPath $rust[$n] -Destination (Join-Path $app $n) -Force }
        $null = & (Join-Path $PSScriptRoot 'zips.ps1') -Dossier $app -Sortie $racine
        Write-Host "$(@($rust.Keys) -join ' et ') : construits par GitHub (Rust), attestation vérifiée." -ForegroundColor Green
    }
}
$zips = @(Join-Path $racine 'Nevermind.zip')
$cleEssai = $null
if ($Essai) { $cleEssai = New-Object Security.Cryptography.RSACryptoServiceProvider(3072); $cleEssai.PersistKeyInCsp = $false }
try {
    foreach ($zip in $zips) {
        if (-not (Test-Path -LiteralPath $zip)) { throw "Publication annulée : $zip n'a pas été construit." }
        Remove-Item -LiteralPath "$zip.sig" -ErrorAction SilentlyContinue   # jamais l'ancienne signature d'un autre zip
        if ($Essai) { [IO.File]::WriteAllBytes("$zip.sig", $cleEssai.SignData([IO.File]::ReadAllBytes($zip), [Security.Cryptography.CryptoConfig]::MapNameToOID('SHA256'))) }
        else { & (Join-Path (Split-Path $racine -Parent) 'Mindset\outils\signer.ps1') -Fichier $zip }
        if (-not (Test-Path -LiteralPath "$zip.sig")) { throw "Publication annulée : $(Split-Path $zip -Leaf) n'a pas pu être signé." }
    }
} finally { if ($cleEssai) { $cleEssai.Dispose() } }

# Ce que contient le zip : un seul dossier, un seul script principal (Nevermind\fichiers\Nevermind.ps1), et les programmes
Add-Type -AssemblyName System.IO.Compression.FileSystem
foreach ($z in @(,@($zips[0], 'Nevermind', 'Nevermind.ps1'))) {
    $a = [IO.Compression.ZipFile]::OpenRead($z[0])
    try { $noms = @($a.Entries | ForEach-Object { $_.FullName.Replace('\', '/') }) } finally { $a.Dispose() }
    $dossiers = @($noms | ForEach-Object { $_.Split('/')[0] } | Sort-Object -Unique)
    $scripts = @($noms | Where-Object { $_ -match '^[^/]+/fichiers/[^/]+\.ps1$' -and $_ -notmatch '/NevermindAide\.ps1$' })
    $manque = @(@("$($z[1])/fichiers/$($z[2])", "$($z[1])/Mindset.exe", "$($z[1])/Nevermind.exe", "$($z[1])/NevermindAide.exe") + @(if ($pont) { "$($z[1])/NevermindPont.dll" }) | Where-Object { $noms -notcontains $_ })
    if ($dossiers.Count -ne 1 -or $dossiers[0] -ne $z[1] -or $scripts.Count -ne 1 -or $manque.Count) {
        throw "Publication annulée : $(Split-Path $z[0] -Leaf) mal construit (dossiers : $($dossiers -join ', ') ; scripts : $($scripts -join ', ') ; manque : $($manque -join ', '))"
    }
    # Exactement les fichiers de git et les programmes (4, et NevermindPont.dll avec rust\nevermind-pont) : rien en plus
    # (aide-temp, thème perso...), rien en moins
    $attendus = @(@(Get-FichiersGitNevermind) + @($programmes | ForEach-Object { "Nevermind/$_" }) |
        ForEach-Object { $_ -replace '^Nevermind/', "$($z[1])/" })
    $fichiersZip = @($noms | Where-Object { -not $_.EndsWith('/') })
    $enTrop = @($fichiersZip | Where-Object { $attendus -notcontains $_ })
    $absents = @($attendus | Where-Object { $fichiersZip -notcontains $_ })
    if ($enTrop.Count -or $absents.Count) {
        throw "Publication annulée : $(Split-Path $z[0] -Leaf) n'a pas exactement les fichiers de git (en trop : $($enTrop -join ', ') ; absents : $($absents -join ', '))"
    }
    Write-Host "$(Split-Path $z[0] -Leaf) : dossier $($z[1]), script fichiers\$($z[2]), $($noms.Count) fichiers, signé" -ForegroundColor Green
}
if ($Essai) { "Essai fini : $($zips -join ', ') et sa signature (clé jetable). Rien n'est publié."; return }

# 3. Envoi sur GitHub et création de la version
Push-Location $racine
try {
    Invoke-Native git @('tag', "v$Version")   # (le numéro de version est déjà commité, à l'étape 1)
    Invoke-Native git @('push', 'origin', 'HEAD', '--tags')
    if (-not $Notes) { $Notes = "Nevermind $Version" }
    $ghArgs = @('release', 'create', "v$Version") + @($zips | ForEach-Object { $_, "$_.sig" }) + @('--title', "Nevermind $Version$(if ($Beta) { ' (bêta)' })", '--notes', $Notes)
    if ($Beta) { $ghArgs += '--prerelease' }
    Invoke-Native gh $ghArgs
} finally { Pop-Location }

# 4. Mise à jour de la copie installée sur ce PC (celle du raccourci du bureau)
foreach ($f in Get-ChildItem -LiteralPath $app -Recurse -File) {
    $dest = Join-Path $racine $f.FullName.Substring($app.Length + 1)
    # Fichier identique : rien à copier (et il peut être en cours d'utilisation, comme PresentMon pendant une partie).
    if ((Test-Path -LiteralPath $dest) -and (Get-FileHash -LiteralPath $dest).Hash -eq (Get-FileHash -LiteralPath $f.FullName).Hash) { continue }
    New-Item -ItemType Directory -Force -Path (Split-Path $dest -Parent) | Out-Null
    # Nevermind.exe ouvert (l'app tourne dedans) : Windows interdit de l'écraser mais autorise de le renommer.
    # L'ancien est mis de côté (.old) et effacé au prochain lancement de Nevermind. De même pour NevermindPont.dll, chargée
    # par Nevermind (avec rust\nevermind-pont seulement).
    if (($dest -like '*.exe' -or ($pont -and $dest -like '*\NevermindPont.dll')) -and (Test-Path -LiteralPath $dest)) {
        $old = "$dest.old"
        try { if (Test-Path -LiteralPath $old) { [IO.File]::Delete($old) } } catch {}
        try { [IO.File]::Move($dest, $old) } catch {}
    }
    try { Copy-Item -LiteralPath $f.FullName -Destination $dest -Force -ErrorAction Stop }
    catch { Write-Host "Copie impossible (fichier utilisé ?) : $dest" -ForegroundColor Yellow }
}
# (le script et l'icône sous l'ancien nom : jamais deux scripts principaux dans la copie)
foreach ($n in 'OptiGame.ps1', 'OptiGame.ico') {
    $p = Join-Path $racine "fichiers\$n"
    if ((Test-Path -LiteralPath $p) -and (Test-Path -LiteralPath (Join-Path $racine 'fichiers\Nevermind.ps1'))) { try { [IO.File]::Delete($p) } catch {} }
}
"Version $Version publiée."
