# Test de l'Aide face à un environnement piégé (constats 1 et 30 de la revue du 25/10), sans droits et sans toucher au PC.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-aide-environnement.ps1
#
# L'Aide tourne en administrateur, mais la tâche planifiée lui donne les variables de l'utilisateur, qu'un programme
# sans droits peut changer. Ce script lance une vraie Aide de test, dans un dossier temporaire, avec :
#   - de faux modules PowerShell en tête de PSModulePath (et dans un faux Program Files) : ScheduledTasks, NetAdapter,
#     les modules de base de PowerShell... Chacun écrit un fichier témoin s'il est chargé ;
#   - windir, ProgramFiles et ProgramData qui pointent vers de faux dossiers (un faux SystemRoot empêche PowerShell et
#     .NET de démarrer : il n'est pas essayé).
# Elle est lancée deux fois : par NevermindAide.exe (compilé ici depuis outils\lanceur.cs, comme chez l'utilisateur),
# puis par powershell.exe (le script seul). Un client (copie de powershell.exe nommée Mindset.exe, à côté de l'Aide :
# en administrateur, sur la CI, l'Aide ne répond qu'à lui) fait des demandes qui se servent de ces modules, toutes en
# lecture seule. Aucun témoin ne doit apparaître, et le nettoyage ne doit jamais viser le faux dossier de Windows.
# Canal (constat 30) : une demande de plus de 64 Ko, ou une connexion muette, est coupée (5 s au plus), et l'Aide
# continue de répondre ensuite. Un canal du même nom créé avant l'Aide par un autre programme : elle s'arrête au lieu
# de s'y joindre (audit du 25/10, 25a).
# Seuls les processus lancés par ce test sont arrêtés. Code de sortie : 0 si tout passe, 1 sinon.

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$app     = Join-Path $racine 'Nevermind\fichiers'
$echecs  = New-Object System.Collections.Generic.List[string]
$nbTests = 0
function Test-Ok($Condition, [string]$Message) {
    $script:nbTests++
    if (-not [bool]$Condition) { $script:echecs.Add($Message) }
}
$psExe = Join-Path ([Environment]::GetFolderPath('System')) 'WindowsPowerShell\v1.0\powershell.exe'

# Le client : quelques demandes en lecture seule, puis le canal (trop long, muet), puis le journal de l'Aide
$client = @'
param($Nom, [string]$Sortie)
$ErrorActionPreference = 'Stop'
$enc = New-Object Text.UTF8Encoding($false)
function Send-Demande([string]$Json, [int]$Delai = 60000) {
    $c = New-Object System.IO.Pipes.NamedPipeClientStream('.', $Nom, [System.IO.Pipes.PipeDirection]::InOut)
    try {
        $c.Connect(15000)
        $w = New-Object IO.StreamWriter($c, $enc); $w.AutoFlush = $true
        $w.WriteLine($Json)
        $t = (New-Object IO.StreamReader($c, $enc)).ReadLineAsync()
        if (-not $t.Wait($Delai)) { return 'pas de réponse' }
        [string]$t.Result
    } catch { "erreur : $($_.Exception.Message)" } finally { $c.Dispose() }
}
# Attend que la connexion soit coupée par l'Aide (fin de lecture ou erreur) ; rend le temps en secondes, ou -1
function Wait-Coupure($c, [int]$Delai) {
    $debut = Get-Date
    try {
        $o = New-Object byte[] 16
        $t = $c.ReadAsync($o, 0, 16)
        if (-not $t.Wait($Delai)) { return -1 }
        if ($t.Result -gt 0) { return -1 }
    } catch {}
    [math]::Round(((Get-Date) - $debut).TotalSeconds, 1)
}
$r = [ordered]@{}
$fin = (Get-Date).AddSeconds(60)
do { $r.bonjour = Send-Demande '{"a":"aide.bonjour","p":{}}' 5000 } while ($r.bonjour -notmatch '"ok"' -and (Get-Date) -lt $fin -and -not (Start-Sleep -Milliseconds 300))
$r.taches   = Send-Demande '{"a":"lecture.taches","p":{}}'
$r.chemins  = Send-Demande ('{"a":"lecture.chemins","p":{"Pids":["' + $PID + '"]}}')
$r.securite = Send-Demande '{"a":"lecture.securite","p":{}}'
$r.pc       = Send-Demande '{"a":"lecture.pc","p":{}}'
$r.dns      = Send-Demande '{"a":"dns.regler","p":{"Index":999999,"Serveurs":["6.6.6.6"]}}'
$r.fermer   = Send-Demande '{"a":"processus.fermer","p":{"Nom":"nm-programme-inexistant"}}'
$r.nettoyage = Send-Demande '{"a":"nettoyage.lister","p":{"Id":"dump","Top":1000}}'
# Une demande de 70 Ko sans fin de ligne : coupée sans attendre la suite
$c = New-Object System.IO.Pipes.NamedPipeClientStream('.', $Nom, [System.IO.Pipes.PipeDirection]::InOut)
try {
    $c.Connect(15000)
    $gros = $enc.GetBytes(('{"a":"aide.bonjour","p":{"x":"' + ('a' * 70000)))
    try { $c.Write($gros, 0, $gros.Length); $c.Flush() } catch {}
    $r.longue = Wait-Coupure $c 4000
} catch { $r.longue = "erreur : $($_.Exception.Message)" } finally { $c.Dispose() }
# Une connexion muette : coupée après 5 s
$c = New-Object System.IO.Pipes.NamedPipeClientStream('.', $Nom, [System.IO.Pipes.PipeDirection]::InOut)
try { $c.Connect(15000); $r.muette = Wait-Coupure $c 9000 } catch { $r.muette = "erreur : $($_.Exception.Message)" } finally { $c.Dispose() }
$r.apres   = Send-Demande '{"a":"aide.bonjour","p":{}}'
$r.journal = Send-Demande '{"a":"aide.journal","p":{}}'
[IO.File]::WriteAllText($Sortie, (ConvertTo-Json -InputObject $r -Compress), $enc)
'@

