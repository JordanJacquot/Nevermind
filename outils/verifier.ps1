# Vérifications rapides du dépôt, sans rien lancer et sans fenêtre (utilisées par la CI GitHub et utilisables à la main) :
#   - syntaxe de tous les fichiers .ps1 ;
#   - la fenêtre (interface.xaml) est un XML lisible ;
#   - aucun tiret cadratin ni demi-cadratin dans les textes (.ps1, .cs, .xaml, .txt, .md, .bat) ;
#   - chaque .ps1 est en UTF-8 avec BOM (sans BOM, PowerShell 5.1 casse les accents) ;
#   - le fichier LICENSE est présent.
# Mêmes contrôles que outils\publier.ps1 avant une publication (sauf les clés publiques (cles-publiques), comparées à celles de Mindset,
# qui n'est pas dans ce dépôt).
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\verifier.ps1
#
# Code de sortie : 0 si tout va bien, 1 s'il y a au moins un problème (la liste est affichée).

$ErrorActionPreference = 'Stop'
$racine = Split-Path $PSScriptRoot -Parent
$app    = Join-Path $racine 'OptiGame'

# Les fichiers du projet : ceux que git suit, plus les nouveaux pas encore ajoutés (hors fichiers ignorés, comme la
# copie installée à la racine). Sans git : tout le dossier, hors .git, _test et release.
function Get-FichiersProjet {
    $liste = $null
    if (Get-Command git -ErrorAction SilentlyContinue) {
        $avant = [Console]::OutputEncoding
        try {
            [Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
            $sortie = & git -C $racine -c core.quotepath=false ls-files -z --cached --others --exclude-standard 2>$null
            if ($LASTEXITCODE -eq 0) {
                $liste = @(($sortie -join '') -split "`0" | Where-Object { $_ } | Sort-Object -Unique |
                    ForEach-Object { Join-Path $racine ($_ -replace '/', '\') } | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf })
            }
        } catch { $liste = $null } finally { [Console]::OutputEncoding = $avant }
    }
    if ($null -eq $liste) {
        $liste = @(Get-ChildItem -LiteralPath $racine -Recurse -File -Force | ForEach-Object { $_.FullName })
    }
    @($liste | Where-Object { $_ -notmatch '\\(\.git|_test|release)\\' })
}

$problemes = New-Object System.Collections.Generic.List[string]
$fichiers = Get-FichiersProjet
function Get-Relatif([string]$Chemin) { $Chemin.Substring($racine.Length).TrimStart('\') }

# 1. Syntaxe et BOM de chaque .ps1
$ps1 = @($fichiers | Where-Object { $_ -match '\.ps1$' })
foreach ($f in $ps1) {
    $err = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile($f, [ref]$null, [ref]$err)
    foreach ($e in $err) { $problemes.Add("Syntaxe : $(Get-Relatif $f) ligne $($e.Extent.StartLineNumber) : $($e.Message)") }
    $debut = New-Object byte[] 3
    $flux = [IO.File]::OpenRead($f)
    try { $lus = $flux.Read($debut, 0, 3) } finally { $flux.Close() }
    if ($lus -lt 3 -or $debut[0] -ne 0xEF -or $debut[1] -ne 0xBB -or $debut[2] -ne 0xBF) {
        $problemes.Add("Encodage : $(Get-Relatif $f) n'est pas en UTF-8 avec BOM (les accents cassent dans PowerShell 5.1)")
    }
}

# 2. La fenêtre
$xaml = Join-Path $app 'fichiers\modules\interface.xaml'
if (-not (Test-Path -LiteralPath $xaml)) { $problemes.Add('interface.xaml : fichier absent') }
else {
    try { [void][xml][IO.File]::ReadAllText($xaml, [Text.Encoding]::UTF8) } catch { $problemes.Add("interface.xaml : $($_.Exception.Message)") }
}

# 3. Tirets cadratins (U+2014) et demi-cadratins (U+2013)
$utf8Strict = New-Object Text.UTF8Encoding($false, $true)
$textes = @($fichiers | Where-Object { $_ -match '\.(ps1|cs|xaml|txt|md|bat)$' })
foreach ($f in $textes) {
    $octets = [IO.File]::ReadAllBytes($f)
    $debut = 0
    if ($octets.Length -ge 3 -and $octets[0] -eq 0xEF -and $octets[1] -eq 0xBB -and $octets[2] -eq 0xBF) { $debut = 3 }
    try { $texte = $utf8Strict.GetString($octets, $debut, $octets.Length - $debut) }
    catch { $texte = [Text.Encoding]::Default.GetString($octets) }   # (vieux fichier .bat en ANSI)
    $lignes = $texte -split "`n"
    for ($i = 0; $i -lt $lignes.Count; $i++) {
        if ($lignes[$i] -match '[\u2013\u2014]') { $problemes.Add("Tiret cadratin : $(Get-Relatif $f) ligne $($i + 1)") }
    }
}

# 4. La licence
if (-not (Test-Path -LiteralPath (Join-Path $racine 'LICENSE') -PathType Leaf)) { $problemes.Add('Licence : le fichier LICENSE est absent') }

Write-Host "$($ps1.Count) fichiers .ps1 et $($textes.Count) fichiers texte vérifiés."
if ($problemes.Count) {
    Write-Host "$($problemes.Count) problème(s) :" -ForegroundColor Red
    foreach ($p in $problemes) { Write-Host "  - $p" -ForegroundColor Red }
    exit 1
}
Write-Host 'Tout est bon : syntaxe, fenêtre, tirets, encodage et licence.' -ForegroundColor Green
exit 0
