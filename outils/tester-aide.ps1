# Tests de l'Aide administrateur (OptiGame\fichiers\modules\aide-actions.ps1), sans droits administrateur et sans rien
# modifier : chaque demande passe par Invoke-AideAction ... -Verifier, qui contrôle la demande et rend 'ok' sans agir.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-aide.ps1
#
# Ce que le script contrôle :
#   1. avant tout appel, que chaque demande de l'Aide s'arrête bien en mode vérification avant d'agir ;
#   2. que les demandes que Nevermind fait vraiment sont acceptées ;
#   3. que les demandes dangereuses ou mal formées sont refusées ;
#   4. que chaque demande qui prend des paramètres a au moins un cas refusé dans la liste (sinon le test échoue :
#      une nouvelle demande ajoutée à l'Aide doit arriver avec ses refus).
# Certaines vérifications lisent le PC (cartes réseau, processus, existence d'un fichier) : seulement en lecture.
# Code de sortie : 0 si tout passe, 1 sinon.

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$actions = Join-Path $racine 'OptiGame\fichiers\modules\aide-actions.ps1'
$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0

# ---------------------------------------------------------------------------
# Sécurité du test : rien ne doit pouvoir modifier le PC, même si une demande oubliait le mode vérification.
# Ces fonctions remplacent les commandes de Windows qui agissent (une fonction passe avant une commande du même nom).
# ---------------------------------------------------------------------------
foreach ($c in 'Set-Service', 'Start-Service', 'Stop-Service', 'New-NetFirewallRule', 'Set-NetFirewallRule', 'Remove-NetFirewallRule',
               'Set-NetFirewallProfile', 'Set-DnsClientServerAddress', 'Clear-DnsClientCache', 'Checkpoint-Computer', 'Remove-MpPreference',
               'Set-SmbServerConfiguration', 'Disable-WindowsOptionalFeature', 'Stop-Process', 'Remove-Item', 'Set-ItemProperty', 'New-Item') {
    Set-Item -Path "function:global:$c" -Value ([scriptblock]::Create("throw 'Test : la commande $c est interdite pendant le test de l''Aide.'"))
}

# ---------------------------------------------------------------------------
# 1. Lecture du code de l'Aide : la liste des demandes, et pour chacune si elle lit des paramètres ($P) et si elle
#    s'arrête en mode vérification ($Verifier) avant d'agir.
# ---------------------------------------------------------------------------
$tokens = $null; $erreurs = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($actions, [ref]$tokens, [ref]$erreurs)
if ($erreurs) { Write-Host "aide-actions.ps1 n'est pas lisible : $($erreurs[0].Message)" -ForegroundColor Red; exit 1 }
$fonction = $ast.Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Invoke-AideAction' }, $true)
$blocSwitch = $fonction.Body.Find({ param($n) $n -is [System.Management.Automation.Language.SwitchStatementAst] }, $true)
$demandes = [ordered]@{}
foreach ($cl in $blocSwitch.Clauses) {
    $nom = $cl.Item1.Value
    $utiliseP = [bool]$cl.Item2.Find({ param($n) $n -is [System.Management.Automation.Language.VariableExpressionAst] -and $n.VariablePath.UserPath -eq 'P' }, $true)
    $verifie = [bool]$cl.Item2.Find({ param($n) $n -is [System.Management.Automation.Language.VariableExpressionAst] -and $n.VariablePath.UserPath -eq 'Verifier' }, $true)
    $demandes[$nom] = @{ Parametres = $utiliseP; Verifie = $verifie }
}
if ($demandes.Count -lt 10) { Write-Host "Liste des demandes de l'Aide introuvable dans aide-actions.ps1 ($($demandes.Count) trouvées)." -ForegroundColor Red; exit 1 }

# Demandes qui ne passent pas par le mode vérification : seulement des lectures sans effet (justifié ici, une par une)
$SansModeVerification = @{
    'aide.bonjour' = 'rend seulement la version de l''Aide, si elle est administrateur et son numéro de processus'
}
foreach ($n in $demandes.Keys) {
    if (-not $demandes[$n].Verifie -and -not $SansModeVerification.ContainsKey($n)) {
        $echecs.Add("Sécurité : la demande « $n » ne s'arrête pas en mode vérification (-Verifier) : le test ne l'appelle pas.")
    }
}

