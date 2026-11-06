# Les juges des tests des composants de Nevermind en Rust (rust/nevermind-pc/tests/juges/tests-composants.json) : des PC
# simulés (ceux des juges du bloc pc, analyse.json) et des mesures inventées passés dans la version PowerShell (la
# référence, tests.ps1), et ce qu'elle en sort : le panneau du test tel que l'écran le reçoit (Get-PanneauKit, pendant
# et après le test), la page Tests des composants (Get-TuilesTestsKit), les messages et ce qui est demandé au moteur.
#
#   & 'outils\exporter-juges-tests.ps1' [-Nombre 300] [-Graine 4242] [-ParPc 3]
#
# Rien n'est mesuré : le travail des tests ([OGNative]::DiskTest, CpuTest...) est remplacé par des résultats inventés,
# donnés par une suite de « tics » (phase, progression, valeur en direct) comme la minuterie de 100 ms de tests.ps1.
# Les objets WPF sont créés en mémoire, jamais montrés : aucune fenêtre ne s'ouvre, rien ne touche le PC.
param([int]$Nombre = 300, [int]$Graine = 4242, [int]$ParPc = 3)

$ErrorActionPreference = 'Stop'
$env:NEVERMIND_TEST = '1'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
$sortie  = Join-Path $racine 'rust\nevermind-pc\tests\juges'
Add-Type -AssemblyName PresentationCore, PresentationFramework, WindowsBase

# ---------------------------------------------------------------------------
# Sécurité : ce qui agirait sur le PC est interdit (une fonction passe avant une commande du même nom)
# ---------------------------------------------------------------------------
foreach ($c in 'Repair-Volume', 'Get-PhysicalDisk', 'Get-StorageReliabilityCounter', 'Stop-Process', 'Invoke-RestMethod', 'Invoke-WebRequest') {
    Set-Item -Path "function:script:$c" -Value ([scriptblock]::Create("throw 'Juges : $c est interdit.'"))
}

