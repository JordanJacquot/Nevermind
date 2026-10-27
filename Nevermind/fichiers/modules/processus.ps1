# Nevermind : ce que Windows dit des processus (lectures seulement, sans droits administrateur), pour la règle
# « fermable » de NevermindPont.dll (pont.ps1) : la bulle RAM (memoire.ps1), le mode jeu (jeu.ps1) et « Fermer
# <programme> » (aide-client.ps1). La règle elle-même n'est pas ici : elle est dans la DLL, la même que celle de l'Aide.
# (Ces lectures étaient prises dans aide-actions.ps1, que Nevermind ne charge plus : voir le plan Rust, étape 1a, point D.)
#
# Que des définitions : le fil de fond de la page RAM charge aussi ce fichier.

# Ce que Windows dit d'un processus, même pour un autre compte : sa session, son compte, s'il est protégé (les moteurs
# d'antivirus tournent en processus protégés). Chargé à la première lecture qui s'en sert.
function Initialize-AideProcessus {
    if ('AideProcessus' -as [type]) { return }
    try {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
public static class AideProcessus
{
    [DllImport("kernel32.dll", SetLastError = true)] static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    [DllImport("kernel32.dll")] static extern bool ProcessIdToSessionId(uint pid, out uint session);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool GetProcessInformation(IntPtr h, int classe, out uint info, int taille);
    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)] static extern bool QueryFullProcessImageName(IntPtr h, int flags, StringBuilder name, ref int size);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool OpenProcessToken(IntPtr h, uint access, out IntPtr jeton);
    [DllImport("advapi32.dll", SetLastError = true)] static extern bool GetTokenInformation(IntPtr jeton, int classe, IntPtr info, int taille, out int requis);
    const uint Lecture = 0x1000;   // PROCESS_QUERY_LIMITED_INFORMATION

    // Session Windows du processus, -1 si inconnue
    public static int Session(int pid) { uint s; return ProcessIdToSessionId((uint)pid, out s) ? (int)s : -1; }

    // Compte du processus (SID), "" si Windows ne le dit pas
    public static string Compte(int pid)
    {
        IntPtr h = OpenProcess(Lecture, false, pid);
        if (h == IntPtr.Zero) return "";
        IntPtr jeton = IntPtr.Zero, info = IntPtr.Zero;
        try
        {
            if (!OpenProcessToken(h, 0x8, out jeton)) return "";   // TOKEN_QUERY
            int requis;
            GetTokenInformation(jeton, 1, IntPtr.Zero, 0, out requis);   // TokenUser
            if (requis <= 0) return "";
            info = Marshal.AllocHGlobal(requis);
            if (!GetTokenInformation(jeton, 1, info, requis, out requis)) return "";
            return new SecurityIdentifier(Marshal.ReadIntPtr(info)).Value;
        }
        catch { return ""; }
        finally
        {
            if (info != IntPtr.Zero) Marshal.FreeHGlobal(info);
            if (jeton != IntPtr.Zero) CloseHandle(jeton);
            CloseHandle(h);
        }
    }

    // Niveau de protection : 0 aucun, 1 protégé (antivirus, Windows), -1 inconnu (traité comme protégé)
    public static int Protection(int pid)
    {
        IntPtr h = OpenProcess(Lecture, false, pid);
        if (h == IntPtr.Zero) return -1;
        try
        {
            uint niveau;
            if (!GetProcessInformation(h, 7, out niveau, 4)) return -1;   // ProcessProtectionLevelInfo
            return niveau == 0xFFFFFFFE ? 0 : 1;   // PROTECTION_LEVEL_NONE
        }
        finally { CloseHandle(h); }
    }

    // Lancé en administrateur : 0 non, 1 oui, -1 inconnu (jeton illisible : traité comme oui)
    public static int Eleve(int pid)
    {
        IntPtr h = OpenProcess(Lecture, false, pid);
        if (h == IntPtr.Zero) return -1;
        IntPtr jeton = IntPtr.Zero, info = IntPtr.Zero;
        try
        {
            if (!OpenProcessToken(h, 0x8, out jeton)) return -1;   // TOKEN_QUERY
            info = Marshal.AllocHGlobal(4);
            int requis;
            if (!GetTokenInformation(jeton, 20, info, 4, out requis)) return -1;   // TokenElevation
            return Marshal.ReadInt32(info) != 0 ? 1 : 0;
        }
        catch { return -1; }
        finally
        {
            if (info != IntPtr.Zero) Marshal.FreeHGlobal(info);
            if (jeton != IntPtr.Zero) CloseHandle(jeton);
            CloseHandle(h);
        }
    }

    public static string Chemin(int pid)
    {
        IntPtr h = OpenProcess(Lecture, false, pid);
        if (h == IntPtr.Zero) return "";
        try { var sb = new StringBuilder(1024); int n = sb.Capacity; return QueryFullProcessImageName(h, 0, sb, ref n) ? sb.ToString() : ""; }
        finally { CloseHandle(h); }
    }
}
'@
    } catch { if (-not ('AideProcessus' -as [type])) { throw } }   # (un autre fil l'a chargé au même moment)
}

