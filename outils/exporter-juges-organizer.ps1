# Exporte les « juges » de l'Organizer de Nevermind en Rust : la version PowerShell (fichiers\modules\organizer.ps1,
# la référence) est chargée fonction par fonction, en mode test. Les fenêtres du jeu, la fenêtre devant, les touches
# prises par Windows et l'écoute de la souris sont FAUSSES (classes WinFocus, OGNative et MouseHook d'essai) : rien
# du vrai PC n'est lu ni touché, aucun raccourci n'est pris à Windows, aucune fenêtre ne s'ouvre.
#
#   & 'outils\exporter-juges-organizer.ps1'            (depuis l'outil PowerShell, jamais depuis Bash)
#
# Écrit rust\nevermind-jeux\tests\juges\organizer-*.json : chaque cas donne l'entrée et la sortie de la version
# PowerShell. Les tests Rust (tests\organizer.rs) rejouent les mêmes entrées et doivent donner exactement la même sortie.
# Les scénarios de la veille sont tirés au hasard (graine fixe, -Graine pour en tirer d'autres jamais vus).
param([int]$Graine = 20261106, [int]$Scenarios = 400, [string]$Sortie)

$ErrorActionPreference = 'Stop'
$env:NEVERMIND_TEST = '1'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
if (-not $Sortie) { $Sortie = Join-Path $racine 'rust\nevermind-jeux\tests\juges' }
New-Item -ItemType Directory -Force -Path $Sortie | Out-Null
Add-Type -AssemblyName PresentationCore, WindowsBase

# ---------------------------------------------------------------------------
# Les fonctions de la version PowerShell, lues dans les modules (rien ne se lance au chargement)
# ---------------------------------------------------------------------------
$asts = @{}
function Get-Ast([string]$Fichier) {
    if (-not $asts.ContainsKey($Fichier)) {
        $tok = $null; $err = $null
        $asts[$Fichier] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$Fichier.ps1"), [ref]$tok, [ref]$err)
        if ($err) { throw "$Fichier.ps1 illisible : $($err[0].Message)" }
    }
    $asts[$Fichier]
}
function Get-TexteFonction([string]$Fichier, [string]$Nom) {
    $a = (Get-Ast $Fichier).Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $false)
    if (-not $a) { throw "Fonction $Nom introuvable dans $Fichier.ps1" }
    $a.Extent.Text -replace "^function\s+$([regex]::Escape($Nom))", "function script:$Nom"
}
function Import-Fonction([string]$Fichier, [string]$Nom, [scriptblock]$Retouche) {
    $t = Get-TexteFonction $Fichier $Nom
    if ($Retouche) {
        $avant = $t
        $t = & $Retouche $t
        if ($t -eq $avant) { throw "Retouche sans effet sur $Nom" }
    }
    Invoke-Expression $t
}
function Import-Variable([string]$Fichier, [string]$Nom) {
    $a = (Get-Ast $Fichier).Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq "`$$Nom" }, $false)
    if (-not $a) { throw "Variable $Nom introuvable dans $Fichier.ps1" }
    Invoke-Expression ($a.Extent.Text -replace "^\`$$Nom", "`$script:$Nom")
}

# ---------------------------------------------------------------------------
# Le faux Windows : fenêtre devant, programme d'une fenêtre, mise au premier plan, touches prises, souris
# ---------------------------------------------------------------------------
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
public static class Evenements { public static List<string> L = new List<string>(); }
public static class WinFocus
{
    public static IntPtr Fg = IntPtr.Zero;
    public static Dictionary<long, int> Pids = new Dictionary<long, int>();
    public static bool FocusOk = true;
    public static IntPtr Foreground() { return Fg; }
    public static int Pid(IntPtr h) { int p; return Pids.TryGetValue((long)h, out p) ? p : 0; }
    public static bool Focus(IntPtr h)
    {
        bool ok = FocusOk && h != IntPtr.Zero;
        Evenements.L.Add("afficher " + (long)h + " " + (ok ? "oui" : "non"));
        if (ok) Fg = h;
        return ok;
    }
    public static void KeepOnTop(IntPtr h) { }
}
public static class OGNative
{
    public static HashSet<string> Prises = new HashSet<string>();
    public static bool AddHotKey(IntPtr h, int id, uint mods, uint vk)
    {
        bool ok = !Prises.Contains(mods + ":" + vk);
        Evenements.L.Add("prendre " + id + " " + mods + " " + vk + " " + (ok ? "oui" : "non"));
        return ok;
    }
    public static void RemoveHotKey(IntPtr h, int id) { Evenements.L.Add("rendre " + id); }
}
public static class MouseHook
{
    public static bool StartOk = true;
    public static void Clear() { Evenements.L.Add("souris-vider"); }
    public static void Bind(int mods, int button, int id) { Evenements.L.Add("souris-lier " + mods + " " + button + " " + id); }
    public static bool Start(IntPtr w) { Evenements.L.Add("souris-demarrer " + (StartOk ? "oui" : "non")); return StartOk; }
    public static void Stop() { Evenements.L.Add("souris-arreter"); }
}
'@

