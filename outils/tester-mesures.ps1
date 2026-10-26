# Rejeu des enregistrements des mesures continues de l'Aide (cas\mesures\*.json, plan Rust étape 1b) dans la version
# PowerShell d'aujourd'hui : chaque demande passe par Invoke-AideAction (modules\aide-actions.ps1), avec le vrai code
# des mesures (classe AideFps de NevermindAide.ps1, classes TrafficMon et NetFlow de modules\natif.cs). Seuls les appels
# à Windows sont remplacés par l'enregistrement :
#   - PresentMon : un faux PresentMon.exe, compilé dans un dossier d'essai, écrit sur ses sorties les octets enregistrés ;
#   - trace réseau : StartTrace, EnableTraceEx2, OpenTrace, ProcessTrace, ControlTrace et CloseTrace répondent ce que
#     dit l'enregistrement, les événements sont donnés un par un à NetFlow.OnRecord, l'heure est celle de l'enregistrement ;
#   - compteurs TCP : GetExtendedTcpTable et Get/SetPerTcp(6)ConnectionEStats répondent les tables enregistrées.
# La version Rust rejoue les mêmes fichiers (cargo test, essais_mesures.rs) : les deux doivent donner les mêmes réponses.
# Sans droits administrateur, sans vraie mesure : rien n'est lancé sur le PC à part le faux PresentMon.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-mesures.ps1
#   ... -Ecrire <fichier.json>   écrit aussi les réponses obtenues (pour remplir un nouveau cas, à relire avant de l'ajouter)
#
# Code de sortie : 0 si tout est pareil, 1 sinon.
param([string]$Ecrire)
$ErrorActionPreference = 'Stop'
$racine = Split-Path $PSScriptRoot -Parent
$fichiers = Join-Path $racine 'Nevermind\fichiers'
. (Join-Path $PSScriptRoot 'lire-cas.ps1')
$echecs = New-Object Collections.Generic.List[string]
$nbCas = 0; $nbReponses = 0
$obtenus = [ordered]@{}

# ---------------------------------------------------------------------------
# 1. Le code de l'Aide, avec Windows remplacé par l'enregistrement
# ---------------------------------------------------------------------------
. (Join-Path $fichiers 'modules\aide-actions.ps1')

# Une classe C# entière, prise dans un fichier (de « public static class Nom » à son accolade fermante)
function Get-Classe([string]$Texte, [string]$Nom) {
    $i = $Texte.IndexOf("public static class $Nom")
    if ($i -lt 0) { throw "classe $Nom introuvable" }
    $n = 0; $j = $Texte.IndexOf('{', $i)
    for ($k = $j; $k -lt $Texte.Length; $k++) {
        if ($Texte[$k] -eq '{') { $n++ } elseif ($Texte[$k] -eq '}') { $n--; if ($n -eq 0) { return $Texte.Substring($i, $k - $i + 1) } }
    }
    throw "classe $Nom coupée"
}

# Remplace les appels à une DLL de Windows ([DllImport] static extern) par un corps donné ici, nom par nom. Chaque nom
# doit être trouvé une fois : si le code de l'Aide change de forme, le test s'arrête au lieu de passer à côté.
function Set-Remplacements([string]$Texte, [string]$Dll, [hashtable]$Corps) {
    $vus = @{}
    $motif = '\[DllImport\("' + [regex]::Escape($Dll) + '"[^\]]*\)\]\s*static extern (\w+) (\w+)\(([^)]*)\);'
    $r = [regex]::Replace($Texte, $motif, {
        param($m)
        $nom = $m.Groups[2].Value
        if (-not $Corps.ContainsKey($nom)) { return 'static ' + $m.Groups[1].Value + ' ' + $nom + '(' + $m.Groups[3].Value + ') { throw new InvalidOperationException("appel de Windows non prévu par le rejeu"); }' }
        $vus[$nom] = 1 + [int]$vus[$nom]
        'static ' + $m.Groups[1].Value + ' ' + $nom + '(' + $m.Groups[3].Value + ') { ' + $Corps[$nom] + ' }'
    })
    foreach ($n in $Corps.Keys) { if ($vus[$n] -ne 1) { throw "$Dll : $n trouvé $([int]$vus[$n]) fois dans le code de l'Aide (1 attendu)" } }
    $r
}
function Set-Texte([string]$Texte, [string]$Avant, [string]$Apres, [int]$Fois) {
    $n = ([regex]::Matches($Texte, [regex]::Escape($Avant))).Count
    if ($n -ne $Fois) { throw "« $Avant » trouvé $n fois dans le code de l'Aide ($Fois attendu)" }
    $Texte.Replace($Avant, $Apres)
}

