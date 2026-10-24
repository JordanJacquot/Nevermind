# Tests de la mesure des FPS en jeu (lot 4), sur de fausses données, sans droits administrateur, sans lancer PresentMon
# et sans toucher aux vrais réglages, au vrai journal ni aux vrais programmes.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-fps.ps1
#
# Ce script contrôle :
#   1. les jeux à anti-triche EA (FC 27, FIFA, Battlefield 6...) : repérés à leur installation (fichiers EAAntiCheat à
#      côté du jeu, dans de faux dossiers de %TEMP%) ou à leur nom, et guettés avant leur lancement dès la première
#      partie (réglage FpsVeille, 10 jeux au plus, noms que l'Aide accepte) ; un jeu ordinaire n'est pas guetté ;
#   2. la veille de l'Aide n'est jamais redemandée sans un jeu guetté qui vient de se lancer (elle serait relancée sans
#      lui, juste quand sa mesure doit s'y rattacher) ; sans jeu ouvert, elle est demandée avec tous les jeux ;
#   3. le compteur : « L'anti-triche de ce jeu bloque la mesure des FPS » quand Windows refuse la mesure, ou quand un jeu
#      guetté ne donne aucune image après 30 s ; caché en plein écran exclusif (et jamais remis devant), revenu en
#      fenêtré ; la bulle de fin de partie le redit quand rien n'a pu être mesuré ;
#   4. les bulles de fin de partie (FPS, connexion, mode jeu) : une à la fois, aucune perdue, jamais un fichier à moitié
#      écrit pour Mindset ;
#   5. le diagnostic ne propose jamais de fermer msedgewebview2 (écrans de Mindset et de Nevermind) ni l'anti-triche EA ;
#   6. la vraie [FrameMon] (natif.cs, compilée dans un PowerShell à part) sur une très longue partie : la mesure ne
#      s'arrête plus, la mémoire reste bornée, moyenne, durée, nombre d'images et paliers restent justes, et le bilan
#      se calcule dans un autre fil sur une copie.
# Code de sortie : 0 si tout passe, 1 sinon.

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'OptiGame\fichiers\modules'
$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0
function Test-Ok([bool]$Condition, [string]$Message) {
    $script:nbTests++
    if (-not $Condition) { $script:echecs.Add($Message) }
}
Add-Type -AssemblyName WindowsBase

# ---------------------------------------------------------------------------
# Sécurité du test : rien sur le vrai PC (une fonction passe avant une commande du même nom)
# ---------------------------------------------------------------------------
foreach ($c in 'Stop-Process', 'Start-Process', 'Set-ItemProperty', 'New-ItemProperty', 'Remove-ItemProperty', 'Invoke-Aide', 'Stop-ProgrammeNom', 'taskkill', 'taskkill.exe') {
    Set-Item -Path "function:script:$c" -Value ([scriptblock]::Create("throw 'Test : $c est interdit pendant le test des FPS.'"))
}
$script:Journal = New-Object System.Collections.Generic.List[string]
$script:Statuts = New-Object System.Collections.Generic.List[string]
$script:Reglages = @{}
function script:Write-Log([string]$Message) { $script:Journal.Add($Message) }
function script:Set-Status([string]$Text) { $script:Statuts.Add($Text) }
function script:Get-Setting([string]$Nom, $Defaut) { if ($script:Reglages.ContainsKey($Nom)) { $script:Reglages[$Nom] } else { $Defaut } }
function script:Set-Setting([string]$Nom, $Valeur) { $script:Reglages[$Nom] = $Valeur }

# Fausses classes natives : rien n'est lancé, chaque appel à l'Aide est noté
Add-Type -TypeDefinition @'
public static class FrameMon {
    public static int Frames;
    public static bool Remote = true, Paused;
    public static string LastError = "", LastMode = "";
    public static double Sample() { return -1; }
    public static double[] Live() { return new double[] { 144, 120, 140 }; }
}
public static class OGNative { public static int Devant; public static int GetForegroundPid() { return Devant; } }
public static class AideClient {
    public static System.Collections.Generic.List<string> Appels = new System.Collections.Generic.List<string>();
    public static bool Joignable(int ms) { return true; }
    public static string Texte(string action, string json, int ms) { lock (Appels) { Appels.Add(action + " " + json); } return "ok"; }
}
'@

