# Test de la fluidité de Nevermind pendant une partie (lot 4, bugs 2, 9 et 10), sans droits et sans toucher au PC.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-fluidite.ps1
#   (option -FichierAide <chemin de NevermindAide.ps1> : contre-épreuve sur une autre version de l'Aide)
#
# Ce script vérifie :
#   1. Souris (bug 2) : l'écoute des boutons de la souris de l'organiseur tourne sur un fil à elle, jamais sur celui qui
#      la lance (le fil de la fenêtre), reste en place quand ce fil est occupé ou fini, et s'arrête proprement. Aucun clic
#      ni mouvement n'est simulé : aucun raccourci n'est déclaré, tout passe au PC comme d'habitude.
#      Fin d'une mesure des FPS par l'Aide : « fps.arreter » part en arrière plan, la fenêtre ne l'attend plus (une Aide
#      muette, au nom inventé, met 2 s à ne pas répondre : Stop doit rendre la main tout de suite).
#   2. Aide (bug 9) : une vraie Aide, lancée sans droits pour ce test seulement, reçoit 40 connexions qui ne disent rien
#      (bien plus que les canaux qu'elle a) ; elle ne doit pas s'arrêter, puis répondre de nouveau une fois libérée.
#   3. Minuteurs (bug 10) : une minuterie dont le passage attend un travail en arrière plan (comme Trafic et Lag avec une
#      Aide lente) ne se relance plus dans elle même (Invoke-SansEmpiler), et le réveil de l'Aide n'est pas demandé deux
#      fois en même temps (Test-MesuresAide).
# Code de sortie : 0 si tout passe, 1 sinon.
param([string]$FichierAide)

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$app     = Join-Path $racine 'OptiGame\fichiers'
$modules = Join-Path $app 'modules'
if (-not $FichierAide) { $FichierAide = Join-Path $app 'NevermindAide.ps1' }
$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0
function Test-Ok($Condition, [string]$Message) {
    $script:nbTests++
    if (-not [bool]$Condition) { $script:echecs.Add($Message) }
}

# Une fonction d'un fichier de l'appli, reprise seule (sans charger le reste)
function Get-Fonction([string]$Fichier, [string]$Nom) {
    $ast = [System.Management.Automation.Language.Parser]::ParseFile($Fichier, [ref]$null, [ref]$null)
    $f = $ast.Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $true)
    if (-not $f) { throw "$Nom introuvable dans $Fichier" }
    $f.Extent.Text
}

Add-Type -AssemblyName WindowsBase
Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $modules 'natif.cs'), [Text.Encoding]::UTF8))
# Nom de canal inventé : ce test ne parle jamais à la vraie Aide de Jordan
[AideClient]::Suffixe = "test-fluidite-$PID-$([Guid]::NewGuid().ToString('N').Substring(0, 8))"

# ---------------------------------------------------------------------------
# 1. Souris : l'écoute a son propre fil
# ---------------------------------------------------------------------------
$moi = [AppDomain]::GetCurrentThreadId()
$fil = 0
try {
    $ok = [MouseHook]::Start([IntPtr]::Zero)
    Test-Ok $ok 'Souris : écoute refusée par Windows'
    $fil = [int][MouseHook]::ThreadId
    Test-Ok ($fil -ne 0 -and $fil -ne $moi) "Souris : l'écoute tourne sur le fil qui l'a lancée ($fil, fenêtre $moi)"
    # Le fil qui l'a lancée est occupé (comme la fenêtre pendant une reconstruction d'écran) : l'écoute reste là
    Start-Sleep -Milliseconds 600
    Test-Ok ([MouseHook]::Running -and [MouseHook]::ThreadId -eq $fil) 'Souris : écoute perdue pendant que la fenêtre est occupée'
    Test-Ok ([MouseHook]::Start([IntPtr]::Zero) -and [MouseHook]::ThreadId -eq $fil) 'Souris : un deuxième Start relance une autre écoute'
} catch { Test-Ok $false "Souris : $($_.Exception.Message)" }
finally { try { [MouseHook]::Stop() } catch {} }
Test-Ok (-not [MouseHook]::Running -and [MouseHook]::ThreadId -eq 0) 'Souris : écoute toujours active après Stop'
$filVivant = @([Diagnostics.Process]::GetCurrentProcess().Threads | Where-Object { $_.Id -eq $fil }).Count
Test-Ok ($fil -and -not $filVivant) "Souris : le fil de l'écoute ($fil) tourne encore après Stop"
# Lancée depuis un fil qui se termine aussitôt (aucune boucle de messages derrière) : l'écoute tient quand même
$rs = [RunspaceFactory]::CreateRunspace(); $rs.Open()
try {
    $ps = [PowerShell]::Create(); $ps.Runspace = $rs
    [void]$ps.AddScript('[MouseHook]::Start([IntPtr]::Zero)')
    $r = @($ps.Invoke())[0]; $ps.Dispose()
    Start-Sleep -Milliseconds 300
    Test-Ok ($r -and [MouseHook]::Running) 'Souris : écoute perdue quand le fil qui l''a lancée n''attend plus'
} finally { try { [MouseHook]::Stop() } catch {}; $rs.Dispose() }
# L'organiseur s'en sert toujours
$org = [IO.File]::ReadAllText((Join-Path $modules 'organizer.ps1'), [Text.Encoding]::UTF8)
Test-Ok ($org -match '\[MouseHook\]::Start\(\$h\)' -and $org -match '\[MouseHook\]::Stop\(\)') 'Souris : organizer.ps1 ne lance ou n''arrête plus l''écoute'

