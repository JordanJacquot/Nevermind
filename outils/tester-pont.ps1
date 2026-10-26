# Tests de NevermindPont.dll (rust\nevermind-pont) vue depuis Nevermind (modules\pont.ps1), sans droits administrateur,
# sans fenêtre et sans rien modifier sur le PC. Plan Rust, 3.7 et étape 1a (point D) :
#   1. la DLL livrée (Nevermind\NevermindPont.dll, construite au besoin) : sans le runtime C à part (VCRUNTIME), sans la
#      demande d'essai, chargée par son chemin complet comme le fait Nevermind ;
#   2. cas\aide\canal.json passé par Nevermind : Get-SuffixeCanal (la DLL), puis le nom composé par AideClient (natif.cs) ;
#   3. cas\aide\fermeture.json passé par Nevermind : Get-RefusFermeture, avec les tables que donnent memoire.ps1 et
#      aide-client.ps1 ; la liste d'un coup (page RAM) donne la même chose qu'un par un ;
#   4. contre-épreuve : un cas faux glissé dans chaque corpus fait échouer la comparaison ;
#   5. DLL absente, ou fichier qui n'est pas une DLL : Nevermind continue, rien n'est fermable, pas de nom de canal (un nom
#      qu'aucune Aide n'écoute) ; dans un PowerShell à part ;
#   6. une erreur Rust (panique) dans la DLL d'essai, dans un PowerShell à part : rattrapée, PowerShell continue et la DLL
#      répond encore ;
#   7. Nevermind ne garde plus de copie de la règle ni du nom du canal, et ne fait plus lui-même les actions de l'Aide
#      (sauf pendant la désinstallation).
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File outils\tester-pont.ps1
#
# Code de sortie : 0 si tout passe, 1 sinon.
param([switch]$Enfant, [string]$Essai, [string]$Dll)
$ErrorActionPreference = 'Stop'
$racine = Split-Path $PSScriptRoot -Parent
$modules = Join-Path $racine 'Nevermind\fichiers\modules'

# ---------------------------------------------------------------------------------------------------------------------
# Les essais faits dans un PowerShell à part (la DLL ne se décharge pas d'un processus : un processus par essai)
# ---------------------------------------------------------------------------------------------------------------------
if ($Enfant) {
    . (Join-Path $modules 'pont.ps1')
    $script:Journal = New-Object System.Collections.Generic.List[string]
    function Write-Log([string]$m) { $script:Journal.Add($m) }
    $raison = Initialize-Pont $Dll
    $ctx = @{ Antivirus = @(); Windows = 'C:\Windows'; ServicesPid = [Collections.Generic.HashSet[int]]::new(); Reference = @{ Id = 9000; Session = 1; Compte = 'S-1-5-21-1'; Eleve = 0 } }
    $fiche = @{ Id = 800; Chemin = 'C:\Jeux\a.exe'; Session = 1; Compte = 'S-1-5-21-1'; Protection = 0; Eleve = 0 }
    $r = [ordered]@{ Raison = $raison; Pret = Test-Pont }
    if ($Essai -eq 'panique') {
        $r.Avant = Get-SuffixeCanal 'C:\Program Files\Mindset\Programmes\Nevermind'
        $r.Panique = try { [void](Invoke-Pont @{ action = 'essai.panique' }); 'pas d''erreur' } catch { $_.Exception.Message }
        $n = 0; for ($i = 0; $i -lt 200; $i++) { try { [void](Invoke-Pont @{ action = 'essai.panique' }) } catch { $n++ } }
        $r.Paniques = $n
        $r.Apres = Get-SuffixeCanal 'C:\Program Files\Mindset\Programmes\Nevermind'
        $r.Fermeture = Get-RefusFermeture $ctx $fiche
    } else {
        $r.Refus = Get-RefusFermeture $ctx $fiche
        $r.Liste = (Get-RefusFermetureListe $ctx @($fiche, $fiche)) -join '|'
        $r.Suffixe = Get-SuffixeCanal 'C:\Program Files\Mindset\Programmes\Nevermind'
        $r.Programmes = (Get-ProgrammesCommandes @('%ProgramFiles%\a\a.exe', 'b')).Count
        $d = Get-DossiersNettoyage
        $r.Dossiers = @($d.wu).Count + @($d.do).Count + @($d.dump).Count
        # Le client de l'Aide (aide-client.ps1) avec ce nom inconnu : un canal qu'aucune Aide n'écoute
        Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $modules 'natif.cs'), [Text.Encoding]::UTF8))
        $ModulesDir = $modules
        function Get-AppRoot { 'C:\Program Files\Mindset\Programmes\Nevermind' }
        $texte = [IO.File]::ReadAllText((Join-Path $modules 'aide-client.ps1'), [Text.Encoding]::UTF8)
        $debut = $texte.IndexOf('# Le nom de son canal vient de NevermindPont.dll'); $fin = $texte.IndexOf('[AideClient]::Suffixe = $AideSuffixe')
        if ($debut -lt 0 -or $fin -lt $debut) { $r.Canal = 'bloc du nom du canal introuvable dans aide-client.ps1' }
        else {
            . ([scriptblock]::Create($texte.Substring($debut, $fin - $debut + '[AideClient]::Suffixe = $AideSuffixe'.Length)))
            $r.Canal = [AideClient]::Nom()
            $r.Joignable = [AideClient]::Joignable(1)
        }
    }
    $r.Journal = @($script:Journal) -join ' / '
    $r.Vivant = $true
    ConvertTo-Json -InputObject $r -Compress
    exit 0
}

