# Les juges du désinstalleur de Nevermind en Rust (rust/nevermind-desinstalleur/tests/juges) : « tout optimiser puis
# désinstaller » passé dans la version PowerShell (la référence), et ce qu'elle en sort.
#
#   & 'outils\exporter-juges-desinstalleur.ps1' [-Nombre 240] [-Graine 7112026] [-Debut 0] [-Fichier <autre.json>]
#   (-Debut, -Graine et -Fichier : d'autres cas, jamais vus, ailleurs que dans les juges : NEVERMIND_JUGES_DESINST=<fichier>
#   cargo test -p nevermind-desinstalleur les rejoue)
#
# Pour chaque cas :
#   1. un PC simulé (le faux PC de outils\exporter-juges-pc.ps1, repris tel quel : toutes ses fonctions sont lues dans ce
#      script, sans ses deux exports) est analysé, puis « Tout corriger » est fait dessus (sauvegarde.json, réglages
#      notés, registre changé) ; des DNS, des services, des règles du pare-feu, un problème de sauvegarde, des changements
#      définitifs dans l'historique sont ajoutés selon le cas ;
#   2. Invoke-Uninstall (systeme.ps1, lu dans le module, pas recopié) est lancé sur un dossier d'essai (programmes et
#      données dans %TEMP%) : avec -GarderDonnees, -SupprimerDonnees, les deux, ou rien (la fenêtre répond garder,
#      supprimer ou annuler) ;
#   3. tout est noté : la suite exacte des gestes (demandes à l'Aide, registre de l'utilisateur, powercfg, écrans,
#      curseur, tâches planifiées, programmes fermés, clé de l'Aide, raccourcis, la commande qui supprime les fichiers),
#      la question, le message de fin, le journal, ce qui reste dans le dossier des données, et les réglages notés.
# Rien ne touche le vrai PC : tout ce qui agit est remplacé par une fausse version qui note le geste (les fichiers d'essai,
# eux, sont vraiment créés et supprimés dans %TEMP%). Aucune fenêtre ne s'ouvre. Les chemins du dossier d'essai sont
# écrits {APPLI} (le dossier des programmes) et {DONNEES} (le dossier des données) dans les juges.
param([int]$Nombre = 240, [int]$Graine = 7112026, [int]$Debut = 0, [string]$Fichier = "")

$ErrorActionPreference = 'Stop'
$env:NEVERMIND_TEST = '1'
$depot = Split-Path $PSScriptRoot -Parent
$sortieJuges = Join-Path $depot 'rust\nevermind-desinstalleur\tests\juges'
New-Item -ItemType Directory -Force -Path $sortieJuges | Out-Null

# ---------------------------------------------------------------------------
# 1. Le faux PC de exporter-juges-pc.ps1 : toutes ses instructions jusqu'à son premier export (« $cas = ... »)
# ---------------------------------------------------------------------------
$tok = $null; $err = $null
$astPc = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot 'exporter-juges-pc.ps1'), [ref]$tok, [ref]$err)
if ($err) { throw "exporter-juges-pc.ps1 n'est pas lisible : $($err[0].Message)" }
$instructions = @($astPc.EndBlock.Statements)
$finFaux = -1
for ($i = 0; $i -lt $instructions.Count; $i++) { if ($instructions[$i].Extent.Text -match '^\$cas = New-Object') { $finFaux = $i; break } }
if ($finFaux -lt 10) { throw 'exporter-juges-pc.ps1 : la fin de son faux PC est introuvable' }
$NombreCorrections = 0; $Nombre0 = $Nombre
# ($PSScriptRoot est vide dans un bloc créé ainsi : le dossier outils y est écrit en clair)
$textePc = (($instructions[0..($finFaux - 1)] | ForEach-Object { $_.Extent.Text }) -join "`n").Replace('$PSScriptRoot', "'$PSScriptRoot'")
. ([scriptblock]::Create($textePc))
$Nombre = $Nombre0
$script:Rnd = New-Object System.Random $Graine

# ---------------------------------------------------------------------------
# 2. La désinstallation, lue dans les modules
# ---------------------------------------------------------------------------
foreach ($n in 'Get-AppRoot', 'Get-AppExes', 'Get-ChoixDonneesImpose', 'Remove-DonneesDesinstallation', 'Invoke-Uninstall') {
    $def = Get-FonctionAst 'systeme' $n
    if (-not $def) { throw "Fonction $n introuvable dans systeme.ps1" }
    if ($def.Extent.Text -match '\[Microsoft\.Win32\.Registry\]') { throw "Juges : $n écrit le vrai registre" }
    . ([scriptblock]::Create($def.Extent.Text))
}
. ([scriptblock]::Create((Get-FonctionAst 'donnees' 'Get-TexteProblemeSauvegarde').Extent.Text))
$OldAutoStartTasks = @('OptiGame (démarrage)', 'Nexo (démarrage)')

