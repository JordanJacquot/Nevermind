# Lance un script d'essai sur un BUREAU WINDOWS À PART (CreateDesktop), jamais affiché : ses fenêtres (Tauri,
# WebView2) ne peuvent ni apparaître à l'écran, ni prendre le premier plan ou le clavier (chaque bureau a le sien).
# Repris de l'essai de l'étape 0 du dépôt Mindset (rust\essais\sur-bureau.ps1), avec en plus : PowerShell lancé sans
# console (CREATE_NO_WINDOW), et tout ce qu'il a lancé arrêté à la fin par numéro de processus (descendants compris),
# rien d'autre.
#
#   & '.\outils\sur-bureau.ps1' -Script <essai.ps1> [-Arguments "..."] [-Secondes 240] [-Sortie <fichier>]
#
# À lancer depuis une console PowerShell déjà ouverte (pas depuis un autre programme qui en ouvrirait une). La sortie
# du script d'essai est écrite dans -Sortie (par défaut <essai>.sortie.txt à côté de lui), puis affichée. Le code de
# sortie est celui du script d'essai (1 s'il a dépassé le délai).
param(
    [Parameter(Mandatory = $true)][string]$Script,
    [string]$Arguments = '',
    [int]$Secondes = 240,
    [string]$Sortie
)
$ErrorActionPreference = 'Stop'

if (-not ('MindcareBureau' -as [type])) {
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class MindcareBureau
{
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr CreateDesktopW(string nom, IntPtr dev, IntPtr mode, uint flags, uint acces, IntPtr sa);
    [DllImport("user32.dll")] public static extern bool CloseDesktop(IntPtr h);
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct STARTUPINFO { public int cb; public string reserved, desktop, title; public int x, y, w, h, xc, yc, attr, flags; public short show, r2; public IntPtr r3, stdin, stdout, stderr; }
    [StructLayout(LayoutKind.Sequential)] struct PROCESS_INFORMATION { public IntPtr process, thread; public int pid, tid; }
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CreateProcessW(string appli, System.Text.StringBuilder ligne, IntPtr pa, IntPtr ta, bool herite, uint flags, IntPtr env, string dossier, ref STARTUPINFO si, out PROCESS_INFORMATION pi);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    // CREATE_NO_WINDOW : PowerShell sans console, même sur le bureau à part
    const uint CREATE_NO_WINDOW = 0x08000000;
    public static int Lancer(string exe, string ligne, string bureau)
    {
        var si = new STARTUPINFO(); si.cb = Marshal.SizeOf(si); si.desktop = bureau;
        si.flags = 1; si.show = 0;   // STARTF_USESHOWWINDOW, SW_HIDE
        PROCESS_INFORMATION pi;
        if (!CreateProcessW(exe, new System.Text.StringBuilder(ligne), IntPtr.Zero, IntPtr.Zero, false, CREATE_NO_WINDOW, IntPtr.Zero, null, ref si, out pi))
            throw new System.ComponentModel.Win32Exception();
        CloseHandle(pi.thread); CloseHandle(pi.process);
        return pi.pid;
    }
}
'@
}

# Les descendants d'un processus (par ParentProcessId), lui compris
function Get-Descendants([int]$Racine, $Tous) {
    $l = @($Tous | Where-Object { $_.ProcessId -eq $Racine })
    $vus = @{ $Racine = $true }
    $file = New-Object System.Collections.Queue
    $file.Enqueue($Racine)
    while ($file.Count) {
        $p = $file.Dequeue()
        foreach ($x in @($Tous | Where-Object { $_.ParentProcessId -eq $p })) {
            if ($vus.ContainsKey([int]$x.ProcessId)) { continue }
            $vus[[int]$x.ProcessId] = $true
            $l += $x
            $file.Enqueue([int]$x.ProcessId)
        }
    }
    , $l
}

$chemin = (Resolve-Path -LiteralPath $Script).Path
if (-not $Sortie) { $Sortie = [IO.Path]::ChangeExtension($chemin, '.sortie.txt') }
$Sortie = [IO.Path]::GetFullPath($Sortie)
if (Test-Path -LiteralPath $Sortie) { Remove-Item -LiteralPath $Sortie -Force }

$nom = "MindcareEssai-$PID"
$hd = [MindcareBureau]::CreateDesktopW($nom, [IntPtr]::Zero, [IntPtr]::Zero, 0, 0x10000000, [IntPtr]::Zero)   # GENERIC_ALL
if ($hd -eq [IntPtr]::Zero) { throw "Bureau à part impossible ($([Runtime.InteropServices.Marshal]::GetLastWin32Error()))." }
$ps = "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe"
# (le code de sortie du script d'essai est gardé : « exit $LASTEXITCODE » après l'appel)
$commande = "& '$($chemin.Replace("'", "''"))' $Arguments *> '$($Sortie.Replace("'", "''"))'; exit `$LASTEXITCODE"
$ligne = "`"$ps`" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command `"$commande`""
$code = 1
$id = [MindcareBureau]::Lancer($ps, $ligne, "winsta0\$nom")
try {
    $p = Get-Process -Id $id
    $null = $p.Handle   # (gardé tout de suite : sans lui, le code de sortie ne se lit plus une fois le processus fini)
    if ($p.WaitForExit($Secondes * 1000)) { $code = $p.ExitCode } else { Write-Host "Délai dépassé ($Secondes s) : tout est arrêté." }
} finally {
    # Tout ce qui reste de l'essai, par numéro de processus (descendants du PowerShell lancé), et seulement cela
    $tous = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, CommandLine)
    foreach ($x in @(Get-Descendants $id $tous)) {
        try { Stop-Process -Id $x.ProcessId -Force -ErrorAction Stop; Write-Host "Arrêté : $($x.Name) ($($x.ProcessId))" } catch {}
    }
    [void][MindcareBureau]::CloseDesktop($hd)
}
if (Test-Path -LiteralPath $Sortie) { Get-Content -LiteralPath $Sortie }
exit $code
