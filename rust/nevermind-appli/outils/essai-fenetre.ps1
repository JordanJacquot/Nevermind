# Essai réel de la fenêtre de Nevermind en Rust, avec la CONSTRUCTION D'ESSAI de Mindset.exe (option cargo « essai ») et
# un dossier d'essai sous %TEMP%. À lancer SEULEMENT sur un bureau Windows à part :
#
#   & '.\rust\nevermind-appli\outils\sur-bureau.ps1' -Script '.\rust\nevermind-appli\outils\essai-fenetre.ps1' -Arguments "-Exe '<Mindset.exe d'essai>' -Resultats '<dossier>'" -Secondes 400
#
# Le script refuse un exe qui n'est pas la construction d'essai. La construction d'essai ne lit pas HKLM, n'ouvre rien
# par l'Explorateur, ne lance qu'elle-même, et n'utilise que le dossier d'essai et des verrous d'essai.
#
# Vérifié, dans l'ordre :
#  1. lancée cachée (-Demarrage, copie gérée) : une fenêtre « Tauri Window » titrée exactement « Nevermind », sans
#     propriétaire, cachée ; réglages de WebView2 coupés ; la page passe à « caché » ; l'état envoyé (etat.json) ;
#  2. EBWebView seulement sous <essai>\Donnees\WebView2-Tauri\Nevermind ;
#  3. un second lancement caché quitte sans rien montrer ;
#  4. afficher.demande la montre ; la page revient « à l'écran » ; capture ; mémoire et processeur à l'écran ;
#  5. cachée de nouveau : « caché, en pause », puis « en veille » au bout d'une minute ; mémoire et processeur cachée ;
#  6. quitter.demande : elle quitte en moins de 6 s ;
#  7. relancée, WM_CLOSE (le bouton « Fermer » de Mindset) : elle quitte ;
#  8. relancée, un nouveau thème dans suite.json : elle se relance cachée, qui quitte par quitter.demande ;
#  9. plus aucun processus de l'essai, rien de créé hors de l'essai.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Resultats
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
public static class EssaiFenetreNm
{
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    public static string Classe(IntPtr h) { var s = new StringBuilder(256); GetClassName(h, s, 256); return s.ToString(); }
    public static string Titre(IntPtr h) { var s = new StringBuilder(256); GetWindowText(h, s, 256); return s.ToString(); }
    public static IntPtr Proprietaire(IntPtr h) { return GetWindow(h, 4); }
    public static IntPtr[] Fenetres(int pid)
    {
        var l = new List<IntPtr>();
        EnumWindows(delegate (IntPtr h, IntPtr x) { int p; GetWindowThreadProcessId(h, out p); if (p == pid) l.Add(h); return true; }, IntPtr.Zero);
        return l.ToArray();
    }
}
'@