# Demandes sans paramètre : rien à refuser, elles sont dispensées de cas refusé (la liste est vérifiée plus bas)
$SansParametre = @(
    'aide.bonjour',             # état de l'Aide
    'origines.oublier',         # efface les valeurs d'origine que l'Aide a gardées elle-même, rien d'autre
    'pare-feu.debloquer-tout',  # retire seulement les règles créées par Nevermind
    'pare-feu.activer',         # rallume le pare-feu : renforce la sécurité
    'lecture.pc', 'lecture.securite', 'lecture.taches',   # lectures seules
    'fps.lire', 'fps.arreter',  # mesure des FPS déjà lancée : lire, arrêter
    'trafic.tcp',               # lecture des compteurs réseau
    'flux.demarrer', 'flux.veiller', 'flux.lire', 'flux.arreter'   # trace réseau de Windows : lecture seule
)
# Demandes avec paramètres mais sans rien à refuser (justifié ici, une par une)
$DispenseesDeRefus = @{
    'smb1.couper' = 'son seul paramètre (Client) est oui ou non, et la demande ne fait que couper SMB1 (renforce la sécurité)'
}

# ---------------------------------------------------------------------------
# 2. Chargement de l'Aide (seulement des définitions : rien n'est lancé ni modifié au chargement)
# ---------------------------------------------------------------------------
. $actions
if (-not $AideVersion -or -not $AideDossierModules -or -not (Test-Path -LiteralPath (Join-Path $AideDossierModules 'natif.cs'))) {
    Write-Host 'aide-actions.ps1 chargé, mais $AideVersion ou $AideDossierModules ne sont pas bons.' -ForegroundColor Red; exit 1
}

