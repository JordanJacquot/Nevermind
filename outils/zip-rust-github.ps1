# Nevermind en Rust : le Nevermind.zip d'une publication (outils\publier.ps1 -Rust). Il n'est jamais construit sur ce PC :
# il vient de la construction GitHub de CE commit (.github\workflows\release.yml, job « Nevermind en Rust », genre rust),
# et n'est accepté que si son attestation GitHub dit qu'il a été construit par ce dépôt, par release.yml, à partir de ce
# commit (vérifier seulement qu'une attestation existe ne suffit pas). Sinon : erreur, et la publication est annulée
# avant toute signature. Sans ça, la signature de Jordan couvrirait un fichier qu'on aurait pu remplacer en route.
# Avant, les tests de GitHub de ce commit (.github\workflows\tests.yml : tests PowerShell et Rust, contrat de la suite avec le vrai
# Mindset, mise à jour depuis 1.15.6 sans rien perdre, cargo deny...) doivent avoir réussi.
#
#   .\outils\zip-rust-github.ps1             le chemin du Nevermind.zip vérifié ; si GitHub ne l'a pas encore construit pour
#                                            ce commit, la construction est lancée (le commit doit déjà être envoyé),
#                                            puis attendue (60 minutes au plus), comme les tests
#   .\outils\zip-rust-github.ps1 -Essai      rien n'est lancé : seulement une construction déjà faite pour ce commit ;
#                                            s'il n'y en a pas, rien ($null)
#   -SansTest                                n'attend pas les tests de GitHub (urgence seulement)
param([switch]$Essai, [switch]$SansTest)
$ErrorActionPreference = 'Stop'
$Depot = 'JordanJacquot/Nevermind'
$Workflow = '.github/workflows/release.yml'
$Artefact = 'Nevermind-rust-zip'
$racine = Split-Path $PSScriptRoot -Parent

# gh et git : seul leur code de sortie compte (leurs messages normaux passent par la sortie d'erreur)
function Invoke-Outil([string]$Exe, [string[]]$Arguments) {
    $ErrorActionPreference = 'Continue'
    $s = @(& $Exe @Arguments 2>&1 | ForEach-Object { "$_" })
    if ($LASTEXITCODE) { throw "Publication annulée : $Exe $($Arguments -join ' ') a échoué ($(($s | Select-Object -Last 3) -join ' '))" }
    $s
}

$commit = ((Invoke-Outil git @('-C', $racine, 'rev-parse', 'HEAD')) -join '').Trim()
if ($commit -notmatch '^[0-9a-f]{40}$') { throw "Publication annulée : commit illisible ($commit)." }

# Les exécutions d'un workflow pour ce commit (envoi ou lancement à la main), les plus récentes d'abord
function Get-Executions([string]$Fichier) {
    $l = (Invoke-Outil gh @('run', 'list', '--repo', $Depot, '--workflow', $Fichier, '--commit', $commit, '--limit', '20', '--json', 'databaseId,headSha,event,status,conclusion,createdAt,url')) -join "`n" | ConvertFrom-Json
    @($l | Where-Object { $_.headSha -eq $commit -and $_.event -in 'push', 'workflow_dispatch' } | Sort-Object createdAt -Descending)
}
# Une exécution réussie de release.yml a-t-elle déposé le zip en Rust ? (une exécution « powershell » réussit sans lui)
function Test-Artefact([string]$Id) {
    $noms = @(Invoke-Outil gh @('api', "repos/$Depot/actions/runs/$Id/artifacts", '--jq', '.artifacts[] | select(.expired == false) | .name'))
    $noms -ccontains $Artefact
}

# 1. Les tests de GitHub de ce commit (60 minutes au plus)
if (-not $SansTest) {
    $limite = (Get-Date).AddMinutes(60)
    while ($true) {
        $t = @(Get-Executions 'tests.yml')
        if (@($t | Where-Object { $_.conclusion -eq 'success' }).Count) { Write-Host "Tests de GitHub du commit $commit : réussis ($(@($t | Where-Object { $_.conclusion -eq 'success' })[0].url))."; break }
        $enCours = @($t | Where-Object { $_.status -ne 'completed' })
        if ($Essai) {
            $etat = if ($enCours.Count) { 'en cours' } elseif ($t.Count) { "ratés ($($t[0].conclusion))" } else { 'pas lancés (commit pas envoyé ?)' }
            Write-Host "Essai : tests de GitHub du commit $commit $etat. Une vraie publication attendrait qu'ils réussissent." -ForegroundColor Yellow
            break
        }
        if ($t.Count -and -not $enCours.Count) { throw "Publication annulée : les tests de GitHub du commit $commit ont échoué ($($t[0].conclusion)) : $($t[0].url)" }
        if ((Get-Date) -gt $limite) { throw "Publication annulée : les tests de GitHub du commit $commit n'ont pas fini en 60 minutes." }
        Start-Sleep -Seconds 15
    }
}