function script:Write-Log([string]$M) { [Evenements]::L.Add("journal $M") }
function script:Set-Status([string]$T) { [Evenements]::L.Add("statut $T") }
function script:Get-Date { param([Parameter(Position = 0)]$Date, [string]$Format) $d = [datetime]'2026-11-06T15:00:00'; if ($Format) { $d.ToString($Format) } else { $d } }
function script:Save-JsonSur([string]$Path, $Objet, [int]$Profondeur = 6) { [Evenements]::L.Add('enregistrer') }
$script:Fausses = @()
$script:FauxSurvol = $false

# Les fonctions de Nevermind utilisées par l'Organizer
Import-Fonction 'recherche' 'ConvertTo-SearchText'
Import-Fonction 'bibliotheque' 'Get-GameHue'
foreach ($v in 'OrgMaxKeys', 'OrgHotkeyBase', 'OrgClassColors', 'OrgBreeds') { Import-Variable 'organizer' $v }
foreach ($f in 'Get-OrgConfig', 'Save-OrgConfig', 'ConvertFrom-DofusTitle', 'Get-OrgList', 'Get-OrgOnline', 'Move-OrgChar',
    'Remove-OrgChar', 'Get-OrgTarget', 'Show-OrgChar', 'ConvertTo-OrgHotkey', 'ConvertTo-OrgMouse', 'Format-OrgKey',
    'Unregister-OrgHotkeys', 'Stop-OrgWatch', 'Set-OrgOn', 'Get-OrgBreedId', 'Get-OrgColor', 'Set-OrgCapturedKey',
    'Add-OrgModifiers', 'Receive-OrgKey', 'Receive-OrgMouse') {
    $r = switch ($f) {
        'Add-OrgModifiers' { { param($t) $t.Replace('[int][System.Windows.Input.Keyboard]::Modifiers', '$script:FauxMods') } }
        default { $null }
    }
    Import-Fonction 'organizer' $f $r
}
# La veille : la vraie, sans la fenêtre WPF (le survol de la barre est simulé)
Import-Fonction 'organizer' 'Update-OrgWatch' { param($t) $t.Replace('$script:OrgBar.Win.IsMouseOver', '$script:FauxSurvol') }
# Les raccourcis : les vrais, sans la fenêtre WPF (la touche arrive par « touche » dans les scénarios)
Import-Fonction 'organizer' 'Register-OrgHotkeys' {
    param($t)
    $t = $t.Replace('$h = (New-Object System.Windows.Interop.WindowInteropHelper $Window).EnsureHandle()', '$h = [IntPtr]1')
    [regex]::Replace($t, '(?s)if \(-not \$script:OrgHook\) \{.*?AddHook\(\$script:OrgHook\)\s*\}', '')
}
# Les fenêtres du jeu : celles du scénario, avec le vrai découpage du titre
function script:Get-DofusWindows {
    @($script:Fausses | ForEach-Object {
        $t = ConvertFrom-DofusTitle $_.Title
        @{ Name = $t.Name; Class = $t.Class; Pid = [int]$_.Pid; Hwnd = [IntPtr][int64]$_.Hwnd; Title = $_.Title }
    })
}
# La veille allumée (Start-OrgWatch sans minuterie WPF : les tics arrivent par « tic » dans les scénarios)
function script:Start-OrgWatch { $script:OrgVeille = $true; $script:OrgTick = 0; Update-OrgWatch -Full }
$vraiStop = ${function:script:Stop-OrgWatch}
function script:Stop-OrgWatch { $script:OrgVeille = $false; & $vraiStop }
# La barre flottante et la vue, sans WPF : ce qu'elles montrent est noté
function script:Show-OrgBar {
    if ($script:OrgBar) {
        if (-not $script:OrgBar.Visible) { $script:OrgBar.Visible = $true; [Evenements]::L.Add('montrer') }
        if ($script:OrgBarSig -ne $script:OrgSig) { Update-OrgBar }
        return
    }
    $script:OrgBar = @{ Visible = $false }
    Update-OrgBar
    $script:OrgBar.Visible = $true; [Evenements]::L.Add('montrer')
    Set-OrgBarOnTop
}
function script:Hide-OrgBar { if ($script:OrgBar -and $script:OrgBar.Visible) { $script:OrgBar.Visible = $false; [Evenements]::L.Add('cacher') } }
function script:Set-OrgBarOnTop { if ($script:OrgBar) { [Evenements]::L.Add('devant-barre') } }
function script:Update-OrgBar {
    if (-not $script:OrgBar) { return }
    $script:OrgBarSig = $script:OrgSig
    $fg = [WinFocus]::Foreground(); $fgPid = [WinFocus]::Pid($fg)
    $n = 0; $items = @()
    foreach ($ch in @(Get-OrgOnline)) {
        $n++
        $on = $ch.Hwnd -eq $fg -or ($fgPid -and $ch.Pid -eq $fgPid)
        $key = (Get-OrgConfig).Hotkeys["P$n"]
        $items += "$(if ($on) { '>' })$($ch.Name)$(if ($ch.Class) { " ($($ch.Class))" })$(if ($key) { " : $(Format-OrgKey $key)" })"
    }
    [Evenements]::L.Add("barre $(if ($n) { $items -join ' ; ' } else { 'Aucun perso connecté' })")
}
function script:Build-OrgPanel { $null = Get-OrgList; [Evenements]::L.Add('vue') }
function script:Receive-OrgIcons { }