# Ce que Windows dit d'un processus (une « fiche ») : @{ Id; Chemin; Session; Compte; Protection; Eleve }
# (Protection et Eleve : 0 non, 1 oui, -1 illisible). Les tests donnent de fausses fiches à Get-RefusFermeture.
function Read-AideFiche([int]$Id, [string]$Chemin) {
    Initialize-AideProcessus
    $c = [AideProcessus]::Chemin($Id); if (-not $c) { $c = $Chemin }
    @{ Id = $Id; Chemin = $c; Session = [AideProcessus]::Session($Id); Compte = [AideProcessus]::Compte($Id); Protection = [AideProcessus]::Protection($Id); Eleve = [AideProcessus]::Eleve($Id) }
}
# Celui pour qui on ferme (Nevermind) : @{ Id; Session; Compte; Eleve }
function Get-AideReference([int]$Id) {
    Initialize-AideProcessus
    @{ Id = $Id; Session = [AideProcessus]::Session($Id); Compte = [AideProcessus]::Compte($Id); Eleve = [AideProcessus]::Eleve($Id) }
}

# Ce que Windows déclare, lu à chaque geste : les programmes des antivirus inscrits au Centre de sécurité, et les
# processus de service. Pas de liste à tenir à jour : un nouvel antivirus est reconnu tout seul. Reference (le programme
# pour qui on ferme) est ajouté par celui qui demande. Une erreur si Windows ne donne pas les services.
function Get-AideContexte {
    $c = @{ Antivirus = Get-AideAntivirus; Windows = [Environment]::GetFolderPath('Windows'); ServicesPid = [Collections.Generic.HashSet[int]]::new() }
    foreach ($s in @(Get-CimInstance -ClassName Win32_Service -Property ProcessId -OperationTimeoutSec 10 -ErrorAction Stop)) {
        if ([int]$s.ProcessId -gt 0) { [void]$c.ServicesPid.Add([int]$s.ProcessId) }
    }
    $c
}

# Les programmes des antivirus inscrits au Centre de sécurité de Windows (chemins complets, lus par la DLL dans ce que
# Windows écrit : « %ProgramFiles%\...\x.exe », « windowsdefender://... »)
function Get-AideAntivirus {
    $l = [Collections.Generic.List[string]]::new()
    try {   # (absent des Windows Server : aucun antivirus inscrit)
        $ecrits = @(foreach ($av in @(Get-CimInstance -Namespace 'root\SecurityCenter2' -ClassName AntiVirusProduct -OperationTimeoutSec 5 -ErrorAction Stop)) {
            [string]$av.pathToSignedProductExe; [string]$av.pathToSignedReportingExe
        })
        if ($ecrits.Count) { foreach ($p in (Get-ProgrammesCommandes $ecrits)) { if ($p) { $l.Add($p) } } }
    } catch {}
    , $l
}
