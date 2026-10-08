# Teste Nexo automatiquement : lance une copie de l'app hors de l'écran, avec des données
# à part (_test\donnees), parcourt toutes les pages et vérifie qu'il n'y a aucune erreur.
# Rien n'est modifié sur le PC : les boîtes de confirmation répondent toujours « Non ».
#
#   .\outils\tester.ps1              test rapide (toutes les pages)
#   .\outils\tester.ps1 -Complet     + scan du réseau, fiche d'un appareil et audit de sécurité
#   .\outils\tester.ps1 -Captures    + captures d'écran dans _test\captures
#
# Code de sortie : 0 si tout est bon, 1 sinon.

param([switch]$Complet, [switch]$Captures, [int]$Delai = 300)
$ErrorActionPreference = 'Stop'
$racine = Split-Path $PSScriptRoot -Parent
$source = Join-Path $racine 'OptiGame\fichiers'
$test = Join-Path $racine '_test'
$app = Join-Path $test 'app'
$donnees = Join-Path $test 'donnees'
$utf8 = New-Object Text.UTF8Encoding($true)

# Copie propre de l'app et des données de test
foreach ($d in $app, $donnees, (Join-Path $test 'captures')) {
    if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force }
}
New-Item -ItemType Directory -Force -Path $donnees, (Join-Path $test 'captures') | Out-Null
Copy-Item -LiteralPath $source -Destination (Join-Path $app 'fichiers') -Recurse
$vrai = Join-Path $env:LOCALAPPDATA 'OptiGame'
foreach ($f in 'fabricants.txt', 'appareils.json') {
    if (Test-Path -LiteralPath (Join-Path $vrai $f)) { Copy-Item -LiteralPath (Join-Path $vrai $f) -Destination $donnees }
}
Remove-Item -LiteralPath (Join-Path $test 'resultats.json') -ErrorAction SilentlyContinue

function Edit-File([string]$Path, [string]$Old, [string]$New, [switch]$Regex) {
    $c = [IO.File]::ReadAllText($Path, [Text.Encoding]::UTF8)
    $n = if ($Regex) { [regex]::Matches($c, $Old).Count } else { ([regex]::Matches($c, [regex]::Escape($Old))).Count }
    if ($n -ne 1) { throw "Test : repère introuvable dans $(Split-Path $Path -Leaf) ($Old)" }
    $c = if ($Regex) { [regex]::Replace($c, $Old, $New) } else { $c.Replace($Old, $New) }
    [IO.File]::WriteAllText($Path, $c, $utf8)
}
$main = Join-Path $app 'fichiers\OptiGame.ps1'
# Pas de demande de droits admin (le test est lancé depuis une console déjà admin ou non)
Edit-File $main '(?s)if \(-not \$principal\.IsInRole.*?\n\}\r?\n' '' -Regex
# Données à part
Edit-File (Join-Path $app 'fichiers\modules\donnees.ps1') "`$DataDir    = Join-Path `$env:LOCALAPPDATA 'OptiGame'" "`$DataDir    = '$donnees'"
# Code de test à la place de l'ouverture normale de la fenêtre
$code = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'test-app.ps1'), [Text.Encoding]::UTF8) -replace "`r?`n", "`r`n"
$code = $code.Replace('__TEST__', $test).Replace('$__COMPLET__', $(if ($Complet) { '$true' } else { '$false' })).Replace('$__CAPTURES__', $(if ($Captures) { '$true' } else { '$false' }))
Edit-File $main "`$Window.Show()`r`n[System.Windows.Threading.Dispatcher]::Run()`r`n" "$code`r`n"

Write-Host "Test de Nexo$(if ($Complet) { ' (complet)' }) en cours, patiente..."
$env:OPTIGAME_TEST = '1'   # pas d'écran de chargement pendant le test
$err = Join-Path $test 'erreurs.txt'
$p = Start-Process powershell.exe -ArgumentList '-NoProfile', '-STA', '-ExecutionPolicy', 'Bypass', '-File', "`"$main`"" -PassThru -WindowStyle Hidden -RedirectStandardError $err
if (-not $p.WaitForExit($Delai * 1000)) { $p.Kill(); Write-Host "ÉCHEC : l'app ne s'est pas terminée en $Delai s." -ForegroundColor Red; exit 1 }

$ok = $true
$json = Join-Path $test 'resultats.json'
if (-not (Test-Path -LiteralPath $json)) {
    Write-Host 'ÉCHEC : aucun résultat, l''app a planté avant la fin du test.' -ForegroundColor Red
    $ok = $false
} else {
    $r = ConvertFrom-Json ([IO.File]::ReadAllText($json, [Text.Encoding]::UTF8))
    $res = @($r.Resultats)
    foreach ($x in $res) {
        $col = if ($x.Ok) { 'Green' } else { 'Red' }
        Write-Host ('{0,-6} {1,-36} {2}' -f $(if ($x.Ok) { 'OK' } else { 'ÉCHEC' }), $x.Test, $x.Detail) -ForegroundColor $col
        if (-not $x.Ok) { $ok = $false }
    }
    if (@($r.Messages).Count) { Write-Host "Messages affichés pendant le test : $(@($r.Messages).Count)" -ForegroundColor DarkGray }
}
$stderr = if (Test-Path -LiteralPath $err) { (Get-Content -LiteralPath $err -Raw) } else { '' }
if ($stderr -and $stderr.Trim()) {
    Write-Host 'Erreurs PowerShell :' -ForegroundColor Red
    Write-Host ($stderr.Trim() -split "`r?`n" | Select-Object -First 15 | Out-String)
    $ok = $false
}
if ($Captures) { Write-Host "Captures : $(Join-Path $test 'captures')" -ForegroundColor DarkGray }
if ($ok) { Write-Host 'Tout est bon.' -ForegroundColor Green; exit 0 }
Write-Host 'Des tests ont échoué.' -ForegroundColor Red
exit 1
