# Tests « ne jamais perdre de données », sans droits administrateur et sans rien toucher sur le PC.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-donnees.ps1
#
# Ce script contrôle :
#   1. l'écriture sûre des fichiers de données (Save-JsonSur) : à côté puis remplacé d'un coup, l'ancien en copie de
#      secours (.bak) ; à la lecture (Read-JsonSur), un fichier abîmé est mis de côté et noté au journal, sa copie de
#      secours reprise ; un fichier pris par un autre programme n'est jamais réécrit ;
#   2. les préférences (Get-Setting) : un parametres.json abîmé ne fait pas perdre SvcOriginal (de quoi remettre les
#      services comme avant) ; un état d'origine qui n'a pas pu être écrit (préférences ou sauvegarde.json pris par un
#      autre programme) : rien n'est changé (audit du 25/10, 25b) ;
#   3. « Tout restaurer » avec une Aide administrateur qui ne répond pas : la sauvegarde garde ce qui n'a pas pu être
#      remis, seules les entrées vraiment remises de l'historique sont marquées annulées, et « Réessayer » finit le travail ;
#      Puis les couleurs d'un pack de thème : seulement « #RRGGBB », sinon le thème de base (audit du 25/10, constat 7) ;
#   4. les minuteries importantes (ordres de Mindset, thème de la suite, attente, écran) sont protégées de l'intérieur ;
#   5. le mode jeu : les applis fermées pendant la partie sont écrites dans les données et relancées au lancement suivant
#      si Nevermind a planté pendant le jeu (audit du 25/10, constat 13).
# Le code de Nevermind n'est pas chargé en entier : seules les fonctions utiles sont lues dans les modules, et tout ce qui
# pourrait toucher au PC (registre, Aide, services, alimentation) est remplacé par une fausse version.
# Code de sortie : 0 si tout passe, 1 sinon.

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0
function Test-Ok([bool]$Condition, [string]$Message) {
    $script:nbTests++
    if (-not $Condition) { $script:echecs.Add($Message) }
}

# ---------------------------------------------------------------------------
# Sécurité du test : rien ne doit pouvoir modifier le PC (une fonction passe avant une commande du même nom)
# ---------------------------------------------------------------------------
$script:Journal = New-Object System.Collections.Generic.List[string]
function script:Write-Log([string]$Message) { $script:Journal.Add($Message) }
$script:AideRepond = $false
$script:AideAppels = New-Object System.Collections.Generic.List[string]
function script:Invoke-Aide([string]$Action, $Params) {
    $script:AideAppels.Add($Action)
    if (-not $script:AideRepond) { throw 'L''Aide administrateur ne répond pas.' }
    if ($Action -eq 'origines.oublier') { $script:FaussesOrigines = @() }   # (comme l'Aide : ses copies d'origine sont effacées)
    'ok'
}
# Faux registre : rien n'est écrit dans le vrai. Les réglages du PC passent par l'Aide, comme dans Nevermind.
$script:FauxRegistre = @{}
function script:Write-RegValue([string]$Path, [string]$Name, $Value, [string]$Kind) {
    if (-not (Test-RegUtilisateur $Path)) { [void](Invoke-Aide 'reg.ecrire' @{}) }
    $script:FauxRegistre["$Path|$Name"] = $Value
}
function script:Remove-RegValue([string]$Path, [string]$Name) {
    if (-not (Test-RegUtilisateur $Path)) { [void](Invoke-Aide 'reg.effacer' @{}) }
    $script:FauxRegistre.Remove("$Path|$Name")
}
function script:Get-NetFirewallRule { }
function script:powercfg { }
function script:Sync-Mouse { }
$script:FaussesOrigines = @()
function script:Read-OriginesRegistre { @($script:FaussesOrigines) }   # (jamais les vraies valeurs gardées par l'Aide)
function script:Save-History { }
function script:Update-HistoryList { }

