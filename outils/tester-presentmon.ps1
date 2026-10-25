# Test du contrôle de PresentMon (issue #11), sans droits administrateur et sans jamais lancer PresentMon.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-presentmon.ps1
#
# PresentMon (Intel) peut tourner en administrateur : il n'est lancé que s'il est bien signé par Intel. L'Aide le vérifie
# avant chaque lancement (Get-AidePresentMon) ; quand elle ne répond pas, Nevermind le lance lui-même (Start-FpsMesure)
# et doit faire le même contrôle. Ce script vérifie :
#   1. que le repli de Start-FpsMesure accepte le vrai PresentMon du dossier de l'app, et refuse une copie modifiée
#      (un octet ajouté), un autre programme signé par Microsoft (notepad.exe copié) et un fichier absent ; un refus
#      ne lance rien, dit à l'utilisateur quoi faire et note la raison au journal ;
#   2. que ce repli bloque d'abord le fichier en lecture seule ([FrameMon]::Verrouiller), le contrôle ensuite, puis le
#      lance ou le relâche : pendant le contrôle, le fichier ne peut plus être remplacé (essayé sur les copies) ;
#   3. que chaque endroit des scripts qui lance PresentMon ([FrameMon]::Start, [AideFps]::Start ou Veiller) contrôle
#      la signature juste avant, dans la même fonction, et que [FrameMon]::Start est précédé de [FrameMon]::Verrouiller
#      avant ce contrôle ;
#   4. que la vraie [FrameMon] (natif.cs) refuse de lancer sans verrou, et que son verrou bloque vraiment le fichier.
# Les copies sont faites dans %TEMP% puis effacées. [FrameMon] est remplacé par une fausse version qui note les étapes
# et pose un vrai verrou en lecture seule, comme la vraie : seule la décision est vérifiée, rien n'est lancé.
# Code de sortie : 0 si tout passe, 1 sinon.

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$app     = Join-Path $racine 'Nevermind\fichiers'
$modules = Join-Path $app 'modules'
$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0
function Test-Ok([bool]$Condition, [string]$Message) {
    $script:nbTests++
    if (-not $Condition) { $script:echecs.Add($Message) }
}

# ---------------------------------------------------------------------------
# Le faux FrameMon : il note les demandes de lancement au lieu de lancer quoi que ce soit
# ---------------------------------------------------------------------------
Add-Type -TypeDefinition @'
public static class FrameMon {
    public static System.Collections.Generic.List<string> Lances = new System.Collections.Generic.List<string>();
    public static System.Collections.Generic.List<string> Etapes = new System.Collections.Generic.List<string>();
    public static string LastError = "";
    public static bool Attached;
    static System.IO.FileStream verrou;
    static string verrouExe = "";
    public static bool Verrouille { get { return verrou != null; } }
    public static bool StartRemote(int pid) { LastError = "Aide : demande refusée (essai)"; return false; }
    public static bool Verrouiller(string exe) {
        Etapes.Add("verrou");
        try { verrou = new System.IO.FileStream(exe, System.IO.FileMode.Open, System.IO.FileAccess.Read, System.IO.FileShare.Read); verrouExe = exe; return true; }
        catch (System.Exception ex) { LastError = ex.Message; return false; }
    }
    public static void Relacher() { Etapes.Add("relache"); Stop(); }
    public static void Stop() { if (verrou != null) { verrou.Dispose(); verrou = null; } verrouExe = ""; }
    // Comme la vraie : rien sans le verrou posé sur ce fichier
    public static bool Start(string exe, int pid) {
        Etapes.Add("lance");
        if (verrou == null || verrouExe != exe) { LastError = "pas verrouillé"; return false; }
        Lances.Add(exe); return true;
    }
}
'@

