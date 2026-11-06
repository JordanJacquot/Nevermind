#Requires -Version 5.1
# Juges de l'hôte de Nevermind en Rust (rust\nevermind-appli) : l'état que la version PowerShell envoie au kit
# (Get-EcranNevermind d'accueil-console.ps1, puis ce que Send-Vue de vue.ps1 y ajoute) pour des blocs factices.
#
# Pour chaque cas : des faits inventés (analyse, protection, jeux, réseau, performances), des pages au hasard données
# par les blocs factices, Nevermind affiché dans Mindset ou non, l'écran prêt ou non, le thème (dont des packs faits
# dans un dossier d'essai), le prénom et la connexion. Les vrais modules sont chargés en mode test
# (NEVERMIND_TEST) ; seules les fonctions des blocs sont remplacées. Rien ne touche le PC : aucune fenêtre, aucune
# lecture de Windows, le dossier d'essai est sous %TEMP% et effacé à la fin.
#
#   & '.\outils\exporter-juges-hote.ps1' [-Cas 300] [-Graine 7]
#
# Sortie : rust\nevermind-appli\tests\juges\hote-accueil.json ({ cas: [{ entree, sortie }] }).
param([int]$Cas = 300, [int]$Graine = 7, [string]$Sortie)
$ErrorActionPreference = 'Stop'
$racine = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
if (-not $Sortie) { $Sortie = Join-Path $racine 'rust\nevermind-appli\tests\juges\hote-accueil.json' }   # (-Sortie : un juge d'ailleurs)
$env:NEVERMIND_TEST = '1'
$essai = Join-Path $env:TEMP "nevermind-juges-hote-$PID"
$packs = Join-Path $essai 'packs'
New-Item -ItemType Directory -Force -Path $packs | Out-Null
$env:NEVERMIND_PACKS = $packs

# Ce que les modules attendent d'avant eux (Nevermind.ps1, donnees.ps1)
$AppDir = Join-Path $racine 'Nevermind\fichiers'
$DataDir = Join-Path $essai 'donnees'
$Managed = $false
$Suite = @{ Theme = $null; Dir = '' }
$AppVersion = '1.15.6'
$script:Reglages = @{}
function Write-Log([string]$t) {}
function Get-Setting([string]$Nom, $Defaut) { if ($script:Reglages.ContainsKey($Nom)) { $script:Reglages[$Nom] } else { $Defaut } }
function Set-Setting([string]$Nom, $Valeur) { $script:Reglages[$Nom] = $Valeur }
$GamesIndex = 9; $NetIndex = 10; $TrafficIndex = 11; $OverlayIndex = 12

# Les packs d'essai : un fichier PNG de quelques octets pour chaque image
$png = [byte[]](0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0)
$lesPacks = [ordered]@{
    'essai-simple' = @{ Fichiers = @('logo.png', 'fond.png', 'bas.png', 'police.ttf'); Json = [ordered]@{ Name = 'Simple'; Base = 'rubis'; Hello = 'Coucou {0} !'
        Logo = 'logo.png'; LogoFrames = 3; LogoDelay = 80; LogoGlow = '#FF00FF'; Background = 'fond.png'; BackgroundOpacity = 0.5; BackgroundAlign = 'haut-gauche'
        Footer = @{ File = 'bas.png'; Frames = 4; Delay = 90; Height = 30 }; Font = 'police.ttf'; FontScope = 'tout' } }
    'essai-console' = @{ Fichiers = @('t.png', 'm.png', 'c.png', 'f.png', 'l.png'); Json = [ordered]@{ Name = 'Console'; Base = 'terminal'
        Console = [ordered]@{ Tuiles = [ordered]@{ pc = 't.png'; jeux = 't.png'; mauvais = 'absent.png' }; Mascotte = 'm.png'; Chargement = @{ File = 'c.png'; Effet = 'aucun' }; Fond = 'f.png'; FondOpacite = 0.8; Logo = 'l.png' } } }
    'essai-couleurs' = @{ Fichiers = @('logo.png'); Json = [ordered]@{ Name = 'Couleurs'; Hello = 'Mal {1} écrite'
        Colors = [ordered]@{ P = '#40E0D0'; S = '#FFB000'; T = '#FF5577'; NH = 200; NS = 0.5 }; Logo = 'logo.png' } }
}
foreach ($id in $lesPacks.Keys) {
    $d = Join-Path $packs $id; New-Item -ItemType Directory -Force -Path $d | Out-Null
    foreach ($f in $lesPacks[$id].Fichiers) { [IO.File]::WriteAllBytes((Join-Path $d $f), $png) }
    [IO.File]::WriteAllText((Join-Path $d 'pack.json'), ($lesPacks[$id].Json | ConvertTo-Json -Depth 6), (New-Object Text.UTF8Encoding $false))
}