# (chemin long : sur la CI, %TEMP% est en nom court, alors que Windows donne le chemin long du client)
Add-Type -Namespace TestAideEnv -Name Chemins -MemberDefinition '[DllImport("kernel32.dll", CharSet = CharSet.Unicode)] public static extern uint GetLongPathName(string court, System.Text.StringBuilder long_, uint taille);'
$tmp = Join-Path $env:TEMP "nevermind-test-environnement-$PID"
[void][IO.Directory]::CreateDirectory($tmp)
$long = New-Object System.Text.StringBuilder 1024
if ([TestAideEnv.Chemins]::GetLongPathName($tmp, $long, 1024)) { $tmp = $long.ToString() }
$lances = New-Object System.Collections.Generic.List[object]
try {
    # --- La copie de l'appli : l'Aide, ses modules, NevermindAide.exe compilé ici, et le client « Mindset.exe »
    $appli = Join-Path $tmp 'appli'
    $copie = Join-Path $appli 'fichiers'
    [void][IO.Directory]::CreateDirectory($copie)
    Copy-Item -LiteralPath (Join-Path $app 'NevermindAide.ps1') -Destination $copie
    Copy-Item -LiteralPath (Join-Path $app 'Nevermind.ps1') -Destination $copie   # (seulement présent : sans lui, le lanceur s'arrête avec un message)
    Copy-Item -LiteralPath (Join-Path $app 'modules') -Destination $copie -Recurse
    Copy-Item -LiteralPath $psExe -Destination (Join-Path $appli 'Mindset.exe')
    $csc = Join-Path ([Environment]::GetFolderPath('Windows')) 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
    $sma = @(Get-ChildItem (Join-Path ([Environment]::GetFolderPath('Windows')) 'Microsoft.NET\assembly\GAC_MSIL\System.Management.Automation') -Recurse -Filter 'System.Management.Automation.dll' | Select-Object -First 1)[0].FullName
    $exeAide = Join-Path $appli 'NevermindAide.exe'
    $sortieCsc = & $csc /nologo /target:winexe /platform:anycpu "/win32manifest:$(Join-Path $PSScriptRoot 'lanceur.manifest')" /reference:System.Windows.Forms.dll /reference:System.Web.Extensions.dll "/reference:$sma" "/out:$exeAide" /define:AIDE (Join-Path $PSScriptRoot 'lanceur.cs') 2>&1
    if ($LASTEXITCODE) { throw "compilation de NevermindAide.exe impossible : $($sortieCsc -join ' ')" }

    # --- Le piège : faux modules (chacun écrit un témoin), faux dossiers de Windows
    $temoins = Join-Path $tmp 'temoins'
    [void][IO.Directory]::CreateDirectory($temoins)
    $fauxModules = Join-Path $tmp 'documents\WindowsPowerShell\Modules'
    $fauxWindows = Join-Path $tmp 'faux-windows'
    $fauxProgrammes = Join-Path $tmp 'faux-programmes'
    $fauxDonnees = Join-Path $tmp 'faux-programdata'
    # (chaque faux module déclare les commandes du vrai dont l'Aide se sert : c'est ce qui le fait choisir au chargement
    # automatique, le premier trouvé dans PSModulePath l'emporte)
    $commandes = [ordered]@{
        'Microsoft.PowerShell.Management' = 'Get-Process', 'Join-Path', 'Test-Path', 'Get-ChildItem', 'Get-ItemProperty'
        'Microsoft.PowerShell.Utility' = 'Get-Date', 'ConvertFrom-Json', 'ConvertTo-Json', 'Select-Object', 'Sort-Object', 'Add-Type', 'New-Object', 'Start-Sleep'
        'Microsoft.PowerShell.Security' = 'Get-AuthenticodeSignature'
        'CimCmdlets' = 'Get-CimInstance'
        'ScheduledTasks' = 'Get-ScheduledTask'
        'NetAdapter' = 'Get-NetAdapter'
        'NetTCPIP' = 'Get-NetRoute'
        'NetSecurity' = 'Get-NetFirewallRule', 'New-NetFirewallRule'
        'DnsClient' = 'Set-DnsClientServerAddress', 'Clear-DnsClientCache'
        'Defender' = 'Get-MpPreference', 'Get-MpThreat', 'Get-MpThreatDetection'
        'Storage' = 'Get-PhysicalDisk', 'Get-StorageReliabilityCounter'
        'SecureBoot' = 'Confirm-SecureBootUEFI'
        'SmbShare' = 'Set-SmbServerConfiguration'
        'Dism' = 'Disable-WindowsOptionalFeature'
    }
    foreach ($dossier in $fauxModules, (Join-Path $fauxProgrammes 'WindowsPowerShell\Modules')) {
        foreach ($n in $commandes.Keys) {
            $d = Join-Path $dossier $n
            [void][IO.Directory]::CreateDirectory($d)
            $ecrire = "[IO.File]::AppendAllText('$((Join-Path $temoins "$n.txt") -replace "'", "''")', ""faux module $n chargé par le processus `$PID``r``n"")"
            $code = @($ecrire) + @($commandes[$n] | ForEach-Object { "function $_ { $ecrire }" }) + 'Export-ModuleMember -Function *'
            [IO.File]::WriteAllText((Join-Path $d "$n.psm1"), ($code -join "`r`n"), (New-Object Text.UTF8Encoding $true))
        }
    }
    $fichierPiege = Join-Path $fauxWindows 'Minidump\nevermind-piege.txt'
    [void][IO.Directory]::CreateDirectory((Split-Path $fichierPiege))
    [IO.File]::WriteAllText($fichierPiege, 'piège')
    [IO.File]::SetLastWriteTime($fichierPiege, (Get-Date).AddDays(-3))
    [void][IO.Directory]::CreateDirectory($fauxDonnees)

    $fichierClient = Join-Path $tmp 'client.ps1'
    [IO.File]::WriteAllText($fichierClient, $client, (New-Object Text.UTF8Encoding $true))
    . ([scriptblock]::Create([System.Management.Automation.Language.Parser]::ParseFile((Join-Path $copie 'modules\aide-actions.ps1'), [ref]$null, [ref]$null).Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Get-AideSuffixe' }, $true).Extent.Text))
    $nomCanal = "Nevermind-Aide-$([Security.Principal.WindowsIdentity]::GetCurrent().User.Value)-$(Get-AideSuffixe $appli)"

    foreach ($variante in 'NevermindAide.exe', 'powershell.exe') {
        Get-ChildItem -LiteralPath $temoins -File | ForEach-Object { [IO.File]::Delete($_.FullName) }
        $psi = if ($variante -eq 'NevermindAide.exe') { New-Object Diagnostics.ProcessStartInfo($exeAide) }
               else { New-Object Diagnostics.ProcessStartInfo($psExe, "-NoProfile -ExecutionPolicy Bypass -File `"$(Join-Path $copie 'NevermindAide.ps1')`" -Parent $PID") }
        $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true
        $psi.EnvironmentVariables['PSModulePath'] = "$fauxModules;$(Join-Path $fauxProgrammes 'WindowsPowerShell\Modules');$env:PSModulePath"
        $psi.EnvironmentVariables['windir'] = $fauxWindows
        $psi.EnvironmentVariables['ProgramFiles'] = $fauxProgrammes
        $psi.EnvironmentVariables['ProgramW6432'] = $fauxProgrammes
        $psi.EnvironmentVariables['ProgramData'] = $fauxDonnees
        $aide = [Diagnostics.Process]::Start($psi); $lances.Add($aide)
        $sortie = Join-Path $tmp "resultat-$variante.json"
        $cl = Start-Process (Join-Path $appli 'Mindset.exe') -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$fichierClient`"", $nomCanal, "`"$sortie`"" -WindowStyle Hidden -PassThru
        $lances.Add($cl)
        if (-not $cl.WaitForExit(240000)) { Test-Ok $false "$variante : le client n'a pas fini en 4 minutes" }
        $r = $null
        try { $r = [IO.File]::ReadAllText($sortie, [Text.Encoding]::UTF8) | ConvertFrom-Json } catch {}
        if (-not $r) { Test-Ok $false "$variante : le client n'a rien rapporté$(if ($aide.HasExited) { " (Aide arrêtée, code $($aide.ExitCode))" })"; continue }
        Test-Ok ($r.bonjour -match '"ok":true') "$variante : l'Aide ne répond pas ($($r.bonjour))"
        foreach ($d in 'taches', 'chemins', 'securite', 'pc', 'fermer', 'nettoyage') { Test-Ok ($r.$d -match '"ok":true') "$variante : la demande $d échoue ($(([string]$r.$d).Substring(0, [math]::Min(200, ([string]$r.$d).Length))))" }
        Test-Ok ($r.dns -match '"ok":false' -and $r.dns -match 'carte') "$variante : la demande DNS sur une carte inexistante ne répond pas comme prévu ($($r.dns))"
        # Aucun faux module chargé
        $vus = @(Get-ChildItem -LiteralPath $temoins -File | ForEach-Object { [IO.Path]::GetFileNameWithoutExtension($_.Name) })
        Test-Ok (-not $vus.Count) "$variante : faux module(s) chargé(s) par l'Aide : $($vus -join ', ')"
        # Le nettoyage vise le vrai dossier de Windows, jamais celui de la variable windir
        Test-Ok ($r.nettoyage -notmatch 'nevermind-piege') "$variante : le nettoyage lit le faux dossier de Windows (variable windir)"
        # Canal : demande trop longue, connexion muette
        Test-Ok (($r.longue -as [double]) -ge 0) "$variante : une demande de 70 Ko sans fin de ligne n'est pas coupée ($($r.longue))"
        Test-Ok (($r.muette -as [double]) -ge 3 -and ($r.muette -as [double]) -le 8) "$variante : une connexion muette n'est pas coupée après 5 s ($($r.muette))"
        Test-Ok ($r.apres -match '"ok":true') "$variante : l'Aide ne répond plus après les connexions coupées ($($r.apres))"
        $journal = try { (($r.journal | ConvertFrom-Json).r) -join "`n" } catch { [string]$r.journal }
        Test-Ok ($journal -match 'trop longue' -and $journal -match 'trop lente') "$variante : journal de l'Aide sans les connexions coupées"
        if ($echecs.Count) { Write-Host "Journal de l'Aide ($variante) :`n$journal" }
        else { Write-Host "Info : $variante : aucun faux module chargé, 70 Ko coupés en $($r.longue) s, connexion muette coupée en $($r.muette) s" }
        try { if (-not $aide.HasExited) { $aide.Kill(); [void]$aide.WaitForExit(5000) } } catch {}
    }

    # Canal pris AVANT l'Aide (audit du 25/10, 25a) : un programme crée le canal de ce nom, avec des droits larges.
    # L'Aide doit refuser de s'y joindre et s'arrêter (avant : elle y ajoutait ses connexions, avec les droits de l'autre).
    $large = New-Object System.IO.Pipes.PipeSecurity
    $large.AddAccessRule((New-Object System.IO.Pipes.PipeAccessRule((New-Object Security.Principal.SecurityIdentifier('S-1-1-0')), 'FullControl', 'Allow')))
    $occupant = New-Object System.IO.Pipes.NamedPipeServerStream($nomCanal, 'InOut', 32, 'Byte', 'Asynchronous', 4096, 4096, $large)
    try {
        $psi = New-Object Diagnostics.ProcessStartInfo($psExe, "-NoProfile -ExecutionPolicy Bypass -File `"$(Join-Path $copie 'NevermindAide.ps1')`" -Parent $PID")
        $psi.UseShellExecute = $false; $psi.CreateNoWindow = $true
        $aide = [Diagnostics.Process]::Start($psi); $lances.Add($aide)
        Test-Ok ($aide.WaitForExit(60000)) 'Canal pris avant l''Aide : elle s''y joint au lieu de s''arrêter'
    } finally { $occupant.Dispose() }
} catch { Test-Ok $false "Test impossible : $($_.Exception.Message)" }
finally {
    foreach ($p in $lances) { try { if (-not $p.HasExited) { $p.Kill(); [void]$p.WaitForExit(5000) } } catch {} }
    Start-Sleep -Milliseconds 300
    try { [IO.Directory]::Delete($tmp, $true) } catch {}
}

if ($echecs.Count) {
    Write-Host "$($echecs.Count) échec(s) sur $nbTests vérifications :" -ForegroundColor Red
    foreach ($e in $echecs) { Write-Host "  - $e" -ForegroundColor Red }
    exit 1
}
Write-Host "Aide face à un environnement piégé : $nbTests vérifications, tout passe." -ForegroundColor Green
exit 0