# ---------------------------------------------------------------------------
# 1. Le repli de Start-FpsMesure (aide-client.ps1), lu dans le module (rien ne se lance au chargement)
# ---------------------------------------------------------------------------
$tok = $null; $err = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules 'aide-client.ps1'), [ref]$tok, [ref]$err)
if ($err) { Write-Host "aide-client.ps1 n'est pas lisible : $($err[0].Message)" -ForegroundColor Red; exit 1 }
foreach ($n in 'Start-FpsMesure', 'Get-RaisonRefusPresentMon') {
    $def = $ast.Find({ param($x) $x -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $x.Name -eq $n }, $true)
    if (-not $def) { Write-Host "Fonction $n introuvable dans aide-client.ps1" -ForegroundColor Red; exit 1 }
    . ([scriptblock]::Create($def.Extent.Text))
}
# Le contrôle note s'il a lieu sur un fichier déjà bloqué, et essaie de remplacer les copies de %TEMP% à ce moment-là
$controleVrai = ${function:Get-RaisonRefusPresentMon}
function Get-RaisonRefusPresentMon([string]$Exe) {
    [FrameMon]::Etapes.Add("controle verrou=$([FrameMon]::Verrouille)")
    if ($Exe.StartsWith($dossier, [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $Exe)) {
        try { [IO.File]::Move($Exe, "$Exe.remplace"); [IO.File]::Move("$Exe.remplace", $Exe); [FrameMon]::Etapes.Add('remplacable') } catch { }
    }
    & $controleVrai $Exe
}
$script:Journal = New-Object System.Collections.ArrayList
$script:Statut = ''
function Write-Log([string]$T) { [void]$script:Journal.Add($T) }
function Set-Status([string]$T) { $script:Statut = $T }
$script:AideRepond = $false
function Test-MesuresAide { $script:AideRepond }

$vrai = Join-Path $app 'outils-tiers\PresentMon.exe'
if (-not (Test-Path -LiteralPath $vrai)) { Write-Host "PresentMon absent : $vrai" -ForegroundColor Red; exit 1 }
$dossier = Join-Path $env:TEMP "nevermind-test-presentmon-$PID"
[void][IO.Directory]::CreateDirectory($dossier)
try {
    $modifie = Join-Path $dossier 'modifie\PresentMon.exe'
    [void][IO.Directory]::CreateDirectory((Split-Path $modifie))
    [IO.File]::Copy($vrai, $modifie)
    $flux = [IO.File]::Open($modifie, 'Append'); try { $flux.WriteByte(0) } finally { $flux.Close() }
    $autre = Join-Path $dossier 'autre\PresentMon.exe'
    [void][IO.Directory]::CreateDirectory((Split-Path $autre))
    [IO.File]::Copy((Join-Path $env:windir 'notepad.exe'), $autre)
    $absent = Join-Path $dossier 'absent\PresentMon.exe'

    $message = 'L''outil de mesure des FPS a été modifié : Nevermind ne le lance pas. Réinstalle Nevermind depuis Mindset.'
    $cas = @(
        @{ Nom = 'vrai PresentMon (signé par Intel)'; Exe = $vrai; Accepte = $true }
        @{ Nom = 'copie modifiée (un octet ajouté)'; Exe = $modifie; Accepte = $false }
        @{ Nom = 'autre programme signé par Microsoft (notepad.exe)'; Exe = $autre; Accepte = $false }
        @{ Nom = 'fichier absent'; Exe = $absent; Accepte = $false }
    )
    # Sans l'Aide, puis avec une Aide qui refuse (le repli passe alors aussi par Nevermind)
    foreach ($aide in $false, $true) {
        $script:AideRepond = $aide
        $quand = if ($aide) { 'Aide qui refuse' } else { 'sans l''Aide' }
        foreach ($c in $cas) {
            [FrameMon]::Lances.Clear(); [FrameMon]::Etapes.Clear(); [FrameMon]::LastError = ''; $script:Journal.Clear(); $script:Statut = ''
            $script:PresentMonExe = $c.Exe
            $r = Start-FpsMesure 1234
            $lances = @([FrameMon]::Lances)
            $etapes = [FrameMon]::Etapes -join ', '
            $verrouReste = [FrameMon]::Verrouille
            [FrameMon]::Stop()
            # L'ordre : verrou, contrôle sur le fichier bloqué, puis lancement (gardé bloqué) ou relâche
            $attendu = if ($c.Accepte) { 'verrou, controle verrou=True, lance' } elseif ($c.Exe -eq $absent) { 'verrou, relache' } else { 'verrou, controle verrou=True, relache' }
            Test-Ok ($etapes -eq $attendu) "$($c.Nom), $quand : étapes « $etapes » au lieu de « $attendu »"
            Test-Ok ($verrouReste -eq [bool]$c.Accepte) "$($c.Nom), $quand : fichier encore bloqué après la décision : $verrouReste (attendu : $([bool]$c.Accepte))"
            if ($c.Accepte) {
                Test-Ok ($r -eq $true -and $lances.Count -eq 1 -and $lances[0] -eq $c.Exe) "$($c.Nom), $quand : devait être lancé (lancements : $($lances.Count))"
                Test-Ok (-not $script:Statut) "$($c.Nom), $quand : message inattendu : $($script:Statut)"
            } else {
                Test-Ok ($r -eq $false -and $lances.Count -eq 0) "$($c.Nom), $quand : lancé alors qu'il devait être refusé"
                Test-Ok ($script:Statut -eq $message) "$($c.Nom), $quand : message à l'utilisateur « $($script:Statut) »"
                Test-Ok (@($script:Journal | Where-Object { $_ -like '*PresentMon n''est pas lancé*' }).Count -eq 1) "$($c.Nom), $quand : raison absente du journal ($($script:Journal -join ' / '))"
                Test-Ok ([FrameMon]::LastError -like 'PresentMon refusé*') "$($c.Nom), $quand : LastError « $([FrameMon]::LastError) »"
            }
        }
    }
    # La raison donnée pour chaque refus
    Test-Ok ((Get-RaisonRefusPresentMon $modifie) -like 'signature non valable*') "Copie modifiée : raison « $(Get-RaisonRefusPresentMon $modifie) »"
    Test-Ok ((Get-RaisonRefusPresentMon $autre) -like '*Microsoft*' -or (Get-RaisonRefusPresentMon $autre) -like 'signature*') "notepad.exe : raison « $(Get-RaisonRefusPresentMon $autre) »"
    Test-Ok ($null -eq (Get-RaisonRefusPresentMon $vrai)) "Vrai PresentMon : refusé ($(Get-RaisonRefusPresentMon $vrai))"
} finally {
    [IO.Directory]::Delete($dossier, $true)
}

# ---------------------------------------------------------------------------
# 3. Aucun lancement de PresentMon sans contrôle : dans chaque script, chaque [FrameMon]::Start, [AideFps]::Start ou
#    [AideFps]::Veiller est précédé, dans la même fonction, du contrôle de la signature ; pour [FrameMon]::Start, ce
#    contrôle vient lui-même après [FrameMon]::Verrouiller
# ---------------------------------------------------------------------------
$controles = @{ 'FrameMon' = 'Get-RaisonRefusPresentMon'; 'AideFps' = 'Get-AidePresentMon' }
$nbLancements = 0
foreach ($f in @(Get-ChildItem -LiteralPath $app -Recurse -Filter '*.ps1')) {
    $tok = $null; $err = $null
    $a = [System.Management.Automation.Language.Parser]::ParseFile($f.FullName, [ref]$tok, [ref]$err)
    $appels = @($a.FindAll({ param($x) $x -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and $x.Static -and
        $x.Expression -is [System.Management.Automation.Language.TypeExpressionAst] -and $controles.ContainsKey($x.Expression.TypeName.Name) -and
        "$($x.Member)" -in 'Start', 'Veiller' }, $true))
    foreach ($appel in $appels) {
        $nbLancements++
        $controle = $controles[$appel.Expression.TypeName.Name]
        $p = $appel.Parent
        while ($p -and $p -isnot [System.Management.Automation.Language.FunctionDefinitionAst]) { $p = $p.Parent }
        $ou = "$($f.Name) ligne $($appel.Extent.StartLineNumber)"
        if (-not $p) { Test-Ok $false "$ou : PresentMon lancé hors d'une fonction"; continue }
        $avant = @($p.FindAll({ param($x) $x -is [System.Management.Automation.Language.CommandAst] -and $x.GetCommandName() -eq $controle -and
            $x.Extent.StartOffset -lt $appel.Extent.StartOffset }, $true))
        Test-Ok ($avant.Count -ge 1) "$ou : PresentMon lancé ([$($appel.Expression.TypeName.Name)]::$($appel.Member)) sans contrôle de sa signature ($controle) avant"
        if ($appel.Expression.TypeName.Name -eq 'FrameMon' -and $avant.Count) {
            $premierControle = ($avant | ForEach-Object { $_.Extent.StartOffset } | Measure-Object -Minimum).Minimum
            $verrous = @($p.FindAll({ param($x) $x -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and $x.Static -and
                $x.Expression -is [System.Management.Automation.Language.TypeExpressionAst] -and $x.Expression.TypeName.Name -eq 'FrameMon' -and
                "$($x.Member)" -eq 'Verrouiller' -and $x.Extent.StartOffset -lt $premierControle }, $true))
            Test-Ok ($verrous.Count -ge 1) "$ou : PresentMon contrôlé ($controle) sans avoir été bloqué avant ([FrameMon]::Verrouiller)"
        }
    }
}
Test-Ok ($nbLancements -ge 3) "Seulement $nbLancements lancements de PresentMon trouvés dans les scripts (au moins 3 attendus)"

# ---------------------------------------------------------------------------
# 4. La vraie [FrameMon] (natif.cs, compilée dans un PowerShell à part) : Start refuse sans Verrouiller, le verrou
#    empêche de déplacer ou d'effacer le fichier, Relacher le libère. Sur un faux fichier de %TEMP%, rien n'est lancé.
# ---------------------------------------------------------------------------
$essai = @'
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition ([IO.File]::ReadAllText($args[0], [Text.Encoding]::UTF8))
$d = Join-Path $env:TEMP "nevermind-test-verrou-$PID"; [void][IO.Directory]::CreateDirectory($d)
$x = Join-Path $d 'PresentMon.exe'; [IO.File]::WriteAllText($x, 'pas un programme')
try {
    "sans-verrou=$([FrameMon]::Start($x, 1))"
    "verrou=$([FrameMon]::Verrouiller($x))"
    try { [IO.File]::Move($x, "$x.b"); 'deplace=True' } catch { 'deplace=False' }
    try { [IO.File]::Delete($x) } catch { }
    "efface=$(-not (Test-Path -LiteralPath $x))"
    [FrameMon]::Relacher()
    try { [IO.File]::Move($x, "$x.b"); 'libre=True' } catch { 'libre=False' }
    "absent=$([FrameMon]::Verrouiller((Join-Path $d 'absent.exe')))"
    [FrameMon]::Stop()
} finally { [IO.Directory]::Delete($d, $true) }
'@
$fEssai = Join-Path $env:TEMP "nevermind-test-verrou-$PID.ps1"
[IO.File]::WriteAllText($fEssai, $essai, (New-Object Text.UTF8Encoding $true))
try { $sortie = @(& powershell -NoProfile -ExecutionPolicy Bypass -File $fEssai (Join-Path $modules 'natif.cs')) -join ' ' } finally { Remove-Item -LiteralPath $fEssai -Force }
Test-Ok ($sortie -eq 'sans-verrou=False verrou=True deplace=False efface=False libre=True absent=False') "Vraie FrameMon : « $sortie »"

# ---------------------------------------------------------------------------
# Résultat
# ---------------------------------------------------------------------------
if ($echecs.Count) {
    foreach ($e in $echecs) { Write-Host "ÉCHEC  $e" -ForegroundColor Red }
    Write-Host "$($echecs.Count) échec(s) sur $nbTests vérifications." -ForegroundColor Red
    exit 1
}
Write-Host "Contrôle de PresentMon : $nbTests vérifications, tout passe ($nbLancements lancements de PresentMon contrôlés dans les scripts)." -ForegroundColor Green
exit 0
