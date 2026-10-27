# Banc d'essai de l'Aide en Rust (plan Rust, étape 1a) : les mêmes demandes envoyées, sur leur vrai canal, à l'Aide
# PowerShell (Nevermind\fichiers\NevermindAide.ps1) et à l'Aide Rust (NevermindAide.exe construit par cargo), et leurs
# réponses comparées. Sans droits administrateur et sans rien modifier :
#   - toutes les demandes REFUSÉES du corpus (cas\aide\demandes.json) : même refus, même message, mot pour mot ;
#   - des lectures (aide.bonjour, nettoyage.lister, lecture.chemins, lecture.taches, lecture.pc...) : mêmes réponses ;
#   - les mesures continues (étape 1b), telles qu'elles tournent sans droits : trafic.tcp (mêmes connexions, comptage
#     refusé par Windows), trace réseau refusée (même message), et le vrai PresentMon (celui du dépôt, signé par Intel)
#     lancé par chaque Aide sur ce banc puis en veille : il est refusé par Windows, et ses lignes relayées sont les mêmes.
#     Aucun jeu n'est mesuré. Les enregistrements des mesures, eux, sont rejoués par outils\tester-mesures.ps1 et cargo test.
# Chaque Aide tourne dans sa propre copie (dossier d'essai), donc sur son propre canal ; elles s'arrêtent seules.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-aide-rust.ps1 -Exe <NevermindAide.exe construit>
#
# Code de sortie : 0 si tout est pareil, 1 sinon.
param([Parameter(Mandatory)][string]$Exe)
$ErrorActionPreference = 'Stop'
$racine = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'lire-cas.ps1')
if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) { Write-Host "NevermindAide.exe introuvable : $Exe" -ForegroundColor Red; exit 1 }

