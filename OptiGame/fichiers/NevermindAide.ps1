#Requires -Version 5.1
<#
    Nevermind Aide : la seule partie de Nevermind qui tourne en administrateur.
    Lancée par NevermindAide.exe, depuis la tâche planifiée « \Mindset\Nevermind (aide) » que Mindset crée à
    l'installation (donc sans demande de Windows). Le programme est dans le dossier protégé de Mindset : seul un
    administrateur peut le modifier.
    La tâche tourne en SYSTEM : elle ne reprend rien de l'utilisateur (ni ses variables d'environnement, ni ses modules).
    L'utilisateur pour qui elle travaille est inscrit dans ses arguments (-Sid) par l'administrateur qui l'a créée, jamais
    déduit d'autre chose. Si la tâche manque ou ne répond pas, Nevermind passe par la demande de Windows (l'utilisateur
    clique « Oui ») : l'Aide tourne alors sous le compte qui a accepté, pour l'utilisateur -Sid.
    Aucune fenêtre (en SYSTEM, personne ne la verrait), et aucun dossier de l'utilisateur n'est utilisé.

    Elle écoute sur un canal nommé réservé à l'utilisateur (refusé depuis le réseau), une demande par connexion :
    une ligne JSON { a = action, p = paramètres }, une ligne JSON en réponse { ok, r } ou { ok = false, e = message }.
    Elle ne répond qu'au programme qui fait tourner Nevermind, Mindset.exe de ce même dossier, reconnu par son chemin
    complet (Test-AideClient), et ne fait que ce qui est dans modules\aide-actions.ps1.
    Elle s'arrête toute seule quand Nevermind n'est plus ouvert.

    NevermindAide.ps1 -Sid <utilisateur>  l'utilisateur servi (obligatoire en SYSTEM ; sinon, par défaut, le compte de l'Aide)
    NevermindAide.ps1 -Parent <pid>  (tests) : reste ouverte tant que ce processus vit, au lieu de chercher Nevermind.
#>
param([int]$Parent, [string]$Sid)
# Avant toute commande : les modules de PowerShell ne viennent que du dossier de Windows, chargés par leur chemin complet.
# La tâche planifiée donne à l'Aide les variables de l'utilisateur (PSModulePath compris) : sans ça, un programme sans
# droits pourrait glisser un faux module « ScheduledTasks » ou « NetAdapter » que l'Aide chargerait en administrateur.
# NevermindAide.exe remet aussi ces variables avant d'ouvrir PowerShell (outils\lanceur.cs) ; ceci couvre le reste
# (tests lancés par powershell.exe). Les dossiers de Windows sont lus par [Environment]::GetFolderPath, jamais dans $env:.
# (certains modules de Windows, comme Storage, chargent leurs fichiers depuis $env:windir : les variables des dossiers de
# Windows sont donc remises elles aussi, avant le premier module)
$AideDossierPS = [IO.Path]::Combine([Environment]::GetFolderPath('System'), 'WindowsPowerShell\v1.0\Modules')
$env:windir = [Environment]::GetFolderPath('Windows')
$env:SystemRoot = $env:windir
$env:ProgramFiles = [Environment]::GetFolderPath('ProgramFiles')
$env:ProgramW6432 = $env:ProgramFiles
${env:ProgramFiles(x86)} = [Environment]::GetFolderPath('ProgramFilesX86')
$env:ProgramData = [Environment]::GetFolderPath('CommonApplicationData')
$env:ALLUSERSPROFILE = $env:ProgramData
$env:PSModulePath = $AideDossierPS
$PSModuleAutoLoadingPreference = 'None'
# Les modules de base, tout de suite ; les modules lourds (réseau, pare-feu, disques...) seulement quand une demande s'en
# sert, toujours par leur chemin complet (Import-AideModule, modules\aide-actions.ps1)
$AideModulesWindows = [Collections.Generic.List[string]]::new()
foreach ($m in 'Microsoft.PowerShell.Management', 'Microsoft.PowerShell.Utility', 'Microsoft.PowerShell.Security', 'CimCmdlets') {
    $c = [IO.Path]::Combine($AideDossierPS, $m)
    Import-Module -Name $c -Global -ErrorAction Stop
    $AideModulesWindows.Add($c)
}
$ErrorActionPreference = 'Stop'
$AppDir = $PSScriptRoot
$Racine = Split-Path $AppDir -Parent

