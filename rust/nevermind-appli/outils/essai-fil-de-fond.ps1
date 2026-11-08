# Audit G3, sur la vraie fenêtre de la construction d'essai : « Appliquer » les optimisations avec une fausse Aide
# dont le point de restauration met 40 s (NEVERMIND_ESSAI_AIDE_LENTE). Nevermind doit rester vivant pendant ce temps
# (le geste rendu tout de suite, les pages qui changent), puis quitter vite même pendant un geste long.
# À lancer SEULEMENT sur un bureau à part (sur-bureau.ps1). Rien ne touche le vrai PC : dossier d'essai, fausse Aide.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Resultats,
    [int]$Lente = 40
)
$ErrorActionPreference = 'Stop'
$echecs = New-Object System.Collections.Generic.List[string]
function Ok([string]$t) { Write-Output "OK : $t" }
function Echec([string]$t) { Write-Output "ÉCHEC : $t"; $echecs.Add($t) }
function Verifier([bool]$c, [string]$t) { if ($c) { Ok $t } else { Echec $t } }

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class FondNm
{
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    public static string Classe(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
    public static string Titre(IntPtr h) { var s = new StringBuilder(256); GetWindowText(h, s, 256); return s.ToString(); }
    public static IntPtr[] Fenetres(int pid)
    {
        var l = new List<IntPtr>();
        EnumWindows(delegate (IntPtr h, IntPtr x) { int p; GetWindowThreadProcessId(h, out p); if (p == pid) l.Add(h); return true; }, IntPtr.Zero);
        return l.ToArray();
    }
}
'@

$texte = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($Exe))
if (-not $texte.Contains('NEVERMIND_ESSAI_AIDE_LENTE')) { Write-Output "ÉCHEC : $Exe n'est pas la construction d'essai de G3 : rien n'est lancé."; exit 1 }
New-Item -ItemType Directory -Force -Path $Resultats | Out-Null
$racine = Join-Path $Resultats 'essai-appli'
if (Test-Path -LiteralPath $racine) { Remove-Item -LiteralPath $racine -Recurse -Force }
$programmes = Join-Path $racine 'Mindset\Programmes\Nevermind'
$donnees = Join-Path $racine 'Donnees\Nevermind'
$local = Join-Path $racine 'Local'
New-Item -ItemType Directory -Force -Path $programmes, $donnees, (Join-Path $local 'Mindset') | Out-Null
$exeEssai = Join-Path $programmes 'Mindset.exe'
Copy-Item -LiteralPath $Exe -Destination $exeEssai
$sans = New-Object Text.UTF8Encoding($false)
$j = [ordered]@{ theme = 'neon'; applis = [ordered]@{ nevermind = $programmes }; modifie = (Get-Date -Format s) }
[IO.File]::WriteAllText((Join-Path $local 'Mindset\suite.json'), ($j | ConvertTo-Json -Depth 4), $sans)
$journal = Join-Path $donnees 'journal.txt'

