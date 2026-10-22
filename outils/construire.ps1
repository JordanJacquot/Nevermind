# Construit le dossier de Nevermind prêt à partager, puis OptiGame.zip. Le zip et son dossier gardent le nom
# « OptiGame » : les versions déjà installées cherchent ce nom pour se mettre à jour.
#
#   OptiGame\
#     Nevermind.exe                             <- à lancer (icône Nevermind)
#     Désinstaller Nevermind.exe                <- désinstallation propre
#     NevermindAide.exe                         <- la partie administrateur (lancée par la tâche que crée Mindset)
#     LISEZMOI.txt
#     fichiers\OptiGame.ps1, OptiGame.ico, lanceurs de secours (.bat)
#     fichiers\modules\                    <- le code découpé par partie

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$app     = Join-Path $racine 'OptiGame'
$fich    = Join-Path $app 'fichiers'
$icones  = Join-Path $PSScriptRoot 'icones'
$csc     = Join-Path $env:windir 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
$source  = Join-Path $PSScriptRoot 'lanceur.cs'
$manif   = Join-Path $PSScriptRoot 'lanceur.manifest'
# Moteur PowerShell hébergé dans Nevermind.exe (celui de Windows, toujours présent)
$sma     = @(Get-ChildItem (Join-Path $env:windir 'Microsoft.NET\assembly\GAC_MSIL\System.Management.Automation') -Recurse -Filter 'System.Management.Automation.dll' | Select-Object -First 1)[0].FullName

& (Join-Path $PSScriptRoot 'icone.ps1') -OutDir $icones | Out-Null
New-Item -ItemType Directory -Force -Path $fich | Out-Null
# Fichiers d'autres auteurs : chacun doit avoir l'empreinte et la signature notées dans son SOURCES.md, sinon rien
# n'est construit (un fichier remplacé par erreur ou par un tiers ne part jamais chez les utilisateurs)
$tiers = @(
    ,@('lib\webview2\\Microsoft.Web.WebView2.Core.dll', 'D60E6E94245078CB56E44EBD5F360DC04BF18E460136BE93BFC4CFCA43B98505', 'Microsoft Corporation')
    ,@('lib\webview2\\Microsoft.Web.WebView2.Wpf.dll', 'F05A06BBC03AFCB83DC98168B8D343D9015B2A533EDBFAC90C52D88894B653E4', 'Microsoft Corporation')
    ,@('lib\webview2\\WebView2Loader.dll', '3426DCC55FDFB8B5E7AC623CF33B4CCF283FBF68A0C5A65BDDFDB4D749A8ABEE', 'Microsoft Corporation')
    ,@('outils-tiers\PresentMon.exe', 'B2A706BC6AD475749E3B7E3409263AA1E6906D45BDCF993F6DBC0F660188F1AF', 'Intel Corporation')
)
foreach ($t in $tiers) {
    $f = Join-Path $fich $t[0]
    if (-not (Test-Path -LiteralPath $f)) { throw "Fichier tiers absent : $($t[0])" }
    $h = (Get-FileHash -LiteralPath $f -Algorithm SHA256).Hash
    if ($h -ne $t[1]) { throw "Fichier tiers modifié : $($t[0]) (empreinte $h, attendue $($t[1]) : voir son SOURCES.md)" }
    $s = Get-AuthenticodeSignature -LiteralPath $f
    if ($s.Status -ne 'Valid' -or $s.SignerCertificate.Subject -notmatch "CN=$([regex]::Escape($t[2]))(,|$)") { throw "Fichier tiers mal signé : $($t[0]) ($($s.Status), $($s.SignerCertificate.Subject))" }
}
Copy-Item (Join-Path $icones 'OptiGame.ico') (Join-Path $fich 'OptiGame.ico') -Force

function Build-Exe([string]$Out, [string]$Icon, [string]$Define) {
    $opts = @('/nologo', '/target:winexe', '/optimize+', '/platform:anycpu', "/win32icon:$Icon", "/win32manifest:$manif",
              '/reference:System.Windows.Forms.dll', '/reference:System.Web.Extensions.dll', "/reference:$sma", "/out:$Out")
    if ($Define) { $opts += "/define:$Define" }
    & $csc @opts $source
    if ($LASTEXITCODE) { throw "Échec de compilation de $Out" }
}
Build-Exe (Join-Path $app 'Nevermind.exe') (Join-Path $icones 'OptiGame.ico') ''
Build-Exe (Join-Path $app 'Désinstaller Nevermind.exe') (Join-Path $icones 'OptiGame-desinstaller.ico') 'UNINSTALL'
Build-Exe (Join-Path $app 'NevermindAide.exe') (Join-Path $icones 'OptiGame.ico') 'AIDE'

foreach ($old in 'OptiGame.exe', 'Désinstaller OptiGame.exe', 'Nexo.exe', 'Désinstaller Nexo.exe') { $p = Join-Path $app $old; if (Test-Path -LiteralPath $p) { [IO.File]::Delete($p) } }
$zip = Join-Path $racine 'OptiGame.zip'
Compress-Archive -Path $app -DestinationPath $zip -Force
"OK: $zip"
