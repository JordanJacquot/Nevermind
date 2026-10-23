#Requires -Version 5.1
<#
    Nevermind Aide : la seule partie de Nevermind qui tourne en administrateur.
    Lancée par NevermindAide.exe, depuis la tâche planifiée « \Mindset\Nevermind (aide) » que Mindset crée à
    l'installation (donc sans demande de Windows). Le programme est dans le dossier protégé de Mindset : seul un
    administrateur peut le modifier.

    Elle écoute sur un canal nommé réservé à l'utilisateur (refusé depuis le réseau), une demande par connexion :
    une ligne JSON { a = action, p = paramètres }, une ligne JSON en réponse { ok, r } ou { ok = false, e = message }.
    Elle ne répond qu'au programme qui fait tourner Nevermind, Mindset.exe de ce même dossier, reconnu par son chemin
    complet (Test-AideClient), et ne fait que ce qui est dans modules\aide-actions.ps1.
    Elle s'arrête toute seule quand Nevermind n'est plus ouvert.

    NevermindAide.ps1 -Parent <pid>  (tests) : reste ouverte tant que ce processus vit, au lieu de chercher Nevermind.
#>
param([int]$Parent)
$ErrorActionPreference = 'Stop'
$AppDir = $PSScriptRoot
$Racine = Split-Path $AppDir -Parent
$FichierActions = Join-Path $AppDir 'modules\aide-actions.ps1'
. $FichierActions

# Journal gardé en mémoire (200 dernières lignes) : Nevermind le demande (« aide.journal ») et l'écrit dans le sien.
# L'Aide n'écrit aucun fichier : en administrateur, écrire dans un dossier modifiable sans droits permettrait à un
# programme d'y glisser un lien et de lui faire écrire ailleurs. Rien n'est réglable par variable d'environnement non
# plus (la tâche planifiée reprend celles de l'utilisateur).
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
$verrou = New-Object System.Threading.Mutex($true, "Local\Nevermind-Aide-$Suffixe", [ref]$creee)
if (-not $creee) { exit }