# Les valeurs que Nevermind envoie vraiment (reprises de ses modules, voir outils\test-app.ps1)
$IfeoPath      = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options'
$ApprovedRoot  = 'Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved'
$DoPath        = 'Registry::HKEY_USERS\S-1-5-20\Software\Microsoft\Windows\CurrentVersion\DeliveryOptimization\Settings'
$DiagKey       = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection'
$DiagPolicyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection'
$DnsChoices    = @(@(), @('1.1.1.1', '1.0.0.1'), @('8.8.8.8', '8.8.4.4'), @('9.9.9.9', '149.112.112.112'))
$notepad = @("$env:windir\notepad.exe", "$env:windir\System32\notepad.exe", "$env:windir\System32\cmd.exe") | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
$carte = @(Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' })[0]
if (-not $carte) { $carte = @(Get-NetAdapter -ErrorAction SilentlyContinue)[0] }
function Test-Fichier([string]$Chemin) { Test-Path -LiteralPath $Chemin -PathType Leaf }

# ---------------------------------------------------------------------------
# 3. Demandes acceptées : toutes celles que Nevermind fait vraiment
# ---------------------------------------------------------------------------
$acceptees = @(
    @('aide.bonjour', @{}),
    @('reg.ecrire', @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers'; Nom = 'HwSchMode'; Valeur = 2; Type = 'DWord' }),
    @('reg.effacer', @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers'; Nom = 'HwSchMode' }),
    @('reg.ecrire', @{ Cle = $DoPath; Nom = 'DownloadMode'; Valeur = 0; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization'; Nom = 'DOPercentageMaxBackgroundBandwidth'; Valeur = 10; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization'; Nom = 'DODownloadMode'; Valeur = 0; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = $DiagKey; Nom = 'AllowTelemetry'; Valeur = 1; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = $DiagPolicyKey; Nom = 'AllowTelemetry'; Valeur = 1; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore'; Nom = 'SystemRestorePointCreationFrequency'; Valeur = 0; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server'; Nom = 'fDenyTSConnections'; Valeur = 1; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'; Nom = 'EnableLUA'; Valeur = 1; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = "$IfeoPath\RocketLeague.exe\PerfOptions"; Nom = 'CpuPriorityClass'; Valeur = 3; Type = 'DWord' }),
    @('reg.effacer', @{ Cle = "$IfeoPath\RocketLeague.exe\PerfOptions"; Nom = 'CpuPriorityClass' }),
    @('reg.ecrire', @{ Cle = "HKLM:\$ApprovedRoot\Run"; Nom = 'SecurityHealth'; Valeur = @(3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0); Type = 'Binary' }),
    @('reg.ecrire', @{ Cle = "HKLM:\$ApprovedRoot\StartupFolder"; Nom = 'Truc.lnk'; Valeur = @(2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0); Type = 'Binary' }),
    @('service.regler', @{ Nom = 'DiagTrack'; Demarrage = 'Disabled'; Arreter = $true }),
    @('service.regler', @{ Nom = 'DiagTrack'; Demarrage = 'Automatic'; Lancer = $true }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque Bloc-notes'; Programme = $notepad; Services = @() }),
    @('pare-feu.debloquer', @{ Nom = 'Nevermind : bloque Bloc-notes' }),
    @('pare-feu.debloquer', @{ Nom = 'OptiGame : bloque Bloc-notes (ancien nom)' }),
    @('pare-feu.debloquer-tout', @{}),
    @('pare-feu.activer', @{}),
    @('origines.oublier', @{}),
    @('restauration.creer', @{ Description = 'Nevermind' }),
    @('restauration.creer', @{ Description = 'Nevermind (manuel)' }),
    @('antivirus.retirer-exclusions', @{ Elements = @('C:\Jeux\truc.exe') }),
    @('wifi.manuel', @{ Profils = @('Freebox-Invite', 'Gare SNCF') }),
    @('smb1.couper', @{ Client = $true }),
    @('smb1.couper', @{ Client = $false }),
    @('nettoyage.lister', @{ Id = 'wintemp'; Top = 300 }),
    @('nettoyage.lancer', @{ Id = 'dump' }),
    @('processus.fermer', @{ Nom = 'chrome' }),
    @('lecture.pc', @{}),
    @('lecture.securite', @{}),
    @('lecture.taches', @{}),
    @('lecture.chemins', @{ Pids = @('4', '1234', '56789') }),
    @('fps.demarrer', @{ Pid = "$PID" }),
    @('fps.lire', @{}),
    @('fps.arreter', @{}),
    @('fps.veiller', @{ Noms = @('bf6.exe') }),
    @('fps.veiller', @{ Noms = @('bf6.exe', 'Cyberpunk2077.exe', 'cod (2).exe') }),
    @('flux.veiller', @{}),
    @('trafic.tcp', @{}),
    @('flux.demarrer', @{}),
    @('flux.lire', @{}),
    @('flux.arreter', @{})
)
# Chaque catégorie du nettoyage
foreach ($id in 'wintemp', 'wu', 'do', 'wer', 'dump') { $acceptees += , @('nettoyage.lancer', @{ Id = $id }); $acceptees += , @('nettoyage.lister', @{ Id = $id }) }
# DNS : sur une vraie carte réseau du PC (lue seulement)
if ($carte) {
    foreach ($choix in $DnsChoices) { $acceptees += , @('dns.regler', @{ Index = [int]$carte.ifIndex; Serveurs = @($choix) }) }
    $acceptees += , @('dns.regler', @{ Index = [int]$carte.ifIndex; Serveurs = @('192.168.1.254') })
}

# ---------------------------------------------------------------------------
# 4. Demandes refusées. Le message doit être un vrai refus de l'Aide (pas une erreur quelconque).
# ---------------------------------------------------------------------------
$refusees = @(
    # Registre : hors liste, valeur interdite, chemin détourné
    @('reg.ecrire', @{ Cle = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run'; Nom = 'Pirate'; Valeur = 'C:\x.exe'; Type = 'String' }),
    @('reg.ecrire', @{ Cle = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'; Nom = 'EnableLUA'; Valeur = 0; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server'; Nom = 'fDenyTSConnections'; Valeur = 0; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = "$IfeoPath\RocketLeague.exe"; Nom = 'Debugger'; Valeur = 'cmd.exe'; Type = 'String' }),
    @('reg.ecrire', @{ Cle = "$IfeoPath\RocketLeague.exe\PerfOptions"; Nom = 'Debugger'; Valeur = 'cmd.exe'; Type = 'String' }),
    @('reg.ecrire', @{ Cle = "$IfeoPath\lsass.exe\PerfOptions"; Nom = 'CpuPriorityClass'; Valeur = 3; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = "$IfeoPath\MsMpEng.exe\PerfOptions"; Nom = 'CpuPriorityClass'; Valeur = 1; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = "$IfeoPath\RocketLeague.exe\PerfOptions"; Nom = 'CpuPriorityClass'; Valeur = 4; Type = 'DWord' }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$env:windir\System32\MpSigStub.exe" }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$env:windir\System32\smartscreen.exe" }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$env:windir\System32\MRT.exe" }),
    @('reg.ecrire', @{ Cle = "$IfeoPath\..\Run\PerfOptions"; Nom = 'CpuPriorityClass'; Valeur = 3; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = "$IfeoPath\..\..\Run\x.exe\PerfOptions"; Nom = 'CpuPriorityClass'; Valeur = 3; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers\..\Lsa'; Nom = 'HwSchMode'; Valeur = 2; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers'; Nom = 'HwSchMode'; Valeur = 9; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers'; Nom = 'HwSchMode'; Valeur = 2; Type = 'String' }),
    @('reg.ecrire', @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers'; Nom = 'HwSchMode'; Valeur = '2; calc'; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'; Nom = 'Pirate'; Valeur = 1; Type = 'DWord' }),
    @('reg.ecrire', @{ Cle = "HKLM:\$ApprovedRoot\Run"; Nom = 'SecurityHealth'; Valeur = @(3, 0); Type = 'Binary' }),
    @('reg.ecrire', @{ Cle = "HKLM:\$ApprovedRoot\Run"; Nom = 'SecurityHealth'; Valeur = @(9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0); Type = 'Binary' }),
    @('reg.effacer', @{ Cle = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'; Nom = 'EnableLUA' }),
    @('reg.effacer', @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server'; Nom = 'fDenyTSConnections' }),
    @('reg.effacer', @{ Cle = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run'; Nom = 'SecurityHealth' }),
    @('reg.effacer', @{ Cle = 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers\..\..\Services\WinDefend'; Nom = 'Start' }),
    # Services : hors liste (dont l'antivirus), démarrage inconnu
    @('service.regler', @{ Nom = 'WinDefend'; Demarrage = 'Disabled' }),
    @('service.regler', @{ Nom = 'mpssvc'; Demarrage = 'Disabled'; Arreter = $true }),
    @('service.regler', @{ Nom = 'Spooler'; Demarrage = 'Disabled' }),
    @('service.regler', @{ Nom = 'DiagTrack'; Demarrage = 'Boot' }),
    # DNS : carte inexistante, trop de serveurs, serveur inconnu
    @('dns.regler', @{ Index = 0; Serveurs = @('1.1.1.1', '1.0.0.1') }),
    @('dns.regler', @{ Index = 1; Serveurs = @('1.1.1.1', '1.0.0.1', '8.8.8.8', '8.8.4.4', '9.9.9.9') }),
    # Pare-feu : nom de règle étranger, service ou programme indispensable
    @('pare-feu.bloquer', @{ Nom = 'Règle quelconque'; Programme = $notepad }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque *'; Programme = $notepad }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = 'notepad.exe' }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$env:windir\System32\..\notepad.exe" }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$env:windir\System32\drivers\etc\hosts" }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$env:windir\System32\svchost.exe" }),
    @('pare-feu.bloquer', @{ Nom = 'OptiGame : bloque X'; Programme = "$env:windir\System32\svchost.exe"; Services = @('wuauserv') }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$env:windir\System32\svchost.exe"; Services = @('DiagTrack', 'WinDefend') }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$env:windir\System32\svchost.exe"; Services = @('Dnscache; calc') }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$env:windir\explorer.exe" }),
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$env:windir\System32\lsass.exe" }),
    @('pare-feu.debloquer', @{ Nom = 'Core Networking - DNS (UDP-Out)' }),
    @('pare-feu.debloquer', @{ Nom = 'Windows Defender Firewall' }),
    # Point de restauration : description libre
    @('restauration.creer', @{ Description = 'autre chose' }),
    # Antivirus : liste vide ou mal formée
    @('antivirus.retirer-exclusions', @{ Elements = @() }),
    @('antivirus.retirer-exclusions', @{ Elements = @("C:\x`nC:\y") }),
    @('antivirus.retirer-exclusions', @{ Elements = @(1..201 | ForEach-Object { "C:\x$_" }) }),
    # Wi-Fi : nom de profil invalide
    @('wifi.manuel', @{ Profils = @('a" b') }),
    @('wifi.manuel', @{ Profils = @() }),
    @('wifi.manuel', @{ Profils = @('x' * 65) }),
    @('wifi.manuel', @{ Profils = @("Box`nname=*") }),
    @('wifi.manuel', @{ Profils = @('Box\') }),
    # Nettoyage : jamais un chemin envoyé par Nevermind
    @('nettoyage.lancer', @{ Id = 'C:\Users' }),
    @('nettoyage.lancer', @{ Id = "$env:USERPROFILE\Documents" }),
    @('nettoyage.lancer', @{ Id = '..\..' }),
    @('nettoyage.lister', @{ Id = 'C:\' }),
    @('nettoyage.lister', @{ Id = 'system32' }),
    # Processus : Windows et l'antivirus jamais fermés
    @('processus.fermer', @{ Nom = 'MsMpEng' }),
    @('processus.fermer', @{ Nom = 'MsMpEng  ' }),
    @('processus.fermer', @{ Nom = 'MsSense' }),
    @('processus.fermer', @{ Nom = 'smartscreen' }),
    @('processus.fermer', @{ Nom = 'SecurityHealthHost' }),
    @('processus.fermer', @{ Nom = 'MsMpEng.exe' }),
    @('processus.fermer', @{ Nom = 'lsass' }),
    @('processus.fermer', @{ Nom = 'Nevermind' }),
    @('processus.fermer', @{ Nom = 'chrome*' }),
    @('processus.fermer', @{ Nom = 'a;b' }),
    @('processus.fermer', @{ Chemin = "$env:windir\explorer.exe" }),
    @('processus.fermer', @{ Chemin = "$env:windir\System32\cmd.exe" }),
    @('processus.fermer', @{ Chemin = 'C:\Jeux\..\Windows\explorer.exe' }),
    # Lectures et mesures : paramètres mal formés
    @('lecture.chemins', @{ Pids = @('12; Remove-Item C:\') }),
    @('lecture.chemins', @{ Pids = @() }),
    @('lecture.chemins', @{ Pids = @(1..501 | ForEach-Object { "$_" }) }),
    @('fps.demarrer', @{ Pid = '4' }),
    @('fps.demarrer', @{ Pid = '0' }),
    @('fps.demarrer', @{ Pid = '1234; calc' }),
    @('fps.veiller', @{ Noms = @() }),
    @('fps.veiller', @{ Noms = @('C:\Jeux\bf6.exe') }),
    @('fps.veiller', @{ Noms = @('bf6.exe" --output_file C:\x.csv "a.exe') }),
    @('fps.veiller', @{ Noms = @('notepad') }),
    @('fps.veiller', @{ Noms = @(1..11 | ForEach-Object { "jeu$_.exe" }) }),
    # Demande inconnue
    @('inconnue', @{}),
    @('reg.lire', @{ Cle = 'HKLM:\SAM'; Nom = 'x' })
)
# Selon ce qui existe sur ce PC (lu seulement)
if ($carte) { $refusees += , @('dns.regler', @{ Index = [int]$carte.ifIndex; Serveurs = @('6.6.6.6') }) }
if ($carte) { $refusees += , @('dns.regler', @{ Index = [int]$carte.ifIndex; Serveurs = @('1.1.1.1', '6.6.6.6') }) }
foreach ($p in "$env:ProgramFiles\Windows Defender\MpCmdRun.exe", "$env:ProgramFiles\Windows Defender\MsMpEng.exe", "$env:ProgramFiles\Windows Defender\ConfigSecurityPolicy.exe") {
    if (Test-Fichier $p) {
        $refusees += , @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = $p })
        $refusees += , @('processus.fermer', @{ Chemin = $p })
    }
}
# Le même dossier de l'antivirus écrit en nom court (C:\PROGRA~1\WINDOW~1), s'il existe sur ce disque
try {
    $court = (New-Object -ComObject Scripting.FileSystemObject).GetFolder("$env:ProgramFiles\Windows Defender").ShortPath
    if ($court -and $court -ne "$env:ProgramFiles\Windows Defender" -and (Test-Fichier "$court\MpCmdRun.exe")) {
        $refusees += , @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$court\MpCmdRun.exe" })
        $refusees += , @('processus.fermer', @{ Chemin = "$court\MpCmdRun.exe" })
    }
} catch {}

# ---------------------------------------------------------------------------
# 5. Contrôle de la couverture : chaque demande qui prend des paramètres a au moins un cas refusé
# ---------------------------------------------------------------------------
$avecRefus = @{}; foreach ($a in $refusees) { $avecRefus[$a[0]] = $true }
$avecAccord = @{}; foreach ($a in $acceptees) { $avecAccord[$a[0]] = $true }
foreach ($n in $demandes.Keys) {
    $d = $demandes[$n]
    if ($n -in $SansParametre) {
        if ($d.Parametres) { $echecs.Add("Couverture : « $n » est dans la liste des demandes sans paramètre, mais elle en lit un : ajoute un cas refusé et retire la de la liste.") }
        continue
    }
    if (-not $d.Parametres) { $echecs.Add("Couverture : « $n » ne prend pas de paramètre : ajoute la à la liste des demandes sans paramètre, avec la raison.") ; continue }
    if ($DispenseesDeRefus.ContainsKey($n)) { continue }
    if (-not $avecRefus.ContainsKey($n)) { $echecs.Add("Couverture : la demande « $n » prend des paramètres mais n'a aucun cas refusé dans ce test.") }
}
foreach ($n in $demandes.Keys) {
    if (-not $avecAccord.ContainsKey($n) -and -not ($n -eq 'dns.regler' -and -not $carte)) { $echecs.Add("Couverture : la demande « $n » n'a aucun cas accepté dans ce test.") }
}
foreach ($n in @($SansParametre) + @($DispenseesDeRefus.Keys) + @($SansModeVerification.Keys)) {
    if (-not $demandes.Contains($n)) { $echecs.Add("Couverture : « $n » est citée dans ce test mais n'existe plus dans l'Aide.") }
}

# ---------------------------------------------------------------------------
# 6. Les essais (toujours avec -Verifier, et jamais une demande qui ne s'arrête pas en mode vérification)
# ---------------------------------------------------------------------------
$refusAttendu = 'Demande refusée|L''Aide garde ce réglage|L''Aide ne connaît pas'
function Test-Appelable([string]$Action) {
    -not $demandes.Contains($Action) -or $demandes[$Action].Verifie -or $SansModeVerification.ContainsKey($Action)
}
foreach ($a in $acceptees) {
    $nbTests++
    if (-not (Test-Appelable $a[0])) { $echecs.Add("Non essayée (pas de mode vérification) : $($a[0])"); continue }
    try { [void](Invoke-AideAction $a[0] $a[1] -Verifier) }
    catch { $echecs.Add("Refusée à tort : $($a[0]) $(ConvertTo-Json $a[1] -Compress) ($($_.Exception.Message))") }
}
foreach ($a in $refusees) {
    $nbTests++
    if (-not (Test-Appelable $a[0])) { $echecs.Add("Non essayée (pas de mode vérification) : $($a[0])"); continue }
    $message = $null
    try { [void](Invoke-AideAction $a[0] $a[1] -Verifier) } catch { $message = $_.Exception.Message }
    if ($null -eq $message) { $echecs.Add("Acceptée à tort : $($a[0]) $(ConvertTo-Json $a[1] -Compress)") }
    elseif ($message -notmatch $refusAttendu) { $echecs.Add("Refusée, mais pas par la liste de l'Aide : $($a[0]) $(ConvertTo-Json $a[1] -Compress) ($message)") }
}

# ---------------------------------------------------------------------------
# 7. Qui peut parler à l'Aide : seulement Mindset.exe du dossier de cette copie de Nevermind (le programme qui la fait
#    tourner), reconnu par son chemin complet. Toutes les applis de la suite tournent dans un « Mindset.exe » : un
#    Mindset.exe ailleurs, le relais Nevermind.exe ou tout autre programme ne la pilote jamais.
# ---------------------------------------------------------------------------
$dossierAppli = 'C:\Program Files\Mindset\Programmes\Nevermind'
$clients = @(
    @("$dossierAppli\Mindset.exe", $true),
    @('C:\PROGRAM FILES\mindset\programmes\nevermind\MINDSET.EXE', $true),   # (Windows ne tient pas compte des majuscules)
    @("$dossierAppli\Nevermind.exe", $false),                    # le relais : il ne parle jamais à l'Aide
    @("$dossierAppli\NevermindAide.exe", $false),
    @("$dossierAppli\Désinstaller Nevermind.exe", $false),
    @('C:\Program Files\Mindset\Mindset\Mindset.exe', $false),   # Mindset lui-même
    @('C:\Program Files\Mindset\Programmes\Evermind\Mindset.exe', $false),   # une autre appli de la suite
    @("$env:USERPROFILE\Downloads\Mindset.exe", $false),         # n'importe quel Mindset.exe posé ailleurs
    @("$dossierAppli\fichiers\Mindset.exe", $false),
    @("$dossierAppli\..\Nevermind\Mindset.exe", $false),         # chemin détourné (Windows donne toujours le chemin réel)
    @("$dossierAppli\Mindset.exe.old", $false),
    @("$dossierAppli\Mindset.exe ", $false),
    @("$dossierAppli\Mindset", $false),
    @("${dossierAppli}2\Mindset.exe", $false),
    @("$env:windir\System32\WindowsPowerShell\v1.0\powershell.exe", $false),
    @('', $false)
)
foreach ($c in $clients) {
    $nbTests++
    $r = Test-AideClient $c[0] $dossierAppli
    if ($r -ne $c[1]) { $echecs.Add("Client de l'Aide $(if ($c[1]) { 'refusé à tort' } else { 'accepté à tort' }) : « $($c[0]) »") }
}
$nbTests++
if (Test-AideClient "$dossierAppli\Mindset.exe" '') { $echecs.Add('Client de l''Aide accepté sans dossier de référence') }
# NevermindAide.ps1 se sert bien de ce contrôle (connexions et « Nevermind ouvert ? »), et ne cherche aucun programme par
# son nom seul
$codeAide = [IO.File]::ReadAllText((Join-Path $racine 'OptiGame\fichiers\NevermindAide.ps1'), [Text.Encoding]::UTF8)
$nbTests++
if (([regex]::Matches($codeAide, 'Test-AideClient\b')).Count -lt 2) { $echecs.Add('NevermindAide.ps1 : Test-AideClient n''est plus utilisé pour les connexions et pour savoir si Nevermind est ouvert') }
$nbTests++
if ($codeAide -match 'Get-Process\s+-Name|GetProcessesByName|-ne \$Exe\b') { $echecs.Add('NevermindAide.ps1 : un programme y est cherché par son nom') }

# ---------------------------------------------------------------------------
# Résultat
# ---------------------------------------------------------------------------
$couvertes = @($demandes.Keys | Where-Object { $avecRefus.ContainsKey($_) }).Count
Write-Host "$($demandes.Count) demandes dans l'Aide, $couvertes avec au moins un refus testé, $($SansParametre.Count) sans paramètre, $($DispenseesDeRefus.Count) dispensée(s)."
Write-Host "$nbTests essais : $($acceptees.Count) demandes à accepter, $($refusees.Count) à refuser$(if (-not $carte) { ' (aucune carte réseau : essais DNS sur carte réelle sautés)' })."
if ($echecs.Count) {
    Write-Host "$($echecs.Count) échec(s) :" -ForegroundColor Red
    foreach ($e in $echecs) { Write-Host "  - $e" -ForegroundColor Red }
    exit 1
}
Write-Host 'Tout est bon : l''Aide accepte ce qu''il faut et refuse le reste.' -ForegroundColor Green
exit 0
