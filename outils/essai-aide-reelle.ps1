# Nevermind : l'Aide en SYSTEM essayée en vrai, sur la machine jetable de GitHub Actions (workflow
# .github\workflows\aide-reelle.yml, lancé à la main). Vraies tâches planifiées, vrais comptes Windows, vraie installation
# par l'installateur de Mindset : JAMAIS sur un vrai PC. Le script refuse de tourner hors de GitHub Actions.
#
#   .\outils\essai-aide-reelle.ps1 -Cas 0     préparation (Nevermind construit, Mindset construit, clé de TEST, paquets)
#   .\outils\essai-aide-reelle.ps1 -Cas N     un cas (1 à 7), voir la liste plus bas
#
# Le client est le vrai Mindset.exe de Nevermind (outils\lanceur.cs), installé par Mindset dans Program Files\Mindset,
# lancé sous le compte voulu : dans le paquet d'essai, seul fichiers\OptiGame.ps1 est remplacé par un petit client qui
# se sert des vrais modules (natif.cs, aide-actions.ps1, aide-client.ps1). L'Aide (NevermindAide.exe et son script) est
# celle du dépôt. La demande de Windows (où l'on clique « Oui ») ne s'automatise pas : le client note la commande que
# Nevermind lui passe, et l'essai la lance en administrateur, comme si l'administrateur avait accepté.
# La clé de signature est une clé RSA de TEST créée ici, ajoutée aux clés publiques de la copie d'essai de Mindset.
#
#   1. installation pour un compte standard au nom accentué (« Élodie Test ») : tâche « \Mindset\Nevermind (aide) » en
#      SYSTEM, -Sid d'Élodie dans ses arguments, lue et lancée par elle seule
#   2. son Mindset.exe lance la tâche et obtient des actions en lecture seule ; l'Aide tourne en SYSTEM, compile dans
#      aide-temp (réservé au système et aux administrateurs) ; « fermer » ne ferme que le programme d'Élodie
#   3. un autre compte standard (Bruno) : ni la tâche, ni le canal d'Élodie
#   4. mise à jour depuis Nevermind 1.10.0 (installé par Mindset 2.10.0 : l'ancienne tâche « droits les plus élevés ») :
#      l'ancienne tâche remplacée par la tâche SYSTEM ; et réparée aussi quand Mindset refait ses tâches
#   5. tâche ancienne, cassée ou désactivée : la demande de Windows, jamais l'ancienne tâche
#   6. désinstallation (désinstalleur de Nevermind, puis de Mindset) : plus aucune tâche
#   7. la tâche SYSTEM ne voit pas l'environnement d'Élodie (variable témoin dans HKCU\Environment, contre-épreuve sous
#      son compte), sans aucune variable COR_*
# Code de sortie 1 si un cas échoue.
param([int]$Cas = -1)

if ($env:GITHUB_ACTIONS -ne 'true') {
    Write-Host 'Refusé : cet essai crée des comptes Windows et des tâches planifiées, et installe Nevermind pour de vrai. Il ne tourne que sur la machine jetable de GitHub Actions.' -ForegroundColor Red
    exit 2
}
$ErrorActionPreference = 'Stop'
$Racine = 'C:\essai'
$PowerShellExe = Join-Path ([Environment]::GetFolderPath('System')) 'WindowsPowerShell\v1.0\powershell.exe'
$Schtasks = Join-Path ([Environment]::GetFolderPath('System')) 'schtasks.exe'
$NomTache = 'Nevermind (aide)'
$Depot = Split-Path $PSScriptRoot -Parent                 # le dépôt de Nevermind (checkout)
$MindsetGit = Join-Path $Racine 'mindset'                 # le dépôt de Mindset (checkout du workflow)
$MindsetDepot = Join-Path $Racine 'mindset-depot'         # sa copie construite, avec la clé de TEST
$Mindset210 = Join-Path $Racine 'mindset-210'             # Mindset 2.10.0 (son installateur crée l'ancienne tâche)
$Paquets = Join-Path $Racine 'paquets'
$Sorties = Join-Path $Racine 'sorties'
$Clients = Join-Path $Racine 'clients'
$ProgrammesC = Join-Path $env:ProgramFiles 'Mindset'
$NevermindC = Join-Path $ProgrammesC 'Nevermind'
$MindsetC = Join-Path $ProgrammesC 'Mindset'
$Moi = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value