$moi = [Security.Principal.WindowsIdentity]::GetCurrent()
$admin = (New-Object Security.Principal.WindowsPrincipal($moi)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$NomTuyau = "Nevermind-Aide-$($moi.User.Value)-$Suffixe"

# Droits du canal : l'utilisateur lit et écrit, les administrateurs (l'Aide) créent les connexions, le réseau est refusé.
# L'utilisateur sans droits ne peut donc pas ouvrir lui-même un faux canal à ce nom tant que l'Aide tourne.
$secu = New-Object System.IO.Pipes.PipeSecurity
$secu.AddAccessRule((New-Object System.IO.Pipes.PipeAccessRule((New-Object Security.Principal.SecurityIdentifier('S-1-5-2')), 'FullControl', 'Deny')))
$droitsUtilisateur = if ($admin) { 'ReadWrite' } else { 'ReadWrite, CreateNewInstance' }   # Aide sans droits : tests seulement
$secu.AddAccessRule((New-Object System.IO.Pipes.PipeAccessRule($moi.User, $droitsUtilisateur, 'Allow')))
$secu.AddAccessRule((New-Object System.IO.Pipes.PipeAccessRule((New-Object Security.Principal.SecurityIdentifier('S-1-5-32-544')), 'FullControl', 'Allow')))

function New-Tuyau {
    New-Object System.IO.Pipes.NamedPipeServerStream($NomTuyau, [System.IO.Pipes.PipeDirection]::InOut, 8,
        [System.IO.Pipes.PipeTransmissionMode]::Byte, [System.IO.Pipes.PipeOptions]::Asynchronous, 65536, 65536, $secu)
}

# Une demande : lue, vérifiée et faite dans un fil à part (une analyse antivirus ne bloque pas les autres demandes)
$Traitement = {
    param($c)
    if (-not (Get-Command Invoke-AideAction -ErrorAction SilentlyContinue)) { . $c.Actions }
    $t = $c.Tuyau
    $resume = ''
    try {
        $enc = New-Object Text.UTF8Encoding($false)
        $lecteur = New-Object IO.StreamReader($t, $enc, $false, 65536, $true)
        $ecrivain = New-Object IO.StreamWriter($t, $enc, 65536, $true)
        $ligne = $lecteur.ReadLine()
        if ($ligne) {
            $action = ''
            try {
                $d = $ligne | ConvertFrom-Json
                $action = [string]$d.a
                # Le journal de l'Aide (« Signaler un problème » de Nevermind), sans compter cette demande-ci ; il est gardé
                if ($action -eq 'aide.journal') { $r = @($c.Lignes.ToArray()) }
                else { $r = Invoke-AideAction $action $d.p }
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

$pool = [RunspaceFactory]::CreateRunspacePool(1, 6)
$pool.Open()
$enCours = New-Object System.Collections.ArrayList

# Nevermind (la fenêtre) ouvert ? L'Aide reste tant que Mindset.exe de ce dossier tourne. Reconnu par son chemin complet :
# les autres applis de la suite et Mindset lui-même tournent aussi dans un « Mindset.exe ».
function Test-NevermindOuvert {
    if ($Parent) { return [bool](Get-Process -Id $Parent -ErrorAction SilentlyContinue) }
    foreach ($p in [Diagnostics.Process]::GetProcesses()) {
        try { if (Test-AideClient ([AideTuyau]::Chemin($p.Id)) $Racine) { return $true } } catch {} finally { $p.Dispose() }
    }
    $false
}

try { $tuyau = New-Tuyau }
catch {
    # Canal déjà pris par un autre programme : l'Aide ne s'en sert pas
    Write-AideLog "Canal déjà utilisé par un autre programme, l'Aide s'arrête ($($_.Exception.Message))"
    exit
}
Write-AideLog "Aide $AideVersion démarrée$(if (-not $admin) { ' SANS droits administrateur' }) (processus $PID)"
if ($admin) { try { Rename-ReglesOptiGame } catch { Write-AideLog "Règles du pare-feu non renommées : $($_.Exception.Message)" } }
$seul = Get-Date   # depuis quand Nevermind n'est plus ouvert
$tours = 0
$attente = $tuyau.BeginWaitForConnection($null, $null)
try {
    while ($true) {
        if (-not $attente.AsyncWaitHandle.WaitOne(1000)) {
            # Demandes terminées : on range
            foreach ($j in @($enCours)) {
                if (-not $j.H.IsCompleted) { continue }
                try { [void]$j.PS.EndInvoke($j.H) } catch { Write-AideLog "Demande : $($_.Exception.Message)" }
                $j.PS.Dispose(); [void]$enCours.Remove($j)
            }
            if ((++$tours % 5) -ne 0) { continue }   # Nevermind cherché toutes les 5 s
            if (Test-NevermindOuvert) { $seul = Get-Date }
            elseif (((Get-Date) - $seul).TotalSeconds -ge 20 -and -not $enCours.Count) { break }
            continue
        }
        $tuyau.EndWaitForConnection($attente)
        $connecte = $tuyau
        # Le canal suivant est prêt avant de traiter celui-ci : il n'y a jamais de moment sans Aide à l'écoute
        $tuyau = New-Tuyau
        $attente = $tuyau.BeginWaitForConnection($null, $null)
        $client = [AideTuyau]::Client($connecte.SafePipeHandle)
        $chemin = if ($client) { [AideTuyau]::Chemin($client) } else { '' }
        # Sans droits administrateur (tests), l'Aide ne peut rien faire de plus que son client : elle accepte tout le monde
        if ($admin -and -not (Test-AideClient $chemin $Racine)) {
            Write-AideLog "Refusé : demande venant de $(if ($chemin) { $chemin } else { "processus $client" })"
            $connecte.Dispose()
            continue
        }
        $seul = Get-Date
        $ps = [PowerShell]::Create()
        $ps.RunspacePool = $pool
        [void]$ps.AddScript($Traitement.ToString()).AddArgument(@{ Tuyau = $connecte; Actions = $FichierActions; Lignes = $Lignes })
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
