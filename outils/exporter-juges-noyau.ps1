# Exporte les « juges » du noyau de Nevermind en Rust (rust\nevermind-noyau\tests\juges\*.json) depuis la version
# PowerShell, qui fait foi : les tests Rust (cargo test -p nevermind-noyau) rejouent ces cas et doivent donner
# exactement la même sortie.
#
#   & 'outils\exporter-juges-noyau.ps1'      (depuis l'outil PowerShell, ou un processus caché à part)
#
# Rien ne touche le PC : seules les fonctions utiles sont lues dans les modules (rien ne se lance au chargement),
# les fichiers sont écrits dans un dossier temporaire effacé à la fin, l'Aide est une fausse (une classe AideClient
# de ce script), le réseau est un faux Invoke-RestMethod, l'heure est figée (Get-Date de ce script).
# Juges écrits :
#   json.json        ConvertTo-Json (forme lisible et -Compress) de valeurs tirées au hasard (graine fixe) ;
#   fichiers.json    Save-JsonSur / Read-JsonSur : fichiers avant, valeur lue, fichiers après, journal ;
#   reglages.json    Get-Setting / Set-Setting (-Exiger) : suites d'opérations, parametres.json après, journal ;
#   journal.json     la mise au propre du journal (1 Mo, 3000 lignes, journal-ancien.txt) et LogError ;
#   aide.json        le texte des demandes sur le canal, Invoke-Aide (réponses, refus, Aide absente, délai) ;
#   maj.json         $GetReleaseScript, Test-NewerVersion, Invoke-UpdateCheck (textes), clé et signature d'essai ;
#   assistance.json  historique (ConvertFrom-HistoryEntry, détails techniques, titres, définitif...), nouveautés,
#                    masque des données personnelles, bienvenue ;
#   sauvegarde.json  Import-Backup (fichier, copie de secours, abîmé, origines du registre factices, absente) ;
#   pages.json       la fiche de la visite guidée et des résultats, le panneau « Signaler un problème » (objets WPF jamais
#                    affichés : aucune fenêtre).

$ErrorActionPreference = 'Stop'
$env:NEVERMIND_TEST = '1'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
$sortie  = Join-Path $racine 'rust\nevermind-noyau\tests\juges'
# (cas jamais vus : une autre graine et un autre dossier, NEVERMIND_GRAINE et NEVERMIND_JUGES_SORTIE ; le test Rust
# les lit avec NEVERMIND_JUGES)
if ($env:NEVERMIND_JUGES_SORTIE) { $sortie = $env:NEVERMIND_JUGES_SORTIE }
$graine = if ($env:NEVERMIND_GRAINE) { [int]$env:NEVERMIND_GRAINE } else { 20261106 }
New-Item -ItemType Directory -Force -Path $sortie | Out-Null
$utf8 = New-Object Text.UTF8Encoding($false)

# ---------------------------------------------------------------------------
# Les fonctions des modules, lues sans rien lancer
# ---------------------------------------------------------------------------
$asts = @{}
foreach ($f in 'donnees', 'assistance', 'mises-a-jour', 'aide-client') {
    $tok = $null; $err = $null
    $asts[$f] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$f.ps1"), [ref]$tok, [ref]$err)
    if ($err) { throw "$f.ps1 n'est pas lisible : $($err[0].Message)" }
}
function Import-Fonction([string]$Fichier, [string]$Nom) {
    $a = $asts[$Fichier].Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $false)
    if (-not $a) { throw "Fonction $Nom introuvable dans $Fichier.ps1" }
    Invoke-Expression ($a.Extent.Text -replace "^function\s+$([regex]::Escape($Nom))", "function script:$Nom")
}
function Get-Variable-Module([string]$Fichier, [string]$Nom) {
    $a = $asts[$Fichier].Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq "`$$Nom" }, $false)
    if (-not $a) { throw "Variable $Nom introuvable dans $Fichier.ps1" }
    $a.Right.Extent.Text
}

# L'heure figée
$script:Maintenant = [datetime]'2026-11-06T21:05:09'
function script:Get-Date { param([Parameter(Position = 0)]$Date, [string]$Format)
    if ($Format -eq 's') { return $script:Maintenant.ToString('s') }
    if ($Format) { return $script:Maintenant.ToString($Format) }
    $script:Maintenant
}
# Le journal capté (sauf pour les juges du journal lui-même)
$script:Journal = New-Object System.Collections.Generic.List[string]
function script:Write-Log([string]$Message) { $script:Journal.Add($Message) }

