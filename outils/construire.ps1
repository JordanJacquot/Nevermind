# Construit le dossier de Nevermind prêt à partager, puis son zip, Nevermind.zip (outils\zips.ps1).
#
#   Nevermind\
#     Mindset.exe                               <- le programme qui fait tourner Nevermind (icône Nevermind) ; toutes les
#                                                  applis de la suite tournent dans un « Mindset.exe » de leur dossier
#     Nevermind.exe                             <- petit relais : lance Mindset.exe de ce dossier (anciens raccourcis)
#     Désinstaller Nevermind.exe                <- désinstallation propre
#     NevermindAide.exe                         <- la partie administrateur (lancée par la tâche que crée Mindset)
#     LISEZMOI.txt
#     fichiers\Nevermind.ps1, Nevermind.ico, lanceurs de secours (.bat)
#     fichiers\modules\                    <- le code découpé par partie

$ErrorActionPreference = 'Stop'
$racine  = Split-Path $PSScriptRoot -Parent
$app     = Join-Path $racine 'Nevermind'
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
Copy-Item (Join-Path $icones 'Nevermind.ico') (Join-Path $fich 'Nevermind.ico') -Force

# Version des programmes : celle de Nevermind (Nevermind.ps1), dans un petit fichier compilé avec lanceur.cs
$ps1 = [IO.File]::ReadAllText((Join-Path $fich 'Nevermind.ps1'), [Text.Encoding]::UTF8)
if ($ps1 -notmatch "(?m)^\`$AppVersion = '(\d+(\.\d+){1,3})'") { throw 'Numéro de version introuvable dans Nevermind.ps1' }
$v4 = @($Matches[1].Split('.')); while ($v4.Count -lt 4) { $v4 += '0' }
$v4 = $v4 -join '.'
$versionCs = Join-Path ([IO.Path]::GetTempPath()) "nevermind-version-$PID.cs"
[IO.File]::WriteAllText($versionCs, "[assembly: System.Reflection.AssemblyVersion(`"$v4`")]`r`n[assembly: System.Reflection.AssemblyFileVersion(`"$v4`")]`r`n", (New-Object Text.UTF8Encoding($false)))

function Build-Exe([string]$Out, [string]$Icon, [string]$Define) {
    $opts = @('/nologo', '/target:winexe', '/optimize+', '/platform:anycpu', "/win32icon:$Icon", "/win32manifest:$manif",
              '/reference:System.Windows.Forms.dll', '/reference:System.Web.Extensions.dll', "/reference:$sma", "/out:$Out")
    if ($Define) { $opts += "/define:$Define" }
    & $csc @opts $source $versionCs
    if ($LASTEXITCODE) { throw "Échec de compilation de $Out" }
}
Build-Exe (Join-Path $app 'Mindset.exe') (Join-Path $icones 'Nevermind.ico') ''
Build-Exe (Join-Path $app 'Nevermind.exe') (Join-Path $icones 'Nevermind.ico') 'RELAIS'
Build-Exe (Join-Path $app 'Désinstaller Nevermind.exe') (Join-Path $icones 'Nevermind-desinstaller.ico') 'UNINSTALL'
Build-Exe (Join-Path $app 'NevermindAide.exe') (Join-Path $icones 'Nevermind.ico') 'AIDE'
[IO.File]::Delete($versionCs)
# NevermindPont.dll (les règles de l'Aide en Rust pour Nevermind), posée dans Nevermind\ ; zips.ps1 la prend
if (Test-Path -LiteralPath (Join-Path $racine 'rust\nevermind-pont\Cargo.toml')) { & (Join-Path $PSScriptRoot 'construire-pont.ps1') | Out-Null }

foreach ($z in @(& (Join-Path $PSScriptRoot 'zips.ps1') -Dossier $app -Sortie $racine)) { "OK: $z" }