$moi = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$admin = (New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($admin) { Write-Host 'Ce banc se lance SANS droits administrateur (en administrateur, les Aides ne répondraient qu''à Mindset.exe).' -ForegroundColor Red; exit 1 }

# Les deux copies d'essai, chacune avec son Aide
$essai = Join-Path ([IO.Path]::GetTempPath()) "nevermind-aide-rust-$PID"
$copiePs = Join-Path $essai 'powershell'
$copieRust = Join-Path $essai 'rust'
[void][IO.Directory]::CreateDirectory($copiePs)
[void][IO.Directory]::CreateDirectory($copieRust)
Copy-Item -LiteralPath (Join-Path $racine 'Nevermind\fichiers') -Destination $copiePs -Recurse
Copy-Item -LiteralPath $Exe -Destination (Join-Path $copieRust 'NevermindAide.exe')
# (PresentMon, à sa place dans le dossier de Nevermind : fichiers\outils-tiers)
$pmRust = Join-Path $copieRust 'fichiers\outils-tiers'
[void][IO.Directory]::CreateDirectory($pmRust)
Copy-Item -LiteralPath (Join-Path $racine 'Nevermind\fichiers\outils-tiers\PresentMon.exe') -Destination $pmRust

function Get-NomCanal([string]$Dossier) {
    $d = ([IO.Path]::GetFullPath($Dossier).TrimEnd('\')).ToLowerInvariant()
    $o = [Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($d))
    "Nevermind-Aide-$moi-$(-join ($o[0..5] | ForEach-Object { $_.ToString('x2') }))"
}
$canaux = @{ PowerShell = Get-NomCanal $copiePs; Rust = Get-NomCanal $copieRust }

# Une demande, une réponse (une ligne JSON), comme AideClient (natif.cs)
function Send-Demande([string]$Canal, [string]$Ligne) {
    $c = New-Object IO.Pipes.NamedPipeClientStream('.', $Canal, [IO.Pipes.PipeDirection]::InOut)
    try {
        $c.Connect(10000)
        $enc = New-Object Text.UTF8Encoding($false)
        $w = New-Object IO.StreamWriter($c, $enc, 65536, $true); $w.WriteLine($Ligne); $w.Flush()
        $r = New-Object IO.StreamReader($c, $enc, $false, 65536, $true)
        $t = $r.ReadLine()
        if ($null -eq $t) { return $null }
        $t | ConvertFrom-Json
    } finally { $c.Dispose() }
}

# Une réponse sous une forme comparable : champs triés, nombres écrits pareil (543144 = 543144.0)
function ConvertTo-Canonique($V) {
    if ($null -eq $V) { return 'null' }
    if ($V -is [string]) { return (ConvertTo-Json -InputObject $V -Compress) }
    if ($V -is [bool]) { return "$V".ToLowerInvariant() }
    if ($V -is [decimal] -or $V -is [double] -or $V -is [single]) {
        if ([math]::Truncate([decimal]$V) -eq [decimal]$V) { return ([decimal]$V).ToString('0', [Globalization.CultureInfo]::InvariantCulture) }
        return ([decimal]$V).ToString([Globalization.CultureInfo]::InvariantCulture)
    }
    if ($V -is [ValueType]) { return ([string]$V) }
    if ($V -is [array]) { return '[' + (@(foreach ($x in $V) { ConvertTo-Canonique $x }) -join ',') + ']' }
    $p = @($V.PSObject.Properties | Sort-Object Name)
    '{' + (@(foreach ($x in $p) { (ConvertTo-Json -InputObject $x.Name -Compress) + ':' + (ConvertTo-Canonique $x.Value) }) -join ',') + '}'
}

$lances = @()
$echecs = New-Object Collections.Generic.List[string]
$nb = 0
try {
    $psExe = Join-Path ([Environment]::GetFolderPath('System')) 'WindowsPowerShell\v1.0\powershell.exe'
    $lances += Start-Process -FilePath $psExe -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$copiePs\fichiers\NevermindAide.ps1`"", '-Sid', $moi -WindowStyle Hidden -PassThru
    $lances += Start-Process -FilePath (Join-Path $copieRust 'NevermindAide.exe') -ArgumentList '-Sid', $moi -PassThru
    foreach ($n in $canaux.Keys) {
        $fin = (Get-Date).AddSeconds(60)
        while ((Get-Date) -lt $fin -and -not [IO.Directory]::GetFiles('\\.\pipe\').Contains("\\.\pipe\$($canaux[$n])")) { Start-Sleep -Milliseconds 200 }
        if (-not [IO.Directory]::GetFiles('\\.\pipe\').Contains("\\.\pipe\$($canaux[$n])")) { throw "l'Aide $n n'écoute pas" }
    }

    # Ce que le corpus attend de ce PC (comme tester-aide.ps1)
    $notepad = @("$env:windir\notepad.exe", "$env:windir\System32\notepad.exe", "$env:windir\System32\cmd.exe") | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    $avecPasserelle = @(Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue | Sort-Object RouteMetric | ForEach-Object { [int]$_.ifIndex })
    $carte = @(Get-NetAdapter -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' } | Sort-Object { $i = [array]::IndexOf($avecPasserelle, [int]$_.ifIndex); if ($i -lt 0) { 999 } else { $i } })[0]
    $valeurs = @{ '{windows}' = [Environment]::GetFolderPath('Windows'); '{bloc-notes}' = $notepad; '{pid}' = "$PID"; '{carte}' = $(if ($carte) { [int]$carte.ifIndex } else { -1 }) }
    $corpus = Read-Cas 'aide\demandes.json' $valeurs -Minimum 161

    # 1. Les demandes refusées : même refus, même message
    foreach ($c in @($corpus.cas | Where-Object { -not $_.attendu.accepte })) {
        $ligne = ConvertTo-Json -InputObject @{ a = $c.entree.action; p = $c.entree.parametres } -Depth 10 -Compress
        $r = @{}; foreach ($n in $canaux.Keys) { $r[$n] = Send-Demande $canaux[$n] $ligne }
        $nb++
        if (-not $r.PowerShell -or -not $r.Rust) { $echecs.Add("Sans réponse : $(Format-Cas $c)"); continue }
        if ($r.Rust.ok -or $r.PowerShell.ok) { $echecs.Add("Acceptée par $(if ($r.Rust.ok) { 'Rust' } else { 'PowerShell' }) : $(Format-Cas $c)"); continue }
        if ([string]$r.Rust.e -cne [string]$r.PowerShell.e) { $echecs.Add("Message différent : $(Format-Cas $c)`n      PowerShell : $($r.PowerShell.e)`n      Rust       : $($r.Rust.e)") }
    }

    # 2. Des lectures : mêmes réponses (sans les champs propres à chaque Aide)
    $lectures = @(
        @{ a = 'aide.bonjour'; p = @{} },
        @{ a = 'nettoyage.lister'; p = @{ Id = 'wu'; Top = 5 } },
        @{ a = 'nettoyage.lister'; p = @{ Id = 'do'; Top = 5 } },
        @{ a = 'nettoyage.lister'; p = @{ Id = 'dump'; Top = 5 } },
        @{ a = 'lecture.chemins'; p = @{ Pids = @("$PID") } },
        @{ a = 'lecture.taches'; p = @{} },
        @{ a = 'lecture.securite'; p = @{} },
        @{ a = 'lecture.pc'; p = @{} },
        @{ a = 'processus.fermer'; p = @{ Nom = 'nm-programme-inexistant' } },
        @{ a = 'dns.regler'; p = @{ Index = 999999; Serveurs = @('6.6.6.6') } },
        @{ a = 'fps.lire'; p = @{} },
        @{ a = 'fps.arreter'; p = @{} },
        @{ a = 'flux.lire'; p = @{} },
        @{ a = 'flux.arreter'; p = @{} }
    )
    foreach ($l in $lectures) {
        $ligne = ConvertTo-Json -InputObject $l -Depth 10 -Compress
        $r = @{}; foreach ($n in $canaux.Keys) { $r[$n] = Send-Demande $canaux[$n] $ligne }
        $nb++
        foreach ($n in $canaux.Keys) { if ($r[$n].r -and $r[$n].r.PSObject.Properties['Pid']) { $r[$n].r.Pid = 0; $r[$n].r.Temp = '' } }
        $j = @{}; foreach ($n in $canaux.Keys) { $j[$n] = ConvertTo-Canonique $r[$n] }
        if ($j.PowerShell -cne $j.Rust) { $echecs.Add("Lecture différente : $ligne`n      PowerShell : $($j.PowerShell)`n      Rust       : $($j.Rust)") }
    }

    # 3. Les mesures continues (étape 1b), sans droits : Windows refuse de compter, de tracer et à PresentMon, mais les
    #    demandes, les réponses et les erreurs font tout le trajet dans les deux Aides
    # Trace réseau : même refus de Windows, même message
    foreach ($ligne in '{"a":"flux.demarrer","p":{}}', '{"a":"flux.veiller","p":{}}', '{"a":"flux.lire","p":{}}', '{"a":"flux.arreter","p":{}}') {
        $r = @{}; foreach ($n in $canaux.Keys) { $r[$n] = Send-Demande $canaux[$n] $ligne }
        $nb++
        $j = @{}; foreach ($n in $canaux.Keys) { $j[$n] = ConvertTo-Canonique $r[$n] }
        if ($j.PowerShell -cne $j.Rust) { $echecs.Add("Trace réseau différente : $ligne`n      PowerShell : $($j.PowerShell)`n      Rust       : $($j.Rust)") }
    }
    # Compteurs TCP : même comptage (refusé sans droits) ; les connexions changent d'un instant à l'autre, donc Rust doit
    # voir celles que PowerShell voit juste avant ET juste après, et rien qu'il ne voie ni avant ni après
    $nb++
    $tcpOk = $false; $detail = ''
    for ($k = 1; $k -le 5 -and -not $tcpOk; $k++) {
        $a = Send-Demande $canaux.PowerShell '{"a":"trafic.tcp","p":{}}'
        $b = Send-Demande $canaux.Rust '{"a":"trafic.tcp","p":{}}'
        $c2 = Send-Demande $canaux.PowerShell '{"a":"trafic.tcp","p":{}}'
        $la = @($a.r.Lignes | ForEach-Object { [string]$_ }); $lb = @($b.r.Lignes | ForEach-Object { [string]$_ }); $lc = @($c2.r.Lignes | ForEach-Object { [string]$_ })
        $manquent = @($la | Where-Object { $lc -ccontains $_ -and $lb -cnotcontains $_ })
        $enTrop = @($lb | Where-Object { $la -cnotcontains $_ -and $lc -cnotcontains $_ })
        $tcpOk = $a.ok -and $b.ok -and $c2.ok -and ([string]$a.r.CompteursOk -ceq [string]$b.r.CompteursOk) -and -not $manquent.Count -and -not $enTrop.Count
        $detail = "PowerShell $($la.Count) puis $($lc.Count) connexions, Rust $($lb.Count) ; comptage $($a.r.CompteursOk) / $($b.r.CompteursOk) ; manquent : $($manquent -join ', ') ; en trop : $($enTrop -join ', ')"
    }
    if (-not $tcpOk) { $echecs.Add("trafic.tcp différent : $detail") }
    else { Write-Host "trafic.tcp : $detail" }
    # PresentMon : le vrai (celui du dépôt, signé par Intel), lancé par chaque Aide sur ce banc, puis en veille sur un
    # programme absent. Sans droits, Windows le refuse : les lignes relayées jusqu'à « #FIN » doivent être les mêmes.
    foreach ($etape in @(@{ a = 'fps.demarrer'; p = @{ Pid = $PID } }, @{ a = 'fps.veiller'; p = @{ Noms = @('nevermind-banc-absent.exe') } })) {
        $ligne = ConvertTo-Json -InputObject $etape -Depth 5 -Compress
        $lu = @{}
        foreach ($n in 'PowerShell', 'Rust') {
            $r = Send-Demande $canaux[$n] $ligne
            $texte = ''
            if ($r.ok) {
                $fin = (Get-Date).AddSeconds(20)
                while ((Get-Date) -lt $fin -and -not $texte.EndsWith("#FIN`n")) {
                    Start-Sleep -Milliseconds 250
                    $texte += [string](Send-Demande $canaux[$n] '{"a":"fps.lire","p":{}}').r
                }
            }
            [void](Send-Demande $canaux[$n] '{"a":"fps.arreter","p":{}}')
            $lu[$n] = "$(ConvertTo-Canonique $r) puis $(ConvertTo-Json -InputObject $texte -Compress)"
        }
        $nb++
        if ($lu.PowerShell -cne $lu.Rust) { $echecs.Add("PresentMon relayé différemment : $ligne`n      PowerShell : $($lu.PowerShell)`n      Rust       : $($lu.Rust)") }
        elseif (-not $lu.Rust.Contains('#FIN')) { $echecs.Add("PresentMon toujours en cours sans droits : $ligne : $($lu.Rust)") }
        else { Write-Host "$($etape.a) : $($lu.Rust)" }
    }
    $j = Send-Demande $canaux.Rust '{"a":"aide.journal","p":{}}'
    Write-Host "Journal de l'Aide Rust : $(@($j.r).Count) lignes, la première : $(@($j.r)[0])"
} catch {
    $echecs.Add("Banc interrompu : $($_.Exception.Message)")
} finally {
    foreach ($p in $lances) { try { if (-not $p.HasExited) { $p.Kill() } } catch {} }
    Start-Sleep -Milliseconds 500
    try { Remove-Item -LiteralPath $essai -Recurse -Force -ErrorAction SilentlyContinue } catch {}
}

Write-Host "$nb demandes envoyées aux deux Aides."
if ($echecs.Count) {
    Write-Host "$($echecs.Count) différence(s) :" -ForegroundColor Red
    foreach ($e in $echecs) { Write-Host "  - $e" -ForegroundColor Red }
    exit 1
}
Write-Host 'Les deux Aides répondent pareil.' -ForegroundColor Green
exit 0