# Le dossier d'essai : les programmes et les données, refaits à chaque cas
$script:Bac = [IO.Path]::GetFullPath((Join-Path $env:TEMP "nevermind-juges-desinst-$([guid]::NewGuid().ToString('N').Substring(0, 8))"))
$script:Racine = Join-Path $script:Bac 'appli\Nevermind'
$AppDir = Join-Path $script:Racine 'fichiers'
$DataDir = Join-Path $script:Bac 'donnees'

# Les fausses versions de ce que la désinstallation touche (une fonction passe avant une commande du même nom)
$script:Journal = New-Object System.Collections.Generic.List[string]
$script:Messages = New-Object System.Collections.Generic.List[string]
function Write-Log([string]$Message) { $script:Journal.Add($Message) }
function Test-Path {
    param([Parameter(Position = 0)][string]$Path, [string]$LiteralPath, $PathType)
    $p = if ($LiteralPath) { $LiteralPath } else { $Path }
    if ($p -like '*hiberfil.sys') { return [bool]$script:Hiberfil }
    if ($p.StartsWith($script:Bac, [StringComparison]::OrdinalIgnoreCase)) { return [IO.File]::Exists($p) -or [IO.Directory]::Exists($p) }
    $false
}
function Remove-Item {
    param([string[]]$LiteralPath, [switch]$Recurse, [switch]$Force, $ErrorAction)
    if ($LiteralPath.Count -eq 1 -and $LiteralPath[0] -like 'HKLM:*') { Add-Geste 'registre.effacer.cle' $LiteralPath[0]; return }
    if ($Recurse) {
        # (« Supprimer » : le dossier des données d'essai, vraiment supprimé)
        if (-not $LiteralPath[0].StartsWith($script:Bac, [StringComparison]::OrdinalIgnoreCase)) { throw "Juges : Remove-Item -Recurse hors du dossier d'essai ($($LiteralPath[0]))" }
        [IO.Directory]::Delete($LiteralPath[0], $true)
        return
    }
    # (« Tout restaurer » : la sauvegarde effacée, notée comme dans exporter-juges-pc.ps1)
    Add-Geste 'fichier.effacer' @($LiteralPath | ForEach-Object { [IO.Path]::GetFileName($_) })
}
function Stop-ScheduledTask { param($TaskPath, $TaskName, $ErrorAction) Add-Geste 'tache.arreter' ([ordered]@{ TaskName = $TaskName; TaskPath = $TaskPath }) }
function Get-Process { param($ErrorAction) $script:FauxProcessus }
function Set-AutoStart([bool]$On) { Add-Geste 'demarrage.retirer' $null; if ($script:DemarrageRefuse) { throw 'Accès refusé.' } }
function Test-AutoStart { [bool]$script:AutoStart }
function Test-DesktopShortcut { [bool]$script:RaccourciBureau }
function Get-ScheduledTask { param($TaskName, $TaskPath, $ErrorAction) if ($script:AnciennesTaches -contains $TaskName) { [pscustomobject]@{ TaskName = $TaskName } } }
function script:Unregister-ScheduledTask { param($TaskName, $TaskPath, $Confirm, $ErrorAction) Add-Geste 'tache.supprimer' ([ordered]@{ TaskName = $TaskName; TaskPath = $TaskPath }) }
function Get-DesktopShortcutPath { Add-Geste 'raccourci.bureau.effacer' $null; Join-Path $script:Bac 'bureau\Nevermind.lnk' }
function Remove-MenuShortcut { Add-Geste 'raccourci.menu.retirer' $null }
function Show-ChoixDesinstallation([string]$Texte) { $script:Question = $Texte; $script:ReponseFenetre }
function Show-MessageFinDesinstallation([string]$Texte) { $script:Messages.Add($Texte) }
function Start-Process { param($FilePath, $ArgumentList, $WindowStyle, $WorkingDirectory) Add-Geste 'suppression' ([string]@($ArgumentList)[1]) }
function Get-NetFirewallRule { param($DisplayName, $ErrorAction) if ($DisplayName -like 'Nevermind*' -and $script:ReglesPareFeu -gt 0) { @(1..$script:ReglesPareFeu) } else { @() } }
function Get-NetAdapter { param([switch]$IncludeHidden, $ErrorAction) foreach ($c in @($script:Cartes)) { [pscustomobject]@{ InterfaceGuid = "{$($c.Guid)}"; ifIndex = $c.Index } } }
function Import-History { $script:History = @($script:FauxHistorique) }