# Pour qui l'Aide travaille. En SYSTEM : seulement l'utilisateur inscrit dans la tâche (-Sid), sinon elle ne démarre pas.
# Lancée par la demande de Windows : l'utilisateur -Sid (celui qui l'a demandée : l'administrateur qui a accepté a vu
# la demande), ou à défaut son propre compte.
$moi = [Security.Principal.WindowsIdentity]::GetCurrent()
$admin = (New-Object Security.Principal.WindowsPrincipal($moi)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$EnSysteme = $moi.User.Value -eq 'S-1-5-18'
if ($Sid -cnotmatch '^S-1-5-21-\d+-\d+-\d+-\d+\z') {
    if ($EnSysteme) { exit }
    $Sid = $moi.User.Value
}
$Utilisateur = New-Object Security.Principal.SecurityIdentifier($Sid)

# Le dossier temporaire où PowerShell compile le code C# de l'Aide (Add-Type) : jamais le TEMP reçu (celui de SYSTEM,
# C:\Windows\Temp, où tout utilisateur peut créer des fichiers ; ou celui de l'utilisateur), mais aide-temp, dans le
# dossier protégé de Nevermind. Avec les droits administrateur, il est réservé au système et aux administrateurs.
# (En SYSTEM, les Windows récents donnent de toute façon C:\Windows\SystemTemp, réservé lui aussi, quoi que dise TEMP.)
$AideTemp = Join-Path $Racine 'aide-temp'
try {
    $dt = [IO.Directory]::CreateDirectory($AideTemp)
    if ($dt.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'lien' }
    if ($admin) {
        $acl = New-Object Security.AccessControl.DirectorySecurity
        $acl.SetAccessRuleProtection($true, $false)
        foreach ($s in 'S-1-5-18', 'S-1-5-32-544') {
            $acl.AddAccessRule((New-Object Security.AccessControl.FileSystemAccessRule((New-Object Security.Principal.SecurityIdentifier($s)), 'FullControl', 'ContainerInherit, ObjectInherit', 'None', 'Allow')))
        }
        $dt.SetAccessControl($acl)
    }
    $env:TEMP = $AideTemp; $env:TMP = $AideTemp
} catch {
    if ($admin) { exit }   # (avec les droits administrateur, rien n'est compilé ailleurs)
}

$FichierActions = Join-Path $AppDir 'modules\aide-actions.ps1'
. $FichierActions

# Journal gardé en mémoire (200 dernières lignes) : Nevermind le demande (« aide.journal ») et l'écrit dans le sien.
# L'Aide n'écrit aucun fichier (sauf la compilation dans aide-temp) : en administrateur, écrire dans un dossier
# modifiable sans droits permettrait à un programme d'y glisser un lien et de lui faire écrire ailleurs. Rien n'est
# réglable par variable d'environnement non plus.
$Lignes = [Collections.ArrayList]::Synchronized((New-Object Collections.ArrayList))
function Write-AideLog([string]$Message) {
    [void]$Lignes.Add("$(Get-Date -Format s) $Message")
    while ($Lignes.Count -gt 200) { $Lignes.RemoveAt(0) }
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;
public static class AideTuyau
{
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool GetNamedPipeClientProcessId(SafePipeHandle pipe, out uint pid);
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern bool QueryFullProcessImageName(IntPtr h, int flags, StringBuilder name, ref int size);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    public static int Client(SafePipeHandle pipe) { uint pid; return GetNamedPipeClientProcessId(pipe, out pid) ? (int)pid : 0; }
    public static string Chemin(int pid)
    {
        IntPtr h = OpenProcess(0x1000, false, pid);   // PROCESS_QUERY_LIMITED_INFORMATION
        if (h == IntPtr.Zero) return "";
        try { var sb = new StringBuilder(1024); int n = sb.Capacity; return QueryFullProcessImageName(h, 0, sb, ref n) ? sb.ToString() : ""; }
        finally { CloseHandle(h); }
    }
}

// Mesure des FPS pour Nevermind (PresentMon demande les droits administrateur) : PresentMon tourne ici, ses lignes sont
// gardées jusqu'à ce que Nevermind les lise (« fps.lire »), 20 000 au plus. « #ERR » : une erreur, « #FIN » : arrêté.
// Veille (« fps.veiller ») : certains anti-triche (BF6) refusent toute nouvelle mesure une fois le jeu lancé, mais
// laissent tourner celle qui existait avant. PresentMon est alors lancé avant le jeu, sur le nom de son programme, et
// la mesure du jeu (« fps.demarrer ») s'y rattache au lieu de repartir de zéro.
public static class AideFps
{
    static System.Diagnostics.Process proc;
    static string exePm = "";
    static readonly System.Collections.Concurrent.ConcurrentQueue<string> lignes = new System.Collections.Concurrent.ConcurrentQueue<string>();
    static volatile bool fin;
    static volatile string entete;          // première ligne de PresentMon (noms des colonnes), redonnée au rattachement
    static string[] veille = new string[0];  // programmes guettés (minuscules), vide hors veille
    static volatile bool rattachee, donnerEntete;

    public static void Start(string exe, int pid)
    {
        rattachee = true;
        Lancer(exe, "--process_id " + pid + " --terminate_on_proc_exit");
    }

    // Guette ces programmes (« bf6.exe ») avant leur lancement. Déjà en veille sur les mêmes : rien à faire.
    public static void Veiller(string exe, string[] noms)
    {
        var tri = (string[])noms.Clone(); Array.Sort(tri, StringComparer.OrdinalIgnoreCase);
        if (EnVeille() && string.Join("|", tri) == string.Join("|", veille)) return;
        var args = new StringBuilder();
        foreach (var n in tri) args.Append("--process_name \"").Append(n).Append("\" ");
        rattachee = false;
        Lancer(exe, args.ToString().Trim());
        veille = tri;
    }

    // Veille en cours, pas encore prise par un jeu
    public static bool EnVeille() { var p = proc; return veille.Length > 0 && !rattachee && p != null && !fin; }

    // Le jeu (son programme) est-il guetté par la veille en cours ? Si oui, la mesure s'y rattache.
    public static bool Rattacher(string nom)
    {
        if (!EnVeille() || Array.IndexOf(veille, nom.ToLowerInvariant()) < 0) return false;
        donnerEntete = entete != null;
        rattachee = true;
        return true;
    }

    public static string Guettes() { return EnVeille() ? string.Join(", ", veille) : ""; }

    static void Lancer(string exe, string cible)
    {
        Stop();
        string x; while (lignes.TryDequeue(out x)) { }
        fin = false; exePm = exe; entete = null;
        var psi = new System.Diagnostics.ProcessStartInfo(exe,
            cible + " --output_stdout --no_console_stats --stop_existing_session --session_name Nevermind");
        psi.UseShellExecute = false; psi.CreateNoWindow = true;
        psi.RedirectStandardOutput = true; psi.RedirectStandardError = true;
        var p = new System.Diagnostics.Process();
        p.StartInfo = psi;
        p.EnableRaisingEvents = true;
        // (un PresentMon arrêté peut encore parler après le lancement du suivant : on ne garde que le PresentMon en cours)
        p.OutputDataReceived += (s, e) =>
        {
            if (e.Data == null || !ReferenceEquals(proc, s)) return;
            if (entete == null) { entete = e.Data; if (!rattachee) return; }
            lignes.Enqueue(e.Data); string y; while (lignes.Count > 20000 && lignes.TryDequeue(out y)) { }
        };
        p.ErrorDataReceived += (s, e) => { if (!string.IsNullOrEmpty(e.Data) && ReferenceEquals(proc, s)) lignes.Enqueue("#ERR " + e.Data); };
        p.Exited += (s, e) => { if (ReferenceEquals(proc, s)) fin = true; };
        proc = p;
        try { p.Start(); } catch { proc = null; throw; }
        p.BeginOutputReadLine();
        p.BeginErrorReadLine();
    }

    public static string Lire()
    {
        var sb = new StringBuilder();
        string l; int n = 0;
        // Première lecture après un rattachement : les noms des colonnes d'abord
        if (donnerEntete) { donnerEntete = false; var e = entete; if (e != null) sb.Append(e).Append('\n'); }
        while (n < 5000 && lignes.TryDequeue(out l)) { sb.Append(l).Append('\n'); n++; }
        if (fin && lignes.IsEmpty) sb.Append("#FIN\n");
        return sb.ToString();
    }

    public static void Stop()
    {
        veille = new string[0];
        var p = proc; proc = null;
        if (p == null) return;
        try { if (!p.HasExited) p.Kill(); } catch { }
        try { p.Dispose(); } catch { }
        try
        {
            var psi = new System.Diagnostics.ProcessStartInfo(exePm, "--terminate_existing_session --session_name Nevermind");
            psi.UseShellExecute = false; psi.CreateNoWindow = true;
            using (var t = System.Diagnostics.Process.Start(psi)) { t.WaitForExit(3000); }
        }
        catch { }
    }
}
'@

# Une seule Aide à la fois
$creee = $false
$Suffixe = Get-AideSuffixe $Racine
$verrou = New-Object System.Threading.Mutex($true, "Local\Nevermind-Aide-$Sid-$Suffixe", [ref]$creee)
if (-not $creee) { exit }

# Le canal porte le compte de l'utilisateur servi : Nevermind le retrouve avec le sien
$NomTuyau = "Nevermind-Aide-$Sid-$Suffixe"

# Droits du canal, posés ici un par un : l'utilisateur servi lit et écrit (lui seul, aucun autre compte), le réseau est
# refusé, et seule l'Aide crée les connexions : le système en SYSTEM, sinon les administrateurs (demande de Windows).
# L'utilisateur sans droits ne peut donc pas ouvrir lui-même un faux canal à ce nom tant que l'Aide tourne.
$secu = New-Object System.IO.Pipes.PipeSecurity
$secu.AddAccessRule((New-Object System.IO.Pipes.PipeAccessRule((New-Object Security.Principal.SecurityIdentifier('S-1-5-2')), 'FullControl', 'Deny')))
$droitsUtilisateur = if ($admin) { 'ReadWrite' } else { 'ReadWrite, CreateNewInstance' }   # Aide sans droits : tests seulement
$secu.AddAccessRule((New-Object System.IO.Pipes.PipeAccessRule($Utilisateur, $droitsUtilisateur, 'Allow')))
$createur = New-Object Security.Principal.SecurityIdentifier($(if ($EnSysteme) { 'S-1-5-18' } else { 'S-1-5-32-544' }))
$secu.AddAccessRule((New-Object System.IO.Pipes.PipeAccessRule($createur, 'FullControl', 'Allow')))
# Propriétaire du canal : le système ou les administrateurs, que Nevermind vérifie (natif.cs, AideClient.ServeurAccepte) ;
# un programme sans droits ne peut pas le donner à un canal
if ($admin) { $secu.SetOwner($createur) }

function New-Tuyau {
    New-Object System.IO.Pipes.NamedPipeServerStream($NomTuyau, [System.IO.Pipes.PipeDirection]::InOut, 32,
        [System.IO.Pipes.PipeTransmissionMode]::Byte, [System.IO.Pipes.PipeOptions]::Asynchronous, 65536, 65536, $secu)
}

# Canal suivant, ou $null quand toutes les connexions sont prises (demandes lentes qui s'entassent) : l'Aide ne s'arrête
# pas pour autant, elle réessaie dès qu'une demande se termine (sinon PresentMon et la trace réseau seraient coupés)
$script:TuyauxPris = $false
function New-TuyauSuivant {
    try {
        $t = New-Tuyau
        if ($script:TuyauxPris) { $script:TuyauxPris = $false; Write-AideLog 'Connexions libérées, l''Aide écoute de nouveau' }
        $t
    } catch {
        if (-not $script:TuyauxPris) { $script:TuyauxPris = $true; Write-AideLog "Toutes les connexions sont prises, nouvel essai dans un instant ($($_.Exception.Message))" }
        $null
    }
}

# Une demande : lue, vérifiée et faite dans un fil à part (une analyse antivirus ne bloque pas les autres demandes)
$Traitement = {
    param($c)
    if (-not (Get-Command Invoke-AideAction -ErrorAction SilentlyContinue)) { . $c.Actions }
    $t = $c.Tuyau
    $resume = ''
    try {
        $enc = New-Object Text.UTF8Encoding($false)
        $ecrivain = New-Object IO.StreamWriter($t, $enc, 65536, $true)
        # La demande : une ligne de 64 Ko au plus, arrivée en 5 s au plus. Sinon la connexion est coupée (un client muet
        # ou bavard ne bloque plus un fil de l'Aide et ne lui fait pas garder des mégaoctets en mémoire).
        $tampon = New-Object byte[] 65536; $lu = 0; $fin = -1
        $limite = [DateTime]::UtcNow.AddSeconds(5)
        while ($fin -lt 0) {
            if ($lu -ge $tampon.Length) { throw 'demande trop longue (plus de 64 Ko), connexion coupée' }
            $reste = [int][math]::Ceiling(($limite - [DateTime]::UtcNow).TotalMilliseconds)
            if ($reste -le 0) { throw 'demande trop lente (plus de 5 s), connexion coupée' }
            $lecture = $t.ReadAsync($tampon, $lu, $tampon.Length - $lu)
            if (-not $lecture.Wait($reste)) { throw 'demande trop lente (plus de 5 s), connexion coupée' }
            $n = $lecture.Result
            if ($n -le 0) { break }
            $i = [Array]::IndexOf($tampon, [byte]10, $lu, $n)
            $lu += $n
            if ($i -ge 0) { $fin = $i }
        }
        $ligne = $enc.GetString($tampon, 0, $(if ($fin -ge 0) { $fin } else { $lu })).TrimEnd("`r")
        if ($ligne) {
            $action = ''
            try {
                $d = $ligne | ConvertFrom-Json
                $action = [string]$d.a
                # Le journal de l'Aide (« Signaler un problème » de Nevermind), sans compter cette demande-ci ; il est gardé
                if ($action -eq 'aide.journal') { $r = @($c.Lignes.ToArray()) }
                else { $r = Invoke-AideAction $action $d.p -Client $c.Client }
                $rep = @{ ok = $true; r = $r }
                $resume = if ($action -ne 'aide.journal') { "$action : fait" }
            } catch {
                $rep = @{ ok = $false; e = $_.Exception.Message }
                $resume = "$action : refusé ou échoué ($($_.Exception.Message))"
            }
            $ecrivain.WriteLine((ConvertTo-Json -InputObject $rep -Depth 8 -Compress))
            $ecrivain.Flush()
            $t.WaitForPipeDrain()
        }
    } catch { $resume = "Échange interrompu : $($_.Exception.Message)" }
    finally { try { $t.Dispose() } catch {} }
    if ($resume) { [void]$c.Lignes.Add("$(Get-Date -Format s) $resume"); while ($c.Lignes.Count -gt 200) { $c.Lignes.RemoveAt(0) } }
}

# Les fils qui traitent les demandes : mêmes modules (chemin complet), et aucun chargement automatique non plus
$issFils = [Management.Automation.Runspaces.InitialSessionState]::CreateDefault()
$issFils.ExecutionPolicy = 'Bypass'
$issFils.ImportPSModule([string[]]$AideModulesWindows.ToArray())
$issFils.Variables.Add((New-Object Management.Automation.Runspaces.SessionStateVariableEntry('PSModuleAutoLoadingPreference', 'None', '')))
$pool = [RunspaceFactory]::CreateRunspacePool(1, 6, $issFils, $Host)
$pool.Open()
$enCours = New-Object System.Collections.ArrayList

# Le programme qui parle à l'Aide est-il Nevermind, de l'utilisateur servi ? Mindset.exe de ce dossier (son chemin
# complet) et, avec les droits administrateur, sous le compte -Sid : en SYSTEM, l'Aide voit aussi les programmes des
# autres comptes ouverts sur le PC, qui peuvent utiliser la même copie de Nevermind.
Initialize-AideProcessus
function Test-ClientServi([int]$Id, [string]$Chemin) {
    if (-not (Test-AideClient $Chemin $Racine)) { return $false }
    -not $admin -or [AideProcessus]::Compte($Id) -eq $Sid
}

# Nevermind (la fenêtre) ouvert ? L'Aide reste tant que Mindset.exe de ce dossier tourne pour l'utilisateur servi. Reconnu
# par son chemin complet : les autres applis de la suite et Mindset lui-même tournent aussi dans un « Mindset.exe ».
function Test-NevermindOuvert {
    if ($Parent) { return [bool](Get-Process -Id $Parent -ErrorAction SilentlyContinue) }
    foreach ($p in [Diagnostics.Process]::GetProcesses()) {
        try { if (Test-ClientServi $p.Id ([AideTuyau]::Chemin($p.Id))) { return $true } } catch {} finally { $p.Dispose() }
    }
    $false
}

try { $tuyau = New-Tuyau }
catch {
    # Canal déjà pris par un autre programme : l'Aide ne s'en sert pas
    Write-AideLog "Canal déjà utilisé par un autre programme, l'Aide s'arrête ($($_.Exception.Message))"
    exit
}
Write-AideLog "Aide $AideVersion démarrée$(if ($EnSysteme) { ' en SYSTEM' } elseif (-not $admin) { ' SANS droits administrateur' }) pour $Sid (processus $PID)"
if ($admin) { try { Rename-ReglesOptiGame } catch { Write-AideLog "Règles du pare-feu non renommées : $($_.Exception.Message)" } }
$seul = Get-Date   # depuis quand Nevermind n'est plus ouvert
$cherche = Get-Date   # dernière recherche de Nevermind
$attente = $tuyau.BeginWaitForConnection($null, $null)
try {
    while ($true) {
        # (sans canal libre : courte pause, puis nouvel essai)
        $pret = if ($tuyau) { $attente.AsyncWaitHandle.WaitOne(1000) } else { Start-Sleep -Milliseconds 200; $false }
        # Demandes terminées : on range (aussi quand les demandes se suivent sans pause, « fps.lire » toutes les 250 ms)
        foreach ($j in @($enCours)) {
            if (-not $j.H.IsCompleted) { continue }
            try { [void]$j.PS.EndInvoke($j.H) } catch { Write-AideLog "Demande : $($_.Exception.Message)" }
            $j.PS.Dispose(); [void]$enCours.Remove($j)
        }
        if (-not $pret) {
            if (-not $tuyau) {
                $tuyau = New-TuyauSuivant
                if ($tuyau) { $attente = $tuyau.BeginWaitForConnection($null, $null) }
            }
            if (((Get-Date) - $cherche).TotalSeconds -lt 5) { continue }   # Nevermind cherché toutes les 5 s
            $cherche = Get-Date
            if (Test-NevermindOuvert) { $seul = Get-Date }
            elseif (((Get-Date) - $seul).TotalSeconds -ge 20 -and -not $enCours.Count) { break }
            continue
        }
        $tuyau.EndWaitForConnection($attente)
        $connecte = $tuyau
        # Le canal suivant est prêt avant de traiter celui-ci : il n'y a jamais de moment sans Aide à l'écoute
        # (sauf quand toutes les connexions sont prises : New-TuyauSuivant)
        $tuyau = New-TuyauSuivant
        if ($tuyau) { $attente = $tuyau.BeginWaitForConnection($null, $null) }
        $client = [AideTuyau]::Client($connecte.SafePipeHandle)
        $chemin = if ($client) { [AideTuyau]::Chemin($client) } else { '' }
        # Sans droits administrateur (tests), l'Aide ne peut rien faire de plus que son client : elle accepte tout le monde
        if ($admin -and -not (Test-ClientServi $client $chemin)) {
            Write-AideLog "Refusé : demande venant de $(if ($chemin) { $chemin } else { "processus $client" })$(if ($client) { " (compte $([AideProcessus]::Compte($client)))" })"
            $connecte.Dispose()
            continue
        }
        $seul = Get-Date
        $ps = [PowerShell]::Create()
        $ps.RunspacePool = $pool
        [void]$ps.AddScript($Traitement.ToString()).AddArgument(@{ Tuyau = $connecte; Actions = $FichierActions; Lignes = $Lignes; Client = $client })
        [void]$enCours.Add(@{ PS = $ps; H = $ps.BeginInvoke() })
    }
} catch {
    Write-AideLog "Erreur de l'Aide : $($_.Exception.Message)"
} finally {
    try { $tuyau.Dispose() } catch {}
    # Mesures en cours : PresentMon et la trace réseau s'arrêtent avec l'Aide
    try { [AideFps]::Stop() } catch {}
    try { if (('NetFlow' -as [type]) -and [NetFlow]::Running) { [NetFlow]::Stop() } } catch {}
    Write-AideLog 'Aide arrêtée (Nevermind fermé)'
    try { $verrou.ReleaseMutex() } catch {}
    # Les fils du groupe gardent sinon Nevermind.exe ouvert
    try { $pool.Close(); $pool.Dispose() } catch {}
}