$hasard = New-Object System.Random $Graine
function Au-Hasard($Liste) { $Liste[$hasard.Next($Liste.Count)] }
function Oui([double]$p = 0.5) { $hasard.NextDouble() -lt $p }
$mots = @('Réglages', 'été', 'Ça marche', 'Prêt à jouer', 'à régler', '« guillemets »', ('Long ' * 12), '', 'Point', 'Œil', 'x')

# Un morceau de page au hasard (des objets, des listes, des objets vides, des accents)
function New-Valeur([int]$Prof) {
    $r = $hasard.Next(10)
    if ($Prof -ge 3 -or $r -lt 3) { return (Au-Hasard @((Au-Hasard $mots), $hasard.Next(-5, 200), (Oui), $null, 0.25, 'fichier-pas')) }
    if ($r -lt 5) { return @{} }
    if ($r -lt 7) { return , @(for ($i = 0; $i -lt $hasard.Next(0, 4); $i++) { New-Valeur ($Prof + 1) }) }
    $o = [ordered]@{}
    for ($i = 0; $i -lt $hasard.Next(1, 5); $i++) { $o[(Au-Hasard @('titre', 'sous', 'blocs', 'module', 'texte', 'valeur', 'elements', 'id'))] = New-Valeur ($Prof + 1) }
    $o
}
function New-Page {
    $p = [ordered]@{ titre = (Au-Hasard $mots); icone = (Au-Hasard @('jauge', 'radar', 'bouclier')); couleur = (Au-Hasard @('P', 'S', 'T')) }
    $p.blocs = @(for ($i = 0; $i -lt $hasard.Next(0, 5); $i++) {
        if (Oui 0.25) { [ordered]@{ type = 'mesures'; elements = @([ordered]@{ id = "m$i"; nom = 'CPU'; valeur = $hasard.Next(100); texte = "$($hasard.Next(100)) %"; ton = (Au-Hasard @('', 'ok', 'urgent')) }) } }
        else { [ordered]@{ type = (Au-Hasard @('ligne', 'texte', 'titre')); texte = (Au-Hasard $mots); valeur = (New-Valeur 1) } }
    })
    if (Oui 0.3) { $p.module = New-Valeur 1 }
    $p
}