$Exe = (Resolve-Path -LiteralPath $Exe).Path
$texte = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($Exe))
if (-not $texte.Contains('NEVERMIND_ESSAI_NOM')) { Write-Output "ÉCHEC : $Exe n'est pas la construction d'essai : rien n'est lancé."; exit 1 }
$racine = Join-Path $env:TEMP "nevermind-essai-fenetre-$PID"
if (Test-Path -LiteralPath $racine) { Remove-Item -LiteralPath $racine -Recurse -Force }
$programmes = Join-Path $racine 'Mindset\Programmes\Nevermind'
$donnees = Join-Path $racine 'Donnees\Nevermind'
$local = Join-Path $racine 'Local'
New-Item -ItemType Directory -Force -Path $programmes, $donnees, (Join-Path $local 'Mindset') | Out-Null
$exeEssai = Join-Path $programmes 'Mindset.exe'
Copy-Item -LiteralPath $Exe -Destination $exeEssai
New-Item -ItemType Directory -Force -Path $Resultats | Out-Null
$nomEssai = "fenetre-$PID"
$suite = Join-Path $local 'Mindset\suite.json'
function Write-Suite([string]$Theme) {
    $j = [ordered]@{ theme = $Theme; applis = [ordered]@{ nevermind = $programmes }; modifie = (Get-Date -Format s) }
    [IO.File]::WriteAllText($suite, ($j | ConvertTo-Json -Depth 4), (New-Object Text.UTF8Encoding($false)))
}
Write-Suite 'crepuscule'
$journal = Join-Path $donnees 'journal.txt'
function Get-Journal {
    if (-not (Test-Path -LiteralPath $journal)) { return @() }
    $f = New-Object IO.FileStream($journal, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
    try { $l = (New-Object IO.StreamReader($f, [Text.Encoding]::UTF8)).ReadToEnd() } finally { $f.Dispose() }
    @($l -split "`r`n" | Where-Object { $_ })
}
function Compter([string]$Motif) { @(Get-Journal | Where-Object { $_ -like "*$Motif*" }).Count }
$horsEssai = @((Join-Path $env:LOCALAPPDATA 'Mindset\WebView2-Tauri\Nevermind'), (Join-Path $env:LOCALAPPDATA 'JordanJacquot.Nevermind'), (Join-Path $env:LOCALAPPDATA 'Mindset\Nevermind\ecran-webview'))
$avant = @{}
foreach ($d in $horsEssai) { $avant[$d] = if (Test-Path -LiteralPath $d) { (Get-Item -LiteralPath $d).LastWriteTimeUtc } else { $null } }

function Lancer([string[]]$Arguments) {
    $psi = New-Object Diagnostics.ProcessStartInfo $exeEssai
    $psi.Arguments = ($Arguments -join ' ')
    $psi.UseShellExecute = $false
    $psi.WorkingDirectory = $programmes
    $psi.EnvironmentVariables['NEVERMIND_ESSAI_DONNEES'] = $donnees
    $psi.EnvironmentVariables['NEVERMIND_ESSAI_LOCAL'] = $local
    $psi.EnvironmentVariables['NEVERMIND_ESSAI_NOM'] = $nomEssai
    [Diagnostics.Process]::Start($psi)
}
function Attendre([scriptblock]$Condition, [int]$Ms) {
    $t = [Diagnostics.Stopwatch]::StartNew()
    while ($t.ElapsedMilliseconds -lt $Ms) { if (& $Condition) { return $true }; Start-Sleep -Milliseconds 100 }
    [bool](& $Condition)
}
function Get-Principale([int]$Id) { @([EssaiFenetreNm]::Fenetres($Id) | Where-Object { [EssaiFenetreNm]::Classe($_) -ceq 'Tauri Window' -and [EssaiFenetreNm]::Titre($_) -ceq 'Nevermind' }) }
# Les processus de l'essai : le programme et ses WebView2 (leur ligne de commande donne le dossier d'essai)
function Get-ProcessusEssai([int]$Id) {
    @(Get-CimInstance Win32_Process -Property ProcessId, Name, CommandLine, WorkingSetSize, PrivatePageCount | Where-Object { $_.ProcessId -eq $Id -or ($_.Name -eq 'msedgewebview2.exe' -and $_.CommandLine -like "*$racine*") })
}
# Mémoire (privée et ensemble de travail, en Mo) et processeur (en % d'un cœur) pendant $Secondes
function Mesurer([int]$Id, [int]$Secondes, [string]$Quand) {
    $ps = Get-ProcessusEssai $Id
    $t0 = @{}; foreach ($p in $ps) { try { $t0[[int]$p.ProcessId] = (Get-Process -Id $p.ProcessId).TotalProcessorTime.TotalMilliseconds } catch {} }
    Start-Sleep -Seconds $Secondes
    $cpu = 0.0
    foreach ($p in $ps) { try { $cpu += (Get-Process -Id $p.ProcessId).TotalProcessorTime.TotalMilliseconds - $t0[[int]$p.ProcessId] } catch {} }
    $ps = Get-ProcessusEssai $Id
    $prive = [math]::Round((($ps | Measure-Object PrivatePageCount -Sum).Sum) / 1MB, 1)
    $ws = [math]::Round((($ps | Measure-Object WorkingSetSize -Sum).Sum) / 1MB, 1)
    $l = "MESURE $Quand : $($ps.Count) processus (dont $(@($ps | Where-Object Name -eq 'msedgewebview2.exe').Count) WebView2), mémoire privée $prive Mo, ensemble de travail $ws Mo, processeur $([math]::Round($cpu / ($Secondes * 10), 2)) % d'un cœur sur $Secondes s"
    Write-Output $l
    Add-Content -LiteralPath (Join-Path $Resultats 'mesures.txt') -Value $l -Encoding UTF8
}
$tousLesPid = New-Object System.Collections.Generic.List[int]

# --- 1. Lancée cachée ---
$p1 = Lancer @('-Demarrage'); $tousLesPid.Add($p1.Id)
Verifier (Attendre { (Get-Principale $p1.Id).Count -ge 1 } 20000) 'une fenêtre « Tauri Window » titrée exactement « Nevermind »'
$f = (Get-Principale $p1.Id) | Select-Object -First 1
if ($f) {
    Verifier ((Get-Principale $p1.Id).Count -eq 1) 'une seule fenêtre principale'
    Verifier ([EssaiFenetreNm]::Proprietaire($f) -eq [IntPtr]::Zero) 'sans propriétaire'
    Verifier (-not [EssaiFenetreNm]::IsWindowVisible($f)) 'cachée au départ (-Demarrage)'
}
Verifier (Attendre { (Compter 'outils de développement et zoom coupés') -ge 1 } 15000) 'réglages de WebView2 coupés et relus (journal)'
Verifier (Attendre { (Compter 'Écran : caché, en pause') -ge 1 } 15000) 'la page cachée passe à « caché » (journal)'
Verifier (Attendre { Test-Path -LiteralPath (Join-Path $donnees 'etat.json') } 15000) "l'état envoyé au kit (etat.json)"
if (Test-Path -LiteralPath (Join-Path $donnees 'etat.json')) {
    $etat = Get-Content -LiteralPath (Join-Path $donnees 'etat.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    Verifier ($etat.app.nom -eq 'Nevermind' -and @($etat.accueil.tuiles).Count -eq 7) "l'accueil : 7 tuiles"
    # (etat.json est la clé de comparaison de l'état, comme $env:ESSAI_DOSSIER : sans le thème, ajouté à l'envoi)
    Verifier ((Compter 'Thème de la suite (Mindset) : crepuscule') -ge 1) "le thème de la suite (crepuscule)"
}

# --- 2. EBWebView ---
$attendu = Join-Path $racine 'Donnees\WebView2-Tauri\Nevermind\EBWebView'
Verifier (Attendre { Test-Path -LiteralPath $attendu } 10000) "EBWebView créé sous $attendu"
$autres = @(Get-ChildItem -LiteralPath $racine -Recurse -Directory -Filter 'EBWebView' -ErrorAction SilentlyContinue | Where-Object { $_.FullName -ne $attendu })
Verifier ($autres.Count -eq 0) "aucun autre EBWebView dans l'essai"

# --- 3. Second lancement caché ---
$p2 = Lancer @('-Demarrage'); $tousLesPid.Add($p2.Id)
Verifier ($p2.WaitForExit(12000)) 'le second lancement caché quitte (verrou tenu, attente de 5 s)'
Verifier ((Get-Principale $p2.Id).Count -eq 0) 'le second lancement ne montre rien'
Verifier ((Compter 'Lancement caché abandonné') -ge 1) 'noté au journal'
Verifier (-not $p1.HasExited) 'la première copie tourne toujours'

# --- 4. afficher.demande la montre ---
Mesurer $p1.Id 15 'cachée (avant la veille)'
$vus = Compter "Écran : de nouveau à l'écran"
[IO.File]::WriteAllText((Join-Path $donnees 'afficher.demande'), "$PID")
if ($f) { Verifier (Attendre { [EssaiFenetreNm]::IsWindowVisible($f) } 5000) "afficher.demande la montre" }
Verifier (Attendre { (Compter "Écran : de nouveau à l'écran") -gt $vus } 8000) "la page revient « à l'écran » (journal)"
Verifier (-not (Test-Path -LiteralPath (Join-Path $donnees 'afficher.demande'))) 'afficher.demande effacé'
Start-Sleep -Milliseconds 3000
$capture = Join-Path $donnees 'capture.png'
[IO.File]::WriteAllText((Join-Path $donnees 'capture.demande'), '1')
Verifier (Attendre { Test-Path -LiteralPath $capture } 5000) 'capture faite par la construction d''essai'
if (Test-Path -LiteralPath $capture) { Start-Sleep -Milliseconds 300; Copy-Item -LiteralPath $capture -Destination (Join-Path $Resultats 'capture-accueil.png') -Force }
# Un geste : la page Paramètres (aller), puis une capture
[IO.File]::WriteAllText((Join-Path $donnees 'actions.demande'), '{"kit":"aller","ecran":"parametres"}')
Start-Sleep -Milliseconds 1500
Mesurer $p1.Id 15 'à l''écran'

# --- 5. Cachée de nouveau ---
$caches = Compter 'Écran : caché, en pause'
if ($f) { [void][EssaiFenetreNm]::ShowWindow($f, 0) }
Verifier (Attendre { (Compter 'Écran : caché, en pause') -gt $caches } 8000) 'cachée de nouveau : la page repasse à « caché »'
Verifier (Attendre { (Compter 'Écran : en veille') -ge 1 } 75000) 'une minute cachée : la page en veille profonde'
Start-Sleep -Seconds 3
Mesurer $p1.Id 20 'cachée, en veille'

# --- 6. quitter.demande ---
$t = [Diagnostics.Stopwatch]::StartNew()
[IO.File]::WriteAllText((Join-Path $donnees 'quitter.demande'), "$PID")
Verifier ($p1.WaitForExit(6000)) "quitter.demande : elle quitte en moins de 6 s ($($t.ElapsedMilliseconds) ms)"
Verifier (-not (Test-Path -LiteralPath (Join-Path $donnees 'quitter.demande'))) 'quitter.demande effacé'

# --- 7. WM_CLOSE ---
$p3 = Lancer @('-Demarrage'); $tousLesPid.Add($p3.Id)
if (Attendre { (Get-Principale $p3.Id).Count -ge 1 } 20000) {
    Start-Sleep -Milliseconds 1500
    $f3 = (Get-Principale $p3.Id) | Select-Object -First 1
    [void][EssaiFenetreNm]::PostMessage($f3, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)
    Verifier ($p3.WaitForExit(6000)) 'WM_CLOSE : elle quitte'
} else { Echec 'relancée : pas de fenêtre' }

# --- 8. Nouveau thème : relance cachée ---
$p4 = Lancer @('-Demarrage'); $tousLesPid.Add($p4.Id)
if (Attendre { (Get-Principale $p4.Id).Count -ge 1 } 20000) {
    Start-Sleep -Milliseconds 3500
    Write-Suite 'rubis'
    Verifier ($p4.WaitForExit(9000)) 'nouveau thème : la copie quitte pour se relancer'
    $relance = $null
    [void](Attendre {
        $script:relance = @(Get-CimInstance Win32_Process -Filter "Name = 'Mindset.exe'" | Where-Object { $_.ExecutablePath -eq $exeEssai -and -not $tousLesPid.Contains([int]$_.ProcessId) }) | Select-Object -First 1
        $null -ne $script:relance } 10000)
    Verifier ($null -ne $relance) 'une nouvelle copie a été lancée'
    if ($relance) {
        $tousLesPid.Add([int]$relance.ProcessId)
        Verifier ($relance.CommandLine -match '-Demarrage') "relancée cachée"
        Verifier ((Compter 'Thème de la suite changé dans Mindset : rubis') -ge 1) 'relance notée au journal'
        $param = Get-Content -LiteralPath (Join-Path $donnees 'parametres.json') -Raw | ConvertFrom-Json
        Verifier ($param.Theme -eq 'rubis') 'le thème enregistré (parametres.json)'
        Start-Sleep -Milliseconds 3000
        [IO.File]::WriteAllText((Join-Path $donnees 'quitter.demande'), "$PID")
        $pr = Get-Process -Id ([int]$relance.ProcessId) -ErrorAction SilentlyContinue
        Verifier ((-not $pr) -or $pr.WaitForExit(6000)) 'la copie relancée quitte par quitter.demande'
    }
} else { Echec 'relancée pour le thème : pas de fenêtre' }

# --- 9. Plus rien ---
Start-Sleep -Milliseconds 2000
$restants = @(Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $exeEssai })
$restants += @(Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" | Where-Object { $_.CommandLine -like "*$racine*" })
Verifier ($restants.Count -eq 0) "plus aucun processus de l'essai"
foreach ($r in $restants) { try { Stop-Process -Id $r.ProcessId -Force -ErrorAction Stop } catch {} }
foreach ($d in $horsEssai) {
    $maintenant = if (Test-Path -LiteralPath $d) { (Get-Item -LiteralPath $d).LastWriteTimeUtc } else { $null }
    Verifier ($maintenant -eq $avant[$d]) "rien de créé ni changé hors de l'essai : $d"
}
Get-Journal | Set-Content -LiteralPath (Join-Path $Resultats 'journal-essai.txt') -Encoding UTF8
Write-Output '--- Journal de l''essai ---'
Get-Journal | ForEach-Object { Write-Output "  $_" }
Start-Sleep -Milliseconds 1000
try { Remove-Item -LiteralPath $racine -Recurse -Force -ErrorAction Stop } catch { Write-Output "Dossier d'essai pas encore effacé : $racine" }
if ($echecs.Count) { Write-Output "$($echecs.Count) échec(s)."; exit 1 }
Write-Output 'Tout est bon.'
exit 0
