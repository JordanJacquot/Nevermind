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
#      sans Aide ni droits, ou si la lecture échoue : « résultat inconnu », jamais « Aucune menace » ;
#   3. mise à jour autonome (mises-a-jour.ps1), mêmes règles que Mindset : une clé publique qui contient un élément de
#      clé privée (même vide, préfixé ou dans un sous-élément) ou sans Modulus est refusée ; un zip signé mais abîmé
#      (taille ou CRC) ou avec une entrée chiffrée n'est pas installé (contre-épreuve : .NET seul les acceptait) ; le
#      zip est dézippé depuis les octets vérifiés. Clé de test, zips locaux, tout dans un dossier de test (jamais Internet).
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

    # -----------------------------------------------------------------------
    # 3. Mise à jour autonome : clés et zips aussi stricts que Mindset
    # -----------------------------------------------------------------------
    $astMaj = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules 'mises-a-jour.ps1'), [ref]$tok, [ref]$err)
    if ($err) { throw "mises-a-jour.ps1 n'est pas lisible : $($err[0].Message)" }
    foreach ($n in 'Test-Signature', 'Expand-ZipVerifie', 'Install-Update') {
        $a = $astMaj.Find({ param($x) $x -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $x.Name -eq $n }, $false)
        if (-not $a) { throw "Fonction $n introuvable dans mises-a-jour.ps1" }
        Invoke-Expression ($a.Extent.Text -replace "^function\s+$([regex]::Escape($n))", "function script:$n")
    }
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    $bac = Join-Path ([IO.Path]::GetTempPath()) "nevermind-test-maj-$([guid]::NewGuid().ToString('N'))"
    New-Item -ItemType Directory -Force -Path $bac | Out-Null
    $tempAvant = $env:TEMP
    $rsa = New-Object System.Security.Cryptography.RSACryptoServiceProvider 3072
    try {
        $env:TEMP = $bac   # (le dossier de travail de la mise à jour, dans le dossier de test)
        $cles = Join-Path $bac 'cles'; New-Item -ItemType Directory -Force -Path $cles | Out-Null
        $publique = $rsa.ToXmlString($false)
        [IO.File]::WriteAllText((Join-Path $cles 'test.xml'), $publique)
        function Get-SignatureTest([byte[]]$Octets) { $rsa.SignData($Octets, [System.Security.Cryptography.CryptoConfig]::MapNameToOID('SHA256')) }
        $src = Join-Path $bac 'src'
        New-Item -ItemType Directory -Force -Path (Join-Path $src 'Nevermind\fichiers') | Out-Null
        [IO.File]::WriteAllText((Join-Path $src 'Nevermind\Nevermind.exe'), 'nouveau programme')
        [IO.File]::WriteAllText((Join-Path $src 'Nevermind\fichiers\Nevermind.ps1'), "`$AppVersion = '9.9.9'`r`n# nouvelle version, assez longue pour être compressée`r`n" + ('# ligne' * 50))
        $zBon = Join-Path $bac 'bon.zip'
        [IO.Compression.ZipFile]::CreateFromDirectory($src, $zBon)
        $octetsBons = [IO.File]::ReadAllBytes($zBon)
        [IO.File]::WriteAllBytes("$zBon.sig", (Get-SignatureTest $octetsBons))

        # Clés : un élément de clé privée, même vide, préfixé ou caché, et la clé est refusée
        $sigBon = [IO.File]::ReadAllBytes("$zBon.sig")
        function Test-Cle([string]$Xml) {
            $f = Join-Path $bac ("cle-" + [guid]::NewGuid().ToString('N') + '.xml')
            [IO.File]::WriteAllText($f, $Xml)
            Test-Signature $octetsBons $sigBon $f
        }
        Test-Ok (Test-Cle $publique) 'Mise à jour : clé publique seule, la bonne signature est acceptée'
        foreach ($ajout in '<D>AQAB</D>', '<P></P>', '<Q/>', '<DP>AQAB</DP>', '<DQ>AQAB</DQ>', '<InverseQ>AQAB</InverseQ>', '<x:D xmlns:x="urn:essai">AQAB</x:D>', '<Autre><InverseQ>AQAB</InverseQ></Autre>') {
            $xml = $publique.Replace('</RSAKeyValue>', "$ajout</RSAKeyValue>")
            $r = New-Object System.Security.Cryptography.RSACryptoServiceProvider
            $publiqueSelonNet = $false
            try { $r.FromXmlString($xml); $publiqueSelonNet = $r.PublicOnly } catch {} finally { $r.Dispose() }
            Test-Ok ($publiqueSelonNet -and -not (Test-Cle $xml)) "Mise à jour : clé avec l'élément privé $ajout refusée (contre-épreuve, .NET seul la croit publique : $publiqueSelonNet)"
        }
        Test-Ok (-not (Test-Cle ([regex]::Replace($publique, '<Modulus>.*</Modulus>', '')))) 'Mise à jour : clé sans Modulus refusée'
        Test-Ok (-not (Test-Cle ([regex]::Replace($publique, '<Exponent>.*</Exponent>', '')))) 'Mise à jour : clé sans Exponent refusée'

        # Zips trafiqués dans leurs en-têtes (répertoire central et en-tête local), puis signés de nouveau
        function New-ZipTrafique([string]$Nom, [scriptblock]$Changer) {
            $o = [IO.File]::ReadAllBytes($zBon)
            for ($i = 0; $i -le $o.Length - 46; $i++) {
                if ([BitConverter]::ToUInt32($o, $i) -ne 0x02014b50) { continue }
                $nomEntree = [Text.Encoding]::UTF8.GetString($o, $i + 46, [BitConverter]::ToUInt16($o, $i + 28)).Replace('\', '/')
                if ($nomEntree -eq 'Nevermind/fichiers/Nevermind.ps1') { & $Changer $o $i ([int][BitConverter]::ToUInt32($o, $i + 42)) }
            }
            $zip = Join-Path $bac "$Nom.zip"
            [IO.File]::WriteAllBytes($zip, $o)
            [IO.File]::WriteAllBytes("$zip.sig", (Get-SignatureTest $o))
            $zip
        }
        function Set-Taille([byte[]]$o, [int]$c, [int]$l, [uint32]$Taille) { [BitConverter]::GetBytes($Taille).CopyTo($o, $c + 24); [BitConverter]::GetBytes($Taille).CopyTo($o, $l + 22) }
        $zChiffre = New-ZipTrafique 'chiffre' { param($o, $c, $l) $o[$c + 8] = $o[$c + 8] -bor 1; $o[$l + 6] = $o[$l + 6] -bor 1 }
        $zLong = New-ZipTrafique 'plus-long' { param($o, $c, $l) Set-Taille $o $c $l 3 }
        $zCourtCrc = New-ZipTrafique 'plus-court-crc' { param($o, $c, $l) Set-Taille $o $c $l 100000; $o[$c + 16] = $o[$c + 16] -bxor 0xFF; $o[$l + 14] = $o[$l + 14] -bxor 0xFF }
        $zCourt = New-ZipTrafique 'plus-court' { param($o, $c, $l) Set-Taille $o $c $l 100000 }
        $zCrc = New-ZipTrafique 'crc' { param($o, $c, $l) $o[$c + 16] = $o[$c + 16] -bxor 0xFF; $o[$l + 14] = $o[$l + 14] -bxor 0xFF }
        $zDeflate = New-ZipTrafique 'deflate' { param($o, $c, $l) $debut = $l + 30 + [BitConverter]::ToUInt16($o, $l + 26) + [BitConverter]::ToUInt16($o, $l + 28); for ($k = 2; $k -lt 12; $k++) { $o[$debut + $k] = $o[$debut + $k] -bxor 0x5A } }
        function Invoke-Dezip([string]$Zip) {
            $d = Join-Path $bac ("x-" + [guid]::NewGuid().ToString('N'))
            try { Expand-ZipVerifie ([IO.File]::ReadAllBytes($Zip)) $d; @{ Dossier = $d; Erreur = '' } } catch { @{ Dossier = $d; Erreur = $_.Exception.Message } }
        }
        function Test-NetSeul([string]$Zip) {   # (contre-épreuve : ce que faisait Expand-Archive, c'est-à-dire .NET seul)
            $d = Join-Path $bac ("net-" + [guid]::NewGuid().ToString('N'))
            try { [IO.Compression.ZipFile]::ExtractToDirectory($Zip, $d); $true } catch { $false }
        }
        $x = Invoke-Dezip $zBon
        Test-Ok (-not $x.Erreur -and [IO.File]::ReadAllText((Join-Path $x.Dossier 'Nevermind\fichiers\Nevermind.ps1')) -match "9\.9\.9") "Mise à jour : un bon zip est dézippé ($($x.Erreur))"
        $abime = 'Le zip est abîmé : rien n''a été installé.'
        $chiffre = 'Le zip contient un fichier chiffré : rien n''a été installé.'
        # (contre-épreuve sur les quatre premiers : .NET seul les dézippait sans rien dire)
        foreach ($cas in @(@($zChiffre, 'entrée chiffrée', $chiffre, $true), @($zLong, 'fichier plus long que la taille annoncée', $abime, $true), @($zCourtCrc, 'fichier plus court que la taille annoncée et CRC faux', $abime, $true), @($zCrc, 'CRC faux, bonne taille', $abime, $true), @($zCourt, 'taille annoncée trop grande, CRC juste', $abime, $false), @($zDeflate, 'contenu compressé abîmé', $abime, $false))) {
            $netSeul = Test-NetSeul $cas[0]
            $x = Invoke-Dezip $cas[0]
            Test-Ok (($netSeul -or -not $cas[3]) -and $x.Erreur -eq $cas[2]) "Mise à jour : zip signé, $($cas[1]), refusé avec « $($cas[2]) » (reçu : $($x.Erreur) ; .NET seul l'acceptait : $netSeul)"
        }
        $zIllisible = Join-Path $bac 'illisible.zip'; [IO.File]::WriteAllBytes($zIllisible, [byte[]](1..200))
        $x = Invoke-Dezip $zIllisible
        Test-Ok ($x.Erreur -eq $abime) "Mise à jour : zip illisible refusé comme abîmé ($($x.Erreur))"
        $code = ${function:Expand-ZipVerifie}.ToString()
        Test-Ok ($code -match 'param\(\[byte\[\]\]\$Octets, \[string\]\$Dest\)') 'Mise à jour : Expand-ZipVerifie passe avec ses paramètres dans le fil séparé'

        # Install-Update en entier, avec de fausses versions de la fenêtre, du téléchargement et du fil séparé
        $script:Managed = $false; $script:AppVersion = '1.0.0'; $script:PublicKeyFile = $cles
        $script:ui = @{ UpdateBanner = [pscustomobject]@{ Visibility = 'Visible' } }
        $script:Window = [pscustomobject]@{}; $script:Window | Add-Member ScriptMethod Close { $script:Ferme = $true }
        function script:Confirm-Action([string]$Texte) { $true }
        function script:Set-Busy($On) { }
        function script:Set-Status([string]$Texte) { }
        function script:Invoke-Async([scriptblock]$Script, $Argument) {
            if ($Argument.Url) {   # (le téléchargement : copie des fichiers locaux, jamais Internet)
                New-Item -ItemType Directory -Force -Path $Argument.Dir | Out-Null
                Copy-Item -LiteralPath $Argument.Url -Destination $Argument.Zip; Copy-Item -LiteralPath $Argument.SigUrl -Destination "$($Argument.Zip).sig"; return 'OK'
            }
            # (le dézippage : dans un vrai fil séparé, comme Invoke-Async, où les fonctions de Nevermind n'existent pas)
            $ps = [PowerShell]::Create()
            try { [void]$ps.AddScript($Script.ToString()).AddArgument($Argument); $ps.Invoke() } finally { $ps.Dispose() }
        }
        function Invoke-Maj([string]$Zip) {
            $script:appRoot = Join-Path $bac ("installe-" + [guid]::NewGuid().ToString('N'))
            New-Item -ItemType Directory -Force -Path (Join-Path $script:appRoot 'fichiers') | Out-Null
            [IO.File]::WriteAllText((Join-Path $script:appRoot 'fichiers\Nevermind.ps1'), "`$AppVersion = '1.0.0'")
            $script:Messages.Clear(); $script:Ferme = $false
            $script:PendingUpdate = @{ Version = '9.9.9'; Url = $Zip; SigUrl = "$Zip.sig" }
            Install-Update
            [IO.File]::ReadAllText((Join-Path $script:appRoot 'fichiers\Nevermind.ps1'))
        }
        $v = Invoke-Maj $zBon
        Test-Ok ($v -match '9\.9\.9' -and $script:Ferme -and -not $script:Messages.Count) "Mise à jour : un bon zip signé est installé (messages : $($script:Messages -join ' '))"
        foreach ($cas in @(@($zChiffre, $chiffre), @($zLong, $abime), @($zCourtCrc, $abime), @($zCrc, $abime))) {
            $v = Invoke-Maj $cas[0]
            $reste = @(Get-ChildItem -LiteralPath $bac -Directory -Filter 'Nevermind-maj-*').Count
            Test-Ok ($v -match '1\.0\.0' -and -not $script:Ferme -and ($script:Messages -join ' ') -match [regex]::Escape($cas[1]) -and -not $reste) "Mise à jour : $(Split-Path $cas[0] -Leaf) signé n'est pas installé, message « $($cas[1]) », rien ne reste (messages : $($script:Messages -join ' ') ; dossiers restés : $reste)"
        }
        $code = (Get-Item function:Install-Update).ScriptBlock.ToString()
        Test-Ok ($code -notmatch 'Expand-Archive' -and $code -match '\$octets = \[IO\.File\]::ReadAllBytes\(\$zip\); \$signed = Test-Signature \$octets' -and $code -match 'Octets = \$octets') 'Mise à jour : dézippée depuis les octets dont la signature a été vérifiée (pas le fichier relu)'
    } finally {
        $env:TEMP = $tempAvant
        $rsa.Dispose()
        Remove-Item -LiteralPath $bac -Recurse -Force -ErrorAction SilentlyContinue
    }
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
