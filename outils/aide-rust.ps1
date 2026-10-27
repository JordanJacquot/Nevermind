# Les programmes en Rust de Nevermind pour une publication (outils\publier.ps1, passage en Rust, étape 1a) :
# NevermindAide.exe (rust\nevermind-aide) et NevermindPont.dll (rust\nevermind-pont), chacun seulement si son dossier est
# dans le commit publié. Ils ne sont jamais construits sur ce PC : ils viennent de la construction GitHub de CE commit
# (.github\workflows\release.yml, job « Programmes en Rust »), et chacun n'est accepté que si son attestation GitHub dit
# qu'il a été construit par ce dépôt, par release.yml, à partir de ce commit (vérifier seulement qu'une attestation
# existe ne suffit pas), et s'il n'a besoin d'aucun runtime C à part (outils\verifier-imports.ps1). Sinon : erreur, et la
# publication est annulée avant toute signature. Sans ça, la signature de Jordan couvrirait un fichier qu'on aurait pu
# remplacer en route.
#
#   .\outils\aide-rust.ps1           une table nom -> chemin des fichiers vérifiés ; si GitHub ne les a pas encore
#                                    construits pour ce commit, le commit est envoyé et la construction lancée, puis attendue
#   .\outils\aide-rust.ps1 -Essai    rien n'est envoyé ni lancé : seulement une construction déjà faite pour ce commit ;
#                                    s'il n'y en a pas, rien (l'essai de publication garde ce que construire.ps1 a fait)
param([switch]$Essai)
$ErrorActionPreference = 'Stop'
$Depot = 'JordanJacquot/Nevermind'
$Workflow = '.github/workflows/release.yml'
$NomJob = 'Programmes en Rust'   # (début du nom du job dans release.yml)
$Artefact = 'Nevermind-rust'
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
# Ce que ce commit construit en Rust (son dossier dans le commit, pas seulement sur le disque)
function Test-DansCommit([string]$Chemin) {
    $ErrorActionPreference = 'Continue'
    & git -C $racine cat-file -e "$($commit):$Chemin" 2>$null
    $LASTEXITCODE -eq 0
}
$attendus = @(if (Test-DansCommit 'rust/nevermind-aide/Cargo.toml') { 'NevermindAide.exe' }
              if (Test-DansCommit 'rust/nevermind-pont/Cargo.toml') { 'NevermindPont.dll' })
if (-not $attendus.Count) { throw "Publication annulée : ni rust\nevermind-aide ni rust\nevermind-pont dans le commit $commit." }
$noms = $attendus -join ' et '

# Les constructions GitHub de ce commit qui ont le job Rust : leur numéro, l'état et le résultat de ce job
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
            Write-Host "Essai : GitHub n'a pas encore construit $noms (Rust) pour le commit $commit ; l'essai garde ce que construire.ps1 a fait." -ForegroundColor Yellow
            return
        }
        # Le commit sur GitHub (il y serait envoyé de toute façon par la publication), puis sa construction
        $branche = ((Invoke-Outil git @('-C', $racine, 'rev-parse', '--abbrev-ref', 'HEAD')) -join '').Trim()
        if ($branche -eq 'HEAD') { throw 'Publication annulée : aucune branche en cours (HEAD détaché).' }
        Write-Host "Envoi du commit sur GitHub et construction de $noms (Rust) par release.yml..."
        $null = Invoke-Outil git @('-C', $racine, 'push', 'origin', 'HEAD')
        $null = Invoke-Outil gh @('workflow', 'run', 'release.yml', '--repo', $Depot, '--ref', $branche)
    }
    # Attendre la fin du job Rust (40 minutes au plus)
    $limite = (Get-Date).AddMinutes(40)
    while (-not $bonne) {
        if ((Get-Date) -gt $limite) { throw "Publication annulée : GitHub n'a pas fini de construire $noms (Rust) pour le commit $commit en 40 minutes." }
        Start-Sleep -Seconds 15
        $c = @(Find-Constructions)
        $bonne = @($c | Where-Object { $_.Resultat -eq 'success' }) | Select-Object -First 1
        $ratee = @($c | Where-Object { $_.Etat -eq 'completed' -and $_.Resultat -ne 'success' }) | Select-Object -First 1
        if (-not $bonne -and $ratee -and -not @($c | Where-Object { $_.Etat -ne 'completed' }).Count) {
            throw "Publication annulée : la construction GitHub de $noms (Rust) a échoué ($($ratee.Resultat)) : $($ratee.Url)"
        }
    }
}

# Les fichiers de cette construction : exactement ceux attendus, chacun attesté (ce dépôt, release.yml, ce commit, source
# et fichier de construction) et sans runtime C à part
$dossier = Join-Path ([IO.Path]::GetTempPath()) "nevermind-rust-$PID"
if (Test-Path -LiteralPath $dossier) { Remove-Item -LiteralPath $dossier -Recurse -Force }
$null = Invoke-Outil gh @('run', 'download', $bonne.Id, '--repo', $Depot, '--name', $Artefact, '--dir', $dossier)
$recus = @(Get-ChildItem -LiteralPath $dossier -Recurse -File | ForEach-Object { $_.FullName.Substring($dossier.Length + 1) } | Sort-Object)
if (($recus -join '|') -ne (@($attendus | Sort-Object) -join '|')) {
    throw "Publication annulée : l'artefact $Artefact de la construction $($bonne.Url) contient « $($recus -join ', ') » au lieu de « $($attendus -join ', ') »."
}
$verifies = [ordered]@{}
foreach ($n in $attendus) {
    $f = Join-Path $dossier $n
    $ErrorActionPreference = 'Continue'
    $verif = @(& gh attestation verify $f --repo $Depot --signer-workflow "$Depot/$Workflow" --source-digest $commit --signer-digest $commit --deny-self-hosted-runners 2>&1 | ForEach-Object { "$_" })
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($code) { throw "Publication annulée : $n téléchargé n'est pas attesté comme construit par $Workflow de $Depot à partir du commit $commit ($(($verif | Select-Object -Last 3) -join ' '))." }
    & (Join-Path $PSScriptRoot 'verifier-imports.ps1') -Fichiers $f | Out-Host
    if ($LASTEXITCODE) { throw "Publication annulée : $n a besoin d'un runtime C à installer à part." }
    Write-Host "$n (Rust) : construction $($bonne.Url), attestation vérifiée (dépôt, release.yml, commit $commit), SHA-256 $((Get-FileHash -LiteralPath $f -Algorithm SHA256).Hash)"
    $verifies[$n] = $f
}
$verifies