# ---------------------------------------------------------------------------
# Les fonctions et variables des modules
# ---------------------------------------------------------------------------
$asts = @{}
foreach ($f in 'tests', 'visuels', 'interface', 'themes', 'ecran-panneau', 'ecran-divers', 'ecran-pc', 'systeme', 'analyse') {
    $tok = $null; $err = $null
    $asts[$f] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$f.ps1"), [ref]$tok, [ref]$err)
    if ($err) { throw "$f.ps1 n'est pas lisible : $($err[0].Message)" }
}
function Get-FonctionAst([string]$Fichier, [string]$Nom) {
    $asts[$Fichier].Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $false)
}
# Le moteur des tests ([OGNative] de natif.cs) : un faux, rempli par les tics
function ConvertTo-SansNatif([string]$Texte) {
    $Texte.Replace('[OGNative]::', '$script:OG.').Replace('[System.Windows.Forms.Screen]::AllScreens', '$script:FauxEcrans')
}
$aCharger = [ordered]@{
    'tests'         = 'New-TestTile', 'Add-TestButton', 'Set-TileSummary', 'Set-TestState', 'Show-TestPanel', 'Hide-TestPanel', 'Set-TestButtons', 'Invoke-ComponentTest',
                      'Show-LastResult', 'Get-GHz', 'Test-DiskSpeed', 'Show-DiskHealth', 'Test-DiskErrors', 'Test-Cpu', 'Test-Memory', 'Show-GpuMonitor', 'Test-NetSpeed',
                      'Get-BloatGrade', 'New-GradeTile', 'Test-Bufferbloat', 'Build-TestsTab'
    'visuels'       = 'Get-ArcGeometry', 'Get-Color', 'Get-LightHex', 'New-AlphaBrush', 'Get-SmoothFigure', 'New-Gauge', 'Update-GaugeVisual', 'Set-GaugeLive', 'New-GaugeRow',
                      'New-LiveChart', 'Add-ChartPoint', 'Update-Chart', 'New-CompareBars', 'New-StatTile', 'New-StatRow', 'New-Verdict', 'New-SectionTitle', 'New-Details',
                      'New-Stepper', 'Update-Stepper', 'New-LinearBrush', 'New-LoaderArc', 'New-Glow'
    'interface'     = 'Get-Brush', 'New-Thickness', 'New-Text', 'New-Card', 'New-Grid', 'Add-ToGrid', 'New-Button'
    'themes'        = 'Get-UiFontSize'
    'ecran-panneau' = 'Get-TexteWpf', 'Get-ContenuWpf', 'Get-ImageWpf', 'Test-DessinWpf', 'Test-JaugeWpf', 'Get-ChiffreJaugeWpf', 'Add-BlocKitWpf', 'Add-BlocsWpf',
                      'Get-TextesWpf', 'Test-ControlesWpf', 'Get-PanneauKit', 'Invoke-ActionPanneau'
    'ecran-divers'  = 'Get-TuilesTestsKit', 'Invoke-ActionDivers'
    'ecran-pc'      = 'Get-TonKit'
    'systeme'       = 'Format-Size'
    'analyse'       = 'Get-LoadColor'
}
foreach ($f in $aCharger.Keys) {
    foreach ($n in $aCharger[$f]) {
        $def = Get-FonctionAst $f $n
        if (-not $def) { throw "Fonction $n introuvable dans $f.ps1" }
        $texte = (ConvertTo-SansNatif $def.Extent.Text).Replace('-or -not $b.IsVisible', '-or $b.Visibility -ne ''Visible''')
        # (l'ancien fichier d'essai d'un test interrompu : noté, jamais cherché ni effacé sur le vrai PC)
        $texte = $texte.Replace('$ancien = Join-Path (Split-Path $file -Parent) ''OptiGame-test-disque.tmp''', '$ancien = [IO.Path]::Combine([IO.Path]::GetDirectoryName($file), ''OptiGame-test-disque.tmp'')').Replace('if (Test-Path -LiteralPath $ancien) { try { [IO.File]::Delete($ancien) } catch {} }', '[void]$script:Gestes.Add("effacer $ancien")')
        if ($n -eq 'Test-DiskSpeed' -and $texte -notmatch 'Gestes.Add') { throw 'Test-DiskSpeed : l''ancien fichier n''est pas remplacé' }
        . ([scriptblock]::Create($texte))
    }
}
$variables = [ordered]@{ 'interface' = 'Colors'; 'ecran-panneau' = 'IconesPanneau'; 'ecran-divers' = 'IconesTests'; 'visuels' = 'NexoCyan' }
foreach ($f in $variables.Keys) {
    foreach ($aff in @($asts[$f].EndBlock.Statements | Where-Object { $_ -is [System.Management.Automation.Language.AssignmentStatementAst] })) {
        $v = $aff.Left; if ($v -is [System.Management.Automation.Language.ConvertExpressionAst]) { $v = $v.Child }
        if ($v -is [System.Management.Automation.Language.VariableExpressionAst] -and $variables[$f] -contains $v.VariablePath.UserPath) { . ([scriptblock]::Create($aff.Extent.Text)) }
    }
}
foreach ($n in 'Colors', 'IconesPanneau', 'IconesTests', 'NexoCyan') { if ($null -eq (Get-Variable $n -ValueOnly -ErrorAction SilentlyContinue)) { throw "Variable `$$n introuvable" } }
# La minuterie de 100 ms des tests (son corps, lu dans tests.ps1) et la page Tests (lue dans Get-PagesDivers)
$tic = $asts['tests'].Find({ param($n) $n -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and $n.Member.Value -eq 'Add_Tick' -and $n.Expression.Extent.Text -eq '$script:TestTimer' }, $true)
if (-not $tic) { throw 'Minuterie des tests introuvable' }
$script:Tic = [scriptblock]::Create((ConvertTo-SansNatif $tic.Arguments[0].ScriptBlock.EndBlock.Extent.Text))
$pagesDivers = Get-FonctionAst 'ecran-divers' 'Get-PagesDivers'
$affTests = $pagesDivers.Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$pages.tests' }, $true)
$lectureTests = $pagesDivers.Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$tt' }, $true)
$script:PageTests = [scriptblock]::Create("$($lectureTests.Extent.Text)`n`$pages = @{}`n$($affTests.Extent.Text)`n`$pages.tests")
# (la surveillance de la carte graphique, toutes les 500 ms : son corps, lu dans Show-GpuMonitor)
$ticGpu = (Get-FonctionAst 'tests' 'Show-GpuMonitor').Find({ param($n) $n -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and $n.Member.Value -eq 'Add_Tick' }, $true)
$script:TicGpu = [scriptblock]::Create($ticGpu.Arguments[0].ScriptBlock.EndBlock.Extent.Text)

