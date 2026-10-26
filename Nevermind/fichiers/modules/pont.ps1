# Nevermind : NevermindPont.dll, les règles de l'Aide en Rust (rust\nevermind-pont), appelées par Nevermind.
# Plan Rust, 3.7 et étape 1a (point D) : Nevermind ne garde plus sa propre copie de ces règles. La règle « fermable »
# (bulle RAM, mode jeu, « Fermer <programme> »), le nom du canal de l'Aide, le programme d'une commande de Windows et les
# dossiers de Windows que l'Aide vide viennent de la DLL : LE même code que NevermindAide.exe (rust\aide-regles).
#
# La DLL est chargée une seule fois, par son CHEMIN COMPLET, depuis le dossier protégé de l'appli (à côté de
# NevermindAide.exe), par Initialize-Pont (Nevermind.ps1). Jamais un autre chemin : ni variable d'environnement, ni
# dossier de l'utilisateur. Une petite déclaration C# (NevermindPont) l'appelle : un texte JSON en entrée, un texte JSON
# en sortie, la mémoire rendue à la DLL. Toute erreur Rust est rattrapée dans la DLL (réponse « ok » à faux).
#
# DLL absente, abîmée ou qui ne répond pas : Nevermind continue, au plus sûr. Rien n'est fermable (« Fermer » n'est pas
# proposé, le mode jeu ne ferme rien), et le nom du canal reste inconnu : Nevermind ne parle à aucune Aide (il dit
# alors qu'elle ne répond pas et propose de le mettre à jour depuis Mindset, qui répare le dossier).
#
# Que des définitions : le fil de fond de la page RAM charge aussi ce fichier (la DLL, elle, est déjà chargée dans le
# processus). Testé par outils\tester-pont.ps1.

$PontInterface = 1   # (le numéro d'interface de la DLL : nevermind-pont, demandes.rs, INTERFACE)
$PontRefusAbsente = 'règle de fermeture absente'

# Charge la DLL (une fois par processus). Rend $null si elle est prête, sinon la raison (pour le journal).
function Initialize-Pont([string]$Dll) {
    if ('NevermindPont' -as [type]) { if ([NevermindPont]::Pret) { return $null } else { return 'DLL déjà essayée, pas prête' } }
    if (-not $Dll -or -not [IO.Path]::IsPathRooted($Dll) -or $Dll.Contains('"')) { return "chemin non valable ($Dll)" }
    if (-not [IO.File]::Exists($Dll)) { return "DLL absente ($Dll)" }
    try {
        Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class NevermindPont
{
    [DllImport(@"$Dll", CallingConvention = CallingConvention.Cdecl)] static extern IntPtr nevermind_pont_appel(byte[] entree);
    [DllImport(@"$Dll", CallingConvention = CallingConvention.Cdecl)] static extern void nevermind_pont_liberer(IntPtr texte);
    public static bool Pret = false;
    // Une demande (JSON), sa réponse (JSON). null : la DLL n'a rien rendu.
    public static string Appeler(string json)
    {
        byte[] b = Encoding.UTF8.GetBytes(json + "\0");
        IntPtr r = nevermind_pont_appel(b);
        if (r == IntPtr.Zero) return null;
        try
        {
            int n = 0; while (Marshal.ReadByte(r, n) != 0) n++;
            byte[] o = new byte[n]; Marshal.Copy(r, o, 0, n);
            return Encoding.UTF8.GetString(o);
        }
        finally { nevermind_pont_liberer(r); }
    }
}
"@
    } catch { if (-not ('NevermindPont' -as [type])) { return "déclaration refusée ($($_.Exception.Message))" } }
    # (le premier appel charge vraiment la DLL : abîmée, ou pas pour ce Windows, l'erreur arrive ici)
    try { $v = [NevermindPont]::Appeler('{"action":"version"}') | ConvertFrom-Json }
    catch { return "DLL qui ne se charge pas ($($_.Exception.Message))" }
    if (-not $v -or -not $v.ok -or [int]$v.interface -ne $PontInterface) { return "DLL d'une autre version (interface $($v.interface), attendue $PontInterface)" }
    [NevermindPont]::Pret = $true
    $null
}

