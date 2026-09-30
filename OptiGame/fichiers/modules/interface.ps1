# OptiGame : chargement de la fenêtre et aides pour l'interface.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Interface (la fenêtre est décrite dans interface.xaml)
# ---------------------------------------------------------------------------
[xml]$Xaml = [IO.File]::ReadAllText((Join-Path $ModulesDir 'interface.xaml'), [Text.Encoding]::UTF8)

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
    # Pendant le chargement, le détail de ce que fait l'app s'affiche sous la barre
    if ($ui.StartupOverlay -and $ui.StartupOverlay.Visibility -eq 'Visible') { $ui.StartupDetail.Text = $Text }
    Update-UI
}

# Écran de chargement du démarrage : étape en cours et barre qui avance en douceur
function Set-StartupStep([string]$Text, [double]$Pct) {
    $ui.StartupStep.Text = $Text
    $a = New-Object System.Windows.Media.Animation.DoubleAnimation
    $a.To = $Pct; $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds(500))
    $ui.StartupBar.BeginAnimation([System.Windows.Controls.Primitives.RangeBase]::ValueProperty, $a)
    Update-UI
}

function Hide-StartupOverlay {
    $o = $ui.StartupOverlay
    if ($o.Visibility -ne 'Visible') { return }
    $a = New-Object System.Windows.Media.Animation.DoubleAnimation
    $a.To = 0; $a.Duration = [System.Windows.Duration]::new([TimeSpan]::FromMilliseconds(300))
    # Une fois l'écran parti : « Prêt. », puis visite guidée ou nouveautés
    $a.Add_Completed({ $ui.StartupOverlay.Visibility = 'Collapsed'; if ($script:LogoGlow) { $script:LogoGlow.BeginAnimation([System.Windows.Media.Effects.DropShadowEffect]::OpacityProperty, $null); $ui.StartupLogo.Effect = $null }; $t = $script:StartupThen; $script:StartupThen = $null; if ($t) { & $t } })
    $o.BeginAnimation([System.Windows.UIElement]::OpacityProperty, $a)
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
    if ($r.Battery.Count) {
        $w = @{}
        try { $s = Get-CimInstance -Namespace root\wmi -ClassName BatteryStaticData -ErrorAction Stop | Select-Object -First 1; $w.Maker = ([string]$s.ManufactureName).Trim(); $w.Serial = ([string]$s.SerialNumber).Trim(); $w.Chem = $s.Chemistry; $w.Design = [double]$s.DesignedCapacity } catch {}
        try { $w.Full = [double](Get-CimInstance -Namespace root\wmi -ClassName BatteryFullChargedCapacity -ErrorAction Stop | Select-Object -First 1).FullChargedCapacity } catch {}
        try { $w.Volt = [double](Get-CimInstance -Namespace root\wmi -ClassName BatteryStatus -ErrorAction Stop | Select-Object -First 1).Voltage } catch {}
        try { $w.Runtime = [double](Get-CimInstance -Namespace root\wmi -ClassName BatteryRuntime -ErrorAction Stop | Select-Object -First 1).EstimatedRuntime } catch {}
        $r.BatWmi = $w
    }
    $r.UpsHints = @()
    try {
        foreach ($p in @(Get-CimInstance Win32_PnPEntity -Filter "DeviceID LIKE 'USB%' OR DeviceID LIKE 'HID%'" -ErrorAction Stop | Where-Object { [string]$_.DeviceID -match '^(USB|HID)\\VID_(051D|0463|0764|10AF|06DA|0D9F)&|^(USB|HID)\\VID_0665&PID_5161|^(USB|HID)\\VID_0925&PID_1234' -or ([string]$_.DeviceID -match '^(USB|HID)\\VID_09AE&' -and [string]$_.Name -match '(?i)ups|battery|batterie|power') -or [string]$_.Name -match '(?i)\bups\b|onduleur|uninterruptible|back-?ups|smart-?ups' })) {
            $vid = if ([string]$p.DeviceID -match 'VID_([0-9A-F]{4})') { $Matches[1] } else { '' }
            $r.UpsHints += @{ Kind = 'usb'; Name = [string]$p.Name; Vid = $vid; Ok = ([string]$p.Status -eq 'OK') }
        }
    } catch {}
    $upsSoft = '(?i)powerchute|powerpanel|viewpower|winpower|upsilon|intelligent power protector|power ?shield|upsmon|power ?master|apc data service|pbeagent|eaton ipp|cyberpower|smartpower|upsmart'
    try { foreach ($s in @(Get-CimInstance Win32_Service -ErrorAction Stop | Where-Object { "$($_.Name) $($_.DisplayName)" -match $upsSoft })) { $r.UpsHints += @{ Kind = 'soft'; Name = [string]$s.DisplayName; Ok = ([string]$s.State -eq 'Running') } } } catch {}
    foreach ($k in 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*') {
        try { foreach ($a in @(Get-ItemProperty $k -ErrorAction SilentlyContinue | Where-Object { [string]$_.DisplayName -match $upsSoft })) { $r.UpsHints += @{ Kind = 'app'; Name = [string]$a.DisplayName; Ok = $true } } } catch {}
    }
    $r.Chassis = @((Get-CimInstance Win32_SystemEnclosure -ErrorAction SilentlyContinue).ChassisTypes | ForEach-Object { [int]$_ })
    $r.PCType = [int](Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue).PCSystemType
    $r.CPU = Get-CimInstance Win32_Processor | Select-Object -First 1
    $r.GPUs = @(Get-CimInstance Win32_VideoController)
    # Dernier pilote NVIDIA « Game Ready » (le même pour toutes les GeForce récentes).
    if ($r.GPUs | Where-Object { $_.Name -match 'NVIDIA|GeForce' }) {
        try {
            [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
            $u = 'https://gfwsl.geforce.com/services_toolkit/services/com/nvidia/services/AjaxDriverService.php?func=DriverManualLookup&psid=127&pfid=995&osID=57&languageCode=1033&beta=0&isWHQL=1&dltype=-1&dch=1&upCRD=0&qnf=0&sort1=0&numberOfResults=1'
            $nv = (Invoke-RestMethod -Uri $u -TimeoutSec 6 -UseBasicParsing).IDS[0].downloadInfo
            if ($nv.Version) { $r.NvLatest = @{ Version = [string]$nv.Version; Date = [string]$nv.ReleaseDateTime; Url = [string]$nv.DetailsURL } }
        } catch {}
    }
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

function Get-Brush([string]$Hex) {
    switch ($Hex) {
        'card' { return $Window.FindResource('CardBg') }
        'card-hover' { return $Window.FindResource('CardHoverBg') }
        'card-border' { return $Window.FindResource('CardBorder') }
    }
    [System.Windows.Media.BrushConverter]::new().ConvertFromString($Hex)
}
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
    else { Open-Url $Target }
}