# ---------------------------------------------------------------------------
# Les petites fonctions : touches, souris, noms, couleurs, classes, titres, capture
# ---------------------------------------------------------------------------
function Save-Juge([string]$Nom, $Objet) {
    $t = ConvertTo-Json -InputObject $Objet -Depth 30 -Compress
    # (les tirets longs des cas d'essai écrits en code (u2013, u2014) : jamais tels quels dans le dépôt)
    $t = $t.Replace([string][char]0x2013, ([string][char]92 + 'u2013')).Replace([string][char]0x2014, ([string][char]92 + 'u2014'))
    [IO.File]::WriteAllText((Join-Path $Sortie "$Nom.json"), $t, (New-Object Text.UTF8Encoding($false)))
    Write-Host "$Nom : écrit"
}
$hasard = New-Object System.Random $Graine
function Pioche($Liste) { $Liste[$hasard.Next($Liste.Count)] }

$noms = @([Enum]::GetNames([System.Windows.Input.Key]))
$jetons = New-Object System.Collections.Generic.List[string]
foreach ($n in $noms) { $jetons.Add($n); $jetons.Add($n.ToLowerInvariant()); $jetons.Add($n.ToUpperInvariant()) }
foreach ($i in 0..260) { $jetons.Add([string]$i) }
foreach ($c in 33..126) { $jetons.Add([string][char]$c) }
foreach ($x in 'é', 'É', 'ç', 'ù', '²', '٣', 'ß', 'Ω', 'µ', ' ', '', 'ENTER', 'enter', 'Esc', 'PgUp', 'PGDN', 'PrtSc', 'Ins', 'Del',
    'Windows', 'Win', 'LeftWindows', 'RightWindows', 'Application', 'Break', 'Backspace', 'Bksp', 'BS', 'Shift', 'LeftShift',
    'Control', 'Ctrl', 'Alt', 'Maj', 'Semicolon', 'Plus', 'Comma', 'Minus', 'Period', 'Question', 'Tilde', 'OpenBrackets',
    'Pipe', 'CloseBrackets', 'Quotes', 'Backslash', 'Finish', 'Attn', 'CrSel', 'ExSel', 'EraseEof', 'Play', 'Zoom', 'Pa1',
    '-1', '+5', '05', '0x10', '1e1', '99999999999', '2147483647', '-2147483648', 'a,b', 'A, B', 'A,,B', ',A', 'F1,F2', 'D1, 5',
    'tab,', 'Souris4', 'Molette', 'F25', 'F0', 'NumPad10', 'D10', 'Ａ', 'İ', 'ı', 'ﬁ', 'Next', 'Prior', '  A  ', "A`t", '+', '++') { $jetons.Add($x) }