# ---------------------------------------------------------------------------
# Lecture des fonctions utiles dans les modules (rien ne se lance au chargement)
# ---------------------------------------------------------------------------
$asts = @{}
foreach ($f in 'donnees', 'systeme', 'assistance', 'evenements', 'interface', 'accueil-console', 'microsoft') {
    $tok = $null; $err = $null
    $asts[$f] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$f.ps1"), [ref]$tok, [ref]$err)
    if ($err) { Write-Host "$f.ps1 n'est pas lisible : $($err[0].Message)" -ForegroundColor Red; exit 1 }
}
function Import-Fonction([string]$Fichier, [string]$Nom) {
    $a = $asts[$Fichier].Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $false)
    if (-not $a) { Write-Host "Fonction $Nom introuvable dans $Fichier.ps1" -ForegroundColor Red; exit 1 }
    Invoke-Expression ($a.Extent.Text -replace "^function\s+$([regex]::Escape($Nom))", "function script:$Nom")
}
foreach ($n in 'Save-TexteSur', 'Save-JsonSur', 'Read-JsonSur', 'Get-Setting', 'Set-Setting', 'Get-CleRegNormale', 'Read-FichierSauvegarde',
               'Import-Backup', 'Save-Backup', 'Test-RegUtilisateur', 'Save-Original', 'Save-BackupOuOublier') { Import-Fonction 'donnees' $n }
Import-Fonction 'microsoft' 'Set-ServiceOff'
function script:Get-Service { [pscustomobject]@{ Name = 'DiagTrack'; StartType = 'Automatic'; Status = 'Running' } }   # (faux service : rien n'est lu)
foreach ($n in 'Restore-AllSettings', 'Test-HistoriqueRemis') { Import-Fonction 'systeme' $n }
foreach ($n in 'Set-HistoryAllUndone', 'Test-HistoriqueDefinitif', 'Test-HistoriqueNote') { Import-Fonction 'assistance' $n }

$script:Bac = Join-Path $env:TEMP "nevermind-donnees-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
New-Item -ItemType Directory -Force -Path $script:Bac | Out-Null
$script:Bac = [IO.Path]::GetFullPath($script:Bac)
# (clés d'origine du test : des noms qui n'existent pas, rien n'est lu ni effacé dans le vrai registre)
$OriginesMachine = "SOFTWARE\MindsetTest-$([guid]::NewGuid().ToString('N'))\Origines"
$OriginesUtilisateur = "Software\MindsetTest-$([guid]::NewGuid().ToString('N'))\Origines"
function Get-Abimes([string]$Nom) { @(Get-ChildItem -LiteralPath $script:Bac -Filter "$Nom*.abime-*" -File -ErrorAction SilentlyContinue) }

