# Juge « veille et mesure par l'Aide » : la suite exacte des demandes que la version PowerShell (1.15.x) fait à l'Aide
# autour d'une partie (démarrage, veille des FPS et de la trace réseau, lancement d'un jeu à anti-triche ou non,
# lecture, fin, Aide qui refuse), le journal et la partie enregistrée dans fps.json. Rejoué par
# rust\nevermind-appli\tests\veille_aide.rs (blocs perf et trafic du Rust contre une fausse Aide sur le canal de test).
#
#   & 'outils\exporter-juges-veille.ps1'        (depuis l'outil PowerShell, jamais depuis Bash)
#
# Rien sur le vrai PC : pas d'Aide (une fausse classe AideClient note chaque demande et répond selon le scénario), pas
# de processus (une fausse liste), pas de PresentMon (un faux fichier jamais lancé), un dossier de données à part.
# Les fonctions sont prises telles quelles dans les vrais fichiers ; la vraie classe FrameMon de natif.cs est compilée
# avec la fausse AideClient. Seuls changent : la liste des processus ([FauxProc] à la place de
# [Diagnostics.Process]), l'horloge (Get-Date), Invoke-Aide (fausse, même fin que la vraie : « refusé ou raté »), et la
# mesure du lag réduite aux lignes qui parlent à l'Aide (Start-FluxReseau, Get-FluxReseau, Stop-FluxReseau, vrais).

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
$sortie  = Join-Path $racine 'rust\nevermind-appli\tests\juges\veille-aide.json'
Add-Type -AssemblyName WindowsBase

