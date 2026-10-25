# Nevermind : numéros des pages d'avant, navigation, et ce que l'accueil sait de Nevermind (l'écran lui-même est
# dessiné par la console : accueil-console.ps1).
# Chargé par Nevermind.ps1, qui définit $AppDir et $ModulesDir.

# ---------------------------------------------------------------------------
# Navigation : la page n° HubIndex est celle où l'écran de la console est montré
# ---------------------------------------------------------------------------
$HubIndex = 8
$GamesIndex = 9   # bibliothèque de jeux (bibliotheque.ps1)
$NetIndex = 10
$PageNames = @{ 0 = 'Tableau de bord'; 1 = 'Optimisation gaming'; 2 = 'Démarrage'; 3 = 'Connexion'; 4 = 'Nettoyage'; 5 = 'Tests'; 6 = 'Sécurité'; 7 = 'Sauvegarde' }
function Show-Page([int]$Index) { $ui.Tabs.SelectedIndex = $Index }

# Prénom pour le « Salut » : nom complet du compte Windows, sinon le nom d'utilisateur
function Get-FirstName {
    if ($script:FirstName) { return $script:FirstName }
    $n = ''
    try {
        $full = ([adsi]"WinNT://$env:COMPUTERNAME/$env:USERNAME,user").FullName
        if ($full) { $n = ([string]$full).Trim().Split(' ')[0] }
    } catch {}
    if (-not $n) { $n = [string]$env:USERNAME }
    if ($n) { $n = $n.Substring(0, 1).ToUpper() + $n.Substring(1) }
    $script:FirstName = $n
    $n
}

# Les points à régler : « importants » (un vrai problème, ou une correction que Nevermind fait en un clic) et ce qui se
# fait à la main (un pilote à installer...). Pour l'écran (tuile Ton PC) et le résumé de Mindset.
function Get-HubFindings($A) {
    $all = @(if ($A) { $A.Active | Where-Object { $_.Status -ne 'ok' -and $_.Gain -gt 0 } })
    $sort = @{ Expression = { $_.Gain }; Descending = $true }
    $important = @($all | Where-Object { $_.Status -eq 'bad' -or ($_.Fix -and $_.Fix.Auto) } |
        Sort-Object @{ Expression = { $_.Status -ne 'bad' } }, $sort)
    $manual = @($all | Where-Object { $important -notcontains $_ } | Sort-Object $sort)
    @{ Important = $important; Manual = $manual }
}

# Le dernier jeu lancé (encore installé), avec sa dernière partie mesurée
function Get-LastPlayedGame {
    if ($null -eq $script:Games) { return $null }
    $games = @(Get-LibraryGames)
    if (-not $games.Count) { return $null }
    $log = Get-PlayLog
    $best = $null; $bestDate = [datetime]::MinValue
    foreach ($g in $games) {
        $e = $log[$g.Name]
        if (-not $e -or -not $e.Last) { continue }
        try { $d = [datetime]$e.Last } catch { continue }
        if ($d -gt $bestDate) { $best = $g; $bestDate = $d }
    }
    if (-not $best) { return $null }
    $ss = @(Get-FpsSessions | Where-Object { (Get-SessionName $_) -eq $best.Name } | Sort-Object { [datetime]$_.Date })
    @{ Game = $best; Log = $log[$best.Name]; Last = $(if ($ss.Count) { $ss[-1] }); Prev = $(if ($ss.Count -gt 1) { $ss[-2] }) }
}

# Le petit texte d'un outil pour l'accueil de la console (Get-StatKit, accueil-console.ps1)
function Set-HubStat([int]$Index, [string]$Text, [string]$Color) {
    if ($script:Kit) { $script:Kit.Stats[$Index] = @{ T = $Text; C = $Color } }
}

# Copie gérée par Mindset : un petit résumé pour la carte de Nevermind dans l'accueil de Mindset (etat.json, dans le
# dossier de données). Que du texte et des nombres : Mindset l'affiche et n'exécute jamais rien qui en vient.
function Write-EtatMindset($A) {
    if (-not $Managed -or -not $A) { return }
    $fd = Get-HubFindings $A
    $top = if ($fd.Important.Count) { $fd.Important[0] } elseif ($fd.Manual.Count) { $fd.Manual[0] } else { $null }
    $etat = [ordered]@{
        Version = 1; Score = [int]$A.Score
        Protection = $(if ($null -ne $script:SecurityScore) { [int]$script:SecurityScore } else { $null })
        AFaire = $(if ($top) { [string]$top.Titre } else { '' }); Urgent = [bool]($top -and $top.Status -eq 'bad')
        NbAFaire = $fd.Important.Count + $fd.Manual.Count
    }
    $cle = $etat | ConvertTo-Json -Compress
    if ($cle -eq $script:EtatMindsetEcrit) { return }   # (rien de neuf : pas d'écriture)
    $etat.Maj = Get-Date -Format s
    try { [IO.File]::WriteAllText((Join-Path $DataDir 'etat.json'), ($etat | ConvertTo-Json -Compress), (New-Object Text.UTF8Encoding $false)); $script:EtatMindsetEcrit = $cle } catch {}
}

# L'accueil à jour : le résumé pour Mindset, le petit texte de la tuile Performances (tant que la liste des réglages
# n'est pas prête, Get-ReglagesKit), puis l'écran. Pas avant la fin de la préparation de l'accueil ($script:HubPret).
function Update-Hub {
    $a = $script:LastAnalysis
    Write-EtatMindset $a
    if (-not $script:HubPret) { return }
    if ($a) {
        $tw = @($a.Active | Where-Object { $_.Id -like 'tweak:*' -and $_.Status -eq 'warn' }).Count
        if ($tw) { Set-HubStat 1 "$tw réglage$(if ($tw -gt 1) {'s'}) à faire" $Colors.warn } else { Set-HubStat 1 'Tout est optimisé' '#00E5FF' }
    } else { Set-HubStat 1 'Analyse en cours...' }
    try { Update-EcranNevermind } catch { Write-Log "Accueil console : $_" }
}