# ---------------------------------------------------------------------------
# Chargement : seulement les fonctions et variables utiles, prises dans les vrais fichiers
# ---------------------------------------------------------------------------
function Import-Def([string]$Fichier, [string[]]$Noms) {
    $tok = $null; $err = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules $Fichier), [ref]$tok, [ref]$err)
    if ($err) { Write-Host "$Fichier n'est pas lisible : $($err[0].Message)" -ForegroundColor Red; exit 1 }
    foreach ($n in $Noms) {
        $def = if ($n.StartsWith('$')) {
            $ast.Find({ param($x) $x -is [System.Management.Automation.Language.AssignmentStatementAst] -and $x.Parent -eq $ast.EndBlock -and $x.Left.Extent.Text -eq $n }, $false)
        } else {
            $ast.Find({ param($x) $x -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $x.Name -eq $n }, $true)
        }
        if (-not $def) { Write-Host "$n introuvable dans $Fichier" -ForegroundColor Red; exit 1 }
        . ([scriptblock]::Create($def.Extent.Text))
    }
}
. Import-Def 'jeu.ps1' @('$GenericExe', '$FpsAntiTricheNoms', 'Test-JeuAntiTricheEA', 'Get-JeuxAntiTricheEA', 'Update-FpsVeilleAntiTriche', 'Update-FpsTarget', 'Complete-FpsTarget')
. Import-Def 'aide-client.ps1' @('$FpsVeilleConnus', 'Get-FpsVeilleJeux', 'Add-FpsVeilleJeu', 'Test-FpsRefus', 'Update-FpsVeille')
. Import-Def 'assistance.ps1' @('$NotifyEcart', 'Show-Notify', 'Send-NotifySuivante')
. Import-Def 'diagnostic-fps.ps1' @('$SystemProcs', '$Browsers', 'Add-DiagItem', 'Get-SessionGame', 'Get-FpsDiagnosis')

