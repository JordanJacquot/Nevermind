# Nevermind : l'Aide en SYSTEM essayée en vrai, sur la machine jetable de GitHub Actions (workflow
# .github\workflows\aide-reelle.yml, lancé à la main). Vraies tâches planifiées, vrais comptes Windows, vraie installation
# par l'installateur de Mindset : JAMAIS sur un vrai PC. Le script refuse de tourner hors de GitHub Actions.
#
#   .\outils\essai-aide-reelle.ps1 -Cas 0     préparation (Nevermind construit, Mindset construit, clé de TEST, paquets)
#   .\outils\essai-aide-reelle.ps1 -Cas N     un cas (1 à 10), voir la liste plus bas
#   -Aide <exe>     (avec -Cas 0) le NevermindAide.exe à juger, mis dans les paquets à la place de celui que construit
#                   outils\construire.ps1 (le lanceur C#) : par exemple l'exe Rust construit dans le même run
#   -Aides <a;b>    (avec -Cas 10) les NevermindAide.exe à comparer, l'un après l'autre sur la même machine ; sans
#                   -Aides, celui des paquets, deux fois
#
# Tout ce qui est vérifié l'est de l'extérieur (tâches, canal, processus, registre, réponses) : le même essai juge
# n'importe quel NevermindAide.exe qui garde la même ligne de commande (-Sid) et le même canal.
#
# Le client est le vrai Mindset.exe de Nevermind (outils\lanceur.cs), installé par Mindset dans Program Files\Mindset,
# lancé sous le compte voulu : dans le paquet d'essai, seul fichiers\Nevermind.ps1 est remplacé par un petit client qui
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
#   8. changement de nom (OptiGame devient Nevermind) : Nevermind 1.11.0 installé par Mindset 2.12.0 puis mis à jour par le
#      nouveau Mindset (Nevermind.zip) ; la version renommée refuse de démarrer tant que 1.11.0 est ouverte, puis démarre
#      seule avec ses deux verrous, une seule instance (le zip de compatibilité pour Mindset 2.12.0 n'est plus publié :
#      Mindset 2.12.0 sert seulement à installer 1.11.0)
#   9. l'Aide lancée en secours par la demande de Windows (sous le compte qui accepte, avec les droits administrateur),
#      dans un environnement piégé (variables, dossiers du profil, modules) : pour son propre compte elle répond, sans
#      jamais rien prendre du piège ; jamais de canal pour un « SID » qui n'est pas un compte d'utilisateur
#  10. tout optimiser puis tout restaurer : l'état de la machine (registre, services, DNS, pare-feu, plans
#      d'alimentation, tâches) relevé avant, après l'optimisation et après la restauration ; la restauration remet
#      tout, et deux NevermindAide.exe (-Aides) donnent les mêmes états et les mêmes réponses
# Code de sortie 1 si un cas échoue.
#  11. (avec -Banc <outils\tester-aide-rust.ps1> -Aide <exe Rust>) le banc PowerShell / Rust, qui refuse de tourner en
#      administrateur : lancé ici sous un compte local sans droits créé pour l'essai
#  13. relancer la carte réseau de la connexion (carte.relancer) : l'Aide relance pour de vrai la carte de la machine de
#      GitHub, la carte s'éteint puis se rallume, la connexion revient (60 s au plus) ; les autres cartes de la machine
#      et les numéros mal formés sont refusés sans qu'aucune carte ne bouge
#   -Pont <dll>     (avec -Cas 0) NevermindPont.dll construite dans le même run (rust\nevermind-pont), mise dans les
#                   paquets à côté des programmes : la version de Nevermind qui s'en sert ne marche pas sans elle
param([int]$Cas = -1, [string]$Aide = '', [string]$Aides = '', [string]$Banc = '', [string]$Pont = '')

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
# En secours, l'Aide jugée refuse-t-elle un -Sid qui n'est pas son propre compte ? (noté au cas 0, voir Initialize-Essai)
function Test-SecoursPropreCompte { $f = Join-Path $Racine 'aide-secours.txt'; (Test-Path -LiteralPath $f) -and [IO.File]::ReadAllText($f).Trim() -eq 'propre-compte' }

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
# Les variables d'environnement d'un processus qui tourne (lues dans sa mémoire, de l'extérieur : n'importe quel
# NevermindAide.exe, quel que soit son langage). L'essai tourne en administrateur ; le droit de déboguer est pris pour lire
# un processus de SYSTEM ou d'un autre compte.
function Get-EnvProcessus([int]$Id) {
    if (-not ('EssaiEnv' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
public static class EssaiEnv
{
    [StructLayout(LayoutKind.Sequential)] struct PBI { public IntPtr R1; public IntPtr Peb; public IntPtr R2a; public IntPtr R2b; public IntPtr Id; public IntPtr R3; }
    [DllImport("ntdll.dll")] static extern int NtQueryInformationProcess(IntPtr h, int classe, ref PBI pbi, int taille, out int lu);
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr OpenProcess(uint acces, bool herite, int pid);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool ReadProcessMemory(IntPtr h, IntPtr adresse, byte[] tampon, IntPtr taille, out IntPtr lu);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    static byte[] Lire(IntPtr h, IntPtr a, int n)
    {
        var b = new byte[n]; IntPtr lu;
        if (!ReadProcessMemory(h, a, b, (IntPtr)n, out lu)) throw new Win32Exception(Marshal.GetLastWin32Error());
        return b;
    }
    // Processus 64 bits : PEB + 0x20 = ProcessParameters ; ProcessParameters + 0x80 = Environment, + 0x3F0 = EnvironmentSize
    public static Dictionary<string, string> Variables(int pid)
    {
        IntPtr h = OpenProcess(0x0410, false, pid);   // PROCESS_QUERY_INFORMATION | PROCESS_VM_READ
        if (h == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
        try
        {
            var pbi = new PBI(); int lu;
            int s = NtQueryInformationProcess(h, 0, ref pbi, Marshal.SizeOf(typeof(PBI)), out lu);
            if (s != 0) throw new InvalidOperationException("NtQueryInformationProcess : " + s.ToString("X"));
            IntPtr pp = (IntPtr)BitConverter.ToInt64(Lire(h, IntPtr.Add(pbi.Peb, 0x20), 8), 0);
            IntPtr env = (IntPtr)BitConverter.ToInt64(Lire(h, IntPtr.Add(pp, 0x80), 8), 0);
            long taille = BitConverter.ToInt64(Lire(h, IntPtr.Add(pp, 0x3F0), 8), 0);
            if (taille <= 0 || taille > 4 * 1024 * 1024) taille = 32768;
            byte[] o = null;
            while (o == null && taille >= 2) { try { o = Lire(h, env, (int)taille); } catch { taille /= 2; } }
            if (o == null) throw new InvalidOperationException("environnement illisible");
            var r = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            foreach (string l in Encoding.Unicode.GetString(o).Split('\0'))
            {
                if (l.Length == 0) break;
                int i = l.IndexOf('=', 1);   // (les variables « =C: » commencent par =)
                if (i > 0) r[l.Substring(0, i)] = l.Substring(i + 1);
            }
            return r;
        }
        finally { CloseHandle(h); }
    }
}
'@
    }
    try { [Diagnostics.Process]::EnterDebugMode() } catch {}
    [EssaiEnv]::Variables($Id)
}
# Le canal d'une Aide : celui de ce compte et de ce dossier de Nevermind (même calcul que l'Aide, aide-actions.ps1)
function Get-NomCanal([string]$Sid, [string]$Dossier) {
    if (-not (Get-Command Get-AideSuffixe -ErrorAction SilentlyContinue)) {
        $actions = Join-Path $Depot 'Nevermind\fichiers\modules\aide-actions.ps1'
        . ([scriptblock]::Create(([System.Management.Automation.Language.Parser]::ParseFile($actions, [ref]$null, [ref]$null)).Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Get-AideSuffixe' }, $true).Extent.Text))
        Set-Item -Path function:global:Get-AideSuffixe -Value ${function:Get-AideSuffixe}
    }
    "Nevermind-Aide-$Sid-$(Get-AideSuffixe $Dossier)"
}
function Test-Canal([string]$Nom) { [bool]@([IO.Directory]::GetFiles('\\.\pipe\') | Where-Object { $_ -like "*\$Nom" }).Count }
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
function Install-Appli([string]$Installateur, [string]$Zip, [string]$Sid, [string]$DossierZip = 'Nevermind') {
    $f = Join-Path $Sorties "install-$([guid]::NewGuid().ToString('N').Substring(0, 8)).json"
    # (depuis le lot 1 de l'audit du 25/10, l'installateur de Mindset prend l'appli dans son apps.json : -Appli ; les
    # anciens, 2.10.0 et 2.12.0, ne connaissent pas ce paramètre)
    $appli = @(if ([IO.File]::ReadAllText($Installateur, [Text.Encoding]::UTF8) -match '\[string\]\$Appli\b') { '-Appli', 'nevermind' })
    & $PowerShellExe -NoProfile -ExecutionPolicy Bypass -File $Installateur -Action appli @appli -Zip $Zip -Signature "$Zip.sig" -Destination $NevermindC -DossierZip $DossierZip -Exe Nevermind.exe -TacheAide $NomTache -ExeAide NevermindAide.exe -Sid $Sid -Resultat $f | Out-Host
    $r = Read-Json $f
    Write-Host "         | installation de $(Split-Path $Zip -Leaf) (installateur $Installateur) : $($r.ok) $($r.message)" -ForegroundColor DarkGray
    $r
}
$InstallateurNeuf = Join-Path $MindsetDepot 'Mindset\fichiers\installer-admin.ps1'
$Installateur210 = Join-Path $Mindset210 'Mindset\fichiers\installer-admin.ps1'
$PaquetEssai = Join-Path $Paquets 'essai\Nevermind.zip'     # Nevermind du dépôt, Nevermind.ps1 remplacé par le client
$PaquetVrai = Join-Path $Paquets 'vrai\Nevermind.zip'       # Nevermind du dépôt, tel quel
$Paquet110 = Join-Path $Paquets '1.10.0\OptiGame.zip'      # la Release v1.10.0 (signée par la vraie clé ; ancien nom du zip)
$Paquet111 = Join-Path $Paquets '1.11.0\OptiGame.zip'      # la Release v1.11.0, la dernière d'avant le changement de nom
$Mindset212 = Join-Path $Racine 'mindset-212'             # Mindset 2.12.0, le dernier d'avant le changement de nom
$Installateur212 = Join-Path $Mindset212 'Mindset\fichiers\installer-admin.ps1'

# Le client : Mindset.exe de Nevermind (le paquet d'essai) sous le compte $Compte. Il lit sa consigne dans
# C:\essai\clients\<son SID>.json. Une demande de Windows qu'il fait est lancée ici, en administrateur, si -Accepter.
function Invoke-Client($Compte, [hashtable]$Consigne, [switch]$Fond) {
    $id = [guid]::NewGuid().ToString('N').Substring(0, 8)
    $Consigne.Sortie = Join-Path $Sorties "client-$id.json"
    $Consigne.RunAsFichier = Join-Path $Sorties "runas-$id.json"
    Write-Utf8 (Join-Path $Clients "$($Compte.Sid).json") ($Consigne | ConvertTo-Json -Depth 5)
    # (sans Cred : sous le compte de l'essai lui-même, l'administrateur qui accepte les demandes de Windows)
    $p = if ($Compte.Cred) { Start-Process -FilePath (Join-Path $NevermindC 'Mindset.exe') -Credential $Compte.Cred -LoadUserProfile -WorkingDirectory $env:SystemRoot -PassThru }
         else { Start-Process -FilePath (Join-Path $NevermindC 'Mindset.exe') -WorkingDirectory $env:SystemRoot -PassThru }
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

# Le client du paquet d'essai (il remplace fichiers\Nevermind.ps1)
$TexteClient = @'
# Essai de l'Aide en vrai (outils\essai-aide-reelle.ps1) : ce fichier remplace Nevermind.ps1 dans le paquet d'essai.
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
    # (la version de Nevermind avec NevermindPont.dll : le nom du canal et la règle « fermable » viennent de la DLL,
    # chargée comme le fait Nevermind.ps1)
    foreach ($m in 'pont', 'processus') { $f = Join-Path $ModulesDir "$m.ps1"; if (Test-Path -LiteralPath $f) { . $f } }
    if (Get-Command Initialize-Pont -ErrorAction SilentlyContinue) {
        $raison = Initialize-Pont (Join-Path $racineNevermind 'NevermindPont.dll')
        if ($raison) { $script:Journal.Add("NevermindPont.dll : $raison") }
    }
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
            $ErrorActionPreference = 'Continue'   # (le refus de Windows arrive sur la sortie d'erreur)
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
    # Depuis Mindset 2.15 (étape 2 du passage en Rust), installer-admin.ps1 n'est plus qu'un relais vers
    # MindsetInstallateur.exe : construit par le workflow (variable MINDSET_INSTALLATEUR), posé à côté de Mindset.exe
    # comme dans le vrai zip (il part avec lui quand l'essai installe Mindset dans Program Files, cas 4 et 6)
    $relais = [IO.File]::ReadAllText($InstallateurNeuf, [Text.Encoding]::UTF8).Contains('MindsetInstallateur.exe')
    if ($relais) {
        if (-not $env:MINDSET_INSTALLATEUR -or -not (Test-Path -LiteralPath $env:MINDSET_INSTALLATEUR -PathType Leaf)) { throw 'MindsetInstallateur.exe absent : la variable MINDSET_INSTALLATEUR doit donner l''exe construit par le workflow.' }
        Copy-Item -LiteralPath $env:MINDSET_INSTALLATEUR -Destination (Join-Path $MindsetDepot 'Mindset\MindsetInstallateur.exe') -Force
        $hi = (Get-FileHash -LiteralPath $env:MINDSET_INSTALLATEUR -Algorithm SHA256).Hash
        Test-Cas "MindsetInstallateur.exe construit dans ce run, à côté de Mindset.exe (SHA-256 $hi)" { (Get-FileHash -LiteralPath (Join-Path $MindsetDepot 'Mindset\MindsetInstallateur.exe') -Algorithm SHA256).Hash -eq $hi }
    }
    # Le lot A2 (tâche d'aide en SYSTEM, -Sid de l'utilisateur ; anciennes tâches sous le compte refaites en SYSTEM) : dans
    # installer-admin.ps1 avant 2.15, dans le code Rust de MindsetInstallateur.exe ensuite (le comportement lui-même est
    # jugé en vrai aux cas 1 et 4)
    Test-Cas 'son installateur crée la tâche d''aide en SYSTEM (Mindset avec le lot A2)' {
        if (-not $relais) { return [IO.File]::ReadAllText($InstallateurNeuf, [Text.Encoding]::UTF8) -match 'function Update-TachesAide' }
        $rs = [IO.File]::ReadAllText((Join-Path $MindsetGit 'rust\mindset-installateur\src\installateur.rs'), [Text.Encoding]::UTF8)
        $m = [regex]::Match($rs, '(?s)\bfn enregistrer_tache_aide\(.*?\r?\n    \}')
        $m.Success -and $m.Value.Contains('compte: CompteTache::Systeme') -and $m.Value.Contains('arguments: format!("-Sid {sid}")') -and
            $rs -match '\bfn refaire_taches_aide\(' -and $rs -match 'self\.refaire_taches_aide\('
    }
    # Mindset 2.10.0 (son installateur, tel que publié)
    $ErrorActionPreference = 'Continue'   # (git écrit ses messages sur la sortie d'erreur)
    if (Test-Path -LiteralPath $Mindset210) { & git -C $MindsetGit worktree remove --force $Mindset210 2>&1 | Out-Null }
    & git -C $MindsetGit worktree add --detach $Mindset210 v2.10.0 2>&1 | ForEach-Object { "$_" } | Out-Host
    # Mindset 2.12.0, le dernier d'avant le changement de nom (son installateur, tel que publié) : il installe la vraie
    # Release 1.11.0 (cas 8), signée par la vraie clé
    if (Test-Path -LiteralPath $Mindset212) { & git -C $MindsetGit worktree remove --force $Mindset212 2>&1 | Out-Null }
    & git -C $MindsetGit worktree add --detach $Mindset212 v2.12.0 2>&1 | ForEach-Object { "$_" } | Out-Host
    $ErrorActionPreference = 'Stop'
    Test-Cas 'Mindset 2.10.0 : son installateur crée l''ancienne tâche d''aide (droits les plus élevés, sous le compte)' { [IO.File]::ReadAllText($Installateur210, [Text.Encoding]::UTF8) -match 'New-ScheduledTaskPrincipal -UserId \$Sid -LogonType Interactive -RunLevel Highest' }
    Test-Cas 'Mindset 2.12.0 : il ne connaît que les anciens noms (OptiGame.zip, dossier OptiGame, fichiers\OptiGame.ps1, verrou Local\OptiGame-Instance)' {
        $a = @([IO.File]::ReadAllText((Join-Path $Mindset212 'Mindset\fichiers\apps.json'), [Text.Encoding]::UTF8).TrimStart([char]0xFEFF) | ConvertFrom-Json) | Where-Object { $_.id -eq 'nevermind' }
        Write-Host "         | apps.json de Mindset 2.12.0 : zip $($a.zip), dossier $($a.dossier_zip), script $($a.script), verrou $($a.mutex)" -ForegroundColor DarkGray
        $a.zip -eq 'OptiGame.zip' -and $a.dossier_zip -eq 'OptiGame' -and $a.script -eq 'fichiers\OptiGame.ps1' -and $a.mutex -eq 'Local\OptiGame-Instance'
    }

    # Nevermind du dépôt, construit
    & (Join-Path $Depot 'outils\construire.ps1') | Out-Host
    $og = Join-Path $Depot 'Nevermind'
    Test-Cas 'Nevermind construit (Mindset.exe, NevermindAide.exe, désinstalleur)' { (Test-Path -LiteralPath "$og\Mindset.exe") -and (Test-Path -LiteralPath "$og\NevermindAide.exe") -and (Test-Path -LiteralPath "$og\Désinstaller Nevermind.exe") }
    # L'Aide à juger : celle que construit construire.ps1 (le lanceur C#), ou l'exe donné par -Aide, mis à sa place dans
    # les paquets (même nom, même dossier : rien d'autre ne change)
    $jugee = "lanceur C# (outils\construire.ps1)"
    if ($Aide) {
        $source = [IO.Path]::GetFullPath($Aide)
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Aide à juger introuvable : $source" }
        Copy-Item -LiteralPath $source -Destination "$og\NevermindAide.exe" -Force
        $jugee = $source
    }
    if ($Pont) {
        if (-not (Test-Path -LiteralPath $Pont -PathType Leaf)) { throw "NevermindPont.dll introuvable : $Pont" }
        Copy-Item -LiteralPath $Pont -Destination "$og\NevermindPont.dll" -Force
        Test-Cas "NevermindPont.dll construite dans ce run, à côté des programmes (SHA-256 $((Get-FileHash -LiteralPath "$og\NevermindPont.dll" -Algorithm SHA256).Hash))" { Test-Path -LiteralPath "$og\NevermindPont.dll" }
    }
    $h = (Get-FileHash -LiteralPath "$og\NevermindAide.exe" -Algorithm SHA256).Hash
    Write-Utf8 (Join-Path $Racine 'aide-jugee.txt') "$jugee ; SHA-256 $h"
    # La règle du secours que suit l'Aide jugée (décision de Jordan du 27/10) : la nouvelle Aide (-Aide, l'exe Rust) refuse un
    # -Sid qui n'est pas son propre compte ; le lanceur C# actuel sert le compte -Sid (cas 5 et 9)
    Write-Utf8 (Join-Path $Racine 'aide-secours.txt') $(if ($Aide) { 'propre-compte' } else { 'ancienne' })
    Test-Cas "NevermindAide.exe jugé : $jugee (SHA-256 $h)" { -not $Aide -or (Get-FileHash -LiteralPath $Aide -Algorithm SHA256).Hash -eq $h }
    foreach ($genre in 'vrai', 'essai') {
        $d = Join-Path $Paquets $genre
        if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force }
        New-Item -ItemType Directory -Force -Path $d | Out-Null
        if ($genre -eq 'vrai') {
            # Le zip d'une version : Nevermind.zip (outils\zips.ps1)
            $null = & (Join-Path $Depot 'outils\zips.ps1') -Dossier $og -Sortie $d
            New-Signature (Join-Path $d 'Nevermind.zip')
            continue
        }
        Copy-Item -LiteralPath $og -Destination $d -Recurse
        # (le même numéro de version que le vrai : Mindset refuse un zip dont il ne peut pas lire la version)
        $versionVraie = [regex]::Match([IO.File]::ReadAllText((Join-Path $og 'fichiers\Nevermind.ps1')), "(?m)^\`$AppVersion\s*=\s*'[^']+'").Value
        [IO.File]::WriteAllText((Join-Path $d 'Nevermind\fichiers\Nevermind.ps1'), "$versionVraie`r`n$TexteClient", (New-Object Text.UTF8Encoding $true))
        Compress-Archive -Path (Join-Path $d 'Nevermind') -DestinationPath (Join-Path $d 'Nevermind.zip') -Force
        New-Signature (Join-Path $d 'Nevermind.zip')
    }
    # Les Releases v1.10.0 et v1.11.0 (zip à l'ancien nom), vérifiées avec les vraies clés de Mindset
    foreach ($v in '1.10.0', '1.11.0') {
        $d = Join-Path $Paquets $v
        if (Test-Path -LiteralPath $d) { Remove-Item -LiteralPath $d -Recurse -Force }
        New-Item -ItemType Directory -Force -Path $d | Out-Null
        $ErrorActionPreference = 'Continue'
        & gh release download "v$v" -R JordanJacquot/Nevermind -p 'OptiGame.zip*' -D $d 2>&1 | ForEach-Object { "$_" } | Out-Host
        $ErrorActionPreference = 'Stop'
    }
    . (Join-Path $MindsetGit 'Mindset\fichiers\modules\signature.ps1')
    Test-Cas 'Releases Nevermind v1.10.0 et v1.11.0 téléchargées, signées par la vraie clé' {
        (Test-Signature ([IO.File]::ReadAllBytes($Paquet110)) ([IO.File]::ReadAllBytes("$Paquet110.sig")) (Join-Path $MindsetGit 'Mindset\fichiers\cles-publiques')) -and
            (Test-Signature ([IO.File]::ReadAllBytes($Paquet111)) ([IO.File]::ReadAllBytes("$Paquet111.sig")) (Join-Path $MindsetGit 'Mindset\fichiers\cles-publiques'))
    }
    Test-Cas 'paquets d''essai signés avec la clé de TEST, acceptés par la copie d''essai de Mindset' {
        $cles = Join-Path $MindsetDepot 'Mindset\fichiers\cles-publiques'
        @($PaquetEssai, $PaquetVrai | Where-Object { -not (Test-Signature ([IO.File]::ReadAllBytes($_)) ([IO.File]::ReadAllBytes("$_.sig")) $cles) }).Count -eq 0
    }
    Test-Cas 'les deux paquets contiennent l''Aide jugée (même empreinte)' {
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $sha = [Security.Cryptography.SHA256]::Create()
        $empreintes = @(foreach ($z in $PaquetEssai, $PaquetVrai) {
            $a = [IO.Compression.ZipFile]::OpenRead($z)
            try {
                $e = @($a.Entries | Where-Object { $_.FullName.Replace('\', '/') -match '^[^/]+/NevermindAide\.exe$' })
                if ($e.Count -ne 1) { 'absent ou en double' }
                else { $s = $e[0].Open(); try { -join ($sha.ComputeHash($s) | ForEach-Object { $_.ToString('X2') }) } finally { $s.Dispose() } }
            } finally { $a.Dispose() }
        })
        Write-Host "         | NevermindAide.exe des paquets : $($empreintes -join ', ')" -ForegroundColor DarkGray
        $empreintes.Count -eq 2 -and -not @($empreintes | Where-Object { $_ -ne $h }).Count
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
    # (où l'Aide compile : aide-temp, ou C:\Windows\SystemTemp que les Windows récents donnent à SYSTEM quoi que dise TEMP ;
    # dans les deux cas, ni Élodie ni les utilisateurs ne peuvent y écrire)
    $ecrit = { param($d) try { @((Get-Acl -LiteralPath $d).Access | Where-Object { $_.AccessControlType -eq 'Allow' -and ([int64]$_.FileSystemRights -band 0x116) -and
        $_.IdentityReference.Translate([Security.Principal.SecurityIdentifier]).Value -notin 'S-1-5-18', 'S-1-5-32-544', 'S-1-3-0' }).Count } catch { 99 } }
    Test-Cas 'l''Aide répond : en SYSTEM, avec les droits administrateur, compile dans un dossier où personne d''autre que le système et les administrateurs n''écrit' {
        $t = ([string]$bj.Temp).TrimEnd('\')
        Write-Host "         | dossier de compilation : $t ($(& $ecrit $t) droit(s) d'écriture pour d'autres)" -ForegroundColor DarkGray
        $bj -and [string]$bj.Compte -eq 'S-1-5-18' -and $bj.Admin -eq $true -and $t -in "$NevermindC\aide-temp", (Join-Path ([Environment]::GetFolderPath('Windows')) 'SystemTemp') -and (& $ecrit $t) -eq 0
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
    Test-Cas 'Bruno ne peut pas ouvrir le canal d''Élodie (accès refusé)' { -not $s.R.PSObject.Properties['canal'] -and [string]$s.E.canal -match 'denied|refus' }
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
    $r = Install-Appli $Installateur210 $Paquet110 $e.Sid 'OptiGame'
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
    # (Nevermind retiré d'abord : Mindset refuse d'installer une version plus ancienne que celle en place, 1.10.0 après 1.11.0)
    Remove-Item -LiteralPath $NevermindC -Recurse -Force -ErrorAction SilentlyContinue
    $r = Install-Appli $Installateur210 $Paquet110 $e.Sid 'OptiGame'
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
    # (décision de Jordan du 27/10 : la nouvelle Aide, lancée par l'administrateur qui accepte, refuse de servir Élodie,
    # un autre compte que le sien : elle s'arrête sans rien faire ; le lanceur C# actuel la sert)
    $propreCompte = Test-SecoursPropreCompte
    $canalElodie = Get-NomCanal $e.Sid $NevermindC
    $verifier = {
        param($S, [string]$Quoi)
        $bj = Get-Bonjour $S
        if ($propreCompte) {
            $arretees = Wait-Jusque { -not @($script:RunAsLancees | Where-Object { -not $_.HasExited }).Count } 20
            Test-Cas "$Quoi : demande de Windows pour NevermindAide.exe -Sid <Élodie>, acceptée par l'administrateur ; l'Aide refuse ce compte qui n'est pas le sien (arrêtée, aucun canal, aucune réponse)" {
                @($S.RunAs).Count -eq 1 -and $S.RunAs[0] -eq "RunAs|$exe|-Sid $($e.Sid)" -and -not $bj -and @($script:RunAsLancees).Count -eq 1 -and $arretees -and -not (Test-Canal $canalElodie)
            }
        } else {
            Test-Cas "$Quoi : demande de Windows pour NevermindAide.exe -Sid <Élodie>, acceptée, l'Aide répond (administrateur, pas SYSTEM)" {
                @($S.RunAs).Count -eq 1 -and $S.RunAs[0] -eq "RunAs|$exe|-Sid $($e.Sid)" -and $bj -and $bj.Admin -eq $true -and [string]$bj.Compte -eq $Moi
            }
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
    # (l'Aide rendue muette de l'extérieur, quel que soit l'exe jugé : à sa place, un programme qui n'écoute jamais)
    Register-ScheduledTask -TaskPath '\Mindset\' -TaskName $NomTache -Xml $bonne.Xml -Force | Out-Null
    Rename-Item -LiteralPath $exe -NewName 'NevermindAide.exe.cache'
    Copy-Item -LiteralPath "$Racine\appli\Dormeur.exe" -Destination $exe
    $avant = (Get-InfoTache).Dernier
    $s = Invoke-Client $e ($client + @{ AttenteWindows = 10 })
    Stop-Aides
    Remove-Item -LiteralPath $exe -Force
    Rename-Item -LiteralPath "$exe.cache" -NewName 'NevermindAide.exe'
    Test-Cas 'tâche SYSTEM reconnue et lancée, mais muette (Aide qui n''écoute pas) : 20 s, puis la demande de Windows pour NevermindAide.exe -Sid <Élodie>' {
        $s.Etat -eq 'systeme' -and (Get-InfoTache).Dernier -ne $avant -and $s.Duree -ge 20 -and @($s.RunAs).Count -eq 1 -and $s.RunAs[0] -eq "RunAs|$exe|-Sid $($e.Sid)"
    }

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
    # Une copie de Nevermind dans Program Files (modifiable par les seuls administrateurs), son Aide lancée par une tâche
    # SYSTEM identique à la vraie : ses variables d'environnement sont lues dans sa mémoire, de l'extérieur, une fois son
    # canal ouvert (quel que soit l'exe jugé). Contre-épreuve : un programme lancé sous le compte d'Élodie, lu de même.
    $copie = Join-Path $env:ProgramFiles 'MindsetEssaiEnv\Nevermind'
    New-Item -ItemType Directory -Force -Path (Split-Path $copie -Parent) | Out-Null
    Copy-Item -LiteralPath $NevermindC -Destination $copie -Recurse -Force
    $t = Get-InfoTache
    Register-ScheduledTask -TaskPath '\MindsetEssai\' -TaskName 'Environnement' -Xml $t.Xml.Replace($NevermindC, $copie) -Force | Out-Null
    Start-ScheduledTask -TaskPath '\MindsetEssai\' -TaskName 'Environnement'
    $canal = Get-NomCanal $e.Sid $copie
    $sy = @{ Vars = @{} }
    if (Wait-Jusque { Test-Canal $canal } 60) {
        $a = @(Get-AidesEnCours | Where-Object { $_.Chemin -like "$copie\*" -and $_.Sid -eq 'S-1-5-18' }) | Select-Object -First 1
        if ($a) { $sy.Compte = $a.Sid; try { $sy.Vars = Get-EnvProcessus $a.Id } catch { Write-Host "         | environnement de l'Aide illisible : $($_.Exception.Message)" -ForegroundColor DarkGray } }
    } else { Write-Host "         | l'Aide de la copie n'a pas ouvert son canal ($canal)" -ForegroundColor DarkGray }
    $pe = Start-Process -FilePath "$Racine\appli\Dormeur.exe" -Credential $e.Cred -LoadUserProfile -WorkingDirectory $env:SystemRoot -PassThru
    $el = @{ Vars = @{} }
    Start-Sleep -Seconds 2
    try {
        $el.Compte = [string](Invoke-CimMethod -InputObject (Get-CimInstance Win32_Process -Filter "ProcessId=$($pe.Id)") -MethodName GetOwnerSid).Sid
        $el.Vars = Get-EnvProcessus $pe.Id
    } catch { Write-Host "         | environnement du témoin illisible : $($_.Exception.Message)" -ForegroundColor DarkGray }
    try { $pe.Kill() } catch {}
    Stop-Aides
    foreach ($x in @(@('SYSTEM', $sy), @('Élodie', $el))) { Write-Host "         | $($x[0]) : compte $($x[1].Compte), $($x[1].Vars.Count) variables, NEVERMIND_TEMOIN_ENV=$($x[1].Vars['NEVERMIND_TEMOIN_ENV']), TEMP=$($x[1].Vars['TEMP']), PSModulePath=$($x[1].Vars['PSModulePath'])" -ForegroundColor DarkGray }
    Test-Cas 'contre-épreuve : un programme lancé sous le compte d''Élodie voit sa variable témoin' { $el.Compte -eq $e.Sid -and $el.Vars['NEVERMIND_TEMOIN_ENV'] -eq '1' }
    Test-Cas 'l''Aide lancée par la tâche SYSTEM ne voit ni la variable témoin, ni rien du dossier d''Élodie, ni aucune variable COR_* ou COMPlus_*' {
        $sy.Compte -eq 'S-1-5-18' -and $sy.Vars.Count -and -not $sy.Vars.ContainsKey('NEVERMIND_TEMOIN_ENV') -and -not @($sy.Vars.Values | Where-Object { $_ -like "*$NomElodie*" -or $_ -like "*$marque*" }).Count -and
            -not @($sy.Vars.Keys | Where-Object { $_ -match '^(COR_|COMPlus_|CORECLR_)' }).Count
    }
    Remove-Item -LiteralPath (Split-Path $copie -Parent) -Recurse -Force -ErrorAction SilentlyContinue
}

# =====================================================================================================================
# 8. Changement de nom (OptiGame devient Nevermind) : mise à jour par le nouveau Mindset, une seule instance
# =====================================================================================================================
# Le vrai Nevermind tourne ici, sous le compte de l'essai (géré par Mindset : suite.json le note, lancé caché avec
# -Demarrage, comme Mindset le fait). Les verrous sont ceux de la session de l'essai.
$AncienNom = Join-Path $Racine 'ancien'   # Nevermind 1.11.0 ouvert ailleurs (dossier de travail : il s'ouvre sans Mindset)
function Test-Verrou([string]$Nom) {
    try { $m = [Threading.Mutex]::OpenExisting($Nom); $m.Dispose(); $true }
    catch { $i = $_.Exception; while ($i.InnerException) { $i = $i.InnerException }; $i -is [UnauthorizedAccessException] }
}
function Get-Nevermind([string]$Dossier) { @(Get-CimInstance Win32_Process -Filter "Name='Mindset.exe'" -ErrorAction SilentlyContinue | Where-Object { [string]$_.ExecutablePath -like "$Dossier\*" }) }
function Stop-Nevermind {
    foreach ($p in @(Get-Nevermind $ProgrammesC) + @(Get-Nevermind $AncienNom)) { try { Stop-Process -Id $p.ProcessId -Force -ErrorAction Stop } catch {} }
    $null = Wait-Jusque { -not (Test-Verrou 'Local\Nevermind-Instance') -and -not (Test-Verrou 'Local\OptiGame-Instance') } 30
}
function Start-Nevermind([string]$Exe) { Start-Process -FilePath $Exe -ArgumentList '-Demarrage' -WorkingDirectory $env:SystemRoot -PassThru }
function Get-VersionScript([string]$Fichier) {
    if (-not (Test-Path -LiteralPath $Fichier)) { return '' }
    [regex]::Match([IO.File]::ReadAllText($Fichier, [Text.Encoding]::UTF8), "(?m)^\`$AppVersion\s*=\s*'([^']+)'").Groups[1].Value
}
function Show-Nevermind([string]$Quoi) {
    $s = @(Get-ChildItem -LiteralPath "$NevermindC\fichiers" -Filter '*.ps1' -File -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Name) ($(Get-VersionScript $_.FullName))" })
    Write-Host "         | $Quoi : scripts $($s -join ', ') ; Mindset.exe ouverts $(@(Get-Nevermind $ProgrammesC).Count) ; verrous Nevermind $(Test-Verrou 'Local\Nevermind-Instance'), OptiGame $(Test-Verrou 'Local\OptiGame-Instance')" -ForegroundColor DarkGray
}
function Get-JournalNevermind {
    @(foreach ($d in (Join-Path $env:LOCALAPPDATA 'Mindset\Nevermind'), (Join-Path $env:LOCALAPPDATA 'OptiGame')) {
        $f = Join-Path $d 'journal.txt'
        if (Test-Path -LiteralPath $f) { Get-Content -LiteralPath $f -Encoding UTF8 -ErrorAction SilentlyContinue }
    })
}
function Test-Cas8 {
    Write-Partie '8. Changement de nom : Nevermind 1.11.0 (OptiGame) mis à jour par le nouveau Mindset, une seule instance'
    Reset-Machine
    Stop-Nevermind
    $e = Get-Compte $NomElodie
    $version = Get-VersionScript (Join-Path $Depot 'Nevermind\fichiers\Nevermind.ps1')
    $suite = Join-Path $env:LOCALAPPDATA 'Mindset\suite.json'
    try {
        # a. Nevermind 1.11.0 installé par Mindset 2.12.0 (la vraie Release), puis mis à jour par le nouveau Mindset
        $r = Install-Appli $Installateur212 $Paquet111 $e.Sid 'OptiGame'
        Show-Nevermind 'après 1.11.0'
        Test-Cas 'Nevermind 1.11.0 installé par Mindset 2.12.0 : fichiers\OptiGame.ps1, version 1.11.0' { $r.ok -and (Get-VersionScript "$NevermindC\fichiers\OptiGame.ps1") -eq '1.11.0' }
        $r = Install-Appli $InstallateurNeuf $PaquetVrai $e.Sid 'Nevermind'
        Show-Nevermind 'après la mise à jour'
        Test-Cas "mise à jour par le nouveau Mindset avec Nevermind.zip : fichiers\Nevermind.ps1 ($version), plus de fichiers\OptiGame.ps1 à côté" {
            $r.ok -and (Get-VersionScript "$NevermindC\fichiers\Nevermind.ps1") -eq $version -and -not (Test-Path -LiteralPath "$NevermindC\fichiers\OptiGame.ps1") -and
                @(Get-ChildItem -LiteralPath "$NevermindC\fichiers" -Filter '*.ps1' -File).Count -eq 2   # (Nevermind.ps1 et NevermindAide.ps1)
        }
        New-Item -ItemType Directory -Force -Path (Split-Path $suite) | Out-Null
        Write-Utf8 $suite (@{ applis = @{ nevermind = $NevermindC } } | ConvertTo-Json -Compress)

        # b. Une ancienne version encore ouverte (verrou Local\OptiGame-Instance) : la version renommée ne démarre pas
        if (Test-Path -LiteralPath $AncienNom) { Remove-Item -LiteralPath $AncienNom -Recurse -Force }
        Expand-Archive -LiteralPath $Paquet111 -DestinationPath $AncienNom -Force
        New-Item -ItemType Directory -Force -Path (Join-Path $AncienNom '.git') | Out-Null
        $vieux = Start-Nevermind (Join-Path $AncienNom 'OptiGame\Mindset.exe')
        # (la machine de GitHub peut ne pas avoir de session où une fenêtre s'ouvre : Nevermind n'y démarre alors pas du tout,
        # même 1.11.0 ; le démarrage et l'instance unique sont alors seulement vérifiés par outils\tester-renommage.ps1)
        $lancer = Wait-Jusque { Test-Verrou 'Local\OptiGame-Instance' } 120
        if (-not $lancer) { Add-Note 'Nevermind 1.11.0 ne démarre pas sur cette machine (aucun verrou en 120 s) : démarrages non essayés ici, voir tester-renommage.ps1'; Show-Nevermind '1.11.0 lancée' }
        else {
        Test-Cas 'Nevermind 1.11.0 démarre (il tient Local\OptiGame-Instance, pas Local\Nevermind-Instance)' { -not (Test-Verrou 'Local\Nevermind-Instance') }
        $neuf = Start-Nevermind "$NevermindC\Mindset.exe"
        $arrete = Wait-Jusque { $neuf.HasExited } 60
        Show-Nevermind 'pendant que 1.11.0 est ouverte'
        Test-Cas 'la version renommée refuse de démarrer tant que 1.11.0 est ouverte : elle s''arrête seule, sans garder Local\Nevermind-Instance' { $arrete -and -not (Test-Verrou 'Local\Nevermind-Instance') -and -not $vieux.HasExited }
        Test-Cas 'son journal le dit (« Lancement caché abandonné »)' { @(Get-JournalNevermind | Where-Object { $_ -match 'Lancement caché abandonné' }).Count -ge 1 }
        Stop-Nevermind

        # c. Seule : elle démarre, tient ses deux verrous, et une seule instance
        $neuf = Start-Nevermind "$NevermindC\Mindset.exe"
        Test-Cas 'seule, la version renommée démarre et tient Local\Nevermind-Instance ET Local\OptiGame-Instance (les anciens Mindset la voient ouverte)' {
            Wait-Jusque { (Test-Verrou 'Local\Nevermind-Instance') -and (Test-Verrou 'Local\OptiGame-Instance') } 120
        }
        Start-Sleep -Seconds 10
        Show-Nevermind 'version renommée ouverte'
        Test-Cas "elle reste ouverte, son journal dit « Démarrage Nevermind $version »" { -not $neuf.HasExited -and @(Get-JournalNevermind | Where-Object { $_ -match "Démarrage Nevermind $([regex]::Escape($version))" }).Count -ge 1 }
        $deux = Start-Nevermind "$NevermindC\Mindset.exe"
        $arrete = Wait-Jusque { $deux.HasExited } 60
        Show-Nevermind 'après un deuxième lancement'
        Test-Cas 'un deuxième lancement s''arrête seul : une seule instance' { $arrete -and -not $neuf.HasExited -and @(Get-Nevermind $ProgrammesC).Count -eq 1 }
        Stop-Nevermind
        }
        # (Mindset 2.12.0 n'installe plus une nouvelle version : le zip de compatibilité n'est plus publié depuis la
        # décision de Jordan du 29/10, un ancien Mindset se met d'abord à jour lui-même)
    } finally {
        Stop-Nevermind
        Remove-Item -LiteralPath $suite -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $AncienNom -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# =====================================================================================================================
# 9. L'Aide lancée en secours par la demande de Windows : sous le compte qui accepte, avec les droits administrateur
# =====================================================================================================================
# Comme au cas 5, le « Oui » de la demande de Windows est simulé : l'essai (administrateur) lance NevermindAide.exe -Sid
# lui-même. Ici dans un environnement piégé : variables qui mènent à de faux dossiers de Windows, de faux modules
# PowerShell (partout où PowerShell en chercherait), un faux profil, un faux dossier temporaire. Un faux module chargé,
# un profil lu ou un fichier écrit dans le piège laisse une trace.
$ModulesPieges = @('Microsoft.PowerShell.Management', 'Microsoft.PowerShell.Utility', 'Microsoft.PowerShell.Security', 'CimCmdlets', 'ScheduledTasks',
    'NetSecurity', 'NetAdapter', 'NetTCPIP', 'DnsClient', 'Storage', 'ConfigDefender', 'Defender', 'SmbShare', 'Dism', 'PSReadLine')
function New-ModulesPieges([string]$Dossier) {
    foreach ($m in $ModulesPieges) {
        New-Item -ItemType Directory -Force -Path (Join-Path $Dossier $m) | Out-Null
        Write-Utf8 (Join-Path $Dossier "$m\$m.psm1") "[IO.File]::WriteAllText('$Sorties\piege-$m-' + `$PID + '.txt', [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName)"
    }
}
function Get-TracesPiege { @(Get-ChildItem -LiteralPath $Sorties -Filter 'piege-*.txt' -File -ErrorAction SilentlyContinue | ForEach-Object { "$($_.Name) ($([IO.File]::ReadAllText($_.FullName)))" }) }
# NevermindAide.exe -Sid <Sid> lancé par l'administrateur (la demande de Windows acceptée), avec ces variables-là
function Start-AideSecours([string]$Sid, [hashtable]$Variables, [string]$Dossier) {
    $psi = New-Object Diagnostics.ProcessStartInfo("$NevermindC\NevermindAide.exe")
    $psi.Arguments = "-Sid $Sid"
    $psi.UseShellExecute = $false
    $psi.WorkingDirectory = $Dossier
    foreach ($k in $Variables.Keys) { $psi.EnvironmentVariables[$k] = [string]$Variables[$k] }
    [Diagnostics.Process]::Start($psi)
}
function Test-Cas9 {
    Write-Partie '9. L''Aide lancée en secours par la demande de Windows : sous le compte, avec les droits administrateur'
    Reset-Machine
    $e = Get-Compte $NomElodie
    $null = Install-Appli $InstallateurNeuf $PaquetEssai $e.Sid
    Stop-Aides
    $piege = Join-Path $Racine 'piege'
    if (Test-Path -LiteralPath $piege) { Remove-Item -LiteralPath $piege -Recurse -Force }
    try {
        New-ModulesPieges "$piege\modules"
        New-ModulesPieges "$piege\windows\System32\WindowsPowerShell\v1.0\Modules"
        New-ModulesPieges "$piege\pf\WindowsPowerShell\Modules"
        New-ModulesPieges "$piege\profil\Documents\WindowsPowerShell\Modules"
        foreach ($n in 'profile.ps1', 'Microsoft.PowerShell_profile.ps1') {
            Write-Utf8 "$piege\profil\Documents\WindowsPowerShell\$n" "[IO.File]::WriteAllText('$Sorties\piege-profil-' + `$PID + '.txt', 'profil lu')"
        }
        foreach ($d in 'temp', 'pd', 'cwd', 'profil\AppData\Roaming', 'profil\AppData\Local') { New-Item -ItemType Directory -Force -Path "$piege\$d" | Out-Null }
        Get-ChildItem -LiteralPath $Sorties -Filter 'piege-*.txt' -File -ErrorAction SilentlyContinue | Remove-Item -Force
        $fichiersAvant = @(Get-ChildItem -LiteralPath $piege -Recurse -Force | ForEach-Object { $_.FullName })
        # (windir et SystemRoot piégés à part, en d. : le moteur .NET du lanceur C# les lit avant la première ligne de l'Aide)
        $variables = @{
            NEVERMIND_TEMOIN_ENV = '1'; PSModulePath = "$piege\modules"
            ProgramFiles = "$piege\pf"; ProgramW6432 = "$piege\pf"; 'ProgramFiles(x86)' = "$piege\pf"; ProgramData = "$piege\pd"; ALLUSERSPROFILE = "$piege\pd"
            TEMP = "$piege\temp"; TMP = "$piege\temp"; USERPROFILE = "$piege\profil"; HOMEDRIVE = $piege.Substring(0, 2); HOMEPATH = "$piege\profil".Substring(2)
            APPDATA = "$piege\profil\AppData\Roaming"; LOCALAPPDATA = "$piege\profil\AppData\Local"
        }
        $variablesWindows = $variables.Clone(); $variablesWindows.windir = "$piege\windows"; $variablesWindows.SystemRoot = "$piege\windows"
        $moiCompte = @{ Nom = "$env:USERNAME (administrateur)"; Sid = $Moi; Cred = $null }

        # a. -Sid de son propre compte : elle répond à Nevermind de ce compte (ouvert avant : l'Aide reste tant qu'il l'est)
        $canal = Get-NomCanal $Moi $NevermindC
        $garde = Invoke-Client $moiCompte @{ Mode = 'garder'; Secondes = 180 } -Fond
        $p = Start-AideSecours $Moi $variables "$piege\cwd"
        $ouvert = Wait-Jusque { Test-Canal $canal } 60
        $vars = @{}
        if ($ouvert) { try { $vars = Get-EnvProcessus $p.Id } catch { Write-Host "         | environnement illisible : $($_.Exception.Message)" -ForegroundColor DarkGray } }
        $aide = @(Get-AidesEnCours | Where-Object { $_.Id -eq $p.Id })
        $s = Invoke-Client $moiCompte @{ Mode = 'appel'; SansDemande = $true; Actions = @(@{ Nom = 'bonjour'; Action = 'aide.bonjour'; P = @{} }, @{ Nom = 'taches'; Action = 'lecture.taches'; P = @{} }) }
        $bj = Get-Bonjour $s
        Test-Cas 'lancée pour son propre compte : elle tourne sous ce compte (pas en SYSTEM) et ouvre le canal de ce compte' {
            $ouvert -and $aide.Count -eq 1 -and $aide[0].Sid -eq $Moi
        }
        Test-Cas 'elle répond à Nevermind de ce compte (c''est bien elle : son processus) : son compte, avec les droits administrateur, compile dans aide-temp (jamais dans le TEMP reçu)' {
            $t = ([string]$bj.Temp).TrimEnd('\')
            Write-Host "         | bonjour : processus $($bj.Pid) (Aide $($p.Id)), compte $($bj.Compte), administrateur $($bj.Admin), dossier de compilation $t" -ForegroundColor DarkGray
            $bj -and [int]$bj.Pid -eq $p.Id -and [string]$bj.Compte -eq $Moi -and $bj.Admin -eq $true -and $t -in "$NevermindC\aide-temp", (Join-Path ([Environment]::GetFolderPath('Windows')) 'SystemTemp')
        }
        Test-Cas 'une lecture qui charge un module de Windows (lecture.taches) marche' { $s.R.PSObject.Properties['taches'] -and -not $s.E.PSObject.Properties['taches'] }
        $traces = Get-TracesPiege
        $nouveaux = @(Get-ChildItem -LiteralPath $piege -Recurse -Force | ForEach-Object { $_.FullName } | Where-Object { $fichiersAvant -notcontains $_ })
        foreach ($x in @($traces) + @($nouveaux)) { Write-Host "         | piège : $x" -ForegroundColor DarkGray }
        Test-Cas 'rien pris du compte : aucun faux module chargé, aucun profil lu, rien écrit dans les faux dossiers (temporaire, profil, Windows)' { -not $traces.Count -and -not $nouveaux.Count }
        $dossiers = 'windir', 'SystemRoot', 'ProgramFiles', 'ProgramW6432', 'ProgramFiles(x86)', 'ProgramData', 'ALLUSERSPROFILE', 'PSModulePath', 'TEMP', 'TMP'
        foreach ($k in $dossiers) { if ($vars.ContainsKey($k)) { Write-Host "         | $k=$($vars[$k])" -ForegroundColor DarkGray } }
        Test-Cas 'les dossiers de Windows et le dossier temporaire de l''Aide ne mènent pas au piège (pour ce qu''elle lance : PresentMon, netsh...)' {
            $vars.Count -and -not @($dossiers | Where-Object { $vars.ContainsKey($_) -and ([string]$vars[$_]).IndexOf($piege, [StringComparison]::OrdinalIgnoreCase) -ge 0 }).Count
        }
        try { $p.Kill() } catch {}
        try { $garde.Kill() } catch {}
        Stop-Aides

        # b. -Sid d'un autre compte (Élodie, compte standard : l'administrateur accepte pour elle, comme au cas 5)
        $canalElodie = Get-NomCanal $e.Sid $NevermindC
        $p = Start-AideSecours $e.Sid $variables "$piege\cwd"
        $servie = Wait-Jusque { Test-Canal $canalElodie } 30
        Test-Cas 'lancée pour un autre compte : jamais de canal au nom du compte administrateur qui l''a lancée' { -not (Test-Canal $canal) }
        $tourne = -not $p.HasExited
        Write-Host "         | -Sid d'Élodie : $(if ($servie) { 'canal ouvert pour elle' } else { 'aucun canal' }), Aide $(if ($tourne) { 'toujours là' } else { "arrêtée, code $($p.ExitCode)" })" -ForegroundColor DarkGray
        if (Test-SecoursPropreCompte) {
            # (décision de Jordan du 27/10)
            Test-Cas 'lancée pour un autre compte (Élodie) : refusé, aucun canal pour elle, et l''Aide s''arrête' { -not $servie -and (Wait-Jusque { $p.HasExited } 15) }
        } elseif ($servie) { Add-Note 'lanceur C# actuel : en secours, il sert encore un autre compte (Élodie) ; la nouvelle Aide le refuse (décision de Jordan du 27/10)' }
        else { Add-Note 'lanceur C# actuel : en secours, aucun canal pour un autre compte (Élodie), contrairement au cas 5' }
        try { $p.Kill() } catch {}
        Stop-Aides

        # c. Un « SID » qui n'est pas un compte d'utilisateur : jamais de canal à ce nom, jamais en SYSTEM
        foreach ($faux in 'S-1-5-18', 'S-1-5-32-544', 'pas-un-sid') {
            $p = Start-AideSecours $faux $variables "$piege\cwd"
            Start-Sleep -Seconds 5
            $c = Get-NomCanal $faux $NevermindC
            Test-Cas "-Sid « $faux » : aucun canal à ce nom, aucune Aide en SYSTEM" { -not (Test-Canal $c) -and -not @(Get-AidesEnCours | Where-Object { $_.Sid -eq 'S-1-5-18' }).Count }
            try { $p.Kill() } catch {}
            Stop-Aides
        }

        # d. windir et SystemRoot piégés eux aussi : elle ne s'en sert jamais (elle répond sans eux, ou ne démarre pas)
        $p = Start-AideSecours $Moi $variablesWindows "$piege\cwd"
        $ouvert = Wait-Jusque { Test-Canal $canal } 20
        $vars = @{}
        if ($ouvert) { try { $vars = Get-EnvProcessus $p.Id } catch {} }
        Write-Host "         | windir et SystemRoot piégés : $(if ($ouvert) { "canal ouvert, windir=$($vars['windir']), SystemRoot=$($vars['SystemRoot'])" } elseif ($p.HasExited) { "pas démarrée (code $($p.ExitCode))" } else { 'pas de canal' })" -ForegroundColor DarkGray
        Test-Cas 'windir et SystemRoot piégés eux aussi : rien pris du piège, et si elle répond, ses dossiers de Windows sont les vrais' {
            -not @(Get-TracesPiege).Count -and (-not $ouvert -or ($vars.Count -and -not @('windir', 'SystemRoot', 'PSModulePath' | Where-Object { ([string]$vars[$_]).IndexOf($piege, [StringComparison]::OrdinalIgnoreCase) -ge 0 }).Count))
        }
        if (-not $ouvert) { Add-Note 'windir et SystemRoot piégés : cette Aide ne démarre pas (le moteur .NET du lanceur C# les lit avant elle) ; rien de pris, mais pas de secours dans ce cas' }
        try { $p.Kill() } catch {}
        Stop-Aides
        $nouveaux = @(Get-ChildItem -LiteralPath $piege -Recurse -Force | ForEach-Object { $_.FullName } | Where-Object { $fichiersAvant -notcontains $_ })
        foreach ($x in $nouveaux) { Write-Host "         | piège : $x" -ForegroundColor DarkGray }
        Test-Cas 'toujours rien pris du piège après tous ces lancements (aucun faux module, aucun profil, aucun fichier écrit)' { -not (Get-TracesPiege).Count -and -not $nouveaux.Count }
    } finally {
        Stop-Aides
        Remove-Item -LiteralPath $piege -Recurse -Force -ErrorAction SilentlyContinue
        Get-ChildItem -LiteralPath $Sorties -Filter 'piege-*.txt' -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
    }
}

# =====================================================================================================================
# 10. Tout optimiser puis tout restaurer, comparé d'un NevermindAide.exe à l'autre
# =====================================================================================================================
# Les réglages que l'Aide accepte de changer (sa liste, aide-actions.ps1) : Nevermind les envoie pour « optimiser » ;
# pour « Tout restaurer », il renvoie les valeurs d'origine relevées avant (ici, l'état « avant » de l'essai).
$RegEssai = @(
    ,@('HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers', 'HwSchMode', 'DWord', 2)
    ,@('HKU:\S-1-5-20\Software\Microsoft\Windows\CurrentVersion\DeliveryOptimization\Settings', 'DownloadMode', 'DWord', 0)
    ,@('HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization', 'DOPercentageMaxBackgroundBandwidth', 'DWord', 10)
    ,@('HKLM:\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization', 'DODownloadMode', 'DWord', 0)
    ,@('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection', 'AllowTelemetry', 'DWord', 0)
    ,@('HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection', 'AllowTelemetry', 'DWord', 0)
    ,@('HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore', 'SystemRestorePointCreationFrequency', 'DWord', 0)
    ,@('HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\EssaiNevermind.exe\PerfOptions', 'CpuPriorityClass', 'DWord', 3)
    ,@('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run', 'EssaiNevermind', 'Binary', @(3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0))
)
function Open-CleLecture([string]$Cle) {
    if ($Cle -match '^HKLM:\\(.+)$') { return [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($Matches[1]) }
    if ($Cle -match '^HKU:\\(.+)$') { return [Microsoft.Win32.Registry]::Users.OpenSubKey($Matches[1]) }
    $null
}
# Une valeur du registre : $null si absente, sinon @{ Type; Valeur ; Texte }
function Get-ValeurReg([string]$Cle, [string]$Nom) {
    $k = Open-CleLecture $Cle
    if (-not $k) { return $null }
    try {
        if ($k.GetValueNames() -notcontains $Nom) { return $null }
        $type = $k.GetValueKind($Nom).ToString()
        $v = $k.GetValue($Nom, $null, 'DoNotExpandEnvironmentNames')
        $texte = if ($v -is [byte[]]) { ($v | ForEach-Object { $_.ToString('X2') }) -join ' ' } elseif ($v -is [array]) { $v -join '|' } else { [string]$v }
        @{ Type = $type; Valeur = $(if ($v -is [byte[]]) { [int[]]$v } else { $v }); Texte = "$type=$texte" }
    } finally { $k.Close() }
}
function Get-TexteReg([string]$Cle, [string]$Nom) { $v = Get-ValeurReg $Cle $Nom; if ($v) { $v.Texte } else { 'absente' } }
# Un objet JSON écrit par l'Aide (valeurs d'origine), remis dans un ordre fixe : seul le contenu compte, pas la mise en forme
function ConvertTo-JsonCanonique([string]$Texte) {
    try { $o = $Texte | ConvertFrom-Json } catch { return "illisible : $Texte" }
    if ($o -isnot [Management.Automation.PSCustomObject]) { return "$o" }
    (@($o.PSObject.Properties | Sort-Object Name | ForEach-Object { "$($_.Name)=$(@($_.Value) -join ',')" }) -join ' ; ')
}
# La carte réseau de l'essai (celle de la route par défaut) et les services que l'essai règle
function Get-CarteEssai {
    $r = @(Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | Sort-Object RouteMetric) | Select-Object -First 1
    $ad = Get-NetAdapter -InterfaceIndex $r.ifIndex -ErrorAction Stop
    @{ Index = [int]$ad.ifIndex; Guid = '{' + ([string]$ad.InterfaceGuid).Trim('{', '}') + '}'; Nom = [string]$ad.Name }
}
function Get-ServicesEssai {
    $memoire = @(Get-Service -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '^(edgeupdate|edgeupdatem|gupdate|gupdatem|MozillaMaintenance|AdobeARMservice|GoogleUpdater(Internal)?Service[\d.]{0,20})$' } |
        Where-Object { (Get-TexteReg "HKLM:\SYSTEM\CurrentControlSet\Services\$($_.Name)" 'Start') -ne 'DWord=4' } |
        Sort-Object Name | Select-Object -First 4 | ForEach-Object { $_.Name })   # (pas un service désactivé : l'Aide ne remet jamais « Désactivé »)
    @{ Telemetrie = 'DiagTrack'; Memoire = $memoire }
}
# L'état de la machine : tout ce que l'Aide peut toucher (et des témoins qu'elle ne doit jamais toucher), en textes
function Get-EtatMachine($Carte, $Services) {
    $etat = [ordered]@{}
    foreach ($r in $RegEssai) { $etat["registre|$($r[0])|$($r[1])"] = Get-TexteReg $r[0] $r[1] }
    foreach ($n in @($Services.Telemetrie) + @($Services.Memoire)) {
        $c = "HKLM:\SYSTEM\CurrentControlSet\Services\$n"
        $etat["service|$n"] = "démarrage $(Get-TexteReg $c 'Start') ; différé $(Get-TexteReg $c 'DelayedAutostart')"
    }
    $etat["service en marche|$($Services.Telemetrie)"] = [string](Get-Service -Name $Services.Telemetrie -ErrorAction SilentlyContinue).Status
    # (DNS : une valeur vide et une valeur absente disent la même chose, « automatique » ; Windows laisse une valeur vide
    # après une remise à l'automatique, Set-DnsClientServerAddress -ResetServerAddresses)
    foreach ($b in 'Tcpip', 'Tcpip6') {
        $v = Get-TexteReg "HKLM:\SYSTEM\CurrentControlSet\Services\$b\Parameters\Interfaces\$($Carte.Guid)" 'NameServer'
        $etat["dns|$b|$($Carte.Nom)"] = if ($v -in 'absente', 'String=') { 'automatique' } else { $v }
    }
    $regles = @(foreach ($g in @(Get-NetFirewallRule -DisplayName 'Nevermind : bloque *' -ErrorAction SilentlyContinue)) {
        $prog = [string](Get-NetFirewallApplicationFilter -AssociatedNetFirewallRule $g -ErrorAction SilentlyContinue).Program
        $svc = [string](Get-NetFirewallServiceFilter -AssociatedNetFirewallRule $g -ErrorAction SilentlyContinue).Service
        "$($g.DisplayName) / $($g.Direction) / $($g.Action) / actif $($g.Enabled) / profil $($g.Profile) / $prog / service $svc"
    })
    $etat['pare-feu|règles de Nevermind'] = if ($regles.Count) { (@($regles | Sort-Object) -join ' ; ') } else { 'aucune' }
    $etat['pare-feu|profils'] = (@(Get-NetFirewallProfile -ErrorAction SilentlyContinue | Sort-Object Name | ForEach-Object { "$($_.Name) $($_.Enabled)" }) -join ' ; ')
    $pc = Join-Path ([Environment]::GetFolderPath('System')) 'powercfg.exe'
    $etat['alimentation|plan actif'] = [regex]::Match((& $pc /getactivescheme | Out-String), '[0-9a-fA-F]{8}(-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}').Value
    $etat['alimentation|plans'] = (@([regex]::Matches((& $pc /list | Out-String), '[0-9a-fA-F]{8}(-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}') | ForEach-Object { $_.Value } | Sort-Object) -join ' ')
    foreach ($t in @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskPath -notlike '\Mindset\*' -and $_.TaskPath -notlike '\MindsetEssai\*' })) {
        $etat["tâche|$($t.TaskPath)$($t.TaskName)"] = if ($t.State -eq 'Disabled') { 'désactivée' } else { 'active' }
    }
    # Ce que l'Aide retient elle-même (HKLM\SOFTWARE\Mindset\Nevermind\Aide) : comparé d'une Aide à l'autre, pas à « avant »
    $racineMemoire = 'SOFTWARE\Mindset\Nevermind\Aide'
    $pile = New-Object Collections.Generic.Stack[string]
    $pile.Push($racineMemoire)
    while ($pile.Count) {
        $sous = $pile.Pop()
        $k = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($sous)
        if (-not $k) { continue }
        try {
            foreach ($n in $k.GetSubKeyNames()) { $pile.Push("$sous\$n") }
            foreach ($n in $k.GetValueNames()) {
                $v = [string]$k.GetValue($n, $null, 'DoNotExpandEnvironmentNames')
                $etat["mémoire de l'Aide|$($sous.Substring($racineMemoire.Length))|$n"] = if ($v.StartsWith('{')) { ConvertTo-JsonCanonique $v } else { $v }
            }
        } finally { $k.Close() }
    }
    $etat
}
# Les différences entre deux états (-SansMemoire : seulement la machine)
function Compare-Etats($A, $B, [switch]$SansMemoire) {
    $cles = @(@($A.Keys) + @($B.Keys) | Sort-Object -Unique)
    @(foreach ($c in $cles) {
        if ($SansMemoire -and $c -like 'mémoire de l''Aide|*') { continue }
        $x = if ($A.Contains($c)) { [string]$A[$c] } else { '(rien)' }
        $y = if ($B.Contains($c)) { [string]$B[$c] } else { '(rien)' }
        if ($x -cne $y) { "$c : « $x » puis « $y »" }
    })
}
function Show-Differences([string]$Titre, $Differences) {
    if (-not @($Differences).Count) { return }
    Write-Host "         | $Titre :" -ForegroundColor DarkGray
    foreach ($d in @($Differences) | Select-Object -First 40) { Write-Host "         |   $d" -ForegroundColor DarkGray }
}
# Les réponses de l'Aide, en textes (« ok » ou le message de refus, mot pour mot)
function Get-Reponses($S, $Actions) {
    $r = [ordered]@{}
    foreach ($a in $Actions) {
        $n = [string]$a.Nom
        $r[$n] = if ($S.R -and $S.R.PSObject.Properties[$n]) { "fait : $(ConvertTo-Json -InputObject $S.R.$n -Compress -Depth 4)" }
                 elseif ($S.E -and $S.E.PSObject.Properties[$n]) { "refusé : $($S.E.$n)" } else { 'pas de réponse' }
    }
    $r
}
# Les lectures du PC et de l'antivirus, résumées : ce qui ne bouge pas d'une minute à l'autre (pas les températures)
function Get-ResumeDivers($S) {
    $r = [ordered]@{}
    $pc = if ($S.R -and $S.R.PSObject.Properties['pc']) { $S.R.pc }
    $r['lecture.pc'] = if ($pc) {
        $disques = @(if ($pc.Fiabilite) { $pc.Fiabilite.PSObject.Properties | Sort-Object Name | ForEach-Object { "$($_.Name) [$(@($_.Value.PSObject.Properties | ForEach-Object { $_.Name } | Sort-Object) -join ',')]" } })
        "Secure Boot $($pc.SecureBoot) ; TPM lu $($pc.TpmOk) ; TPM $($pc.Tpm.SpecVersion) ; disques $($disques -join ' ; ')"
    } else { "pas de réponse : $($S.E.pc)" }
    $se = if ($S.R -and $S.R.PSObject.Properties['securite']) { $S.R.securite }
    $r['lecture.securite'] = if ($se) { "exclusions $($se.ExclusionsNombre) ; détections lues $($se.DetectionsLues) ; menaces $(@($se.Menaces).Count)" } else { "pas de réponse : $($S.E.securite)" }
    foreach ($n in 'exclusions', 'smb1') {
        $r[$n] = if ($S.R -and $S.R.PSObject.Properties[$n]) { "fait : $(ConvertTo-Json -InputObject $S.R.$n -Compress)" } elseif ($S.E -and $S.E.PSObject.Properties[$n]) { "refusé : $($S.E.$n)" } else { 'pas de réponse' }
    }
    $r
}
function Invoke-Passage([string]$Exe, [string]$Titre) {
    Write-Host "  -- $Titre" -ForegroundColor Cyan
    Reset-Machine
    $e = Get-Compte $NomElodie
    $null = Install-Appli $InstallateurNeuf $PaquetEssai $e.Sid
    if ($Exe) { Stop-Aides; Copy-Item -LiteralPath $Exe -Destination "$NevermindC\NevermindAide.exe" -Force }
    $empreinte = (Get-FileHash -LiteralPath "$NevermindC\NevermindAide.exe" -Algorithm SHA256).Hash
    Write-Host "         | NevermindAide.exe installé : $(if ($Exe) { $Exe } else { 'celui des paquets' }) (SHA-256 $empreinte)" -ForegroundColor DarkGray
    $carte = Get-CarteEssai
    $services = Get-ServicesEssai
    Write-Host "         | carte $($carte.Nom) ($($carte.Index)) ; services $(@($services.Telemetrie) + @($services.Memoire) -join ', ')" -ForegroundColor DarkGray
    $avant = Get-EtatMachine $carte $services

    # Tout optimiser (et des demandes que l'Aide doit refuser, pour comparer ses messages)
    $optim = [Collections.Generic.List[object]]::new()
    $i = 0
    foreach ($r in $RegEssai) { $i++; $optim.Add(@{ Nom = "reg$i"; Action = 'reg.ecrire'; P = @{ Cle = $r[0]; Nom = $r[1]; Type = $r[2]; Valeur = $r[3] } }) }
    $optim.Add(@{ Nom = 'service-telemetrie'; Action = 'service.regler'; P = @{ Nom = $services.Telemetrie; Demarrage = 'Disabled'; Arreter = $true } })
    foreach ($n in $services.Memoire) { $optim.Add(@{ Nom = "service-$n"; Action = 'service.regler'; P = @{ Nom = $n; Demarrage = 'Manual'; Arreter = $true } }) }
    $optim.Add(@{ Nom = 'dns'; Action = 'dns.regler'; P = @{ Index = $carte.Index; Serveurs = @('1.1.1.1', '1.0.0.1') } })
    $optim.Add(@{ Nom = 'pare-feu'; Action = 'pare-feu.bloquer'; P = @{ Nom = 'Nevermind : bloque Dormeur'; Programme = "$Racine\appli\Dormeur.exe" } })
    $refus = @(
        @{ Nom = 'refus-uac'; Action = 'reg.ecrire'; P = @{ Cle = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'; Nom = 'EnableLUA'; Type = 'DWord'; Valeur = 0 } }
        @{ Nom = 'refus-cle'; Action = 'reg.ecrire'; P = @{ Cle = 'HKLM:\SOFTWARE\EssaiNevermind'; Nom = 'X'; Type = 'DWord'; Valeur = 1 } }
        @{ Nom = 'refus-type'; Action = 'reg.ecrire'; P = @{ Cle = $RegEssai[0][0]; Nom = $RegEssai[0][1]; Type = 'String'; Valeur = '2' } }
        @{ Nom = 'refus-windows'; Action = 'reg.ecrire'; P = @{ Cle = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\lsass.exe\PerfOptions'; Nom = 'CpuPriorityClass'; Type = 'DWord'; Valeur = 3 } }
        @{ Nom = 'refus-temps-reel'; Action = 'reg.ecrire'; P = @{ Cle = $RegEssai[7][0]; Nom = 'CpuPriorityClass'; Type = 'DWord'; Valeur = 4 } }
        @{ Nom = 'refus-service-desactive'; Action = 'service.regler'; P = @{ Nom = 'gupdate'; Demarrage = 'Disabled' } }
        @{ Nom = 'refus-service'; Action = 'service.regler'; P = @{ Nom = 'Spooler'; Demarrage = 'Manual' } }
        @{ Nom = 'refus-dns'; Action = 'dns.regler'; P = @{ Index = $carte.Index; Serveurs = @('6.6.6.6') } }
        @{ Nom = 'refus-pare-feu'; Action = 'pare-feu.bloquer'; P = @{ Nom = 'Nevermind : bloque svchost'; Programme = (Join-Path ([Environment]::GetFolderPath('System')) 'svchost.exe') } }
        @{ Nom = 'refus-inconnue'; Action = 'reg.tout'; P = @{} }
    )
    foreach ($x in $refus) { $optim.Add($x) }
    $s1 = Invoke-Client $e @{ Mode = 'appel'; SansDemande = $true; Actions = @($optim) }
    Start-Sleep -Seconds 2
    $apres = Get-EtatMachine $carte $services

    # Tout restaurer : les valeurs d'avant, dans l'ordre de Nevermind (Restore-AllSettings)
    $rest = [Collections.Generic.List[object]]::new()
    $rest.Add(@{ Nom = 'pare-feu'; Action = 'pare-feu.debloquer-tout'; P = @{} })
    $i = 0
    foreach ($r in $RegEssai) {
        $i++
        $t = $avant["registre|$($r[0])|$($r[1])"]   # (« absente », ou « Type=valeur »)
        if ($t -eq 'absente') { $rest.Add(@{ Nom = "reg$i"; Action = 'reg.effacer'; P = @{ Cle = $r[0]; Nom = $r[1] } }); continue }
        $type, $texte = $t -split '=', 2
        $valeur = if ($type -eq 'Binary') { @($texte -split ' ' | ForEach-Object { [Convert]::ToInt32($_, 16) }) } else { [long]$texte }
        $rest.Add(@{ Nom = "reg$i"; Action = 'reg.ecrire'; P = @{ Cle = $r[0]; Nom = $r[1]; Type = $type; Valeur = $valeur } })
    }
    $nomsDemarrage = @{ '2' = 'Automatic'; '3' = 'Manual'; '4' = 'Disabled' }
    foreach ($n in @($services.Telemetrie) + @($services.Memoire)) {
        $d = [regex]::Match($avant["service|$n"], 'démarrage DWord=(\d)').Groups[1].Value
        $lancer = $n -eq $services.Telemetrie -and $avant["service en marche|$n"] -eq 'Running'
        $rest.Add(@{ Nom = "service-$n"; Action = 'service.regler'; P = @{ Nom = $n; Demarrage = [string]$nomsDemarrage[$d]; Lancer = $lancer } })
    }
    $dnsAvant = $avant["dns|Tcpip|$($carte.Nom)"]
    $serveurs = if ($dnsAvant -like 'String=?*') { @($dnsAvant.Substring(7) -split '[,\s]+' | Where-Object { $_ }) } else { @() }
    $rest.Add(@{ Nom = 'dns'; Action = 'dns.regler'; P = @{ Index = $carte.Index; Serveurs = $serveurs } })
    $rest.Add(@{ Nom = 'oublier'; Action = 'origines.oublier'; P = @{} })
    $s2 = Invoke-Client $e @{ Mode = 'appel'; SansDemande = $true; Actions = @($rest) }
    Start-Sleep -Seconds 2
    $remis = Get-EtatMachine $carte $services

    # Lectures du PC et actions définitives (« Tout restaurer » ne les annule pas) : santé des disques, TPM, Secure Boot,
    # antivirus (une exclusion d'essai ajoutée ici, retirée par l'Aide), SMB1 coupé côté serveur
    $exclusion = Join-Path $Racine 'exclusion-essai'
    $defender = $false
    try { Add-MpPreference -ExclusionPath $exclusion -ErrorAction Stop; $defender = @((Get-MpPreference -ErrorAction Stop).ExclusionPath) -contains $exclusion } catch { Write-Host "         | antivirus de Windows pas réglable ici : $($_.Exception.Message)" -ForegroundColor DarkGray }
    $divers = @(
        @{ Nom = 'pc'; Action = 'lecture.pc'; P = @{} }
        @{ Nom = 'securite'; Action = 'lecture.securite'; P = @{} }
        @{ Nom = 'exclusions'; Action = 'antivirus.retirer-exclusions'; P = @{ Elements = @($exclusion) } }
        @{ Nom = 'smb1'; Action = 'smb1.couper'; P = @{ Client = $false } }
    )
    $s3 = Invoke-Client $e @{ Mode = 'appel'; SansDemande = $true; Actions = @($divers) }
    $resteExclusion = $null; try { $resteExclusion = @((Get-MpPreference -ErrorAction Stop).ExclusionPath) -contains $exclusion } catch {}
    $smb1 = ''; try { $smb1 = [string](Get-SmbServerConfiguration -ErrorAction Stop).EnableSMB1Protocol } catch {}
    Stop-Aides
    [pscustomobject]@{
        Titre = $Titre; Empreinte = $empreinte; Avant = $avant; Apres = $apres; Remis = $remis; Services = $services; Carte = $carte
        Optim = Get-Reponses $s1 @($optim); Restauration = Get-Reponses $s2 @($rest); Refus = @($refus | ForEach-Object { $_.Nom })
        Divers = Get-ResumeDivers $s3; Defender = $defender; ResteExclusion = $resteExclusion; Smb1 = $smb1
    }
}
function Test-Cas10 {
    Write-Partie '10. Tout optimiser puis tout restaurer : la machine revient à son état, et deux NevermindAide.exe font pareil'
    $liste = @($Aides -split ';' | Where-Object { $_ })
    if (-not $liste.Count) { $liste = @('', '') }       # (celui des paquets, deux fois)
    elseif ($liste.Count -eq 1) { $liste += $liste[0] }
    $passages = New-Object Collections.Generic.List[object]
    $n = 0
    foreach ($x in $liste) {
        $n++
        $p = @(Invoke-Passage $x "passage $n : $(if ($x) { $x } else { 'NevermindAide.exe des paquets' })")[-1]
        $passages.Add($p)
        Write-Utf8 (Join-Path $Sorties "optimiser-restaurer-$n.json") ($p | ConvertTo-Json -Depth 6)
        $t = "passage $n"
        $refuses = @($p.Refus)
        Test-Cas "$t, optimiser : chaque demande permise est faite, chaque demande interdite refusée" {
            foreach ($k in $p.Optim.Keys) { Write-Host "         |   $k : $($p.Optim[$k])" -ForegroundColor DarkGray }
            -not @($p.Optim.Keys | Where-Object { ($refuses -contains $_) -ne ($p.Optim[$_] -like 'refusé : *') -or ($refuses -notcontains $_ -and $p.Optim[$_] -ne 'fait : "ok"') }).Count
        }
        Test-Cas "$t, après l'optimisation : chaque réglage a la valeur demandée (registre, services, DNS, pare-feu)" {
            $faux = @()
            foreach ($r in $RegEssai) {
                $v = $p.Apres["registre|$($r[0])|$($r[1])"]
                $attendu = if ($r[2] -eq 'Binary') { "Binary=$((@($r[3]) | ForEach-Object { ([int]$_).ToString('X2') }) -join ' ')" } else { "$($r[2])=$($r[3])" }
                if ($v -ne $attendu) { $faux += "$($r[1]) : $v au lieu de $attendu" }
            }
            if ($p.Apres["service|$($p.Services.Telemetrie)"] -notlike 'démarrage DWord=4 ;*') { $faux += "télémétrie : $($p.Apres["service|$($p.Services.Telemetrie)"])" }
            if ($p.Apres["service en marche|$($p.Services.Telemetrie)"] -ne 'Stopped') { $faux += "télémétrie en marche : $($p.Apres["service en marche|$($p.Services.Telemetrie)"])" }
            foreach ($s in $p.Services.Memoire) { if ($p.Apres["service|$s"] -notlike 'démarrage DWord=3 ;*') { $faux += "$s : $($p.Apres["service|$s"])" } }
            if ($p.Apres["dns|Tcpip|$($p.Carte.Nom)"] -notmatch '^String=1\.1\.1\.1[, ]1\.0\.0\.1$') { $faux += "DNS : $($p.Apres["dns|Tcpip|$($p.Carte.Nom)"])" }
            if (@(([string]$p.Apres['pare-feu|règles de Nevermind']) -split ' ; ' | Where-Object { $_ -like 'Nevermind : bloque Dormeur / *' -and $_ -like '* / Block / *' }).Count -ne 2) { $faux += "pare-feu : $($p.Apres['pare-feu|règles de Nevermind'])" }
            foreach ($f in $faux) { Write-Host "         |   $f" -ForegroundColor DarkGray }
            -not $faux.Count
        }
        Test-Cas "$t, l'Aide retient les valeurs d'origine (une par réglage du registre) et un DNS d'origine" {
            @($p.Apres.Keys | Where-Object { $_ -like 'mémoire de l''Aide|\Origines|*' }).Count -eq $RegEssai.Count -and @($p.Apres.Keys | Where-Object { $_ -like 'mémoire de l''Aide|\Dns|*' }).Count -eq 1
        }
        Test-Cas "$t, restaurer : chaque demande est faite" {
            foreach ($k in $p.Restauration.Keys) { Write-Host "         |   $k : $($p.Restauration[$k])" -ForegroundColor DarkGray }
            -not @($p.Restauration.Keys | Where-Object { $p.Restauration[$_] -ne 'fait : "ok"' }).Count
        }
        $d = Compare-Etats $p.Avant $p.Remis -SansMemoire
        Show-Differences 'pas revenu' $d
        Test-Cas "$t, après la restauration : la machine est revenue à son état d'avant ($($p.Avant.Count) points relevés : registre, services, DNS, pare-feu, plans d'alimentation, tâches)" { -not $d.Count }
        Test-Cas "$t, après la restauration : les valeurs d'origine sont oubliées" { -not @($p.Remis.Keys | Where-Object { $_ -like 'mémoire de l''Aide|\Origines|*' }).Count }
        Test-Cas "$t, lectures du PC (disques, TPM, Secure Boot) et de l'antivirus rendues, exclusion d'essai retirée, SMB1 coupé côté serveur" {
            foreach ($k in $p.Divers.Keys) { Write-Host "         |   $k : $($p.Divers[$k])" -ForegroundColor DarkGray }
            Write-Host "         |   exclusion d'essai ajoutée $($p.Defender), encore là après $($p.ResteExclusion) ; SMB1 serveur $($p.Smb1)" -ForegroundColor DarkGray
            $p.Divers['lecture.pc'] -notlike 'pas de réponse*' -and $p.Divers['lecture.securite'] -notlike 'pas de réponse*' -and $p.Divers['exclusions'] -eq 'fait : "ok"' -and
                $p.Divers['smb1'] -eq 'fait : "ok"' -and ($p.Smb1 -in '', 'False') -and (-not $p.Defender -or $p.ResteExclusion -eq $false)
        }
        if (-not $p.Defender) { Add-Note "$t : l'antivirus de Windows n'a pas pris l'exclusion d'essai, son retrait n'est pas vérifié ici" }
        if ($n -gt 1) {
            $a = $passages[0]
            $comparaisons = @(
                @('la machine de départ', (Compare-Etats $a.Avant $p.Avant -SansMemoire)),
                @('l''état après l''optimisation (mémoire de l''Aide comprise)', (Compare-Etats $a.Apres $p.Apres)),
                @('l''état après la restauration (mémoire de l''Aide comprise)', (Compare-Etats $a.Remis $p.Remis)),
                @('les réponses à l''optimisation (mot pour mot)', (Compare-Etats $a.Optim $p.Optim)),
                @('les réponses à la restauration (mot pour mot)', (Compare-Etats $a.Restauration $p.Restauration)),
                @('les lectures du PC et de l''antivirus, et les réponses aux actions définitives', (Compare-Etats $a.Divers $p.Divers))
            )
            foreach ($c in $comparaisons) {
                Show-Differences "différent du passage 1" $c[1]
                Test-Cas "passage $n comparé au passage 1 : même $($c[0])" { -not @($c[1]).Count }
            }
        }
    }
    if (($liste | Select-Object -Unique).Count -lt 2 -or -not $liste[0]) { Add-Note 'un seul NevermindAide.exe comparé à lui-même (deux passages) : la comparaison entre deux Aides viendra avec l''exe Rust' }
}

# =====================================================================================================================
# 11. Le banc PowerShell / Rust (outils\tester-aide-rust.ps1, du dépôt de l'Aide en Rust) sous un compte sans droits
# =====================================================================================================================
function Test-Cas11 {
    Write-Partie '11. Banc PowerShell / Rust sous un compte sans droits (mêmes demandes sur le vrai canal, mêmes réponses)'
    if (-not $Banc -or -not (Test-Path -LiteralPath $Banc -PathType Leaf) -or -not $Aide -or -not (Test-Path -LiteralPath $Aide -PathType Leaf)) {
        Add-Note "pas de banc à lancer (banc « $Banc », exe « $Aide »)"; return
    }
    Stop-Aides
    $b = Get-Compte 'Banc Essai'
    # Le banc et ce qu'il lit (outils, cas, Nevermind\fichiers), copiés dans C:\essai que tout le monde lit
    $source = Split-Path (Split-Path ([IO.Path]::GetFullPath($Banc)) -Parent) -Parent
    $dest = Join-Path $Racine 'banc'
    if (Test-Path -LiteralPath $dest) { Remove-Item -LiteralPath $dest -Recurse -Force }
    New-Item -ItemType Directory -Force -Path "$dest\Nevermind" | Out-Null
    Copy-Item -LiteralPath (Join-Path $source 'outils') -Destination $dest -Recurse
    Copy-Item -LiteralPath (Join-Path $source 'cas') -Destination $dest -Recurse
    Copy-Item -LiteralPath (Join-Path $source 'Nevermind\fichiers') -Destination "$dest\Nevermind" -Recurse
    Copy-Item -LiteralPath $Aide -Destination "$dest\NevermindAide.exe"
    $sortie = Join-Path $Sorties 'banc.txt'; $erreurs = Join-Path $Sorties 'banc-erreurs.txt'
    $p = Start-Process -FilePath $PowerShellExe -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "$dest\outils\$(Split-Path $Banc -Leaf)", '-Exe', "$dest\NevermindAide.exe" `
        -Credential $b.Cred -LoadUserProfile -WorkingDirectory $dest -RedirectStandardOutput $sortie -RedirectStandardError $erreurs -PassThru
    $null = $p.Handle   # (sans ce handle gardé, PowerShell 5.1 ne rend pas le code de sortie)
    $fini = $p.WaitForExit(1800000)
    if (-not $fini) { try { $p.Kill() } catch {} }
    foreach ($f in $sortie, $erreurs) { foreach ($l in @(Get-Content -LiteralPath $f -Encoding Oem -ErrorAction SilentlyContinue)) { Write-Host "         | $l" -ForegroundColor DarkGray } }
    Test-Cas "banc lancé sous « Banc Essai » (sans droits) : PowerShell et Rust répondent pareil (code $(if ($fini) { $p.ExitCode } else { 'trop long' }))" { $fini -and $p.ExitCode -eq 0 }
    Stop-Aides
}

# =====================================================================================================================
# 13. Relancer la carte réseau de la connexion (carte.relancer) : pour de vrai, sur la machine de GitHub
# =====================================================================================================================
# Les états de la carte, relevés de l'extérieur pendant une demande, dès avant elle : un fil natif (C#) qui lit l'état
# de la carte toutes les 20 ms (NetworkInterface, sans passer par CIM : Get-NetAdapter prend plus de 100 ms par lecture
# et a manqué une relance de moins d'une seconde, run 40527121588). Chaque changement est noté avec son heure (ms
# depuis le début du guet). La preuve que l'Aide l'a bien éteinte puis rallumée, ou qu'elle n'y a pas touché.
function Start-GuetCarte([string]$Guid) {
    if (-not ('GuetCarte' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Net.NetworkInformation;
using System.Threading;
public sealed class GuetCarte
{
    readonly string guid;
    readonly List<string> etats = new List<string>();
    volatile bool fin;
    readonly Thread fil;
    public int Lectures;
    public GuetCarte(string guid)
    {
        this.guid = guid;
        fil = new Thread(Boucle) { IsBackground = true };
        fil.Start();
    }
    void Boucle()
    {
        var montre = Stopwatch.StartNew();
        string dernier = null;
        while (!fin)
        {
            string e;
            try
            {
                var c = NetworkInterface.GetAllNetworkInterfaces().FirstOrDefault(n => string.Equals(n.Id, guid, StringComparison.OrdinalIgnoreCase));
                e = c == null ? "absente" : c.OperationalStatus.ToString();
            }
            catch (Exception x) { e = "illisible (" + x.GetType().Name + ")"; }
            Lectures++;
            if (e != dernier) { lock (etats) etats.Add(e + "@" + montre.ElapsedMilliseconds + "ms"); dernier = e; }
            Thread.Sleep(20);
        }
    }
    public string[] Arreter() { fin = true; fil.Join(2000); lock (etats) return etats.ToArray(); }
}
'@
    }
    New-Object GuetCarte $Guid
}
function Stop-GuetCarte($G) { , @($G.Arreter()) }

# Les signes qui restent après coup, relevés avant et après une demande : les nouvelles lignes des journaux de Windows
# (système, profil réseau, NCSI, Plug and Play) et les compteurs de la carte (une carte relancée repart de zéro ?)
$JournauxCarte = @('System', 'Microsoft-Windows-NetworkProfile/Operational', 'Microsoft-Windows-NCSI/Operational', 'Microsoft-Windows-Kernel-PnP/Configuration', 'Microsoft-Windows-Kernel-PnP/Device Configuration')
function Get-ReperesCarte([string]$Nom) {
    $r = @{ Journaux = @{}; Octets = $null }
    foreach ($j in $JournauxCarte) {
        $r.Journaux[$j] = try { [long](Get-WinEvent -LogName $j -MaxEvents 1 -ErrorAction Stop).RecordId } catch { [long]0 }
    }
    try { $r.Octets = [long](Get-NetAdapterStatistics -Name $Nom -ErrorAction Stop).ReceivedBytes } catch {}
    $r
}
# Ce qui a changé depuis les repères : les nouvelles lignes des journaux (« journal / fournisseur / numéro : début du
# texte ») et les octets reçus avant et après
function Get-TracesCarte($Reperes, [string]$Nom) {
    $lignes = @(foreach ($j in $JournauxCarte) {
        $depuis = $Reperes.Journaux[$j]
        try {
            foreach ($ev in @(Get-WinEvent -LogName $j -MaxEvents 200 -ErrorAction Stop | Where-Object { $_.RecordId -gt $depuis } | Sort-Object RecordId)) {
                $texte = ([string]$ev.Message -replace '\s+', ' ')
                "$j / $($ev.ProviderName) / $($ev.Id) : $($texte.Substring(0, [math]::Min(160, $texte.Length)))"
            }
        } catch {}
    })
    $octets = $null; try { $octets = [long](Get-NetAdapterStatistics -Name $Nom -ErrorAction Stop).ReceivedBytes } catch {}
    @{ Lignes = $lignes; OctetsAvant = $Reperes.Octets; OctetsApres = $octets }
}
function Show-TracesCarte([string]$Titre, $T) {
    Write-Host "         | $Titre : octets reçus $($T.OctetsAvant) avant, $($T.OctetsApres) après ; $(@($T.Lignes).Count) nouvelle(s) ligne(s) de journal" -ForegroundColor DarkGray
    foreach ($l in @($T.Lignes) | Select-Object -First 40) { Write-Host "         |   $l" -ForegroundColor DarkGray }
}
# La connexion est là : la carte allumée, sa route par défaut, et un vrai échange avec GitHub (TCP 443)
function Test-ConnexionCarte([int]$Index) {
    $a = Get-NetAdapter -InterfaceIndex $Index -ErrorAction SilentlyContinue
    if (-not $a -or [string]$a.Status -ne 'Up') { return $false }
    if (-not @(Get-NetRoute -InterfaceIndex $Index -DestinationPrefix '0.0.0.0/0' -PolicyStore ActiveStore -ErrorAction SilentlyContinue).Count) { return $false }
    $t = New-Object Net.Sockets.TcpClient
    try { $t.ConnectAsync('github.com', 443).Wait(3000) -and $t.Connected } catch { $false } finally { $t.Dispose() }
}
function Test-Cas13 {
    Write-Partie '13. Relancer la carte réseau de la connexion : l''Aide la relance pour de vrai, la connexion revient ; tout le reste est refusé'
    Reset-Machine
    $e = Get-Compte $NomElodie
    $null = Install-Appli $InstallateurNeuf $PaquetEssai $e.Sid
    $carte = Get-CarteEssai
    foreach ($a in @(Get-NetAdapter -IncludeHidden -ErrorAction SilentlyContinue | Sort-Object ifIndex)) {
        Write-Host "         | carte $($a.ifIndex) « $($a.Name) » : $($a.InterfaceDescription) ; $($a.Status) ; matérielle $($a.HardwareInterface) ; prise $($a.ConnectorPresent) ; type $($a.InterfaceType) ; support $($a.NdisPhysicalMedium) ; vitesse $($a.LinkSpeed)$(if ($a.ifIndex -eq $carte.Index) { ' ; CELLE DE LA CONNEXION' })" -ForegroundColor DarkGray
    }
    Test-Cas "la connexion marche avant l'essai (carte $($carte.Nom), numéro $($carte.Index))" { Test-ConnexionCarte $carte.Index }

    # a. Les refus : numéros mal formés, carte absente, chaque autre carte de la machine. Aucune carte ne bouge.
    $refus = [Collections.Generic.List[object]]::new()
    $refus.Add(@{ Nom = 'zero'; Action = 'carte.relancer'; P = @{ Index = 0 } })
    $refus.Add(@{ Nom = 'negatif'; Action = 'carte.relancer'; P = @{ Index = -1 } })
    $refus.Add(@{ Nom = 'texte'; Action = 'carte.relancer'; P = @{ Index = "$($carte.Index)" } })
    $refus.Add(@{ Nom = 'mot'; Action = 'carte.relancer'; P = @{ Index = 'Ethernet' } })
    $refus.Add(@{ Nom = 'absente'; Action = 'carte.relancer'; P = @{ Index = 999999 } })
    foreach ($a in @(Get-NetAdapter -IncludeHidden -ErrorAction SilentlyContinue | Where-Object { [int]$_.ifIndex -ne $carte.Index } | Select-Object -First 12)) {
        $refus.Add(@{ Nom = "autre-$($a.ifIndex)"; Action = 'carte.relancer'; P = @{ Index = [int]$a.ifIndex } })
    }
    $reperes1 = Get-ReperesCarte $carte.Nom
    $g = Start-GuetCarte $carte.Guid
    Start-Sleep -Milliseconds 300   # (le guet a déjà lu la carte avant la première demande)
    $s1 = Invoke-Client $e @{ Mode = 'appel'; SansDemande = $true; Actions = @($refus) }
    Start-Sleep -Milliseconds 500
    $lectures1 = $g.Lectures
    $etats1 = Stop-GuetCarte $g
    Show-TracesCarte 'pendant les refus (contre-épreuve)' (Get-TracesCarte $reperes1 $carte.Nom)
    Write-Host "         | guet pendant les refus : $lectures1 lectures ; états vus : $($etats1 -join ', ')" -ForegroundColor DarkGray
    Test-Cas "chaque demande interdite est refusée par la liste de l'Aide ($($refus.Count) demandes : numéros mal formés, carte absente, les autres cartes)" {
        -not @($refus | Where-Object { -not ($s1.E -and $s1.E.PSObject.Properties[$_.Nom] -and [string]$s1.E.($_.Nom) -like 'Demande refusée*') -or ($s1.R -and $s1.R.PSObject.Properties[$_.Nom]) }).Count
    }
    Test-Cas "pendant ces refus, la carte de la connexion n'a pas bougé (guet toutes les 20 ms : $($etats1 -join ', '))" { $etats1.Count -eq 1 -and $etats1[0] -like 'Up@*' -and $lectures1 -gt 20 }

    # b. La carte de la connexion : relancée pour de vrai, puis la connexion revient
    $avant = (Get-NetAdapter -InterfaceIndex $carte.Index).LinkSpeed
    $reperes2 = Get-ReperesCarte $carte.Nom
    $g = Start-GuetCarte $carte.Guid
    Start-Sleep -Milliseconds 300
    $debut = Get-Date
    $s2 = Invoke-Client $e @{ Mode = 'appel'; SansDemande = $true; Actions = @(@{ Nom = 'relancer'; Action = 'carte.relancer'; P = @{ Index = $carte.Index } }) }
    $revenue = Wait-Jusque { Test-ConnexionCarte $carte.Index } 60
    $duree = [math]::Round(((Get-Date) - $debut).TotalSeconds, 1)
    $lectures2 = $g.Lectures
    $etats2 = Stop-GuetCarte $g
    $apres = (Get-NetAdapter -InterfaceIndex $carte.Index -ErrorAction SilentlyContinue).LinkSpeed
    Show-TracesCarte 'pendant la relance' (Get-TracesCarte $reperes2 $carte.Nom)
    Write-Host "         | vitesse avant $avant, après $apres ; guet pendant la relance : $lectures2 lectures ; états vus : $($etats2 -join ', ')" -ForegroundColor DarkGray
    Test-Cas 'l''Aide accepte de relancer la carte de la connexion (réponse « ok »)' { $s2.R -and $s2.R.PSObject.Properties['relancer'] -and [string]$s2.R.relancer -eq 'ok' }
    Test-Cas "la carte a bien été éteinte puis rallumée (guet toutes les 20 ms : $($etats2 -join ', '))" { $etats2[0] -like 'Up@*' -and @($etats2 | Where-Object { $_ -notlike 'Up@*' }).Count -gt 0 -and $etats2[-1] -like 'Up@*' }
    Test-Cas "la connexion revient en 60 s au plus (revenue en $duree s depuis la demande)" { $revenue }
    Stop-Aides
}

# =====================================================================================================================
$titres = @{ 0 = 'Préparation'; 1 = 'Installation : tâche SYSTEM pour Élodie'; 2 = 'Élodie obtient des actions en lecture seule'; 3 = 'Un autre compte refusé'
    4 = 'Mise à jour depuis Nevermind 1.10.0'; 5 = 'Repli par la demande de Windows'; 6 = 'Désinstallation'; 7 = 'Environnement d''Élodie'
    8 = 'Changement de nom : mise à jour par le nouveau Mindset, une seule instance'; 9 = 'Aide lancée en secours par la demande de Windows'
    10 = 'Tout optimiser puis tout restaurer'; 11 = 'Banc PowerShell / Rust sous un compte sans droits'; 13 = 'Relancer la carte réseau de la connexion' }
if (-not $titres.ContainsKey($Cas)) { Write-Host 'Indique -Cas 0 (préparation), -Cas 1 à 11 ou -Cas 13.' -ForegroundColor Yellow; exit 2 }
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
        8 { Test-Cas8 }
        9 { Test-Cas9 }
        10 { Test-Cas10 }
        11 { Test-Cas11 }
        13 { Test-Cas13 }
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