# 2. La construction de Nevermind en Rust (release.yml, genre rust) de ce commit
function Find-Bonne {
    foreach ($r in @(Get-Executions 'release.yml' | Where-Object { $_.conclusion -eq 'success' })) {
        if (Test-Artefact ([string]$r.databaseId)) { return $r }
    }
    $null
}
$bonne = Find-Bonne
if (-not $bonne) {
    if ($Essai) {
        Write-Host "Essai : GitHub n'a pas encore construit Nevermind en Rust pour le commit $commit." -ForegroundColor Yellow
        return $null
    }
    $branche = ((Invoke-Outil git @('-C', $racine, 'rev-parse', '--abbrev-ref', 'HEAD')) -join '').Trim()
    if ($branche -eq 'HEAD') { throw 'Publication annulée : aucune branche en cours (HEAD détaché).' }
    if (-not @(Get-Executions 'release.yml' | Where-Object { $_.status -ne 'completed' }).Count) {
        Write-Host "Construction de Nevermind en Rust par GitHub (release.yml, genre rust) pour le commit $commit..."
        $null = Invoke-Outil gh @('workflow', 'run', 'release.yml', '--repo', $Depot, '--ref', $branche, '-f', 'genre=rust')
    }
    $depuis = Get-Date
    $limite = $depuis.AddMinutes(60)
    $toutFini = $null
    while (-not $bonne) {
        if ((Get-Date) -gt $limite) { throw "Publication annulée : GitHub n'a pas fini de construire Nevermind en Rust pour le commit $commit en 60 minutes." }
        Start-Sleep -Seconds 15
        $bonne = Find-Bonne
        $toutes = @(Get-Executions 'release.yml')
        # (une minute de grâce : la construction lancée à la main met quelques secondes à apparaître ; puis deux minutes
        # après la fin de toutes les constructions : GitHub montre leurs fichiers un peu après les avoir dites finies.
        # Avant : 0.91.0 annulé alors que la construction avait réussi et déposé le zip.)
        $finies = -not @($toutes | Where-Object { $_.status -ne 'completed' }).Count
        if (-not $finies) { $toutFini = $null } elseif (-not $toutFini) { $toutFini = Get-Date }
        if (-not $bonne -and ((Get-Date) - $depuis).TotalSeconds -gt 60 -and $finies -and ((Get-Date) - $toutFini).TotalSeconds -gt 120) {
            $derniere = if ($toutes.Count) { "$($toutes[0].conclusion) : $($toutes[0].url)" } else { 'aucune construction trouvée' }
            throw "Publication annulée : la construction GitHub de Nevermind en Rust n'a rien donné ($derniere)."
        }
    }
}

# 3. Le zip de cette construction : exactement Nevermind.zip, attesté (ce dépôt, release.yml, ce commit)
$dossier = Join-Path ([IO.Path]::GetTempPath()) "nevermind-rust-$PID"
if (Test-Path -LiteralPath $dossier) { Remove-Item -LiteralPath $dossier -Recurse -Force }
$null = Invoke-Outil gh @('run', 'download', [string]$bonne.databaseId, '--repo', $Depot, '--name', $Artefact, '--dir', $dossier)
$recus = @(Get-ChildItem -LiteralPath $dossier -Recurse -File | ForEach-Object { $_.FullName.Substring($dossier.Length + 1) })
if (($recus -join '|') -cne 'Nevermind.zip') { throw "Publication annulée : l'artefact $Artefact de la construction $($bonne.url) contient « $($recus -join ', ') » au lieu de « Nevermind.zip »." }
$zip = Join-Path $dossier 'Nevermind.zip'
$ErrorActionPreference = 'Continue'
$verif = @(& gh attestation verify $zip --repo $Depot --signer-workflow "$Depot/$Workflow" --source-digest $commit --signer-digest $commit --deny-self-hosted-runners 2>&1 | ForEach-Object { "$_" })
$code = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
if ($code) { throw "Publication annulée : Nevermind.zip téléchargé n'est pas attesté comme construit par $Workflow de $Depot à partir du commit $commit ($(($verif | Select-Object -Last 3) -join ' '))." }
Write-Host "Nevermind.zip (Rust) : construction $($bonne.url), attestation vérifiée (dépôt, release.yml, commit $commit), SHA-256 $((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash)" -ForegroundColor Green
$zip
