# Nevermind en Rust : « Désinstaller Nevermind.exe » essayé pour de vrai. SUR GITHUB SEULEMENT (machine jetable, en
# administrateur) : le test change de vrais réglages de Windows (registre, service, plan d'alimentation, tâches
# planifiées), crée la tâche SYSTEM de l'Aide et écrit suite.json dans %LOCALAPPDATA%\Mindset du compte. Sur un PC, il
# refuse de tourner.
#
#   & '.\outils\tester-desinstalleur-rust.ps1' -Nouvelle <dossier Nevermind en Rust construit>
#
# Comme chez un utilisateur dont Nevermind est géré par Mindset :
#   1. Nevermind en Rust est posé dans <tmp>\Mindset\Programmes\Nevermind (son dossier noté dans suite.json), avec des
#      données inventées dans <tmp>\Mindset\Donnees\Nevermind ; la tâche « \Mindset\Nevermind (aide) » est créée en
#      SYSTEM (-Sid du compte), comme le fait Mindset, et l'Aide est lancée ;
#   2. « tout optimiser » : de vrais réglages changés (HwSchMode dans HKLM, l'enregistrement de la Game Bar dans HKCU, une
#      valeur HKCU qui n'existait pas, le service DiagTrack, un plan d'alimentation créé et activé), notés dans
#      sauvegarde.json et parametres.json comme Nevermind les note ; les tâches « Nevermind (démarrage) » et
#      « OptiGame (démarrage) », la clé HKLM de l'Aide, les raccourcis du bureau et du menu Démarrer vers cette copie ;
#   3. « Désinstaller Nevermind.exe -GarderDonnees » (comme le lance le désinstalleur de Mindset) : il rend 0 ; tout est
#      remis comme avant ; l'Aide est arrêtée, sa tâche et sa clé retirées ; les tâches et les raccourcis retirés ;
#      Nevermind.exe et les autres programmes disparaissent, puis le dossier ; les données restent, sauf la sauvegarde,
#      l'historique et les demandes ; aucune fenêtre (piloté par Mindset : ni question, ni message de fin, qui est
#      écrit au journal ; le désinstalleur de Mindset montre son propre bilan) ; ses conseils pour ce bilan sont dans
#      <tmp>\Mindset\Donnees\Nevermind.conseils-desinstallation.txt (UTF-8 sans BOM, au plus 10 lignes de 300
#      caractères, « Redémarre le PC... » d'abord : des réglages ont été remis) ;
#   4. de nouveau, avec « -SupprimerDonnees » : tout le dossier des données part, le fichier des conseils reste ;
#   5. avec les deux options à la fois : la question s'affiche ; fermée (= Annuler), rien n'est touché, il rend 0.
# Code de sortie 1 si un cas échoue.
param([Parameter(Mandatory = $true)][string]$Nouvelle)

