# Les programmes et DLL livrés avec Nevermind ne doivent dépendre d'aucun runtime C à installer à part (VCRUNTIME140.dll,
# MSVCP140.dll...) : beaucoup de PC ne l'ont pas, et l'Aide ou NevermindPont.dll ne se chargeraient pas. Le Rust est donc
# construit avec le runtime C dedans (crt-static). Ce contrôle lit la table des imports de chaque fichier (imports
# ordinaires et différés), sans rien lancer, et refuse ceux qui importent VCRUNTIME*.dll ou MSVCP*.dll.
#
#   .\outils\verifier-imports.ps1 -Fichiers <exe ou dll>, ...     code de sortie 1 si un fichier est refusé
#   -Liste : affiche aussi toutes les DLL importées
param([Parameter(Mandatory = $true)][string[]]$Fichiers, [switch]$Liste)
$ErrorActionPreference = 'Stop'
$Interdites = '^(vcruntime\d+(_\d+)?|msvcp\d+(_\w+)?)\.dll$'

# Les DLL importées par un fichier PE (32 ou 64 bits)
function Get-Imports([string]$Chemin) {
    $o = [IO.File]::ReadAllBytes($Chemin)
    if ($o.Length -lt 64 -or $o[0] -ne 0x4D -or $o[1] -ne 0x5A) { throw "pas un programme Windows : $Chemin" }
    $pe = [BitConverter]::ToInt32($o, 0x3C)
    if ([BitConverter]::ToUInt32($o, $pe) -ne 0x4550) { throw "en-tête PE introuvable : $Chemin" }
    $coff = $pe + 4
    $nbSections = [BitConverter]::ToUInt16($o, $coff + 2)
    $tailleOpt = [BitConverter]::ToUInt16($o, $coff + 16)
    $opt = $coff + 20
    $magic = [BitConverter]::ToUInt16($o, $opt)
    $repertoires = if ($magic -eq 0x20B) { $opt + 112 } elseif ($magic -eq 0x10B) { $opt + 96 } else { throw "format inconnu ($magic) : $Chemin" }
    $sections = $opt + $tailleOpt
    # (une adresse en mémoire, RVA, ramenée à sa place dans le fichier)
    $versFichier = {
        param([uint32]$Rva)
        for ($i = 0; $i -lt $nbSections; $i++) {
            $s = $sections + 40 * $i
            $va = [BitConverter]::ToUInt32($o, $s + 12); $taille = [Math]::Max([BitConverter]::ToUInt32($o, $s + 8), [BitConverter]::ToUInt32($o, $s + 16))
            if ($Rva -ge $va -and $Rva -lt $va + $taille) { return [int]($Rva - $va + [BitConverter]::ToUInt32($o, $s + 20)) }
        }
        -1
    }
    $lireNom = { param([int]$p) $f = $p; while ($f -lt $o.Length -and $o[$f]) { $f++ }; [Text.Encoding]::ASCII.GetString($o, $p, $f - $p) }
    $noms = New-Object Collections.Generic.List[string]
    # Imports ordinaires (répertoire 1, descripteurs de 20 octets, nom à +12) et différés (répertoire 13, 32 octets, nom à +4)
    foreach ($r in @(@(1, 20, 12), @(13, 32, 4))) {
        $rva = [BitConverter]::ToUInt32($o, $repertoires + 8 * $r[0])
        if (-not $rva) { continue }
        $p = & $versFichier $rva
        if ($p -lt 0) { continue }
        while ($p + $r[1] -le $o.Length) {
            $nom = [BitConverter]::ToUInt32($o, $p + $r[2])
            if (-not $nom) { break }
            $q = & $versFichier $nom
            if ($q -ge 0) { $noms.Add((& $lireNom $q)) }
            $p += $r[1]
        }
    }
    @($noms | Sort-Object -Unique)
}

$refuses = 0
foreach ($f in $Fichiers) {
    if (-not (Test-Path -LiteralPath $f -PathType Leaf)) { Write-Host "Introuvable : $f" -ForegroundColor Red; $refuses++; continue }
    try { $imports = Get-Imports $f } catch { Write-Host "Illisible : $($_.Exception.Message)" -ForegroundColor Red; $refuses++; continue }
    $mauvaises = @($imports | Where-Object { $_ -match $Interdites })
    if ($Liste) { Write-Host "$(Split-Path $f -Leaf) importe : $($imports -join ', ')" }
    if ($mauvaises.Count) { Write-Host "Refusé : $f a besoin de $($mauvaises -join ', ') (runtime C à installer à part ; construire avec crt-static)" -ForegroundColor Red; $refuses++ }
    else { Write-Host "OK : $(Split-Path $f -Leaf) n'a besoin d'aucun runtime C à part ($($imports.Count) DLL importées)" }
}
if ($refuses) { exit 1 }
exit 0
