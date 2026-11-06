# Exporte les « juges » des blocs Jeux et Divers de Nevermind en Rust : la version PowerShell (la référence) est
# chargée fonction par fonction, en mode test, sur un faux PC fabriqué dans un dossier temporaire (fausses
# bibliothèques Steam, Epic, GOG, Ankama, Xbox, faux registre, faux fichiers de données). Rien du vrai PC n'est lu :
# le registre est simulé, les disques aussi, et les dossiers de données sont ceux de l'essai.
#
#   & 'outils\exporter-juges-jeux.ps1'            (depuis l'outil PowerShell, jamais depuis Bash)
#
# Écrit rust\nevermind-jeux\tests\juges\*.json et rust\nevermind-divers\tests\juges\*.json : chaque cas donne
# l'entrée (le faux PC, déclaré) et la sortie de la version PowerShell. Les tests Rust refont le même faux PC et
# doivent donner exactement la même sortie.

$ErrorActionPreference = 'Stop'
$env:NEVERMIND_TEST = '1'
$racine  = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'
$sortieJeux = Join-Path $racine 'rust\nevermind-jeux\tests\juges'
$sortieDivers = Join-Path $racine 'rust\nevermind-divers\tests\juges'
New-Item -ItemType Directory -Force -Path $sortieJeux, $sortieDivers | Out-Null
Add-Type -AssemblyName System.Drawing

# ---------------------------------------------------------------------------
# Les fonctions de la version PowerShell, lues dans les modules (rien ne se lance au chargement)
# ---------------------------------------------------------------------------
$asts = @{}
function Get-Ast([string]$Fichier) {
    if (-not $asts.ContainsKey($Fichier)) {
        $tok = $null; $err = $null
        $asts[$Fichier] = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules "$Fichier.ps1"), [ref]$tok, [ref]$err)
        if ($err) { throw "$Fichier.ps1 illisible : $($err[0].Message)" }
    }
    $asts[$Fichier]
}
function Import-Fonction([string]$Fichier, [string]$Nom, [scriptblock]$Retouche) {
    $a = (Get-Ast $Fichier).Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $Nom }, $false)
    if (-not $a) { throw "Fonction $Nom introuvable dans $Fichier.ps1" }
    $t = $a.Extent.Text -replace "^function\s+$([regex]::Escape($Nom))", "function script:$Nom"
    if ($Retouche) { $t = & $Retouche $t }
    Invoke-Expression $t
}
# Une variable du module (« $SearchEntries = @(...) »), recalculée ici
function Import-Variable([string]$Fichier, [string]$Nom) {
    $a = (Get-Ast $Fichier).Find({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq "`$$Nom" }, $false)
    if (-not $a) { throw "Variable $Nom introuvable dans $Fichier.ps1" }
    Invoke-Expression ($a.Extent.Text -replace "^\`$$Nom", "`$script:$Nom")
}

# ---------------------------------------------------------------------------
# L'heure fixée (les dates relatives : « hier », « il y a 3 jours ») et les petites aides de Nevermind
# ---------------------------------------------------------------------------
$script:Maintenant = [datetime]'2026-11-06T15:00:00'
function script:Get-Date { param([Parameter(Position = 0)]$Date, [string]$Format) $d = if ($Date) { [datetime]$Date } else { $script:Maintenant }; if ($Format) { $d.ToString($Format) } else { $d } }
function script:Write-Log([string]$M) { }
function script:Set-Status([string]$T) { }
function script:Get-ImageVue([string]$Chemin) { if ($Chemin) { "https://fichier.local/$Chemin" } }