# ---------------------------------------------------------------------------
# Fausses classes : l'Aide, les processus, la fenêtre au premier plan ; puis la vraie FrameMon
# ---------------------------------------------------------------------------
$natif = [IO.File]::ReadAllText((Join-Path $modules 'natif.cs'), [Text.Encoding]::UTF8)
$debut = $natif.IndexOf('public static class FrameMon')
$fin = $natif.IndexOf('public static class NetProbe')
if ($debut -lt 0 -or $fin -lt $debut) { throw 'FrameMon introuvable dans natif.cs' }
$frameMon = $natif.Substring($debut, $fin - $debut)
Add-Type -TypeDefinition (@'
using System;
using System.Collections.Generic;
public static class AideClient {
    public static List<string> Appels = new List<string>();
    public static Dictionary<string, string> Reponses = new Dictionary<string, string>();
    public static string LignesFps = "";
    static bool fpsDonne;
    public static bool Joignable(int ms) { return true; }
    public static void Vider() { lock (Appels) { Appels.Clear(); Reponses.Clear(); fpsDonne = false; } }
    public static void Noter(string a, string p) { lock (Appels) { Appels.Add(a + " " + p); } }
    static string Json(string t) { return "\"" + t.Replace("\\", "\\\\").Replace("\"", "\\\"").Replace("\r", "\\r").Replace("\n", "\\n") + "\""; }
    public static string Reponse(string a) {
        lock (Appels) {
            string r;
            if (a == "fps.demarrer") fpsDonne = false;
            if (Reponses.TryGetValue(a, out r)) return r;
            if (a == "fps.lire") { string t = fpsDonne ? "" : LignesFps; fpsDonne = true; return "{\"ok\":true,\"r\":" + Json(t) + "}"; }
            return "{\"ok\":true,\"r\":\"ok\"}";
        }
    }
    // (la fin de la vraie AideClient.Texte : « ok », sinon l'erreur de l'Aide)
    public static string Texte(string action, string parametres, int reponseMs) {
        Noter(action, parametres);
        string rep = Reponse(action);
        if (!System.Text.RegularExpressions.Regex.IsMatch(rep, "\"ok\"\\s*:\\s*true")) {
            var e = System.Text.RegularExpressions.Regex.Match(rep, "\"e\"\\s*:\\s*\"((?:[^\"\\\\]|\\\\.)*)\"");
            throw new InvalidOperationException(e.Success ? System.Text.RegularExpressions.Regex.Unescape(e.Groups[1].Value) : "L'Aide a refusé.");
        }
        var r = System.Text.RegularExpressions.Regex.Match(rep, "\"r\"\\s*:\\s*\"((?:[^\"\\\\]|\\\\.)*)\"");
        return r.Success ? System.Text.RegularExpressions.Regex.Unescape(r.Groups[1].Value) : "";
    }
}
public class FauxP { public int Id; public string ProcessName; public string Path; }
public static class FauxProc {
    public static List<FauxP> Liste = new List<FauxP>();
    public static FauxP[] Tous() { return Liste.ToArray(); }
    public static FauxP[] ParNom(string n) { return Liste.FindAll(p => string.Equals(p.ProcessName, n, StringComparison.OrdinalIgnoreCase)).ToArray(); }
}
public static class TrafficMon { public static string GetProcessPath(int pid) { var p = FauxProc.Liste.Find(x => x.Id == pid); return p == null ? "" : p.Path; } }
public static class OGNative { public static int Devant; public static int GetForegroundPid() { return Devant; } }
'@ + $frameMon) -Language CSharp

# ---------------------------------------------------------------------------
# Les vraies fonctions
# ---------------------------------------------------------------------------
function Import-Def([string]$Fichier, [string[]]$Noms) {
    $tok = $null; $err = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules $Fichier), [ref]$tok, [ref]$err)
    if ($err) { throw "$Fichier illisible : $($err[0].Message)" }
    foreach ($n in $Noms) {
        $def = if ($n.StartsWith('$')) {
            $ast.Find({ param($x) $x -is [System.Management.Automation.Language.AssignmentStatementAst] -and $x.Parent -eq $ast.EndBlock -and $x.Left.Extent.Text -eq $n }, $false)
        } else {
            $ast.Find({ param($x) $x -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $x.Name -eq $n }, $true)
        }
        if (-not $def) { throw "$n introuvable dans $Fichier" }
        $t = $def.Extent.Text.Replace('[Diagnostics.Process]::GetProcessesByName(', '[FauxProc]::ParNom(').Replace('[Diagnostics.Process]::GetProcesses()', '[FauxProc]::Tous()')
        . ([scriptblock]::Create($t))
    }
}
. Import-Def 'aide-client.ps1' @('Test-MesuresAide', 'Start-FpsMesure', 'Get-RaisonRefusPresentMon', '$FpsVeilleConnus', 'Get-FpsVeilleJeux', 'Add-FpsVeilleJeu', 'Test-FpsRefus', 'Update-FpsVeille', 'Start-FluxReseau', 'Get-FluxReseau', 'Stop-FluxReseau')
. Import-Def 'lag.ps1' @('Test-LagMeasure', 'Test-FluxVeilleUtile', 'Stop-FluxVeille')
. Import-Def 'jeu.ps1' @('$GenericExe', '$FpsAntiTricheNoms', 'Test-JeuAntiTricheEA', 'Get-JeuxAntiTricheEA', 'Update-FpsVeilleAntiTriche', 'Test-GameRunning', 'Start-GameSession', 'Stop-GameSession',
    'Test-FpsMeasure', 'Test-FpsOverlay', 'Start-FpsTarget', 'Stop-FpsTarget', 'Complete-FpsTarget', 'Update-FpsTarget', 'Get-FpsSessions', 'Compress-Series', 'Save-FpsSession', 'Test-FpsProblem', 'Format-PlayTime')
. Import-Def 'donnees.ps1' @('Save-TexteSur', 'Save-JsonSur', 'Read-JsonSur')

