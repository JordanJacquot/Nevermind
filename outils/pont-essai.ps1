# Pour les tests : trouve NevermindPont.dll (rust\nevermind-pont), et la construit si elle manque ou si elle est plus
# vieille que son code (outils\construire-pont.ps1, seulement si Rust est installé).
#
#   . (Join-Path $PSScriptRoot 'pont-essai.ps1')
#   $dll = Get-PontEssai            # la DLL livrée : Nevermind\NevermindPont.dll (là où Nevermind la charge)
#   $dll = Get-PontEssai -Panique   # la DLL d'essai (avec « essai.panique ») : _test\pont-essai\NevermindPont.dll
#
# Les tests de Nevermind la chargent avec Initialize-Pont (Nevermind\fichiers\modules\pont.ps1), comme Nevermind.

function Get-PontEssai([switch]$Panique) {
    $depot = Split-Path $PSScriptRoot -Parent
    $dll = if ($Panique) { Join-Path $depot '_test\pont-essai\NevermindPont.dll' } else { Join-Path $depot 'Nevermind\NevermindPont.dll' }
    # (le code de la DLL : le sien et celui des règles de l'Aide)
    $sources = @(Get-ChildItem -LiteralPath (Join-Path $depot 'rust') -Recurse -File -Include '*.rs', 'Cargo.toml', 'Cargo.lock' -ErrorAction SilentlyContinue |
                 Where-Object { $_.FullName -notmatch '\\target\\' -and $_.FullName -match '\\rust\\(nevermind-pont|aide-regles)\\|\\rust\\Cargo\.(toml|lock)$' })
    $recent = ($sources | Measure-Object -Property LastWriteTimeUtc -Maximum).Maximum
    $aJour = (Test-Path -LiteralPath $dll -PathType Leaf) -and (-not $recent -or (Get-Item -LiteralPath $dll).LastWriteTimeUtc -ge $recent)
    if ($aJour) { return $dll }
    $rust = (Get-Command cargo -ErrorAction SilentlyContinue) -or (Test-Path -LiteralPath 'D:\Outils\Rust\cargo\bin\cargo.exe')
    if (-not $rust) {
        if (Test-Path -LiteralPath $dll -PathType Leaf) { return $dll }   # (plus vieille que son code, mais rien pour la reconstruire)
        throw "NevermindPont.dll introuvable ($dll) et Rust absent : lancer outils\construire-pont.ps1 là où Rust est installé"
    }
    Write-Host "NevermindPont.dll $(if ($Panique) { 'd''essai ' })absente ou plus vieille que son code : construction (outils\construire-pont.ps1)..."
    $sortie = @(& (Join-Path $PSScriptRoot 'construire-pont.ps1') -Essai:$Panique)
    if (-not (Test-Path -LiteralPath $dll -PathType Leaf)) { throw "NevermindPont.dll pas construite ($($sortie -join ' '))" }
    $dll
}