try {
    # -----------------------------------------------------------------------
    # 1. Écriture et lecture sûres
    # -----------------------------------------------------------------------
    $f = Join-Path $script:Bac 'fps.json'
    Save-JsonSur $f @(@{ Id = 'a' })
    Save-JsonSur $f @(@{ Id = 'a' }, @{ Id = 'b' })
    Test-Ok ([IO.File]::Exists($f) -and [IO.File]::Exists("$f.bak") -and -not [IO.File]::Exists("$f.tmp")) 'Écriture : fichier, copie de secours (.bak), pas de .tmp qui traîne'
    $l = Read-JsonSur $f
    Test-Ok (@($l).Count -eq 2) "Écriture : le fichier a la dernière version ($(@($l).Count) élément(s))"
    $un = @(Read-JsonSur "$f.bak")
    Test-Ok ($un.Count -eq 1 -and $un[0].Id -eq 'a') 'Lecture : la copie de secours a la version d''avant'

    # Fichier abîmé (arrêt brutal au milieu d'une écriture) : la copie de secours est reprise, l'abîmé mis de côté
    $script:Journal.Clear()
    [IO.File]::WriteAllText($f, '[{"Id":"a"},{"Id":')
    $l = Read-JsonSur $f
    Test-Ok (@($l).Count -eq 1 -and @($l)[0].Id -eq 'a') 'Fichier abîmé : sa copie de secours est reprise (pas une liste vide)'
    Test-Ok ((Get-Abimes 'fps.json').Count -eq 1 -and -not [IO.File]::Exists($f)) 'Fichier abîmé : mis de côté (.abime-<date>), rien n''est effacé'
    Test-Ok (@($script:Journal | Where-Object { $_ -like 'Fichier abîmé*' }).Count -eq 1 -and @($script:Journal | Where-Object { $_ -like '*repris de sa copie de secours*' }).Count -eq 1) 'Fichier abîmé : noté au journal, et la reprise aussi'
    Save-JsonSur $f @(@($l) + @{ Id = 'c' })
    Test-Ok (@(Read-JsonSur $f).Count -eq 2 -and @(Read-JsonSur "$f.bak").Count -eq 1) 'Après un fichier abîmé : l''enregistrement suivant garde tout, la copie de secours reste bonne'

    # Fichier vide (coupure de courant) : abîmé aussi
    $g = Join-Path $script:Bac 'lag.json'
    Save-JsonSur $g @(@{ N = 1 }, @{ N = 2 }); Save-JsonSur $g @(@{ N = 1 }, @{ N = 2 }, @{ N = 3 })
    [IO.File]::WriteAllText($g, '')
    Test-Ok (@(Read-JsonSur $g).Count -eq 2) 'Fichier vide : sa copie de secours est reprise'

    # Les deux abîmés : $null (pas d'erreur), les deux mis de côté
    $h = Join-Path $script:Bac 'jeux.json'
    [IO.File]::WriteAllText($h, '{abîmé'); [IO.File]::WriteAllText("$h.bak", 'pas du json')
    $r = Read-JsonSur $h
    Test-Ok ($null -eq $r -and (Get-Abimes 'jeux.json').Count -eq 2 -and -not [IO.File]::Exists($h) -and -not [IO.File]::Exists("$h.bak")) 'Fichier et copie de secours abîmés : rien, mais les deux sont gardés de côté'

    # Fichier pris par un autre programme : jamais réécrit (il garde peut-être plus que ce qu'on a lu)
    $o = Join-Path $script:Bac 'organizer.json'
    Save-JsonSur $o @{ Order = @('a', 'b', 'c') }
    [IO.File]::Delete("$o.bak")
    $verrou = [IO.File]::Open($o, 'Open', 'ReadWrite', 'None')
    try {
        $r = Read-JsonSur $o
        $refuse = $false
        try { Save-JsonSur $o @{ Order = @() } } catch { $refuse = $true }
    } finally { $verrou.Close() }
    Test-Ok ($null -eq $r -and $refuse) 'Fichier pris par un autre programme : pas lu, et pas réécrit vide'
    Test-Ok (@((Read-JsonSur $o).Order).Count -eq 3 -and (Get-Abimes 'organizer.json').Count -eq 0) 'Fichier pris par un autre programme : intact, pas mis de côté'

    # -----------------------------------------------------------------------
    # 2. Préférences abîmées : SvcOriginal (de quoi remettre les services) n'est pas perdu
    # -----------------------------------------------------------------------
    $SettingsFile = Join-Path $script:Bac 'parametres.json'
    $script:Settings = $null
    Set-Setting 'SvcOriginal' @('DiagTrack|Automatic|True')
    Set-Setting 'Theme' 'nuit'
    Set-Setting 'Bulles' $false   # (la copie de secours est la version d'avant : elle a SvcOriginal et Theme)
    [IO.File]::WriteAllText($SettingsFile, '{"SvcOriginal":["DiagT')
    $script:Settings = $null
    Test-Ok (@(Get-Setting 'SvcOriginal' @()).Count -eq 1 -and (Get-Setting 'Theme') -eq 'nuit') 'Préférences abîmées : reprises de leur copie de secours (SvcOriginal gardé)'
    Set-Setting 'Visite' $true
    $script:Settings = $null
    Test-Ok (@(Get-Setting 'SvcOriginal' @()).Count -eq 1 -and (Get-Setting 'Visite') -eq $true) 'Préférences abîmées : l''enregistrement suivant ne perd rien'

    # -----------------------------------------------------------------------
    # 2 bis. Audit du 25/10 (25b) : l'état d'origine doit être écrit AVANT le changement, sinon rien ne change
    # -----------------------------------------------------------------------
    # Préférences prises par un autre programme au lancement : le service n'est pas coupé (son origine serait perdue)
    $SettingsFile = Join-Path $script:Bac 'parametres-bloque.json'
    $script:Settings = $null
    Set-Setting 'Theme' 'nuit'
    $script:Settings = $null
    $script:AideRepond = $true; $script:AideAppels.Clear()
    $verrou = [IO.File]::Open($SettingsFile, 'Open', 'ReadWrite', 'None')
    try {
        $refuse = $false
        try { Set-ServiceOff 'DiagTrack' } catch { $refuse = $_.Exception.Message -match 'rien n''a été changé' }
    } finally { $verrou.Close() }
    Test-Ok ($refuse -and -not ($script:AideAppels -contains 'service.regler')) "Préférences bloquées : le service est coupé sans que son état d'origine soit noté (refus : $refuse, demandes : $($script:AideAppels -join ', '))"
    Test-Ok (-not @(Get-Setting 'SvcOriginal' @()).Count) 'Préférences bloquées : une origine pas écrite reste en mémoire'
    # Contre-épreuve : fichier libre, l'origine est notée puis le service coupé
    $script:Settings = $null; $script:JsonBloques = @{}; $script:AideAppels.Clear()
    Set-ServiceOff 'DiagTrack'
    $script:Settings = $null
    Test-Ok (@(Get-Setting 'SvcOriginal' @()).Count -eq 1 -and ($script:AideAppels -contains 'service.regler')) 'Préférences libres : origine du service notée, puis service coupé'

    # sauvegarde.json impossible à écrire (disque plein, fichier pris) : l'origine d'un réglage n'est pas gardée en
    # mémoire comme si elle était écrite (au 2e essai, le réglage changeait sans sauvegarde)
    $script:DataDir = $script:Bac
    $BackupFile = Join-Path $script:Bac 'sauvegarde-pleine.json'
    $script:Backup = @{ Registry = @{}; PowerScheme = $null; CreatedScheme = $null; Overlay = $null; Dns = @{}; Displays = @{}; Oublies = @() }
    $cleEssai = "HKLM:\SOFTWARE\Policies\NevermindTest-$([guid]::NewGuid().ToString('N'))"   # (n'existe pas : rien n'est lu)
    Save-Backup
    $verrou = [IO.File]::Open($BackupFile, 'Open', 'ReadWrite', 'None')
    try {
        $refuse = $false
        try { Save-Original $cleEssai 'X' } catch { $refuse = $true }
        $refuse2 = $false
        try { Save-Original $cleEssai 'X' } catch { $refuse2 = $true }
    } finally { $verrou.Close() }
    Test-Ok ($refuse -and $refuse2 -and -not $script:Backup.Registry.ContainsKey("$cleEssai|X")) "Sauvegarde impossible à écrire : l'origine du réglage est gardée en mémoire et le 2e essai passe (refus : $refuse / $refuse2)"
    Save-Original $cleEssai 'X'
    $j = Get-Content -LiteralPath $BackupFile -Raw -Encoding UTF8 | ConvertFrom-Json
    Test-Ok ([bool]$j.Registry.PSObject.Properties["$cleEssai|X"]) 'Sauvegarde libre : l''origine du réglage est écrite'

    # -----------------------------------------------------------------------
    # 3. « Tout restaurer » avec une Aide qui ne répond pas
    # -----------------------------------------------------------------------
    $script:DataDir = $script:Bac
    $BackupFile = Join-Path $script:Bac 'sauvegarde.json'
    $SettingsFile = Join-Path $script:Bac 'parametres-restaurer.json'
    $script:Settings = $null
    Set-Setting 'SvcOriginal' @('DiagTrack|Automatic|True')
    $script:History = $null
    $script:Backup = @{ Registry = @{}; PowerScheme = $null; CreatedScheme = $null; Overlay = $null; Dns = @{}; Displays = @{}; Oublies = @() }
    $pc = 'HKLM:\SOFTWARE\Policies\Test'; $moi = 'HKCU:\Software\Test'
    $script:Backup.Registry["$pc|A"] = @{ Path = $pc; Name = 'A'; Existed = $true; Value = 1; Kind = 'DWord' }
    $script:Backup.Registry["$moi|B"] = @{ Path = $moi; Name = 'B'; Existed = $false; Value = $null; Kind = $null }
    $script:Backup.Dns['12'] = '192.168.1.1'
    Save-Backup
    # Les copies d'origine gardées dans le registre (la même valeur HKCU, et celle du PC gardée par l'Aide)
    $script:FaussesOrigines = @(
        [pscustomobject]@{ Path = $moi; Name = 'B'; Existed = $false; Value = $null; Kind = $null },
        [pscustomobject]@{ Path = $pc; Name = 'A'; Existed = $true; Value = 1; Kind = 'DWord' })
    $script:History = New-Object System.Collections.ArrayList
    [void]$script:History.Add(@{ Id = 'moi'; Titre = 'Réglage de l''utilisateur'; Annule = $false; Log = @(@{ Type = 'reg'; Path = $moi; Name = 'B'; Existed = $false }) })
    [void]$script:History.Add(@{ Id = 'pc'; Titre = 'Réglage du PC'; Annule = $false; Log = @(@{ Type = 'reg'; Path = $pc; Name = 'A'; Existed = $true; Value = 1; Kind = 'DWord' }) })
    [void]$script:History.Add(@{ Id = 'svc'; Titre = 'Service'; Annule = $false; Log = @(@{ Type = 'svc'; Name = 'DiagTrack'; StartType = 'Automatic'; Running = $true }) })
    [void]$script:History.Add(@{ Id = 'dns'; Titre = 'DNS'; Annule = $false; Log = @(@{ Type = 'dns'; Servers = @() }) })
    [void]$script:History.Add(@{ Id = 'def'; Titre = 'Protection'; Annule = $false; Log = @(@{ Type = 'definitif'; Quoi = 'x'; Retour = 'y' }) })

    $script:AideRepond = $false
    $errors = Restore-AllSettings; $errors = @($errors)
    Set-HistoryAllUndone
    Test-Ok ($errors.Count -eq 3) "Aide muette : 3 réglages pas remis sont dits (dit : $($errors.Count) : $($errors -join ' / '))"
    Test-Ok ([IO.File]::Exists($BackupFile)) 'Aide muette : la sauvegarde n''est pas effacée'
    $j = Get-Content -LiteralPath $BackupFile -Raw -Encoding UTF8 | ConvertFrom-Json
    Test-Ok ([bool]$j.Registry.PSObject.Properties["$pc|A"] -and -not $j.Registry.PSObject.Properties["$moi|B"]) 'Aide muette : la sauvegarde garde le réglage du PC pas remis, et seulement lui'
    Test-Ok ([bool]$j.Dns.PSObject.Properties['12']) 'Aide muette : la sauvegarde garde le DNS pas remis'
    Test-Ok (@(Get-Setting 'SvcOriginal' @()).Count -eq 1) 'Aide muette : le service pas remis reste noté (SvcOriginal)'
    Test-Ok ($script:Backup.Registry.Count -eq 1 -and $script:Backup.Registry.ContainsKey("$pc|A")) 'Aide muette : relue, la sauvegarde ne reprend pas le réglage déjà remis (copie d''origine du registre oubliée)'
    $annules = @($script:History | Where-Object { $_.Annule } | ForEach-Object { $_.Id })
    Test-Ok (($annules -join ',') -eq 'moi') "Aide muette : seul le changement vraiment remis est marqué annulé (marqués : $($annules -join ', '))"
    Test-Ok (-not $script:FauxRegistre.ContainsKey("$moi|B")) 'Aide muette : le réglage de l''utilisateur est bien remis'

    # « Réessayer » : l'Aide répond, tout est remis
    $script:AideRepond = $true
    $errors = Restore-AllSettings; $errors = @($errors)
    Set-HistoryAllUndone
    Test-Ok ($errors.Count -eq 0) "Réessayer : tout est remis (reste : $($errors -join ' / '))"
    Test-Ok (-not [IO.File]::Exists($BackupFile) -and -not [IO.File]::Exists("$BackupFile.bak")) 'Réessayer : la sauvegarde part une fois tout remis'
    Test-Ok (@(Get-Setting 'SvcOriginal' @()).Count -eq 0 -and $script:FauxRegistre["$pc|A"] -eq 1) 'Réessayer : le service et le réglage du PC sont remis'
    $annules = @($script:History | Where-Object { $_.Annule } | ForEach-Object { $_.Id })
    Test-Ok (($annules -join ',') -eq 'moi,pc,svc,dns') "Réessayer : tout l'historique est annulé, sauf la protection définitive (marqués : $($annules -join ', '))"
    Test-Ok ($script:AideAppels -contains 'origines.oublier') 'Réessayer : les copies d''origine gardées par l''Aide sont oubliées'

    # -----------------------------------------------------------------------
    # 3 bis. Audit du 25/10 (constat 7) : les couleurs d'un pack de thème, collées dans la fenêtre, sont vérifiées
    # -----------------------------------------------------------------------
    $astThemes = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules 'themes.ps1'), [ref]$null, [ref]$null)
    foreach ($n in 'Test-CouleursPack', 'Get-ThemePack', 'Get-ThemePackConsole') {
        $def = $astThemes.Find({ param($x) $x -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $x.Name -eq $n }, $false)
        if ($def) { Invoke-Expression ($def.Extent.Text -replace "^function\s+$([regex]::Escape($n))", "function script:$n") } else { Test-Ok $false "themes.ps1 : $n introuvable" }
    }
    $AppThemes = [ordered]@{ neon = @{ Name = 'Néon'; P = '#00E5FF'; S = '#B04BFF'; T = '#FF2EB5'; Map = @{} } }
    $PacksDir = Join-Path $script:Bac 'packs'
    $packs = [ordered]@{
        bon      = '{"Name":"Bon","Colors":{"P":"#112233","S":"#445566","T":"#778899","Map":{"0C0920":"#101010"}}}'
        rouge    = '{"Name":"Rouge","Colors":{"P":"red","S":"#445566","T":"#778899"}}'
        court    = '{"Name":"Court","Colors":{"P":"#0F0","S":"#445566","T":"#778899"}}'
        piege    = '{"Name":"Piège","Colors":{"P":"#112233\" Background=\"{x:Static x}\"><Grid","S":"#445566","T":"#778899"}}'
        carte    = '{"Name":"Carte","Colors":{"P":"#112233","S":"#445566","T":"#778899","Map":{"0C0920":"101010\"/><Button"}}}'
        nombre   = '{"Name":"Nombre","Colors":{"P":"#112233","S":"#445566","T":"#778899","NH":"beaucoup"}}'
    }
    foreach ($k in $packs.Keys) { $d = Join-Path $PacksDir $k; [void][IO.Directory]::CreateDirectory($d); [IO.File]::WriteAllText((Join-Path $d 'pack.json'), $packs[$k]) }
    $script:Journal.Clear()
    $lus = @{}
    foreach ($k in $packs.Keys) { try { $lus[$k] = Get-ThemePack $k } catch { $lus[$k] = "erreur : $($_.Exception.Message)" } }
    Test-Ok ($AppThemes.Contains('pack-bon') -and $lus.bon.Base -eq 'pack-bon') 'Pack de thème : des couleurs bien écrites ne sont pas prises'
    foreach ($k in 'rouge', 'court', 'piege', 'carte', 'nombre') {
        Test-Ok (-not $AppThemes.Contains("pack-$k") -and $lus[$k] -is [hashtable] -and $lus[$k].Base -eq 'neon') "Pack de thème « $k » : couleurs pas refusées, ou le pack n'est plus lisible ($(if ($lus[$k] -is [hashtable]) { $lus[$k].Base } else { $lus[$k] }))"
    }
    Test-Ok (@($script:Journal | Where-Object { $_ -match 'couleurs refusées' }).Count -eq 5) "Pack de thème : les couleurs refusées ne sont pas notées au journal ($(@($script:Journal) -join ' / '))"

    # -----------------------------------------------------------------------
    # 4. Minuteries importantes protégées de l'intérieur (un passage qui plante arrêterait la minuterie pour de bon)
    # -----------------------------------------------------------------------
    $ticks = @{}
    foreach ($fic in 'evenements', 'interface', 'accueil-console') {
        foreach ($c in $asts[$fic].FindAll({ param($n) $n -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and $n.Member.Value -eq 'Add_Tick' }, $true)) {
            $ticks["$fic|$($c.Expression.Extent.Text)"] = $c.Arguments[0]
        }
    }
    foreach ($m in @(@('evenements', '$script:ShowWatch', 'ordres de Mindset (quitter, afficher une page, bulles)'), @('evenements', '$script:SuiteWatch', 'thème de la suite'),
                     @('interface', '$script:WaitTimer', 'attente des travaux en arrière plan'), @('accueil-console', '$t', 'écran d''accueil'))) {
        $sb = $ticks["$($m[0])|$($m[1])"]
        # (tout le passage dans un try : un seul bloc, qui est un try)
        $st = if ($sb) { @($sb.ScriptBlock.EndBlock.Statements) } else { @() }
        $protegee = $st.Count -eq 1 -and $st[0] -is [System.Management.Automation.Language.TryStatementAst]
        Test-Ok ([bool]$protegee) "Minuterie « $($m[2]) » ($($m[0]).ps1) : protégée de l'intérieur (try/catch)"
    }

    # -----------------------------------------------------------------------
    # 5. Mode jeu (audit du 25/10, constat 13) : les applis fermées pendant la partie sont écrites dans les données ;
    #    si Nevermind plante pendant le jeu, son lancement suivant relance ce qui ne l'a pas été. Faux programmes : rien
    #    n'est fermé ni lancé sur le PC.
    # -----------------------------------------------------------------------
    $astJeu = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules 'jeu.ps1'), [ref]$null, [ref]$null)
    $defApps = $astJeu.Find({ param($x) $x -is [System.Management.Automation.Language.AssignmentStatementAst] -and $x.Left.Extent.Text -eq '$GameModeApps' -and $x.Parent -eq $astJeu.EndBlock }, $true)
    Invoke-Expression $defApps.Extent.Text
    $GameModeFile = Join-Path $script:Bac 'mode-jeu.json'
    foreach ($n in 'Start-GameSession', 'Stop-GameSession', 'Save-GameModePending', 'Clear-GameModePending', 'Restart-GameModeApps', 'Resume-GameModePending') {
        $def = $astJeu.Find({ param($x) $x -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $x.Name -eq $n }, $false)
        if ($def) { Invoke-Expression ($def.Extent.Text -replace "^function\s+$([regex]::Escape($n))", "function script:$n") } else { Write-Host "Info : $n absent de jeu.ps1" }
    }
    # Faux PC : la liste des programmes ouverts, les lancements notés, les fermetures par la règle commune simulées
    $script:FauxProcs = New-Object System.Collections.ArrayList
    function New-FauxProc([string]$Nom, [int]$Id, [string]$Chemin) { [void]$script:FauxProcs.Add([pscustomobject]@{ ProcessName = $Nom; Id = $Id; Path = $Chemin }) }
    function script:Get-Process { param([string]$Name, [int]$Id, $ErrorAction)
        $r = @($script:FauxProcs | Where-Object { (-not $Name -or $_.ProcessName -eq $Name) -and (-not $Id -or $_.Id -eq $Id) })
        if ($Id -and -not $r.Count -and $ErrorAction -eq 'Stop') { throw "Aucun processus $Id" }
        $r }
    function script:Stop-ProcessusRegle($Processus, [string]$Chemin, $Contexte) { foreach ($p in @($Processus)) { $script:FauxProcs.Remove($p) }; @{ Fermes = @($Processus).Count; Aide = 0; Refus = @() } }
    function script:Get-AideContexte { @{} }
    function script:Get-AideReference($P) { $null }
    $script:Lances = New-Object System.Collections.Generic.List[string]
    function script:Start-Unelevated([string]$Path, [string]$Arguments) { $script:Lances.Add("$Path|$Arguments") }
    $script:Bulles = New-Object System.Collections.Generic.List[string]
    function script:Show-Notify([string]$Titre, [string]$Texte) { $script:Bulles.Add("$Titre|$Texte") }
    foreach ($n in 'Update-GameModeStatus', 'Update-LibraryIfShown', 'Stop-FpsTarget', 'Add-PlayTime') { Set-Item "function:script:$n" {} }
    function script:Test-FpsMeasure { $false }
    function script:Test-LagMeasure { $false }
    function script:Get-GameModeSelection { @('onedrive', 'teams') }
    function script:Get-Setting([string]$Name, $Default = $null) { if ($Name -eq 'GameMode') { $true } else { $Default } }
    $od = 'C:\Users\Joueur\AppData\Local\Microsoft\OneDrive\OneDrive.exe'; $tm = 'C:\Users\Joueur\AppData\Local\Microsoft\Teams\ms-teams.exe'
    function Start-PartieEssai([bool]$JeuReste) {
        $script:FauxProcs.Clear(); $script:Lances.Clear(); $script:Bulles.Clear()
        New-FauxProc 'OneDrive' 101 $od; New-FauxProc 'ms-teams' 102 $tm; New-FauxProc 'jeu' 4242 'D:\Jeux\jeu.exe'
        $script:GameSession = $null
        Start-GameSession 'Jeu d''essai' @($script:FauxProcs | Where-Object { $_.Id -eq 4242 })[0]
        # Nevermind plante : sa mémoire est perdue ; la partie est finie ou non quand il est relancé
        $script:GameSession = $null; $script:GameModeRepris = $false
        if (-not $JeuReste) { $script:FauxProcs.Remove(@($script:FauxProcs | Where-Object { $_.Id -eq 4242 })[0]) }
    }
    $attendus = "$od|/background,$tm|"
    try {
        # Partie normale : la liste est écrite pendant le jeu, effacée à la fin (applis relancées comme avant)
        $script:FauxProcs.Clear(); $script:Lances.Clear()
        New-FauxProc 'OneDrive' 101 $od; New-FauxProc 'jeu' 4242 'D:\Jeux\jeu.exe'
        $script:GameSession = $null
        Start-GameSession 'Jeu d''essai' @($script:FauxProcs | Where-Object { $_.Id -eq 4242 })[0]
        $j = Read-JsonSur $GameModeFile
        Test-Ok ($j -and @($j.Closed).Count -eq 1 -and $j.Closed[0].Id -eq 'onedrive' -and $j.Closed[0].Path -eq $od) 'Mode jeu : les applis fermées ne sont pas écrites dans les données pendant la partie'
        Stop-GameSession
        Test-Ok (($script:Lances -join ',') -eq "$od|/background") "Mode jeu : fin de partie, OneDrive pas relancé comme avant ($($script:Lances -join ','))"
        Test-Ok (-not [IO.File]::Exists($GameModeFile) -and -not [IO.File]::Exists("$GameModeFile.bak")) 'Mode jeu : la liste reste dans les données après une fin de partie normale'

        # Nevermind plante pendant la partie, relancé après le jeu : les applis sont relancées à son lancement
        Start-PartieEssai $false
        Test-Ok (-not $script:Lances.Count) 'Mode jeu : une appli est relancée pendant la partie'
        try { Resume-GameModePending } catch { Test-Ok $false "Mode jeu : reprise en erreur ($($_.Exception.Message))" }
        Test-Ok (($script:Lances -join ',') -eq $attendus) "Mode jeu : après un plantage, applis relancées au lancement suivant : « $($script:Lances -join ',') » au lieu de « $attendus »"
        Test-Ok (-not [IO.File]::Exists($GameModeFile) -and -not [IO.File]::Exists("$GameModeFile.bak")) 'Mode jeu : la liste reste dans les données après la reprise (relancées à chaque lancement)'
        Test-Ok (@($script:Bulles | Where-Object { $_ -eq 'Fin du mode jeu|Les applis fermées ont été relancées.' }).Count -eq 1) "Mode jeu : la bulle de fin n'est pas montrée à la reprise ($($script:Bulles -join ' / '))"
        $script:Lances.Clear(); $script:GameModeRepris = $false
        Resume-GameModePending
        Test-Ok (-not $script:Lances.Count) 'Mode jeu : applis relancées une 2e fois au lancement d''après'

        # Une appli déjà rouverte par l'utilisateur n'est pas lancée deux fois
        Start-PartieEssai $false
        New-FauxProc 'OneDrive' 103 $od
        Resume-GameModePending
        Test-Ok (($script:Lances -join ',') -eq "$tm|") "Mode jeu : une appli déjà rouverte est relancée ($($script:Lances -join ','))"

        # Nevermind relancé pendant la partie : rien n'est lancé tout de suite, la partie reprend et les applis reviennent à sa fin
        Start-PartieEssai $true
        Resume-GameModePending
        Test-Ok (-not $script:Lances.Count -and $script:GameSession -and $script:GameSession.Pid -eq 4242 -and @($script:GameSession.Closed).Count -eq 2) 'Mode jeu : relancé pendant la partie, la partie n''est pas reprise (ou les applis sont relancées en plein jeu)'
        Stop-GameSession
        Test-Ok (($script:Lances -join ',') -eq $attendus -and -not [IO.File]::Exists($GameModeFile)) "Mode jeu : partie reprise, applis pas relancées à sa fin ($($script:Lances -join ','))"

        # Fichier modifié par un autre programme : rien d'autre que les applis de la liste, au nom attendu, avec leurs arguments
        $script:Lances.Clear(); $script:Bulles.Clear(); $script:FauxProcs.Clear(); $script:GameModeRepris = $false; $script:GameSession = $null
        Save-JsonSur $GameModeFile @{ Game = 'x'; Pid = 1; Proc = 'x'; Start = (Get-Date).ToString('o'); Closed = @(
            @{ Id = 'onedrive'; Name = 'OneDrive'; Path = 'C:\Users\Joueur\Downloads\virus.exe'; Args = '/piege' },
            @{ Id = 'inconnu'; Name = 'Autre'; Path = 'C:\Users\Joueur\Downloads\autre.exe' },
            @{ Id = 'teams'; Name = 'Microsoft Teams'; Path = $tm; Args = '/piege' }) } 4
        Resume-GameModePending
        Test-Ok (($script:Lances -join ',') -eq "$tm|") "Mode jeu : un fichier modifié fait lancer autre chose ($($script:Lances -join ','))"
        Test-Ok (@($script:Bulles | Where-Object { $_ -eq 'Fin du mode jeu|Relance toi-même : OneDrive.' }).Count -eq 1) "Mode jeu : l'appli refusée n'est pas signalée à relancer à la main ($($script:Bulles -join ' / '))"
    } catch { Test-Ok $false "Mode jeu : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))" }
    Remove-Item function:script:Get-Process
}
catch {
    $echecs.Add("Arrêt inattendu : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))")
}
finally {
    $null = & "$env:SystemRoot\System32\cmd.exe" /c rd /s /q "$($script:Bac)" 2>&1
}

if ($echecs.Count) {
    Write-Host "$($echecs.Count) échec(s) sur $nbTests contrôles :" -ForegroundColor Red
    $echecs | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
    exit 1
}
Write-Host "Données : les $nbTests contrôles sont bons." -ForegroundColor Green
exit 0
