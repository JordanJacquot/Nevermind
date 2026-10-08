# Nexo : ce que Windows envoie à Microsoft. Les identifiants du PC, les réglages qui envoient
# des données en plus du minimum (avec « Couper », annulable), et les envois vus en direct.
# Le contenu est chiffré : ce qui est envoyé vient de la documentation de Microsoft, pas d'une lecture.
# Chargé par OptiGame.ps1 après trafic.ps1.

$DiagKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection'
$DiagPolicyKey = 'HKLM:\SOFTWARE\Policies\Microsoft\Windows\DataCollection'
$CdmKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager'
$CdmValues = 'SubscribedContent-338388Enabled', 'SubscribedContent-338389Enabled', 'SubscribedContent-353694Enabled', 'SubscribedContent-353696Enabled', 'SilentInstalledAppsEnabled', 'SystemPaneSuggestionsEnabled', 'SoftLandingEnabled'

# Serveurs de Microsoft : à quel service de Windows ils servent
$MsDomains = '(?i)microsoft|windows\.com|windowsupdate|live\.com|live\.net|msn\.com|bing\.(com|net)|office|skype|xbox|azure|msedge|msftconnecttest|onedrive|sharepoint|1drv|microsoftonline|msauth|trafficmanager\.net|msecnd'
$MsEndpoints = @(
    @('v10\.events\.data|v20\.events\.data|self\.events\.data|mobile\.events\.data|browser\.events\.data|events\.data\.microsoft|umwatson|watson\.', 'Télémétrie de Windows', 'identifiant de l''appareil, modèle et matériel, version de Windows, plantages'),
    @('settings-win\.data|settings\.data\.microsoft', 'Réglages de la télémétrie', 'Windows demande à Microsoft quoi collecter (identifiant de l''appareil, version)'),
    @('arc\.msn\.com|iris\.microsoft|ris\.api|g\.msn\.com|api\.msn\.com|assets\.msn|ntp\.msn|www\.msn\.com|img-s-msn|msn\.com', 'Pubs et suggestions de Windows', 'langue, région, identifiant publicitaire s''il est activé, pour choisir les pubs et les suggestions'),
    @('login\.live|login\.microsoftonline|account\.live|account\.microsoft|msauth|msa\.', 'Compte Microsoft', 'ton identifiant de compte et un jeton de connexion chiffré'),
    @('wns\.windows|notify\.windows|client\.wns', 'Notifications', 'un identifiant de l''appareil pour recevoir les notifications des applis'),
    @('activity\.windows\.com', 'Historique d''activité', 'les applis et fichiers que tu ouvres, pour les retrouver sur tes autres appareils'),
    @('windowsupdate|delivery\.mp\.microsoft|update\.microsoft|dl\.delivery|tlu\.dl|download\.windowsupdate', 'Windows Update', 'version de Windows et liste des mises à jour installées, pour savoir quoi télécharger'),
    @('licensing\.mp|displaycatalog|storeedgefd|storecatalogrevocation|store\.microsoft', 'Microsoft Store et licences', 'identifiant de compte et de l''appareil, pour vérifier tes achats'),
    @('onedrive|skyapi|storage\.live|1drv|sharepoint', 'OneDrive', 'tes fichiers synchronisés et ton identifiant de compte'),
    @('bing\.(com|net)', 'Recherche Bing', 'ce que tu tapes dans la recherche Windows, pour proposer des résultats web'),
    @('xboxlive|xbox\.com', 'Xbox', 'ton compte Xbox, tes succès et ta présence en ligne'),
    @('smartscreen|wd\.microsoft|wdcp|defender', 'Protection (SmartScreen, Defender)', 'l''empreinte des fichiers et l''adresse des sites vérifiés, pour bloquer les virus'),
    @('edge\.microsoft|msedge', 'Microsoft Edge', 'synchronisation et services du navigateur'),
    @('msftconnecttest', 'Test de connexion', 'rien de personnel : Windows vérifie juste qu''Internet marche'),
    @('office|officeapps|outlook', 'Office et Outlook', 'tes documents ou tes mails si tu les utilises, et ton identifiant de compte')
)

