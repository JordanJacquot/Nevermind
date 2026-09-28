#Requires -Version 5.1
<#
    OptiGame 1.0.10
    Analyse et optimisation gaming pour Windows 10 et 11.

    Chaque réglage modifié est sauvegardé dans %LOCALAPPDATA%\OptiGame\sauvegarde.json
    et peut être annulé depuis l'onglet Sauvegarde.

    OptiGame.ps1 -Uninstall  remet les réglages comme avant et supprime l'application.
#>
param([switch]$Uninstall)

$AppVersion = '1.0.10'
$UpdateRepo = 'JordanJacquot/OptiGame'   # dépôt GitHub où sont publiées les mises à jour

# ---------------------------------------------------------------------------
# Droits administrateur
# ---------------------------------------------------------------------------
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase, System.Drawing, System.Windows.Forms

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    try {
        Start-Process -FilePath 'powershell.exe' -Verb RunAs -ErrorAction Stop -ArgumentList @(
            @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', "`"$PSCommandPath`"") + @(if ($Uninstall) { '-Uninstall' }))
    } catch {
        [System.Windows.MessageBox]::Show(
            "OptiGame a besoin des droits administrateur pour modifier les réglages de Windows.`n`nRelance l'application et clique sur « Oui » quand Windows le demande.",
            'OptiGame', 'OK', 'Warning') | Out-Null
    }
    exit
}

# Retire la marque « téléchargé depuis Internet » des fichiers d'OptiGame, pour que
# Windows n'affiche plus d'avertissement aux lancements suivants.
try {
    $appRoot = if ((Split-Path $PSScriptRoot -Leaf) -eq 'fichiers') { Split-Path $PSScriptRoot -Parent } else { $PSScriptRoot }
    @(Get-ChildItem -LiteralPath $PSScriptRoot -File -ErrorAction SilentlyContinue) +
    @(Get-ChildItem -LiteralPath $appRoot -File -ErrorAction SilentlyContinue | Where-Object { $_.Name -match '^(OptiGame\.exe|Désinstaller OptiGame\.exe|LISEZMOI\.txt)$' }) |
        Unblock-File -ErrorAction SilentlyContinue
} catch {}

# ---------------------------------------------------------------------------
# Fonctions natives (écrans, souris, barre de titre sombre)
# ---------------------------------------------------------------------------
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

public static class OGNative
{
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DEVMODE
    {
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmDeviceName;
        public short dmSpecVersion;
        public short dmDriverVersion;
        public short dmSize;
        public short dmDriverExtra;
        public int dmFields;
        public int dmPositionX;
        public int dmPositionY;
        public int dmDisplayOrientation;
        public int dmDisplayFixedOutput;
        public short dmColor;
        public short dmDuplex;
        public short dmYResolution;
        public short dmTTOption;
        public short dmCollate;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string dmFormName;
        public short dmLogPixels;
        public int dmBitsPerPel;
        public int dmPelsWidth;
        public int dmPelsHeight;
        public int dmDisplayFlags;
        public int dmDisplayFrequency;
        public int dmICMMethod;
        public int dmICMIntent;
        public int dmMediaType;
        public int dmDitherType;
        public int dmReserved1;
        public int dmReserved2;
        public int dmPanningWidth;
        public int dmPanningHeight;
    }

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct DISPLAY_DEVICE
    {
        public int cb;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 32)] public string DeviceName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceString;
        public int StateFlags;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceID;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 128)] public string DeviceKey;
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern bool EnumDisplaySettings(string deviceName, int modeNum, ref DEVMODE devMode);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern bool EnumDisplayDevices(string device, uint devNum, ref DISPLAY_DEVICE displayDevice, uint flags);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool SystemParametersInfo(uint action, uint param, int[] vparam, uint winIni);

    [DllImport("dwmapi.dll")]
    static extern int DwmSetWindowAttribute(IntPtr hwnd, int attr, ref int value, int size);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    static extern int ChangeDisplaySettingsEx(string deviceName, ref DEVMODE devMode, IntPtr hwnd, uint flags, IntPtr lParam);

    // Change la fréquence d'un écran en gardant sa résolution. Retourne 0 si réussi.
    public static int SetRefreshRate(string deviceName, int hz)
    {
        DEVMODE cur = new DEVMODE();
        cur.dmSize = (short)Marshal.SizeOf(cur);
        if (!EnumDisplaySettings(deviceName, -1, ref cur)) return -100;
        cur.dmDisplayFrequency = hz;
        cur.dmFields = 0x80000 | 0x100000 | 0x400000;
        return ChangeDisplaySettingsEx(deviceName, ref cur, IntPtr.Zero, 0x01, IntPtr.Zero);
    }

    // Mode d'alimentation de Windows 10/11 (curseur « Meilleures performances »).
    [DllImport("powrprof.dll")]
    static extern uint PowerGetEffectiveOverlayScheme(out Guid scheme);

    [DllImport("powrprof.dll")]
    static extern uint PowerSetActiveOverlayScheme(Guid scheme);

    public static string GetOverlay()
    {
        Guid g;
        return PowerGetEffectiveOverlayScheme(out g) == 0 ? g.ToString() : "";
    }

    public static uint SetOverlay(string scheme)
    {
        return PowerSetActiveOverlayScheme(new Guid(scheme));
    }

    // =====================================================================
    // Tests des composants (lancés dans un fil séparé, progression lue par l'interface)
    // =====================================================================
    public static volatile bool Cancel;
    public static double Progress;
    public static string Phase = "";
    public static double LiveValue;   // valeur instantanée pour la courbe en direct

    [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern Microsoft.Win32.SafeHandles.SafeFileHandle CreateFile(string name, uint access, uint share, IntPtr sec, uint disposition, uint flags, IntPtr template);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool WriteFile(Microsoft.Win32.SafeHandles.SafeFileHandle h, IntPtr buffer, uint count, out uint done, IntPtr overlapped);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool ReadFile(Microsoft.Win32.SafeHandles.SafeFileHandle h, IntPtr buffer, uint count, out uint done, IntPtr overlapped);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool SetFilePointerEx(Microsoft.Win32.SafeHandles.SafeFileHandle h, long distance, out long newPosition, uint method);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr VirtualAlloc(IntPtr address, UIntPtr size, uint type, uint protect);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool VirtualFree(IntPtr address, UIntPtr size, uint type);

    // Vitesse d'un disque, sans passer par le cache de Windows.
    // Retourne { écriture Mo/s, lecture Mo/s, lecture 4K Mo/s, opérations 4K par seconde } ou null si arrêté.
    public static double[] DiskTest(string file, long size)
    {
        const int block = 8 * 1024 * 1024;
        const uint GENERIC_READ = 0x80000000, GENERIC_WRITE = 0x40000000;
        const uint NO_BUFFERING = 0x20000000, WRITE_THROUGH = 0x80000000, SEQUENTIAL = 0x08000000, RANDOM = 0x10000000;
        size = Math.Max(block, size / block * block);
        long blocks = size / block;
        long written = 0;
        const double PhaseSeconds = 6;
        IntPtr buf = VirtualAlloc(IntPtr.Zero, (UIntPtr)block, 0x3000, 0x04);
        if (buf == IntPtr.Zero) throw new Exception("Mémoire insuffisante pour le test.");
        double[] r = new double[4];
        try
        {
            byte[] rnd = new byte[block];
            new Random(42).NextBytes(rnd);
            Marshal.Copy(rnd, 0, buf, block);
            uint done;

            Phase = "write";
            using (var h = CreateFile(file, GENERIC_WRITE, 0, IntPtr.Zero, 2, NO_BUFFERING | WRITE_THROUGH, IntPtr.Zero))
            {
                if (h.IsInvalid) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
                var sw = System.Diagnostics.Stopwatch.StartNew();
                double last = 0; LiveValue = 0;
                for (long i = 0; i < blocks; i++)
                {
                    if (Cancel) return null;
                    if (sw.Elapsed.TotalSeconds > PhaseSeconds) break;
                    if (!WriteFile(h, buf, (uint)block, out done, IntPtr.Zero)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
                    written = i + 1;
                    Progress = 40.0 * Math.Max((double)(i + 1) / blocks, sw.Elapsed.TotalSeconds / PhaseSeconds);
                    double now = sw.Elapsed.TotalSeconds;
                    double inst = block / 1048576.0 / Math.Max(1e-6, now - last);
                    LiveValue = LiveValue == 0 ? inst : LiveValue * 0.85 + inst * 0.15;
                    last = now;
                }
                r[0] = written * (double)block / 1048576.0 / sw.Elapsed.TotalSeconds;
            }

            Phase = "read";
            using (var h = CreateFile(file, GENERIC_READ, 1, IntPtr.Zero, 3, NO_BUFFERING | SEQUENTIAL, IntPtr.Zero))
            {
                if (h.IsInvalid) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
                var sw = System.Diagnostics.Stopwatch.StartNew();
                double last = 0; LiveValue = 0;
                long readBlocks = 0;
                for (long i = 0; i < written; i++)
                {
                    if (Cancel) return null;
                    if (sw.Elapsed.TotalSeconds > PhaseSeconds) break;
                    if (!ReadFile(h, buf, (uint)block, out done, IntPtr.Zero)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
                    readBlocks = i + 1;
                    Progress = 40 + 30.0 * Math.Max((double)(i + 1) / written, sw.Elapsed.TotalSeconds / PhaseSeconds);
                    double now = sw.Elapsed.TotalSeconds;
                    double inst = block / 1048576.0 / Math.Max(1e-6, now - last);
                    LiveValue = LiveValue == 0 ? inst : LiveValue * 0.85 + inst * 0.15;
                    last = now;
                }
                r[1] = readBlocks * (double)block / 1048576.0 / sw.Elapsed.TotalSeconds;
            }

            Phase = "random";
            using (var h = CreateFile(file, GENERIC_READ, 1, IntPtr.Zero, 3, NO_BUFFERING | RANDOM, IntPtr.Zero))
            {
                if (h.IsInvalid) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
                var rng = new Random(7);
                long pages = written * (long)block / 4096, count = 0, pos;
                var sw = System.Diagnostics.Stopwatch.StartNew();
                while (sw.Elapsed.TotalSeconds < 5)
                {
                    if (Cancel) return null;
                    SetFilePointerEx(h, (long)(rng.NextDouble() * pages) * 4096, out pos, 0);
                    if (!ReadFile(h, buf, 4096, out done, IntPtr.Zero)) throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
                    count++;
                    if ((count & 127) == 0)
                    {
                        Progress = 70 + 30 * Math.Min(1, sw.Elapsed.TotalSeconds / 5);
                        LiveValue = count / Math.Max(1e-6, sw.Elapsed.TotalSeconds);
                    }
                }
                double s = sw.Elapsed.TotalSeconds;
                r[3] = count / s;
                r[2] = count * 4096 / 1048576.0 / s;
            }
            Progress = 100;
            return r;
        }
        finally
        {
            VirtualFree(buf, UIntPtr.Zero, 0x8000);
            try { System.IO.File.Delete(file); } catch { }
        }
    }

    // Calcul déterministe: le même résultat doit toujours sortir, sinon le processeur est instable.
    static long CpuWork(long seed, long iterations)
    {
        long x = seed;
        double d = seed;
        for (long i = 0; i < iterations; i++)
        {
            x = x * 6364136223846793005L + 1442695040888963407L;
            d = d * 1.0000001 + (x & 1023) * 0.5;
            if (d > 1e12) d = d / 3.0;
        }
        return x ^ (long)d;
    }

    // Retourne { score 1 cœur, score tous les cœurs, erreurs de calcul, nombre de threads } ou null si arrêté.
    public static double[] CpuTest(double singleSeconds, double multiSeconds)
    {
        const long chunk = 2000000;
        long reference = CpuWork(12345, chunk);
        double total = singleSeconds + multiSeconds;
        long errors = 0;

        Phase = "single";
        long n = 0;
        var sw = System.Diagnostics.Stopwatch.StartNew();
        while (sw.Elapsed.TotalSeconds < singleSeconds)
        {
            if (Cancel) return null;
            if (CpuWork(12345, chunk) != reference) errors++;
            n++;
            Progress = 100 * sw.Elapsed.TotalSeconds / total;
        }
        double single = n / sw.Elapsed.TotalSeconds;

        Phase = "multi";
        int threads = Environment.ProcessorCount;
        long count = 0;
        var start = System.Diagnostics.Stopwatch.StartNew();
        var ths = new System.Threading.Thread[threads];
        for (int t = 0; t < threads; t++)
        {
            ths[t] = new System.Threading.Thread(() =>
            {
                long local = 0, bad = 0;
                while (start.Elapsed.TotalSeconds < multiSeconds && !Cancel)
                {
                    if (CpuWork(12345, chunk) != reference) bad++;
                    local++;
                }
                System.Threading.Interlocked.Add(ref count, local);
                System.Threading.Interlocked.Add(ref errors, bad);
            });
            ths[t].IsBackground = true;
            ths[t].Priority = System.Threading.ThreadPriority.BelowNormal;
            ths[t].Start();
        }
        foreach (var th in ths)
        {
            while (!th.Join(100)) Progress = 100 * (singleSeconds + Math.Min(multiSeconds, start.Elapsed.TotalSeconds)) / total;
        }
        if (Cancel) return null;
        double multi = count / start.Elapsed.TotalSeconds;
        Progress = 100;
        return new double[] { single * 10, multi * 10, errors, threads };
    }

    static long a_len(List<long[]> mem) { return mem.Count > 0 ? mem[0].Length : 0; }

    static void ParallelChunks(List<long[]> mem, Action<long[], int> work, double p0, double p1)
    {
        int next = -1, finished = 0, total = mem.Count;
        var clock = System.Diagnostics.Stopwatch.StartNew();
        LiveValue = 0;
        int threads = Math.Min(Environment.ProcessorCount, total);
        var ths = new System.Threading.Thread[threads];
        for (int t = 0; t < threads; t++)
        {
            ths[t] = new System.Threading.Thread(() =>
            {
                while (!Cancel)
                {
                    int i = System.Threading.Interlocked.Increment(ref next);
                    if (i >= total) break;
                    work(mem[i], i);
                    int f = System.Threading.Interlocked.Increment(ref finished);
                    Progress = p0 + (p1 - p0) * f / total;
                    LiveValue = f * (a_len(mem) * 8.0) / 1073741824.0 / Math.Max(1e-6, clock.Elapsed.TotalSeconds);
                }
            });
            ths[t].IsBackground = true;
            ths[t].Start();
        }
        foreach (var th in ths) th.Join();
    }

    // Mémoire vive: écrit des motifs, les relit et compte les erreurs.
    // Retourne { écriture Go/s, lecture Go/s, copie Go/s, erreurs, Go testés } ou null si arrêté.
    public static double[] MemTest(long bytes)
    {
        const int chunkLongs = 8 * 1024 * 1024;
        const long mult = -7046029254386353131L;
        int chunks = (int)Math.Max(2, bytes / (chunkLongs * 8L));
        var mem = new List<long[]>();
        try
        {
            Phase = "alloc";
            for (int i = 0; i < chunks; i++)
            {
                if (Cancel) return null;
                mem.Add(new long[chunkLongs]);
                Progress = 5.0 * (i + 1) / chunks;
            }
            double gb = chunks * (chunkLongs * 8.0) / 1073741824.0;
            long errors = 0;

            Phase = "write";
            var sw = System.Diagnostics.Stopwatch.StartNew();
            ParallelChunks(mem, (a, c) => { long k = ((long)c << 32) ^ 0x5555555555555555L; for (int j = 0; j < a.Length; j++) a[j] = k ^ (j * mult); }, 5, 30);
            if (Cancel) return null;
            double write = gb / sw.Elapsed.TotalSeconds;

            Phase = "read";
            sw.Restart();
            ParallelChunks(mem, (a, c) => { long k = ((long)c << 32) ^ 0x5555555555555555L; long e = 0; for (int j = 0; j < a.Length; j++) if (a[j] != (k ^ (j * mult))) e++; if (e > 0) System.Threading.Interlocked.Add(ref errors, e); }, 30, 55);
            if (Cancel) return null;
            double read = gb / sw.Elapsed.TotalSeconds;

            Phase = "pattern";
            ParallelChunks(mem, (a, c) => { long k = ~(((long)c << 32) ^ 0x5555555555555555L); for (int j = 0; j < a.Length; j++) a[j] = ~(k ^ (j * mult)); }, 55, 70);
            ParallelChunks(mem, (a, c) => { long k = ~(((long)c << 32) ^ 0x5555555555555555L); long e = 0; for (int j = 0; j < a.Length; j++) if (a[j] != ~(k ^ (j * mult))) e++; if (e > 0) System.Threading.Interlocked.Add(ref errors, e); }, 70, 85);
            if (Cancel) return null;

            Phase = "copy";
            sw.Restart();
            int pairs = 0;
            for (int i = 0; i + 1 < chunks; i += 2)
            {
                if (Cancel) return null;
                Array.Copy(mem[i], mem[i + 1], chunkLongs);
                pairs++;
                Progress = 85 + 15.0 * (i + 2) / chunks;
            }
            double copy = pairs * (chunkLongs * 8.0) / 1073741824.0 / sw.Elapsed.TotalSeconds;
            Progress = 100;
            return new double[] { write, read, copy, errors, gb };
        }
        finally
        {
            mem.Clear();
            GC.Collect();
        }
    }

    // Débit Internet en Mb/s (téléchargement ou envoi), plusieurs connexions en parallèle.
    // Si un serveur refuse (trop de tests, panne), on passe au suivant. Retourne -1 si aucun n'a répondu.
    public static double NetSpeed(string[] urls, bool upload, double seconds, int streams, double p0, double p1)
    {
        int urlIndex = 0;
        System.Net.ServicePointManager.SecurityProtocol = System.Net.SecurityProtocolType.Tls12;
        System.Net.ServicePointManager.DefaultConnectionLimit = 64;
        System.Net.ServicePointManager.Expect100Continue = false;
        long total = 0;
        byte[] data = new byte[65536];
        new Random(3).NextBytes(data);
        var sw = System.Diagnostics.Stopwatch.StartNew();
        var ths = new System.Threading.Thread[streams];
        for (int t = 0; t < streams; t++)
        {
            ths[t] = new System.Threading.Thread(() =>
            {
                byte[] buf = new byte[65536];
                while (sw.Elapsed.TotalSeconds < seconds && !Cancel)
                {
                    System.Net.HttpWebRequest req = null;
                    int ui = urlIndex;
                    if (ui >= urls.Length) break;
                    try
                    {
                        req = (System.Net.HttpWebRequest)System.Net.WebRequest.Create(urls[ui]);
                        req.UserAgent = "OptiGame";
                        req.Timeout = 10000;
                        req.ReadWriteTimeout = 10000;
                        if (upload)
                        {
                            req.Method = "POST";
                            req.ContentType = "application/octet-stream";
                            req.AllowWriteStreamBuffering = false;
                            req.SendChunked = true;
                            using (var s = req.GetRequestStream())
                            {
                                for (int k = 0; k < 400 && sw.Elapsed.TotalSeconds < seconds && !Cancel; k++)
                                {
                                    s.Write(data, 0, data.Length);
                                    System.Threading.Interlocked.Add(ref total, data.Length);
                                }
                            }
                            try { using (req.GetResponse()) { } } catch { }
                        }
                        else
                        {
                            using (var resp = req.GetResponse())
                            using (var s = resp.GetResponseStream())
                            {
                                int n;
                                while ((n = s.Read(buf, 0, buf.Length)) > 0)
                                {
                                    System.Threading.Interlocked.Add(ref total, n);
                                    if (sw.Elapsed.TotalSeconds >= seconds || Cancel) { req.Abort(); break; }
                                }
                            }
                        }
                    }
                    catch
                    {
                        if (sw.Elapsed.TotalSeconds >= seconds || Cancel) break;
                        System.Threading.Interlocked.CompareExchange(ref urlIndex, ui + 1, ui);
                        System.Threading.Thread.Sleep(100);
                    }
                }
            });
            ths[t].IsBackground = true;
            ths[t].Start();
        }
        long lastBytes = 0; double lastTime = 0; LiveValue = 0;
        foreach (var th in ths)
        {
            while (!th.Join(100))
            {
                Progress = p0 + (p1 - p0) * Math.Min(1, sw.Elapsed.TotalSeconds / seconds);
                double now = sw.Elapsed.TotalSeconds;
                if (now - lastTime >= 0.3)
                {
                    long b = System.Threading.Interlocked.Read(ref total);
                    LiveValue = (b - lastBytes) * 8 / 1e6 / (now - lastTime);
                    lastBytes = b; lastTime = now;
                }
            }
        }
        double elapsed = Math.Min(sw.Elapsed.TotalSeconds, seconds + 0.5);
        Progress = p1;
        if (total == 0) return -1;
        return total * 8 / 1e6 / elapsed;
    }

    // =====================================================================
    // Scan du réseau local
    // =====================================================================
    public static int Found;

    // Ping de toutes les adresses en parallèle. Retourne "ip|ms" pour celles qui répondent.
    public static string[] PingSweep(string[] ips, int timeoutMs)
    {
        var results = new List<string>();
        var sync = new object();
        Found = 0;
        int done = 0;
        var sem = new System.Threading.SemaphoreSlim(64);
        var tasks = new List<System.Threading.Tasks.Task>();
        foreach (string ip in ips)
        {
            if (Cancel) break;
            sem.Wait();
            var ping = new System.Net.NetworkInformation.Ping();
            string addr = ip;
            tasks.Add(ping.SendPingAsync(addr, timeoutMs).ContinueWith(t =>
            {
                try
                {
                    if (t.Status == System.Threading.Tasks.TaskStatus.RanToCompletion && t.Result.Status == System.Net.NetworkInformation.IPStatus.Success)
                    {
                        lock (sync) { results.Add(addr + "|" + t.Result.RoundtripTime); }
                        System.Threading.Interlocked.Increment(ref Found);
                    }
                }
                finally
                {
                    ping.Dispose();
                    sem.Release();
                    int d = System.Threading.Interlocked.Increment(ref done);
                    Progress = 70.0 * d / ips.Length;
                }
            }));
        }
        System.Threading.Tasks.Task.WaitAll(tasks.ToArray());
        return results.ToArray();
    }

    // Noms des appareils (ceux que la box connaît). Retourne "ip|nom".
    public static string[] ResolveNames(string[] ips, int timeoutMs)
    {
        var tasks = new List<System.Threading.Tasks.Task<string>>();
        foreach (string ip in ips)
        {
            string a = ip;
            tasks.Add(System.Threading.Tasks.Task.Run(() =>
            {
                try { return a + "|" + System.Net.Dns.GetHostEntry(a).HostName; } catch { return a + "|"; }
            }));
        }
        try { System.Threading.Tasks.Task.WaitAll(tasks.ToArray(), timeoutMs); } catch { }
        var list = new List<string>();
        foreach (var t in tasks) if (t.Status == System.Threading.Tasks.TaskStatus.RanToCompletion) list.Add(t.Result);
        return list.ToArray();
    }

    // Ports TCP ouverts sur un appareil (tentatives de connexion en parallèle).
    public static int[] ScanPorts(string ip, int[] ports, int timeoutMs)
    {
        var open = new List<int>();
        var sync = new object();
        var tasks = new List<System.Threading.Tasks.Task>();
        foreach (int port in ports)
        {
            int p = port;
            tasks.Add(System.Threading.Tasks.Task.Run(() =>
            {
                using (var client = new System.Net.Sockets.TcpClient())
                {
                    try
                    {
                        var t = client.ConnectAsync(ip, p);
                        if (t.Wait(timeoutMs) && client.Connected) { lock (sync) { open.Add(p); } }
                    }
                    catch { }
                }
            }));
        }
        try { System.Threading.Tasks.Task.WaitAll(tasks.ToArray()); } catch { }
        open.Sort();
        return open.ToArray();
    }

    // Tentative de connexion TCP sans bloquer de fil (pour scanner beaucoup de ports à la fois).
    static async System.Threading.Tasks.Task<bool> TryConnect(string ip, int port, int timeoutMs, System.Threading.SemaphoreSlim sem)
    {
        await sem.WaitAsync().ConfigureAwait(false);
        var client = new System.Net.Sockets.TcpClient();
        try
        {
            var t = client.ConnectAsync(ip, port);
            var observe = t.ContinueWith(x => { var ignore = x.Exception; }, System.Threading.Tasks.TaskContinuationOptions.OnlyOnFaulted);
            var w = await System.Threading.Tasks.Task.WhenAny(t, System.Threading.Tasks.Task.Delay(timeoutMs)).ConfigureAwait(false);
            return w == t && !t.IsFaulted && client.Connected;
        }
        catch { return false; }
        finally { client.Close(); sem.Release(); }
    }

    // Ports TCP ouverts sur plusieurs appareils. Retourne "ip|port". Fait avancer Progress de pFrom à pTo.
    public static string[] ScanHosts(string[] ips, int[] ports, int timeoutMs, double pFrom, double pTo)
    {
        var sem = new System.Threading.SemaphoreSlim(128);
        var jobs = new List<System.Threading.Tasks.Task<bool>>();
        var keys = new List<string>();
        int total = Math.Max(1, ips.Length * ports.Length);
        int[] done = new int[1];
        foreach (string ip in ips)
        {
            foreach (int port in ports)
            {
                keys.Add(ip + "|" + port);
                jobs.Add(TryConnect(ip, port, timeoutMs, sem).ContinueWith(x =>
                {
                    int d = System.Threading.Interlocked.Increment(ref done[0]);
                    Progress = pFrom + (pTo - pFrom) * d / total;
                    return x.Status == System.Threading.Tasks.TaskStatus.RanToCompletion && x.Result;
                }));
            }
        }
        try { System.Threading.Tasks.Task.WaitAll(jobs.ToArray()); } catch { }
        var open = new List<string>();
        for (int i = 0; i < jobs.Count; i++) if (jobs[i].Status == System.Threading.Tasks.TaskStatus.RanToCompletion && jobs[i].Result) open.Add(keys[i]);
        return open.ToArray();
    }

    public static void SetDarkTitleBar(IntPtr hwnd)
    {
        int on = 1;
        if (DwmSetWindowAttribute(hwnd, 20, ref on, 4) != 0) DwmSetWindowAttribute(hwnd, 19, ref on, 4);
    }

    // Retourne "nom|carte|largeur|hauteur|Hz actuels|Hz max|principal" pour chaque écran actif.
    public static string[] GetDisplays()
    {
        List<string> result = new List<string>();
        DISPLAY_DEVICE d = new DISPLAY_DEVICE();
        d.cb = Marshal.SizeOf(d);
        for (uint i = 0; EnumDisplayDevices(null, i, ref d, 0); i++)
        {
            if ((d.StateFlags & 1) != 0)
            {
                DEVMODE cur = new DEVMODE();
                cur.dmSize = (short)Marshal.SizeOf(cur);
                if (EnumDisplaySettings(d.DeviceName, -1, ref cur))
                {
                    int max = cur.dmDisplayFrequency;
                    DEVMODE m = new DEVMODE();
                    m.dmSize = (short)Marshal.SizeOf(m);
                    for (int n = 0; EnumDisplaySettings(d.DeviceName, n, ref m); n++)
                    {
                        if (m.dmPelsWidth == cur.dmPelsWidth && m.dmPelsHeight == cur.dmPelsHeight && m.dmDisplayFrequency > max)
                            max = m.dmDisplayFrequency;
                    }
                    result.Add(d.DeviceName + "|" + d.DeviceString + "|" + cur.dmPelsWidth + "|" + cur.dmPelsHeight + "|" + cur.dmDisplayFrequency + "|" + max + "|" + (((d.StateFlags & 4) != 0) ? "1" : "0"));
                }
            }
            d = new DISPLAY_DEVICE();
            d.cb = Marshal.SizeOf(d);
        }
        return result.ToArray();
    }
}
'@

# ---------------------------------------------------------------------------
# Données et sauvegarde
# ---------------------------------------------------------------------------
$DataDir    = Join-Path $env:LOCALAPPDATA 'OptiGame'
$BackupFile = Join-Path $DataDir 'sauvegarde.json'
$LogFile    = Join-Path $DataDir 'journal.txt'
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null

function Write-Log([string]$Message) {
    try { Add-Content -Path $LogFile -Value "$(Get-Date -Format s) $Message" -Encoding UTF8 } catch {}
}

function Import-Backup {
    $script:Backup = @{ Registry = @{}; PowerScheme = $null; CreatedScheme = $null; Overlay = $null; Dns = @{}; Displays = @{} }
    if (-not (Test-Path $BackupFile)) { return }
    try {
        $j = Get-Content $BackupFile -Raw -Encoding UTF8 | ConvertFrom-Json
        if ($j.Registry) {
            foreach ($p in $j.Registry.PSObject.Properties) {
                $v = $p.Value
                $script:Backup.Registry[$p.Name] = @{ Path = $v.Path; Name = $v.Name; Existed = [bool]$v.Existed; Value = $v.Value; Kind = $v.Kind }
            }
        }
        if ($j.PowerScheme)   { $script:Backup.PowerScheme = [string]$j.PowerScheme }
        if ($j.CreatedScheme) { $script:Backup.CreatedScheme = [string]$j.CreatedScheme }
        if ($j.Dns) { foreach ($p in $j.Dns.PSObject.Properties) { $script:Backup.Dns[$p.Name] = [string]$p.Value } }
        if ($j.Displays) { foreach ($p in $j.Displays.PSObject.Properties) { $script:Backup.Displays[$p.Name] = [int]$p.Value } }
        if ($j.Overlay) { $script:Backup.Overlay = [string]$j.Overlay }
    } catch { Write-Log "Lecture de la sauvegarde impossible: $_" }
}

# Points que l'utilisateur a choisi d'ignorer (ex: un écran volontairement en 60 Hz).
$IgnoreFile = Join-Path $DataDir 'ignores.json'

function Import-Ignored {
    $script:Ignored = @()
    if (Test-Path $IgnoreFile) {
        try { $arr = ConvertFrom-Json (Get-Content $IgnoreFile -Raw -Encoding UTF8); $script:Ignored = @(@($arr) | ForEach-Object { [string]$_ }) } catch {}
    }
}

function Save-Ignored {
    ConvertTo-Json -InputObject @($script:Ignored) | Set-Content -Path $IgnoreFile -Encoding UTF8
}

function Set-DisplayRate([string]$Device, [int]$Hz) {
    $current = @([OGNative]::GetDisplays()) | Where-Object { ($_ -split '\|')[0] -eq $Device } | Select-Object -First 1
    if ($current -and -not $script:Backup.Displays.ContainsKey($Device)) {
        $script:Backup.Displays[$Device] = [int](($current -split '\|')[4])
        Save-Backup
    }
    $r = [OGNative]::SetRefreshRate($Device, $Hz)
    if ($r -ne 0) { throw "Windows a refusé le changement de fréquence (code $r)." }
    if ($null -ne $script:RunLog -and $current) {
        [void]$script:RunLog.Add(@{ Type = 'display'; Device = $Device; Hz = [int](($current -split '\|')[4]) })
    }
}

# État actuel d'une valeur du registre (pour pouvoir revenir en arrière).
function Get-RegState([string]$Path, [string]$Name) {
    $s = @{ Existed = $false; Value = $null; Kind = $null }
    if (Test-Path -LiteralPath $Path) {
        $item = Get-Item -LiteralPath $Path
        if ($item.GetValueNames() -contains $Name) {
            $s.Existed = $true
            $s.Value = $item.GetValue($Name, $null, 'DoNotExpandEnvironmentNames')
            $s.Kind = $item.GetValueKind($Name).ToString()
        }
    }
    $s
}

function Save-Backup {
    $script:Backup | ConvertTo-Json -Depth 6 | Set-Content -Path $BackupFile -Encoding UTF8
}

function Get-RegValue([string]$Path, [string]$Name) {
    try {
        $p = Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop
        return , $p.$Name
    } catch { return $null }
}

# Mémorise la valeur d'origine avant la première modification.
function Save-Original([string]$Path, [string]$Name) {
    $key = "$Path|$Name"
    if ($script:Backup.Registry.ContainsKey($key)) { return }
    $exists = $false; $val = $null; $kind = $null
    if (Test-Path -LiteralPath $Path) {
        $item = Get-Item -LiteralPath $Path
        if ($item.GetValueNames() -contains $Name) {
            $exists = $true
            $val = $item.GetValue($Name, $null, 'DoNotExpandEnvironmentNames')
            $kind = $item.GetValueKind($Name).ToString()
            if ($val -is [byte[]]) { $val = [int[]]$val }
        }
    }
    $script:Backup.Registry[$key] = @{ Path = $Path; Name = $Name; Existed = $exists; Value = $val; Kind = $kind }
    Save-Backup
}

# Ouvre une clé du registre à partir d'un chemin PowerShell (HKCU:\..., HKLM:\..., Registry::HKEY_USERS\...).
function Open-RegKey([string]$Path, [bool]$Create) {
    $pth = $Path -replace '^Registry::', ''
    $hive = $null; $sub = ''
    if ($pth -match '^(HKCU:|HKEY_CURRENT_USER)\\?(.*)$')     { $hive = [Microsoft.Win32.Registry]::CurrentUser; $sub = $matches[2] }
    elseif ($pth -match '^(HKLM:|HKEY_LOCAL_MACHINE)\\?(.*)$') { $hive = [Microsoft.Win32.Registry]::LocalMachine; $sub = $matches[2] }
    elseif ($pth -match '^HKEY_USERS\\?(.*)$')                 { $hive = [Microsoft.Win32.Registry]::Users; $sub = $matches[1] }
    if (-not $hive) { throw "Chemin de registre non pris en charge: $Path" }
    if ($Create) { return $hive.CreateSubKey($sub) }
    $hive.OpenSubKey($sub, $true)
}

# Écrit ou supprime une valeur, noms spéciaux compris (crochets, antislashs).
function Write-RegValue([string]$Path, [string]$Name, $Value, [string]$Kind) {
    $k = Open-RegKey $Path $true
    try { $k.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]$Kind) } finally { $k.Close() }
}

function Remove-RegValue([string]$Path, [string]$Name) {
    $k = Open-RegKey $Path $false
    if ($k) { try { $k.DeleteValue($Name, $false) } finally { $k.Close() } }
}

function Set-Reg([string]$Path, [string]$Name, $Value, [string]$Kind = 'DWord') {
    Save-Original $Path $Name
    if ($null -ne $script:RunLog) {
        $st = Get-RegState $Path $Name
        [void]$script:RunLog.Add(@{ Type = 'reg'; Path = $Path; Name = $Name; Existed = $st.Existed; Value = $st.Value; Kind = $st.Kind })
    }
    Write-RegValue $Path $Name $Value $Kind
}

# ---------------------------------------------------------------------------
# Optimisations gaming
# ---------------------------------------------------------------------------
$GuidPattern = '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}'
$HighPerfGuid = '8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c'
$BalancedGuid = '381b4222-f694-41f0-9685-ff5bb260df2e'
$BestPerfOverlay = 'ded574b5-45a0-4f42-8737-46345c09c238'
$UltimateGuid = 'e9a42b02-d5df-448d-aa00-03f14749eb61'

function Get-ActiveScheme {
    $o = powercfg /getactivescheme | Out-String
    if ($o -match "($GuidPattern)\s*\((.+)\)") { return @{ Guid = $matches[1].ToLower(); Name = $matches[2].Trim() } }
    $null
}

function Test-PowerPlan {
    $s = Get-ActiveScheme
    if (-not $s) { return $false }
    $good = @($HighPerfGuid, $UltimateGuid)
    if ($script:Backup.CreatedScheme) { $good += $script:Backup.CreatedScheme }
    if (($good -contains $s.Guid) -or ($s.Name -match 'perf|ultim|gaming')) { return $true }
    # Mode « Meilleures performances » (portables récents, plan Utilisation normale)
    try { return ([OGNative]::GetOverlay() -eq $BestPerfOverlay) } catch { return $false }
}

# Passe le curseur de Windows sur « Meilleures performances ». Ne marche qu'avec le plan Utilisation normale.
function Set-BestPerfOverlay {
    $s = Get-ActiveScheme
    if (-not $s -or $s.Guid -ne $BalancedGuid) { return $false }
    try { $prev = [OGNative]::GetOverlay() } catch { return $false }
    if (-not $prev) { return $false }
    if ([OGNative]::SetOverlay($BestPerfOverlay) -ne 0) { return $false }
    if (-not $script:Backup.Overlay) { $script:Backup.Overlay = $prev; Save-Backup }
    if ($null -ne $script:RunLog) { [void]$script:RunLog.Add(@{ Type = 'overlay'; Guid = $prev }) }
    $true
}

function Set-PowerPlan {
    # Sur un portable, on garde le plan de Windows et on pousse le curseur à fond:
    # c'est le réglage prévu par les fabricants (veille moderne, ventilation...).
    if ($script:IsLaptop -and (Set-BestPerfOverlay)) { return }
    $current = Get-ActiveScheme
    $target = $null
    if ((powercfg /list | Out-String) -match $HighPerfGuid) {
        $target = $HighPerfGuid
    } elseif ($script:Backup.CreatedScheme -and ((powercfg /list | Out-String) -match $script:Backup.CreatedScheme)) {
        $target = $script:Backup.CreatedScheme
    } else {
        $out = powercfg -duplicatescheme $HighPerfGuid | Out-String
        if ($out -match $GuidPattern) { $target = $matches[0].ToLower(); $script:Backup.CreatedScheme = $target }
    }
    if (-not $target) {
        if (Set-BestPerfOverlay) { return }
        throw "Ce PC ne propose pas le plan Performances élevées. Va dans Paramètres > Système > Alimentation et choisis le mode « Meilleures performances »."
    }
    if ($current -and -not $script:Backup.PowerScheme) { $script:Backup.PowerScheme = $current.Guid }
    Save-Backup
    if ($null -ne $script:RunLog -and $current) { [void]$script:RunLog.Add(@{ Type = 'power'; Guid = $current.Guid }) }
    powercfg /setactive $target | Out-Null
}

$MousePath = 'HKCU:\Control Panel\Mouse'

function Sync-Mouse {
    $speed = [int](Get-RegValue $MousePath 'MouseSpeed')
    $t1 = [int](Get-RegValue $MousePath 'MouseThreshold1')
    $t2 = [int](Get-RegValue $MousePath 'MouseThreshold2')
    [void][OGNative]::SystemParametersInfo(4, 0, [int[]]@($t1, $t2, $speed), 3)
}

function Disable-MouseAccel {
    Set-Reg $MousePath 'MouseSpeed' '0' 'String'
    Set-Reg $MousePath 'MouseThreshold1' '0' 'String'
    Set-Reg $MousePath 'MouseThreshold2' '0' 'String'
    Sync-Mouse
}

$AccessPaths = @(
    'HKCU:\Control Panel\Accessibility\StickyKeys',
    'HKCU:\Control Panel\Accessibility\Keyboard Response',
    'HKCU:\Control Panel\Accessibility\ToggleKeys'
)

function Test-AccessHotkeys {
    foreach ($p in $AccessPaths) {
        $f = Get-RegValue $p 'Flags'
        if ($null -ne $f -and (([int]$f) -band 4)) { return $false }
    }
    $true
}

function Disable-AccessHotkeys {
    foreach ($p in $AccessPaths) {
        $f = Get-RegValue $p 'Flags'
        if ($null -ne $f) { Set-Reg $p 'Flags' ([string](([int]$f) -band (-bnot 4))) 'String' }
    }
}

$DxPath = 'HKCU:\Software\Microsoft\DirectX\UserGpuPreferences'

function Enable-WindowedOptim {
    $cur = [string](Get-RegValue $DxPath 'DirectXUserGlobalSettings')
    $parts = @($cur -split ';' | Where-Object { $_ -and $_ -notmatch '^SwapEffectUpgradeEnable=' })
    $parts += 'SwapEffectUpgradeEnable=1'
    Set-Reg $DxPath 'DirectXUserGlobalSettings' (($parts -join ';') + ';') 'String'
}

$DoPath = 'Registry::HKEY_USERS\S-1-5-20\Software\Microsoft\Windows\CurrentVersion\DeliveryOptimization\Settings'

$Tweaks = @(
    @{
        Id = 'power'; What = 'Mettre Windows en mode performances : plan « Performances élevées » sur un PC fixe, mode « Meilleures performances » sur un portable.'; Impact = 'Important'
        Titre = "Plan d'alimentation"
        Description = "Le processeur reste à pleine vitesse au lieu de ralentir pour économiser l'énergie, ce qui réduit les micro saccades. Sur un portable, la batterie se vide plus vite: le réglage s'applique surtout quand il est branché."
        Ok = 'Mode performances actif: le processeur tourne à pleine vitesse.'
        Ko = "Le plan actuel laisse le processeur ralentir pour économiser l'énergie."
        Test = { Test-PowerPlan }; Apply = { Set-PowerPlan }
    },
    @{
        Id = 'dvr'; What = 'Couper l''option « Enregistrer ce qui s''est passé » de la Xbox Game Bar.'; Impact = 'Important'
        Titre = 'Enregistrement en arrière plan (Xbox Game Bar)'
        Description = "Coupe l'enregistrement permanent des 30 dernières secondes de jeu, qui coûte des FPS. Les captures manuelles (Win + Alt + R), ShadowPlay et Radeon ReLive continuent de fonctionner."
        Ok = "Désactivé: la Game Bar ne filme pas tes parties en continu."
        Ko = 'La Game Bar filme tes parties en continu, ce qui coûte des FPS.'
        Test = { (Get-RegValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\GameDVR' 'HistoricalCaptureEnabled') -ne 1 }
        Apply = { Set-Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\GameDVR' 'HistoricalCaptureEnabled' 0 }
    },
    @{
        Id = 'gamemode'; What = 'Activer le mode jeu de Windows.'; Impact = 'Moyen'
        Titre = 'Mode jeu Windows'
        Description = "Windows donne la priorité au jeu lancé et évite d'installer des mises à jour ou d'afficher des notifications pendant que tu joues."
        Ok = 'Activé.'
        Ko = 'Désactivé: Windows peut lancer des tâches de fond pendant tes parties.'
        Test = { (Get-RegValue 'HKCU:\Software\Microsoft\GameBar' 'AutoGameModeEnabled') -ne 0 }
        Apply = {
            Set-Reg 'HKCU:\Software\Microsoft\GameBar' 'AutoGameModeEnabled' 1
            Set-Reg 'HKCU:\Software\Microsoft\GameBar' 'AllowAutoGameMode' 1
        }
    },
    @{
        Id = 'hags'; What = 'Activer la planification GPU à accélération matérielle (prise en compte au prochain redémarrage).'; Impact = 'Moyen'; Reboot = $true
        Titre = 'Planification GPU à accélération matérielle'
        Description = "La carte graphique gère elle même sa mémoire: un peu moins de latence, et c'est obligatoire pour la génération d'images DLSS 3. Nécessite une carte récente (NVIDIA GTX 1000 ou plus, AMD RX 5000 ou plus)."
        Ok = 'Activée.'
        Ko = 'Désactivée (ou non prise en charge par ta carte graphique).'
        Test = { (Get-RegValue 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' 'HwSchMode') -eq 2 }
        Apply = { Set-Reg 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers' 'HwSchMode' 2 }
    },
    @{
        Id = 'windowed'; What = 'Activer « Optimisations pour les jeux en mode fenêtré » dans les paramètres graphiques de Windows.'; Impact = 'Moyen'; MinBuild = 22000
        Titre = 'Optimisations pour les jeux en mode fenêtré'
        Description = "Réduit la latence des jeux DirectX 10 et 11 lancés en fenêtré ou en plein écran sans bordure, et permet d'utiliser Auto HDR et le taux de rafraîchissement variable."
        Ok = 'Activées.'
        Ko = 'Désactivées: plus de latence en fenêtré et en plein écran sans bordure.'
        Test = { [string](Get-RegValue $DxPath 'DirectXUserGlobalSettings') -match 'SwapEffectUpgradeEnable=1' }
        Apply = { Enable-WindowedOptim }
    },
    @{
        Id = 'mouse'; What = 'Décocher « Améliorer la précision du pointeur » (effet immédiat, ta sensibilité en jeu ne change pas).'; Impact = 'Moyen'
        Titre = 'Accélération de la souris'
        Description = "Désactive « Améliorer la précision du pointeur ». Un même mouvement de la main donne toujours le même déplacement à l'écran: indispensable pour bien viser dans les FPS."
        Ok = 'Désactivée: ta visée est constante.'
        Ko = 'Activée: le curseur dépend de la vitesse de ton geste, mauvais pour viser.'
        Test = { [string](Get-RegValue $MousePath 'MouseSpeed') -eq '0' }
        Apply = { Disable-MouseAccel }
    },
    @{
        Id = 'hotkeys'; What = 'Désactiver les raccourcis clavier des touches rémanentes, filtres et bascules. Ces fonctions restent disponibles dans les paramètres d''accessibilité.'; Impact = 'Léger'
        Titre = "Raccourcis d'accessibilité"
        Description = "Empêche la fenêtre des touches rémanentes de s'ouvrir quand tu appuies 5 fois sur Maj en pleine partie (idem pour les touches filtres et bascules)."
        Ok = 'Désactivés: plus de fenêtre surprise en pleine partie.'
        Ko = "Appuyer 5 fois sur Maj ouvre une fenêtre qui te sort du jeu."
        Test = { Test-AccessHotkeys }; Apply = { Disable-AccessHotkeys }
    },
    @{
        Id = 'delivery'; What = 'Désactiver le partage des mises à jour Windows avec d''autres PC.'; Impact = 'Léger'
        Titre = 'Partage des mises à jour sur Internet'
        Description = "Empêche Windows d'envoyer ses mises à jour à d'autres PC via ta connexion, pour garder ta bande passante montante pour tes parties."
        Ok = 'Pas de partage sur Internet.'
        Ko = "Windows envoie des mises à jour à d'autres PC via ta connexion."
        Test = { (Get-RegValue $DoPath 'DownloadMode') -ne 3 }
        Apply = { Set-Reg $DoPath 'DownloadMode' 0 }
    },
    @{
        Id = 'transparency'; What = 'Désactiver les effets de transparence de Windows.'; Impact = 'Léger'; Recommended = $false
        Titre = 'Effets de transparence'
        Description = "Petit gain sur les PC modestes seulement. C'est surtout esthétique: garde les si ton PC est puissant."
        Ok = 'Désactivés.'
        Ko = 'Activés (impact très léger, surtout utile de les couper sur un PC modeste).'
        Test = { (Get-RegValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'EnableTransparency') -eq 0 }
        Apply = { Set-Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize' 'EnableTransparency' 0 }
    }
)

# Portable ou PC fixe ? Le type de boîtier déclaré par le PC passe avant la batterie:
# un PC fixe branché sur un onduleur USB a une « batterie » mais reste un PC fixe.
function Test-IsLaptop($Battery, $Data) {
    $mobileChassis  = 8, 9, 10, 11, 12, 14, 18, 21, 30, 31, 32
    $desktopChassis = 3, 4, 5, 6, 7, 13, 15, 16, 17, 23, 24, 35, 36
    if ($Data) { $chassis = @($Data.Chassis); $pcType = [int]$Data.PCType }
    else {
        $chassis = @((Get-CimInstance Win32_SystemEnclosure -ErrorAction SilentlyContinue).ChassisTypes | ForEach-Object { [int]$_ })
        $pcType = [int](Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue).PCSystemType
    }
    if ($pcType -eq 2 -or ($chassis | Where-Object { $mobileChassis -contains $_ })) { return $true }
    if ($chassis | Where-Object { $desktopChassis -contains $_ }) { return $false }
    @($Battery).Count -gt 0
}

function Get-AvailableTweaks {
    $Tweaks | Where-Object { -not $_.MinBuild -or $script:Build -ge $_.MinBuild }
}

function Test-Tweak($Tweak) {
    try { [bool](& $Tweak.Test) } catch { $false }
}

function Get-TweakWeight($Tweak) {
    if ($Tweak.Recommended -eq $false) { return 0 }
    switch ($Tweak.Impact) { 'Important' { 3 } 'Moyen' { 2 } default { 1 } }
}

# ---------------------------------------------------------------------------
# Programmes au démarrage
# ---------------------------------------------------------------------------
$ApprovedRoot = 'Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved'
$StartupSources = @(
    @{ Kind = 'Reg'; Path = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'; Approved = "HKCU:\$ApprovedRoot\Run"; Label = 'Utilisateur' },
    @{ Kind = 'Reg'; Path = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Run'; Approved = "HKLM:\$ApprovedRoot\Run"; Label = 'Tous les utilisateurs' },
    @{ Kind = 'Reg'; Path = 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run'; Approved = "HKLM:\$ApprovedRoot\Run32"; Label = 'Tous les utilisateurs (32 bits)' },
    @{ Kind = 'Folder'; Path = [Environment]::GetFolderPath('Startup'); Approved = "HKCU:\$ApprovedRoot\StartupFolder"; Label = 'Dossier Démarrage' },
    @{ Kind = 'Folder'; Path = [Environment]::GetFolderPath('CommonStartup'); Approved = "HKLM:\$ApprovedRoot\StartupFolder"; Label = 'Dossier Démarrage commun' }
)

function Test-StartupEnabled([string]$Approved, [string]$Name) {
    $v = Get-RegValue $Approved $Name
    if ($v -is [byte[]] -and $v.Length -gt 0) { return -not ($v[0] -band 1) }
    $true
}

# Retrouve le programme lancé par une entrée de démarrage (ou $null si ce n'est pas un vrai programme).
function Resolve-StartupExe([string]$Command) {
    if (-not $Command) { return $null }
    $cmd = [Environment]::ExpandEnvironmentVariables($Command.Trim())
    if ($cmd -match '^"([^"]+)"') { $exe = $matches[1] }
    elseif ($cmd -match '^(.+?\.(exe|lnk|bat|cmd|url))(\s|$)') { $exe = $matches[1] }
    else { $exe = $cmd }
    if ($exe -notmatch '[\\/]') { $exe = Join-Path "$env:windir\System32" $exe }
    try { if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { return $null } } catch { return $null }
    if ($exe -like '*.lnk') {
        try {
            $t = (New-Object -ComObject WScript.Shell).CreateShortcut($exe).TargetPath
            if ($t -and (Test-Path -LiteralPath $t -PathType Leaf)) { return $t }
        } catch {}
    }
    $exe
}

function Get-StartupItems {
    foreach ($s in $StartupSources) {
        if (-not $s.Path -or -not (Test-Path -LiteralPath $s.Path)) { continue }
        $entries = @()
        if ($s.Kind -eq 'Reg') {
            $key = Get-Item -LiteralPath $s.Path
            foreach ($n in $key.GetValueNames()) {
                if ($n) { $entries += @{ Name = $n; Display = $n; Command = [string]$key.GetValue($n) } }
            }
        } else {
            foreach ($f in (Get-ChildItem -LiteralPath $s.Path -File -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne 'desktop.ini' })) {
                $entries += @{ Name = $f.Name; Display = $f.BaseName; Command = $f.FullName }
            }
        }
        foreach ($e in $entries) {
            $exe = Resolve-StartupExe $e.Command
            if (-not $exe) { continue }
            $enabled = Test-StartupEnabled $s.Approved $e.Name
            [pscustomobject]@{
                Nom       = $e.Display
                Etat      = if ($enabled) { 'Activé' } else { 'Désactivé' }
                Source    = $s.Label
                Commande  = $e.Command
                Approved  = $s.Approved
                ValueName = $e.Name
                Enabled   = $enabled
                Exe       = $exe
            }
        }
    }
}

# Jeux installés via Steam et Epic Games, avec leurs exécutables probables.
function Get-InstalledGames {
    $bad = 'unins|setup|install|redist|dxsetup|directx|crash|report|easyanticheat|anticheat|eac_|beservice|battleye|update|helper|prereq|dotnet|webhelper|vcredist|python|java|browser|error|cleanup|touchup|repair|bootstrapper|resourcecompiler|^ui(32|64)$|diagnos|benchmark_?tool'
    $notGames = '^(wallpaper_engine|Steamworks Shared|SteamVR|Steam Controller Configs|Steamworks Common Redistributables)$'
    $games = @()
    $dirs = @()

    $steam = (Get-ItemProperty 'HKCU:\Software\Valve\Steam' -ErrorAction SilentlyContinue).SteamPath
    if ($steam) {
        $steam = $steam -replace '/', '\'
        $libs = @($steam)
        $vdf = Join-Path $steam 'steamapps\libraryfolders.vdf'
        if (Test-Path -LiteralPath $vdf) {
            foreach ($m in [regex]::Matches((Get-Content -LiteralPath $vdf -Raw), '"path"\s+"([^"]+)"')) { $libs += ($m.Groups[1].Value -replace '\\\\', '\') }
        }
        $seen = @{}
        foreach ($lib in $libs) {
            $key = $lib.TrimEnd('\').ToLower()
            if ($seen.ContainsKey($key)) { continue }
            $seen[$key] = $true
            $common = Join-Path $lib 'steamapps\common'
            if (Test-Path -LiteralPath $common) {
                $dirs += Get-ChildItem -LiteralPath $common -Directory -ErrorAction SilentlyContinue |
                    Where-Object { $_.Name -notmatch $notGames } |
                    ForEach-Object { @{ Name = $_.Name; Dir = $_.FullName; Exe = $null } }
            }
        }
    }
    foreach ($m in @(Get-ChildItem "$env:ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item" -ErrorAction SilentlyContinue)) {
        try {
            $j = Get-Content -LiteralPath $m.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($j.InstallLocation -and (Test-Path -LiteralPath $j.InstallLocation)) {
                $exe = if ($j.LaunchExecutable) { Join-Path $j.InstallLocation $j.LaunchExecutable } else { $null }
                $dirs += @{ Name = $j.DisplayName; Dir = $j.InstallLocation; Exe = $exe }
            }
        } catch {}
    }

    foreach ($d in $dirs) {
        $exes = @(Get-ChildItem -LiteralPath $d.Dir -Filter *.exe -Recurse -Depth 3 -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Length -gt 200KB -and $_.BaseName -notmatch $bad } | Select-Object -First 6 | ForEach-Object { $_.FullName })
        if ($d.Exe -and (Test-Path -LiteralPath $d.Exe)) { $exes = @($d.Exe) + $exes }
        $exes = @($exes | Select-Object -Unique)
        if ($exes.Count) { $games += @{ Name = $d.Name; Exes = $exes } }
    }
    $games
}

# Préférence de carte graphique d'un exécutable (Paramètres > Écran > Graphiques). 2 = hautes performances.
function Get-GpuPreference([string]$Exe) {
    $k = Open-RegKey $DxPath $false
    if (-not $k) { return $null }
    try { [string]$k.GetValue($Exe) } finally { $k.Close() }
}

$SafeStartup = '\b(Blitz|Discord|Steam|Epic ?Games|EpicGamesLauncher|Spotify|OneDrive|Teams|Skype|EADesktop|EA app|Origin|Battle\.net|Ubisoft|Uplay|GOG Galaxy|GalaxyClient|Riot ?Client|Overwolf|Medal|Zoom|WhatsApp|Telegram|Messenger|CCleaner|MicrosoftEdgeAutoLaunch|Opera|Brave|Adobe Creative Cloud|CCXProcess|AdobeGCInvoker)\b|MicrosoftEdgeAutoLaunch|\bEA\b|EALauncher'

function Set-StartupState($Item, [bool]$Enable) {
    $first = if ($Enable) { 2 } else { 3 }
    Set-Reg $Item.Approved $Item.ValueName ([byte[]]@($first, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)) 'Binary'
}

# ---------------------------------------------------------------------------
# Réseau
# ---------------------------------------------------------------------------
function Get-ActiveNet {
    $n = Invoke-Async $ActiveNetWork | Select-Object -First 1
    if ($n) { $n } else { $null }
}

$DnsChoices = @(
    @(),
    @('1.1.1.1', '1.0.0.1'),
    @('8.8.8.8', '8.8.4.4'),
    @('9.9.9.9', '149.112.112.112')
)

# ---------------------------------------------------------------------------
# Nettoyage
# ---------------------------------------------------------------------------
$CleanTargets = @(
    @{ Titre = 'Fichiers temporaires (utilisateur)'; Paths = @($env:TEMP) },
    @{ Titre = 'Fichiers temporaires (Windows)'; Paths = @("$env:windir\Temp") },
    @{ Titre = 'Fichiers de mises à jour Windows déjà installées'; Paths = @("$env:windir\SoftwareDistribution\Download") },
    @{ Titre = "Cache d'optimisation de la distribution"; Paths = @("$env:windir\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache") },
    @{ Titre = "Rapports d'erreurs Windows"; Paths = @("$env:ProgramData\Microsoft\Windows\WER\ReportArchive", "$env:ProgramData\Microsoft\Windows\WER\ReportQueue") },
    @{ Titre = 'Anciens rapports de plantage (minidumps)'; Paths = @("$env:windir\Minidump") }
)

$SizeScript = {
    param($paths)
    $sum = 0
    foreach ($p in $paths) {
        if (Test-Path -LiteralPath $p) {
            $m = Get-ChildItem -LiteralPath $p -Recurse -Force -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum
            if ($m.Sum) { $sum += $m.Sum }
        }
    }
    [double]$sum
}

$CleanScript = {
    param($paths)
    foreach ($p in $paths) {
        if (Test-Path -LiteralPath $p) {
            Get-ChildItem -LiteralPath $p -Force -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Format-Size([double]$Bytes) {
    if ($Bytes -ge 1TB) { return '{0:N1} To' -f ($Bytes / 1TB) }
    if ($Bytes -ge 1GB) { return '{0:N1} Go' -f ($Bytes / 1GB) }
    if ($Bytes -ge 1MB) { return '{0:N0} Mo' -f ($Bytes / 1MB) }
    '{0:N0} Ko' -f ($Bytes / 1KB)
}

# ---------------------------------------------------------------------------
# Restauration de tous les réglages modifiés par OptiGame
# ---------------------------------------------------------------------------
function Restore-AllSettings {
    $errors = @()
    foreach ($e in @($script:Backup.Registry.Values)) {
        try {
            if ($e.Existed) {
                $val = switch ($e.Kind) {
                    'Binary' { [byte[]]@($e.Value) }
                    'DWord'  { [int]$e.Value }
                    'QWord'  { [long]$e.Value }
                    default  { $e.Value }
                }
                Write-RegValue $e.Path $e.Name $val $e.Kind
            } else {
                Remove-RegValue $e.Path $e.Name
            }
        } catch { $errors += "$($e.Path)\$($e.Name): $($_.Exception.Message)" }
    }
    if ($script:Backup.PowerScheme) { powercfg /setactive $script:Backup.PowerScheme | Out-Null }
    if ($script:Backup.Overlay) { try { [void][OGNative]::SetOverlay($script:Backup.Overlay) } catch { $errors += "Mode d'alimentation: $($_.Exception.Message)" } }
    if ($script:Backup.CreatedScheme -and $script:Backup.CreatedScheme -ne $script:Backup.PowerScheme) {
        powercfg /delete $script:Backup.CreatedScheme 2>$null | Out-Null
    }
    foreach ($k in @($script:Backup.Dns.Keys)) {
        try {
            $v = $script:Backup.Dns[$k]
            if ($v) { Set-DnsClientServerAddress -InterfaceIndex ([int]$k) -ServerAddresses @($v -split '[,\s]+' | Where-Object { $_ }) -ErrorAction Stop }
            else { Set-DnsClientServerAddress -InterfaceIndex ([int]$k) -ResetServerAddresses -ErrorAction Stop }
        } catch { $errors += "DNS: $($_.Exception.Message)" }
    }
    foreach ($k in @($script:Backup.Displays.Keys)) {
        $r = [OGNative]::SetRefreshRate($k, [int]$script:Backup.Displays[$k])
        if ($r -ne 0) { $errors += "Écran $($k): fréquence non restaurée (code $r)" }
    }
    Sync-Mouse
    Remove-Item $BackupFile -ErrorAction SilentlyContinue
    Import-Backup
    , $errors
}

# ---------------------------------------------------------------------------
# Désinstallation
# ---------------------------------------------------------------------------
function Invoke-Uninstall {
    Import-Backup
    $n = $script:Backup.Registry.Count + $script:Backup.Dns.Count + $script:Backup.Displays.Count + $(if ($script:Backup.PowerScheme) { 1 } else { 0 }) + $(if ($script:Backup.Overlay) { 1 } else { 0 })
    $steps = @()
    if ($n) { $steps += "  - remettre les $n réglage$(if ($n -gt 1) {'s'}) de Windows modifié$(if ($n -gt 1) {'s'}) par OptiGame comme avant" }
    $steps += "  - supprimer ses données (sauvegarde, journal, préférences)"
    $steps += "  - supprimer les fichiers d'OptiGame de ce dossier"
    $q = "Désinstaller OptiGame ?`n`nL'application va :`n" + ($steps -join "`n") + "`n`nLes points de restauration Windows sont conservés."
    if ([System.Windows.MessageBox]::Show($q, 'Désinstaller OptiGame', 'YesNo', 'Question') -ne 'Yes') { return }

    $errors = @()
    if ($n) { $errors += Restore-AllSettings }
    try { Remove-Item -LiteralPath $DataDir -Recurse -Force -ErrorAction Stop } catch { $errors += "Données: $($_.Exception.Message)" }

    # Fichiers de l'application: uniquement ceux livrés avec OptiGame, jamais le reste du dossier.
    $here = $PSScriptRoot
    $root = if ((Split-Path $here -Leaf) -eq 'fichiers') { Split-Path $here -Parent } else { $here }
    $files = @(
        (Join-Path $root 'OptiGame.exe'),
        (Join-Path $root 'Désinstaller OptiGame.exe'),
        (Join-Path $root 'LISEZMOI.txt'),
        (Join-Path $here 'OptiGame.ps1'),
        (Join-Path $here 'OptiGame.ico'),
        (Join-Path $here 'Lancer OptiGame (secours).bat'),
        (Join-Path $here 'Désinstaller OptiGame (secours).bat')
    ) | Select-Object -Unique | Where-Object { Test-Path -LiteralPath $_ }

    $msg = 'OptiGame est désinstallé.'
    if ($n) { $msg += "`n`nRedémarre le PC pour que tous les réglages d'origine soient pris en compte." }
    if ($errors) { $msg += "`n`nCertains éléments n'ont pas pu être restaurés :`n" + ($errors -join "`n") }
    [System.Windows.MessageBox]::Show($msg, 'OptiGame', 'OK', 'Information') | Out-Null

    # Les fichiers sont supprimés juste après la fermeture de ce script (ils sont en cours d'utilisation).
    $cmd = 'ping 127.0.0.1 -n 4 >nul'
    foreach ($f in $files) { $cmd += " & del /f /q `"$f`"" }
    if ($here -ne $root) { $cmd += " & rmdir `"$here`"" }
    $cmd += " & rmdir `"$root`""
    Start-Process -FilePath 'cmd.exe' -ArgumentList '/c', $cmd -WindowStyle Hidden -WorkingDirectory $env:TEMP
}

if ($Uninstall) {
    Invoke-Uninstall
    exit
}

# ---------------------------------------------------------------------------
# Interface
# ---------------------------------------------------------------------------
[xml]$Xaml = @'
<Window xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="OptiGame" Width="1120" Height="740" MinWidth="920" MinHeight="600"
        WindowStartupLocation="CenterScreen" Background="#0E1014"
        FontFamily="Segoe UI" Foreground="#E6E8EE">
  <Window.Resources>
    <Style x:Key="BtnPrimary" TargetType="Button">
      <Setter Property="Background" Value="#22D37A"/>
      <Setter Property="Foreground" Value="#0B0D10"/>
      <Setter Property="FontWeight" Value="SemiBold"/>
      <Setter Property="FontSize" Value="13"/>
      <Setter Property="Padding" Value="16,9"/>
      <Setter Property="Cursor" Value="Hand"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Button">
            <Border x:Name="B" Background="{TemplateBinding Background}" CornerRadius="8" Padding="{TemplateBinding Padding}">
              <ContentPresenter HorizontalAlignment="Center" VerticalAlignment="Center"/>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="B" Property="Opacity" Value="0.85"/></Trigger>
              <Trigger Property="IsEnabled" Value="False"><Setter TargetName="B" Property="Opacity" Value="0.4"/></Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>
    <Style x:Key="BtnSecondary" TargetType="Button" BasedOn="{StaticResource BtnPrimary}">
      <Setter Property="Background" Value="#262C38"/>
      <Setter Property="Foreground" Value="#E6E8EE"/>
    </Style>
    <Style x:Key="Card" TargetType="Border">
      <Setter Property="Background" Value="#181C24"/>
      <Setter Property="BorderBrush" Value="#232937"/>
      <Setter Property="BorderThickness" Value="1"/>
      <Setter Property="CornerRadius" Value="12"/>
      <Setter Property="Padding" Value="18"/>
    </Style>
    <Style x:Key="H1" TargetType="TextBlock">
      <Setter Property="FontSize" Value="24"/>
      <Setter Property="FontWeight" Value="Bold"/>
      <Setter Property="Foreground" Value="White"/>
    </Style>
    <Style x:Key="H2" TargetType="TextBlock">
      <Setter Property="FontSize" Value="16"/>
      <Setter Property="FontWeight" Value="SemiBold"/>
      <Setter Property="Foreground" Value="White"/>
    </Style>
    <Style x:Key="Sub" TargetType="TextBlock">
      <Setter Property="FontSize" Value="13"/>
      <Setter Property="Foreground" Value="#9AA3B2"/>
      <Setter Property="TextWrapping" Value="Wrap"/>
      <Setter Property="Margin" Value="0,4,0,0"/>
    </Style>
    <Style TargetType="CheckBox">
      <Setter Property="Foreground" Value="#E6E8EE"/>
      <Setter Property="Cursor" Value="Hand"/>
    </Style>
    <Style TargetType="TabItem">
      <Setter Property="Foreground" Value="#9AA3B2"/>
      <Setter Property="FontSize" Value="14"/>
      <Setter Property="Cursor" Value="Hand"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="TabItem">
            <Border x:Name="Bd" Background="Transparent" BorderBrush="Transparent" BorderThickness="3,0,0,0" CornerRadius="6" Padding="14,10" Margin="0,2">
              <ContentPresenter ContentSource="Header"/>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="IsMouseOver" Value="True">
                <Setter Property="Foreground" Value="White"/>
              </Trigger>
              <Trigger Property="Tag" Value="parent">
                <Setter TargetName="Bd" Property="Background" Value="#1C212B"/>
                <Setter TargetName="Bd" Property="BorderBrush" Value="#22D37A"/>
                <Setter Property="Foreground" Value="White"/>
              </Trigger>
              <Trigger Property="IsSelected" Value="True">
                <Setter TargetName="Bd" Property="Background" Value="#1C212B"/>
                <Setter TargetName="Bd" Property="BorderBrush" Value="#22D37A"/>
                <Setter Property="Foreground" Value="White"/>
              </Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>
    <!-- Barres de défilement sombres -->
    <SolidColorBrush x:Key="{x:Static SystemColors.ControlBrushKey}" Color="#181C24"/>
    <SolidColorBrush x:Key="{x:Static SystemColors.ControlLightBrushKey}" Color="#181C24"/>
    <SolidColorBrush x:Key="{x:Static SystemColors.ControlLightLightBrushKey}" Color="#181C24"/>
    <SolidColorBrush x:Key="{x:Static SystemColors.ControlDarkBrushKey}" Color="#181C24"/>
    <SolidColorBrush x:Key="{x:Static SystemColors.ControlDarkDarkBrushKey}" Color="#181C24"/>
    <Style x:Key="ScrollThumb" TargetType="Thumb">
      <Setter Property="OverridesDefaultStyle" Value="True"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="Thumb">
            <Border x:Name="T" Background="#343C4C" CornerRadius="4"/>
            <ControlTemplate.Triggers>
              <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="T" Property="Background" Value="#4A5468"/></Trigger>
              <Trigger Property="IsDragging" Value="True"><Setter TargetName="T" Property="Background" Value="#22D37A"/></Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>
    <Style x:Key="ScrollPage" TargetType="RepeatButton">
      <Setter Property="OverridesDefaultStyle" Value="True"/>
      <Setter Property="Focusable" Value="False"/>
      <Setter Property="IsTabStop" Value="False"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="RepeatButton"><Border Background="Transparent"/></ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>
    <Style TargetType="ScrollBar">
      <Setter Property="OverridesDefaultStyle" Value="True"/>
      <Setter Property="Width" Value="10"/>
      <Setter Property="MinWidth" Value="10"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="ScrollBar">
            <Border Background="Transparent" Padding="2,2,2,2">
              <Track x:Name="PART_Track" Orientation="Vertical" IsDirectionReversed="True">
                <Track.DecreaseRepeatButton><RepeatButton Style="{StaticResource ScrollPage}" Command="ScrollBar.PageUpCommand"/></Track.DecreaseRepeatButton>
                <Track.Thumb><Thumb Style="{StaticResource ScrollThumb}" MinHeight="30"/></Track.Thumb>
                <Track.IncreaseRepeatButton><RepeatButton Style="{StaticResource ScrollPage}" Command="ScrollBar.PageDownCommand"/></Track.IncreaseRepeatButton>
              </Track>
            </Border>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
      <Style.Triggers>
        <Trigger Property="Orientation" Value="Horizontal">
          <Setter Property="Width" Value="Auto"/>
          <Setter Property="MinWidth" Value="0"/>
          <Setter Property="Height" Value="10"/>
          <Setter Property="MinHeight" Value="10"/>
          <Setter Property="Template">
            <Setter.Value>
              <ControlTemplate TargetType="ScrollBar">
                <Border Background="Transparent" Padding="2,2,2,2">
                  <Track x:Name="PART_Track" Orientation="Horizontal" IsDirectionReversed="False">
                    <Track.DecreaseRepeatButton><RepeatButton Style="{StaticResource ScrollPage}" Command="ScrollBar.PageLeftCommand"/></Track.DecreaseRepeatButton>
                    <Track.Thumb><Thumb Style="{StaticResource ScrollThumb}" MinWidth="30"/></Track.Thumb>
                    <Track.IncreaseRepeatButton><RepeatButton Style="{StaticResource ScrollPage}" Command="ScrollBar.PageRightCommand"/></Track.IncreaseRepeatButton>
                  </Track>
                </Border>
              </ControlTemplate>
            </Setter.Value>
          </Setter>
        </Trigger>
      </Style.Triggers>
    </Style>    <Style x:Key="Switch" TargetType="CheckBox">
      <Setter Property="Cursor" Value="Hand"/>
      <Setter Property="Focusable" Value="False"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="CheckBox">
            <Border x:Name="Track" Width="46" Height="26" CornerRadius="13" Background="#343C4C">
              <Ellipse x:Name="Knob" Width="20" Height="20" Fill="#C9CED8" HorizontalAlignment="Left" Margin="3,0,0,0"/>
            </Border>
            <ControlTemplate.Triggers>
              <Trigger Property="IsChecked" Value="True">
                <Setter TargetName="Track" Property="Background" Value="#22D37A"/>
                <Setter TargetName="Knob" Property="HorizontalAlignment" Value="Right"/>
                <Setter TargetName="Knob" Property="Margin" Value="0,0,3,0"/>
                <Setter TargetName="Knob" Property="Fill" Value="White"/>
              </Trigger>
              <Trigger Property="IsMouseOver" Value="True"><Setter TargetName="Track" Property="Opacity" Value="0.85"/></Trigger>
            </ControlTemplate.Triggers>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>
    <Style TargetType="ProgressBar">
      <Setter Property="Height" Value="8"/>
      <Setter Property="Maximum" Value="100"/>
      <Setter Property="Foreground" Value="#22D37A"/>
      <Setter Property="Background" Value="#232937"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="ProgressBar">
            <Grid>
              <Border x:Name="PART_Track" Background="{TemplateBinding Background}" CornerRadius="4"/>
              <Border x:Name="PART_Indicator" Background="{TemplateBinding Foreground}" CornerRadius="4" HorizontalAlignment="Left"/>
            </Grid>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>
    <Style TargetType="GridViewColumnHeader">
      <Setter Property="Foreground" Value="#9AA3B2"/>
      <Setter Property="Template">
        <Setter.Value>
          <ControlTemplate TargetType="GridViewColumnHeader">
            <Border Background="#12151B" BorderBrush="#232937" BorderThickness="0,0,1,1" Padding="8,7">
              <ContentPresenter HorizontalAlignment="Left"/>
            </Border>
          </ControlTemplate>
        </Setter.Value>
      </Setter>
    </Style>
  </Window.Resources>

  <Grid>
    <Grid.RowDefinitions>
      <RowDefinition Height="Auto"/>
      <RowDefinition Height="*"/>
      <RowDefinition Height="Auto"/>
    </Grid.RowDefinitions>

    <!-- Bandeau « nouvelle version disponible » -->
    <Border x:Name="UpdateBanner" Visibility="Collapsed" Background="#12281F" BorderBrush="#22D37A" BorderThickness="0,0,0,1" Padding="18,10">
      <DockPanel>
        <StackPanel DockPanel.Dock="Right" Orientation="Horizontal" VerticalAlignment="Center">
          <Button x:Name="BtnUpdateLater" Style="{StaticResource BtnSecondary}" Content="Plus tard" Margin="0,0,10,0"/>
          <Button x:Name="BtnUpdate" Style="{StaticResource BtnPrimary}" Content="Mettre à jour"/>
        </StackPanel>
        <StackPanel Orientation="Horizontal" VerticalAlignment="Center">
          <Ellipse Width="9" Height="9" Fill="#22D37A" Margin="0,0,10,0" VerticalAlignment="Center"/>
          <TextBlock x:Name="UpdateText" Foreground="White" FontSize="13.5" FontWeight="SemiBold" VerticalAlignment="Center"/>
        </StackPanel>
      </DockPanel>
    </Border>

    <TabControl x:Name="Tabs" Grid.Row="1" TabStripPlacement="Left" Background="Transparent" BorderThickness="0">
      <TabControl.Template>
        <ControlTemplate TargetType="TabControl">
          <Grid>
            <Grid.ColumnDefinitions>
              <ColumnDefinition Width="230"/>
              <ColumnDefinition Width="*"/>
            </Grid.ColumnDefinitions>
            <Border Background="#12151B" BorderBrush="#232937" BorderThickness="0,0,1,0">
              <DockPanel Margin="14,22,14,16">
                <DockPanel DockPanel.Dock="Top" Margin="6,0,0,26">
                  <Image x:Name="LogoImg" Width="42" Height="42" Margin="0,0,10,0" VerticalAlignment="Center" RenderOptions.BitmapScalingMode="HighQuality"/>
                  <StackPanel VerticalAlignment="Center">
                    <TextBlock FontSize="22" FontWeight="Bold"><Run Text="Opti" Foreground="White"/><Run Text="Game" Foreground="#22D37A"/></TextBlock>
                    <TextBlock Text="Optimisation gaming" Foreground="#9AA3B2" FontSize="12"/>
                  </StackPanel>
                </DockPanel>
                <TextBlock x:Name="VersionText" DockPanel.Dock="Bottom" Foreground="#5B6475" FontSize="11" Margin="8,0,0,0"/>
                <StackPanel IsItemsHost="True"/>
              </DockPanel>
            </Border>
            <Grid Grid.Column="1">
              <Grid.RowDefinitions>
                <RowDefinition Height="Auto"/>
                <RowDefinition Height="*"/>
              </Grid.RowDefinitions>
              <DockPanel x:Name="NavBar" Margin="28,14,28,0" Visibility="Collapsed" LastChildFill="False">
                <Button x:Name="NavBack" Style="{StaticResource BtnSecondary}" Content="←  Ordinateur" Padding="12,6" FontSize="12.5"/>
                <TextBlock Text="›" Foreground="#5B6475" FontSize="16" Margin="12,0,10,2" VerticalAlignment="Center"/>
                <TextBlock x:Name="NavCrumb" Foreground="#9AA3B2" FontSize="13" VerticalAlignment="Center"/>
              </DockPanel>
              <ContentPresenter Grid.Row="1" ContentSource="SelectedContent" Margin="28,18,28,16"/>
            </Grid>
          </Grid>
        </ControlTemplate>
      </TabControl.Template>

      <!-- Tableau de bord -->
      <TabItem Visibility="Collapsed">
        <TabItem.Header>
          <StackPanel Orientation="Horizontal">
            <TextBlock Text="&#xE80F;" FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets" FontSize="15" Margin="0,2,12,0"/>
            <TextBlock Text="Tableau de bord"/>
          </StackPanel>
        </TabItem.Header>
        <ScrollViewer VerticalScrollBarVisibility="Auto">
          <StackPanel Margin="0,0,8,0">
            <DockPanel>
              <Button x:Name="BtnAnalyze" DockPanel.Dock="Right" Style="{StaticResource BtnPrimary}" Content="Relancer l'analyse" VerticalAlignment="Center"/>
              <StackPanel>
                <TextBlock Style="{StaticResource H1}" Text="Tableau de bord"/>
                <TextBlock Style="{StaticResource Sub}" Text="La santé de chaque composant de ton PC et ce qui freine tes jeux."/>
              </StackPanel>
            </DockPanel>

            <Grid Margin="0,18,0,0">
              <Grid.ColumnDefinitions>
                <ColumnDefinition Width="230"/>
                <ColumnDefinition Width="*"/>
              </Grid.ColumnDefinitions>
              <Border Style="{StaticResource Card}" Margin="0,0,12,0">
                <StackPanel HorizontalAlignment="Center" VerticalAlignment="Center">
                  <Grid Width="130" Height="130">
                    <Ellipse Stroke="#232937" StrokeThickness="10"/>
                    <Ellipse x:Name="ScoreRing" Stroke="#4EA8FF" StrokeThickness="10" StrokeDashArray="0 1000" StrokeDashCap="Round" RenderTransformOrigin="0.5,0.5">
                      <Ellipse.RenderTransform><RotateTransform Angle="-90"/></Ellipse.RenderTransform>
                    </Ellipse>
                    <StackPanel VerticalAlignment="Center" HorizontalAlignment="Center">
                      <TextBlock x:Name="ScoreText" Text="..." FontSize="38" FontWeight="Bold" Foreground="White" HorizontalAlignment="Center"/>
                      <TextBlock Text="sur 100" Foreground="#9AA3B2" FontSize="12" HorizontalAlignment="Center"/>
                    </StackPanel>
                  </Grid>
                  <TextBlock x:Name="ScoreLabel" Text="Analyse en cours" FontSize="15" FontWeight="SemiBold" HorizontalAlignment="Center" Margin="0,12,0,0"/>
                  <TextBlock x:Name="ScoreCounts" Foreground="#9AA3B2" FontSize="12" HorizontalAlignment="Center" Margin="0,4,0,0"/>
                  <TextBlock x:Name="ScorePotential" Foreground="#22D37A" FontSize="12" FontWeight="SemiBold" HorizontalAlignment="Center" TextAlignment="Center" TextWrapping="Wrap" Margin="0,8,0,0"/>
                </StackPanel>
              </Border>
              <Border Grid.Column="1" Style="{StaticResource Card}">
                <DockPanel>
                  <DockPanel DockPanel.Dock="Top" Margin="0,0,0,18">
                    <TextBlock x:Name="LiveStamp" DockPanel.Dock="Right" Foreground="#5B6475" FontSize="11" VerticalAlignment="Center"/>
                    <StackPanel Orientation="Horizontal">
                      <Ellipse Width="8" Height="8" Fill="#22D37A" VerticalAlignment="Center" Margin="0,2,8,0"/>
                      <TextBlock Style="{StaticResource H2}" Text="En direct"/>
                    </StackPanel>
                  </DockPanel>
                  <UniformGrid x:Name="LiveGrid" Rows="1" Columns="4" VerticalAlignment="Center">
                    <StackPanel Margin="0,0,18,0">
                      <TextBlock Text="Processeur" Foreground="#9AA3B2" FontSize="12"/>
                      <TextBlock x:Name="LiveCpuVal" Text="..." FontSize="28" FontWeight="Bold" Foreground="White" Margin="0,2,0,8"/>
                      <ProgressBar x:Name="LiveCpuBar"/>
                      <TextBlock x:Name="LiveCpuSub" Foreground="#9AA3B2" FontSize="11.5" Margin="0,7,0,0" TextWrapping="Wrap"/>
                    </StackPanel>
                    <StackPanel Margin="0,0,18,0">
                      <TextBlock Text="Mémoire vive" Foreground="#9AA3B2" FontSize="12"/>
                      <TextBlock x:Name="LiveRamVal" Text="..." FontSize="28" FontWeight="Bold" Foreground="White" Margin="0,2,0,8"/>
                      <ProgressBar x:Name="LiveRamBar"/>
                      <TextBlock x:Name="LiveRamSub" Foreground="#9AA3B2" FontSize="11.5" Margin="0,7,0,0" TextWrapping="Wrap"/>
                    </StackPanel>
                    <StackPanel Margin="0,0,18,0">
                      <TextBlock Text="Carte graphique" Foreground="#9AA3B2" FontSize="12"/>
                      <TextBlock x:Name="LiveGpuVal" Text="..." FontSize="28" FontWeight="Bold" Foreground="White" Margin="0,2,0,8"/>
                      <ProgressBar x:Name="LiveGpuBar"/>
                      <TextBlock x:Name="LiveGpuSub" Foreground="#9AA3B2" FontSize="11.5" Margin="0,7,0,0" TextWrapping="Wrap"/>
                    </StackPanel>
                    <StackPanel x:Name="LiveTempBox">
                      <TextBlock Text="Température GPU" Foreground="#9AA3B2" FontSize="12"/>
                      <TextBlock x:Name="LiveTempVal" Text="..." FontSize="28" FontWeight="Bold" Foreground="White" Margin="0,2,0,8"/>
                      <ProgressBar x:Name="LiveTempBar"/>
                      <TextBlock x:Name="LiveTempSub" Foreground="#9AA3B2" FontSize="11.5" Margin="0,7,0,0" TextWrapping="Wrap"/>
                    </StackPanel>
                  </UniformGrid>
                </DockPanel>
              </Border>
            </Grid>

            <Border Style="{StaticResource Card}" Margin="0,12,0,0">
              <StackPanel>
                <DockPanel>
                  <Button x:Name="BtnFixAll" DockPanel.Dock="Right" Style="{StaticResource BtnPrimary}" Content="Tout corriger" VerticalAlignment="Center" Margin="16,0,0,0"/>
                  <StackPanel>
                    <TextBlock Style="{StaticResource H2}" Text="Pour gagner des points"/>
                    <TextBlock x:Name="ImproveSub" Style="{StaticResource Sub}" Text="Clique sur un point pour voir ce qu'il faut faire."/>
                  </StackPanel>
                </DockPanel>
                <StackPanel x:Name="ImprovePanel" Margin="0,14,0,0"/>
              </StackPanel>
            </Border>

            <DockPanel Margin="0,28,0,12">
              <TextBlock x:Name="HealthSummary" DockPanel.Dock="Right" Foreground="#9AA3B2" FontSize="12" VerticalAlignment="Bottom"/>
              <TextBlock Style="{StaticResource H2}" FontSize="19" Text="Santé des composants"/>
            </DockPanel>
            <Grid>
              <Grid.ColumnDefinitions>
                <ColumnDefinition Width="*"/>
                <ColumnDefinition Width="12"/>
                <ColumnDefinition Width="*"/>
              </Grid.ColumnDefinitions>
              <StackPanel x:Name="HealthLeft"/>
              <StackPanel x:Name="HealthRight" Grid.Column="2"/>
            </Grid>

            <DockPanel Margin="0,20,0,12">
              <TextBlock x:Name="FindingsSummary" DockPanel.Dock="Right" Foreground="#9AA3B2" FontSize="12" VerticalAlignment="Bottom"/>
              <TextBlock Style="{StaticResource H2}" FontSize="19" Text="Recommandations"/>
            </DockPanel>
            <StackPanel x:Name="FindingsPanel"/>
          </StackPanel>
        </ScrollViewer>
      </TabItem>

      <!-- Optimisation gaming -->
      <TabItem Visibility="Collapsed">
        <TabItem.Header>
          <StackPanel Orientation="Horizontal">
            <TextBlock Text="&#xE7FC;" FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets" FontSize="15" Margin="0,2,12,0"/>
            <TextBlock Text="Optimisation gaming"/>
          </StackPanel>
        </TabItem.Header>
        <Grid>
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="*"/>
            <RowDefinition Height="Auto"/>
          </Grid.RowDefinitions>
          <DockPanel>
            <StackPanel DockPanel.Dock="Right" Orientation="Horizontal" VerticalAlignment="Center">
              <Button x:Name="BtnSelectAll" Style="{StaticResource BtnSecondary}" Content="Tout le recommandé" Margin="0,0,10,0"/>
              <Button x:Name="BtnApply" Style="{StaticResource BtnPrimary}" Content="Appliquer la sélection"/>
            </StackPanel>
            <StackPanel>
              <TextBlock Style="{StaticResource H1}" Text="Optimisation gaming"/>
              <TextBlock Style="{StaticResource Sub}" Text="Coche ce que tu veux appliquer. Tout est réversible depuis l'onglet Sauvegarde."/>
            </StackPanel>
          </DockPanel>
          <CheckBox x:Name="ChkRestore" Grid.Row="1" IsChecked="True" Margin="0,16,0,12" Foreground="#9AA3B2"
                    Content="Créer un point de restauration Windows avant d'appliquer (recommandé)"/>
          <ScrollViewer Grid.Row="2" VerticalScrollBarVisibility="Auto">
            <StackPanel x:Name="GamingPanel"/>
          </ScrollViewer>
          <TextBlock Grid.Row="3" Style="{StaticResource Sub}" FontSize="12" Margin="0,12,0,0"
                     Text="Pourquoi si peu de réglages ? Les « tweaks miracles » d'Internet (réglages réseau secrets, services désactivés, nettoyeurs de RAM...) n'apportent rien ou cassent Windows. OptiGame n'applique que des réglages dont l'effet est reconnu."/>
        </Grid>
      </TabItem>

      <!-- Démarrage -->
      <TabItem Visibility="Collapsed">
        <TabItem.Header>
          <StackPanel Orientation="Horizontal">
            <TextBlock Text="&#xE7E8;" FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets" FontSize="15" Margin="0,2,12,0"/>
            <TextBlock Text="Démarrage"/>
          </StackPanel>
        </TabItem.Header>
        <Grid>
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="*"/>
          </Grid.RowDefinitions>
          <DockPanel>
            <StackPanel DockPanel.Dock="Right" Orientation="Horizontal" VerticalAlignment="Center">
              <Button x:Name="BtnRefreshStartup" Style="{StaticResource BtnSecondary}" Content="Actualiser" Margin="0,0,10,0"/>
              <Button x:Name="BtnDisableStartup" Style="{StaticResource BtnPrimary}" Content="Désactiver ce qui est conseillé"/>
            </StackPanel>
            <StackPanel>
              <TextBlock Style="{StaticResource H1}" Text="Programmes au démarrage"/>
              <TextBlock x:Name="StartupCount" Style="{StaticResource Sub}"/>
            </StackPanel>
          </DockPanel>
          <ScrollViewer Grid.Row="1" Margin="0,16,0,0" VerticalScrollBarVisibility="Auto">
            <StackPanel x:Name="StartupPanel" Margin="0,0,8,0"/>
          </ScrollViewer>
        </Grid>
      </TabItem>
      <!-- Réseau -->
      <TabItem Visibility="Collapsed">
        <TabItem.Header>
          <StackPanel Orientation="Horizontal">
            <TextBlock Text="&#xE774;" FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets" FontSize="15" Margin="0,2,12,0"/>
            <TextBlock Text="Connexion"/>
          </StackPanel>
        </TabItem.Header>
        <Grid>
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="*"/>
          </Grid.RowDefinitions>
          <DockPanel>
            <Button x:Name="BtnPing" DockPanel.Dock="Right" Style="{StaticResource BtnPrimary}" Content="Tester ma connexion" VerticalAlignment="Center"/>
            <StackPanel>
              <TextBlock Style="{StaticResource H1}" Text="Connexion"/>
              <TextBlock Style="{StaticResource Sub}" Text="Mesure ton ping, sa stabilité (gigue) et les pertes de paquets."/>
            </StackPanel>
          </DockPanel>
          <ScrollViewer Grid.Row="1" Margin="0,18,0,0" VerticalScrollBarVisibility="Auto">
            <StackPanel>
              <Border Style="{StaticResource Card}" Margin="0,0,0,10">
                <StackPanel x:Name="NetInfoPanel"/>
              </Border>
              <StackPanel x:Name="PingPanel"/>
              <Border Style="{StaticResource Card}" Margin="0,2,0,0">
                <StackPanel>
                  <TextBlock Style="{StaticResource H2}" Text="Serveur DNS"/>
                  <TextBlock x:Name="DnsCurrent" Style="{StaticResource Sub}" Margin="0,4,0,12"/>
                  <StackPanel Orientation="Horizontal">
                    <ComboBox x:Name="DnsCombo" Width="270" SelectedIndex="0" Padding="8,7" VerticalContentAlignment="Center">
                      <ComboBoxItem Content="Automatique (celui de ta box)"/>
                      <ComboBoxItem Content="Cloudflare (1.1.1.1)"/>
                      <ComboBoxItem Content="Google (8.8.8.8)"/>
                      <ComboBoxItem Content="Quad9 (9.9.9.9)"/>
                    </ComboBox>
                    <Button x:Name="BtnDnsApply" Style="{StaticResource BtnPrimary}" Content="Appliquer" Margin="10,0,0,0"/>
                    <Button x:Name="BtnDnsFlush" Style="{StaticResource BtnSecondary}" Content="Vider le cache DNS" Margin="10,0,0,0"/>
                  </StackPanel>
                  <TextBlock Style="{StaticResource Sub}" FontSize="12" Margin="0,12,0,0"
                             Text="Bon à savoir: le DNS ne change pas ton ping en jeu, il sert seulement à trouver l'adresse des serveurs. Un DNS rapide accélère un peu la navigation et la connexion des launchers."/>
                </StackPanel>
              </Border>
            </StackPanel>
          </ScrollViewer>
        </Grid>
      </TabItem>

      <!-- Nettoyage -->
      <TabItem Visibility="Collapsed">
        <TabItem.Header>
          <StackPanel Orientation="Horizontal">
            <TextBlock Text="&#xE74D;" FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets" FontSize="15" Margin="0,2,12,0"/>
            <TextBlock Text="Nettoyage"/>
          </StackPanel>
        </TabItem.Header>
        <Grid>
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="*"/>
            <RowDefinition Height="Auto"/>
          </Grid.RowDefinitions>
          <DockPanel>
            <StackPanel DockPanel.Dock="Right" Orientation="Horizontal" VerticalAlignment="Center">
              <Button x:Name="BtnCleanScan" Style="{StaticResource BtnSecondary}" Content="Analyser" Margin="0,0,10,0"/>
              <Button x:Name="BtnClean" Style="{StaticResource BtnPrimary}" Content="Nettoyer la sélection"/>
            </StackPanel>
            <StackPanel>
              <TextBlock Style="{StaticResource H1}" Text="Nettoyage"/>
              <TextBlock Style="{StaticResource Sub}" Text="Libère de la place en supprimant les fichiers inutiles."/>
            </StackPanel>
          </DockPanel>
          <TextBlock x:Name="CleanTotal" Grid.Row="1" Style="{StaticResource H2}" Margin="0,18,0,12" Text="Clique sur Analyser pour voir ce qui peut être libéré."/>
          <ScrollViewer Grid.Row="2" VerticalScrollBarVisibility="Auto">
            <StackPanel x:Name="CleanPanel"/>
          </ScrollViewer>
          <TextBlock Grid.Row="3" Style="{StaticResource Sub}" FontSize="12" Margin="0,12,0,0"
                     Text="OptiGame ne touche jamais à tes documents, à la corbeille ni aux caches de shaders des jeux: les vider provoquerait des saccades le temps qu'ils se reconstruisent."/>
        </Grid>
      </TabItem>

      <!-- Tests -->
      <TabItem Visibility="Collapsed">
        <TabItem.Header>
          <StackPanel Orientation="Horizontal">
            <TextBlock Text="&#xE9D9;" FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets" FontSize="15" Margin="0,2,12,0"/>
            <TextBlock Text="Tests"/>
          </StackPanel>
        </TabItem.Header>
        <Grid>
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="*"/>
          </Grid.RowDefinitions>
          <StackPanel>
            <TextBlock Style="{StaticResource H1}" Text="Tests des composants"/>
            <TextBlock Style="{StaticResource Sub}" Text="Vérifie que chaque pièce de ton PC fonctionne bien et à la bonne vitesse. Ferme tes jeux avant de lancer un test."/>
          </StackPanel>
          <ScrollViewer Grid.Row="1" Margin="0,16,0,0" VerticalScrollBarVisibility="Auto">
            <UniformGrid x:Name="TestsPanel" Columns="2" VerticalAlignment="Top"/>
          </ScrollViewer>
        </Grid>
      </TabItem>

      <!-- Sécurité -->
      <TabItem Visibility="Collapsed">
        <TabItem.Header>
          <StackPanel Orientation="Horizontal">
            <TextBlock Text="&#xE72E;" FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets" FontSize="15" Margin="0,2,12,0"/>
            <TextBlock Text="Sécurité"/>
          </StackPanel>
        </TabItem.Header>
        <ScrollViewer VerticalScrollBarVisibility="Auto">
          <StackPanel Margin="0,0,8,0">
            <DockPanel>
              <Button x:Name="BtnSecRefresh" DockPanel.Dock="Right" Style="{StaticResource BtnSecondary}" Content="Revérifier" VerticalAlignment="Center"/>
              <StackPanel>
                <TextBlock Style="{StaticResource H1}" Text="Sécurité"/>
                <TextBlock Style="{StaticResource Sub}" Text="Recherche des virus et de tout ce qui pourrait être dangereux sur ton PC."/>
              </StackPanel>
            </DockPanel>
            <Grid Margin="0,18,0,0">
              <Grid.ColumnDefinitions>
                <ColumnDefinition Width="230"/>
                <ColumnDefinition Width="*"/>
              </Grid.ColumnDefinitions>
              <Border Style="{StaticResource Card}" Margin="0,0,12,0">
                <StackPanel x:Name="SecGaugeHost" HorizontalAlignment="Center" VerticalAlignment="Center"/>
              </Border>
              <Border Grid.Column="1" Style="{StaticResource Card}">
                <Grid>
                  <Grid.ColumnDefinitions>
                    <ColumnDefinition Width="*"/>
                    <ColumnDefinition Width="Auto"/>
                  </Grid.ColumnDefinitions>
                  <StackPanel x:Name="SecLines" VerticalAlignment="Center"/>
                  <StackPanel Grid.Column="1" Margin="16,0,0,0" VerticalAlignment="Center" Width="220">
                    <Button x:Name="BtnScanQuick" Style="{StaticResource BtnPrimary}" Content="Analyse rapide" Margin="0,0,0,8"/>
                    <Button x:Name="BtnScanFull" Style="{StaticResource BtnSecondary}" Content="Analyse complète" Margin="0,0,0,8"/>
                    <Button x:Name="BtnScanFolder" Style="{StaticResource BtnSecondary}" Content="Analyser un dossier" Margin="0,0,0,8"/>
                    <Button x:Name="BtnScanUpdate" Style="{StaticResource BtnSecondary}" Content="Mettre à jour la base"/>
                  </StackPanel>
                </Grid>
              </Border>
            </Grid>
            <TextBlock x:Name="SecScanNote" Style="{StaticResource Sub}" FontSize="12" Margin="0,10,0,0"/>
            <DockPanel Margin="0,24,0,12">
              <TextBlock x:Name="SecChecksSummary" DockPanel.Dock="Right" Foreground="#9AA3B2" FontSize="12" VerticalAlignment="Bottom"/>
              <TextBlock Style="{StaticResource H2}" FontSize="19" Text="Points à vérifier"/>
            </DockPanel>
            <StackPanel x:Name="SecChecks">
              <TextBlock Text="Vérification en cours..." Foreground="#9AA3B2" FontSize="13"/>
            </StackPanel>
            <TextBlock Style="{StaticResource H2}" FontSize="19" Text="Menaces trouvées récemment" Margin="0,24,0,12"/>
            <StackPanel x:Name="SecHistory"/>
            <TextBlock Style="{StaticResource Sub}" FontSize="12" Margin="0,16,0,0"
                       Text="Les points à vérifier sont des indices, pas des preuves : un programme non signé n'est pas forcément un virus. En cas de doute, lance une analyse."/>
          </StackPanel>
        </ScrollViewer>
      </TabItem>

      <!-- Sauvegarde -->
      <TabItem Visibility="Collapsed">
        <TabItem.Header>
          <StackPanel Orientation="Horizontal">
            <TextBlock Text="&#xE777;" FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets" FontSize="15" Margin="0,2,12,0"/>
            <TextBlock Text="Sauvegarde"/>
          </StackPanel>
        </TabItem.Header>
        <ScrollViewer VerticalScrollBarVisibility="Auto">
          <StackPanel>
            <TextBlock Style="{StaticResource H1}" Text="Sauvegarde et rapport"/>
            <TextBlock Style="{StaticResource Sub}" Text="Reviens en arrière à tout moment, ou partage l'état de ton PC."/>

            <Border Style="{StaticResource Card}" Margin="0,18,0,10">
              <DockPanel>
                <Button x:Name="BtnUndo" DockPanel.Dock="Right" Style="{StaticResource BtnPrimary}" Content="Tout annuler" VerticalAlignment="Center" Margin="16,0,0,0"/>
                <StackPanel>
                  <TextBlock Style="{StaticResource H2}" Text="Annuler les changements d'OptiGame"/>
                  <TextBlock x:Name="BackupSummary" Style="{StaticResource Sub}"/>
                </StackPanel>
              </DockPanel>
            </Border>

            <Border Style="{StaticResource Card}" Margin="0,0,0,10">
              <DockPanel>
                <StackPanel DockPanel.Dock="Right" Orientation="Horizontal" VerticalAlignment="Center" Margin="16,0,0,0">
                  <Button x:Name="BtnOpenRestore" Style="{StaticResource BtnSecondary}" Content="Ouvrir la restauration" Margin="0,0,10,0"/>
                  <Button x:Name="BtnRestorePoint" Style="{StaticResource BtnPrimary}" Content="Créer"/>
                </StackPanel>
                <StackPanel>
                  <TextBlock Style="{StaticResource H2}" Text="Point de restauration Windows"/>
                  <TextBlock Style="{StaticResource Sub}" Text="Une photo complète des réglages de Windows. En cas de souci, la restauration du système te ramène à cet état."/>
                </StackPanel>
              </DockPanel>
            </Border>

            <Border Style="{StaticResource Card}" Margin="0,0,0,10">
              <DockPanel>
                <Button x:Name="BtnExport" DockPanel.Dock="Right" Style="{StaticResource BtnPrimary}" Content="Exporter" VerticalAlignment="Center" Margin="16,0,0,0"/>
                <StackPanel>
                  <TextBlock Style="{StaticResource H2}" Text="Rapport de ton PC"/>
                  <TextBlock Style="{StaticResource Sub}" Text="Une page web avec ta configuration, ton score et les points à améliorer. Pratique pour comparer avec tes potes ou demander de l'aide."/>
                </StackPanel>
              </DockPanel>
            </Border>

            <Border Style="{StaticResource Card}" Margin="0,0,0,10">
              <DockPanel>
                <Button x:Name="BtnCheckUpdate" DockPanel.Dock="Right" Style="{StaticResource BtnSecondary}" Content="Vérifier" VerticalAlignment="Center" Margin="16,0,0,0"/>
                <StackPanel>
                  <TextBlock Style="{StaticResource H2}" Text="Mises à jour"/>
                  <TextBlock x:Name="UpdateStatus" Style="{StaticResource Sub}"/>
                </StackPanel>
              </DockPanel>
            </Border>
          </StackPanel>
        </ScrollViewer>
      </TabItem>

      <!-- Ordinateur (accueil) -->
      <TabItem>
        <TabItem.Header>
          <StackPanel Orientation="Horizontal">
            <TextBlock Text="&#xE7F4;" FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets" FontSize="15" Margin="0,2,12,0"/>
            <TextBlock Text="Ordinateur"/>
          </StackPanel>
        </TabItem.Header>
        <ScrollViewer VerticalScrollBarVisibility="Auto">
          <StackPanel Margin="0,0,8,0">
            <TextBlock Style="{StaticResource H1}" Text="Ordinateur"/>
            <TextBlock x:Name="HubSub" Style="{StaticResource Sub}"/>
            <Border Style="{StaticResource Card}" Margin="0,18,0,0" Padding="18,14">
              <Grid>
                <Grid.ColumnDefinitions>
                  <ColumnDefinition Width="Auto"/>
                  <ColumnDefinition Width="Auto"/>
                  <ColumnDefinition Width="*"/>
                </Grid.ColumnDefinitions>
                <StackPanel x:Name="HubGaugeOpt" VerticalAlignment="Center"/>
                <StackPanel x:Name="HubGaugeSec" Grid.Column="1" VerticalAlignment="Center"/>
                <StackPanel x:Name="HubSummary" Grid.Column="2" VerticalAlignment="Center" Margin="24,0,0,0"/>
              </Grid>
            </Border>
            <UniformGrid x:Name="HubCards" Columns="3" Margin="0,16,0,0"/>
          </StackPanel>
        </ScrollViewer>
      </TabItem>

      <!-- Réseau (scan des appareils) -->
      <TabItem>
        <TabItem.Header>
          <StackPanel Orientation="Horizontal">
            <TextBlock Text="&#xE701;" FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets" FontSize="15" Margin="0,2,12,0"/>
            <TextBlock Text="Réseau"/>
          </StackPanel>
        </TabItem.Header>
        <Grid>
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="*"/>
          </Grid.RowDefinitions>
          <DockPanel>
            <Button x:Name="BtnNetScan" DockPanel.Dock="Right" Style="{StaticResource BtnPrimary}" Content="Scanner le réseau" VerticalAlignment="Center"/>
            <StackPanel>
              <TextBlock Style="{StaticResource H1}" Text="Réseau"/>
              <TextBlock Style="{StaticResource Sub}" Text="Découvre tous les appareils connectés à ton réseau : téléphones, consoles, TV, box, objets connectés..."/>
            </StackPanel>
          </DockPanel>
          <ScrollViewer Grid.Row="1" Margin="0,18,0,0" VerticalScrollBarVisibility="Auto">
            <StackPanel Margin="0,0,8,0">
              <Grid>
                <Grid.ColumnDefinitions>
                  <ColumnDefinition Width="300"/>
                  <ColumnDefinition Width="*"/>
                </Grid.ColumnDefinitions>
                <Border Style="{StaticResource Card}" Margin="0,0,12,0">
                  <StackPanel x:Name="NetHero" HorizontalAlignment="Center" VerticalAlignment="Center"/>
                </Border>
                <Border Grid.Column="1" Style="{StaticResource Card}">
                  <StackPanel>
                    <TextBlock Style="{StaticResource H2}" Text="Ton réseau"/>
                    <StackPanel x:Name="NetScanInfo" Margin="0,10,0,0"/>
                  </StackPanel>
                </Border>
              </Grid>
              <Border Style="{StaticResource Card}" Margin="0,12,0,0">
                <DockPanel>
                  <StackPanel DockPanel.Dock="Right" Orientation="Horizontal" VerticalAlignment="Center" Margin="12,0,0,0">
                    <Button x:Name="BtnNetAuditView" Style="{StaticResource BtnSecondary}" Content="Voir le rapport" Margin="0,0,8,0" Visibility="Collapsed"/>
                    <Button x:Name="BtnNetAudit" Style="{StaticResource BtnPrimary}" Content="Lancer l'audit"/>
                  </StackPanel>
                  <Border x:Name="NetAuditScoreBox" Width="56" Height="56" CornerRadius="28" BorderThickness="3" BorderBrush="#343C4C" Margin="0,0,16,0" VerticalAlignment="Center">
                    <TextBlock x:Name="NetAuditScore" Text="?" Foreground="#9AA3B2" FontSize="19" FontWeight="Bold" HorizontalAlignment="Center" VerticalAlignment="Center"/>
                  </Border>
                  <StackPanel VerticalAlignment="Center">
                    <TextBlock Style="{StaticResource H2}" Text="Audit de sécurité"/>
                    <TextBlock x:Name="NetAuditText" Foreground="#9AA3B2" FontSize="12.5" TextWrapping="Wrap" Margin="0,4,0,0"/>
                  </StackPanel>
                </DockPanel>
              </Border>
              <DockPanel Margin="0,24,0,12">
                <TextBlock x:Name="NetDevSummary" DockPanel.Dock="Right" Foreground="#9AA3B2" FontSize="12" VerticalAlignment="Bottom"/>
                <TextBlock Style="{StaticResource H2}" FontSize="19" Text="Appareils connectés"/>
              </DockPanel>
              <UniformGrid x:Name="NetDevices" Columns="3"/>
              <TextBlock x:Name="NetDevHint" Style="{StaticResource Sub}" FontSize="12" Margin="0,8,0,0" Text="Clique sur « Scanner le réseau » pour voir les appareils connectés."/>
            </StackPanel>
          </ScrollViewer>
        </Grid>
      </TabItem>
    </TabControl>

    <Border Grid.Row="2" Background="#12151B" BorderBrush="#232937" BorderThickness="0,1,0,0" Padding="16,8">
      <TextBlock x:Name="StatusText" Text="Prêt." Foreground="#9AA3B2" FontSize="12"/>
    </Border>

    <!-- Fiche détaillée d'un point à corriger -->
    <Grid x:Name="Overlay" Grid.RowSpan="3" Visibility="Collapsed">
      <Border x:Name="OverlayBackdrop" Background="#D0080A0D"/>
      <Border Background="#161A21" BorderBrush="#2C3342" BorderThickness="1" CornerRadius="14"
              Width="640" Margin="24" VerticalAlignment="Center" HorizontalAlignment="Center">
        <Grid Margin="28,24,28,22">
          <Grid.RowDefinitions>
            <RowDefinition Height="*"/>
            <RowDefinition Height="Auto"/>
          </Grid.RowDefinitions>
          <ScrollViewer VerticalScrollBarVisibility="Auto" MaxHeight="520">
            <StackPanel x:Name="SheetBody"/>
          </ScrollViewer>
          <DockPanel Grid.Row="1" Margin="0,22,0,0" LastChildFill="False">
            <Button x:Name="SheetIgnore" DockPanel.Dock="Left" Style="{StaticResource BtnSecondary}" Content="Ignorer ce point"/>
            <Button x:Name="SheetRun" DockPanel.Dock="Right" Style="{StaticResource BtnPrimary}" Content="Exécuter" Margin="10,0,0,0"/>
            <Button x:Name="SheetOpen" DockPanel.Dock="Right" Style="{StaticResource BtnSecondary}" Content="Ouvrir" Margin="10,0,0,0"/>
            <Button x:Name="SheetClose" DockPanel.Dock="Right" Style="{StaticResource BtnSecondary}" Content="Fermer"/>
          </DockPanel>
        </Grid>
      </Border>
    </Grid>

    <!-- Panneau des tests -->
    <Grid x:Name="TestOverlay" Grid.RowSpan="3" Visibility="Collapsed">
      <Border x:Name="TestBackdrop" Background="#D0080A0D"/>
      <Border x:Name="TestCard" Background="#141820" BorderBrush="#2C3342" BorderThickness="1" CornerRadius="16"
              Width="760" Margin="24,16" VerticalAlignment="Center" HorizontalAlignment="Center">
        <Grid Margin="26,22,26,20">
          <Grid.RowDefinitions>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="Auto"/>
            <RowDefinition Height="*"/>
            <RowDefinition Height="Auto"/>
          </Grid.RowDefinitions>
          <DockPanel>
            <Button x:Name="BtnTestX" DockPanel.Dock="Right" Style="{StaticResource BtnSecondary}" Content="✕" Padding="12,6" VerticalAlignment="Top"/>
            <Border DockPanel.Dock="Left" Width="48" Height="48" CornerRadius="12" Background="#1A2A40" Margin="0,0,14,0">
              <TextBlock x:Name="TestTag" Foreground="#4EA8FF" FontWeight="Bold" FontSize="12.5" HorizontalAlignment="Center" VerticalAlignment="Center"/>
            </Border>
            <StackPanel VerticalAlignment="Center">
              <TextBlock x:Name="TestTitle" Foreground="White" FontSize="19" FontWeight="Bold" TextTrimming="CharacterEllipsis"/>
              <StackPanel Orientation="Horizontal" Margin="0,3,0,0">
                <TextBlock x:Name="TestSub" Foreground="#9AA3B2" FontSize="12.5" VerticalAlignment="Center"/>
                <Ellipse x:Name="TestStateDot" Width="8" Height="8" Margin="14,0,6,0" VerticalAlignment="Center" Fill="#4EA8FF"/>
                <TextBlock x:Name="TestStateText" FontSize="12.5" FontWeight="SemiBold" VerticalAlignment="Center"/>
              </StackPanel>
            </StackPanel>
          </DockPanel>
          <Grid Grid.Row="1" Margin="0,16,0,12">
            <Grid.ColumnDefinitions>
              <ColumnDefinition Width="*"/>
              <ColumnDefinition Width="Auto"/>
            </Grid.ColumnDefinitions>
            <ProgressBar x:Name="TestProgress" Height="5" VerticalAlignment="Center"/>
            <TextBlock x:Name="TestPct" Grid.Column="1" Foreground="#9AA3B2" FontSize="12" Margin="12,0,0,0" MinWidth="36" TextAlignment="Right"/>
          </Grid>
          <ScrollViewer x:Name="TestScroll" Grid.Row="2" VerticalScrollBarVisibility="Auto" MaxHeight="450">
            <StackPanel x:Name="TestBody" Margin="0,0,12,0"/>
          </ScrollViewer>
          <DockPanel Grid.Row="3" Margin="0,16,0,0" LastChildFill="False">
            <Button x:Name="BtnTestStop" DockPanel.Dock="Left" Style="{StaticResource BtnSecondary}" Content="Arrêter le test"/>
            <Button x:Name="BtnTestClose" DockPanel.Dock="Right" Style="{StaticResource BtnPrimary}" Content="Fermer"/>
            <Button x:Name="BtnTestAgain" DockPanel.Dock="Right" Style="{StaticResource BtnSecondary}" Content="Refaire le test" Margin="0,0,10,0"/>
          </DockPanel>
        </Grid>
      </Border>
    </Grid>
  </Grid>
</Window>
'@

$Window = [Windows.Markup.XamlReader]::Load((New-Object System.Xml.XmlNodeReader $Xaml))
$ui = @{}
foreach ($node in $Xaml.SelectNodes('//*[@*[local-name()="Name"]]')) {
    $n = $node.Attributes | Where-Object { $_.LocalName -eq 'Name' } | Select-Object -First 1
    if ($n) { $ui[$n.Value] = $Window.FindName($n.Value) }
}
$ui.Tabs = $Window.FindName('Tabs')

$Colors = @{ ok = '#22D37A'; warn = '#F5A524'; bad = '#F04438'; info = '#4EA8FF' }
$BusyButtons = 'BtnAnalyze', 'BtnSelectAll', 'BtnApply', 'BtnRefreshStartup', 'BtnDisableStartup',
               'BtnPing', 'BtnDnsApply', 'BtnDnsFlush', 'BtnCleanScan', 'BtnClean', 'BtnUndo', 'BtnRestorePoint', 'BtnExport'

# ---------------------------------------------------------------------------
# Aides pour l'interface
# ---------------------------------------------------------------------------
function Update-UI {
    $Window.Dispatcher.Invoke([Action]{}, [System.Windows.Threading.DispatcherPriority]::Background)
}

function Set-Status([string]$Text) {
    $ui.StatusText.Text = $Text
    Update-UI
}

function Set-Busy([bool]$Busy) {
    foreach ($b in $BusyButtons) { if ($ui[$b]) { $ui[$b].IsEnabled = -not $Busy } }
    $Window.Cursor = if ($Busy) { [System.Windows.Input.Cursors]::AppStarting } else { $null }
}

function Show-Message([string]$Text, [string]$Icon = 'Information') {
    [System.Windows.MessageBox]::Show($Window, $Text, 'OptiGame', 'OK', $Icon) | Out-Null
}

function Confirm-Action([string]$Text) {
    ([System.Windows.MessageBox]::Show($Window, $Text, 'OptiGame', 'YesNo', 'Question')) -eq 'Yes'
}

# Exécute une action en affichant les erreurs au lieu de planter.
function Invoke-Safe([scriptblock]$Action) {
    try { & $Action }
    catch {
        Write-Log "ERREUR: $($_.Exception.Message) $($_.InvocationInfo.PositionMessage)"
        Show-Message "Oups, quelque chose s'est mal passé:`n`n$($_.Exception.Message)" 'Warning'
        Set-Status 'Une erreur est survenue.'
    }
    finally { Set-Busy $false }
}

# Attend la fin d'un travail en arrière plan sans jamais figer la fenêtre:
# la fenêtre continue de tout traiter normalement (clics, animations) pendant l'attente.
$script:WaitFrames = New-Object System.Collections.ArrayList
$script:WaitTimer = New-Object System.Windows.Threading.DispatcherTimer
$script:WaitTimer.Interval = [TimeSpan]::FromMilliseconds(20)
$script:WaitTimer.Add_Tick({
    foreach ($w in @($script:WaitFrames)) {
        if ($w.H.IsCompleted) { $w.F.Continue = $false; $script:WaitFrames.Remove($w) }
    }
    if (-not $script:WaitFrames.Count) { $script:WaitTimer.Stop() }
})

function Wait-Handle($Handle) {
    if ($Handle.IsCompleted) { return }
    $frame = New-Object System.Windows.Threading.DispatcherFrame
    [void]$script:WaitFrames.Add(@{ H = $Handle; F = $frame })
    if (-not $script:WaitTimer.IsEnabled) { $script:WaitTimer.Start() }
    [System.Windows.Threading.Dispatcher]::PushFrame($frame)
}

# Lance un script dans un fil séparé (réutilisé d'un appel à l'autre) pour que la fenêtre reste fluide.
function Invoke-Async([scriptblock]$Script, $Argument) {
    $ps = [PowerShell]::Create()
    if ($script:Pool) { $ps.RunspacePool = $script:Pool }
    [void]$ps.AddScript($Script.ToString())
    if ($null -ne $Argument) { [void]$ps.AddArgument($Argument) }
    $handle = $ps.BeginInvoke()
    Wait-Handle $handle
    try { $out = $ps.EndInvoke($handle) } finally { $ps.Dispose() }
    foreach ($o in $out) { $o }
}

# Volume d'un lecteur sans charger de module (instantané).
function Get-VolInfo([string]$Letter) {
    try {
        $di = [IO.DriveInfo]::new($Letter)
        if ($di.IsReady) { [pscustomobject]@{ Size = $di.TotalSize; SizeRemaining = $di.AvailableFreeSpace; FileSystemLabel = $di.VolumeLabel } }
    } catch {}
}

# Toutes les informations lentes à lire, rassemblées en arrière plan.
$AnalysisDataWork = {
    param($sysDrive)
    $r = @{}
    $r.OS = Get-CimInstance Win32_OperatingSystem
    $r.Battery = @(Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue)
    $r.Chassis = @((Get-CimInstance Win32_SystemEnclosure -ErrorAction SilentlyContinue).ChassisTypes | ForEach-Object { [int]$_ })
    $r.PCType = [int](Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue).PCSystemType
    $r.CPU = Get-CimInstance Win32_Processor | Select-Object -First 1
    $r.GPUs = @(Get-CimInstance Win32_VideoController)
    $r.Mem = @(Get-CimInstance Win32_PhysicalMemory)
    $r.MemDiag = Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-MemoryDiagnostics-Results' } -MaxEvents 1 -ErrorAction SilentlyContinue
    try { $r.SysDisk = [string](Get-Partition -DriveLetter $sysDrive.TrimEnd(':') -ErrorAction Stop).DiskNumber } catch {}
    $r.Disks = @(foreach ($d in @(Get-PhysicalDisk -ErrorAction SilentlyContinue | Sort-Object { [int]$_.DeviceId })) {
        $vols = @()
        try {
            foreach ($pt in @(Get-Partition -DiskNumber ([int]$d.DeviceId) -ErrorAction Stop | Where-Object { [int][char]$_.DriveLetter -ne 0 })) {
                $vols += @{ DriveLetter = [string]$pt.DriveLetter; Vol = (Get-Volume -DriveLetter $pt.DriveLetter -ErrorAction SilentlyContinue) }
            }
        } catch {}
        $rel = $null
        try { $rel = $d | Get-StorageReliabilityCounter -ErrorAction Stop } catch {}
        # Copie en texte: hors de ce fil, les types (SSD, NVMe...) arriveraient sous forme de codes.
        $disk = [pscustomobject]@{ DeviceId = [string]$d.DeviceId; FriendlyName = [string]$d.FriendlyName; MediaType = [string]$d.MediaType; BusType = [string]$d.BusType
            HealthStatus = [string]$d.HealthStatus; Size = [uint64]$d.Size; SerialNumber = [string]$d.SerialNumber; FirmwareVersion = [string]$d.FirmwareVersion }
        @{ Disk = $disk; Vols = $vols; Rel = $rel; Letters = @($vols | ForEach-Object { $_.DriveLetter }) }
    })
    $r.LogicalC = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='$sysDrive'"
    $r.BaseBoard = Get-CimInstance Win32_BaseBoard -ErrorAction SilentlyContinue
    $r.BIOS = Get-CimInstance Win32_BIOS -ErrorAction SilentlyContinue
    try { $r.SecureBoot = [bool](Confirm-SecureBootUEFI -ErrorAction Stop) } catch { $r.SecureBoot = $null }
    try { $r.Tpm = Get-CimInstance -Namespace 'root\cimv2\security\microsofttpm' -ClassName Win32_Tpm -OperationTimeoutSec 3 -ErrorAction Stop; $r.TpmOk = $true } catch { $r.TpmOk = $false }
    $since = (Get-Date).AddDays(-30)
    $count = { param($f) try { @(Get-WinEvent -FilterHashtable $f -ErrorAction Stop).Count } catch { 0 } }
    $r.Bsod = & $count @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WER-SystemErrorReporting'; Id = 1001; StartTime = $since }
    $r.Crash = & $count @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-Kernel-Power'; Id = 41; StartTime = $since }
    $r.WheaErr = & $count @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WHEA-Logger'; Level = @(1, 2); StartTime = $since }
    $r.WheaWarn = & $count @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WHEA-Logger'; Level = 3; StartTime = $since }
    $r.Net = & {
        try {
            $route = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction Stop | Sort-Object RouteMetric | Select-Object -First 1
            $ad = Get-NetAdapter -InterfaceIndex $route.ifIndex -ErrorAction Stop
            $wifi = ([string]$ad.PhysicalMediaType -match '802\.11') -or ($ad.InterfaceDescription -match 'Wi-?Fi|Wireless|WLAN')
            $ip = Get-NetIPAddress -InterfaceIndex $route.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue | Where-Object { $_.IPAddress -notlike '169.254*' } | Select-Object -First 1
            $dns = try { (Get-DnsClientServerAddress -InterfaceIndex $route.ifIndex -AddressFamily IPv4 -ErrorAction Stop).ServerAddresses -join ', ' } catch { '' }
            @{ IfIndex = $route.ifIndex; Gateway = $route.NextHop; Name = $ad.Name; Desc = $ad.InterfaceDescription; Speed = $ad.LinkSpeed; Wifi = $wifi; Guid = $ad.InterfaceGuid
               Mac = $ad.MacAddress; Ip = $(if ($ip) { $ip.IPAddress } else { $null }); Prefix = $(if ($ip) { [int]$ip.PrefixLength } else { 24 }); Dns = $dns }
        } catch { $null }
    }
    $r
}

# Connexion réseau active (dans un fil séparé: les modules réseau sont lents à charger).
$ActiveNetWork = {
    try {
        $route = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction Stop | Sort-Object RouteMetric | Select-Object -First 1
        $ad = Get-NetAdapter -InterfaceIndex $route.ifIndex -ErrorAction Stop
        $wifi = ([string]$ad.PhysicalMediaType -match '802\.11') -or ($ad.InterfaceDescription -match 'Wi-?Fi|Wireless|WLAN')
        $ip = Get-NetIPAddress -InterfaceIndex $route.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue | Where-Object { $_.IPAddress -notlike '169.254*' } | Select-Object -First 1
        $dns = try { (Get-DnsClientServerAddress -InterfaceIndex $route.ifIndex -AddressFamily IPv4 -ErrorAction Stop).ServerAddresses -join ', ' } catch { '' }
        @{ IfIndex = $route.ifIndex; Gateway = $route.NextHop; Name = $ad.Name; Desc = $ad.InterfaceDescription; Speed = $ad.LinkSpeed; Wifi = $wifi; Guid = $ad.InterfaceGuid
           Mac = $ad.MacAddress; Ip = $(if ($ip) { $ip.IPAddress } else { $null }); Prefix = $(if ($ip) { [int]$ip.PrefixLength } else { 24 }); Dns = $dns }
    } catch { $null }
}

# Données lentes de l'onglet Sécurité (antivirus, fichiers, signatures, tâches), en arrière plan.
$SecDataWork = {
    param($a)
    $r = @{ OtherAv = @(); FirewallOff = @(); Exclusions = @(); Active = @(); Threats = @(); Detections = @(); Double = @(); Scripts = @(); Hidden = @(); SusStart = @(); Tasks = @() }
    try { $r.Mp = Get-MpComputerStatus -ErrorAction Stop } catch {}
    try { $r.OtherAv = @(Get-CimInstance -Namespace root/SecurityCenter2 -ClassName AntivirusProduct -ErrorAction Stop | Where-Object { $_.displayName -notmatch 'Windows Defender|Microsoft Defender' } | ForEach-Object { $_.displayName }) } catch {}
    try { $r.FirewallOff = @(Get-NetFirewallProfile -ErrorAction Stop | Where-Object { -not $_.Enabled } | ForEach-Object { $_.Name }) } catch {}
    try { $pref = Get-MpPreference -ErrorAction Stop; $r.Exclusions = @(@($pref.ExclusionPath) + @($pref.ExclusionProcess) + @($pref.ExclusionExtension) | Where-Object { $_ -and $_ -notmatch '^N/A' }) } catch {}
    try { $r.Threats = @(Get-MpThreat -ErrorAction Stop); $r.Active = @($r.Threats | Where-Object { $_.IsActive }) } catch {}
    try { $r.Detections = @(Get-MpThreatDetection -ErrorAction Stop | Sort-Object InitialDetectionTime -Descending | Select-Object -First 15) } catch {}
    $signed = { param($f) try { (Get-AuthenticodeSignature -FilePath $f -ErrorAction Stop).Status -eq 'Valid' } catch { $false } }
    foreach ($d in $a.UserDirs) {
        if (-not (Test-Path -LiteralPath $d)) { continue }
        foreach ($f in @(Get-ChildItem -LiteralPath $d -File -Recurse -Depth 2 -Force -ErrorAction SilentlyContinue)) {
            if ($f.Name -match '\.(pdf|docx?|xlsx?|jpe?g|png|gif|txt|mp4|mp3|zip|rar)\s*\.(exe|scr|bat|cmd|com|pif|vbs|js|jse|hta|lnk)$') { $r.Double += $f.FullName }
            elseif ($d -like '*Downloads' -and $f.Extension -match '^\.(scr|pif|vbs|vbe|js|jse|hta|wsf)$') { $r.Scripts += $f.FullName }
        }
    }
    foreach ($sf in $a.Folders) {
        $files = if ($sf.Depth) { Get-ChildItem -LiteralPath $sf.Path -File -Recurse -Depth $sf.Depth -Force -ErrorAction SilentlyContinue } else { Get-ChildItem -LiteralPath $sf.Path -File -Force -ErrorAction SilentlyContinue }
        foreach ($f in @($files | Where-Object { $_.Extension -match '^\.(exe|scr|com|pif)$' })) { if (-not (& $signed $f.FullName)) { $r.Hidden += $f.FullName } }
    }
    foreach ($exe in $a.StartupExes) {
        $dir = Split-Path $exe -Parent
        $inRisk = ($a.RiskDirs | Where-Object { $_ -and $dir -eq $_ }) -or $dir -like "$($a.Temp)*"
        if ($inRisk -or -not (& $signed $exe)) { $r.SusStart += $exe }
    }
    foreach ($t2 in @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskPath -notlike '\Microsoft\*' -and $_.State -ne 'Disabled' })) {
        foreach ($act in @($t2.Actions)) {
            $exe = [Environment]::ExpandEnvironmentVariables([string]$act.Execute).Trim('"')
            $argsTxt = [string]$act.Arguments
            $hiddenCmd = $exe -match '(powershell|pwsh|cmd|wscript|cscript|mshta)(\.exe)?$' -and $argsTxt -match '(-enc|-encodedcommand|frombase64|downloadstring|downloadfile|invoke-expression|\biex\b|-w(indowstyle)?\s+h(idden)?|http)'
            $riskPath = $exe -like "$($a.Temp)*" -or $exe -like "$($a.Public)*"
            if ($hiddenCmd -or $riskPath) { $r.Tasks += @{ Name = $t2.TaskName; Path = $t2.TaskPath; Cmd = "$exe $argsTxt".Trim(); Bad = $hiddenCmd } }
        }
    }
    $r
}

$DiskInfoWork = {
    param($id)
    $d = Get-PhysicalDisk -ErrorAction SilentlyContinue | Where-Object { [string]$_.DeviceId -eq $id } | Select-Object -First 1
    $rel = $null
    if ($d) { try { $rel = $d | Get-StorageReliabilityCounter -ErrorAction Stop } catch {} }
    if ($d) {
        $d = [pscustomobject]@{ DeviceId = [string]$d.DeviceId; FriendlyName = [string]$d.FriendlyName; MediaType = [string]$d.MediaType; BusType = [string]$d.BusType
            HealthStatus = [string]$d.HealthStatus; Size = [uint64]$d.Size; SerialNumber = [string]$d.SerialNumber; FirmwareVersion = [string]$d.FirmwareVersion }
    }
    @{ D = $d; Rel = $rel }
}

function Get-Brush([string]$Hex) { [System.Windows.Media.BrushConverter]::new().ConvertFromString($Hex) }
function New-Thickness($l, $t, $r, $b) { [System.Windows.Thickness]::new($l, $t, $r, $b) }

function New-Text([string]$Text, [double]$Size = 13, [string]$Color = '#E6E8EE', [switch]$Bold, [switch]$Semi) {
    $t = New-Object System.Windows.Controls.TextBlock
    $t.Text = $Text
    $t.FontSize = $Size
    $t.Foreground = Get-Brush $Color
    $t.TextWrapping = 'Wrap'
    if ($Bold) { $t.FontWeight = [System.Windows.FontWeights]::Bold }
    elseif ($Semi) { $t.FontWeight = [System.Windows.FontWeights]::SemiBold }
    $t
}

function New-Card {
    $b = New-Object System.Windows.Controls.Border
    $b.Style = $Window.FindResource('Card')
    $b.Padding = New-Thickness 16 14 16 14
    $b.Margin = New-Thickness 0 0 0 8
    $b
}

function New-Grid([string[]]$Cols) {
    $g = New-Object System.Windows.Controls.Grid
    foreach ($c in $Cols) {
        $cd = New-Object System.Windows.Controls.ColumnDefinition
        $cd.Width = switch ($c) {
            'Auto'  { [System.Windows.GridLength]::Auto }
            '*'     { [System.Windows.GridLength]::new(1, [System.Windows.GridUnitType]::Star) }
            default { [System.Windows.GridLength]::new([double]$c) }
        }
        $g.ColumnDefinitions.Add($cd)
    }
    $g
}

function Add-ToGrid($Grid, $Element, [int]$Column) {
    [System.Windows.Controls.Grid]::SetColumn($Element, $Column)
    [void]$Grid.Children.Add($Element)
}

function New-Badge([string]$Text, [string]$Color) {
    $b = New-Object System.Windows.Controls.Border
    $b.CornerRadius = [System.Windows.CornerRadius]::new(6)
    $b.Padding = New-Thickness 8 2 8 2
    $b.Margin = New-Thickness 10 0 0 0
    $b.VerticalAlignment = 'Center'
    $bg = Get-Brush $Color
    $bg.Opacity = 0.16
    $b.Background = $bg
    $b.Child = New-Text $Text 11 $Color -Semi
    $b
}

function New-Button([string]$Text, [string]$Style = 'BtnSecondary') {
    $btn = New-Object System.Windows.Controls.Button
    $btn.Content = $Text
    $btn.Style = $Window.FindResource($Style)
    $btn.VerticalAlignment = 'Center'
    $btn
}

function Invoke-FindingAction([string]$Target) {
    if ($Target -like 'tab:*') { $ui.Tabs.SelectedIndex = [int]$Target.Substring(4) }
    elseif ($Target -like 'run:*') {
        $parts = $Target.Substring(4) -split ' ', 2
        if ($parts.Count -gt 1) { Start-Process $parts[0] -ArgumentList $parts[1] } else { Start-Process $parts[0] }
    }
    else { Start-Process $Target }
}

# ---------------------------------------------------------------------------
# Tableau de bord
# ---------------------------------------------------------------------------
function Add-Finding($List, [string]$Status, [string]$Titre, [string]$Detail, [int]$Weight, [string]$Action, [string]$ActionLabel, [string]$Id, $Fix) {
    if (-not $Id) { $Id = $Titre }
    [void]$List.Add([pscustomobject]@{
        Id = $Id; Status = $Status; Titre = $Titre; Detail = $Detail; Weight = $Weight
        Action = $Action; ActionLabel = $ActionLabel; Fix = $Fix; Gain = 0
    })
}

# Décrit comment corriger un point.
#   -Auto  : l'app règle le problème toute seule avec le bouton Exécuter.
#   sinon  : l'utilisateur suit les étapes (Steps), l'app peut l'aider avec un bouton (Run / Open).
function New-Fix {
    param(
        [switch]$Auto,
        [string[]]$What,
        [string]$Why,
        [string[]]$Steps,
        [scriptblock]$Run,
        $RunArgs,
        [string]$RunLabel = 'Exécuter',
        [string]$Confirm,
        [string]$Done,
        [switch]$NoRescan,
        [string]$Open,
        [string]$OpenLabel = 'Ouvrir',
        [switch]$Reboot,
        [switch]$Restore
    )
    @{
        Auto = [bool]$Auto; What = $What; Why = $Why; Steps = $Steps
        Run = $Run; Args = $RunArgs; RunLabel = $RunLabel; Confirm = $Confirm; Done = $Done; NoRescan = [bool]$NoRescan
        Open = $Open; OpenLabel = $OpenLabel; Reboot = [bool]$Reboot; Restore = [bool]$Restore
    }
}

$BiosRun = {
    shutdown.exe /r /fw /t 10
    if ($LASTEXITCODE) { throw "Ce PC ne permet pas de redémarrer directement dans le BIOS (code $LASTEXITCODE). Redémarre et appuie sur Suppr ou F2 pendant le démarrage." }
}
$BiosConfirm = "Le PC va redémarrer directement dans le BIOS dans 10 secondes.`n`nEnregistre ton travail et ferme tes jeux avant. Continuer ?"
$BiosDone = 'Redémarrage dans le BIOS dans 10 secondes...'

function Invoke-CleanAll {
    foreach ($t in $CleanTargets) {
        Set-Status "Nettoyage: $($t.Titre)..."
        [void](Invoke-Async $CleanScript $t.Paths)
    }
}

function Get-DriverLink([string]$Name) {
    if ($Name -match 'NVIDIA|GeForce') { return 'https://www.nvidia.com/fr-fr/drivers/' }
    if ($Name -match 'AMD|Radeon')     { return 'https://www.amd.com/fr/support/download/drivers.html' }
    if ($Name -match 'Intel')          { return 'https://www.intel.fr/content/www/fr/fr/support/detect.html' }
    'ms-settings:windowsupdate'
}

# ---------------------------------------------------------------------------
# Score et gains
# ---------------------------------------------------------------------------
# Un point vert compte entièrement, un orange à 40 %, un rouge pas du tout.
# Le gain d'un point = ce que le score gagnerait s'il passait au vert.
function Measure-Score($Findings) {
    $active = @($Findings | Where-Object { $script:Ignored -notcontains $_.Id })
    $scored = @($active | Where-Object { $_.Weight -gt 0 -and $_.Status -ne 'info' })
    $total = 0.0; $got = 0.0; $autoRaw = 0.0
    foreach ($item in $scored) {
        $credit = switch ($item.Status) { 'ok' { 1.0 } 'warn' { 0.4 } default { 0.0 } }
        $total += $item.Weight
        $got += $item.Weight * $credit
        $item.Gain = $item.Weight * (1 - $credit)
        if ($item.Fix -and $item.Fix.Auto -and $item.Status -ne 'ok') { $autoRaw += $item.Gain }
    }
    foreach ($item in $active) {
        $item.Gain = if ($total -and $scored -contains $item) { [int][math]::Round(100 * $item.Gain / $total) } else { 0 }
    }
    $score = if ($total) { [int][math]::Round(100 * $got / $total) } else { 100 }
    $potential = if ($total) { [int][math]::Round(100 * ($got + $autoRaw) / $total) } else { 100 }
    @{ Score = $score; Potential = $potential; Active = $active }
}

function Show-Score([int]$Score, [int]$Bad, [int]$Warn, [int]$Potential) {
    if ($Score -ge 85)     { $label = 'Excellent';     $color = $Colors.ok }
    elseif ($Score -ge 65) { $label = 'Bien';          $color = '#9BE15D' }
    elseif ($Score -ge 45) { $label = 'À améliorer';   $color = $Colors.warn }
    else                   { $label = 'Mal optimisé';  $color = $Colors.bad }
    $len = [math]::PI * (130 - 10) / 10
    $ui.ScoreRing.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(($len * $Score / 100), 1000))
    $ui.ScoreRing.Stroke = Get-Brush $color
    $ui.ScoreText.Text = [string]$Score
    $ui.ScoreLabel.Text = $label
    $ui.ScoreLabel.Foreground = Get-Brush $color
    $parts = @()
    if ($Bad)  { $parts += "$Bad problème$(if ($Bad -gt 1) {'s'})" }
    if ($Warn) { $parts += "$Warn à améliorer" }
    $ui.ScoreCounts.Text = if ($parts) { $parts -join ', ' } else { 'Rien à signaler' }
    $ui.ScorePotential.Text = if ($Potential -gt $Score) { "Jusqu'à $Potential en un clic" } else { '' }
    @{ Label = $label; Color = $color }
}

# ---------------------------------------------------------------------------
# « Pour gagner des points » et recommandations
# ---------------------------------------------------------------------------
function Get-FixKind($f) {
    if ($f.Fix -and $f.Fix.Auto) { return @{ Text = "L'app s'en charge"; Color = $Colors.ok } }
    @{ Text = 'À faire toi même'; Color = $Colors.info }
}

function Show-Improvements($Active) {
    $panel = $ui.ImprovePanel
    $panel.Children.Clear()
    $items = @($Active | Where-Object { $_.Status -ne 'ok' -and $_.Gain -gt 0 } |
        Sort-Object @{ Expression = { $_.Gain }; Descending = $true }, @{ Expression = { -not ($_.Fix -and $_.Fix.Auto) } })
    $auto = @($items | Where-Object { $_.Fix -and $_.Fix.Auto })

    if (-not $items.Count) {
        [void]$panel.Children.Add((New-Text "Rien à gagner de plus : ton PC est au top pour le jeu !" 13 $Colors.ok -Semi))
        $ui.ImproveSub.Text = ''
        $ui.BtnFixAll.Visibility = 'Collapsed'
        return
    }
    $ui.ImproveSub.Text = "$($items.Count) amélioration$(if ($items.Count -gt 1) {'s'}) possible$(if ($items.Count -gt 1) {'s'}), dont $($auto.Count) que l'app peut faire pour toi. Clique sur une ligne pour voir le détail."
    if ($auto.Count) {
        $sum = ($auto | Measure-Object Gain -Sum).Sum
        $ui.BtnFixAll.Content = "Tout corriger (+$sum pts)"
        $ui.BtnFixAll.Visibility = 'Visible'
    } else {
        $ui.BtnFixAll.Visibility = 'Collapsed'
    }

    foreach ($f in $items) {
        $row = New-Object System.Windows.Controls.Border
        $row.CornerRadius = [System.Windows.CornerRadius]::new(10)
        $row.Padding = New-Thickness 12 10 12 10
        $row.Margin = New-Thickness 0 0 0 6
        $row.Background = Get-Brush '#1D222C'
        $row.Cursor = [System.Windows.Input.Cursors]::Hand
        $row.Tag = $f
        $g = New-Grid @('Auto', '*', 'Auto', 'Auto')

        $pill = New-Object System.Windows.Controls.Border
        $pill.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $pill.Width = 64
        $pill.Padding = New-Thickness 0 5 0 5
        $pbg = Get-Brush $Colors.ok; $pbg.Opacity = 0.16
        $pill.Background = $pbg
        $pt = New-Text "+$($f.Gain) pts" 13 $Colors.ok -Bold
        $pt.HorizontalAlignment = 'Center'; $pt.TextWrapping = 'NoWrap'
        $pill.Child = $pt
        $pill.VerticalAlignment = 'Center'
        Add-ToGrid $g $pill 0

        $title = New-Text $f.Titre 14 '#FFFFFF' -Semi
        $title.Margin = New-Thickness 14 0 10 0
        $title.VerticalAlignment = 'Center'
        Add-ToGrid $g $title 1

        $k = Get-FixKind $f
        $badge = New-Badge $k.Text $k.Color
        $badge.Margin = New-Thickness 0 0 12 0
        Add-ToGrid $g $badge 2

        $chev = New-Text '›' 22 '#9AA3B2' -Bold
        $chev.VerticalAlignment = 'Center'
        $chev.Margin = New-Thickness 0 -4 0 0
        Add-ToGrid $g $chev 3

        $row.Child = $g
        $row.Add_MouseEnter({ param($s, $e) $s.Background = Get-Brush '#252B37' })
        $row.Add_MouseLeave({ param($s, $e) $s.Background = Get-Brush '#1D222C' })
        $row.Add_MouseLeftButtonUp({ param($s, $e) Open-Sheet @($s.Tag) })
        [void]$panel.Children.Add($row)
    }
}

function Add-FindingCard($Panel, $f, [switch]$Ignored) {
    $card = New-Card
    $g = New-Grid @('Auto', '*', 'Auto')
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 12; $dot.Height = 12
    $dot.Fill = Get-Brush $(if ($Ignored) { $Muted } else { $Colors[$f.Status] })
    $dot.VerticalAlignment = 'Top'
    $dot.Margin = New-Thickness 0 4 14 0
    Add-ToGrid $g $dot 0

    $sp = New-Object System.Windows.Controls.StackPanel
    $head = New-Object System.Windows.Controls.WrapPanel
    [void]$head.Children.Add((New-Text $f.Titre 14 '#FFFFFF' -Semi))
    if ($f.Gain -gt 0 -and -not $Ignored) { [void]$head.Children.Add((New-Badge "+$($f.Gain) pts" $Colors.ok)) }
    [void]$sp.Children.Add($head)
    if ($f.Detail) {
        $d = New-Text $f.Detail 12.5 '#9AA3B2'
        $d.Margin = New-Thickness 0 3 0 0
        [void]$sp.Children.Add($d)
    }
    Add-ToGrid $g $sp 1

    $btn = $null
    if ($Ignored) {
        $btn = New-Button 'Ne plus ignorer'
        $btn.Tag = $f
        $btn.Add_Click({ param($s, $e) Invoke-Safe { Set-IgnoreFinding $s.Tag $false } })
    } elseif ($f.Status -ne 'ok' -and $f.Fix) {
        $btn = if ($f.Fix.Auto) { New-Button 'Corriger' 'BtnPrimary' } else { New-Button 'Comment faire' }
        $btn.Tag = $f
        $btn.Add_Click({ param($s, $e) Open-Sheet @($s.Tag) })
    } elseif ($f.Action) {
        $btn = New-Button $f.ActionLabel
        $btn.Tag = $f.Action
        $btn.Add_Click({ param($s, $e) Invoke-FindingAction $s.Tag })
    }
    if ($btn) {
        $btn.Margin = New-Thickness 14 0 0 0
        Add-ToGrid $g $btn 2
    }
    $card.Child = $g
    if ($Ignored) { $card.Opacity = 0.75 }
    [void]$Panel.Children.Add($card)
}

# Affiche d'abord ce qui est à corriger ; les points OK et ignorés sont repliés.
function Show-Findings($All, $Active) {
    $panel = $ui.FindingsPanel
    $panel.Children.Clear()
    $issues = @($Active | Where-Object { $_.Status -ne 'ok' })
    $script:OkFindings = @($Active | Where-Object { $_.Status -eq 'ok' })
    $script:IgnoredFindings = @($All | Where-Object { $script:Ignored -contains $_.Id -and $_.Status -ne 'ok' })
    foreach ($f in $issues) { Add-FindingCard $panel $f }
    if (-not $issues.Count) {
        Add-FindingCard $panel ([pscustomobject]@{ Status = 'ok'; Titre = 'Rien à corriger'; Detail = 'Ton PC est bien réglé pour le jeu.'; Gain = 0; Fix = $null; Action = $null })
    }
    $more = New-Object System.Windows.Controls.WrapPanel
    $more.Margin = New-Thickness 0 4 0 0
    if ($script:OkFindings.Count) {
        $b = New-Button $(if ($script:OkFindings.Count -gt 1) { "Voir les $($script:OkFindings.Count) points déjà OK" } else { 'Voir le point déjà OK' })
        $b.Margin = New-Thickness 0 0 10 0
        $b.Add_Click({
            param($s, $e)
            $s.Visibility = 'Collapsed'
            foreach ($f in $script:OkFindings) { Add-FindingCard $ui.FindingsPanel $f }
        })
        [void]$more.Children.Add($b)
    }
    if ($script:IgnoredFindings.Count) {
        $b = New-Button $(if ($script:IgnoredFindings.Count -gt 1) { "Voir les $($script:IgnoredFindings.Count) points ignorés" } else { 'Voir le point ignoré' })
        $b.Add_Click({
            param($s, $e)
            $s.Visibility = 'Collapsed'
            foreach ($f in $script:IgnoredFindings) { Add-FindingCard $ui.FindingsPanel $f -Ignored }
        })
        [void]$more.Children.Add($b)
    }
    [void]$panel.Children.Add($more)
    $ui.FindingsSummary.Text = "$($issues.Count) point$(if ($issues.Count -gt 1) {'s'}) à regarder" +
        $(if ($script:IgnoredFindings.Count) { ", $($script:IgnoredFindings.Count) ignoré$(if ($script:IgnoredFindings.Count -gt 1) {'s'})" } else { '' })
}

# ---------------------------------------------------------------------------
# Fiche détaillée (fenêtre par dessus l'application)
# ---------------------------------------------------------------------------
function Add-SheetSection([string]$Title, [string[]]$Lines, [switch]$Numbered, [switch]$Bullets) {
    $h = New-Text $Title 13 '#9AA3B2' -Semi
    $h.Margin = New-Thickness 0 18 0 6
    [void]$ui.SheetBody.Children.Add($h)
    $i = 0
    foreach ($l in $Lines) {
        $i++
        $prefix = if ($Numbered) { "$i.  " } elseif ($Bullets) { '•  ' } else { '' }
        $t = New-Text "$prefix$l" 14 '#E6E8EE'
        $t.Margin = New-Thickness $(if ($prefix) { 4 } else { 0 }) 2 0 4
        [void]$ui.SheetBody.Children.Add($t)
    }
}

function Add-SheetInfo([string]$Text, [string]$Color) {
    $b = New-Object System.Windows.Controls.Border
    $bg = Get-Brush $Color; $bg.Opacity = 0.10
    $b.Background = $bg
    $b.CornerRadius = [System.Windows.CornerRadius]::new(8)
    $b.Padding = New-Thickness 12 9 12 9
    $b.Margin = New-Thickness 0 16 0 0
    $b.Child = New-Text $Text 13 $Color
    [void]$ui.SheetBody.Children.Add($b)
}

function Open-Sheet($Items) {
    $script:SheetMode = 'fix'
    $script:SheetItems = @($Items)
    $ui.SheetClose.Content = 'Fermer'
    $ui.SheetClose.Visibility = 'Visible'
    $body = $ui.SheetBody
    $body.Children.Clear()

    if ($script:SheetItems.Count -eq 1) {
        $f = $script:SheetItems[0]
        $fix = $f.Fix
        $isIgnored = $script:Ignored -contains $f.Id

        $head = New-Grid @('Auto', '*')
        $dot = New-Object System.Windows.Shapes.Ellipse
        $dot.Width = 14; $dot.Height = 14
        $dot.Fill = Get-Brush $Colors[$f.Status]
        $dot.Margin = New-Thickness 0 8 14 0
        $dot.VerticalAlignment = 'Top'
        Add-ToGrid $head $dot 0
        Add-ToGrid $head (New-Text $f.Titre 21 '#FFFFFF' -Bold) 1
        [void]$body.Children.Add($head)

        $badges = New-Object System.Windows.Controls.WrapPanel
        $badges.Margin = New-Thickness 18 8 0 0
        if ($f.Gain -gt 0) { [void]$badges.Children.Add((New-Badge "+$($f.Gain) points au score" $Colors.ok)) }
        if ($fix) { $k = Get-FixKind $f; [void]$badges.Children.Add((New-Badge $k.Text $k.Color)) }
        if ($fix -and $fix.Reboot) { [void]$badges.Children.Add((New-Badge 'Redémarrage requis' '#9AA3B2')) }
        foreach ($c in $badges.Children) { $c.Margin = New-Thickness 0 0 8 0 }
        [void]$body.Children.Add($badges)

        if ($f.Detail) { Add-SheetSection "Ce qu'on a trouvé" @($f.Detail) }
        if ($fix -and $fix.Why) { Add-SheetSection 'Pourquoi ça compte' @($fix.Why) }
        if ($fix -and $fix.Auto) {
            Add-SheetSection "Ce que l'app va faire quand tu cliques sur Exécuter" $fix.What -Bullets
            $safe = 'Tu peux revenir en arrière à tout moment depuis l''onglet Sauvegarde.'
            if ($fix.Restore) { $safe = 'Un point de restauration Windows est créé avant. ' + $safe }
            Add-SheetInfo $safe $Colors.ok
        } elseif ($fix) {
            if ($fix.Steps) { Add-SheetSection 'Ce que tu dois faire' $fix.Steps -Numbered }
            if ($fix.What) { Add-SheetSection "Ce que l'app peut faire pour t'aider" $fix.What -Bullets }
        }
        if ($isIgnored) { Add-SheetInfo 'Ce point est ignoré : il ne compte plus dans ton score.' $Colors.info }

        $ui.SheetRun.Visibility = if ($fix -and $fix.Run -and -not $isIgnored) { 'Visible' } else { 'Collapsed' }
        if ($fix) { $ui.SheetRun.Content = $fix.RunLabel }
        $open = if ($fix -and $fix.Open) { $fix.Open } else { $f.Action }
        $ui.SheetOpen.Visibility = if ($open) { 'Visible' } else { 'Collapsed' }
        $ui.SheetOpen.Tag = $open
        $ui.SheetOpen.Content = if ($fix -and $fix.Open) { $fix.OpenLabel } elseif ($f.ActionLabel) { $f.ActionLabel } else { 'Ouvrir' }
        $ui.SheetIgnore.Visibility = if ($f.Status -ne 'ok') { 'Visible' } else { 'Collapsed' }
        $ui.SheetIgnore.Content = if ($isIgnored) { 'Ne plus ignorer' } else { "Ignorer (c'est voulu)" }
    } else {
        $auto = @($script:SheetItems)
        $sum = ($auto | Measure-Object Gain -Sum).Sum
        [void]$body.Children.Add((New-Text 'Tout corriger en un clic' 21 '#FFFFFF' -Bold))
        $s = New-Text "L'app va appliquer $($auto.Count) correction$(if ($auto.Count -gt 1) {'s'}), pour environ +$sum points :" 14 '#9AA3B2'
        $s.Margin = New-Thickness 0 6 0 0
        [void]$body.Children.Add($s)
        foreach ($f in $auto) { Add-SheetSection "$($f.Titre)   (+$($f.Gain) pts)" $f.Fix.What -Bullets }
        $safe = 'Un point de restauration Windows est créé avant. Tu peux tout annuler depuis l''onglet Sauvegarde.'
        if ($auto | Where-Object { $_.Fix.Reboot }) { $safe += ' Certains réglages demandent un redémarrage.' }
        Add-SheetInfo $safe $Colors.ok
        $ui.SheetRun.Visibility = 'Visible'
        $ui.SheetRun.Content = "Exécuter les $($auto.Count) corrections"
        $ui.SheetOpen.Visibility = 'Collapsed'
        $ui.SheetIgnore.Visibility = 'Collapsed'
    }
    $ui.Overlay.Visibility = 'Visible'
}

function Close-Sheet { $ui.Overlay.Visibility = 'Collapsed' }

function Open-FixAll {
    if (-not $script:LastAnalysis) { return }
    $auto = @($script:LastAnalysis.Active | Where-Object { $_.Status -ne 'ok' -and $_.Gain -gt 0 -and $_.Fix -and $_.Fix.Auto } |
        Sort-Object Gain -Descending)
    if ($auto.Count) { Open-Sheet $auto }
}

function Invoke-SheetRun {
    $items = @($script:SheetItems | Where-Object { $_.Fix -and $_.Fix.Run })
    if (-not $items.Count) { return }
    if ($items.Count -eq 1 -and $items[0].Fix.Confirm -and -not (Confirm-Action $items[0].Fix.Confirm)) { return }
    Close-Sheet
    Set-Busy $true
    $before = if ($script:LastAnalysis) { $script:LastAnalysis.Score } else { $null }

    if (($items | Where-Object { $_.Fix.Restore }) -and -not $script:RestoreDone) {
        if (-not (New-RestorePoint)) { Set-Status 'Annulé.'; return }
        $script:RestoreDone = $true
    }
    $done = @(); $failed = @(); $reboot = $false
    $script:RunLog = New-Object System.Collections.ArrayList
    try {
        foreach ($f in $items) {
            Set-Status "En cours : $($f.Titre)..."
            try {
                & $f.Fix.Run $f.Fix.Args
                $done += $f
                if ($f.Fix.Reboot) { $reboot = $true }
            } catch {
                $failed += "$($f.Titre) : $($_.Exception.Message)"
                Write-Log "Échec correction $($f.Id): $_"
            }
        }
    } finally {
        $log = $script:RunLog
        $script:RunLog = $null
    }

    if ($done.Count -eq 1 -and $done[0].Fix.NoRescan) {
        $msg = if ($done[0].Fix.Done) { $done[0].Fix.Done } else { 'C''est lancé.' }
        Set-Status $msg
        Show-Message $msg
        return
    }

    # Un écran a changé de fréquence: on vérifie qu'il affiche toujours quelque chose.
    $reverted = @(Confirm-DisplayChange $log)

    Build-GamingTab
    Update-StartupList
    Update-BackupSummary
    Invoke-Analysis
    $after = $script:LastAnalysis.Score

    $kept = @($done | Where-Object { -not ($_.Id -like 'display:*' -and $reverted -contains $_.Id.Substring(8)) })
    $lines = @()
    if ($kept.Count) {
        $lines += "$($kept.Count) correction$(if ($kept.Count -gt 1) {'s'}) appliquée$(if ($kept.Count -gt 1) {'s'}) :"
        foreach ($f in $kept) { $lines += "•  $($f.Titre)" }
    }
    if ($reverted.Count) {
        $lines += "L'écran est revenu à son ancienne fréquence car la nouvelle ne s'affichait pas. Ce point est maintenant ignoré : il ne compte plus dans ton score."
    }
    if ($null -ne $before -and $kept.Count) { $lines += "Score : $before → $after" }
    if ($failed) { $lines += 'Non appliqué :'; $lines += $failed }
    if ($reboot -and $kept.Count) { $lines += 'Redémarre ton PC pour que tout soit pris en compte.' }
    $note = if ($kept | Where-Object { $_.Id -eq 'disk-space' }) { 'Les fichiers supprimés par le nettoyage ne peuvent pas être récupérés. Tout le reste peut être annulé.' } else { $null }
    $title = if ($kept.Count) { "C'est fait !" } elseif ($reverted.Count) { 'Retour à l''ancien réglage' } else { 'Rien n''a été appliqué' }
    Set-Status $title
    Show-ResultSheet $title $lines $log $note
}

# Après un changement de fréquence: demande si l'affichage est correct, sinon revient
# automatiquement en arrière au bout de 15 secondes (comme Windows).
function Confirm-DisplayChange($Log) {
    $disp = @($Log | Where-Object { $_.Type -eq 'display' })
    if (-not $disp.Count) { return @() }
    $script:SheetMode = 'display'
    $script:DisplayChoice = $null

    $body = $ui.SheetBody
    $body.Children.Clear()
    [void]$body.Children.Add((New-Text 'Tes écrans s''affichent bien ?' 21 '#FFFFFF' -Bold))
    $t = New-Text "La fréquence de l'écran vient d'être changée. Si un écran est resté noir ou affiche un message d'erreur, ne touche à rien : l'app revient toute seule à l'ancien réglage." 14 '#E6E8EE'
    $t.Margin = New-Thickness 0 12 0 0
    [void]$body.Children.Add($t)
    Add-SheetInfo 'Sans réponse, retour automatique à l''ancien réglage.' $Colors.warn
    $ui.SheetRun.Content = 'Oui, garder'
    $ui.SheetRun.Visibility = 'Visible'
    $ui.SheetIgnore.Visibility = 'Visible'
    $ui.SheetOpen.Visibility = 'Collapsed'
    $ui.SheetClose.Visibility = 'Collapsed'
    $ui.Overlay.Visibility = 'Visible'
    try { [void]$Window.Activate() } catch {}

    $end = (Get-Date).AddSeconds(15)
    while (-not $script:DisplayChoice) {
        $left = [math]::Ceiling(($end - (Get-Date)).TotalSeconds)
        if ($left -le 0) { $script:DisplayChoice = 'timeout'; break }
        $ui.SheetIgnore.Content = "Revenir en arrière ($left)"
        Update-UI
        Start-Sleep -Milliseconds 100
    }
    $choice = $script:DisplayChoice
    Close-Sheet
    $script:SheetMode = 'fix'
    $ui.SheetClose.Visibility = 'Visible'
    if ($choice -eq 'keep') { return @() }

    $devices = @()
    foreach ($d in $disp) {
        [void][OGNative]::SetRefreshRate($d.Device, $d.Hz)
        [void]$Log.Remove($d)
        $devices += $d.Device
        if ($script:Ignored -notcontains "display:$($d.Device)") { $script:Ignored += "display:$($d.Device)" }
    }
    Save-Ignored
    Write-Log "Fréquence annulée ($choice) pour: $($devices -join ', ')"
    $devices
}

# Annule exactement les changements notés dans le journal, du plus récent au plus ancien.
function Undo-RunLog($Log) {
    $errors = @()
    for ($i = $Log.Count - 1; $i -ge 0; $i--) {
        $e = $Log[$i]
        try {
            switch ($e.Type) {
                'reg' {
                    if ($e.Existed) {
                        Write-RegValue $e.Path $e.Name $e.Value $e.Kind
                    } else {
                        Remove-RegValue $e.Path $e.Name
                    }
                }
                'power' { powercfg /setactive $e.Guid | Out-Null }
                'overlay' { [void][OGNative]::SetOverlay($e.Guid) }
                'display' {
                    $r = [OGNative]::SetRefreshRate($e.Device, $e.Hz)
                    if ($r -ne 0) { throw "Écran $($e.Device): fréquence non restaurée (code $r)" }
                }
            }
        } catch { $errors += $_.Exception.Message }
    }
    Sync-Mouse
    , $errors
}

# Fiche de résultat avec le bouton « Revenir en arrière ».
function Show-ResultSheet([string]$Title, [string[]]$Lines, $Log, [string]$Note) {
    $script:SheetMode = 'result'
    $script:ResultLog = $Log
    $body = $ui.SheetBody
    $body.Children.Clear()
    $head = New-Grid @('Auto', '*')
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 14; $dot.Height = 14
    $dot.Fill = Get-Brush $Colors.ok
    $dot.Margin = New-Thickness 0 8 14 0
    $dot.VerticalAlignment = 'Top'
    Add-ToGrid $head $dot 0
    Add-ToGrid $head (New-Text $Title 21 '#FFFFFF' -Bold) 1
    [void]$body.Children.Add($head)
    foreach ($l in $Lines) {
        $t = New-Text $l 14 '#E6E8EE'
        $t.Margin = New-Thickness 0 8 0 0
        [void]$body.Children.Add($t)
    }
    if ($Note) { Add-SheetInfo $Note $Colors.info }
    $canUndo = $Log -and $Log.Count
    if ($canUndo) { Add-SheetInfo 'Si quelque chose ne va pas, « Revenir en arrière » annule exactement ces changements.' $Colors.ok }
    $ui.SheetRun.Visibility = 'Collapsed'
    $ui.SheetOpen.Visibility = 'Collapsed'
    $ui.SheetIgnore.Visibility = if ($canUndo) { 'Visible' } else { 'Collapsed' }
    $ui.SheetIgnore.Content = 'Revenir en arrière'
    $ui.SheetClose.Content = 'OK'
    $ui.SheetClose.Visibility = 'Visible'
    $ui.Overlay.Visibility = 'Visible'
}

function Invoke-UndoLastRun {
    $log = $script:ResultLog
    Close-Sheet
    if (-not $log -or -not $log.Count) { return }
    Set-Busy $true
    Set-Status 'Retour en arrière...'
    $before = if ($script:LastAnalysis) { $script:LastAnalysis.Score } else { $null }
    $errors = Undo-RunLog $log
    $script:ResultLog = $null
    Build-GamingTab
    Update-StartupList
    Update-BackupSummary
    Invoke-Analysis
    $lines = @('Les réglages sont revenus exactement comme avant.')
    if ($null -ne $before) { $lines += "Score : $before → $($script:LastAnalysis.Score)" }
    if ($errors) { $lines += 'Pas pu être restauré :'; $lines += $errors }
    Set-Status 'Retour en arrière effectué.'
    Show-ResultSheet 'Retour en arrière effectué' $lines $null $null
}

function Set-IgnoreFinding($f, [bool]$Ignore) {
    if ($Ignore) { if ($script:Ignored -notcontains $f.Id) { $script:Ignored += $f.Id } }
    else { $script:Ignored = @($script:Ignored | Where-Object { $_ -ne $f.Id }) }
    Save-Ignored
    Close-Sheet
    Invoke-Analysis
    Set-Status $(if ($Ignore) { "« $($f.Titre) » est ignoré et ne compte plus dans le score." } else { "« $($f.Titre) » compte de nouveau dans le score." })
}

# ---------------------------------------------------------------------------
# Santé des composants
# ---------------------------------------------------------------------------
$StatusLabels = @{ ok = 'Bon état'; warn = 'À surveiller'; bad = 'Problème'; info = 'Info' }
$Muted = '#5B6475'

function Get-LoadColor([double]$Pct, [double]$Warn = 75, [double]$Bad = 90) {
    if ($Pct -ge $Bad) { $Colors.bad } elseif ($Pct -ge $Warn) { $Colors.warn } else { $Colors.ok }
}

function Get-EventCount([hashtable]$Filter) {
    try { @(Get-WinEvent -FilterHashtable $Filter -ErrorAction Stop).Count } catch { 0 }
}

function Format-Duration([TimeSpan]$Span) {
    if ($Span.TotalDays -ge 1) { return "$([int][math]::Floor($Span.TotalDays)) j $($Span.Hours) h" }
    "$($Span.Hours) h $($Span.Minutes) min"
}

function New-Component([string]$Tag, [string]$Titre, [string]$Sous) {
    @{
        Tag = $Tag; Titre = $Titre; Sous = $Sous; Status = 'ok'
        Bars = New-Object System.Collections.ArrayList
        Lines = [ordered]@{}
        Notes = New-Object System.Collections.ArrayList
        Action = $null; ActionLabel = $null
    }
}

function Set-Worse($C, [string]$Status) {
    $rank = @{ info = 0; ok = 0; warn = 1; bad = 2 }
    if ($rank[$Status] -gt $rank[$C.Status]) { $C.Status = $Status }
}

function Add-Note($C, [string]$Status, [string]$Text) {
    [void]$C.Notes.Add(@{ Status = $Status; Text = $Text })
    Set-Worse $C $Status
}

$SmiPath = @("$env:windir\System32\nvidia-smi.exe", "$env:ProgramFiles\NVIDIA Corporation\NVSMI\nvidia-smi.exe") |
    Where-Object { Test-Path $_ } | Select-Object -First 1

# Interroge nvidia-smi et renvoie les valeurs par nom de champ (null si non disponible).
function Invoke-Smi([string]$Fields) {
    if (-not $SmiPath) { return $null }
    try {
        $o = & $SmiPath "--query-gpu=$Fields" '--format=csv,noheader,nounits' 2>$null | Select-Object -First 1
        if (-not $o) { return $null }
        $vals = $o -split ','
        $names = $Fields -split ','
        $h = @{}
        for ($i = 0; $i -lt $names.Count; $i++) {
            $v = if ($i -lt $vals.Count) { $vals[$i].Trim() } else { '' }
            $h[$names[$i]] = if ($v -match '^[\d\.]+$') { [double]::Parse($v, [Globalization.CultureInfo]::InvariantCulture) } else { $null }
        }
        $h
    } catch { $null }
}

function Get-GpuVram([string]$Name) {
    $base = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}'
    foreach ($k in (Get-ChildItem $base -ErrorAction SilentlyContinue | Where-Object { $_.PSChildName -match '^\d{4}$' })) {
        $p = Get-ItemProperty $k.PSPath -ErrorAction SilentlyContinue
        if ($p -and $p.DriverDesc -eq $Name) {
            $m = $p.'HardwareInformation.qwMemorySize'
            if (-not $m) { $m = $p.'HardwareInformation.MemorySize' }
            if ($m -is [byte[]]) { $m = [BitConverter]::ToUInt32($m, 0) }
            if ($m) { return [double]$m }
        }
    }
    $null
}

function New-HealthCard($C) {
    $color = $Colors[$C.Status]
    $card = New-Card
    $card.Padding = New-Thickness 18 16 18 16
    $card.Margin = New-Thickness 0 0 0 12
    $sp = New-Object System.Windows.Controls.StackPanel

    # En-tête: pastille, titre, état
    $head = New-Grid @('Auto', '*', 'Auto')
    $tag = New-Object System.Windows.Controls.Border
    $tag.Width = 46; $tag.Height = 46
    $tag.CornerRadius = [System.Windows.CornerRadius]::new(10)
    $bg = Get-Brush $color; $bg.Opacity = 0.14
    $tag.Background = $bg
    $tt = New-Text $C.Tag 12 $color -Bold
    $tt.TextWrapping = 'NoWrap'
    $tt.HorizontalAlignment = 'Center'; $tt.VerticalAlignment = 'Center'
    $tag.Child = $tt
    Add-ToGrid $head $tag 0
    $ts = New-Object System.Windows.Controls.StackPanel
    $ts.Margin = New-Thickness 12 0 8 0
    $ts.VerticalAlignment = 'Center'
    [void]$ts.Children.Add((New-Text $C.Titre 15 '#FFFFFF' -Semi))
    if ($C.Sous) { [void]$ts.Children.Add((New-Text $C.Sous 12 '#9AA3B2')) }
    Add-ToGrid $head $ts 1
    $badge = New-Badge $StatusLabels[$C.Status] $color
    $badge.Margin = New-Thickness 0 0 0 0
    $badge.VerticalAlignment = 'Top'
    Add-ToGrid $head $badge 2
    [void]$sp.Children.Add($head)

    # Jauges
    foreach ($b in $C.Bars) {
        $row = New-Grid @('*', 'Auto')
        $row.Margin = New-Thickness 0 14 0 6
        Add-ToGrid $row (New-Text $b.Label 12.5 '#9AA3B2') 0
        Add-ToGrid $row (New-Text $b.Text 12.5 '#E6E8EE' -Semi) 1
        [void]$sp.Children.Add($row)
        $pb = New-Object System.Windows.Controls.ProgressBar
        $pb.Value = [math]::Min(100.0, [math]::Max(0.0, [double]$b.Value))
        $pb.Foreground = Get-Brush $b.Color
        [void]$sp.Children.Add($pb)
    }

    # Détails
    if ($C.Lines.Count) {
        $lines = New-Object System.Windows.Controls.StackPanel
        $lines.Margin = New-Thickness 0 12 0 0
        foreach ($k in $C.Lines.Keys) {
            $v = $C.Lines[$k]
            $txt = $v; $col = '#E6E8EE'
            if ($v -is [array]) { $txt = $v[0]; $col = $v[1] }
            $r = New-Grid @('165', '*')
            $r.Margin = New-Thickness 0 3 0 3
            Add-ToGrid $r (New-Text $k 12.5 '#9AA3B2') 0
            Add-ToGrid $r (New-Text ([string]$txt) 12.5 $col) 1
            [void]$lines.Children.Add($r)
        }
        [void]$sp.Children.Add($lines)
    }

    # Explications
    foreach ($n in $C.Notes) {
        $nb = New-Object System.Windows.Controls.Border
        $nbg = Get-Brush $Colors[$n.Status]; $nbg.Opacity = 0.10
        $nb.Background = $nbg
        $nb.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $nb.Padding = New-Thickness 12 8 12 8
        $nb.Margin = New-Thickness 0 10 0 0
        $nb.Child = New-Text $n.Text 12.5 $Colors[$n.Status]
        [void]$sp.Children.Add($nb)
    }

    if ($C.Action) {
        $btn = New-Button $C.ActionLabel
        $btn.HorizontalAlignment = 'Left'
        $btn.Margin = New-Thickness 0 12 0 0
        $btn.Tag = $C.Action
        $btn.Add_Click({ param($s, $e) Invoke-FindingAction $s.Tag })
        [void]$sp.Children.Add($btn)
    }
    $card.Child = $sp
    $card
}

# Répartit les cartes sur deux colonnes en équilibrant leur hauteur.
function Show-HealthCards($Cards) {
    $ui.HealthLeft.Children.Clear()
    $ui.HealthRight.Children.Clear()
    $hl = 0; $hr = 0
    foreach ($c in $Cards) {
        $h = 4 + 2 * $c.Bars.Count + $c.Lines.Count + 2 * $c.Notes.Count + $(if ($c.Action) { 2 } else { 0 })
        if ($hl -le $hr) { [void]$ui.HealthLeft.Children.Add((New-HealthCard $c)); $hl += $h }
        else { [void]$ui.HealthRight.Children.Add((New-HealthCard $c)); $hr += $h }
    }
    $n = @{ ok = 0; warn = 0; bad = 0 }
    foreach ($c in $Cards) { if ($n.ContainsKey($c.Status)) { $n[$c.Status]++ } else { $n.ok++ } }
    $parts = @("$($n.ok) en bon état")
    if ($n.warn) { $parts += "$($n.warn) à surveiller" }
    if ($n.bad)  { $parts += "$($n.bad) avec un problème" }
    $ui.HealthSummary.Text = $parts -join ', '
}

# ---------------------------------------------------------------------------
# Analyse complète
# ---------------------------------------------------------------------------
function Invoke-Analysis {
    Set-Busy $true
    Set-Status 'Analyse de ton PC en cours...'
    $F = New-Object System.Collections.ArrayList
    $cards = New-Object System.Collections.ArrayList
    $info = [ordered]@{}
    $since = (Get-Date).AddDays(-30)

    # Système (lecture en arrière plan, lancée dès l'ouverture de l'app)
    Set-Status 'Lecture des informations du PC...'
    if ($script:Prefetch) {
        $pf = $script:Prefetch; $script:Prefetch = $null
        Wait-Handle $pf.Handle
        try { $data = @($pf.PS.EndInvoke($pf.Handle))[0] } finally { $pf.PS.Dispose() }
    } else {
        $data = Invoke-Async $AnalysisDataWork $env:SystemDrive | Select-Object -First 1
    }
    $script:AnalysisData = $data
    $os = $data.OS
    $script:Build = [int]$os.BuildNumber
    $battery = @($data.Battery)
    $script:IsLaptop = Test-IsLaptop $battery $data
    if ($script:IsLaptop -and ($battery | Where-Object { $_.BatteryStatus -eq 1 })) {
        Add-Finding $F 'warn' 'Portable sur batterie' 'Sur batterie, Windows bride le processeur et la carte graphique. Branche le chargeur pour jouer.' 2 -Id 'laptop-battery' -Fix (New-Fix `
            -Why 'Sur batterie, le processeur et la carte graphique tournent au ralenti pour économiser l''énergie: tu peux perdre la moitié de tes FPS.' `
            -Steps @('Branche le chargeur de ton portable avant de jouer.', 'Dans Paramètres > Système > Alimentation, choisis le mode « Meilleures performances ».', 'Relance l''analyse.') `
            -Open 'ms-settings:powersleep' -OpenLabel 'Paramètres d''alimentation')
    }

    # --- Processeur
    Set-Status 'Analyse du processeur...'
    $cpu = $script:AnalysisData.CPU
    $cpuName = ($cpu.Name -replace '\s+', ' ').Trim()
    $Live.BaseMHz = [int]$cpu.MaxClockSpeed
    $info['Processeur'] = "$cpuName ($($cpu.NumberOfCores) cœurs, $($cpu.NumberOfLogicalProcessors) threads)"
    $c = New-Component 'CPU' 'Processeur' $cpuName
    $c.Lines['Cœurs'] = "$($cpu.NumberOfCores) cœurs, $($cpu.NumberOfLogicalProcessors) threads"
    $c.Lines['Fréquence de base'] = '{0:N1} GHz' -f ($cpu.MaxClockSpeed / 1000)
    if ($cpu.L3CacheSize) { $c.Lines['Cache L3'] = "$([math]::Round($cpu.L3CacheSize / 1024)) Mo" }
    $c.Lines['Température'] = @('Non fournie par Windows (utilise HWiNFO)', $Muted)
    [void]$cards.Add($c)
    Update-UI

    # --- Carte graphique
    Set-Status 'Analyse de la carte graphique...'
    $gpus = @($data.GPUs | Where-Object { $_.Name -notmatch 'Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta' })
    $info['Carte graphique'] = ($gpus | ForEach-Object { $_.Name }) -join ' + '
    $dedicatedPattern = 'NVIDIA|GeForce|Radeon RX|Radeon Pro|Arc'
    $hasDedicated = [bool]($gpus | Where-Object { $_.Name -match $dedicatedPattern })
    foreach ($g in $gpus) {
        $c = New-Component 'GPU' 'Carte graphique' $g.Name
        if ($g.Name -match 'Microsoft Basic') {
            Add-Note $c 'bad' 'Aucun vrai pilote installé: les jeux tournent très mal.'
            $c.Action = 'ms-settings:windowsupdate'; $c.ActionLabel = 'Mettre à jour'
            Add-Finding $F 'bad' 'Pilote graphique manquant' "Windows utilise un pilote générique: les jeux tournent très mal. Installe le pilote de ta carte graphique." 3 -Id 'gpu-nodriver' -Fix (New-Fix `
                -Why 'Sans le vrai pilote, la carte graphique ne sert presque à rien: pas d''accélération 3D correcte.' `
                -Steps @('Clique sur « Windows Update » et installe toutes les mises à jour, y compris les mises à jour facultatives de pilotes.', 'Si ça ne suffit pas, télécharge le pilote sur le site du fabricant de ta carte (NVIDIA, AMD ou Intel).', 'Redémarre puis relance l''analyse.') `
                -Open 'ms-settings:windowsupdate' -OpenLabel 'Windows Update')
            [void]$cards.Add($c)
            continue
        }
        $vram = Get-GpuVram $g.Name
        if ($vram) { $c.Lines['Mémoire vidéo'] = Format-Size $vram }
        $w = if ($hasDedicated -and $g.Name -notmatch $dedicatedPattern) { 1 } else { 2 }
        if ($g.DriverDate) {
            $age = ((Get-Date) - $g.DriverDate).Days
            $c.Lines['Pilote'] = @("$($g.DriverVersion) du $($g.DriverDate.ToString('dd/MM/yyyy'))", $(if ($age -gt 180) { $Colors.warn } else { '#E6E8EE' }))
            if ($age -gt 180) {
                $months = [math]::Floor($age / 30)
                Add-Note $c 'warn' "Pilote vieux d'environ $months mois: mets le à jour pour de meilleures performances."
                $c.Action = Get-DriverLink $g.Name; $c.ActionLabel = 'Télécharger le pilote'
                $steps = if ($g.Name -match 'NVIDIA|GeForce') {
                    @('Ouvre l''application NVIDIA si tu l''as (onglet Pilotes), ou clique sur « Site du pilote ».', 'Télécharge le dernier pilote « Game Ready » pour ta carte.', 'Lance l''installation (installation rapide). L''écran peut clignoter, c''est normal.', 'Relance l''analyse d''OptiGame.')
                } elseif ($g.Name -match 'AMD|Radeon') {
                    @('Ouvre AMD Software (clic droit sur le bureau) et va dans « Pilotes et logiciels », ou clique sur « Site du pilote ».', 'Installe la dernière version recommandée.', 'Redémarre si l''installation le demande, puis relance l''analyse.')
                } else {
                    @('Clique sur « Site du pilote » et laisse l''assistant détecter ta carte.', 'Installe le pilote proposé.', 'Redémarre si besoin, puis relance l''analyse.')
                }
                Add-Finding $F 'warn' "Pilote graphique ancien ($($g.Name))" "Ton pilote date d'il y a environ $months mois (version $($g.DriverVersion))." $w -Id "gpu-driver:$($g.Name)" -Fix (New-Fix `
                    -Why 'Chaque nouveau pilote apporte des optimisations pour les jeux récents et corrige des bugs (plantages, textures qui clignotent...).' `
                    -Steps $steps -Open (Get-DriverLink $g.Name) -OpenLabel 'Site du pilote')
            } else {
                Add-Finding $F 'ok' "Pilote graphique à jour ($($g.Name))" "Pilote de moins de 6 mois (version $($g.DriverVersion))." $w -Id "gpu-driver:$($g.Name)"
            }
        }
        if ($g.Name -match 'NVIDIA|GeForce') {
            $s = Invoke-Smi 'temperature.gpu,fan.speed,power.draw,power.limit,pcie.link.width.current,pcie.link.width.max'
            if ($s) {
                if ($null -ne $s['temperature.gpu']) {
                    $t = [int]$s['temperature.gpu']
                    $c.Lines['Température'] = @("$t °C", (Get-LoadColor $t 80 87))
                    if ($t -ge 80) { Add-Note $c 'warn' "Carte graphique chaude ($t °C) alors que le PC ne joue pas forcément: dépoussière le PC et vérifie les ventilateurs." }
                }
                if ($null -ne $s['fan.speed']) { $c.Lines['Ventilateurs'] = "$([int]$s['fan.speed']) %" }
                if ($null -ne $s['power.draw']) {
                    $c.Lines['Consommation'] = ('{0:N0} W' -f $s['power.draw']) + $(if ($s['power.limit']) { ' sur {0:N0} W max' -f $s['power.limit'] } else { '' })
                }
                if ($s['pcie.link.width.current'] -and $s['pcie.link.width.max']) {
                    $wc = [int]$s['pcie.link.width.current']; $wm = [int]$s['pcie.link.width.max']
                    $c.Lines['Liaison PCIe'] = @("x$wc (max x$wm)", $(if ($wc -lt $wm) { $Colors.warn } else { '#E6E8EE' }))
                    if (-not $script:IsLaptop -and $wc -lt $wm -and $wm -ge 16) {
                        Add-Note $c 'warn' "La carte fonctionne en x$wc au lieu de x${wm}: vérifie qu'elle est branchée sur le premier port PCIe (le plus proche du processeur)."
                        Add-Finding $F 'warn' "Carte graphique en PCIe x$wc" "Elle devrait être en x$wm. Souvent elle est branchée sur le mauvais port de la carte mère: tu peux perdre des FPS." 2 -Id 'gpu-pcie' -Fix (New-Fix `
                            -Why 'Avec moins de lignes PCIe, la carte graphique reçoit ses données moins vite, surtout quand sa mémoire vidéo est pleine.' `
                            -Steps @('Éteins le PC et débranche le câble d''alimentation.', 'Ouvre le boîtier: la carte graphique doit être sur le port PCIe le plus haut (le plus proche du processeur), souvent renforcé en métal.', 'Vérifie qu''elle est bien enfoncée et que le clip de verrouillage est fermé.', 'Si elle est déjà sur le bon port, un SSD branché sur certains emplacements M.2 peut partager ses lignes: regarde le manuel de ta carte mère.'))
                    }
                }
            }
        }
        [void]$cards.Add($c)
    }

    # --- Portable à deux cartes graphiques: les jeux doivent utiliser la carte dédiée
    $dedicated = @($gpus | Where-Object { $_.Name -match $dedicatedPattern } | Select-Object -First 1)
    $integrated = @($gpus | Where-Object { $_.Name -notmatch $dedicatedPattern -and $_.Name -notmatch 'Microsoft Basic' })
    if ($script:IsLaptop -and $dedicated.Count -and $integrated.Count) {
        Set-Status 'Recherche de tes jeux (Steam, Epic)...'
        $dgpu = $dedicated[0].Name
        $games = @(Invoke-Async ([scriptblock]::Create("function Get-InstalledGames {${function:Get-InstalledGames}}; Get-InstalledGames")))
        $todo = @($games | Where-Object { $g = $_; @($g.Exes | Where-Object { (Get-GpuPreference $_) -notmatch 'GpuPreference=2' }).Count })
        if (-not $games.Count) {
            Add-Finding $F 'info' 'Jeux sur la carte graphique dédiée' "Aucun jeu Steam ou Epic trouvé. Pour tes autres jeux, choisis « Hautes performances » dans Paramètres > Écran > Graphiques." 0 -Id 'hybrid-gpu' -Fix (New-Fix `
                -Why "Ton portable a deux cartes graphiques. Windows peut lancer un jeu sur la puce intégrée, beaucoup moins puissante que ta $dgpu." `
                -Steps @('Ouvre Paramètres > Système > Écran > Graphiques.', 'Ajoute ton jeu (bouton « Parcourir ») s''il n''est pas dans la liste.', 'Clique dessus, puis « Options » et choisis « Hautes performances ».') `
                -Open 'ms-settings:display-advancedgraphics' -OpenLabel 'Paramètres graphiques')
        } elseif ($todo.Count) {
            $names = @($todo | ForEach-Object { $_.Name })
            $list = if ($names.Count -gt 6) { (($names | Select-Object -First 6) -join ', ') + " et $($names.Count - 6) autre$(if ($names.Count -gt 7) {'s'})" } else { $names -join ', ' }
            Add-Finding $F 'warn' "$($todo.Count) jeu$(if ($todo.Count -gt 1) {'x'}) sans carte graphique imposée" "Windows peut lancer ces jeux sur la puce intégrée au lieu de ta $dgpu : $list." 2 -Id 'hybrid-gpu' -Fix (New-Fix -Auto `
                -What @("Régler $($todo.Count) jeu$(if ($todo.Count -gt 1) {'x'}) Steam / Epic sur « Hautes performances » pour qu'ils utilisent toujours la $dgpu : $list.", 'C''est le même réglage que Paramètres > Écran > Graphiques, en une seule fois.') `
                -Why "Ton portable a deux cartes graphiques. Sur la puce intégrée, un jeu peut tourner 3 à 5 fois moins vite. Pense aussi à brancher le chargeur: sur batterie, la carte dédiée est bridée." `
                -Run { param($a) foreach ($g in $a.Games) { foreach ($e in $g.Exes) { if ((Get-GpuPreference $e) -notmatch 'GpuPreference=2') { Set-Reg $DxPath $e 'GpuPreference=2;' 'String' } } } } `
                -RunArgs @{ Games = $todo } -Open 'ms-settings:display-advancedgraphics' -OpenLabel 'Paramètres graphiques')
        } else {
            Add-Finding $F 'ok' 'Jeux sur la carte graphique dédiée' "Tes $($games.Count) jeux Steam / Epic utilisent la $dgpu." 2 -Id 'hybrid-gpu'
        }
    }

    # --- Écrans
    $displays = @()
    try { $displays = @([OGNative]::GetDisplays()) } catch { Write-Log "Écrans: $_" }
    if ($displays.Count) {
        $c = New-Component 'HZ' 'Écrans' "$($displays.Count) écran$(if ($displays.Count -gt 1) {'s'}) connecté$(if ($displays.Count -gt 1) {'s'})"
        $dispTexts = @()
        $i = 0
        foreach ($d in $displays) {
            $i++
            $p = $d -split '\|'
            $w = [int]$p[2]; $h = [int]$p[3]; $cur = [int]$p[4]; $max = [int]$p[5]
            $dispTexts += "${w}x$h à $cur Hz"
            $dev = $p[0]
            $dispId = "display:$dev"
            # Seul l'écran principal (celui où l'on joue) compte dans la note.
            $primary = ($displays.Count -eq 1) -or ($p.Count -gt 6 -and $p[6] -eq '1')
            $role = if ($displays.Count -gt 1) { if ($primary) { ' (principal)' } else { ' (secondaire)' } } else { '' }
            $bridled = $max -gt 60 -and ($max - $cur) -ge 5
            $ignored = $bridled -and ($script:Ignored -contains $dispId)
            $sev = if ($primary) { 'bad' } else { 'warn' }
            $lineColor = if ($ignored) { '#9AA3B2' } elseif ($bridled) { $Colors[$sev] } else { '#E6E8EE' }
            $lineText = "${w}x$h à $cur Hz" + $(if ($ignored) { ' (volontaire)' } elseif ($bridled) { " (peut faire $max Hz)" } else { '' })
            $c.Lines["Écran $i$role"] = @($lineText, $lineColor)
            if ($bridled) {
                if (-not $ignored) {
                    $noteTxt = "L'écran $i tourne à $cur Hz au lieu de $max Hz." + $(if (-not $primary) { " C'est un écran secondaire : ça ne compte pas dans la note." } else { '' })
                    Add-Note $c $sev $noteTxt
                }
                $detail = "Ton écran $i ($w x $h) peut monter à $max Hz mais Windows l'utilise à $cur Hz." +
                    $(if (-not $primary) { " Écran secondaire : simple avertissement, il ne fait pas baisser ta note." } else { '' })
                Add-Finding $F $sev "Écran $i$role bridé à $cur Hz" $detail $(if ($primary) { 3 } else { 0 }) -Id $dispId -Fix (New-Fix -Auto `
                    -What @("Passer l'écran $i ($w x $h) de $cur Hz à $max Hz, sans changer sa résolution.") `
                    -Why "Plus de Hz, c'est une image plus fluide et moins de latence: c'est le réglage qui se voit le plus en jeu. Si tu utilises cet écran seulement pour des vidéos, clique sur « Ignorer (c'est voulu) »: il ne comptera plus dans ton score. Si l'écran devient noir après le changement, ne touche à rien: l'app revient toute seule à l'ancien réglage au bout de 15 secondes." `
                    -Run { param($a) Set-DisplayRate $a.Device $a.Hz } -RunArgs @{ Device = $dev; Hz = $max } `
                    -Open 'ms-settings:display-advanced' -OpenLabel 'Affichage avancé')
            } elseif ($cur -gt 60) {
                Add-Finding $F 'ok' "Écran $i$role à $cur Hz" 'Ton écran tourne à sa fréquence maximale.' $(if ($primary) { 3 } else { 0 }) -Id $dispId
            } else {
                Add-Finding $F 'info' "Écran $i$role en 60 Hz" "Ton écran est à sa fréquence maximale (60 Hz). Un écran 144 Hz est l'une des meilleures améliorations pour jouer." 0 -Id $dispId
            }
        }
        $info['Écran'] = $dispTexts -join ' + '
        if ($c.Status -ne 'ok') { $c.Action = 'ms-settings:display-advanced'; $c.ActionLabel = 'Affichage avancé' }
        [void]$cards.Add($c)
    }
    Update-UI

    # --- Mémoire vive
    Set-Status 'Analyse de la mémoire...'
    $mem = @($script:AnalysisData.Mem)
    if ($mem.Count) {
        $first = $mem[0]
        $totalGB = [math]::Round((($mem | Measure-Object Capacity -Sum).Sum) / 1GB)
        $speed = if ($first.ConfiguredClockSpeed) { [int]$first.ConfiguredClockSpeed } else { [int]$first.Speed }
        $memType = switch ([int]$first.SMBIOSMemoryType) { 24 { 'DDR3' } 26 { 'DDR4' } 30 { 'LPDDR4' } 34 { 'DDR5' } 35 { 'LPDDR5' } default { '' } }
        $info['Mémoire'] = "$totalGB Go $memType à $speed MT/s ($($mem.Count) barrette$(if ($mem.Count -gt 1) {'s'}))"

        $c = New-Component 'RAM' 'Mémoire vive' "$totalGB Go $memType à $speed MT/s"
        $totB = [double]$os.TotalVisibleMemorySize * 1KB
        $usedB = $totB - [double]$os.FreePhysicalMemory * 1KB
        $usedPct = 100 * $usedB / $totB
        [void]$c.Bars.Add(@{ Label = 'Utilisée en ce moment'; Value = $usedPct; Text = "$(Format-Size $usedB) sur $(Format-Size $totB)"; Color = (Get-LoadColor $usedPct 80 90) })
        if ($usedPct -ge 85) { Add-Note $c 'warn' 'Mémoire presque pleine en ce moment: ferme ton navigateur et les programmes inutiles avant de jouer.' }
        $c.Lines['Mode'] = if ($mem.Count -ge 2) { @('Double canal', $Colors.ok) } else { @('Simple canal', $Colors.warn) }
        $n = 0
        foreach ($m in $mem) {
            $n++
            $brand = ([string]$m.Manufacturer).Trim()
            if ($brand -match '^(Unknown|Undefined|0+)$') { $brand = '' }
            $c.Lines["Barrette $n"] = (@("$([math]::Round($m.Capacity / 1GB)) Go", $brand, ([string]$m.PartNumber).Trim()) | Where-Object { $_ }) -join ' '
        }
        $md = $data.MemDiag
        if ($md) {
            $when = $md.TimeCreated.ToString('dd/MM/yyyy')
            if ($md.Id -in 1101, 1201) { $c.Lines['Test mémoire Windows'] = @("Aucune erreur ($when)", $Colors.ok) }
            elseif ($md.Id -in 1102, 1202) {
                $c.Lines['Test mémoire Windows'] = @("Erreurs détectées ($when)", $Colors.bad)
                Add-Note $c 'bad' "Le test mémoire de Windows a trouvé des erreurs. Désactive l'XMP / EXPO pour tester, et si ça continue, une barrette est défectueuse."
                Add-Finding $F 'bad' 'Mémoire défectueuse ou instable' "Le dernier test mémoire de Windows a trouvé des erreurs: plantages et écrans bleus possibles." 3 -Id 'ram-errors' -Fix (New-Fix `
                    -Why 'Une mémoire qui fait des erreurs provoque plantages, écrans bleus et fichiers corrompus.' `
                    -Steps @('Entre dans le BIOS et remets la mémoire en réglage par défaut (désactive XMP / EXPO).', 'Relance le test mémoire de Windows avec le bouton ci dessous.', 'Si les erreurs continuent, teste les barrettes une par une: celle qui provoque des erreurs est défectueuse.') `
                    -What @('Lancer le test mémoire de Windows (le PC redémarre, le test dure environ 15 minutes).') `
                    -Run { Start-Process 'mdsched.exe' } -RunLabel 'Lancer le test mémoire' -NoRescan -Done 'Le test mémoire de Windows est lancé: choisis « Redémarrer maintenant ».')
            }
        } else {
            $c.Lines['Test mémoire Windows'] = @('Jamais lancé', $Muted)
        }
        $c.Action = 'run:mdsched.exe'; $c.ActionLabel = 'Tester la mémoire (redémarre le PC)'
        [void]$cards.Add($c)

        $ramFix = New-Fix `
            -Why 'Quand la mémoire est pleine, Windows utilise le disque à la place: grosses saccades garanties.' `
            -Steps @('En attendant: ferme ton navigateur, Discord et les programmes inutiles avant de jouer.', 'Regarde combien d''emplacements libres a ta carte mère (ou si ton portable accepte plus de mémoire).', 'Achète un kit identique à ta mémoire actuelle (même type et même vitesse), ou un kit de 2 x 8 Go / 2 x 16 Go.')
        if ($totalGB -lt 8)      { Add-Finding $F 'bad'  "Seulement $totalGB Go de mémoire" "Trop peu pour les jeux actuels: 16 Go est le minimum conseillé aujourd'hui." 2 -Id 'ram-amount' -Fix $ramFix }
        elseif ($totalGB -lt 16) { Add-Finding $F 'warn' "$totalGB Go de mémoire" 'Ça passe, mais beaucoup de jeux récents conseillent 16 Go.' 2 -Id 'ram-amount' -Fix $ramFix }
        else                     { Add-Finding $F 'ok'   "$totalGB Go de mémoire" 'Suffisant pour les jeux actuels.' 2 -Id 'ram-amount' }

        $channelFix = New-Fix `
            -Why 'En double canal, la mémoire a deux fois plus de débit. Les jeux qui dépendent du processeur y gagnent beaucoup.' `
            -Steps @('Ajoute une 2e barrette identique (même capacité, même vitesse), ou remplace par un kit de 2 barrettes.', 'Sur une carte mère à 4 emplacements, mets les 2 barrettes sur les ports A2 et B2 (en général le 2e et le 4e en partant du processeur, voir le manuel).', 'Relance l''analyse: la ligne « Mode » doit indiquer « Double canal ».')
        if ($mem.Count -eq 1) {
            if ($script:IsLaptop) {
                Add-Finding $F 'warn' 'Une seule barrette de mémoire' "La mémoire fonctionne sans doute en simple canal, ce qui peut coûter beaucoup de FPS (surtout avec une puce graphique intégrée). Vérifie si ton portable accepte une 2e barrette." 1 -Id 'ram-channel' -Fix $channelFix
            } else {
                Add-Finding $F 'bad' 'Une seule barrette de mémoire' "La mémoire fonctionne en simple canal: jusqu'à 10 à 30 % de FPS en moins dans certains jeux. Ajoute une 2e barrette identique (ou prends un kit de 2)." 3 -Id 'ram-channel' -Fix $channelFix
            }
        } else {
            Add-Finding $F 'ok' 'Mémoire en double canal' "$($mem.Count) barrettes installées." 2 -Id 'ram-channel'
        }

        if (-not $script:IsLaptop -and $memType -in 'DDR4', 'DDR5') {
            $low = ($memType -eq 'DDR4' -and $speed -le 2666) -or ($memType -eq 'DDR5' -and $speed -le 4800)
            if ($low) {
                Add-Finding $F 'warn' "Mémoire à $speed MT/s: profil XMP / EXPO à vérifier" "La plupart des barrettes gaming sont vendues pour une vitesse plus élevée, mais il faut activer le profil XMP (Intel) ou EXPO / DOCP (AMD) dans le BIOS. Si tes barrettes sont vendues pour $speed MT/s, tout va bien." 2 -Id 'ram-xmp' -Fix (New-Fix `
                    -Why 'Sans profil XMP / EXPO, la mémoire tourne à une vitesse de sécurité: souvent 10 à 20 % de FPS minimum en moins dans les jeux gourmands en processeur.' `
                    -Steps @('Clique sur « Redémarrer dans le BIOS » (enregistre ton travail avant).', 'Dans le BIOS, cherche XMP (Intel), EXPO ou DOCP (AMD). Souvent dans le menu Ai Tweaker, OC ou Extreme Tweaker.', 'Choisis le Profil 1, puis enregistre et quitte avec F10.', 'Si le PC devient instable, retourne dans le BIOS et remets ce réglage sur Auto.') `
                    -What @('Redémarrer le PC directement dans le BIOS, pour que tu n''aies pas à chercher la bonne touche au démarrage.') `
                    -Run $BiosRun -RunLabel 'Redémarrer dans le BIOS' -Confirm $BiosConfirm -NoRescan -Done $BiosDone)
            } else {
                Add-Finding $F 'ok' "Mémoire à $speed MT/s" 'Le profil XMP / EXPO semble actif.' 2 -Id 'ram-xmp'
            }
        }
    }
    Update-UI

    # --- Disques
    Set-Status 'Analyse des disques...'
    $sysDisk = $null
    $sysDisk = [string]$data.SysDisk
    foreach ($dd in @($data.Disks)) {
        $d = $dd.Disk
        $media = [string]$d.MediaType; $bus = [string]$d.BusType
        $kind = if ($bus -eq 'NVMe') { 'SSD NVMe' } elseif ($media -eq 'SSD') { 'SSD' } elseif ($media -eq 'HDD') { 'Disque dur' } else { 'Disque' }
        $tag = if ($bus -eq 'USB') { 'USB' } elseif ($media -eq 'HDD') { 'HDD' } else { 'SSD' }
        $isSys = [string]$d.DeviceId -eq $sysDisk
        $sous = "$kind, $(Format-Size $d.Size)" + $(if ($bus -eq 'USB') { ', externe' } else { '' }) + $(if ($isSys) { ', Windows' } else { '' })
        $c = New-Component $tag (([string]$d.FriendlyName).Trim()) $sous

        $parts = @()
        $parts = @($dd.Vols)
        foreach ($pt in $parts) {
            $v = $pt.Vol
            if (-not $v -or -not $v.Size) { continue }
            $pct = 100 * ($v.Size - $v.SizeRemaining) / $v.Size
            $label = "Lecteur $($pt.DriveLetter):" + $(if ($v.FileSystemLabel) { " $($v.FileSystemLabel)" } else { '' })
            [void]$c.Bars.Add(@{ Label = $label; Value = $pct; Text = "$(Format-Size $v.SizeRemaining) libres sur $(Format-Size $v.Size)"; Color = (Get-LoadColor $pct 80 90) })
            if ($pct -ge 90) { Add-Note $c 'warn' "Le lecteur $($pt.DriveLetter): est presque plein." }
        }

        switch ([string]$d.HealthStatus) {
            'Healthy'   { $c.Lines['État SMART'] = @('Bon', $Colors.ok) }
            'Warning'   { $c.Lines['État SMART'] = @('Avertissement', $Colors.warn); Add-Note $c 'warn' 'Le disque signale un problème: sauvegarde tes fichiers importants.' }
            'Unhealthy' { $c.Lines['État SMART'] = @('Défaillant', $Colors.bad); Add-Note $c 'bad' 'Le disque est en train de lâcher: sauvegarde tes fichiers MAINTENANT et prévois de le remplacer.' }
            default     { $c.Lines['État SMART'] = @('Inconnu', $Muted) }
        }
        $rel = $null
        $rel = $dd.Rel
        if ($rel) {
            if ($null -ne $rel.Wear -and $tag -ne 'HDD') {
                $wear = [int]$rel.Wear
                $c.Lines['Usure'] = @("$wear %", (Get-LoadColor $wear 70 90))
                if ($wear -ge 90) { Add-Note $c 'bad' "SSD usé à $wear %: il approche de sa fin de vie, prévois son remplacement." }
                elseif ($wear -ge 70) { Add-Note $c 'warn' "SSD usé à $wear %: surveille le et sauvegarde tes fichiers." }
            }
            if ($rel.Temperature -gt 0) {
                $t = [int]$rel.Temperature
                $warnT = if ($tag -eq 'HDD') { 50 } else { 70 }
                $c.Lines['Température'] = @("$t °C", (Get-LoadColor $t $warnT ($warnT + 10)))
                if ($t -ge $warnT) { Add-Note $c 'warn' "Disque chaud ($t °C): vérifie la ventilation du boîtier (un dissipateur aide beaucoup sur un SSD NVMe)." }
            }
            if ($rel.PowerOnHours -gt 0) { $c.Lines["Heures d'utilisation"] = '{0:N0} h' -f $rel.PowerOnHours }
            if ($null -ne $rel.ReadErrorsUncorrected) {
                $re = [long]$rel.ReadErrorsUncorrected
                $c.Lines['Erreurs de lecture'] = @("$re", $(if ($re -gt 0) { $Colors.warn } else { $Colors.ok }))
                if ($re -gt 0) { Add-Note $c 'warn' "$re erreur(s) de lecture non corrigée(s): sauvegarde tes fichiers importants." }
            }
        } else {
            $c.Lines['Usure et température'] = @('Non fournies par ce disque', $Muted)
        }

        $diskFix = New-Fix `
            -Why (($c.Notes | ForEach-Object { $_.Text }) -join ' ') `
            -Steps @('Copie tes fichiers importants (photos, documents, sauvegardes de jeux) sur un autre disque ou dans le cloud.', 'Regarde le détail dans la carte du disque (usure, température, erreurs, remplissage).', 'Si le disque est défaillant ou très usé, remplace le rapidement: les jeux se réinstallent, tes fichiers perso non.')
        if ($c.Status -eq 'bad') {
            Add-Finding $F 'bad' "Disque en mauvaise santé: $($d.FriendlyName)" 'Sauvegarde tes fichiers importants dès maintenant.' 3 -Id "disk:$($d.FriendlyName)" -Fix $diskFix
        } elseif ($c.Status -eq 'warn') {
            Add-Finding $F 'warn' "Disque à surveiller: $($d.FriendlyName)" 'Détails dans la carte du disque, dans « Santé des composants ».' 2 -Id "disk:$($d.FriendlyName)" -Fix $diskFix
        }
        if ($isSys) {
            if ($media -eq 'HDD') {
                Add-Finding $F 'bad' 'Windows est sur un disque dur mécanique' "Démarrage lent, chargements très longs et saccades dans les jeux récents. Passer à un SSD est l'amélioration la plus visible possible." 3 -Id 'disk-hdd' -Fix (New-Fix `
                    -Why 'Un SSD est 5 à 50 fois plus rapide qu''un disque dur: Windows démarre en quelques secondes et les jeux chargent beaucoup plus vite.' `
                    -Steps @('Achète un SSD (un NVMe si ta carte mère a un emplacement M.2, sinon un SSD SATA 2,5 pouces).', 'Clone ton disque actuel vers le SSD avec le logiciel gratuit du fabricant (Samsung Magician, Crucial Acronis, WD...), ou réinstalle Windows dessus.', 'Garde l''ancien disque dur pour stocker tes fichiers.'))
            } elseif ($kind -like 'SSD*') {
                Add-Finding $F 'ok' "Windows est sur un $kind" 'Chargements rapides.' 3 -Id 'disk-hdd'
            }
            $info['Disque système'] = "$kind $($d.FriendlyName)"
        }
        [void]$cards.Add($c)
    }
    $ld = $data.LogicalC
    if ($ld -and $ld.Size) {
        $freeGB = [math]::Round($ld.FreeSpace / 1GB)
        $pct = [math]::Round(100 * $ld.FreeSpace / $ld.Size)
        $spaceFix = New-Fix -Auto `
            -What @('Supprimer les fichiers temporaires de Windows et de ton compte.', 'Supprimer les fichiers de mises à jour Windows déjà installées et les anciens rapports d''erreurs.', 'Tes documents, tes jeux et la corbeille ne sont pas touchés.') `
            -Why 'Windows et les jeux ralentissent quand le disque système est plein, et les mises à jour de jeux peuvent échouer. Si ça ne suffit pas, désinstalle les jeux auxquels tu ne joues plus.' `
            -Run { Invoke-CleanAll } -Open 'tab:4' -OpenLabel 'Onglet Nettoyage'
        if ($pct -lt 10)     { Add-Finding $F 'bad'  "Disque presque plein ($freeGB Go libres)" 'Windows et les jeux ralentissent quand le disque est plein.' 2 -Id 'disk-space' -Fix $spaceFix }
        elseif ($pct -lt 20) { Add-Finding $F 'warn' "Espace disque limité ($freeGB Go libres)" 'Garde au moins 20 % de libre pour de bonnes performances.' 2 -Id 'disk-space' -Fix $spaceFix }
        else                 { Add-Finding $F 'ok'   "Espace disque suffisant ($freeGB Go libres)" '' 2 -Id 'disk-space' }
    }
    Update-UI

    # --- Carte mère et BIOS
    Set-Status 'Analyse de la carte mère...'
    $bb = $data.BaseBoard
    $bios = $data.BIOS
    $c = New-Component 'BIOS' 'Carte mère et BIOS' ("$($bb.Manufacturer) $($bb.Product)".Trim())
    if ($bios) {
        $c.Lines['Version du BIOS'] = [string]$bios.SMBIOSBIOSVersion
        if ($bios.ReleaseDate) {
            $years = [math]::Floor(((Get-Date) - $bios.ReleaseDate).TotalDays / 365)
            $ageTxt = if ($years -ge 1) { " (il y a $years an$(if ($years -gt 1) {'s'}))" } else { '' }
            $c.Lines['Date du BIOS'] = @("$($bios.ReleaseDate.ToString('dd/MM/yyyy'))$ageTxt", $(if ($years -ge 3) { $Colors.warn } else { '#E6E8EE' }))
            if ($years -ge 3 -and -not $script:IsLaptop) {
                Add-Note $c 'info' "BIOS de plus de 3 ans: une mise à jour depuis le site du fabricant peut améliorer la stabilité et la compatibilité mémoire. À faire avec prudence (ne jamais couper le courant pendant la mise à jour)."
            }
        }
    }
    $c.Lines['Démarrage'] = if ($env:firmware_type -eq 'UEFI') { 'UEFI' } else { @('Legacy (ancien BIOS)', $Colors.warn) }
    try {
        if ($null -eq $data.SecureBoot) { throw 'indisponible' }
        $sb = [bool]$data.SecureBoot
        if ($sb) { $c.Lines['Secure Boot'] = @('Activé', $Colors.ok) }
        else {
            $c.Lines['Secure Boot'] = @('Désactivé', $Colors.warn)
            Add-Note $c 'warn' "Certains jeux récents (Valorant, Battlefield 6, Call of Duty) exigent le Secure Boot pour leur anti triche. Active le dans le BIOS si un jeu refuse de se lancer."
            Add-Finding $F 'warn' 'Secure Boot désactivé' "Des jeux comme Valorant, Battlefield 6 ou les derniers Call of Duty peuvent refuser de se lancer." 1 -Id 'secureboot' -Fix (New-Fix `
                -Why 'Les anti triche récents vérifient que le Secure Boot est actif. Sans lui, certains jeux refusent de démarrer.' `
                -Steps @('Clique sur « Redémarrer dans le BIOS » (enregistre ton travail avant).', 'Va dans le menu Boot (ou Sécurité) > Secure Boot et mets le sur « Enabled ». Sur certaines cartes, il faut d''abord mettre « OS Type » sur « Windows UEFI mode ».', 'Enregistre avec F10 et redémarre.') `
                -What @('Redémarrer le PC directement dans le BIOS.') `
                -Run $BiosRun -RunLabel 'Redémarrer dans le BIOS' -Confirm $BiosConfirm -NoRescan -Done $BiosDone)
        }
    } catch { $c.Lines['Secure Boot'] = @('Non disponible', $Muted) }
    try {
        if (-not $data.TpmOk) { throw 'indisponible' }
        $tpm = $data.Tpm
        if ($tpm) { $c.Lines['Puce TPM'] = @("Version $(([string]$tpm.SpecVersion -split ',')[0].Trim())", $Colors.ok) }
        else { $c.Lines['Puce TPM'] = @('Non détectée', $Colors.warn) }
    } catch {}
    [void]$cards.Add($c)

    # --- Stabilité
    Set-Status 'Lecture du journal des erreurs...'
    $bsod = [int]$data.Bsod
    $crash = [int]$data.Crash
    $wheaErr = [int]$data.WheaErr
    $wheaWarn = [int]$data.WheaWarn
    $uptime = (Get-Date) - $os.LastBootUpTime
    $c = New-Component 'SYS' 'Stabilité du système' 'Sur les 30 derniers jours'
    $c.Lines['Écrans bleus'] = @("$bsod", $(if ($bsod -ge 2) { $Colors.bad } elseif ($bsod) { $Colors.warn } else { $Colors.ok }))
    $c.Lines['Arrêts brutaux'] = @("$crash", $(if ($crash) { $Colors.warn } else { $Colors.ok }))
    $c.Lines['Erreurs matérielles'] = @("$wheaErr", $(if ($wheaErr) { $Colors.bad } else { $Colors.ok }))
    $c.Lines['Erreurs corrigées'] = @("$wheaWarn", $(if ($wheaWarn -ge 20) { $Colors.warn } else { $Colors.ok }))
    $c.Lines['Allumé depuis'] = Format-Duration $uptime
    $crashFix = New-Fix `
        -Why 'Les plantages viennent le plus souvent d''un pilote défectueux, d''un réglage XMP / overclock trop agressif ou d''une surchauffe.' `
        -Steps @('Clique sur « Historique » pour voir quand le PC a planté et quel programme ou pilote est en cause.', 'Mets à jour Windows et le pilote de ta carte graphique.', 'Si tu as activé XMP / EXPO, un overclock ou un undervolt, remets les réglages par défaut dans le BIOS pour tester.', 'Surveille les températures en jeu avec HWiNFO: au delà de 90 °C, nettoie le PC ou améliore le refroidissement.') `
        -Open 'run:perfmon.exe /rel' -OpenLabel 'Historique'
    if ($bsod -ge 2) {
        Add-Note $c 'bad' "$bsod écrans bleus ce mois ci. Causes fréquentes: pilote défectueux, XMP / overclock instable, surchauffe."
        Add-Finding $F 'bad' "$bsod écrans bleus en 30 jours" 'Ton PC plante régulièrement.' 3 -Id 'bsod' -Fix $crashFix
    } elseif ($bsod -eq 1) {
        Add-Note $c 'warn' "Un écran bleu ce mois ci. Si ça se reproduit, regarde l'historique de fiabilité."
        Add-Finding $F 'warn' 'Un écran bleu en 30 jours' "Un plantage isolé n'est pas grave, mais surveille si ça se reproduit." 2 -Id 'bsod' -Fix $crashFix
    }
    if ($wheaErr) {
        Add-Note $c 'bad' "$wheaErr erreur(s) matérielle(s) grave(s). Souvent un overclock, un undervolt ou un profil XMP instable, parfois une surchauffe."
        Add-Finding $F 'bad' 'Erreurs matérielles détectées' "Windows a enregistré $wheaErr erreur(s) matérielle(s) grave(s) ce mois ci. Souvent un overclock / undervolt ou un profil XMP instable." 2 -Id 'whea' -Fix $crashFix
    } elseif ($wheaWarn -ge 20) {
        Add-Note $c 'warn' "$wheaWarn erreurs matérielles corrigées automatiquement. Pas critique, mais souvent signe d'un réglage limite (overclock, undervolt, XMP)."
    }
    if ($crash -gt $bsod) {
        Add-Note $c 'info' "Arrêts brutaux: coupure de courant, bouton d'alimentation maintenu, ou PC qui s'éteint seul (alimentation, surchauffe) si ça arrive en jeu."
    }
    if ($uptime.TotalDays -ge 7) {
        Add-Note $c 'info' "Pense à redémarrer de temps en temps: avec le démarrage rapide de Windows, « Arrêter » ne remet pas tout à zéro."
    }
    $c.Action = 'run:perfmon.exe /rel'; $c.ActionLabel = 'Historique de fiabilité'
    [void]$cards.Add($c)

    # --- Batterie
    if ($script:IsLaptop) {
        $b = $battery[0]
        $c = New-Component 'BAT' 'Batterie' ([string]$b.Name).Trim()
        $design = (Get-CimInstance -Namespace root\wmi -ClassName BatteryStaticData -ErrorAction SilentlyContinue | Select-Object -First 1).DesignedCapacity
        $full = (Get-CimInstance -Namespace root\wmi -ClassName BatteryFullChargedCapacity -ErrorAction SilentlyContinue | Select-Object -First 1).FullChargedCapacity
        if ($design -and $full) {
            $health = [math]::Min(100.0, 100 * $full / $design)
            [void]$c.Bars.Add(@{ Label = 'Santé de la batterie'; Value = $health; Text = "$([int]$health) %"; Color = $(if ($health -lt 60) { $Colors.bad } elseif ($health -lt 80) { $Colors.warn } else { $Colors.ok }) })
            $c.Lines["Capacité d'origine"] = '{0:N0} mWh' -f $design
            $c.Lines['Capacité actuelle'] = '{0:N0} mWh' -f $full
            if ($health -lt 60) { Add-Note $c 'bad' 'La batterie a perdu beaucoup de capacité: elle mérite d''être remplacée.' }
            elseif ($health -lt 80) { Add-Note $c 'warn' 'La batterie commence à s''user.' }
        }
        $cycles = (Get-CimInstance -Namespace root\wmi -ClassName BatteryCycleCount -ErrorAction SilentlyContinue | Select-Object -First 1).CycleCount
        if ($cycles) { $c.Lines['Cycles de charge'] = "$cycles" }
        if ($null -ne $b.EstimatedChargeRemaining) { $c.Lines['Charge'] = "$($b.EstimatedChargeRemaining) %$(if ($b.BatteryStatus -eq 2) { ', sur secteur' } else { ', sur batterie' })" }
        [void]$cards.Add($c)
    }

    # --- Réseau
    Set-Status 'Analyse du réseau...'
    $script:Net = $data.Net
    if (-not $script:Net) { $script:Net = Get-ActiveNet }
    if ($script:Net) {
        $info['Réseau'] = "$(if ($script:Net.Wifi) { 'Wi-Fi' } else { 'Ethernet (câble)' }), $($script:Net.Speed)"
        $c = New-Component 'NET' 'Réseau' $script:Net.Desc
        $c.Lines['Connexion'] = if ($script:Net.Wifi) { @('Wi-Fi', $Colors.warn) } else { @('Câble Ethernet', $Colors.ok) }
        $c.Lines['Vitesse de la carte'] = [string]$script:Net.Speed
        $c.Action = 'tab:3'; $c.ActionLabel = 'Tester le ping'
        if ($script:Net.Wifi) {
            Add-Note $c 'info' 'Le Wi-Fi ajoute de la latence et des pertes de paquets. Pour jouer en ligne, le câble reste le plus stable.'
            Add-Finding $F 'warn' 'Connexion en Wi-Fi' 'Le Wi-Fi ajoute de la latence et des pertes de paquets, surtout loin de la box.' 1 -Id 'wifi' -Fix (New-Fix `
                -Why 'En Wi-Fi, le ping varie et des paquets se perdent: ça se traduit par du lag et des tirs qui ne comptent pas.' `
                -Steps @('Si possible, branche un câble Ethernet entre le PC et la box (un long câble plat se cache facilement).', 'Sinon, un adaptateur CPL (réseau par les prises électriques) est une bonne alternative.', 'En Wi-Fi, connecte toi au réseau 5 GHz ou 6 GHz de ta box plutôt qu''au 2,4 GHz, et rapproche le PC de la box.') `
                -Open 'tab:3' -OpenLabel 'Tester le ping')
        } else {
            Add-Finding $F 'ok' 'Connexion par câble' 'La connexion la plus stable pour jouer en ligne.' 1 -Id 'wifi'
        }
        [void]$cards.Add($c)
    }
    $info['Windows'] = "$($os.Caption -replace 'Microsoft ', '') (build $($os.BuildNumber))"
    $info['Type'] = if ($script:IsLaptop) { 'PC portable' } else { 'PC fixe' }

    # --- Démarrage
    $startItems = @(Get-StartupItems | Where-Object { $_.Enabled })
    $enabled = $startItems.Count
    if ($enabled -gt 12) {
        $safe = @($startItems | Where-Object { $_.Nom -match $SafeStartup -or $_.Commande -match $SafeStartup })
        $startFix = if ($safe.Count) {
            New-Fix -Auto `
                -What @("Désactiver le lancement automatique de : $(($safe | ForEach-Object { $_.Nom }) -join ', ').", 'Ces programmes restent installés et se lancent normalement quand tu les ouvres.') `
                -Why 'Chaque programme au démarrage ralentit l''allumage du PC et occupe de la mémoire pendant tes parties. Pour les autres, choisis toi même dans l''onglet Démarrage.' `
                -Run { param($a) foreach ($i in $a.Items) { Set-StartupState $i $false } } -RunArgs @{ Items = $safe } `
                -Open 'tab:2' -OpenLabel 'Onglet Démarrage'
        } else {
            New-Fix -Why 'Chaque programme au démarrage ralentit l''allumage du PC et occupe de la mémoire pendant tes parties.' `
                -Steps @('Ouvre l''onglet Démarrage.', 'Désactive les programmes dont tu n''as pas besoin dès l''allumage (launchers, messageries, outils de mise à jour).', 'Garde l''antivirus et les pilotes (audio, carte graphique, souris).') `
                -Open 'tab:2' -OpenLabel 'Onglet Démarrage'
        }
        Add-Finding $F 'warn' "$enabled programmes se lancent au démarrage" "Ils ralentissent l'allumage du PC et occupent de la mémoire pendant tes parties." 1 -Id 'startup' -Fix $startFix
    } else {
        Add-Finding $F 'ok' "$enabled programmes au démarrage" 'Nombre raisonnable.' 1 'tab:2' 'Voir' -Id 'startup'
    }

    # --- Sécurité (information seulement)
    $hvci = Get-RegValue 'HKLM:\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity' 'Enabled'
    if ($hvci -eq 1) {
        Add-Finding $F 'info' 'Intégrité de la mémoire activée' "Cette protection de Windows bloque certains logiciels malveillants mais peut coûter quelques pourcents de FPS. Microsoft conseille de la laisser activée: OptiGame n'y touche pas, c'est à toi de décider." 0 -Id 'hvci' -Fix (New-Fix `
            -Why 'C''est un compromis entre sécurité et performances. Microsoft recommande de la laisser activée, et certains anti triche l''exigent.' `
            -Steps @('Si tu veux la désactiver: ouvre Sécurité Windows > Sécurité des appareils > Isolation du noyau.', 'Coupe « Intégrité de la mémoire » et redémarre.', 'Si un jeu ou un anti triche la réclame, réactive la au même endroit.') `
            -Open 'windowsdefender://coreisolation' -OpenLabel 'Isolation du noyau')
    }

    # --- Réglages gaming
    foreach ($t in (Get-AvailableTweaks)) {
        if (Test-Tweak $t) {
            Add-Finding $F 'ok' $t.Titre $t.Ok (Get-TweakWeight $t) -Id "tweak:$($t.Id)"
        } else {
            $st = if ($t.Recommended -eq $false) { 'info' } else { 'warn' }
            Add-Finding $F $st $t.Titre $t.Ko (Get-TweakWeight $t) -Id "tweak:$($t.Id)" -Fix (New-Fix -Auto `
                -What @($t.What) -Why $t.Description -Run $t.Apply -Reboot:([bool]$t.Reboot) -Restore -Open 'tab:1' -OpenLabel 'Onglet Gaming')
        }
    }

    $m = Measure-Score $F
    $score = $m.Score
    $order = @{ bad = 0; warn = 1; info = 2; ok = 3 }
    $sorted = @($F | Sort-Object @{ Expression = { $order[$_.Status] } }, @{ Expression = { -$_.Gain } }, @{ Expression = { -$_.Weight } })
    $active = @($sorted | Where-Object { $script:Ignored -notcontains $_.Id })
    $nbBad = @($active | Where-Object { $_.Status -eq 'bad' }).Count
    $nbWarn = @($active | Where-Object { $_.Status -eq 'warn' }).Count

    Show-HealthCards $cards
    Show-Improvements $active
    Show-Findings $sorted $active
    $s = Show-Score $score $nbBad $nbWarn $m.Potential
    $script:LastAnalysis = @{ Info = $info; Cards = $cards; Findings = $active; Active = $active; Score = $score; Potential = $m.Potential; Label = $s.Label; Color = $s.Color; Date = Get-Date }
    Set-Status "Analyse terminée: score de $score sur 100."
}

# ---------------------------------------------------------------------------
# Mesures en direct (dans un fil séparé pour ne pas ralentir la fenêtre)
# ---------------------------------------------------------------------------
$Live = [hashtable]::Synchronized(@{ Run = $true; Smi = $SmiPath; BaseMHz = 0 })

$LiveScript = {
    param($sync)
    function Num($v) {
        $v = ([string]$v).Trim()
        if ($v -match '^[\d\.]+$') { [double]::Parse($v, [Globalization.CultureInfo]::InvariantCulture) } else { $null }
    }
    while ($sync.Run) {
        try {
            $pi = Get-CimInstance Win32_PerfFormattedData_Counters_ProcessorInformation -Filter "Name='_Total'" -ErrorAction Stop
            $sync.Cpu = [math]::Min(100.0, [double]$pi.PercentProcessorUtility)
            $sync.CpuPerf = [double]$pi.PercentProcessorPerformance
        } catch {}
        try {
            $os = Get-CimInstance Win32_OperatingSystem -ErrorAction Stop
            $sync.RamTotal = [double]$os.TotalVisibleMemorySize * 1024
            $sync.RamUsed = ([double]$os.TotalVisibleMemorySize - [double]$os.FreePhysicalMemory) * 1024
        } catch {}
        $gpuDone = $false
        if ($sync.Smi) {
            try {
                $o = & $sync.Smi '--query-gpu=utilization.gpu,temperature.gpu,memory.used,memory.total,power.draw' '--format=csv,noheader,nounits' 2>$null | Select-Object -First 1
                if ($o) {
                    $v = $o -split ','
                    $sync.Gpu = Num $v[0]; $sync.GpuTemp = Num $v[1]
                    $sync.VramUsed = Num $v[2]; $sync.VramTotal = Num $v[3]; $sync.GpuPower = Num $v[4]
                    $gpuDone = $true
                }
            } catch {}
        }
        if (-not $gpuDone) {
            try {
                $eng = Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine -ErrorAction Stop | Where-Object { $_.Name -like '*engtype_3D' }
                $sync.Gpu = [math]::Min(100.0, [double](($eng | Measure-Object UtilizationPercentage -Sum).Sum))
            } catch {}
        }
        $sync.Updated = Get-Date
        Start-Sleep -Milliseconds $(if ($sync.Fast) { 400 } else { 1500 })
    }
}

function Set-Gauge($Val, $Bar, $Sub, $Value, [string]$Text, [string]$SubText, [double]$Warn = 75, [double]$Bad = 90) {
    if ($null -eq $Value) { $Val.Text = 'N/D'; $Bar.Value = 0; $Sub.Text = $SubText; return }
    $Val.Text = $Text
    $Bar.Value = [math]::Min(100.0, [math]::Max(0.0, [double]$Value))
    $Bar.Foreground = Get-Brush (Get-LoadColor $Value $Warn $Bad)
    $Sub.Text = $SubText
}

function Update-LiveUI {
    if (-not $Live.Updated) { return }
    $cpuSub = if ($Live.CpuPerf -and $Live.BaseMHz) { 'Fréquence {0:N1} GHz' -f ($Live.BaseMHz * $Live.CpuPerf / 100 / 1000) } else { '' }
    Set-Gauge $ui.LiveCpuVal $ui.LiveCpuBar $ui.LiveCpuSub $Live.Cpu "$([int]$Live.Cpu) %" $cpuSub

    if ($Live.RamTotal) {
        $pct = 100 * $Live.RamUsed / $Live.RamTotal
        Set-Gauge $ui.LiveRamVal $ui.LiveRamBar $ui.LiveRamSub $pct "$([int]$pct) %" "$(Format-Size $Live.RamUsed) sur $(Format-Size $Live.RamTotal)" 80 90
    }

    $gpuSub = if ($Live.VramTotal) { "VRAM {0:N1} / {1:N0} Go" -f ($Live.VramUsed / 1024), ($Live.VramTotal / 1024) } else { '' }
    $gpuText = if ($null -ne $Live.Gpu) { "$([int]$Live.Gpu) %" } else { '' }
    Set-Gauge $ui.LiveGpuVal $ui.LiveGpuBar $ui.LiveGpuSub $Live.Gpu $gpuText $gpuSub 101 101

    if ($null -ne $Live.GpuTemp) {
        $ui.LiveTempBox.Visibility = 'Visible'; $ui.LiveGrid.Columns = 4
        $powerSub = if ($null -ne $Live.GpuPower) { 'Consommation {0:N0} W' -f $Live.GpuPower } else { '' }
        Set-Gauge $ui.LiveTempVal $ui.LiveTempBar $ui.LiveTempSub $Live.GpuTemp "$([int]$Live.GpuTemp) °C" $powerSub 80 87
    } else {
        $ui.LiveTempBox.Visibility = 'Collapsed'; $ui.LiveGrid.Columns = 3
    }
    $ui.LiveStamp.Text = "Actualisé à $($Live.Updated.ToString('HH:mm:ss'))"
}

function Start-Live {
    $script:LivePs = [PowerShell]::Create()
    [void]$script:LivePs.AddScript($LiveScript.ToString()).AddArgument($Live)
    [void]$script:LivePs.BeginInvoke()
    $script:LiveTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:LiveTimer.Interval = [TimeSpan]::FromSeconds(1)
    $script:LiveTimer.Add_Tick({ try { Update-LiveUI } catch {} })
    $script:LiveTimer.Start()
}

# ---------------------------------------------------------------------------
# Onglet gaming
# ---------------------------------------------------------------------------
function Build-GamingTab {
    $panel = $ui.GamingPanel
    $panel.Children.Clear()
    $script:TweakRows = @()
    foreach ($t in (Get-AvailableTweaks)) {
        $ok = Test-Tweak $t
        $card = New-Card
        $g = New-Grid @('Auto', '*')

        $cb = New-Object System.Windows.Controls.CheckBox
        $cb.VerticalAlignment = 'Top'
        $cb.Margin = New-Thickness 0 3 14 0
        $cb.LayoutTransform = [System.Windows.Media.ScaleTransform]::new(1.25, 1.25)
        if ($ok) { $cb.IsChecked = $false; $cb.IsEnabled = $false }
        else { $cb.IsChecked = ($t.Recommended -ne $false) }
        Add-ToGrid $g $cb 0

        $sp = New-Object System.Windows.Controls.StackPanel
        $head = New-Object System.Windows.Controls.WrapPanel
        [void]$head.Children.Add((New-Text $t.Titre 14.5 '#FFFFFF' -Semi))
        if ($ok) {
            [void]$head.Children.Add((New-Badge 'Déjà optimisé' $Colors.ok))
        } else {
            $impactColor = switch ($t.Impact) { 'Important' { $Colors.bad } 'Moyen' { $Colors.warn } default { $Colors.info } }
            [void]$head.Children.Add((New-Badge "Impact $($t.Impact.ToLower())" $impactColor))
            if ($t.Recommended -eq $false) { [void]$head.Children.Add((New-Badge 'Optionnel' '#9AA3B2')) }
        }
        if ($t.Reboot) { [void]$head.Children.Add((New-Badge 'Redémarrage requis' '#9AA3B2')) }
        [void]$sp.Children.Add($head)
        $desc = New-Text $t.Description 12.5 '#9AA3B2'
        $desc.Margin = New-Thickness 0 5 0 0
        [void]$sp.Children.Add($desc)
        Add-ToGrid $g $sp 1

        $card.Child = $g
        if ($ok) { $card.Opacity = 0.7 }
        [void]$panel.Children.Add($card)
        $script:TweakRows += @{ Tweak = $t; CheckBox = $cb }
    }
}

function New-RestorePoint {
    Set-Status 'Création du point de restauration (ça peut prendre une minute)...'
    # Windows refuse sinon plus d'un point de restauration par 24 h.
    Set-Reg 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore' 'SystemRestorePointCreationFrequency' 0
    $r = Invoke-Async {
        try { Checkpoint-Computer -Description 'OptiGame' -RestorePointType MODIFY_SETTINGS -ErrorAction Stop; 'OK' }
        catch { $_.Exception.Message }
    }
    if ("$r" -eq 'OK') { Set-Status 'Point de restauration créé.'; return $true }
    Write-Log "Point de restauration: $r"
    Confirm-Action ("Impossible de créer le point de restauration.`n`nMotif: $r`n`n" +
        "La protection du système est peut-être désactivée sur ce PC. Tu peux quand même continuer: " +
        "OptiGame garde une sauvegarde de chaque réglage modifié et peut tout annuler depuis l'onglet Sauvegarde.`n`nContinuer ?")
}

function Invoke-ApplyTweaks {
    $sel = @($script:TweakRows | Where-Object { $_.CheckBox.IsEnabled -and $_.CheckBox.IsChecked })
    if (-not $sel.Count) { Show-Message 'Aucune optimisation cochée.'; return }
    Set-Busy $true
    if ($ui.ChkRestore.IsChecked -and -not (New-RestorePoint)) { Set-Status 'Annulé.'; return }
    $done = @(); $failed = @(); $reboot = $false
    $before = if ($script:LastAnalysis) { $script:LastAnalysis.Score } else { $null }
    $script:RunLog = New-Object System.Collections.ArrayList
    try {
        foreach ($r in $sel) {
            Set-Status "Application: $($r.Tweak.Titre)..."
            try {
                & $r.Tweak.Apply
                $done += $r.Tweak.Titre
                if ($r.Tweak.Reboot) { $reboot = $true }
            } catch {
                $failed += "$($r.Tweak.Titre): $($_.Exception.Message)"
                Write-Log "Échec $($r.Tweak.Id): $_"
            }
        }
    } finally {
        $log = $script:RunLog
        $script:RunLog = $null
    }
    Build-GamingTab
    Update-BackupSummary
    Invoke-Analysis
    $lines = @()
    if ($done.Count) {
        $lines += "$($done.Count) optimisation$(if ($done.Count -gt 1) {'s'}) appliquée$(if ($done.Count -gt 1) {'s'}) :"
        foreach ($d in $done) { $lines += "•  $d" }
        if ($null -ne $before) { $lines += "Score : $before → $($script:LastAnalysis.Score)" }
    }
    if ($failed) { $lines += 'Non appliqué :'; $lines += $failed }
    if ($reboot -and $done.Count) { $lines += 'Redémarre ton PC pour que tout soit pris en compte.' }
    $title = if ($done.Count) { "C'est fait !" } else { 'Rien n''a été appliqué' }
    Set-Status $title
    Show-ResultSheet $title $lines $log $null
}

# ---------------------------------------------------------------------------
# Onglet démarrage
# ---------------------------------------------------------------------------
$KeepStartup = '\b(Realtek|RtkAud|NVIDIA|AMD|Radeon|Intel|SecurityHealth|Sécurité Windows|Windows Security|Defender|Avast|AVG|Kaspersky|Bitdefender|Norton|McAfee|ESET|Malwarebytes|Synaptics|ELAN|Dolby|Nahimic|Waves|MaxxAudio|Wacom|Bluetooth)\b'
$DeviceStartup = '\b(Logitech|LGHUB|Razer|Corsair|iCUE|SteelSeries|HyperX|NGENUITY|Roccat|Glorious|Armoury|Aura|MSI Center|Mystic Light|Alienware|Stream Deck|Elgato)\b'
$HostExes = '^(rundll32|cmd|powershell|pwsh|wscript|cscript|conhost|explorer|mshta)\.exe$'

# Icône d'un programme, prête pour l'interface.
function Get-ExeIcon([string]$Exe) {
    try {
        $ic = [System.Drawing.Icon]::ExtractAssociatedIcon($Exe)
        if (-not $ic) { return $null }
        $src = [System.Windows.Interop.Imaging]::CreateBitmapSourceFromHIcon($ic.Handle, [System.Windows.Int32Rect]::Empty,
            [System.Windows.Media.Imaging.BitmapSizeOptions]::FromEmptyOptions())
        $src.Freeze()
        $ic.Dispose()
        $src
    } catch { $null }
}

# Nom lisible, éditeur et conseil pour une entrée de démarrage.
function Get-StartupInfo($Item) {
    $name = $Item.Nom; $company = ''
    $isHost = [IO.Path]::GetFileName($Item.Exe) -match $HostExes
    try {
        $vi = [Diagnostics.FileVersionInfo]::GetVersionInfo($Item.Exe)
        $company = ([string]$vi.CompanyName).Trim()
        if (-not $isHost) {
            $prod = ([string]$vi.ProductName).Trim(); $desc = ([string]$vi.FileDescription).Trim()
            if ($prod -and $prod.Length -le 40 -and $prod -notmatch 'Windows.*(Operating System|Système)') { $name = $prod }
            elseif ($desc -and $desc.Length -le 50) { $name = $desc }
        }
    } catch {}
    $file = [IO.Path]::GetFileName($Item.Exe)
    # Noms trop vagues (« Update », « Launcher »...) : le nom de l'entrée est plus parlant.
    if ($name -match '^(Update|Updater|Launcher|Helper|Service|Tray|App|Client|Setup)$' -and $Item.Nom -match '[A-Za-z]{3}') { $name = $Item.Nom }
    if ($file -match '^EpicGamesLauncher') { $name = 'Epic Games Launcher' }
    # Nom, éditeur, entrée et nom du fichier (pas le dossier complet : un programme rangé
    # dans le dossier de Steam n'est pas Steam).
    $text = "$name $company $($Item.Nom) $file"
    if ($text -match $SafeStartup) {
        $adv = @{ Kind = 'safe'; Label = 'Tu peux le désactiver'; Color = $Colors.ok; Why = 'Il se lance quand tu l''ouvres, pas besoin qu''il démarre avec Windows.' }
    } elseif ($text -match $KeepStartup) {
        $adv = @{ Kind = 'keep'; Label = 'À garder'; Color = $Colors.warn; Why = 'Pilote ou protection de ton PC : laisse le activé.' }
    } elseif ($text -match $DeviceStartup) {
        $adv = @{ Kind = 'choice'; Label = 'À toi de voir'; Color = '#9AA3B2'; Why = 'Garde le si tu utilises les réglages de ta souris, ton clavier ou tes lumières.' }
    } else {
        $adv = @{ Kind = 'choice'; Label = 'À toi de voir'; Color = '#9AA3B2'; Why = 'Désactive le si tu ne t''en sers pas dès que tu allumes ton PC.' }
    }
    # Applis lancées par un petit programme de mise à jour (Discord...) : on prend l'icône de la vraie appli.
    $iconExe = $Item.Exe
    if ($Item.Commande -match '--processStart\s+"?([^"\s]+\.exe)') {
        $real = Get-ChildItem -LiteralPath (Split-Path $Item.Exe -Parent) -Filter $matches[1] -Recurse -Depth 2 -File -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if ($real) { $iconExe = $real.FullName }
    }
    @{ Item = $Item; Name = $name; Company = $company; Advice = $adv; IconExe = $iconExe }
}

function Update-StartupCount {
    $on = @($script:StartupEntries | Where-Object { $_.Item.Enabled }).Count
    $ui.StartupCount.Text = if ($on) {
        "$on programme$(if ($on -gt 1) {'s se lancent'} else {' se lance'}) quand tu allumes ton PC. Moins il y en a, plus il démarre vite et plus il reste de mémoire pour tes jeux. Rien n'est supprimé : tu peux changer d'avis quand tu veux."
    } else { 'Aucun programme ne se lance quand tu allumes ton PC.' }
    $safe = @($script:StartupEntries | Where-Object { $_.Advice.Kind -eq 'safe' -and $_.Item.Enabled }).Count
    $ui.BtnDisableStartup.Content = "Désactiver ce qui est conseillé ($safe)"
    $ui.BtnDisableStartup.Visibility = if ($safe) { 'Visible' } else { 'Collapsed' }
}

function Update-StartupList {
    $order = @{ safe = 0; choice = 1; keep = 2 }
    $script:StartupEntries = @(Get-StartupItems | ForEach-Object { Get-StartupInfo $_ } |
        Sort-Object @{ Expression = { -not $_.Item.Enabled } }, @{ Expression = { $order[$_.Advice.Kind] } }, @{ Expression = { $_.Name } })
    $panel = $ui.StartupPanel
    $panel.Children.Clear()
    foreach ($s in $script:StartupEntries) {
        $card = New-Card
        $card.Padding = New-Thickness 14 12 16 12
        $g = New-Grid @('Auto', '*', 'Auto')

        $icon = Get-ExeIcon $s.IconExe
        if ($icon) {
            $img = New-Object System.Windows.Controls.Image
            $img.Source = $icon; $img.Width = 32; $img.Height = 32
            [System.Windows.Media.RenderOptions]::SetBitmapScalingMode($img, 'HighQuality')
            $iconEl = $img
        } else {
            $iconEl = New-Object System.Windows.Controls.Border
            $iconEl.Width = 32; $iconEl.Height = 32
            $iconEl.CornerRadius = [System.Windows.CornerRadius]::new(8)
            $iconEl.Background = Get-Brush '#262C38'
        }
        $iconEl.Margin = New-Thickness 0 0 14 0
        $iconEl.VerticalAlignment = 'Center'
        Add-ToGrid $g $iconEl 0

        $sp = New-Object System.Windows.Controls.StackPanel
        $sp.VerticalAlignment = 'Center'
        $head = New-Object System.Windows.Controls.WrapPanel
        [void]$head.Children.Add((New-Text $s.Name 14.5 '#FFFFFF' -Semi))
        [void]$head.Children.Add((New-Badge $s.Advice.Label $s.Advice.Color))
        [void]$sp.Children.Add($head)
        $sub = New-Text $s.Advice.Why 12.5 '#9AA3B2'
        $sub.Margin = New-Thickness 0 3 0 0
        [void]$sp.Children.Add($sub)
        Add-ToGrid $g $sp 1

        $sw = New-Object System.Windows.Controls.CheckBox
        $sw.Style = $Window.FindResource('Switch')
        $sw.IsChecked = [bool]$s.Item.Enabled
        $sw.VerticalAlignment = 'Center'
        $sw.Margin = New-Thickness 16 0 0 0
        $sw.ToolTip = 'Activé = se lance quand tu allumes ton PC'
        $s.Card = $card
        $sw.Tag = $s
        $sw.Add_Click({ param($sender, $e) Invoke-Safe { Set-StartupToggle $sender } })
        Add-ToGrid $g $sw 2

        $card.Child = $g
        if (-not $s.Item.Enabled) { $card.Opacity = 0.6 }
        [void]$panel.Children.Add($card)
    }
    if (-not $script:StartupEntries.Count) {
        [void]$panel.Children.Add((New-Text 'Aucun programme ne se lance avec Windows.' 14 '#9AA3B2'))
    }
    Update-StartupCount
}

function Set-StartupToggle($Switch) {
    $s = $Switch.Tag
    $on = [bool]$Switch.IsChecked
    Set-StartupState $s.Item $on
    $s.Item.Enabled = $on
    $s.Card.Opacity = if ($on) { 1 } else { 0.6 }
    Update-StartupCount
    Update-BackupSummary
    Set-Status $(if ($on) { "« $($s.Name) » se lancera de nouveau quand tu allumes ton PC." } else { "« $($s.Name) » ne se lancera plus quand tu allumes ton PC." })
}

function Disable-RecommendedStartup {
    $todo = @($script:StartupEntries | Where-Object { $_.Advice.Kind -eq 'safe' -and $_.Item.Enabled })
    if (-not $todo.Count) { return }
    $names = ($todo | ForEach-Object { $_.Name }) -join ', '
    if (-not (Confirm-Action "Ces programmes ne se lanceront plus quand tu allumes ton PC :`n`n$names`n`nIls restent installés et s'ouvrent normalement quand tu cliques dessus. Continuer ?")) { return }
    Set-Busy $true
    $script:RunLog = New-Object System.Collections.ArrayList
    try { foreach ($s in $todo) { Set-StartupState $s.Item $false } }
    finally { $log = $script:RunLog; $script:RunLog = $null }
    Update-StartupList
    Update-BackupSummary
    $lines = @("$($todo.Count) programme$(if ($todo.Count -gt 1) {'s'}) ne se lancer$(if ($todo.Count -gt 1) {'ont'} else {'a'}) plus au démarrage :")
    foreach ($s in $todo) { $lines += "•  $($s.Name)" }
    Set-Status 'Programmes désactivés au démarrage.'
    Show-ResultSheet "C'est fait !" $lines $log $null
}

# ---------------------------------------------------------------------------
# Onglet réseau
# ---------------------------------------------------------------------------
function Update-NetInfo {
    $panel = $ui.NetInfoPanel
    $panel.Children.Clear()
    if (-not $script:Net) { $script:Net = Get-ActiveNet }
    [void]$panel.Children.Add((New-Text 'Ta connexion' 16 '#FFFFFF' -Semi))
    if (-not $script:Net) {
        [void]$panel.Children.Add((New-Text 'Aucune connexion Internet détectée.' 13 $Colors.bad))
        $ui.DnsCurrent.Text = ''
        return
    }
    $n = $script:Net
    $rows = [ordered]@{
        'Type'    = if ($n.Wifi) { 'Wi-Fi' } else { 'Ethernet (câble)' }
        'Carte'   = $n.Desc
        'Vitesse' = $n.Speed
        'Box'     = $n.Gateway
    }
    foreach ($k in $rows.Keys) {
        $g = New-Grid @('100', '*')
        $g.Margin = New-Thickness 0 6 0 0
        Add-ToGrid $g (New-Text $k 13 '#9AA3B2') 0
        Add-ToGrid $g (New-Text ([string]$rows[$k]) 13) 1
        [void]$panel.Children.Add($g)
    }
    try {
        $dns = [string]$n.Dns
        if (-not $dns) { throw 'DNS inconnu' }
        $ui.DnsCurrent.Text = "DNS actuel: $dns"
    } catch { $ui.DnsCurrent.Text = '' }
}

function Measure-Latency([string]$Target, [string]$Label, [int]$Count = 20) {
    $ping = New-Object System.Net.NetworkInformation.Ping
    $times = New-Object System.Collections.Generic.List[double]
    $lost = 0
    for ($i = 1; $i -le $Count; $i++) {
        Set-Status "Test de $Label... ($i sur $Count)"
        try {
            $r = $ping.Send($Target, 1000)
            if ($r.Status -eq 'Success') { $times.Add($r.RoundtripTime) } else { $lost++ }
        } catch { $lost++ }
        Start-Sleep -Milliseconds 80
    }
    $ping.Dispose()
    $avg = 0; $jitter = 0
    if ($times.Count) {
        $avg = ($times | Measure-Object -Average).Average
        if ($times.Count -gt 1) {
            $diffs = for ($i = 1; $i -lt $times.Count; $i++) { [math]::Abs($times[$i] - $times[$i - 1]) }
            $jitter = ($diffs | Measure-Object -Average).Average
        }
    }
    @{ Avg = [math]::Round($avg, 1); Jitter = [math]::Round($jitter, 1); Loss = [math]::Round(100 * $lost / $Count) }
}

function Invoke-NetTest {
    Set-Busy $true
    $script:Net = Get-ActiveNet
    Update-NetInfo
    $panel = $ui.PingPanel
    $panel.Children.Clear()
    $script:PingResults = @()
    if (-not $script:Net) { Set-Status 'Pas de connexion.'; return }

    $targets = @()
    if ($script:Net.Gateway -and $script:Net.Gateway -ne '0.0.0.0') { $targets += @{ Label = 'Ta box (réseau local)'; Host = $script:Net.Gateway; Local = $true } }
    $targets += @{ Label = 'Internet: Cloudflare'; Host = '1.1.1.1'; Local = $false }
    $targets += @{ Label = 'Internet: Google'; Host = '8.8.8.8'; Local = $false }

    foreach ($t in $targets) {
        $m = Measure-Latency $t.Host $t.Label
        if ($t.Local) {
            if ($m.Loss -gt 0 -or $m.Avg -gt 5 -or $m.Jitter -gt 3) {
                $st = 'warn'; $verdict = "Réseau local instable. C'est typique du Wi-Fi (murs, distance, voisins): rapproche toi de la box ou passe en câble."
            } else { $st = 'ok'; $verdict = 'Liaison avec ta box rapide et stable.' }
        } else {
            if ($m.Loss -ge 5)       { $st = 'bad';  $verdict = 'Pertes de paquets: tu risques de la téléportation et des tirs qui ne comptent pas.' }
            elseif ($m.Loss -gt 0)   { $st = 'warn'; $verdict = 'Quelques pertes de paquets.' }
            elseif ($m.Jitter -gt 15){ $st = 'warn'; $verdict = 'Ping instable (gigue élevée): sensations de lag irrégulières.' }
            elseif ($m.Avg -lt 30)   { $st = 'ok';   $verdict = 'Excellent pour jouer en ligne.' }
            elseif ($m.Avg -lt 60)   { $st = 'ok';   $verdict = 'Bon pour jouer en ligne.' }
            elseif ($m.Avg -lt 100)  { $st = 'warn'; $verdict = 'Moyen: jouable, mais tu seras désavantagé dans les jeux compétitifs.' }
            else                     { $st = 'bad';  $verdict = 'Ping élevé.' }
        }
        if ($m.Loss -eq 100) { $st = 'bad'; $verdict = 'Aucune réponse (ce serveur bloque peut être le ping).' }

        $card = New-Card
        $g = New-Grid @('Auto', '*', 'Auto')
        $dot = New-Object System.Windows.Shapes.Ellipse
        $dot.Width = 12; $dot.Height = 12; $dot.Fill = Get-Brush $Colors[$st]
        $dot.VerticalAlignment = 'Top'; $dot.Margin = New-Thickness 0 4 14 0
        Add-ToGrid $g $dot 0
        $sp = New-Object System.Windows.Controls.StackPanel
        [void]$sp.Children.Add((New-Text "$($t.Label) ($($t.Host))" 14 '#FFFFFF' -Semi))
        $v = New-Text $verdict 12.5 '#9AA3B2'; $v.Margin = New-Thickness 0 3 0 0
        [void]$sp.Children.Add($v)
        Add-ToGrid $g $sp 1
        $stats = New-Text "$($m.Avg) ms   gigue $($m.Jitter) ms   pertes $($m.Loss) %" 13 $Colors[$st] -Semi
        $stats.VerticalAlignment = 'Center'; $stats.Margin = New-Thickness 14 0 0 0
        Add-ToGrid $g $stats 2
        $card.Child = $g
        [void]$panel.Children.Add($card)
        $script:PingResults += [pscustomobject]@{ Label = "$($t.Label) ($($t.Host))"; Avg = $m.Avg; Jitter = $m.Jitter; Loss = $m.Loss; Status = $st; Verdict = $verdict }
    }
    Set-Status 'Test réseau terminé.'
}

function Set-Dns([int]$Choice) {
    if (-not $script:Net) { Show-Message 'Aucune connexion détectée.'; return }
    $idx = $script:Net.IfIndex
    $key = [string]$idx
    if (-not $script:Backup.Dns.ContainsKey($key)) {
        $static = Get-RegValue "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$($script:Net.Guid)" 'NameServer'
        $script:Backup.Dns[$key] = [string]$static
        Save-Backup
    }
    $servers = $DnsChoices[$Choice]
    if ($servers.Count) { Set-DnsClientServerAddress -InterfaceIndex $idx -ServerAddresses $servers -ErrorAction Stop }
    else { Set-DnsClientServerAddress -InterfaceIndex $idx -ResetServerAddresses -ErrorAction Stop }
    Clear-DnsClientCache
    $script:Net = Get-ActiveNet
    Update-NetInfo
    Update-BackupSummary
    Set-Status 'DNS modifié.'
}

# ---------------------------------------------------------------------------
# Onglet nettoyage
# ---------------------------------------------------------------------------
function Invoke-CleanScan {
    Set-Busy $true
    $panel = $ui.CleanPanel
    $panel.Children.Clear()
    $script:CleanRows = @()
    $total = 0
    foreach ($c in $CleanTargets) {
        Set-Status "Calcul: $($c.Titre)..."
        $size = [double](Invoke-Async $SizeScript $c.Paths)
        $total += $size
        $card = New-Card
        $g = New-Grid @('*', 'Auto')
        $cb = New-Object System.Windows.Controls.CheckBox
        $cb.Content = $c.Titre
        $cb.FontSize = 14
        $cb.IsChecked = $size -gt 0
        $cb.VerticalContentAlignment = 'Center'
        Add-ToGrid $g $cb 0
        Add-ToGrid $g (New-Text (Format-Size $size) 14 $(if ($size -gt 500MB) { $Colors.warn } else { '#9AA3B2' }) -Semi) 1
        $card.Child = $g
        [void]$panel.Children.Add($card)
        $script:CleanRows += @{ Target = $c; CheckBox = $cb; Size = $size }
    }
    $ui.CleanTotal.Text = "$(Format-Size $total) peuvent être libérés."
    Set-Status 'Analyse du nettoyage terminée.'
}

function Invoke-Clean {
    $sel = @($script:CleanRows | Where-Object { $_.CheckBox.IsChecked })
    if (-not $sel.Count) { Show-Message "Clique d'abord sur Analyser, puis coche ce que tu veux nettoyer."; return }
    Set-Busy $true
    $freed = 0
    foreach ($r in $sel) {
        Set-Status "Nettoyage: $($r.Target.Titre)..."
        [void](Invoke-Async $CleanScript $r.Target.Paths)
        $after = [double](Invoke-Async $SizeScript $r.Target.Paths)
        $freed += [math]::Max(0.0, $r.Size - $after)
    }
    Invoke-CleanScan
    $msg = "$(Format-Size $freed) libérés. Certains fichiers en cours d'utilisation ont pu être laissés en place, c'est normal."
    Set-Status $msg
    Show-Message $msg
}

# ---------------------------------------------------------------------------
# Onglet sauvegarde
# ---------------------------------------------------------------------------
function Get-BackupCount {
    $script:Backup.Registry.Count + $script:Backup.Dns.Count + $script:Backup.Displays.Count + $(if ($script:Backup.PowerScheme) { 1 } else { 0 }) + $(if ($script:Backup.Overlay) { 1 } else { 0 })
}

function Update-BackupSummary {
    $n = Get-BackupCount
    $ui.BackupSummary.Text = if ($n) {
        "$n réglage$(if ($n -gt 1) {'s'}) modifié$(if ($n -gt 1) {'s'}) par OptiGame. Un clic remet tout comme avant."
    } else {
        "OptiGame n'a encore rien modifié sur ce PC."
    }
}

function Invoke-UndoAll {
    if (-not (Get-BackupCount)) { Show-Message "Il n'y a aucun changement à annuler."; return }
    if (-not (Confirm-Action "Remettre tous les réglages modifiés par OptiGame comme ils étaient avant ?")) { return }
    Set-Busy $true
    Set-Status 'Restauration des réglages...'
    $errors = Restore-AllSettings
    Update-BackupSummary
    Build-GamingTab
    Update-StartupList
    Update-NetInfo
    $msg = 'Tous les réglages ont été remis comme avant. Redémarre le PC pour que tout soit pris en compte.'
    if ($errors) { $msg += "`n`nCertains éléments n'ont pas pu être restaurés:`n" + ($errors -join "`n") }
    Show-Message $msg
    Invoke-Analysis
}

function Export-Report {
    if (-not $script:LastAnalysis) { Invoke-Analysis }
    $a = $script:LastAnalysis
    $dlg = New-Object Microsoft.Win32.SaveFileDialog
    $dlg.Filter = 'Page web (*.html)|*.html'
    $dlg.FileName = "Rapport OptiGame $env:COMPUTERNAME $(Get-Date -Format 'yyyy-MM-dd').html"
    $dlg.InitialDirectory = [Environment]::GetFolderPath('Desktop')
    if ($dlg.ShowDialog($Window) -ne $true) { return }

    $enc = { param($s) [System.Net.WebUtility]::HtmlEncode([string]$s) }
    $infoRows = ($a.Info.Keys | ForEach-Object { "<tr><th>$(& $enc $_)</th><td>$(& $enc $a.Info[$_])</td></tr>" }) -join "`n"
    $findRows = ($a.Findings | ForEach-Object {
        "<div class='f'><span class='dot' style='background:$($Colors[$_.Status])'></span><div><b>$(& $enc $_.Titre)</b><p>$(& $enc $_.Detail)</p></div></div>"
    }) -join "`n"
    $compHtml = ($a.Cards | ForEach-Object {
        $c = $_
        $rows = @()
        foreach ($b in $c.Bars) { $rows += "<tr><th>$(& $enc $b.Label)</th><td>$(& $enc $b.Text) ($([int]$b.Value) % utilisé)</td></tr>" }
        foreach ($k in $c.Lines.Keys) {
            $v = $c.Lines[$k]
            $t = if ($v -is [array]) { $v[0] } else { $v }
            $rows += "<tr><th>$(& $enc $k)</th><td>$(& $enc $t)</td></tr>"
        }
        foreach ($n in $c.Notes) { $rows += "<tr><td colspan='2' style='color:$($Colors[$n.Status])'>$(& $enc $n.Text)</td></tr>" }
        "<div class='c'><div class='ch'><b>$(& $enc $c.Titre)</b><span style='color:$($Colors[$c.Status])'>$(& $enc $StatusLabels[$c.Status])</span></div><div class='sub cs'>$(& $enc $c.Sous)</div><table>$($rows -join '')</table></div>"
    }) -join "`n"
    $pingHtml = ''
    if ($script:PingResults) {
        $pingRows = ($script:PingResults | ForEach-Object {
            "<tr><th>$(& $enc $_.Label)</th><td style='color:$($Colors[$_.Status])'>$($_.Avg) ms, gigue $($_.Jitter) ms, pertes $($_.Loss) %</td></tr>"
        }) -join "`n"
        $pingHtml = "<h2>Réseau</h2><table>$pingRows</table>"
    }

    $html = @"
<!DOCTYPE html>
<html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Rapport OptiGame</title>
<style>
body{margin:0;background:#0E1014;color:#E6E8EE;font:15px/1.5 'Segoe UI',system-ui,sans-serif}
main{max-width:860px;margin:0 auto;padding:32px 16px}
h1{margin:0;font-size:28px}h1 span{color:#22D37A}
h2{margin:32px 0 12px;font-size:18px}
.sub{color:#9AA3B2}
.score{display:flex;align-items:center;gap:20px;background:#181C24;border:1px solid #232937;border-radius:12px;padding:20px;margin-top:24px}
.score b{font-size:48px;color:$($a.Color)}
table{width:100%;border-collapse:collapse;background:#181C24;border:1px solid #232937;border-radius:12px;overflow:hidden}
th,td{text-align:left;padding:10px 14px;border-bottom:1px solid #232937;vertical-align:top}
th{color:#9AA3B2;font-weight:normal;width:180px}
.f{display:flex;gap:14px;background:#181C24;border:1px solid #232937;border-radius:12px;padding:12px 16px;margin-bottom:8px}
.f p{margin:2px 0 0;color:#9AA3B2;font-size:14px}
.dot{flex:none;width:12px;height:12px;border-radius:50%;margin-top:6px}
.c{margin-bottom:14px}
.ch{display:flex;justify-content:space-between;gap:12px;font-size:16px}
.cs{margin:0 0 8px;font-size:13px}
</style></head><body><main>
<h1>Opti<span>Game</span></h1>
<div class="sub">Rapport de $(& $enc $env:COMPUTERNAME), le $($a.Date.ToString('dd/MM/yyyy à HH:mm'))</div>
<div class="score"><b>$($a.Score)</b><div><div style="font-size:20px;font-weight:600">$(& $enc $a.Label)</div><div class="sub">Score d'optimisation gaming sur 100</div></div></div>
<h2>Configuration</h2><table>$infoRows</table>
<h2>Santé des composants</h2>
$compHtml
$pingHtml
<h2>Détail de l'analyse</h2>
$findRows
</main></body></html>
"@
    Set-Content -Path $dlg.FileName -Value $html -Encoding UTF8
    Start-Process $dlg.FileName
    Set-Status 'Rapport exporté.'
}

# ---------------------------------------------------------------------------
# Onglet Tests
# ---------------------------------------------------------------------------
$script:TestButtons = New-Object System.Collections.ArrayList
$script:TestRunning = $false

# Travail lancé dans un fil séparé: seules les fonctions de [OGNative] y sont disponibles.
$DiskWork = {
    param($a)
    try { $r = [OGNative]::DiskTest($a.File, [long]$a.Size); if ($null -eq $r) { @{ Cancelled = $true } } else { @{ R = $r } } }
    catch { @{ Error = $_.Exception.GetBaseException().Message } }
}
$CpuWork = {
    param($a)
    try { $r = [OGNative]::CpuTest([double]$a.Single, [double]$a.Multi); if ($null -eq $r) { @{ Cancelled = $true } } else { @{ R = $r } } }
    catch { @{ Error = $_.Exception.GetBaseException().Message } }
}
$MemWork = {
    param($a)
    try { $r = [OGNative]::MemTest([long]$a.Bytes); if ($null -eq $r) { @{ Cancelled = $true } } else { @{ R = $r } } }
    catch { @{ Error = $_.Exception.GetBaseException().Message } }
}
$NetWork = {
    param($a)
    try {
        [OGNative]::Phase = 'ping'
        $ping = New-Object System.Net.NetworkInformation.Ping
        $times = @()
        for ($i = 0; $i -lt 10; $i++) {
            if ([OGNative]::Cancel) { return @{ Cancelled = $true } }
            try { $p = $ping.Send('1.1.1.1', 1000); if ($p.Status -eq 'Success') { $times += $p.RoundtripTime; [OGNative]::LiveValue = $p.RoundtripTime } } catch {}
            [OGNative]::Progress = $i + 1
            Start-Sleep -Milliseconds 100
        }
        [OGNative]::Phase = 'down'
        $servers = [string[]]@('https://speed.cloudflare.com/__down?bytes=25000000', 'https://proof.ovh.net/files/1Gb.dat', 'https://nbg1-speed.hetzner.com/1GB.bin', 'https://fsn1-speed.hetzner.com/1GB.bin')
        $down = [OGNative]::NetSpeed($servers, $false, 8, 4, 10, 55)
        if ([OGNative]::Cancel) { return @{ Cancelled = $true } }
        [OGNative]::Phase = 'up'
        $up = [OGNative]::NetSpeed([string[]]@('https://speed.cloudflare.com/__up'), $true, 8, 4, 55, 100)
        if ([OGNative]::Cancel) { return @{ Cancelled = $true } }
        $avg = if ($times.Count) { ($times | Measure-Object -Average).Average } else { -1 }
        @{ R = @($avg, $down, $up, (10 - $times.Count)) }
    } catch { @{ Error = $_.Exception.GetBaseException().Message } }
}
$RepairWork = {
    param($a)
    $out = @()
    [OGNative]::Phase = 'scan'
    $i = 0
    foreach ($l in $a.Letters) {
        try { $out += "$l|$(Repair-Volume -DriveLetter $l -Scan -ErrorAction Stop)" } catch { $out += "$l|ERR $($_.Exception.Message)" }
        $i++
        [OGNative]::Progress = 100 * $i / @($a.Letters).Count
    }
    @{ R = $out }
}

# ---------------------------------------------------------------------------
# Animations
# ---------------------------------------------------------------------------
$script:Anims = New-Object System.Collections.ArrayList
$script:AnimTimer = New-Object System.Windows.Threading.DispatcherTimer
$script:AnimTimer.Interval = [TimeSpan]::FromMilliseconds(20)
$script:AnimTimer.Add_Tick({
    $now = [DateTime]::Now
    foreach ($a in @($script:Anims)) {
        $el = ($now - $a.Start).TotalMilliseconds - $a.Delay
        if ($el -lt 0) { continue }
        $p = [math]::Min(1.0, $el / $a.Ms)
        $ease = 1 - [math]::Pow(1 - $p, 3)
        try { & $a.Step $ease $a.State } catch {}
        if ($p -ge 1) { $script:Anims.Remove($a) }
    }
    if (-not $script:Anims.Count) { $script:AnimTimer.Stop() }
})

# Anime une valeur de 0 à 1 (départ rapide, fin douce) en appelant $Step à chaque image.
function Start-Anim([scriptblock]$Step, $State, [int]$Ms = 1100, [int]$Delay = 0) {
    [void]$script:Anims.Add(@{ Step = $Step; State = $State; Ms = $Ms; Delay = $Delay; Start = [DateTime]::Now })
    if (-not $script:AnimTimer.IsEnabled) { $script:AnimTimer.Start() }
}

function Start-WpfAnim($Element, $Property, [double]$To, [int]$Ms = 900, [int]$Delay = 0) {
    $a = New-Object System.Windows.Media.Animation.DoubleAnimation
    $a.To = $To
    $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds($Ms))
    $a.BeginTime = [TimeSpan]::FromMilliseconds($Delay)
    $ease = New-Object System.Windows.Media.Animation.CubicEase
    $ease.EasingMode = 'EaseOut'
    $a.EasingFunction = $ease
    $Element.BeginAnimation($Property, $a)
}

function Start-Pulse($Element) {
    if (-not $Element.CacheMode) { $Element.CacheMode = New-Object System.Windows.Media.BitmapCache }
    $a = New-Object System.Windows.Media.Animation.DoubleAnimation
    $a.From = 1; $a.To = 0.25
    $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds(700))
    $a.AutoReverse = $true
    $a.RepeatBehavior = [System.Windows.Media.Animation.RepeatBehavior]::Forever
    $Element.BeginAnimation([System.Windows.UIElement]::OpacityProperty, $a)
}

function Stop-Pulse($Element) {
    $Element.BeginAnimation([System.Windows.UIElement]::OpacityProperty, $null)
    $Element.Opacity = 1
}

function New-Glow([string]$Hex, [double]$Blur = 16, [double]$Opacity = 0.55) {
    $fx = New-Object System.Windows.Media.Effects.DropShadowEffect
    $fx.Color = [System.Windows.Media.ColorConverter]::ConvertFromString($Hex)
    $fx.BlurRadius = $Blur; $fx.ShadowDepth = 0; $fx.Opacity = $Opacity
    $fx
}

# ---------------------------------------------------------------------------
# Jauges circulaires, courbes en direct, barres de comparaison
# ---------------------------------------------------------------------------
function Get-ArcGeometry([double]$C, [double]$R, [double]$Start, [double]$Sweep) {
    if ($Sweep -le 0.05) { return $null }
    $a1 = $Start * [math]::PI / 180
    $a2 = ($Start + $Sweep) * [math]::PI / 180
    $p1 = [System.Windows.Point]::new($C + $R * [math]::Cos($a1), $C + $R * [math]::Sin($a1))
    $p2 = [System.Windows.Point]::new($C + $R * [math]::Cos($a2), $C + $R * [math]::Sin($a2))
    $seg = [System.Windows.Media.ArcSegment]::new($p2, [System.Windows.Size]::new($R, $R), 0.0, ($Sweep -gt 180), [System.Windows.Media.SweepDirection]::Clockwise, $true)
    $fig = New-Object System.Windows.Media.PathFigure
    $fig.StartPoint = $p1
    [void]$fig.Segments.Add($seg)
    $geo = New-Object System.Windows.Media.PathGeometry
    [void]$geo.Figures.Add($fig)
    $geo
}

function New-Gauge([string]$Label, [double]$Value, [double]$Max, [string]$Fmt, [string]$Unit, [string]$Color, [int]$Delay = 0) {
    $c = 72; $r = 60
    $root = New-Object System.Windows.Controls.StackPanel
    $root.Width = 150
    $root.Margin = New-Thickness 6 0 6 10
    $g = New-Object System.Windows.Controls.Grid
    $g.Width = 144; $g.Height = 144
    foreach ($spec in @(@{ Hex = '#232937'; Sweep = 270 }, @{ Hex = $Color; Sweep = 0 })) {
        $path = New-Object System.Windows.Shapes.Path
        $path.Stroke = Get-Brush $spec.Hex
        $path.StrokeThickness = 11
        $path.StrokeStartLineCap = 'Round'; $path.StrokeEndLineCap = 'Round'
        $path.Data = Get-ArcGeometry $c $r 135 $spec.Sweep
        [void]$g.Children.Add($path)
        $arc = $path
    }
    $arc.Effect = New-Glow $Color 18 0.6
    $center = New-Object System.Windows.Controls.StackPanel
    $center.VerticalAlignment = 'Center'; $center.HorizontalAlignment = 'Center'
    $num = New-Text '0' 26 '#FFFFFF' -Bold
    $num.HorizontalAlignment = 'Center'; $num.TextWrapping = 'NoWrap'
    $u = New-Text $Unit 11.5 '#9AA3B2'
    $u.HorizontalAlignment = 'Center'
    [void]$center.Children.Add($num)
    [void]$center.Children.Add($u)
    [void]$g.Children.Add($center)
    [void]$root.Children.Add($g)
    $lbl = New-Text $Label 13 '#C9CED8' -Semi
    $lbl.HorizontalAlignment = 'Center'; $lbl.TextAlignment = 'Center'
    $lbl.Margin = New-Thickness 0 -8 0 0
    [void]$root.Children.Add($lbl)
    $state = @{ Arc = $arc; Num = $num; From = 0.0; To = $Value; Cur = 0.0; Max = [math]::Max(1e-6, $Max); Fmt = $Fmt; C = $c; R = $r }
    Start-Anim { param($e, $s) $v = $s.From + ($s.To - $s.From) * $e; $s.Cur = $v; $f = [math]::Min(1.0, [math]::Max(0.0, $v / $s.Max)); $s.Arc.Data = Get-ArcGeometry $s.C $s.R 135 (270 * $f); $s.Num.Text = $s.Fmt -f $v } $state 1300 $Delay
    @{ El = $root; State = $state }
}

# Fait glisser une jauge vers une nouvelle valeur (mode « en direct »).
function Set-GaugeLive($Gauge, [double]$Value) {
    $s = $Gauge.State
    $s.From = $s.Cur; $s.To = $Value
    Start-Anim { param($e, $st) $v = $st.From + ($st.To - $st.From) * $e; $st.Cur = $v; $f = [math]::Min(1.0, [math]::Max(0.0, $v / $st.Max)); $st.Arc.Data = Get-ArcGeometry $st.C $st.R 135 (270 * $f); $st.Num.Text = $st.Fmt -f $v } $s 700
}

function New-GaugeRow([array]$Gauges) {
    $wp = New-Object System.Windows.Controls.WrapPanel
    $wp.HorizontalAlignment = 'Center'
    $wp.Margin = New-Thickness 0 6 0 4
    foreach ($g in $Gauges) { [void]$wp.Children.Add($g.El) }
    $wp
}

function New-LiveChart([string]$Color, [string]$Unit, [string]$Fmt = '{0:N0}') {
    $w = 660; $h = 150
    $cv = New-Object System.Windows.Controls.Canvas
    $cv.Width = $w; $cv.Height = $h; $cv.ClipToBounds = $true
    foreach ($y in 0.25, 0.5, 0.75) {
        $ln = New-Object System.Windows.Shapes.Line
        $ln.X1 = 0; $ln.X2 = $w; $ln.Y1 = $h * $y; $ln.Y2 = $h * $y
        $ln.Stroke = Get-Brush '#1C212B'; $ln.StrokeThickness = 1
        [void]$cv.Children.Add($ln)
    }
    $col = [System.Windows.Media.ColorConverter]::ConvertFromString($Color)
    $grad = New-Object System.Windows.Media.LinearGradientBrush
    $grad.StartPoint = [System.Windows.Point]::new(0, 0); $grad.EndPoint = [System.Windows.Point]::new(0, 1)
    $top = [System.Windows.Media.Color]::FromArgb(110, $col.R, $col.G, $col.B)
    $bottom = [System.Windows.Media.Color]::FromArgb(0, $col.R, $col.G, $col.B)
    [void]$grad.GradientStops.Add([System.Windows.Media.GradientStop]::new($top, 0))
    [void]$grad.GradientStops.Add([System.Windows.Media.GradientStop]::new($bottom, 1))
    $fill = New-Object System.Windows.Shapes.Polygon
    $fill.Fill = $grad
    [void]$cv.Children.Add($fill)
    $ref = New-Object System.Windows.Shapes.Line
    $ref.X1 = 0; $ref.X2 = $w; $ref.Stroke = Get-Brush '#F5A524'; $ref.StrokeThickness = 1.2
    $ref.StrokeDashArray = [System.Windows.Media.DoubleCollection]::new([double[]]@(4, 4))
    $ref.Visibility = 'Collapsed'
    [void]$cv.Children.Add($ref)
    $refText = New-Text '' 11 '#F5A524'
    $refText.Visibility = 'Collapsed'
    [void]$cv.Children.Add($refText)
    $line = New-Object System.Windows.Shapes.Polyline
    $line.Stroke = Get-Brush $Color; $line.StrokeThickness = 2.5
    $line.StrokeLineJoin = 'Round'
    $line.Effect = New-Glow $Color 10 0.7
    [void]$cv.Children.Add($line)
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 11; $dot.Height = 11; $dot.Fill = Get-Brush '#FFFFFF'
    $dot.Effect = New-Glow $Color 14 0.9
    $dot.Visibility = 'Hidden'
    [void]$cv.Children.Add($dot)
    $maxText = New-Text '' 11 '#5B6475'
    [System.Windows.Controls.Canvas]::SetLeft($maxText, 4); [System.Windows.Controls.Canvas]::SetTop($maxText, 2)
    [void]$cv.Children.Add($maxText)
    $border = New-Object System.Windows.Controls.Border
    $border.Background = Get-Brush '#10131A'
    $border.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $border.Padding = New-Thickness 12 10 12 10
    $border.Margin = New-Thickness 0 10 0 6
    $border.Child = $cv
    @{ El = $border; Line = $line; Fill = $fill; Dot = $dot; MaxText = $maxText; Ref = $ref; RefText = $refText; RefValue = $null
       Values = New-Object System.Collections.ArrayList; W = $w; H = $h; Unit = $Unit; Fmt = $Fmt }
}

function Add-ChartPoint($Chart, [double]$Value) {
    [void]$Chart.Values.Add($Value)
    if ($Chart.Values.Count -gt 160) { $Chart.Values.RemoveAt(0) }
    Update-Chart $Chart
}

function Update-Chart($Chart) {
    $vals = $Chart.Values
    $n = $vals.Count
    if ($n -lt 2) { return }
    $max = [double]($vals | Measure-Object -Maximum).Maximum
    if ($Chart.RefValue) { $max = [math]::Max($max, $Chart.RefValue) }
    if ($max -le 0) { $max = 1 }
    $max *= 1.15
    $w = $Chart.W; $h = $Chart.H
    $step = $w / 159
    $pts = New-Object System.Windows.Media.PointCollection
    for ($i = 0; $i -lt $n; $i++) { [void]$pts.Add([System.Windows.Point]::new($i * $step, $h - [double]$vals[$i] / $max * ($h - 8))) }
    $Chart.Line.Points = $pts
    $fp = New-Object System.Windows.Media.PointCollection
    foreach ($pt in $pts) { [void]$fp.Add($pt) }
    [void]$fp.Add([System.Windows.Point]::new(($n - 1) * $step, $h))
    [void]$fp.Add([System.Windows.Point]::new(0, $h))
    $Chart.Fill.Points = $fp
    $last = $pts[$n - 1]
    [System.Windows.Controls.Canvas]::SetLeft($Chart.Dot, $last.X - 5.5)
    [System.Windows.Controls.Canvas]::SetTop($Chart.Dot, $last.Y - 5.5)
    $Chart.Dot.Visibility = 'Visible'
    $Chart.MaxText.Text = "max $($Chart.Fmt -f ($max / 1.15)) $($Chart.Unit)"
    if ($Chart.RefValue) {
        $y = $h - $Chart.RefValue / $max * ($h - 8)
        $Chart.Ref.Y1 = $y; $Chart.Ref.Y2 = $y; $Chart.Ref.Visibility = 'Visible'
        [System.Windows.Controls.Canvas]::SetLeft($Chart.RefText, $w - 150)
        [System.Windows.Controls.Canvas]::SetTop($Chart.RefText, $y - 16)
        $Chart.RefText.Visibility = 'Visible'
    }
}

# Barres horizontales qui se remplissent: ton composant comparé à des références.
function New-CompareBars([array]$Rows, [string]$Unit) {
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Margin = New-Thickness 0 8 0 0
    $max = [double](($Rows | ForEach-Object { $_.Value }) | Measure-Object -Maximum).Maximum
    $barMax = 430.0
    $i = 0
    foreach ($row in $Rows) {
        $g = New-Grid @('150', '440', '*')
        $g.Margin = New-Thickness 0 5 0 5
        $lbl = New-Text $row.Label 13 $(if ($row.Mine) { '#FFFFFF' } else { '#9AA3B2' })
        if ($row.Mine) { $lbl.FontWeight = [System.Windows.FontWeights]::SemiBold }
        $lbl.VerticalAlignment = 'Center'
        Add-ToGrid $g $lbl 0
        $track = New-Object System.Windows.Controls.Border
        $track.Height = 12; $track.CornerRadius = [System.Windows.CornerRadius]::new(6)
        $track.Background = Get-Brush '#1D222C'
        $track.Width = $barMax; $track.HorizontalAlignment = 'Left'; $track.VerticalAlignment = 'Center'
        $bar = New-Object System.Windows.Controls.Border
        $bar.Height = 12; $bar.CornerRadius = [System.Windows.CornerRadius]::new(6)
        $bar.HorizontalAlignment = 'Left'; $bar.Width = 0
        $bar.Background = Get-Brush $(if ($row.Mine) { $row.Color } else { '#3A4252' })
        if ($row.Mine) { $bar.Effect = New-Glow $row.Color 12 0.6 }
        $track.Child = $bar
        Add-ToGrid $g $track 1
        $val = New-Text '' 13 $(if ($row.Mine) { '#FFFFFF' } else { '#9AA3B2' }) -Semi
        $val.VerticalAlignment = 'Center'; $val.Margin = New-Thickness 12 0 0 0
        Add-ToGrid $g $val 2
        [void]$sp.Children.Add($g)
        $target = [math]::Max(4.0, $barMax * $row.Value / [math]::Max(1.0, $max))
        Start-WpfAnim $bar ([System.Windows.FrameworkElement]::WidthProperty) $target 1000 (150 * $i)
        Start-Anim { param($e, $s) $s.T.Text = ('{0:N0} ' -f ($s.V * $e)) + $s.U } @{ T = $val; V = [double]$row.Value; U = $Unit } 1000 (150 * $i)
        $i++
    }
    $sp
}

# Grande tuile chiffrée (score, nombre d'erreurs...) qui compte jusqu'à sa valeur.
function New-StatTile([string]$Label, [double]$Value, [string]$Fmt, [string]$Color = '#FFFFFF', [int]$Delay = 0) {
    $b = New-Object System.Windows.Controls.Border
    $b.Background = Get-Brush '#1A1F29'
    $b.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $b.Padding = New-Thickness 18 12 18 12
    $b.Margin = New-Thickness 0 0 10 10
    $b.MinWidth = 150
    $sp = New-Object System.Windows.Controls.StackPanel
    $num = New-Text '0' 28 $Color -Bold
    $num.TextWrapping = 'NoWrap'
    [void]$sp.Children.Add($num)
    [void]$sp.Children.Add((New-Text $Label 12.5 '#9AA3B2'))
    $b.Child = $sp
    Start-Anim { param($e, $s) $s.T.Text = $s.F -f ($s.V * $e) } @{ T = $num; V = $Value; F = $Fmt } 1200 $Delay
    $b
}

function New-StatRow([array]$Tiles) {
    $wp = New-Object System.Windows.Controls.WrapPanel
    $wp.Margin = New-Thickness 0 10 0 0
    foreach ($t2 in $Tiles) { [void]$wp.Children.Add($t2) }
    $wp
}

function New-Verdict([string]$Status, [string]$Text) {
    $b = New-Object System.Windows.Controls.Border
    $bg = Get-Brush $Colors[$Status]; $bg.Opacity = 0.12
    $b.Background = $bg
    $b.BorderBrush = Get-Brush $Colors[$Status]; $b.BorderThickness = New-Thickness 0 0 0 0
    $b.CornerRadius = [System.Windows.CornerRadius]::new(10)
    $b.Padding = New-Thickness 14 11 14 11
    $b.Margin = New-Thickness 0 14 0 0
    $g = New-Grid @('Auto', '*')
    $icon = New-Text $(switch ($Status) { 'ok' { '✓' } 'bad' { '!' } 'warn' { '!' } default { 'i' } }) 15 $Colors[$Status] -Bold
    $icon.Margin = New-Thickness 0 0 12 0
    Add-ToGrid $g $icon 0
    Add-ToGrid $g (New-Text $Text 13.5 $Colors[$Status] -Semi) 1
    $b.Child = $g
    $b.Opacity = 0
    Start-WpfAnim $b ([System.Windows.UIElement]::OpacityProperty) 1 600 700
    $b
}

function New-SectionTitle([string]$Text) {
    $title = New-Text $Text 12 '#5B6475' -Semi
    $title.Margin = New-Thickness 0 16 0 2
    $title
}

# Détails repliables: « Voir toutes les infos ».
function New-Details([array]$Rows) {
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Margin = New-Thickness 0 12 0 0
    $btn = New-Button 'Voir toutes les infos  ▾'
    $btn.HorizontalAlignment = 'Left'
    $box = New-Object System.Windows.Controls.StackPanel
    $box.Visibility = 'Collapsed'
    $box.Margin = New-Thickness 4 10 0 0
    foreach ($r in $Rows) {
        $g = New-Grid @('240', '*')
        $g.Margin = New-Thickness 0 3 0 3
        Add-ToGrid $g (New-Text $r[0] 12.5 '#9AA3B2') 0
        $col = if ($r.Count -gt 2) { $r[2] } else { '#E6E8EE' }
        Add-ToGrid $g (New-Text ([string]$r[1]) 12.5 $col) 1
        [void]$box.Children.Add($g)
    }
    $btn.Tag = $box
    $btn.Add_Click({
        param($s, $e)
        $open = $s.Tag.Visibility -ne 'Visible'
        $s.Tag.Visibility = if ($open) { 'Visible' } else { 'Collapsed' }
        $s.Content = if ($open) { 'Masquer les infos  ▴' } else { 'Voir toutes les infos  ▾' }
    })
    [void]$sp.Children.Add($btn)
    [void]$sp.Children.Add($box)
    $sp
}

# Étapes du test: ✓ faites, en cours (clignote), à venir.
function New-Stepper($Steps) {
    $wp = New-Object System.Windows.Controls.WrapPanel
    $wp.Margin = New-Thickness 0 2 0 8
    $chips = @{}
    foreach ($k in $Steps.Keys) {
        $b = New-Object System.Windows.Controls.Border
        $b.CornerRadius = [System.Windows.CornerRadius]::new(14)
        $b.Padding = New-Thickness 12 5 12 5
        $b.Margin = New-Thickness 0 0 8 6
        $b.Background = Get-Brush '#1A1F29'
        $txt = New-Text "○  $($Steps[$k])" 12.5 '#5B6475' -Semi
        $txt.TextWrapping = 'NoWrap'
        $b.Child = $txt
        [void]$wp.Children.Add($b)
        $chips[$k] = @{ B = $b; T = $txt; Label = $Steps[$k] }
    }
    @{ El = $wp; Chips = $chips; Keys = @($Steps.Keys); Cur = $null }
}

function Update-Stepper($Stepper, [string]$Phase, [switch]$AllDone) {
    if (-not $AllDone -and (-not $Phase -or $Stepper.Cur -eq $Phase -or -not $Stepper.Chips.ContainsKey($Phase))) { return }
    $idx = if ($AllDone) { $Stepper.Keys.Count } else { [array]::IndexOf($Stepper.Keys, $Phase) }
    for ($i = 0; $i -lt $Stepper.Keys.Count; $i++) {
        $ch = $Stepper.Chips[$Stepper.Keys[$i]]
        Stop-Pulse $ch.B
        if ($i -lt $idx) {
            $bg = Get-Brush $Colors.ok; $bg.Opacity = 0.15
            $ch.B.Background = $bg; $ch.T.Text = "✓  $($ch.Label)"; $ch.T.Foreground = Get-Brush $Colors.ok
        } elseif ($i -eq $idx) {
            $bg = Get-Brush $Colors.info; $bg.Opacity = 0.2
            $ch.B.Background = $bg; $ch.T.Text = "●  $($ch.Label)"; $ch.T.Foreground = Get-Brush '#FFFFFF'
            Start-Pulse $ch.B
        } else {
            $ch.B.Background = Get-Brush '#1A1F29'; $ch.T.Text = "○  $($ch.Label)"; $ch.T.Foreground = Get-Brush '#5B6475'
        }
    }
    $Stepper.Cur = $Phase
}

# ---------------------------------------------------------------------------
# Tuiles de l'onglet et panneau de test
# ---------------------------------------------------------------------------
function New-TestTile([string]$Tag, [string]$Title, [string]$Sub, [string]$Desc) {
    $card = New-Card
    $card.Padding = New-Thickness 18 16 18 16
    $card.Margin = New-Thickness 0 0 12 12
    $sp = New-Object System.Windows.Controls.StackPanel
    $head = New-Grid @('Auto', '*', 'Auto')
    $tagB = New-Object System.Windows.Controls.Border
    $tagB.Width = 44; $tagB.Height = 44
    $tagB.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $bg = Get-Brush $Colors.info; $bg.Opacity = 0.14
    $tagB.Background = $bg
    $tt = New-Text $Tag 12 $Colors.info -Bold
    $tt.TextWrapping = 'NoWrap'; $tt.HorizontalAlignment = 'Center'; $tt.VerticalAlignment = 'Center'
    $tagB.Child = $tt
    Add-ToGrid $head $tagB 0
    $ts = New-Object System.Windows.Controls.StackPanel
    $ts.Margin = New-Thickness 12 0 8 0
    $ts.VerticalAlignment = 'Center'
    $titleText = New-Text $Title 15 '#FFFFFF' -Semi
    $titleText.TextTrimming = 'CharacterEllipsis'; $titleText.TextWrapping = 'NoWrap'
    [void]$ts.Children.Add($titleText)
    if ($Sub) { [void]$ts.Children.Add((New-Text $Sub 12 '#9AA3B2')) }
    Add-ToGrid $head $ts 1
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 10; $dot.Height = 10; $dot.Fill = Get-Brush '#343C4C'
    $dot.VerticalAlignment = 'Top'; $dot.Margin = New-Thickness 0 6 0 0
    $dot.ToolTip = 'Pas encore testé'
    Add-ToGrid $head $dot 2
    [void]$sp.Children.Add($head)
    $d = New-Text $Desc 12.5 '#9AA3B2'
    $d.Margin = New-Thickness 0 10 0 0
    [void]$sp.Children.Add($d)
    $summary = New-Object System.Windows.Controls.WrapPanel
    $summary.Margin = New-Thickness 0 12 0 0
    $none = New-Text 'Pas encore testé' 12.5 '#5B6475'
    [void]$summary.Children.Add($none)
    [void]$sp.Children.Add($summary)
    $btns = New-Object System.Windows.Controls.WrapPanel
    $btns.Margin = New-Thickness 0 12 0 0
    [void]$sp.Children.Add($btns)
    $card.Child = $sp
    [void]$ui.TestsPanel.Children.Add($card)
    $tile = @{ Card = $card; Buttons = $btns; Summary = $summary; Dot = $dot; Tag = $Tag; Title = $Title; Sub = $Sub; Last = $null; View = $null }
    $view = New-Button 'Voir le résultat'
    $view.Margin = New-Thickness 0 0 8 6
    $view.Visibility = 'Collapsed'
    $view.Tag = $tile
    $view.Add_Click({ param($s, $e) Invoke-Safe { Show-LastResult $s.Tag } })
    $tile.View = $view
    $tile
}

function Add-TestButton($Tile, [string]$Text, [scriptblock]$OnClick, $Context, [switch]$Primary) {
    $b = New-Button $Text $(if ($Primary) { 'BtnPrimary' } else { 'BtnSecondary' })
    $b.Margin = New-Thickness 0 0 8 6
    $b.Tag = @{ T = $Tile; Ctx = $Context }
    $b.Add_Click($OnClick)
    [void]$Tile.Buttons.Children.Add($b)
    [void]$script:TestButtons.Add($b)
}

function Set-TileSummary($Tile, [array]$Chips, [string]$Status) {
    $Tile.Summary.Children.Clear()
    foreach ($c2 in $Chips) {
        $b = New-Object System.Windows.Controls.Border
        $b.Background = Get-Brush '#1A1F29'
        $b.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $b.Padding = New-Thickness 10 5 10 6
        $b.Margin = New-Thickness 0 0 6 6
        $sp = New-Object System.Windows.Controls.StackPanel
        [void]$sp.Children.Add((New-Text $c2[1] 14 '#FFFFFF' -Bold))
        [void]$sp.Children.Add((New-Text $c2[0] 11 '#9AA3B2'))
        $b.Child = $sp
        [void]$Tile.Summary.Children.Add($b)
    }
    $Tile.Dot.Fill = Get-Brush $Colors[$Status]
    $Tile.Dot.ToolTip = "Testé à $((Get-Date).ToString('HH:mm'))"
    if (-not $Tile.Buttons.Children.Contains($Tile.View)) { [void]$Tile.Buttons.Children.Add($Tile.View) }
    $Tile.View.Visibility = 'Visible'
}

function Set-TestState([string]$State, [string]$Text) {
    $map = @{ run = $Colors.info; live = $Colors.ok; ok = $Colors.ok; warn = $Colors.warn; bad = $Colors.bad; info = '#9AA3B2' }
    $ui.TestStateDot.Fill = Get-Brush $map[$State]
    $ui.TestStateText.Text = $Text
    $ui.TestStateText.Foreground = Get-Brush $map[$State]
    if ($State -in 'run', 'live') { Start-Pulse $ui.TestStateDot } else { Stop-Pulse $ui.TestStateDot }
}

function Show-TestPanel($Tile) {
    $ui.TestTag.Text = $Tile.Tag
    $ui.TestTitle.Text = $Tile.Title
    $ui.TestSub.Text = $Tile.Sub
    $ui.TestBody.Children.Clear()
    $ui.TestProgress.Value = 0
    $ui.TestPct.Text = ''
    $ui.TestOverlay.Visibility = 'Visible'
    $ui.TestCard.Opacity = 0
    Start-WpfAnim $ui.TestCard ([System.Windows.UIElement]::OpacityProperty) 1 250
}

function Hide-TestPanel {
    if ($script:TestRunning) { return }
    $script:DevPing = $null
    if ($script:MonitorTimer) { $script:MonitorTimer.Stop(); $script:MonitorTimer = $null }
    $Live.Fast = $false
    $ui.TestOverlay.Visibility = 'Collapsed'
}

function Set-TestButtons([string]$Mode) {
    $ui.BtnTestStop.Visibility = if ($Mode -eq 'run') { 'Visible' } else { 'Collapsed' }
    $ui.BtnTestAgain.Visibility = if ($Mode -eq 'done') { 'Visible' } else { 'Collapsed' }
    $ui.BtnTestClose.IsEnabled = $Mode -ne 'run'
    $ui.BtnTestX.IsEnabled = $Mode -ne 'run'
}

$script:TestTimer = New-Object System.Windows.Threading.DispatcherTimer
$script:TestTimer.Interval = [TimeSpan]::FromMilliseconds(100)
$script:TestTimer.Add_Tick({
    $cur = $script:CurTest
    if (-not $cur) { return }
    $prog = [math]::Min(100.0, [OGNative]::Progress)
    $ui.TestProgress.Value = $prog
    $ui.TestPct.Text = '{0:N0} %' -f $prog
    $ph = [OGNative]::Phase
    Update-Stepper $cur.Stepper $ph
    if ($ph -and $Live.CpuPerf) {
        if (-not $cur.Freq.ContainsKey($ph)) { $cur.Freq[$ph] = New-Object System.Collections.ArrayList }
        [void]$cur.Freq[$ph].Add([double]$Live.CpuPerf)
    }
    if ($cur.Chart -and $ph -and $cur.Def.Chart.Phases -contains $ph) {
        if ($cur.Def.Chart.Source -eq 'cpu') {
            if ($Live.Updated -eq $cur.LastSample) { return }
            $cur.LastSample = $Live.Updated
            $v = if ($Live.CpuPerf -and $Live.BaseMHz) { $Live.BaseMHz * $Live.CpuPerf / 100 / 1000 } else { 0 }
        } else { $v = [OGNative]::LiveValue }
        Add-ChartPoint $cur.Chart $v
        $cur.LiveVal.Text = $cur.Def.Chart.Fmt -f $v
        $cur.LiveLabel.Text = $cur.Def.Steps[$ph]
    }
})

# Lance un test: panneau animé pendant le test, puis résultat en jauges.
function Invoke-ComponentTest($Tile, $Def, $Ctx, [string]$Rerun) {
    if ($script:TestRunning) { Show-Message 'Un test est déjà en cours : attends qu''il se termine.'; return }
    $script:TestRunning = $true
    $script:LastRun = @{ Fn = $Rerun; Tile = $Tile; Ctx = $Ctx }
    foreach ($b in $script:TestButtons) { $b.IsEnabled = $false }
    Show-TestPanel $Tile
    Set-TestState 'run' 'Test en cours'
    Set-TestButtons 'run'
    $Live.Fast = $true
    $body = $ui.TestBody
    $stepper = New-Stepper $Def.Steps
    [void]$body.Children.Add($stepper.El)
    $cur = @{ Def = $Def; Stepper = $stepper; Chart = $null; LiveVal = $null; LiveLabel = $null; Freq = @{}; LastSample = $null }
    if ($Def.Chart) {
        $head = New-Object System.Windows.Controls.StackPanel
        $head.Orientation = 'Horizontal'
        $cur.LiveVal = New-Text '0' 44 '#FFFFFF' -Bold
        $cur.LiveVal.TextWrapping = 'NoWrap'
        $unit = New-Text $Def.Chart.Unit 16 '#9AA3B2' -Semi
        $unit.VerticalAlignment = 'Bottom'; $unit.Margin = New-Thickness 8 0 0 10
        [void]$head.Children.Add($cur.LiveVal)
        [void]$head.Children.Add($unit)
        $cur.LiveLabel = New-Text 'Préparation...' 13 '#9AA3B2'
        [void]$body.Children.Add($cur.LiveLabel)
        [void]$body.Children.Add($head)
        $cur.Head = $head
        $cur.Chart = New-LiveChart $Def.Chart.Color $Def.Chart.Unit $Def.Chart.Fmt
        if ($Def.Chart.Ref) { $cur.Chart.RefValue = $Def.Chart.Ref; $cur.Chart.RefText.Text = $Def.Chart.RefLabel }
        [void]$body.Children.Add($cur.Chart.El)
    }
    [OGNative]::Cancel = $false
    [OGNative]::Progress = 0
    [OGNative]::Phase = ''
    [OGNative]::LiveValue = 0
    $script:CurTest = $cur
    $script:TestTimer.Start()
    try {
        $r = Invoke-Async $Def.Work $Def.Arg | Select-Object -First 1
    } finally {
        $script:TestTimer.Stop()
        $script:CurTest = $null
        $script:TestRunning = $false
        $Live.Fast = $false
        foreach ($b in $script:TestButtons) { $b.IsEnabled = $true }
        Set-TestButtons 'done'
    }
    if (-not $r -or $r.Error) {
        if ($r.Error) { Write-Log "Test: $($r.Error)" }
        Set-TestState 'bad' 'Échec'
        [void]$body.Children.Add((New-Verdict 'bad' "Le test n'a pas pu aller au bout : $(if ($r.Error) { $r.Error } else { 'erreur inconnue' })"))
        return
    }
    if ($r.Cancelled) {
        Set-TestState 'info' 'Arrêté'
        [void]$body.Children.Add((New-Verdict 'info' 'Test arrêté.'))
        return
    }
    Update-Stepper $stepper '' -AllDone
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = '100 %'
    if ($cur.LiveLabel) { $cur.LiveLabel.Text = 'Courbe du test'; $cur.Head.Visibility = 'Collapsed' }
    $res = @{ R = @($r.R); Freq = $cur.Freq; ChartValues = $(if ($cur.Chart) { @($cur.Chart.Values) } else { @() }) }
    $Tile.Last = @{ Def = $Def; Res = $res; Ctx = $Ctx }
    $out = & $Def.Render $res $Ctx $body
    Set-TestState $out.Status $(switch ($out.Status) { 'ok' { 'Terminé' } 'warn' { 'Terminé : à surveiller' } 'bad' { 'Problème détecté' } default { 'Terminé' } })
    Show-ResultTop
    Set-TileSummary $Tile $out.Chips $out.Status
    Set-Status "$($Tile.Title) : test terminé."
}

# Réaffiche le dernier résultat (les jauges se réaniment).
function Show-LastResult($Tile) {
    $last = $Tile.Last
    if (-not $last) { return }
    if ($last.Live) { & $last.Live $Tile $last.Ctx; return }
    Show-TestPanel $Tile
    Set-TestButtons 'done'
    $script:LastRun = @{ Fn = $last.Fn; Tile = $Tile; Ctx = $last.Ctx }
    $body = $ui.TestBody
    $stepper = New-Stepper $last.Def.Steps
    [void]$body.Children.Add($stepper.El)
    Update-Stepper $stepper '' -AllDone
    if ($last.Def.Chart -and $last.Res.ChartValues.Count) {
        [void]$body.Children.Add((New-Text 'Courbe du test' 13 '#9AA3B2'))
        $ch = New-LiveChart $last.Def.Chart.Color $last.Def.Chart.Unit $last.Def.Chart.Fmt
        if ($last.Def.Chart.Ref) { $ch.RefValue = $last.Def.Chart.Ref; $ch.RefText.Text = $last.Def.Chart.RefLabel }
        foreach ($v in $last.Res.ChartValues) { [void]$ch.Values.Add([double]$v) }
        [void]$body.Children.Add($ch.El)
        Update-Chart $ch
    }
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    $out = & $last.Def.Render $last.Res $last.Ctx $body
    Set-TestState $out.Status $(if ($out.Status -eq 'ok') { 'Terminé' } elseif ($out.Status -eq 'bad') { 'Problème détecté' } else { 'Terminé : à surveiller' })
    Show-ResultTop
}

# Fait défiler le panneau jusqu'au résultat.
function Show-ResultTop {
    $ui.TestBody.UpdateLayout()
    $target = $ui.TestBody.Children | Where-Object { $_ -is [System.Windows.Controls.TextBlock] -and $_.Text -eq 'RÉSULTAT' } | Select-Object -First 1
    if ($target) {
        $y = $target.TranslatePoint([System.Windows.Point]::new(0, 0), $ui.TestBody).Y
        $ui.TestScroll.ScrollToVerticalOffset([math]::Max(0.0, $y - 8))
    }
}

function Get-GHz($Samples, [switch]$Max) {
    if (-not $Samples -or -not $Samples.Count -or -not $Live.BaseMHz) { return $null }
    $v = if ($Max) { ($Samples | Measure-Object -Maximum).Maximum } else { ($Samples | Measure-Object -Average).Average }
    $Live.BaseMHz * $v / 100 / 1000
}

# ---------------------------------------------------------------------------
# Disques
# ---------------------------------------------------------------------------
function Test-DiskSpeed($Tile, $Ctx) {
    $vol = Get-VolInfo $Ctx.Letter
    if (-not $vol -or $vol.SizeRemaining -lt 1GB) { Show-Message "Il faut au moins 1 Go de libre sur le lecteur $($Ctx.Letter): pour faire ce test."; return }
    $size = if ($vol.SizeRemaining -gt 40GB) { 8GB } elseif ($vol.SizeRemaining -gt 10GB) { 2GB } else { 256MB }
    $file = if ("$($Ctx.Letter):" -eq $env:SystemDrive) { Join-Path $env:TEMP 'OptiGame-test-disque.tmp' } else { "$($Ctx.Letter):\OptiGame-test-disque.tmp" }
    $steps = [ordered]@{ write = 'Écriture'; read = 'Lecture'; random = 'Petits fichiers' }
    $def = @{
        Steps = $steps; Work = $DiskWork; Arg = @{ File = $file; Size = [long]$size }
        Chart = @{ Unit = 'Mo/s'; Fmt = '{0:N0}'; Color = $Colors.ok; Source = 'engine'; Phases = @('write', 'read') }
        Render = {
            param($res, $ctx, $body)
            $r = $res.R
            $scale = switch ($ctx.Kind) { 'NVMe' { 7000 } 'SSD' { 600 } 'HDD' { 250 } default { 1000 } }
            [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
            [void]$body.Children.Add((New-GaugeRow @(
                (New-Gauge 'Lecture' $r[1] $scale '{0:N0}' 'Mo/s' $Colors.ok 0),
                (New-Gauge 'Écriture' $r[0] $scale '{0:N0}' 'Mo/s' $Colors.info 150),
                (New-Gauge 'Petits fichiers' $r[3] 25000 '{0:N0}' 'par seconde' '#B18CFF' 300)
            )))
            [void]$body.Children.Add((New-SectionTitle 'COMPARAISON (LECTURE)'))
            [void]$body.Children.Add((New-CompareBars @(
                @{ Label = 'Ton disque'; Value = $r[1]; Mine = $true; Color = $Colors.ok },
                @{ Label = 'Disque dur'; Value = 150 },
                @{ Label = 'SSD classique'; Value = 550 },
                @{ Label = 'SSD NVMe récent'; Value = 5000 }
            ) 'Mo/s'))
            $min = switch ($ctx.Kind) { 'NVMe' { 1200 } 'SSD' { 350 } 'HDD' { 80 } default { 0 } }
            if ($min -and $r[1] -lt $min) {
                $status = 'warn'
                $txt = 'Plus lent que la normale pour ce type de disque : disque presque plein, qui chauffe, ou SSD branché sur un port lent.'
            } else {
                $status = 'ok'
                $txt = 'Vitesse normale pour ce type de disque.'
            }
            [void]$body.Children.Add((New-Verdict $status $txt))
            @{ Status = $status; Chips = @(@('Lecture', ('{0:N0} Mo/s' -f $r[1])), @('Écriture', ('{0:N0} Mo/s' -f $r[0]))) }
        }
    }
    $def.Chart.Phases = @('write', 'read')
    Set-Status "Test de vitesse : $($Ctx.Name)..."
    Invoke-ComponentTest $Tile $def $Ctx 'Test-DiskSpeed'
}

function Show-DiskHealth($Tile, $Ctx) {
    $pd = Invoke-Async $DiskInfoWork $Ctx.Id | Select-Object -First 1
    $d = $pd.D
    if (-not $d) { Show-Message 'Disque introuvable.'; return }
    $script:LastRun = @{ Fn = 'Show-DiskHealth'; Tile = $Tile; Ctx = $Ctx }
    Show-TestPanel $Tile
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    $body = $ui.TestBody
    $rel = $null
    $rel = $pd.Rel
    $status = 'ok'; $notes = @()
    switch ([string]$d.HealthStatus) {
        'Warning'   { $status = 'warn'; $notes += 'Le disque signale lui même un problème.' }
        'Unhealthy' { $status = 'bad'; $notes += 'Le disque annonce une panne proche.' }
    }
    $gauges = @()
    $i = 0
    if ($rel -and $null -ne $rel.Wear -and $Ctx.Kind -ne 'HDD') {
        $life = 100 - [int]$rel.Wear
        $gauges += New-Gauge 'Durée de vie restante' $life 100 '{0:N0}' '%' $(if ($life -le 10) { $Colors.bad } elseif ($life -le 30) { $Colors.warn } else { $Colors.ok }) 0
        if ($life -le 10) { $status = 'bad'; $notes += "Il reste environ $life % de durée de vie." } elseif ($life -le 30) { if ($status -eq 'ok') { $status = 'warn' }; $notes += "Il reste environ $life % de durée de vie." }
    } else {
        $sh = switch ([string]$d.HealthStatus) { 'Healthy' { 100 } 'Warning' { 50 } 'Unhealthy' { 10 } default { 0 } }
        $gauges += New-Gauge 'État SMART' $sh 100 '{0:N0}' '%' $(if ($sh -ge 100) { $Colors.ok } elseif ($sh -ge 50) { $Colors.warn } else { $Colors.bad }) 0
    }
    if ($rel -and $rel.Temperature -gt 0) {
        $warnT = if ($Ctx.Kind -eq 'HDD') { 50 } else { 70 }
        $gauges += New-Gauge 'Température' $rel.Temperature 90 '{0:N0}' '°C' (Get-LoadColor $rel.Temperature $warnT ($warnT + 10)) 150
        if ($rel.Temperature -ge $warnT) { if ($status -eq 'ok') { $status = 'warn' }; $notes += 'Le disque chauffe : vérifie la ventilation.' }
    }
    $fillPct = $null
    foreach ($l in $Ctx.Letters) {
        $v = Get-VolInfo $l
        if ($v -and $v.Size) { $fillPct = 100 * ($v.Size - $v.SizeRemaining) / $v.Size; break }
    }
    if ($null -ne $fillPct) {
        $gauges += New-Gauge 'Rempli' $fillPct 100 '{0:N0}' '%' (Get-LoadColor $fillPct 80 90) 300
        if ($fillPct -ge 90) { if ($status -eq 'ok') { $status = 'warn' }; $notes += 'Le disque est presque plein.' }
    }
    [void]$body.Children.Add((New-SectionTitle 'SANTÉ DU DISQUE'))
    [void]$body.Children.Add((New-GaugeRow $gauges))
    $tiles = @()
    if ($rel -and $rel.PowerOnHours -gt 0) { $tiles += New-StatTile 'heures allumé au total' ([double]$rel.PowerOnHours) '{0:N0}' '#FFFFFF' 200 }
    if ($rel -and $null -ne $rel.ReadErrorsUncorrected) {
        $errs = [double]$rel.ReadErrorsUncorrected + $(if ($null -ne $rel.WriteErrorsUncorrected) { [double]$rel.WriteErrorsUncorrected } else { 0 })
        $tiles += New-StatTile 'erreurs non réparées' $errs '{0:N0}' $(if ($errs) { $Colors.warn } else { $Colors.ok }) 350
        if ($errs) { if ($status -eq 'ok') { $status = 'warn' }; $notes += "$errs erreur(s) de lecture ou d'écriture." }
    }
    if ($rel -and $rel.StartStopCycleCount -gt 0) { $tiles += New-StatTile 'démarrages' ([double]$rel.StartStopCycleCount) '{0:N0}' '#FFFFFF' 500 }
    if ($tiles) { [void]$body.Children.Add((New-StatRow $tiles)) }
    $rows = @(
        @('Modèle', ([string]$d.FriendlyName).Trim()),
        @('Numéro de série', ([string]$d.SerialNumber).Trim().TrimEnd('.')),
        @('Version du micrologiciel', [string]$d.FirmwareVersion),
        @('Type', "$($Ctx.KindLabel), branché en $([string]$d.BusType)"),
        @('Capacité', (Format-Size $d.Size)),
        @('État SMART', $(switch ([string]$d.HealthStatus) { 'Healthy' { 'Bon' } 'Warning' { 'Avertissement' } 'Unhealthy' { 'Défaillant' } default { 'Inconnu' } }))
    )
    if ($rel) {
        if ($null -ne $rel.Wear) { $rows += , @('Usure', "$([int]$rel.Wear) %") }
        if ($rel.TemperatureMax -gt 0) { $rows += , @('Température limite du fabricant', "$([int]$rel.TemperatureMax) °C") }
        if ($null -ne $rel.ReadErrorsCorrected) { $rows += , @('Erreurs de lecture réparées', ('{0:N0}' -f $rel.ReadErrorsCorrected)) }
        if ($null -ne $rel.ReadErrorsUncorrected) { $rows += , @('Erreurs de lecture non réparées', ('{0:N0}' -f $rel.ReadErrorsUncorrected)) }
        if ($null -ne $rel.WriteErrorsUncorrected) { $rows += , @('Erreurs d''écriture non réparées', ('{0:N0}' -f $rel.WriteErrorsUncorrected)) }
        if ($rel.ReadLatencyMax -gt 0) { $rows += , @('Temps de réponse max en lecture', "$($rel.ReadLatencyMax) ms") }
        if ($rel.WriteLatencyMax -gt 0) { $rows += , @('Temps de réponse max en écriture', "$($rel.WriteLatencyMax) ms") }
    } else {
        $rows += , @('Détails SMART', 'Non fournis par ce disque', '#9AA3B2')
    }
    foreach ($l in $Ctx.Letters) {
        $v = Get-VolInfo $l
        if ($v -and $v.Size) { $rows += , @("Lecteur $($l):", "$(Format-Size $v.SizeRemaining) libres sur $(Format-Size $v.Size)") }
    }
    [void]$body.Children.Add((New-Details $rows))
    $txt = switch ($status) {
        'ok'   { 'Ce disque est en bonne santé.' }
        'warn' { 'À surveiller : ' + ($notes -join ' ') + ' Pense à sauvegarder tes fichiers importants.' }
        'bad'  { 'Attention : ' + ($notes -join ' ') + ' Sauvegarde tes fichiers maintenant et prévois de remplacer ce disque.' }
    }
    [void]$body.Children.Add((New-Verdict $status $txt))
    Set-TestState $status $(switch ($status) { 'ok' { 'Bonne santé' } 'warn' { 'À surveiller' } default { 'Problème détecté' } })
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    $chips = @(, @('Santé', $(switch ($status) { 'ok' { 'Bonne' } 'warn' { 'À surveiller' } default { 'Problème' } })))
    if ($rel -and $rel.Temperature -gt 0) { $chips += , @('Température', "$([int]$rel.Temperature) °C") }
    if (-not $Tile.Last) { $Tile.Last = @{ Live = { param($t2, $c2) Show-DiskHealth $t2 $c2 }; Ctx = $Ctx } }
    Set-TileSummary $Tile $chips $status
}

function Test-DiskErrors($Tile, $Ctx) {
    $def = @{
        Steps = [ordered]@{ scan = 'Recherche d''erreurs (quelques minutes)' }
        Work = $RepairWork; Arg = @{ Letters = @($Ctx.Letters) }
        Chart = $null
        Render = {
            param($res, $ctx, $body)
            [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
            $bad = $false
            $tiles = @()
            $i = 0
            foreach ($line in $res.R) {
                $l, $v = ([string]$line) -split '\|', 2
                $ok = $v -eq 'NoErrorsFound'
                if (-not $ok -and $v -notlike 'ERR*') { $bad = $true }
                $label = if ($ok) { 'aucune erreur' } elseif ($v -like 'ERR*') { 'vérification impossible' } else { 'erreurs trouvées' }
                $tiles += New-StatTile "Lecteur $($l): $label" $(if ($ok) { 0 } else { 1 }) $(if ($ok) { '✓' } else { '!' }) $(if ($ok) { $Colors.ok } else { $Colors.warn }) (150 * $i)
                $i++
            }
            [void]$body.Children.Add((New-StatRow $tiles))
            if ($bad) {
                [void]$body.Children.Add((New-Verdict 'warn' 'Windows a trouvé des erreurs dans le système de fichiers. Redémarre le PC : Windows les répare souvent tout seul.'))
                @{ Status = 'warn'; Chips = @(, @('Erreurs', 'Trouvées')) }
            } else {
                [void]$body.Children.Add((New-Verdict 'ok' 'Aucune erreur trouvée sur ce disque.'))
                @{ Status = 'ok'; Chips = @(, @('Erreurs', 'Aucune')) }
            }
        }
    }
    Set-Status "Recherche d'erreurs : $($Ctx.Name)..."
    Invoke-ComponentTest $Tile $def $Ctx 'Test-DiskErrors'
}

# ---------------------------------------------------------------------------
# Processeur
# ---------------------------------------------------------------------------
function Test-Cpu($Tile, $Ctx) {
    $base = $Live.BaseMHz / 1000
    $def = @{
        Steps = [ordered]@{ single = 'Un seul cœur'; multi = 'Tous les cœurs' }
        Work = $CpuWork; Arg = @{ Single = $Ctx.Single; Multi = $Ctx.Multi }
        Chart = @{ Unit = 'GHz'; Fmt = '{0:N1}'; Color = $Colors.info; Source = 'cpu'; Phases = @('single', 'multi'); Ref = $base; RefLabel = ('fréquence de base {0:N1} GHz' -f $base) }
        Render = {
            param($res, $ctx, $body)
            $r = $res.R
            $maxSingle = Get-GHz $res.Freq['single'] -Max
            $avgMulti = Get-GHz $res.Freq['multi']
            $base2 = $Live.BaseMHz / 1000
            $top = [math]::Max(6.0, [math]::Ceiling(([double]$maxSingle) + 0.5))
            [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
            [void]$body.Children.Add((New-StatRow @(
                (New-StatTile 'points sur un cœur' $r[0] '{0:N0}' '#FFFFFF' 0),
                (New-StatTile "points sur les $([int]$r[3]) cœurs" $r[1] '{0:N0}' '#FFFFFF' 150),
                (New-StatTile 'erreurs de calcul' $r[2] '{0:N0}' $(if ($r[2]) { $Colors.bad } else { $Colors.ok }) 300)
            )))
            $g = @()
            if ($maxSingle) { $g += New-Gauge 'Fréquence max' $maxSingle $top '{0:N1}' 'GHz' $Colors.info 200 }
            if ($avgMulti) { $g += New-Gauge 'En pleine charge' $avgMulti $top '{0:N1}' 'GHz' $(if ($avgMulti -lt $base2 * 0.95) { $Colors.warn } else { $Colors.ok }) 350 }
            $g += New-Gauge 'Tous les cœurs vs un seul' ($r[1] / [math]::Max(1.0, $r[0])) ([math]::Max(1.0, $r[3])) '{0:N1}' 'fois plus' '#B18CFF' 500
            [void]$body.Children.Add((New-GaugeRow $g))
            if ($r[2] -gt 0) {
                $status = 'bad'; $txt = 'Le processeur a fait des erreurs de calcul : il est instable (overclock ou undervolt trop poussé, XMP instable, surchauffe). Remets les réglages du BIOS par défaut et refais le test.'
            } elseif ($avgMulti -and $avgMulti -lt $base2 * 0.95) {
                $status = 'warn'; $txt = 'Sous forte charge, le processeur passe sous sa fréquence de base : il chauffe trop ou manque d''alimentation. Vérifie le ventirad et la pâte thermique.'
            } else {
                $status = 'ok'; $txt = $(if ($ctx.Multi -ge 120) { 'Aucune erreur pendant 5 minutes à pleine charge : ton processeur est stable.' } else { 'Tout est normal : aucune erreur et le processeur garde bien sa vitesse.' })
            }
            [void]$body.Children.Add((New-Verdict $status $txt))
            @{ Status = $status; Chips = @(@('1 cœur', ('{0:N0} pts' -f $r[0])), @('Tous les cœurs', ('{0:N0} pts' -f $r[1])), @('En charge', $(if ($avgMulti) { '{0:N1} GHz' -f $avgMulti } else { '?' }))) }
        }
    }
    Set-Status 'Test du processeur...'
    Invoke-ComponentTest $Tile $def $Ctx 'Test-Cpu'
}

# ---------------------------------------------------------------------------
# Mémoire vive
# ---------------------------------------------------------------------------
function Test-Memory($Tile, $Ctx) {
    $free = if ($Live.RamTotal) { [double]$Live.RamTotal - [double]$Live.RamUsed } else { [double]2GB }
    $bytes = [long][math]::Min([double]2GB, [math]::Max([double]256MB, $free * 0.5))
    $def = @{
        Steps = [ordered]@{ alloc = 'Réservation'; write = 'Écriture'; read = 'Vérification'; pattern = '2e passage'; copy = 'Copie' }
        Work = $MemWork; Arg = @{ Bytes = $bytes }
        Chart = @{ Unit = 'Go/s'; Fmt = '{0:N1}'; Color = '#B18CFF'; Source = 'engine'; Phases = @('write', 'read', 'pattern') }
        Render = {
            param($res, $ctx, $body)
            $r = $res.R
            [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
            [void]$body.Children.Add((New-GaugeRow @(
                (New-Gauge 'Lecture' $r[1] 100 '{0:N1}' 'Go/s' $Colors.ok 0),
                (New-Gauge 'Écriture' $r[0] 100 '{0:N1}' 'Go/s' $Colors.info 150),
                (New-Gauge 'Copie' $r[2] 100 '{0:N1}' 'Go/s' '#B18CFF' 300)
            )))
            [void]$body.Children.Add((New-StatRow @(
                (New-StatTile 'Go vérifiés' $r[4] '{0:N1}' '#FFFFFF' 300),
                (New-StatTile 'erreurs trouvées' $r[3] '{0:N0}' $(if ($r[3]) { $Colors.bad } else { $Colors.ok }) 450)
            )))
            if ($r[3] -gt 0) {
                $status = 'bad'; $txt = 'Des erreurs ont été trouvées. Désactive le profil XMP / EXPO dans le BIOS et refais le test. Si ça continue, une barrette est défectueuse : lance le test complet de Windows.'
            } else {
                $status = 'ok'; $txt = 'Aucune erreur. Ce test rapide ne vérifie qu''une partie de la mémoire : en cas de plantages, lance le test complet de Windows.'
            }
            [void]$body.Children.Add((New-Verdict $status $txt))
            @{ Status = $status; Chips = @(@('Lecture', ('{0:N1} Go/s' -f $r[1])), @('Erreurs', ('{0:N0}' -f $r[3]))) }
        }
    }
    Set-Status 'Test de la mémoire...'
    Invoke-ComponentTest $Tile $def $Ctx 'Test-Memory'
}

# ---------------------------------------------------------------------------
# Carte graphique: surveillance en direct
# ---------------------------------------------------------------------------
function Show-GpuMonitor($Tile, $Ctx) {
    $g = $Ctx.Gpu
    $script:LastRun = @{ Fn = 'Show-GpuMonitor'; Tile = $Tile; Ctx = $Ctx }
    Show-TestPanel $Tile
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    Set-TestState 'live' 'En direct'
    $Live.Fast = $true
    $body = $ui.TestBody
    $nvidia = $g.Name -match 'NVIDIA|GeForce' -and $SmiPath
    $info = $null
    if ($nvidia) {
        $fields = 'driver_version,vbios_version,pstate,clocks.gr,clocks.max.gr,clocks.mem,clocks.max.mem,temperature.gpu,fan.speed,power.draw,power.limit,pcie.link.gen.current,pcie.link.gen.max,pcie.link.width.current,pcie.link.width.max,utilization.gpu,memory.used,memory.total,clocks_throttle_reasons.active'
        $o = & $SmiPath "--query-gpu=$fields" '--format=csv,noheader,nounits' 2>$null | Select-Object -First 1
        if ($o) { $info = @($o -split ',' | ForEach-Object { $_.Trim() }) }
    }
    $num = { param($s) if ($s -match '^[\d\.]+$') { [double]::Parse($s, [Globalization.CultureInfo]::InvariantCulture) } else { 0 } }
    $plimit = if ($info) { [math]::Max(50.0, (& $num $info[10])) } else { 300 }
    [void]$body.Children.Add((New-Text 'Lance un jeu, puis reviens ici avec Alt + Tab : tout se met à jour en direct.' 13 '#9AA3B2'))
    $gUse = New-Gauge 'Utilisation' ([double]$Live.Gpu) 100 '{0:N0}' '%' $Colors.info 0
    $gauges = @($gUse)
    $gTemp = $null; $gPow = $null
    if ($nvidia) {
        $gTemp = New-Gauge 'Température' ([double]$Live.GpuTemp) 100 '{0:N0}' '°C' $Colors.ok 150
        $gPow = New-Gauge 'Consommation' ([double]$Live.GpuPower) $plimit '{0:N0}' 'W' $Colors.warn 300
        $gauges += $gTemp; $gauges += $gPow
    }
    [void]$body.Children.Add((New-GaugeRow $gauges))
    [void]$body.Children.Add((New-Text 'Utilisation de la carte graphique' 13 '#9AA3B2'))
    $chart = New-LiveChart $Colors.info '%' '{0:N0}'
    [void]$body.Children.Add($chart.El)
    $status = 'ok'
    if ($info) {
        $bits = 0
        try { $bits = [Convert]::ToUInt64(($info[18] -replace '^0x', ''), 16) } catch {}
        $why = @()
        if ($bits -band 0x4)  { $why += 'limite de consommation (normal en pleine charge)' }
        if ($bits -band 0x20) { $why += 'chauffe' }
        if ($bits -band 0x40) { $why += 'surchauffe' }
        if ($bits -band 0x8)  { $why += 'ralentissement matériel' }
        if ($bits -band 0x80) { $why += 'alimentation insuffisante' }
        $rows = @(
            @('Fréquence du GPU', "$($info[3]) MHz (max $($info[4]) MHz)"),
            @('Fréquence de la mémoire', "$($info[5]) MHz (max $($info[6]) MHz)"),
            @('Mémoire vidéo utilisée', ('{0:N1} Go sur {1:N0} Go' -f ((& $num $info[16]) / 1024), ((& $num $info[17]) / 1024))),
            @('Ventilateurs', "$($info[8]) %"),
            @('Liaison PCIe', "Gen $($info[11]) x$($info[13])  (max Gen $($info[12]) x$($info[14]))"),
            @('Ralentissements', $(if ($why) { $why -join ', ' } else { 'Aucun' })),
            @('Pilote', $info[0]),
            @('BIOS de la carte', $info[1])
        )
        [void]$body.Children.Add((New-Details $rows))
        if ($bits -band 0xE8) { $status = 'warn'; [void]$body.Children.Add((New-Verdict 'warn' 'La carte ralentit à cause de la chaleur ou de l''alimentation : dépoussière le PC et vérifie la ventilation du boîtier.')) }
        else { [void]$body.Children.Add((New-Verdict 'ok' 'Aucun ralentissement. À vide, la carte baisse sa vitesse pour économiser l''énergie : c''est normal.')) }
    } else {
        $rows = @(@('Modèle', $g.Name), @('Pilote', [string]$g.DriverVersion), @('Résolution', "$($g.CurrentHorizontalResolution) x $($g.CurrentVerticalResolution)"))
        [void]$body.Children.Add((New-Details $rows))
        [void]$body.Children.Add((New-Verdict 'info' 'Température et consommation ne sont lisibles que sur les cartes NVIDIA. Pour une carte AMD : AMD Software > Performances.'))
    }
    $script:GpuLive = @{ Use = $gUse; Temp = $gTemp; Pow = $gPow; Chart = $chart; Last = $null }
    $script:MonitorTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:MonitorTimer.Interval = [TimeSpan]::FromMilliseconds(500)
    $script:MonitorTimer.Add_Tick({
        $m = $script:GpuLive
        if (-not $m -or $Live.Updated -eq $m.Last) { return }
        $m.Last = $Live.Updated
        Set-GaugeLive $m.Use ([double]$Live.Gpu)
        if ($m.Temp) { Set-GaugeLive $m.Temp ([double]$Live.GpuTemp) }
        if ($m.Pow) { Set-GaugeLive $m.Pow ([double]$Live.GpuPower) }
        Add-ChartPoint $m.Chart ([double]$Live.Gpu)
    })
    $script:MonitorTimer.Start()
    $ui.TestProgress.Value = 0; $ui.TestPct.Text = ''
    $chips = @(, @('Température', $(if ($Live.GpuTemp) { "$([int]$Live.GpuTemp) °C" } else { '?' })))
    if ($info) { $chips += , @('Consommation', "$([int](& $num $info[9])) W") }
    $Tile.Last = @{ Live = { param($t2, $c2) Show-GpuMonitor $t2 $c2 }; Ctx = $Ctx }
    Set-TileSummary $Tile $chips $status
}

# ---------------------------------------------------------------------------
# Réseau
# ---------------------------------------------------------------------------
function Test-NetSpeed($Tile, $Ctx) {
    $def = @{
        Steps = [ordered]@{ ping = 'Ping'; down = 'Téléchargement'; up = 'Envoi' }
        Work = $NetWork; Arg = @{}
        Chart = @{ Unit = 'Mb/s'; Fmt = '{0:N0}'; Color = $Colors.ok; Source = 'engine'; Phases = @('down', 'up') }
        Render = {
            param($res, $ctx, $body)
            $r = $res.R
            $scale = if ([math]::Max($r[1], $r[2]) -gt 1000) { 2500 } else { 1000 }
            [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
            $g = @()
            if ($r[1] -ge 0) { $g += New-Gauge 'Téléchargement' $r[1] $scale '{0:N0}' 'Mb/s' $Colors.ok 0 }
            if ($r[2] -ge 0) { $g += New-Gauge 'Envoi' $r[2] $scale '{0:N0}' 'Mb/s' $Colors.info 150 }
            if ($r[0] -ge 0) { $g += New-Gauge 'Ping' $r[0] 100 '{0:N0}' 'ms' $(if ($r[0] -gt 60) { $Colors.warn } else { $Colors.ok }) 300 }
            [void]$body.Children.Add((New-GaugeRow $g))
            if ($r[1] -gt 0) {
                $min = 50 * 8000 / $r[1] / 60
                [void]$body.Children.Add((New-StatRow @((New-StatTile 'minutes pour télécharger un jeu de 50 Go' $min '{0:N0}' '#FFFFFF' 400))))
            }
            if ($r[1] -lt 0 -or $r[2] -lt 0) {
                $status = 'info'; $txt = 'Les serveurs de test n''ont pas répondu (trop de tests d''affilée ou pas de connexion). Réessaie dans quelques minutes.'
            } elseif ($r[1] -lt 10) {
                $status = 'warn'; $txt = 'Connexion lente : les téléchargements seront longs. Pour jouer, c''est surtout le ping qui compte.'
            } elseif ($r[0] -gt 60) {
                $status = 'warn'; $txt = 'Le débit est correct mais le ping est élevé : en Wi-Fi, rapproche toi de la box ou branche un câble.'
            } else {
                $status = 'ok'; $txt = 'Bonne connexion pour jouer et télécharger.'
            }
            [void]$body.Children.Add((New-Verdict $status $txt))
            @{ Status = $status; Chips = @(@('Téléchargement', $(if ($r[1] -ge 0) { '{0:N0} Mb/s' -f $r[1] } else { '?' })), @('Ping', $(if ($r[0] -ge 0) { '{0:N0} ms' -f $r[0] } else { '?' }))) }
        }
    }
    Set-Status 'Test de la connexion...'
    Invoke-ComponentTest $Tile $def $Ctx 'Test-NetSpeed'
}

# ---------------------------------------------------------------------------
# Écrans: pixels morts
# ---------------------------------------------------------------------------
function Start-PixelTest([int]$Index) {
    $screens = [System.Windows.Forms.Screen]::AllScreens
    if ($Index -ge $screens.Count) { return }
    $b = $screens[$Index].Bounds
    $src = [System.Windows.PresentationSource]::FromVisual($Window)
    $scale = if ($src) { $src.CompositionTarget.TransformToDevice.M11 } else { 1 }
    $w = New-Object System.Windows.Window
    $w.WindowStyle = 'None'; $w.ResizeMode = 'NoResize'; $w.Topmost = $true; $w.ShowInTaskbar = $false
    $w.WindowStartupLocation = 'Manual'
    $w.Width = 100; $w.Height = 100
    $w.Left = ($b.X + $b.Width / 2) / $scale - 50
    $w.Top = ($b.Y + $b.Height / 2) / $scale - 50
    $w.Cursor = [System.Windows.Input.Cursors]::None
    $script:PixColors = @('#000000', '#FFFFFF', '#FF0000', '#00FF00', '#0000FF', '#808080')
    $script:PixIndex = 0
    $w.Background = Get-Brush $script:PixColors[0]
    $hint = New-Text "Cherche les points qui ne sont pas de la bonne couleur.`n`nClic ou Espace : couleur suivante        Échap : quitter" 20 '#9AA3B2'
    $hint.HorizontalAlignment = 'Center'; $hint.VerticalAlignment = 'Center'; $hint.TextAlignment = 'Center'
    $w.Content = $hint
    $w.Add_SourceInitialized({ param($s, $e) $s.WindowState = 'Maximized' })
    $w.Add_MouseDown({
        param($s, $e)
        $script:PixIndex++
        if ($script:PixIndex -ge $script:PixColors.Count) { $s.Close(); return }
        $s.Content = $null
        $s.Background = Get-Brush $script:PixColors[$script:PixIndex]
    })
    $w.Add_KeyDown({
        param($s, $e)
        if ($e.Key -eq 'Escape') { $s.Close(); return }
        if ($e.Key -in 'Space', 'Enter', 'Right') {
            $script:PixIndex++
            if ($script:PixIndex -ge $script:PixColors.Count) { $s.Close(); return }
            $s.Content = $null
            $s.Background = Get-Brush $script:PixColors[$script:PixIndex]
        }
    })
    [void]$w.ShowDialog()
}

# ---------------------------------------------------------------------------
# Construction de l'onglet
# ---------------------------------------------------------------------------
function Test-3DMark {
    $steam = [string](Get-ItemProperty 'HKCU:\Software\Valve\Steam' -ErrorAction SilentlyContinue).SteamPath
    if (-not $steam) { return $false }
    $libs = @($steam -replace '/', '\')
    $vdf = Join-Path $libs[0] 'steamapps\libraryfolders.vdf'
    if (Test-Path -LiteralPath $vdf) { foreach ($m in [regex]::Matches((Get-Content -LiteralPath $vdf -Raw), '"path"\s+"([^"]+)"')) { $libs += ($m.Groups[1].Value -replace '\\\\', '\') } }
    [bool]($libs | Where-Object { Test-Path -LiteralPath (Join-Path $_ 'steamapps\common\3DMark') })
}

function Build-TestsTab {
    $ui.TestsPanel.Children.Clear()
    $script:TestButtons.Clear()
    if (-not $script:AnalysisData) { $script:AnalysisData = Invoke-Async $AnalysisDataWork $env:SystemDrive | Select-Object -First 1 }

    foreach ($dd in @($script:AnalysisData.Disks)) {
        $d = $dd.Disk
        $media = [string]$d.MediaType; $bus = [string]$d.BusType
        $kind = if ($bus -eq 'NVMe') { 'NVMe' } elseif ($media -eq 'SSD') { 'SSD' } elseif ($media -eq 'HDD') { 'HDD' } elseif ($bus -eq 'USB') { 'USB' } else { 'SSD' }
        $kindLabel = switch ($kind) { 'NVMe' { 'SSD NVMe' } 'SSD' { 'SSD' } 'HDD' { 'Disque dur' } default { 'Disque externe' } }
        $letters = @()
        $letters = @($dd.Letters)
        $sub = "$kindLabel, $(Format-Size $d.Size)" + $(if ($letters) { "  /  $(($letters | ForEach-Object { "$($_):" }) -join ' ')" } else { '' })
        $tag = switch ($kind) { 'HDD' { 'HDD' } 'USB' { 'USB' } default { 'SSD' } }
        $tile = New-TestTile $tag (([string]$d.FriendlyName).Trim()) $sub 'Vitesse réelle et santé du disque (usure, température, erreurs).'
        $ctx = @{ Id = [string]$d.DeviceId; Name = ([string]$d.FriendlyName).Trim(); Kind = $kind; KindLabel = $kindLabel; Letters = $letters; Letter = $(if ($letters) { $letters[0] } else { $null }) }
        if ($ctx.Letter) { Add-TestButton $tile 'Tester la vitesse' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-DiskSpeed $x.T $x.Ctx } } $ctx -Primary }
        Add-TestButton $tile 'Santé' { param($s, $e) $x = $s.Tag; Invoke-Safe { Show-DiskHealth $x.T $x.Ctx } } $ctx
        if ($ctx.Letter) { Add-TestButton $tile 'Erreurs' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-DiskErrors $x.T $x.Ctx } } $ctx }
    }

    $cpu = $script:AnalysisData.CPU
    $tile = New-TestTile 'CPU' (($cpu.Name -replace '\s+', ' ').Trim()) "$($cpu.NumberOfCores) cœurs, $($cpu.NumberOfLogicalProcessors) threads" 'Puissance, vitesse tenue quand il chauffe et stabilité. Ferme tes jeux avant.'
    Add-TestButton $tile 'Test rapide (30 s)' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-Cpu $x.T $x.Ctx } } @{ Single = 8; Multi = 22 } -Primary
    Add-TestButton $tile 'Stabilité (5 min)' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-Cpu $x.T $x.Ctx } } @{ Single = 5; Multi = 295 }

    $mem = @($script:AnalysisData.Mem)
    $totalGB = [math]::Round((($mem | Measure-Object Capacity -Sum).Sum) / 1GB)
    $tile = New-TestTile 'RAM' 'Mémoire vive' "$totalGB Go, $($mem.Count) barrette$(if ($mem.Count -gt 1) {'s'})" 'Vitesse et recherche d''erreurs en quelques secondes.'
    Add-TestButton $tile 'Tester la mémoire' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-Memory $x.T $x.Ctx } } @{} -Primary
    Add-TestButton $tile 'Test complet Windows' { param($s, $e) Start-Process 'mdsched.exe' } @{}

    foreach ($g in @($script:AnalysisData.GPUs | Where-Object { $_.Name -notmatch 'Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta|Microsoft Basic' })) {
        $tile = New-TestTile 'GPU' $g.Name 'Carte graphique' 'Température, utilisation et consommation en direct, et ralentissements éventuels.'
        Add-TestButton $tile 'Surveiller en direct' { param($s, $e) $x = $s.Tag; Invoke-Safe { Show-GpuMonitor $x.T $x.Ctx } } @{ Gpu = $g } -Primary
        if (Test-3DMark) {
            Add-TestButton $tile '3DMark' { param($s, $e) Start-Process 'steam://rungameid/223850' } @{}
        }
    }

    $tile = New-TestTile 'NET' 'Connexion Internet' 'Ping, téléchargement, envoi' 'Réactivité et vitesse de ta connexion, en 20 secondes.'
    Add-TestButton $tile 'Tester ma connexion' { param($s, $e) $x = $s.Tag; Invoke-Safe { Test-NetSpeed $x.T $x.Ctx } } @{} -Primary

    $screens = [System.Windows.Forms.Screen]::AllScreens
    $tile = New-TestTile 'HZ' 'Écrans' "$($screens.Count) écran$(if ($screens.Count -gt 1) {'s'})" 'Couleurs unies en plein écran pour repérer les pixels morts. Échap pour quitter.'
    for ($i = 0; $i -lt $screens.Count; $i++) {
        Add-TestButton $tile "Écran $($i + 1)$(if ($screens[$i].Primary -and $screens.Count -gt 1) { ' (principal)' })" { param($s, $e) $x = $s.Tag; Start-PixelTest $x.Ctx.Index } @{ Index = $i } -Primary:($i -eq 0)
    }

    if ($script:IsLaptop -and @($script:AnalysisData.Battery).Count) {
        $tile = New-TestTile 'BAT' 'Batterie' 'Rapport de Windows' 'Capacité d''origine, capacité actuelle et autonomie estimée.'
        Add-TestButton $tile 'Voir le rapport' {
            param($s, $e)
            $out = Join-Path $env:TEMP 'rapport-batterie.html'
            Start-Process -FilePath 'powercfg.exe' -ArgumentList '/batteryreport', '/output', "`"$out`"" -Wait -WindowStyle Hidden
            if (Test-Path $out) { Start-Process $out }
        } @{} -Primary
    }
}

# ---------------------------------------------------------------------------
# Onglet Sécurité (antivirus Microsoft Defender + recherche de points suspects)
# ---------------------------------------------------------------------------
$MpCmd = Join-Path $env:ProgramFiles 'Windows Defender\MpCmdRun.exe'
$script:SecButtons = New-Object System.Collections.ArrayList

# État de l'antivirus et des protections de Windows.
function Get-ProtectionStatus {
    $riskDirs = @($env:TEMP, "$env:SystemDrive\Users\Public", $env:ProgramData, $env:APPDATA, $env:LOCALAPPDATA)
    $arg = @{
        UserDirs = @((Join-Path ([Environment]::GetFolderPath('UserProfile')) 'Downloads'), [Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('MyDocuments'))
        Folders = @(Get-SuspectFolders); StartupExes = @(Get-StartupItems | Where-Object { $_.Enabled } | ForEach-Object { $_.Exe })
        RiskDirs = $riskDirs; Temp = $env:TEMP; Public = "$env:SystemDrive\Users\Public"
    }
    $d = Invoke-Async $SecDataWork $arg | Select-Object -First 1
    $off = @($d.FirewallOff)
    @{
        Mp = $d.Mp; OtherAv = @($d.OtherAv); FirewallOff = $off; Firewall = -not $off.Count
        Uac = (Get-RegValue 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' 'EnableLUA') -ne 0
        Exclusions = @($d.Exclusions); Active = @($d.Active); Threats = @($d.Threats); Detections = @($d.Detections)
        Double = @($d.Double); Scripts = @($d.Scripts); Hidden = @($d.Hidden); SusStart = @($d.SusStart); Tasks = @($d.Tasks)
    }
}
# Note de protection sur 100.
function Get-ProtectionScore($S, [array]$Checks) {
    $score = 0
    $mp = $S.Mp
    $avOn = ($mp -and $mp.RealTimeProtectionEnabled -and $mp.AMRunningMode -eq 'Normal') -or $S.OtherAv.Count
    if ($avOn) { $score += 30 }
    if (($mp -and $mp.AntivirusSignatureAge -le 3) -or ($S.OtherAv.Count -and -not ($mp -and $mp.AMRunningMode -eq 'Normal'))) { $score += 15 }
    if ($S.Firewall) { $score += 15 }
    if ($S.Uac) { $score += 10 }
    if (-not $S.Exclusions.Count) { $score += 10 }
    if ($mp -and $mp.QuickScanAge -le 14) { $score += 10 } elseif ($S.OtherAv.Count) { $score += 10 }
    if (-not $S.Active.Count) { $score += 10 }
    foreach ($c in $Checks) { if ($c.Status -eq 'bad') { $score -= 10 } elseif ($c.Status -eq 'warn') { $score -= 3 } }
    if ($S.Active.Count) { $score = [math]::Min(30.0, $score) }
    [int][math]::Max(0.0, [math]::Min(100.0, $score))
}

function Test-Signed([string]$Path) {
    try { (Get-AuthenticodeSignature -FilePath $Path -ErrorAction Stop).Status -eq 'Valid' } catch { $false }
}

function Get-SuspectFolders {
    @(
        @{ Path = $env:TEMP; Depth = 0; Label = 'dossier temporaire' },
        @{ Path = $env:APPDATA; Depth = 0; Label = 'AppData\Roaming' },
        @{ Path = $env:LOCALAPPDATA; Depth = 0; Label = 'AppData\Local' },
        @{ Path = "$env:SystemDrive\Users\Public"; Depth = 3; Label = 'dossier Public' },
        @{ Path = $env:ProgramData; Depth = 0; Label = 'ProgramData' }
    ) | Where-Object { $_.Path -and (Test-Path -LiteralPath $_.Path) }
}

# Recherche des points suspects. Retourne une liste de constats avec actions possibles.
function Get-SecurityChecks($S) {
    $list = New-Object System.Collections.ArrayList
    $mp = $S.Mp

    # Antivirus
    if ($mp -and $mp.AMRunningMode -eq 'Normal') {
        if (-not $mp.RealTimeProtectionEnabled) {
            [void]$list.Add(@{ Status = 'bad'; Title = 'Protection en temps réel désactivée'; Detail = 'Les virus ne sont plus bloqués au moment où ils arrivent. Réactive la protection.'
                Actions = @(@{ Label = 'Ouvrir la protection'; Script = { Start-Process 'windowsdefender://threatsettings' } }) })
        }
        if ($mp.AntivirusSignatureAge -gt 3) {
            [void]$list.Add(@{ Status = 'warn'; Title = "Base de virus vieille de $($mp.AntivirusSignatureAge) jours"; Detail = 'Les nouveaux virus ne sont pas encore connus de ton antivirus. Mets la base à jour.'
                Actions = @(@{ Label = 'Mettre à jour'; Script = { Update-Definitions } }) })
        }
    } elseif (-not $S.OtherAv.Count) {
        [void]$list.Add(@{ Status = 'bad'; Title = 'Aucun antivirus actif détecté'; Detail = 'Ton PC n''est pas protégé. Active Microsoft Defender dans Sécurité Windows.'
            Actions = @(@{ Label = 'Ouvrir Sécurité Windows'; Script = { Start-Process 'windowsdefender://threat' } }) })
    }

    # Exclusions de l'antivirus
    if ($S.Exclusions.Count) {
        [void]$list.Add(@{ Status = 'warn'; Title = "$($S.Exclusions.Count) élément$(if ($S.Exclusions.Count -gt 1) {'s'}) exclu$(if ($S.Exclusions.Count -gt 1) {'s'}) de l'antivirus"
            Detail = 'Ces dossiers ou programmes ne sont jamais analysés. Si tu ne les as pas ajoutés toi même (ou un jeu que tu connais), c''est suspect : les virus s''ajoutent souvent eux mêmes en exclusion.'
            Items = $S.Exclusions
            Actions = @(@{ Label = 'Retirer les exclusions'; Confirm = 'Retirer toutes les exclusions de Microsoft Defender ? Ces éléments seront de nouveau analysés.'; Arg = $S.Exclusions
                           Script = { param($a) foreach ($x in $a) { Remove-MpPreference -ExclusionPath $x -ErrorAction SilentlyContinue; Remove-MpPreference -ExclusionProcess $x -ErrorAction SilentlyContinue; Remove-MpPreference -ExclusionExtension $x -ErrorAction SilentlyContinue } } }) })
    }

    # Pare-feu et contrôle des comptes
    if (-not $S.Firewall) {
        [void]$list.Add(@{ Status = 'bad'; Title = 'Pare-feu désactivé'; Detail = "Le pare-feu de Windows est coupé ($($S.FirewallOff -join ', ')) : ton PC est exposé sur le réseau."
            Actions = @(@{ Label = 'Réactiver le pare-feu'; Confirm = 'Réactiver le pare-feu de Windows sur tous les réseaux ?'; Script = { Set-NetFirewallProfile -Profile Domain, Public, Private -Enabled True } }) })
    }
    if (-not $S.Uac) {
        [void]$list.Add(@{ Status = 'bad'; Title = 'Contrôle des comptes désactivé'; Detail = 'Les programmes peuvent modifier Windows sans te demander la permission. Il faut le réactiver (redémarrage nécessaire).'
            Actions = @(@{ Label = 'Réactiver'; Confirm = 'Réactiver le contrôle des comptes ? Un redémarrage sera nécessaire.'; Script = { Set-Reg 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' 'EnableLUA' 1 } }) })
    }
    Update-UI

    # Fichier hosts
    $hosts = Join-Path $env:windir 'System32\drivers\etc\hosts'
    $entries = @()
    try { $entries = @(Get-Content -LiteralPath $hosts -ErrorAction Stop | ForEach-Object { $_.Trim() } | Where-Object { $_ -and $_ -notmatch '^#' -and $_ -notmatch '\s(localhost|localhost\.localdomain|broadcasthost)$' -and $_ -notmatch '^(127\.0\.0\.1|::1)\s+localhost' }) } catch {}
    if ($entries.Count) {
        [void]$list.Add(@{ Status = 'warn'; Title = "Fichier hosts modifié ($($entries.Count) ligne$(if ($entries.Count -gt 1) {'s'}))"
            Detail = 'Ce fichier peut rediriger des sites vers d''autres adresses. Des logiciels l''utilisent pour bloquer la pub, mais un virus peut s''en servir pour t''envoyer vers de faux sites.'
            Items = $entries
            Actions = @(@{ Label = 'Ouvrir le fichier'; Arg = $hosts; Script = { param($a) Start-Process 'notepad.exe' -ArgumentList "`"$a`"" } }) })
    }

    # Proxy
    $inet = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
    $proxyOn = (Get-RegValue $inet 'ProxyEnable') -eq 1
    $pac = [string](Get-RegValue $inet 'AutoConfigURL')
    if ($proxyOn -or $pac) {
        $what = if ($pac) { "script $pac" } else { [string](Get-RegValue $inet 'ProxyServer') }
        [void]$list.Add(@{ Status = 'warn'; Title = 'Un proxy détourne ta navigation'; Detail = "Tout ton trafic web passe par : $what. Si tu ne l'as pas configuré toi même (VPN, travail), c'est suspect."
            Actions = @(@{ Label = 'Paramètres du proxy'; Script = { Start-Process 'ms-settings:network-proxy' } }) })
    }
    Update-UI

    # Fichiers piégés (double extension, scripts) dans les dossiers de l'utilisateur
    $double = @($S.Double); $scripts = @($S.Scripts)
    if ($double.Count) {
        [void]$list.Add(@{ Status = 'bad'; Title = "$($double.Count) fichier$(if ($double.Count -gt 1) {'s'}) déguisé$(if ($double.Count -gt 1) {'s'})"
            Detail = 'Un nom comme « facture.pdf.exe » fait croire à un document, mais c''est un programme. C''est la ruse la plus courante des virus : ne l''ouvre surtout pas.'
            Items = $double
            Actions = @(@{ Label = 'Analyser avec Defender'; Arg = $double; Script = { param($a) Invoke-DefenderScan 'CustomScan' $a 'Analyse des fichiers déguisés' } },
                        @{ Label = 'Ouvrir l''emplacement'; Arg = $double[0]; Script = { param($a) Start-Process 'explorer.exe' -ArgumentList "/select,`"$a`"" } }) })
    }
    if ($scripts.Count) {
        [void]$list.Add(@{ Status = 'warn'; Title = "$($scripts.Count) script$(if ($scripts.Count -gt 1) {'s'}) dans tes téléchargements"
            Detail = 'Ces types de fichiers (.vbs, .js, .hta, .scr...) servent rarement à autre chose qu''à installer des virus quand ils viennent d''Internet. Si tu ne sais pas d''où ils viennent, supprime les.'
            Items = $scripts
            Actions = @(@{ Label = 'Analyser avec Defender'; Arg = $scripts; Script = { param($a) Invoke-DefenderScan 'CustomScan' $a 'Analyse des scripts téléchargés' } },
                        @{ Label = 'Ouvrir l''emplacement'; Arg = $scripts[0]; Script = { param($a) Start-Process 'explorer.exe' -ArgumentList "/select,`"$a`"" } }) })
    }
    Update-UI

    # Programmes non signés cachés dans des dossiers à risque
    $hidden = @($S.Hidden)
    if ($hidden.Count) {
        [void]$list.Add(@{ Status = 'warn'; Title = "$($hidden.Count) programme$(if ($hidden.Count -gt 1) {'s'}) non signé$(if ($hidden.Count -gt 1) {'s'}) dans des dossiers à risque"
            Detail = 'Ces programmes sont posés directement dans des dossiers où les virus aiment se cacher, et aucun éditeur ne les a signés. Ce n''est pas forcément grave (vieux installeurs...), mais ça vaut une analyse.'
            Items = $hidden
            Actions = @(@{ Label = 'Analyser avec Defender'; Arg = $hidden; Script = { param($a) Invoke-DefenderScan 'CustomScan' $a 'Analyse des programmes cachés' } },
                        @{ Label = 'Ouvrir l''emplacement'; Arg = $hidden[0]; Script = { param($a) Start-Process 'explorer.exe' -ArgumentList "/select,`"$a`"" } }) })
    }

    # Programmes au démarrage placés dans des dossiers à risque ou non signés
    $susStart = @(Get-StartupItems | Where-Object { $_.Enabled -and ($S.SusStart -contains $_.Exe) })
    if ($susStart.Count) {
        [void]$list.Add(@{ Status = 'warn'; Title = "$($susStart.Count) programme$(if ($susStart.Count -gt 1) {'s'}) au démarrage à vérifier"
            Detail = 'Ils se lancent avec Windows et ne sont pas signés par un éditeur, ou sont rangés dans un dossier inhabituel. Si tu ne les reconnais pas, analyse les et désactive les.'
            Items = @($susStart | ForEach-Object { "$($_.Nom)  :  $($_.Exe)" })
            Actions = @(@{ Label = 'Analyser avec Defender'; Arg = @($susStart | ForEach-Object { $_.Exe }); Script = { param($a) Invoke-DefenderScan 'CustomScan' $a 'Analyse des programmes au démarrage' } },
                        @{ Label = 'Désactiver au démarrage'; Arg = $susStart; Confirm = 'Désactiver ces programmes au démarrage ? Ils restent installés.'; Script = { param($a) foreach ($i in $a) { Set-StartupState $i $false }; Update-StartupList } }) })
    }
    Update-UI

    # Tâches planifiées suspectes
    $susTasks = @($S.Tasks)
    if ($susTasks.Count) {
        $bad = [bool]($susTasks | Where-Object { $_.Bad })
        [void]$list.Add(@{ Status = $(if ($bad) { 'bad' } else { 'warn' }); Title = "$($susTasks.Count) tâche$(if ($susTasks.Count -gt 1) {'s'}) planifiée$(if ($susTasks.Count -gt 1) {'s'}) suspecte$(if ($susTasks.Count -gt 1) {'s'})"
            Detail = 'Ces tâches lancent en cachette des commandes qui téléchargent ou exécutent du code, ou des programmes rangés dans des dossiers temporaires. C''est une technique classique des virus pour revenir après chaque redémarrage.'
            Items = @($susTasks | ForEach-Object { "$($_.Name)  :  $($_.Cmd)" })
            Actions = @(@{ Label = 'Désactiver ces tâches'; Arg = $susTasks; Confirm = 'Désactiver ces tâches planifiées ? Tu pourras les réactiver dans le Planificateur de tâches.'
                           Script = { param($a) foreach ($x in $a) { Disable-ScheduledTask -TaskName $x.Name -TaskPath $x.Path -ErrorAction SilentlyContinue | Out-Null } } },
                        @{ Label = 'Planificateur de tâches'; Script = { Start-Process 'taskschd.msc' } }) })
    }
    Set-Status 'Vérification terminée.'
    $list
}

# ---------------------------------------------------------------------------
# Affichage de l'onglet
# ---------------------------------------------------------------------------
function New-StatusLine([string]$Label, [string]$Value, [string]$Status) {
    $g = New-Grid @('Auto', '160', '*')
    $g.Margin = New-Thickness 0 5 0 5
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 9; $dot.Height = 9; $dot.Fill = Get-Brush $Colors[$Status]
    $dot.Margin = New-Thickness 0 0 10 0; $dot.VerticalAlignment = 'Center'
    Add-ToGrid $g $dot 0
    Add-ToGrid $g (New-Text $Label 13 '#9AA3B2') 1
    Add-ToGrid $g (New-Text $Value 13 '#FFFFFF' -Semi) 2
    $g
}

function New-SecurityCard($Check) {
    $card = New-Card
    $card.Padding = New-Thickness 18 14 18 14
    $sp = New-Object System.Windows.Controls.StackPanel
    $head = New-Grid @('Auto', '*')
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 12; $dot.Height = 12; $dot.Fill = Get-Brush $Colors[$Check.Status]
    $dot.Margin = New-Thickness 0 5 12 0; $dot.VerticalAlignment = 'Top'
    Add-ToGrid $head $dot 0
    $txt = New-Object System.Windows.Controls.StackPanel
    [void]$txt.Children.Add((New-Text $Check.Title 14.5 '#FFFFFF' -Semi))
    $d = New-Text $Check.Detail 12.5 '#9AA3B2'
    $d.Margin = New-Thickness 0 3 0 0
    [void]$txt.Children.Add($d)
    Add-ToGrid $head $txt 1
    [void]$sp.Children.Add($head)
    if ($Check.Items) {
        $box = New-Object System.Windows.Controls.Border
        $box.Background = Get-Brush '#10131A'
        $box.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $box.Padding = New-Thickness 12 8 12 8
        $box.Margin = New-Thickness 24 10 0 0
        $items = New-Object System.Windows.Controls.StackPanel
        $shown = @($Check.Items | Select-Object -First 5)
        foreach ($i in $shown) {
            $t2 = New-Text ([string]$i) 12 '#C9CED8'
            $t2.TextTrimming = 'CharacterEllipsis'; $t2.TextWrapping = 'NoWrap'; $t2.ToolTip = [string]$i
            $t2.Margin = New-Thickness 0 2 0 2
            [void]$items.Children.Add($t2)
        }
        if (@($Check.Items).Count -gt 5) { [void]$items.Children.Add((New-Text "et $(@($Check.Items).Count - 5) autre(s)..." 12 '#5B6475')) }
        $box.Child = $items
        [void]$sp.Children.Add($box)
    }
    if ($Check.Actions) {
        $wp = New-Object System.Windows.Controls.WrapPanel
        $wp.Margin = New-Thickness 24 10 0 0
        $first = $true
        foreach ($a in $Check.Actions) {
            $b = New-Button $a.Label $(if ($first) { 'BtnPrimary' } else { 'BtnSecondary' })
            $b.Margin = New-Thickness 0 0 8 0
            $b.Tag = $a
            $b.Add_Click({
                param($s, $e)
                $act = $s.Tag
                if ($act.Confirm -and -not (Confirm-Action $act.Confirm)) { return }
                Invoke-Safe {
                    & $act.Script $act.Arg
                    if ($act.After) { & $act.After } elseif (-not $act.NoRefresh -and -not $script:ScanRunning) { Update-SecurityTab }
                }
            })
            [void]$wp.Children.Add($b)
            $first = $false
        }
        [void]$sp.Children.Add($wp)
    }
    $card.Child = $sp
    $card
}

function Get-ThreatHistory($S) {
    $out = @()
    try {
        $threats = @{}
        foreach ($t2 in @($S.Threats)) { $threats[[string]$t2.ThreatID] = $t2 }
        foreach ($d in @($S.Detections)) {
            $t2 = $threats[[string]$d.ThreatID]
            $state = switch ([int]$d.ThreatStatusID) { 1 { 'Détectée' } 2 { 'Nettoyée' } 3 { 'En quarantaine' } 4 { 'Supprimée' } 5 { 'Autorisée' } 6 { 'Bloquée' } default { 'Traitée' } }
            $active = $t2 -and $t2.IsActive
            $out += @{
                Name = $(if ($t2) { $t2.ThreatName } else { "Menace $($d.ThreatID)" })
                Severity = $(if ($t2) { switch ([int]$t2.SeverityID) { 1 { 'Faible' } 2 { 'Moyenne' } 4 { 'Élevée' } 5 { 'Grave' } default { 'Inconnue' } } } else { '' })
                When = $d.InitialDetectionTime
                File = (@($d.Resources) | Select-Object -First 1) -replace '^file:_', ''
                State = $(if ($active) { 'Toujours active' } else { $state })
                Active = $active
            }
        }
    } catch {}
    $out
}

function Update-SecurityTab {
    Set-Status 'Vérification de la sécurité...'
    $S = Get-ProtectionStatus
    $checks = @(Get-SecurityChecks $S)
    $score = Get-ProtectionScore $S $checks
    $mp = $S.Mp

    # Jauge et état
    $ui.SecGaugeHost.Children.Clear()
    $col = if ($score -ge 80) { $Colors.ok } elseif ($score -ge 50) { $Colors.warn } else { $Colors.bad }
    $g = New-Gauge 'Niveau de protection' $score 100 '{0:N0}' 'sur 100' $col 0
    [void]$ui.SecGaugeHost.Children.Add($g.El)
    $ui.SecLines.Children.Clear()
    if ($mp -and $mp.AMRunningMode -eq 'Normal') {
        [void]$ui.SecLines.Children.Add((New-StatusLine 'Antivirus' 'Microsoft Defender' 'ok'))
        [void]$ui.SecLines.Children.Add((New-StatusLine 'Protection en direct' $(if ($mp.RealTimeProtectionEnabled) { 'Activée' } else { 'Désactivée' }) $(if ($mp.RealTimeProtectionEnabled) { 'ok' } else { 'bad' })))
        $age = [int]$mp.AntivirusSignatureAge
        [void]$ui.SecLines.Children.Add((New-StatusLine 'Base de virus' $(if ($age -le 0) { 'À jour (aujourd''hui)' } else { "Il y a $age jour$(if ($age -gt 1) {'s'})" }) $(if ($age -le 3) { 'ok' } else { 'warn' })))
        $last = if ($mp.QuickScanEndTime) { $mp.QuickScanEndTime.ToString('dd/MM/yyyy à HH:mm') } else { 'Jamais' }
        [void]$ui.SecLines.Children.Add((New-StatusLine 'Dernière analyse' $last $(if ($mp.QuickScanAge -le 14) { 'ok' } else { 'warn' })))
    } elseif ($S.OtherAv.Count) {
        [void]$ui.SecLines.Children.Add((New-StatusLine 'Antivirus' ($S.OtherAv -join ', ') 'ok'))
        [void]$ui.SecLines.Children.Add((New-StatusLine 'Analyses' 'Faites les avec ton antivirus' 'info'))
    } else {
        [void]$ui.SecLines.Children.Add((New-StatusLine 'Antivirus' 'Aucun antivirus actif' 'bad'))
    }
    [void]$ui.SecLines.Children.Add((New-StatusLine 'Pare-feu' $(if ($S.Firewall) { 'Activé' } else { 'Désactivé' }) $(if ($S.Firewall) { 'ok' } else { 'bad' })))
    [void]$ui.SecLines.Children.Add((New-StatusLine 'Contrôle des comptes' $(if ($S.Uac) { 'Activé' } else { 'Désactivé' }) $(if ($S.Uac) { 'ok' } else { 'bad' })))
    $defenderOk = $mp -and $mp.AMRunningMode -eq 'Normal'
    foreach ($b in $script:SecButtons) { $b.IsEnabled = [bool]$defenderOk }
    $ui.SecScanNote.Text = if ($defenderOk) { 'Les analyses utilisent Microsoft Defender, l''antivirus intégré à Windows.' } elseif ($S.OtherAv.Count) { "Ton antivirus ($($S.OtherAv -join ', ')) remplace Microsoft Defender : lance les analyses depuis ton antivirus." } else { 'Active Microsoft Defender pour pouvoir lancer une analyse.' }

    # Points à vérifier
    $ui.SecChecks.Children.Clear()
    if ($S.Active.Count) {
        [void]$ui.SecChecks.Children.Add((New-SecurityCard @{ Status = 'bad'; Title = "$($S.Active.Count) menace$(if ($S.Active.Count -gt 1) {'s'}) toujours active$(if ($S.Active.Count -gt 1) {'s'})"
            Detail = 'Microsoft Defender a trouvé des virus qui ne sont pas encore supprimés.'
            Items = @($S.Active | ForEach-Object { $_.ThreatName })
            Actions = @(@{ Label = 'Supprimer les menaces'; Confirm = 'Supprimer toutes les menaces actives trouvées par Microsoft Defender ?'; Script = { Remove-MpThreat -ErrorAction Stop } }) }))
    }
    foreach ($c in ($checks | Sort-Object @{ Expression = { if ($_.Status -eq 'bad') { 0 } else { 1 } } })) { [void]$ui.SecChecks.Children.Add((New-SecurityCard $c)) }
    if (-not $checks.Count -and -not $S.Active.Count) {
        $ok = New-Card
        $ok.Padding = New-Thickness 18 14 18 14
        $row = New-Grid @('Auto', '*')
        $ic = New-Text '✓' 18 $Colors.ok -Bold
        $ic.Margin = New-Thickness 0 0 12 0
        Add-ToGrid $row $ic 0
        Add-ToGrid $row (New-Text 'Rien de suspect : pas de fichier déguisé, pas de programme caché, pas de tâche douteuse, pas de redirection.' 13.5 $Colors.ok -Semi) 1
        $ok.Child = $row
        [void]$ui.SecChecks.Children.Add($ok)
    }
    $ui.SecChecksSummary.Text = if ($checks.Count) { "$($checks.Count) point$(if ($checks.Count -gt 1) {'s'}) à vérifier" } else { 'Tout est propre' }

    # Historique
    $ui.SecHistory.Children.Clear()
    $hist = @(Get-ThreatHistory $S)
    if (-not $hist.Count) {
        [void]$ui.SecHistory.Children.Add((New-Text 'Aucune menace trouvée sur ce PC récemment.' 13 '#9AA3B2'))
    }
    foreach ($h in $hist) {
        $card = New-Card
        $card.Padding = New-Thickness 16 10 16 10
        $row = New-Grid @('*', 'Auto')
        $sp = New-Object System.Windows.Controls.StackPanel
        [void]$sp.Children.Add((New-Text "$($h.Name)" 13.5 '#FFFFFF' -Semi))
        $sub = New-Text "$($h.When.ToString('dd/MM/yyyy HH:mm'))   $($h.File)" 12 '#9AA3B2'
        $sub.TextTrimming = 'CharacterEllipsis'; $sub.TextWrapping = 'NoWrap'; $sub.ToolTip = $h.File
        [void]$sp.Children.Add($sub)
        Add-ToGrid $row $sp 0
        $chips = New-Object System.Windows.Controls.StackPanel
        $chips.Orientation = 'Horizontal'; $chips.VerticalAlignment = 'Center'
        if ($h.Severity) { [void]$chips.Children.Add((New-Badge "Gravité $($h.Severity.ToLower())" $Colors.warn)) }
        [void]$chips.Children.Add((New-Badge $h.State $(if ($h.Active) { $Colors.bad } else { $Colors.ok })))
        Add-ToGrid $row $chips 1
        $card.Child = $row
        [void]$ui.SecHistory.Children.Add($card)
    }
    $script:SecurityScore = $score
    Set-Status "Sécurité : niveau de protection $score sur 100."
}

# ---------------------------------------------------------------------------
# Analyses Microsoft Defender (dans le panneau animé)
# ---------------------------------------------------------------------------
$ScanWork = {
    param($a)
    try {
        if ($a.Type -eq 'CustomScan') {
            foreach ($p in $a.Paths) { if (Test-Path -LiteralPath $p) { Start-MpScan -ScanType CustomScan -ScanPath $p -ErrorAction Stop } }
        } else {
            Start-MpScan -ScanType $a.Type -ErrorAction Stop
        }
        @{ Ok = $true }
    } catch { @{ Error = $_.Exception.GetBaseException().Message } }
}

function New-Radar {
    $g = New-Object System.Windows.Controls.Grid
    $g.Width = 190; $g.Height = 190
    $g.HorizontalAlignment = 'Center'
    $g.Margin = New-Thickness 0 10 0 6
    foreach ($r in 90, 64, 38) {
        $e = New-Object System.Windows.Shapes.Ellipse
        $e.Width = $r * 2; $e.Height = $r * 2
        $e.Stroke = Get-Brush '#1F2633'; $e.StrokeThickness = 1.5
        [void]$g.Children.Add($e)
    }
    $sweep = New-Object System.Windows.Shapes.Path
    $sweep.Data = Get-ArcGeometry 95 88 -90 70
    $sweep.Stroke = Get-Brush $Colors.ok; $sweep.StrokeThickness = 5
    $sweep.StrokeStartLineCap = 'Round'; $sweep.StrokeEndLineCap = 'Round'
    $sweep.Effect = New-Glow $Colors.ok 18 0.9
    $sweep.CacheMode = New-Object System.Windows.Media.BitmapCache
    $sweep.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5)
    $rot = New-Object System.Windows.Media.RotateTransform
    $sweep.RenderTransform = $rot
    [void]$g.Children.Add($sweep)
    $shield = New-Object System.Windows.Shapes.Path
    $shield.Data = [System.Windows.Media.Geometry]::Parse('M 30,2 L 56,11 L 56,31 C 56,48 44,58 30,64 C 16,58 4,48 4,31 L 4,11 Z')
    $shield.Fill = Get-Brush '#1A2A22'; $shield.Stroke = Get-Brush $Colors.ok; $shield.StrokeThickness = 2.5
    $shield.Width = 60; $shield.Height = 66; $shield.Stretch = 'Fill'
    $shield.HorizontalAlignment = 'Center'; $shield.VerticalAlignment = 'Center'
    $shield.Effect = New-Glow $Colors.ok 20 0.5
    [void]$g.Children.Add($shield)
    $a = New-Object System.Windows.Media.Animation.DoubleAnimation
    $a.From = 0; $a.To = 360
    $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds(1600))
    $a.RepeatBehavior = [System.Windows.Media.Animation.RepeatBehavior]::Forever
    $rot.BeginAnimation([System.Windows.Media.RotateTransform]::AngleProperty, $a)
    Start-Pulse $shield
    @{ El = $g; Shield = $shield; Sweep = $sweep; Rot = $rot }
}

function Invoke-DefenderScan([string]$Type, [string[]]$Paths, [string]$Title) {
    if ($script:ScanRunning -or $script:TestRunning) { Show-Message 'Une analyse ou un test est déjà en cours.'; return }
    $names = @{ QuickScan = 'Analyse rapide'; FullScan = 'Analyse complète'; CustomScan = 'Analyse personnalisée' }
    if (-not $Title) { $Title = $names[$Type] }
    $script:ScanRunning = $true
    $script:TestRunning = $true
    Show-TestPanel @{ Tag = 'AV'; Title = $Title; Sub = 'Microsoft Defender' }
    Set-TestState 'run' 'Analyse en cours'
    Set-TestButtons 'run'
    $body = $ui.TestBody
    $radar = New-Radar
    [void]$body.Children.Add($radar.El)
    $time = New-Text '00:00' 40 '#FFFFFF' -Bold
    $time.HorizontalAlignment = 'Center'
    [void]$body.Children.Add($time)
    $hint = New-Text $(switch ($Type) {
        'QuickScan'  { 'Analyse des endroits où se cachent les virus. Ça prend en général quelques minutes, tu peux continuer à utiliser ton PC.' }
        'FullScan'   { 'Analyse de tous les fichiers du PC. Ça peut prendre une heure ou plus : tu peux continuer à utiliser ton PC.' }
        default      { "Analyse de $(@($Paths).Count) élément$(if (@($Paths).Count -gt 1) {'s'})." }
    }) 13 '#9AA3B2'
    $hint.HorizontalAlignment = 'Center'; $hint.TextAlignment = 'Center'
    $hint.Margin = New-Thickness 40 4 40 10
    [void]$body.Children.Add($hint)
    $ui.TestPct.Text = ''
    $start = Get-Date
    $script:ScanStart = $start
    $script:ScanClock = @{ T = $time; Start = $start }
    $script:ScanTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:ScanTimer.Interval = [TimeSpan]::FromMilliseconds(500)
    $script:ScanTimer.Add_Tick({
        $el = (Get-Date) - $script:ScanClock.Start
        $script:ScanClock.T.Text = '{0:00}:{1:00}' -f [math]::Floor($el.TotalMinutes), $el.Seconds
        $ui.TestProgress.Value = (($el.TotalSeconds * 12) % 100)
    })
    $script:ScanTimer.Start()
    foreach ($b in $script:SecButtons) { $b.IsEnabled = $false }
    [OGNative]::Cancel = $false
    try {
        $r = Invoke-Async $ScanWork @{ Type = $Type; Paths = @($Paths) } | Select-Object -First 1
    } finally {
        $script:ScanTimer.Stop()
        $script:ScanRunning = $false
        $script:TestRunning = $false
        foreach ($b in $script:SecButtons) { $b.IsEnabled = $true }
        Set-TestButtons 'done'
        $ui.BtnTestAgain.Visibility = 'Collapsed'
    }
    $radar.Rot.BeginAnimation([System.Windows.Media.RotateTransform]::AngleProperty, $null)
    Stop-Pulse $radar.Shield
    $radar.Sweep.Visibility = 'Collapsed'
    $dur = (Get-Date) - $start
    $ui.TestProgress.Value = 100
    $body.Children.Remove($hint)
    if ([OGNative]::Cancel) {
        Set-TestState 'info' 'Arrêtée'
        [void]$body.Children.Add((New-Verdict 'info' 'Analyse arrêtée avant la fin.'))
        Update-SecurityTab
        return
    }
    if ($r.Error) {
        Set-TestState 'bad' 'Échec'
        [void]$body.Children.Add((New-Verdict 'bad' "L'analyse n'a pas pu se faire : $($r.Error)"))
        return
    }
    $found = @()
    try { $found = @(Get-MpThreatDetection -ErrorAction Stop | Where-Object { $_.InitialDetectionTime -ge $start.AddSeconds(-5) }) } catch {}
    $threatNames = @{}
    try { foreach ($t2 in @(Get-MpThreat -ErrorAction Stop)) { $threatNames[[string]$t2.ThreatID] = $t2.ThreatName } } catch {}
    $time.Text = '{0:00}:{1:00}' -f [math]::Floor($dur.TotalMinutes), $dur.Seconds
    if (-not $found.Count) {
        $radar.Shield.Fill = Get-Brush $Colors.ok
        $check = New-Text '✓' 30 '#0B0D10' -Bold
        $check.HorizontalAlignment = 'Center'; $check.VerticalAlignment = 'Center'
        [void]$radar.El.Children.Add($check)
        $scale = New-Object System.Windows.Media.ScaleTransform 0.6, 0.6
        $radar.Shield.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5)
        $radar.Shield.RenderTransform = $scale
        Start-WpfAnim $scale ([System.Windows.Media.ScaleTransform]::ScaleXProperty) 1 600
        Start-WpfAnim $scale ([System.Windows.Media.ScaleTransform]::ScaleYProperty) 1 600
        Set-TestState 'ok' 'Aucune menace'
        [void]$body.Children.Add((New-Verdict 'ok' "Aucune menace trouvée. Analyse terminée en " + $(if ($dur.TotalSeconds -lt 60) { "moins d'une minute." } else { "$([int][math]::Round($dur.TotalMinutes)) min." })))
    } else {
        $radar.Shield.Stroke = Get-Brush $Colors.bad
        $radar.Shield.Fill = Get-Brush '#3A1A1A'
        $radar.Shield.Effect = New-Glow $Colors.bad 20 0.6
        Set-TestState 'bad' "$($found.Count) menace$(if ($found.Count -gt 1) {'s'}) trouvée$(if ($found.Count -gt 1) {'s'})"
        foreach ($d in $found) {
            $name = $threatNames[[string]$d.ThreatID]
            $file = (@($d.Resources) | Select-Object -First 1) -replace '^file:_', ''
            [void]$body.Children.Add((New-Verdict 'bad' "$name   $file"))
        }
        $del = New-Button 'Supprimer les menaces' 'BtnPrimary'
        $del.HorizontalAlignment = 'Left'
        $del.Margin = New-Thickness 0 14 0 0
        $del.Add_Click({
            if (-not (Confirm-Action 'Supprimer toutes les menaces trouvées par Microsoft Defender ?')) { return }
            Invoke-Safe { Remove-MpThreat -ErrorAction Stop; Show-Message 'Menaces supprimées.'; Update-SecurityTab }
        })
        [void]$body.Children.Add($del)
    }
    Update-SecurityTab
}

function Update-Definitions {
    Set-Busy $true
    Set-Status 'Mise à jour de la base de virus...'
    $r = Invoke-Async { try { Update-MpSignature -ErrorAction Stop; 'OK' } catch { $_.Exception.GetBaseException().Message } } | Select-Object -First 1
    if ("$r" -eq 'OK') { Set-Status 'Base de virus à jour.' } else { Show-Message "La mise à jour n'a pas pu se faire :`n`n$r" 'Warning' }
    Update-SecurityTab
}

function Invoke-FolderScan {
    $dlg = New-Object System.Windows.Forms.FolderBrowserDialog
    $dlg.Description = 'Choisis le dossier à analyser'
    $dlg.ShowNewFolderButton = $false
    if ($dlg.ShowDialog() -ne 'OK') { return }
    Invoke-DefenderScan 'CustomScan' @($dlg.SelectedPath) "Analyse de $(Split-Path $dlg.SelectedPath -Leaf)"
}

# ---------------------------------------------------------------------------
# Navigation : accueil « Ordinateur » avec une carte par fonction
# ---------------------------------------------------------------------------
$HubIndex = 8
$NetIndex = 9
$PageNames = @{ 0 = 'Tableau de bord'; 1 = 'Optimisation gaming'; 2 = 'Démarrage'; 3 = 'Connexion'; 4 = 'Nettoyage'; 5 = 'Tests'; 6 = 'Sécurité'; 7 = 'Sauvegarde' }
$HubPages = @(
    @{ Index = 0; Glyph = 0xE80F; Title = 'Tableau de bord'; Desc = 'Santé des composants, score et ce qui peut être amélioré.'; Color = '#22D37A' },
    @{ Index = 1; Glyph = 0xE7FC; Title = 'Optimisation gaming'; Desc = 'Les réglages de Windows qui font gagner des FPS.'; Color = '#B18CFF' },
    @{ Index = 5; Glyph = 0xE9D9; Title = 'Tests'; Desc = 'Vitesse et santé de chaque composant.'; Color = '#4EA8FF' },
    @{ Index = 6; Glyph = 0xE72E; Title = 'Sécurité'; Desc = 'Antivirus et recherche de tout ce qui est suspect.'; Color = '#22D37A' },
    @{ Index = 2; Glyph = 0xE7E8; Title = 'Démarrage'; Desc = 'Les programmes qui se lancent avec Windows.'; Color = '#F5A524' },
    @{ Index = 3; Glyph = 0xE774; Title = 'Connexion'; Desc = 'Ping, stabilité de la connexion et serveur DNS.'; Color = '#4EA8FF' },
    @{ Index = 4; Glyph = 0xE74D; Title = 'Nettoyage'; Desc = 'Libère de la place sur le disque.'; Color = '#FF7AB6' },
    @{ Index = 7; Glyph = 0xE777; Title = 'Sauvegarde'; Desc = 'Tout annuler, rapport du PC et mises à jour.'; Color = '#9AA3B2' }
)

function Show-Page([int]$Index) { $ui.Tabs.SelectedIndex = $Index }

function Update-NavBar {
    $i = $ui.Tabs.SelectedIndex
    $sub = $i -ge 0 -and $i -lt $HubIndex
    $ui.Tabs.Items[$HubIndex].Tag = if ($sub) { 'parent' } else { $null }
    if ($script:NavBar) {
        $script:NavBar.Visibility = if ($sub) { 'Visible' } else { 'Collapsed' }
        if ($sub) { $script:NavCrumb.Text = $PageNames[$i] }
    }
}

function Build-Hub {
    $ui.HubCards.Children.Clear()
    $script:HubStats = @{}
    $n = 0
    foreach ($pg in $HubPages) {
        $card = New-Object System.Windows.Controls.Border
        $card.Background = Get-Brush '#181C24'
        $card.BorderBrush = Get-Brush '#232937'
        $card.BorderThickness = New-Thickness 1 1 1 1
        $card.CornerRadius = [System.Windows.CornerRadius]::new(14)
        $card.Padding = New-Thickness 18 16 18 16
        $card.Margin = New-Thickness 0 0 12 12
        $card.Cursor = [System.Windows.Input.Cursors]::Hand
        $move = New-Object System.Windows.Media.TranslateTransform
        $card.RenderTransform = $move
        $sp = New-Object System.Windows.Controls.StackPanel
        $head = New-Grid @('Auto', '*', 'Auto')
        $ic = New-Object System.Windows.Controls.Border
        $ic.Width = 46; $ic.Height = 46
        $ic.CornerRadius = [System.Windows.CornerRadius]::new(12)
        $bg = Get-Brush $pg.Color; $bg.Opacity = 0.15
        $ic.Background = $bg
        $gl = New-Object System.Windows.Controls.TextBlock
        $gl.Text = [string][char]$pg.Glyph
        $gl.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
        $gl.FontSize = 20
        $gl.Foreground = Get-Brush $pg.Color
        $gl.HorizontalAlignment = 'Center'; $gl.VerticalAlignment = 'Center'
        $ic.Child = $gl
        Add-ToGrid $head $ic 0
        $chev = New-Text '›' 24 '#5B6475' -Bold
        $chev.VerticalAlignment = 'Center'
        Add-ToGrid $head $chev 2
        [void]$sp.Children.Add($head)
        $t1 = New-Text $pg.Title 16 '#FFFFFF' -Semi
        $t1.Margin = New-Thickness 0 12 0 0
        [void]$sp.Children.Add($t1)
        $d = New-Text $pg.Desc 12.5 '#9AA3B2'
        $d.Margin = New-Thickness 0 3 0 0
        $d.MinHeight = 34
        [void]$sp.Children.Add($d)
        $stat = New-Text ' ' 13 $pg.Color -Semi
        $stat.Margin = New-Thickness 0 10 0 0
        [void]$sp.Children.Add($stat)
        $card.Child = $sp
        $card.Tag = @{ Index = $pg.Index; Color = $pg.Color; Move = $move; Chev = $chev }
        $card.Add_MouseEnter({
            param($s, $e)
            $s.BorderBrush = Get-Brush $s.Tag.Color
            $s.Background = Get-Brush '#1C212B'
            $s.Tag.Chev.Foreground = Get-Brush $s.Tag.Color
            Start-WpfAnim $s.Tag.Move ([System.Windows.Media.TranslateTransform]::YProperty) -3 180
        })
        $card.Add_MouseLeave({
            param($s, $e)
            $s.BorderBrush = Get-Brush '#232937'
            $s.Background = Get-Brush '#181C24'
            $s.Tag.Chev.Foreground = Get-Brush '#5B6475'
            Start-WpfAnim $s.Tag.Move ([System.Windows.Media.TranslateTransform]::YProperty) 0 180
        })
        $card.Add_MouseLeftButtonUp({ param($s, $e) Show-Page $s.Tag.Index })
        $card.Opacity = 0
        Start-WpfAnim $card ([System.Windows.UIElement]::OpacityProperty) 1 400 (60 * $n)
        [void]$ui.HubCards.Children.Add($card)
        $script:HubStats[$pg.Index] = $stat
        $n++
    }
}

function Update-Hub {
    $a = $script:LastAnalysis
    $info = if ($a) { $a.Info } else { @{} }
    $ui.HubSub.Text = "$env:COMPUTERNAME" + $(if ($info['Windows']) { "   /   $($info['Windows'])" } else { '' })

    # Jauges
    $ui.HubGaugeOpt.Children.Clear(); $ui.HubGaugeSec.Children.Clear()
    if ($a) {
        $col = if ($a.Score -ge 85) { $Colors.ok } elseif ($a.Score -ge 65) { '#9BE15D' } elseif ($a.Score -ge 45) { $Colors.warn } else { $Colors.bad }
        [void]$ui.HubGaugeOpt.Children.Add((New-Gauge 'Optimisation' $a.Score 100 '{0:N0}' 'sur 100' $col 0).El)
    }
    if ($null -ne $script:SecurityScore) {
        $s = $script:SecurityScore
        $col = if ($s -ge 80) { $Colors.ok } elseif ($s -ge 50) { $Colors.warn } else { $Colors.bad }
        [void]$ui.HubGaugeSec.Children.Add((New-Gauge 'Protection' $s 100 '{0:N0}' 'sur 100' $col 150).El)
    }

    # Résumé du PC
    $ui.HubSummary.Children.Clear()
    foreach ($k in 'Processeur', 'Carte graphique', 'Mémoire', 'Disque système', 'Réseau') {
        if ($info[$k]) {
            $g = New-Grid @('130', '*')
            $g.Margin = New-Thickness 0 4 0 4
            Add-ToGrid $g (New-Text $k 12.5 '#9AA3B2') 0
            $v = New-Text ([string]$info[$k]) 12.5 '#E6E8EE' -Semi
            $v.TextTrimming = 'CharacterEllipsis'; $v.TextWrapping = 'NoWrap'
            Add-ToGrid $g $v 1
            [void]$ui.HubSummary.Children.Add($g)
        }
    }

    # Infos en direct sur chaque carte
    $st = $script:HubStats
    if (-not $st) { return }
    if ($a) {
        $st[0].Text = "Score $($a.Score) sur 100"
        $todo = @($a.Active | Where-Object { $_.Id -like 'tweak:*' -and $_.Status -eq 'warn' }).Count
        $st[1].Text = if ($todo) { "$todo réglage$(if ($todo -gt 1) {'s'}) à faire" } else { 'Tout est optimisé' }
    } else { $st[0].Text = 'Analyse en cours...'; $st[1].Text = ' ' }
    $tested = @($ui.TestsPanel.Children | Where-Object { $_.Child -and $_.Child.Children.Count -gt 2 -and $_.Child.Children[2].Children.Count -and -not ($_.Child.Children[2].Children[0] -is [System.Windows.Controls.TextBlock]) }).Count
    $st[5].Text = if ($tested) { "$tested composant$(if ($tested -gt 1) {'s'}) testé$(if ($tested -gt 1) {'s'})" } else { 'Aucun test pour le moment' }
    $st[6].Text = if ($null -ne $script:SecurityScore) { "Protection $($script:SecurityScore) sur 100" } else { 'Clique pour vérifier' }
    $on = @($script:StartupEntries | Where-Object { $_.Item.Enabled }).Count
    $st[2].Text = "$on programme$(if ($on -gt 1) {'s'}) au démarrage"
    $ping = @($script:PingResults | Where-Object { $_.Label -like 'Internet*' } | Select-Object -First 1)
    $st[3].Text = if ($ping.Count) { "Ping $($ping[0].Avg) ms" } else { 'Tester ma connexion' }
    $st[4].Text = 'Clique pour analyser'
    $n = Get-BackupCount
    $st[7].Text = if ($n) { "$n réglage$(if ($n -gt 1) {'s'}) modifié$(if ($n -gt 1) {'s'})" } else { "Version $AppVersion" }
}

# ---------------------------------------------------------------------------
# Section Réseau : scan des appareils connectés
# ---------------------------------------------------------------------------
$OuiFile = Join-Path $DataDir 'fabricants.txt'
$KnownFile = Join-Path $DataDir 'appareils.json'
$AuditFile = Join-Path $DataDir 'audit.json'

$NetScanWork = {
    param($a)
    try {
        [OGNative]::Phase = 'ping'
        $alive = @([OGNative]::PingSweep([string[]]$a.Ips, 800))
        if ([OGNative]::Cancel) { return @{ Cancelled = $true } }
        [OGNative]::Phase = 'arp'
        [OGNative]::Progress = 72
        Start-Sleep -Milliseconds 300
        # Seulement les vrais appareils du réseau (pas les adresses techniques multicast / broadcast)
        $inNet = @{}; foreach ($x in $a.Ips) { $inNet[$x] = $true }
        $arp = @(Get-NetNeighbor -InterfaceIndex $a.If -AddressFamily IPv4 -ErrorAction SilentlyContinue |
            Where-Object { $inNet.ContainsKey([string]$_.IPAddress) -and $_.State -notin 'Unreachable', 'Incomplete', 'Permanent' -and $_.LinkLayerAddress -and $_.LinkLayerAddress -notmatch '^(00-00-00-00-00-00|FF-FF-FF-FF-FF-FF|01-00-5E.*)$' } |
            ForEach-Object { "$($_.IPAddress)|$($_.LinkLayerAddress)|$($_.State)" })
        [OGNative]::Phase = 'names'
        [OGNative]::Progress = 80
        $ips = @(@($alive | ForEach-Object { ($_ -split '\|')[0] }) + @($arp | ForEach-Object { ($_ -split '\|')[0] }) | Select-Object -Unique)
        $names = @([OGNative]::ResolveNames([string[]]$ips, 2500))
        [OGNative]::Phase = 'vendors'
        [OGNative]::Progress = 92
        if (-not (Test-Path -LiteralPath $a.Oui)) {
            try {
                [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
                $tmp = "$($a.Oui).csv"
                Invoke-WebRequest 'https://standards-oui.ieee.org/oui/oui.csv' -OutFile $tmp -UseBasicParsing -Headers @{ 'User-Agent' = 'Mozilla/5.0 OptiGame' } -TimeoutSec 60
                Import-Csv -LiteralPath $tmp | ForEach-Object { "$($_.Assignment)|$($_.'Organization Name')" } | Set-Content -LiteralPath $a.Oui -Encoding UTF8
                Remove-Item -LiteralPath $tmp -ErrorAction SilentlyContinue
            } catch {}
        }
        $ouiMap = $null
        if ($a.LoadOui -and (Test-Path -LiteralPath $a.Oui)) {
            $ouiMap = @{}
            foreach ($l in [IO.File]::ReadLines($a.Oui)) { $i = $l.IndexOf('|'); if ($i -gt 0) { $ouiMap[$l.Substring(0, $i).Trim([char]0xFEFF)] = $l.Substring($i + 1) } }
        }
        [OGNative]::Progress = 100
        @{ Alive = $alive; Arp = $arp; Names = $names; OuiMap = $ouiMap }
    } catch { @{ Error = $_.Exception.GetBaseException().Message } }
}

function Get-SubnetIps([string]$Ip, [int]$Prefix) {
    if ($Prefix -lt 24) { $Prefix = 24 }
    $b = ([Net.IPAddress]::Parse($Ip)).GetAddressBytes(); [array]::Reverse($b)
    $n = [double][BitConverter]::ToUInt32($b, 0)
    $size = [math]::Pow(2, 32 - $Prefix)
    $net = [math]::Floor($n / $size) * $size
    for ($i = 1; $i -lt $size - 1; $i++) {
        $bb = [BitConverter]::GetBytes([uint32]($net + $i)); [array]::Reverse($bb)
        ([Net.IPAddress]::new($bb)).ToString()
    }
}

function Get-Vendor([string]$Mac) {
    if (-not $Mac) { return '' }
    $hex = ($Mac -replace '[-:]', '').ToUpper()
    if ($hex.Length -lt 6) { return '' }
    if ([Convert]::ToInt32($hex.Substring(1, 1), 16) -band 2) { return 'Adresse privée' }
    if (-not $script:Oui -and (Test-Path -LiteralPath $OuiFile)) {
        $script:Oui = @{}
        foreach ($l in [IO.File]::ReadLines($OuiFile)) { $i = $l.IndexOf('|'); if ($i -gt 0) { $script:Oui[$l.Substring(0, $i).Trim([char]0xFEFF)] = $l.Substring($i + 1) } }
    }
    if (-not $script:Oui) { return '' }
    $v = [string]$script:Oui[$hex.Substring(0, 6)]
    $v = $v -replace '(?i)[,\s]+(inc|incorporated|co|ltd|corporation|corp|gmbh|s\.?a\.?s|sarl|s\.a|limited|llc|b\.v|ag|oy|ab)\b\.?', ''
    $v = $v -replace '(?i)\s+(technologies|technology|electronics|communications|broadband)\b', ''
    $v.Trim(' ', ',', '.')
}

function Get-DeviceKind($D) {
    $t = "$($D.Host) $($D.Vendor)"
    if ($D.Self) { return @{ Kind = 'Ce PC'; Glyph = 0xE7F4; Color = $Colors.info } }
    if ($D.Gateway) { return @{ Kind = 'Box Internet'; Glyph = 0xE80F; Color = $Colors.ok } }
    if ($t -match '(?i)\brt-|router|routeur|archer|\bdeco\b|orbi|mesh|access.?point|repeater|répéteur|ubiquiti|unifi') { return @{ Kind = 'Routeur ou répéteur Wi-Fi'; Glyph = 0xE774; Color = $Colors.ok } }
    if ($t -match '(?i)iphone|ipad|android|galaxy|pixel|redmi|oneplus|oppo|honor|phone|motorola|poco') { return @{ Kind = 'Téléphone ou tablette'; Glyph = 0xE8EA; Color = '#B18CFF' } }
    if ($t -match '(?i)playstation|\bps[345]\b|sony interactive|nintendo|xbox|switch') { return @{ Kind = 'Console de jeu'; Glyph = 0xE7FC; Color = '#FF7AB6' } }
    if ($t -match '(?i)webos|\btv\b|tizen|bravia|androidtv|chromecast|roku|fire.?tv|lg innotek|hisense|\btcl\b') { return @{ Kind = 'TV ou multimédia'; Glyph = 0xE7F4; Color = $Colors.warn } }
    if ($t -match '(?i)printer|imprimante|hewlett|\bhp\b|canon|epson|brother|lexmark|kyocera') { return @{ Kind = 'Imprimante'; Glyph = 0xE749; Color = '#9AA3B2' } }
    if ($t -match '(?i)sagemcom|sercomm|arcadyan|technicolor|freebox|livebox|bbox|decodeur|décodeur') { return @{ Kind = 'Box ou décodeur TV'; Glyph = 0xE80F; Color = $Colors.ok } }
    if ($t -match '(?i)espressif|tuya|shelly|sonoff|signify|philips lighting|amazon|google|nest|ring|meross|netatmo|tapo|xiaomi') { return @{ Kind = 'Objet connecté'; Glyph = 0xE80F; Color = '#4EA8FF' } }
    if ($t -match '(?i)\bapple\b') { return @{ Kind = 'Appareil Apple'; Glyph = 0xE8EA; Color = '#B18CFF' } }
    if ($t -match '(?i)desktop|laptop|\bpc|asustek|micro-star|gigabyte|dell|lenovo|acer|intel|realtek|killer') { return @{ Kind = 'Ordinateur'; Glyph = 0xE7F4; Color = $Colors.info } }
    if ($D.Vendor -eq 'Adresse privée') { return @{ Kind = 'Téléphone probable'; Glyph = 0xE8EA; Color = '#B18CFF' } }
    @{ Kind = 'Appareil'; Glyph = 0xE774; Color = '#9AA3B2' }
}

function New-NetRadar([switch]$Spin) {
    $g = New-Object System.Windows.Controls.Grid
    $g.Width = 200; $g.Height = 200
    $g.HorizontalAlignment = 'Center'
    foreach ($r in 96, 68, 40) {
        $e = New-Object System.Windows.Shapes.Ellipse
        $e.Width = $r * 2; $e.Height = $r * 2
        $e.Stroke = Get-Brush '#1F2633'; $e.StrokeThickness = 1.5
        [void]$g.Children.Add($e)
    }
    $dots = New-Object System.Windows.Controls.Canvas
    $dots.Width = 200; $dots.Height = 200
    [void]$g.Children.Add($dots)
    $sweep = New-Object System.Windows.Shapes.Path
    $sweep.Data = Get-ArcGeometry 100 94 -90 70
    $sweep.Stroke = Get-Brush $Colors.info; $sweep.StrokeThickness = 5
    $sweep.StrokeStartLineCap = 'Round'; $sweep.StrokeEndLineCap = 'Round'
    $sweep.Effect = New-Glow $Colors.info 18 0.9
    $sweep.CacheMode = New-Object System.Windows.Media.BitmapCache
    $sweep.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5)
    $rot = New-Object System.Windows.Media.RotateTransform
    $sweep.RenderTransform = $rot
    $sweep.Visibility = if ($Spin) { 'Visible' } else { 'Collapsed' }
    [void]$g.Children.Add($sweep)
    $center = New-Object System.Windows.Controls.Border
    $center.Width = 64; $center.Height = 64
    $center.CornerRadius = [System.Windows.CornerRadius]::new(32)
    $center.Background = Get-Brush '#1A2A40'
    $center.BorderBrush = Get-Brush $Colors.info; $center.BorderThickness = New-Thickness 2 2 2 2
    $center.Effect = New-Glow $Colors.info 20 0.5
    $num = New-Text '' 22 '#FFFFFF' -Bold
    $num.HorizontalAlignment = 'Center'; $num.VerticalAlignment = 'Center'
    $icon = New-Object System.Windows.Controls.TextBlock
    $icon.Text = [string][char]0xE774
    $icon.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
    $icon.FontSize = 26; $icon.Foreground = Get-Brush $Colors.info
    $icon.HorizontalAlignment = 'Center'; $icon.VerticalAlignment = 'Center'
    $inner = New-Object System.Windows.Controls.Grid
    [void]$inner.Children.Add($icon)
    [void]$inner.Children.Add($num)
    $center.Child = $inner
    $center.HorizontalAlignment = 'Center'; $center.VerticalAlignment = 'Center'
    [void]$g.Children.Add($center)
    if ($Spin) {
        $a = New-Object System.Windows.Media.Animation.DoubleAnimation
        $a.From = 0; $a.To = 360
        $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds(1400))
        $a.RepeatBehavior = [System.Windows.Media.Animation.RepeatBehavior]::Forever
        $rot.BeginAnimation([System.Windows.Media.RotateTransform]::AngleProperty, $a)
    }
    @{ El = $g; Dots = $dots; Rot = $rot; Sweep = $sweep; Num = $num; Icon = $icon; Shown = 0 }
}

# Ajoute un point lumineux sur le radar pour chaque appareil trouvé.
$script:Rnd = New-Object System.Random
function Add-RadarDot($Radar) {
    $rnd = $script:Rnd
    $ang = $rnd.NextDouble() * 2 * [math]::PI
    $dist = 45 + $rnd.NextDouble() * 45
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 9; $dot.Height = 9
    $dot.Fill = Get-Brush '#FFFFFF'
    $dot.Effect = New-Glow $Colors.info 14 1
    [System.Windows.Controls.Canvas]::SetLeft($dot, 100 + $dist * [math]::Cos($ang) - 4.5)
    [System.Windows.Controls.Canvas]::SetTop($dot, 100 + $dist * [math]::Sin($ang) - 4.5)
    $dot.Opacity = 0
    [void]$Radar.Dots.Children.Add($dot)
    Start-WpfAnim $dot ([System.Windows.UIElement]::OpacityProperty) 1 400
}

function Update-NetScanInfo {
    $ui.NetScanInfo.Children.Clear()
    $net = Get-ActiveNet
    if (-not $net) { [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Connexion' 'Aucune' 'bad')); return }
    $ipInfo = if ($net.Ip) { @{ IPAddress = $net.Ip; PrefixLength = [int]$net.Prefix } } else { $null }
    if ($net.Wifi) {
        $w = netsh wlan show interfaces 2>$null
        $ssid = ($w | Where-Object { $_ -match '^\s+SSID\s+:\s+(.+)$' } | Select-Object -First 1) -replace '^\s+SSID\s+:\s+', ''
        $sig = ($w | Where-Object { $_ -match '^\s+Signal\s+:\s+(\d+)' } | Select-Object -First 1) -replace '^\s+Signal\s+:\s+', ''
        [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Connexion' "Wi-Fi « $ssid »  ($sig)" $(if ([int]($sig -replace '\D', '') -ge 60) { 'ok' } else { 'warn' })))
    } else {
        [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Connexion' "Câble Ethernet, $($net.Speed)" 'ok'))
    }
    if ($ipInfo) {
        [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Adresse de ce PC' $ipInfo.IPAddress 'info'))
        $count = [math]::Pow(2, 32 - [math]::Max(24.0, $ipInfo.PrefixLength)) - 2
        [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Taille du réseau' "$count adresses possibles" 'info'))
    }
    [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Box' $net.Gateway 'ok'))
    $pub = Invoke-Async { try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; [string](Invoke-RestMethod 'https://api.ipify.org' -TimeoutSec 5) } catch { '' } } | Select-Object -First 1
    if ("$pub") { [void]$ui.NetScanInfo.Children.Add((New-StatusLine 'Adresse Internet' "$pub" 'info')) }
}

function Show-NetHeroIdle {
    $ui.NetHero.Children.Clear()
    $r = New-NetRadar
    [void]$ui.NetHero.Children.Add($r.El)
    $t = New-Text 'Prêt à scanner ton réseau' 14 '#FFFFFF' -Semi
    $t.HorizontalAlignment = 'Center'; $t.Margin = New-Thickness 0 12 0 0
    [void]$ui.NetHero.Children.Add($t)
    $h = New-Text 'Clique sur « Scanner le réseau » en haut à droite.' 12.5 '#9AA3B2'
    $h.HorizontalAlignment = 'Center'; $h.Margin = New-Thickness 0 4 0 0
    [void]$ui.NetHero.Children.Add($h)
}

function New-DeviceTile($D, [int]$Index) {
    $card = New-Card
    $card.Padding = New-Thickness 16 14 16 14
    $card.Margin = New-Thickness 0 0 12 12
    $g = New-Grid @('Auto', '*')
    $k = $D.KindInfo
    $ic = New-Object System.Windows.Controls.Border
    $ic.Width = 46; $ic.Height = 46
    $ic.CornerRadius = [System.Windows.CornerRadius]::new(12)
    $bg = Get-Brush $k.Color; $bg.Opacity = 0.15
    $ic.Background = $bg
    $ic.VerticalAlignment = 'Top'
    $gl = New-Object System.Windows.Controls.TextBlock
    $gl.Text = [string][char]$k.Glyph
    $gl.FontFamily = New-Object System.Windows.Media.FontFamily 'Segoe Fluent Icons, Segoe MDL2 Assets'
    $gl.FontSize = 20; $gl.Foreground = Get-Brush $k.Color
    $gl.HorizontalAlignment = 'Center'; $gl.VerticalAlignment = 'Center'
    $ic.Child = $gl
    Add-ToGrid $g $ic 0
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Margin = New-Thickness 12 0 0 0
    $name = New-Text $D.Title 14.5 '#FFFFFF' -Semi
    $name.TextTrimming = 'CharacterEllipsis'; $name.TextWrapping = 'NoWrap'; $name.ToolTip = $D.Title
    [void]$sp.Children.Add($name)
    [void]$sp.Children.Add((New-Text $k.Kind 12 '#9AA3B2'))
    $badges = New-Object System.Windows.Controls.WrapPanel
    $badges.Margin = New-Thickness -10 6 0 0
    if ($D.Self) { [void]$badges.Children.Add((New-Badge 'Ce PC' $Colors.info)) }
    if ($D.Gateway) { [void]$badges.Children.Add((New-Badge 'Ta box' $Colors.ok)) }
    if ($D.New) { [void]$badges.Children.Add((New-Badge 'Nouveau' $Colors.warn)) }
    if ($null -ne $D.Ms) { [void]$badges.Children.Add((New-Badge $(if ($D.Ms -lt 1) { '< 1 ms' } else { "$($D.Ms) ms" }) '#9AA3B2')) }
    if ($badges.Children.Count) { [void]$sp.Children.Add($badges) }
    $det = New-Text "$($D.Ip)$(if ($D.Vendor) { '   ' + $D.Vendor })" 11.5 '#5B6475'
    $det.Margin = New-Thickness 0 8 0 0
    $det.TextTrimming = 'CharacterEllipsis'; $det.TextWrapping = 'NoWrap'
    $det.ToolTip = "Adresse : $($D.Ip)`nAdresse physique : $($D.Mac)`nFabricant : $($D.Vendor)"
    [void]$sp.Children.Add($det)
    if ($D.Gateway) {
        $b = New-Button 'Ouvrir la box'
        $b.HorizontalAlignment = 'Left'
        $b.Margin = New-Thickness 0 10 0 0
        $b.Tag = "http://$($D.Ip)"
        $b.Add_Click({ param($s, $e) Start-Process $s.Tag })
        [void]$sp.Children.Add($b)
    }
    Add-ToGrid $g $sp 1
    $card.Child = $g
    if ($D.New) { $card.BorderBrush = Get-Brush $Colors.warn }
    $card.Cursor = [System.Windows.Input.Cursors]::Hand
    $card.Tag = $D
    $card.Add_MouseEnter({ param($s, $e) $s.BorderBrush = Get-Brush $s.Tag.KindInfo.Color; $s.Background = Get-Brush '#1C212B' })
    $card.Add_MouseLeave({ param($s, $e) $s.BorderBrush = Get-Brush $(if ($s.Tag.New) { $Colors.warn } else { '#232937' }); $s.Background = Get-Brush '#181C24' })
    $card.Add_MouseLeftButtonUp({ param($s, $e) Invoke-Safe { Show-DeviceDetail $s.Tag } })
    $card.Opacity = 0
    $move = New-Object System.Windows.Media.TranslateTransform 0, 12
    $card.RenderTransform = $move
    Start-WpfAnim $card ([System.Windows.UIElement]::OpacityProperty) 1 450 (70 * $Index)
    Start-WpfAnim $move ([System.Windows.Media.TranslateTransform]::YProperty) 0 450 (70 * $Index)
    $card
}

function Invoke-NetworkScan {
    if ($script:NetScanning) { return }
    $net = Get-ActiveNet
    if (-not $net) { Show-Message 'Aucune connexion réseau détectée.'; return }
    $ipInfo = if ($net.Ip) { @{ IPAddress = $net.Ip; PrefixLength = [int]$net.Prefix } } else { $null }
    if (-not $ipInfo) { Show-Message 'Impossible de lire l''adresse de ce PC.'; return }
    $ips = @(Get-SubnetIps $ipInfo.IPAddress $ipInfo.PrefixLength)
    $script:NetScanning = $true
    $ui.BtnNetScan.IsEnabled = $false
    Set-Status 'Scan du réseau...'

    $ui.NetHero.Children.Clear()
    $radar = New-NetRadar -Spin
    $radar.Icon.Visibility = 'Collapsed'
    $radar.Num.Text = '0'
    [void]$ui.NetHero.Children.Add($radar.El)
    $phase = New-Text 'Recherche des appareils...' 13 '#9AA3B2' -Semi
    $phase.HorizontalAlignment = 'Center'; $phase.Margin = New-Thickness 0 12 0 8
    [void]$ui.NetHero.Children.Add($phase)
    $bar = New-Object System.Windows.Controls.ProgressBar
    $bar.Width = 220; $bar.Height = 5
    [void]$ui.NetHero.Children.Add($bar)

    [OGNative]::Cancel = $false; [OGNative]::Found = 0; [OGNative]::Progress = 0; [OGNative]::Phase = ''
    $script:NetScanUi = @{ Radar = $radar; Phase = $phase; Bar = $bar }
    $script:NetTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:NetTimer.Interval = [TimeSpan]::FromMilliseconds(120)
    $script:NetTimer.Add_Tick({
        $u = $script:NetScanUi
        $f = [OGNative]::Found
        while ($u.Radar.Shown -lt $f) { Add-RadarDot $u.Radar; $u.Radar.Shown++ }
        $u.Radar.Num.Text = "$f"
        $u.Bar.Value = [OGNative]::Progress
        $u.Phase.Text = switch ([OGNative]::Phase) { 'ping' { 'Recherche des appareils...' } 'arp' { 'Recherche des appareils discrets...' } 'names' { 'Récupération des noms...' } 'vendors' { 'Identification des fabricants...' } default { 'Préparation...' } }
    })
    $script:NetTimer.Start()
    try {
        $r = Invoke-Async $NetScanWork @{ Ips = $ips; If = $net.IfIndex; Oui = $OuiFile; LoadOui = (-not $script:Oui) } | Select-Object -First 1
        if ($r -and $r.OuiMap) { $script:Oui = $r.OuiMap }
    } finally {
        $script:NetTimer.Stop()
        $script:NetScanning = $false
        $ui.BtnNetScan.IsEnabled = $true
    }
    if (-not $r -or $r.Error) {
        Show-NetHeroIdle
        Show-Message "Le scan n'a pas pu se faire : $($r.Error)" 'Warning'
        return
    }

    # Assemblage des appareils
    $inNet = @{}; foreach ($x in $ips) { $inNet[$x] = $true }
    $devs = @{}
    foreach ($l in @($r.Alive)) { $p = ([string]$l) -split '\|'; $devs[$p[0]] = @{ Ip = $p[0]; Ms = [int]$p[1]; Mac = $null } }
    foreach ($l in @($r.Arp)) {
        $p = ([string]$l) -split '\|'
        if (-not $inNet.ContainsKey($p[0])) { continue }
        if ($devs.ContainsKey($p[0])) { $devs[$p[0]].Mac = $p[1] }
        elseif ($p[2] -in 'Reachable', 'Stale', 'Delay', 'Probe') { $devs[$p[0]] = @{ Ip = $p[0]; Ms = $null; Mac = $p[1] } }
    }
    $self = $ipInfo.IPAddress
    $selfMac = $net.Mac
    if (-not $devs.ContainsKey($self)) { $devs[$self] = @{ Ip = $self; Ms = 0; Mac = $null } }
    $devs[$self].Mac = $selfMac
    $names = @{}
    foreach ($l in @($r.Names)) { $p = ([string]$l) -split '\|', 2; if ($p[1] -and $p[1] -ne $p[0]) { $names[$p[0]] = ($p[1] -replace '(?i)\.(home|lan|local|localdomain|box|fritz\.box|station|bbox)$', '') } }

    # Appareils déjà vus, avec la date de leur première apparition
    $knownMap = @{}
    if (Test-Path -LiteralPath $KnownFile) {
        try {
            $j = ConvertFrom-Json (Get-Content -LiteralPath $KnownFile -Raw -Encoding UTF8)
            if ($j -is [string]) { $knownMap[$j] = '' }
            elseif ($j -is [array]) { foreach ($m in $j) { $knownMap[[string]$m] = '' } }
            elseif ($j) { foreach ($pp in $j.PSObject.Properties) { $knownMap[$pp.Name] = [string]$pp.Value } }
        } catch {}
    }
    $known = @($knownMap.Keys)
    $first = -not $known.Count
    $list = foreach ($d in $devs.Values) {
        $d.Self = $d.Ip -eq $self
        $d.Gateway = $d.Ip -eq $net.Gateway
        $d.Host = if ($d.Self) { $env:COMPUTERNAME } else { [string]$names[$d.Ip] }
        $d.Vendor = Get-Vendor $d.Mac
        $d.KindInfo = Get-DeviceKind $d
        $d.Title = if ($d.Self) { "$env:COMPUTERNAME (ce PC)" } elseif ($d.Host -and $d.Host -ne 'lan') { $d.Host } elseif ($d.Gateway) { 'Box Internet' } elseif ($d.Vendor -and $d.Vendor -ne 'Adresse privée') { $d.Vendor } else { 'Appareil inconnu' }
        $d.New = (-not $first) -and $d.Mac -and ($known -notcontains $d.Mac) -and -not $d.Self
        $d
    }
    $list = @($list | Sort-Object @{ Expression = { if ($_.Self) { 0 } elseif ($_.Gateway) { 1 } else { 2 } } }, @{ Expression = { [version]$_.Ip } })
    $today = (Get-Date).ToString('yyyy-MM-dd')
    foreach ($dv in $list) { if ($dv.Mac -and -not $knownMap.ContainsKey([string]$dv.Mac)) { $knownMap[[string]$dv.Mac] = $today } }
    $script:KnownDevices = $knownMap
    $script:NetList = $list
    try { ConvertTo-Json -InputObject $knownMap | Set-Content -LiteralPath $KnownFile -Encoding UTF8 } catch {}
    $newCount = @($list | Where-Object { $_.New }).Count

    # Résultat
    $radar.Rot.BeginAnimation([System.Windows.Media.RotateTransform]::AngleProperty, $null)
    $radar.Sweep.Visibility = 'Collapsed'
    $ui.NetHero.Children.Remove($bar)
    Start-Anim { param($e, $s) $s.T.Text = '{0:N0}' -f ($s.V * $e) } @{ T = $radar.Num; V = [double]$list.Count } 900
    $phase.Text = "appareil$(if ($list.Count -gt 1) {'s'}) connecté$(if ($list.Count -gt 1) {'s'})"
    $phase.Foreground = Get-Brush '#FFFFFF'
    if ($newCount) {
        $nw = New-Text "dont $newCount nouveau$(if ($newCount -gt 1) {'x'}) depuis le dernier scan" 12.5 $Colors.warn -Semi
        $nw.HorizontalAlignment = 'Center'
        [void]$ui.NetHero.Children.Add($nw)
    }
    $ui.NetDevices.Children.Clear()
    $i = 0
    foreach ($d in $list) { [void]$ui.NetDevices.Children.Add((New-DeviceTile $d $i)); $i++ }
    $ui.NetDevSummary.Text = "$($list.Count) appareil$(if ($list.Count -gt 1) {'s'})" + $(if ($newCount) { ", $newCount nouveau$(if ($newCount -gt 1) {'x'})" } else { '' })
    $ui.NetDevHint.Text = if ($first) {
        'Premier scan : ces appareils sont mémorisés, OptiGame te signalera tout nouvel appareil au prochain scan. Clique sur un appareil pour voir ses détails.'
    } elseif ($newCount) {
        'Un appareil « Nouveau » n''était pas là au scan précédent. Si tu ne le reconnais pas, change le mot de passe de ton Wi-Fi depuis la page de ta box. Attention : les téléphones récents changent parfois d''adresse et peuvent apparaître comme nouveaux.'
    } else {
        'Aucun nouvel appareil depuis le dernier scan. Clique sur un appareil pour voir ses détails, son ping en direct et ses services ouverts.'
    }
    Set-Status "Scan terminé : $($list.Count) appareils trouvés."
}

# ---------------------------------------------------------------------------
# Fiche détaillée d'un appareil du réseau
# ---------------------------------------------------------------------------
$DevTags = @{
    'Ce PC' = 'PC'; 'Box Internet' = 'BOX'; 'Routeur ou répéteur Wi-Fi' = 'WIFI'; 'Téléphone ou tablette' = 'TEL'
    'Console de jeu' = 'JEU'; 'TV ou multimédia' = 'TV'; 'Imprimante' = 'IMP'; 'Box ou décodeur TV' = 'BOX'
    'Objet connecté' = 'IOT'; 'Appareil Apple' = 'APP'; 'Ordinateur' = 'PC'; 'Téléphone probable' = 'TEL'; 'Appareil' = 'NET'
}

# Port : nom, niveau (info, warn, bad), explication, adresse web éventuelle
$PortInfo = @{
    21    = @('Transfert de fichiers (FTP)', 'warn', 'Les fichiers et les mots de passe passent en clair sur le réseau.', '')
    22    = @('Accès à distance sécurisé (SSH)', 'info', 'Permet de prendre la main sur l''appareil à distance, de façon chiffrée.', '')
    23    = @('Accès à distance non protégé (Telnet)', 'bad', 'Accès à distance sans aucun chiffrement : à désactiver dans les réglages de l''appareil.', '')
    25    = @('Envoi de mails (SMTP)', 'info', 'Serveur d''envoi de mails.', '')
    53    = @('Serveur DNS', 'info', 'Traduit les noms de sites en adresses. Normal pour une box ou un routeur.', '')
    80    = @('Page web', 'info', 'Page de réglages accessible depuis un navigateur.', 'http://{0}')
    110   = @('Réception de mails (POP3)', 'info', 'Serveur de mails.', '')
    135   = @('Services Windows', 'info', 'Communication interne de Windows. Normal sur un PC Windows.', '')
    139   = @('Partage de fichiers (ancien)', 'warn', 'Ancienne version du partage de fichiers Windows. Normal sur un PC, mais à éviter ailleurs.', '')
    143   = @('Réception de mails (IMAP)', 'info', 'Serveur de mails.', '')
    443   = @('Page web sécurisée', 'info', 'Page de réglages chiffrée, accessible depuis un navigateur.', 'https://{0}')
    445   = @('Partage de fichiers Windows', 'info', 'Dossiers ou imprimantes partagés. Vérifie que tu partages seulement ce que tu veux.', '')
    515   = @('Impression', 'info', 'Service d''impression réseau.', '')
    548   = @('Partage de fichiers Apple', 'info', 'Partage de fichiers d''un Mac ou d''un NAS.', '')
    554   = @('Flux vidéo', 'info', 'Souvent une caméra de surveillance ou un décodeur TV.', '')
    631   = @('Impression', 'info', 'Service d''impression réseau.', '')
    1883  = @('Objets connectés (MQTT)', 'info', 'Messagerie utilisée par la domotique.', '')
    3389  = @('Bureau à distance Windows', 'warn', 'Permet de prendre le contrôle du PC à distance. Désactive le si tu ne t''en sers pas.', '')
    5000  = @('Interface web (NAS, AirPlay)', 'info', 'Page de réglages ou service de diffusion.', 'http://{0}:5000')
    5001  = @('Interface web sécurisée (NAS)', 'info', 'Page de réglages chiffrée.', 'https://{0}:5001')
    5357  = @('Découverte réseau Windows', 'info', 'Permet aux autres appareils de voir ce PC sur le réseau.', '')
    5900  = @('Contrôle à distance (VNC)', 'warn', 'Prise de contrôle de l''écran à distance, souvent mal protégée.', '')
    7000  = @('AirPlay', 'info', 'Diffusion depuis un iPhone, un iPad ou un Mac.', '')
    8008  = @('Google Cast', 'info', 'Diffusion depuis un téléphone (Chromecast).', '')
    8009  = @('Google Cast', 'info', 'Diffusion depuis un téléphone (Chromecast).', '')
    8080  = @('Page web (autre port)', 'info', 'Page de réglages accessible depuis un navigateur.', 'http://{0}:8080')
    8123  = @('Home Assistant', 'info', 'Interface de domotique.', 'http://{0}:8123')
    8443  = @('Page web sécurisée (autre port)', 'info', 'Page de réglages chiffrée.', 'https://{0}:8443')
    9100  = @('Imprimante réseau', 'info', 'Impression directe sur l''imprimante.', '')
    9295  = @('PlayStation Remote Play', 'info', 'Jouer à distance sur la console.', '')
    32400 = @('Serveur Plex', 'info', 'Serveur de films et séries.', 'http://{0}:32400/web')
    62078 = @('Synchronisation iPhone ou iPad', 'info', 'Synchronisation avec un ordinateur. Normal sur un appareil Apple.', '')
}

function New-InfoRows([array]$Rows) {
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.Margin = New-Thickness 0 6 0 0
    foreach ($r in $Rows) {
        $g = New-Grid @('200', '*')
        $g.Margin = New-Thickness 0 3 0 3
        Add-ToGrid $g (New-Text $r[0] 13 '#9AA3B2') 0
        $v = New-Text ([string]$r[1]) 13 $(if ($r.Count -gt 2) { $r[2] } else { '#FFFFFF' }) -Semi
        $v.TextWrapping = 'Wrap'
        Add-ToGrid $g $v 1
        [void]$sp.Children.Add($g)
    }
    $sp
}

# Un ping toutes les 400 ms, sans bloquer la fenêtre.
function Update-DevPing {
    $m = $script:DevPing
    if (-not $m) { return }
    if ($m.Task) {
        if (-not $m.Task.IsCompleted) { return }
        $m.Sent++
        $rtt = $null
        try { if (-not $m.Task.IsFaulted -and [string]$m.Task.Result.Status -eq 'Success') { $rtt = [double]$m.Task.Result.RoundtripTime } } catch {}
        if ($null -ne $rtt) {
            [void]$m.Times.Add($rtt)
            if ($m.Times.Count -gt 60) { $m.Times.RemoveAt(0) }
            $m.Gauge.State.Max = [math]::Max(100.0, ($m.Times | Measure-Object -Maximum).Maximum * 1.2)
            Set-GaugeLive $m.Gauge $rtt
            Add-ChartPoint $m.Chart $rtt
        } else {
            $m.Lost++
            Add-ChartPoint $m.Chart 0
        }
        $loss = 100 * $m.Lost / [math]::Max(1.0, $m.Sent)
        if ($m.Times.Count) {
            $avg = ($m.Times | Measure-Object -Average).Average
            $jit = 0
            for ($i = 1; $i -lt $m.Times.Count; $i++) { $jit += [math]::Abs($m.Times[$i] - $m.Times[$i - 1]) }
            if ($m.Times.Count -gt 1) { $jit = $jit / ($m.Times.Count - 1) }
            $m.S.Avg.Text = if ($avg -lt 1) { '< 1 ms' } else { '{0:N0} ms' -f $avg }
            $m.S.Min.Text = '{0:N0} ms' -f ($m.Times | Measure-Object -Minimum).Minimum
            $m.S.Max.Text = '{0:N0} ms' -f ($m.Times | Measure-Object -Maximum).Maximum
            $m.S.Jit.Text = '{0:N1} ms' -f $jit
        }
        $m.S.Loss.Text = '{0:N0} %' -f $loss
        $m.S.Loss.Foreground = Get-Brush $(if ($loss -gt 0) { $Colors.warn } else { $Colors.ok })
        if ($m.Sent -ge 3) {
            if (-not $m.Times.Count) {
                $m.S.Verdict.Text = 'Cet appareil ne répond pas au ping. C''est normal pour certains téléphones, consoles ou PC qui le bloquent.'
                $m.S.Verdict.Foreground = Get-Brush '#9AA3B2'
            } elseif ($loss -ge 5 -or $jit -gt 15) {
                $m.S.Verdict.Text = 'Connexion instable : des réponses se perdent ou arrivent en retard. Souvent le signe d''un Wi-Fi faible.'
                $m.S.Verdict.Foreground = Get-Brush $Colors.warn
            } else {
                $m.S.Verdict.Text = 'Connexion stable.'
                $m.S.Verdict.Foreground = Get-Brush $Colors.ok
            }
        }
        $m.Task = $null
    }
    try { $m.Task = $m.Ping.SendPingAsync($m.Ip, 1000) } catch { $m.Task = $null }
}

function Show-DeviceDetail($D) {
    if ($script:TestRunning) { return }
    $token = [guid]::NewGuid()
    $script:DevToken = $token
    if ($script:MonitorTimer) { $script:MonitorTimer.Stop(); $script:MonitorTimer = $null }
    $tag = $DevTags[$D.KindInfo.Kind]; if (-not $tag) { $tag = 'NET' }
    Show-TestPanel @{ Tag = $tag; Title = $D.Title; Sub = $D.KindInfo.Kind }
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    Set-TestState 'live' 'Ping en direct'
    $ui.TestProgress.Value = 0
    $body = $ui.TestBody

    # Identité
    [void]$body.Children.Add((New-SectionTitle 'IDENTITÉ'))
    $first = [string]$script:KnownDevices[[string]$D.Mac]
    $firstTxt = if ($D.Self) { 'Ce PC' } elseif ($first) { ([datetime]$first).ToString('dd/MM/yyyy') } elseif ($D.Mac) { 'Avant la mise à jour 1.0.8' } else { 'Inconnue' }
    $rows = @(
        @('Adresse sur le réseau', $D.Ip),
        @('Adresse physique (MAC)', $(if ($D.Mac) { $D.Mac } else { 'Inconnue' })),
        @('Fabricant', $(if ($D.Vendor) { $D.Vendor } else { 'Inconnu' })),
        @('Type', $D.KindInfo.Kind)
    )
    if ($D.Host) { $rows += , @('Nom sur le réseau', $D.Host) }
    $rows += , @('Vu pour la première fois', $firstTxt)
    if ($D.New) { $rows += , @('Statut', 'Nouvel appareil depuis le dernier scan', $Colors.warn) }
    if ($D.Vendor -eq 'Adresse privée') { $rows += , @('Bon à savoir', 'Les téléphones récents cachent leur vraie adresse physique : le fabricant ne peut pas être connu.', '#9AA3B2') }
    [void]$body.Children.Add((New-InfoRows $rows))

    # Ping en direct
    [void]$body.Children.Add((New-SectionTitle 'TEMPS DE RÉPONSE EN DIRECT'))
    $row = New-Grid @('Auto', '*')
    $gauge = New-Gauge 'Temps de réponse' 0 100 '{0:N0}' 'ms' $Colors.info 0
    Add-ToGrid $row $gauge.El 0
    $stats = @{}
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.VerticalAlignment = 'Center'
    $sp.Margin = New-Thickness 18 0 0 0
    foreach ($s in @(@('Avg', 'Moyenne'), @('Min', 'Plus rapide'), @('Max', 'Plus lent'), @('Jit', 'Variation (gigue)'), @('Loss', 'Réponses perdues'))) {
        $g = New-Grid @('170', '*')
        $g.Margin = New-Thickness 0 3 0 3
        Add-ToGrid $g (New-Text $s[1] 13 '#9AA3B2') 0
        $v = New-Text '...' 13 '#FFFFFF' -Semi
        Add-ToGrid $g $v 1
        [void]$sp.Children.Add($g)
        $stats[$s[0]] = $v
    }
    $verdict = New-Text 'Mesure en cours...' 12.5 '#9AA3B2' -Semi
    $verdict.Margin = New-Thickness 0 10 0 0
    [void]$sp.Children.Add($verdict)
    $stats.Verdict = $verdict
    Add-ToGrid $row $sp 1
    [void]$body.Children.Add($row)
    $chart = New-LiveChart $Colors.info 'ms' '{0:N0}'
    [void]$body.Children.Add($chart.El)

    $script:DevPing = @{ Ip = $D.Ip; Ping = (New-Object System.Net.NetworkInformation.Ping); Task = $null; Times = (New-Object System.Collections.ArrayList); Lost = 0; Sent = 0; Gauge = $gauge; Chart = $chart; S = $stats }
    $script:MonitorTimer = New-Object System.Windows.Threading.DispatcherTimer
    $script:MonitorTimer.Interval = [TimeSpan]::FromMilliseconds(400)
    $script:MonitorTimer.Add_Tick({ try { Update-DevPing } catch {} })
    $script:MonitorTimer.Start()

    # Services ouverts (en arrière plan)
    [void]$body.Children.Add((New-SectionTitle 'SERVICES OUVERTS'))
    $wait = New-Text 'Recherche des services proposés par cet appareil...' 13 '#9AA3B2'
    [void]$body.Children.Add($wait)
    $ports = [int[]]@($PortInfo.Keys)
    $open = @(Invoke-Async { param($a) [OGNative]::ScanPorts($a.Ip, [int[]]$a.Ports, 600) } @{ Ip = $D.Ip; Ports = $ports })
    if ($script:DevToken -ne $token -or $ui.TestOverlay.Visibility -ne 'Visible') { return }
    $body.Children.Remove($wait)
    $worst = 'ok'
    $notes = @()
    if (-not $open.Count) {
        [void]$body.Children.Add((New-Text 'Aucun service ouvert parmi les plus courants. C''est normal pour un téléphone, une console ou une TV : ils n''acceptent pas de connexions.' 13 '#9AA3B2'))
    }
    $i = 0
    foreach ($port in $open) {
        $pi = $PortInfo[[int]$port]
        if (-not $pi) { continue }
        $col = switch ($pi[1]) { 'bad' { $Colors.bad } 'warn' { $Colors.warn } default { $Colors.info } }
        if ($pi[1] -eq 'bad') { $worst = 'bad'; $notes += $pi[0] } elseif ($pi[1] -eq 'warn' -and $worst -ne 'bad') { $worst = 'warn'; $notes += $pi[0] }
        $card = New-Object System.Windows.Controls.Border
        $card.Background = Get-Brush '#1A1F29'
        $card.CornerRadius = [System.Windows.CornerRadius]::new(10)
        $card.Padding = New-Thickness 12 9 12 9
        $card.Margin = New-Thickness 0 0 0 6
        $g = New-Grid @('Auto', '*', 'Auto')
        $badge = New-Object System.Windows.Controls.Border
        $bg = Get-Brush $col; $bg.Opacity = 0.16
        $badge.Background = $bg
        $badge.CornerRadius = [System.Windows.CornerRadius]::new(8)
        $badge.MinWidth = 62
        $badge.Padding = New-Thickness 8 4 8 4
        $badge.VerticalAlignment = 'Center'
        $bt = New-Text "$port" 13 $col -Bold
        $bt.HorizontalAlignment = 'Center'
        $badge.Child = $bt
        Add-ToGrid $g $badge 0
        $txt = New-Object System.Windows.Controls.StackPanel
        $txt.Margin = New-Thickness 12 0 8 0
        [void]$txt.Children.Add((New-Text $pi[0] 13.5 '#FFFFFF' -Semi))
        [void]$txt.Children.Add((New-Text $pi[2] 12 '#9AA3B2'))
        Add-ToGrid $g $txt 1
        if ($pi[3]) {
            $b = New-Button 'Ouvrir'
            $b.Tag = $pi[3] -f $D.Ip
            $b.Add_Click({ param($s, $e) Start-Process $s.Tag })
            Add-ToGrid $g $b 2
        }
        $card.Child = $g
        $card.Opacity = 0
        Start-WpfAnim $card ([System.Windows.UIElement]::OpacityProperty) 1 350 (80 * $i)
        [void]$body.Children.Add($card)
        $i++
    }
    $txt = switch ($worst) {
        'bad'  { "À corriger : $($notes -join ', '). Désactive ce service dans les réglages de l'appareil, ou demande à la personne qui l'a installé." }
        'warn' { "À surveiller : $($notes -join ', '). Si tu ne sais pas pourquoi c'est ouvert, désactive le dans les réglages de l'appareil." }
        default { if ($open.Count) { 'Rien d''inhabituel pour ce type d''appareil.' } else { 'Rien à signaler.' } }
    }
    [void]$body.Children.Add((New-Verdict $worst $txt))
    $note = New-Text "Seuls les $($ports.Count) services les plus courants sont vérifiés." 11.5 '#5B6475'
    $note.Margin = New-Thickness 0 8 0 0
    [void]$body.Children.Add($note)
}

# ---------------------------------------------------------------------------
# Audit de sécurité du réseau
# ---------------------------------------------------------------------------
$AuditPorts = @(21, 23, 80, 139, 443, 445, 554, 3389, 5900)
$AuditTips = @(
    'Si beaucoup de monde connaît le mot de passe de ton Wi-Fi, change le de temps en temps depuis la page de ta box.',
    'Crée un Wi-Fi invité (souvent proposé par la box) pour tes visiteurs et tes objets connectés.',
    'Change le mot de passe par défaut des caméras, NAS et imprimantes réseau.',
    'Laisse les mises à jour automatiques activées sur ta box, ton PC et tes appareils.'
)
$AuditCats = @{ wifi = 'Wi-Fi'; box = 'Box'; dns = 'DNS'; devices = 'Appareils'; pc = 'Ce PC' }

# Travail lancé dans un fil séparé: seules les fonctions de [OGNative] y sont disponibles.
$NetAuditWork = {
    param($a)
    $out = @{}
    try {
        $openRe = '(?i)^(ouvrir|ouvert|open)'
        $field = {
            param($lines, $re)
            foreach ($l in $lines) { if ($l -match "^\s*($re)\s*:\s*(.+?)\s*$") { return $Matches[2] } }
            $null
        }

        # Wi-Fi utilisé et Wi-Fi mémorisés
        [OGNative]::Phase = 'wifi'; [OGNative]::Progress = 2
        $lines = @(netsh wlan show interfaces 2>$null)
        $ssid = & $field $lines 'SSID'
        if ($ssid) { $out.Wifi = @{ Ssid = [string]$ssid; Auth = [string](& $field $lines 'Authentication|Authentification'); Cipher = [string](& $field $lines 'Cipher|Chiffrement') } }
        $names = @(netsh wlan show profiles 2>$null | ForEach-Object { if ($_ -match '^\s{2,}[^:<]+:\s(.+?)\s*$') { $Matches[1] } }) | Select-Object -First 40
        $out.Profiles = @(foreach ($n in $names) {
            $pl = @(netsh wlan show profile "name=$n" 2>$null)
            $auth = [string](& $field $pl 'Authentication|Authentification')
            $ciph = [string](& $field $pl 'Cipher|Chiffrement')
            $mode = [string](& $field $pl 'Connection mode|Mode de connexion')
            $isOpen = ($auth -match $openRe) -or ($ciph -match 'WEP')
            "$n|$auth|$ciph|$(if ($mode -match '(?i)auto') { 1 } else { 0 })|$(if ($isOpen) { 1 } else { 0 })"
        })
        [OGNative]::Progress = 8

        # Box : annonces UPnP (SSDP), WPS, règles d'ouverture de ports
        [OGNative]::Phase = 'box'
        $ssdp = @()
        try {
            $u = New-Object Net.Sockets.UdpClient((New-Object Net.IPEndPoint([Net.IPAddress]::Parse($a.Ip), 0)))
            $u.Client.ReceiveTimeout = 400
            $msg = [Text.Encoding]::ASCII.GetBytes("M-SEARCH * HTTP/1.1`r`nHOST: 239.255.255.250:1900`r`nMAN: `"ssdp:discover`"`r`nMX: 2`r`nST: ssdp:all`r`n`r`n")
            [void]$u.Send($msg, $msg.Length, '239.255.255.250', 1900)
            $ep = New-Object Net.IPEndPoint([Net.IPAddress]::Any, 0)
            $end = (Get-Date).AddSeconds(3)
            while ((Get-Date) -lt $end) {
                try { $r = $u.Receive([ref]$ep) } catch { continue }
                $st = ''; $loc = ''
                foreach ($l in ([Text.Encoding]::ASCII.GetString($r) -split "`r`n")) {
                    if ($l -match '^(?i)ST:\s*(.+)$') { $st = $Matches[1].Trim() } elseif ($l -match '^(?i)LOCATION:\s*(.+)$') { $loc = $Matches[1].Trim() }
                }
                $ssdp += "$($ep.Address)|$st|$loc"
                [OGNative]::Progress = [math]::Min(18.0, [OGNative]::Progress + 0.2)
            }
            $u.Close()
        } catch {}
        $out.Ssdp = @($ssdp | Select-Object -Unique)
        [OGNative]::Progress = 18

        $gwLoc = @($out.Ssdp | ForEach-Object { $p = $_ -split '\|', 3; if ($p[0] -eq $a.Gateway -and $p[1] -match 'InternetGatewayDevice|WAN(IP|PPP)Connection') { $p[2] } }) | Select-Object -First 1
        if ($gwLoc) {
            $igd = @{ Maps = @(); External = '' }
            try {
                $x = [xml](Invoke-WebRequest -Uri $gwLoc -UseBasicParsing -TimeoutSec 4).Content
                $svc = @($x.GetElementsByTagName('service') | Where-Object { $_.serviceType -match 'WAN(IP|PPP)Connection' }) | Select-Object -First 1
                if ($svc) {
                    $ctl = [string]$svc.controlURL
                    $url = if ($ctl -match '^https?://') { $ctl } else { ([uri]$gwLoc).GetLeftPart('Authority') + $(if ($ctl.StartsWith('/')) { $ctl } else { '/' + $ctl }) }
                    $type = [string]$svc.serviceType
                    $soap = {
                        param($action, $inner)
                        $body = "<?xml version=`"1.0`"?><s:Envelope xmlns:s=`"http://schemas.xmlsoap.org/soap/envelope/`" s:encodingStyle=`"http://schemas.xmlsoap.org/soap/encoding/`"><s:Body><u:$action xmlns:u=`"$type`">$inner</u:$action></s:Body></s:Envelope>"
                        try { [xml](Invoke-WebRequest -Uri $url -Method Post -Body $body -ContentType 'text/xml; charset="utf-8"' -Headers @{ SOAPAction = "`"$type#$action`"" } -UseBasicParsing -TimeoutSec 3).Content } catch { $null }
                    }
                    $e = & $soap 'GetExternalIPAddress' ''
                    if ($e) { $igd.External = [string]($e.GetElementsByTagName('NewExternalIPAddress') | Select-Object -First 1).InnerText }
                    for ($i = 0; $i -lt 64; $i++) {
                        $r = & $soap 'GetGenericPortMappingEntry' "<NewPortMappingIndex>$i</NewPortMappingIndex>"
                        if (-not $r) { break }
                        $m = @{}
                        foreach ($n in $r.GetElementsByTagName('*')) { $m[$n.LocalName] = [string]$n.InnerText }
                        $igd.Maps += "$($m.NewExternalPort)|$($m.NewProtocol)|$($m.NewInternalPort)|$($m.NewInternalClient)|$(($m.NewPortMappingDescription) -replace '\|', ' ')"
                    }
                }
            } catch {}
            $out.Igd = $igd
        }
        [OGNative]::Progress = 25

        # DNS : réponses truquées ?
        [OGNative]::Phase = 'dns'
        $dns = @{ Servers = @(); Redirect = @(); NxHijack = '' }
        try { $dns.Servers = @(Get-DnsClientServerAddress -InterfaceIndex $a.If -ErrorAction Stop | ForEach-Object { $_.ServerAddresses } | Where-Object { $_ -notlike 'fec0:*' } | ForEach-Object { [string]$_ }) } catch {}
        foreach ($t in @(@('one.one.one.one', '1.1.1.1', '1.0.0.1'), @('dns.google', '8.8.8.8', '8.8.4.4'))) {
            try {
                $ips = @([Net.Dns]::GetHostAddresses($t[0]) | Where-Object { $_.AddressFamily -eq 'InterNetwork' } | ForEach-Object { $_.IPAddressToString })
                $wrong = @($ips | Where-Object { $_ -notin @($t[1], $t[2]) })
                if ($wrong.Count) { $dns.Redirect += "$($t[0]) répond $($wrong -join ', ') au lieu de $($t[1])" }
            } catch {}
        }
        try {
            $fake = "optigame-$([guid]::NewGuid().ToString('N').Substring(0, 12)).com"
            $got = @([Net.Dns]::GetHostAddresses($fake) | ForEach-Object { $_.IPAddressToString })
            if ($got.Count) { $dns.NxHijack = $got -join ', ' }
        } catch {}
        $out.Dns = $dns
        try { [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; $out.PublicIp = [string](Invoke-RestMethod 'https://api.ipify.org' -TimeoutSec 5) } catch {}
        [OGNative]::Progress = 35

        # Services ouverts sur les appareils
        [OGNative]::Phase = 'devices'
        $out.Ports = @([OGNative]::ScanHosts([string[]]@($a.Ips), [int[]]@($a.Ports), 800, 35.0, 85.0))

        # Ce PC
        [OGNative]::Phase = 'pc'; [OGNative]::Progress = 86
        $pc = @{ Smb1 = $false; FirewallOff = @(); Shares = @() }
        try { $pc.Smb1 = [bool](Get-SmbServerConfiguration -ErrorAction Stop).EnableSMB1Protocol } catch {}
        $pc.Smb1Client = Test-Path 'HKLM:\SYSTEM\CurrentControlSet\Services\mrxsmb10'
        $pc.Rdp = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server' -ErrorAction SilentlyContinue).fDenyTSConnections -eq 0
        try { $pc.FirewallOff = @(Get-NetFirewallProfile -ErrorAction Stop | Where-Object { [string]$_.Enabled -eq 'False' } | ForEach-Object { [string]$_.Name }) } catch {}
        try { $pc.Category = [string](Get-NetConnectionProfile -InterfaceIndex $a.If -ErrorAction Stop | Select-Object -First 1).NetworkCategory } catch {}
        [OGNative]::Progress = 92
        try {
            $every = @('Everyone', 'Tout le monde')
            try { $every += ([Security.Principal.SecurityIdentifier]'S-1-1-0').Translate([Security.Principal.NTAccount]).Value } catch {}
            foreach ($s in @(Get-SmbShare -ErrorAction Stop | Where-Object { -not $_.Special -and $_.Name -notmatch '\$$' })) {
                $rights = @(Get-SmbShareAccess -Name $s.Name -ErrorAction SilentlyContinue | Where-Object { $every -contains ([string]$_.AccountName -replace '^.*\\', '') -and [string]$_.AccessControlType -eq 'Allow' } | ForEach-Object { [string]$_.AccessRight })
                $pc.Shares += "$($s.Name)|$($s.Path)|$($rights -join ',')"
            }
        } catch {}
        $out.Pc = $pc
        [OGNative]::Progress = 100
        $out
    } catch { @{ Error = $_.Exception.GetBaseException().Message } }
}

function Add-AuditCheck($List, [string]$Status, [string]$Cat, [string]$Title, [string]$Detail, $Items, $Actions) {
    [void]$List.Add(@{
        Status = $Status; Cat = $Cat; Title = $Title; Detail = $Detail
        Items = @(@($Items) | Where-Object { $_ }); Actions = @(@($Actions) | Where-Object { $_ })
    })
}

function Get-NetAuditChecks($R, $Devs, $Net) {
    $list = New-Object System.Collections.ArrayList
    $gw = [string]$Net.Gateway
    $boxAct = @{ Label = 'Ouvrir la box'; Arg = "http://$gw"; Script = { param($u) Start-Process $u }; NoRefresh = $true }
    $dnsAct = @{ Label = 'Changer de DNS'; NoRefresh = $true; Script = { Hide-TestPanel; Show-Page 3 } }
    $rerun = { Invoke-NetAudit }
    $openRe = '(?i)^(ouvrir|ouvert|open)'
    $byIp = @{}
    foreach ($d in @($Devs)) { $byIp[[string]$d.Ip] = $d }
    $nameOf = { param($ip) $dv = $byIp[[string]$ip]; if ($dv) { "$($dv.Title) ($ip)" } else { [string]$ip } }
    $plural = { param($n, $one, $many) if ($n -gt 1) { "$n $many" } else { "$n $one" } }

    # Wi-Fi
    $w = $R.Wifi
    $auth = ''
    if ($w) {
        $auth = [string]$w.Auth; $ciph = [string]$w.Cipher; $ssid = [string]$w.Ssid
        if ($auth -match 'WPA3') { Add-AuditCheck $list 'ok' 'wifi' "Wi-Fi « $ssid » protégé en WPA3" 'Le meilleur niveau de protection actuel.' }
        elseif ($auth -match 'WPA2' -and $ciph -match 'TKIP') { Add-AuditCheck $list 'warn' 'wifi' "Wi-Fi « $ssid » : chiffrement ancien (TKIP)" 'Le WPA2 est bon, mais le chiffrement TKIP est dépassé et ralentit le Wi-Fi. Choisis « WPA2 (AES) » ou « WPA2/WPA3 » dans les réglages Wi-Fi de ta box.' $null @($boxAct) }
        elseif ($auth -match 'WPA2') { Add-AuditCheck $list 'ok' 'wifi' "Wi-Fi « $ssid » protégé en WPA2" 'Bonne protection. Si ta box propose le mode WPA2/WPA3, il est encore plus sûr.' }
        elseif ($auth -match 'WPA') { Add-AuditCheck $list 'bad' 'wifi' "Wi-Fi « $ssid » : protection dépassée (WPA)" 'Cette ancienne méthode se casse facilement. Passe en WPA2 ou WPA3 dans les réglages Wi-Fi de ta box.' $null @($boxAct) }
        elseif ($auth -match 'WEP' -or $ciph -match 'WEP') { Add-AuditCheck $list 'bad' 'wifi' "Wi-Fi « $ssid » protégé en WEP" 'Le WEP se casse en quelques minutes : n''importe qui à portée peut entrer sur ton réseau. Passe en WPA2 ou WPA3 dans les réglages Wi-Fi de ta box.' $null @($boxAct) }
        elseif ($auth -match $openRe) { Add-AuditCheck $list 'bad' 'wifi' "Wi-Fi « $ssid » sans mot de passe" 'Tout le monde à portée peut se connecter et voir ce qui passe en clair. Si c''est ton Wi-Fi, mets un mot de passe WPA2 ou WPA3 depuis la page de ta box. Si c''est un Wi-Fi public, évite les sites sensibles.' $null @($boxAct) }
        else { Add-AuditCheck $list 'info' 'wifi' "Wi-Fi « $ssid » : protection non reconnue ($auth)" 'Vérifie dans la page de ta box que le Wi-Fi est en WPA2 ou WPA3.' $null @($boxAct) }
    } else {
        Add-AuditCheck $list 'info' 'wifi' 'Ce PC est branché par câble' 'La protection du Wi-Fi ne peut pas être lue depuis ce PC. Vérifie dans la page de ta box qu''il est en WPA2 ou WPA3.' $null @($boxAct)
    }
    $profiles = @(foreach ($l in @($R.Profiles)) { $x = ([string]$l) -split '\|'; if ($x.Count -ge 5) { [pscustomobject]@{ Name = $x[0]; Auto = $x[3] -eq '1'; Open = $x[4] -eq '1' } } })
    $openAuto = @($profiles | Where-Object { $_.Auto -and $_.Open } | ForEach-Object { $_.Name })
    if ($openAuto.Count) {
        Add-AuditCheck $list 'warn' 'wifi' "Connexion automatique à $(& $plural $openAuto.Count 'Wi-Fi ouvert' 'Wi-Fi ouverts')" 'Ton PC se reconnecte tout seul à ces réseaux sans protection. Un pirate peut créer un faux Wi-Fi avec le même nom pour espionner ta connexion.' $openAuto @(
            @{ Label = 'Passer en connexion manuelle'; Arg = $openAuto; After = $rerun
               Confirm = 'Ces Wi-Fi ne se connecteront plus automatiquement. Tu pourras toujours t''y connecter en cliquant dessus. Continuer ?'
               Script = { param($names) foreach ($n in $names) { netsh wlan set profileparameter "name=$n" connectionmode=manual | Out-Null } } })
    } elseif ($profiles.Count) {
        Add-AuditCheck $list 'ok' 'wifi' 'Aucun Wi-Fi ouvert en connexion automatique' "$(& $plural $profiles.Count 'Wi-Fi mémorisé' 'Wi-Fi mémorisés') sur ce PC, tous protégés ou en connexion manuelle."
    }

    # Box : WPS, UPnP, double NAT, page de réglages
    $ssdp = @(foreach ($l in @($R.Ssdp)) { $x = ([string]$l) -split '\|', 3; [pscustomobject]@{ Ip = $x[0]; St = $x[1] } })
    $wpsIps = @($ssdp | Where-Object { $_.St -match 'wifialliance-org:(device:WFADevice|service:WFAWLANConfig)' } | ForEach-Object { $_.Ip } | Select-Object -Unique)
    if ($wpsIps.Count) {
        $title = if ($wpsIps.Count -eq 1 -and $wpsIps[0] -eq $gw) { 'Le WPS semble activé sur ta box' } else { "Le WPS semble activé sur $(& $plural $wpsIps.Count 'appareil' 'appareils')" }
        Add-AuditCheck $list 'warn' 'box' $title 'Le WPS permet de connecter un appareil en appuyant sur un bouton, mais son code PIN est une faille connue qui permet de trouver le mot de passe du Wi-Fi. Si tu ne t''en sers pas, désactive le dans les réglages Wi-Fi de ta box et de tes répéteurs.' @($wpsIps | ForEach-Object { & $nameOf $_ }) @($boxAct)
    } else {
        Add-AuditCheck $list 'ok' 'box' 'WPS non annoncé sur le réseau' 'Aucun appareil ne propose la connexion par code PIN WPS.'
    }
    $risky = @{ 21 = 'FTP'; 22 = 'SSH'; 23 = 'Telnet'; 80 = 'page web'; 139 = 'partage Windows'; 445 = 'partage Windows'; 554 = 'caméra'; 3389 = 'Bureau à distance'; 5900 = 'VNC'; 8080 = 'page web' }
    $igd = $R.Igd
    if ($igd) {
        $maps = @(foreach ($m in @($igd.Maps)) { $x = ([string]$m) -split '\|', 5; if ($x.Count -ge 5) { [pscustomobject]@{ Ext = $x[0]; Proto = $x[1]; Int = [int]$x[2]; Client = $x[3]; Desc = $x[4] } } })
        $mapText = { param($m) "Port $($m.Ext) ($($m.Proto)) vers $(& $nameOf $m.Client)$(if ($m.Desc) { ' : ' + $m.Desc })" }
        $danger = @($maps | Where-Object { $risky.ContainsKey($_.Int) })
        $safe = @($maps | Where-Object { -not $risky.ContainsKey($_.Int) })
        if ($danger.Count) {
            Add-AuditCheck $list 'bad' 'box' "$(& $plural $danger.Count 'service sensible ouvert' 'services sensibles ouverts') sur Internet" 'Ces règles, créées automatiquement par un appareil (UPnP), rendent un service sensible joignable depuis tout Internet : n''importe qui peut essayer de s''y connecter. Supprime les dans la page de ta box (rubrique NAT, PAT ou UPnP) si tu ne sais pas à quoi elles servent.' @($danger | ForEach-Object { "$(& $mapText $_) ($($risky[$_.Int]))" }) @($boxAct)
        }
        if ($safe.Count -or -not $maps.Count) {
            $title = if ($safe.Count) { "UPnP activé : $(& $plural $safe.Count 'port ouvert' 'ports ouverts') automatiquement" } else { 'UPnP activé, aucun port ouvert' }
            Add-AuditCheck $list 'ok' 'box' $title 'L''UPnP permet à tes jeux et consoles d''ouvrir les ports dont ils ont besoin (NAT ouvert). Rien de sensible dans ces règles.' @($safe | ForEach-Object { & $mapText $_ })
        }
        $ext = [string]$igd.External
        if ($ext -match '^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|100\.(6[4-9]|[7-9]\d|1[01]\d|12[0-7])\.)') {
            Add-AuditCheck $list 'info' 'box' 'Double NAT détecté' "Ta box n'a pas directement une adresse Internet ($ext) : elle est derrière un autre routeur, ou ton opérateur partage l'adresse entre plusieurs clients. En jeu, ça peut donner un « NAT strict » et des soucis pour héberger une partie."
        }
    } else {
        Add-AuditCheck $list 'ok' 'box' 'UPnP non détecté sur ta box' 'Aucun appareil ne peut ouvrir de port vers Internet tout seul. Si tu as un « NAT strict » en jeu, activer l''UPnP dans ta box peut aider.'
    }
    $open = @{}
    foreach ($l in @($R.Ports)) { $x = ([string]$l) -split '\|'; if (-not $open.ContainsKey($x[0])) { $open[$x[0]] = @() }; $open[$x[0]] += [int]$x[1] }
    $gwPorts = @($open[$gw])
    if ($gwPorts -contains 80 -and $gwPorts -notcontains 443) {
        Add-AuditCheck $list 'info' 'box' 'Page de réglages de la box non chiffrée' 'La page de ta box est en http simple : son mot de passe passe en clair sur ton réseau. Pas grave tant que seules des personnes de confiance sont connectées chez toi.' $null @($boxAct)
    }

    # DNS
    $dns = $R.Dns
    $knownDns = @{ '1.1.1.1' = 'Cloudflare'; '1.0.0.1' = 'Cloudflare'; '8.8.8.8' = 'Google'; '8.8.4.4' = 'Google'; '9.9.9.9' = 'Quad9'; '149.112.112.112' = 'Quad9'; '208.67.222.222' = 'OpenDNS'; '208.67.220.220' = 'OpenDNS'; '94.140.14.14' = 'AdGuard'; '94.140.15.15' = 'AdGuard' }
    $srv = @(@($dns.Servers) | Where-Object { $_ } | Select-Object -Unique | ForEach-Object {
        $lbl = if ($_ -eq $gw) { 'ta box' } elseif ($knownDns[$_]) { $knownDns[$_] } elseif ($_ -match '^(fe80:|192\.168\.|10\.|172\.)') { 'réseau local' } else { 'fournisseur d''accès ou autre' }
        "Serveur $_ ($lbl)"
    })
    if (@($dns.Redirect).Count) {
        Add-AuditCheck $list 'bad' 'dns' 'Tes recherches Internet sont détournées' 'Des adresses connues ne donnent pas la bonne réponse : ton serveur DNS peut t''envoyer vers de faux sites. Si tu n''utilises pas de filtre (contrôle parental, bloqueur de pub), change de DNS et vérifie les réglages DNS de ta box.' (@($dns.Redirect) + $srv) @($dnsAct, $boxAct)
    } elseif ($dns.NxHijack) {
        Add-AuditCheck $list 'warn' 'dns' 'Ton DNS redirige les adresses qui n''existent pas' 'Quand tu tapes une mauvaise adresse, tu arrives sur une page de pub au lieu d''une erreur. Pas dangereux, mais un DNS comme Cloudflare (1.1.1.1) évite ça et répond souvent plus vite.' $srv @($dnsAct)
    } else {
        Add-AuditCheck $list 'ok' 'dns' 'DNS fiable' 'Les réponses DNS sont correctes : pas de redirection vers de faux sites.' $srv
    }

    # Appareils
    $telnet = @(); $ftp = @(); $remote = @(); $cams = @()
    foreach ($ip in $open.Keys) {
        $ps = $open[$ip]; $nm = & $nameOf $ip
        if ($ps -contains 23) { $telnet += $nm }
        if ($ps -contains 21) { $ftp += $nm }
        if ($ps -contains 3389) { $remote += "$nm : Bureau à distance" }
        if ($ps -contains 5900) { $remote += "$nm : VNC" }
        if ($ps -contains 554) { $cams += $nm }
    }
    $checked = @($Devs | Where-Object { -not $_.Self }).Count
    if ($telnet.Count) { Add-AuditCheck $list 'bad' 'devices' "Telnet ouvert sur $(& $plural $telnet.Count 'appareil' 'appareils')" 'Telnet permet de prendre la main sur un appareil sans aucun chiffrement, et ces appareils ont souvent un mot de passe par défaut connu de tous. Désactive le dans les réglages de l''appareil, ou mets à jour son logiciel.' $telnet }
    if ($ftp.Count) { Add-AuditCheck $list 'warn' 'devices' "Transfert de fichiers FTP ouvert sur $(& $plural $ftp.Count 'appareil' 'appareils')" 'Le FTP envoie les fichiers et les mots de passe en clair. Normal sur certains NAS ou box : si tu ne t''en sers pas, désactive le dans les réglages de l''appareil.' $ftp }
    if ($remote.Count) { Add-AuditCheck $list 'warn' 'devices' 'Prise de contrôle à distance ouverte' 'Ces appareils acceptent qu''on prenne le contrôle de leur écran. Si ce n''est pas voulu, désactive le dans leurs réglages, et utilise toujours un mot de passe fort.' $remote }
    if ($cams.Count) { Add-AuditCheck $list 'info' 'devices' "Flux vidéo sur $(& $plural $cams.Count 'appareil' 'appareils')" 'Souvent une caméra ou un décodeur TV. Vérifie que tes caméras sont protégées par un mot de passe que tu as choisi toi même.' $cams }
    if (-not ($telnet.Count + $ftp.Count + $remote.Count)) { Add-AuditCheck $list 'ok' 'devices' "Aucun service dangereux sur tes $(& $plural $checked 'appareil' 'appareils')" 'Pas de Telnet, de FTP ni de prise de contrôle à distance ouverts.' }
    $devText = { param($d) "$($d.Title) ($($d.Ip))$(if ($d.Vendor -eq 'Adresse privée') { ', adresse masquée' } elseif ($d.Vendor) { ', ' + $d.Vendor })" }
    $new = @($Devs | Where-Object { $_.New })
    $unk = @($Devs | Where-Object { $_.Title -eq 'Appareil inconnu' -and -not $_.New })
    if ($new.Count) { Add-AuditCheck $list 'warn' 'devices' "$(& $plural $new.Count 'nouvel appareil' 'nouveaux appareils') depuis le dernier scan" 'Si tu ne les reconnais pas, regarde la liste des appareils dans la page de ta box et change le mot de passe du Wi-Fi.' @($new | ForEach-Object { & $devText $_ }) @($boxAct) }
    if ($unk.Count) { Add-AuditCheck $list 'info' 'devices' "$(& $plural $unk.Count 'appareil non identifié' 'appareils non identifiés')" 'Leur nom et leur fabricant sont cachés : souvent des téléphones récents qui masquent leur adresse. Tu peux les retrouver dans la liste des appareils de ta box.' @($unk | ForEach-Object { & $devText $_ }) @($boxAct) }
    if (-not $new.Count -and -not $unk.Count) { Add-AuditCheck $list 'ok' 'devices' 'Tous les appareils sont identifiés' 'Aucun appareil inconnu ou nouveau sur ton réseau.' }

    # Ce PC
    $pc = $R.Pc
    if (@($pc.FirewallOff).Count) {
        Add-AuditCheck $list 'bad' 'pc' 'Pare-feu de Windows désactivé' "Il est coupé sur : $(@($pc.FirewallOff) -join ', '). Ton PC accepte alors toutes les connexions venant du réseau." $null @(
            @{ Label = 'Réactiver le pare-feu'; After = $rerun; Confirm = 'Réactiver le pare-feu de Windows sur tous les réseaux ?'; Script = { Set-NetFirewallProfile -Profile Domain, Public, Private -Enabled True } })
    } else {
        Add-AuditCheck $list 'ok' 'pc' 'Pare-feu de Windows activé' 'Il bloque les connexions non voulues vers ce PC.'
    }
    if ($pc.Smb1 -or $pc.Smb1Client) {
        Add-AuditCheck $list 'bad' 'pc' 'Ancien partage de fichiers SMBv1 activé' 'Cette vieille version du partage de fichiers Windows est la faille utilisée par le virus WannaCry. Plus aucun appareil récent n''en a besoin.' $null @(
            @{ Label = 'Désactiver SMBv1'; Arg = [bool]$pc.Smb1Client; After = $rerun
               Confirm = 'Désactiver SMBv1 ? Un redémarrage peut être nécessaire. Seuls de très vieux appareils (NAS ou imprimantes d''avant 2010) pourraient ne plus accéder à ce PC.'
               Script = {
                   param($client)
                   Set-SmbServerConfiguration -EnableSMB1Protocol $false -Force -Confirm:$false -ErrorAction SilentlyContinue
                   if ($client) {
                       Set-Status 'Désactivation de SMBv1...'
                       [void](Invoke-Async { Disable-WindowsOptionalFeature -Online -FeatureName SMB1Protocol -NoRestart -ErrorAction SilentlyContinue | Out-Null })
                   }
               } })
    } else {
        Add-AuditCheck $list 'ok' 'pc' 'SMBv1 désactivé' 'L''ancienne version du partage de fichiers (faille WannaCry) est coupée.'
    }
    if ($pc.Rdp) {
        Add-AuditCheck $list 'warn' 'pc' 'Bureau à distance activé' 'N''importe qui sur ton réseau peut essayer de se connecter à ce PC avec ton mot de passe Windows. Si tu ne t''en sers pas, désactive le.' $null @(
            @{ Label = 'Désactiver'; After = $rerun
               Confirm = 'Désactiver le Bureau à distance ? Tu pourras revenir en arrière depuis l''onglet Sauvegarde.'
               Script = { Set-Reg 'HKLM:\SYSTEM\CurrentControlSet\Control\Terminal Server' 'fDenyTSConnections' 1; Update-BackupSummary } })
    } else {
        Add-AuditCheck $list 'ok' 'pc' 'Bureau à distance désactivé' 'Personne ne peut prendre le contrôle de ce PC à distance.'
    }
    $cat = [string]$pc.Category
    if ($w -and $auth -match $openRe -and $cat -eq 'Private') {
        Add-AuditCheck $list 'bad' 'pc' 'Wi-Fi ouvert réglé en réseau privé' 'Sur un Wi-Fi sans mot de passe, ton PC doit être en mode public pour rester invisible des autres appareils.' $null @(
            @{ Label = 'Passer en public'; Arg = $Net.IfIndex; After = $rerun; Script = { param($i) Set-NetConnectionProfile -InterfaceIndex $i -NetworkCategory Public -ErrorAction Stop } })
    } elseif ($cat -eq 'Public') {
        Add-AuditCheck $list 'ok' 'pc' 'Réseau en mode public' 'Ton PC est invisible pour les autres appareils du réseau. C''est le réglage le plus sûr (le partage de fichiers entre PC est alors bloqué).'
    } elseif ($cat -eq 'Private') {
        Add-AuditCheck $list 'ok' 'pc' 'Réseau en mode privé' 'Normal chez toi : les autres appareils peuvent voir ce PC. Sur un Wi-Fi d''hôtel ou de gare, choisis toujours « Public ».'
    }
    $rightsTxt = @{ Full = 'contrôle total'; Change = 'modification'; Read = 'lecture' }
    $shares = @(foreach ($l in @($pc.Shares)) { $x = ([string]$l) -split '\|', 3; [pscustomobject]@{ Name = $x[0]; Path = $x[1]; Every = $x[2] } })
    $everyone = @($shares | Where-Object { $_.Every })
    $shareAct = @{ Label = 'Gérer les partages'; NoRefresh = $true; Script = { Start-Process 'fsmgmt.msc' } }
    if ($everyone.Count) {
        Add-AuditCheck $list 'warn' 'pc' "$(& $plural $everyone.Count 'dossier partagé' 'dossiers partagés') avec tout le monde" 'N''importe quel appareil de ton réseau peut ouvrir ces dossiers. Vérifie que c''est voulu, et retire « Tout le monde » des autorisations sinon.' @($everyone | ForEach-Object { "$($_.Name) ($($_.Path)) : $((@($_.Every -split ',') | ForEach-Object { if ($rightsTxt[$_]) { $rightsTxt[$_] } else { $_ } }) -join ', ')" }) @($shareAct)
    } elseif ($shares.Count) {
        Add-AuditCheck $list 'info' 'pc' "$(& $plural $shares.Count 'dossier partagé' 'dossiers partagés') sur le réseau" 'Seules les personnes autorisées peuvent y accéder.' @($shares | ForEach-Object { "$($_.Name) ($($_.Path))" }) @($shareAct)
    } else {
        Add-AuditCheck $list 'ok' 'pc' 'Aucun dossier partagé' 'Tes fichiers ne sont pas accessibles depuis le réseau.'
    }
    $list
}

function Get-AuditColor([int]$Score) { if ($Score -ge 85) { $Colors.ok } elseif ($Score -ge 60) { $Colors.warn } else { $Colors.bad } }
function Get-AuditLabel([int]$Score) { if ($Score -ge 85) { 'Réseau bien protégé' } elseif ($Score -ge 60) { 'Quelques points à améliorer' } else { 'Réseau à risque' } }

function New-AuditLine($C) {
    $g = New-Grid @('Auto', '*')
    $g.Margin = New-Thickness 0 5 0 5
    $dot = New-Object System.Windows.Shapes.Ellipse
    $dot.Width = 9; $dot.Height = 9; $dot.Fill = Get-Brush $Colors[$C.Status]
    $dot.Margin = New-Thickness 2 6 12 0; $dot.VerticalAlignment = 'Top'
    Add-ToGrid $g $dot 0
    $sp = New-Object System.Windows.Controls.StackPanel
    $head = New-Object System.Windows.Controls.TextBlock
    $head.TextWrapping = 'Wrap'
    $cat = New-Object System.Windows.Documents.Run ("$($AuditCats[$C.Cat])   ")
    $cat.Foreground = Get-Brush '#5B6475'; $cat.FontSize = 11.5; $cat.FontWeight = 'SemiBold'
    $tt = New-Object System.Windows.Documents.Run $C.Title
    $tt.Foreground = Get-Brush '#FFFFFF'; $tt.FontSize = 13.5; $tt.FontWeight = 'SemiBold'
    [void]$head.Inlines.Add($cat); [void]$head.Inlines.Add($tt)
    [void]$sp.Children.Add($head)
    [void]$sp.Children.Add((New-Text $C.Detail 12 '#9AA3B2'))
    foreach ($i in @($C.Items | Select-Object -First 6)) {
        $it = New-Text "•  $i" 11.5 '#6B7486'
        $it.TextTrimming = 'CharacterEllipsis'; $it.TextWrapping = 'NoWrap'; $it.ToolTip = [string]$i
        [void]$sp.Children.Add($it)
    }
    Add-ToGrid $g $sp 1
    $g
}

function Show-NetAuditResult($A) {
    $body = $ui.TestBody
    $bad = @($A.Checks | Where-Object { $_.Status -eq 'bad' }).Count
    $warn = @($A.Checks | Where-Object { $_.Status -eq 'warn' }).Count
    $ok = @($A.Checks | Where-Object { $_.Status -eq 'ok' }).Count
    [void]$body.Children.Add((New-SectionTitle 'RÉSULTAT'))
    $row = New-Grid @('Auto', '*')
    $col = Get-AuditColor $A.Score
    $gauge = New-Gauge 'Sécurité du réseau' $A.Score 100 '{0:N0}' 'sur 100' $col 0
    Add-ToGrid $row $gauge.El 0
    $sp = New-Object System.Windows.Controls.StackPanel
    $sp.VerticalAlignment = 'Center'
    $sp.Margin = New-Thickness 20 0 0 0
    [void]$sp.Children.Add((New-Text (Get-AuditLabel $A.Score) 20 $col -Bold))
    $sub = New-Text "Audit du $($A.Date.ToString('dd/MM/yyyy à HH:mm')), $($A.Count) appareils vérifiés." 12.5 '#9AA3B2'
    $sub.Margin = New-Thickness 0 4 0 0
    [void]$sp.Children.Add($sub)
    $chips = New-Object System.Windows.Controls.WrapPanel
    $chips.Margin = New-Thickness 0 10 0 0
    foreach ($c in @(@($bad, 'à corriger', 'bad'), @($warn, 'à surveiller', 'warn'), @($ok, 'OK', 'ok'))) {
        if (-not $c[0] -and $c[2] -ne 'ok') { continue }
        $b = New-Badge "$($c[0]) $($c[1])" $Colors[$c[2]]
        $b.Margin = New-Thickness 0 0 8 6
        [void]$chips.Children.Add($b)
    }
    [void]$sp.Children.Add($chips)
    $exp = New-Button 'Exporter le rapport'
    $exp.HorizontalAlignment = 'Left'
    $exp.Margin = New-Thickness 0 6 0 0
    $exp.Add_Click({ Invoke-Safe { Export-NetAudit } })
    [void]$sp.Children.Add($exp)
    Add-ToGrid $row $sp 1
    [void]$body.Children.Add($row)

    foreach ($sec in @(@('bad', 'À CORRIGER'), @('warn', 'À SURVEILLER'), @('info', 'BON À SAVOIR'))) {
        $items = @($A.Checks | Where-Object { $_.Status -eq $sec[0] })
        if (-not $items.Count) { continue }
        [void]$body.Children.Add((New-SectionTitle $sec[1]))
        $i = 0
        foreach ($c in $items) {
            $card = New-SecurityCard $c
            $card.Margin = New-Thickness 0 4 0 4
            $card.Opacity = 0
            Start-WpfAnim $card ([System.Windows.UIElement]::OpacityProperty) 1 350 (80 * $i)
            [void]$body.Children.Add($card)
            $i++
        }
    }
    $oks = @($A.Checks | Where-Object { $_.Status -eq 'ok' })
    if ($oks.Count) {
        [void]$body.Children.Add((New-SectionTitle 'TOUT VA BIEN'))
        $box = New-Object System.Windows.Controls.Border
        $box.Background = Get-Brush '#1A1F29'
        $box.CornerRadius = [System.Windows.CornerRadius]::new(10)
        $box.Padding = New-Thickness 14 8 14 8
        $box.Margin = New-Thickness 0 4 0 0
        $list = New-Object System.Windows.Controls.StackPanel
        foreach ($c in $oks) { [void]$list.Children.Add((New-AuditLine $c)) }
        $box.Child = $list
        [void]$body.Children.Add($box)
    }
    [void]$body.Children.Add((New-SectionTitle 'BONS RÉFLEXES'))
    foreach ($t in $AuditTips) {
        $tip = New-Text "•  $t" 13 '#C9CED8'
        $tip.Margin = New-Thickness 2 3 0 3
        [void]$body.Children.Add($tip)
    }
    $st = if ($bad) { 'bad' } elseif ($warn) { 'warn' } else { 'ok' }
    Set-TestState $st $(switch ($st) { 'bad' { 'Points à corriger' } 'warn' { 'À surveiller' } default { 'Tout va bien' } })
}

function Update-NetAuditCard {
    if (-not $script:NetAuditSaved -and (Test-Path -LiteralPath $AuditFile)) {
        try { $script:NetAuditSaved = ConvertFrom-Json (Get-Content -LiteralPath $AuditFile -Raw -Encoding UTF8) } catch {}
    }
    $s = $script:NetAuditSaved
    if (-not $s) {
        $ui.NetAuditText.Text = 'Vérifie la sécurité de ton Wi-Fi, de ta box, de tes appareils et de ce PC. Tu obtiens une note, un rapport clair et des corrections en un clic.'
        return
    }
    $col = Get-AuditColor ([int]$s.Score)
    $ui.NetAuditScore.Text = "$($s.Score)"
    $ui.NetAuditScore.Foreground = Get-Brush $col
    $ui.NetAuditScoreBox.BorderBrush = Get-Brush $col
    $parts = @()
    if ([int]$s.Bad) { $parts += "$($s.Bad) à corriger" }
    if ([int]$s.Warn) { $parts += "$($s.Warn) à surveiller" }
    $detail = if ($parts.Count) { $parts -join ', ' } else { 'rien à corriger' }
    $ui.NetAuditText.Text = "$(Get-AuditLabel ([int]$s.Score)). Dernier audit le $($s.Date) : $detail."
    $ui.BtnNetAudit.Content = 'Relancer l''audit'
    $ui.BtnNetAuditView.Visibility = if ($script:NetAudit) { 'Visible' } else { 'Collapsed' }
}

function Invoke-NetAudit {
    if ($script:TestRunning) { return }
    if ($script:NetScanning) { Show-Message 'Attends la fin du scan du réseau, puis relance l''audit.'; return }
    if (-not $script:NetList) { Invoke-NetworkScan }
    if (-not $script:NetList) { return }
    $net = Get-ActiveNet
    if (-not $net -or -not $net.Ip) { Show-Message 'Aucune connexion réseau détectée.'; return }
    $script:DevPing = $null
    if ($script:MonitorTimer) { $script:MonitorTimer.Stop(); $script:MonitorTimer = $null }
    $script:TestRunning = $true
    $ui.BtnNetAudit.IsEnabled = $false
    $ui.BtnNetAuditView.IsEnabled = $false
    Show-TestPanel @{ Tag = 'SÉCU'; Title = 'Audit de sécurité du réseau'; Sub = 'Wi-Fi, box, DNS, appareils et ce PC' }
    Set-TestState 'run' 'Audit en cours'
    Set-TestButtons 'run'
    $ui.BtnTestStop.Visibility = 'Collapsed'
    $body = $ui.TestBody
    $stepper = New-Stepper ([ordered]@{ wifi = 'Wi-Fi'; box = 'Box et UPnP'; dns = 'DNS'; devices = 'Appareils'; pc = 'Ce PC' })
    [void]$body.Children.Add($stepper.El)
    $count = @($script:NetList | Where-Object { -not $_.Self }).Count
    $wait = New-Text "Vérification de ton Wi-Fi, de ta box et de $count appareils. Ça prend une vingtaine de secondes, rien n'est modifié pendant l'audit." 13 '#9AA3B2'
    $wait.Margin = New-Thickness 0 6 0 0
    [void]$body.Children.Add($wait)
    [OGNative]::Cancel = $false; [OGNative]::Progress = 0; [OGNative]::Phase = ''
    $script:CurTest = @{ Def = @{}; Stepper = $stepper; Chart = $null; Freq = @{} }
    $script:TestTimer.Start()
    try {
        $ips = [string[]]@($script:NetList | Where-Object { -not $_.Self } | ForEach-Object { $_.Ip })
        $r = Invoke-Async $NetAuditWork @{ Ip = $net.Ip; Gateway = $net.Gateway; If = $net.IfIndex; Ips = $ips; Ports = $AuditPorts } | Select-Object -First 1
    } finally {
        $script:TestTimer.Stop()
        $script:CurTest = $null
        $script:TestRunning = $false
        $ui.BtnNetAudit.IsEnabled = $true
        $ui.BtnNetAuditView.IsEnabled = $true
        Set-TestButtons 'done'
    }
    $script:LastRun = @{ Fn = { Invoke-NetAudit }; Tile = $null; Ctx = $null }
    if (-not $r -or $r.Error) {
        if ($r.Error) { Write-Log "Audit réseau: $($r.Error)" }
        Set-TestState 'bad' 'Échec'
        [void]$body.Children.Add((New-Verdict 'bad' "L'audit n'a pas pu aller au bout : $(if ($r.Error) { $r.Error } else { 'erreur inconnue' })"))
        return
    }
    Update-Stepper $stepper '' -AllDone
    $body.Children.Remove($wait)
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = '100 %'
    $checks = @(Get-NetAuditChecks $r $script:NetList $net)
    $bad = @($checks | Where-Object { $_.Status -eq 'bad' }).Count
    $warn = @($checks | Where-Object { $_.Status -eq 'warn' }).Count
    $score = [int][math]::Max(0.0, 100.0 - 20 * $bad - 7 * $warn)
    $script:NetAudit = @{ Score = $score; Date = (Get-Date); Checks = $checks; Count = $count; Net = $net }
    $script:NetAuditSaved = [pscustomobject]@{ Score = $score; Date = (Get-Date).ToString('dd/MM/yyyy à HH:mm'); Bad = $bad; Warn = $warn }
    try { ConvertTo-Json -InputObject $script:NetAuditSaved | Set-Content -LiteralPath $AuditFile -Encoding UTF8 } catch {}
    Show-NetAuditResult $script:NetAudit
    Show-ResultTop
    Update-NetAuditCard
    Set-Status "Audit du réseau : $score sur 100."
}

function Show-NetAuditReport {
    if (-not $script:NetAudit -or $script:TestRunning) { return }
    Show-TestPanel @{ Tag = 'SÉCU'; Title = 'Audit de sécurité du réseau'; Sub = 'Wi-Fi, box, DNS, appareils et ce PC' }
    Set-TestButtons 'done'
    $script:LastRun = @{ Fn = { Invoke-NetAudit }; Tile = $null; Ctx = $null }
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    Show-NetAuditResult $script:NetAudit
}

function Export-NetAudit {
    $A = $script:NetAudit
    if (-not $A) { return }
    $dlg = New-Object Microsoft.Win32.SaveFileDialog
    $dlg.Filter = 'Page web (*.html)|*.html'
    $dlg.FileName = "Audit réseau OptiGame $(Get-Date -Format 'yyyy-MM-dd').html"
    $dlg.InitialDirectory = [Environment]::GetFolderPath('Desktop')
    if ($dlg.ShowDialog($Window) -ne $true) { return }
    $enc = { param($s) [System.Net.WebUtility]::HtmlEncode([string]$s) }
    $labels = @{ bad = 'À corriger'; warn = 'À surveiller'; info = 'Bon à savoir'; ok = 'Tout va bien' }
    $sections = foreach ($st in 'bad', 'warn', 'info', 'ok') {
        $items = @($A.Checks | Where-Object { $_.Status -eq $st })
        if (-not $items.Count) { continue }
        $cards = foreach ($c in $items) {
            $li = if ($c.Items) { '<ul>' + ((@($c.Items) | ForEach-Object { "<li>$(& $enc $_)</li>" }) -join '') + '</ul>' } else { '' }
            "<div class='f'><span class='dot' style='background:$($Colors[$st])'></span><div><span class='cat'>$(& $enc $AuditCats[$c.Cat])</span><b>$(& $enc $c.Title)</b><p>$(& $enc $c.Detail)</p>$li</div></div>"
        }
        "<h2 style='color:$($Colors[$st])'>$(& $enc $labels[$st])</h2>`n$($cards -join "`n")"
    }
    $tips = ($AuditTips | ForEach-Object { "<li>$(& $enc $_)</li>" }) -join ''
    $col = Get-AuditColor $A.Score
    $html = @"
<!DOCTYPE html>
<html lang="fr"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Audit réseau OptiGame</title>
<style>
body{margin:0;background:#0E1014;color:#E6E8EE;font:15px/1.5 'Segoe UI',system-ui,sans-serif}
main{max-width:860px;margin:0 auto;padding:32px 16px}
h1{margin:0;font-size:28px}h1 span{color:#22D37A}
h2{margin:32px 0 12px;font-size:18px}
.sub{color:#9AA3B2}
.score{display:flex;align-items:center;gap:20px;background:#181C24;border:1px solid #232937;border-radius:12px;padding:20px;margin-top:24px}
.score b{font-size:48px;color:$col}
.f{display:flex;gap:14px;background:#181C24;border:1px solid #232937;border-radius:12px;padding:12px 16px;margin-bottom:8px}
.f p{margin:2px 0 0;color:#9AA3B2;font-size:14px}
.f ul{margin:6px 0 0;padding-left:18px;color:#C9CED8;font-size:13px}
.cat{display:inline-block;margin-right:10px;color:#5B6475;font-size:12px;font-weight:600;text-transform:uppercase}
.dot{flex:none;width:12px;height:12px;border-radius:50%;margin-top:6px}
.tips{background:#181C24;border:1px solid #232937;border-radius:12px;padding:12px 16px 12px 34px;color:#C9CED8}
</style></head><body><main>
<h1>Opti<span>Game</span></h1>
<div class="sub">Audit de sécurité du réseau, depuis $(& $enc $env:COMPUTERNAME), le $($A.Date.ToString('dd/MM/yyyy à HH:mm'))</div>
<div class="score"><b>$($A.Score)</b><div><div style="font-size:20px;font-weight:600">$(& $enc (Get-AuditLabel $A.Score))</div><div class="sub">Note de sécurité sur 100, $($A.Count) appareils vérifiés</div></div></div>
$($sections -join "`n")
<h2>Bons réflexes</h2>
<ul class="tips">$tips</ul>
</main></body></html>
"@
    Set-Content -Path $dlg.FileName -Value $html -Encoding UTF8
    Start-Process $dlg.FileName
    Set-Status 'Rapport d''audit exporté.'
}

# ---------------------------------------------------------------------------
# Mises à jour (GitHub)
# ---------------------------------------------------------------------------
# Script autonome: il tourne dans un fil séparé pour ne pas figer la fenêtre.
$GetReleaseScript = {
    param($repo)
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $r = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers @{ 'User-Agent' = 'OptiGame' } -TimeoutSec 10
        $asset = @($r.assets | Where-Object { $_.name -eq 'OptiGame.zip' }) | Select-Object -First 1
        if (-not $asset) { return @{ Error = 'Aucun fichier OptiGame.zip dans la dernière version publiée.' } }
        @{ Version = ([string]$r.tag_name -replace '^[vV]', ''); Url = [string]$asset.browser_download_url }
    } catch { @{ Error = $_.Exception.Message } }
}

function Test-NewerVersion([string]$Remote, [string]$Local) {
    try { return ([version]$Remote -gt [version]$Local) } catch { return $false }
}

function Invoke-UpdateCheck([switch]$Manual) {
    $ui.UpdateStatus.Text = 'Recherche d''une nouvelle version...'
    $r = Invoke-Async $GetReleaseScript $UpdateRepo | Select-Object -First 1
    if (-not $r -or $r.Error) {
        $ui.UpdateStatus.Text = "Version $AppVersion. Impossible de vérifier les mises à jour (pas de connexion ?)."
        if ($r.Error) { Write-Log "Mise à jour: $($r.Error)" }
        if ($Manual) { Show-Message "Impossible de vérifier les mises à jour pour le moment.`n`nVérifie ta connexion Internet et réessaie." 'Warning' }
        return
    }
    if (Test-NewerVersion $r.Version $AppVersion) {
        $script:PendingUpdate = $r
        $ui.UpdateText.Text = "Nouvelle version $($r.Version) disponible (tu as la $AppVersion)."
        $ui.UpdateBanner.Visibility = 'Visible'
        $ui.UpdateStatus.Text = "Version $AppVersion. La version $($r.Version) est disponible."
        $ui.BtnCheckUpdate.Content = 'Mettre à jour'
    } else {
        $ui.UpdateStatus.Text = "Version $AppVersion : tu as la dernière version."
        if ($Manual) { Set-Status 'OptiGame est à jour.' }
    }
}

function Install-Update {
    $rel = $script:PendingUpdate
    if (-not $rel) { return }
    if (-not (Confirm-Action "Installer la version $($rel.Version) d'OptiGame ?`n`nL'app va se fermer, se mettre à jour puis se relancer. Tes réglages et ta sauvegarde sont conservés.")) { return }
    Set-Busy $true
    $ui.UpdateBanner.Visibility = 'Collapsed'
    Set-Status "Téléchargement de la version $($rel.Version)..."
    $tmp = Join-Path $env:TEMP "OptiGame-maj-$(Get-Random)"
    $res = Invoke-Async {
        param($a)
        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            New-Item -ItemType Directory -Force -Path $a.Dir | Out-Null
            $zip = Join-Path $a.Dir 'OptiGame.zip'
            Invoke-WebRequest -Uri $a.Url -OutFile $zip -UseBasicParsing -Headers @{ 'User-Agent' = 'OptiGame' } -TimeoutSec 120
            Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $a.Dir 'x') -Force
            'OK'
        } catch { $_.Exception.Message }
    } @{ Url = $rel.Url; Dir = $tmp } | Select-Object -First 1
    $src = Join-Path $tmp 'x\OptiGame'
    if ("$res" -ne 'OK' -or -not (Test-Path -LiteralPath (Join-Path $src 'fichiers\OptiGame.ps1'))) {
        Write-Log "Échec de la mise à jour: $res"
        Show-Message "La mise à jour n'a pas pu être téléchargée.`n`n$res`n`nRéessaie plus tard." 'Warning'
        Set-Status 'Mise à jour annulée.'
        return
    }
    Set-Status 'Installation de la mise à jour...'
    $errors = @()
    foreach ($f in Get-ChildItem -LiteralPath $src -Recurse -File) {
        $dest = Join-Path $appRoot $f.FullName.Substring($src.Length + 1)
        if ((Test-Path -LiteralPath $dest) -and (Get-FileHash -LiteralPath $dest).Hash -eq (Get-FileHash -LiteralPath $f.FullName).Hash) { continue }
        try {
            New-Item -ItemType Directory -Force -Path (Split-Path $dest -Parent) | Out-Null
            Copy-Item -LiteralPath $f.FullName -Destination $dest -Force -ErrorAction Stop
            Unblock-File -LiteralPath $dest -ErrorAction SilentlyContinue
        } catch { $errors += "$($f.Name): $($_.Exception.Message)" }
    }
    [IO.Directory]::Delete($tmp, $true)
    if ($errors) {
        Write-Log "Mise à jour incomplète: $($errors -join ' | ')"
        Show-Message "La mise à jour n'a pas pu remplacer tous les fichiers :`n`n$($errors -join "`n")" 'Warning'
        return
    }
    Write-Log "Mise à jour installée: $AppVersion -> $($rel.Version)"
    $script:Relaunch = Join-Path $appRoot 'fichiers\OptiGame.ps1'
    $Window.Close()
}

# ---------------------------------------------------------------------------
# Événements
# ---------------------------------------------------------------------------
$IconPath = Join-Path $PSScriptRoot 'OptiGame.ico'
if (Test-Path $IconPath) {
    try {
        # Chargée en mémoire pour ne pas bloquer le fichier (il doit pouvoir être remplacé par une mise à jour).
        $iconStream = New-Object IO.MemoryStream (, [IO.File]::ReadAllBytes($IconPath))
        $script:IconFrames = [System.Windows.Media.Imaging.BitmapDecoder]::Create($iconStream, 'None', 'OnLoad').Frames
        $Window.Icon = $script:IconFrames | Sort-Object PixelWidth | Where-Object { $_.PixelWidth -ge 32 } | Select-Object -First 1
    } catch { Write-Log "Icône: $_" }
}

$Window.Add_SourceInitialized({
    try { [OGNative]::SetDarkTitleBar((New-Object System.Windows.Interop.WindowInteropHelper $Window).Handle) } catch {}
})

$Window.Add_Closed({
    $Live.Run = $false
    if ($script:LiveTimer) { $script:LiveTimer.Stop() }
})

$ui.BtnFixAll.Add_Click({ Open-FixAll })
$script:SheetMode = 'fix'
$ui.SheetClose.Add_Click({ Close-Sheet })
$ui.OverlayBackdrop.Add_MouseLeftButtonUp({ if ($script:SheetMode -ne 'display') { Close-Sheet } })
$ui.SheetRun.Add_Click({
    if ($script:SheetMode -eq 'display') { $script:DisplayChoice = 'keep'; return }
    Invoke-Safe { Invoke-SheetRun }
})
$ui.SheetOpen.Add_Click({ if ($ui.SheetOpen.Tag) { Close-Sheet; Invoke-FindingAction $ui.SheetOpen.Tag } })
$ui.SheetIgnore.Add_Click({
    switch ($script:SheetMode) {
        'display' { $script:DisplayChoice = 'revert' }
        'result'  { Invoke-Safe { Invoke-UndoLastRun } }
        default {
            $f = $script:SheetItems[0]
            Invoke-Safe { Set-IgnoreFinding $f (-not ($script:Ignored -contains $f.Id)) }
        }
    }
})
$Window.Add_SizeChanged({ $ui.TestScroll.MaxHeight = [math]::Max(300.0, $Window.ActualHeight - 300) })
$ui.BtnTestStop.Add_Click({
    [OGNative]::Cancel = $true
    Set-TestState 'info' 'Arrêt en cours...'
    if ($script:ScanRunning -and (Test-Path $MpCmd)) { Start-Process -FilePath $MpCmd -ArgumentList '-Cancel' -WindowStyle Hidden }
})
foreach ($n in 'BtnScanQuick', 'BtnScanFull', 'BtnScanFolder', 'BtnScanUpdate') { [void]$script:SecButtons.Add($ui[$n]) }
$ui.BtnScanQuick.Add_Click({ Invoke-Safe { Invoke-DefenderScan 'QuickScan' } })
$ui.BtnScanFull.Add_Click({
    if (-not (Confirm-Action "L'analyse complète vérifie tous les fichiers du PC : elle peut durer une heure ou plus. Tu peux continuer à utiliser ton PC pendant ce temps. Lancer l'analyse ?")) { return }
    Invoke-Safe { Invoke-DefenderScan 'FullScan' }
})
$ui.BtnScanFolder.Add_Click({ Invoke-Safe { Invoke-FolderScan } })
$ui.BtnScanUpdate.Add_Click({ Invoke-Safe { Update-Definitions } })
$ui.BtnNetScan.Add_Click({ Invoke-Safe { Invoke-NetworkScan } })
$ui.BtnNetAudit.Add_Click({ Invoke-Safe { Invoke-NetAudit } })
$ui.BtnNetAuditView.Add_Click({ Invoke-Safe { Show-NetAuditReport } })
$ui.BtnSecRefresh.Add_Click({ Invoke-Safe { Update-SecurityTab } })
$ui.BtnTestClose.Add_Click({ Hide-TestPanel })
$ui.BtnTestX.Add_Click({ Hide-TestPanel })
$ui.TestBackdrop.Add_MouseLeftButtonUp({ Hide-TestPanel })
$ui.BtnTestAgain.Add_Click({
    $run = $script:LastRun
    if ($run -and $run.Fn) { Invoke-Safe { & $run.Fn $run.Tile $run.Ctx } }
})
$Window.Add_KeyDown({
    param($s, $e)
    if ($e.Key -eq 'Escape' -and $ui.TestOverlay.Visibility -eq 'Visible') { Hide-TestPanel; return }
    if ($e.Key -ne 'Escape' -or $ui.Overlay.Visibility -ne 'Visible') { return }
    if ($script:SheetMode -eq 'display') { $script:DisplayChoice = 'revert' } else { Close-Sheet }
})

$ui.BtnAnalyze.Add_Click({ Invoke-Safe { Invoke-Analysis } })
$ui.BtnSelectAll.Add_Click({
    foreach ($r in $script:TweakRows) { if ($r.CheckBox.IsEnabled) { $r.CheckBox.IsChecked = ($r.Tweak.Recommended -ne $false) } }
})
$ui.BtnApply.Add_Click({ Invoke-Safe { Invoke-ApplyTweaks } })
$ui.BtnRefreshStartup.Add_Click({ Invoke-Safe { Update-StartupList } })
$ui.BtnDisableStartup.Add_Click({ Invoke-Safe { Disable-RecommendedStartup } })
$ui.BtnPing.Add_Click({ Invoke-Safe { Invoke-NetTest } })
$ui.BtnDnsApply.Add_Click({ Invoke-Safe { Set-Dns $ui.DnsCombo.SelectedIndex } })
$ui.BtnDnsFlush.Add_Click({ Invoke-Safe { Clear-DnsClientCache; Set-Status 'Cache DNS vidé.' } })
$ui.BtnCleanScan.Add_Click({ Invoke-Safe { Invoke-CleanScan } })
$ui.BtnClean.Add_Click({ Invoke-Safe { Invoke-Clean } })
$ui.BtnUndo.Add_Click({ Invoke-Safe { Invoke-UndoAll } })
$ui.BtnRestorePoint.Add_Click({
    Invoke-Safe {
        Set-Busy $true
        $r = Invoke-Async {
            try { Checkpoint-Computer -Description 'OptiGame (manuel)' -RestorePointType MODIFY_SETTINGS -ErrorAction Stop; 'OK' }
            catch { $_.Exception.Message }
        }
        if ("$r" -eq 'OK') { Set-Status 'Point de restauration créé.'; Show-Message 'Point de restauration créé.' }
        else { Show-Message "Impossible de créer le point de restauration:`n`n$r`n`nLa protection du système est peut-être désactivée (Panneau de configuration > Système > Protection du système)." 'Warning' }
    }
})
$ui.BtnOpenRestore.Add_Click({ Start-Process 'rstrui.exe' })
$ui.BtnExport.Add_Click({ Invoke-Safe { Export-Report } })
$ui.Tabs.Add_SelectionChanged({
    param($s, $e)
    if ($e.OriginalSource -ne $ui.Tabs) { return }
    Update-NavBar
    if ($ui.Tabs.SelectedIndex -eq $HubIndex -and $script:HubStats) { Invoke-Safe { Update-Hub }; return }
    if ($ui.Tabs.SelectedIndex -eq $NetIndex -and -not $script:NetBuilt) {
        $script:NetBuilt = $true
        Invoke-Safe { Show-NetHeroIdle; Update-NetAuditCard; Update-NetScanInfo }
        return
    }
    if ($ui.Tabs.SelectedIndex -eq 6 -and -not $script:SecurityBuilt) {
        $script:SecurityBuilt = $true
        Invoke-Safe { Update-SecurityTab }
        return
    }
    if ($ui.Tabs.SelectedIndex -eq 5 -and -not $script:TestsBuilt) {
        $script:TestsBuilt = $true
        Set-Status 'Préparation des tests...'
        Invoke-Safe { Build-TestsTab }
        Set-Status 'Choisis un composant à tester.'
    }
})
$ui.BtnUpdate.Add_Click({ Invoke-Safe { Install-Update } })
$ui.BtnUpdateLater.Add_Click({ $ui.UpdateBanner.Visibility = 'Collapsed' })
$ui.BtnCheckUpdate.Add_Click({
    if ($script:PendingUpdate) { Invoke-Safe { Install-Update } } else { Invoke-Safe { Invoke-UpdateCheck -Manual } }
})

$Window.Add_ContentRendered({
    $v = $ui.Tabs.Template.FindName('VersionText', $ui.Tabs)
    if ($v) { $v.Text = "Version $AppVersion" }
    $logo = $ui.Tabs.Template.FindName('LogoImg', $ui.Tabs)
    if ($logo -and $script:IconFrames) {
        $logo.Source = $script:IconFrames | Sort-Object PixelWidth | Where-Object { $_.PixelWidth -ge 128 } | Select-Object -First 1
    }
    $script:NavBar = $ui.Tabs.Template.FindName('NavBar', $ui.Tabs)
    $script:NavCrumb = $ui.Tabs.Template.FindName('NavCrumb', $ui.Tabs)
    $back = $ui.Tabs.Template.FindName('NavBack', $ui.Tabs)
    if ($back) { $back.Add_Click({ Show-Page $HubIndex }) }
    Build-Hub
    $ui.Tabs.SelectedIndex = $HubIndex
    Update-Hub
    Start-Live
    Invoke-Safe {
        Invoke-Analysis
        Build-GamingTab
        Update-StartupList
        Update-NetInfo
        Update-BackupSummary
    }
    Invoke-Safe {
        if (-not $script:SecurityBuilt) { $script:SecurityBuilt = $true; Update-SecurityTab }
        Update-Hub
        Set-Status 'Prêt.'
    }
    try { Invoke-UpdateCheck } catch { Write-Log "Vérification de mise à jour: $_" }
})

# ---------------------------------------------------------------------------
# Lancement
# ---------------------------------------------------------------------------
Import-Backup
Import-Ignored
$script:KnownDevices = @{}
$script:RestoreDone = $false
$script:Build = [int](Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' -ErrorAction SilentlyContinue).CurrentBuildNumber
$script:Pool = [RunspaceFactory]::CreateRunspacePool(1, 4)
$script:Pool.Open()
$pfPs = [PowerShell]::Create()
$pfPs.RunspacePool = $script:Pool
[void]$pfPs.AddScript($AnalysisDataWork.ToString()).AddArgument($env:SystemDrive)
$script:Prefetch = @{ PS = $pfPs; Handle = $pfPs.BeginInvoke() }
$script:TweakRows = @()
$script:CleanRows = @()
$script:PingResults = @()
Write-Log "Démarrage OptiGame $AppVersion"
[void]$Window.ShowDialog()
if ($script:Relaunch -and (Test-Path -LiteralPath $script:Relaunch)) {
    Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', "`"$script:Relaunch`"")
}