# ---------------------------------------------------------------------------
# Le faux décor : fenêtre, thème, animations (finies tout de suite), messages
# ---------------------------------------------------------------------------
function ConvertTo-ThemeHex([string]$Hex) { $Hex }
function Set-PackFont { }
$PackSizeAll = $false; $MonoFont = 'Consolas'
function Start-WpfAnim { }
function Start-FromTo { }
function Start-Pulse { }
function Stop-Pulse { }
function Start-Anim([scriptblock]$Step, $State, [int]$Ms = 1100, [int]$Delay = 0) { & $Step 1.0 $State }   # (l'état final, celui que l'écran montre)
function Show-ResultTop { }
function Update-EcranNevermind { }
$fond = New-Object System.Windows.Media.LinearGradientBrush
$styles = @{ CardBg = $fond; CardHoverBg = $fond; CardBorder = $fond; Card = (New-Object System.Windows.Style ([System.Windows.Controls.Border]))
    BtnPrimary = (New-Object System.Windows.Style ([System.Windows.Controls.Button])); BtnSecondary = (New-Object System.Windows.Style ([System.Windows.Controls.Button])) }
$Window = New-Object psobject
$Window | Add-Member ScriptMethod FindResource { param($k) $styles[$k] }
$Window | Add-Member ScriptMethod TryFindResource { param($k) $styles[$k] }
$script:Messages = New-Object System.Collections.ArrayList
$script:Journal = New-Object System.Collections.ArrayList
$script:Gestes = New-Object System.Collections.ArrayList
$script:Statut = ''
function Show-Message([string]$Text) { [void]$script:Messages.Add($Text) }
function Set-Status([string]$Text) { $script:Statut = $Text }
function Write-Log([string]$Message) { [void]$script:Journal.Add($Message) }
function Invoke-Safe([scriptblock]$Sb) { & $Sb }
function Open-Url([string]$U) { [void]$script:Gestes.Add("ouvrir $U") }
function Start-Process($FilePath, $ArgumentList, [switch]$Wait, $WindowStyle) { [void]$script:Gestes.Add("lancer $FilePath $(@($ArgumentList) -join ' ')".Trim()) }
function Start-PixelTest([int]$Index) { [void]$script:Gestes.Add("pixels $Index") }
function Test-3DMark { $script:Faux3DMark }
function Get-VolInfo([string]$Letter) { $v = $script:FauxVols[$Letter]; if ($v) { [pscustomobject]@{ Size = [double]$v.Size; SizeRemaining = [double]$v.SizeRemaining } } }
function Get-AideFiabiliteDisque([string]$Id) { $script:FauxRelAide }
function Get-FauxSmi { $script:FauxSmi }
$SmiPath = $null
$DiskWork = { 'disque' }; $CpuWork = { 'cpu' }; $MemWork = { 'memoire' }; $NetWork = { 'reseau' }; $BloatWork = { 'lag' }; $RepairWork = { 'erreurs' }; $DiskInfoWork = { 'sante' }
$script:TestTimer = New-Object psobject
$script:TestTimer | Add-Member ScriptMethod Start { }
$script:TestTimer | Add-Member ScriptMethod Stop { }