# Le faux registre : clé (en minuscules, « HKLM:\... ») vers ses valeurs ; ses sous-clés dans l'ordre de déclaration
$script:FauxReg = @{}
$script:FauxEnfants = @{}
function script:ConvertTo-CleFausse([string]$P) {
    $c = ([string]$P) -replace '^FAUX::', '' -replace '^Registry::', ''
    $c = $c -replace '^HKEY_LOCAL_MACHINE\\', 'HKLM:\' -replace '^HKEY_CURRENT_USER\\', 'HKCU:\' -replace '^HKEY_CLASSES_ROOT\\', 'HKCR:\'
    $c.TrimEnd('\').ToLowerInvariant()
}
function script:Test-Registre([string]$P) { [string]$P -match '^(FAUX::|Registry::|HK(LM|CU|CR|U):)' }
function script:New-ProprietesFausses([string]$Cle) {
    $v = $script:FauxReg[(ConvertTo-CleFausse $Cle)]
    if ($null -eq $v) { return $null }
    $o = [ordered]@{}
    foreach ($k in $v.Keys) { $o[$(if ($k -eq '') { '(default)' } else { $k })] = $v[$k] }
    $o.PSChildName = ($Cle.TrimEnd('\') -split '\\')[-1]
    $o.PSPath = "FAUX::$Cle"
    [pscustomobject]$o
}
function script:Get-ItemProperty {
    [CmdletBinding()] param([Parameter(Position = 0)]$Path, [Parameter(Position = 1)]$Name, $LiteralPath)
    $chemins = if ($LiteralPath) { @($LiteralPath) } else { @($Path) }
    foreach ($c in $chemins) {
        if (-not (Test-Registre $c)) { Microsoft.PowerShell.Management\Get-ItemProperty -LiteralPath $c; continue }
        $c = ([string]$c) -replace '^FAUX::', ''
        if ($c -match '\\\*$') {
            $parent = $c.Substring(0, $c.Length - 2)
            foreach ($e in @($script:FauxEnfants[(ConvertTo-CleFausse $parent)])) { if ($e) { New-ProprietesFausses "$parent\$e" } }
        } else {
            $o = New-ProprietesFausses $c
            if ($null -eq $o -and $ErrorActionPreference -eq 'Stop') { throw "clé absente : $c" }
            $o
        }
    }
}
function script:Get-ChildItem {
    [CmdletBinding()] param([Parameter(Position = 0)]$Path, $LiteralPath, $Filter, [switch]$Recurse, $Depth, [switch]$File, [switch]$Directory)
    $p = if ($LiteralPath) { $LiteralPath } else { $Path }
    if (Test-Registre $p) {
        $c = ([string]$p) -replace '^FAUX::', ''
        foreach ($e in @($script:FauxEnfants[(ConvertTo-CleFausse $c)])) { if ($e) { [pscustomobject]@{ PSPath = "FAUX::$c\$e"; PSChildName = $e } } }
        return
    }
    $null = $PSBoundParameters.Remove('Path')
    if ($Path) { Microsoft.PowerShell.Management\Get-ChildItem -Path $Path @PSBoundParameters } else { Microsoft.PowerShell.Management\Get-ChildItem @PSBoundParameters }
}
function script:Get-RegValue([string]$Path, [string]$Name) { $v = $script:FauxReg[(ConvertTo-CleFausse $Path)]; if ($v -and $v.Contains($Name)) { , $v[$Name] } }
function script:Get-GpuPreference([string]$Exe) { $v = $script:FauxReg['hkcu:\software\microsoft\directx\usergpupreferences']; if ($v -and $v.Contains($Exe)) { [string]$v[$Exe] } }
function script:Get-FauxDisques { foreach ($d in $script:FauxDisques) { [pscustomobject]@{ RootDirectory = [IO.DirectoryInfo]$d } } }

# ---------------------------------------------------------------------------
# Le faux PC : déclaré (pour que Rust refasse le même), puis fabriqué dans le bac
# ---------------------------------------------------------------------------
$script:Bac = [IO.Path]::GetFullPath((Join-Path $env:TEMP "nevermind-juges-jeux-$([guid]::NewGuid().ToString('N').Substring(0, 8))"))
New-Item -ItemType Directory -Force -Path $script:Bac | Out-Null
$script:VraiMaintenant = [datetime]::Now

function script:Resolve-Bac([string]$S) { $S.Replace('{BAC}', $script:Bac) }
# Fabrique un faux PC dans un dossier neuf du bac : fichiers (texte T, taille Taille, image B64), âges en jours (Age),
# registre (Registre : Ruche, Cle, Valeurs) et disques. Renvoie le dossier.
function script:New-FauxPc($Pc) {
    $d = Join-Path $script:Bac ([guid]::NewGuid().ToString('N').Substring(0, 8))
    New-Item -ItemType Directory -Force -Path $d | Out-Null
    $script:Ici = $d
    $sub = { param($s) ([string]$s).Replace('{BAC2}', $d.Replace('\', '\\')).Replace('{BAC}', $d) }
    foreach ($f in @($Pc.Fichiers)) {
        if (-not $f) { continue }
        $p = Join-Path $d $f.C
        if ($f.Dossier) { New-Item -ItemType Directory -Force -Path $p | Out-Null; continue }
        New-Item -ItemType Directory -Force -Path (Split-Path $p -Parent) | Out-Null
        if ($null -ne $f.T) { [IO.File]::WriteAllText($p, (& $sub $f.T), (New-Object Text.UTF8Encoding($false))) }
        elseif ($f.B64) { [IO.File]::WriteAllBytes($p, [Convert]::FromBase64String($f.B64)) }
        else { [IO.File]::WriteAllBytes($p, (New-Object byte[] ([int]$f.Taille))) }
    }
    # Les âges à la fin (créer un fichier change la date de son dossier), les plus profonds d'abord
    foreach ($f in @($Pc.Fichiers | Where-Object { $_ -and $null -ne $_.Age } | Sort-Object { -([string]$_.C).Length })) {
        $p = Join-Path $d $f.C
        $t = $script:VraiMaintenant.AddDays(-[double]$f.Age)
        if ([IO.Directory]::Exists($p)) { [IO.Directory]::SetLastWriteTime($p, $t) } else { [IO.File]::SetLastWriteTime($p, $t) }
    }
    $script:FauxReg = @{}; $script:FauxEnfants = @{}
    foreach ($r in @($Pc.Registre)) {
        if (-not $r) { continue }
        $cle = "$($r.Ruche):\$($r.Cle)"
        $k = ConvertTo-CleFausse $cle
        if (-not $script:FauxReg.ContainsKey($k)) { $script:FauxReg[$k] = [ordered]@{} }
        foreach ($n in $r.Valeurs.Keys) { $script:FauxReg[$k][$n] = (& $sub $r.Valeurs[$n]) }
        # Chaque clé est rangée sous ses parents
        $parts = $cle -split '\\'
        for ($i = 1; $i -lt $parts.Count; $i++) {
            $parent = ConvertTo-CleFausse ($parts[0..($i - 1)] -join '\')
            if (-not $script:FauxEnfants.ContainsKey($parent)) { $script:FauxEnfants[$parent] = New-Object System.Collections.ArrayList }
            if (-not ($script:FauxEnfants[$parent] -contains $parts[$i])) { [void]$script:FauxEnfants[$parent].Add($parts[$i]) }
        }
    }
    $script:FauxDisques = @($Pc.Disques | Where-Object { $_ } | ForEach-Object { & $sub $_ })
    foreach ($n in 'ProgramData', 'APPDATA') { Set-Item "env:$n" -Value (& $sub $(if ($Pc.Env -and $Pc.Env[$n]) { $Pc.Env[$n] } else { "{BAC}\vide-$n" })) }
    $d
}
# Une sortie avec les chemins du bac remplacés par {BAC}
function script:ConvertTo-Juge($Objet) {
    $j = ConvertTo-Json -InputObject $Objet -Depth 30 -Compress
    # (sans tenir compte de la casse : Ubisoft garde les dossiers en minuscules)
    [regex]::Replace($j, [regex]::Escape(($script:Ici -replace '\\', '\\')), '{BAC}', 'IgnoreCase')
}
function script:Save-Juge([string]$Dossier, [string]$Nom, $Cas) {
    $t = '[' + ((@($Cas) | ForEach-Object { if ($_ -is [string]) { $_ } else { ConvertTo-Json -InputObject $_ -Depth 30 -Compress } }) -join ",`n") + ']'
    [IO.File]::WriteAllText((Join-Path $Dossier "$Nom.json"), $t, (New-Object Text.UTF8Encoding($false)))
    Write-Host "$Nom : $(@($Cas).Count) cas"
}
# Une petite image JPEG (largeur, hauteur) en base64
function script:New-ImageB64([int]$W, [int]$H) {
    $b = New-Object System.Drawing.Bitmap $W, $H
    $ms = New-Object IO.MemoryStream
    $b.Save($ms, [System.Drawing.Imaging.ImageFormat]::Jpeg); $b.Dispose()
    $s = [Convert]::ToBase64String($ms.ToArray()); $ms.Dispose(); $s
}

$OrigEnv = @{ ProgramData = $env:ProgramData; APPDATA = $env:APPDATA }
try {
    . (Join-Path $PSScriptRoot 'exporter-juges-jeux.cas.ps1')
}
finally {
    $env:ProgramData = $OrigEnv.ProgramData; $env:APPDATA = $OrigEnv.APPDATA
    Remove-Item Env:NEVERMIND_TEST -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $script:Bac -Recurse -Force -ErrorAction SilentlyContinue
}
