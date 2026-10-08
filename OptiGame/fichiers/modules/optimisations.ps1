# OptiGame : optimisations gaming, programmes au démarrage, jeux installés.
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

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
# Onduleur : batterie au plomb (celles des portables sont au lithium), ou nom et marque d'onduleur.
function Test-IsUps($B) {
    if (-not $B) { return $false }
    if ([int]$B.Chemistry -eq 3) { return $true }
    "$($B.Name) $($B.DeviceID) $($B.Description)" -match '(?i)\bups\b|onduleur|back-?ups|smart-?ups|\bapc\b|eaton|cyberpower|powerwalker|bluewalker|salicru|riello|infosec|tripp.?lite|vertiv|liebert|ablerex|powercom|mustek|legrand|socomec|\bcp\d{3,4}|\bbr\d{3,4}|\bbx\d{3,4}|\bbe\d{3,4}|\bvi ?\d{3,4}'
}

# PC fixe certain (boîtier de bureau, tour, mini PC) : une « batterie » y est forcément un onduleur.
function Test-IsDesktop($Data) {
    $chassis = @($Data.Chassis); $pcType = [int]$Data.PCType
    if ($pcType -eq 2) { return $false }
    if ($chassis | Where-Object { 8, 9, 10, 11, 12, 14, 18, 21, 30, 31, 32 -contains $_ }) { return $false }
    [bool]($chassis | Where-Object { 3, 4, 5, 6, 7, 13, 15, 16, 17, 23, 24, 35, 36 -contains $_ })
}

# Marques d'onduleurs d'après l'identifiant USB du fabricant
$UpsVendors = @{ '051D' = 'APC'; '0463' = 'Eaton'; '0764' = 'CyberPower'; '09AE' = 'Tripp Lite'; '10AF' = 'Liebert (Vertiv)'; '06DA' = 'PowerWalker, Salicru ou Riello'; '0D9F' = 'Powercom'; '0665' = 'onduleur générique'; '0925' = 'onduleur générique' }

