# NevermindAide.exe en Rust pour une publication (outils\publier.ps1, passage en Rust, étape 1a). Il n'est jamais
# construit sur ce PC : il vient de la construction GitHub de CE commit (.github\workflows\release.yml, job « rust »),
# et il n'est accepté que si son attestation GitHub dit qu'il a été construit par ce dépôt, par release.yml, à partir de
# ce commit (vérifier seulement qu'une attestation existe ne suffit pas). Sinon : erreur, et la publication est annulée
# avant toute signature. Sans ça, la signature de Jordan couvrirait un exe qu'on aurait pu remplacer en route.
#
#   .\outils\aide-rust.ps1           le chemin de l'exe vérifié ; si GitHub ne l'a pas encore construit pour ce commit,
#                                    le commit est envoyé et la construction lancée, puis attendue
#   .\outils\aide-rust.ps1 -Essai    rien n'est envoyé ni lancé : seulement une construction déjà faite pour ce commit ;
#                                    s'il n'y en a pas, rien (l'essai de publication garde l'Aide de construire.ps1)
param([switch]$Essai)
$ErrorActionPreference = 'Stop'
$Depot = 'JordanJacquot/Nevermind'
$Workflow = '.github/workflows/release.yml'
$NomJob = 'NevermindAide.exe en Rust'   # (début du nom du job dans release.yml)
$Artefact = 'NevermindAide-rust'
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
# rust\nevermind-aide doit être dans ce commit, pas seulement sur le disque
$null = Invoke-Outil git @('-C', $racine, 'cat-file', '-e', "$($commit):rust/nevermind-aide/Cargo.toml")

# Les constructions GitHub de ce commit qui ont un job Rust : leur numéro, l'état et le résultat de ce job
function Find-Constructions {
    $runs = (Invoke-Outil gh @('run', 'list', '--repo', $Depot, '--workflow', 'release.yml', '--commit', $commit, '--limit', '20', '--json', 'databaseId,headSha,event,createdAt')) -join "`n" | ConvertFrom-Json
    foreach ($r in @($runs | Where-Object { $_.headSha -eq $commit -and $_.event -in 'push', 'workflow_dispatch' } | Sort-Object createdAt -Descending)) {
        $v = (Invoke-Outil gh @('run', 'view', [string]$r.databaseId, '--repo', $Depot, '--json', 'jobs,url')) -join "`n" | ConvertFrom-Json
        $j = @($v.jobs | Where-Object { [string]$_.name -like "$NomJob*" }) | Select-Object -First 1
        if ($j) { [pscustomobject]@{ Id = [string]$r.databaseId; Etat = [string]$j.status; Resultat = [string]$j.conclusion; Url = [string]$v.url } }
    }
}

$c = @(Find-Constructions)
$bonne = @($c | Where-Object { $_.Resultat -eq 'success' }) | Select-Object -First 1
if (-not $bonne) {
    if (-not @($c | Where-Object { $_.Etat -ne 'completed' }).Count) {
        if ($Essai) {
            Write-Host "Essai : GitHub n'a pas encore construit NevermindAide.exe (Rust) pour le commit $commit ; l'essai garde celui de construire.ps1." -ForegroundColor Yellow
            return
        }
        # Le commit sur GitHub (il y serait envoyé de toute façon par la publication), puis sa construction
        $branche = ((Invoke-Outil git @('-C', $racine, 'rev-parse', '--abbrev-ref', 'HEAD')) -join '').Trim()
        if ($branche -eq 'HEAD') { throw 'Publication annulée : aucune branche en cours (HEAD détaché).' }
        Write-Host "Envoi du commit sur GitHub et construction de NevermindAide.exe (Rust) par release.yml..."
        $null = Invoke-Outil git @('-C', $racine, 'push', 'origin', 'HEAD')
        $null = Invoke-Outil gh @('workflow', 'run', 'release.yml', '--repo', $Depot, '--ref', $branche)
    }
    # Attendre la fin du job Rust (40 minutes au plus)
    $limite = (Get-Date).AddMinutes(40)
    while (-not $bonne) {
        if ((Get-Date) -gt $limite) { throw "Publication annulée : GitHub n'a pas fini de construire NevermindAide.exe (Rust) pour le commit $commit en 40 minutes." }
        Start-Sleep -Seconds 15
        $c = @(Find-Constructions)
        $bonne = @($c | Where-Object { $_.Resultat -eq 'success' }) | Select-Object -First 1
        $ratee = @($c | Where-Object { $_.Etat -eq 'completed' -and $_.Resultat -ne 'success' }) | Select-Object -First 1
        if (-not $bonne -and $ratee -and -not @($c | Where-Object { $_.Etat -ne 'completed' }).Count) {
            throw "Publication annulée : la construction GitHub de NevermindAide.exe (Rust) a échoué ($($ratee.Resultat)) : $($ratee.Url)"
        }
    }
}

# L'exe de cette construction, puis son attestation : ce dépôt, release.yml, ce commit (source et fichier de construction)
$dossier = Join-Path ([IO.Path]::GetTempPath()) "nevermind-aide-rust-$PID"
if (Test-Path -LiteralPath $dossier) { Remove-Item -LiteralPath $dossier -Recurse -Force }
$null = Invoke-Outil gh @('run', 'download', $bonne.Id, '--repo', $Depot, '--name', $Artefact, '--dir', $dossier)
$exe = Join-Path $dossier 'NevermindAide.exe'
if (-not (Test-Path -LiteralPath $exe -PathType Leaf) -or @(Get-ChildItem -LiteralPath $dossier -Recurse -File).Count -ne 1) {
    throw "Publication annulée : l'artefact $Artefact de la construction $($bonne.Url) ne contient pas seulement NevermindAide.exe."
}
$ErrorActionPreference = 'Continue'
$verif = @(& gh attestation verify $exe --repo $Depot --signer-workflow "$Depot/$Workflow" --source-digest $commit --signer-digest $commit --deny-self-hosted-runners 2>&1 | ForEach-Object { "$_" })
$code = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
if ($code) {
    throw "Publication annulée : NevermindAide.exe téléchargé n'est pas attesté comme construit par $Workflow de $Depot à partir du commit $commit ($(($verif | Select-Object -Last 3) -join ' '))."
}
Write-Host "NevermindAide.exe (Rust) : construction $($bonne.Url), attestation vérifiée (dépôt, release.yml, commit $commit), SHA-256 $((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash)"
$exe