# Fin d'une mesure des FPS par l'Aide : Stop rend la main tout de suite, l'arrêt se termine en arrière plan
try {
    [FrameMon].GetField('remoteOn', [Reflection.BindingFlags]'NonPublic, Static').SetValue($null, $true)
    $chrono = [Diagnostics.Stopwatch]::StartNew()
    [FrameMon]::Stop()
    $ms = $chrono.ElapsedMilliseconds
    Test-Ok ($ms -lt 500) "FPS : la fin de mesure attend l'Aide ($ms ms au lieu de rendre la main)"
    Test-Ok ([FrameMon]::ArretEnCours) 'FPS : « fps.arreter » n''est pas parti en arrière plan'
    Test-Ok (-not [FrameMon]::Remote) 'FPS : mesure par l''Aide toujours notée en cours après Stop'
    Test-Ok ([FrameMon]::AttendreArret(10000) -and -not [FrameMon]::ArretEnCours) 'FPS : l''arrêt en arrière plan ne se termine pas'
} catch { Test-Ok $false "FPS : $($_.Exception.Message)" }
# (le lancement suivant attend cet arrêt : il ne doit pas être coupé par lui)
$cs = [IO.File]::ReadAllText((Join-Path $modules 'natif.cs'), [Text.Encoding]::UTF8)
Test-Ok ($cs -match '(?s)public static bool StartRemote\(int pid\)\s*\{\s*Stop\(\);\s*AttendreArret\(' -and $cs -match '(?s)Arreter\(false\);\s*AttendreArret\(') 'FPS : un nouveau lancement n''attend plus la fin de l''arrêt précédent'

