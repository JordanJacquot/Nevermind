# Nevermind : onglet Sécurité (antivirus, points suspects, analyses Defender).
# Chargé par Nevermind.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Onglet Sécurité (antivirus Microsoft Defender + recherche de points suspects)
# ---------------------------------------------------------------------------
$MpCmd = Join-Path $env:ProgramFiles 'Windows Defender\MpCmdRun.exe'
$script:SecButtons = New-Object System.Collections.ArrayList

# État de l'antivirus et des protections de Windows.
# Lecture de fond lancée dès l'ouverture de l'app (Nevermind.ps1), en même temps que l'analyse du PC :
# elle tourne pendant que la fenêtre attend, au lieu de la faire hoqueter pendant qu'elle construit les pages.
function Start-ProtectionPrefetch {
    $ps = [PowerShell]::Create()
    $ps.RunspacePool = $script:Pool
    [void]$ps.AddScript($SecDataWork.ToString()).AddArgument((Get-ProtectionArgs))
    $script:SecPrefetch = @{ PS = $ps; Handle = $ps.BeginInvoke() }
}

function Get-ProtectionArgs {
    $riskDirs = @($env:TEMP, "$env:SystemDrive\Users\Public", $env:ProgramData, $env:APPDATA, $env:LOCALAPPDATA)
    @{
        UserDirs = @((Join-Path ([Environment]::GetFolderPath('UserProfile')) 'Downloads'), [Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('MyDocuments'))
        Folders = @(Get-SuspectFolders); StartupExes = @(Get-StartupItems | Where-Object { $_.Enabled } | ForEach-Object { $_.Exe })
        RiskDirs = $riskDirs; Temp = $env:TEMP; Public = "$env:SystemDrive\Users\Public"; OwnTask = $AutoStartTask; OwnScript = (Get-AppScript)
    }
}

function Get-ProtectionStatus {
    $d = $null
    if ($script:SecPrefetch) {
        $pf = $script:SecPrefetch; $script:SecPrefetch = $null
        Wait-Handle $pf.Handle
        try { $d = @($pf.PS.EndInvoke($pf.Handle))[0] } catch { $d = $null } finally { $pf.PS.Dispose() }
    }
    if (-not $d) { $d = Invoke-Async $SecDataWork (Get-ProtectionArgs) | Select-Object -First 1 }
    try { Add-AideLectureSecurite $d } catch { Write-Log "Aide : lecture de l'antivirus ($_)" }
    $off = @($d.FirewallOff)
    @{
        Mp = $d.Mp; OtherAv = @($d.OtherAv); FirewallOff = $off; Firewall = -not $off.Count
        Uac = (Get-RegValue 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System' 'EnableLUA') -ne 0
        Exclusions = @($d.Exclusions); Active = @($d.Active); Threats = @($d.Threats); Detections = @($d.Detections)
        # (nombre d'exclusions lu par l'Aide, sans leurs chemins : 0 si Nevermind voit lui-même la liste)
        ExclusionsNombre = $(if ($d.ExclusionsNombre -gt @($d.Exclusions).Count) { [int]$d.ExclusionsNombre } else { 0 })
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
    if (-not $S.Exclusions.Count -and -not $S.ExclusionsNombre) { $score += 10 }
    if ($mp -and $mp.QuickScanAge -le 14) { $score += 10 } elseif ($S.OtherAv.Count) { $score += 10 }
    if (-not $S.Active.Count) { $score += 10 }
    foreach ($c in $Checks) { if ($c.Status -eq 'bad') { $score -= 10 } elseif ($c.Status -eq 'warn') { $score -= 3 } }
    if ($S.Active.Count) { $score = [math]::Min(30.0, $score) }
    [int][math]::Max(0.0, [math]::Min(100.0, $score))
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

# Éléments reconnus par l'utilisateur (« C'est moi ») : ils ne comptent plus dans les points à vérifier
# ni dans le niveau de protection. La clé contient le chemin ou la commande exacte : si un virus
# modifie un élément approuvé, il ne correspond plus et redevient suspect.
function Get-SecTrusted { @(@(Get-Setting 'SecuriteApprouves' @()) | Where-Object { $_ -and $_.Key }) }

function Save-SecTrusted([array]$Entries) {
    Set-Setting 'SecuriteApprouves' @($Entries | ForEach-Object { @{ Key = [string]$_.Key; Text = [string]$_.Text; Kind = [string]$_.Kind } })
}

function Add-SecTrusted([array]$Entries) {
    $all = @(Get-SecTrusted)
    foreach ($e in $Entries) { if (-not @($all | Where-Object { $_.Key -eq $e.Key }).Count) { $all += $e } }
    Save-SecTrusted $all
    Write-Log "Sécurité : approuvé par l'utilisateur : $(($Entries | ForEach-Object { $_.Text }) -join ' | ')"
}

function Remove-SecTrusted([string]$Key) {
    Save-SecTrusted @(Get-SecTrusted | Where-Object { $_.Key -ne $Key })
}

function New-TrustEntry([string]$Kind, [string]$Key, [string]$Text) { @{ Kind = $Kind; Key = $Key; Text = $Text } }

# Recherche des points suspects. Retourne une liste de constats avec actions possibles.
# Les constats faits d'éléments (Trust, aligné sur Items) proposent « C'est moi » sur chaque élément.
function Get-SecurityChecks($S) {
    $list = New-Object System.Collections.ArrayList
    $mp = $S.Mp
    $trusted = @{}
    foreach ($t in Get-SecTrusted) { $trusted[[string]$t.Key] = $true }

    # Antivirus
    if ($mp -and $mp.AMRunningMode -eq 'Normal') {
        if (-not $mp.RealTimeProtectionEnabled) {
            [void]$list.Add(@{ Status = 'bad'; Title = 'Protection en temps réel désactivée'; Detail = 'Les virus ne sont plus bloqués au moment où ils arrivent. Réactive la protection.'
                Actions = @(@{ Label = 'Ouvrir la protection'; Script = { Open-Url 'windowsdefender://threatsettings' } }) })
        }
        if ($mp.AntivirusSignatureAge -gt 3) {
            [void]$list.Add(@{ Status = 'warn'; Title = "Base de virus vieille de $($mp.AntivirusSignatureAge) jours"; Detail = 'Les nouveaux virus ne sont pas encore connus de ton antivirus. Mets la base à jour.'
                Actions = @(@{ Label = 'Mettre à jour'; Script = { Update-Definitions } }) })
        }
    } elseif (-not $S.OtherAv.Count) {
        [void]$list.Add(@{ Status = 'bad'; Title = 'Aucun antivirus actif détecté'; Detail = 'Ton PC n''est pas protégé. Active Microsoft Defender dans Sécurité Windows.'
            Actions = @(@{ Label = 'Ouvrir Sécurité Windows'; Script = { Open-Url 'windowsdefender://threat' } }) })
    }

    # Exclusions de l'antivirus
    $excl = @($S.Exclusions | Where-Object { -not $trusted["exclusion|$_"] })
    if ($excl.Count) {
        $trust = @($excl | ForEach-Object { New-TrustEntry 'Exclusion de l''antivirus' "exclusion|$_" $_ })
        [void]$list.Add(@{ Status = 'warn'; Title = "$($excl.Count) élément$(if ($excl.Count -gt 1) {'s'}) exclu$(if ($excl.Count -gt 1) {'s'}) de l'antivirus"
            Detail = 'Ces dossiers ou programmes ne sont jamais analysés. Si tu ne les as pas ajoutés toi-même (ou un jeu que tu connais), c''est suspect : les virus s''ajoutent souvent eux-mêmes en exclusion.'
            Items = @($trust | ForEach-Object { $_.Text }); Trust = $trust
            Actions = @(@{ Label = 'Retirer les exclusions'; Confirm = 'Retirer ces exclusions de Microsoft Defender ? Ces éléments seront de nouveau analysés. Ce changement protège ton PC : « Tout restaurer » ne l''annulera pas (la façon de revenir en arrière est notée dans l''historique).'; Arg = $excl
                           Script = { param($a) [void](Invoke-Aide 'antivirus.retirer-exclusions' @{ Elements = @($a) }); Add-ChangementDefinitif 'Exclusions de l''antivirus retirées' "Exclusions retirées : $(@($a) -join ', ')" 'Sécurité Windows, Protection contre les virus et menaces, « Gérer les paramètres », Exclusions : ajouter de nouveau l''élément.' } }) })
    }
    # Sans droits : l'Aide ne donne que leur nombre (leurs chemins diraient où un virus échapperait à l'antivirus). On
    # les regarde dans Sécurité Windows. « Je les connais » vaut pour ce nombre-là : une exclusion de plus se revoit.
    $nbExcl = [int]$S.ExclusionsNombre
    if (-not $excl.Count -and $nbExcl -gt 0 -and -not $trusted["exclusions|$nbExcl"]) {
        $trust = @(New-TrustEntry 'Exclusions de l''antivirus' "exclusions|$nbExcl" "$nbExcl exclusion$(if ($nbExcl -gt 1) {'s'}) de l'antivirus")
        [void]$list.Add(@{ Status = 'warn'; Title = "$nbExcl élément$(if ($nbExcl -gt 1) {'s'}) exclu$(if ($nbExcl -gt 1) {'s'}) de l'antivirus"
            Detail = 'Ces dossiers ou programmes ne sont jamais analysés. Si tu ne les as pas ajoutés toi-même (ou un jeu que tu connais), c''est suspect : les virus s''ajoutent souvent eux-mêmes en exclusion. Regarde-les dans Sécurité Windows, Protection contre les virus et menaces, « Gérer les paramètres », Exclusions.'
            Trust = $trust
            Actions = @(@{ Label = 'Voir les exclusions'; NoRefresh = $true; Script = { Open-Url 'windowsdefender://threatsettings' } }) })
    }

    # Pare-feu et contrôle des comptes
    if (-not $S.Firewall) {
        [void]$list.Add(@{ Status = 'bad'; Title = 'Pare-feu désactivé'; Detail = "Le pare-feu de Windows est coupé ($($S.FirewallOff -join ', ')) : ton PC est exposé sur le réseau."
            Actions = @(@{ Label = 'Réactiver le pare-feu'; Confirm = 'Réactiver le pare-feu de Windows sur tous les réseaux ? Ce changement protège ton PC : « Tout restaurer » ne l''annulera pas (la façon de revenir en arrière est notée dans l''historique).'; Script = { [void](Invoke-Aide 'pare-feu.activer'); Add-ChangementDefinitif 'Pare-feu de Windows réactivé' 'Pare-feu réactivé sur tous les réseaux' 'Sécurité Windows, « Pare-feu et protection du réseau » : choisir le réseau et désactiver le pare-feu.' } }) })
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
    $entries = @($entries | Where-Object { -not $trusted["hosts|$_"] })
    if ($entries.Count) {
        $trust = @($entries | ForEach-Object { New-TrustEntry 'Ligne du fichier hosts' "hosts|$_" $_ })
        [void]$list.Add(@{ Status = 'warn'; Title = "Fichier hosts modifié ($($entries.Count) ligne$(if ($entries.Count -gt 1) {'s'}))"
            Detail = 'Ce fichier peut rediriger des sites vers d''autres adresses. Des logiciels l''utilisent pour bloquer la pub, mais un virus peut s''en servir pour t''envoyer vers de faux sites.'
            Items = @($trust | ForEach-Object { $_.Text }); Trust = $trust
            Actions = @(@{ Label = 'Ouvrir le fichier'; Arg = $hosts; Script = { param($a) Start-Process 'notepad.exe' -ArgumentList "`"$a`"" } }) })
    }

    # Proxy
    $inet = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
    $proxyOn = (Get-RegValue $inet 'ProxyEnable') -eq 1
    $pac = [string](Get-RegValue $inet 'AutoConfigURL')
    if ($proxyOn -or $pac) {
        $what = if ($pac) { "script $pac" } else { [string](Get-RegValue $inet 'ProxyServer') }
    }
    if (($proxyOn -or $pac) -and -not $trusted["proxy|$what"]) {
        [void]$list.Add(@{ Status = 'warn'; Title = 'Un proxy détourne ta navigation'; Detail = "Tout ton trafic web passe par : $what. Si tu ne l'as pas configuré toi-même (VPN, travail), c'est suspect."
            Trust = @(New-TrustEntry 'Proxy' "proxy|$what" $what)
            Actions = @(@{ Label = 'Paramètres du proxy'; Script = { Open-Url 'ms-settings:network-proxy' } }) })
    }
    Update-UI

    # Fichiers piégés (double extension, scripts) dans les dossiers de l'utilisateur
    $double = @($S.Double | Where-Object { -not $trusted["fichier|$_"] }); $scripts = @($S.Scripts | Where-Object { -not $trusted["fichier|$_"] })
    if ($double.Count) {
        $trust = @($double | ForEach-Object { New-TrustEntry 'Fichier' "fichier|$_" $_ })
        [void]$list.Add(@{ Status = 'bad'; Title = "$($double.Count) fichier$(if ($double.Count -gt 1) {'s'}) déguisé$(if ($double.Count -gt 1) {'s'})"
            Detail = 'Un nom comme « facture.pdf.exe » fait croire à un document, mais c''est un programme. C''est la ruse la plus courante des virus : ne l''ouvre surtout pas.'
            Items = $double; Trust = $trust
            Actions = @(@{ Label = 'Analyser avec Defender'; Arg = $double; Script = { param($a) Invoke-DefenderScan 'CustomScan' $a 'Analyse des fichiers déguisés' } },
                        @{ Label = 'Ouvrir l''emplacement'; Arg = $double[0]; Script = { param($a) Start-Process 'explorer.exe' -ArgumentList "/select,`"$a`"" } }) })
    }
    if ($scripts.Count) {
        $trust = @($scripts | ForEach-Object { New-TrustEntry 'Fichier' "fichier|$_" $_ })
        [void]$list.Add(@{ Status = 'warn'; Title = "$($scripts.Count) script$(if ($scripts.Count -gt 1) {'s'}) dans tes téléchargements"
            Detail = 'Ces types de fichiers (.vbs, .js, .hta, .scr...) servent rarement à autre chose qu''à installer des virus quand ils viennent d''Internet. Si tu ne sais pas d''où ils viennent, supprime-les.'
            Items = $scripts; Trust = $trust
            Actions = @(@{ Label = 'Analyser avec Defender'; Arg = $scripts; Script = { param($a) Invoke-DefenderScan 'CustomScan' $a 'Analyse des scripts téléchargés' } },
                        @{ Label = 'Ouvrir l''emplacement'; Arg = $scripts[0]; Script = { param($a) Start-Process 'explorer.exe' -ArgumentList "/select,`"$a`"" } }) })
    }
    Update-UI

    # Programmes non signés cachés dans des dossiers à risque
    $hidden = @($S.Hidden | Where-Object { -not $trusted["fichier|$_"] })
    if ($hidden.Count) {
        $trust = @($hidden | ForEach-Object { New-TrustEntry 'Programme' "fichier|$_" $_ })
        [void]$list.Add(@{ Status = 'warn'; Title = "$($hidden.Count) programme$(if ($hidden.Count -gt 1) {'s'}) non signé$(if ($hidden.Count -gt 1) {'s'}) dans des dossiers à risque"
            Detail = 'Ces programmes sont posés directement dans des dossiers où les virus aiment se cacher, et aucun éditeur ne les a signés. Ce n''est pas forcément grave (vieux installeurs...), mais ça vaut une analyse.'
            Items = $hidden; Trust = $trust
            Actions = @(@{ Label = 'Analyser avec Defender'; Arg = $hidden; Script = { param($a) Invoke-DefenderScan 'CustomScan' $a 'Analyse des programmes cachés' } },
                        @{ Label = 'Ouvrir l''emplacement'; Arg = $hidden[0]; Script = { param($a) Start-Process 'explorer.exe' -ArgumentList "/select,`"$a`"" } }) })
    }

    # Programmes au démarrage placés dans des dossiers à risque ou non signés
    $susStart = @(Get-StartupItems | Where-Object { $_.Enabled -and ($S.SusStart -contains $_.Exe) -and -not $trusted["demarrage|$($_.Nom)|$($_.Exe)"] })
    if ($susStart.Count) {
        $trust = @($susStart | ForEach-Object { New-TrustEntry 'Programme au démarrage' "demarrage|$($_.Nom)|$($_.Exe)" "$($_.Nom)  :  $($_.Exe)" })
        [void]$list.Add(@{ Status = 'warn'; Title = "$($susStart.Count) programme$(if ($susStart.Count -gt 1) {'s'}) au démarrage à vérifier"
            Detail = 'Ils se lancent avec Windows et ne sont pas signés par un éditeur, ou sont rangés dans un dossier inhabituel. Si tu ne les reconnais pas, analyse-les et désactive-les.'
            Items = @($trust | ForEach-Object { $_.Text }); Trust = $trust
            Actions = @(@{ Label = 'Analyser avec Defender'; Arg = @($susStart | ForEach-Object { $_.Exe }); Script = { param($a) Invoke-DefenderScan 'CustomScan' $a 'Analyse des programmes au démarrage' } },
                        @{ Label = 'Désactiver au démarrage'; Arg = $susStart; Confirm = 'Désactiver ces programmes au démarrage ? Ils restent installés.'; Script = { param($a) foreach ($i in $a) { Set-StartupState $i $false }; Update-StartupList } }) })
    }
    Update-UI

    # Tâches planifiées suspectes
    $susTasks = @($S.Tasks | Where-Object { -not $trusted["tache|$($_.Path)$($_.Name)|$($_.Cmd)"] })
    if ($susTasks.Count) {
        $trust = @($susTasks | ForEach-Object { New-TrustEntry 'Tâche planifiée' "tache|$($_.Path)$($_.Name)|$($_.Cmd)" "$($_.Name)  :  $($_.Cmd)" })
        $bad = [bool]($susTasks | Where-Object { $_.Bad })
        [void]$list.Add(@{ Status = $(if ($bad) { 'bad' } else { 'warn' }); Title = "$($susTasks.Count) tâche$(if ($susTasks.Count -gt 1) {'s'}) planifiée$(if ($susTasks.Count -gt 1) {'s'}) suspecte$(if ($susTasks.Count -gt 1) {'s'})"
            Detail = 'Ces tâches lancent en cachette des commandes qui téléchargent ou exécutent du code, ou des programmes rangés dans des dossiers temporaires. C''est une technique classique des virus pour revenir après chaque redémarrage.'
            Items = @($trust | ForEach-Object { $_.Text }); Trust = $trust
            Actions = @(@{ Label = 'Désactiver ces tâches'; Arg = $susTasks; Confirm = 'Désactiver ces tâches planifiées ? Tu pourras les réactiver dans le Planificateur de tâches.'
                           Script = { param($a) Disable-SuspectTasks $a } },
                        @{ Label = 'Planificateur de tâches'; Script = { Start-Process 'taskschd.msc' } }) })
    }
    Set-Status 'Vérification terminée.'
    $list
}

# « Désactiver ces tâches » : chaque tâche est désactivée puis relue. Sans droits, Windows refuse celles qu'un administrateur
# a créées : jamais comptées comme faites. Aucune désactivée : une erreur (rien n'est noté comme fait dans l'historique) ;
# une partie seulement : un message qui nomme celles qui restent.
function Disable-SuspectTasks([array]$Tasks) {
    $faites = @(); $ratees = @()
    foreach ($x in @($Tasks | Where-Object { $_ })) {
        $ok = $false
        try {
            Disable-ScheduledTask -TaskName $x.Name -TaskPath $x.Path -ErrorAction Stop | Out-Null
            $ok = [string](Get-ScheduledTask -TaskName $x.Name -TaskPath $x.Path -ErrorAction Stop).State -eq 'Disabled'
        } catch { Write-Log "Sécurité : tâche « $($x.Path)$($x.Name) » non désactivée ($($_.Exception.Message))" }
        if ($ok) { $faites += [string]$x.Name } else { $ratees += [string]$x.Name }
    }
    if (-not $ratees.Count) { return }
    $plus = $ratees.Count -gt 1
    $txt = "Je n'ai pas pu désactiver $(if ($plus) { 'ces tâches' } else { 'cette tâche' }) : $($ratees -join ', ').`n`nWindows ne me laisse pas faire : $(if ($plus) { 'elles ont sans doute été créées' } else { 'elle a sans doute été créée' }) par un administrateur. Lance une analyse rapide de Defender, et si tu ne $(if ($plus) { 'les' } else { 'la' }) reconnais pas, demande de l'aide à quelqu'un de confiance."
    if (-not $faites.Count) { throw $txt }
    Show-Message "$($faites.Count) tâche$(if ($faites.Count -gt 1) {'s'}) désactivée$(if ($faites.Count -gt 1) {'s'}).`n`n$txt" 'Warning'
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
    Add-ToGrid $g (New-Text $Label 13 '#A6A1BC') 1
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
    $d = New-Text $Check.Detail 12.5 '#A6A1BC'
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
        # Un bouton « C'est moi » par élément quand il y en a plusieurs (sinon celui des actions suffit)
        $perItem = @($Check.Trust).Count -gt 1 -and @($Check.Trust).Count -eq @($Check.Items).Count
        $n = 0
        foreach ($i in $shown) {
            $t2 = New-Text ([string]$i) 12 '#D3CDE3'
            if ($Check.ShowAll) { $t2.TextWrapping = 'Wrap' } else { $t2.TextTrimming = 'CharacterEllipsis'; $t2.TextWrapping = 'NoWrap'; $t2.ToolTip = [string]$i }
            $t2.Margin = New-Thickness 0 2 0 2
            if ($perItem) {
                $line = New-Grid @('*', 'Auto')
                $t2.VerticalAlignment = 'Center'
                Add-ToGrid $line $t2 0
                $ib = New-TrustButton 'C''est moi' @($Check.Trust[$n]) 'Marquer cet élément comme sûr ? Il ne comptera plus dans le niveau de protection. S''il change (autre commande, autre fichier), Nevermind le signalera de nouveau.'
                $ib.Margin = New-Thickness 10 2 0 2
                Add-ToGrid $line $ib 1
                [void]$items.Children.Add($line)
            } else { [void]$items.Children.Add($t2) }
            $n++
        }
        if (-not $Check.ShowAll -and @($Check.Items).Count -gt 5) { [void]$items.Children.Add((New-Text "et $(@($Check.Items).Count - 5) autre(s)..." 12 '#655E7E')) }
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
        if (@($Check.Trust).Count) {
            $cnt = @($Check.Trust).Count
            $tb = New-TrustButton $(if ($cnt -gt 1) { 'C''est moi, tout est OK' } else { 'C''est moi, c''est OK' }) @($Check.Trust) $(if ($cnt -gt 1) { "Marquer ces $cnt éléments comme sûrs ? Ils ne compteront plus dans le niveau de protection. S'ils changent (autre commande, autre fichier), Nevermind les signalera de nouveau." } else { 'Marquer cet élément comme sûr ? Il ne comptera plus dans le niveau de protection. S''il change (autre commande, autre fichier), Nevermind le signalera de nouveau.' })
            $tb.Padding = New-Thickness 14 6 14 6; $tb.FontSize = 13
            $tb.Margin = New-Thickness 0 0 8 0
            [void]$wp.Children.Add($tb)
        }
        [void]$sp.Children.Add($wp)
    }
    $card.Child = $sp
    $card
}

# Bouton qui marque des éléments comme sûrs (« C'est moi ») puis revérifie.
function New-TrustButton([string]$Label, [array]$Entries, [string]$Confirm) {
    $b = New-Button $Label 'BtnSecondary'
    $b.Padding = New-Thickness 10 3 10 3; $b.FontSize = 11.5
    $b.ToolTip = 'Tu sais d''où ça vient (c''est toi qui l''as mis en place) : Nevermind arrête de le compter comme suspect.'
    $b.Tag = @{ E = $Entries; C = $Confirm }
    $b.Add_Click({
        param($s, $e)
        if (-not (Confirm-Action $s.Tag.C)) { return }
        Invoke-Safe { Add-SecTrusted $s.Tag.E; if (-not $script:ScanRunning) { Update-SecurityTab } }
    })
    $b
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

# La protection vérifiée : la note et les données pour l'écran (ecran-protection.ps1), et les boutons d'analyse
function Update-SecurityTab {
    Set-Status 'Vérification de la sécurité...'
    $S = Get-ProtectionStatus
    $checks = @(Get-SecurityChecks $S)
    $score = Get-ProtectionScore $S $checks
    $mp = $S.Mp

    # Les boutons d'analyse (l'écran appuie dessus : clic:BtnScanQuick...) ne marchent qu'avec Microsoft Defender
    $defenderOk = $mp -and $mp.AMRunningMode -eq 'Normal'
    foreach ($b in $script:SecButtons) { $b.IsEnabled = [bool]$defenderOk }
    $ui.SecScanNote.Text = if ($defenderOk) { 'Les analyses utilisent Microsoft Defender, l''antivirus intégré à Windows.' } elseif ($S.OtherAv.Count) { "Ton antivirus ($($S.OtherAv -join ', ')) remplace Microsoft Defender : lance les analyses depuis ton antivirus." } else { 'Active Microsoft Defender pour pouvoir lancer une analyse.' }
    $hist = @(Get-ThreatHistory $S)
    $script:SecurityScore = $score
    $script:SecDonnees = @{ S = $S; Checks = $checks; Score = $score; Hist = $hist }   # (pour l'écran : ecran-protection.ps1)
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

# Menaces trouvées depuis le début d'une analyse : { Lu ($false = résultat inconnu), Menaces = @(@{ Nom; Fichier }) }.
# Sans droits, Nevermind ne peut pas se fier à sa propre lecture (Windows peut répondre « rien » sans erreur) : l'Aide lit
# les détections en administrateur (lecture.securite). Sans Aide ni droits, une liste vide reste « résultat inconnu »,
# jamais « aucune menace » ; une lecture qui échoue aussi.
function Get-ScanThreats([datetime]$Start) {
    $depuis = $Start.AddSeconds(-5)
    $det = $null; $noms = @{}; $fiable = $false
    if (-not $script:EstAdmin) {
        $l = Get-AideLecture 'lecture.securite'
        if ($l) {
            $det = @(@($l.Detections) | Where-Object { $_ } | ForEach-Object { @{ Id = [string]$_.ThreatID; Quand = [datetime][long]$_.Quand; Res = @($_.Resources) } })
            foreach ($m in @(@($l.Menaces) | Where-Object { $_ })) { $noms[[string]$m.ThreatID] = [string]$m.ThreatName }
            $fiable = [bool]$l.DetectionsLues -or [bool]@($det).Count   # (lecture de Defender ratée dans l'Aide : résultat inconnu, pas « aucune menace »)
        }
    }
    if (-not $fiable) {
        try {
            $det = @(Get-MpThreatDetection -ErrorAction Stop | ForEach-Object { @{ Id = [string]$_.ThreatID; Quand = [datetime]$_.InitialDetectionTime; Res = @($_.Resources) } })
            $fiable = [bool]$script:EstAdmin
        } catch { Write-Log "Analyse Defender : détections non lues ($($_.Exception.Message))"; $det = @() }
        try { foreach ($t in @(Get-MpThreat -ErrorAction Stop)) { $noms[[string]$t.ThreatID] = [string]$t.ThreatName } } catch {}
    }
    $menaces = @(@($det) | Where-Object { $_ -and $_.Quand -ge $depuis } | ForEach-Object { @{ Nom = [string]$noms[$_.Id]; Fichier = ((@($_.Res) | Select-Object -First 1) -replace '^file:_', '') } })
    @{ Lu = $fiable -or $menaces.Count -gt 0; Menaces = $menaces }
}

# -Fixe : sans animation (sous la console, le panneau est montré dans l'écran et le radar reste caché derrière)
function New-Radar([switch]$Fixe) {
    $g = New-Object System.Windows.Controls.Grid
    $g.Width = 190; $g.Height = 190
    $g.HorizontalAlignment = 'Center'
    $g.Margin = New-Thickness 0 10 0 6
    foreach ($r in 90, 64, 38) {
        $e = New-Object System.Windows.Shapes.Ellipse
        $e.Width = $r * 2; $e.Height = $r * 2
        $e.Stroke = Get-Brush '#1CFFFFFF'; $e.StrokeThickness = 1.5
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
    if (-not $Fixe) {
        $rot.BeginAnimation([System.Windows.Media.RotateTransform]::AngleProperty, $a)
        Start-Pulse $shield
    }
    @{ El = $g; Shield = $shield; Sweep = $sweep; Rot = $rot }
}

function Invoke-DefenderScan([string]$Type, [string[]]$Paths, [string]$Title) {
    if ($script:ScanRunning -or $script:TestRunning) { Show-Message 'Une analyse ou un test est déjà en cours.'; return }
    $names = @{ QuickScan = 'Analyse rapide'; FullScan = 'Analyse complète'; CustomScan = 'Analyse personnalisée' }
    if (-not $Title) { $Title = $names[$Type] }
    $script:ScanRunning = $true
    $script:TestRunning = $true
    $script:LastScanResult = ''
    Show-TestPanel @{ Tag = 'AV'; Title = $Title; Sub = 'Microsoft Defender' }
    Set-TestState 'run' 'Analyse en cours'
    Set-TestButtons 'run'
    $body = $ui.TestBody
    $console = [bool]($script:Kit -and $script:Kit.Actif)
    $radar = New-Radar -Fixe:$console
    if ($console) { $radar.El.Tag = 'SansImageKit' }   # (pas photographié dans le panneau de la console : l'état et le temps suffisent)
    [void]$body.Children.Add($radar.El)
    $time = New-Text '00:00' 40 '#FFFFFF' -Bold
    $time.HorizontalAlignment = 'Center'
    [void]$body.Children.Add($time)
    $hint = New-Text $(switch ($Type) {
        'QuickScan'  { 'Analyse des endroits où se cachent les virus. Ça prend en général quelques minutes, tu peux continuer à utiliser ton PC.' }
        'FullScan'   { 'Analyse de tous les fichiers du PC. Ça peut prendre une heure ou plus : tu peux continuer à utiliser ton PC.' }
        default      { "Analyse de $(@($Paths).Count) élément$(if (@($Paths).Count -gt 1) {'s'})." }
    }) 13 '#A6A1BC'
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
        $script:LastScanResult = 'cancel'
        Set-TestState 'info' 'Arrêtée'
        [void]$body.Children.Add((New-Verdict 'info' 'Analyse arrêtée avant la fin.'))
        Update-SecurityTab
        return
    }
    if ($r.Error) {
        $script:LastScanResult = 'error'
        Set-TestState 'bad' 'Échec'
        [void]$body.Children.Add((New-Verdict 'bad' "L'analyse n'a pas pu se faire : $($r.Error)"))
        return
    }
    $lu = Get-ScanThreats $start
    $found = @($lu.Menaces)
    $time.Text = '{0:00}:{1:00}' -f [math]::Floor($dur.TotalMinutes), $dur.Seconds
    $script:LastScanResult = if ($found.Count) { 'threats' } elseif ($lu.Lu) { 'clean' } else { 'unknown' }
    if (-not $lu.Lu) {
        # Résultat pas lu : jamais « Aucune menace »
        Set-TestState 'warn' 'Résultat inconnu'
        [void]$body.Children.Add((New-Verdict 'warn' 'L''analyse est terminée, mais je n''ai pas pu lire son résultat. Pour le voir, ouvre Sécurité Windows, puis « Protection contre les virus et menaces ».'))
    } elseif (-not $found.Count) {
        $radar.Shield.Fill = Get-Brush $Colors.ok
        $check = New-Text '✓' 30 '#07060C' -Bold
        $check.HorizontalAlignment = 'Center'; $check.VerticalAlignment = 'Center'
        [void]$radar.El.Children.Add($check)
        $scale = New-Object System.Windows.Media.ScaleTransform 0.6, 0.6
        $radar.Shield.RenderTransformOrigin = [System.Windows.Point]::new(0.5, 0.5)
        $radar.Shield.RenderTransform = $scale
        Start-WpfAnim $scale ([System.Windows.Media.ScaleTransform]::ScaleXProperty) 1 600
        Start-WpfAnim $scale ([System.Windows.Media.ScaleTransform]::ScaleYProperty) 1 600
        Set-TestState 'ok' 'Aucune menace'
        [void]$body.Children.Add((New-Verdict 'ok' ("Aucune menace trouvée. Analyse terminée en " + $(if ($dur.TotalSeconds -lt 60) { "moins d'une minute." } else { "$([int][math]::Round($dur.TotalMinutes)) min." }))))
    } else {
        $radar.Shield.Stroke = Get-Brush $Colors.bad
        $radar.Shield.Fill = Get-Brush '#3A1A1A'
        $radar.Shield.Effect = New-Glow $Colors.bad 20 0.6
        Set-TestState 'bad' "$($found.Count) menace$(if ($found.Count -gt 1) {'s'}) trouvée$(if ($found.Count -gt 1) {'s'})"
        foreach ($d in $found) { [void]$body.Children.Add((New-Verdict 'bad' "$($d.Nom)   $($d.Fichier)")) }
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
