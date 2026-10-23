# Teste Nevermind automatiquement : lance une copie de l'app hors de l'écran, avec des données
# à part (_test\donnees), parcourt toutes les pages et vérifie qu'il n'y a aucune erreur.
# Rien n'est modifié sur le PC : les boîtes de confirmation répondent toujours « Non ».
#
#   .\outils\tester.ps1              test rapide (toutes les pages)
#   .\outils\tester.ps1 -Complet     + scan du réseau, fiche d'un appareil et audit de sécurité
#   .\outils\tester.ps1 -Captures    + captures d'écran dans _test\captures
#   .\outils\tester.ps1 -Hote        l'app tourne dans Mindset.exe (moteur PowerShell hébergé), lancé par le relais Nevermind.exe,
#                                    comme chez les utilisateurs
#
# Code de sortie : 0 si tout est bon, 1 sinon.

param([switch]$Complet, [switch]$Captures, [switch]$Hote, [int]$Delai = 300)
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
$vrai = @((Join-Path $env:LOCALAPPDATA 'Mindset\Nevermind'), (Join-Path $env:LOCALAPPDATA 'OptiGame') | Where-Object { Test-Path -LiteralPath $_ })[0]
if (-not $vrai) { $vrai = Join-Path $env:LOCALAPPDATA 'OptiGame' }
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
# (Nevermind ne demande plus les droits administrateur, sauf pour se désinstaller : rien à retirer pour le test)
# Données à part
Edit-File $main "`$DataDir = Resolve-DataDir" "`$DataDir = '$donnees'"
# Code de test à la place de l'ouverture normale de la fenêtre
$code = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'test-app.ps1'), [Text.Encoding]::UTF8) -replace "`r?`n", "`r`n"
$code = $code.Replace('__TEST__', $test).Replace('$__COMPLET__', $(if ($Complet) { '$true' } else { '$false' })).Replace('$__CAPTURES__', $(if ($Captures) { '$true' } else { '$false' }))
Edit-File $main "`$Window.Show()`r`n[System.Windows.Threading.Dispatcher]::Run()`r`n" "$code`r`n"