$mods = @('', 'Ctrl+', 'Alt+', 'Maj+', 'Win+', 'Ctrl+Maj+', 'ctrl+alt+', 'CTRL+MAJ+ALT+WIN+', 'Ctrl + ', ' Maj+', 'Shift+')
$textes = New-Object System.Collections.Generic.List[string]
foreach ($j in $jetons) { $textes.Add($j); $textes.Add("$(Pioche $mods)$j") }
for ($i = 0; $i -lt 600; $i++) {
    $k = $hasard.Next(1, 5)
    $textes.Add((@(1..$k | ForEach-Object { if ($hasard.Next(3) -eq 0) { Pioche @('Ctrl', 'Alt', 'Maj', 'Win', 'ctrl', ' Maj ') } else { $jetons[$hasard.Next($jetons.Count)] } }) -join '+'))
}
foreach ($x in '', '+', 'Ctrl+', 'A+', '+A', 'Ctrl++A', ' ctrl + a ', 'A+B', 'A+Molette', 'Molette+A', 'Ctrl+Souris4', 'souris5', 'Alt+Maj+Win+Molette',
    'Souris4+Souris5', 'Souris3', 'Ctrl+Souris4+', 'Ctrl+ +Souris4', 'Win+Souris5') { $textes.Add($x) }

$touches = @(foreach ($t in $textes) {
    $hk = ConvertTo-OrgHotkey $t
    $ms = ConvertTo-OrgMouse $t
    [ordered]@{ e = $t; hk = $(if ($hk) { @([int]$hk.Mods, [int]$hk.Vk) } else { $null }); souris = $(if ($ms) { @([int]$ms.Mods, [int]$ms.Button) } else { $null }); nom = (Format-OrgKey $t) }
})

# Touche capturée dans la vue : code Windows de la touche + modificateurs tenus (Receive-OrgKey, Add-OrgModifiers)
$captures = @(foreach ($vk in 0..255) {
    foreach ($m in 0, 2, 5, 15, $hasard.Next(16)) {
        $script:FauxMods = $m
        $script:OrgCapture = 'Next'
        $script:OrgConfig = $null
        $script:OrgCaptureRes = '<rien>'
        $e = [pscustomobject]@{ Key = [System.Windows.Input.KeyInterop]::KeyFromVirtualKey($vk); SystemKey = [System.Windows.Input.Key]::None; Handled = $false }
        # (Set-OrgCapturedKey noté seulement)
        ${function:script:Set-OrgCapturedKey} = { param([string]$Text) $script:OrgCaptureRes = $Text; $script:OrgCapture = $null }
        ${function:script:Build-OrgPanel} = { $script:OrgCaptureRes = '<annule>' }
        Receive-OrgKey $e
        [ordered]@{ vk = $vk; mods = $m; r = $script:OrgCaptureRes; garde = [bool]$script:OrgCapture }
    }
})
foreach ($b in 'XButton1', 'XButton2', 'Middle', 'Left', 'Right') {
    foreach ($m in 0, 2, 9) {
        $script:FauxMods = $m; $script:OrgCapture = 'P1'; $script:OrgCaptureRes = '<rien>'
        Receive-OrgMouse ([pscustomobject]@{ ChangedButton = $b; Handled = $false })
        $captures += [ordered]@{ bouton = $b; mods = $m; r = $script:OrgCaptureRes }
    }
}
Import-Fonction 'organizer' 'Set-OrgCapturedKey'
function script:Build-OrgPanel { $null = Get-OrgList; [Evenements]::L.Add('vue') }

# Couleur et numéro de classe (Get-OrgColor, Get-OrgBreedId)
$classes = @('Iop', 'iop', 'IOP', 'Crâ', 'Cra', 'Xélor', 'xelor', 'Eniripsa', 'Sacrieur', 'Ecaflip', 'Écaflip', 'Enutrof', 'Sram',
    'Feca', 'Féca', 'Osamodas', 'Sadida', 'Pandawa', 'Roublard', 'Zobal', 'Steamer', 'Eliotrope', 'Éliotrope', 'Huppermage',
    'Ouginak', 'Forgelance', '', ' iop ', 'Iop!', 'Inconnu', 'Cra Iop', 'Ouginak2', 'Ελ', '3.0')