$lesCas = New-Object System.Collections.ArrayList
$themesEssai = @('neon', 'crepuscule', 'terminal', 'arcade', 'rubis', 'retro', 'dresseur', 'inconnu', 'pack:essai-simple', 'pack:essai-console', 'pack:essai-couleurs', 'pack:absent')
for ($n = 0; $n -lt $Cas; $n++) {
    $themeCas = if ($n -lt $themesEssai.Count) { $themesEssai[$n] } else { Au-Hasard $themesEssai }
    $env:NEVERMIND_THEME = $themeCas
    . (Join-Path $modules 'themes.ps1')
    . (Join-Path $modules 'vue.ps1')
    . (Join-Path $modules 'navigation.ps1')
    . (Join-Path $modules 'accueil-console.ps1')

    # Les faits des blocs, en PowerShell et pour le Rust
    $faits = [ordered]@{}
    $script:LastAnalysis = $null
    if (Oui 0.75) {
        $actifs = @(for ($i = 0; $i -lt $hasard.Next(0, 6); $i++) {
            [pscustomobject]@{ Status = (Au-Hasard @('ok', 'warn', 'bad')); Gain = (Au-Hasard @(0, 1, 3, 8)); Fix = $(if (Oui) { @{ Auto = (Oui) } }); Titre = "Point $i"; Id = "x:$i" }
        })
        $score = $hasard.Next(0, 101)
        $script:LastAnalysis = [pscustomobject]@{ Score = $score; Active = $actifs }
        $faits.pc = [ordered]@{ score = $score; a_regler = @($actifs | Where-Object { $_.Status -ne 'ok' -and $_.Gain -gt 0 }).Count }
    }
    $script:SecurityScore = $null
    if (Oui 0.6) { $script:SecurityScore = $hasard.Next(0, 101); $faits.protection = [ordered]@{ score = $script:SecurityScore } }
    $script:Games = $null; $script:JeuxListe = @(); $script:Dernier = $null; $script:Image = $null
    if (Oui 0.7) {
        $script:Games = @{}
        $script:JeuxListe = @(1..$hasard.Next(0, 4) | ForEach-Object { "Jeu $_" })
        if (Oui 0.25) { $script:JeuxListe = @(1..1234 | ForEach-Object { $_ }) }
        $faits.jeux = [ordered]@{ nombre = $script:JeuxListe.Count }
        if (Oui 0.6) {
            $fps = Au-Hasard @(0, 59.5, 143.6, 60.49, 1234.5, 2.5)
            $script:Dernier = @{ Game = @{ Name = (Au-Hasard @('Valorant', 'Battlefield 6', 'Été « jeu »')) }; Last = @{ Avg = $fps } }
            $faits.jeux.dernier = $script:Dernier.Game.Name
            $faits.jeux.fps = $fps
            if (Oui) { $script:Image = "C:\Jeux\$n\hero.jpg"; $faits.jeux.image = $script:Image }
        }
    }
    $script:NetList = @(); $script:NetScanning = $false
    if (Oui 0.6) {
        $script:NetList = @(1..$hasard.Next(0, 4) | ForEach-Object { "a$_" })
        if (-not $script:NetList.Count -and (Oui)) { $script:NetList = @() }
        $script:NetScanning = Oui
        $faits.reseau = [ordered]@{ appareils = @($script:NetList | Where-Object { $_ }).Count; scan = $script:NetScanning }
    }
    $script:SousPerf = $null
    if (Oui 0.5) { $script:SousPerf = Au-Hasard @('3 réglages à faire', 'Tout est optimisé', '1 réglage à faire'); $faits.perf = [ordered]@{ sous = $script:SousPerf } }

    # Les pages des blocs factices (de 1 à 3 blocs)
    $blocs = @(for ($b = 0; $b -lt $hasard.Next(1, 4); $b++) {
        $p = [ordered]@{}
        for ($i = 0; $i -lt $hasard.Next(0, 3); $i++) { $p[(Au-Hasard @('pc', 'perf', 'jeux', 'reseau', 'trafic', 'aide', "page$b$i"))] = New-Page }
        $p
    })
    $script:PagesBlocs = $blocs

    function Get-PagesPc { $r = [ordered]@{}; foreach ($b in $script:PagesBlocs) { foreach ($k in $b.Keys) { $r[$k] = $b[$k] } }; $r }
    foreach ($f in 'Get-PagesPerf', 'Get-PagesProtection', 'Get-PagesJeux', 'Get-PagesReseau', 'Get-PagesDivers') { Set-Item "function:$f" { [ordered]@{} } }
    function Get-LibraryGames { $script:JeuxListe }
    function Get-DernierJeuKit { $script:Dernier }
    function Get-HeroFile($Game) { $script:Image }
    function Get-CoverFile($Game) { $null }
    function Get-ReglagesKit { @{ Lignes = @(if ($script:SousPerf) { 1 }) } }
    function Get-TitreReglagesKit($R) { $script:SousPerf }

    $script:Embedded = Oui 0.4
    $script:Vue.Pret = Oui 0.7
    $script:Kit.Focus = $null
    $script:Kit.RetourEcran = $(if (Oui 0.5) { Au-Hasard @('jeux', 'parametres', 'pc') })
    $script:FirstName = Au-Hasard @('Jordan', 'Élodie', 'Léa', 'X')
    $connexion = Au-Hasard @('cable', 'wifi', 'aucune')
    $script:ConnexionVue = @{ Valeur = $connexion; Lue = Get-Date }

    $e = Get-EcranNevermind
    Send-Vue $e
    $texte = ($script:Vue.Etat | ConvertTo-Json -Depth 16 -Compress) -replace '":\{\}', '":null'
    $entree = [ordered]@{
        theme = $themeCas; prenom = $script:FirstName; connexion = $connexion; accrochee = [bool]$script:Embedded; pret = [bool]$script:Vue.Pret
        retour = $script:Kit.RetourEcran; faits = $faits; blocs = @($blocs)
    }
    [void]$lesCas.Add("{`"entree`":$((($entree | ConvertTo-Json -Depth 20 -Compress)) -replace '":\{\}', '":{}'),`"sortie`":$texte}")
}
$packsJson = [ordered]@{}; foreach ($id in $lesPacks.Keys) { $packsJson[$id] = $lesPacks[$id] }
$tout = "{`"packs`":$($packsJson | ConvertTo-Json -Depth 8 -Compress),`"cas`":[$($lesCas -join ',')]}"
New-Item -ItemType Directory -Force -Path (Split-Path $sortie) | Out-Null
[IO.File]::WriteAllText($sortie, $tout, (New-Object Text.UTF8Encoding $false))
Remove-Item -LiteralPath $essai -Recurse -Force -ErrorAction SilentlyContinue
Write-Output "$($lesCas.Count) cas écrits dans $sortie"
