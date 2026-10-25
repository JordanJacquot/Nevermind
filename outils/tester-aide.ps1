# Tests de l'Aide administrateur (Nevermind\fichiers\modules\aide-actions.ps1), sans droits administrateur et sans rien
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
$actions = Join-Path $racine 'Nevermind\fichiers\modules\aide-actions.ps1'
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
# (la carte qui mène à Internet de préférence : celle qui a une passerelle)
$avecPasserelle = @(Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | Sort-Object RouteMetric | ForEach-Object { [int]$_.ifIndex })
$carte = @(Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' } | Sort-Object { $i = [array]::IndexOf($avecPasserelle, [int]$_.ifIndex); if ($i -lt 0) { 999 } else { $i } })[0]
if (-not $carte) { $carte = @(Get-NetAdapter -ErrorAction SilentlyContinue)[0] }
function Test-Fichier([string]$Chemin) { try { Test-Path -LiteralPath $Chemin -PathType Leaf -ErrorAction Stop } catch { $false } }

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
    @('reg.ecrire', @{ Cle = "HKLM:\$ApprovedRoot\Run"; Nom = 'NevermindTestLogiciel'; Valeur = @(3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0); Type = 'Binary' }),
    @('reg.ecrire', @{ Cle = "HKLM:\$ApprovedRoot\StartupFolder"; Nom = 'Truc.lnk'; Valeur = @(2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0); Type = 'Binary' }),
    @('reg.effacer', @{ Cle = "HKLM:\$ApprovedRoot\Run"; Nom = 'SecurityHealth' }),   # (le remettre actif reste permis)
    @('service.regler', @{ Nom = 'DiagTrack'; Demarrage = 'Disabled'; Arreter = $true }),
    @('service.regler', @{ Nom = 'DiagTrack'; Demarrage = 'Automatic'; Lancer = $true }),
    @('service.regler', @{ Nom = 'edgeupdate'; Demarrage = 'Manual'; Arreter = $true }),   # (page RAM : passé en manuel, puis remis)
    @('service.regler', @{ Nom = 'edgeupdate'; Demarrage = 'Automatic'; Lancer = $false }),
    @('service.regler', @{ Nom = 'GoogleUpdaterService140.0.1.0'; Demarrage = 'Manual'; Arreter = $true }),
    @('service.regler', @{ Nom = 'AdobeARMservice'; Demarrage = 'Manual'; Arreter = $true }),
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
    @('nettoyage.lister', @{ Id = 'wu'; Top = 300 }),
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
foreach ($id in 'wu', 'do', 'dump') { $acceptees += , @('nettoyage.lancer', @{ Id = $id }); $acceptees += , @('nettoyage.lister', @{ Id = $id }) }
# DNS : sur une vraie carte réseau du PC (lue seulement)
# (ceux de la box : seulement la passerelle de cette carte ou les DNS que la box donne, lus sur ce PC)
$dnsLocaux = @()
if ($carte) {
    foreach ($choix in $DnsChoices) { $acceptees += , @('dns.regler', @{ Index = [int]$carte.ifIndex; Serveurs = @($choix) }) }
    $locaux = Get-AideDnsLocaux $carte
    $dnsLocaux = @($locaux | Where-Object { Test-AideAdresseLocale $_ })
    foreach ($ip in $dnsLocaux) { $acceptees += , @('dns.regler', @{ Index = [int]$carte.ifIndex; Serveurs = @($ip) }) }
    if (-not $dnsLocaux.Count) { Write-Host 'Info : aucune box (passerelle ou DNS donné par DHCP en adresse privée) sur la carte de ce PC : DNS de la box non essayé.' }
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
    @('service.regler', @{ Nom = 'WSearch'; Demarrage = 'Manual'; Arreter = $true }),
    @('service.regler', @{ Nom = 'SysMain'; Demarrage = 'Manual' }),
    @('service.regler', @{ Nom = 'edgeupdate2'; Demarrage = 'Manual' }),
    @('service.regler', @{ Nom = 'GoogleUpdaterService1; calc'; Demarrage = 'Manual' }),
    @('service.regler', @{ Nom = 'edgeupdate'; Demarrage = 'Boot' }),
    # Audit du 25/10 (constat 8) : un service de mise à jour jamais « Désactivé » (seulement « Manuel ») ; Sécurité
    # Windows et l'antivirus jamais coupés au démarrage
    @('service.regler', @{ Nom = 'edgeupdate'; Demarrage = 'Disabled'; Arreter = $true }),
    @('service.regler', @{ Nom = 'gupdate'; Demarrage = 'Disabled' }),
    @('service.regler', @{ Nom = 'GoogleUpdaterService140.0.1.0'; Demarrage = 'Disabled' }),
    @('reg.ecrire', @{ Cle = "HKLM:\$ApprovedRoot\Run"; Nom = 'SecurityHealth'; Valeur = @(3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0); Type = 'Binary' }),
    @('reg.ecrire', @{ Cle = "HKLM:\$ApprovedRoot\Run32"; Nom = 'SecurityHealth'; Valeur = @(3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0); Type = 'Binary' }),
    @('reg.ecrire', @{ Cle = "HKLM:\$ApprovedRoot\Run"; Nom = 'Windows Defender'; Valeur = @(7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0); Type = 'Binary' }),
    @('reg.ecrire', @{ Cle = "HKLM:\$ApprovedRoot\StartupFolder"; Nom = 'SecurityHealthSystray.lnk'; Valeur = @(3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0); Type = 'Binary' }),
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
    @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = "$env:windir\System32\svchost.exe"; Services = @('wuauserv') }),
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
    # (constat 3 de l'audit du 25/10 : les dossiers où les utilisateurs écrivent ne sont plus vidés par l'Aide)
    @('nettoyage.lancer', @{ Id = 'wintemp' }),
    @('nettoyage.lister', @{ Id = 'wintemp' }),
    @('nettoyage.lancer', @{ Id = 'wer' }),
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
# Une adresse privée qui n'est ni la box ni un DNS donné par elle : un faux DNS posé sur le réseau local (constat 12)
if ($carte) {
    foreach ($ip in '10.254.254.253', '192.168.1.254', '172.31.254.254', 'fd12:3456::53', 'fe80::dead:beef') {
        if ((ConvertTo-AideIp $ip) -notin $dnsLocaux) { $refusees += , @('dns.regler', @{ Index = [int]$carte.ifIndex; Serveurs = @($ip) }) }
    }
    if ($dnsLocaux.Count) { $refusees += , @('dns.regler', @{ Index = [int]$carte.ifIndex; Serveurs = @($dnsLocaux[0], '10.254.254.253') }) }
}
# Les antivirus inscrits au Centre de sécurité de Windows (lus sur ce PC ; aucun sur les Windows Server de la CI)
$ctxTest = Get-AideContexte
$nbAntivirus = 0
foreach ($av in $ctxTest.Antivirus) {
    if (-not (Test-Fichier $av)) { continue }
    $nbAntivirus++
    $refusees += , @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = $av })
    if (-not $av.StartsWith("$env:windir\", [StringComparison]::OrdinalIgnoreCase)) { $refusees += , @('processus.fermer', @{ Chemin = $av }) }
    # (un autre programme du même dossier : le moteur, l'interface...)
    $voisin = @(Get-ChildItem -LiteralPath (Split-Path $av) -Filter *.exe -ErrorAction SilentlyContinue | Where-Object { $_.FullName -ne $av } | Select-Object -First 1)
    if ($voisin -and -not $av.StartsWith("$env:windir\", [StringComparison]::OrdinalIgnoreCase)) { $refusees += , @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = $voisin[0].FullName }) }
}
# Les services (leur programme) : jamais fermés ni bloqués en entier. Hors du dossier de Windows pour « fermer » (le
# dossier de Windows est déjà refusé), dans Windows pour « bloquer » (permis pour les autres programmes de Windows).
$exeServices = @($ctxTest.ServicesExe | Where-Object { (Test-Fichier $_) -and [IO.Path]::GetFileName($_) -notmatch $AideExesProteges })
$horsWindows = @($exeServices | Where-Object { -not $_.StartsWith("$env:windir\", [StringComparison]::OrdinalIgnoreCase) -and -not (Test-AideAntivirus $ctxTest $_) } | Select-Object -First 3)
$dansWindows = @($exeServices | Where-Object { $_.StartsWith("$env:windir\", [StringComparison]::OrdinalIgnoreCase) } | Select-Object -First 3)
foreach ($s in $horsWindows) { $refusees += , @('processus.fermer', @{ Chemin = $s }); $refusees += , @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = $s }) }
foreach ($s in $dansWindows) { $refusees += , @('pare-feu.bloquer', @{ Nom = 'Nevermind : bloque X'; Programme = $s }) }
Write-Host "Info : $nbAntivirus programme(s) d'antivirus inscrit(s), $($horsWindows.Count) + $($dansWindows.Count) programme(s) de service essayés."
if (-not $dansWindows.Count) { $echecs.Add('Services : aucun programme de service trouvé dans le dossier de Windows (Win32_Service illisible ?)') }
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
$codeAide = [IO.File]::ReadAllText((Join-Path $racine 'Nevermind\fichiers\NevermindAide.ps1'), [Text.Encoding]::UTF8)
$nbTests++
if (([regex]::Matches($codeAide, 'Test-AideClient\b')).Count -lt 2) { $echecs.Add('NevermindAide.ps1 : Test-AideClient n''est plus utilisé pour les connexions et pour savoir si Nevermind est ouvert') }
$nbTests++
if ($codeAide -match 'Get-Process\s+-Name|GetProcessesByName|-ne \$Exe\b') { $echecs.Add('NevermindAide.ps1 : un programme y est cherché par son nom') }

# ---------------------------------------------------------------------------
# 8. Fermer un programme (constat 12) : seulement un programme du compte et de la session de celui qui demande (Nevermind),
#    jamais un service, un processus protégé (antivirus) ni le programme d'un autre compte. Essayé sur un programme de
#    ce test seulement (une copie de ping.exe qui attend), sans droits ; les processus de Windows sont seulement lus.
# ---------------------------------------------------------------------------
$tmpFermer = Join-Path $env:TEMP "nevermind-test-aide-$PID"
$essai = $null
try {
    [void][IO.Directory]::CreateDirectory($tmpFermer)
    $exeEssai = Join-Path $tmpFermer 'nm-essai-fermer.exe'
    [IO.File]::Copy((Join-Path $env:windir 'System32\PING.EXE'), $exeEssai, $true)
    $exeEssai = (Get-Item -LiteralPath $exeEssai).FullName   # (chemin long, celui que Windows donne)
    $psi = New-Object Diagnostics.ProcessStartInfo($exeEssai, '-n 60 127.0.0.1')
    $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true; $psi.RedirectStandardOutput = $true
    $essai = [Diagnostics.Process]::Start($psi)
    Start-Sleep -Milliseconds 300
    $ctxP = Get-AideContexte
    $nbTests++
    $r = Get-AideRefusProcessus $ctxP $PID $essai.Id $exeEssai
    if ($r) { $echecs.Add("Fermer : un programme de l'utilisateur, dans sa session, est refusé ($r)") }
    # Un service : jamais
    $pidService = @($ctxP.ServicesPid | Where-Object { $_ -gt 4 } | Select-Object -First 1)[0]
    $nbTests++
    if (-not $pidService) { $echecs.Add('Fermer : aucun service en cours trouvé (Win32_Service illisible ?)') }
    elseif (-not (Get-AideRefusProcessus $ctxP $PID $pidService '')) { $echecs.Add("Fermer : un service (processus $pidService) peut être fermé") }
    # Des processus de Windows d'une autre session ou d'un autre compte (csrss, wininit, smss...) : jamais
    foreach ($n in 'csrss', 'wininit', 'smss', 'lsass', 'MsMpEng') {
        foreach ($pr in @(Get-Process -Name $n -ErrorAction SilentlyContinue)) {
            $nbTests++
            if (-not (Get-AideRefusProcessus $ctxP $PID $pr.Id '')) { $echecs.Add("Fermer : $n (processus $($pr.Id)) peut être fermé") }
            $pr.Dispose()
        }
    }
    # Le niveau de protection est bien lu : ce programme-ci n'est pas protégé, csrss l'est (lisible en administrateur)
    $nbTests++
    if ([AideProcessus]::Protection($essai.Id) -ne 0) { $echecs.Add('Fermer : un programme ordinaire est vu comme protégé') }
    $csrss = @(Get-Process -Name csrss -ErrorAction SilentlyContinue)[0]
    if ($csrss) { Write-Host "Info : protection de csrss lue : $([AideProcessus]::Protection($csrss.Id)) (1 protégé, -1 illisible sans droits)" }
    # Demandé au nom d'un programme d'une autre session (un service) : rien n'est fermé
    $nbTests++
    $n = Invoke-AideAction 'processus.fermer' @{ Chemin = $exeEssai } -Client $pidService
    if ($n -ne 0 -or $essai.HasExited) { $echecs.Add("Fermer : un programme a été fermé à la demande d'un autre compte ou d'une autre session ($n)") }
    # Sans client connu : refusé
    $nbTests++
    $message = $null
    try { [void](Invoke-AideAction 'processus.fermer' @{ Chemin = $exeEssai }) } catch { $message = $_.Exception.Message }
    if ($message -notmatch 'Demande refusée') { $echecs.Add('Fermer : accepté sans savoir qui le demande') }
    # Demandé par ce test (même compte, même session) : fermé
    $nbTests++
    $n = Invoke-AideAction 'processus.fermer' @{ Chemin = $exeEssai } -Client $PID
    if ($n -ne 1 -or -not $essai.WaitForExit(5000)) { $echecs.Add("Fermer : le programme du test n'a pas été fermé ($n)") }
} catch { $echecs.Add("Fermer : $($_.Exception.Message)") }
finally {
    if ($essai) { try { if (-not $essai.HasExited) { $essai.Kill() } } catch {}; $essai.Dispose() }
    Start-Sleep -Milliseconds 200
    try { [IO.Directory]::Delete($tmpFermer, $true) } catch {}
}

# ---------------------------------------------------------------------------
# 9. Aucun dossier de Windows lu dans les variables d'environnement (constat 1) : l'Aide les prend chez Windows
# ---------------------------------------------------------------------------
$codeActions = [IO.File]::ReadAllText($actions, [Text.Encoding]::UTF8)
$nbTests++
$lignesEnv = @($codeActions -split "`n" | Where-Object { $_ -notmatch '^\s*#' -and $_ -match '\$env:|\$\{env:' })
if ($lignesEnv.Count) { $echecs.Add("aide-actions.ps1 lit encore des variables d'environnement : $($lignesEnv[0].Trim())") }
# Modules : l'Aide coupe leur chargement automatique. Toute commande d'un module autre que ceux de base doit être
# précédée, dans sa fonction ou sa demande, d'un Import-AideModule de ce module (sinon : commande introuvable chez
# l'utilisateur seulement, en administrateur).
$base = 'Microsoft.PowerShell.Core', 'Microsoft.PowerShell.Management', 'Microsoft.PowerShell.Utility', 'Microsoft.PowerShell.Security', 'CimCmdlets'
$astAide = [System.Management.Automation.Language.Parser]::ParseInput($codeAide, [ref]$null, [ref]$null)
$fonctionsAide = @(foreach ($a in $ast, $astAide) { $a.FindAll({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $true) | ForEach-Object { $_.Name } })
$appels = @(foreach ($a in @(@('aide-actions.ps1', $ast), @('NevermindAide.ps1', $astAide))) {
    foreach ($cmd in $a[1].FindAll({ param($n) $n -is [System.Management.Automation.Language.CommandAst] }, $true)) {
        $nom = $cmd.GetCommandName()
        if ($nom -and $nom -notin $fonctionsAide) { , @($a[0], $cmd, $nom) }
    }
})
# (le module de chaque commande, demandé à un PowerShell propre : ce test remplace certaines commandes par des pièges)
$propre = [PowerShell]::Create()
[void]$propre.AddScript('param($Noms) $r = @{}; foreach ($n in $Noms) { $c = Get-Command -Name $n -ErrorAction SilentlyContinue; if ($c) { $r[$n] = [string]$c.ModuleName } }; $r').AddArgument([string[]]@($appels | ForEach-Object { $_[2] } | Select-Object -Unique))
$modulesDe = @($propre.Invoke())[0]; $propre.Dispose()
foreach ($a in $appels) {
    $fichier, $cmd, $nom = $a
    $nbTests++
    $module = $modulesDe[$nom]
    if (-not $module) { $echecs.Add("$fichier : commande « $nom » introuvable dans les modules de Windows"); continue }
    if ($module -in $base) { continue }
    if ($fichier -eq 'NevermindAide.ps1') { $echecs.Add("NevermindAide.ps1 : « $nom » (module $module) n'est pas chargé au démarrage de l'Aide"); continue }
    $p = $cmd.Parent
    while ($p -and -not ($p -is [System.Management.Automation.Language.FunctionDefinitionAst]) -and -not ($p -is [System.Management.Automation.Language.StatementBlockAst] -and $p.Parent -is [System.Management.Automation.Language.SwitchStatementAst])) { $p = $p.Parent }
    $imports = if ($p) { @($p.FindAll({ param($n) $n -is [System.Management.Automation.Language.CommandAst] -and $n.GetCommandName() -eq 'Import-AideModule' }, $true)) } else { @() }
    $charges = @($imports | ForEach-Object { $_.CommandElements | Select-Object -Skip 1 | ForEach-Object { $_.Extent.Text -split '\s*[,/]\s*' } })
    if ($charges -notcontains $module) { $echecs.Add("aide-actions.ps1 ligne $($cmd.Extent.StartLineNumber) : « $nom » (module $module) sans Import-AideModule $module avant") }
}
$nbTests++
if ($codeAide -notmatch "(?m)^\`$PSModuleAutoLoadingPreference = 'None'" -or $codeAide -notmatch "\`$env:PSModulePath = \`$AideDossierPS") { $echecs.Add('NevermindAide.ps1 : le chargement automatique des modules n''est plus coupé') }
$nbTests++
if ($AideWindows -ne [Environment]::GetFolderPath('Windows') -or $AideNettoyage['wu'][0] -ne (Join-Path ([Environment]::GetFolderPath('Windows')) 'SoftwareDistribution\Download')) { $echecs.Add('aide-actions.ps1 : dossiers de Windows mal lus') }
# Constat 3 (audit du 25/10) : plus aucun dossier où les utilisateurs écrivent (C:\Windows\Temp, rapports d'erreurs)
$nbTests++
$ecrits = @(foreach ($id in $AideNettoyage.Keys) { foreach ($d in $AideNettoyage[$id]) { if ($d -match '\\Temp$|\\WER\\') { "$id ($d)" } } })
if ($ecrits.Count) { $echecs.Add("aide-actions.ps1 : l'Aide vide encore un dossier où les utilisateurs écrivent : $($ecrits -join ', ')") }

# Constat 8 (audit du 25/10) : ce que l'Aide refuse même à Nevermind
function Test-Constat8([bool]$Ok, [string]$Message) { $script:nbTests++; if (-not $Ok) { $script:echecs.Add("Constat 8 : $Message") } }
# Points de restauration : un par 24 h au plus
$maintenant = [DateTime]::UtcNow
Test-Constat8 (-not (Test-AideRestaurationRecente 0 $maintenant)) 'aucun point noté : un point est refusé'
Test-Constat8 (Test-AideRestaurationRecente $maintenant.AddHours(-1).Ticks $maintenant) 'un point il y a 1 h : un 2e est créé'
Test-Constat8 (-not (Test-AideRestaurationRecente $maintenant.AddHours(-25).Ticks $maintenant)) 'un point il y a 25 h : le suivant est refusé'
Test-Constat8 (-not (Test-AideRestaurationRecente $maintenant.AddHours(3).Ticks $maintenant)) 'un point noté dans le futur (horloge changée) bloque tout'
$texteActions = [IO.File]::ReadAllText((Join-Path $AideDossierModules 'aide-actions.ps1'))
Test-Constat8 ($texteActions -match "(?s)'restauration\.creer' \{.*?Test-AideRestaurationRecente.*?Checkpoint-Computer.*?DerniereRestauration") 'restauration.creer ne vérifie pas le dernier point avant d''en créer un, ou ne le note pas'
# Tâches planifiées : seulement celles du compte qui demande et du système
$sidTest = 'S-1-5-21-1111111111-2222222222-3333333333-1001'
Test-Constat8 (Test-AideTacheDuCompte $sidTest '' $sidTest) 'une tâche du compte qui demande est cachée'
Test-Constat8 (-not (Test-AideTacheDuCompte 'S-1-5-21-1111111111-2222222222-3333333333-1002' '' $sidTest)) 'une tâche d''un autre compte est montrée'
Test-Constat8 (Test-AideTacheDuCompte 'SYSTEM' '' $sidTest) 'une tâche du système est cachée'
Test-Constat8 (Test-AideTacheDuCompte '' 'S-1-5-32-545' $sidTest) 'une tâche pour tous les utilisateurs est cachée'
Test-Constat8 (-not (Test-AideTacheDuCompte '' 'S-1-5-32-544' $sidTest)) 'une tâche des administrateurs (autres comptes) est montrée'
Test-Constat8 (-not (Test-AideTacheDuCompte '' '' $sidTest) -and -not (Test-AideTacheDuCompte $sidTest '' '')) 'compte inconnu : une tâche est montrée'
Test-Constat8 ($texteActions -match "(?s)'lecture\.taches' \{.*?Test-AideTacheDuCompte") 'lecture.taches ne filtre pas les tâches par compte'
# Exclusions de l'antivirus : leur nombre seulement
$blocSecu = [regex]::Match($texteActions, "(?s)'lecture\.securite' \{.*?return \`$r").Value
Test-Constat8 ($blocSecu -match 'ExclusionsNombre = ' -and $blocSecu -notmatch '[.{ ]Exclusions = ') 'lecture.securite rend encore les chemins des exclusions'
# Programmes au démarrage : Sécurité Windows et les antivirus inscrits
$avTest = @('C:\Program Files\AntivirusTest\avtest-service.exe')
Test-Constat8 (Test-AideDemarrageSecurite "HKLM:\$ApprovedRoot\Run" 'AvTestUI' '"C:\Program Files\AntivirusTest\avtest-ui.exe" /nogui' $avTest) 'l''interface d''un antivirus inscrit peut être coupée au démarrage'
Test-Constat8 (Test-AideDemarrageSecurite "HKLM:\$ApprovedRoot\Run" 'Defender' '"%ProgramFiles%\Windows Defender\MSASCuiL.exe"' $avTest) 'Microsoft Defender peut être coupé au démarrage'
Test-Constat8 (-not (Test-AideDemarrageSecurite "HKLM:\$ApprovedRoot\Run" 'Spotify' '"C:\Program Files\Spotify\Spotify.exe" /background' $avTest)) 'un programme ordinaire est pris pour un antivirus'

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