# Les demandes en arrière-plan (veille, arrêt de la trace en veille) sont attendues : l'ordre des demandes est fixe
${function:Update-FpsVeille-Vraie} = ${function:Update-FpsVeille}
function Update-FpsVeille { Update-FpsVeille-Vraie; if ($script:FpsVeilleTache) { [void]$script:FpsVeilleTache.H.AsyncWaitHandle.WaitOne(15000) } }
${function:Stop-FluxVeille-Vraie} = ${function:Stop-FluxVeille}
function Stop-FluxVeille { Stop-FluxVeille-Vraie; if ($script:FluxVeilleArret) { [void]$script:FluxVeilleArret.H.AsyncWaitHandle.WaitOne(15000) } }

# ---------------------------------------------------------------------------
# Le reste de Nevermind, en faux
# ---------------------------------------------------------------------------
function Write-Log([string]$Message) { $script:Journal.Add($Message) }
function Set-Status([string]$Text) {}
function Get-Setting([string]$Name, $Default = $null) { if ($script:Reglages.ContainsKey($Name)) { $script:Reglages[$Name] } else { $Default } }
function Set-Setting([string]$Name, $Value) { $script:Reglages[$Name] = $Value }
function Get-Date { $script:Maintenant }
function Get-Process { param([int]$Id = -1, $ErrorAction)
    $l = [FauxProc]::Tous()
    if ($Id -ge 0) { $x = @($l | Where-Object { $_.Id -eq $Id }); if (-not $x.Count -and $ErrorAction -eq 'Stop') { throw "Processus $Id absent" }; return $x }
    $l
}
function Invoke-Aide([string]$Action, $Parametres = @{}, [int]$Delai = 120000, [switch]$SansDemande) {
    [AideClient]::Noter($Action, (ConvertTo-Json -InputObject $Parametres -Depth 8 -Compress))
    $j = [AideClient]::Reponse($Action) | ConvertFrom-Json
    if (-not $j.ok) { Write-Log "Aide : $Action refusé ou raté : $($j.e)"; throw [string]$j.e }
    $j.r
}
foreach ($f in 'Show-Notify', 'Build-FpsPanel', 'New-ActionNotif', 'Add-FpsSysSample', 'Add-FpsProcSample', 'Show-FpsOverlay', 'Hide-FpsOverlay', 'Save-GameModePending',
    'Clear-GameModePending', 'Update-GameModeStatus', 'Update-LibraryIfShown', 'Add-PlayTime', 'Get-GameModeSelection', 'Get-FpsDiagData', 'Get-TrafficTcp') {
    Set-Item -Path "function:script:$f" -Value { $null }
}
function Restart-GameModeApps { @() }
# La mesure du lag, réduite à ce qui parle à l'Aide (les mêmes lignes que lag.ps1 : Start-LagSession, Update-LagSession,
# Stop-LagSession ; les pings et l'annuaire ne demandent rien à l'Aide)
function Start-LagSession([string]$Game, [int]$ProcId, [int]$Seconds = 0) {
    if ($script:LagSession) { return }
    $flux = Start-FluxReseau
    $etw = [bool]$flux
    if (-not $etw -and $ProcId -and (Test-FpsRefus $script:FluxErreur)) {
        $pn = try { (Get-Process -Id $ProcId -ErrorAction Stop).ProcessName } catch { '' }
        if ($pn) { Add-FpsVeilleJeu $pn }
    }
    $script:LagSession = @{ Game = $Game; Pid = $ProcId; Seconds = $Seconds; Etw = $etw; Flux = $flux }
    Write-Log "Lag: mesure de la connexion ($Game)$(if ($etw) { ', serveur repérable en UDP' })"
}
function Update-LagSession { $s = $script:LagSession; if ($s -and $s.Etw) { try { [void]@(Get-FluxReseau $s.Flux) } catch {} } }
function Stop-LagSession { $s = $script:LagSession; $script:LagSession = $null; if ($s -and $s.Etw) { Stop-FluxReseau $s.Flux } }