# ---------------------------------------------------------------------------
# 3. Les cas
# ---------------------------------------------------------------------------
$FichiersAppli = @('Mindset.exe', 'Nevermind.exe', 'NevermindAide.exe', 'NevermindPont.dll', 'OptiGame.exe', 'Désinstaller OptiGame.exe', 'Nexo.exe',
    'Désinstaller Nexo.exe', 'LISEZMOI.txt', 'fichiers\Nevermind.ps1', 'fichiers\OptiGame.ps1', 'fichiers\NevermindAide.ps1', 'fichiers\Nevermind.ico',
    'fichiers\OptiGame.ico', 'fichiers\cle-publique.xml', 'fichiers\Lancer Nevermind (secours).bat', 'fichiers\Désinstaller Nevermind (secours).bat',
    'fichiers\Lancer OptiGame (secours).bat', 'fichiers\Lancer Nexo (secours).bat', 'fichiers\Désinstaller Nexo (secours).bat',
    'fichiers\Désinstaller OptiGame (secours).bat', 'fichiers\outils-tiers\PresentMon.exe', 'fichiers\modules\systeme.ps1', 'aide-temp\x.dll', 'autre-chose.txt')
$FichiersDonnees = @('parametres.json', 'ignores.json', 'jeux.json', 'fps.json', 'lag.json', 'organizer.json', 'appareils.json', 'journal.txt', 'journal-ancien.txt',
    'jaquettes\jeu.png', 'packs\theme\pack.json', 'sauvegarde.json', 'sauvegarde.json.bak', 'sauvegarde.json.tmp', 'sauvegarde-abimee-20260101-101010.json',
    'sauvegarde-abimee-x.json.old', 'afficher.demande', 'quitter.demande', 'historique.json', 'historique.json.bak', 'historique.json.tmp', 'Historique.JSON.old',
    'nettoyage\nettoyage-2026-11-01.log', 'Données accentuées é.txt')
$Pas = @('aide:service.regler', 'aide:reg.ecrire', 'aide:dns.regler', 'aide:origines.oublier', 'aide:pare-feu.debloquer-tout')

function New-Fichiers([string]$Dossier, [string[]]$Noms) {
    foreach ($f in $Noms) {
        $p = Join-Path $Dossier $f
        New-Item -ItemType Directory -Force -Path (Split-Path $p) | Out-Null
        [IO.File]::WriteAllText($p, '{}')
    }
}
function Get-Fichiers([string]$Dossier) {
    if (-not [IO.Directory]::Exists($Dossier)) { return $null }
    , @(Get-ChildItem -LiteralPath $Dossier -Recurse -File -Force | ForEach-Object { $_.FullName.Substring($Dossier.Length + 1) } | Sort-Object)
}
function Get-Paires($Table) { , @(foreach ($k in @($Table.Keys)) { , @([string]$k, (Get-Copie $Table[$k])) }) }