function Get-MsService([string]$Name) {
    if (-not $Name) { return $null }
    foreach ($m in $MsEndpoints) { if ($Name -match $m[0]) { return @{ Label = $m[1]; Text = $m[2] } } }
    $null
}

# Services Windows d'un processus svchost (cache par processus)
function Get-SvcNames([int]$ProcId) {
    if (-not $script:SvcByPid) { $script:SvcByPid = @{} }
    if ($script:SvcByPid.ContainsKey($ProcId)) { return , $script:SvcByPid[$ProcId] }
    # Liste de tous les services relue au plus toutes les 5 secondes (instantané, sans WMI)
    if (-not $script:SvcMapAt -or ((Get-Date) - $script:SvcMapAt).TotalSeconds -gt 5) {
        $script:SvcMapAt = Get-Date
        $map = @{}
        try { foreach ($l in @([SvcMap]::Pids())) { $x = ([string]$l) -split '\|', 3; $k = [int]$x[0]; if (-not $map.ContainsKey($k)) { $map[$k] = @() }; $map[$k] += @{ Name = $x[1]; Title = $x[2] } } } catch {}
        $script:SvcMapAll = $map
    }
    $r = @($script:SvcMapAll[$ProcId] | Where-Object { $_ } | Sort-Object { $_.Name })
    if ($r.Count) { $script:SvcByPid[$ProcId] = $r }
    , $r
}

function Get-RegNum([string]$Path, [string]$Name) {
    $v = Get-RegValue $Path $Name
    if ($null -eq $v -or $v -is [array]) { return $null }
    try { [int]$v } catch { $null }
}

function Get-DiagLevel {
    $p = Get-RegNum $DiagPolicyKey 'AllowTelemetry'
    if ($null -ne $p) { return @{ Level = $p; Policy = $true } }
    $v = Get-RegNum $DiagKey 'AllowTelemetry'
    @{ Level = $(if ($null -ne $v) { $v } else { 1 }); Policy = $false; Default = ($null -eq $v) }
}

# Service arrêté et désactivé, noté pour « Revenir en arrière » et « Tout restaurer »
function Set-ServiceOff([string]$Name) {
    $s = Get-Service -Name $Name -ErrorAction Stop
    $start = [string]$s.StartType; $run = $s.Status -eq 'Running'
    $orig = @(Get-Setting 'SvcOriginal' @())
    if (-not @($orig | Where-Object { ([string]$_ -split '\|')[0] -eq $Name }).Count) { Set-Setting 'SvcOriginal' @($orig + "$Name|$start|$run") }
    if ($null -ne $script:RunLog) { [void]$script:RunLog.Add(@{ Type = 'svc'; Name = $Name; StartType = $start; Running = $run }) }
    Set-Service -Name $Name -StartupType Disabled -ErrorAction Stop
    if ($run) { Stop-Service -Name $Name -Force -ErrorAction Stop }
}