$script:Reussis = 0
$script:Echoues = New-Object Collections.Generic.List[string]
$script:Notes = New-Object Collections.Generic.List[string]
function Write-Partie([string]$Titre) { Write-Host ''; Write-Host $Titre -ForegroundColor Cyan }
function Add-Note([string]$Texte) { $script:Notes.Add($Texte); Write-Host "  NOTE   $Texte" -ForegroundColor Yellow }
function Test-Cas([string]$Nom, [scriptblock]$Condition) {
    $detail = ''
    try { $r = @(& $Condition) } catch { $r = @(); $detail = "$($_.Exception.Message) $($_.ScriptStackTrace)" }
    if ($r.Count -and $r[-1] -is [bool] -and $r[-1]) {
        $script:Reussis++
        Write-Host "  OK     $Nom" -ForegroundColor Green
    } else {
        $script:Echoues.Add($Nom)
        Write-Host "  ÉCHEC  $Nom$(if ($detail) { " ($detail)" })" -ForegroundColor Red
        Show-Diagnostic
    }
}
function Show-Diagnostic {
    try {
        foreach ($t in @(Get-ScheduledTask -TaskPath '\Mindset\' -ErrorAction SilentlyContinue)) {
            $i = Get-ScheduledTaskInfo -TaskPath '\Mindset\' -TaskName $t.TaskName -ErrorAction SilentlyContinue
            Write-Host "         | tâche $($t.TaskName) [$($t.State)] compte $($t.Principal.UserId) $($t.Principal.LogonType) $($t.Principal.RunLevel) ; dernier lancement $($i.LastRunTime) code $($i.LastTaskResult) ; $($t.Actions[0].Execute) $($t.Actions[0].Arguments)" -ForegroundColor DarkGray
        }
        foreach ($p in @(Get-AidesEnCours)) { Write-Host "         | Aide en cours : $($p.Id) compte $($p.Sid) session $($p.Session) $($p.Chemin)" -ForegroundColor DarkGray }
    } catch {}
}
function Read-Json([string]$Fichier) {
    if (-not (Test-Path -LiteralPath $Fichier)) { return $null }
    try { [IO.File]::ReadAllText($Fichier, [Text.Encoding]::UTF8).TrimStart([char]0xFEFF) | ConvertFrom-Json } catch { $null }
}
function Write-Utf8([string]$Fichier, [string]$Texte) { [IO.File]::WriteAllText($Fichier, $Texte, (New-Object Text.UTF8Encoding $false)) }
function Wait-Jusque([scriptblock]$Condition, [int]$Secondes = 60) {
    $limite = (Get-Date).AddSeconds($Secondes)
    while ((Get-Date) -lt $limite) { if (& $Condition) { return $true }; Start-Sleep -Milliseconds 500 }
    [bool](& $Condition)
}
function New-Signature([string]$Fichier) {
    $rsa = New-Object Security.Cryptography.RSACryptoServiceProvider
    try {
        $rsa.PersistKeyInCsp = $false
        $rsa.FromXmlString([IO.File]::ReadAllText((Join-Path $Racine 'cle\privee-test.xml')))
        [IO.File]::WriteAllBytes("$Fichier.sig", $rsa.SignData([IO.File]::ReadAllBytes($Fichier), [Security.Cryptography.CryptoConfig]::MapNameToOID('SHA256')))
    } finally { $rsa.Dispose() }
}

# Un compte Windows local de test (créé, ou mot de passe renouvelé), avec son profil
function Get-Compte([string]$Nom) {
    $mdp = 'Aa1!' + [guid]::NewGuid().ToString('N').Substring(0, 14)
    $s = ConvertTo-SecureString $mdp -AsPlainText -Force
    $u = Get-LocalUser -Name $Nom -ErrorAction SilentlyContinue
    if ($u) { Set-LocalUser -Name $Nom -Password $s } else { $u = New-LocalUser -Name $Nom -Password $s -PasswordNeverExpires -AccountNeverExpires }
    $cred = New-Object Management.Automation.PSCredential(".\$Nom", $s)
    $p = Start-Process -FilePath "$env:SystemRoot\System32\cmd.exe" -ArgumentList '/c', 'exit' -Credential $cred -LoadUserProfile -WorkingDirectory $Racine -PassThru -WindowStyle Hidden
    [void]$p.WaitForExit(60000)
    $sid = $u.SID.Value
    $profil = [string](Get-Item -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\$sid").GetValue('ProfileImagePath')
    @{ Nom = $Nom; Sid = $sid; Cred = $cred; Profil = $profil }
}
$NomElodie = "$([char]0xC9)lodie Test"

# Ce que Windows dit d'une tâche (XML exporté, droits, dernier lancement)
function Get-InfoTache([string]$Nom = $NomTache, [string]$Dossier = '\Mindset\') {
    $xml = $null
    try { $xml = [xml](Export-ScheduledTask -TaskPath $Dossier -TaskName $Nom -ErrorAction Stop) } catch { return $null }
    $pr = $xml.Task.Principals.Principal
    $i = Get-ScheduledTaskInfo -TaskPath $Dossier -TaskName $Nom -ErrorAction SilentlyContinue
    $sddl = ''
    try { $s = New-Object -ComObject Schedule.Service; $s.Connect(); $sddl = $s.GetFolder($Dossier.TrimEnd('\')).GetTask($Nom).GetSecurityDescriptor(4) } catch {}
    [pscustomobject]@{
        Compte = [string]$pr.UserId; RunLevel = $(if ([string]$pr.RunLevel) { [string]$pr.RunLevel } else { 'LeastPrivilege' })
        Commande = [string]$xml.Task.Actions.Exec.Command; Arguments = [string]$xml.Task.Actions.Exec.Arguments
        Active = ([string]$xml.Task.Settings.Enabled -ne 'false'); Dernier = $(if ($i) { $i.LastRunTime } else { $null }); Sddl = [string]$sddl; Xml = $xml.OuterXml
    }
}
function ConvertTo-Sid([string]$Compte) {
    if ($Compte -match '^S-1-') { return $Compte }
    try { (New-Object Security.Principal.NTAccount($Compte)).Translate([Security.Principal.SecurityIdentifier]).Value } catch { '' }
}
# Les droits sur la tâche : le système et les administrateurs, et l'utilisateur $Sid en lecture et lancement ; personne d'autre
function Test-DroitsTache([string]$Sddl, [string]$Sid) {
    $lui = $false
    foreach ($m in [regex]::Matches($Sddl, '\(([A-Z]+);[^;]*;([^;]*);[^;]*;[^;]*;([^)]+)\)')) {
        $type = $m.Groups[1].Value; $droits = $m.Groups[2].Value; $qui = $m.Groups[3].Value
        $qui = switch ($qui) { 'SY' { 'S-1-5-18' } 'BA' { 'S-1-5-32-544' } default { $qui } }
        if ($type -ne 'A' -or $qui -in 'S-1-5-18', 'S-1-5-32-544') { continue }
        if ($qui -eq $Sid -and $droits -in 'FRFX', '0x1200a9') { $lui = $true; continue }
        Write-Host "         | droit de trop sur la tâche : $($m.Value)" -ForegroundColor DarkGray
        return $false
    }
    $lui
}
# La tâche SYSTEM de l'Aide, pour $Sid, vers NevermindAide.exe de $Dossier
function Test-TacheSysteme([string]$Sid, [string]$Dossier = $NevermindC) {
    $t = Get-InfoTache
    if (-not $t) { Write-Host '         | tâche absente' -ForegroundColor DarkGray; return $false }
    Write-Host "         | tâche : $($t.Compte) $($t.RunLevel) ; $($t.Commande) $($t.Arguments) ; $($t.Sddl)" -ForegroundColor DarkGray
    (ConvertTo-Sid $t.Compte) -eq 'S-1-5-18' -and $t.RunLevel -eq 'HighestAvailable' -and $t.Commande -eq "`"$Dossier\NevermindAide.exe`"" -and
        $t.Arguments -ceq "-Sid $Sid" -and (Test-DroitsTache $t.Sddl $Sid)
}
# Les Aides qui tournent : PID, compte, session
function Get-AidesEnCours {
    foreach ($p in @(Get-CimInstance Win32_Process -Filter "Name='NevermindAide.exe'" -ErrorAction SilentlyContinue)) {
        $sid = ''; try { $sid = [string](Invoke-CimMethod -InputObject $p -MethodName GetOwnerSid -ErrorAction Stop).Sid } catch {}
        [pscustomobject]@{ Id = [int]$p.ProcessId; Sid = $sid; Session = [int]$p.SessionId; Chemin = [string]$p.ExecutablePath }
    }
}
function Stop-Aides {
    foreach ($d in '\Mindset\', '\MindsetEssai\') { foreach ($t in @(Get-ScheduledTask -TaskPath $d -ErrorAction SilentlyContinue)) { try { Stop-ScheduledTask -TaskPath $d -TaskName $t.TaskName -ErrorAction SilentlyContinue } catch {} } }
    foreach ($p in @(Get-AidesEnCours)) { try { Stop-Process -Id $p.Id -Force -ErrorAction Stop } catch { & "$env:SystemRoot\System32\taskkill.exe" /F /PID $p.Id 2>&1 | Out-Null } }
    $null = Wait-Jusque { -not @(Get-AidesEnCours).Count } 20
}
# Remet la machine à zéro : ni Mindset ni Nevermind, ni tâche \Mindset\, ni registre de Mindset
function Reset-Machine {
    Stop-Aides
    foreach ($p in @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)) {
        $c = [string]$p.ExecutablePath
        if ($c -like "$ProgrammesC\*" -or $c -like "$env:ProgramFiles\MindsetEssaiEnv\*" -or $c -like "$Racine\appli\*") { try { Stop-Process -Id $p.ProcessId -Force } catch {} }
    }
    Start-Sleep -Milliseconds 500
    foreach ($d in '\Mindset\', '\MindsetEssai\') {
        foreach ($t in @(Get-ScheduledTask -TaskPath $d -ErrorAction SilentlyContinue)) { Unregister-ScheduledTask -TaskPath $d -TaskName $t.TaskName -Confirm:$false -ErrorAction SilentlyContinue }
        try { $s = New-Object -ComObject Schedule.Service; $s.Connect(); $s.GetFolder('\').DeleteFolder($d.Trim('\'), 0) } catch {}
    }
    foreach ($d in $ProgrammesC, "$env:ProgramFiles\MindsetEssaiEnv") {
        if (Test-Path -LiteralPath $d) { & "$env:SystemRoot\System32\takeown.exe" /F $d /R /D Y 2>&1 | Out-Null; & "$env:SystemRoot\System32\icacls.exe" $d /reset /T /C /Q 2>&1 | Out-Null; Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue }
    }
    foreach ($c in 'HKLM:\SOFTWARE\Mindset', 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Mindset') { Remove-Item -Path $c -Recurse -Force -ErrorAction SilentlyContinue }
    Get-ChildItem -LiteralPath $Clients -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
}

# L'installateur de Mindset (installer-admin.ps1 -Action appli, en administrateur, comme une demande de Windows acceptée)
function Install-Appli([string]$Installateur, [string]$Zip, [string]$Sid) {
    $f = Join-Path $Sorties "install-$([guid]::NewGuid().ToString('N').Substring(0, 8)).json"
    & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installateur -Action appli -Zip $Zip -Signature "$Zip.sig" -Destination $NevermindC -DossierZip OptiGame -Exe Nevermind.exe -TacheAide $NomTache -ExeAide NevermindAide.exe -Sid $Sid -Resultat $f | Out-Host
    $r = Read-Json $f
    Write-Host "         | installation de $(Split-Path $Zip -Leaf) (installateur $Installateur) : $($r.ok) $($r.message)" -ForegroundColor DarkGray
    $r
}
$InstallateurNeuf = Join-Path $MindsetDepot 'Mindset\fichiers\installer-admin.ps1'
$Installateur210 = Join-Path $Mindset210 'Mindset\fichiers\installer-admin.ps1'
$PaquetEssai = Join-Path $Paquets 'essai\OptiGame.zip'     # Nevermind du dépôt, OptiGame.ps1 remplacé par le client
$PaquetVrai = Join-Path $Paquets 'vrai\OptiGame.zip'       # Nevermind du dépôt, tel quel
$Paquet110 = Join-Path $Paquets '1.10.0\OptiGame.zip'      # la Release v1.10.0 (signée par la vraie clé)

# Le client : Mindset.exe de Nevermind (le paquet d'essai) sous le compte $Compte. Il lit sa consigne dans
# C:\essai\clients\<son SID>.json. Une demande de Windows qu'il fait est lancée ici, en administrateur, si -Accepter.
function Invoke-Client($Compte, [hashtable]$Consigne, [switch]$Fond) {
    $id = [guid]::NewGuid().ToString('N').Substring(0, 8)
    $Consigne.Sortie = Join-Path $Sorties "client-$id.json"
    $Consigne.RunAsFichier = Join-Path $Sorties "runas-$id.json"
    Write-Utf8 (Join-Path $Clients "$($Compte.Sid).json") ($Consigne | ConvertTo-Json -Depth 5)
    $p = Start-Process -FilePath (Join-Path $NevermindC 'Mindset.exe') -Credential $Compte.Cred -LoadUserProfile -WorkingDirectory $env:SystemRoot -PassThru
    if ($Fond) { Start-Sleep -Seconds 3; return $p }
    $limite = (Get-Date).AddSeconds(300)
    $script:RunAsLancees = @()
    while (-not $p.HasExited -and (Get-Date) -lt $limite) {
        if (Test-Path -LiteralPath $Consigne.RunAsFichier) {
            $d = Read-Json $Consigne.RunAsFichier
            if ($d) {
                Remove-Item -LiteralPath $Consigne.RunAsFichier -Force
                # « Oui » dans la demande de Windows : l'administrateur (runneradmin) lance la commande de Nevermind
                $a = Start-Process -FilePath ([string]$d.FilePath) -ArgumentList @($d.Args) -WorkingDirectory $env:SystemRoot -PassThru
                $script:RunAsLancees += $a
                Write-Host "         | demande de Windows acceptée : $($d.FilePath) $(@($d.Args) -join ' ') (processus $($a.Id))" -ForegroundColor DarkGray
            }
        }
        Start-Sleep -Milliseconds 300
    }
    if (-not $p.HasExited) { try { $p.Kill() } catch {}; Write-Host '         | client arrêté (trop long)' -ForegroundColor DarkGray }
    $s = Read-Json $Consigne.Sortie
    if (-not $s) { Write-Host "         | client $($Consigne.Mode) ($($Compte.Nom)) : pas de sortie (code $($p.ExitCode))" -ForegroundColor DarkGray; return [pscustomobject]@{ Erreur = 'pas de sortie'; RunAs = @(); Journal = @(); R = $null; E = $null } }
    Write-Host "         | client $($Consigne.Mode) ($($Compte.Nom), $($s.Duree) s) : état « $($s.Etat) », demandes de Windows $(@($s.RunAs).Count)$(if ($s.Erreur) { ", ERREUR $($s.Erreur)" })" -ForegroundColor DarkGray
    foreach ($k in @($s.R.PSObject.Properties)) { Write-Host "         |   $($k.Name) : $($k.Value | ConvertTo-Json -Compress -Depth 3)" -ForegroundColor DarkGray }
    foreach ($k in @($s.E.PSObject.Properties)) { Write-Host "         |   erreur $($k.Name) : $($k.Value)" -ForegroundColor DarkGray }
    foreach ($l in @($s.Journal)) { Write-Host "         |   journal : $l" -ForegroundColor DarkGray }
    $s
}
function Get-Bonjour($S) { if ($S -and $S.R -and $S.R.PSObject.Properties['bonjour']) { $S.R.bonjour } else { $null } }

# Le client du paquet d'essai (il remplace fichiers\OptiGame.ps1)
$TexteClient = @'
# Essai de l'Aide en vrai (outils\essai-aide-reelle.ps1) : ce fichier remplace OptiGame.ps1 dans le paquet d'essai.
# Mindset.exe de Nevermind le lance sous le compte de l'essai ; il se sert des vrais modules de Nevermind.
$ErrorActionPreference = 'Stop'
$debut = [DateTime]::Now
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$cfg = [IO.File]::ReadAllText("C:\essai\clients\$sid.json", [Text.Encoding]::UTF8) | ConvertFrom-Json
$s = [ordered]@{ Compte = $sid; Pid = $PID; Session = [Diagnostics.Process]::GetCurrentProcess().SessionId; Etat = ''; Erreur = ''; RunAs = @(); R = [ordered]@{}; E = [ordered]@{}; Journal = @() }
$script:Journal = New-Object Collections.Generic.List[string]
try {
    $AppDir = $PSScriptRoot
    $ModulesDir = Join-Path $AppDir 'modules'
    $racineNevermind = Split-Path $AppDir -Parent
    function Get-AppRoot { $racineNevermind }
    function Write-Log([string]$m) { $script:Journal.Add($m) }
    function Wait-Handle { }
    function Invoke-Async($Sb, $X) { & $Sb $X }
    Add-Type -AssemblyName WindowsBase
    Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $ModulesDir 'natif.cs'), [Text.Encoding]::UTF8))
    . (Join-Path $ModulesDir 'aide-actions.ps1')
    . (Join-Path $ModulesDir 'aide-client.ps1')
    $AideAttenteWindows = if ($cfg.AttenteWindows) { [int]$cfg.AttenteWindows } else { 90 }
    # La demande de Windows : notée, et passée à l'essai (qui la lance en administrateur) si la consigne l'accepte
    function Start-Process {
        param([string]$FilePath, [string[]]$ArgumentList, [string]$Verb, [string]$WorkingDirectory, $ErrorAction)
        $s.RunAs += "$Verb|$FilePath|$($ArgumentList -join ' ')"
        if ($Verb -ne 'RunAs' -or -not $cfg.Accepter) { throw 'L''opération a été annulée par l''utilisateur.' }
        [IO.File]::WriteAllText($cfg.RunAsFichier, (ConvertTo-Json -InputObject @{ FilePath = $FilePath; Args = @($ArgumentList) } -Compress), (New-Object Text.UTF8Encoding $false))
    }
    $s.Etat = Get-AideEtatTache (Read-AideTache $AideTache $AideSchtasks) $AideExe $AideSid
    switch ([string]$cfg.Mode) {
        'appel' {
            foreach ($a in @($cfg.Actions)) {
                try { $s.R[[string]$a.Nom] = Invoke-Aide ([string]$a.Action) $a.P 60000 -SansDemande:([bool]$cfg.SansDemande) }
                catch { $s.E[[string]$a.Nom] = $_.Exception.Message }
            }
        }
        'canal' {
            # Le canal d'un autre compte, ouvert directement
            $c = New-Object System.IO.Pipes.NamedPipeClientStream('.', [string]$cfg.Canal, [System.IO.Pipes.PipeDirection]::InOut)
            try { $c.Connect(5000); $s.R['canal'] = 'ouvert' } catch { $s.E['canal'] = "$($_.Exception.GetType().Name) : $($_.Exception.Message)" } finally { $c.Dispose() }
        }
        'lancer' {
            $null = & $AideSchtasks /Run /TN $AideTache 2>&1; $s.R['lancer'] = $LASTEXITCODE
            $null = & $AideSchtasks /Query /TN $AideTache 2>&1; $s.R['lire'] = $LASTEXITCODE
        }
        'garder' { Start-Sleep -Seconds ([int]$cfg.Secondes) }
    }
} catch { $s.Erreur = "$($_.Exception.Message) $($_.ScriptStackTrace)" }
$s.Journal = @($script:Journal)
$s.Duree = [math]::Round(([DateTime]::Now - $debut).TotalSeconds, 1)
[IO.File]::WriteAllText([string]$cfg.Sortie, (ConvertTo-Json -InputObject $s -Depth 6), (New-Object Text.UTF8Encoding $false))
'@

# =====================================================================================================================
# 0. Préparation
# =====================================================================================================================
function Initialize-Essai {
    Write-Partie '0. Préparation'
    foreach ($d in $Racine, "$Racine\cle", $Paquets, $Sorties, $Clients, "$Racine\appli") { New-Item -ItemType Directory -Force -Path $d | Out-Null }
    # Tout le monde lit C:\essai ; les clients (autres comptes) écrivent leurs sorties
    & "$env:SystemRoot\System32\icacls.exe" $Racine /grant '*S-1-1-0:(OI)(CI)RX' /Q | Out-Null
    & "$env:SystemRoot\System32\icacls.exe" $Sorties /grant '*S-1-1-0:(OI)(CI)M' /Q | Out-Null
    & "$env:SystemRoot\System32\wevtutil.exe" sl Microsoft-Windows-TaskScheduler/Operational /e:true | Out-Null

    $rsa = New-Object Security.Cryptography.RSACryptoServiceProvider(3072)
    try {
        $rsa.PersistKeyInCsp = $false
        Write-Utf8 (Join-Path $Racine 'cle\privee-test.xml') $rsa.ToXmlString($true)
        Write-Utf8 (Join-Path $Racine 'cle\publique-test.xml') $rsa.ToXmlString($false)
    } finally { $rsa.Dispose() }

    # Mindset (la branche choisie dans le workflow), construit, avec la clé de TEST
    if (Test-Path -LiteralPath $MindsetDepot) { Remove-Item -LiteralPath $MindsetDepot -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $MindsetDepot | Out-Null
    Copy-Item -LiteralPath (Join-Path $MindsetGit 'Mindset') -Destination $MindsetDepot -Recurse
    Copy-Item -LiteralPath (Join-Path $MindsetGit 'outils') -Destination $MindsetDepot -Recurse
    Copy-Item -LiteralPath (Join-Path $Racine 'cle\publique-test.xml') -Destination (Join-Path $MindsetDepot 'Mindset\fichiers\cles-publiques\essai-test.xml')
    & (Join-Path $MindsetDepot 'outils\construire.ps1') | Out-Host
    Test-Cas "Mindset construit ($(& git -C $MindsetGit log --oneline -1))" { Test-Path -LiteralPath (Join-Path $MindsetDepot 'Mindset\Mindset.exe') }
    Test-Cas 'son installateur crée la tâche d''aide en SYSTEM (Mindset avec le lot A2)' { [IO.File]::ReadAllText($InstallateurNeuf, [Text.Encoding]::UTF8) -match 'function Update-TachesAide' }
    # Mindset 2.10.0 (son installateur, tel que publié)
    $ErrorActionPreference = 'Continue'   # (git écrit ses messages sur la sortie d'erreur)
    if (Test-Path -LiteralPath $Mindset210) { & git -C $MindsetGit worktree remove --force $Mindset210 2>&1 | Out-Null }
    & git -C $MindsetGit worktree add --detach $Mindset210 v2.10.0 2>&1 | ForEach-Object { "$_" } | Out-Host
    $ErrorActionPreference = 'Stop'
    Test-Cas 'Mindset 2.10.0 : son installateur crée l''ancienne tâche d''aide (droits les plus élevés, sous le compte)' { [IO.File]::ReadAllText($Installateur210, [Text.Encoding]::UTF8) -match 'New-ScheduledTaskPrincipal -UserId \$Sid -LogonType Interactive -RunLevel Highest' }

    # Nevermind du dépôt, construit
    & (Join-Path $Depot 'outils\construire.ps1') | Out-Host
    $og = Join-Path $Depot 'OptiGame'
    Test-Cas 'Nevermind construit (Mindset.exe, NevermindAide.exe, désinstalleur)' { (Test-Path -LiteralPath "$og\Mindset.exe") -and (Test-Path -LiteralPath "$og\NevermindAide.exe") -and (Test-Path -LiteralPath "$og\Désinstaller Nevermind.exe") }
    foreach ($genre in 'vrai', 'essai') {
        $d = Join-Path $Paquets $genre
        if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force }
        New-Item -ItemType Directory -Force -Path $d | Out-Null
        Copy-Item -LiteralPath $og -Destination $d -Recurse
        if ($genre -eq 'essai') { [IO.File]::WriteAllText((Join-Path $d 'OptiGame\fichiers\OptiGame.ps1'), $TexteClient, (New-Object Text.UTF8Encoding $true)) }
        Compress-Archive -Path (Join-Path $d 'OptiGame') -DestinationPath (Join-Path $d 'OptiGame.zip') -Force
        New-Signature (Join-Path $d 'OptiGame.zip')
    }
    # La Release v1.10.0, vérifiée avec les vraies clés de Mindset
    $d = Join-Path $Paquets '1.10.0'
    if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $d | Out-Null
    $ErrorActionPreference = 'Continue'
    & gh release download v1.10.0 -R JordanJacquot/Nevermind -p 'OptiGame.zip*' -D $d 2>&1 | ForEach-Object { "$_" } | Out-Host
    $ErrorActionPreference = 'Stop'
    . (Join-Path $MindsetGit 'Mindset\fichiers\modules\signature.ps1')
    Test-Cas 'Release Nevermind v1.10.0 téléchargée, signée par la vraie clé' {
        Test-Signature ([IO.File]::ReadAllBytes($Paquet110)) ([IO.File]::ReadAllBytes("$Paquet110.sig")) (Join-Path $MindsetGit 'Mindset\fichiers\cles-publiques')
    }
    Test-Cas 'paquets d''essai signés avec la clé de TEST, acceptés par la copie d''essai de Mindset' {
        $cles = Join-Path $MindsetDepot 'Mindset\fichiers\cles-publiques'
        (Test-Signature ([IO.File]::ReadAllBytes($PaquetEssai)) ([IO.File]::ReadAllBytes("$PaquetEssai.sig")) $cles) -and (Test-Signature ([IO.File]::ReadAllBytes($PaquetVrai)) ([IO.File]::ReadAllBytes("$PaquetVrai.sig")) $cles)
    }
    # Un petit programme qui attend (fermé par l'Aide au cas 2)
    $cs = Join-Path $Racine 'appli\dormeur.cs'
    Write-Utf8 $cs 'static class Dormeur { static void Main() { System.Threading.Thread.Sleep(System.Threading.Timeout.Infinite); } }'
    & (Join-Path $env:windir 'Microsoft.NET\Framework64\v4.0.30319\csc.exe') /nologo /target:winexe "/out:$Racine\appli\Dormeur.exe" $cs | Out-Host
    Test-Cas 'programme d''essai Dormeur.exe construit' { Test-Path -LiteralPath "$Racine\appli\Dormeur.exe" }
}

# =====================================================================================================================
# 1. Installation pour Élodie : la tâche SYSTEM
# =====================================================================================================================
function Test-Cas1 {
    Write-Partie '1. Installation pour un compte standard au nom accentué : la tâche de l''Aide en SYSTEM'
    Reset-Machine
    $e = Get-Compte $NomElodie
    Test-Cas 'le compte « Élodie Test » existe, avec son dossier' { $e.Profil -eq "C:\Users\$NomElodie" }
    $r = Install-Appli $InstallateurNeuf $PaquetEssai $e.Sid
    Test-Cas 'Nevermind installé par l''installateur de Mindset dans Program Files\Mindset\Nevermind' { $r.ok -and (Test-Path -LiteralPath "$NevermindC\NevermindAide.exe") }
    Test-Cas 'tâche « \Mindset\Nevermind (aide) » : SYSTEM, NevermindAide.exe -Sid <Élodie>, lue et lancée par Élodie seule' { Test-TacheSysteme $e.Sid }
}

# =====================================================================================================================
# 2. Élodie : son Mindset.exe parle à l'Aide en SYSTEM
# =====================================================================================================================
function Test-Cas2 {
    Write-Partie '2. Le Mindset.exe d''Élodie lance la tâche et obtient des actions en lecture seule'
    Reset-Machine
    $e = Get-Compte $NomElodie
    $b = Get-Compte 'Bruno Essai'
    $null = Install-Appli $InstallateurNeuf $PaquetEssai $e.Sid
    $avant = (Get-InfoTache).Dernier
    $dormeur = "$Racine\appli\Dormeur.exe"
    $de = Start-Process -FilePath $dormeur -Credential $e.Cred -LoadUserProfile -WorkingDirectory $env:SystemRoot -PassThru
    $db = Start-Process -FilePath $dormeur -Credential $b.Cred -LoadUserProfile -WorkingDirectory $env:SystemRoot -PassThru
    Start-Sleep -Seconds 2
    $s = Invoke-Client $e @{ Mode = 'appel'; Actions = @(@{ Nom = 'bonjour'; Action = 'aide.bonjour'; P = @{} }, @{ Nom = 'taches'; Action = 'lecture.taches'; P = @{} },
                                                    @{ Nom = 'fermer'; Action = 'processus.fermer'; P = @{ Chemin = $dormeur } }) }
    $bj = Get-Bonjour $s
    Test-Cas 'Nevermind reconnaît la tâche SYSTEM d''Élodie, la lance (dernier lancement changé), sans demande de Windows' {
        $s.Etat -eq 'systeme' -and (Get-InfoTache).Dernier -ne $avant -and -not @($s.RunAs).Count
    }
    Test-Cas 'l''Aide répond : en SYSTEM, avec les droits administrateur, compile dans aide-temp de son dossier' {
        $bj -and [string]$bj.Compte -eq 'S-1-5-18' -and $bj.Admin -eq $true -and [string]$bj.Temp -ieq "$NevermindC\aide-temp\"
    }
    Test-Cas 'lecture seule (lecture.taches) : la liste des tâches du PC, vue par l''Aide' { $s.R.PSObject.Properties['taches'] -and @($s.R.taches | Where-Object { $_.Nom -eq $NomTache }).Count }
    $aides = @(Get-AidesEnCours)
    foreach ($a in $aides) { Write-Host "         | Aide : $($a.Id) compte $($a.Sid) session $($a.Session)" -ForegroundColor DarkGray }
    Test-Cas 'une seule Aide, en SYSTEM, dans la session 0 (aucune fenêtre possible sur l''écran d''Élodie)' { $aides.Count -eq 1 -and $aides[0].Sid -eq 'S-1-5-18' -and $aides[0].Session -eq 0 }
    Test-Cas 'aide-temp : réservé au système et aux administrateurs (ni Élodie, ni les utilisateurs)' {
        $acl = Get-Acl -LiteralPath "$NevermindC\aide-temp"
        $qui = @($acl.Access | ForEach-Object { $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value } | Select-Object -Unique)
        Write-Host "         | aide-temp : $($qui -join ', ')" -ForegroundColor DarkGray
        $acl.AreAccessRulesProtected -and -not @($qui | Where-Object { $_ -notin 'S-1-5-18', 'S-1-5-32-544' }).Count
    }
    Start-Sleep -Seconds 1
    Test-Cas '« fermer » (processus.fermer) en SYSTEM : seul le programme d''Élodie est fermé, pas celui de Bruno (même session)' {
        [int]$s.R.fermer -eq 1 -and $de.HasExited -and -not $db.HasExited
    }
    try { $db.Kill() } catch {}
    # L'Aide s'arrête seule quand plus aucun Mindset.exe de Nevermind ne tourne pour Élodie
    Test-Cas 'l''Aide s''arrête seule une fois Nevermind fermé (moins d''une minute)' { Wait-Jusque { -not @(Get-AidesEnCours).Count } 60 }
}

# =====================================================================================================================
# 3. Bruno, un autre compte standard
# =====================================================================================================================
function Test-Cas3 {
    Write-Partie '3. Un autre compte standard : ni la tâche ni le canal d''Élodie'
    Reset-Machine
    $e = Get-Compte $NomElodie
    $b = Get-Compte 'Bruno Essai'
    $null = Install-Appli $InstallateurNeuf $PaquetEssai $e.Sid
    $avant = (Get-InfoTache).Dernier
    $s = Invoke-Client $b @{ Mode = 'lancer' }
    Test-Cas 'Bruno ne peut ni lire ni lancer la tâche d''Élodie' { $null -ne $s.R.lancer -and [int]$s.R.lancer -ne 0 -and [int]$s.R.lire -ne 0 -and (Get-InfoTache).Dernier -eq $avant }
    $s = Invoke-Client $b @{ Mode = 'appel'; SansDemande = $true; Actions = @(@{ Nom = 'bonjour'; Action = 'aide.bonjour'; P = @{} }) }
    Test-Cas 'le Mindset.exe de Bruno ne voit pas de tâche pour lui (état vide), ne la lance pas, n''obtient rien' { $s.Etat -eq '' -and -not (Get-Bonjour $s) -and (Get-InfoTache).Dernier -eq $avant -and -not @($s.RunAs).Count }
    # L'Aide d'Élodie tourne (Élodie a Nevermind ouvert) : Bruno essaie d'ouvrir son canal
    $garde = Invoke-Client $e @{ Mode = 'garder'; Secondes = 90 } -Fond
    $null = & $Schtasks /Run /TN "\Mindset\$NomTache" 2>&1
    $actions = Join-Path $NevermindC 'fichiers\modules\aide-actions.ps1'
    . ([scriptblock]::Create(([System.Management.Automation.Language.Parser]::ParseFile($actions, [ref]$null, [ref]$null)).Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Get-AideSuffixe' }, $true).Extent.Text))
    $canal = "Nevermind-Aide-$($e.Sid)-$(Get-AideSuffixe $NevermindC)"
    $ecoute = Wait-Jusque { @([IO.Directory]::GetFiles('\\.\pipe\') | Where-Object { $_ -like "*\$canal" }).Count } 60
    Test-Cas 'l''Aide d''Élodie écoute (en SYSTEM) sur son canal' { $ecoute -and @(Get-AidesEnCours | Where-Object { $_.Sid -eq 'S-1-5-18' }).Count -eq 1 }
    $s = Invoke-Client $b @{ Mode = 'canal'; Canal = $canal }
    Test-Cas 'Bruno ne peut pas ouvrir le canal d''Élodie (accès refusé)' { -not $s.R.PSObject.Properties['canal'] -and [string]$s.E.canal -match 'UnauthorizedAccess' }
    $s = Invoke-Client $e @{ Mode = 'canal'; Canal = $canal }
    Test-Cas 'contre-épreuve : Élodie, elle, ouvre son canal' { [string]$s.R.canal -eq 'ouvert' }
    try { $garde.Kill() } catch {}
}

# =====================================================================================================================
# 4. Mise à jour depuis Nevermind 1.10.0 : l'ancienne tâche remplacée
# =====================================================================================================================
function Test-Cas4 {
    Write-Partie '4. Mise à jour depuis Nevermind 1.10.0 (installé par Mindset 2.10.0)'
    Reset-Machine
    $e = Get-Compte $NomElodie
    $r = Install-Appli $Installateur210 $Paquet110 $e.Sid
    $t = Get-InfoTache
    Test-Cas 'Nevermind 1.10.0 installé par Mindset 2.10.0 : l''ancienne tâche, sous le compte d''Élodie, « droits les plus élevés », sans -Sid' {
        $r.ok -and $t -and (ConvertTo-Sid $t.Compte) -eq $e.Sid -and $t.RunLevel -eq 'HighestAvailable' -and -not $t.Arguments
    }
    $r = Install-Appli $InstallateurNeuf $PaquetEssai $e.Sid
    Test-Cas 'mise à jour par le nouveau Mindset : l''ancienne tâche est remplacée par la tâche SYSTEM pour Élodie' { $r.ok -and (Test-TacheSysteme $e.Sid) }
    $s = Invoke-Client $e @{ Mode = 'appel'; Actions = @(@{ Nom = 'bonjour'; Action = 'aide.bonjour'; P = @{} }) }
    Test-Cas 'ensuite, l''Aide répond en SYSTEM, sans demande de Windows' { $s.Etat -eq 'systeme' -and [string](Get-Bonjour $s).Compte -eq 'S-1-5-18' -and -not @($s.RunAs).Count }
    Stop-Aides

    # Réparée aussi par Mindset (ses tâches refaites : sa mise à jour, ou le remplacement de son ancienne tâche)
    $f = Join-Path $Sorties "mindset-$([guid]::NewGuid().ToString('N').Substring(0, 8)).json"
    & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $InstallateurNeuf -Action mindset -Source (Join-Path $MindsetDepot 'Mindset') -Version '9.9.1' -Sid $e.Sid -Resultat $f | Out-Host
    Test-Cas 'Mindset installé pour Élodie' { (Read-Json $f).ok }
    $r = Install-Appli $Installateur210 $Paquet110 $e.Sid
    Test-Cas 'Nevermind 1.10.0 réinstallé par Mindset 2.10.0 : l''ancienne tâche est revenue' { $t = Get-InfoTache; (ConvertTo-Sid $t.Compte) -eq $e.Sid }
    $f = Join-Path $Sorties "taches-$([guid]::NewGuid().ToString('N').Substring(0, 8)).json"
    & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $InstallateurNeuf -Action taches -Destination $MindsetC -Sid $e.Sid -Resultat $f | Out-Host
    Test-Cas 'Mindset refait ses tâches (Action taches) : la tâche d''aide d''Élodie passe en SYSTEM, vers le même programme' { (Read-Json $f).ok -and (Test-TacheSysteme $e.Sid) }
}

# =====================================================================================================================
# 5. Tâche ancienne, cassée ou désactivée : la demande de Windows, jamais l'ancienne tâche
# =====================================================================================================================
function Test-Cas5 {
    Write-Partie '5. Tâche ancienne, cassée ou désactivée : la demande de Windows'
    Reset-Machine
    $e = Get-Compte $NomElodie
    $null = Install-Appli $InstallateurNeuf $PaquetEssai $e.Sid
    $bonne = Get-InfoTache
    $exe = "$NevermindC\NevermindAide.exe"
    $client = @{ Mode = 'appel'; Accepter = $true; Actions = @(@{ Nom = 'bonjour'; Action = 'aide.bonjour'; P = @{} }) }
    $verifier = {
        param($S, [string]$Quoi)
        $bj = Get-Bonjour $S
        Test-Cas "$Quoi : demande de Windows pour NevermindAide.exe -Sid <Élodie>, acceptée, l'Aide répond (administrateur, pas SYSTEM)" {
            @($S.RunAs).Count -eq 1 -and $S.RunAs[0] -eq "RunAs|$exe|-Sid $($e.Sid)" -and $bj -and $bj.Admin -eq $true -and [string]$bj.Compte -eq $Moi
        }
    }

    # a. L'ancienne tâche (comme Mindset 2.10.0 la créait), à la place de la tâche SYSTEM
    $act = New-ScheduledTaskAction -Execute "`"$exe`"" -WorkingDirectory $NevermindC
    $set = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew
    Register-ScheduledTask -TaskPath '\Mindset\' -TaskName $NomTache -Action $act -Principal (New-ScheduledTaskPrincipal -UserId $e.Sid -LogonType Interactive -RunLevel Highest) -Settings $set -Force | Out-Null
    $avant = (Get-InfoTache).Dernier
    $s = Invoke-Client $e $client
    Test-Cas 'ancienne tâche : reconnue comme n''étant pas la bonne, jamais lancée' { $s.Etat -eq 'autre' -and (Get-InfoTache).Dernier -eq $avant }
    & $verifier $s 'ancienne tâche'
    Stop-Aides

    # b. Cassée : la tâche SYSTEM démarre, mais son programme n'existe pas (l'Aide ne répond jamais)
    Register-ScheduledTask -TaskPath '\Mindset\' -TaskName $NomTache -Xml $bonne.Xml -Force | Out-Null
    $xml = $bonne.Xml.Replace('NevermindAide.exe', 'NevermindAbsent.exe')
    Register-ScheduledTask -TaskPath '\Mindset\' -TaskName $NomTache -Xml $xml -Force | Out-Null
    $s = Invoke-Client $e $client
    Test-Cas 'tâche SYSTEM vers un programme absent : pas reconnue, pas lancée' { $s.Etat -eq 'autre' }
    & $verifier $s 'tâche vers un programme absent'
    Stop-Aides

    # c. La bonne tâche, mais qui ne répond pas (lancée, rien n'écoute) : 20 s, puis la demande de Windows
    Register-ScheduledTask -TaskPath '\Mindset\' -TaskName $NomTache -Xml $bonne.Xml -Force | Out-Null
    Rename-Item -LiteralPath "$NevermindC\fichiers\NevermindAide.ps1" -NewName 'NevermindAide.ps1.cache'
    $avant = (Get-InfoTache).Dernier
    $s = Invoke-Client $e ($client + @{ AttenteWindows = 10 })
    Rename-Item -LiteralPath "$NevermindC\fichiers\NevermindAide.ps1.cache" -NewName 'NevermindAide.ps1'
    Test-Cas 'tâche SYSTEM reconnue et lancée, mais muette (script de l''Aide absent) : 20 s, puis la demande de Windows pour NevermindAide.exe -Sid <Élodie>' {
        $s.Etat -eq 'systeme' -and (Get-InfoTache).Dernier -ne $avant -and $s.Duree -ge 20 -and @($s.RunAs).Count -eq 1 -and $s.RunAs[0] -eq "RunAs|$exe|-Sid $($e.Sid)"
    }
    Stop-Aides

    # d. Désactivée
    Disable-ScheduledTask -TaskPath '\Mindset\' -TaskName $NomTache | Out-Null
    $avant = (Get-InfoTache).Dernier
    $s = Invoke-Client $e $client
    Test-Cas 'tâche SYSTEM désactivée : pas lancée' { $s.Etat -eq 'autre' -and (Get-InfoTache).Dernier -eq $avant }
    & $verifier $s 'tâche désactivée'
    Stop-Aides

    # e. Demande de Windows refusée : un message simple ; une lecture en fond n'en fait jamais
    $s = Invoke-Client $e @{ Mode = 'appel'; Accepter = $false; Actions = @(@{ Nom = 'bonjour'; Action = 'aide.bonjour'; P = @{} }, @{ Nom = 'encore'; Action = 'aide.bonjour'; P = @{} }) }
    Test-Cas 'demande de Windows refusée : « clique Oui », et pas de deuxième demande juste après' { [string]$s.E.bonjour -match 'Oui' -and @($s.RunAs).Count -eq 1 -and $s.E.encore }
    $s = Invoke-Client $e @{ Mode = 'appel'; SansDemande = $true; Accepter = $true; Actions = @(@{ Nom = 'bonjour'; Action = 'aide.bonjour'; P = @{} }) }
    Test-Cas 'lecture en fond (-SansDemande) : aucune demande de Windows' { -not @($s.RunAs).Count -and -not (Get-Bonjour $s) }
}

# =====================================================================================================================
# 6. Désinstallation : plus aucune tâche
# =====================================================================================================================
function Close-Fenetres([int]$Id) {
    # Les messages de fin du désinstalleur (personne ici pour cliquer « OK »)
    foreach ($p in @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object { $_.ProcessId -eq $Id -or $_.ParentProcessId -eq $Id })) {
        try { $x = [Diagnostics.Process]::GetProcessById([int]$p.ProcessId); if ($x.MainWindowHandle -ne [IntPtr]::Zero) { [void]$x.CloseMainWindow() } } catch {}
    }
}
function Test-Cas6 {
    Write-Partie '6. Désinstallation'
    Reset-Machine
    $e = Get-Compte $NomElodie
    $r = Install-Appli $InstallateurNeuf $PaquetVrai $e.Sid
    Test-Cas 'le vrai Nevermind installé pour Élodie, tâche SYSTEM' { $r.ok -and (Test-TacheSysteme $e.Sid) }
    $null = & $Schtasks /Run /TN "\Mindset\$NomTache" 2>&1
    Test-Cas 'l''Aide tourne en SYSTEM' { Wait-Jusque { @(Get-AidesEnCours | Where-Object { $_.Sid -eq 'S-1-5-18' }).Count } 30 }
    # Le désinstalleur de Nevermind, comme Mindset le lance (choix des données déjà fait), en administrateur
    $p = Start-Process -FilePath "$NevermindC\Désinstaller Nevermind.exe" -ArgumentList '-SupprimerDonnees' -WorkingDirectory $NevermindC -PassThru
    $fini = Wait-Jusque { Close-Fenetres $p.Id; -not (Get-InfoTache) -and -not @(Get-AidesEnCours).Count -and -not (Test-Path -LiteralPath "$NevermindC\NevermindAide.exe") } 180
    Write-Host "         | désinstalleur : $(if ($p.HasExited) { "fini (code $($p.ExitCode))" } else { 'encore ouvert' }) ; tâche $([bool](Get-InfoTache)) ; Aides $(@(Get-AidesEnCours).Count) ; NevermindAide.exe $(Test-Path -LiteralPath "$NevermindC\NevermindAide.exe") ; aide-temp $(Test-Path -LiteralPath "$NevermindC\aide-temp")" -ForegroundColor DarkGray
    Test-Cas 'désinstalleur de Nevermind : tâche de l''Aide retirée, Aide SYSTEM arrêtée, NevermindAide.exe et aide-temp supprimés' { $fini -and (Wait-Jusque { -not (Test-Path -LiteralPath "$NevermindC\aide-temp") } 30) }
    try { if (-not $p.HasExited) { $p.Kill() } } catch {}

    # Désinstallation de Mindset (son installateur, Action supprimer) : toutes les tâches \Mindset\, l'Aide comprise
    Reset-Machine
    $f = Join-Path $Sorties "mindset-$([guid]::NewGuid().ToString('N').Substring(0, 8)).json"
    & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $InstallateurNeuf -Action mindset -Source (Join-Path $MindsetDepot 'Mindset') -Version '9.9.1' -Sid $e.Sid -Resultat $f | Out-Host
    $null = Install-Appli $InstallateurNeuf $PaquetEssai $e.Sid
    Test-Cas 'avant : trois tâches \Mindset\ (démarrage, installation, Nevermind (aide) en SYSTEM)' { @(Get-ScheduledTask -TaskPath '\Mindset\').Count -eq 3 -and (Test-TacheSysteme $e.Sid) }
    $f = Join-Path $Sorties "supprimer-$([guid]::NewGuid().ToString('N').Substring(0, 8)).json"
    & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $MindsetC 'fichiers\installer-admin.ps1') -Action supprimer -Destination $MindsetC -Resultat $f | Out-Host
    Test-Cas 'désinstallation de Mindset : plus aucune tâche \Mindset\ (l''Aide comprise)' {
        $dossier = $true; try { $s = New-Object -ComObject Schedule.Service; $s.Connect(); $null = $s.GetFolder('\Mindset') } catch { $dossier = $false }
        -not @(Get-ScheduledTask -TaskPath '\Mindset\' -ErrorAction SilentlyContinue).Count -and -not $dossier
    }
}

# =====================================================================================================================
# 7. Environnement : la tâche SYSTEM ne voit rien d'Élodie
# =====================================================================================================================
function Test-Cas7 {
    Write-Partie '7. Environnement : l''Aide en SYSTEM ne voit pas la variable témoin d''Élodie'
    Reset-Machine
    $e = Get-Compte $NomElodie
    $null = Install-Appli $InstallateurNeuf $PaquetEssai $e.Sid
    $marque = Join-Path $Racine 'modules-temoin-NEVERMIND'
    New-Item -ItemType Directory -Force -Path $marque | Out-Null
    # Les réglages d'Élodie (HKCU\Environment) : une variable témoin inoffensive et son PSModulePath (aucune variable COR_*)
    $code = "[Environment]::SetEnvironmentVariable('NEVERMIND_TEMOIN_ENV', '1', 'User'); [Environment]::SetEnvironmentVariable('PSModulePath', '$marque', 'User')"
    $p = Start-Process -FilePath $PowerShellExe -ArgumentList '-NoProfile', '-EncodedCommand', ([Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($code))) -Credential $e.Cred -LoadUserProfile -WorkingDirectory $Racine -PassThru -WindowStyle Hidden
    [void]$p.WaitForExit(60000)
    # Une copie de Nevermind dans Program Files (modifiable par les seuls administrateurs) dont l'Aide écrit, au tout
    # début de son script, ce qu'elle a reçu : lancée par la tâche SYSTEM (à l'identique), puis sous le compte d'Élodie
    $copie = Join-Path $env:ProgramFiles 'MindsetEssaiEnv\Nevermind'
    New-Item -ItemType Directory -Force -Path (Split-Path $copie -Parent) | Out-Null
    Copy-Item -LiteralPath $NevermindC -Destination $copie -Recurse -Force
    $f = Join-Path $copie 'fichiers\NevermindAide.ps1'
    $texte = [IO.File]::ReadAllText($f, [Text.Encoding]::UTF8)
    $ecrire = "& { `$l = [Collections.Generic.List[string]]::new(); `$l.Add('COMPTE=' + [Security.Principal.WindowsIdentity]::GetCurrent().User.Value); `$v = [Environment]::GetEnvironmentVariables(); foreach (`$k in `$v.Keys) { `$l.Add('VAR ' + `$k + '=' + `$v[`$k]) }; [IO.File]::WriteAllLines('$Sorties\env-' + [Security.Principal.WindowsIdentity]::GetCurrent().User.Value + '.txt', `$l) }"
    $i = $texte.IndexOf('param([int]$Parent, [string]$Sid)')
    if ($i -lt 0) { throw 'ligne param introuvable dans NevermindAide.ps1' }
    $j = $texte.IndexOf("`n", $i) + 1
    [IO.File]::WriteAllText($f, $texte.Substring(0, $j) + $ecrire + "`r`n" + $texte.Substring($j), (New-Object Text.UTF8Encoding $true))
    $t = Get-InfoTache
    Register-ScheduledTask -TaskPath '\MindsetEssai\' -TaskName 'Environnement' -Xml $t.Xml.Replace($NevermindC, $copie) -Force | Out-Null
    Start-ScheduledTask -TaskPath '\MindsetEssai\' -TaskName 'Environnement'
    $pe = Start-Process -FilePath "$copie\NevermindAide.exe" -ArgumentList '-Sid', $e.Sid -Credential $e.Cred -LoadUserProfile -WorkingDirectory $env:SystemRoot -PassThru
    $null = Wait-Jusque { (Test-Path -LiteralPath "$Sorties\env-S-1-5-18.txt") -and (Test-Path -LiteralPath "$Sorties\env-$($e.Sid).txt") } 60
    try { $pe.Kill() } catch {}
    Stop-Aides
    $lire = { param($f) $x = @{ Vars = @{} }; foreach ($l in @(Get-Content -LiteralPath $f -Encoding UTF8 -ErrorAction SilentlyContinue)) { if ($l -match '^COMPTE=(.*)') { $x.Compte = $Matches[1] } elseif ($l -match '^VAR ([^=]+)=(.*)') { $x.Vars[$Matches[1]] = $Matches[2] } }; $x }
    $sy = & $lire "$Sorties\env-S-1-5-18.txt"; $el = & $lire "$Sorties\env-$($e.Sid).txt"
    foreach ($x in @(@('SYSTEM', $sy), @('Élodie', $el))) { Write-Host "         | $($x[0]) : compte $($x[1].Compte), NEVERMIND_TEMOIN_ENV=$($x[1].Vars['NEVERMIND_TEMOIN_ENV']), TEMP=$($x[1].Vars['TEMP']), PSModulePath=$($x[1].Vars['PSModulePath'])" -ForegroundColor DarkGray }
    Test-Cas 'contre-épreuve : l''Aide lancée sous le compte d''Élodie voit sa variable témoin' { $el.Compte -eq $e.Sid -and $el.Vars['NEVERMIND_TEMOIN_ENV'] -eq '1' }
    Test-Cas 'l''Aide lancée par la tâche SYSTEM ne voit ni la variable témoin, ni rien du dossier d''Élodie, ni aucune variable COR_* ou COMPlus_*' {
        $sy.Compte -eq 'S-1-5-18' -and -not $sy.Vars.ContainsKey('NEVERMIND_TEMOIN_ENV') -and -not @($sy.Vars.Values | Where-Object { $_ -like "*$NomElodie*" -or $_ -like "*$marque*" }).Count -and
            -not @($sy.Vars.Keys | Where-Object { $_ -match '^(COR_|COMPlus_|CORECLR_)' }).Count
    }
    Remove-Item -LiteralPath (Split-Path $copie -Parent) -Recurse -Force -ErrorAction SilentlyContinue
}

# =====================================================================================================================
$titres = @{ 0 = 'Préparation'; 1 = 'Installation : tâche SYSTEM pour Élodie'; 2 = 'Élodie obtient des actions en lecture seule'; 3 = 'Un autre compte refusé'
    4 = 'Mise à jour depuis Nevermind 1.10.0'; 5 = 'Repli par la demande de Windows'; 6 = 'Désinstallation'; 7 = 'Environnement d''Élodie' }
if (-not $titres.ContainsKey($Cas)) { Write-Host 'Indique -Cas 0 (préparation) ou -Cas 1 à 7.' -ForegroundColor Yellow; exit 2 }
try {
    switch ($Cas) {
        0 { if ($env:GITHUB_STEP_SUMMARY) { [IO.File]::AppendAllText($env:GITHUB_STEP_SUMMARY, "| Cas | Quoi | Résultat |`n|---|---|---|`n", (New-Object Text.UTF8Encoding $false)) }; Initialize-Essai }
        1 { Test-Cas1 }
        2 { Test-Cas2 }
        3 { Test-Cas3 }
        4 { Test-Cas4 }
        5 { Test-Cas5 }
        6 { Test-Cas6 }
        7 { Test-Cas7 }
    }
} catch {
    $script:Echoues.Add("erreur : $($_.Exception.Message)")
    Write-Host "ERREUR : $($_.Exception.Message) $($_.ScriptStackTrace)" -ForegroundColor Red
    Show-Diagnostic
}

$total = $script:Reussis + $script:Echoues.Count
$etat = if ($script:Echoues.Count) { "échoué ($($script:Echoues.Count) sur $total) : $($script:Echoues -join ' ; ')" } elseif ($script:Notes.Count) { "adapté ($($script:Reussis) sur $total) : $($script:Notes -join ' ; ')" } else { "réussi ($($script:Reussis) sur $total)" }
if ($env:GITHUB_STEP_SUMMARY) { [IO.File]::AppendAllText($env:GITHUB_STEP_SUMMARY, "| $Cas | $($titres[$Cas]) | $etat |`n", (New-Object Text.UTF8Encoding $false)) }
Write-Host ''
if ($script:Echoues.Count) { Write-Host "Cas $Cas : $etat" -ForegroundColor Red; exit 1 }
Write-Host "Cas $Cas : $etat" -ForegroundColor Green
exit 0