function Test-IsLaptop($Battery, $Data) {
    $Battery = @($Battery | Where-Object { -not (Test-IsUps $_) })
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

# Jeux installés, tous launchers confondus : Steam, Epic, Ubisoft Connect, EA app, GOG Galaxy, Battle.net,
# Riot, Rockstar, Amazon Games, Xbox / Game Pass. Chaque jeu : { Name, Exes (exécutables probables), Source }.
# Autonome (tourne dans un fil séparé) : n'utilise aucune autre fonction d'OptiGame.
function Get-InstalledGames {
    $bad = 'unins|setup|install|redist|dxsetup|directx|crash|report|easyanticheat|anticheat|eac_|beservice|battleye|_be$|update|helper|prereq|dotnet|webhelper|vcredist|python|java|browser|error|cleanup|touchup|repair|bootstrapper|resourcecompiler|^ui(32|64)$|diagnos|benchmark_?tool|ubisoftgamelauncher|uplay|^upc$|link2ea|socialclub|rockstarservice|cefsharp|leagueclient|riotclient|vanguard|^vgc$|blizzard ?error|agent$|gamelaunchhelper|launcher'
    $notGames = '^(wallpaper_engine|Steamworks Shared|SteamVR|Steam Controller Configs|Steamworks Common Redistributables|GameSave|Minecraft Launcher)$'
    # Les launchers eux mêmes ne sont pas des jeux
    $launchers = '^(Battle\.net|Ubisoft Connect|Uplay|EA app|EA Desktop|Origin|Riot Client|Riot Vanguard|Rockstar Games Launcher|Rockstar Games Social Club|GOG GALAXY|GOG Galaxy|Amazon Games|Xbox|Epic Games Launcher)\s*$'
    $games = @()
    $dirs = @()
    $norm = { param($p) if ($p) { ([string]$p -replace '/', '\').Trim().Trim('"').TrimEnd('\') } else { '' } }

    # Steam
    $steamCommon = @()
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
            # Vrai nom de chaque jeu (« Rocket League » au lieu du dossier « rocketleague »).
            $names = @{}
            foreach ($acf in @(Get-ChildItem -LiteralPath (Join-Path $lib 'steamapps') -Filter 'appmanifest_*.acf' -File -ErrorAction SilentlyContinue)) {
                $txt = Get-Content -LiteralPath $acf.FullName -Raw -Encoding UTF8 -ErrorAction SilentlyContinue
                if ($txt -match '"installdir"\s+"([^"]+)"') {
                    $installDir = $Matches[1].ToLower()
                    if ($txt -match '"name"\s+"([^"]+)"') { $names[$installDir] = $Matches[1] }
                }
            }
            $common = Join-Path $lib 'steamapps\common'
            if (Test-Path -LiteralPath $common) {
                $steamCommon += $common.ToLower()
                $dirs += Get-ChildItem -LiteralPath $common -Directory -ErrorAction SilentlyContinue |
                    Where-Object { $_.Name -notmatch $notGames } |
                    ForEach-Object { @{ Name = $(if ($names[$_.Name.ToLower()]) { $names[$_.Name.ToLower()] } else { $_.Name }); Dir = $_.FullName; Exe = $null; Source = 'Steam' } }
            }
        }
    }
    # Epic Games
    foreach ($m in @(Get-ChildItem "$env:ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item" -ErrorAction SilentlyContinue)) {
        try {
            $j = Get-Content -LiteralPath $m.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
            if ($j.InstallLocation -and (Test-Path -LiteralPath $j.InstallLocation)) {
                $exe = if ($j.LaunchExecutable) { Join-Path $j.InstallLocation $j.LaunchExecutable } else { $null }
                $dirs += @{ Name = $j.DisplayName; Dir = $j.InstallLocation; Exe = $exe; Source = 'Epic Games' }
            }
        } catch {}
    }
    # GOG Galaxy (nom, dossier et exécutable exacts)
    foreach ($k in @(Get-ChildItem 'HKLM:\SOFTWARE\WOW6432Node\GOG.com\Games' -ErrorAction SilentlyContinue)) {
        $p = Get-ItemProperty $k.PSPath -ErrorAction SilentlyContinue
        $dir = & $norm $p.path
        if ($p.gameName -and $dir -and (Test-Path -LiteralPath $dir)) { $dirs += @{ Name = [string]$p.gameName; Dir = $dir; Exe = $(& $norm $p.exe); Source = 'GOG' } }
    }
    # Ubisoft Connect : jeux installés même sans entrée « Programmes et fonctionnalités »
    $ubiNames = @{}
    foreach ($k in @(Get-ChildItem 'HKLM:\SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs' -ErrorAction SilentlyContinue)) {
        $dir = & $norm (Get-ItemProperty $k.PSPath -ErrorAction SilentlyContinue).InstallDir
        if ($dir) { $ubiNames[$dir.ToLower()] = $k.PSChildName }
    }
    # Programmes installés : les jeux des autres launchers s'y déclarent (nom, dossier, icône = souvent l'exécutable)
    $sources = @(
        @{ Re = '^Uplay Install'; Pub = 'Ubisoft'; Src = 'Ubisoft Connect' },
        @{ Re = '^Riot Game '; Pub = 'Riot Games'; Src = 'Riot' },
        @{ Re = ''; Pub = 'Electronic Arts'; Src = 'EA app' },
        @{ Re = ''; Pub = 'Blizzard Entertainment'; Src = 'Battle.net' },
        @{ Re = ''; Pub = 'Rockstar Games'; Src = 'Rockstar' },
        @{ Re = ''; Pub = 'GOG\.com|GOG Ltd'; Src = 'GOG' },
        @{ Re = '^AmazonGames/'; Pub = 'Amazon Games|Amazon Game Studios'; Src = 'Amazon Games' }
    )
    $uninst = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*', 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
    foreach ($u in @(Get-ItemProperty $uninst -ErrorAction SilentlyContinue)) {
        $src = $null
        foreach ($x in $sources) { if (($x.Re -and $u.PSChildName -match $x.Re) -or ($u.Publisher -and $u.Publisher -match $x.Pub)) { $src = $x.Src; break } }
        if (-not $src -or -not $u.DisplayName -or $u.DisplayName -match $launchers) { continue }
        $dir = & $norm $u.InstallLocation
        if (-not $dir -or -not (Test-Path -LiteralPath $dir)) { continue }
        # Déjà trouvé par Steam (un jeu EA ou Blizzard acheté sur Steam)
        $low = $dir.ToLower()
        if (@($steamCommon | Where-Object { $low.StartsWith($_) }).Count) { continue }
        if ($ubiNames.ContainsKey($low)) { $ubiNames.Remove($low) }
        $icon = (& $norm ($u.DisplayIcon -replace ',\s*-?\d+$', ''))
        $exe = if ($icon -match '\.exe$' -and $icon.ToLower().StartsWith($low)) { $icon } else { $null }
        $dirs += @{ Name = [string]$u.DisplayName.Trim(); Dir = $dir; Exe = $exe; Source = $src }
    }
    foreach ($d in @($ubiNames.Keys)) { if (Test-Path -LiteralPath $d) { $dirs += @{ Name = (Split-Path $d -Leaf); Dir = $d; Exe = $null; Source = 'Ubisoft Connect' } } }
    # Xbox / Game Pass : dossiers « XboxGames » à la racine des disques
    foreach ($drv in @([IO.DriveInfo]::GetDrives() | Where-Object { $_.DriveType -eq 'Fixed' -and $_.IsReady })) {
        $xb = Join-Path $drv.RootDirectory.FullName 'XboxGames'
        if (-not (Test-Path -LiteralPath $xb)) { continue }
        foreach ($g in @(Get-ChildItem -LiteralPath $xb -Directory -ErrorAction SilentlyContinue | Where-Object { $_.Name -notmatch $notGames })) {
            $content = Join-Path $g.FullName 'Content'
            $dirs += @{ Name = $g.Name; Dir = $(if (Test-Path -LiteralPath $content) { $content } else { $g.FullName }); Exe = $null; Source = 'Xbox' }
        }
    }

    $done = @{}
    foreach ($d in $dirs) {
        if (-not $d.Dir -or $done.ContainsKey($d.Dir.ToLower())) { continue }
        $done[$d.Dir.ToLower()] = $true
        # Les plus gros exécutables les moins enfouis d'abord : c'est presque toujours le jeu
        $exes = @(Get-ChildItem -LiteralPath $d.Dir -Filter *.exe -Recurse -Depth 3 -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Length -gt 200KB -and $_.BaseName -notmatch $bad } |
            Sort-Object @{ Expression = { ($_.FullName.Substring($d.Dir.Length) -split '\\').Count } }, @{ Expression = { $_.Length }; Descending = $true } |
            Select-Object -First 6 | ForEach-Object { $_.FullName })
        # Exécutable déclaré par le launcher : en tête, sauf si c'est un lanceur (Rocket League déclare « Launcher.exe »)
        if ($d.Exe -and (Test-Path -LiteralPath $d.Exe) -and ([IO.Path]::GetFileNameWithoutExtension($d.Exe) -notmatch $bad -or -not $exes.Count)) { $exes = @($d.Exe) + $exes }
        $exes = @($exes | Select-Object -Unique)
        if ($exes.Count) { $games += @{ Name = $d.Name; Exes = $exes; Source = $d.Source } }
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