$natif = [IO.File]::ReadAllText((Join-Path $fichiers 'modules\natif.cs'), [Text.Encoding]::UTF8)
$aide = [IO.File]::ReadAllText((Join-Path $fichiers 'NevermindAide.ps1'), [Text.Encoding]::UTF8)

$tcp = Set-Remplacements (Get-Classe $natif 'TrafficMon') 'iphlpapi.dll' @{
    GetExtendedTcpTable = 'return RejeuTcp.Table(table, ref size, af);'
    SetPerTcpConnectionEStats = 'return RejeuTcp.Compter(RejeuTcp.Trouver4(row.LocalAddr, row.LocalPort, row.RemoteAddr, row.RemotePort));'
    GetPerTcpConnectionEStats = 'return RejeuTcp.Octets(RejeuTcp.Trouver4(row.LocalAddr, row.LocalPort, row.RemoteAddr, row.RemotePort), rod);'
    SetPerTcp6ConnectionEStats = 'return RejeuTcp.Compter(RejeuTcp.Trouver6(row.LocalAddr, row.LocalPort, row.RemoteAddr, row.RemotePort));'
    GetPerTcp6ConnectionEStats = 'return RejeuTcp.Octets(RejeuTcp.Trouver6(row.LocalAddr, row.LocalPort, row.RemoteAddr, row.RemotePort), rod);'
}
$flux = Set-Remplacements (Get-Classe $natif 'NetFlow') 'advapi32.dll' @{
    StartTraceW = 'handle = 1; return RejeuTrace.Demarrage();'
    ControlTraceW = 'return 0;'
    EnableTraceEx2 = 'return RejeuTrace.Activation;'
    OpenTraceW = 'RejeuTrace.Fin.Reset(); return 5;'
    ProcessTrace = 'RejeuTrace.Fin.WaitOne(); return 0;'
    CloseTrace = 'RejeuTrace.Fin.Set(); return 0;'
}
$flux = Set-Texte $flux 'DateTime.UtcNow.ToFileTimeUtc()' 'RejeuTrace.Maintenant' 2
$flux = [regex]::Replace($flux, 'foreach \(var ni in System\.Net\.NetworkInformation\.NetworkInterface\.GetAllNetworkInterfaces\(\)\)\s*foreach \(var ua in ni\.GetIPProperties\(\)\.UnicastAddresses\) local\.Add\(ua\.Address\.ToString\(\)\.Split\(''%''\)\[0\]\);',
    'foreach (var t in RejeuTrace.Locales) local.Add(System.Net.IPAddress.Parse(t).ToString().Split(''%'')[0]);')
if (-not $flux.Contains('RejeuTrace.Locales')) { throw 'NetFlow.Start : lecture des adresses du PC introuvable' }
$i = $aide.IndexOf("Add-Type -TypeDefinition @'")
$fps = Get-Classe $aide.Substring($i) 'AideFps'