$tmp = Join-Path $env:TEMP "nevermind-test-fps-$PID"
[void][IO.Directory]::CreateDirectory($tmp)
try {

# ---------------------------------------------------------------------------
# 1. Jeux à anti-triche EA repérés à l'installation ou à leur nom
# ---------------------------------------------------------------------------
function New-FauxJeu([string]$Dossier, [string]$Exe, [string[]]$Autres = @(), [switch]$DossierEAAC) {
    $d = Join-Path $tmp $Dossier
    [void][IO.Directory]::CreateDirectory($d)
    foreach ($f in @($Exe) + $Autres) { [IO.File]::WriteAllText((Join-Path $d $f), 'faux') }
    if ($DossierEAAC) { [void][IO.Directory]::CreateDirectory((Join-Path $d 'EAAntiCheat')) }
    Join-Path $d $Exe
}
$fc27 = New-FauxJeu 'EA SPORTS FC 27' 'FC27.exe' @('EAAntiCheat.GameServiceLauncher.exe', 'EAAntiCheat.Installer.exe', 'FC27_Trial.exe')
$jeux = @(
    @{ Name = 'EA SPORTS FC 27'; Exes = @($fc27, (Join-Path (Split-Path $fc27) 'FC27_Trial.exe')) }
    @{ Name = 'Battlefield 6'; Exes = @(New-FauxJeu 'Battlefield 6' 'bf6.exe' -DossierEAAC) }
    @{ Name = 'Jeu EA neuf'; Exes = @(New-FauxJeu 'Jeu EA neuf' 'Javelin2027.exe' @('EAAntiCheat.Installer.exe')) }
    @{ Name = 'FC 28 (dossier illisible)'; Exes = @('Z:\nulle-part\FC28.exe') }
    @{ Name = 'FIFA 23'; Exes = @('Z:\nulle-part\FIFA23.exe') }
    @{ Name = 'Rocket League'; Exes = @(New-FauxJeu 'rocketleague' 'RocketLeague.exe') }
    @{ Name = 'Jeu générique'; Exes = @(New-FauxJeu 'generique' 'game.exe' @('EAAntiCheat.Installer.exe')) }
    @{ Name = 'Nom accentué'; Exes = @(New-FauxJeu 'accent' 'Jeué.exe' @('EAAntiCheat.Installer.exe')) }
    @{ Name = 'Sans programme'; Exes = @() }
)
$noms = @(Get-JeuxAntiTricheEA $jeux)
foreach ($n in 'fc27.exe', 'bf6.exe', 'javelin2027.exe', 'fc28.exe', 'fifa23.exe') { Test-Ok ($noms -contains $n) "Anti-triche EA : $n devrait être guetté (trouvés : $($noms -join ', '))" }
foreach ($n in 'rocketleague.exe', 'game.exe', 'fc27_trial.exe') { Test-Ok ($noms -notcontains $n) "Anti-triche EA : $n ne devrait pas être guetté" }
Test-Ok (-not @($noms | Where-Object { $_ -notmatch '^[a-z0-9][a-z0-9 ._()-]{0,60}\.exe$' }).Count) "Anti-triche EA : un nom refusé par l'Aide ($($noms -join ', '))"
Test-Ok (Test-JeuAntiTricheEA $fc27) 'FC27.exe à côté de EAAntiCheat.GameServiceLauncher.exe : pas reconnu'
Test-Ok (-not (Test-JeuAntiTricheEA (Join-Path $tmp 'rocketleague\RocketLeague.exe'))) 'Rocket League pris pour un jeu EA'

# Réglage : ajoutés une fois, sans doublon avec bf6.exe (déjà connu) ni avec un jeu déjà appris, 10 au plus
$script:Reglages = @{ FpsVeille = @('fc27.exe') }
$script:Games = $jeux
Update-FpsVeilleAntiTriche
$v = @(Get-FpsVeilleJeux)
Test-Ok ($v.Count -eq 5 -and $v -contains 'fc27.exe' -and $v -contains 'fifa23.exe' -and $v -contains 'bf6.exe') "Veille : jeux guettés « $($v -join ', ') »"
Test-Ok (@($script:Reglages.FpsVeille | Where-Object { $_ -eq 'fc27.exe' }).Count -eq 1) 'Veille : fc27.exe ajouté deux fois'
Update-FpsVeilleAntiTriche
Test-Ok (@(Get-FpsVeilleJeux).Count -eq 5) 'Veille : un deuxième passage ajoute encore des jeux'
$script:Games = @(1..15 | ForEach-Object { @{ Name = "FC$(10 + $_)"; Exes = @("Z:\x\FC$(10 + $_).exe") } })
Update-FpsVeilleAntiTriche
Test-Ok (@(Get-FpsVeilleJeux).Count -eq 10) "Veille : $(@(Get-FpsVeilleJeux).Count) jeux guettés (l'Aide en refuse plus de 10)"

# ---------------------------------------------------------------------------
# 2. La veille n'est jamais redemandée sans un jeu guetté qui vient de se lancer
# ---------------------------------------------------------------------------
function script:Test-FpsMeasure { $true }
function script:Test-LagMeasure { $false }
function script:Test-FluxVeilleUtile($Flux) { $false }
$moi = "$([Diagnostics.Process]::GetCurrentProcess().ProcessName.ToLowerInvariant()).exe"   # un « jeu guetté » qui tourne : ce test
$script:Reglages = @{ FpsVeille = @('fc27.exe', $moi) }
$script:FpsTarget = $null; $script:LagSession = $null; $script:FpsVeilleTache = $null; $script:FpsVeilleDemande = $null
[AideClient]::Appels.Clear()
Update-FpsVeille
Start-Sleep -Milliseconds 300
if ($script:FpsVeilleTache) { [void]$script:FpsVeilleTache.H.AsyncWaitHandle.WaitOne(5000) }
Test-Ok (-not [AideClient]::Appels.Count) "Veille redemandée alors qu'un jeu guetté tourne : « $([AideClient]::Appels -join ' | ') »"
Test-Ok ($null -eq $script:FpsVeilleDemande) 'Veille : l''attente de 30 s comptée sans rien avoir demandé'
$script:Reglages = @{ FpsVeille = @('fc27.exe') }
Update-FpsVeille
if ($script:FpsVeilleTache) { [void]$script:FpsVeilleTache.H.AsyncWaitHandle.WaitOne(5000) }
$a = @([AideClient]::Appels)
Test-Ok ($a.Count -eq 1 -and $a[0] -match '^fps\.veiller ' -and $a[0] -match 'bf6\.exe' -and $a[0] -match 'fc27\.exe') "Veille sans jeu ouvert : « $($a -join ' | ') »"

# ---------------------------------------------------------------------------
# 3. Le compteur : anti-triche et plein écran exclusif
# ---------------------------------------------------------------------------
function script:Add-FpsSysSample($T) { }
function script:Add-FpsProcSample($T) { }
function script:Set-OverlayPosition([int]$ProcId) { }
function script:Get-Brush($c) { $c }
function script:Get-FpsColor($v) { 'vert' }
function script:Stop-FpsTarget { $script:Arrete = $true }
function New-FauxCompteur {
    $w = [pscustomobject]@{ IsVisible = $false; Topmost = $true; Montre = 0 }
    $w | Add-Member ScriptMethod Show { $this.IsVisible = $true; $this.Montre++ }
    $w | Add-Member ScriptMethod Hide { $this.IsVisible = $false }
    @{ Win = $w; Fps = [pscustomobject]@{ Text = ''; Foreground = '' }; Sub = [pscustomobject]@{ Text = '' }; Pid = 0 }
}
function New-FausseCible([string]$Cle) { @{ Pid = $PID; Name = 'Jeu essai'; Key = $Cle; Start = Get-Date; Series = (New-Object System.Collections.ArrayList); Ticks = 0; Exclusive = $false; Warned = $false } }
[OGNative]::Devant = $PID
$script:EstAdmin = $false
$antiTriche = 'L''anti-triche de ce jeu bloque la mesure des FPS'

# Plein écran exclusif : compteur caché, jamais remis devant ; revient quand le jeu repasse en fenêtré
$script:FpsTarget = New-FausseCible 'jeuessai'; $script:Overlay = New-FauxCompteur; $script:OverlayTicks = 0
[FrameMon]::Frames = 5000; [FrameMon]::LastError = ''; [FrameMon]::LastMode = 'Hardware: Legacy Flip'
for ($i = 0; $i -lt 12; $i++) { Update-FpsTarget }
Test-Ok (-not $script:Overlay.Win.IsVisible -and $script:Overlay.Win.Montre -eq 0) 'Plein écran exclusif : le compteur est quand même affiché'
Test-Ok ($script:FpsTarget.Exclusive) 'Plein écran exclusif : pas noté pour la partie'
Test-Ok (@($script:Statuts | Where-Object { $_ -match 'plein écran' -and $_ -match 'caché' }).Count -eq 1) "Plein écran exclusif : message « $($script:Statuts -join ' | ') »"
$script:Overlay.Win.IsVisible = $true
Update-FpsTarget
Test-Ok (-not $script:Overlay.Win.IsVisible) 'Plein écran exclusif : le compteur visible n''est pas caché'
[FrameMon]::LastMode = 'Hardware Composed: Independent Flip'
Update-FpsTarget
Test-Ok ($script:Overlay.Win.IsVisible -and $script:Overlay.Fps.Text -eq '144') 'Retour en fenêtré : le compteur ne revient pas'
Test-Ok ($script:FpsTarget.Exclusive) 'Retour en fenêtré : la partie n''est plus notée en plein écran exclusif'

# Windows refuse la mesure (anti-triche) : le message clair, et le jeu sera guetté la prochaine fois
$script:Reglages = @{}; $script:Statuts.Clear()
$script:FpsTarget = New-FausseCible 'fc29'; $script:Overlay = New-FauxCompteur
[FrameMon]::Frames = 0; [FrameMon]::LastMode = ''; [FrameMon]::LastError = 'error: access denied. "Performance Log Users" user group.'
Update-FpsTarget
Test-Ok ($script:FpsTarget.Refus) 'Refus de Windows : pas reconnu'
Test-Ok ($script:Overlay.Sub.Text -eq $antiTriche -and $script:Overlay.Fps.Text -eq '?') "Refus : le compteur affiche « $($script:Overlay.Fps.Text) / $($script:Overlay.Sub.Text) »"
Test-Ok (@($script:Statuts | Where-Object { $_ -match 'anti-triche de ce jeu bloque la mesure des FPS' }).Count -eq 1) "Refus : message « $($script:Statuts -join ' | ') »"
Test-Ok (@(Get-FpsVeilleJeux) -contains 'fc29.exe') 'Refus : le jeu n''est pas retenu pour la veille'

# Jeu guetté, aucune erreur, mais aucune image après 30 s devant : c'est encore l'anti-triche
$script:Reglages = @{ FpsVeille = @('fc27.exe') }; $script:Statuts.Clear()
$script:FpsTarget = New-FausseCible 'fc27'; $script:Overlay = New-FauxCompteur
[FrameMon]::LastError = ''
for ($i = 0; $i -lt 59; $i++) { Update-FpsTarget }
Test-Ok (-not $script:FpsTarget.Refus) 'Jeu guetté : anti-triche annoncé avant 30 s'
Update-FpsTarget
Test-Ok ($script:FpsTarget.Refus -and $script:Overlay.Sub.Text -eq $antiTriche) "Jeu guetté sans image après 30 s : « $($script:Overlay.Sub.Text) »"
# Jeu ordinaire sans image : pas d'anti-triche inventé
$script:FpsTarget = New-FausseCible 'jeuordinaire'; $script:FpsTarget.Start = (Get-Date).AddSeconds(-40); $script:Overlay = New-FauxCompteur
for ($i = 0; $i -lt 80; $i++) { Update-FpsTarget }
Test-Ok (-not $script:FpsTarget.Refus -and $script:Overlay.Sub.Text -match 'Aucune image') "Jeu ordinaire sans image : « $($script:Overlay.Sub.Text) »"
# Jeu en fond : le compteur sans image n'avance pas vers l'annonce
[OGNative]::Devant = 0
$script:FpsTarget = New-FausseCible 'fc27'; $script:Overlay = New-FauxCompteur
for ($i = 0; $i -lt 80; $i++) { Update-FpsTarget }
Test-Ok (-not $script:FpsTarget.Refus) 'Jeu en fond : anti-triche annoncé alors que le jeu n''était pas devant'
[OGNative]::Devant = $PID

# Fin de partie sans aucune image : la bulle le dit
$script:Bulles = New-Object System.Collections.Generic.List[string]
function script:Show-Notify([string]$Title, [string]$Text, $OnClick) { $script:Bulles.Add("$Title|$Text") }
$bilanVide = [pscustomobject]@{ Result = [double[][]]@([double[]]@(0, 0, 0, 0, 0), [double[]]@(0, 0), [double[]]@(-1, -1, 0)) }
$t = New-FausseCible 'fc27'; $t.Refus = $true; $t.Err = ''
Complete-FpsTarget $t $bilanVide
Test-Ok ($script:Bulles.Count -eq 1 -and $script:Bulles[0] -match 'anti-triche de ce jeu bloque la mesure des FPS') "Fin de partie bloquée : bulle « $($script:Bulles -join ' | ') »"
$script:Bulles.Clear()
$t = New-FausseCible 'jeuordinaire'; $t.Err = ''
Complete-FpsTarget $t $bilanVide
Test-Ok (-not $script:Bulles.Count) 'Fin de partie sans image, sans anti-triche : une bulle inutile'
Remove-Item function:script:Show-Notify
. Import-Def 'assistance.ps1' @('Show-Notify')

# ---------------------------------------------------------------------------
# 4. Bulles pour Mindset : une à la fois, aucune perdue, jamais à moitié écrite
# ---------------------------------------------------------------------------
$DataDir = Join-Path $tmp 'donnees'
[void][IO.Directory]::CreateDirectory($DataDir)
$Managed = $true; $script:Closing = $false
$fBulle = Join-Path $DataDir 'notification.demande'
function Read-Bulle { $j = [IO.File]::ReadAllText($fBulle, [Text.Encoding]::UTF8) | ConvertFrom-Json; [IO.File]::Delete($fBulle); [string]$j.titre }
$script:Statuts.Clear()
Show-Notify 'Partie terminée : Jeu essai' '144 FPS en moyenne.'
Show-Notify 'Connexion pendant Jeu essai' 'Ta connexion était stable.'
Show-Notify 'Fin du mode jeu' 'Les applis fermées ont été relancées.'
Test-Ok ([IO.File]::Exists($fBulle)) 'Bulles : rien d''écrit pour Mindset'
Test-Ok (-not @(Get-ChildItem -LiteralPath $DataDir -Filter '*.ecriture').Count) 'Bulles : un fichier en cours d''écriture est resté'
$vues = @()
if ([IO.File]::Exists($fBulle)) { $vues += Read-Bulle }
Send-NotifySuivante
Test-Ok (-not [IO.File]::Exists($fBulle)) 'Bulles : la suivante remplace la précédente sans laisser le temps de la voir'
$script:NotifyDerniere = (Get-Date).AddSeconds(-10)
Send-NotifySuivante
if ([IO.File]::Exists($fBulle)) { $vues += Read-Bulle }
# (Mindset n'a pas encore pris la bulle : elle n'est pas écrasée)
$script:NotifyDerniere = (Get-Date).AddSeconds(-10)
Send-NotifySuivante
$script:NotifyDerniere = (Get-Date).AddSeconds(-10)
Send-NotifySuivante
if ([IO.File]::Exists($fBulle)) { $vues += Read-Bulle }
Test-Ok (($vues -join '|') -eq 'Partie terminée : Jeu essai|Connexion pendant Jeu essai|Fin du mode jeu') "Bulles montrées : « $($vues -join ' | ') »"
Test-Ok (-not $script:NotifyFile.Count -and -not $script:NotifyTimer.IsEnabled) 'Bulles : la file ou sa minuterie ne s''arrête pas'
Test-Ok ($script:Statuts.Count -eq 3) 'Bulles : la barre d''état ne reçoit pas les trois messages'
# Bulle non prise par Mindset (fermé) : écrite une fois, jamais écrasée ; celles qui attendent plus de 5 minutes sont oubliées
[IO.File]::WriteAllText($fBulle, '{"titre":"ancienne"}')
Show-Notify 'Nouvelle' 'texte'
$script:NotifyFile[0].Quand = (Get-Date).AddMinutes(-6)
$script:NotifyDerniere = (Get-Date).AddSeconds(-10)
Send-NotifySuivante
Test-Ok (-not $script:NotifyFile.Count -and (Read-Bulle) -eq 'ancienne') 'Bulles : une bulle vieille de 6 minutes attend encore, ou la bulle non prise a été écrasée'

# ---------------------------------------------------------------------------
# 5. Diagnostic : jamais « Fermer msedgewebview2 » ni l'anti-triche EA
# ---------------------------------------------------------------------------
function script:Get-SessionName($S) { 'Jeu essai' }
function script:Test-PowerPlan { $true }
function script:Get-FpsSessions { @() }
function script:Test-GamePriority($g) { $true }
$script:AnalysisData = $null; $script:LastAnalysis = $null; $script:GameIndex = @{}
$session = [pscustomobject]@{ Id = 'x'; Key = 'jeuessai'; Avg = 90; Low1 = 30; Seconds = 600; Series = @()
    Diag = [pscustomobject]@{ CpuRatio = 0.9; GpuRatio = 0.5; Stutters = 200; Cpu = 80; Perf = 100; Ram = 95; Gpu = 50; Temp = 60; Vram = 50; Disk = 10
        OnBattery = $false; Ups = $false; Laptop = $false; Dual = $false; Nvidia = $false; Rtx = $false; Amd = $false; GameMode = $true; Hz = 144; HzMax = 144; Media = 'SSD'; Path = ''
        Top = @([pscustomobject]@{ Name = 'msedgewebview2'; Pct = 12 }, [pscustomobject]@{ Name = 'EAAntiCheat.GameService'; Pct = 8 }, [pscustomobject]@{ Name = 'Discord'; Pct = 6 })
        TopMem = @([pscustomobject]@{ Name = 'msedgewebview2'; MB = 3000 }, [pscustomobject]@{ Name = 'EAAntiCheat.GameService'; MB = 900 }, [pscustomobject]@{ Name = 'Discord'; MB = 800 }) } }
$diag = Get-FpsDiagnosis $session
$boutons = @($diag.Items | ForEach-Object { @($_.Actions) } | ForEach-Object { [string]$_.Label })
Test-Ok ($boutons -notcontains 'Fermer msedgewebview2') "Diagnostic : propose de fermer msedgewebview2 ($($boutons -join ', '))"
Test-Ok (-not @($boutons | Where-Object { $_ -match 'EAAntiCheat' }).Count) "Diagnostic : propose de fermer l'anti-triche ($($boutons -join ', '))"
Test-Ok ($boutons -contains 'Fermer Discord') "Diagnostic : ne propose plus de fermer un vrai programme en trop ($($boutons -join ', '))"

} catch { $echecs.Add("Erreur pendant le test : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))") }
finally { try { [IO.Directory]::Delete($tmp, $true) } catch {} }