# Réglages qui envoient des données à Microsoft en plus du minimum.
# On = envoie en ce moment ; Off = script qui coupe (tout passe par Set-Reg ou Set-ServiceOff, donc annulable).
function Get-PrivacyItems {
    $diag = Get-DiagLevel
    $adid = Get-RegNum 'HKCU:\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo' 'Enabled'
    $tail = Get-RegNum 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Privacy' 'TailoredExperiencesWithDiagnosticDataEnabled'
    $tipc = Get-RegNum 'HKCU:\Software\Microsoft\Input\TIPC' 'Enabled'
    $cdm = @($CdmValues | ForEach-Object { Get-RegNum $CdmKey $_ } | Where-Object { $_ -eq 1 }).Count
    $iris = Get-RegNum 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' 'Start_IrisRecommendations'
    $bing = Get-RegNum 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Search' 'BingSearchEnabled'
    $bingPol = Get-RegNum 'HKCU:\Software\Policies\Microsoft\Windows\Explorer' 'DisableSearchBoxSuggestions'
    $speech = Get-RegNum 'HKCU:\Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy' 'HasAccepted'
    $dt = Get-Service -Name 'DiagTrack' -ErrorAction SilentlyContinue
    @(
        @{ Id = 'diag'; Title = 'Données de diagnostic facultatives'; On = ($diag.Level -ge 2); Admin = $true
           Text = 'En plus du minimum : les sites visités dans Edge, les applis que tu utilises et combien de temps, et le détail du matériel, avec l''identifiant de l''appareil.'
           # Fixé par une stratégie de groupe : c'est elle qui décide, la changer elle aussi
           Off = { Set-Reg $DiagKey 'AllowTelemetry' 1; if ($null -ne (Get-RegNum $DiagPolicyKey 'AllowTelemetry')) { Set-Reg $DiagPolicyKey 'AllowTelemetry' 1 } } },
        @{ Id = 'adid'; Title = 'Identifiant publicitaire'; On = ($adid -ne 0)
           Text = 'Un numéro unique donné aux applis et à leurs régies pub pour te reconnaître d''une appli à l''autre et cibler les pubs.'
           Off = { Set-Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo' 'Enabled' 0 } },
        @{ Id = 'tailored'; Title = 'Expériences personnalisées'; On = ($tail -ne 0)
           Text = 'Microsoft se sert de tes données de diagnostic pour choisir les conseils, pubs et recommandations qu''il te montre.'
           Off = { Set-Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Privacy' 'TailoredExperiencesWithDiagnosticDataEnabled' 0 } },
        @{ Id = 'suggest'; Title = 'Pubs et suggestions de Windows'; On = ($cdm -gt 0 -or $iris -eq 1)
           Text = 'Pubs du menu Démarrer et de l''écran de verrouillage, applis installées toutes seules, conseils. Windows envoie ta langue, ta région et ton identifiant publicitaire pour les choisir.'
           Off = { foreach ($n in $CdmValues) { Set-Reg $CdmKey $n 0 }; Set-Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' 'Start_IrisRecommendations' 0 } },
        @{ Id = 'bing'; Title = 'Recherche Bing dans le menu Démarrer'; On = ($bing -ne 0 -and $bingPol -ne 1)
           Text = 'Ce que tu tapes dans la recherche de Windows part chez Bing pour proposer des résultats web. S''applique après redémarrage.'
           Off = { Set-Reg 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Search' 'BingSearchEnabled' 0; Set-Reg 'HKCU:\Software\Policies\Microsoft\Windows\Explorer' 'DisableSearchBoxSuggestions' 1 } },
        @{ Id = 'typing'; Title = 'Envoi de ce que tu tapes et écris'; On = ($tipc -ne 0)
           Text = 'Des extraits de ce que tu tapes au clavier et écris au stylet, pour améliorer la saisie de Microsoft.'
           Off = { Set-Reg 'HKCU:\Software\Microsoft\Input\TIPC' 'Enabled' 0; Set-Reg 'HKCU:\Software\Microsoft\InputPersonalization' 'RestrictImplicitTextCollection' 1; Set-Reg 'HKCU:\Software\Microsoft\InputPersonalization' 'RestrictImplicitInkCollection' 1 } },
        @{ Id = 'speech'; Title = 'Reconnaissance vocale en ligne'; On = ($speech -eq 1)
           Text = 'Ta voix est envoyée à Microsoft quand tu dictes du texte ou parles à une appli.'
           Off = { Set-Reg 'HKCU:\Software\Microsoft\Speech_OneCore\Settings\OnlineSpeechPrivacy' 'HasAccepted' 0 } },
        @{ Id = 'diagtrack'; Title = 'Service de télémétrie (DiagTrack)'; On = [bool]($dt -and ($dt.Status -eq 'Running' -or [string]$dt.StartType -ne 'Disabled')); Admin = $true
           Text = 'C''est lui qui envoie les données de diagnostic avec l''identifiant de l''appareil. L''arrêter ne gêne ni Windows Update, ni les jeux, ni le Store.'
           Off = { Set-ServiceOff 'DiagTrack' } }
    )
}

# Identifiants du PC : lesquels existent et lesquels partent
function Get-PcIdentifiers {
    $diag = Get-DiagLevel
    $dt = Get-Service -Name 'DiagTrack' -ErrorAction SilentlyContinue
    $dtOn = $dt -and $dt.Status -eq 'Running'
    $adid = Get-RegNum 'HKCU:\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo' 'Enabled'
    $mid = ([string](Get-RegValue 'HKLM:\SOFTWARE\Microsoft\SQMClient' 'MachineId')).Trim('{', '}')
    $msa = try { [string](Get-LocalUser -Name $env:USERNAME -ErrorAction Stop).PrincipalSource } catch { '' }
    @(
        @('Identifiant de l''appareil', $(if ($dtOn) { 'Envoyé avec chaque rapport de diagnostic' } else { 'Pas envoyé : service de télémétrie arrêté' }), $(if ($dtOn) { $Colors.warn } else { $Colors.ok })),
        @('Niveau des diagnostics', $(if ($diag.Level -ge 2) { 'Facultatives (le maximum)' } else { "Obligatoires (le minimum)$(if ($diag.Default) { ', par défaut' })" }), $(if ($diag.Level -ge 2) { $Colors.warn } else { $Colors.ok })),
        @('Identifiant publicitaire', $(if ($adid -ne 0) { 'Activé : donné aux applis pour les pubs' } else { 'Désactivé' }), $(if ($adid -ne 0) { $Colors.warn } else { $Colors.ok })),
        @('Identifiant de l''installation', $(if ($mid) { "Présent ($($mid.Substring(0, [math]::Min(5, $mid.Length)))...), utilisé par les rapports d'erreurs" } else { 'Absent' }), '#FFFFFF'),
        @('Compte Microsoft', $(if ($msa -eq 'MicrosoftAccount') { 'Lié à ce PC : ton identifiant de compte part pour la synchro, le Store, OneDrive, Xbox' } else { 'Compte local : pas d''identifiant de compte envoyé' }), '#FFFFFF')
    )
}

# Programmes qui parlent à Microsoft en ce moment (surveillance du Trafic)
function Get-MsTraffic {
    $st = $script:Traffic
    if (-not $st) { return @() }
    $rows = @()
    foreach ($a in @($st.Apps.Values)) {
        $hits = @()
        foreach ($d in @($a.Dest.Values)) {
            if ($d.Private) { continue }
            $nm = [string]$st.Dns[[string]$d.Remote]
            $ow = Get-ServerOwner $d.Remote
            if ($nm -match $MsDomains -or ($ow -and "$($ow.O) $($ow.N)" -match '(?i)microsoft|msft')) { $hits += @{ D = $d; Name = $nm; Svc = (Get-MsService $nm) } }
        }
        if (-not $hits.Count) { continue }
        $o = 0.0; $i = 0.0
        foreach ($h in $hits) { $o += $h.D.Out; $i += $h.D.In }
        $rows += @{ App = $a; Hits = $hits; Out = $o; In = $i }
    }
    @($rows | Sort-Object @{ Expression = { $_.Out + $_.In } }, @{ Expression = { $_.Hits.Count } } -Descending)
}

# Coupe des réglages (journal pour « Revenir en arrière »), puis réaffiche la page
function Invoke-PrivacyOff([array]$Items) {
    $script:RunLog = New-Object System.Collections.ArrayList
    $errs = @()
    try {
        foreach ($it in $Items) { try { & $it.Off } catch { $errs += "$($it.Title) : $($_.Exception.Message)" } }
    } finally { $log = $script:RunLog; $script:RunLog = $null }
    [void](Add-History $(if ($Items.Count -gt 1) { 'Envois à Microsoft réduits' } else { "Coupé : $($Items[0].Title)" }) @($Items | ForEach-Object { $_.Title }) $log)
    if ($errs.Count) { Show-Message "Certains réglages n'ont pas pu être changés :`n`n$($errs -join "`n")" }
    Set-Status "$($Items.Count - $errs.Count) réglage(s) coupé(s). Annulable depuis la page Sauvegarde, historique."
    if (-not $script:TestRunning) { Show-WindowsPrivacy }
}

function New-PrivacyRow($It) {
    $b = New-Object System.Windows.Controls.Border
    $b.Background = Get-Brush '#1E232D'
    $b.BorderBrush = Get-Brush $(if ($It.On) { $Colors.warn } else { $Colors.ok })
    $b.BorderThickness = New-Thickness 3 0 0 0
    $b.CornerRadius = [System.Windows.CornerRadius]::new(8)
    $b.Padding = New-Thickness 16 11 14 11
    $b.Margin = New-Thickness 0 0 0 8
    $g = New-Grid @('*', 'Auto')
    $sp = New-Object System.Windows.Controls.StackPanel
    $hd = New-Object System.Windows.Controls.WrapPanel
    [void]$hd.Children.Add((New-Text $It.Title 14 '#FFFFFF' -Semi))
    [void]$hd.Children.Add((New-Badge $(if ($It.On) { 'Envoie' } else { 'Coupé' }) $(if ($It.On) { $Colors.warn } else { $Colors.ok })))
    [void]$sp.Children.Add($hd)
    $d = New-Text $It.Text 12 '#9AA3B2'
    $d.Margin = New-Thickness 0 4 0 0
    [void]$sp.Children.Add($d)
    Add-ToGrid $g $sp 0
    if ($It.On) {
        $btn = New-Button 'Couper'
        $btn.VerticalAlignment = 'Center'; $btn.Margin = New-Thickness 16 0 0 0
        $btn.Tag = $It
        $btn.Add_Click({ param($s, $e) $x = $s.Tag; Invoke-Safe { Invoke-PrivacyOff @($x) } })
        Add-ToGrid $g $btn 1
    }
    $b.Child = $g
    $b
}

function Show-WindowsPrivacy {
    if ($script:TestRunning) { return }
    Show-TestPanel @{ Tag = 'WIN'; Title = 'Ce que Windows envoie à Microsoft'; Sub = 'Identifiants, réglages et envois vus en direct' }
    Set-TestButtons 'done'
    $ui.BtnTestAgain.Visibility = 'Collapsed'
    $ui.TestProgress.Value = 100; $ui.TestPct.Text = ''
    $items = @(Get-PrivacyItems)
    $on = @($items | Where-Object { $_.On })
    Set-TestState $(if ($on.Count) { 'warn' } else { 'ok' }) $(if ($on.Count) { "$($on.Count) à couper" } else { 'Le minimum' })
    $body = $ui.TestBody

    # En résumé et action principale
    $v = New-Verdict $(if ($on.Count) { 'warn' } else { 'ok' }) $(if ($on.Count) { "$($on.Count) réglage$(if ($on.Count -gt 1) {'s'}) envoie$(if ($on.Count -gt 1) {'nt'}) des données à Microsoft en plus du minimum. Tu peux tout couper, c'est annulable." } else { 'Windows n''envoie que le minimum à Microsoft : tout ce qui peut être coupé l''est.' })
    $v.Margin = New-Thickness 0 4 0 0
    [void]$body.Children.Add($v)
    $wp = New-Object System.Windows.Controls.WrapPanel
    $wp.Margin = New-Thickness 0 10 0 0
    if ($on.Count) {
        $all = New-Button 'Tout couper' 'BtnPrimary'
        $all.Margin = New-Thickness 0 0 8 0
        $all.Tag = $on
        $all.Add_Click({ param($s, $e) $x = $s.Tag; Invoke-Safe { Invoke-PrivacyOff @($x) } })
        [void]$wp.Children.Add($all)
    }
    $vw = New-Button 'Voir le détail réel (outil de Microsoft)'
    $vw.Margin = New-Thickness 0 0 8 0
    $vw.ToolTip = 'Réglages de Windows, « Diagnostics et commentaires », « Afficher les données de diagnostic » : la liste exacte de ce qui est parti.'
    $vw.Add_Click({ Invoke-Safe { Open-Url 'ms-settings:privacy-feedback' } })
    [void]$wp.Children.Add($vw)
    [void]$body.Children.Add($wp)

    # Identifiants
    [void]$body.Children.Add((New-SectionTitle 'LES IDENTIFIANTS DE TON PC'))
    [void]$body.Children.Add((New-InfoRows (Get-PcIdentifiers)))

    # Réglages
    [void]$body.Children.Add((New-SectionTitle 'CE QUI ENVOIE DES DONNÉES'))
    foreach ($it in @($items | Sort-Object @{ Expression = { $_.On } } -Descending)) { [void]$body.Children.Add((New-PrivacyRow $it)) }

    # En direct
    [void]$body.Children.Add((New-SectionTitle 'EN CE MOMENT'))
    if (-not $script:Traffic) {
        [void]$body.Children.Add((New-Text 'Lance la surveillance de la page Trafic pour voir quels programmes parlent à Microsoft.' 12.5 '#9AA3B2'))
    } else {
        $ms = @(Get-MsTraffic)
        if (-not $ms.Count) { [void]$body.Children.Add((New-Text 'Aucun échange avec Microsoft vu depuis le début de la surveillance.' 12.5 '#9AA3B2')) }
        foreach ($r in @($ms | Select-Object -First 12)) {
            $b = New-Object System.Windows.Controls.Border
            $b.Background = Get-Brush '#1E232D'
            $b.CornerRadius = [System.Windows.CornerRadius]::new(8)
            $b.Padding = New-Thickness 16 10 16 10
            $b.Margin = New-Thickness 0 0 0 6
            $sp = New-Object System.Windows.Controls.StackPanel
            $hd = New-Grid @('*', 'Auto')
            $t = New-Text $r.App.Title 13.5 '#FFFFFF' -Semi
            $t.TextTrimming = 'CharacterEllipsis'; $t.TextWrapping = 'NoWrap'
            Add-ToGrid $hd $t 0
            $vol = New-Text "↑ $(Format-Bytes $r.Out)   ↓ $(Format-Bytes $r.In)" 12 '#C9CED8'
            $vol.Margin = New-Thickness 12 0 0 0
            Add-ToGrid $hd $vol 1
            [void]$sp.Children.Add($hd)
            $svcs = @{}
            foreach ($h in $r.Hits) { $k = if ($h.Svc) { $h.Svc.Label } else { '' }; if (-not $svcs.ContainsKey($k)) { $svcs[$k] = $h.Svc } }
            foreach ($k in @($svcs.Keys | Sort-Object { $_ -eq '' })) {
                $line = if ($k) { "$k : $($svcs[$k].Text)" } else { 'Autre service Microsoft : le nom du serveur ne dit pas lequel' }
                $l = New-Text $line 12 $(if ($k -in 'Télémétrie de Windows', 'Pubs et suggestions de Windows') { $Colors.warn } else { '#9AA3B2' })
                $l.Margin = New-Thickness 0 3 0 0
                [void]$sp.Children.Add($l)
            }
            $b.Child = $sp
            [void]$body.Children.Add($b)
        }
    }
    $n = New-Text 'Le contenu est chiffré : ce qui est envoyé est décrit d''après la documentation de Microsoft, pas lu. Les données « obligatoires » (identifiant de l''appareil, version, plantages) ne peuvent pas être coupées sur Windows Famille ou Pro sans arrêter le service de télémétrie.' 11.5 '#5B6475'
    $n.Margin = New-Thickness 0 12 0 0
    [void]$body.Children.Add($n)
}