$Bac = Join-Path $env:TEMP "nevermind-juges-noyau-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
New-Item -ItemType Directory -Force -Path $Bac | Out-Null
$Bac = [IO.Path]::GetFullPath($Bac)
function Get-Fichiers([string]$D) {
    $o = [ordered]@{}
    foreach ($f in @(Get-ChildItem -LiteralPath $D -File -Force | Sort-Object Name)) { $o[$f.Name] = [Convert]::ToBase64String([IO.File]::ReadAllBytes($f.FullName)) }
    $o
}
function Set-Fichiers([string]$D, $Fichiers) {
    if (Test-Path -LiteralPath $D) { Remove-Item -LiteralPath $D -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $D | Out-Null
    foreach ($k in @($Fichiers.Keys)) { [IO.File]::WriteAllBytes((Join-Path $D $k), [Convert]::FromBase64String($Fichiers[$k])) }
}
function B64([string]$T, [switch]$Bom) { $b = $utf8.GetBytes($T); if ($Bom) { $b = [byte[]](0xEF, 0xBB, 0xBF) + $b }; [Convert]::ToBase64String($b) }
function Get-JournalRelatif([string]$D) { @($script:Journal | ForEach-Object { $_.Replace($D, '<dossier>') }) }
# ConvertFrom-Json de PowerShell 5.1 rend une liste « enveloppée » : ConvertTo-Json l'écrirait {"value":[...],"Count":n}.
# Les valeurs des juges sont des listes ordinaires, comme celles que Nevermind écrit (@(...)).
function ConvertFrom-JsonNu([string]$T) { $v = ConvertFrom-Json $T; if ($v -is [array]) { , ([object[]]$v) } else { $v } }
function Save-Juge([string]$Nom, $Contenu) {
    [IO.File]::WriteAllText((Join-Path $sortie $Nom), (ConvertTo-Json -InputObject $Contenu -Depth 30), $utf8)
    Write-Host "$Nom : $(@($Contenu.cas).Count) cas"
}

try {
    # =======================================================================
    # 1. ConvertTo-Json (forme des fichiers)
    # =======================================================================
    $rnd = New-Object Random $graine
    $lettres = @('a', 'b', 'é', 'è', 'à', 'ç', 'Œ', ' ', '"', '\', '/', "'", '<', '>', '&', "`t", "`n", "`r", [string][char]1, [string][char]0x1f,
                 [string][char]0x7f, [string][char]0x85, [string][char]0xa0, [string][char]0x2028, [string][char]0x2029, [string][char]0xfeff, '😀', '€', 'Z', '9', ':', '{', '[', ',')
    function New-Texte { $n = $rnd.Next(0, 12); -join @(for ($i = 0; $i -lt $n; $i++) { $lettres[$rnd.Next(0, $lettres.Count)] }) }
    function New-Valeur([int]$Prof) {
        $t = $rnd.Next(0, $(if ($Prof -ge 4) { 6 } else { 9 }))
        switch ($t) {
            0 { $null } 1 { $true } 2 { $false }
            3 { $rnd.Next(-100000, 100000) }
            4 { [long]$rnd.Next() * 1000003 }
            5 { New-Texte }
            6 { $n = $rnd.Next(0, 5); , @(for ($i = 0; $i -lt $n; $i++) { New-Valeur ($Prof + 1) }) }
            7 { $o = [ordered]@{}; $n = $rnd.Next(0, 5); for ($i = 0; $i -lt $n; $i++) { $k = "$(New-Texte)$i"; $o[$k] = New-Valeur ($Prof + 1) }; $o }
            8 { [double]($rnd.Next(-5000, 5000)) / 8 }
        }
    }
    $casJson = New-Object System.Collections.ArrayList
    $fixes = @('{}', '[]', '[[]]', '{"a":[]}', '{"a":{}}', '{"a":[1,[2,[3,{}]]]}', '"texte"', '5', 'true', 'null', '[1]', '{"clé\u0027x":{"e":{},"f":[{},[],"a"]}}', '{"n":1E+20,"d":1.2E-05,"g":0.1,"h":1234567.891,"e":-3.5E-07}')
    foreach ($t in $fixes) { [void]$casJson.Add($t) }
    for ($i = 0; $i -lt $(if ($env:NEVERMIND_GRAINE) { 2000 } else { 300 }); $i++) {
        $v = [ordered]@{}; $n = $rnd.Next(1, 6); for ($k = 0; $k -lt $n; $k++) { $v["k$k$(New-Texte)"] = New-Valeur 1 }
        [void]$casJson.Add((ConvertTo-Json -InputObject $v -Depth 30 -Compress))
    }
    $cas = @(foreach ($t in $casJson) {
        $v = ConvertFrom-JsonNu $t
        @{ entree = $t; lisible = (ConvertTo-Json -InputObject $v -Depth 30); compact = (ConvertTo-Json -InputObject $v -Depth 30 -Compress) }
    })
    Save-Juge 'json.json' @{ format = 1; sujet = 'ConvertTo-Json de Windows PowerShell 5.1 (forme lisible et -Compress) sur des valeurs relues par ConvertFrom-Json'; cas = $cas }

    # =======================================================================
    # 2. Fichiers sûrs
    # =======================================================================
    foreach ($n in 'Save-TexteSur', 'Save-JsonSur', 'Read-JsonSur', 'Get-Setting', 'Set-Setting') { Import-Fonction 'donnees' $n }
    $D = Join-Path $Bac 'f'
    $cas = New-Object System.Collections.ArrayList
    $p = Join-Path $D 'x.json'
    function Add-CasLecture([string]$Nom, $Avant, [switch]$Liste, [switch]$Pris) {
        Set-Fichiers $D $Avant
        $script:JsonBloques = @{}; $script:Journal.Clear()
        $verrou = if ($Pris) { [IO.File]::Open($p, 'Open', 'ReadWrite', 'None') }
        try {
            $r = if ($Liste) { , @(Read-JsonSur $p) } else { Read-JsonSur $p }
            $ecrit = $null
            if ($Pris) { try { Save-JsonSur $p @{ a = 1 }; $ecrit = 'ok' } catch { $ecrit = $_.Exception.Message } }
        } finally { if ($verrou) { $verrou.Close() } }
        [void]$cas.Add(@{ nom = $Nom; operation = 'lire'; liste = [bool]$Liste; pris = [bool]$Pris; avant = $Avant
            resultat = $(if ($null -eq $r) { $null } else { ConvertTo-Json -InputObject $r -Depth 30 -Compress }); ecriture = $ecrit
            apres = (Get-Fichiers $D); journal = @(Get-JournalRelatif $D) })
    }
    Add-CasLecture 'absent' @{}
    Add-CasLecture 'objet' @{ 'x.json' = (B64 '{"a":1,"b":"é"}') }
    Add-CasLecture 'liste' @{ 'x.json' = (B64 '[{"Id":"a"},{"Id":"b"}]') } -Liste
    Add-CasLecture 'liste d''un seul' @{ 'x.json' = (B64 '[{"Id":"a"}]') } -Liste
    Add-CasLecture 'texte null' @{ 'x.json' = (B64 'null') }
    Add-CasLecture 'vide, sans copie' @{ 'x.json' = (B64 '') }
    Add-CasLecture 'espaces, copie bonne' @{ 'x.json' = (B64 "  `r`n "); 'x.json.bak' = (B64 '{"v":2}') }
    Add-CasLecture 'avec BOM' @{ 'x.json' = (B64 '{"a":"àé"}' -Bom) }
    Add-CasLecture 'UTF-16' @{ 'x.json' = [Convert]::ToBase64String([byte[]](0xFF, 0xFE) + [Text.Encoding]::Unicode.GetBytes('{"u":"16"}')) }
    Add-CasLecture 'abîmé, copie bonne' @{ 'x.json' = (B64 '{"a":'); 'x.json.bak' = (B64 '{"a":0}') }
    Add-CasLecture 'abîmé, copie abîmée' @{ 'x.json' = (B64 '{abîmé'); 'x.json.bak' = (B64 'pas du json') }
    Add-CasLecture 'abîmé, sans copie' @{ 'x.json' = (B64 '[1,2') }
    Add-CasLecture 'bon, copie abîmée' @{ 'x.json' = (B64 '{"ok":true}'); 'x.json.bak' = (B64 '{zz') }
    Add-CasLecture 'absent, copie bonne' @{ 'x.json.bak' = (B64 '{"de":"la copie"}') }
    Add-CasLecture 'absent, copie liste' @{ 'x.json.bak' = (B64 '[1,2,3]') } -Liste
    Add-CasLecture 'abîmé, mis de côté déjà là' @{ 'x.json' = (B64 '{'); 'x.json.abime-20261106-210509' = (B64 'avant') }
    Add-CasLecture 'pris par un autre programme' @{ 'x.json' = (B64 '{"o":[1,2,3]}') } -Pris
    Add-CasLecture 'pris, copie bonne' @{ 'x.json' = (B64 '{"o":1}'); 'x.json.bak' = (B64 '{"o":0}') } -Pris
    function Add-CasEcriture([string]$Nom, $Avant, [string]$Json, [switch]$Compress, [switch]$Bom) {
        Set-Fichiers $D $Avant
        $script:JsonBloques = @{}; $script:Journal.Clear()
        $v = ConvertFrom-JsonNu $Json
        if ($Compress) { Save-JsonSur $p $v -Compress -Bom:$Bom } else { Save-JsonSur $p $v -Bom:$Bom }
        [void]$cas.Add(@{ nom = $Nom; operation = 'ecrire'; valeur = $Json; compact = [bool]$Compress; bom = [bool]$Bom; avant = $Avant; apres = (Get-Fichiers $D); journal = @(Get-JournalRelatif $D) })
    }
    Add-CasEcriture 'nouveau' @{} '{"a":1,"l":[1,2],"o":{"p":"é"}}'
    Add-CasEcriture 'remplace, ancien en copie' @{ 'x.json' = (B64 '{"vieux":1}'); 'x.json.bak' = (B64 '{"tres vieux":0}') } '{"neuf":2}'
    Add-CasEcriture 'compact avec BOM' @{ 'x.json' = (B64 '[1]') } '[1,2,{"a":"<&>"}]' -Compress -Bom
    Add-CasEcriture 'liste vide' @{} '[]'
    Add-CasEcriture 'remplace un abîmé' @{ 'x.json' = (B64 '{abime') } '{"v":1}'
    Save-Juge 'fichiers.json' @{ format = 1; sujet = 'Save-JsonSur et Read-JsonSur (donnees.ps1) : fichiers avant et après, valeur lue, journal'; cas = @($cas) }

    # =======================================================================
    # 3. Réglages
    # =======================================================================
    $SettingsFile = Join-Path $D 'parametres.json'
    $cas = New-Object System.Collections.ArrayList
    function Add-CasReglages([string]$Nom, $Avant, $Ops) {
        Set-Fichiers $D $Avant
        $script:Settings = $null; $script:JsonBloques = @{}; $script:Journal.Clear()
        $verrou = $null
        $res = @(foreach ($o in $Ops) {
            switch ($o.op) {
                'lire' { $v = Get-Setting $o.nom (ConvertFrom-JsonNu $o.defaut); @{ op = 'lire'; nom = $o.nom; defaut = $o.defaut; valeur = (ConvertTo-Json -InputObject $v -Depth 30 -Compress) } }
                'ecrire' {
                    $e = $null
                    try { Set-Setting $o.nom (ConvertFrom-JsonNu $o.valeur) -Exiger:([bool]$o.exiger) } catch { $e = $_.Exception.Message }
                    @{ op = 'ecrire'; nom = $o.nom; valeur = $o.valeur; exiger = [bool]$o.exiger; erreur = $e }
                }
                'prendre' { $verrou = [IO.File]::Open($SettingsFile, 'OpenOrCreate', 'ReadWrite', 'None'); @{ op = 'prendre' } }
                'lacher' { $verrou.Close(); $verrou = $null; @{ op = 'lacher' } }
            }
        })
        if ($verrou) { $verrou.Close() }
        $final = if (Test-Path -LiteralPath $SettingsFile) { [IO.File]::ReadAllText($SettingsFile) } else { $null }
        [void]$cas.Add(@{ nom = $Nom; avant = $Avant; operations = $res; parametres = $final; journal = @(Get-JournalRelatif $D)
            fichiers = @((Get-Fichiers $D).Keys) })
    }
    $L = { param($n, $d) @{ op = 'lire'; nom = $n; defaut = $d } }
    $E = { param($n, $v, $x) @{ op = 'ecrire'; nom = $n; valeur = $v; exiger = [bool]$x } }
    Add-CasReglages 'premier lancement' @{} @((& $L 'Beta' 'false'), (& $E 'Beta' 'true'), (& $L 'Beta' 'false'), (& $L 'Absent' '"défaut"'))
    Add-CasReglages 'fichier existant' @{ 'parametres.json' = (B64 '{"TourDone":true,"LastVersion":"1.15.5","FpsVeille":["a.exe"]}') } @((& $L 'TourDone' 'false'), (& $L 'FpsVeille' '[]'), (& $E 'LastVersion' '"1.15.6"'))
    Add-CasReglages 'valeur nulle' @{ 'parametres.json' = (B64 '{"X":null}') } @((& $L 'X' '5'), (& $E 'Y' 'null'))
    Add-CasReglages 'abîmé : copie reprise' @{ 'parametres.json' = (B64 '{"SvcOriginal":'); 'parametres.json.bak' = (B64 '{"SvcOriginal":["DiagTrack|Automatic|Running"]}') } @((& $L 'SvcOriginal' '[]'), (& $E 'Beta' 'false'))
    Add-CasReglages 'abîmé sans copie' @{ 'parametres.json' = (B64 'zz') } @((& $L 'A' '1'), (& $E 'A' '2'))
    Add-CasReglages 'pris : Exiger refuse' @{ 'parametres.json' = (B64 '{"A":1}') } @((& $L 'A' '0'), @{ op = 'prendre' }, (& $E 'SvcOriginal' '["x|Manual|Stopped"]' $true), (& $E 'B' '3'), @{ op = 'lacher' }, (& $L 'SvcOriginal' '"rien"'), (& $L 'B' '0'))
    Add-CasReglages 'pris au démarrage : jamais réécrit' @{ 'parametres.json' = (B64 '{"A":1}') } @(@{ op = 'prendre' }, (& $L 'A' '0'), @{ op = 'lacher' }, (& $E 'A' '2'), (& $E 'C' '[1]' $true), (& $L 'C' '0'))
    Add-CasReglages 'objets et listes' @{} @((& $E 'O' '{"x":{"y":[1,{"z":"é"}]}}'), (& $E 'L' '["a"]'), (& $E 'V' '[]'), (& $L 'O' 'null'), (& $L 'L' 'null'))
    Save-Juge 'reglages.json' @{ format = 1; sujet = 'Get-Setting et Set-Setting -Exiger (donnees.ps1) : valeurs lues, erreurs, parametres.json, journal'; cas = @($cas) }

    # =======================================================================
    # 4. Journal (le vrai Write-Log, sur des fichiers d'essai)
    # =======================================================================
    $raccourcir = $asts['donnees'].Find({ param($n) $n -is [System.Management.Automation.Language.TryStatementAst] -and $n.Extent.Text -match 'journal-ancien' }, $false).Extent.Text
    $cas = New-Object System.Collections.ArrayList
    $J = Join-Path $Bac 'j'
    foreach ($c in @(@{ n = 2000; t = 600 }, @{ n = 3000; t = 400 }, @{ n = 3001; t = 400 }, @{ n = 5000; t = 260 }, @{ n = 2999; t = 380 }, @{ n = 12000; t = 100 })) {
        if (Test-Path -LiteralPath $J) { Remove-Item -LiteralPath $J -Recurse -Force }
        New-Item -ItemType Directory -Force -Path $J | Out-Null
        $DataDir = $J; $LogFile = Join-Path $J 'journal.txt'
        $sb = New-Object Text.StringBuilder
        for ($i = 0; $i -lt $c.n; $i++) { [void]$sb.Append("2026-11-06T21:05:09 ligne $i é ").Append('x' * ($c.t - 30)).Append("`r`n") }
        [IO.File]::WriteAllText($LogFile, $sb.ToString(), $utf8)
        $taille = (New-Object IO.FileInfo $LogFile).Length
        Invoke-Expression $raccourcir
        $h = @{}
        foreach ($f in 'journal.txt', 'journal-ancien.txt') {
            $pf = Join-Path $J $f
            $h[$f] = if (Test-Path -LiteralPath $pf) { (Get-FileHash -LiteralPath $pf -Algorithm SHA256).Hash } else { $null }
        }
        [void]$cas.Add(@{ nom = "$($c.n) lignes de $($c.t) octets"; lignes = $c.n; longueur = $c.t; taille = $taille; empreintes = $h })
    }
    # LogError : un journal qui ne peut pas être écrit (un dossier à sa place) : noté une fois dans les préférences
    Remove-Item -LiteralPath $J -Recurse -Force; New-Item -ItemType Directory -Force -Path (Join-Path $J 'journal.txt') | Out-Null
    $LogFile = Join-Path $J 'journal.txt'; $SettingsFile = Join-Path $J 'parametres.json'
    $script:Settings = $null; $script:LogError = $null; $script:JsonBloques = @{}
    Import-Fonction 'donnees' 'Write-Log'
    Write-Log 'première'; $le1 = Get-Setting 'LogError'
    Write-Log 'deuxième'; $script:Settings = $null; $le2 = Get-Setting 'LogError'
    function script:Write-Log([string]$Message) { $script:Journal.Add($Message) }
    $journalErreur = @{ LogError = $le1; apresDeuxieme = $le2 }
    Save-Juge 'journal.json' @{ format = 1; sujet = 'Le journal (donnees.ps1) : mise au propre au-delà de 1 Mo et 3000 lignes, LogError'; cas = @($cas); logError = $journalErreur }

    # =======================================================================
    # 5. L'Aide : les demandes et Invoke-Aide (fausse Aide)
    # =======================================================================
    Add-Type -TypeDefinition @"
public static class AideClient
{
    public static string ExeAttendu = "";
    public static string Suffixe = "";
    public static bool Ecoute = false;
    public static string Reponse = "";
    public static string Erreur = "";
    public static System.Collections.Generic.List<string> Recues = new System.Collections.Generic.List<string>();
    public static bool Joignable(int ms) { return Ecoute; }
    public static string Appeler(string demande, int connexionMs, int reponseMs)
    {
        Recues.Add(demande);
        if (Erreur != "") throw new System.InvalidOperationException(Erreur);
        return Reponse;
    }
}
"@
    foreach ($n in 'Invoke-Aide', 'Get-AideEtatTache', 'Read-AideTache', 'Get-VariablesDangereuses', 'Get-FpsVeilleJeux', 'Add-FpsVeilleJeu', 'Test-FpsRefus') { Import-Fonction 'aide-client' $n }
    $AideTravail = Invoke-Expression (Get-Variable-Module 'aide-client' 'AideTravail')
    $AideVariablesDangereuses = (Get-Variable-Module 'aide-client' 'AideVariablesDangereuses').Trim("'")
    $FpsVeilleConnus = @('bf6.exe')
    function script:Wait-Handle { }
    function script:Invoke-Async($Sb, $X) { & $Sb $X }
    $AideTache = '\Mindset\Nevermind (aide)'; $AideExe = 'C:\Essai\NevermindAide.exe'; $AideSid = 'S-1-5-21-1-2-3-1001'; $AideSchtasks = 'schtasks.exe'
    $AideAttenteTache = 0; $AideAttenteWindows = 0; $AideDelaiRefus = 600; $AideFichierActions = ''
    $script:EstAdmin = $false
    $cas = New-Object System.Collections.ArrayList
    # Le texte exact des demandes
    $demandes = @(
        @('aide.bonjour', '{}'), @('reg.ecrire', '{"Cle":"HKLM:\\SOFTWARE\\X","Nom":"n","Valeur":1,"Type":"DWord"}'),
        @('processus.fermer', '{"Nom":"l''appli <test> & co"}'), @('fps.veiller', '{"Noms":["bf6.exe","jeu é.exe"]}'),
        @('restauration.creer', '{"Description":"Nevermind (manuel)"}'), @('reg.ecrire', '{"Cle":"HKLM:\\A","Nom":"B","Valeur":[1,2,255],"Type":"Binary"}'),
        @('dns.regler', '{"Index":12,"Serveurs":["1.1.1.1","1.0.0.1"],"Nom":"Wi-Fi"}'), @('lecture.pc', 'null'), @('x', '{"t":"\t\n\u0001\u2028"}')
    )
    $textes = @(foreach ($dm in $demandes) { @{ action = $dm[0]; parametres = $dm[1]; demande = (ConvertTo-Json -InputObject @{ a = $dm[0]; p = (ConvertFrom-JsonNu $dm[1]) } -Depth 8 -Compress) } })
    # Invoke-Aide : des suites de demandes, ce que la fausse Aide répond, ce qui est rendu et noté
    function Add-CasAide([string]$Nom, $Etapes) {
        $script:AideSansTache = $true; $script:AideSansDemande = $true; $script:AideRefus = $null; $script:AideVoie = $null
        $script:Journal.Clear(); [AideClient]::Recues.Clear()
        $res = @(foreach ($e in $Etapes) {
            [AideClient]::Ecoute = [bool]$e.ecoute; [AideClient]::Reponse = [string]$e.reponse; [AideClient]::Erreur = [string]$e.erreur
            $r = $null; $err = $null
            try { $r = Invoke-Aide $e.action (ConvertFrom-JsonNu $e.parametres) 300 } catch { $err = $_.Exception.Message }
            @{ action = $e.action; parametres = $e.parametres; ecoute = [bool]$e.ecoute; reponse = $e.reponse; erreurAide = $e.erreur
               resultat = $(if ($null -eq $err) { ConvertTo-Json -InputObject $r -Depth 20 -Compress }); erreur = $err }
        })
        [void]$cas.Add(@{ nom = $Nom; etapes = $res; journal = @($script:Journal); recues = @([AideClient]::Recues) })
    }
    $ok = { param($r) "{`"ok`":true,`"r`":$r}" }
    Add-CasAide 'réponse simple' @(@{ action = 'aide.bonjour'; parametres = '{}'; ecoute = $true; reponse = (& $ok '"bonjour"') })
    Add-CasAide 'deux réponses, une voie' @(@{ action = 'trafic.tcp'; parametres = '{}'; ecoute = $true; reponse = (& $ok '{"CompteursOk":true,"Lignes":["a","b"]}') }, @{ action = 'lecture.pc'; parametres = '{}'; ecoute = $true; reponse = (& $ok '{"SecureBoot":true,"Fiabilite":{}}') })
    Add-CasAide 'refusée par l''Aide' @(@{ action = 'reg.ecrire'; parametres = '{"Cle":"HKLM:\\X","Nom":"n","Valeur":1,"Type":"DWord"}'; ecoute = $true; reponse = '{"ok":false,"e":"Clé refusée : HKLM:\\X"}' })
    Add-CasAide 'Aide absente' @(@{ action = 'aide.bonjour'; parametres = '{}'; ecoute = $false })
    Add-CasAide 'absente puis là' @(@{ action = 'flux.lire'; parametres = '{}'; ecoute = $false }, @{ action = 'flux.lire'; parametres = '{}'; ecoute = $true; reponse = (& $ok '["l1","l2"]') }, @{ action = 'flux.lire'; parametres = '{}'; ecoute = $false })
    Add-CasAide 'réponse en retard' @(@{ action = 'fps.demarrer'; parametres = '{"Pid":42}'; ecoute = $true; erreur = 'L''Aide n''a pas répondu à temps.' })
    Add-CasAide 'coupée sans réponse' @(@{ action = 'aide.journal'; parametres = '{}'; ecoute = $true; erreur = 'L''Aide a coupé l''échange sans répondre.' }, @{ action = 'aide.journal'; parametres = '{}'; ecoute = $true; reponse = (& $ok '[]') })
    Add-CasAide 'réponse nulle et liste' @(@{ action = 'a'; parametres = '{}'; ecoute = $true; reponse = '{"ok":true,"r":null}' }, @{ action = 'b'; parametres = '{}'; ecoute = $true; reponse = (& $ok '[1,"deux",{"t":3}]') })
    Add-CasAide 'ok absent' @(@{ action = 'c'; parametres = '{}'; ecoute = $true; reponse = '{"r":1}' })
    # Get-VariablesDangereuses (le tri sur des noms donnés) et Test-FpsRefus
    $listes = @(@('PATH', 'COR_ENABLE_PROFILING', 'cor_profiler', 'DOTNET_TieredCompilation', 'COMPlus_x', 'TEMP'), @('DOTNET_STARTUP_HOOKS', 'dotnet_additional_deps', 'DOTNET_SHARED_STORE', 'DOTNET_SHARED_STOREX', 'CORECLR_A'), @(), @('Cor_A', 'COR_A', 'cor_b'))
    $variables = @(foreach ($l in $listes) { @{ noms = @($l); dangereuses = @(@($l | Where-Object { $_ -match $AideVariablesDangereuses } | Sort-Object -Unique)) } })
    $refus = @(foreach ($t in 'Access denied', 'accès refusé par Windows', 'Membre de Performance Log Users requis', 'ACCESS DENIED', 'Accès Refusé', 'autre erreur', '') { @{ texte = $t; refus = (Test-FpsRefus $t) } })
    # Get-FpsVeilleJeux / Add-FpsVeilleJeu
    $SettingsFile = Join-Path $D 'parametres.json'; Set-Fichiers $D @{ 'parametres.json' = (B64 '{"FpsVeille":["Jeu.EXE","bf6.exe",""]}') }
    $script:Settings = $null; $script:JsonBloques = @{}; $script:Journal.Clear()
    $v1 = @(Get-FpsVeilleJeux); Add-FpsVeilleJeu 'Autre'; Add-FpsVeilleJeu 'JEU.exe'; $v2 = @(Get-FpsVeilleJeux)
    $veille = @{ avant = $v1; apres = $v2; journal = @($script:Journal); reglage = (ConvertTo-Json -InputObject (Get-Setting 'FpsVeille') -Compress) }
    Save-Juge 'aide.json' @{ format = 1; sujet = 'Le canal de l''Aide (aide-client.ps1) : texte des demandes, Invoke-Aide (réponses, refus, absente, délai), variables dangereuses, veille des FPS'
        demandes = $textes; cas = @($cas); variables = $variables; refusFps = $refus; veille = $veille }

    # =======================================================================
    # 6. Mises à jour
    # =======================================================================
    Import-Fonction 'mises-a-jour' 'Test-NewerVersion'
    Import-Fonction 'mises-a-jour' 'Invoke-UpdateCheck'
    $GetReleaseScript = Invoke-Expression (Get-Variable-Module 'mises-a-jour' 'GetReleaseScript')
    $versions = @('1.2', '1.2.0', '1.2.0.0', '1.15.6', '1.15.10', '2.0', '1', '', 'a.b', '1.2.3.4.5', '01.2', '1.2 ', ' 1.2', '1.-2', '2147483647.0', '2147483648.0', '1.2.3-beta', 'v1.2', '0.0', '1.15.6.1')
    $comparaisons = @(foreach ($a in $versions) { foreach ($b in $versions) { @{ distante = $a; locale = $b; plus = (Test-NewerVersion $a $b) } } })
    $script:Reponses = @{}
    function script:Invoke-RestMethod { param($Uri, $Headers, $TimeoutSec)
        if (-not $script:Reponses.ContainsKey($Uri)) { throw "Le serveur distant a retourné une erreur : (404) Introuvable." }
        $r = $script:Reponses[$Uri]; if ($r -is [string]) { $r = ConvertFrom-Json $r }; $r
    }
    $api = 'https://api.github.com/repos/JordanJacquot/Nevermind/releases'
    $asset = { param($n) "{`"name`":`"$n`",`"browser_download_url`":`"https://github.com/dl/$n`"}" }
    $sorties = @(
        @{ nom = 'latest normal'; beta = $false; ancien = $false; reponses = @{ "$api/latest" = "{`"tag_name`":`"v1.16.0`",`"prerelease`":false,`"body`":`"- un\n- deux`",`"assets`":[$(& $asset 'Nevermind.zip'),$(& $asset 'Nevermind.zip.sig')]}" } }
        @{ nom = 'latest sans signature'; beta = $false; ancien = $false; reponses = @{ "$api/latest" = "{`"tag_name`":`"1.16.0`",`"assets`":[$(& $asset 'Nevermind.zip')]}" } }
        @{ nom = 'ancien nom'; beta = $false; ancien = $true; reponses = @{ "$api/latest" = "{`"tag_name`":`"V1.13.0`",`"assets`":[$(& $asset 'Nevermind.zip'),$(& $asset 'OptiGame.zip'),$(& $asset 'OptiGame.zip.sig')]}" } }
        @{ nom = 'seulement OptiGame'; beta = $false; ancien = $false; reponses = @{ "$api/latest" = "{`"tag_name`":`"v1.12.0`",`"assets`":[$(& $asset 'optigame.ZIP'),$(& $asset 'OptiGame.zip.SIG')]}" } }
        @{ nom = 'pas de zip'; beta = $false; ancien = $false; reponses = @{ "$api/latest" = "{`"tag_name`":`"v2.0`",`"assets`":[$(& $asset 'autre.zip')]}" } }
        @{ nom = 'erreur réseau'; beta = $false; ancien = $false; reponses = @{} }
        @{ nom = 'bêta : la première pas brouillon'; beta = $true; ancien = $false; reponses = @{ "$api`?per_page=10" = "[{`"draft`":true,`"tag_name`":`"v9`"},{`"draft`":false,`"prerelease`":true,`"tag_name`":`"v1.17.0-b`",`"assets`":[$(& $asset 'Nevermind.zip')]},{`"tag_name`":`"v1.16.0`"}]" } }
        @{ nom = 'bêta : liste vide'; beta = $true; ancien = $false; reponses = @{ "$api`?per_page=10" = '[]' } }
        @{ nom = 'bêta : une seule'; beta = $true; ancien = $false; reponses = @{ "$api`?per_page=10" = "[{`"tag_name`":`"v1.16.1`",`"prerelease`":true,`"assets`":[$(& $asset 'Nevermind.zip'),$(& $asset 'Nevermind.zip.sig')]}]" } }
    )
    $publications = @(foreach ($s in $sorties) {
        $script:Reponses = $s.reponses
        $r = & $GetReleaseScript @{ Repo = 'JordanJacquot/Nevermind'; Beta = $s.beta; Ancien = $s.ancien }
        @{ nom = $s.nom; beta = $s.beta; ancien = $s.ancien; reponses = $s.reponses; resultat = $r }
    })
    # Invoke-UpdateCheck : les textes
    $AppVersion = '1.15.6'; $UpdateRepo = 'JordanJacquot/Nevermind'
    function script:Get-AppScript { 'C:\Essai\fichiers\Nevermind.ps1' }
    function script:Test-MajAuto { [bool](Get-Setting 'MajAuto' $true) }
    $script:Messages = New-Object System.Collections.Generic.List[string]
    function script:Show-Message([string]$T, [string]$N) { $script:Messages.Add("$N|$T") }
    function script:Set-Status([string]$T) { $script:Messages.Add("statut|$T") }
    $verifs = @(foreach ($v in @(
        @{ nom = 'géré par Mindset'; geree = $true; manuel = $false; reglages = '{}'; sortie = 0 }
        @{ nom = 'recherche coupée'; geree = $false; manuel = $false; reglages = '{"MajAuto":false}'; sortie = 0 }
        @{ nom = 'recherche coupée, bouton'; geree = $false; manuel = $true; reglages = '{"MajAuto":false}'; sortie = 0 }
        @{ nom = 'nouvelle version'; geree = $false; manuel = $false; reglages = '{}'; sortie = 0 }
        @{ nom = 'nouvelle bêta'; geree = $false; manuel = $true; reglages = '{"Beta":true}'; sortie = 6 }
        @{ nom = 'pas de connexion, au lancement'; geree = $false; manuel = $false; reglages = '{}'; sortie = 5 }
        @{ nom = 'pas de connexion, bouton'; geree = $false; manuel = $true; reglages = '{}'; sortie = 5 }
        @{ nom = 'à jour, bouton'; geree = $false; manuel = $true; reglages = '{}'; sortie = 2; version = '1.16.0' }
        @{ nom = 'à jour, au lancement'; geree = $false; manuel = $false; reglages = '{}'; sortie = 2; version = '1.16.0' }
        @{ nom = 'plus ancienne en ligne'; geree = $false; manuel = $true; reglages = '{}'; sortie = 0; version = '1.20.0' }
    )) {
        $Managed = $v.geree; $AppVersion = if ($v.version) { $v.version } else { '1.15.6' }
        Set-Fichiers $D @{ 'parametres.json' = (B64 $v.reglages) }; $script:Settings = $null; $script:JsonBloques = @{}
        $script:Reponses = $sorties[$v.sortie].reponses; $script:PendingUpdate = $null; $script:Messages.Clear(); $script:Journal.Clear()
        $ui = @{ UpdateStatus = [pscustomobject]@{ Text = '' }; UpdateText = [pscustomobject]@{ Text = '' }; UpdateBanner = [pscustomobject]@{ Visibility = 'Collapsed' }; BtnCheckUpdate = [pscustomobject]@{ Content = 'Vérifier les mises à jour' } }
        if ($v.manuel) { Invoke-UpdateCheck -Manual } else { Invoke-UpdateCheck }
        @{ nom = $v.nom; geree = $v.geree; manuel = $v.manuel; reglages = $v.reglages; version = $AppVersion; reponses = $script:Reponses
           statut = $ui.UpdateStatus.Text; bandeau = $(if ($ui.UpdateBanner.Visibility -eq 'Visible') { $ui.UpdateText.Text } else { '' }); bouton = $ui.BtnCheckUpdate.Content
           enAttente = $(if ($script:PendingUpdate) { $script:PendingUpdate.Version }); messages = @($script:Messages); journal = @($script:Journal) }
    })
    $Managed = $false
    # Une clé et une signature d'essai (RSA 3072, SHA-256, comme les vraies) pour l'installation
    $rsa = New-Object System.Security.Cryptography.RSACryptoServiceProvider 3072
    $ms = New-Object IO.MemoryStream
    Add-Type -AssemblyName System.IO.Compression
    $za = New-Object IO.Compression.ZipArchive($ms, [IO.Compression.ZipArchiveMode]::Create, $true)
    foreach ($e in @(@('Nevermind/fichiers/Nevermind.ps1', "`$AppVersion = '1.16.0'"), @('Nevermind/fichiers/modules/nouveau.ps1', '# nouveau'), @('Nevermind/Nevermind.exe', 'exe'), @('Nevermind/fichiers/ui/', ''))) {
        $en = $za.CreateEntry($e[0])
        if ($e[1]) { $w = New-Object IO.StreamWriter($en.Open(), $utf8); $w.Write($e[1]); $w.Dispose() }
    }
    $za.Dispose()
    $zip = $ms.ToArray()
    $sig = $rsa.SignData($zip, [System.Security.Cryptography.CryptoConfig]::MapNameToOID('SHA256'))
    $installation = @{ cle = $rsa.ToXmlString($false); zip = [Convert]::ToBase64String($zip); signature = [Convert]::ToBase64String($sig) }
    $rsa.Dispose()
    Save-Juge 'maj.json' @{ format = 1; sujet = 'Mises à jour (mises-a-jour.ps1) : la version choisie, Test-NewerVersion, les textes de Invoke-UpdateCheck, un zip signé d''essai'
        comparaisons = $comparaisons; publications = $publications; cas = $verifs; installation = $installation }

    # =======================================================================
    # 7. Assistance
    # =======================================================================
    foreach ($n in 'ConvertFrom-HistoryEntry', 'Test-HistoriqueDefinitif', 'Test-HistoriqueNote', 'Test-HistoriquePartDefinitive', 'Get-RetourDefinitif', 'Format-ValeurTech',
                   'Test-ChampHisto', 'Format-DemarrageService', 'Format-PlanAlim', 'Format-CurseurAlim', 'Format-ServeursDns', 'Get-DetailsTechniques', 'Get-HistoryItems', 'Get-HistoryTitle', 'Import-History') { Import-Fonction 'assistance' $n }
    $HighPerfGuid = '8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c'; $BalancedGuid = '381b4222-f694-41f0-9685-ff5bb260df2e'; $BestPerfOverlay = 'ded574b5-45a0-4f42-8737-46345c09c238'; $UltimateGuid = 'e9a42b02-d5df-448d-aa00-03f14749eb61'
    $BatSettings = [ordered]@{ CritAction = '637ea02f-bbcb-4015-8e2c-a1c7b9c0b546'; CritLevel = '9a66d8d7-4ff7-4ef9-b5a2-5a326ca2a469'; LowLevel = '8183ba9a-e910-48da-8769-14ae6dc1170a'; LowNotify = 'bcded951-187b-4d05-bccc-f7e51960c258'; LowAction = 'd8742dcb-3e6a-4b3c-b3fe-374623cdcf06' }
    $BatActions = @('ne rien faire', 'mise en veille', 'mise en veille prolongée', 'arrêt du PC')
    $BatNoms = @{ CritAction = 'action à batterie critique'; CritLevel = 'niveau de batterie critique'; LowLevel = 'niveau de batterie faible'; LowNotify = 'alerte de batterie faible'; LowAction = 'action à batterie faible' }
    $tok = $null; $err = $null
    $tdb = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules 'tableau-de-bord.ps1'), [ref]$tok, [ref]$err)
    Invoke-Expression ($tdb.Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Format-ValeurBat' }, $false).Extent.Text)
    $script:Backup = @{ CreatedScheme = '11111111-2222-3333-4444-555555555555' }
    $long = 'v' * 130
    $entrees = @(
        '{"Type":"reg","Path":"HKCU:\\Control Panel\\Mouse","Name":"MouseSpeed","Existed":true,"Value":"1","Kind":"String","Nouveau":"0"}'
        '{"Type":"reg","Path":"HKLM:\\X","Name":"N","Existed":false,"Value":null,"Kind":null}'
        '{"Type":"reg","Path":"HKLM:\\X","Name":"N","Existed":true,"Value":5,"Kind":"DWord","Efface":true}'
        '{"Type":"reg","Path":"HKLM:\\X","Name":"B","Existed":true,"Value":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18],"Kind":"Binary","Nouveau":[0]}'
        '{"Type":"reg","Path":"HKLM:\\X","Name":"Q","Existed":true,"Value":"12345678901","Kind":"QWord","Nouveau":null}'
        '{"Type":"reg","Path":"HKLM:\\X","Name":"M","Existed":true,"Value":["a","b"],"Kind":"MultiString"}'
        '{"Type":"reg","Path":"HKLM:\\X","Name":"D","Existed":true,"Value":"7","Kind":"DWord"}'
        "{`"Type`":`"reg`",`"Path`":`"HKLM:\\X`",`"Name`":`"L`",`"Existed`":true,`"Value`":`"$long`",`"Kind`":`"String`"}"
        '{"Type":"svc","Name":"DiagTrack","StartType":"Automatic","Running":true,"Nouveau":"Disabled"}'
        '{"Type":"svc","Name":"SysMain","StartType":"Manual","Running":false}'
        '{"Type":"svc","Name":"X","StartType":"Boot"}'
        '{"Type":"power","Guid":"381b4222-f694-41f0-9685-ff5bb260df2e","Nom":"","Nouveau":"8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"}'
        '{"Type":"power","Guid":"aaaa","Nom":"Mon plan"}'
        '{"Type":"power","Guid":"11111111-2222-3333-4444-555555555555","Nouveau":"e9a42b02-d5df-448d-aa00-03f14749eb61"}'
        '{"Type":"pcfg","Key":"CritAction","Guid":"637ea02f-bbcb-4015-8e2c-a1c7b9c0b546","Ac":0,"Dc":2,"Nouveau":3}'
        '{"Type":"pcfg","Guid":"9a66d8d7-4ff7-4ef9-b5a2-5a326ca2a469","Ac":5,"Dc":7}'
        '{"Type":"pcfg","Key":"LowNotify","Ac":1,"Dc":0,"Nouveau":1}'
        '{"Type":"pcfg","Key":"LowAction","Ac":9,"Dc":null}'
        '{"Type":"pcfg","Guid":"ffff","Ac":1,"Dc":2,"Nouveau":3}'
        '{"Type":"display","Device":"\\\\.\\DISPLAY1","Hz":60,"Nouveau":144}'
        '{"Type":"display","Device":"\\\\.\\DISPLAY2","Hz":75}'
        '{"Type":"overlay","Guid":"00000000-0000-0000-0000-000000000000","Nouveau":"ded574b5-45a0-4f42-8737-46345c09c238"}'
        '{"Type":"dns","Servers":["192.168.1.1",""],"Nouveau":["1.1.1.1","1.0.0.1"]}'
        '{"Type":"dns","Servers":[]}'
        '{"Type":"dns","Servers":null,"Nouveau":[]}'
        '{"Type":"fw","Name":"Nevermind - Bloquer jeu.exe"}'
        '{"Type":"definitif","Quoi":"SMBv1 coupé","Retour":"Panneau de configuration, Fonctionnalités."}'
        '{"Type":"note","Quoi":"Appli fermée depuis la page RAM","Chemins":["C:\\a.exe","C:\\b.exe"]}'
        '{"Type":"note","Quoi":"Rien d''autre"}'
        '{"Type":"inconnu","Quoi":"x"}'
    )
    $details = @(foreach ($t in $entrees) {
        $e = ConvertFrom-HistoryEntry (ConvertFrom-Json $t)
        @{ entree = $t; convertie = (ConvertTo-Json -InputObject $e -Depth 10 -Compress); details = @(Get-DetailsTechniques @{ Log = @($e) }) }
    })
    $groupes = @(
        @(), @('{"Type":"definitif","Quoi":"a","Retour":"r1"}'), @('{"Type":"definitif","Quoi":"a","Retour":"r1"}', '{"Type":"definitif","Quoi":"b","Retour":"r1"}', '{"Type":"definitif","Quoi":"c","Retour":"r2"}'),
        @('{"Type":"note","Quoi":"n"}'), @('{"Type":"note","Quoi":"n"}', '{"Type":"reg","Path":"HKCU:\\A","Name":"b","Existed":false}'),
        @('{"Type":"definitif","Quoi":"a"}', '{"Type":"svc","Name":"x","StartType":"Manual"}')
    )
    $qualites = @(foreach ($g in $groupes) {
        $h = @{ Log = @($g | ForEach-Object { ConvertFrom-HistoryEntry (ConvertFrom-Json $_) }) }
        @{ log = @($g); definitif = [bool](Test-HistoriqueDefinitif $h); note = [bool](Test-HistoriqueNote $h); partDefinitive = [bool](Test-HistoriquePartDefinitive $h); retour = [string](Get-RetourDefinitif $h) }
    })
    $titres = @(foreach ($l in @(@(), @('Rien'), @('•  Un seul changement'), @('•  a', '•  b'), @('Intro', '•  a', 'x', '•b', '•   c', '• d'), @('•  1', '•  2', '•  3', '•  4', '•  5'), @('pas de puce', '- tiret'))) {
        @{ titre = 'Titre de la fiche'; lignes = @($l); resultat = (Get-HistoryTitle 'Titre de la fiche' $l); elements = @(Get-HistoryItems $l) }
    })
    # Import-History : un historique.json lu (types remis)
    $HistoryFile = Join-Path $D 'historique.json'
    Set-Fichiers $D @{ 'historique.json' = (B64 "[{`"Id`":`"abc123def456`",`"Date`":`"06/11/2026 à 21:05`",`"Titre`":`"Souris`",`"Details`":[`"a`",`"`",null],`"Log`":[$($entrees[0]),$($entrees[3]),$($entrees[22]),null],`"Annule`":false},null,{`"Id`":7,`"Titre`":null,`"Annule`":1}]") }
    $script:JsonBloques = @{}
    Import-History
    $historique = @{ fichier = (Get-Fichiers $D)['historique.json']; lu = (ConvertTo-Json -InputObject @($script:History) -Depth 10 -Compress) }
    # « Quoi de neuf » : les lignes des notes
    $notes = @("- un`n- deux`r`n`r`n* trois", "", "   ", "Titre`n-pas un tiret`n-  deux espaces`n*`tavec tab`n  - indenté", "• déjà`n1. étape")
    $nouveautes = @(foreach ($n in $notes) { @{ notes = $n; lignes = @($n -split "`r?`n" | ForEach-Object { $_.Trim() -replace '^[-*]\s+', '•  ' } | Where-Object { $_ }) } })
    # Le masque des données personnelles (Export-ProblemReport)
    $maskAst = $asts['assistance'].Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$mask' }, $true)
    $mask = Invoke-Expression $maskAst.Right.Extent.Text
    $vrais = @{ p = $env:USERPROFILE; u = $env:USERNAME; c = $env:COMPUTERNAME }
    $masques = New-Object System.Collections.ArrayList
    try {
        foreach ($m in @(@{ p = 'C:\Users\jorda'; u = 'Jordan'; c = 'PC-JORDAN' }, @{ p = 'C:\Users\Élo'; u = 'Élodie'; c = 'BUREAU' }, @{ p = 'C:\Users\ab'; u = 'ab'; c = 'X1' }, @{ p = 'D:\Profils\Marc'; u = 'Marc'; c = 'Marc-PC' })) {
            $env:USERPROFILE = $m.p; $env:USERNAME = $m.u; $env:COMPUTERNAME = $m.c
            foreach ($t in @('C:\Users\jorda\AppData\x', 'c:\users\JORDA\doc', 'C:\Users\jordan\x', 'C:\Users\jorda', 'C:\Users\jorda.old\x', 'C:\Users\jorda-2', 'Jordan sur PC-JORDAN', 'jordan', 'C:\Users\Élo\é', 'C:\Users\Éloise', 'ab ab abc', 'Marc-PC de Marc', 'rien')) {
                [void]$masques.Add(@{ profil = $m.p; compte = $m.u; pc = $m.c; texte = $t; masque = (& $mask $t) })
            }
        }
    } finally { $env:USERPROFILE = $vrais.p; $env:USERNAME = $vrais.u; $env:COMPUTERNAME = $vrais.c }
    Save-Juge 'assistance.json' @{ format = 1; sujet = 'Assistance (assistance.ps1) : historique, détails techniques, titres, nouveautés, masque des données personnelles'
        cas = $details; qualites = $qualites; titres = $titres; historique = $historique; nouveautes = $nouveautes; masques = @($masques) }

    # =======================================================================
    # 8. Sauvegarde (Import-Backup)
    # =======================================================================
    foreach ($n in 'Get-CleRegNormale', 'Read-FichierSauvegarde', 'Import-Backup', 'Get-TexteProblemeSauvegarde') { Import-Fonction 'donnees' $n }
    $script:FaussesOrigines = @()
    function script:Read-OriginesRegistre { @($script:FaussesOrigines | ForEach-Object { ConvertFrom-Json $_ }) }
    $BackupFile = Join-Path $D 'sauvegarde.json'; $DataDir = $D
    $cas = New-Object System.Collections.ArrayList
    $bon = '{"Registry":{"HKCU:\\A|b":{"Path":"HKCU:\\A","Name":"b","Existed":true,"Value":1,"Kind":"DWord"}},"PowerScheme":"381b4222","CreatedScheme":null,"Overlay":"","Dns":{"12":"1.1.1.1"},"Displays":{"\\\\.\\DISPLAY1":"60"},"Oublies":["HKLM:\\Z|y",""]}'
    function Add-CasSauvegarde([string]$Nom, $Avant, $Origines) {
        Set-Fichiers $D $Avant
        $script:FaussesOrigines = @($Origines); $script:History = $null; $script:JsonBloques = @{}; $script:Journal.Clear()
        Import-Backup
        [void]$cas.Add(@{ nom = $Nom; avant = $Avant; origines = @($Origines); sauvegarde = (ConvertTo-Json -InputObject $script:Backup -Depth 10 -Compress)
            probleme = $script:BackupProbleme; texte = (Get-TexteProblemeSauvegarde); apres = @((Get-Fichiers $D).Keys); journal = @(Get-JournalRelatif $D) })
    }
    Add-CasSauvegarde 'absente, rien à reprendre' @{} @()
    Add-CasSauvegarde 'bonne' @{ 'sauvegarde.json' = (B64 $bon -Bom) } @()
    Add-CasSauvegarde 'abîmée, copie bonne' @{ 'sauvegarde.json' = (B64 '{"Registry":'); 'sauvegarde.json.bak' = (B64 $bon) } @()
    Add-CasSauvegarde 'sans Registry' @{ 'sauvegarde.json' = (B64 '{"Dns":{}}') } @()
    Add-CasSauvegarde 'absente, origines du registre' @{} @('{"Path":"HKLM:\\SOFTWARE\\X","Name":"n","Existed":true,"Value":3,"Kind":"DWord"}', '{"Path":"","Name":"x"}', '{"Path":"Registry::HKEY_LOCAL_MACHINE\\SOFTWARE\\Y","Name":"m","Existed":false}')
    Add-CasSauvegarde 'bonne, origines déjà connues ou oubliées' @{ 'sauvegarde.json' = (B64 $bon) } @('{"Path":"HKEY_CURRENT_USER\\A","Name":"b","Existed":false}', '{"Path":"HKLM:\\Z","Name":"y","Existed":true,"Value":1}', '{"Path":"HKLM:\\W","Name":"v","Existed":true,"Value":"t","Kind":"String"}')
    Add-CasSauvegarde 'disparue, historique pas annulé' @{ 'historique.json' = (B64 '[{"Id":"a","Titre":"t","Annule":false,"Log":[{"Type":"reg","Path":"HKLM:\\X","Name":"n"}]}]') } @()
    Add-CasSauvegarde 'disparue, historique annulé' @{ 'historique.json' = (B64 '[{"Id":"a","Titre":"t","Annule":true,"Log":[{"Type":"reg","Path":"HKLM:\\X","Name":"n"}]}]') } @()
    Save-Juge 'sauvegarde.json' @{ format = 1; sujet = 'Import-Backup (donnees.ps1) : sauvegarde lue, problème, texte, fichiers, journal'; cas = @($cas) }

    # =======================================================================
    # 9. Les pages de l'assistance (fiche et panneau de l'écran), lues dans de vrais objets WPF jamais affichés
    # =======================================================================
    Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase
    $asts['interface'] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules 'interface.ps1'), [ref]$tok, [ref]$err)
    $asts['tableau-de-bord'] = $tdb
    foreach ($f in 'ecran-pc', 'ecran-panneau', 'tests', 'accueil-console') { $asts[$f] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$f.ps1"), [ref]$tok, [ref]$err) }
    foreach ($n in 'Get-Brush', 'New-Thickness', 'New-Text', 'New-Grid', 'Add-ToGrid', 'New-Button') { Import-Fonction 'interface' $n }
    foreach ($n in 'Add-SheetInfo', 'Show-ResultSheet', 'Close-Sheet') { Import-Fonction 'tableau-de-bord' $n }
    foreach ($n in 'Show-Tour', 'Show-TourStep', 'Show-ReportPanel') { Import-Fonction 'assistance' $n }
    foreach ($n in 'Get-TonKit', 'Get-FicheKit') { Import-Fonction 'ecran-pc' $n }
    foreach ($n in 'Get-TexteWpf', 'Get-ContenuWpf', 'Test-DessinWpf', 'Test-JaugeWpf', 'Add-BlocKitWpf', 'Add-BlocsWpf', 'Get-TextesWpf', 'Test-ControlesWpf', 'Get-PanneauKit') { Import-Fonction 'ecran-panneau' $n }
    foreach ($n in 'Show-TestPanel', 'Set-TestButtons', 'Set-TestState') { Import-Fonction 'tests' $n }
    function script:ConvertTo-ThemeHex([string]$H) { $H }
    function script:Get-UiFontSize([double]$S) { $S }
    function script:Set-Setting { }
    function script:Start-Pulse { }
    function script:Stop-Pulse { }
    function script:Start-WpfAnim { }
    function script:Start-FromTo { }
    $script:Styles = @{ BtnPrimary = (New-Object System.Windows.Style); BtnSecondary = (New-Object System.Windows.Style) }
    $Window = New-Object PSObject
    $Window | Add-Member ScriptMethod FindResource { param($n) $script:Styles[$n] }
    $Window | Add-Member ScriptMethod TryFindResource { param($n) $script:Styles[$n] }
    $Colors = Invoke-Expression (Get-Variable-Module 'interface' 'Colors')
    $IconesTon = Invoke-Expression (Get-Variable-Module 'ecran-pc' 'IconesTon')
    $IconesPanneau = Invoke-Expression (Get-Variable-Module 'ecran-panneau' 'IconesPanneau')
    $tsAst = $asts['accueil-console'].Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq '$script:TourSteps' }, $true)
    $TourSteps = Invoke-Expression $tsAst.Right.Extent.Text
    $nouveau = { param($T) $o = New-Object "System.Windows.Controls.$T"; $o }
    $ui = @{ SheetBody = (& $nouveau StackPanel); SheetRun = (& $nouveau Button); SheetOpen = (& $nouveau Button); SheetIgnore = (& $nouveau Button); SheetClose = (& $nouveau Button); Overlay = (& $nouveau Border)
             TestTag = (& $nouveau TextBlock); TestTitle = (& $nouveau TextBlock); TestSub = (& $nouveau TextBlock); TestBody = (& $nouveau StackPanel); TestProgress = (& $nouveau ProgressBar)
             TestPct = (& $nouveau TextBlock); TestStateDot = (New-Object System.Windows.Shapes.Ellipse); TestStateText = (& $nouveau TextBlock); BtnTestStop = (& $nouveau Button); BtnTestAgain = (& $nouveau Button)
             BtnTestClose = (& $nouveau Button); BtnTestX = (& $nouveau Button); TestOverlay = (& $nouveau Border); TestCard = (& $nouveau Border); TestCardScale = (New-Object System.Windows.Media.ScaleTransform); Tabs = [pscustomobject]@{ SelectedIndex = 0 } }
    $ui.BtnTestAgain.Content = 'Recommencer'; $ui.BtnTestStop.Content = 'Arrêter'
    $pages = New-Object System.Collections.ArrayList
    foreach ($de in 'parametres', 'accueil', 'aide') {
        $script:Kit = @{ FicheDe = $de; PanneauDe = $de }
        Show-Tour
        for ($i = 0; $i -lt $TourSteps.Count; $i++) { Show-TourStep $i; [void]$pages.Add(@{ quoi = 'visite'; etape = $i; de = $de; page = (ConvertTo-Json -InputObject (Get-FicheKit) -Depth 10 -Compress) }) }
        foreach ($r in @(@{ t = 'Quoi de neuf dans la version 1.16.0'; l = @('•  un', '•  deux', 'Titre', '•  trois', '1. étape', '2. suite', 'fin'); n = 'Tu avais la version 1.15.5.' }, @{ t = 'Quoi de neuf dans la version 1.16.0'; l = @('Corrections et améliorations.'); n = 'Nevermind vient d''être mis à jour.' }, @{ t = 'Sans note'; l = @(); n = '' })) {
            Show-ResultSheet $r.t $r.l $null $r.n
            [void]$pages.Add(@{ quoi = 'resultat'; titre = $r.t; lignes = @($r.l); note = $r.n; de = $de; page = (ConvertTo-Json -InputObject (Get-FicheKit) -Depth 10 -Compress) })
        }
        $script:TestRunning = $false
        Show-ReportPanel
        [void]$pages.Add(@{ quoi = 'signalement'; de = $de; page = (ConvertTo-Json -InputObject (Get-PanneauKit) -Depth 10 -Compress) })
    }
    Save-Juge 'pages.json' @{ format = 1; sujet = 'La fiche (Get-FicheKit) de la visite guidée et des fiches de résultat, le panneau (Get-PanneauKit) de « Signaler un problème »'; cas = @($pages) }
} finally {
    try { [IO.Directory]::Delete($Bac, $true) } catch {}
}
