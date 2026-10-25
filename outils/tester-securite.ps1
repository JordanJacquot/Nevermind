# Tests de la page Sécurité : jamais « fait » ni « Aucune menace » quand ça n'a pas pu être fait ou lu. Sans droits
# administrateur et sans rien toucher sur le PC : les commandes de Windows (tâches planifiées, Defender) et l'Aide sont
# remplacées par de fausses versions, aucune tâche n'est désactivée et Defender n'est pas lu.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-securite.ps1
#
# Ce script contrôle :
#   1. « Désactiver ces tâches » : une tâche que Windows refuse de désactiver (ou qui reste active) n'est jamais comptée
#      comme faite ; aucune faite : une erreur qui dit « je n'ai pas pu » (rien dans l'historique) ; une partie : un
#      message qui nomme celles qui restent ;
#   2. lecture des menaces après une analyse Defender : par l'Aide (lecture.securite) quand Nevermind n'a pas les droits ;
#      sans Aide ni droits, ou si la lecture échoue : « résultat inconnu », jamais « Aucune menace ».
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

# ---------------------------------------------------------------------------
# Fausses versions (une fonction passe avant une commande du même nom) : rien n'est touché sur le PC
# ---------------------------------------------------------------------------
$script:Journal = New-Object System.Collections.Generic.List[string]
$script:Messages = New-Object System.Collections.Generic.List[string]
function script:Write-Log([string]$Message) { $script:Journal.Add($Message) }
function script:Show-Message([string]$Text, [string]$Icon = 'Information') { $script:Messages.Add("[$Icon] $Text") }
# Tâches : Refus = noms que Windows refuse de désactiver ; Muettes = noms « désactivés » sans erreur mais restés actifs
$script:Refus = @(); $script:Muettes = @(); $script:Desactivees = @{}
function script:Disable-ScheduledTask {
    [CmdletBinding()] param([string]$TaskName, [string]$TaskPath)
    if ($script:Refus -contains $TaskName) { throw 'Accès refusé.' }
    if ($script:Muettes -notcontains $TaskName) { $script:Desactivees["$TaskPath$TaskName"] = $true }
}
function script:Get-ScheduledTask {
    [CmdletBinding()] param([string]$TaskName, [string]$TaskPath)
    [pscustomobject]@{ TaskName = $TaskName; State = $(if ($script:Desactivees["$TaskPath$TaskName"]) { 'Disabled' } else { 'Ready' }) }
}
# Defender et l'Aide
$script:EstAdmin = $false
$script:AideLecture = $null; $script:AideAppels = 0
function script:Get-AideLecture([string]$Action) { $script:AideAppels++; if ($Action -ne 'lecture.securite') { throw "Action inattendue : $Action" }; $script:AideLecture }
$script:Detections = @(); $script:DetectionsErreur = $false
function script:Get-MpThreatDetection { [CmdletBinding()] param() if ($script:DetectionsErreur) { throw 'Lecture impossible.' }; $script:Detections }
function script:Get-MpThreat { [CmdletBinding()] param() @([pscustomobject]@{ ThreatID = 7; ThreatName = 'Trojan:Win32/Essai' }) }

$tok = $null; $err = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules 'securite.ps1'), [ref]$tok, [ref]$err)
if ($err) { Write-Host "securite.ps1 n'est pas lisible : $($err[0].Message)" -ForegroundColor Red; exit 1 }
function Get-Fonction([string]$Nom) {
    $a = $ast.Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $false)
    if (-not $a) { Write-Host "Fonction $Nom introuvable dans securite.ps1" -ForegroundColor Red; exit 1 }
    $a
}
foreach ($n in 'Disable-SuspectTasks', 'Get-ScanThreats') { Invoke-Expression ((Get-Fonction $n).Extent.Text -replace "^function\s+$([regex]::Escape($n))", "function script:$n") }

function Invoke-Desactiver($Taches) {
    $script:Messages.Clear()
    try { Disable-SuspectTasks $Taches; '' } catch { [string]$_.Exception.Message }
}

