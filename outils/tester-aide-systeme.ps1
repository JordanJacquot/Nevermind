# Test de l'Aide en SYSTEM (revue du 25/10, lot A2), sans droits et sans toucher au PC : aucune vraie tâche planifiée
# n'est lue ni lancée, aucune demande de Windows n'est faite (elles sont remplacées par des fausses), seules des Aides de
# test sont lancées, dans un dossier temporaire, et seuls leurs processus sont arrêtés.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-aide-systeme.ps1
#
#   1. Nevermind ne lance la tâche de l'Aide que si c'est la tâche SYSTEM pour son utilisateur (Get-AideEtatTache) ;
#   2. sinon, ou si elle ne répond pas, la demande de Windows (-Verb RunAs, -Sid de l'utilisateur), jamais l'ancienne
#      tâche ; refusée : un message simple, pas de nouvelle demande avant 10 minutes ; jamais pour les lectures en fond ;
#   3. Nevermind reconnaît l'Aide en SYSTEM par le propriétaire du canal (il ne peut pas lire le chemin d'un processus
#      SYSTEM) ;
#   4. l'Aide (NevermindAide.exe compilé ici, et le script seul) sert l'utilisateur -Sid, compile dans aide-temp de son
#      dossier (jamais dans le TEMP reçu), et son canal ne donne des droits qu'à cet utilisateur ; NevermindAide.exe sans
#      son script se ferme sans fenêtre ;
#   5. la page Sécurité ne prend pas la tâche d'installation de Mindset 2.10.0 (SYSTEM, -Sid) pour une tâche cachée ;
#   6. lecture.securite dit si les détections de Defender ont été lues (DetectionsLues) : sinon « Résultat inconnu ».
# Chaque correction a sa contre-épreuve (l'ancien code, lu dans git, ou la vérification retirée, fait échouer le test).
# Code de sortie : 0 si tout passe, 1 sinon.

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$app     = Join-Path $racine 'OptiGame\fichiers'
$modules = Join-Path $app 'modules'
$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0
function Test-Ok($Condition, [string]$Message) {
    $script:nbTests++
    if (-not [bool]$Condition) { $script:echecs.Add($Message) }
}
function Get-Fonction([string]$Texte, [string]$Nom) {
    $ast = [System.Management.Automation.Language.Parser]::ParseInput($Texte, [ref]$null, [ref]$null)
    $f = $ast.Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $true)
    if (-not $f) { throw "$Nom introuvable" }
    $f.Extent.Text
}
function Read-Fichier([string]$Chemin) { [IO.File]::ReadAllText($Chemin, [Text.Encoding]::UTF8) }
# Un fichier tel qu'il était dans git avant le lot A2 (Nevermind 1.10.0, et la page Sécurité avant DetectionsLues)
function Read-Git([string]$Ref, [string]$Chemin) {
    $eap = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    try { $t = & git -C $racine show "$($Ref):$Chemin" 2>$null } catch { $t = $null } finally { $ErrorActionPreference = $eap }
    if ($LASTEXITCODE -or -not $t) { Write-Host "Info : $Chemin ($Ref) absent de git ici, contre-épreuve sautée"; return $null }
    ($t -join "`n")
}
$psExe  = Join-Path ([Environment]::GetFolderPath('System')) 'WindowsPowerShell\v1.0\powershell.exe'
$moiSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$enAdmin = (New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
Add-Type -TypeDefinition (Read-Fichier (Join-Path $modules 'natif.cs'))

# Le dossier de test (chemin long : sur la CI, %TEMP% est en nom court)
Add-Type -Namespace TestAideSysteme -Name Chemins -MemberDefinition '[DllImport("kernel32.dll", CharSet = CharSet.Unicode)] public static extern uint GetLongPathName(string court, System.Text.StringBuilder long_, uint taille);'
$tmp = Join-Path $env:TEMP "nevermind-test-aide-systeme-$PID"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$long = New-Object System.Text.StringBuilder 1024
if ([TestAideSysteme.Chemins]::GetLongPathName($tmp, $long, 1024)) { $tmp = $long.ToString() }
$lances = New-Object System.Collections.Generic.List[object]

try {
# ---------------------------------------------------------------------------------------------------------------------
# 1. La tâche de l'Aide : seulement la tâche SYSTEM pour cet utilisateur
# ---------------------------------------------------------------------------------------------------------------------
$texteClient = Read-Fichier (Join-Path $modules 'aide-client.ps1')
. ([scriptblock]::Create((Get-Fonction $texteClient 'Get-AideEtatTache')))
$exe = 'C:\Program Files\Mindset\Nevermind\NevermindAide.exe'
$autreSid = 'S-1-5-21-1111111111-2222222222-3333333333-1001'
function New-XmlTache([string]$Compte, [string]$Commande, [string]$Arguments, [string]$Niveau = 'HighestAvailable', [string]$Active = 'true') {
    $arg = if ($Arguments) { "<Arguments>$([Security.SecurityElement]::Escape($Arguments))</Arguments>" } else { '' }
    "<?xml version=`"1.0`" encoding=`"UTF-16`"?><Task version=`"1.2`" xmlns=`"http://schemas.microsoft.com/windows/2004/02/mit/task`"><Principals><Principal id=`"Author`"><UserId>$Compte</UserId><RunLevel>$Niveau</RunLevel></Principal></Principals><Settings><Enabled>$Active</Enabled></Settings><Actions Context=`"Author`"><Exec><Command>$([Security.SecurityElement]::Escape($Commande))</Command>$arg</Exec></Actions></Task>"
}
$bonne = New-XmlTache 'S-1-5-18' "`"$exe`"" "-Sid $moiSid"
$cas = @(
    @{ Nom = 'la tâche SYSTEM pour cet utilisateur'; Xml = $bonne; Attendu = 'systeme' }
    @{ Nom = 'la même, sans guillemets autour du programme'; Xml = (New-XmlTache 'S-1-5-18' $exe "-Sid $moiSid"); Attendu = 'systeme' }
    @{ Nom = 'l''ancienne tâche « droits les plus élevés » sous le compte de l''utilisateur'; Xml = (New-XmlTache $moiSid "`"$exe`"" ''); Attendu = 'autre' }
    @{ Nom = 'l''ancienne tâche, même avec -Sid'; Xml = (New-XmlTache $moiSid "`"$exe`"" "-Sid $moiSid"); Attendu = 'autre' }
    @{ Nom = 'une tâche SYSTEM pour un autre compte'; Xml = (New-XmlTache 'S-1-5-18' "`"$exe`"" "-Sid $autreSid"); Attendu = 'autre' }
    @{ Nom = 'une tâche SYSTEM vers un autre programme'; Xml = (New-XmlTache 'S-1-5-18' '"C:\Autre\NevermindAide.exe"' "-Sid $moiSid"); Attendu = 'autre' }
    @{ Nom = 'une tâche SYSTEM avec des arguments en plus'; Xml = (New-XmlTache 'S-1-5-18' "`"$exe`"" "-Sid $moiSid -Parent 4"); Attendu = 'autre' }
    @{ Nom = 'une tâche SYSTEM désactivée'; Xml = (New-XmlTache 'S-1-5-18' "`"$exe`"" "-Sid $moiSid" -Active 'false'); Attendu = 'autre' }
    @{ Nom = 'un texte qui n''est pas du XML'; Xml = 'pas une tâche'; Attendu = 'autre' }
    @{ Nom = 'pas de tâche'; Xml = ''; Attendu = '' })
foreach ($c in $cas) {
    $e = Get-AideEtatTache $c.Xml $exe $moiSid
    Test-Ok ($e -eq $c.Attendu) "Tâche de l'Aide : $($c.Nom) donne « $e » (attendu « $($c.Attendu) »)"
}
# (contre-épreuve : Nevermind 1.10.0 lançait la tâche sans la regarder ; avec un contrôle qui accepte tout, l'ancienne
# tâche passerait pour la bonne)
Test-Ok ($null -eq (Read-Git '8c80c4f' 'OptiGame/fichiers/modules/aide-client.ps1') -or (Read-Git '8c80c4f' 'OptiGame/fichiers/modules/aide-client.ps1') -notmatch 'Get-AideEtatTache') 'Contre-épreuve : Nevermind 1.10.0 contrôlait déjà la tâche ?'

# ---------------------------------------------------------------------------------------------------------------------
# 2. Le réveil de l'Aide (le vrai aide-client.ps1, avec un faux schtasks et une fausse demande de Windows)
# ---------------------------------------------------------------------------------------------------------------------
# Une copie de l'Aide (le script seul) : c'est elle que la fausse demande de Windows lance, sans droits
$copie = Join-Path $tmp 'Nevermind'
New-Item -ItemType Directory -Force -Path (Join-Path $copie 'fichiers') | Out-Null
Copy-Item -LiteralPath (Join-Path $app 'NevermindAide.ps1') -Destination (Join-Path $copie 'fichiers')
Copy-Item -LiteralPath $modules -Destination (Join-Path $copie 'fichiers') -Recurse
$ModulesDir = Join-Path $copie 'fichiers\modules'
function Get-AppRoot { $copie }
. ([scriptblock]::Create((Get-Fonction (Read-Fichier (Join-Path $modules 'aide-actions.ps1')) 'Get-AideSuffixe')))
$script:Journal = New-Object System.Collections.Generic.List[string]
function Write-Log([string]$m) { $script:Journal.Add($m) }
function Wait-Handle { }
function Invoke-Async($Sb, $X) { & $Sb $X }
Remove-Item Env:\OPTIGAME_TEST -ErrorAction SilentlyContinue
. ([scriptblock]::Create($texteClient))
[AideClient]::ExeAttendu = $psExe   # (l'Aide de test tourne dans powershell.exe)
if ([AideClient]::Joignable(50)) { throw 'une Aide de cette copie tourne déjà : test impossible' }
# Le faux schtasks : rend le XML de $script:XmlTache à /Query, note chaque /Run
$script:XmlTache = ''
$fauxSchtasks = Join-Path $tmp 'schtasks.ps1'
[IO.File]::WriteAllText($fauxSchtasks, "param([Parameter(ValueFromRemainingArguments = `$true)]`$a)`r`nAdd-Content -LiteralPath '$tmp\schtasks.txt' -Value (`$a -join ' ')`r`nif (`$a[0] -eq '/Query') { `$x = [IO.File]::ReadAllText('$tmp\tache.xml'); if (-not `$x) { exit 1 }; `$x; exit 0 }`r`nexit 0`r`n", (New-Object Text.UTF8Encoding $true))
$AideSchtasks = $fauxSchtasks
$AideAttenteTache = 1
$AideAttenteWindows = 60
# La fausse demande de Windows : notée ; « Oui » lance l'Aide de test (sans droits) avec les arguments reçus
$script:RunAs = New-Object System.Collections.Generic.List[string]
$script:Accepter = $true
function Start-Process {
    param([string]$FilePath, [string[]]$ArgumentList, [string]$Verb, [string]$WorkingDirectory, $ErrorAction)
    $script:RunAs.Add("$Verb|$FilePath|$($ArgumentList -join ' ')")
    if (-not $script:Accepter) { throw 'L''opération a été annulée par l''utilisateur.' }
    $p = Microsoft.PowerShell.Management\Start-Process -FilePath $psExe -ArgumentList (@('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$copie\fichiers\NevermindAide.ps1`"", '-Parent', $PID) + $ArgumentList) -WindowStyle Hidden -PassThru
    $lances.Add($p)
}
function Reset-Essai([string]$Xml) {
    [IO.File]::WriteAllText("$tmp\tache.xml", $Xml); [IO.File]::WriteAllText("$tmp\schtasks.txt", '')
    $script:RunAs.Clear(); $script:AideSansTache = $false; $script:AideRefus = $null; $script:AideSansDemande = $false
}
function Get-Runs { @([IO.File]::ReadAllLines("$tmp\schtasks.txt") | Where-Object { $_ -like '/Run*' }) }
$exeCopie = Join-Path $copie 'NevermindAide.exe'

# a. L'ancienne tâche : jamais lancée ; une lecture en fond ne fait aucune demande de Windows
Reset-Essai (New-XmlTache $moiSid "`"$exeCopie`"" '')
$err = ''; try { [void](Invoke-Aide 'aide.bonjour' @{} 5000 -SansDemande) } catch { $err = $_.Exception.Message }
Test-Ok (-not (Get-Runs).Count) 'Ancienne tâche : lancée par Nevermind (elle reprend l''environnement de l''utilisateur)'
Test-Ok (-not $script:RunAs.Count -and $err) 'Lecture en fond (-SansDemande) : une demande de Windows a été faite'
# b. Une action demandée : la demande de Windows, pour cet utilisateur, et l'Aide répond
Reset-Essai (New-XmlTache $moiSid "`"$exeCopie`"" '')
$r = $null; $err = ''; try { $r = Invoke-Aide 'aide.bonjour' @{} 20000 } catch { $err = $_.Exception.Message }
Test-Ok (-not (Get-Runs).Count) 'Ancienne tâche, action demandée : la tâche a été lancée'
Test-Ok ($script:RunAs.Count -eq 1 -and $script:RunAs[0] -eq "RunAs|$exeCopie|-Sid $moiSid") "Ancienne tâche : pas de demande de Windows pour NevermindAide.exe -Sid <utilisateur> ($($script:RunAs -join ' ; '))"
# (en administrateur, sur la CI, l'Aide de test ne répond qu'au Mindset.exe de son dossier : la réponse est essayée en vrai
# par outils\essai-aide-reelle.ps1)
if (-not $enAdmin) { Test-Ok ($r -and [int]$r.Version -ge 4 -and [string]$r.Compte -eq $moiSid) "Repli par la demande de Windows : l'Aide ne répond pas ($err)" }
foreach ($p in $lances) { try { if (-not $p.HasExited) { $p.Kill() } } catch {} }
$fin = (Get-Date).AddSeconds(10); while ((Get-Date) -lt $fin -and [AideClient]::Joignable(50)) { Start-Sleep -Milliseconds 100 }
# c. La bonne tâche : lancée ; elle ne répond pas (rien ne tourne ici) : la demande de Windows prend le relais
Reset-Essai (New-XmlTache 'S-1-5-18' "`"$exeCopie`"" "-Sid $moiSid")
$script:Accepter = $false
$err = ''; try { [void](Invoke-Aide 'aide.bonjour' @{} 5000) } catch { $err = $_.Exception.Message }
Test-Ok ((Get-Runs).Count -eq 1) "Tâche SYSTEM : pas lancée ($((Get-Runs) -join ' ; '))"
Test-Ok ($script:RunAs.Count -eq 1) 'Tâche SYSTEM muette : pas de demande de Windows ensuite'
Test-Ok ($err -match 'clique « Oui »' -and $script:AideRefus) "Demande de Windows refusée : message « $err »"
# d. Refusée il y a moins de 10 minutes : pas de nouvelle demande
$script:RunAs.Clear()
try { [void](Invoke-Aide 'aide.bonjour' @{} 5000) } catch {}
Test-Ok (-not $script:RunAs.Count) 'Demande de Windows refusée : redemandée tout de suite'
$script:Accepter = $true

# ---------------------------------------------------------------------------------------------------------------------
# 3. Reconnaître l'Aide : canal créé par le système ou les administrateurs, sinon NevermindAide.exe de ce dossier
# ---------------------------------------------------------------------------------------------------------------------
[AideClient]::ExeAttendu = 'C:\Nevermind\NevermindAide.exe'
Test-Ok ([AideClient]::ServeurAccepte('', 'S-1-5-18')) 'Aide en SYSTEM (chemin illisible, canal du système) refusée'
Test-Ok ([AideClient]::ServeurAccepte('', 'S-1-5-32-544')) 'Aide lancée par la demande de Windows (canal des administrateurs) refusée'
Test-Ok ([AideClient]::ServeurAccepte('C:\Nevermind\NevermindAide.exe', $moiSid)) 'Aide sans droits de ce dossier refusée'
Test-Ok (-not [AideClient]::ServeurAccepte('', $moiSid)) 'Canal d''un programme sans droits au chemin illisible accepté'
Test-Ok (-not [AideClient]::ServeurAccepte('C:\Autre\NevermindAide.exe', $moiSid)) 'Canal d''un autre programme sans droits accepté'
[AideClient]::ExeAttendu = ''
Test-Ok (-not [AideClient]::ServeurAccepte('', $moiSid)) 'Sans programme attendu, un chemin vide est accepté'

# ---------------------------------------------------------------------------------------------------------------------
# 4. L'Aide elle-même : utilisateur servi, aide-temp, droits du canal, NevermindAide.exe sans fenêtre
# ---------------------------------------------------------------------------------------------------------------------
if ($enAdmin) { Write-Host 'Info : en administrateur, les Aides de test ne répondent qu''au Mindset.exe de leur dossier : leurs réponses sont essayées en vrai par outils\essai-aide-reelle.ps1' }
$suffixe = Get-AideSuffixe $copie
[AideClient]::Suffixe = $suffixe
[AideClient]::ExeAttendu = $psExe
function Start-AideTest([string]$Script, [string[]]$Plus) {
    $p = Microsoft.PowerShell.Management\Start-Process -FilePath $psExe -ArgumentList (@('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$Script`"", '-Parent', $PID) + $Plus) -WindowStyle Hidden -PassThru
    $lances.Add($p)
    $fin = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $fin -and -not [AideClient]::Joignable(100)) { Start-Sleep -Milliseconds 100 }
    $p
}
function Stop-AideTest($p) {
    try { if (-not $p.HasExited) { $p.Kill() } } catch {}
    $fin = (Get-Date).AddSeconds(10); while ((Get-Date) -lt $fin -and [AideClient]::Joignable(50)) { Start-Sleep -Milliseconds 100 }
}
$scriptCopie = Join-Path $copie 'fichiers\NevermindAide.ps1'
foreach ($sid in @(if (-not $enAdmin) { $moiSid, 'S-1-5-18', 'pas-un-sid' })) {
    $p = Start-AideTest $scriptCopie @('-Sid', $sid)
    $b = $null; try { $b = ([AideClient]::Appeler('{"a":"aide.bonjour","p":{}}', 3000, 15000) | ConvertFrom-Json).r } catch { $b = $null }
    Test-Ok ($b) "Aide lancée avec -Sid « $sid » : pas de réponse sur le canal de l'utilisateur"
    if ($sid -eq $moiSid -and $b) {
        Test-Ok ([string]$b.Temp -ieq "$copie\aide-temp\") "Aide : compile dans « $($b.Temp) » au lieu de aide-temp de son dossier"
        # Droits du canal : l'utilisateur servi, les administrateurs (Aide hors SYSTEM), le réseau refusé ; personne d'autre
        $c = New-Object System.IO.Pipes.NamedPipeClientStream('.', [AideClient]::Nom(), [System.IO.Pipes.PipeDirection]::InOut)
        $regles = @()
        try {
            $c.Connect(5000)
            $regles = @($c.GetAccessControl().GetAccessRules($true, $false, [Security.Principal.SecurityIdentifier]) | ForEach-Object { "$($_.AccessControlType) $($_.IdentityReference.Value)" })
        } catch {} finally { $c.Dispose() }
        $attendu = @('Deny S-1-5-2', "Allow $moiSid", 'Allow S-1-5-32-544')
        Test-Ok ($regles.Count -eq 3 -and -not @($regles | Where-Object { $_ -notin $attendu }).Count) "Canal de l'Aide : droits inattendus ($($regles -join ' ; '))"
    }
    Stop-AideTest $p
}
# Contre-épreuve : l'Aide de Nevermind 1.10.0 (avec les modules d'aujourd'hui) compile dans le TEMP reçu
$ancienne = Read-Git '8c80c4f' 'OptiGame/fichiers/NevermindAide.ps1'
if ($ancienne -and -not $enAdmin) {
    $f = Join-Path $copie 'fichiers\NevermindAide-1.10.0.ps1'
    [IO.File]::WriteAllText($f, $ancienne, (New-Object Text.UTF8Encoding $true))
    $p = Start-AideTest $f @()
    $b = $null; try { $b = ([AideClient]::Appeler('{"a":"aide.bonjour","p":{}}', 3000, 15000) | ConvertFrom-Json).r } catch {}
    Test-Ok ($b -and [string]$b.Temp -and [string]$b.Temp -notlike "$copie\aide-temp*") "Contre-épreuve : l'Aide 1.10.0 compilait déjà dans aide-temp ? ($($b.Temp))"
    Stop-AideTest $p
}
# Le contrôle du client : avec les droits administrateur, Mindset.exe de ce dossier ET le compte servi
$texteAide = Read-Fichier (Join-Path $app 'NevermindAide.ps1')
Test-Ok ($texteAide -match '(?s)function Test-ClientServi.*Test-AideClient \$Chemin \$Racine.*\[AideProcessus\]::Compte\(\$Id\) -eq \$Sid' -and
    ([regex]::Matches($texteAide, 'Test-ClientServi ')).Count -ge 2) 'Aide : le compte du client (et de Nevermind ouvert) n''est plus comparé à l''utilisateur servi'
Test-Ok ($texteAide -match '(?s)if \(\$EnSysteme\) \{ exit \}') 'Aide : en SYSTEM sans -Sid valable, elle ne s''arrête plus'
Test-Ok ($texteAide -notmatch 'MessageBox|Windows\.Forms|Show-') 'Aide : une fenêtre peut s''ouvrir'
# NevermindAide.exe (outils\lanceur.cs) : aucune fenêtre dans sa partie, -Sid transmis
$cs = Read-Fichier (Join-Path $PSScriptRoot 'lanceur.cs')
$partie = if ($cs -match '(?s)#else\s*\r?\n#if AIDE(.*?)\r?\n#else') { $Matches[1] } else { '' }
Test-Ok ($partie -and $partie -notmatch 'MessageBox' -and $partie -match 'AddParameter\("Sid"') 'lanceur.cs : la partie de l''Aide montre une fenêtre ou ne transmet pas -Sid'
$csAncien = Read-Git '8c80c4f' 'outils/lanceur.cs'
if ($csAncien) {
    $avant = if ($csAncien -match '(?s)#else\s*\r?\n(.*?)#if AIDE') { $Matches[1] } else { '' }
    Test-Ok ($avant -match 'MessageBox') 'Contre-épreuve : lanceur.cs 1.10.0 ne montrait déjà aucune fenêtre avant la partie de l''Aide ?'
}
# Compilé et lancé : -Sid transmis (le canal porte ce compte) ; sans son script, il se ferme seul, sans fenêtre
$csc = Join-Path ([Environment]::GetFolderPath('Windows')) 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$sma = @(Get-ChildItem (Join-Path ([Environment]::GetFolderPath('Windows')) 'Microsoft.NET\assembly\GAC_MSIL\System.Management.Automation') -Recurse -Filter 'System.Management.Automation.dll' | Select-Object -First 1)[0].FullName
$exeAide = Join-Path $copie 'NevermindAide.exe'
$null = & $csc /nologo /target:winexe /platform:anycpu "/win32manifest:$(Join-Path $PSScriptRoot 'lanceur.manifest')" /reference:System.Windows.Forms.dll /reference:System.Web.Extensions.dll "/reference:$sma" "/out:$exeAide" /define:AIDE (Join-Path $PSScriptRoot 'lanceur.cs') 2>&1
Test-Ok (Test-Path -LiteralPath $exeAide) 'NevermindAide.exe : compilation impossible'
if ((Test-Path -LiteralPath $exeAide) -and -not $enAdmin) {
    # (sans droits : l'Aide accepte tout le monde, le client est ce test)
    $p = Microsoft.PowerShell.Management\Start-Process -FilePath $exeAide -ArgumentList '-Sid', $moiSid -PassThru
    $lances.Add($p)
    [AideClient]::ExeAttendu = $exeAide
    $fin = (Get-Date).AddSeconds(60); while ((Get-Date) -lt $fin -and -not [AideClient]::Joignable(100)) { Start-Sleep -Milliseconds 100 }
    $b = $null; try { $b = ([AideClient]::Appeler('{"a":"aide.bonjour","p":{}}', 3000, 15000) | ConvertFrom-Json).r } catch {}
    Test-Ok ($b -and [string]$b.Temp -ieq "$copie\aide-temp\") "NevermindAide.exe -Sid : pas de réponse, ou compilation ailleurs que aide-temp ($($b.Temp))"
    Stop-AideTest $p
    [AideClient]::ExeAttendu = $psExe
}
if (Test-Path -LiteralPath $exeAide) {
    $seul = Join-Path $tmp 'seul'
    New-Item -ItemType Directory -Force -Path $seul | Out-Null
    Copy-Item -LiteralPath $exeAide -Destination $seul
    $p = Microsoft.PowerShell.Management\Start-Process -FilePath (Join-Path $seul 'NevermindAide.exe') -ArgumentList '-Sid', $moiSid -PassThru
    $lances.Add($p)
    $ferme = $p.WaitForExit(10000)
    Test-Ok ($ferme -and $p.ExitCode -eq 1) 'NevermindAide.exe sans son script : reste ouvert (une fenêtre l''attendrait en SYSTEM)'
}

# ---------------------------------------------------------------------------------------------------------------------
# 5. Page Sécurité : la tâche d'installation de Mindset 2.10.0 (SYSTEM, -Sid) n'est pas une tâche cachée
# ---------------------------------------------------------------------------------------------------------------------
$tacheMindset = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File "C:\Program Files\Mindset\Mindset\fichiers\installer-admin.ps1" -Action demande -Sid S-1-5-21-1-2-3-1001 -Demande "C:\Users\x\AppData\Local\Mindset\Mindset\installation.demande"'
function Get-TachesVues([string]$Texte, [string]$Arguments) {
    $script:ArgsTache = $Arguments
    . ([scriptblock]::Create((Get-Fonction $Texte 'Add-AideLectureSecurite')))
    function Get-AideLecture { @{ Exclusions = @(); Menaces = @(); Detections = @() } }
    function Invoke-Aide { , @(@{ Nom = 'Mindset (installation)'; Chemin = '\Mindset\'; Exe = 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe'; Args = $script:ArgsTache }) }
    function Test-Path { $true }
    function Get-Acl { [pscustomobject]@{ Access = @() } }   # (script de Mindset protégé : personne d'autre que les administrateurs n'écrit)
    $script:EstAdmin = $false
    $D = @{}
    Add-AideLectureSecurite $D
    @($D.Tasks).Count
}
Test-Ok (-not (Get-TachesVues $texteClient $tacheMindset)) 'Page Sécurité : la tâche d''installation de Mindset 2.10.0 est montrée comme cachée'
Test-Ok ((Get-TachesVues $texteClient ($tacheMindset + ' -EncodedCommand AAAA')) -eq 1) 'Page Sécurité : une tâche modifiée (commande en plus) n''est plus montrée'
$clientAncien = Read-Git '8c80c4f' 'OptiGame/fichiers/modules/aide-client.ps1'
if ($clientAncien) { Test-Ok ((Get-TachesVues $clientAncien $tacheMindset) -eq 1) 'Contre-épreuve : Nevermind 1.10.0 reconnaissait déjà la tâche de Mindset 2.10.0 ?' }

# ---------------------------------------------------------------------------------------------------------------------
# 6. Analyse de l'antivirus : détections non lues = « Résultat inconnu », jamais « aucune menace »
# ---------------------------------------------------------------------------------------------------------------------
$texteActions = Read-Fichier (Join-Path $modules 'aide-actions.ps1')
. ([scriptblock]::Create((Get-Fonction $texteActions 'Invoke-AideAction')))
function Import-AideModule { }
function Get-MpPreference { [pscustomobject]@{ ExclusionPath = @(); ExclusionProcess = @(); ExclusionExtension = @() } }
function Get-MpThreat { }
$script:DefenderMarche = $false
function Get-MpThreatDetection { if (-not $script:DefenderMarche) { throw 'Defender ne répond pas' } }
$l = Invoke-AideAction 'lecture.securite' @{}
Test-Ok ($l.DetectionsLues -eq $false) 'lecture.securite : détections non lues, mais DetectionsLues n''est pas faux'
$script:DefenderMarche = $true
$l2 = Invoke-AideAction 'lecture.securite' @{}
Test-Ok ($l2.DetectionsLues -eq $true) 'lecture.securite : détections lues, mais DetectionsLues n''est pas vrai'
$texteSecu = Read-Fichier (Join-Path $modules 'securite.ps1')
function Test-ScanLu([string]$Texte, $Lecture) {
    . ([scriptblock]::Create((Get-Fonction $Texte 'Get-ScanThreats')))
    $script:Lecture = $Lecture
    function Get-AideLecture { $script:Lecture }
    function Get-MpThreatDetection { throw 'sans droits' }
    $script:EstAdmin = $false
    (Get-ScanThreats (Get-Date)).Lu
}
Test-Ok (-not (Test-ScanLu $texteSecu $l)) 'Analyse : lecture de Defender ratée dans l''Aide, mais « aucune menace » affiché'
Test-Ok (Test-ScanLu $texteSecu $l2) 'Analyse : lecture de Defender réussie, mais « Résultat inconnu »'
$secuAncien = Read-Git '0078e3c' 'OptiGame/fichiers/modules/securite.ps1'
if ($secuAncien) { Test-Ok (Test-ScanLu $secuAncien $l) 'Contre-épreuve : la page Sécurité d''avant affichait déjà « Résultat inconnu » ?' }
} catch {
    Test-Ok $false "Erreur du test : $($_.Exception.Message) $($_.ScriptStackTrace)"
} finally {
    foreach ($p in $lances) { try { if (-not $p.HasExited) { $p.Kill() } } catch {} }
    Start-Sleep -Milliseconds 500
    try { Remove-Item -LiteralPath $tmp -Recurse -Force } catch {}
}

if ($echecs.Count) {
    Write-Host "Aide en SYSTEM : $($echecs.Count) échec(s) sur $nbTests vérifications :"
    $echecs | ForEach-Object { Write-Host " - $_" }
    exit 1
}
Write-Host "Aide en SYSTEM : $nbTests vérifications, tout passe."
exit 0