$echecs = New-Object System.Collections.Generic.List[string]
$nbTests = 0; $nbCorpus = 0
function Test-Ok([bool]$Condition, [string]$Message) { $script:nbTests++; if (-not $Condition) { $script:echecs.Add($Message) } }
. (Join-Path $PSScriptRoot 'lire-cas.ps1')
. (Join-Path $PSScriptRoot 'pont-essai.ps1')
$psExe = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
# Un essai dans un PowerShell à part : sa réponse (JSON), ou son code de sortie s'il s'est arrêté
function Invoke-Enfant([string]$Essai, [string]$Chemin) {
    $o = @(& $psExe -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath -Enfant -Essai $Essai -Dll $Chemin 2>&1)
    $code = $LASTEXITCODE
    $j = $o | Where-Object { "$_" -like '{*' } | Select-Object -Last 1
    if ($code -ne 0 -or -not $j) { return [pscustomobject]@{ Vivant = $false; Code = $code; Sortie = ($o -join ' ') } }
    "$j" | ConvertFrom-Json
}
$tmp = Join-Path ([IO.Path]::GetTempPath()) "nevermind-pont-$PID"
[void][IO.Directory]::CreateDirectory($tmp)

try {
    # -----------------------------------------------------------------------------------------------------------------
    # 1. La DLL livrée, chargée comme le fait Nevermind
    # -----------------------------------------------------------------------------------------------------------------
    $dll = Get-PontEssai
    $octets = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($dll))
    Test-Ok ($octets -notmatch '(?i)vcruntime\d+\.dll') 'La DLL a besoin de VCRUNTIME (absent de beaucoup de PC) : construite sans crt-static'
    Test-Ok (-not $octets.Contains('essai.panique')) 'La DLL livrée contient la demande d''essai (essai.panique) : construite avec l''option essai-panique'
    . (Join-Path $modules 'pont.ps1')
    $script:Journal = New-Object System.Collections.Generic.List[string]
    function Write-Log([string]$m) { $script:Journal.Add($m) }
    $raison = Initialize-Pont $dll
    Test-Ok (-not $raison -and (Test-Pont)) "DLL pas chargée : $raison"
    Test-Ok ((Invoke-Pont @{ action = 'version' }).interface -eq $PontInterface) 'Numéro d''interface de la DLL différent de celui de pont.ps1'
    $inconnue = try { [void](Invoke-Pont @{ action = 'essai.panique' }); '' } catch { $_.Exception.Message }
    Test-Ok ($inconnue -match 'demande inconnue') "La DLL livrée accepte la demande d'essai ($inconnue)"
    # Nevermind la charge par son chemin complet, à côté de ses programmes, jamais d'ailleurs
    $texteNevermind = [IO.File]::ReadAllText((Join-Path $racine 'Nevermind\fichiers\Nevermind.ps1'), [Text.Encoding]::UTF8)
    Test-Ok ($texteNevermind -match "Initialize-Pont \(Join-Path \`$\(if \(\(Split-Path \`$AppDir -Leaf\) -eq 'fichiers'\) \{ Split-Path \`$AppDir -Parent \} else \{ \`$AppDir \}\) 'NevermindPont\.dll'\)") 'Nevermind.ps1 ne charge plus la DLL par son chemin complet dans le dossier de l''appli'
    $textePont = [IO.File]::ReadAllText((Join-Path $modules 'pont.ps1'), [Text.Encoding]::UTF8)
    Test-Ok ($textePont -notmatch '\$env:|GetEnvironmentVariable|LoadLibrary|SetDllDirectory') 'pont.ps1 lit une variable d''environnement ou charge une DLL autrement que par son chemin complet'
    Test-Ok ($textePont -match '\[DllImport\(@"\$Dll"') 'pont.ps1 : la déclaration C# ne nomme plus la DLL par son chemin complet'

    # -----------------------------------------------------------------------------------------------------------------
    # 2. Le nom du canal (cas\aide\canal.json) : le suffixe par la DLL, le nom composé par AideClient (natif.cs), comme
    #    Nevermind le fait (aide-client.ps1)
    # -----------------------------------------------------------------------------------------------------------------
    Add-Type -TypeDefinition ([IO.File]::ReadAllText((Join-Path $modules 'natif.cs'), [Text.Encoding]::UTF8))
    $moi = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    # (le nom d'AideClient est celui du compte qui fait tourner le test : le SID du cas est remplacé par le sien)
    function Test-CasCanal($C) {
        $s = Get-SuffixeCanal ([string]$C.entree.dossier)
        [AideClient]::Suffixe = [string]$s
        $attendu = ([string]$C.attendu.canal).Replace([string]$C.entree.sid, $moi)
        $r = Invoke-Pont @{ action = 'canal'; dossier = [string]$C.entree.dossier; sid = [string]$C.entree.sid }
        if ($s -cne [string]$C.attendu.suffixe) { return "suffixe $s (attendu $($C.attendu.suffixe))" }
        if ([AideClient]::Nom() -cne $attendu) { return "AideClient.Nom() donne $([AideClient]::Nom()) (attendu $attendu)" }
        if ([string]$r.canal -cne [string]$C.attendu.canal -or [string]$r.verrou -cne [string]$C.attendu.verrou) { return "la DLL donne $($r.canal), $($r.verrou)" }
        $null
    }
    $casCanal = @((Read-Cas 'aide\canal.json' -Minimum 12).cas)
    foreach ($c in $casCanal) { $nbCorpus++; $e = Test-CasCanal $c; Test-Ok (-not $e) "Canal de l'Aide, cas « $($c.nom) » : $e" }
    $faux = $casCanal[0] | ConvertTo-Json -Depth 10 | ConvertFrom-Json
    $faux.attendu.suffixe = 'ffffffffffff'
    Test-Ok ([bool](Test-CasCanal $faux)) 'Contre-épreuve : un cas faux de canal.json passe quand même'
    Test-Ok (-not (Get-SuffixeCanal '\\serveur\partage\Nevermind')) 'Un dossier qui n''est pas « X:\ » donne quand même un nom de canal'

    # -----------------------------------------------------------------------------------------------------------------
    # 3. La règle « fermable » (cas\aide\fermeture.json), avec les tables que Nevermind donne (Get-AideContexte,
    #    Get-AideReference, Read-AideFiche ; Processus comme memoire.ps1)
    # -----------------------------------------------------------------------------------------------------------------
    $casFermer = Read-Cas 'aide\fermeture.json' -Minimum 47
    function ConvertTo-Table($O) { $t = @{}; if ($O) { foreach ($p in $O.PSObject.Properties) { $t[$p.Name] = $p.Value } }; $t }
    function Get-CasFermeture($C) {
        $x = ConvertTo-Table $casFermer.contexte
        if ($C.entree.contexte) { foreach ($p in $C.entree.contexte.PSObject.Properties) { $x[$p.Name] = $p.Value } }
        $fi = ConvertTo-Table $casFermer.fiche
        if ($C.entree.fiche) { foreach ($p in $C.entree.fiche.PSObject.Properties) { $fi[$p.Name] = $p.Value } }
        $ctx = @{ Antivirus = @($x.antivirus); Windows = [string]$x.windows; Programmes = [string]$x.programmes; ServicesPid = $null; Reference = $null; Processus = @{} }
        if ($null -ne $x.servicesPid) { $ctx.ServicesPid = [Collections.Generic.HashSet[int]]::new(); foreach ($n in $x.servicesPid) { [void]$ctx.ServicesPid.Add([int]$n) } }
        if ($null -ne $x.reference) { $ctx.Reference = @{ Id = $x.reference.id; Session = $x.reference.session; Compte = $x.reference.compte; Eleve = $x.reference.eleve } }
        foreach ($p in @($x.processus)) { if ($p) { $ctx.Processus[[int]$p.pid] = @{ Parent = $p.parent; Chemin = $p.chemin } } }
        @{ Contexte = $ctx; Fiche = @{ Id = $fi.id; Chemin = $fi.chemin; Session = $fi.session; Compte = $fi.compte; Protection = $fi.protection; Eleve = $fi.eleve }; Eleve = [bool]$C.entree.elevePermis }
    }
    function Test-CasFermeture($C) {
        $e = Get-CasFermeture $C
        $r = Get-RefusFermeture $e.Contexte $e.Fiche -ElevePermis:$e.Eleve
        $attendu = if ($C.attendu.fermable) { $null } else { [string]$C.attendu.raison }
        if ([bool]$C.attendu.fermable -ne (-not $r) -or ($r -and $r -cne $attendu)) { return "donne « $r » (attendu « $attendu »)" }
        $null
    }
    $casF = @($casFermer.cas)
    foreach ($c in $casF) { $nbCorpus++; $e = Test-CasFermeture $c; Test-Ok (-not $e) "Règle « fermable », cas « $($c.nom) » : $e" }
    $faux = $casF[0] | ConvertTo-Json -Depth 10 | ConvertFrom-Json
    $faux.attendu.fermable = -not $faux.attendu.fermable; $faux.attendu.raison = 'fait partie de Windows'
    Test-Ok ([bool](Test-CasFermeture $faux)) 'Contre-épreuve : un cas faux de fermeture.json passe quand même'
    $faux2 = $casF | Where-Object { -not $_.attendu.fermable } | Select-Object -First 1 | ConvertTo-Json -Depth 10 | ConvertFrom-Json
    $faux2.attendu.raison = 'une autre raison'
    Test-Ok ([bool](Test-CasFermeture $faux2)) 'Contre-épreuve : une raison fausse dans fermeture.json passe quand même'
    # La liste d'un coup (page RAM, mode jeu) : la même chose qu'un par un, pour les cas qui partagent le contexte du fichier
    $memes = @($casF | Where-Object { -not $_.entree.contexte -and -not $_.entree.elevePermis })
    $liste = Get-RefusFermetureListe (Get-CasFermeture $memes[0]).Contexte @($memes | ForEach-Object { (Get-CasFermeture $_).Fiche })
    $unParUn = @($memes | ForEach-Object { $e = Get-CasFermeture $_; Get-RefusFermeture $e.Contexte $e.Fiche })
    Test-Ok ($memes.Count -ge 10 -and $liste.Count -eq $memes.Count -and (@(for ($i = 0; $i -lt $memes.Count; $i++) { if ([string]$liste[$i] -cne [string]$unParUn[$i]) { $i } }).Count -eq 0)) "Liste d'un coup différente d'un par un ($($memes.Count) cas)"
    # Un « illisible » reste illisible : jamais 0 à la place (Session, Protection, Eleve absents)
    $e = Get-CasFermeture $casF[0]
    $e.Fiche.Protection = $null
    Test-Ok ((Get-RefusFermeture $e.Contexte $e.Fiche) -eq 'est protégé par Windows (antivirus)') 'Protection absente prise pour « non protégé »'
    # Vitesse : la page RAM demande toute la liste d'un coup
    $beaucoup = @(1..400 | ForEach-Object { @{ Id = 10000 + $_; Chemin = "C:\Programmes\Appli$_\a.exe"; Session = 1; Compte = 'S-1'; Protection = 0; Eleve = 0 } })
    $ctxB = @{ Antivirus = @(); Windows = 'C:\Windows'; ServicesPid = [Collections.Generic.HashSet[int]]::new(); Reference = @{ Id = 9; Session = 1; Compte = 'S-1'; Eleve = 0 }; Processus = @{} }
    foreach ($b in $beaucoup) { $ctxB.Processus[$b.Id] = @{ Parent = 9; Chemin = $b.Chemin } }
    $sw = [Diagnostics.Stopwatch]::StartNew(); $rb = Get-RefusFermetureListe $ctxB $beaucoup; $sw.Stop()
    Test-Ok ($rb.Count -eq 400 -and @($rb | Where-Object { $_ -eq 'fait partie de la suite Mindset' }).Count -eq 400) "400 processus lancés par Nevermind : pas tous de la suite ($(@($rb | Select-Object -Unique) -join ', '))"
    Test-Ok ($sw.ElapsedMilliseconds -lt 3000) "La règle sur 400 processus prend $($sw.ElapsedMilliseconds) ms"
    $msListe = $sw.ElapsedMilliseconds
    # Les autres demandes de Nevermind
    $p = Get-ProgrammesCommandes @('%ProgramFiles%\AVX\avx.exe', 'windowsdefender://threat', '"C:\Program Files\B\b.exe" -k x')
    Test-Ok ($p.Count -eq 3 -and $p[0] -eq (Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'AVX\avx.exe') -and $null -eq $p[1] -and $p[2] -eq 'C:\Program Files\B\b.exe') "Programmes des commandes de Windows : $($p -join ' | ')"
    $d = Get-DossiersNettoyage
    $w = [Environment]::GetFolderPath('Windows')
    Test-Ok ($d.wu[0] -eq "$w\SoftwareDistribution\Download" -and $d.do[0] -eq "$w\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache" -and $d.dump[0] -eq "$w\Minidump") "Dossiers du nettoyage : $($d.wu) / $($d.do) / $($d.dump)"
    # Le fil de fond de la page RAM : il charge le code de Get-CodeFondMemoire (memoire.ps1) et trouve la DLL déjà chargée
    $ModulesDir = $modules
    $defFond = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $modules 'memoire.ps1'), [ref]$null, [ref]$null).Find({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Get-CodeFondMemoire' }, $true)
    . ([scriptblock]::Create($defFond.Extent.Text))
    $e = Get-CasFermeture $casF[0]
    $fond = [PowerShell]::Create()
    try {
        [void]$fond.AddScript({ param($Code, $Ctx, $Fiche) . ([scriptblock]::Create($Code)); function Write-Log([string]$m) { }
            @{ Pret = Test-Pont; Refus = Get-RefusFermeture $Ctx $Fiche; Programme = (Get-ProgrammesCommandes @('"C:\A\a.exe"'))[0] } }.ToString()).AddArgument((Get-CodeFondMemoire)).AddArgument($e.Contexte).AddArgument($e.Fiche)
        $rf = @($fond.Invoke())[0]
        Test-Ok ($rf.Pret -and $null -eq $rf.Refus -and $rf.Programme -eq 'C:\A\a.exe') "Fil de fond de la page RAM : la DLL n'y répond pas ($($rf.Pret), $($rf.Refus), $($rf.Programme) ; $(@($fond.Streams.Error) -join ' '))"
    } finally { $fond.Dispose() }

    # -----------------------------------------------------------------------------------------------------------------
    # 5. DLL absente, ou qui n'en est pas une : Nevermind continue, au plus sûr
    # -----------------------------------------------------------------------------------------------------------------
    $fausse = Join-Path $tmp 'NevermindPont.dll'
    [IO.File]::WriteAllText($fausse, 'pas une DLL')
    foreach ($x in @(@('absente', (Join-Path $tmp 'absente\NevermindPont.dll')), @('pas une DLL', $fausse))) {
        $r = Invoke-Enfant 'sans' $x[1]
        Test-Ok ([bool]$r.Vivant) "DLL $($x[0]) : PowerShell s'est arrêté ($($r.Code) : $($r.Sortie))"
        if (-not $r.Vivant) { continue }
        Test-Ok ($r.Raison -and -not $r.Pret) "DLL $($x[0]) : prise pour prête ($($r.Raison))"
        Test-Ok ($r.Refus -eq $PontRefusAbsente -and $r.Liste -eq "$PontRefusAbsente|$PontRefusAbsente") "DLL $($x[0]) : un processus reste fermable ($($r.Refus) / $($r.Liste))"
        Test-Ok (-not $r.Suffixe -and $r.Programmes -eq 2 -and $r.Dossiers -eq 0) "DLL $($x[0]) : un nom de canal ou des dossiers devinés ($($r.Suffixe), $($r.Dossiers))"
        Test-Ok ($r.Canal -eq "Nevermind-Aide-$moi-aucune" -and -not $r.Joignable) "DLL $($x[0]) : le client de l'Aide vise un autre canal que « aucune » ($($r.Canal), joignable $($r.Joignable))"
        Test-Ok ($r.Journal -match 'nom du canal inconnu') "DLL $($x[0]) : rien au journal ($($r.Journal))"
    }

    # -----------------------------------------------------------------------------------------------------------------
    # 6. Une erreur Rust (panique) dans la DLL : rattrapée, PowerShell continue
    # -----------------------------------------------------------------------------------------------------------------
    $dllEssai = Get-PontEssai -Panique
    $r = Invoke-Enfant 'panique' $dllEssai
    Test-Ok ([bool]$r.Vivant) "Panique dans la DLL : PowerShell s'est arrêté ($($r.Code) : $($r.Sortie))"
    if ($r.Vivant) {
        Test-Ok ($r.Pret -and $r.Avant -eq '4a8522ca5e0c') "DLL d'essai pas prête ($($r.Raison))"
        Test-Ok ($r.Panique -match 'erreur interne de la DLL \(panique : panique volontaire') "Panique : pas rapportée comme une erreur de la DLL ($($r.Panique))"
        Test-Ok ($r.Paniques -eq 200) "200 paniques de suite : $($r.Paniques) rattrapées"
        Test-Ok ($r.Apres -eq '4a8522ca5e0c' -and $null -eq $r.Fermeture) "Après les paniques, la DLL ne répond plus juste ($($r.Apres), $($r.Fermeture))"
    }

    # -----------------------------------------------------------------------------------------------------------------
    # 7. Plus de copie de la règle ni du nom du canal dans Nevermind, plus de repli en administrateur
    # -----------------------------------------------------------------------------------------------------------------
    $aideActions = @('Get-RefusFermeture', 'Get-AideSuffixe', 'Test-AideSuite', 'Test-AideCheminSuite', 'Test-AideAntivirus', 'ConvertFrom-AideCheminWindows', 'Invoke-AideAction')
    foreach ($m in Get-ChildItem -LiteralPath $modules -Filter '*.ps1') {
        if ($m.Name -eq 'aide-actions.ps1') { continue }
        $t = [IO.File]::ReadAllText($m.FullName, [Text.Encoding]::UTF8)
        $defs = @($aideActions | Where-Object { $t -match "(?m)^\s*function $_\b" -and -not ($m.Name -eq 'pont.ps1' -and $_ -eq 'Get-RefusFermeture') })
        Test-Ok (-not $defs.Count) "$($m.Name) redéfinit $($defs -join ', ') (la règle et le nom du canal sont dans la DLL)"
        $appels = @('Get-AideSuffixe', 'ConvertFrom-AideCheminWindows', 'Test-AideSuite' | Where-Object { $t -match "\b$_\b" })
        Test-Ok (-not $appels.Count) "$($m.Name) appelle encore $($appels -join ', ') (aide-actions.ps1)"
        if ($m.Name -notin 'aide-client.ps1') { Test-Ok ($t -notmatch "aide-actions\.ps1'|Invoke-AideAction") "$($m.Name) charge encore aide-actions.ps1 ou fait lui-même une action de l'Aide" }
    }
    Test-Ok ($textePont -match "(?s)function Get-RefusFermetureListe.*?action = 'fermeture\.refus'" -and $textePont -notmatch 'WindowsApps|\$AideExesWindows|Test-AideSuite') 'pont.ps1 : la règle n''est plus demandée à la DLL, ou une partie est recopiée en PowerShell'
    Test-Ok ($texteNevermind -notmatch "'aide-actions'") 'Nevermind.ps1 charge encore aide-actions.ps1 au démarrage'
    $client = [IO.File]::ReadAllText((Join-Path $modules 'aide-client.ps1'), [Text.Encoding]::UTF8)
    Test-Ok ($client -match '\$AideTravail = \{' -and $client -match 'if \(-not \$rep -and \$x\.SurPlace -and \$x\.Admin\) \{' -and $client -notmatch 'if \(-not \$rep -and \$x\.Admin\)') 'aide-client.ps1 : Nevermind fait encore lui-même les actions de l''Aide en administrateur (hors désinstallation)'
    Test-Ok ($client -match "\`$AideSuffixe = 'aucune'" -and $client -match '\$AideSuffixe = Get-SuffixeCanal \(Get-AppRoot\)') 'aide-client.ps1 : le nom du canal ne vient plus de la DLL, ou un nom est deviné sans elle'
}
catch {
    $echecs.Add("Arrêt inattendu : $($_.Exception.Message) (ligne $($_.InvocationInfo.ScriptLineNumber))")
}
finally { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }

if ($echecs.Count) {
    Write-Host "$($echecs.Count) échec(s) sur $nbTests contrôles :" -ForegroundColor Red
    $echecs | ForEach-Object { Write-Host "  - $_" -ForegroundColor Red }
    exit 1
}
Write-Host "Pont (NevermindPont.dll) : les $nbTests contrôles sont bons, dont $nbCorpus cas du corpus passés par Nevermind ($msListe ms pour 400 processus d'un coup)." -ForegroundColor Green
exit 0