function Attendre([scriptblock]$Condition, [int]$Ms) {
    $t = [Diagnostics.Stopwatch]::StartNew()
    while ($t.ElapsedMilliseconds -lt $Ms) { if (& $Condition) { return $true }; Start-Sleep -Milliseconds 100 }
    [bool](& $Condition)
}
function Principale([int]$Id) { @([FondNm]::Fenetres($Id) | Where-Object { [FondNm]::Classe($_) -ceq 'Tauri Window' -and [FondNm]::Titre($_) -ceq 'Nevermind' }) | Select-Object -First 1 }
function Demande([string]$Lignes) { [IO.File]::WriteAllText((Join-Path $donnees 'actions.demande'), $Lignes, $sans) }
function Lignes { if (Test-Path -LiteralPath $journal) { @(Get-Content -LiteralPath $journal -Encoding UTF8) } else { @() } }
function AuJournal([string]$Motif) { [bool](@(Lignes | Where-Object { $_ -like $Motif }).Count) }
function Capture([string]$Nom) {
    $c = Join-Path $donnees 'capture.png'
    if (Test-Path -LiteralPath $c) { Remove-Item -LiteralPath $c -Force }
    [IO.File]::WriteAllText((Join-Path $donnees 'capture.demande'), '1')
    if (Attendre { Test-Path -LiteralPath $c } 6000) { Start-Sleep -Milliseconds 300; Copy-Item -LiteralPath $c -Destination (Join-Path $Resultats "$Nom.png") -Force; Ok "capture $Nom" }
    else { Echec "capture $Nom" }
}
function Lancer {
    $psi = New-Object Diagnostics.ProcessStartInfo $exeEssai
    $psi.Arguments = '-Demarrage'
    $psi.UseShellExecute = $false
    $psi.WorkingDirectory = $programmes
    $psi.EnvironmentVariables['NEVERMIND_ESSAI_DONNEES'] = $donnees
    $psi.EnvironmentVariables['NEVERMIND_ESSAI_LOCAL'] = $local
    $psi.EnvironmentVariables['NEVERMIND_ESSAI_NOM'] = "fond-$PID"
    $psi.EnvironmentVariables['NEVERMIND_ESSAI_AIDE_LENTE'] = "$Lente"
    [Diagnostics.Process]::Start($psi)
}
# Cocher les optimisations conseillées (et les autres), puis « Appliquer » ; rend le moment du clic
function Appliquer {
    $l = @('{"type":"action","id":"opti:recommande","ecran":"optimisation"}')
    foreach ($k in 0..40) { $l += "{`"type`":`"action`",`"id`":`"tweak`",`"valeur`":{`"valeur`":`"$k`",`"oui`":true},`"ecran`":`"optimisation`"}" }
    Demande ($l -join "`n")
    Start-Sleep -Seconds 2
    $n = @(Lignes).Count
    Demande '{"type":"action","id":"clic:BtnApply","ecran":"optimisation"}'
    $n
}

$p = Lancer
try {
    Verifier (Attendre { $null -ne (Principale $p.Id) } 30000) 'fenêtre de Nevermind (construction d''essai)'
    Start-Sleep -Seconds 3
    [IO.File]::WriteAllText((Join-Path $donnees 'afficher.demande'), "$PID")
    $f = Principale $p.Id
    Verifier (Attendre { [FondNm]::IsWindowVisible($f) } 8000) 'montrée'
    Verifier (AuJournal "*Essai : fausse Aide, point de restauration en $Lente s*") 'la fausse Aide lente est en place'
    # (la lecture du PC, seulement lue, et l'analyse : quelques secondes)
    Start-Sleep -Seconds 20
    Demande '{"kit":"aller","ecran":"optimisation"}'
    Start-Sleep -Seconds 3
    Capture 'optimisation-avant'
    Copy-Item -LiteralPath (Join-Path $donnees 'etat.json') -Destination (Join-Path $Resultats 'etat-avant.json') -Force

    # 1. « Appliquer » : le geste rendu tout de suite, le texte d'attente montré
    $avant = Appliquer
    $chrono = [Diagnostics.Stopwatch]::StartNew()
    Verifier (Attendre { AuJournal '*Geste : clic:BtnApply traité en *' } 5000) "« Appliquer » rendu en moins de 5 s (il fallait $Lente s avant)"
    $ligne = @(Lignes | Where-Object { $_ -like '*Geste : clic:BtnApply traité en *' }) | Select-Object -Last 1
    Write-Output "    $ligne (vu après $($chrono.ElapsedMilliseconds) ms)"
    Start-Sleep -Milliseconds 800
    Capture 'pendant-attente'
    # 2. Pendant le point de restauration : les pages changent, chaque geste est traité tout de suite
    foreach ($x in @(@('page:0', 'optimisation'), @('page:9', 'tableau'))) {
        $c = [Diagnostics.Stopwatch]::StartNew()
        Demande "{`"type`":`"action`",`"id`":`"$($x[0])`",`"ecran`":`"$($x[1])`"}"
        Verifier (Attendre { AuJournal "*Geste : $($x[0]) traité en *" } 3000) "pendant l'attente, le geste $($x[0]) est traité ($($c.ElapsedMilliseconds) ms)"
        Start-Sleep -Seconds 2
    }
    Capture 'pendant-autre-page'
    Demande '{"kit":"aller","ecran":"optimisation"}'
    Verifier ($chrono.Elapsed.TotalSeconds -lt $Lente) "tout cela pendant le point de restauration ($([int]$chrono.Elapsed.TotalSeconds) s sur $Lente)"
    # 3. La fin du geste (le point de restauration « créé » par la fausse Aide)
    while ($chrono.Elapsed.TotalSeconds -lt ($Lente + 8)) { Start-Sleep -Milliseconds 500 }
    Capture 'apres'
    Copy-Item -LiteralPath (Join-Path $donnees 'etat.json') -Destination (Join-Path $Resultats 'etat-apres.json') -Force
    # 4. Un autre « Appliquer », puis quitter pendant le point de restauration : moins de 6 s
    $null = Appliquer
    Verifier (Attendre { @(Lignes | Where-Object { $_ -like '*Geste : clic:BtnApply traité en *' }).Count -ge 2 } 5000) 'deuxième « Appliquer » rendu tout de suite'
    Start-Sleep -Seconds 2
    $q = [Diagnostics.Stopwatch]::StartNew()
    [IO.File]::WriteAllText((Join-Path $donnees 'quitter.demande'), "$PID")
    $fini = $p.WaitForExit(15000)
    Verifier ($fini -and $q.Elapsed.TotalSeconds -lt 6) "quitter pendant un geste long : $([math]::Round($q.Elapsed.TotalSeconds, 1)) s (moins de 6 s)"
    Copy-Item -LiteralPath $journal -Destination (Join-Path $Resultats 'journal-essai.txt') -Force
} catch {
    Echec "erreur du script : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))"
} finally {
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force; Write-Output "Arrêté : Nevermind d'essai $($p.Id)" }
}
Write-Output "=== Fin : $($echecs.Count) échec(s)"
if ($echecs.Count) { exit 1 }
exit 0