# ---------------------------------------------------------------------------
# Les scénarios (les mêmes entrées sont rejouées par le Rust)
# ---------------------------------------------------------------------------
$ci = [Globalization.CultureInfo]::InvariantCulture
$jeux = @{
    bf6 = @{ nom = 'Battlefield™ 6'; exes = @('C:\JeuxEssaiNevermind\Battlefield 6\bf6.exe'); source = 'EA' }
    fc27 = @{ nom = 'EA SPORTS FC™ 27'; exes = @('C:\JeuxEssaiNevermind\FC 27\FC27.exe'); source = 'EA' }
    rl = @{ nom = 'Rocket League'; exes = @('C:\JeuxEssaiNevermind\rocketleague\Binaries\Win64\RocketLeague.exe'); source = 'Epic' }
}
$procs = @{
    bf6 = @{ pid = 7000101; nom = 'bf6'; chemin = 'C:\JeuxEssaiNevermind\Battlefield 6\bf6.exe' }
    rl = @{ pid = 7000202; nom = 'RocketLeague'; chemin = 'C:\JeuxEssaiNevermind\rocketleague\Binaries\Win64\RocketLeague.exe' }
}
# Images de PresentMon (deux durées qui alternent, nombre, temps processeur, temps carte) : 30 s à 118 FPS, 20 s de menu
# bloqué à 60 FPS, 33 s à 91 FPS
$images = @(@('7.5', '9.5', 3600, '4.1', '6.2'), @('16.667', '16.667', 1200, '2.0', '3.0'), @('10', '12', 3000, '5.5', '9.75'))
$refus = @{
    'fps.veiller' = '{"ok":false,"e":"PresentMon refusé : accès refusé"}'
    'flux.veiller' = '{"ok":false,"e":"Trace réseau refusée par Windows : accès refusé (droits administrateur, ou anti-triche d''un jeu en cours)"}'
    'fps.demarrer' = '{"ok":false,"e":"Mesure refusée par Windows : accès refusé"}'
    'flux.demarrer' = '{"ok":false,"e":"Trace réseau refusée par Windows : accès refusé (droits administrateur, ou anti-triche d''un jeu en cours)"}'
}
$scenarios = @(
    @{ nom = 'démarrage avec BF6 et FC 27 installés'; jeux = @('bf6', 'fc27', 'rl'); fin = 62; evenements = @() }
    @{ nom = 'BF6 lancé, veille déjà ouverte, partie puis fin'; jeux = @('bf6', 'fc27', 'rl'); fin = 150; reponses = @{ 'fps.demarrer' = '{"ok":true,"r":"rattachee"}' }
        evenements = @(@{ t = 12; lance = 'bf6' }, @{ t = 135; quitte = 'bf6' }) }
    @{ nom = 'jeu sans anti-triche (Rocket League), partie puis fin'; jeux = @('bf6', 'fc27', 'rl'); fin = 195
        evenements = @(@{ t = 58; lance = 'rl' }, @{ t = 180; quitte = 'rl' }) }
    @{ nom = 'Aide qui refuse la veille et les mesures'; jeux = @('bf6', 'fc27', 'rl'); fin = 70; reponses = $refus
        evenements = @(@{ t = 12; lance = 'rl' }, @{ t = 40; quitte = 'rl' }) }
    @{ nom = 'mesure du lag coupée'; jeux = @('bf6', 'rl'); fin = 150; reglages = @{ LagMeasure = $false }; reponses = @{ 'fps.demarrer' = '{"ok":true,"r":"rattachee"}' }
        evenements = @(@{ t = 12; lance = 'bf6' }, @{ t = 135; quitte = 'bf6' }) }
    @{ nom = 'mesure des FPS coupée'; jeux = @('bf6', 'rl'); fin = 150; reglages = @{ FpsMeasure = $false }
        evenements = @(@{ t = 12; lance = 'bf6' }, @{ t = 135; quitte = 'bf6' }) }
    @{ nom = 'BF6 désinstallé pendant la veille'; jeux = @('bf6', 'rl'); fin = 40; evenements = @(@{ t = 20; jeux = @('rl') }) }
    @{ nom = 'aucun jeu guetté installé'; jeux = @('rl'); fin = 40; reglages = @{ FpsVeille = @() }; evenements = @() }
)

