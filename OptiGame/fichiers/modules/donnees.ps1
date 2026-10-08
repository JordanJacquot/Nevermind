# Nevermind : données, sauvegarde et accès au registre.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Données et sauvegarde
# ---------------------------------------------------------------------------
$DataDir    = Join-Path $env:LOCALAPPDATA 'OptiGame'
$BackupFile = Join-Path $DataDir 'sauvegarde.json'
$LogFile    = Join-Path $DataDir 'journal.txt'
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null

function Write-Log([string]$Message) {
    $line = "$(Get-Date -Format s) $Message`r`n"
    try { [IO.File]::AppendAllText($LogFile, $line, (New-Object Text.UTF8Encoding($false))) }
    catch {
        # Journal inaccessible : on garde la cause (une seule fois) dans les préférences, qui elles s'écrivent.
        if (-not $script:LogError) {
            $script:LogError = "$(Get-Date -Format s) $($_.Exception.GetType().Name): $($_.Exception.Message)"
            try { Set-Setting 'LogError' $script:LogError } catch {}
        }
    }
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

# Préférences de l'utilisateur (versions bêta, dernière version vue, visite guidée...).
$SettingsFile = Join-Path $DataDir 'parametres.json'

function Get-Setting([string]$Name, $Default = $null) {
    if ($null -eq $script:Settings) {
        $script:Settings = @{}
        if (Test-Path -LiteralPath $SettingsFile) {
            try { $j = ConvertFrom-Json (Get-Content -LiteralPath $SettingsFile -Raw -Encoding UTF8); foreach ($p in $j.PSObject.Properties) { $script:Settings[$p.Name] = $p.Value } } catch { Write-Log "Lecture des préférences impossible: $_" }
        }
    }
    if ($script:Settings.ContainsKey($Name)) { $script:Settings[$Name] } else { $Default }
}

function Set-Setting([string]$Name, $Value) {
    [void](Get-Setting $Name)
    $script:Settings[$Name] = $Value
    try { ConvertTo-Json -InputObject $script:Settings | Set-Content -LiteralPath $SettingsFile -Encoding UTF8 } catch { Write-Log "Écriture des préférences impossible: $_" }
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

function Clear-Reg([string]$Path, [string]$Name) {
    $st = Get-RegState $Path $Name
    if (-not $st.Existed) { return }
    Save-Original $Path $Name
    if ($null -ne $script:RunLog) { [void]$script:RunLog.Add(@{ Type = 'reg'; Path = $Path; Name = $Name; Existed = $true; Value = $st.Value; Kind = $st.Kind }) }
    Remove-RegValue $Path $Name
}

function Set-Reg([string]$Path, [string]$Name, $Value, [string]$Kind = 'DWord') {
    Save-Original $Path $Name
    if ($null -ne $script:RunLog) {
        $st = Get-RegState $Path $Name
        [void]$script:RunLog.Add(@{ Type = 'reg'; Path = $Path; Name = $Name; Existed = $st.Existed; Value = $st.Value; Kind = $st.Kind })
    }
    Write-RegValue $Path $Name $Value $Kind
}