# La DLL est-elle prête ?
function Test-Pont { [bool](('NevermindPont' -as [type]) -and [NevermindPont]::Pret) }

# Une demande à la DLL ($Demande : une table, voir nevermind-pont\src\demandes.rs). Rend la réponse ; lève une erreur
# si la DLL n'est pas prête ou refuse la demande.
function Invoke-Pont($Demande) {
    if (-not (Test-Pont)) { throw 'NevermindPont.dll n''est pas chargée' }
    $t = [NevermindPont]::Appeler((ConvertTo-Json -InputObject $Demande -Depth 6 -Compress))
    if (-not $t) { throw 'NevermindPont.dll n''a rien répondu' }
    $r = $t | ConvertFrom-Json
    if (-not $r.ok) { throw "NevermindPont.dll : $($r.erreur)" }
    $r
}

# Un nombre lu par Windows, ou $null s'il est absent (jamais 0 à la place : « illisible » n'est pas « non »)
function ConvertTo-PontNombre($V) {
    if ($null -eq $V -or ($V -is [string] -and -not $V)) { return $null }
    try { [long]$V } catch { $null }
}

function Write-PontJournal([string]$Texte) { if (Get-Command Write-Log -ErrorAction SilentlyContinue) { Write-Log $Texte } }

# Une liste de textes rendue par la DLL (un texte ou null par élément demandé), vérifiée : autant d'éléments que demandé
function ConvertFrom-PontListe($Liste, [int]$Attendus) {
    $l = @($Liste)
    if ($l.Count -ne $Attendus) { throw "$($l.Count) réponses pour $Attendus demandes" }
    $o = New-Object object[] $Attendus
    for ($i = 0; $i -lt $Attendus; $i++) { if ($null -ne $l[$i]) { $o[$i] = [string]$l[$i] } }
    , $o
}

# La règle « fermable » (LA règle de l'Aide, aide_regles::fermeture) sur plusieurs processus d'un coup. $Contexte :
# Get-AideContexte (Antivirus, ServicesPid, Windows ; Programmes pour les tests), plus Reference (Get-AideReference) et,
# si connus, Processus (@{ pid = @{ Parent; Chemin } }). $Fiches : Read-AideFiche (@{ Id; Chemin; Session; Compte;
# Protection; Eleve }). Rend une raison par fiche, dans le même ordre ($null : fermable). DLL absente ou en erreur :
# rien n'est fermable. (Rend le tableau lui-même : $r = Get-RefusFermetureListe ..., sans @() autour.)
function Get-RefusFermetureListe($Contexte, $Fiches, [switch]$ElevePermis) {
    $liste = @($Fiches)
    $absente = { , [object[]]@($liste | ForEach-Object { $PontRefusAbsente }) }
    if (-not $liste.Count) { return , @() }
    if (-not (Test-Pont)) { return , (& $absente) }
    $ref = $Contexte.Reference
    $ctx = @{
        windows = $(if ($Contexte.Windows) { [string]$Contexte.Windows } else { [Environment]::GetFolderPath('Windows') })
        programmes = $(if ($Contexte.Programmes) { [string]$Contexte.Programmes } else { [Environment]::GetFolderPath('ProgramFiles') })
        antivirus = @(@($Contexte.Antivirus) | Where-Object { $_ } | ForEach-Object { [string]$_ })
        servicesPid = $(if ($null -ne $Contexte.ServicesPid) { , @($Contexte.ServicesPid | ForEach-Object { [long]$_ }) } else { $null })
        reference = $(if ($ref) { @{ id = ConvertTo-PontNombre $ref.Id; session = ConvertTo-PontNombre $ref.Session; compte = [string]$ref.Compte; eleve = ConvertTo-PontNombre $ref.Eleve } } else { $null })
        processus = @(if ($Contexte.Processus) { foreach ($k in @($Contexte.Processus.Keys)) { $p = $Contexte.Processus[$k]; @{ pid = [long]$k; parent = ConvertTo-PontNombre $p.Parent; chemin = [string]$p.Chemin } } })
    }
    $f = @(foreach ($x in $liste) {
        @{ id = ConvertTo-PontNombre $x.Id; chemin = [string]$x.Chemin; session = ConvertTo-PontNombre $x.Session; compte = [string]$x.Compte
           protection = ConvertTo-PontNombre $x.Protection; eleve = ConvertTo-PontNombre $x.Eleve }
    })
    try {
        return , (ConvertFrom-PontListe (Invoke-Pont @{ action = 'fermeture.refus'; contexte = $ctx; fiches = $f; elevePermis = [bool]$ElevePermis }).refus $liste.Count)
    } catch {
        Write-PontJournal "Règle de fermeture : $($_.Exception.Message)"
        return , (& $absente)
    }
}