$persos = @('Bob', 'Élise', '', 'a', 'Zz', 'Kévin-Le-Grand', 'Ünal', '日本', 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789', ('x' * 300))
$couleurs = @(foreach ($cl in $classes) { foreach ($p in @(Pioche $persos), @(Pioche $persos)) {
    [ordered]@{ classe = $cl; nom = $p; couleur = (Get-OrgColor @{ Class = $cl; Name = $p }); id = [int](Get-OrgBreedId $cl) }
} })
foreach ($p in $persos) { $couleurs += [ordered]@{ classe = 'Inconnu'; nom = $p; couleur = (Get-OrgColor @{ Class = 'Inconnu'; Name = $p }); id = 0 } }

# Titres de fenêtres : morceaux tirés au hasard
$morceaux = @('Bob', 'Élise', 'Iop', 'Cra', 'Dofus', 'Dofus 2.70', '3.0.12', 'Release', 'Beta', '', ' ', 'dofus', 'DOFUS3', '2', 'x - y', 'Kévin')
$titres = @(for ($i = 0; $i -lt 400; $i++) {
    $sep = Pioche @(' - ', ' - ', '-', ' -  ', '  - ', (' ' + [char]0x2013 + ' '))
    $t = (@(1..$hasard.Next(1, 5) | ForEach-Object { Pioche $morceaux }) -join $sep)
    $r = ConvertFrom-DofusTitle $t
    [ordered]@{ e = $t; nom = $r.Name; classe = $r.Class }
})

# ---------------------------------------------------------------------------
# Les réglages (Get-OrgConfig) : fichiers organizer.json d'essai, dans un dossier temporaire
# ---------------------------------------------------------------------------
Import-Fonction 'donnees' 'Read-JsonSur'
$bac = Join-Path $env:TEMP "nevermind-juges-organizer-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
New-Item -ItemType Directory -Force -Path $bac | Out-Null
$script:OrgFile = Join-Path $bac 'organizer.json'
$fichiers = @(
    $null, '', '   ', '{', 'null', '[]', '[1,2]', '"texte"', '42', 'true', '{}',
    '{"On":true}', '{"On":false,"Bar":false,"BarAlways":true}', '{"On":"false"}', '{"On":0}', '{"On":"0"}', '{"On":1}', '{"On":[]}', '{"On":{}}', '{"On":null}',
    '{"BarX":12.5,"BarY":-3}', '{"BarX":"12.5","BarY":"7"}', '{"BarX":"abc","On":true}', '{"On":true,"BarX":"abc","Order":["A"]}', '{"BarX":null,"BarY":0}', '{"BarX":true}', '{"BarX":1e400}',
    '{"Order":["Bob","Élise",null,"",0,5,true,false,"Bob"]}', '{"Order":"Bob"}', '{"Order":[["A","B"]]}', '{"Order":{"a":1}}', '{"Order":[]}', '{"Order":[1.5,-2]}',
    '{"Hotkeys":{"Next":"A","P9":"F9","Bidule":"X"}}', '{"Hotkeys":{"next":"B","P1":null,"P2":""}}', '{"Hotkeys":{"P3":5,"P4":true}}', '{"Hotkeys":"F1"}', '{"Hotkeys":null}', '{"Hotkeys":[]}',
    '{"Classes":{"Bob":"Iop","élise":"Cra","X":null}}', '{"Classes":{"Bob":["Iop"]}}', '{"Classes":"Iop"}',
    "{`"Order`":[`"Bob`"],`"Classes`":{`"Bob`":`"Iop`"},`"Hotkeys`":{`"Next`":`"Souris4`",`"Prev`":`"Ctrl+Souris5`"},`"Bar`":true,`"BarAlways`":false,`"BarX`":100,`"BarY`":20,`"On`":true}",
    '﻿{"On":true}', '{"On":true,}', '{"On":true} x'
)
$configs = @(foreach ($t in $fichiers) {
    Remove-Item -LiteralPath $bac -Recurse -Force -ErrorAction SilentlyContinue; New-Item -ItemType Directory -Force -Path $bac | Out-Null
    if ($null -ne $t) { [IO.File]::WriteAllText($script:OrgFile, $t, (New-Object Text.UTF8Encoding($false))) }
    $script:OrgConfig = $null
    [Evenements]::L.Clear()
    $c = Get-OrgConfig
    [ordered]@{ e = $t
        On = $c.On; Bar = $c.Bar; BarAlways = $c.BarAlways; BarX = $c.BarX; BarY = $c.BarY; Order = @($c.Order)
        Classes = @($c.Classes.Keys | Sort-Object { $_.ToLowerInvariant() } | ForEach-Object { , @($_, $c.Classes[$_]) })
        Hotkeys = @($c.Hotkeys.Keys | Sort-Object { $_.ToLowerInvariant() } | ForEach-Object { , @($_, $c.Hotkeys[$_]) })
        illisible = [bool](@([Evenements]::L | Where-Object { $_ -like 'journal Organizer : réglages illisibles*' }).Count) }
})
Remove-Item -LiteralPath $bac -Recurse -Force -ErrorAction SilentlyContinue