$PresentMonExe = Join-Path $env:TEMP "nevermind-juge-veille-$PID\outils-tiers\PresentMon.exe"
[void](New-Item -ItemType Directory -Force -Path (Split-Path $PresentMonExe))
[IO.File]::WriteAllText($PresentMonExe, 'pas un programme')
$script:EstAdmin = $false
$script:Closing = $true   # (Stop-FpsTarget attend le bilan au lieu de le confier à un minuteur de la fenêtre)
$entete = 'Application,ProcessID,MsBetweenPresents,PresentMode,MsCPUBusy,MsGPUBusy'

function Set-Jeux([string[]]$Cles) {
    $script:Games = @($Cles | ForEach-Object { @{ Name = $jeux[$_].nom; Exes = @($jeux[$_].exes); Source = $jeux[$_].source } })
    # (la table de Update-GameCache)
    $script:GameIndex = @{}
    foreach ($g in $script:Games) {
        foreach ($e in @($g.Exes)) {
            $base = [IO.Path]::GetFileNameWithoutExtension($e).ToLower()
            if ($base.Length -lt 3 -or $base -match $GenericExe) { continue }
            if (-not $script:GameIndex.ContainsKey($base)) { $script:GameIndex[$base] = @{ Game = $g.Name; Exes = @(); Source = $g.Source; ByPath = @{} } }
            $script:GameIndex[$base].Exes += $e.ToLower()
            $script:GameIndex[$base].ByPath[$e.ToLower()] = $g.Name
        }
    }
    Update-FpsVeilleAntiTriche
}