Write-Host "Test de Nevermind$(if ($Complet) { ' (complet)' }) en cours, patiente..."
$env:OPTIGAME_TEST = '1'   # pas d'écran de chargement pendant le test
# Registre : les réglages d'essai du test vont dans une clé à part (HKCU\Software\MindsetTest-<numéro>), effacée à la fin.
# La vraie clé des valeurs d'origine de Nevermind est relue avant et après (lecture seule) : elle ne doit pas changer.
$cleTest = "Software\MindsetTest-$([guid]::NewGuid().ToString('N').Substring(0, 12))"
$env:OPTIGAME_TEST_REG = $cleTest
$vraieOrigines = 'Software\Mindset\Nevermind\Origines'
function Get-ValeursCle([string]$Cle) {
    $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($Cle, $false)
    if (-not $k) { return '(absente)' }
    try { (@($k.GetValueNames() | Sort-Object | ForEach-Object { "$_=$($k.GetValue($_, $null, 'DoNotExpandEnvironmentNames'))" }) -join "`n") } finally { $k.Close() }
}
$originesAvant = Get-ValeursCle $vraieOrigines
function Remove-CleTest { try { [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($cleTest, $false) } catch {} }
$err = Join-Path $test 'erreurs.txt'
if ($Hote) {
    # Même lanceur que chez les utilisateurs, sans la demande de droits administrateur (test lancé tel quel)
    $manif = Join-Path $test 'lanceur-test.manifest'
    [IO.File]::WriteAllText($manif, ([IO.File]::ReadAllText((Join-Path $PSScriptRoot 'lanceur.manifest'), [Text.Encoding]::UTF8) -replace 'requireAdministrator', 'asInvoker'), $utf8)
    $sma = @(Get-ChildItem (Join-Path $env:windir 'Microsoft.NET\assembly\GAC_MSIL\System.Management.Automation') -Recurse -Filter 'System.Management.Automation.dll' | Select-Object -First 1)[0].FullName
    # Mindset.exe (le programme qui fait tourner Nevermind) et Nevermind.exe (le relais qui le lance, puis se ferme)
    $exe = Join-Path $app 'Mindset.exe'
    $relais = Join-Path $app 'Nevermind.exe'
    foreach ($x in @(@($exe, ''), @($relais, '/define:RELAIS'))) {
        $opts = @('/nologo', '/target:winexe', '/platform:anycpu', "/win32manifest:$manif", '/reference:System.Windows.Forms.dll', '/reference:System.Web.Extensions.dll', "/reference:$sma", "/out:$($x[0])")
        if ($x[1]) { $opts += $x[1] }
        & (Join-Path $env:windir 'Microsoft.NET\Framework64\v4.0.30319\csc.exe') @opts (Join-Path $PSScriptRoot 'lanceur.cs') | Out-Null
        if (-not (Test-Path -LiteralPath $x[0])) { Write-Host "ÉCHEC : le lanceur ne compile pas ($(Split-Path $x[0] -Leaf))." -ForegroundColor Red; exit 1 }
    }
    # Lancé par le relais, comme par un ancien raccourci : le relais doit se fermer tout de suite, et le programme qui
    # tourne doit être Mindset.exe de CE dossier (le Gestionnaire des tâches range toute la suite sous « Mindset »)
    $r = Start-Process $relais -PassThru
    $null = $r.Handle   # (sans lui, le code de sortie peut rester illisible)
    if (-not $r.WaitForExit(10000)) { $r.Kill(); Write-Host 'ÉCHEC : le relais Nevermind.exe ne s''est pas fermé en 10 s.' -ForegroundColor Red; exit 1 }
    if ($r.ExitCode) { Write-Host "ÉCHEC : le relais Nevermind.exe a rendu le code $($r.ExitCode)." -ForegroundColor Red; exit 1 }
    $enfant = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($r.Id)" -ErrorAction SilentlyContinue | Where-Object { $_.ExecutablePath -eq $exe })[0]
    if (-not $enfant) { Write-Host 'ÉCHEC : le relais n''a pas lancé Mindset.exe de son dossier.' -ForegroundColor Red; exit 1 }
    $p = Get-Process -Id $enfant.ProcessId
    $script:ProcessusVus = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith("$app\", [StringComparison]::OrdinalIgnoreCase) } | ForEach-Object { "$($_.Name) ($($_.ExecutablePath))" })
    Write-Host "Relais fermé (code 0) ; programmes du dossier de test en cours : $($script:ProcessusVus -join ', ')" -ForegroundColor DarkGray
} else {
    $p = Start-Process powershell.exe -ArgumentList '-NoProfile', '-STA', '-ExecutionPolicy', 'Bypass', '-File', "`"$main`"" -PassThru -WindowStyle Hidden -RedirectStandardError $err
}
$json = Join-Path $test 'resultats.json'
# Le processus doit disparaître tout seul peu après la fermeture de la fenêtre (résultats écrits juste avant) : un
# Mindset.exe resté en vie sans fenêtre garde le verrou, et Mindset ne peut plus le rouvrir
$fin = (Get-Date).AddSeconds($Delai)
while (-not $p.HasExited -and (Get-Date) -lt $fin -and -not (Test-Path -LiteralPath $json)) { Start-Sleep -Milliseconds 250 }
$resteEnVie = $false
if (-not $p.HasExited -and (Test-Path -LiteralPath $json)) {
    if (-not $p.WaitForExit(10000)) { $resteEnVie = $true; $p.Kill() }
}
if (-not $p.HasExited -and -not $resteEnVie -and -not $p.WaitForExit([math]::Max(0, [int]($fin - (Get-Date)).TotalMilliseconds))) { $p.Kill(); Remove-CleTest; Write-Host "ÉCHEC : l'app ne s'est pas terminée en $Delai s." -ForegroundColor Red; exit 1 }

$ok = $true
# Registre : la vraie clé des valeurs d'origine n'a pas bougé, et la clé du test est effacée
$originesApres = Get-ValeursCle $vraieOrigines
Remove-CleTest
if ($originesApres -cne $originesAvant) { Write-Host "ÉCHEC  Registre du PC                          la clé HKCU\$vraieOrigines a changé pendant le test" -ForegroundColor Red; $ok = $false }
else { Write-Host 'OK     Registre du PC                          valeurs d''origine de Nevermind inchangées' -ForegroundColor Green }
if ($resteEnVie) { Write-Host 'ÉCHEC  Sortie du processus                 toujours en vie 10 s après la fermeture de la fenêtre (arrêté de force)' -ForegroundColor Red; $ok = $false }
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