# ---------------------------------------------------------------------------
# La veille (Update-OrgWatch) et tout ce qui la touche : scénarios tirés au hasard
# ---------------------------------------------------------------------------
$poolTitres = @('Bob - Iop - 3.0.12 - Release', 'Alice - Cra - 3.1.4 - Release', 'Élise - Eniripsa - 3.2 - Beta', 'bob - Iop - 3.0',
    'Zed - Dofus 2.70', 'Dofus', 'Dofus 3.1', 'Kévin - Sram - 3.0 - Release', 'Max - Feca - 3.0', 'Lou - Xélor - 3.0', 'Ünal - Ouginak - 3',
    'Moe - Huppermage - 3.0', 'ÉLISE - Eniripsa - 3.2', 'Bob - Cra - 3.0', 'Zed - Zobal - 3.0', 'Nina - Pandawa - 3.0 - Release', 'Dofus - 3.0')
$actions = @('Next', 'Prev') + @(1..8 | ForEach-Object { "P$_" })
$textesTouches = @('F1', 'F2', 'Ctrl+Tab', 'Ctrl+Maj+Tab', 'Souris4', 'Souris5', 'Ctrl+Souris4', 'Molette', 'D1', 'Alt+D2', 'NumPad3', 'Tab', '', 'A', 'F9', 'Bidule')

function New-Scenario([int]$N) {
    $cfg = [ordered]@{ On = $true }
    if ($hasard.Next(2)) { $cfg.Bar = [bool]$hasard.Next(4) }
    if ($hasard.Next(2)) { $cfg.BarAlways = [bool]$hasard.Next(2) }
    if ($hasard.Next(2)) { $cfg.Order = @(1..$hasard.Next(0, 4) | ForEach-Object { (ConvertFrom-DofusTitle (Pioche $poolTitres)).Name } | Where-Object { $_ }) }
    if ($hasard.Next(2)) { $h = [ordered]@{}; foreach ($i in 1..$hasard.Next(1, 5)) { $h[(Pioche $actions)] = (Pioche $textesTouches) }; $cfg.Hotkeys = $h }
    if ($hasard.Next(3) -eq 0) { $cfg.Classes = [ordered]@{ Bob = 'Sram'; Zed = 'Iop' } }
    $etapes = New-Object System.Collections.Generic.List[object]
    $fen = @()
    $prochain = 0
    for ($i = 0; $i -lt $N; $i++) {
        $r = $hasard.Next(100)
        if ($r -lt 18 -or $i -eq 0) {
            # Les fenêtres changent : une ouverte, une fermée, un titre qui change, un jeu relancé
            $fen = @($fen | ForEach-Object { $_ })
            switch ($hasard.Next(5)) {
                0 { if ($fen.Count) { $k = $hasard.Next($fen.Count); $fen = @(for ($q = 0; $q -lt $fen.Count; $q++) { if ($q -ne $k) { $fen[$q] } }) } }
                1 { if ($fen.Count) { $k = $hasard.Next($fen.Count); $fen[$k] = [ordered]@{ Title = (Pioche $poolTitres); Pid = $fen[$k].Pid; Hwnd = $fen[$k].Hwnd } } }
                default { if ($fen.Count -lt 6) { $prochain++; $fen += [ordered]@{ Title = (Pioche $poolTitres); Pid = 1000 + $prochain; Hwnd = 50000 + 16 * $prochain } } }
            }
            $etapes.Add([ordered]@{ fenetres = @($fen) })
        } elseif ($r -lt 40) {
            $choix = @(0, 777, 99999) + @($fen | ForEach-Object { $_.Hwnd }) + @($fen | ForEach-Object { $_.Hwnd })
            $h = Pioche $choix
            $p = if ($h -eq 777 -and $fen.Count) { (Pioche @($fen)).Pid } elseif ($h -eq 99999) { 4242 } else { @($fen | Where-Object { $_.Hwnd -eq $h } | ForEach-Object { $_.Pid })[0] }
            $etapes.Add([ordered]@{ devant = $h; pid = [int]$p })
        } elseif ($r -lt 46) { $etapes.Add([ordered]@{ touche = (Pioche @(7100, 7101, 7111, 7112, 7113, 7118, 7105)) })
        } elseif ($r -lt 48) { $etapes.Add([ordered]@{ prises = @(1..$hasard.Next(0, 3) | ForEach-Object { Pioche @('0:112', '0:113', '2:9', '6:9', '0:49') }) })
        } elseif ($r -lt 50) { $etapes.Add([ordered]@{ capture = (Pioche $actions); texte = (Pioche $textesTouches) })
        } elseif ($r -lt 51) { $etapes.Add([ordered]@{ souris_ok = [bool]$hasard.Next(2) })
        } elseif ($r -lt 52) { $etapes.Add([ordered]@{ focus_ok = [bool]$hasard.Next(3) })
        } elseif ($r -lt 54) { $etapes.Add([ordered]@{ survol = [bool]$hasard.Next(2) })
        } elseif ($r -lt 55) { $etapes.Add([ordered]@{ vue = [bool]$hasard.Next(2) })
        } elseif ($r -lt 56) { $etapes.Add([ordered]@{ on = [bool]$hasard.Next(2) })
        } elseif ($r -lt 57) { $etapes.Add([ordered]@{ barre = [bool]$hasard.Next(2) })
        } elseif ($r -lt 58) { $etapes.Add([ordered]@{ toujours = [bool]$hasard.Next(2) })
        } elseif ($r -lt 59) { $n1 = (ConvertFrom-DofusTitle (Pioche $poolTitres)).Name; $n2 = (ConvertFrom-DofusTitle (Pioche $poolTitres)).Name; $etapes.Add([ordered]@{ deplacer = $n1; vers = $n2 })
        } elseif ($r -lt 60) { $etapes.Add([ordered]@{ retirer = (ConvertFrom-DofusTitle (Pioche $poolTitres)).Name })
        } elseif ($r -lt 61) { $etapes.Add([ordered]@{ clic = (ConvertFrom-DofusTitle (Pioche $poolTitres)).Name })
        } elseif ($r -lt 62) { $etapes.Add([ordered]@{ afficher = (Pioche @(@($fen | ForEach-Object { $_.Hwnd }) + 0 + 123)) })
        } else { $etapes.Add([ordered]@{ tic = $hasard.Next(1, 12) }) }
    }
    [ordered]@{ config = $cfg; etapes = $etapes.ToArray() }
}

