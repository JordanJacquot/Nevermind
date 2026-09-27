#Requires -Version 5.1
<#
    OptiGame 1.0.2
    Analyse et optimisation gaming pour Windows 10 et 11.

    Chaque réglage modifié est sauvegardé dans %LOCALAPPDATA%\OptiGame\sauvegarde.json
    et peut être annulé depuis l'onglet Sauvegarde.

    OptiGame.ps1 -Uninstall  remet les réglages comme avant et supprime l'application.
#>
param([switch]$Uninstall)

$AppVersion = '1.0.2'
$UpdateRepo = 'JordanJacquot/OptiGame'   # dépôt GitHub où sont publiées les mises à jour

# ---------------------------------------------------------------------------
# Droits administrateur
# ---------------------------------------------------------------------------
Add-Type -AssemblyName PresentationFramework, PresentationCore, WindowsBase, System.Drawing

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
        try { $script:Ignored = @(Get-Content $IgnoreFile -Raw -Encoding UTF8 | ConvertFrom-Json | ForEach-Object { [string]$_ }) } catch {}
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
function Test-IsLaptop($Battery) {
    $mobileChassis  = 8, 9, 10, 11, 12, 14, 18, 21, 30, 31, 32
    $desktopChassis = 3, 4, 5, 6, 7, 13, 15, 16, 17, 23, 24, 35, 36
    $chassis = @((Get-CimInstance Win32_SystemEnclosure -ErrorAction SilentlyContinue).ChassisTypes | ForEach-Object { [int]$_ })
    $pcType = [int](Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue).PCSystemType
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
    try {
        $route = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction Stop | Sort-Object RouteMetric | Select-Object -First 1
        $ad = Get-NetAdapter -InterfaceIndex $route.ifIndex -ErrorAction Stop
        $wifi = ([string]$ad.PhysicalMediaType -match '802\.11') -or ($ad.InterfaceDescription -match 'Wi-?Fi|Wireless|WLAN')
        return @{ IfIndex = $route.ifIndex; Gateway = $route.NextHop; Name = $ad.Name; Desc = $ad.InterfaceDescription; Speed = $ad.LinkSpeed; Wifi = $wifi; Guid = $ad.InterfaceGuid }
    } catch { return $null }
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
            <ContentPresenter Grid.Column="1" ContentSource="SelectedContent" Margin="28,22,28,16"/>
          </Grid>
        </ControlTemplate>
      </TabControl.Template>

      <!-- Tableau de bord -->
      <TabItem>
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
      <TabItem>
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
      <TabItem>
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
      <TabItem>
        <TabItem.Header>
          <StackPanel Orientation="Horizontal">
            <TextBlock Text="&#xE774;" FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets" FontSize="15" Margin="0,2,12,0"/>
            <TextBlock Text="Réseau"/>
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
              <TextBlock Style="{StaticResource H1}" Text="Réseau"/>
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
      <TabItem>
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

      <!-- Sauvegarde -->
      <TabItem>
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

# Lance un script dans un fil séparé pour que la fenêtre reste réactive.
function Invoke-Async([scriptblock]$Script, $Argument) {
    $ps = [PowerShell]::Create()
    [void]$ps.AddScript($Script.ToString())
    if ($null -ne $Argument) { [void]$ps.AddArgument($Argument) }
    $handle = $ps.BeginInvoke()
    while (-not $handle.IsCompleted) { Update-UI; Start-Sleep -Milliseconds 60 }
    try { $out = $ps.EndInvoke($handle) } finally { $ps.Dispose() }
    foreach ($o in $out) { $o }
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
        $pb.Value = [math]::Min(100, [math]::Max(0, [double]$b.Value))
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

    # Système
    $os = Get-CimInstance Win32_OperatingSystem
    $script:Build = [int]$os.BuildNumber
    $battery = @(Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue)
    $script:IsLaptop = Test-IsLaptop $battery
    if ($script:IsLaptop -and ($battery | Where-Object { $_.BatteryStatus -eq 1 })) {
        Add-Finding $F 'warn' 'Portable sur batterie' 'Sur batterie, Windows bride le processeur et la carte graphique. Branche le chargeur pour jouer.' 2 -Id 'laptop-battery' -Fix (New-Fix `
            -Why 'Sur batterie, le processeur et la carte graphique tournent au ralenti pour économiser l''énergie: tu peux perdre la moitié de tes FPS.' `
            -Steps @('Branche le chargeur de ton portable avant de jouer.', 'Dans Paramètres > Système > Alimentation, choisis le mode « Meilleures performances ».', 'Relance l''analyse.') `
            -Open 'ms-settings:powersleep' -OpenLabel 'Paramètres d''alimentation')
    }

    # --- Processeur
    Set-Status 'Analyse du processeur...'
    $cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
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
    $gpus = @(Get-CimInstance Win32_VideoController | Where-Object { $_.Name -notmatch 'Remote|Virtual|Parsec|Mirage|DisplayLink|Citrix|Meta' })
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
        $games = @(Get-InstalledGames)
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
    $mem = @(Get-CimInstance Win32_PhysicalMemory)
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
        $md = Get-WinEvent -FilterHashtable @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-MemoryDiagnostics-Results' } -MaxEvents 1 -ErrorAction SilentlyContinue
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
    try { $sysDisk = [string](Get-Partition -DriveLetter $env:SystemDrive.TrimEnd(':') -ErrorAction Stop).DiskNumber } catch {}
    foreach ($d in @(Get-PhysicalDisk -ErrorAction SilentlyContinue | Sort-Object { [int]$_.DeviceId })) {
        $media = [string]$d.MediaType; $bus = [string]$d.BusType
        $kind = if ($bus -eq 'NVMe') { 'SSD NVMe' } elseif ($media -eq 'SSD') { 'SSD' } elseif ($media -eq 'HDD') { 'Disque dur' } else { 'Disque' }
        $tag = if ($bus -eq 'USB') { 'USB' } elseif ($media -eq 'HDD') { 'HDD' } else { 'SSD' }
        $isSys = [string]$d.DeviceId -eq $sysDisk
        $sous = "$kind, $(Format-Size $d.Size)" + $(if ($bus -eq 'USB') { ', externe' } else { '' }) + $(if ($isSys) { ', Windows' } else { '' })
        $c = New-Component $tag (([string]$d.FriendlyName).Trim()) $sous

        $parts = @()
        try { $parts = @(Get-Partition -DiskNumber ([int]$d.DeviceId) -ErrorAction Stop | Where-Object { [int][char]$_.DriveLetter -ne 0 }) } catch {}
        foreach ($pt in $parts) {
            $v = Get-Volume -DriveLetter $pt.DriveLetter -ErrorAction SilentlyContinue
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
        try { $rel = $d | Get-StorageReliabilityCounter -ErrorAction Stop } catch {}
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
    $ld = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='$($env:SystemDrive)'"
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
    $bb = Get-CimInstance Win32_BaseBoard -ErrorAction SilentlyContinue
    $bios = Get-CimInstance Win32_BIOS -ErrorAction SilentlyContinue
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
        $sb = Confirm-SecureBootUEFI -ErrorAction Stop
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
        $tpm = Get-CimInstance -Namespace 'root\cimv2\security\microsofttpm' -ClassName Win32_Tpm -ErrorAction Stop
        if ($tpm) { $c.Lines['Puce TPM'] = @("Version $(([string]$tpm.SpecVersion -split ',')[0].Trim())", $Colors.ok) }
        else { $c.Lines['Puce TPM'] = @('Non détectée', $Colors.warn) }
    } catch {}
    [void]$cards.Add($c)

    # --- Stabilité
    Set-Status 'Lecture du journal des erreurs...'
    $bsod = Get-EventCount @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WER-SystemErrorReporting'; Id = 1001; StartTime = $since }
    $crash = Get-EventCount @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-Kernel-Power'; Id = 41; StartTime = $since }
    $wheaErr = Get-EventCount @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WHEA-Logger'; Level = @(1, 2); StartTime = $since }
    $wheaWarn = Get-EventCount @{ LogName = 'System'; ProviderName = 'Microsoft-Windows-WHEA-Logger'; Level = 3; StartTime = $since }
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
            $health = [math]::Min(100, 100 * $full / $design)
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
    $script:Net = Get-ActiveNet
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
            $sync.Cpu = [math]::Min(100, [double]$pi.PercentProcessorUtility)
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
                $sync.Gpu = [math]::Min(100, [double](($eng | Measure-Object UtilizationPercentage -Sum).Sum))
            } catch {}
        }
        $sync.Updated = Get-Date
        Start-Sleep -Milliseconds 1500
    }
}

function Set-Gauge($Val, $Bar, $Sub, $Value, [string]$Text, [string]$SubText, [double]$Warn = 75, [double]$Bad = 90) {
    if ($null -eq $Value) { $Val.Text = 'N/D'; $Bar.Value = 0; $Sub.Text = $SubText; return }
    $Val.Text = $Text
    $Bar.Value = [math]::Min(100, [math]::Max(0, [double]$Value))
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
        $dns = (Get-DnsClientServerAddress -InterfaceIndex $n.IfIndex -AddressFamily IPv4 -ErrorAction Stop).ServerAddresses -join ', '
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
        $freed += [math]::Max(0, $r.Size - $after)
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
$Window.Add_KeyDown({
    param($s, $e)
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
    Start-Live
    Invoke-Safe {
        Invoke-Analysis
        Build-GamingTab
        Update-StartupList
        Update-NetInfo
        Update-BackupSummary
    }
    try { Invoke-UpdateCheck } catch { Write-Log "Vérification de mise à jour: $_" }
})

# ---------------------------------------------------------------------------
# Lancement
# ---------------------------------------------------------------------------
Import-Backup
Import-Ignored
$script:RestoreDone = $false
$script:Build = [int](Get-CimInstance Win32_OperatingSystem).BuildNumber
$script:TweakRows = @()
$script:CleanRows = @()
$script:PingResults = @()
Write-Log "Démarrage OptiGame $AppVersion"
[void]$Window.ShowDialog()
if ($script:Relaunch -and (Test-Path -LiteralPath $script:Relaunch)) {
    Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-File', "`"$script:Relaunch`"")
}