$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or -not $env:RUNNER_TEMP) {
    Write-Host 'Ce test ne tourne que sur GitHub : il change de vrais réglages de Windows, crée des tâches planifiées et désinstalle Nevermind pour de vrai.' -ForegroundColor Red
    exit 1
}
$admin = (New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { Write-Host 'Ce test doit tourner en administrateur (comme le désinstalleur lancé par celui de Mindset).' -ForegroundColor Red; exit 1 }
if (-not (Test-Path -LiteralPath (Join-Path $Nouvelle 'Désinstaller Nevermind.exe'))) { Write-Host "Désinstaller Nevermind.exe introuvable dans $Nouvelle" -ForegroundColor Red; exit 1 }

$base = Join-Path $env:RUNNER_TEMP 'desinstalleur-nevermind'
$prog = Join-Path $base 'Mindset\Programmes\Nevermind'
$donnees = Join-Path $base 'Mindset\Donnees\Nevermind'
# Les conseils passés à Mindset (piloté par une option), dans le dossier des données de la suite
$conseils = Join-Path $base 'Mindset\Donnees\Nevermind.conseils-desinstallation.txt'
$redemarrer = 'Redémarre le PC pour que tous les réglages d''origine soient pris en compte.'
# Le fichier des conseils est-il comme le contrat le veut, avec « Redémarre le PC » d'abord ?
function Test-Conseils {
    if (-not (Test-Path -LiteralPath $conseils -PathType Leaf)) { throw 'pas de fichier des conseils' }
    $o = [IO.File]::ReadAllBytes($conseils)
    if ($o.Length -ge 3 -and $o[0] -eq 0xEF -and $o[1] -eq 0xBB -and $o[2] -eq 0xBF) { throw 'le fichier a un BOM' }
    $l = @((New-Object Text.UTF8Encoding($false, $true)).GetString($o) -split "`r?`n" | Where-Object { $_ -ne '' })
    if (Test-Path -LiteralPath "$conseils.tmp") { throw 'le fichier d''écriture est resté' }
    $l.Count -ge 1 -and $l.Count -le 10 -and $l[0] -eq $redemarrer -and -not @($l | Where-Object { $_.Length -gt 300 -or $_ -match 'Nevermind est désinstallé' }).Count
}
$local = [Environment]::GetFolderPath('LocalApplicationData')
$suiteFichier = Join-Path $local 'Mindset\suite.json'
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$utf8 = New-Object Text.UTF8Encoding($false)
$utf8Bom = New-Object Text.UTF8Encoding($true)
$cleGraphiques = 'HKLM:\SYSTEM\CurrentControlSet\Control\GraphicsDrivers'
$cleGameDvr = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\GameDVR'
$cleGameBar = 'HKCU:\Software\Microsoft\GameBar'
$script:Echecs = @()
$script:Reussis = 0
function Test-Cas([string]$Nom, [scriptblock]$Condition) {
    $detail = ''
    try { $r = @(& $Condition) } catch { $r = @(); $detail = $_.Exception.Message }
    if ($r.Count -and $r[-1] -is [bool] -and $r[-1]) { $script:Reussis++; Write-Host "  OK     $Nom" -ForegroundColor Green }
    else { $script:Echecs += $Nom; Write-Host "  ÉCHEC  $Nom$(if ($detail) { " ($detail)" })" -ForegroundColor Red }
}
function Wait-Jusque([scriptblock]$Condition, [double]$Secondes) {
    $fin = (Get-Date).AddSeconds($Secondes)
    do { try { if (& $Condition) { return $true } } catch {}; Start-Sleep -Milliseconds 250 } while ((Get-Date) -lt $fin)
    $false
}
function Get-Valeur([string]$Cle, [string]$Nom) { $p = Get-ItemProperty -LiteralPath $Cle -Name $Nom -ErrorAction SilentlyContinue; if ($p) { $p.$Nom } else { $null } }
function Get-AidesEnCours { @(Get-Process -Name NevermindAide -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith("$prog\", [StringComparison]::OrdinalIgnoreCase) }) }
function Get-PlanActif { if ((powercfg /getactivescheme | Out-String) -match '([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})') { $Matches[1].ToLower() } }
function New-Raccourci([string]$Lien, [string]$Cible) {
    New-Item -ItemType Directory -Force -Path (Split-Path $Lien) | Out-Null
    $sh = New-Object -ComObject WScript.Shell
    $s = $sh.CreateShortcut($Lien); $s.TargetPath = $Cible; $s.Save()
}
$lienBureau = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Nevermind.lnk'
$lienMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'Mindset\Nevermind.lnk'

# Nevermind installé, géré par Mindset, son Aide en SYSTEM, puis « tout optimiser » : rend l'état d'origine relevé
function Install-Nevermind {
    Get-AidesEnCours | ForEach-Object { try { $_.Kill(); [void]$_.WaitForExit(5000) } catch {} }
    if (Test-Path -LiteralPath $base) { Remove-Item -LiteralPath $base -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $prog, $donnees | Out-Null
    Copy-Item -Path (Join-Path $Nouvelle '*') -Destination $prog -Recurse -Force
    New-Item -ItemType Directory -Force -Path (Split-Path $suiteFichier) | Out-Null
    [IO.File]::WriteAllText($suiteFichier, (ConvertTo-Json -InputObject @{ theme = 'neon'; applis = @{ nevermind = $prog } } -Compress), $utf8)
    # La tâche de l'Aide, comme Mindset la crée (en SYSTEM, -Sid de l'utilisateur)
    $act = New-ScheduledTaskAction -Execute "$prog\NevermindAide.exe" -Argument "-Sid $sid"
    $pr = New-ScheduledTaskPrincipal -UserId 'S-1-5-18' -LogonType ServiceAccount -RunLevel Highest
    Register-ScheduledTask -TaskPath '\Mindset\' -TaskName 'Nevermind (aide)' -Action $act -Principal $pr -Force | Out-Null
    Start-ScheduledTask -TaskPath '\Mindset\' -TaskName 'Nevermind (aide)'
    $null = Wait-Jusque { (Get-AidesEnCours).Count } 30

    # L'état d'origine (HwSchMode présent d'abord : l'Aide ne l'efface pas)
    if ($null -eq (Get-Valeur $cleGraphiques 'HwSchMode')) { New-ItemProperty -LiteralPath $cleGraphiques -Name HwSchMode -Value 1 -PropertyType DWord -Force | Out-Null }
    if (-not (Test-Path -LiteralPath $cleGameDvr)) { New-Item -Path $cleGameDvr -Force | Out-Null }
    if (-not (Test-Path -LiteralPath $cleGameBar)) { New-Item -Path $cleGameBar -Force | Out-Null }
    New-ItemProperty -LiteralPath $cleGameDvr -Name HistoricalCaptureEnabled -Value 1 -PropertyType DWord -Force | Out-Null
    Remove-ItemProperty -LiteralPath $cleGameBar -Name AutoGameModeEnabled -ErrorAction SilentlyContinue
    $svc = Get-Service DiagTrack
    $origine = @{ HwSchMode = [int](Get-Valeur $cleGraphiques 'HwSchMode'); Plan = (Get-PlanActif); Svc = "$($svc.StartType)"; SvcRunning = ($svc.Status -eq 'Running') }

    # Tout optimiser (ce que Nevermind change, noté comme il le note)
    New-ItemProperty -LiteralPath $cleGraphiques -Name HwSchMode -Value $(if ($origine.HwSchMode -eq 2) { 1 } else { 2 }) -PropertyType DWord -Force | Out-Null
    New-ItemProperty -LiteralPath $cleGameDvr -Name HistoricalCaptureEnabled -Value 0 -PropertyType DWord -Force | Out-Null
    New-ItemProperty -LiteralPath $cleGameBar -Name AutoGameModeEnabled -Value 1 -PropertyType DWord -Force | Out-Null
    Set-Service DiagTrack -StartupType Disabled; Stop-Service DiagTrack -Force -ErrorAction SilentlyContinue
    $cree = $null
    if ((powercfg -duplicatescheme 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c | Out-String) -match '([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})') { $cree = $Matches[1].ToLower(); powercfg /setactive $cree | Out-Null }
    $origine.Cree = $cree
    $sauvegarde = [ordered]@{
        Registry = [ordered]@{
            "$cleGraphiques|HwSchMode" = [ordered]@{ Path = $cleGraphiques; Name = 'HwSchMode'; Existed = $true; Value = $origine.HwSchMode; Kind = 'DWord' }
            "$cleGameDvr|HistoricalCaptureEnabled" = [ordered]@{ Path = $cleGameDvr; Name = 'HistoricalCaptureEnabled'; Existed = $true; Value = 1; Kind = 'DWord' }
            "$cleGameBar|AutoGameModeEnabled" = [ordered]@{ Path = $cleGameBar; Name = 'AutoGameModeEnabled'; Existed = $false; Value = $null; Kind = $null }
        }
        PowerScheme = $origine.Plan; CreatedScheme = $cree; Overlay = $null; Dns = @{}; Displays = @{}; Oublies = @()
    }
    [IO.File]::WriteAllText((Join-Path $donnees 'sauvegarde.json'), (ConvertTo-Json -InputObject $sauvegarde -Depth 6), $utf8Bom)
    [IO.File]::WriteAllText((Join-Path $donnees 'parametres.json'), (ConvertTo-Json -InputObject @{ SvcOriginal = @("DiagTrack|$($origine.Svc)|$($origine.SvcRunning)"); Theme = 'neon' } -Depth 4), $utf8)
    [IO.File]::WriteAllText((Join-Path $donnees 'historique.json'), '[]', $utf8)
    foreach ($f in 'jeux.json', 'fps.json', 'afficher.demande') { [IO.File]::WriteAllText((Join-Path $donnees $f), '{}', $utf8) }
    # Ses tâches, sa clé, ses raccourcis
    $tAct = New-ScheduledTaskAction -Execute "$prog\Mindset.exe" -Argument '-Demarrage'
    foreach ($t in 'Nevermind (démarrage)', 'OptiGame (démarrage)') { Register-ScheduledTask -TaskName $t -Action $tAct -Force | Out-Null }
    New-Item -Path 'HKLM:\SOFTWARE\Mindset\Nevermind\Aide\Dns' -Force | Out-Null
    New-ItemProperty -LiteralPath 'HKLM:\SOFTWARE\Mindset\Nevermind\Aide\Dns' -Name '{00000000-0000-0000-0000-000000000000}' -Value '' -PropertyType String -Force | Out-Null
    New-Raccourci $lienBureau "$prog\Mindset.exe"
    New-Raccourci $lienMenu "$prog\Mindset.exe"
    $origine
}

# Lance le désinstalleur avec ces arguments, ferme ses fenêtres (personne ici pour cliquer) une fois qu'elles se sont
# affichées, attend sa fin : @{ Code; Titres (les titres des fenêtres vues) }
function Invoke-Desinstalleur([string]$Arguments, [int]$Secondes = 300) {
    $exe = Join-Path $prog 'Désinstaller Nevermind.exe'
    $p = if ($Arguments) { Start-Process -FilePath $exe -ArgumentList $Arguments -WorkingDirectory $prog -PassThru } else { Start-Process -FilePath $exe -WorkingDirectory $prog -PassThru }
    $titres = @()
    $fin = (Get-Date).AddSeconds($Secondes)
    while (-not $p.HasExited -and (Get-Date) -lt $fin) {
        Start-Sleep -Milliseconds 500
        try {
            $p.Refresh()
            if ($p.MainWindowHandle -ne [IntPtr]::Zero) { $titres += $p.MainWindowTitle; Start-Sleep -Milliseconds 500; [void]$p.CloseMainWindow() }
        } catch {}
    }
    if (-not $p.HasExited) { try { $p.Kill() } catch {}; return @{ Code = -1; Titres = $titres } }
    @{ Code = $p.ExitCode; Titres = @($titres | Select-Object -Unique) }
}

try {
    # =================================================================================================================
    Write-Host '1 à 3. Tout optimiser, puis « Désinstaller Nevermind.exe -GarderDonnees »'
    $o = Install-Nevermind
    Test-Cas 'avant : l''Aide tourne en SYSTEM pour ce dossier' { (Get-AidesEnCours).Count -ge 1 }
    Test-Cas 'avant : les réglages sont changés' { (Get-Valeur $cleGraphiques 'HwSchMode') -ne $o.HwSchMode -and (Get-Valeur $cleGameDvr 'HistoricalCaptureEnabled') -eq 0 -and "$((Get-Service DiagTrack).StartType)" -eq 'Disabled' }
    $r = Invoke-Desinstalleur '-GarderDonnees'
    Test-Cas "il rend 0 (rendu : $($r.Code))" { $r.Code -eq 0 }
    Test-Cas "aucune fenêtre avec -GarderDonnees, ni question ni message de fin (fenêtres vues : $($r.Titres -join ', '))" { -not @($r.Titres).Count }
    Test-Cas 'HwSchMode remis comme avant (HKLM, par la voie « sur place » de l''Aide)' { (Get-Valeur $cleGraphiques 'HwSchMode') -eq $o.HwSchMode }
    Test-Cas 'Game Bar : la valeur changée est remise, celle qui n''existait pas est effacée (HKCU)' { (Get-Valeur $cleGameDvr 'HistoricalCaptureEnabled') -eq 1 -and $null -eq (Get-Valeur $cleGameBar 'AutoGameModeEnabled') }
    Test-Cas "service DiagTrack remis ($($o.Svc))" { "$((Get-Service DiagTrack).StartType)" -eq $o.Svc }
    Test-Cas 'plan d''alimentation d''origine actif, celui de Nevermind supprimé' { (Get-PlanActif) -eq $o.Plan -and (-not $o.Cree -or (powercfg /list | Out-String) -notmatch [regex]::Escape($o.Cree)) }
    Test-Cas 'tâches « Nevermind (démarrage) » et « OptiGame (démarrage) » retirées' { -not (Get-ScheduledTask -TaskName 'Nevermind (démarrage)' -ErrorAction SilentlyContinue) -and -not (Get-ScheduledTask -TaskName 'OptiGame (démarrage)' -ErrorAction SilentlyContinue) }
    Test-Cas 'tâche de l''Aide retirée, Aide arrêtée, sa clé HKLM effacée' { -not (Get-ScheduledTask -TaskPath '\Mindset\' -TaskName 'Nevermind (aide)' -ErrorAction SilentlyContinue) -and -not (Get-AidesEnCours).Count -and -not (Test-Path -LiteralPath 'HKLM:\SOFTWARE\Mindset\Nevermind\Aide') }
    Test-Cas 'raccourcis du bureau et du menu Démarrer retirés (et le dossier Mindset du menu, vide)' { -not (Test-Path -LiteralPath $lienBureau) -and -not (Test-Path -LiteralPath $lienMenu) -and -not (Test-Path -LiteralPath (Split-Path $lienMenu)) }
    Test-Cas 'Nevermind.exe a disparu (c''est ce que Mindset attend)' { -not (Test-Path -LiteralPath (Join-Path $prog 'Nevermind.exe')) }
    Test-Cas 'puis tout le dossier des programmes (10 s au plus)' { Wait-Jusque { -not (Test-Path -LiteralPath $prog) } 10 }
    Test-Cas 'données gardées : jeux.json, fps.json, parametres.json restent' { (Test-Path -LiteralPath (Join-Path $donnees 'jeux.json')) -and (Test-Path -LiteralPath (Join-Path $donnees 'fps.json')) -and (Test-Path -LiteralPath (Join-Path $donnees 'parametres.json')) }
    Test-Cas 'la sauvegarde, l''historique et les demandes partent' { -not (Test-Path -LiteralPath (Join-Path $donnees 'sauvegarde.json')) -and -not (Test-Path -LiteralPath (Join-Path $donnees 'historique.json')) -and -not (Test-Path -LiteralPath (Join-Path $donnees 'afficher.demande')) }
    Test-Cas 'le journal dit ce qui a été fait, et le message de fin' {
        $j = Get-Content -LiteralPath (Join-Path $donnees 'journal.txt') -Raw -Encoding UTF8
        $j -match 'Désinstallation demandée par Mindset \(données gardées\)' -and $j -match 'Désinstallation, message de fin : Nevermind est désinstallé\.'
    }
    Test-Cas 'les conseils pour le bilan de Mindset : UTF-8 sans BOM, 10 lignes de 300 caractères au plus, « Redémarre le PC » d''abord' { Test-Conseils }

    # =================================================================================================================
    Write-Host '4. « -SupprimerDonnees »'
    $o = Install-Nevermind
    $r = Invoke-Desinstalleur '-SupprimerDonnees'
    Test-Cas "il rend 0 (rendu : $($r.Code))" { $r.Code -eq 0 }
    Test-Cas 'tout le dossier des données est parti' { -not (Test-Path -LiteralPath $donnees) }
    Test-Cas 'les conseils pour le bilan de Mindset sont là quand même (dossier de la suite)' { Test-Conseils }
    Test-Cas 'les réglages sont remis comme avant' { (Get-Valeur $cleGraphiques 'HwSchMode') -eq $o.HwSchMode -and "$((Get-Service DiagTrack).StartType)" -eq $o.Svc }
    Test-Cas 'le dossier des programmes est parti' { Wait-Jusque { -not (Test-Path -LiteralPath $prog) } 10 }

    # =================================================================================================================
    Write-Host '5. Les deux options à la fois : la question, puis Annuler'
    $o = Install-Nevermind
    $r = Invoke-Desinstalleur '-GarderDonnees -SupprimerDonnees'
    Test-Cas "la question s'est affichée (fenêtres vues : $($r.Titres -join ', '))" { @($r.Titres | Where-Object { $_ -eq 'Désinstaller Nevermind' }).Count -ge 1 }
    Test-Cas "annulée : il rend 0 (rendu : $($r.Code)), rien n'est touché" { $r.Code -eq 0 -and (Test-Path -LiteralPath (Join-Path $prog 'Nevermind.exe')) -and (Test-Path -LiteralPath (Join-Path $donnees 'sauvegarde.json')) -and (Get-Valeur $cleGraphiques 'HwSchMode') -ne $o.HwSchMode }
    Test-Cas 'sans choix imposé : pas de fichier des conseils' { -not (Test-Path -LiteralPath $conseils) }
    # (remis comme avant pour la suite du job)
    $null = Invoke-Desinstalleur '-SupprimerDonnees'
}
catch {
    $script:Echecs += "Arrêt inattendu : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))"
    Write-Host "  ÉCHEC  Arrêt inattendu : $($_.Exception.Message)" -ForegroundColor Red
}

Write-Host ''
if ($script:Echecs.Count) { Write-Host "$($script:Echecs.Count) échec(s), $($script:Reussis) réussi(s)." -ForegroundColor Red; exit 1 }
Write-Host "Désinstalleur en Rust : les $($script:Reussis) contrôles sont bons." -ForegroundColor Green
exit 0