function Invoke-Scenario($S) {
    $script:OrgConfig = $null; $script:OrgFile = Join-Path $env:TEMP 'organizer-juge-absent.json'
    foreach ($v in 'OrgKeysOn', 'OrgKeyIds', 'OrgKeyFailed', 'OrgKeysLog', 'OrgKeysHandle', 'OrgTick', 'OrgLast', 'OrgSig', 'OrgHwnds', 'OrgPids',
        'OrgBar', 'OrgBarSig', 'OrgBarFg', 'OrgViewOn', 'OrgDragging', 'OrgTitles', 'OrgVeille', 'OrgCapture', 'OrgFake', 'OrgFakeFg') { Set-Variable -Scope script -Name $v -Value $null }
    $script:Fausses = @(); $script:FauxSurvol = $false
    [WinFocus]::Fg = [IntPtr]::Zero; [WinFocus]::Pids.Clear(); [WinFocus]::FocusOk = $true
    [OGNative]::Prises.Clear(); [MouseHook]::StartOk = $true
    # La configuration de départ (comme lue dans organizer.json)
    $c = @{ On = $false; Order = @(); Bar = $true; BarAlways = $false; BarX = $null; BarY = $null; Classes = @{}; Hotkeys = @{ Next = 'Ctrl+Tab'; Prev = 'Ctrl+Maj+Tab' } }
    for ($i = 1; $i -le $OrgMaxKeys; $i++) { $c.Hotkeys["P$i"] = "F$i" }
    foreach ($k in 'On', 'Bar', 'BarAlways') { if ($S.config.Contains($k)) { $c[$k] = [bool]$S.config[$k] } }
    if ($S.config.Contains('Order')) { $c.Order = @($S.config.Order | Where-Object { $_ } | ForEach-Object { [string]$_ }) }
    if ($S.config.Contains('Hotkeys')) { foreach ($k in $S.config.Hotkeys.Keys) { $c.Hotkeys[$k] = [string]$S.config.Hotkeys[$k] } }
    if ($S.config.Contains('Classes')) { foreach ($k in $S.config.Classes.Keys) { $c.Classes[$k] = [string]$S.config.Classes[$k] } }
    $script:OrgConfig = $c
    $sorties = @()
    # Au démarrage : l'organizer reprend s'il était actif
    [Evenements]::L.Clear()
    if ($c.On) { Start-OrgWatch }
    $sorties += , @([Evenements]::L)
    foreach ($e in $S.etapes) {
        [Evenements]::L.Clear()
        try {
            if ($e.Contains('fenetres')) {
                $script:Fausses = @($e.fenetres)
                foreach ($f in $e.fenetres) { [WinFocus]::Pids[[int64]$f.Hwnd] = [int]$f.Pid }
            } elseif ($e.Contains('devant')) {
                [WinFocus]::Fg = [IntPtr][int64]$e.devant
                if ($e.pid) { [WinFocus]::Pids[[int64]$e.devant] = [int]$e.pid } else { [void][WinFocus]::Pids.Remove([int64]$e.devant) }
            } elseif ($e.Contains('tic')) {
                for ($k = 0; $k -lt $e.tic; $k++) { if ($script:OrgVeille) { Update-OrgWatch } }
            } elseif ($e.Contains('touche')) {
                $id = [int]$e.touche
                if ($script:OrgKeyIds -and $script:OrgKeyIds.ContainsKey($id)) { [void](Show-OrgChar (Get-OrgTarget $script:OrgKeyIds[$id])) }
            } elseif ($e.Contains('prises')) {
                [OGNative]::Prises.Clear(); foreach ($p in $e.prises) { [void][OGNative]::Prises.Add($p) }
            } elseif ($e.Contains('capture')) {
                $script:OrgCapture = $e.capture
                Set-OrgCapturedKey $e.texte
            } elseif ($e.Contains('souris_ok')) { [MouseHook]::StartOk = $e.souris_ok
            } elseif ($e.Contains('focus_ok')) { [WinFocus]::FocusOk = $e.focus_ok
            } elseif ($e.Contains('survol')) { $script:FauxSurvol = $e.survol
            } elseif ($e.Contains('vue')) { $script:OrgViewOn = $e.vue; if ($e.vue) { Build-OrgPanel }
            } elseif ($e.Contains('on')) { Set-OrgOn $e.on
            } elseif ($e.Contains('barre')) { $cc = Get-OrgConfig; $cc.Bar = $e.barre; Save-OrgConfig; if ($script:OrgVeille) { Update-OrgWatch -Full }
            } elseif ($e.Contains('toujours')) { $cc = Get-OrgConfig; $cc.BarAlways = $e.toujours; Save-OrgConfig; if ($script:OrgVeille) { Update-OrgWatch -Full }
            } elseif ($e.Contains('deplacer')) { Move-OrgChar $e.deplacer $e.vers; Build-OrgPanel; Update-OrgBar
            } elseif ($e.Contains('retirer')) { Remove-OrgChar $e.retirer; Build-OrgPanel
            } elseif ($e.Contains('clic')) { $nm = $e.clic; [void](Show-OrgChar (@(Get-OrgOnline | Where-Object { $_.Name -eq $nm })[0]))
            } elseif ($e.Contains('afficher')) { [void](Show-OrgChar @{ Name = ''; Hwnd = [IntPtr][int64]$e.afficher }) }
        } catch { [Evenements]::L.Add("erreur $($_.Exception.Message)") }
        $sorties += , @([Evenements]::L)
    }
    $c = Get-OrgConfig
    [ordered]@{ config = $S.config; etapes = $S.etapes; sorties = $sorties
        fin = [ordered]@{ On = $c.On; Bar = $c.Bar; BarAlways = $c.BarAlways; Order = @($c.Order)
            Classes = @($c.Classes.Keys | Sort-Object { $_.ToLowerInvariant() } | ForEach-Object { , @($_, $c.Classes[$_]) })
            Hotkeys = @($c.Hotkeys.Keys | Sort-Object { $_.ToLowerInvariant() } | ForEach-Object { , @($_, $c.Hotkeys[$_]) }) } }
}

$veille = @(for ($i = 0; $i -lt $Scenarios; $i++) { Invoke-Scenario (New-Scenario $hasard.Next(10, 60)) })

Save-Juge 'organizer-touches' $touches
Save-Juge 'organizer-captures' $captures
Save-Juge 'organizer-couleurs' $couleurs
Save-Juge 'organizer-titres' $titres
Save-Juge 'organizer-config' $configs
Save-Juge 'organizer-veille' $veille
Write-Host "Touches : $($touches.Count), captures : $($captures.Count), couleurs : $($couleurs.Count), titres : $($titres.Count), réglages : $($configs.Count), scénarios : $($veille.Count)"