$cas = New-Object System.Collections.ArrayList
for ($n = $Debut; $n -lt $Debut + $Nombre; $n++) {
    # a. Tout optimiser : « Tout corriger » sur un PC simulé à régler (comme la partie 2 de exporter-juges-pc.ps1)
    $sc = New-PcSimule (9000 + $n)
    foreach ($k in @($sc.registre.Keys)) { if ($k -like '*GameDVR*') { $sc.registre[$k].Value = 1 } }
    if ($n % 2) { $sc.ecrans = @('\\.\DISPLAY1|Écran générique|2560|1440|60|165|1') + @($sc.ecrans | Select-Object -Skip 1) }
    if ($n % 3 -eq 0) { $sc.alimBatterie = [ordered]@{ CritAction = @(0, 1); CritLevel = @(5, 5); LowLevel = @(10, 10); LowNotify = @(1, 0); LowAction = @(0, 1) }; $sc.data.Battery = @([ordered]@{ Name = 'Back-UPS BX700U FW:'; DeviceID = 'UPS1'; Description = ''; Chemistry = 3; BatteryStatus = 2; EstimatedChargeRemaining = 100; EstimatedRunTime = 12 }); $sc.data.Chassis = @(3); $sc.data.PCType = 1 }
    Use-PcSimule $sc
    $script:ChoixEcran = @('keep', 'keep', 'revert')[$n % 3]
    Invoke-Analysis
    $script:ui.Overlay.Visibility = 'Collapsed'
    Open-FixAll
    if ($script:ui.Overlay.Visibility -eq 'Visible') { Invoke-Safe { Invoke-SheetRun } }
    # Ce que d'autres écrans changent aussi : DNS (réseau), services (télémétrie), règles du pare-feu (Trafic)
    $script:Cartes = @()
    if ($n % 4 -eq 1) {
        $script:Backup.Dns['{6b29fc40-ca47-1067-b31d-00dd010662da}'] = '1.1.1.1,1.0.0.1'
        $script:Backup.Dns['12'] = '8.8.8.8 8.8.4.4'
        if ($n % 8 -ne 5) { $script:Cartes = @(@{ Guid = '6B29FC40-CA47-1067-B31D-00DD010662DA'; Index = 7 }) }
    }
    if ($n % 3 -eq 2) { $script:Settings['SvcOriginal'] = @('DiagTrack|Automatic|True', 'dmwappushservice|Manual|False') }
    $script:ReglesPareFeu = if ($n % 5 -eq 0) { 2 } else { 0 }
    $refuse = @(if ($n % 6 -eq 3) { ($Pas[[math]::Floor($n / 6) % $Pas.Count]) -replace '^aide:', '' })
    $script:BackupProbleme = switch ($n % 7) { 1 { 'abimee' } 4 { 'absente' } default { $null } }
    $script:FauxHistorique = @(if ($n % 3 -eq 0) {
        [ordered]@{ Id = 'h1'; Annule = $true; Log = @([ordered]@{ Type = 'definitif'; Quoi = 'Bureau à distance coupé'; Retour = 'page Protection' }, [ordered]@{ Type = 'reg'; Path = 'HKCU:\X'; Name = 'y' }) }
        [ordered]@{ Id = 'h2'; Annule = $false; Log = @([ordered]@{ Type = 'definitif'; Quoi = 'Bureau à distance coupé'; Retour = 'page Protection' }, [ordered]@{ Type = 'definitif'; Quoi = 'bureau à distance coupé'; Retour = 'page Protection' }) }
        $null
    } elseif ($n % 3 -eq 1) { [ordered]@{ Id = 'h3'; Annule = $false; Log = [ordered]@{ Type = 'definitif'; Quoi = 'SMB1 coupé'; Retour = 'Fonctionnalités de Windows' } } })
    $exeAide = Join-Path $script:Racine 'NevermindAide.exe'
    $procs = @(@{ Id = 4; Path = '' }, @{ Id = 77; Path = 'C:\Autre\NevermindAide.exe' })
    if ($n % 2 -eq 0) { $procs += @{ Id = 4242; Path = $exeAide } }
    if ($n % 9 -eq 4) { $procs += @{ Id = 4243; Path = (Join-Path $script:Racine 'NEVERMINDAIDE.EXE') } }   # (sans tenir compte des majuscules, comme -eq)
    $script:FauxProcessus = @(foreach ($p in $procs) {
        $o = [pscustomobject]@{ Id = $p.Id; Path = $p.Path }
        $o | Add-Member -MemberType ScriptMethod -Name Kill -Value { Add-Geste 'processus.fermer' $this.Id }
        $o | Add-Member -MemberType ScriptMethod -Name WaitForExit -Value { param($ms) $true }
        $o
    })
    $script:AnciennesTaches = @(switch ($n % 4) { 0 { } 1 { 'OptiGame (démarrage)' } 2 { 'Nexo (démarrage)' } 3 { 'OptiGame (démarrage)', 'Nexo (démarrage)' } })
    $script:AutoStart = ($n % 5) -in 1, 2
    $script:RaccourciBureau = ($n % 7) -in 2, 3
    $script:DemarrageRefuse = ($n % 11 -eq 3)
    $options = @(switch ($n % 6) { 0 { } 1 { '-GarderDonnees' } 2 { '-SupprimerDonnees' } 3 { '-GarderDonnees', '-SupprimerDonnees' } 4 { '/garderdonnees' } 5 { } })
    $script:ReponseFenetre = @('garder', 'supprimer', $null)[[math]::Floor($n / 6) % 3]
    # (le choix venu de la ligne de commande, comme le lanceur le transmet : outils\lanceur.cs:156-166)
    $donnees = $null
    foreach ($a in $options) {
        $nom = $a.Trim('-', '/')
        if ($nom -eq 'GarderDonnees') { $donnees = if ($null -eq $donnees -or $donnees -eq 'garder') { 'garder' } else { '' } }
        elseif ($nom -eq 'SupprimerDonnees') { $donnees = if ($null -eq $donnees -or $donnees -eq 'supprimer') { 'supprimer' } else { '' } }
    }
    [Environment]::SetEnvironmentVariable('NEVERMIND_DONNEES', $(if ($donnees) { $donnees } else { $null }))

    # b. Le dossier d'essai : des programmes (pas tous), des données (pas toutes, parfois aucune)
    if ([IO.Directory]::Exists($script:Bac)) { [IO.Directory]::Delete($script:Bac, $true) }
    $appli = @($FichiersAppli | Where-Object { $script:Rnd.NextDouble() -lt 0.6 }) + 'Désinstaller Nevermind.exe'
    New-Fichiers $script:Racine $appli
    New-Item -ItemType Directory -Force -Path $AppDir | Out-Null
    $fDonnees = if ($n % 13 -eq 5) { $null } else { @($FichiersDonnees | Where-Object { $script:Rnd.NextDouble() -lt 0.55 }) }
    if ($null -ne $fDonnees) { New-Item -ItemType Directory -Force -Path $DataDir | Out-Null; New-Fichiers $DataDir $fDonnees }

    $entree = [ordered]@{
        numero = $n
        sauvegarde = [ordered]@{ Registry = (Get-Paires $script:Backup.Registry); Dns = (Get-Paires $script:Backup.Dns); Displays = (Get-Paires $script:Backup.Displays)
            PowerScheme = $script:Backup.PowerScheme; CreatedScheme = $script:Backup.CreatedScheme; Overlay = $script:Backup.Overlay; Oublies = @($script:Backup.Oublies) }
        probleme = $script:BackupProbleme
        etat = (Get-EtatFaux)
        plans = @($script:Plans | ForEach-Object { [ordered]@{ Guid = $_.Guid; Nom = $_.Nom } })
        historique = @($script:FauxHistorique)
        cartes = @($script:Cartes | ForEach-Object { [ordered]@{ Guid = $_.Guid; Index = $_.Index } })
        reglesPareFeu = $script:ReglesPareFeu
        refuse = @($refuse)
        processus = @($procs | ForEach-Object { [ordered]@{ Id = $_.Id; Path = $_.Path } })
        anciennesTaches = @($script:AnciennesTaches)
        autoStart = $script:AutoStart; raccourciBureau = $script:RaccourciBureau; demarrageRefuse = $script:DemarrageRefuse
        options = @($options); reponse = $script:ReponseFenetre
        fichiersAppli = @($appli); fichiersDonnees = $fDonnees
    }

    # c. La désinstallation
    [void](Get-Gestes)
    $script:AideRefuse = @($refuse)
    $script:Journal.Clear(); $script:Messages.Clear(); $script:Question = $null; $script:History = $null
    Invoke-Uninstall
    [Environment]::SetEnvironmentVariable('NEVERMIND_DONNEES', $null)
    $sortie = [ordered]@{
        gestes = (Get-Gestes)
        question = $script:Question
        messages = @($script:Messages)
        journal = @($script:Journal)
        donnees = (Get-Fichiers $DataDir)
        reglages = (Get-Copie $script:Settings)
        registre = (Get-Copie $script:FauxReg)
    }
    [void]$cas.Add([ordered]@{ entree = $entree; sortie = $sortie })
}
if ([IO.Directory]::Exists($script:Bac)) { [IO.Directory]::Delete($script:Bac, $true) }

$json = ConvertTo-Json -InputObject @($cas) -Depth 40 -Compress
# Les chemins du dossier d'essai (écrits « \\ » dans le JSON) : {APPLI} et {DONNEES}
$json = $json.Replace($script:Racine.Replace('\', '\\'), '{APPLI}').Replace($DataDir.Replace('\', '\\'), '{DONNEES}')
if ($json.IndexOf($script:Bac.Replace('\', '\\'), [StringComparison]::OrdinalIgnoreCase) -ge 0) { throw 'Juges : un chemin du dossier d''essai est resté dans les juges' }
$cible = if ($Fichier) { $Fichier } else { Join-Path $sortieJuges 'desinstallation.json' }
[IO.File]::WriteAllText($cible, $json, (New-Object Text.UTF8Encoding($false)))
$questions = @($cas | Where-Object { $_.sortie.question }).Count
$annules = @($cas | Where-Object { -not $_.sortie.messages.Count }).Count
Write-Host "desinstallation.json : $($cas.Count) cas ($questions avec la question, $annules annulés), $([math]::Round($json.Length / 1KB)) Ko"
