# Lecture du corpus de cas (cas\<sujet>\*.json, voir cas\LISEZMOI.md), pour les tests qui le passent à la version
# PowerShell. Le même corpus sera passé à la version Rust (cargo test) : un cas se change dans le fichier, jamais ici.
#
#   . (Join-Path $PSScriptRoot 'lire-cas.ps1')
#   $f = Read-Cas 'aide\demandes.json' @{ '{windows}' = 'C:\Windows'; '{carte}' = 12 }
#   foreach ($c in $f.cas) { ... $c.entree ... $c.attendu ... }
#
# Les valeurs du PC d'essai ({windows}, {carte}...) sont remplacées dans les cas seulement : un texte égal à une valeur
# prend sa valeur telle quelle (un nombre reste un nombre), un texte qui la contient la reçoit en texte. Une valeur
# citée dans un cas mais non donnée ici arrête le test (un cas ne doit jamais passer avec « {windows} » tel quel).

function Read-Cas([string]$Fichier, [hashtable]$Valeurs = @{}, [int]$Minimum = 1) {
    $chemin = Join-Path (Join-Path (Split-Path $PSScriptRoot -Parent) 'cas') $Fichier
    $f = [IO.File]::ReadAllText($chemin, [Text.Encoding]::UTF8) | ConvertFrom-Json
    if ([int]$f.format -ne 1) { throw "cas\$Fichier : format $($f.format) inconnu (attendu : 1)" }
    # (un fichier vidé ou coupé ne passe pas : le test donne le nombre de cas qu'il attend au moins)
    if (@($f.cas).Count -lt [math]::Max(1, $Minimum)) { throw "cas\$Fichier : $(@($f.cas).Count) cas, au moins $Minimum attendus" }
    foreach ($c in @($f.cas)) {
        if (-not $c.nom -or $null -eq $c.entree -or $null -eq $c.attendu) { throw "cas\$Fichier : un cas sans nom, entree ou attendu ($(ConvertTo-Json $c -Compress -Depth 10))" }
        $c.entree = Resolve-CasValeurs $c.entree $Valeurs $Fichier
    }
    $f
}

function Resolve-CasValeurs($V, [hashtable]$Valeurs, [string]$Fichier) {
    if ($null -eq $V) { return $null }
    if ($V -is [string]) {
        if ($Valeurs.ContainsKey($V)) { return $Valeurs[$V] }
        foreach ($k in $Valeurs.Keys) { $V = $V.Replace($k, [string]$Valeurs[$k]) }
        if ($V -match '\{[a-z][a-z-]*\}' -and -not $Valeurs.ContainsKey($Matches[0])) { throw "cas\$Fichier : valeur $($Matches[0]) non donnée par le test" }
        return $V
    }
    if ($V -is [array]) {
        $l = New-Object System.Collections.Generic.List[object]
        foreach ($x in $V) { $l.Add((Resolve-CasValeurs $x $Valeurs $Fichier)) }
        return , $l.ToArray()
    }
    if ($V -is [Management.Automation.PSCustomObject]) {
        foreach ($p in @($V.PSObject.Properties)) { $p.Value = Resolve-CasValeurs $p.Value $Valeurs $Fichier }
    }
    $V
}

# Un objet lu dans le JSON en table (un niveau), pour les règles qui attendent des tables PowerShell
function ConvertTo-CasTable($O) {
    if ($null -eq $O) { return $null }
    $t = @{}
    foreach ($p in $O.PSObject.Properties) { $t[$p.Name] = $p.Value }
    $t
}

# Le cas, en une ligne (pour les messages d'échec)
function Format-Cas($C) { "« $($C.nom) » $(ConvertTo-Json -InputObject $C.entree -Compress -Depth 10)" }