$cas = @()
foreach ($s in $scenarios) {
    $donnees = Join-Path $env:TEMP "nevermind-juge-veille-$PID\donnees-$($cas.Count)"
    [void](New-Item -ItemType Directory -Force -Path $donnees)
    $FpsFile = Join-Path $donnees 'fps.json'
    $script:Journal = New-Object System.Collections.Generic.List[string]
    $script:Reglages = @{ FpsMeasure = $true; FpsVeille = @('fc27.exe', 'fc27_trial.exe') }
    if ($s.reglages) { foreach ($k in $s.reglages.Keys) { $script:Reglages[$k] = $s.reglages[$k] } }
    $reglagesDepart = @{}; foreach ($k in $script:Reglages.Keys) { $reglagesDepart[$k] = $script:Reglages[$k] }
    foreach ($v in 'GameSession', 'FpsTarget', 'LagSession', 'FpsVeilleTache', 'FpsVeilleDemande', 'FpsVeilleEtat', 'FluxVeille', 'FluxVeilleArret', 'MesuresAideEchec', 'MesuresAideReveil', 'FluxErreur') { Set-Variable -Scope Script -Name $v -Value $null }
    [AideClient]::Vider()
    if ($s.reponses) { foreach ($k in $s.reponses.Keys) { [AideClient]::Reponses[$k] = $s.reponses[$k] } }
    [FauxProc]::Liste.Clear(); [OGNative]::Devant = 0
    $script:Maintenant = [datetime]'2026-11-06T19:00:00'
    Set-Jeux $s.jeux
    $pas = 0
    while ($pas * 0.25 -le $s.fin) {
        $t = $pas * 0.25
        $script:Maintenant = ([datetime]'2026-11-06T19:00:00').AddSeconds($t)
        foreach ($e in @($s.evenements | Where-Object { $_.t -eq $t })) {
            if ($e.lance) {
                $p = $procs[$e.lance]
                [FauxProc]::Liste.Add((New-Object FauxP -Property @{ Id = $p.pid; ProcessName = $p.nom; Path = $p.chemin }))
                [OGNative]::Devant = $p.pid
                $lignes = @($entete) + @(foreach ($seg in $images) { for ($i = 0; $i -lt $seg[2]; $i++) { "$($p.nom).exe,$($p.pid),$(if ($i % 2) { $seg[1] } else { $seg[0] }),Hardware: Independent Flip,$($seg[3]),$($seg[4])" } })
                [AideClient]::LignesFps = ($lignes -join "`n") + "`n"
            }
            if ($e.quitte) { [void][FauxProc]::Liste.RemoveAll([Predicate[FauxP]] { param($x) $x.ProcessName -eq $procs[$e.quitte].nom }); [OGNative]::Devant = 0 }
            if ($e.jeux) { Set-Jeux $e.jeux }
        }
        if ($script:FpsTarget -and ($pas % 2) -eq 0 -and $t -gt $script:FpsTarget.T0) { Update-FpsTarget; if (-not $script:FpsTarget) { [void][FrameMon]::AttendreArret(8000) } }
        if (($pas % 20) -eq 0) {
            $avant = $script:FpsTarget
            Test-GameRunning
            if ($script:FpsTarget -and -not $avant) {
                $script:FpsTarget.T0 = $t
                # (la vraie FrameMon lit l'Aide dans son propre fil, toutes les 250 ms : on attend sa première lecture)
                $limite = [DateTime]::UtcNow.AddSeconds(10)
                while ([FrameMon]::Frames -eq 0 -and [DateTime]::UtcNow -lt $limite) { Start-Sleep -Milliseconds 20 }
            }
        }
        if ($script:LagSession -and ($pas % 8) -eq 0) { Update-LagSession }
        $pas++
    }
    if ($script:FpsTarget) { Stop-FpsTarget; [void][FrameMon]::AttendreArret(8000) }
    $demandes = @(foreach ($a in @([AideClient]::Appels)) { $i = $a.IndexOf(' '); @{ a = $a.Substring(0, $i); p = ($a.Substring($i + 1) | ConvertFrom-Json) } })
    $parties = @(if (Test-Path -LiteralPath $FpsFile) { @(Get-Content -LiteralPath $FpsFile -Raw -Encoding UTF8 | ConvertFrom-Json) })
    $cas += [pscustomobject]@{
        nom = $s.nom; jeux = @($s.jeux | ForEach-Object { $jeux[$_] }); fin = $s.fin; reglages = $reglagesDepart; reponses = $(if ($s.reponses) { $s.reponses } else { @{} })
        evenements = @($s.evenements | ForEach-Object { $e = $_; @{ t = $e.t; lance = $(if ($e.lance) { $procs[$e.lance] }); quitte = $(if ($e.quitte) { $procs[$e.quitte] }); jeux = $(if ($e.jeux) { @($e.jeux | ForEach-Object { $jeux[$_] }) }) } })
        images = $images; entete = $entete
        demandes = @($demandes | Where-Object { $_.a -notin 'fps.lire', 'flux.lire' })
        lectures = @{ fps = @($demandes | Where-Object { $_.a -eq 'fps.lire' }).Count -gt 0; flux = @($demandes | Where-Object { $_.a -eq 'flux.lire' }).Count -gt 0 }
        journal = @($script:Journal)
        fpsVeille = @(Get-Setting 'FpsVeille' @())
        parties = @($parties | ForEach-Object { $_ | Select-Object Game, Key, Avg, Low1, Low01, Seconds, Frames, Exclusive, Series, MenuSec, MenuFps })
    }
    Write-Host "$($s.nom) : $(@($cas[-1].demandes).Count) demandes, $($parties.Count) partie(s)"
}
Remove-Item -LiteralPath (Join-Path $env:TEMP "nevermind-juge-veille-$PID") -Recurse -Force
[IO.File]::WriteAllText($sortie, (ConvertTo-Json -InputObject @{ cas = $cas } -Depth 10), (New-Object Text.UTF8Encoding $false))
Write-Host "Juge écrit : $sortie"