# Ce que le test demande au moteur, et les tics pendant qu'il tourne (avec le panneau lu au passage)
$script:Demande = $null
$script:Pendant = New-Object System.Collections.ArrayList
function Invoke-Async($Sb, $A) {
    $nom = & $Sb
    if ($nom -eq 'sante') { $script:Demande = [ordered]@{ travail = 'sante'; arg = [string]$A }; return $script:FauxSante }
    $script:Demande = [ordered]@{ travail = $nom; arg = $A }
    foreach ($t in @($script:Tics)) {
        $script:OG.Phase = [string]$t.phase; $script:OG.Progress = [double]$t.progres; $script:OG.LiveValue = [double]$t.direct
        $Live.CpuPerf = $t.perf
        if ($t.nouveau) { $Live.Updated = [int]$Live.Updated + 1 }
        & $script:Tic
        if ($t.lire) { [void]$script:Pendant.Add((Get-PanneauKit)) }
    }
    if ($script:Stop) { $script:OG.Cancel = $true }
    $script:Resultat
}

function New-Ui {
    $u = @{}
    foreach ($n in 'TestsPanel', 'TestBody') { $u[$n] = New-Object System.Windows.Controls.StackPanel }
    foreach ($n in 'TestTag', 'TestTitle', 'TestSub', 'TestPct', 'TestStateText') { $u[$n] = New-Object System.Windows.Controls.TextBlock }
    $u.TestStateDot = New-Object System.Windows.Shapes.Ellipse
    $u.TestProgress = New-Object System.Windows.Controls.ProgressBar
    $u.TestOverlay = New-Object System.Windows.Controls.Grid; $u.TestOverlay.Visibility = 'Collapsed'
    $u.TestCard = New-Object System.Windows.Controls.Border
    $u.TestCardScale = New-Object System.Windows.Media.ScaleTransform
    $u.TestScroll = New-Object System.Windows.Controls.ScrollViewer
    foreach ($p in @(@('BtnTestStop', 'Arrêter le test', 'BtnSecondary'), @('BtnTestClose', 'Fermer', 'BtnPrimary'), @('BtnTestAgain', 'Refaire le test', 'BtnSecondary'), @('BtnTestX', '✕', 'BtnSecondary'))) {
        $b = New-Object System.Windows.Controls.Button; $b.Content = $p[1]; $b.Style = $styles[$p[2]]; $u[$p[0]] = $b
    }
    $u.BtnTestStop.Visibility = 'Collapsed'; $u.BtnTestAgain.Visibility = 'Collapsed'
    $u.BtnTestStop.Add_Click({ $script:OG.Cancel = $true; Set-TestState 'info' 'Arrêt en cours...' })
    $u.BtnTestClose.Add_Click({ Hide-TestPanel })
    $u.BtnTestAgain.Add_Click({ $run = $script:LastRun; if ($run -and $run.Fn) { Invoke-Safe { & $run.Fn $run.Tile $run.Ctx } } })
    $u
}
function Invoke-BoutonKit([string]$Nom) { $b = $ui[$Nom]; $b.RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Primitives.ButtonBase]::ClickEvent))) }

# ---------------------------------------------------------------------------
# Les données inventées
# ---------------------------------------------------------------------------
$rnd = New-Object System.Random $Graine
function Get-Hasard([int]$Max) { $rnd.Next($Max) }
function Test-Chance([double]$P) { $rnd.NextDouble() -lt $P }
function Get-Reel([double]$Min, [double]$Max, [int]$Dec = 2) { [math]::Round($Min + ($Max - $Min) * $rnd.NextDouble(), $Dec) }
function Get-Choix($Liste) { $Liste[$rnd.Next($Liste.Count)] }

$pcs = [IO.File]::ReadAllText((Join-Path $sortie 'analyse.json'), [Text.Encoding]::UTF8) | ConvertFrom-Json
$Phases = @{ disque = @('write', 'read', 'random'); cpu = @('single', 'multi'); memoire = @('alloc', 'write', 'read', 'pattern', 'copy'); reseau = @('ping', 'down', 'up'); lag = @('idle', 'down', 'up'); erreurs = @('scan') }

