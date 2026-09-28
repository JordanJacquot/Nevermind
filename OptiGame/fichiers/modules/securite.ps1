# OptiGame : onglet Sécurité (antivirus, points suspects, analyses Defender).
# Chargé par OptiGame.ps1, qui définit $AppDir et $ModulesDir.

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
        $shown = if ($Check.ShowAll) { @($Check.Items) } else { @($Check.Items | Select-Object -First 5) }
        foreach ($i in $shown) {
            $t2 = New-Text ([string]$i) 12 '#C9CED8'
            if ($Check.ShowAll) { $t2.TextWrapping = 'Wrap' } else { $t2.TextTrimming = 'CharacterEllipsis'; $t2.TextWrapping = 'NoWrap'; $t2.ToolTip = [string]$i }
            $t2.Margin = New-Thickness 0 2 0 2
            [void]$items.Children.Add($t2)
        }
        if (-not $Check.ShowAll -and @($Check.Items).Count -gt 5) { [void]$items.Children.Add((New-Text "et $(@($Check.Items).Count - 5) autre(s)..." 12 '#5B6475')) }
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
            $b.Tag = @{ A = $a; T = $Check.Title }
            $b.Add_Click({
                param($s, $e)
                $act = $s.Tag.A
                $ttl = $s.Tag.T
                if ($act.Confirm -and -not (Confirm-Action $act.Confirm)) { return }
                Invoke-Safe {
                    $script:RunLog = New-Object System.Collections.ArrayList
                    try { & $act.Script $act.Arg } finally { $log = $script:RunLog; $script:RunLog = $null }
                    [void](Add-History "$($act.Label) ($ttl)" @() $log)
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
