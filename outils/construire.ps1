# Construit le dossier de Nexo prêt à partager, puis OptiGame.zip. Le zip et son dossier gardent le nom
# « OptiGame » : les versions déjà installées cherchent ce nom pour se mettre à jour.
#
#   OptiGame\
#     Nexo.exe                             <- à lancer (icône Nexo)
#     Désinstaller Nexo.exe                <- désinstallation propre
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

& (Join-Path $PSScriptRoot 'icone.ps1') -OutDir $icones | Out-Null
New-Item -ItemType Directory -Force -Path $fich | Out-Null
Copy-Item (Join-Path $icones 'OptiGame.ico') (Join-Path $fich 'OptiGame.ico') -Force

function Build-Exe([string]$Out, [string]$Icon, [string]$Define) {
    $opts = @('/nologo', '/target:winexe', '/optimize+', '/platform:anycpu', "/win32icon:$Icon", "/win32manifest:$manif",
              '/reference:System.Windows.Forms.dll', "/out:$Out")
    if ($Define) { $opts += "/define:$Define" }
    & $csc @opts $source
    if ($LASTEXITCODE) { throw "Échec de compilation de $Out" }
}
Build-Exe (Join-Path $app 'Nexo.exe') (Join-Path $icones 'OptiGame.ico') ''
Build-Exe (Join-Path $app 'Désinstaller Nexo.exe') (Join-Path $icones 'OptiGame-desinstaller.ico') 'UNINSTALL'

foreach ($old in 'OptiGame.exe', 'Désinstaller OptiGame.exe') { $p = Join-Path $app $old; if (Test-Path -LiteralPath $p) { [IO.File]::Delete($p) } }
$zip = Join-Path $racine 'OptiGame.zip'
Compress-Archive -Path $app -DestinationPath $zip -Force
"OK: $zip"