# Une valeur parfois étrange (zéro, négative, énorme)
function Get-Mesure([double]$Min, [double]$Max) {
    $x = $rnd.NextDouble()
    if ($x -lt 0.05) { return 0.0 }
    if ($x -lt 0.08) { return -1.0 }
    if ($x -lt 0.11) { return (Get-Reel ($Max * 2) ($Max * 20) 1) }
    Get-Reel $Min $Max 2
}
function New-Tics([string]$Travail) {
    $ph = @($Phases[$Travail]); if (-not $ph) { $ph = @('') }
    $n = 1 + (Get-Hasard 40)
    $l = @()
    for ($i = 0; $i -lt $n; $i++) {
        $p = $ph[[math]::Min($ph.Count - 1, [int][math]::Floor($ph.Count * $i / $n))]
        if (Test-Chance 0.05) { $p = '' }
        $l += [ordered]@{ phase = $p; progres = (Get-Reel 0 (100 * ($i + 1) / $n + 5) 1); direct = (Get-Reel 0 9000 2)
            perf = $(if (Test-Chance 0.1) { $null } else { Get-Reel 20 160 1 }); nouveau = (Test-Chance 0.6); lire = (Test-Chance 0.12) }
    }
    $l
}
function New-Resultat([string]$Travail, $Arg) {
    if (Test-Chance 0.06) { return @{ Cancelled = $true } }
    if (Test-Chance 0.05) { return @{ Error = (Get-Choix @('Accès refusé.', 'Le disque est plein.', "L'opération a expiré", 'Mémoire insuffisante pour le test.')) } }
    if (Test-Chance 0.02) { return $null }
    switch ($Travail) {
        'disque'  { @{ R = @((Get-Mesure 20 7500), (Get-Mesure 20 7500), (Get-Mesure 1 90), (Get-Mesure 100 30000)) } }
        'cpu'     { @{ R = @((Get-Mesure 50 2500), (Get-Mesure 200 60000), $(if (Test-Chance 0.12) { [double](1 + (Get-Hasard 30)) } else { 0.0 }), [double](Get-Choix @(1, 2, 4, 6, 8, 12, 16, 20, 24, 32))) } }
        'memoire' { @{ R = @((Get-Mesure 2 90), (Get-Mesure 2 120), (Get-Mesure 2 60), $(if (Test-Chance 0.1) { [double](Get-Hasard 5000) } else { 0.0 }), (Get-Reel 0.25 2 3)) } }
        'reseau'  { @{ R = @($(if (Test-Chance 0.1) { -1.0 } else { Get-Reel 1 180 1 }), $(if (Test-Chance 0.1) { -1.0 } else { Get-Mesure 0.5 2400 }), $(if (Test-Chance 0.1) { -1.0 } else { Get-Mesure 0.5 900 }), [double](Get-Hasard 11)) } }
        'lag'     { @{ R = @($(if (Test-Chance 0.08) { -1.0 } else { Get-Reel 1 90 1 }), (Get-Reel 1 700 1), (Get-Reel 1 700 1), (Get-Reel 0 30 1), (Get-Reel 0 30 1), (Get-Reel 0 30 1), $(if (Test-Chance 0.1) { -1.0 } else { Get-Reel 1 2000 1 }), (Get-Reel -1 900 1), (Get-Reel 0 25 2)) } }
        'erreurs' { @{ R = @(@($Arg.Letters) | ForEach-Object { "$_|$(Get-Choix @('NoErrorsFound', 'NoErrorsFound', 'ScanNeeded', 'SpotFixNeeded', 'ERR Accès refusé.', 'ERR Le paramètre est incorrect.'))" }) } }
    }
}
function New-Rel {
    if (Test-Chance 0.3) { return $null }
    $r = [ordered]@{}
    foreach ($k in 'Wear', 'Temperature', 'TemperatureMax', 'PowerOnHours', 'ReadErrorsUncorrected', 'WriteErrorsUncorrected', 'ReadErrorsCorrected', 'StartStopCycleCount', 'ReadLatencyMax', 'WriteLatencyMax') {
        $r[$k] = if (Test-Chance 0.25) { $null } else {
            switch ($k) {
                'Wear' { Get-Hasard 101 } 'Temperature' { Get-Choix @(0, 25, 38, 49, 52, 65, 70, 74, 85) } 'TemperatureMax' { Get-Choix @(0, 70, 84) }
                'PowerOnHours' { Get-Choix @(0, 12, 4321, 25000) } 'ReadErrorsUncorrected' { Get-Choix @(0, 0, 0, 3) } 'WriteErrorsUncorrected' { Get-Choix @(0, 0, 2) }
                'ReadErrorsCorrected' { Get-Choix @(0, 17, 123456) } 'StartStopCycleCount' { Get-Choix @(0, 88, 3021) } default { Get-Choix @(0, 4, 1250, 33.5) } }
        }
    }
    $r
}
function New-Smi {
    if (Test-Chance 0.15) { return $null }
    $bits = Get-Choix @('0x0000000000000000', '0x0000000000000001', '0x0000000000000004', '0x0000000000000020', '0x0000000000000040', '0x0000000000000008', '0x0000000000000080', '0x00000000000000E4', 'abc')
    "566.03, 94.06.2F.00.9A, P$(Get-Hasard 9), $(Get-Hasard 2800), 2805, $(Get-Hasard 10000), 10501, $(Get-Hasard 90), $(Get-Choix @('0', '35', '[N/A]')), $(Get-Reel 5 400 2), $(Get-Choix @('170.00', '[N/A]', '30.00')), $(Get-Hasard 5), 4, $(Get-Choix @(8, 16)), 16, $(Get-Hasard 100), $(Get-Hasard 12000), $(Get-Choix @(8192, 12288, 24564)), $bits"
}
# Les valeurs en texte des cases d'un juge : comme ConvertTo-Json les écrit
function ConvertTo-Juge($O) { if ($null -eq $O) { return $null }; ConvertFrom-Json (ConvertTo-Json -InputObject $O -Depth 20 -Compress) }