# ---------------------------------------------------------------------------
# 2. L'Aide ne s'arrête pas quand toutes ses connexions sont prises
# ---------------------------------------------------------------------------
$psExe = "$env:windir\System32\WindowsPowerShell\v1.0\powershell.exe"
$dossierAide = Split-Path $FichierAide -Parent
. ([scriptblock]::Create((Get-Fonction (Join-Path $dossierAide 'modules\aide-actions.ps1') 'Get-AideSuffixe')))
[AideClient]::Suffixe = Get-AideSuffixe (Split-Path $dossierAide -Parent)
[AideClient]::ExeAttendu = $psExe
$aide = $null; $clients = New-Object System.Collections.ArrayList
try {
    if ([AideClient]::Joignable(50)) { throw 'une Aide de cette copie tourne déjà : test impossible' }
    $aide = Start-Process $psExe -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$FichierAide`"", '-Parent', $PID -WindowStyle Hidden -PassThru
    $fin = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $fin -and -not [AideClient]::Joignable(100)) { Start-Sleep -Milliseconds 100 }
    Test-Ok ([AideClient]::Joignable(100)) 'Aide : n''écoute pas après 30 s'
    # 40 connexions qui ne disent rien (demandes lentes qui s'entassent)
    $pris = 0
    for ($i = 0; $i -lt 40; $i++) {
        $c = New-Object System.IO.Pipes.NamedPipeClientStream('.', [AideClient]::Nom(), [System.IO.Pipes.PipeDirection]::InOut)
        try { $c.Connect(400); [void]$clients.Add($c); $pris++ } catch { $c.Dispose() }
    }
    Start-Sleep -Milliseconds 1500
    $aide.Refresh()
    Test-Ok (-not $aide.HasExited) "Aide : arrêtée après $pris connexions en attente (PresentMon et la trace réseau seraient coupés)"
    foreach ($c in $clients) { try { $c.Dispose() } catch {} }
    $clients.Clear()
    # Libérée : elle répond de nouveau
    $rep = $null; $fin = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $fin -and -not $rep) {
        try { $rep = [AideClient]::Appeler('{"a":"aide.bonjour","p":{}}', 1000, 5000) } catch { Start-Sleep -Milliseconds 200 }
    }
    Test-Ok ($rep -match '"ok"\s*:\s*true') "Aide : ne répond plus après les connexions en attente ($rep)"
    if ($rep) {
        $j = [AideClient]::Appeler('{"a":"aide.journal","p":{}}', 1000, 5000)
        Test-Ok ($j -match 'connexions sont prises' -and $j -match 'coute de nouveau') "Aide : journal sans la saturation puis la reprise ($pris connexions)"
    }
} catch { Test-Ok $false "Aide : $($_.Exception.Message)" }
finally {
    foreach ($c in $clients) { try { $c.Dispose() } catch {} }
    if ($aide) { try { if (-not $aide.HasExited) { $aide.Kill() } } catch {} }
}

# ---------------------------------------------------------------------------
# 3. Minuteurs qui ne se réempilent plus
# ---------------------------------------------------------------------------
$interface = Join-Path $modules 'interface.ps1'
$astI = [System.Management.Automation.Language.Parser]::ParseFile($interface, [ref]$null, [ref]$null)
# L'attente de la fenêtre telle quelle (Wait-Handle, Invoke-Async et la minuterie qui libère les attentes)
foreach ($st in $astI.EndBlock.Statements) {
    if ($st.Extent.Text -match '^\$script:(WaitFrames|WaitTimer|PassagesEnCours)\b') { . ([scriptblock]::Create($st.Extent.Text)) }
}
foreach ($n in 'Wait-Handle', 'Invoke-Async') { . ([scriptblock]::Create((Get-Fonction $interface $n))) }
try { . ([scriptblock]::Create((Get-Fonction $interface 'Invoke-SansEmpiler'))) }
catch { Test-Ok $false "Minuteurs : $($_.Exception.Message)"; function Invoke-SansEmpiler($Nom, $Action) { & $Action } }
function Write-Log($m) {}

# Une minuterie toutes les 50 ms dont chaque passage attend 400 ms (une Aide lente), pendant 2,5 s. Comme Trafic
# (Start-TrafficWatch), un premier passage est aussi lancé directement juste après le départ de la minuterie.
# (une minuterie de WPF ne se relance pas elle même pendant son propre passage : c'est ce passage direct qui s'empilait
# avec elle, et le réveil de l'Aide demandé à la fois par Trafic et par Lag, vérifié plus bas)
$script:Passage = { $script:Passages++; $script:Prof++; $script:ProfMax = [math]::Max($script:ProfMax, $script:Prof)
    try { [void](Invoke-Async { Start-Sleep -Milliseconds 400 }) } finally { $script:Prof-- } }
function Invoke-Passage { if ($script:Garde) { Invoke-SansEmpiler 'Essai' $script:Passage } else { & $script:Passage } }
function Test-Minuterie([bool]$Garde) {
    $script:Prof = 0; $script:ProfMax = 0; $script:Passages = 0
    $script:Garde = $Garde
    $t = New-Object System.Windows.Threading.DispatcherTimer
    $t.Interval = [TimeSpan]::FromMilliseconds(50)
    $t.Add_Tick({ try { Invoke-Passage } catch {} })
    $cadre = New-Object System.Windows.Threading.DispatcherFrame
    $stop = New-Object System.Windows.Threading.DispatcherTimer
    $stop.Interval = [TimeSpan]::FromMilliseconds(2500)
    $stop.Add_Tick({ $t.Stop(); $stop.Stop(); $cadre.Continue = $false }.GetNewClosure())
    $t.Start(); $stop.Start()
    Invoke-Passage
    [System.Windows.Threading.Dispatcher]::PushFrame($cadre)
    $t.Stop()
    # (les attentes encore ouvertes se terminent)
    $fin = (Get-Date).AddSeconds(10)
    while ($script:Prof -gt 0 -and (Get-Date) -lt $fin) {
        $c2 = New-Object System.Windows.Threading.DispatcherFrame
        $tt = New-Object System.Windows.Threading.DispatcherTimer; $tt.Interval = [TimeSpan]::FromMilliseconds(100)
        $tt.Add_Tick({ $tt.Stop(); $c2.Continue = $false }.GetNewClosure()); $tt.Start()
        [System.Windows.Threading.Dispatcher]::PushFrame($c2)
    }
    @{ Max = $script:ProfMax; Passages = $script:Passages }
}
$sans = Test-Minuterie $false
Test-Ok ($sans.Max -gt 1) "Minuteurs : la situation du bug n'est pas reproduite sans le garde (profondeur $($sans.Max))"
$avec = Test-Minuterie $true
Test-Ok ($avec.Max -eq 1 -and $avec.Passages -ge 3) "Minuteurs : passages empilés malgré le garde (profondeur $($avec.Max), $($avec.Passages) passages)"
# Une erreur dans un passage ne bloque pas les suivants
try { Invoke-SansEmpiler 'Essai' { throw 'panne' } } catch {}
$script:Fait = $false; Invoke-SansEmpiler 'Essai' { $script:Fait = $true }
Test-Ok $script:Fait 'Minuteurs : après une erreur, plus aucun passage'
# Trafic et Lag passent par le garde
foreach ($x in @(@('trafic.ps1', 'Update-Traffic'), @('lag.ps1', 'Update-LagSession'))) {
    $code = [IO.File]::ReadAllText((Join-Path $modules $x[0]), [Text.Encoding]::UTF8)
    Test-Ok ($code -match "Add_Tick\(\{ try \{ Invoke-SansEmpiler '\w+' \{ $($x[1]) \}") "Minuteurs : $($x[0]) relance $($x[1]) sans le garde"
    $directs = @([regex]::Matches($code, "(?m)^\s*$($x[1])\s*$")).Count
    Test-Ok ($directs -eq 0) "Minuteurs : $($x[0]) appelle $($x[1]) directement, sans le garde ($directs fois)"
}
# Réveil de l'Aide : une seule demande à la fois, même quand un passage en relance un autre pendant l'attente
. ([scriptblock]::Create((Get-Fonction (Join-Path $modules 'aide-client.ps1') 'Test-MesuresAide')))
[AideClient]::Suffixe = "test-fluidite-muette-$PID"
$script:EstAdmin = $false; $script:MesuresAideEchec = $null; $script:Reveils = 0; $script:Interieur = $null
function Invoke-Aide { $script:Reveils++; $script:Interieur = Test-MesuresAide; throw 'l''Aide ne répond pas' }
$r1 = Test-MesuresAide
Test-Ok (-not $r1 -and $script:Reveils -eq 1 -and $script:Interieur -eq $false) "Réveil de l'Aide : demandé $($script:Reveils) fois en même temps"
$script:MesuresAideEchec = $null; $script:Reveils = 0
function Invoke-Aide { $script:Reveils++; 'ok' }
Test-Ok ((Test-MesuresAide) -and $script:Reveils -eq 1) 'Réveil de l''Aide : plus jamais redemandé après un premier réveil'

# ---------------------------------------------------------------------------
# Résultat
# ---------------------------------------------------------------------------
if ($echecs.Count) {
    foreach ($e in $echecs) { Write-Host "ÉCHEC  $e" -ForegroundColor Red }
    Write-Host "$($echecs.Count) échec(s) sur $nbTests vérifications." -ForegroundColor Red
    exit 1
}
Write-Host "Fluidité (souris, Aide, minuteurs) : $nbTests vérifications, tout passe." -ForegroundColor Green
exit 0