# ---------------------------------------------------------------------------
# 6. La vraie [FrameMon] sur une très longue partie (natif.cs compilée dans un PowerShell à part, rien n'est lancé).
#    MaxImages abaissé à 1000 : la même chose qu'au delà de 2 millions d'images, en quelques secondes.
# ---------------------------------------------------------------------------
$essai = @'
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition ([IO.File]::ReadAllText($args[0], [Text.Encoding]::UTF8))
$ci = [Globalization.CultureInfo]::InvariantCulture
# (-Varier : une image plus courte, une plus longue, même moyenne ; comme un vrai jeu, ce n'est pas un palier de menu)
function Envoyer([double]$Ms, [int]$Nb, [switch]$Varier) { for ($i = 0; $i -lt $Nb; $i++) { $x = if (-not $Varier) { $Ms } elseif ($i % 2) { 0.9 * $Ms } else { 1.1 * $Ms }; [FrameMon]::Feed("jeu.exe,1,$($x.ToString($ci))") } }
[FrameMon]::Reset(); [FrameMon]::Paused = $false; [FrameMon]::MaxImages = 1000
[FrameMon]::Feed('Application,ProcessID,MsBetweenPresents')
Envoyer 10 5000 -Varier
Envoyer 20 5000 -Varier
$s = [FrameMon]::Summary(); $l = [FrameMon]::Live()
"gardees=$([FrameMon]::Frames)"
"pas=$([FrameMon]::Pas)"
"images=$([math]::Round($s[3]))"
"secondes=$([math]::Round($s[4], 1))"
"moyenne=$([math]::Round($s[0], 2))"
"direct=$([math]::Round($l[2], 2))"
# Paliers (menu bloqué à 60 FPS pendant 25 s) au milieu d'une partie à 200 FPS : la durée tient compte des images retirées
# (une image gardée sur deux, comme une vraie partie de 3 à 5 heures)
[FrameMon]::Reset(); [FrameMon]::MaxImages = 4000
[FrameMon]::Feed('Application,ProcessID,MsBetweenPresents')
for ($i = 0; $i -lt 3000; $i++) { [FrameMon]::Feed("jeu.exe,1,$(if ($i % 2) { '4.5' } else { '5.5' })") }
Envoyer (1000 / 60) 1500
for ($i = 0; $i -lt 3000; $i++) { [FrameMon]::Feed("jeu.exe,1,$(if ($i % 2) { '4.5' } else { '5.5' })") }
$p = [FrameMon]::Plateau(); $s = [FrameMon]::Summary()
"pas2=$([FrameMon]::Pas)"
"palier=$([math]::Round($p[0], 1))@$($p[1])"
"jeu=$([math]::Round($s[0]))"
# Bilan dans un autre fil, sur une copie : la mesure peut repartir de zéro entre temps
$tache = [FrameMon]::BilanAsync()
[FrameMon]::Reset()
[void]$tache.Wait(10000)
"async=$([math]::Round($tache.Result[0][0]))/$([math]::Round($tache.Result[1][0], 1))"
'@
$fEssai = Join-Path $env:TEMP "nevermind-test-fps-$PID.ps1"
[IO.File]::WriteAllText($fEssai, $essai, (New-Object Text.UTF8Encoding $true))
try { $sortie = @(& powershell -NoProfile -ExecutionPolicy Bypass -File $fEssai (Join-Path $modules 'natif.cs') 2>&1 | ForEach-Object { [string]$_ }) } finally { Remove-Item -LiteralPath $fEssai -Force }
$r = @{}
foreach ($ligne in $sortie) { if ($ligne -match '^(\w+)=(.*)$') { $r[$Matches[1]] = $Matches[2] } }
$ok = $r.Count -ge 10
Test-Ok $ok "Vraie FrameMon : sortie incomplète « $($sortie -join ' ') »"
if ($ok) {
    $ic = [Globalization.CultureInfo]::InvariantCulture
    $num = { param($k) [double]::Parse(($r[$k] -split '[@/]')[0], [Globalization.CultureInfo]::InvariantCulture) }
    Test-Ok ((& $num 'gardees') -le 1000) "Longue partie : $($r.gardees) images gardées en mémoire (1000 au plus)"
    Test-Ok ((& $num 'pas') -ge 16) "Longue partie : une image sur $($r.pas) gardée (attendu au moins 16)"
    Test-Ok ([math]::Abs((& $num 'images') - 10000) -le 2 * (& $num 'pas')) "Longue partie : $($r.images) images comptées (10 000 reçues)"
    Test-Ok ([math]::Abs((& $num 'secondes') - 150) -le 4) "Longue partie : $($r.secondes) s mesurées (150 attendues)"
    Test-Ok ([math]::Abs((& $num 'moyenne') - 66.67) -le 2) "Longue partie : $($r.moyenne) FPS en moyenne (66,7 attendus : la fin de partie ne doit pas être perdue)"
    Test-Ok ([math]::Abs((& $num 'direct') - 66.67) -le 0.01) "Longue partie : moyenne en direct $($r.direct) (66,67 attendus)"
    Test-Ok ($r.pas2 -eq '2') "Paliers : une image sur $($r.pas2) gardée (2 attendu)"
    $pal = $r.palier -split '@'
    Test-Ok ([math]::Abs([double]::Parse($pal[0], $ic) - 25) -le 4 -and [double]::Parse($pal[1], $ic) -eq 60) "Paliers après éclaircissement : $($r.palier) (25 s à 60 FPS attendues)"
    Test-Ok ([math]::Abs((& $num 'jeu') - 200) -le 5) "Paliers : $($r.jeu) FPS en jeu (200 attendus, menu retiré)"
    $as = $r.async -split '/'
    Test-Ok ([math]::Abs([double]::Parse($as[0], $ic) - 200) -le 5 -and [math]::Abs([double]::Parse($as[1], $ic) - 25) -le 4) "Bilan en arrière-plan : « $($r.async) » (200 FPS et 25 s de palier attendus, malgré la remise à zéro)"
}

# ---------------------------------------------------------------------------
# Résultat
# ---------------------------------------------------------------------------
if ($echecs.Count) {
    foreach ($e in $echecs) { Write-Host "ÉCHEC  $e" -ForegroundColor Red }
    Write-Host "$($echecs.Count) échec(s) sur $nbTests vérifications." -ForegroundColor Red
    exit 1
}
Write-Host "Mesure des FPS en jeu : $nbTests vérifications, tout passe." -ForegroundColor Green
exit 0