try {
    $t1 = @{ Name = 'Mise a jour'; Path = '\' }
    $t2 = @{ Name = 'Lanceur cache'; Path = '\Outils\' }

    # -----------------------------------------------------------------------
    # 1. « Désactiver ces tâches »
    # -----------------------------------------------------------------------
    $script:Refus = @(); $script:Muettes = @(); $script:Desactivees = @{}
    $e = Invoke-Desactiver @($t1, $t2)
    Test-Ok (-not $e -and -not $script:Messages.Count -and $script:Desactivees.Count -eq 2) "Tâches désactivées : aucune erreur ni message ($e $($script:Messages -join ' '))"

    $script:Refus = @('Mise a jour', 'Lanceur cache'); $script:Desactivees = @{}
    $e = Invoke-Desactiver @($t1, $t2)
    Test-Ok ($e -match "^Je n'ai pas pu désactiver ces tâches : Mise a jour, Lanceur cache" -and $e -match 'administrateur') "Windows refuse tout : erreur « je n'ai pas pu », rien de compté comme fait ($e)"

    $script:Refus = @(); $script:Muettes = @('Mise a jour'); $script:Desactivees = @{}
    $e = Invoke-Desactiver @($t1)
    Test-Ok ($e -match "^Je n'ai pas pu désactiver cette tâche : Mise a jour") "Tâche restée active sans erreur de Windows : pas comptée comme faite ($e)"

    $script:Refus = @('Lanceur cache'); $script:Muettes = @(); $script:Desactivees = @{}
    $e = Invoke-Desactiver @($t1, $t2)
    $m = $script:Messages -join ' '
    Test-Ok (-not $e -and $m -match '^\[Warning\] 1 tâche désactivée\.' -and $m -match "Je n'ai pas pu désactiver cette tâche : Lanceur cache") "Une partie seulement : message qui nomme celle qui reste ($e $m)"
    Test-Ok (@($script:Journal | Where-Object { $_ -match 'Lanceur cache.*non désactivée' }).Count -ge 1) 'Refus de Windows noté au journal'
    # L'action de la carte passe bien par cette fonction (plus de -ErrorAction SilentlyContinue qui cache le refus)
    $code = [IO.File]::ReadAllText((Join-Path $modules 'securite.ps1'))
    Test-Ok ($code -match "Label = 'Désactiver ces tâches'[^\r\n]*\r?\n\s*Script = \{ param\(\`$a\) Disable-SuspectTasks \`$a \}" -and $code -notmatch 'Disable-ScheduledTask[^\r\n]*SilentlyContinue') 'Carte « tâches suspectes » : l''action passe par Disable-SuspectTasks'

    # -----------------------------------------------------------------------
    # 2. Menaces après une analyse
    # -----------------------------------------------------------------------
    $debut = Get-Date
    $apres = $debut.AddSeconds(30)
    $avant = $debut.AddDays(-3)

    # Sans droits, l'Aide répond : sa lecture fait foi
    $script:EstAdmin = $false
    $script:AideLecture = [pscustomobject]@{ Menaces = @([pscustomobject]@{ ThreatID = '9'; ThreatName = 'Virus:Win32/Essai' })
        Detections = @([pscustomobject]@{ ThreatID = '9'; Quand = $apres.Ticks; Resources = @('file:_C:\Users\x\Downloads\a.exe') }, [pscustomobject]@{ ThreatID = '9'; Quand = $avant.Ticks; Resources = @('file:_C:\vieux.exe') }) }
    $r = Get-ScanThreats $debut
    Test-Ok ($r.Lu -and @($r.Menaces).Count -eq 1 -and $r.Menaces[0].Nom -eq 'Virus:Win32/Essai' -and $r.Menaces[0].Fichier -eq 'C:\Users\x\Downloads\a.exe') "Aide : la menace trouvée pendant l'analyse est lue, avec son nom et son fichier ($(@($r.Menaces).Count))"
    $script:AideLecture = [pscustomobject]@{ Menaces = @(); Detections = @(); DetectionsLues = $true }
    $r = Get-ScanThreats $debut
    Test-Ok ($r.Lu -and -not @($r.Menaces).Count) 'Aide : rien trouvé, résultat lu (« Aucune menace » permis)'
    # (lot A2) L'Aide n'a pas pu lire les détections de Defender (DetectionsLues faux) : une liste vide ne prouve rien
    $script:AideLecture = [pscustomobject]@{ Menaces = @(); Detections = @(); DetectionsLues = $false }
    $r = Get-ScanThreats $debut
    Test-Ok (-not $r.Lu -and -not @($r.Menaces).Count) 'Aide : détections de Defender non lues, résultat inconnu (jamais « Aucune menace »)'

    # Sans droits ni Aide : une lecture vide ne prouve rien
    $script:AideLecture = $null; $script:Detections = @(); $script:DetectionsErreur = $false
    $r = Get-ScanThreats $debut
    Test-Ok (-not $r.Lu) 'Sans droits ni Aide, lecture vide : résultat inconnu (jamais « Aucune menace »)'
    $script:DetectionsErreur = $true
    $r = Get-ScanThreats $debut
    Test-Ok (-not $r.Lu) 'Sans droits ni Aide, lecture en échec : résultat inconnu'
    $script:DetectionsErreur = $false
    $script:Detections = @([pscustomobject]@{ ThreatID = 7; InitialDetectionTime = $apres; Resources = @('file:_C:\b.exe') })
    $r = Get-ScanThreats $debut
    Test-Ok ($r.Lu -and @($r.Menaces).Count -eq 1 -and $r.Menaces[0].Nom -eq 'Trojan:Win32/Essai') 'Sans droits ni Aide, une menace lue quand même : affichée'

    # En administrateur : Nevermind lit lui-même, sans réveiller l'Aide
    $script:EstAdmin = $true; $script:AideAppels = 0
    $script:Detections = @()
    $r = Get-ScanThreats $debut
    Test-Ok ($r.Lu -and -not @($r.Menaces).Count -and -not $script:AideAppels) 'Administrateur, rien trouvé : résultat lu, l''Aide n''est pas appelée'
    $script:DetectionsErreur = $true
    $r = Get-ScanThreats $debut
    Test-Ok (-not $r.Lu -and @($script:Journal | Where-Object { $_ -match 'détections non lues' }).Count) 'Administrateur, lecture en échec : résultat inconnu, noté au journal'

    # La fin d'analyse n'affiche « Aucune menace » que si le résultat a été lu
    $fin = (Get-Fonction 'Invoke-DefenderScan').Extent.Text
    $iInconnu = $fin.IndexOf('if (-not $lu.Lu)'); $iAucune = $fin.IndexOf("'Aucune menace'")
    Test-Ok ($fin -match '\$lu = Get-ScanThreats \$start' -and $iInconnu -ge 0 -and $iAucune -gt $iInconnu -and $fin -notmatch 'Get-MpThreatDetection') 'Fin d''analyse : « Aucune menace » seulement quand le résultat a été lu'
}
catch {
    $echecs.Add("Arrêt inattendu : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))")
}

if ($echecs.Count) {
    Write-Host "$($echecs.Count) échec(s) sur $nbTests contrôles :" -ForegroundColor Red
    $echecs | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
    exit 1
}
Write-Host "Sécurité : les $nbTests contrôles sont bons." -ForegroundColor Green
exit 0
