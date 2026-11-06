# Nevermind en Rust : l'exe livré ne connaît pas la construction d'essai (option cargo « essai ») ni le mode test de la
# version PowerShell : ni NEVERMIND_TEST (rien ne touche le PC), ni NEVERMIND_ESSAI* (dossier de données d'essai...),
# ni NEVERMIND_CAS (le corpus des tests de l'Aide), ni la capture de la fenêtre ou les actions pilotées de l'essai
# (capture.demande, actions.demande, comme Mindcare). Sans ça, une variable d'environnement posée par n'importe quel
# programme changerait ce que fait Nevermind chez l'utilisateur (docs\regles-tauri.md du dépôt Mindset : aucune variable
# ni option cachée dans l'exe livré). Le test cherche ces noms dans l'exe, en texte simple et en UTF-16, sans rien lancer.
#
#   & '.\outils\verifier-sans-essai.ps1' -Exe <Mindset.exe>
#
# Code 0 : aucun nom trouvé ; 1 : un nom trouvé (ou exe introuvable). Contre-épreuve : sur un exe qui contient l'un de
# ces noms, il doit rendre 1.
param([Parameter(Mandatory = $true)][string]$Exe)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) { Write-Host "Introuvable : $Exe"; exit 1 }
$octets = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $Exe).Path)
# (chaque octet tel quel : les noms cherchés sont en ASCII)
$simple = [Text.Encoding]::GetEncoding(28591).GetString($octets)
$noms = 'NEVERMIND_TEST', 'NEVERMIND_ESSAI', 'NEVERMIND_CAS', 'NEVERMIND_DEV', 'capture.demande', 'actions.demande', 'essai-sans-depot'
$trouves = @()
foreach ($n in $noms) {
    $large = ($n.ToCharArray() | ForEach-Object { "$_`0" }) -join ''
    if ($simple.Contains($n) -or $simple.Contains($large)) { $trouves += $n }
}
if ($trouves.Count) { Write-Host "$Exe contient la construction d'essai : $($trouves -join ', ')"; exit 1 }
Write-Host "$Exe : aucune trace de la construction d'essai ($($noms -join ', '))."
exit 0
