// OptiGame : fonctions natives (écrans, souris, barre de titre sombre, tests, réseau).
// Compilé au lancement par OptiGame.ps1 (Add-Type).
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

    // Quand vrai, NetSpeed ne touche ni à Progress ni à LiveValue (utilisé pendant la mesure de latence en charge).
    public static volatile bool QuietSpeed;

    // Latence en charge (« bufferbloat ») : ping au repos, puis pendant un téléchargement, puis pendant un envoi.
    // Retourne { ping repos, ping téléchargement, ping envoi, pertes repos %, pertes téléch. %, pertes envoi %,
    //            débit téléch. Mb/s, débit envoi Mb/s, gigue repos } ou null si annulé.
    public static double[] LoadedLatency(string[] down, string[] up, string host, double idleSec, double loadSec)
    {
        var res = new double[9];
        var ping = new System.Net.NetworkInformation.Ping();
        Func<double, double, double, double[]> measure = (seconds, p0, p1) =>
        {
            var times = new List<double>();
            int sent = 0, lost = 0;
            var sw = System.Diagnostics.Stopwatch.StartNew();
            while (sw.Elapsed.TotalSeconds < seconds && !Cancel)
            {
                var t0 = sw.Elapsed.TotalMilliseconds;
                sent++;
                try
                {
                    var r = ping.Send(host, 1500);
                    if (r.Status == System.Net.NetworkInformation.IPStatus.Success) { times.Add(r.RoundtripTime); LiveValue = r.RoundtripTime; }
                    else lost++;
                }
                catch { lost++; }
                Progress = p0 + (p1 - p0) * Math.Min(1, sw.Elapsed.TotalSeconds / seconds);
                var wait = 150 - (int)(sw.Elapsed.TotalMilliseconds - t0);
                if (wait > 0) System.Threading.Thread.Sleep(wait);
            }
            double avg = -1, jit = 0;
            if (times.Count > 0)
            {
                times.Sort();
                avg = times[times.Count / 2];   // médiane : un ping isolé très lent ne fausse pas la note
                for (int i = 1; i < times.Count; i++) jit += Math.Abs(times[i] - times[i - 1]);
                if (times.Count > 1) jit /= times.Count - 1;
            }
            return new double[] { avg, sent > 0 ? 100.0 * lost / sent : 0, jit };
        };
        Func<string[], bool, double, double, double[]> loaded = (urls, upload, p0, p1) =>
        {
            double mbps = -1;
            QuietSpeed = true;
            var job = System.Threading.Tasks.Task.Run(() => { mbps = NetSpeed(urls, upload, loadSec, 8, 0, 0); });
            System.Threading.Thread.Sleep(1500);   // le temps que la connexion se remplisse
            var m = measure(loadSec - 2.0, p0, p1);
            try { job.Wait(); } catch { }
            QuietSpeed = false;
            return new double[] { m[0], m[1], mbps };
        };
        try
        {
            Phase = "idle";
            var idle = measure(idleSec, 0, 25);
            if (Cancel) return null;
            Phase = "down";
            var d = loaded(down, false, 25, 62);
            if (Cancel) return null;
            Phase = "up";
            var u = loaded(up, true, 62, 100);
            if (Cancel) return null;
            res[0] = idle[0]; res[1] = d[0]; res[2] = u[0];
            res[3] = idle[1]; res[4] = d[1]; res[5] = u[1];
            res[6] = d[2]; res[7] = u[2]; res[8] = idle[2];
            return res;
        }
        finally { QuietSpeed = false; ping.Dispose(); }
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
        long lastBytes = 0; double lastTime = 0; if (!QuietSpeed) LiveValue = 0;
        foreach (var th in ths)
        {
            while (!th.Join(100))
            {
                if (QuietSpeed) continue;
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
        if (!QuietSpeed) Progress = p1;
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
                        int ttl = t.Result.Options != null ? t.Result.Options.Ttl : 0;
                        lock (sync) { results.Add(addr + "|" + t.Result.RoundtripTime + "|" + ttl); }
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

    // Overlay : la souris traverse la fenêtre, elle ne prend jamais le focus et n'apparaît pas dans Alt+Tab.
    [DllImport("user32.dll")] static extern int GetWindowLong(IntPtr hwnd, int index);
    [DllImport("user32.dll")] static extern int SetWindowLong(IntPtr hwnd, int index, int value);
    public static void MakeOverlay(IntPtr hwnd)
    {
        const int GWL_EXSTYLE = -20, WS_EX_TRANSPARENT = 0x20, WS_EX_TOOLWINDOW = 0x80, WS_EX_LAYERED = 0x80000, WS_EX_NOACTIVATE = 0x08000000;
        SetWindowLong(hwnd, GWL_EXSTYLE, GetWindowLong(hwnd, GWL_EXSTYLE) | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE);
    }

    // Programme de la fenêtre au premier plan (le jeu auquel on joue).
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    public static int GetForegroundPid()
    {
        uint pid;
        GetWindowThreadProcessId(GetForegroundWindow(), out pid);
        return (int)pid;
    }

    // Raccourci clavier global (Ctrl+Maj+F pour le compteur de FPS).
    [DllImport("user32.dll")] static extern bool RegisterHotKey(IntPtr hwnd, int id, uint mods, uint vk);
    [DllImport("user32.dll")] static extern bool UnregisterHotKey(IntPtr hwnd, int id);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr hwnd, int msg, IntPtr wp, IntPtr lp);
    public static bool AddHotKey(IntPtr hwnd, int id, uint mods, uint vk) { return RegisterHotKey(hwnd, id, mods | 0x4000, vk); }
    public static void RemoveHotKey(IntPtr hwnd, int id) { UnregisterHotKey(hwnd, id); }
}

// Mesure des FPS d'un jeu avec PresentMon (outil gratuit d'Intel), lancé en arrière plan.
// PresentMon écrit une ligne CSV par image affichée ; on garde la durée de chaque image.
public static class FrameMon
{
    static System.Diagnostics.Process proc;
    static readonly object sync = new object();
    static readonly List<double> all = new List<double>();
    static readonly Queue<double> last1 = new Queue<double>();
    static readonly Queue<double> last10 = new Queue<double>();
    static double sum1, sum10, sampleSum;
    static int sampleCount;
    static int ftIndex = -1, modeIndex = -1, cpuIndex = -1, gpuIndex = -1;
    // Temps de travail du processeur et de la carte graphique pour chaque image (qui des deux freine ?).
    static double busyFt, cpuBusy, gpuBusy;
    static int busyCount;
    // Mode d'affichage vu par PresentMon (« Hardware: Legacy Flip » = vrai plein écran, rien ne peut s'afficher par dessus).
    public static string LastMode = "";
    public static string LastError = "";
    // Vrai quand le jeu n'est pas au premier plan : ces images ne comptent pas (le jeu tourne au ralenti en fond).
    public static volatile bool Paused;

    public static bool Running { get { var p = proc; return p != null && !p.HasExited; } }
    public static int Frames { get { lock (sync) { return all.Count; } } }

    public static void Reset()
    {
        lock (sync)
        {
            all.Clear(); last1.Clear(); last10.Clear(); sum1 = 0; sum10 = 0; sampleSum = 0; sampleCount = 0;
            ftIndex = -1; modeIndex = -1; cpuIndex = -1; gpuIndex = -1; busyFt = 0; cpuBusy = 0; gpuBusy = 0; busyCount = 0;
        }
        LastError = "";
        LastMode = "";
    }

    static string pmExe = "";

    public static bool Start(string exe, int pid)
    {
        Stop();
        Reset();
        pmExe = exe;
        try
        {
            var psi = new System.Diagnostics.ProcessStartInfo(exe,
                "--process_id " + pid + " --output_stdout --no_console_stats --stop_existing_session --session_name OptiGame --terminate_on_proc_exit");
            psi.UseShellExecute = false;
            psi.CreateNoWindow = true;
            psi.RedirectStandardOutput = true;
            psi.RedirectStandardError = true;
            var p = new System.Diagnostics.Process();
            p.StartInfo = psi;
            p.OutputDataReceived += (s, e) => { if (e.Data != null) Feed(e.Data); };
            p.ErrorDataReceived += (s, e) => { if (!string.IsNullOrEmpty(e.Data)) LastError = e.Data; };
            p.Start();
            p.BeginOutputReadLine();
            p.BeginErrorReadLine();
            proc = p;
            return true;
        }
        catch (Exception ex) { LastError = ex.Message; proc = null; return false; }
    }

    public static void Stop()
    {
        var p = proc;
        proc = null;
        if (p == null) return;
        try { if (!p.HasExited) p.Kill(); } catch { }
        try { p.Dispose(); } catch { }
        // PresentMon arrêté de force laisse sa session de mesure ouverte dans Windows : on la ferme.
        if (!string.IsNullOrEmpty(pmExe))
        {
            try
            {
                var psi = new System.Diagnostics.ProcessStartInfo(pmExe, "--terminate_existing_session --session_name OptiGame");
                psi.UseShellExecute = false;
                psi.CreateNoWindow = true;
                using (var t = System.Diagnostics.Process.Start(psi)) { t.WaitForExit(3000); }
            }
            catch { }
        }
    }

    // Une ligne de PresentMon. La première donne le nom des colonnes.
    public static void Feed(string line)
    {
        var cols = line.Split(',');
        lock (sync)
        {
            if (ftIndex < 0)
            {
                for (int i = 0; i < cols.Length; i++)
                {
                    var c = cols[i].Trim();
                    if (ftIndex < 0 && (c == "FrameTime" || c == "MsBetweenAppStart" || c == "msBetweenPresents" || c == "MsBetweenPresents")) ftIndex = i;
                    if (c == "PresentMode") modeIndex = i;
                    if (c == "CPUBusy" || c == "MsCPUBusy") cpuIndex = i;
                    if (c == "GPUBusy" || c == "MsGPUBusy") gpuIndex = i;
                }
                return;
            }
            if (modeIndex >= 0 && modeIndex < cols.Length) LastMode = cols[modeIndex];
            if (ftIndex >= cols.Length || Paused) return;
            double ft;
            if (!double.TryParse(cols[ftIndex], System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out ft)) return;
            if (ft <= 0 || ft > 5000) return;
            if (all.Count < 3000000) all.Add(ft);
            last1.Enqueue(ft); sum1 += ft;
            while (sum1 > 1000 && last1.Count > 1) sum1 -= last1.Dequeue();
            last10.Enqueue(ft); sum10 += ft;
            while (sum10 > 10000 && last10.Count > 1) sum10 -= last10.Dequeue();
            sampleSum += ft; sampleCount++;
            double cb, gb;
            if (cpuIndex >= 0 && gpuIndex >= 0 && cpuIndex < cols.Length && gpuIndex < cols.Length
                && double.TryParse(cols[cpuIndex], System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out cb)
                && double.TryParse(cols[gpuIndex], System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out gb))
            {
                busyFt += ft; cpuBusy += Math.Min(cb, ft); gpuBusy += Math.Min(gb, ft); busyCount++;
            }
        }
    }

    // Diagnostic de la partie : { part du temps où le processeur travaille, part où la carte graphique travaille
    // (0 à 1, -1 si inconnu), nombre de saccades (images au moins 2,5 fois plus longues que la normale et > 25 ms) }
    public static double[] Busy()
    {
        lock (sync)
        {
            var r = new double[] { -1, -1, 0 };
            if (busyCount > 100 && busyFt > 0) { r[0] = cpuBusy / busyFt; r[1] = gpuBusy / busyFt; }
            if (all.Count > 0)
            {
                var arr = all.ToArray();
                Array.Sort(arr);
                double median = arr[arr.Length / 2];
                double limit = Math.Max(25.0, median * 2.5);
                int n = 0;
                foreach (var f in all) if (f > limit) n++;
                r[2] = n;
            }
            return r;
        }
    }

    // FPS moyen depuis le dernier appel (pour la courbe de la partie). -1 si aucune image entre temps.
    public static double Sample()
    {
        lock (sync)
        {
            double r = sampleSum > 0 ? sampleCount * 1000.0 / sampleSum : -1;
            sampleSum = 0; sampleCount = 0;
            return r;
        }
    }

    // « 1 % bas » : FPS moyen des 1 % d'images les plus lentes (c'est là que se voient les saccades).
    static double Low(double[] frameTimes, double pct)
    {
        if (frameTimes.Length == 0) return 0;
        Array.Sort(frameTimes);
        int count = Math.Max(1, (int)Math.Floor(frameTimes.Length * (1 - pct)));
        double sum = 0;
        for (int i = frameTimes.Length - count; i < frameTimes.Length; i++) sum += frameTimes[i];
        return 1000.0 / (sum / count);
    }

    // Pour l'overlay : { FPS de la dernière seconde, 1 % bas des 10 dernières secondes, FPS moyen de la session }
    public static double[] Live()
    {
        lock (sync)
        {
            var r = new double[3];
            if (last1.Count > 0 && sum1 > 0) r[0] = last1.Count * 1000.0 / sum1;
            r[1] = Low(last10.ToArray(), 0.99);
            double total = 0;
            foreach (var f in all) total += f;
            if (total > 0) r[2] = all.Count * 1000.0 / total;
            return r;
        }
    }

    // Bilan de la session : { FPS moyen, 1 % bas, 0,1 % bas, nombre d'images, secondes mesurées }
    public static double[] Summary()
    {
        lock (sync)
        {
            var r = new double[5];
            int n = all.Count;
            if (n == 0) return r;
            double total = 0;
            foreach (var f in all) total += f;
            var arr = all.ToArray();
            r[0] = n * 1000.0 / total;
            r[1] = Low(arr, 0.99);
            r[2] = Low(arr, 0.999);
            r[3] = n;
            r[4] = total / 1000.0;
            return r;
        }
    }
}

// Découverte avancée des appareils du réseau : chaque appareil est interrogé avec les méthodes
// qu'il utilise lui même pour se présenter (rien n'est modifié sur les appareils).
public static class NetProbe
{
    [DllImport("iphlpapi.dll", ExactSpelling = true)]
    static extern int SendARP(uint destIp, uint srcIp, byte[] macAddr, ref uint macLen);

    static uint ToUint(string ip) { return BitConverter.ToUInt32(System.Net.IPAddress.Parse(ip).GetAddressBytes(), 0); }

    // La même chose en tâche de fond (les adresses vides mettent plusieurs secondes à répondre « personne »).
    public static System.Threading.Tasks.Task<string[]> ArpSweepAsync(string[] ips, int threads)
    {
        return System.Threading.Tasks.Task.Run(() => ArpSweep(ips, threads));
    }
    static string Clean(string s) { return (s ?? "").Replace("|", "/").Replace("\r", " ").Replace("\n", " ").Trim(); }

    // Demande directe de l'adresse physique : un appareil allumé répond toujours, même s'il bloque le ping.
    // Retourne "ip|MAC".
    public static string[] ArpSweep(string[] ips, int threads)
    {
        var results = new List<string>();
        var sync = new object();
        int[] next = new int[] { -1 };
        var ths = new List<System.Threading.Thread>();
        for (int t = 0; t < Math.Min(threads, ips.Length); t++)
        {
            var th = new System.Threading.Thread(() =>
            {
                while (true)
                {
                    int i = System.Threading.Interlocked.Increment(ref next[0]);
                    if (i >= ips.Length || OGNative.Cancel) break;
                    try
                    {
                        byte[] mac = new byte[6];
                        uint len = 6;
                        if (SendARP(ToUint(ips[i]), 0, mac, ref len) == 0 && len == 6)
                        {
                            string m = BitConverter.ToString(mac, 0, 6);
                            if (m != "00-00-00-00-00-00") lock (sync) { results.Add(ips[i] + "|" + m); }
                        }
                    }
                    catch { }
                }
            });
            th.IsBackground = true;
            th.Start();
            ths.Add(th);
        }
        foreach (var th in ths) th.Join();
        return results.ToArray();
    }

    // ---------------------------------------------------------------------
    // mDNS / Bonjour : les appareils annoncent leur nom, leur modèle et leurs services.
    // ---------------------------------------------------------------------
    static readonly string[] MdnsServices = {
        "_services._dns-sd._udp.local", "_googlecast._tcp.local", "_airplay._tcp.local", "_raop._tcp.local",
        "_companion-link._tcp.local", "_ipp._tcp.local", "_ipps._tcp.local", "_printer._tcp.local",
        "_pdl-datastream._tcp.local", "_hap._tcp.local", "_spotify-connect._tcp.local", "_sonos._tcp.local",
        "_amzn-wplay._tcp.local", "_smb._tcp.local", "_workstation._tcp.local", "_device-info._tcp.local",
        "_http._tcp.local", "_matter._tcp.local", "_matterc._udp.local", "_hue._tcp.local",
        "_androidtvremote2._tcp.local", "_nvstream._tcp.local", "_rtsp._tcp.local", "_scanner._tcp.local",
        "_uscan._tcp.local", "_mediaremotetv._tcp.local", "_sleep-proxy._udp.local", "_homekit._tcp.local" };

    static byte[] BuildQuery(IList<string> names)
    {
        var b = new List<byte> { 0, 0, 0, 0, 0, (byte)names.Count, 0, 0, 0, 0, 0, 0 };
        foreach (var n in names)
        {
            foreach (var part in n.Split('.'))
            {
                var bytes = System.Text.Encoding.UTF8.GetBytes(part);
                b.Add((byte)bytes.Length);
                b.AddRange(bytes);
            }
            b.Add(0);
            b.Add(0); b.Add(12);          // PTR
            b.Add(0x80); b.Add(1);        // réponse directe demandée (QU), classe IN
        }
        return b.ToArray();
    }

    static string ReadName(byte[] d, ref int pos)
    {
        var sb = new System.Text.StringBuilder();
        int p = pos;
        bool jumped = false;
        int guard = 0;
        while (p < d.Length && guard++ < 128)
        {
            int len = d[p];
            if (len == 0) { p++; break; }
            if ((len & 0xC0) == 0xC0)
            {
                if (p + 1 >= d.Length) { p = d.Length; break; }
                int ptr = ((len & 0x3F) << 8) | d[p + 1];
                if (!jumped) pos = p + 2;
                jumped = true;
                p = ptr;
                continue;
            }
            p++;
            if (p + len > d.Length) { p = d.Length; break; }
            if (sb.Length > 0) sb.Append('.');
            sb.Append(System.Text.Encoding.UTF8.GetString(d, p, len));
            p += len;
        }
        if (!jumped) pos = p;
        return sb.ToString();
    }

    // Lignes "ip|ptr|nom|cible", "ip|srv|nom|hôte:port", "ip|txt|nom|clé=valeur", "ip|a|nom|adresse".
    public static void ParseMdns(byte[] d, string src, List<string> outp)
    {
        if (d.Length < 12) return;
        int qd = (d[4] << 8) | d[5], an = (d[6] << 8) | d[7], ns = (d[8] << 8) | d[9], ar = (d[10] << 8) | d[11];
        int pos = 12;
        for (int i = 0; i < qd && pos < d.Length; i++) { ReadName(d, ref pos); pos += 4; }
        int total = an + ns + ar;
        for (int i = 0; i < total && pos < d.Length; i++)
        {
            string name = ReadName(d, ref pos);
            if (pos + 10 > d.Length) return;
            int type = (d[pos] << 8) | d[pos + 1];
            int rdlen = (d[pos + 8] << 8) | d[pos + 9];
            pos += 10;
            int start = pos;
            if (start + rdlen > d.Length) return;
            try
            {
                if (type == 12) { int p2 = start; outp.Add(src + "|ptr|" + Clean(name) + "|" + Clean(ReadName(d, ref p2))); }
                else if (type == 33 && rdlen > 6) { int p2 = start + 6; int port = (d[start + 4] << 8) | d[start + 5]; outp.Add(src + "|srv|" + Clean(name) + "|" + Clean(ReadName(d, ref p2)) + ":" + port); }
                else if (type == 16)
                {
                    int p2 = start;
                    while (p2 < start + rdlen)
                    {
                        int l = d[p2]; p2++;
                        if (l > 0 && p2 + l <= start + rdlen) outp.Add(src + "|txt|" + Clean(name) + "|" + Clean(System.Text.Encoding.UTF8.GetString(d, p2, l)));
                        p2 += l;
                    }
                }
                else if (type == 1 && rdlen == 4) outp.Add(src + "|a|" + Clean(name) + "|" + d[start] + "." + d[start + 1] + "." + d[start + 2] + "." + d[start + 3]);
            }
            catch { }
            pos = start + rdlen;
        }
    }

    public static string[] Mdns(string localIp, int timeoutMs)
    {
        var outp = new List<string>();
        var dest = new System.Net.IPEndPoint(System.Net.IPAddress.Parse("224.0.0.251"), 5353);
        using (var u = new System.Net.Sockets.UdpClient(new System.Net.IPEndPoint(System.Net.IPAddress.Parse(localIp), 0)))
        {
            u.Client.ReceiveTimeout = 250;
            var asked = new HashSet<string>(MdnsServices);
            for (int i = 0; i < MdnsServices.Length; i += 6)
            {
                var chunk = new List<string>();
                for (int k = i; k < Math.Min(MdnsServices.Length, i + 6); k++) chunk.Add(MdnsServices[k]);
                var pkt = BuildQuery(chunk);
                u.Send(pkt, pkt.Length, dest);
            }
            var sw = System.Diagnostics.Stopwatch.StartNew();
            var ep = new System.Net.IPEndPoint(System.Net.IPAddress.Any, 0);
            bool second = false;
            while (sw.ElapsedMilliseconds < timeoutMs && !OGNative.Cancel)
            {
                try { var r = u.Receive(ref ep); ParseMdns(r, ep.Address.ToString(), outp); } catch (System.Net.Sockets.SocketException) { }
                // Deuxième vague : les types de services annoncés qu'on n'avait pas demandés.
                if (!second && sw.ElapsedMilliseconds > timeoutMs / 2)
                {
                    second = true;
                    var more = new List<string>();
                    foreach (var l in outp.ToArray())
                    {
                        var x = l.Split('|');
                        if (x.Length > 3 && x[1] == "ptr" && x[2] == "_services._dns-sd._udp.local" && asked.Add(x[3])) more.Add(x[3]);
                    }
                    for (int i = 0; i < more.Count; i += 6)
                    {
                        var pkt = BuildQuery(more.GetRange(i, Math.Min(6, more.Count - i)));
                        try { u.Send(pkt, pkt.Length, dest); } catch { }
                    }
                }
            }
        }
        return outp.ToArray();
    }

    // ---------------------------------------------------------------------
    // NetBIOS : nom et groupe de travail des PC Windows (et de certains NAS). Retourne "ip|nom|groupe|MAC".
    // ---------------------------------------------------------------------
    public static string[] NetBios(string[] ips, int timeoutMs)
    {
        var outp = new List<string>();
        var q = new List<byte> { 0x4F, 0x47, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0x20, (byte)'C', (byte)'K' };
        for (int i = 0; i < 30; i++) q.Add((byte)'A');
        q.AddRange(new byte[] { 0, 0, 0x21, 0, 1 });
        var pkt = q.ToArray();
        using (var u = new System.Net.Sockets.UdpClient(0))
        {
            u.Client.ReceiveTimeout = 250;
            foreach (var ip in ips) { try { u.Send(pkt, pkt.Length, new System.Net.IPEndPoint(System.Net.IPAddress.Parse(ip), 137)); } catch { } }
            var sw = System.Diagnostics.Stopwatch.StartNew();
            var ep = new System.Net.IPEndPoint(System.Net.IPAddress.Any, 0);
            while (sw.ElapsedMilliseconds < timeoutMs)
            {
                byte[] r;
                try { r = u.Receive(ref ep); } catch (System.Net.Sockets.SocketException) { continue; }
                if (r.Length < 57) continue;
                int count = r[56];
                string name = "", group = "";
                int p = 57;
                for (int k = 0; k < count && p + 18 <= r.Length; k++, p += 18)
                {
                    string n = System.Text.Encoding.ASCII.GetString(r, p, 15).Trim();
                    int suffix = r[p + 15];
                    bool isGroup = (r[p + 16] & 0x80) != 0;
                    if (suffix == 0 && !isGroup && name == "") name = n;
                    if (suffix == 0 && isGroup && group == "") group = n;
                }
                string mac = p + 6 <= r.Length ? BitConverter.ToString(r, p, 6) : "";
                if (name != "") outp.Add(ep.Address + "|" + Clean(name) + "|" + Clean(group) + "|" + mac);
            }
        }
        return outp.ToArray();
    }

    // ---------------------------------------------------------------------
    // WS-Discovery : caméras (ONVIF), imprimantes et PC Windows se déclarent. Retourne "ip|types|scopes|adresses".
    // ---------------------------------------------------------------------
    public static string[] WsDiscovery(string localIp, int timeoutMs)
    {
        var outp = new List<string>();
        string probe = "<?xml version=\"1.0\" encoding=\"utf-8\"?><soap:Envelope xmlns:soap=\"http://www.w3.org/2003/05/soap-envelope\" xmlns:wsa=\"http://schemas.xmlsoap.org/ws/2004/08/addressing\" xmlns:wsd=\"http://schemas.xmlsoap.org/ws/2005/04/discovery\"><soap:Header><wsa:To>urn:schemas-xmlsoap-org:ws:2005:04:discovery</wsa:To><wsa:Action>http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</wsa:Action><wsa:MessageID>urn:uuid:" + Guid.NewGuid() + "</wsa:MessageID></soap:Header><soap:Body><wsd:Probe/></soap:Body></soap:Envelope>";
        var pkt = System.Text.Encoding.UTF8.GetBytes(probe);
        var rxTypes = new System.Text.RegularExpressions.Regex(@"<(?:\w+:)?Types[^>]*>([^<]*)<", System.Text.RegularExpressions.RegexOptions.IgnoreCase);
        var rxScopes = new System.Text.RegularExpressions.Regex(@"<(?:\w+:)?Scopes[^>]*>([^<]*)<", System.Text.RegularExpressions.RegexOptions.IgnoreCase);
        var rxAddr = new System.Text.RegularExpressions.Regex(@"<(?:\w+:)?XAddrs[^>]*>([^<]*)<", System.Text.RegularExpressions.RegexOptions.IgnoreCase);
        var seen = new HashSet<string>();
        using (var u = new System.Net.Sockets.UdpClient(new System.Net.IPEndPoint(System.Net.IPAddress.Parse(localIp), 0)))
        {
            u.Client.ReceiveTimeout = 250;
            u.Send(pkt, pkt.Length, new System.Net.IPEndPoint(System.Net.IPAddress.Parse("239.255.255.250"), 3702));
            var sw = System.Diagnostics.Stopwatch.StartNew();
            var ep = new System.Net.IPEndPoint(System.Net.IPAddress.Any, 0);
            while (sw.ElapsedMilliseconds < timeoutMs)
            {
                byte[] r;
                try { r = u.Receive(ref ep); } catch (System.Net.Sockets.SocketException) { continue; }
                string x = System.Text.Encoding.UTF8.GetString(r);
                var mt = rxTypes.Match(x); var ms = rxScopes.Match(x); var ma = rxAddr.Match(x);
                string line = ep.Address + "|" + Clean(mt.Success ? mt.Groups[1].Value : "") + "|" + Clean(ms.Success ? ms.Groups[1].Value : "") + "|" + Clean(ma.Success ? ma.Groups[1].Value : "");
                if (seen.Add(line)) outp.Add(line);
            }
        }
        return outp.ToArray();
    }

    // ---------------------------------------------------------------------
    // Titre des pages web des appareils (« TP-Link Archer AX58 », « Livebox »...). Retourne "url|titre|serveur".
    // Les certificats des appareils ne sont pas vérifiés, seulement pour cette lecture de titre.
    // ---------------------------------------------------------------------
    public static string[] HttpTitles(string[] urls, int timeoutMs)
    {
        var outp = new List<string>();
        var sync = new object();
        var rx = new System.Text.RegularExpressions.Regex(@"<title[^>]*>\s*([^<]{1,120}?)\s*</title>", System.Text.RegularExpressions.RegexOptions.IgnoreCase);
        var tasks = new List<System.Threading.Tasks.Task>();
        foreach (var url in urls)
        {
            string u0 = url;
            tasks.Add(System.Threading.Tasks.Task.Run(() =>
            {
                try
                {
                    var req = (System.Net.HttpWebRequest)System.Net.WebRequest.Create(u0);
                    req.Timeout = timeoutMs;
                    req.ReadWriteTimeout = timeoutMs;
                    req.UserAgent = "Mozilla/5.0 OptiGame";
                    req.AllowAutoRedirect = true;
                    req.ServerCertificateValidationCallback = delegate { return true; };
                    System.Net.HttpWebResponse resp;
                    try { resp = (System.Net.HttpWebResponse)req.GetResponse(); }
                    catch (System.Net.WebException we) { resp = we.Response as System.Net.HttpWebResponse; }
                    if (resp == null) return;
                    using (resp)
                    using (var s = resp.GetResponseStream())
                    {
                        var buf = new byte[65536];
                        int total = 0, n;
                        while (total < buf.Length && (n = s.Read(buf, total, buf.Length - total)) > 0) total += n;
                        var html = System.Text.Encoding.UTF8.GetString(buf, 0, total);
                        var m = rx.Match(html);
                        string title = m.Success ? System.Net.WebUtility.HtmlDecode(m.Groups[1].Value) : "";
                        string server = resp.Headers["Server"] ?? "";
                        if (title != "" || server != "") lock (sync) { outp.Add(u0 + "|" + Clean(title) + "|" + Clean(server)); }
                    }
                }
                catch { }
            }));
        }
        try { System.Threading.Tasks.Task.WaitAll(tasks.ToArray(), timeoutMs + 2000); } catch { }
        lock (sync) { return outp.ToArray(); }
    }
}