Add-Type -TypeDefinition (@'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

// Les tables TCP enregistrées, données à TrafficMon à la place de celles de Windows
public static class RejeuTcp
{
    public class Ligne { public uint Etat, LAdr4, LPort, RAdr4, RPort, Pid, LZone, RZone; public byte[] LAdr6, RAdr6; public bool Compter, Lisible; public ulong Out, In; }
    public static List<Ligne> V4, V6;
    public static uint Table(IntPtr t, ref int size, int af)
    {
        var l = af == 2 ? V4 : V6;
        if (l == null) return 87;
        int taille = af == 2 ? 24 : 56, besoin = 4 + l.Count * taille;
        if (t == IntPtr.Zero || size < besoin) { size = besoin; return 122; }
        Marshal.WriteInt32(t, 0, l.Count);
        for (int i = 0; i < l.Count; i++)
        {
            var x = l[i]; long o = t.ToInt64() + 4 + i * taille;
            if (af == 2)
            {
                uint[] v = { x.Etat, x.LAdr4, x.LPort, x.RAdr4, x.RPort, x.Pid };
                for (int k = 0; k < 6; k++) Marshal.WriteInt32(new IntPtr(o + 4 * k), (int)v[k]);
            }
            else
            {
                Marshal.Copy(x.LAdr6, 0, new IntPtr(o), 16);
                Marshal.WriteInt32(new IntPtr(o + 16), (int)x.LZone); Marshal.WriteInt32(new IntPtr(o + 20), (int)x.LPort);
                Marshal.Copy(x.RAdr6, 0, new IntPtr(o + 24), 16);
                Marshal.WriteInt32(new IntPtr(o + 40), (int)x.RZone); Marshal.WriteInt32(new IntPtr(o + 44), (int)x.RPort);
                Marshal.WriteInt32(new IntPtr(o + 48), (int)x.Etat); Marshal.WriteInt32(new IntPtr(o + 52), (int)x.Pid);
            }
        }
        return 0;
    }
    public static Ligne Trouver4(uint la, uint lp, uint ra, uint rp)
    {
        if (V4 != null) foreach (var x in V4) if (x.LAdr4 == la && x.LPort == lp && x.RAdr4 == ra && x.RPort == rp) return x;
        return null;
    }
    static bool Pareil(byte[] a, byte[] b) { for (int i = 0; i < 16; i++) if (a[i] != b[i]) return false; return true; }
    public static Ligne Trouver6(byte[] la, uint lp, byte[] ra, uint rp)
    {
        if (V6 != null) foreach (var x in V6) if (Pareil(x.LAdr6, la) && x.LPort == lp && Pareil(x.RAdr6, ra) && x.RPort == rp) return x;
        return null;
    }
    public static uint Compter(Ligne x) { return x != null && x.Compter ? 0u : 5u; }
    public static uint Octets(Ligne x, IntPtr rod)
    {
        if (x == null || !x.Lisible) return 5;
        Marshal.WriteInt64(rod, 0, (long)x.Out); Marshal.WriteInt64(rod, 16, (long)x.In);
        return 0;
    }
}

// Ce que Windows répond à la trace réseau, et l'heure de l'enregistrement
public static class RejeuTrace
{
    public static Queue<uint> Demarrages = new Queue<uint>();
    public static uint Activation;
    public static long Maintenant;
    public static string[] Locales = new string[0];
    public static readonly System.Threading.ManualResetEvent Fin = new System.Threading.ManualResetEvent(false);
    public static uint Demarrage() { return Demarrages.Count > 0 ? Demarrages.Dequeue() : 0u; }
}

'@ + $tcp + "`n" + $flux + "`n" + $fps)

# Le faux PresentMon : note ses arguments, puis écrit sur ses sorties ce que le test lui donne, étape par étape
# (fichiers <pid>-<n>.txt de son dossier : « O » sortie, « E » sortie d'erreur, « F » fin ; puis <pid>-<n>.fait)
$essai = Join-Path ([IO.Path]::GetTempPath()) "nevermind-mesures-$PID"
[void][IO.Directory]::CreateDirectory($essai)
$fauxPm = Join-Path $essai 'PresentMon.exe'
Add-Type -OutputAssembly $fauxPm -OutputType ConsoleApplication -TypeDefinition @'
using System;
using System.IO;
using System.Threading;
public static class FauxPresentMon
{
    public static int Main()
    {
        string dir = AppDomain.CurrentDomain.BaseDirectory;
        string cl = Environment.CommandLine;
        int i = cl.StartsWith("\"") ? cl.IndexOf('"', 1) + 1 : cl.IndexOf(' ');
        string args = i <= 0 || i >= cl.Length ? "" : cl.Substring(i + 1);
        File.AppendAllText(Path.Combine(dir, "lancements.txt"), args + "\n");
        if (args.Contains("--terminate_existing_session")) return 0;
        int pid = System.Diagnostics.Process.GetCurrentProcess().Id;
        File.WriteAllText(Path.Combine(dir, pid + ".pret"), "");
        var sortie = Console.OpenStandardOutput();
        var erreurs = Console.OpenStandardError();
        for (int n = 1; ; n++)
        {
            string f = Path.Combine(dir, pid + "-" + n + ".txt");
            while (!File.Exists(f)) Thread.Sleep(10);
            byte[] b = File.ReadAllBytes(f);
            if (b[0] == (byte)'F') { File.WriteAllText(Path.Combine(dir, pid + "-" + n + ".fait"), ""); return 0; }
            var s = b[0] == (byte)'O' ? sortie : erreurs;
            s.Write(b, 1, b.Length - 1); s.Flush();
            File.WriteAllText(Path.Combine(dir, pid + "-" + n + ".fait"), "");
        }
    }
}
'@

# Les commandes que les demandes appellent, remplacées (une fonction passe avant une commande du même nom)
$script:JeuxRejeu = @{}
$script:PmRefus = $null
function Get-Process {
    param([int]$Id, $ErrorAction)
    if ($script:JeuxRejeu.ContainsKey($Id)) {
        $n = $script:JeuxRejeu[$Id]
        # (ProcessName de .NET : sans « .exe »)
        return [pscustomobject]@{ Id = $Id; ProcessName = ($n -replace '\.exe$', '') }
    }
    if ($ErrorAction -eq 'Stop') { throw "Aucun processus $Id" }
}
function Get-AidePresentMon { if ($script:PmRefus) { throw $script:PmRefus }; $fauxPm }

# ---------------------------------------------------------------------------
# 2. Comparaison des réponses
# ---------------------------------------------------------------------------
# Une réponse sous une forme comparable : champs triés, nombres écrits pareil (comme tester-aide-rust.ps1)
function ConvertTo-Canonique($V) {
    if ($null -eq $V) { return 'null' }
    if ($V -is [string]) { return (ConvertTo-Json -InputObject $V -Compress) }
    if ($V -is [bool]) { return "$V".ToLowerInvariant() }
    if ($V -is [decimal] -or $V -is [double] -or $V -is [single]) {
        if ([math]::Truncate([decimal]$V) -eq [decimal]$V) { return ([decimal]$V).ToString('0', [Globalization.CultureInfo]::InvariantCulture) }
        return ([decimal]$V).ToString([Globalization.CultureInfo]::InvariantCulture)
    }
    if ($V -is [ValueType]) { return ([string]$V) }
    if ($V -is [array]) { return '[' + (@(foreach ($x in $V) { ConvertTo-Canonique $x }) -join ',') + ']' }
    $p = @($V.PSObject.Properties | Sort-Object Name)
    '{' + (@(foreach ($x in $p) { (ConvertTo-Json -InputObject $x.Name -Compress) + ':' + (ConvertTo-Canonique $x.Value) }) -join ',') + '}'
}

# Une demande, comme dans l'Aide (Traitement, NevermindAide.ps1) : la réponse telle qu'elle part sur le canal, relue
function Invoke-Demande([string]$Action, $P) {
    try { $r = Invoke-AideAction $Action $P -Client 1; $rep = @{ ok = $true; r = $r } }
    catch { $rep = @{ ok = $false; e = $_.Exception.Message } }
    (ConvertTo-Json -InputObject $rep -Depth 8 -Compress) | ConvertFrom-Json
}

function Compare-Cas([string]$Fichier, $Cas, $Reponses, $Autres) {
    $script:nbCas++
    $attendues = @($Cas.attendu.reponses)
    if ($Reponses.Count -ne $attendues.Count) { $echecs.Add("$Fichier « $($Cas.nom) » : $($Reponses.Count) réponses, $($attendues.Count) attendues") }
    for ($k = 0; $k -lt [math]::Min($Reponses.Count, $attendues.Count); $k++) {
        $script:nbReponses++
        $a = ConvertTo-Canonique $attendues[$k]; $o = ConvertTo-Canonique $Reponses[$k]
        if ($a -cne $o) { $echecs.Add("$Fichier « $($Cas.nom) », réponse $($k + 1) :`n      attendu : $a`n      obtenu  : $o") }
    }
    foreach ($n in @($Autres.Keys)) {
        $a = ConvertTo-Canonique @($Cas.attendu.$n); $o = ConvertTo-Canonique @($Autres[$n])
        if ($a -cne $o) { $echecs.Add("$Fichier « $($Cas.nom) », $n :`n      attendu : $a`n      obtenu  : $o") }
    }
    $e = [ordered]@{}; $e.reponses = $Reponses.ToArray()
    foreach ($n in @($Autres.Keys)) { $e[$n] = @($Autres[$n]) }
    if (-not $obtenus.Contains($Fichier)) { $obtenus[$Fichier] = New-Object Collections.Generic.List[object] }
    $obtenus[$Fichier].Add($e)
}

function Get-Statique([type]$T, [string]$Nom) { , $T.GetField($Nom, [Reflection.BindingFlags]'Static, NonPublic, Public').GetValue($null) }
function Set-Statique([type]$T, [string]$Nom, $Valeur) { $T.GetField($Nom, [Reflection.BindingFlags]'Static, NonPublic, Public').SetValue($null, $Valeur) }
function Wait-Condition([scriptblock]$Condition, [int]$Ms = 5000) {
    $fin = (Get-Date).AddMilliseconds($Ms)
    while ((Get-Date) -lt $fin) { if (& $Condition) { return $true }; Start-Sleep -Milliseconds 10 }
    $false
}

try {
    # -----------------------------------------------------------------------
    # 3. PresentMon
    # -----------------------------------------------------------------------
    $f = Read-Cas 'mesures\presentmon.json' -Minimum 9
    $fpsType = [AideFps]
    foreach ($c in @($f.cas)) {
        # (une Aide neuve : tout ce qu'AideFps garde est remis à zéro)
        [AideFps]::Stop()
        Set-Statique $fpsType 'proc' $null; Set-Statique $fpsType 'exePm' ''
        $q = Get-Statique $fpsType 'lignes'; $x = $null; while ($q.TryDequeue([ref]$x)) {}
        foreach ($n in 'fin', 'rattachee', 'donnerEntete') { Set-Statique $fpsType $n $false }
        Set-Statique $fpsType 'entete' $null; Set-Statique $fpsType 'veille' ([string[]]@())
        Get-ChildItem -LiteralPath $essai | Where-Object { $_.Name -ne 'PresentMon.exe' } | Remove-Item -Force
        $script:JeuxRejeu = @{}; foreach ($p in $c.entree.jeux.PSObject.Properties) { $script:JeuxRejeu[[int]$p.Name] = [string]$p.Value }
        $script:PmRefus = $null
        $reponses = New-Object Collections.Generic.List[object]
        $dernier = 0; $etapesPm = @{}
        foreach ($e in @($c.entree.etapes)) {
            if ($e.PSObject.Properties['presentmon_refus']) { $script:PmRefus = $e.presentmon_refus; continue }
            if ($e.demande) {
                $reponses.Add((Invoke-Demande $e.demande $e.p))
                # Un nouveau PresentMon : attendu prêt (ses arguments notés) avant la suite
                $p = Get-Statique $fpsType 'proc'
                if ($p -and $p.Id -ne $dernier) {
                    $dernier = $p.Id; $etapesPm[$dernier] = 0
                    if (-not (Wait-Condition { Test-Path -LiteralPath (Join-Path $essai "$dernier.pret") })) { throw "le faux PresentMon $dernier ne démarre pas" }
                }
                continue
            }
            if (-not $dernier) { throw "« $($c.nom) » : une sortie de PresentMon sans PresentMon lancé" }
            $etapesPm[$dernier]++
            $base = Join-Path $essai "$dernier-$($etapesPm[$dernier])"
            $sorte = if ($e.PSObject.Properties['sortie']) { 'O' } elseif ($e.PSObject.Properties['erreurs']) { 'E' } else { 'F' }
            $texte = if ($sorte -eq 'O') { [string]$e.sortie } elseif ($sorte -eq 'E') { [string]$e.erreurs } else { '' }
            $octets = [byte[]](@([byte][char]$sorte) + [Text.Encoding]::UTF8.GetBytes($texte))
            [IO.File]::WriteAllBytes("$base.tmp", $octets); [IO.File]::Move("$base.tmp", "$base.txt")
            if (-not (Wait-Condition { Test-Path -LiteralPath "$base.fait" })) { throw "le faux PresentMon $dernier n'a pas écrit l'étape $base" }
            if ($sorte -eq 'F') {
                try { [void][Diagnostics.Process]::GetProcessById($dernier).WaitForExit(5000) } catch {}
                [void](Wait-Condition { Get-Statique $fpsType 'fin' } -Ms 2000)
            }
            # (ses lignes arrivées : plus rien ne bouge pendant 300 ms)
            $avant = -1; $stable = Get-Date
            while (((Get-Date) - $stable).TotalMilliseconds -lt 300) {
                $n = (Get-Statique $fpsType 'lignes').Count
                if ($n -ne $avant) { $avant = $n; $stable = Get-Date }
                Start-Sleep -Milliseconds 20
            }
        }
        $lancements = @()
        $l = Join-Path $essai 'lancements.txt'
        if (Test-Path -LiteralPath $l) { $lancements = @([IO.File]::ReadAllLines($l) | Where-Object { $_ -ne '' }) }
        Compare-Cas 'presentmon.json' $c $reponses @{ lancements = $lancements }
    }

    # -----------------------------------------------------------------------
    # 4. Trace réseau
    # -----------------------------------------------------------------------
    $f = Read-Cas 'mesures\trace-reseau.json' -Minimum 4
    $surEvenement = [NetFlow].GetMethod('OnRecord', [Reflection.BindingFlags]'Static, NonPublic')
    foreach ($c in @($f.cas)) {
        if ([NetFlow]::Running) { [NetFlow]::Stop() }
        $base = [long]$c.entree.base
        [RejeuTrace]::Locales = [string[]]@($c.entree.locales)
        $reponses = New-Object Collections.Generic.List[object]
        foreach ($e in @($c.entree.etapes)) {
            if ($e.demande) {
                [RejeuTrace]::Maintenant = $base + 10 * [long]$e.t
                [RejeuTrace]::Demarrages.Clear(); [RejeuTrace]::Activation = 0
                if ($e.windows) {
                    foreach ($x in @($e.windows.demarrage)) { [RejeuTrace]::Demarrages.Enqueue([uint32]$x) }
                    if ($null -ne $e.windows.activation) { [RejeuTrace]::Activation = [uint32]$e.windows.activation }
                }
                $reponses.Add((Invoke-Demande $e.demande $e.p))
            } elseif ($e.fin) {
                [void][RejeuTrace]::Fin.Set()
                if (-not (Wait-Condition { -not [NetFlow]::Running })) { throw 'la trace ne s''arrête pas' }
            } else {
                foreach ($v in @($e.evenements)) {
                    # EVENT_RECORD : date (16), numéro (40), taille des données (86), adresse des données (96)
                    $donnees = if ($v.vide) { $null } elseif ($v.octets) {
                        [byte[]]@(for ($k = 0; $k -lt $v.octets.Length; $k += 2) { [Convert]::ToByte($v.octets.Substring($k, 2), 16) })
                    } else {
                        $v6 = @(26, 27, 58, 59) -contains [int]$v.id
                        $b = New-Object Collections.Generic.List[byte]
                        $b.AddRange([BitConverter]::GetBytes([int]$v.pid)); $b.AddRange([BitConverter]::GetBytes([int]$v.taille))
                        foreach ($a in $v.dst, $v.src) {
                            $ip = [Net.IPAddress]::Parse($a)
                            $o = if ($v6 -and $ip.AddressFamily -eq 'InterNetwork') { $ip.MapToIPv6().GetAddressBytes() } else { $ip.GetAddressBytes() }
                            $b.AddRange([byte[]]$o)
                        }
                        foreach ($pt in $v.dport, $v.sport) { $b.Add([byte](([int]$pt -shr 8) -band 255)); $b.Add([byte]([int]$pt -band 255)) }
                        $b.AddRange([byte[]]::new(8))
                        $b.ToArray()
                    }
                    $rec = [Runtime.InteropServices.Marshal]::AllocHGlobal(112)
                    $ptr = [IntPtr]::Zero
                    try {
                        for ($k = 0; $k -lt 112; $k++) { [Runtime.InteropServices.Marshal]::WriteByte($rec, $k, 0) }
                        [Runtime.InteropServices.Marshal]::WriteInt64($rec, 16, $base + 10 * [long]$v.t)
                        [Runtime.InteropServices.Marshal]::WriteInt16($rec, 40, [int16][int]$v.id)
                        if ($donnees) {
                            $ptr = [Runtime.InteropServices.Marshal]::AllocHGlobal($donnees.Length)
                            [Runtime.InteropServices.Marshal]::Copy($donnees, 0, $ptr, $donnees.Length)
                            [Runtime.InteropServices.Marshal]::WriteInt16($rec, 86, [int16]$donnees.Length)
                            [Runtime.InteropServices.Marshal]::WriteIntPtr($rec, 96, $ptr)
                        }
                        [void]$surEvenement.Invoke($null, @([object]$rec))
                    } finally {
                        [Runtime.InteropServices.Marshal]::FreeHGlobal($rec)
                        if ($ptr -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::FreeHGlobal($ptr) }
                    }
                }
            }
        }
        if ([NetFlow]::Running) { [NetFlow]::Stop() }
        Compare-Cas 'trace-reseau.json' $c $reponses @{}
    }

    # -----------------------------------------------------------------------
    # 5. Compteurs TCP
    # -----------------------------------------------------------------------
    $f = Read-Cas 'mesures\tcp.json' -Minimum 3
    # Un port tel que Windows le range (octets du réseau dans un entier)
    function ConvertTo-PortBrut([int]$P) { [uint32]((($P -band 255) -shl 8) -bor (($P -shr 8) -band 255)) }
    function ConvertTo-Lignes($Table, [bool]$V6) {
        if ($null -eq $Table) { return $null }
        $l = New-Object 'Collections.Generic.List[RejeuTcp+Ligne]'
        foreach ($x in @($Table)) {
            $r = New-Object 'RejeuTcp+Ligne'
            $r.Etat = [uint32]$x.etat; $r.Pid = [uint32]$x.pid
            $r.LPort = ConvertTo-PortBrut $x.port_local; $r.RPort = ConvertTo-PortBrut $x.port_distant
            if ($V6) {
                $r.LAdr6 = [Net.IPAddress]::Parse($x.local).GetAddressBytes(); $r.RAdr6 = [Net.IPAddress]::Parse($x.distante).GetAddressBytes()
                $r.LZone = [uint32]$x.zone_locale; $r.RZone = [uint32]$x.zone_distante
            } else {
                $r.LAdr4 = [BitConverter]::ToUInt32([Net.IPAddress]::Parse($x.local).GetAddressBytes(), 0)
                $r.RAdr4 = [BitConverter]::ToUInt32([Net.IPAddress]::Parse($x.distante).GetAddressBytes(), 0)
            }
            $r.Compter = [bool]$x.compter
            if ($null -ne $x.octets) { $r.Lisible = $true; $r.Out = [uint64]$x.octets[0]; $r.In = [uint64]$x.octets[1] }
            $l.Add($r)
        }
        , $l
    }
    foreach ($c in @($f.cas)) {
        # (une Aide neuve : rien de compté)
        [TrafficMon]::CountersOk = $false
        (Get-Statique ([TrafficMon]) 'enabled').Clear()
        $reponses = New-Object Collections.Generic.List[object]
        foreach ($p in @($c.entree.passages)) {
            [RejeuTcp]::V4 = ConvertTo-Lignes $p.v4 $false
            [RejeuTcp]::V6 = ConvertTo-Lignes $p.v6 $true
            $reponses.Add((Invoke-Demande 'trafic.tcp' $null))
        }
        Compare-Cas 'tcp.json' $c $reponses @{}
    }
} catch {
    $echecs.Add("Rejeu interrompu : $($_.Exception.Message) ($($_.InvocationInfo.ScriptLineNumber))")
} finally {
    try { [AideFps]::Stop() } catch {}
    Start-Sleep -Milliseconds 200
    Remove-Item -LiteralPath $essai -Recurse -Force -ErrorAction SilentlyContinue
}

if ($Ecrire) { [IO.File]::WriteAllText($Ecrire, (ConvertTo-Json -InputObject $obtenus -Depth 12), (New-Object Text.UTF8Encoding($false))) }
Write-Host "$nbCas cas rejoués, $nbReponses réponses comparées (version PowerShell)."
if ($echecs.Count) {
    Write-Host "$($echecs.Count) différence(s) :" -ForegroundColor Red
    foreach ($e in $echecs) { Write-Host "  - $e" -ForegroundColor Red }
    exit 1
}
Write-Host 'Les enregistrements donnent les réponses attendues.' -ForegroundColor Green
exit 0
