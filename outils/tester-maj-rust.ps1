# Nevermind en Rust : la mise à jour depuis la version PowerShell ne perd rien. SUR GITHUB SEULEMENT (job « Contrat » de
# .github\workflows\tests.yml) : le test lance de vrais Nevermind et écrit suite.json dans %LOCALAPPDATA%\Mindset du
# compte (la machine de GitHub est jetée après) ; sur un PC, il refuse de tourner.
#
#   & '.\outils\tester-maj-rust.ps1' -Ancienne <Nevermind.zip de 1.15.6> -Nouvelle <dossier Nevermind en Rust construit>
#
# Comme chez un utilisateur dont Mindset met Nevermind à jour :
#   1. la version PowerShell (le zip publié de 1.15.6) est installée dans <tmp>\Mindset\Programmes\Nevermind, gérée par
#      Mindset (son dossier dans suite.json), avec des données inventées dans <tmp>\Mindset\Donnees\Nevermind
#      (parametres.json, ignores.json, un fichier et une image qui ne sont pas à elle) ; elle est lancée cachée
#      (-Demarrage), écrit ses fichiers, puis quitte par quitter.demande, comme quand Mindset installe une mise à jour ;
#   2. ce qu'elle laisse est relevé : chaque fichier, son empreinte, et pour un fichier JSON chaque champ (ou chaque
#      élément d'une liste) ;
#   3. son dossier de programmes est remplacé par Nevermind en Rust (ce que fait Install-Paquet), lancé caché à son tour,
#      puis arrêté par quitter.demande ;
#   4. rien n'est perdu : chaque fichier est encore là, lisible ; un fichier qui n'est pas du JSON est identique ; un JSON
#      garde chaque champ et chaque élément d'avant (des champs en plus sont permis), et les valeurs inventées sont
#      exactement les mêmes ; le dossier des données n'a pas changé de place (rien de créé dans %LOCALAPPDATA%\OptiGame
#      ni dans %LOCALAPPDATA%\Mindset\Nevermind).
# Ce qui change à chaque lancement n'est pas comparé : etat.json, journal.txt, journal-ancien.txt, les demandes (*.demande),
# le profil WebView2 de la version PowerShell (ecran-webview\) ; d'une copie de secours (*.bak), seulement qu'elle est
# encore là (elle est refaite à chaque écriture).
# Code de sortie 1 si un cas échoue.
param(
    [Parameter(Mandatory = $true)][string]$Ancienne,
    [Parameter(Mandatory = $true)][string]$Nouvelle
)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or -not $env:RUNNER_TEMP) {
    Write-Host 'Ce test ne tourne que sur GitHub : il lance de vrais Nevermind et écrit suite.json dans %LOCALAPPDATA%\Mindset du compte.' -ForegroundColor Red
    exit 1
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
$base = Join-Path $env:RUNNER_TEMP 'maj-nevermind'
$prog = Join-Path $base 'Mindset\Programmes\Nevermind'
$donnees = Join-Path $base 'Mindset\Donnees\Nevermind'
$local = [Environment]::GetFolderPath('LocalApplicationData')
$suiteDir = Join-Path $local 'Mindset'
$suiteFichier = Join-Path $suiteDir 'suite.json'
$ailleurs = @((Join-Path $local 'OptiGame'), (Join-Path $local 'Mindset\Nevermind'))
$utf8 = New-Object Text.UTF8Encoding($false)
$script:Echecs = @()
$script:Reussis = 0
function Test-Cas([string]$Nom, [scriptblock]$Condition) {
    $detail = ''
    try { $r = @(& $Condition) } catch { $r = @(); $detail = $_.Exception.Message }
    if ($r.Count -and $r[-1] -is [bool] -and $r[-1]) { $script:Reussis++; Write-Host "  OK     $Nom" -ForegroundColor Green }
    else { $script:Echecs += $Nom; Write-Host "  ÉCHEC  $Nom$(if ($detail) { " ($detail)" })" -ForegroundColor Red }
}
function Wait-Jusque([scriptblock]$Condition, [double]$Secondes) {
    $fin = (Get-Date).AddSeconds($Secondes)
    do { try { if (& $Condition) { return $true } } catch {}; Start-Sleep -Milliseconds 250 } while ((Get-Date) -lt $fin)
    $false
}
# Lance Mindset.exe de $prog caché (-Demarrage), attend son etat.json, le laisse travailler, puis l'arrête par
# quitter.demande. Rend $true s'il a tourné et quitté de lui-même.
function Invoke-Tour([string]$Nom) {
    Remove-Item -LiteralPath (Join-Path $donnees 'etat.json') -Force -ErrorAction SilentlyContinue
    $p = Start-Process -FilePath (Join-Path $prog 'Mindset.exe') -ArgumentList '-Demarrage' -WorkingDirectory $prog -PassThru
    $parti = Wait-Jusque { Test-Path -LiteralPath (Join-Path $donnees 'etat.json') } 180
    Test-Cas "$Nom : lancée cachée, elle écrit etat.json dans <tmp>\Mindset\Donnees\Nevermind (le même dossier de données)" { $parti }
    $null = Wait-Jusque { $p.HasExited } 20   # (le temps d'écrire ses fichiers au démarrage)
    [IO.File]::WriteAllText((Join-Path $donnees 'quitter.demande'), [string]$PID, $utf8)
    $quitte = Wait-Jusque { $p.HasExited } 30
    Test-Cas "$Nom : quitter.demande, elle quitte d'elle-même en moins de 30 s" { $quitte }
    if (-not $p.HasExited) { try { $p.Kill(); $null = $p.WaitForExit(5000) } catch {} }
    # (ses autres programmes lancés depuis son dossier, s'il en reste : fermés par leur chemin, comme le fait Mindset)
    Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith("$prog\", [StringComparison]::OrdinalIgnoreCase) } | ForEach-Object { try { $_.Kill() } catch {} }
    Remove-Item -LiteralPath (Join-Path $donnees 'quitter.demande') -Force -ErrorAction SilentlyContinue
    if (-not $parti) {
        $j = Join-Path $donnees 'journal.txt'
        if (Test-Path -LiteralPath $j) { Write-Host "  (fin de journal.txt :)"; Get-Content -LiteralPath $j -Tail 30 -Encoding UTF8 | ForEach-Object { "    $_" } }
    }
}
# Ce que contient le dossier des données : chemin relatif -> @{ Hash; Json (le texte de chaque champ ou élément) }
$volatils = '^(etat\.json|journal\.txt|journal-ancien\.txt|[^\\]+\.demande|ecran-webview\\.*)$'
function Get-Releve {
    $r = [ordered]@{}
    foreach ($f in @(Get-ChildItem -LiteralPath $donnees -Recurse -File -Force | Sort-Object FullName)) {
        $rel = $f.FullName.Substring($donnees.Length + 1)
        if ($rel -match $volatils) { continue }
        $e = @{ Hash = (Get-FileHash -LiteralPath $f.FullName -Algorithm SHA256).Hash; Json = $null }
        if ($f.Extension -ieq '.json') {
            try {
                $o = [IO.File]::ReadAllText($f.FullName, [Text.Encoding]::UTF8).TrimStart([char]0xFEFF) | ConvertFrom-Json
                $champs = @{}
                if ($o -is [Management.Automation.PSCustomObject]) { foreach ($p in $o.PSObject.Properties) { $champs["champ:$($p.Name)"] = ConvertTo-Json $p.Value -Depth 20 -Compress } }
                elseif ($o -is [array]) { $i = 0; foreach ($x in $o) { $champs["élément:$(ConvertTo-Json $x -Depth 20 -Compress)"] = ''; $i++ } }
                $e.Json = $champs
            } catch { $e.Json = 'illisible' }
        }
        $r[$rel] = $e
    }
    $r
}

$avantSuite = if (Test-Path -LiteralPath $suiteFichier) { [IO.File]::ReadAllBytes($suiteFichier) } else { $null }
try {
    if (Test-Path -LiteralPath $base) { Remove-Item -LiteralPath $base -Recurse -Force }
    foreach ($a in $ailleurs) { if (Test-Path -LiteralPath $a) { throw "$a existe déjà sur cette machine : le test ne saurait pas qui l'a créé." } }

    Write-Host ''; Write-Host '1. La version PowerShell, gérée par Mindset, avec des données inventées' -ForegroundColor Cyan
    $zipDir = Join-Path $base 'zip'
    [IO.Compression.ZipFile]::ExtractToDirectory((Resolve-Path -LiteralPath $Ancienne).Path, $zipDir)
    New-Item -ItemType Directory -Force -Path (Split-Path $prog -Parent) | Out-Null
    Move-Item -LiteralPath (Join-Path $zipDir 'Nevermind') -Destination $prog
    $versionAncienne = if ([IO.File]::ReadAllText((Join-Path $prog 'fichiers\Nevermind.ps1'), [Text.Encoding]::UTF8) -match "(?m)^\`$AppVersion = '([\d\.]+)'") { $Matches[1] } else { '' }
    Write-Host "  version PowerShell installée : $versionAncienne"
    New-Item -ItemType Directory -Force -Path $suiteDir, $donnees, (Join-Path $donnees 'jaquettes') | Out-Null
    [IO.File]::WriteAllText($suiteFichier, (@{ applis = @{ nevermind = $prog } } | ConvertTo-Json -Compress), $utf8)
    # Les données inventées : des réglages qu'aucune version ne connaît (gardés tels quels par Get-Setting et Set-Setting),
    # un point ignoré, et deux fichiers qui ne sont pas à Nevermind
    $inventes = [ordered]@{ EssaiMaj = 'garder-moi'; EssaiNombre = 42; EssaiAccents = "é à ü œ ç"; EssaiListe = @('un', 'deux') }
    [IO.File]::WriteAllText((Join-Path $donnees 'parametres.json'), ($inventes | ConvertTo-Json -Depth 5), $utf8)
    [IO.File]::WriteAllText((Join-Path $donnees 'ignores.json'), '["essai-maj-ignore"]', $utf8)
    [IO.File]::WriteAllText((Join-Path $donnees 'note-essai.txt'), "Un fichier laissé là par l'utilisateur, é à ü`r`n", $utf8)
    [IO.File]::WriteAllBytes((Join-Path $donnees 'jaquettes\essai.png'), [byte[]](0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4))
    Invoke-Tour "Nevermind $versionAncienne (PowerShell)"
    $releve = Get-Releve
    Write-Host "  relevé : $($releve.Count) fichiers ($(@($releve.Keys) -join ', '))"
    Test-Cas "Nevermind $versionAncienne a gardé les données inventées (sinon ce test ne prouverait rien)" {
        $p = $releve['parametres.json']
        $p -and $p.Json -is [hashtable] -and $p.Json['champ:EssaiMaj'] -ceq '"garder-moi"' -and $p.Json['champ:EssaiNombre'] -ceq '42' -and
        $releve['ignores.json'].Json.ContainsKey('élément:"essai-maj-ignore"') -and $releve.Contains('note-essai.txt') -and $releve.Contains('jaquettes\essai.png')
    }

    Write-Host ''; Write-Host '2. La mise à jour : Nevermind en Rust à la place, mêmes données' -ForegroundColor Cyan
    Remove-Item -LiteralPath $prog -Recurse -Force
    Copy-Item -LiteralPath ([IO.Path]::GetFullPath($Nouvelle)) -Destination $prog -Recurse
    $vj = [IO.File]::ReadAllText((Join-Path $prog 'version.json'), [Text.Encoding]::UTF8) | ConvertFrom-Json
    Write-Host "  version en Rust installée : $($vj.version)"
    Invoke-Tour "Nevermind $($vj.version) (Rust)"

    Write-Host ''; Write-Host '3. Rien de perdu' -ForegroundColor Cyan
    $apres = Get-Releve
    foreach ($rel in $releve.Keys) {
        $a = $releve[$rel]; $b = $apres[$rel]
        if ($a.Json -is [hashtable]) {
            Test-Cas "$rel : toujours là, lisible, chaque champ et élément d'avant gardé" {
                if (-not $b -or $b.Json -isnot [hashtable]) { throw $(if ($b) { 'illisible ou plus du même genre' } else { 'disparu' }) }
                $perdus = @($a.Json.Keys | Where-Object { -not $b.Json.ContainsKey($_) })
                if ($perdus.Count) { throw "perdus : $($perdus -join ', ')" }
                $true
            }
        } elseif ($rel -match '\.bak$') {
            # (une copie de secours est refaite à chaque écriture du fichier : seulement qu'elle soit encore là)
            Test-Cas "$rel : copie de secours toujours là" { [bool]$b }
        } else {
            Test-Cas "$rel : toujours là, identique" { $b -and $b.Hash -eq $a.Hash }
        }
    }
    Test-Cas 'les valeurs inventées de parametres.json sont exactement les mêmes' {
        $b = $apres['parametres.json'].Json
        $changes = @($inventes.Keys | Where-Object { $b["champ:$_"] -cne $releve['parametres.json'].Json["champ:$_"] })
        if ($changes.Count) { throw "changées : $($changes -join ', ')" }
        $true
    }
    Test-Cas 'le dossier des données n''a pas changé de place (rien dans %LOCALAPPDATA%\OptiGame ni %LOCALAPPDATA%\Mindset\Nevermind)' {
        -not @($ailleurs | Where-Object { Test-Path -LiteralPath $_ }).Count
    }
} finally {
    if ($null -ne $avantSuite) { [IO.File]::WriteAllBytes($suiteFichier, $avantSuite) } else { Remove-Item -LiteralPath $suiteFichier -Force -ErrorAction SilentlyContinue }
}

Write-Host ''
if ($script:Echecs.Count) { Write-Host "Mise à jour depuis la version PowerShell : $($script:Echecs.Count) cas raté(s) sur $($script:Reussis + $script:Echecs.Count)." -ForegroundColor Red; exit 1 }
Write-Host "Mise à jour depuis la version PowerShell : $($script:Reussis) cas sur $($script:Reussis), rien de perdu." -ForegroundColor Green
exit 0