# Un pas : le panneau, la page Tests, les messages, ce qui est demandé
function Get-Sortie {
    $p = if ($ui.TestOverlay.Visibility -eq 'Visible') { Get-PanneauKit } else { $null }
    if ($p) { $p.Remove('parent'); $p.Remove('retour') }
    [ordered]@{ panneau = $p; pendant = @($script:Pendant); tests = (& $script:PageTests); messages = @($script:Messages); journal = @($script:Journal); demande = $script:Demande; gestes = @($script:Gestes) }
}
function Clear-Sortie { $script:Pendant.Clear(); $script:Messages.Clear(); $script:Journal.Clear(); $script:Gestes.Clear(); $script:Demande = $null }

# ---------------------------------------------------------------------------
# Les cas
# ---------------------------------------------------------------------------
$cas = New-Object System.Collections.ArrayList
$origSys = $env:SystemDrive; $origTemp = $env:TEMP
$env:SystemDrive = 'C:'; $env:TEMP = 'C:\Users\Essai\AppData\Local\Temp'
try {
for ($k = 0; $k -lt [math]::Min($Nombre, $pcs.Count); $k++) {
    $pc = $pcs[$k].entree
    for ($rep = 0; $rep -lt $ParPc; $rep++) {
        $ui = New-Ui
        $script:Kit = @{}
        $script:OG = @{ Cancel = $false; Progress = 0.0; Phase = ''; LiveValue = 0.0 }
        $script:CurTest = $null; $script:TestRunning = $false; $script:LastRun = $null; $script:MonitorTimer = $null; $script:GpuLive = $null
        $script:TestButtons = New-Object System.Collections.ArrayList
        $script:AnalysisData = $pc.data
        $script:IsLaptop = Test-Chance 0.5
        $script:Faux3DMark = Test-Chance 0.2
        $script:FauxEcrans = @(foreach ($e in @($pc.ecrans)) { [pscustomobject]@{ Primary = (([string]$e -split '\|')[6] -eq '1') } })
        $smi = Test-Chance 0.7
        $SmiPath = if ($smi) { 'Get-FauxSmi' } else { $null }
        $direct0 = [ordered]@{ BaseMHz = [int]$pc.data.CPU.MaxClockSpeed; RamTotal = $(if (Test-Chance 0.85) { [double](Get-Choix @(8, 16, 32, 64)) * 1GB } else { $null }); RamUsed = [double](Get-Reel 1 7 2) * 1GB
            Gpu = (Get-Reel 0 100 1); GpuTemp = $(if (Test-Chance 0.8) { Get-Reel 30 90 1 } else { $null }); GpuPower = (Get-Reel 5 350 1); Updated = 1; CpuPerf = $null; Fast = $false }
        $Live = @{}; foreach ($x in $direct0.Keys) { $Live[$x] = $direct0[$x] }   # ($Live et $live : la même variable pour PowerShell)
        $etapes = New-Object System.Collections.ArrayList
        Clear-Sortie
        Build-TestsTab
        [void]$etapes.Add([ordered]@{ geste = $null; sortie = (Get-Sortie) })
        $n = 1 + (Get-Hasard 5)
        for ($e = 0; $e -lt $n; $e++) {
            Clear-Sortie
            [void](& $script:PageTests)
            $boutons = @($script:Kit.TestsBoutons)
            $panneauOuvert = $ui.TestOverlay.Visibility -eq 'Visible'
            # Le geste : un bouton d'une tuile, ou un bouton du panneau ouvert
            $choix = if ($panneauOuvert -and (Test-Chance 0.6)) { Get-Choix @('panneau:again', 'panneau:close', 'panneau:b:0', 'panneau:b:0', 'panneau:b:1', 'panneau:stop', 'mesures') } else { "test-b:$(Get-Hasard ([math]::Max(1, $boutons.Count)))" }
            $mesure = [ordered]@{}
            # (ce que le prochain test trouvera : mesures inventées, volumes, disque, carte NVIDIA)
            $vols = @{}; foreach ($dd in @($pc.data.Disks)) { foreach ($v in @($dd.Vols)) { if ($v.Vol) { $vols[[string]$v.DriveLetter] = @{ Size = [double]$v.Vol.Size; SizeRemaining = $(if (Test-Chance 0.15) { Get-Choix @(0.5GB, 5GB, 20GB, 50GB) } else { [double]$v.Vol.SizeRemaining }) } } } }
            $script:FauxVols = $vols
            $script:FauxRelAide = New-Rel
            $dd0 = @($pc.data.Disks)[(Get-Hasard ([math]::Max(1, @($pc.data.Disks).Count)))]
            $script:FauxSante = if (Test-Chance 0.07) { @{ D = $null; Rel = $null } } else {
                @{ D = [pscustomobject]@{ DeviceId = [string]$dd0.Disk.DeviceId; FriendlyName = "  $($dd0.Disk.FriendlyName) "; MediaType = [string]$dd0.Disk.MediaType; BusType = [string]$dd0.Disk.BusType
                    HealthStatus = (Get-Choix @('Healthy', 'Healthy', 'Warning', 'Unhealthy', 'Unknown')); Size = [uint64]$dd0.Disk.Size; SerialNumber = (Get-Choix @('  E823_8FA6_BF53_0001_001B_448B_4A2F_1C3E.', 'S5H2NS0N123456', '')); FirmwareVersion = (Get-Choix @('EXF7201Q', '1.0', '')) }
                   Rel = $(if (Test-Chance 0.1) { New-Rel } else { $null }) }
            }
            $script:FauxSmi = New-Smi
            $script:Stop = Test-Chance 0.05
            $trav = $null
            $b = if ($choix -like 'test-b:*') { $boutons[[int]($choix -replace '^test-b:', '')] } else { $null }
            $txt = if ($b) { Get-ContenuWpf $b } else { '' }
            $trav = switch -Regex ($txt) { '^Tester la vitesse' { 'disque' } '^Erreurs' { 'erreurs' } '^Test rapide|^Stabilité' { 'cpu' } '^Tester la mémoire' { 'memoire' } '^Tester ma connexion' { 'reseau' } '^Lag en charge' { 'lag' } default { $null } }
            if ($choix -eq 'panneau:again' -and $script:LastRun) { $trav = switch ([string]$script:LastRun.Fn) { 'Test-DiskSpeed' { 'disque' } 'Test-DiskErrors' { 'erreurs' } 'Test-Cpu' { 'cpu' } 'Test-Memory' { 'memoire' } 'Test-NetSpeed' { 'reseau' } 'Test-Bufferbloat' { 'lag' } default { $null } } }
            if ($txt -eq 'Voir le résultat' -or ($trav -and $txt -ne 'Erreurs')) { }
            $script:Tics = if ($trav) { @(New-Tics $trav) } else { @() }
            $argErreurs = if ($trav -eq 'erreurs') { @{ Letters = @($b.Tag.Ctx.Letters) } } elseif ($script:LastRun -and $script:LastRun.Ctx) { @{ Letters = @($script:LastRun.Ctx.Letters) } } else { @{ Letters = @() } }
            $script:Resultat = if ($trav) { New-Resultat $trav $argErreurs } else { $null }
            if ($choix -eq 'mesures') {
                $Live.Gpu = Get-Reel 0 100 1; $Live.GpuTemp = $(if (Test-Chance 0.8) { Get-Reel 30 90 1 } else { $null }); $Live.GpuPower = Get-Reel 5 350 1; $Live.Updated = [int]$Live.Updated + 1
                if ($script:MonitorTimer) { & $script:TicGpu }
            } elseif ($choix -like 'test-b:*') {
                if ($b -and $b.IsEnabled) { [void](Invoke-ActionDivers $choix $null) }
            } elseif ($choix -match '^panneau:b:') {
                [void](Get-PanneauKit); [void](Invoke-ActionPanneau $choix $true)
            } elseif ($ui.TestOverlay.Visibility -eq 'Visible') {
                switch ($choix) { 'panneau:again' { if ($ui.BtnTestAgain.Visibility -eq 'Visible') { Invoke-BoutonKit 'BtnTestAgain' } } 'panneau:close' { if ($ui.BtnTestClose.IsEnabled) { Invoke-BoutonKit 'BtnTestClose' } } 'panneau:stop' { if ($ui.BtnTestStop.Visibility -eq 'Visible') { Invoke-BoutonKit 'BtnTestStop' } } }
            }
            $mesure = [ordered]@{ tics = @($script:Tics); resultat = $script:Resultat; stop = [bool]$script:Stop; vols = $vols; sante = $script:FauxSante; relAide = $script:FauxRelAide; smi = $script:FauxSmi
                live = [ordered]@{ Gpu = $Live.Gpu; GpuTemp = $Live.GpuTemp; GpuPower = $Live.GpuPower } }
            [void]$etapes.Add([ordered]@{ geste = $choix; mesure = $mesure; sortie = (Get-Sortie) })
        }
        [void]$cas.Add([ordered]@{ numero = $cas.Count; entree = [ordered]@{ pc = $k; ecrans = @($pc.ecrans); portable = [bool]$script:IsLaptop; troisdmark = [bool]$script:Faux3DMark; smi = $smi; live = $direct0
            systemdrive = $env:SystemDrive; temp = $env:TEMP }; etapes = @($etapes) })
    }
}
} finally { $env:SystemDrive = $origSys; $env:TEMP = $origTemp }

$json = ConvertTo-Json -InputObject @($cas) -Depth 40 -Compress
[IO.File]::WriteAllText((Join-Path $sortie 'tests-composants.json'), $json, (New-Object Text.UTF8Encoding($false)))
"Juges des tests des composants : $($cas.Count) cas, $(($cas | ForEach-Object { $_.etapes.Count } | Measure-Object -Sum).Sum) étapes."
