# Tests du changement de nom (OptiGame devient Nevermind), sans droits administrateur et sans rien toucher sur le PC.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-renommage.ps1
#
# Ce script contrôle :
#   1. plus aucun « OptiGame » dans le dépôt (noms de fichiers et contenu), sauf les mentions gardées exprès, fichier par
#      fichier avec leur raison (ce qui reconnaît ou migre une ancienne installation, le zip de compatibilité, les tests) ;
#      contre-épreuve : le dépôt de Nevermind 1.11.0 en a bien plus ;
#   2. le lanceur (outils\lanceur.cs, compilé ici) démarre fichiers\Nevermind.ps1, ou à défaut fichiers\OptiGame.ps1 ;
#   3. le verrou (une seule instance, Nevermind.ps1) : les deux noms sont pris ; tant que l'ancien est tenu par un autre
#      programme (une version d'avant encore ouverte), Nevermind ne démarre pas et ne garde aucun verrou ; un deuxième
#      Nevermind ne démarre pas non plus (verrous de test à des noms uniques : jamais ceux du vrai Nevermind) ;
#   4. les deux zips (outils\zips.ps1) : Nevermind.zip (dossier Nevermind, script Nevermind.ps1) et le zip de compatibilité
#      OptiGame.zip (dossier OptiGame, script OptiGame.ps1, ce que cherchent les Mindset 2.12.0 et plus anciens), un seul
#      script principal dans chacun ;
#   5. les mises à jour faites par Nevermind lui-même : Nevermind.zip d'abord, OptiGame.zip pour une version d'avant ;
#      le script principal trouvé sous ses deux noms (Get-AppScript).
# Code de sortie : 0 si tout passe, 1 sinon.

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$fichiers = Join-Path $racine 'Nevermind\fichiers'
$Avant   = 'e3dbfd2'   # Nevermind 1.11.0, la dernière version d'avant le changement de nom (contre-épreuves)
$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0
function Test-Ok([bool]$Condition, [string]$Message) {
    $script:nbTests++
    if (-not $Condition) { $script:echecs.Add($Message) }
}
function Invoke-Git([string[]]$Arguments) {
    $avant = [Console]::OutputEncoding
    try {
        [Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
        $ErrorActionPreference = 'Continue'
        @(& git -C $racine -c core.quotepath=false @Arguments 2>$null)
    } finally { [Console]::OutputEncoding = $avant }
}
# Les définitions (fonctions ou « $variable = ... ») lues dans un fichier, sans l'exécuter
function Get-Definitions([string]$Texte, [string[]]$Noms) {
    $tok = $null; $err = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseInput($Texte, [ref]$tok, [ref]$err)
    if ($err) { throw "Texte illisible : $($err[0].Message)" }
    @(foreach ($n in $Noms) {
        $def = if ($n.StartsWith('$')) {
            $ast.Find({ param($x) $x -is [System.Management.Automation.Language.AssignmentStatementAst] -and $x.Left.Extent.Text -eq $n }, $true)
        } else {
            $ast.Find({ param($x) $x -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $x.Name -eq $n }, $true)
        }
        if (-not $def) { throw "$n introuvable" }
        $def.Extent.Text
    }) -join "`r`n"
}
$bac = Join-Path $env:TEMP "nevermind-renommage-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
New-Item -ItemType Directory -Force -Path $bac | Out-Null

# ---------------------------------------------------------------------------
# 1. Plus aucun « OptiGame », sauf les mentions gardées exprès
# ---------------------------------------------------------------------------
# Fichier => nombre de lignes où l'ancien nom reste, et pourquoi
$Gardees = [ordered]@{
    '.gitignore'                                   = @(3, 'le zip de compatibilité OptiGame.zip (et sa signature), produit par la construction')
    'Nevermind/fichiers/Désinstaller Nevermind (secours).bat' = @(2, 'script à l''ancien nom dans le zip de compatibilité')
    'Nevermind/fichiers/Lancer Nevermind (secours).bat' = @(2, 'script à l''ancien nom dans le zip de compatibilité')
    'Nevermind/fichiers/Nevermind.ps1'             = @(4, 'ancien dossier de données %LOCALAPPDATA%\OptiGame (migration), ancien verrou')
    'Nevermind/fichiers/modules/aide-actions.ps1'  = @(9, 'règles du pare-feu « OptiGame : bloque ... » d''avant la 1.3.6, reconnues et renommées')
    'Nevermind/fichiers/modules/assistance.ps1'    = @(1, 'lignes « Démarrage OptiGame » des anciens journaux')
    'Nevermind/fichiers/modules/donnees.ps1'       = @(1, 'ancien dossier de données')
    'Nevermind/fichiers/modules/interface.ps1'     = @(2, 'ancienne tâche de démarrage reconnue (script OptiGame.ps1)')
    'Nevermind/fichiers/modules/mises-a-jour.ps1'  = @(6, 'mise à jour depuis une version publiée sous l''ancien nom ; copie à l''ancien nom gardée sous ce nom')
    'Nevermind/fichiers/modules/natif.cs'          = @(1, 'ancienne session réseau « OptiGame-Reseau » fermée au démarrage')
    'Nevermind/fichiers/modules/systeme.ps1'       = @(12, 'anciennes tâches, anciens lanceurs et anciens fichiers retirés ; script à l''ancien nom')
    'Nevermind/fichiers/modules/tests.ps1'         = @(1, 'ancien fichier temporaire du test du disque, effacé')
    'outils/essai-aide-reelle.ps1'                 = @(36, 'essai en vrai : anciennes Releases, Mindset 2.12.0, zip de compatibilité')
    'outils/lanceur.cs'                            = @(4, 'ancien dossier de données ; script à l''ancien nom')
    'outils/publier.ps1'                           = @(6, 'publie aussi le zip de compatibilité')
    'outils/test-app.ps1'                          = @(5, 'test : aucun « OptiGame » affiché ; règle à l''ancien nom ; ancien dossier de données')
    'outils/tester-aide-systeme.ps1'               = @(4, 'contre-épreuves : fichiers de versions d''avant, relus dans git sous leur ancien chemin')
    'outils/tester-aide.ps1'                       = @(1, 'règle du pare-feu à l''ancien nom')
    'outils/tester-desinstallation.ps1'            = @(1, 'ancienne tâche de démarrage')
    'outils/tester-reseau.ps1'                     = @(1, 'test : aucun « OptiGame » affiché')
    'outils/tester.ps1'                            = @(2, 'ancien dossier de données (lecture seule)')
    'outils/zips.ps1'                              = @(6, 'fait le zip de compatibilité')
}
function Get-Mentions([string]$Rev) {
    # Lignes qui contiennent l'ancien nom, par fichier (ce test lui-même mis à part : il liste les exceptions)
    $sortie = if ($Rev) { Invoke-Git @('grep', '-I', '-i', '-c', 'optigame', $Rev) } else { Invoke-Git @('grep', '--untracked', '-I', '-i', '-c', 'optigame') }
    $m = @{}
    foreach ($l in $sortie) {
        if ($Rev) { $l = $l.Substring($Rev.Length + 1) }
        $i = $l.LastIndexOf(':'); $f = $l.Substring(0, $i).Trim('"'); $n = [int]$l.Substring($i + 1)
        if ($f -ne 'outils/tester-renommage.ps1') { $m[$f] = $n }
    }
    $m
}
function Get-EnTrop([hashtable]$Mentions) {
    @(foreach ($f in $Mentions.Keys) {
        $permis = if ($Gardees.Contains($f)) { $Gardees[$f][0] } else { 0 }
        if ($Mentions[$f] -gt $permis) { "$f ($($Mentions[$f]) ligne(s), $permis gardée(s))" }
    })
}
$maintenant = Get-Mentions ''
$enTrop = Get-EnTrop $maintenant
Test-Ok (-not $enTrop.Count) "Textes : « OptiGame » encore là sans raison : $($enTrop -join ' ; ')"
$total = 0; foreach ($n in $maintenant.Values) { $total += $n }
$noms = @(Invoke-Git @('ls-files', '--cached', '--others', '--exclude-standard') | Where-Object { $_ -match '(?i)optigame' })
Test-Ok (-not $noms.Count) "Noms de fichiers : « OptiGame » encore là : $($noms -join ', ')"
$ancien = Get-Mentions $Avant
if ($ancien.Count) {
    Test-Ok ((Get-EnTrop $ancien).Count -gt 20) "Contre-épreuve : Nevermind 1.11.0 ($Avant) devrait avoir bien plus de mentions de l'ancien nom"
    Test-Ok (@(Invoke-Git @('ls-tree', '-r', '--name-only', $Avant) | Where-Object { $_ -match '(?i)optigame' }).Count -gt 50) 'Contre-épreuve : Nevermind 1.11.0 avait un dossier OptiGame'
} else { Write-Host "  (contre-épreuve sautée : $Avant absent de l'historique)" -ForegroundColor Yellow }

try {
    # -----------------------------------------------------------------------
    # 2. Le lanceur démarre Nevermind.ps1, ou à défaut OptiGame.ps1
    # -----------------------------------------------------------------------
    $csc = Join-Path $env:windir 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
    $sma = @(Get-ChildItem (Join-Path $env:windir 'Microsoft.NET\assembly\GAC_MSIL\System.Management.Automation') -Recurse -Filter 'System.Management.Automation.dll' | Select-Object -First 1)[0].FullName
    $lanceur = Join-Path $bac 'lanceur'
    New-Item -ItemType Directory -Force -Path (Join-Path $lanceur 'fichiers') | Out-Null
    # (compilé comme le désinstalleur : même choix du script, et rien d'autre que le script n'est lancé)
    $exe = Join-Path $lanceur 'Désinstaller Nevermind.exe'
    $null = & $csc /nologo /target:winexe /reference:System.Windows.Forms.dll /reference:System.Web.Extensions.dll "/reference:$sma" /define:UNINSTALL "/out:$exe" (Join-Path $PSScriptRoot 'lanceur.cs')
    Test-Ok ($LASTEXITCODE -eq 0) 'Le lanceur se compile'
    $sortie = Join-Path $bac 'lance.txt'
    $faux = "param([switch]`$Uninstall)`r`n[IO.File]::WriteAllText('$sortie', [IO.Path]::GetFileName(`$PSCommandPath))"
    foreach ($c in @(@(@('Nevermind.ps1'), 'Nevermind.ps1'), @(@('OptiGame.ps1'), 'OptiGame.ps1'), @(@('Nevermind.ps1', 'OptiGame.ps1'), 'Nevermind.ps1'))) {
        Get-ChildItem -LiteralPath (Join-Path $lanceur 'fichiers') -File | Remove-Item -Force
        foreach ($n in $c[0]) { [IO.File]::WriteAllText((Join-Path $lanceur "fichiers\$n"), $faux, (New-Object Text.UTF8Encoding $true)) }
        Remove-Item -LiteralPath $sortie -ErrorAction SilentlyContinue
        $p = Start-Process -FilePath $exe -PassThru
        if (-not $p.WaitForExit(30000)) { try { $p.Kill() } catch {} }
        $recu = if (Test-Path -LiteralPath $sortie) { [IO.File]::ReadAllText($sortie) } else { '(rien)' }
        Test-Ok ($recu -eq $c[1]) "Lanceur, script(s) $($c[0] -join ' et ') : il doit lancer $($c[1]) (lancé : $recu)"
    }
    $lanceurAvant = (Invoke-Git @('show', "${Avant}:outils/lanceur.cs")) -join "`n"
    if ($lanceurAvant) { Test-Ok ($lanceurAvant -notmatch 'Nevermind\.ps1') 'Contre-épreuve : le lanceur de 1.11.0 ne connaissait pas Nevermind.ps1' }

    # -----------------------------------------------------------------------
    # 3. Le verrou : les deux noms, l'ancien tenu ailleurs bloque, une seule instance
    # -----------------------------------------------------------------------
    $source = [IO.File]::ReadAllText((Join-Path $fichiers 'Nevermind.ps1'), [Text.Encoding]::UTF8)
    Test-Ok ($source -match "(?m)^\`$InstanceNames = @\('Local\\Nevermind-Instance', 'Local\\OptiGame-Instance'\)") 'Verrou : Nevermind.ps1 prend Local\Nevermind-Instance et l''ancien Local\OptiGame-Instance'
    Test-Ok (@([regex]::Matches($source, 'Mutex\]?::OpenExisting\(''|New-Object System\.Threading\.Mutex\(\$true, ''')).Count -eq 0) 'Verrou : aucun nom de verrou écrit ailleurs dans Nevermind.ps1 (tout passe par $InstanceNames)'
    $code = Get-Definitions $source @('Test-InstanceRunning', 'Enter-Instance', 'Exit-Instance')
    . ([scriptblock]::Create($code))
    $g = [guid]::NewGuid().ToString('N').Substring(0, 12)
    $InstanceNames = @("Local\NevermindEssai-$g", "Local\OptiGameEssai-$g")   # (jamais les vrais noms)
    $script:InstanceMutexes = @()
    # Un autre programme tient l'ancien verrou (une version d'avant encore ouverte)
    $pret = Join-Path $bac 'pret.txt'
    $tenir = "`$m = New-Object System.Threading.Mutex(`$true, '$($InstanceNames[1])'); [IO.File]::WriteAllText('$pret', '1'); Start-Sleep -Seconds 60"
    $vieux = Start-Process -FilePath 'powershell.exe' -ArgumentList '-NoProfile', '-WindowStyle', 'Hidden', '-EncodedCommand', ([Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($tenir))) -WindowStyle Hidden -PassThru
    for ($i = 0; $i -lt 100 -and -not (Test-Path -LiteralPath $pret); $i++) { Start-Sleep -Milliseconds 100 }
    Test-Ok (Test-Path -LiteralPath $pret) 'Verrou : le faux ancien Nevermind tient l''ancien verrou'
    Test-Ok (Test-InstanceRunning) 'Verrou : l''ancien verrou tenu ailleurs = Nevermind est déjà ouvert'
    $r = Enter-Instance 500
    Test-Ok ($r -eq 'occupe') "Verrou : tant que l'ancien est tenu, Nevermind ne démarre pas (réponse : $r)"
    $libre = $true; try { ([Threading.Mutex]::OpenExisting($InstanceNames[0])).Dispose(); $libre = $false } catch {}
    Test-Ok ($libre -and -not @($script:InstanceMutexes).Count) 'Verrou : refusé, il ne garde pas le nouveau verrou (sinon Mindset le croirait ouvert)'
    try { $vieux.Kill() } catch {}; [void]$vieux.WaitForExit(10000)
    Test-Ok (-not (Test-InstanceRunning)) 'Verrou : l''ancien fermé, plus rien n''est tenu'
    $r = Enter-Instance 500
    $tenus = @($InstanceNames | Where-Object { try { ([Threading.Mutex]::OpenExisting($_)).Dispose(); $true } catch { $false } })
    Test-Ok ($r -eq 'ok' -and $tenus.Count -eq 2) "Verrou : seul, Nevermind démarre et tient les deux verrous (réponse : $r, tenus : $($tenus.Count))"
    # Un deuxième Nevermind (autre programme) : il ne démarre pas
    $deux = Join-Path $bac 'deux.txt'
    $code2 = "$code`r`n`$InstanceNames = @('$($InstanceNames[0])', '$($InstanceNames[1])'); `$script:InstanceMutexes = @(); [IO.File]::WriteAllText('$deux', (Enter-Instance 500))"
    $p = Start-Process -FilePath 'powershell.exe' -ArgumentList '-NoProfile', '-WindowStyle', 'Hidden', '-EncodedCommand', ([Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($code2))) -WindowStyle Hidden -PassThru
    [void]$p.WaitForExit(30000)
    $rep = if (Test-Path -LiteralPath $deux) { [IO.File]::ReadAllText($deux) } else { '(rien)' }
    Test-Ok ($rep -eq 'occupe') "Verrou : un deuxième Nevermind ne démarre pas (réponse : $rep)"
    Exit-Instance
    Test-Ok (-not (Test-InstanceRunning)) 'Verrou : à la fermeture, les deux verrous sont rendus'
    $sourceAvant = (Invoke-Git @('show', "${Avant}:OptiGame/fichiers/OptiGame.ps1")) -join "`n"
    if ($sourceAvant) { Test-Ok ($sourceAvant -notmatch 'Nevermind-Instance') 'Contre-épreuve : 1.11.0 ne prenait que l''ancien verrou' }

    # -----------------------------------------------------------------------
    # 4. Les deux zips
    # -----------------------------------------------------------------------
    $app = Join-Path $bac 'construit\Nevermind'
    New-Item -ItemType Directory -Force -Path (Join-Path $app 'fichiers\modules') | Out-Null
    [IO.File]::WriteAllText((Join-Path $app 'fichiers\Nevermind.ps1'), "`$AppVersion = '9.8.7'`r`n# script", (New-Object Text.UTF8Encoding $true))
    foreach ($f in 'fichiers\NevermindAide.ps1', 'fichiers\modules\systeme.ps1', 'fichiers\Nevermind.ico', 'Mindset.exe', 'Nevermind.exe', 'NevermindAide.exe', 'Désinstaller Nevermind.exe', 'LISEZMOI.txt') {
        [IO.File]::WriteAllText((Join-Path $app $f), $f)
    }
    $sortieZips = Join-Path $bac 'zips'
    $zips = @(& (Join-Path $PSScriptRoot 'zips.ps1') -Dossier $app -Sortie $sortieZips)
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    function Get-Entrees([string]$Zip) {
        $a = [IO.Compression.ZipFile]::OpenRead($Zip)
        try { @($a.Entries | ForEach-Object { $_.FullName.Replace('\', '/') } | Where-Object { -not $_.EndsWith('/') }) } finally { $a.Dispose() }
    }
    function Get-TexteEntree([string]$Zip, [string]$Nom) {
        $a = [IO.Compression.ZipFile]::OpenRead($Zip)
        try { $e = @($a.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq $Nom })[0]; if (-not $e) { return $null }; $s = New-Object IO.StreamReader($e.Open()); try { $s.ReadToEnd() } finally { $s.Dispose() } } finally { $a.Dispose() }
    }
    $zn = Join-Path $sortieZips 'Nevermind.zip'; $zo = Join-Path $sortieZips 'OptiGame.zip'
    Test-Ok ($zips.Count -eq 2 -and (Test-Path -LiteralPath $zn) -and (Test-Path -LiteralPath $zo)) 'Zips : Nevermind.zip et OptiGame.zip construits'
    $en = Get-Entrees $zn; $eo = Get-Entrees $zo
    Test-Ok (-not @($en | Where-Object { $_ -notlike 'Nevermind/*' }).Count -and $en -contains 'Nevermind/fichiers/Nevermind.ps1' -and $en -notcontains 'Nevermind/fichiers/OptiGame.ps1') 'Nevermind.zip : dossier Nevermind, script fichiers\Nevermind.ps1 seul'
    Test-Ok (-not @($eo | Where-Object { $_ -notlike 'OptiGame/*' }).Count -and $eo -contains 'OptiGame/fichiers/OptiGame.ps1' -and $eo -notcontains 'OptiGame/fichiers/Nevermind.ps1') 'OptiGame.zip (compatibilité) : dossier OptiGame, script fichiers\OptiGame.ps1 seul (ce que cherchent les Mindset 2.12.0 et plus anciens)'
    Test-Ok ($eo -contains 'OptiGame/Nevermind.exe' -and $eo -contains 'OptiGame/NevermindAide.exe' -and $eo.Count -eq $en.Count) 'OptiGame.zip : le même contenu (programmes compris)'
    Test-Ok ((Get-TexteEntree $zo 'OptiGame/fichiers/OptiGame.ps1') -eq (Get-TexteEntree $zn 'Nevermind/fichiers/Nevermind.ps1') -and (Get-TexteEntree $zo 'OptiGame/fichiers/OptiGame.ps1') -match "AppVersion = '9\.8\.7'") 'OptiGame.zip : le script est le même (version lisible par les anciens Mindset), seul son nom change'
    # Contre-épreuve : un dossier avec les deux scripts est refusé (deux scripts principaux)
    [IO.File]::WriteAllText((Join-Path $app 'fichiers\OptiGame.ps1'), '# ancien')
    $refuse = $false; try { $null = & (Join-Path $PSScriptRoot 'zips.ps1') -Dossier $app -Sortie (Join-Path $bac 'zips2') } catch { $refuse = $true }
    Test-Ok $refuse 'Zips : un dossier avec les deux scripts est refusé'

    # -----------------------------------------------------------------------
    # 5. Mises à jour faites par Nevermind lui-même, script sous ses deux noms
    # -----------------------------------------------------------------------
    $maj = [IO.File]::ReadAllText((Join-Path $fichiers 'modules\mises-a-jour.ps1'), [Text.Encoding]::UTF8)
    . ([scriptblock]::Create((Get-Definitions $maj @('$GetReleaseScript'))))
    $script:Assets = @()
    function Invoke-RestMethod { [pscustomobject]@{ tag_name = 'v9.9.9'; prerelease = $false; body = ''; assets = $script:Assets } }   # (jamais Internet)
    function New-Asset([string]$Nom) { [pscustomobject]@{ name = $Nom; browser_download_url = "https://exemple.test/$Nom" } }
    $script:Assets = @((New-Asset 'OptiGame.zip'), (New-Asset 'OptiGame.zip.sig'), (New-Asset 'Nevermind.zip'), (New-Asset 'Nevermind.zip.sig'))
    $r = & $GetReleaseScript @{ Repo = 'essai/essai'; Beta = $false }
    Test-Ok ($r.Url -like '*/Nevermind.zip' -and $r.SigUrl -like '*/Nevermind.zip.sig') "Mise à jour : Nevermind.zip et sa signature d'abord (reçu : $($r.Url) $($r.SigUrl) $($r.Error))"
    $r = & $GetReleaseScript @{ Repo = 'essai/essai'; Beta = $false; Ancien = $true }
    Test-Ok ($r.Url -like '*/OptiGame.zip' -and $r.SigUrl -like '*/OptiGame.zip.sig') "Mise à jour d'une copie au script à l'ancien nom : le zip de compatibilité (les Mindset 2.12.0 ne reconnaîtraient pas Nevermind.zip ; reçu : $($r.Url))"
    Test-Ok ($maj -match "Ancien = \(\(Split-Path \(Get-AppScript\) -Leaf\) -eq 'OptiGame\.ps1'\)" -and $maj -match 'if \(-not \$rel -or \$Managed\) \{ return \}') 'Mise à jour autonome : jamais pour une copie gérée par Mindset, et le zip de compatibilité pour une copie à l''ancien nom'
    $script:Assets = @((New-Asset 'OptiGame.zip'), (New-Asset 'OptiGame.zip.sig'))
    $r = & $GetReleaseScript @{ Repo = 'essai/essai'; Beta = $false }
    Test-Ok ($r.Url -like '*/OptiGame.zip' -and $r.SigUrl -like '*/OptiGame.zip.sig') "Mise à jour : une version d'avant (OptiGame.zip seul) reste lisible (reçu : $($r.Url) $($r.SigUrl) $($r.Error))"
    $majAvant = (Invoke-Git @('show', "${Avant}:OptiGame/fichiers/modules/mises-a-jour.ps1")) -join "`n"
    if ($majAvant) {
        Remove-Variable GetReleaseScript
        . ([scriptblock]::Create((Get-Definitions $majAvant @('$GetReleaseScript'))))
        $script:Assets = @((New-Asset 'Nevermind.zip'), (New-Asset 'Nevermind.zip.sig'))
        $r = & $GetReleaseScript @{ Repo = 'essai/essai'; Beta = $false }
        Test-Ok ([bool]$r.Error) 'Contre-épreuve : 1.11.0 ne trouvait pas Nevermind.zip'
    }
    Test-Ok ($maj -match "'Nevermind', 'OptiGame' \| ForEach-Object \{ Join-Path \`$tmp" -and $maj -match "'Nevermind\.ps1', 'OptiGame\.ps1' \| ForEach-Object") 'Mise à jour : le dossier et le script du zip sont cherchés sous les deux noms'

    $sys = [IO.File]::ReadAllText((Join-Path $fichiers 'modules\systeme.ps1'), [Text.Encoding]::UTF8)
    . ([scriptblock]::Create((Get-Definitions $sys @('Get-AppScript'))))
    $AppDir = Join-Path $bac 'appdir'
    New-Item -ItemType Directory -Force -Path $AppDir | Out-Null
    [IO.File]::WriteAllText((Join-Path $AppDir 'OptiGame.ps1'), '#')
    Test-Ok ((Split-Path (Get-AppScript) -Leaf) -eq 'OptiGame.ps1') 'Script principal : OptiGame.ps1 trouvé quand il est seul (zip de compatibilité)'
    [IO.File]::WriteAllText((Join-Path $AppDir 'Nevermind.ps1'), '#')
    Test-Ok ((Split-Path (Get-AppScript) -Leaf) -eq 'Nevermind.ps1') 'Script principal : Nevermind.ps1 d''abord'
    Test-Ok ($sys -match "\(Join-Path \`$here 'Nevermind\.ps1'\),\s+\(Join-Path \`$here 'OptiGame\.ps1'\)") 'Désinstallation : le script est supprimé sous ses deux noms'
}
catch {
    $echecs.Add("Arrêt inattendu : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))")
}
finally {
    try { Exit-Instance } catch {}
    $null = & "$env:SystemRoot\System32\cmd.exe" /c rd /s /q "$bac" 2>&1
}

Write-Host "Mentions de l'ancien nom gardées : $total ligne(s) dans $($maintenant.Count) fichier(s) (liste et raisons dans ce test)."
if ($echecs.Count) {
    Write-Host "$($echecs.Count) échec(s) sur $nbTests contrôles :" -ForegroundColor Red
    $echecs | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
    exit 1
}
Write-Host "Changement de nom : les $nbTests contrôles sont bons." -ForegroundColor Green
exit 0