# La même règle pour un seul processus ($null : il peut être fermé)
function Get-RefusFermeture($Contexte, $Fiche, [switch]$ElevePermis) {
    (Get-RefusFermetureListe $Contexte @($Fiche) -ElevePermis:$ElevePermis)[0]
}

# Le suffixe du nom du canal de l'Aide pour ce dossier (12 caractères), $null s'il est inconnu (DLL absente, dossier
# qui n'est pas un chemin « X:\ »)
function Get-SuffixeCanal([string]$Dossier) {
    if (-not (Test-Pont)) { return $null }
    try { $r = Invoke-Pont @{ action = 'canal'; dossier = $Dossier; sid = '' }; if ($r.suffixe) { [string]$r.suffixe } else { $null } }
    catch { Write-PontJournal "Canal de l'Aide : $($_.Exception.Message)"; $null }
}

# Le programme de chaque commande écrite par Windows (« %ProgramFiles%\...\x.exe », « "C:\...\x.exe" -k ... ») : son
# chemin complet, ou $null (variable inconnue, pas un programme). Les dossiers viennent de Windows, jamais des variables
# de l'utilisateur. DLL absente : rien de reconnu.
function Get-ProgrammesCommandes([string[]]$Commandes) {
    $liste = @($Commandes)
    if (-not $liste.Count) { return , @() }
    $rien = { , (New-Object object[] $liste.Count) }
    if (-not (Test-Pont)) { return , (& $rien) }
    $dossiers = @{ windows = [Environment]::GetFolderPath('Windows'); programmes = [Environment]::GetFolderPath('ProgramFiles')
                   programmesX86 = [Environment]::GetFolderPath('ProgramFilesX86'); donneesCommunes = [Environment]::GetFolderPath('CommonApplicationData') }
    try {
        return , (ConvertFrom-PontListe (Invoke-Pont @{ action = 'dossiers.programmes'; dossiers = $dossiers; commandes = @($liste | ForEach-Object { [string]$_ }) }).programmes $liste.Count)
    } catch { Write-PontJournal "Programmes de Windows : $($_.Exception.Message)"; return , (& $rien) }
}

# Les dossiers de Windows que l'Aide vide (catégories wu, do, dump du nettoyage), montrés dans la page Nettoyage. Seule
# l'Aide les lit et les vide, avec sa propre liste : ici, ils ne servent qu'à l'affichage. DLL absente : listes vides.
function Get-DossiersNettoyage {
    $r = @{ wu = @(); do = @(); dump = @() }
    if (-not (Test-Pont)) { return $r }
    try {
        $d = (Invoke-Pont @{ action = 'nettoyage.dossiers'; windows = [Environment]::GetFolderPath('Windows') }).dossiers
        foreach ($k in @($r.Keys)) { $r[$k] = @(@($d.$k) | Where-Object { $_ } | ForEach-Object { [string]$_ }) }
    } catch { Write-PontJournal "Dossiers du nettoyage : $($_.Exception.Message)" }
    $r
}
