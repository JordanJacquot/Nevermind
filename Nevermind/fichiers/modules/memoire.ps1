# Nevermind : « Qu'est-ce qui prend ma mémoire ? » (la bulle RAM de la page Performances, ecran-perf.ps1).
#
# Le constat : la mémoire regroupée par appli (tous les processus d'une appli additionnés, les « Hôte de service »
# rattachés à leurs services), du plus gros au plus petit, avec une étiquette pour chacune : « Windows en a besoin »,
# « Inutile au quotidien » (une liste connue et prudente) ou « À vérifier ». Puis la part cachée : la mémoire réservée au
# matériel, celle des pilotes, la mémoire compressée, et le cache (qui ne compte pas : Windows le rend dès qu'une appli
# en a besoin). Une alerte à part si les pilotes ou le matériel gardent trop de mémoire, avec le pilote suspect quand
# Nevermind le retrouve (étiquettes de la mémoire des pilotes, lues sans droits administrateur).
#
# Les corrections : seulement ce qui est sûr (des services de mise à jour ou de logiciels du fabricant passés en
# « manuel »). Elles sont déclarées (Get-ChangementsMemoire), montrées
# avant, notées dans l'historique et remises par « Tout restaurer ». Jamais un service de Windows, jamais un processus
# fermé.
#
# Get-DonneesMemoire lit le PC (lecture seule) ; Get-ConstatMemoire ne fait que calculer à partir de ces données, ce qui
# permet de le tester sur de fausses données (outils\tester-memoire.ps1).
# Chargé par Nevermind.ps1 après microsoft.ps1.

$EtiquettesMemoire = @{ windows = 'Windows en a besoin'; inutile = 'Inutile au quotidien'; verifier = 'À vérifier' }

# Les services que Nevermind peut passer en « manuel » sans risque : mises à jour et logiciels du fabricant. En manuel,
# rien n'est désinstallé : le service se lance quand son logiciel en a besoin. (La même liste est dans l'Aide,
# aide-actions.ps1 : tester-memoire vérifie qu'elle accepte chacun d'eux.)
$ServicesInutilesMemoire = @(
    @{ Motif = '^edgeupdatem?$'; Nom = 'Mise à jour de Microsoft Edge' }
    @{ Motif = '^(gupdatem?|GoogleUpdater(Internal)?Service[\d.]{0,20})$'; Nom = 'Mise à jour de Google Chrome' }
    @{ Motif = '^AdobeARMservice$'; Nom = 'Mise à jour d''Adobe Acrobat' }
    @{ Motif = '^AdobeUpdateService$'; Nom = 'Mise à jour des logiciels Adobe' }
    @{ Motif = '^MozillaMaintenance$'; Nom = 'Mise à jour de Firefox' }
    @{ Motif = '^(HPAppHelperCap|HPDiagsCap|HPSysInfoCap|HpTouchpointAnalyticsService)$'; Nom = 'Logiciel HP (aide et statistiques)' }
    @{ Motif = '^(SupportAssistAgent|DDVDataCollector|DDVCollectorSvcApi|DDVRulesProcessor)$'; Nom = 'Logiciel Dell (SupportAssist et statistiques)' }
    @{ Motif = '^(ASUSSystemAnalysis|ASUSSystemDiagnosis|ASUSLinkNear|ASUSLinkRemote)$'; Nom = 'Logiciel ASUS (diagnostic, lien avec le téléphone)' }
    @{ Motif = '^(ESRV_SVC_QUEENCREEK|SystemUsageReportSvc_QUEENCREEK)$'; Nom = 'Mesure d''énergie Intel (statistiques)' }
    @{ Motif = '^NvTelemetryContainer$'; Nom = 'Statistiques NVIDIA' }
)

# Les programmes de Windows (et les pilotes qui l'accompagnent) : toujours « Windows en a besoin »
$ExesWindowsMemoire = '^(system|secure system|registry|smss|csrss|wininit|winlogon|services|lsass|lsaiso|lsm|svchost|dwm|explorer|fontdrvhost|sihost|ctfmon|conhost|audiodg|spoolsv|taskhostw|runtimebroker|dllhost|wmiprvse|wmiapsrv|searchhost|searchapp|searchindexer|searchprotocolhost|searchfilterhost|startmenuexperiencehost|shellexperiencehost|shellhost|textinputhost|msmpeng|nissrv|mpdefendercoreservice|securityhealthservice|securityhealthsystray|smartscreen|sgrmbroker|wudfhost|dashost|unsecapp|applicationframehost|lockapp|backgroundtaskhost|aggregatorhost|mousocoreworker|tiworker|trustedinstaller|wlanext|msdtc|systemsettings|useroobebroker|comppkgsrv|sppsvc|ngciso|igfxem|igfxcuiservice|igfxext|nvdisplay\.container|nvcontainer|atiesrxx|atieclxx|amdfendrsr|rtkauduservice64|intelaudioservice)$'

# Le « produit » des programmes de Windows (ils ont tous le même) : on prend alors leur description
$ProduitsWindowsMemoire = 'Windows. Operating System|Syst.me d.{1,6}exploitation|^Microsoft. Windows'
# Les programmes de Windows les plus visibles, en clair (les autres : leur description)
$NomsWindowsMemoire = @{ msmpeng = 'Antivirus de Windows (Microsoft Defender)'; mpdefendercoreservice = 'Antivirus de Windows (Microsoft Defender)'; nissrv = 'Antivirus de Windows (Microsoft Defender)'
    dwm = 'Affichage de Windows (fenêtres et effets)'; explorer = 'Explorateur de Windows (Bureau et barre des tâches)'; system = 'Windows lui-même (noyau)'; registry = 'Registre de Windows'
    csrss = 'Windows lui-même (sessions)'; lsass = 'Sécurité de Windows (connexion)'; searchhost = 'Recherche de Windows'; searchindexer = 'Recherche de Windows'; searchprotocolhost = 'Recherche de Windows'; searchfilterhost = 'Recherche de Windows'
    startmenuexperiencehost = 'Menu Démarrer'; shellexperiencehost = 'Menu Démarrer'; textinputhost = 'Clavier et saisie de Windows'; ctfmon = 'Clavier et saisie de Windows'
    audiodg = 'Son de Windows'; wmiprvse = 'Informations du PC (WMI)'; runtimebroker = 'Autorisations des applis'; powershell = 'Windows PowerShell'; nvcontainer = 'Pilote NVIDIA'; 'nvdisplay.container' = 'Pilote NVIDIA' }

# Les applis connues : leur nom en clair, leur étiquette et une phrase. Motif : le nom du programme (sans .exe).
$AppsConnuesMemoire = @(
    @{ Motif = '^msedge$'; Nom = 'Microsoft Edge'; Kind = 'verifier'; Note = 'Ton navigateur. Le fermer libère cette mémoire.' }
    @{ Motif = '^msedgewebview2$'; Nom = 'Affichage des applis (WebView2)'; Kind = 'verifier'; Note = 'Sert à afficher l''intérieur de certaines applis, dont Nevermind et Mindset. Part quand ces applis sont fermées.' }
    @{ Motif = '^chrome$'; Nom = 'Google Chrome'; Kind = 'verifier'; Note = 'Ton navigateur. Le fermer libère cette mémoire.' }
    @{ Motif = '^firefox$'; Nom = 'Firefox'; Kind = 'verifier'; Note = 'Ton navigateur. Le fermer libère cette mémoire.' }
    @{ Motif = '^(opera|opera_gx)$'; Nom = 'Opera'; Kind = 'verifier'; Note = 'Ton navigateur. Le fermer libère cette mémoire.' }
    @{ Motif = '^brave$'; Nom = 'Brave'; Kind = 'verifier'; Note = 'Ton navigateur. Le fermer libère cette mémoire.' }
    @{ Motif = '^(mindset|nevermind|nevermindaide)$'; Nom = 'Mindset et ses applis'; Kind = 'verifier'; Note = 'Les applis de la suite Mindset, dont Nevermind ouvert en ce moment.' }
    @{ Motif = '^microsoftedgeupdate$'; Nom = 'Mise à jour de Microsoft Edge'; Kind = 'inutile'; Note = 'Cherche les mises à jour de Edge. Pas besoin qu''il tourne tout le temps.' }
    @{ Motif = '^(googleupdate|googlecrashhandler(64)?)$'; Nom = 'Mise à jour de Google Chrome'; Kind = 'inutile'; Note = 'Cherche les mises à jour de Chrome. Pas besoin qu''il tourne tout le temps.' }
    @{ Motif = '^(armsvc|adobearm|adobeupdateservice|acrotray)$'; Nom = 'Mise à jour d''Adobe'; Kind = 'inutile'; Note = 'Cherche les mises à jour d''Adobe. Pas besoin qu''il tourne tout le temps.' }
    # Lumières (RGB) seulement : inutile au quotidien. Origine = $true : la phrase « d'où il vient » est ajoutée selon le PC
    @{ Motif = '^(lightingservice|ledkeeper\d*|auracreatorservice|aurawallpaperservice|signalrgb|openrgb|mysticlight.*|rzchromastreamserver|rzchromasdkservice|chromasdk.*)$'; Nom = ''; Kind = 'inutile'; Note = 'Gère les lumières (RGB) du PC. Les lumières gardent souvent leur dernière couleur sans lui.'; Origine = $true }
    # Logiciels du fabricant qui ne servent pas au quotidien
    @{ Motif = '^(hpcommrecovery|hpsysinfo|hpdiags.*|hptouchpoint.*|hpsupportassistant|hpanalyticscomp|supportassist.*|dellsupportassist.*|asussystemanalysis|asussystemdiagnosis|asuslink.*|myasus|acerquickaccess|carecenter.*)$'; Nom = ''; Kind = 'inutile'; Note = 'Logiciel du fabricant du PC (aide, statistiques). Pas besoin qu''il tourne tout le temps.' }
    # Logiciels qui règlent aussi la souris, le casque ou les ventilateurs : à vérifier
    @{ Motif = '^(icue|corsair.*|razer.*|rzsynapse.*|lghub.*|steelseries.*|armourycrate.*|armourysocketserver|nzxt.*|msi\.centralserver|msi_center.*|dragoncenter.*)$'; Nom = ''; Kind = 'verifier'; Note = 'Règle tes lumières, ta souris, ton casque ou tes ventilateurs.'; Origine = $true }
    @{ Motif = '^(steam|steamwebhelper|gameoverlayui(64)?|eaanticheat\.gameservice|easteamlauncher|ealocalhostsvc|epicgameslauncher|epicwebhelper|eadesktop|eabackgroundservice|ubisoftconnect|upc|battle\.net|riotclientservices|goggalaxy|galaxyclient)$'; Nom = ''; Kind = 'verifier'; Note = 'Lanceur de jeux.'; Origine = $true }
    @{ Motif = '^(discord|ms-teams|teams|spotify|whatsapp|skype|zoom)$'; Nom = ''; Kind = 'verifier'; Note = 'Messagerie ou musique.'; Origine = $true }
    @{ Motif = '^onedrive$'; Nom = 'OneDrive'; Kind = 'verifier'; Note = 'Synchronise tes fichiers.'; Origine = $true }
    @{ Motif = '^(phoneexperiencehost|crossdeviceresume)$'; Nom = 'Lien avec le téléphone'; Kind = 'verifier'; Note = 'Relie ton téléphone au PC. Si tu ne t''en sers pas, il peut être coupé dans les paramètres de Windows.' }
    @{ Motif = '^(widgets|widgetservice)$'; Nom = 'Widgets de Windows'; Kind = 'verifier'; Note = 'Le panneau météo et actualités. Il se coupe dans les paramètres de la barre des tâches.' }
    @{ Motif = '^officeclicktorun$'; Nom = 'Microsoft Office (mises à jour)'; Kind = 'verifier'; Note = 'Met à jour Office. Utile si tu te sers de Word ou d''Excel.' }
    @{ Motif = '^(avastsvc|avastui|avgsvc|avgui|mcshield|mfemms|mcapexe|modulecoreservice|nortonsecurity|ns|nsbu|bdagent|bdservicehost|vsserv|avp|avpui|ekrn|egui|mbamservice|mbamtray|mbam)$'; Nom = ''; Kind = 'verifier'; Note = 'Antivirus en plus de celui de Windows. Un seul suffit : garde celui que tu as choisi.' }
    @{ Motif = '^(vmmem|vmmemwsl|vmwp)$'; Nom = 'Machine virtuelle'; Kind = 'verifier'; Note = 'Une machine virtuelle ou Linux (WSL) est ouverte. La fermer libère cette mémoire.' }
)

# Les pilotes reconnus à leur nom ou à leur fabricant : le genre de matériel, pour l'alerte des pilotes
function Get-GenrePilote([string]$Texte) {
    switch -Regex ($Texte) {
        'Killer|Rivet'                                          { return 'carte réseau Killer' }
        'Realtek.*(Ethernet|PCIe|GbE|LAN|Wireless|WLAN|Wi-?Fi)|rt\d+x\d+|rtwlan|rtcx' { return 'carte réseau Realtek' }
        'Ethernet|Wireless|Wi-?Fi|WLAN|Network|NDIS|LAN '      { return 'carte réseau' }
        'Avast|AVG|McAfee|Norton|Symantec|Bitdefender|Kaspersky|ESET|Malwarebytes|Sophos|Trend' { return 'antivirus' }
        'RGB|Aura|Lighting|iCUE|Corsair|Razer|Mystic|SignalRGB|OpenRGB|ASUS|MSI|Gigabyte|ASRock|NZXT' { return 'logiciel RGB ou de carte mère' }
        'NVIDIA|Radeon|AMD|Graphics|Display'                    { return 'carte graphique' }
        'Storage|RST|NVMe|SATA|AHCI|RAID'                       { return 'disque' }
        'Audio|Sound|Nahimic|Sonic'                             { return 'son' }
    }
    ''
}

# ---------------------------------------------------------------------------
# Les étiquettes de la mémoire des pilotes (pool non paginé) : chaque pilote marque sa mémoire de 4 lettres
# ---------------------------------------------------------------------------
$NativePoolMemoire = @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class NmPoolMemoire {
    [DllImport("ntdll.dll")] static extern int NtQuerySystemInformation(int classe, IntPtr tampon, int taille, out int rendu);
    // SystemPoolTagInformation (22) : lisible sans droits administrateur. Rend « étiquette|non paginé|paginé ».
    public static string[] Etiquettes() {
        int taille = 0x100000; IntPtr t = IntPtr.Zero; int st = 0; int rendu;
        for (int essai = 0; essai < 6; essai++) {
            t = Marshal.AllocHGlobal(taille);
            st = NtQuerySystemInformation(22, t, taille, out rendu);
            if (st != unchecked((int)0xC0000004)) break;
            Marshal.FreeHGlobal(t); t = IntPtr.Zero; taille = Math.Max(taille * 2, rendu + 0x1000);
        }
        if (t == IntPtr.Zero) return new string[0];
        try {
            if (st != 0) return new string[0];
            int n = Marshal.ReadInt32(t);
            bool x64 = IntPtr.Size == 8;
            int pas = x64 ? 40 : 28; int debut = x64 ? 8 : 4;
            var l = new List<string>(n);
            for (int i = 0; i < n; i++) {
                IntPtr e = new IntPtr(t.ToInt64() + debut + (long)i * pas);
                byte[] tag = new byte[4]; Marshal.Copy(e, tag, 0, 4);
                long np = x64 ? Marshal.ReadInt64(e, 32) : (uint)Marshal.ReadInt32(e, 24);
                long p = x64 ? Marshal.ReadInt64(e, 16) : (uint)Marshal.ReadInt32(e, 12);
                l.Add(System.Text.Encoding.ASCII.GetString(tag) + "|" + np + "|" + p);
            }
            return l.ToArray();
        } finally { Marshal.FreeHGlobal(t); }
    }
}
'@

# Les plus grosses étiquettes de la mémoire des pilotes : @{ Tag; Octets }
function Get-EtiquettesPool([int]$Nombre = 3) {
    if (-not ('NmPoolMemoire' -as [type])) { Add-Type -TypeDefinition $NativePoolMemoire }
    $l = @(foreach ($x in @([NmPoolMemoire]::Etiquettes())) { $p = ([string]$x) -split '\|'; if ($p.Count -eq 3) { @{ Tag = $p[0]; Octets = [double]$p[1] } } })
    @($l | Sort-Object { $_.Octets } -Descending | Select-Object -First $Nombre)
}

# Le pilote qui porte une étiquette : les fichiers .sys de Windows qui contiennent ces 4 lettres (lecture seule, 6 s
# environ : lancé en tâche de fond). Une étiquette trouvée dans trop de fichiers ne désigne personne : liste vide.
# Rend @{ étiquette = @(@{ Fichier; Description; Societe }) }.
function Find-PilotesEtiquettes([string[]]$Tags) {
    $r = @{}
    $aChercher = @($Tags | Where-Object { $_ } | Select-Object -Unique)
    if (-not $aChercher.Count) { return $r }
    $trouves = @{}; foreach ($t in $aChercher) { $trouves[$t] = New-Object System.Collections.ArrayList }
    $fichiers = @(Get-ChildItem -LiteralPath (Join-Path $env:windir 'System32\drivers') -Filter '*.sys' -File -ErrorAction SilentlyContinue)
    foreach ($f in $fichiers) {
        try { $texte = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($f.FullName)) } catch { continue }
        foreach ($t in $aChercher) { if ($texte.Contains($t)) { [void]$trouves[$t].Add($f.FullName) } }
    }
    foreach ($t in $aChercher) {
        $l = @($trouves[$t])
        $r[$t] = @(if ($l.Count -and $l.Count -le 3) {
            foreach ($c in $l) {
                $v = try { [Diagnostics.FileVersionInfo]::GetVersionInfo($c) } catch { $null }
                @{ Fichier = [IO.Path]::GetFileName($c); Description = $(if ($v) { ([string]$v.FileDescription).Trim() }); Societe = $(if ($v) { ([string]$v.CompanyName).Trim() }) }
            }
        })
    }
    $r
}

# ---------------------------------------------------------------------------
# La lecture du PC (lecture seule, en tâche de fond) : les données que Get-ConstatMemoire calcule
# ---------------------------------------------------------------------------
# $Demarrage : les programmes (nom du .exe, et ceux que nomme leur commande) de la page Démarrage, lus par l'interface ;
# $Connus : les pilotes déjà retrouvés pour une étiquette (la recherche des .sys n'est faite qu'une fois par séance).
# Les étiquettes des pilotes ne sont lues que si leur mémoire est élevée ; celles dont le pilote n'est pas encore connu
# ont Pilotes vide : leur recherche suit, à part (Find-PilotesEtiquettes).
function Get-DonneesMemoire($Demarrage, $Connus) {
    $d = @{ Pc = $env:COMPUTERNAME; Date = Get-Date; Windir = $env:windir; Installee = 0.0; Visible = 0.0; Disponible = 0.0; Cache = 0.0; Modifiee = 0.0
        PoolNP = 0.0; PoolP = 0.0; Processus = @(); Services = @(); Demarrage = @($Demarrage | Where-Object { $_ }); Etiquettes = @() }
    try {
        $os = Get-CimInstance Win32_OperatingSystem -Property TotalVisibleMemorySize, FreePhysicalMemory -ErrorAction Stop
        $d.Visible = [double]$os.TotalVisibleMemorySize * 1KB; $d.Disponible = [double]$os.FreePhysicalMemory * 1KB
    } catch { Write-Log "Mémoire : système illisible ($($_.Exception.Message))" }
    try { $d.Installee = [double](@(Get-CimInstance Win32_PhysicalMemory -Property Capacity -ErrorAction Stop) | Measure-Object -Property Capacity -Sum).Sum } catch {}
    try {
        $m = Get-CimInstance Win32_PerfRawData_PerfOS_Memory -ErrorAction Stop
        $d.Disponible = [double]$m.AvailableBytes
        $d.Cache = [double]$m.StandbyCacheCoreBytes + [double]$m.StandbyCacheNormalPriorityBytes + [double]$m.StandbyCacheReserveBytes
        $d.Modifiee = [double]$m.ModifiedPageListBytes
        $d.PoolNP = [double]$m.PoolNonpagedBytes; $d.PoolP = [double]$m.PoolPagedBytes
    } catch { Write-Log "Mémoire : compteurs illisibles ($($_.Exception.Message))" }
    # La mémoire propre de chaque processus (celle que montre le Gestionnaire des tâches)
    $prive = @{}
    try { foreach ($x in @(Get-CimInstance Win32_PerfRawData_PerfProc_Process -Property IDProcess, WorkingSetPrivate -ErrorAction Stop)) { if ([int]$x.IDProcess) { $prive[[int]$x.IDProcess] = [double]$x.WorkingSetPrivate } } } catch {}
    $infos = @{}
    try {
        $d.Processus = @(foreach ($p in @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, ExecutablePath, CommandLine, WorkingSetSize -ErrorAction Stop)) {
            $chemin = [string]$p.ExecutablePath
            if ($chemin -and -not $infos.ContainsKey($chemin)) { $infos[$chemin] = try { [Diagnostics.FileVersionInfo]::GetVersionInfo($chemin) } catch { $null } }
            $v = if ($chemin) { $infos[$chemin] } else { $null }
            $o = if ($prive.ContainsKey([int]$p.ProcessId)) { $prive[[int]$p.ProcessId] } else { [double]$p.WorkingSetSize }
            if ([string]$p.Name -eq 'Memory Compression') { $o = [math]::Max($o, [double]$p.WorkingSetSize) }
            @{ Pid = [int]$p.ProcessId; Parent = [int]$p.ParentProcessId; Nom = [string]$p.Name; Chemin = $chemin; Ligne = [string]$p.CommandLine; Octets = $o
                Produit = $(if ($v) { ([string]$v.ProductName).Trim() }); Description = $(if ($v) { ([string]$v.FileDescription).Trim() }); Societe = $(if ($v) { ([string]$v.CompanyName).Trim() }) }
        })
    } catch { Write-Log "Mémoire : liste des programmes illisible ($($_.Exception.Message))" }
    try {
        $d.Services = @(foreach ($s in @(Get-CimInstance Win32_Service -Property Name, DisplayName, ProcessId, StartMode, State -ErrorAction Stop)) {
            @{ Nom = [string]$s.Name; Titre = [string]$s.DisplayName; Pid = [int]$s.ProcessId; Demarrage = [string]$s.StartMode; Etat = [string]$s.State }
        })
    } catch { Write-Log "Mémoire : services illisibles ($($_.Exception.Message))" }
    # Pilotes : seulement si leur mémoire est élevée, les plus grosses étiquettes (et leur pilote, s'il est déjà connu)
    $seuils = Get-SeuilsPilotes $(if ($d.Installee) { $d.Installee } else { $d.Visible })
    if ($d.PoolNP -ge $seuils.Surveiller) {
        try {
            $et = @(Get-EtiquettesPool 3 | Where-Object { $_.Octets -ge 50MB })
            $d.Etiquettes = @(foreach ($e in $et) { @{ Tag = $e.Tag; Octets = $e.Octets; Pilotes = @(if ($Connus -and $Connus.ContainsKey($e.Tag)) { $Connus[$e.Tag] }) } })
        } catch { Write-Log "Mémoire : étiquettes des pilotes illisibles ($($_.Exception.Message))" }
    }
    $d
}
# Les seuils de la mémoire des pilotes, selon la mémoire du PC : alerte franche au-delà de 5 % de la mémoire (entre 1 et
# 2 Go), « à surveiller » un peu en dessous (au plus 0,8 Go)
function Get-SeuilsPilotes([double]$Memoire) {
    $fort = [math]::Min([double]2GB, [math]::Max([double]1GB, 0.05 * $Memoire))
    @{ Fort = $fort; Surveiller = [math]::Min([double]0.8GB, 0.75 * $fort) }
}

# ---------------------------------------------------------------------------
# Le constat (calcul seul, sur les données lues ou injectées)
# ---------------------------------------------------------------------------
# L'appli d'un processus : @{ Cle; Nom; Kind; Note }
function Get-AppliMemoire($P, $Services, [string]$Windir) {
    $exe = ([string]$P.Nom) -replace '(?i)\.exe$', ''
    $bas = $exe.ToLower()
    $svc = @($Services | Where-Object { $_ })
    # Un service de la liste « inutile » dans ce processus : c'est lui qui compte
    foreach ($s in $svc) {
        foreach ($i in $ServicesInutilesMemoire) {
            if ([string]$s.Nom -match $i.Motif) { return @{ Cle = "inutile:$($i.Nom)"; Nom = $i.Nom; Kind = 'inutile'; Note = 'Service qui ne sert pas au quotidien. Nevermind peut le passer en « manuel » : il se lancera seulement quand son logiciel en a besoin.' } }
        }
    }
    # Les « Hôte de service » : rattachés à leurs services
    if ($bas -eq 'svchost') {
        if ($svc.Count) {
            $tri = @($svc | Sort-Object { [string]$_.Nom })
            $titre = if ($tri[0].Titre) { [string]$tri[0].Titre } else { [string]$tri[0].Nom }
            $nom = if ($tri.Count -gt 1) { "$titre et $($tri.Count - 1) autre$(if ($tri.Count -gt 2) { 's' }) service$(if ($tri.Count -gt 2) { 's' })" } else { $titre }
            return @{ Cle = "svc:$((@($tri | ForEach-Object { $_.Nom })) -join ',')"; Nom = $nom; Kind = 'windows'; Note = "Service de Windows (Hôte de service : $((@($tri | ForEach-Object { $_.Nom }) | Select-Object -First 4) -join ', ')$(if ($tri.Count -gt 4) { '...' }))." }
        }
        return @{ Cle = 'svc:'; Nom = 'Services de Windows'; Kind = 'windows'; Note = 'Hôte de service de Windows.' }
    }
    foreach ($a in $AppsConnuesMemoire) {
        if ($bas -match $a.Motif) {
            $nom = if ($a.Nom) { $a.Nom } elseif ($P.Produit -and $P.Produit -notmatch $ProduitsWindowsMemoire) { [string]$P.Produit } elseif ($P.Description) { [string]$P.Description } else { $exe }
            return @{ Cle = "app:$nom"; Nom = $nom; Kind = $a.Kind; Note = $a.Note; Origine = [bool]$a.Origine }
        }
    }
    $windows = $bas -match $ExesWindowsMemoire -or ($P.Chemin -and $Windir -and ([string]$P.Chemin).StartsWith($Windir.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase))
    # (le nom de l'appli : celui du produit, sauf pour Windows, dont chaque programme a sa description)
    $nom = if ($NomsWindowsMemoire.ContainsKey($bas)) { $NomsWindowsMemoire[$bas] } elseif ($P.Produit -and $P.Produit -notmatch $ProduitsWindowsMemoire) { [string]$P.Produit } elseif ($P.Description) { [string]$P.Description } else { $exe }
    if ($windows) { return @{ Cle = "app:$nom"; Nom = $nom; Kind = 'windows'; Note = 'Fait partie de Windows ou d''un pilote de ton matériel.' } }
    @{ Cle = "app:$nom"; Nom = $nom; Kind = 'verifier'; Note = 'Un logiciel installé sur ce PC.'; Origine = $true }
}

# D'où vient une appli, et donc comment s'en passer : la page Démarrage seulement si elle y est vraiment
function Get-OrigineMemoire($G) {
    if ($G.AuDemarrage) { return 'Il se lance avec le PC : si tu ne t''en sers pas, coupe-le dans la page Démarrage.' }
    if ($G.ParWindows) { return 'Il n''est pas dans la page Démarrage : c''est son logiciel qui le fait lancer par Windows. Si tu ne t''en sers pas, désinstaller ce logiciel libère cette mémoire.' }
    'Il ne se lance pas avec le PC : il est ouvert en ce moment. Le fermer libère cette mémoire.'
}

# Le constat complet : @{ chiffres ; Applis (triées) ; Alertes ; Corrections }
function Get-ConstatMemoire($D) {
    $visible = [double]$D.Visible
    $inst = [double]$D.Installee
    $lisible = $inst -ge $visible -and $inst -gt 0
    if (-not $lisible) { $inst = $visible }   # (mémoire installée illisible : pas de « réservé » inventé)
    $reservee = [math]::Max(0.0, $inst - $visible)
    $utilisee = [math]::Max(0.0, $visible - [double]$D.Disponible)

    $svcParPid = @{}
    foreach ($s in @($D.Services)) {
        if (-not $s -or -not [int]$s.Pid) { continue }
        if (-not $svcParPid.ContainsKey([int]$s.Pid)) { $svcParPid[[int]$s.Pid] = New-Object System.Collections.ArrayList }
        [void]$svcParPid[[int]$s.Pid].Add($s)
    }
    $nomParPid = @{}; $parentParPid = @{}
    foreach ($p in @($D.Processus)) { if ($p) { $nomParPid[[int]$p.Pid] = ([string]$p.Nom).ToLower(); $parentParPid[[int]$p.Pid] = [int]$p.Parent } }
    $demarrage = @(@($D.Demarrage) | Where-Object { $_ } | ForEach-Object { ([string]$_).ToLower() })
    $groupes = @{}
    $compressee = 0.0
    $edgeAvance = $false; $chromeAvance = $false
    foreach ($p in @($D.Processus)) {
        if (-not $p) { continue }
        $exe = ([string]$p.Nom) -replace '(?i)\.exe$', ''
        if ($exe -eq 'Memory Compression') { $compressee += [double]$p.Octets; continue }
        if ($exe -eq 'Idle' -or [int]$p.Pid -eq 0) { continue }
        if ($exe -eq 'msedge' -and [string]$p.Ligne -match '--no-startup-window') { $edgeAvance = $true }
        if ($exe -eq 'chrome' -and [string]$p.Ligne -match '--no-startup-window') { $chromeAvance = $true }
        $a = Get-AppliMemoire $p @($svcParPid[[int]$p.Pid]) ([string]$D.Windir)
        $g = $groupes[$a.Cle]
        if (-not $g) {
            $g = @{ Nom = $a.Nom; Kind = $a.Kind; Note = $a.Note; Origine = [bool]$a.Origine; AuDemarrage = $false; ParWindows = $false; Octets = 0.0; Nb = 0; Exes = New-Object System.Collections.ArrayList; Services = New-Object System.Collections.ArrayList; Corrections = @() }
            $groupes[$a.Cle] = $g
        }
        $g.Octets += [double]$p.Octets; $g.Nb++
        # (lui ou un de ses parents, jusqu'à 4 crans : Steam et son « WebHelper »)
        $x = [int]$p.Pid; $nomX = "$exe.exe".ToLower()
        for ($i = 0; $i -lt 5 -and $nomX; $i++) {
            if ($demarrage -contains $nomX) { $g.AuDemarrage = $true; break }
            $x = [int]$parentParPid[$x]; $nomX = if ($x) { $nomParPid[$x] } else { $null }
        }
        if ($nomParPid[[int]$p.Parent] -in 'services.exe', 'svchost.exe') { $g.ParWindows = $true }   # (lancé par un service ou une tâche planifiée)
        if ($g.Exes -notcontains "$exe.exe") { [void]$g.Exes.Add("$exe.exe") }
        foreach ($s in @($svcParPid[[int]$p.Pid])) { if ($s -and $g.Services -notcontains [string]$s.Nom) { [void]$g.Services.Add([string]$s.Nom) } }
    }

    # Les corrections sûres
    $corrections = New-Object System.Collections.ArrayList
    foreach ($s in @($D.Services)) {
        if (-not $s -or [string]$s.Demarrage -notin 'Auto', 'Automatic') { continue }
        $i = @($ServicesInutilesMemoire | Where-Object { [string]$s.Nom -match $_.Motif })[0]
        if (-not $i) { continue }
        [void]$corrections.Add(@{ Id = "svc:$($s.Nom)"; Nom = "$($i.Nom) ($($s.Nom)) : passe en « manuel »"
            Changes = @(@{ Type = 'svc'; Name = [string]$s.Nom; Demarrage = 'Manual' }) })
    }

    # Les étiquettes des applis selon ce qui a été trouvé
    foreach ($g in $groupes.Values) {
        $g.Corrections = @(@(foreach ($c in $corrections) { foreach ($x in @($c.Changes)) { if ($x.Type -eq 'svc' -and $g.Services -contains $x.Name) { $c.Id } } }) | Select-Object -Unique)
        if ($g.Exes -contains 'msedge.exe' -and $edgeAvance) {
            $g.Kind = 'inutile'; $g.Note = 'Edge se lance en avance avec Windows pour s''ouvrir plus vite. Pour le couper : dans Edge, Paramètres, puis Système et performances, et désactive « Boost de démarrage ».'
        }
        if ($g.Exes -contains 'chrome.exe' -and $chromeAvance) {
            $g.Kind = 'inutile'; $g.Note = 'Chrome reste ouvert en arrière-plan, même fenêtres fermées. Pour le couper : dans Chrome, Paramètres, puis Système, et désactive « Poursuivre l''exécution des applications en arrière-plan ».'
        }
        if ($g.Origine) { $g.Note = "$($g.Note) $(Get-OrigineMemoire $g)" }
        $g.Etiquette = $EtiquettesMemoire[$g.Kind]
    }
    $applis = @($groupes.Values | Sort-Object { $_.Octets } -Descending)
    $totalApplis = 0.0; foreach ($g in $applis) { $totalApplis += $g.Octets }

    # Les alertes
    $alertes = New-Object System.Collections.ArrayList
    $seuils = Get-SeuilsPilotes $inst
    $np = [double]$D.PoolNP
    if ($np -ge $seuils.Surveiller) {
        $fort = $np -ge $seuils.Fort
        # Le pilote suspect : d'abord le pilote d'un fabricant (ceux de Windows fuient rarement), dans la plus grosse
        # étiquette qui en a un ; sinon un pilote de Windows
        $suspect = ''; $nomPilote = ''
        $choix = $null
        foreach ($tiersSeuls in $true, $false) {
            foreach ($e in @($D.Etiquettes)) {
                $pil = @(@($e.Pilotes) | Where-Object { $_ -and (-not $tiersSeuls -or [string]$_.Societe -notmatch '^Microsoft') })
                if ($pil.Count) { $choix = @{ Pilote = $pil[0]; Octets = [double]$e.Octets }; break }
            }
            if ($choix) { break }
        }
        if ($choix) {
            $x = $choix.Pilote
            $genre = Get-GenrePilote "$($x.Fichier) $($x.Description) $($x.Societe)"
            $qui = if ($x.Description) { "$($x.Description) ($($x.Fichier))" } else { [string]$x.Fichier }
            $suspect = "C'est probablement le pilote $qui$(if ($genre) { ", $genre" })$(if ($x.Societe) { ", de $($x.Societe)" }), qui en garde $(Format-Size $choix.Octets)."
            $nomPilote = if ($x.Description) { [string]$x.Description } else { [string]$x.Fichier }
        }
        [void]$alertes.Add(@{ Id = 'pilotes'; Ton = $(if ($fort) { 'urgent' } else { 'attention' })
            Titre = $(if ($fort) { "Un pilote garde trop de mémoire ($(Format-Size $np))" } else { "Mémoire des pilotes un peu élevée ($(Format-Size $np)), à surveiller" })
            Texte = "$(if ($suspect) { "$suspect " })Souvent un pilote qui garde de la mémoire sans la rendre : carte réseau (Killer, Realtek), antivirus en plus de celui de Windows, logiciel RGB ou de carte mère."
            Quoi = $(if ($fort) { "Installe la dernière version $(if ($nomPilote) { "du pilote « $nomPilote »" } else { 'du pilote en cause (souvent celui de la carte réseau)' }) depuis le site du fabricant du PC ou de la carte mère, puis redémarre le PC." } else { "Redémarre le PC et regarde de nouveau dans quelques jours : si ça monte encore, mets à jour $(if ($nomPilote) { "le pilote « $nomPilote »" } else { 'le pilote de ta carte réseau' })." }) })
    }
    if ($lisible -and $reservee -ge 1GB) {
        [void]$alertes.Add(@{ Id = 'materiel'; Ton = 'attention'; Titre = "$(Format-Size $reservee) réservés au matériel"
            Texte = 'Cette mémoire est installée mais Windows ne peut pas s''en servir : souvent la puce graphique intégrée au processeur qui se la réserve, réglée dans le BIOS.'
            Quoi = 'Dans le BIOS, baisse la mémoire de la puce graphique (« UMA Frame Buffer » ou « iGPU Memory »), ou demande à quelqu''un qui s''y connaît.' })
    }

    @{ Pc = [string]$D.Pc; Date = $(if ($D.Date) { [datetime]$D.Date } else { Get-Date }); Installee = $inst; InstalleeLue = $lisible; Visible = $visible
        Reservee = $reservee; Utilisee = $utilisee; Disponible = [double]$D.Disponible; Cache = [double]$D.Cache; Modifiee = [double]$D.Modifiee
        PoolNP = $np; PoolP = [double]$D.PoolP; Compressee = $compressee; Seuils = $seuils
        Pourcent = $(if ($visible) { [int][math]::Round(100 * $utilisee / $visible) } else { 0 })
        Applis = $applis; TotalApplis = $totalApplis
        Reste = [math]::Max(0.0, $utilisee - $totalApplis - $np - $compressee)
        Alertes = @($alertes); Corrections = @($corrections); Etiquettes = @($D.Etiquettes) }
}

# Ce que changent les corrections, déclaré à l'avance (lecture seule, voir Set-Declarations)
function Get-ChangementsMemoire($Constat) {
    @(foreach ($c in @($Constat.Corrections)) { foreach ($x in @($c.Changes)) { if ($x) { $x } } })
}

# La part cachée, en lignes : @{ Nom; Octets; Note }
function Get-PartCacheeMemoire($C) {
    @(
        if ($C.InstalleeLue) { @{ Nom = 'Réservé au matériel'; Octets = $C.Reservee; Note = 'Installée mais pas utilisable par Windows (souvent la puce graphique intégrée).' } }
        @{ Nom = 'Pilotes (mémoire fixe)'; Octets = $C.PoolNP; Note = "La mémoire que les pilotes gardent toujours en mémoire vive. Normal jusqu'à $(Format-Size $C.Seuils.Surveiller) environ sur ce PC." }
        @{ Nom = 'Pilotes et Windows (mémoire qui peut aller sur le disque)'; Octets = $C.PoolP; Note = 'Windows peut la mettre de côté sur le disque si la mémoire manque.' }
        @{ Nom = 'Mémoire compressée'; Octets = $C.Compressee; Note = 'Windows serre ce qui sert peu pour gagner de la place : c''est plutôt bon signe.' }
        @{ Nom = 'Autre (Windows lui-même, mémoire partagée)'; Octets = $C.Reste; Note = 'Ce qui n''appartient à aucune appli en particulier.' }
        @{ Nom = 'En cache (ne compte pas)'; Octets = $C.Cache; Note = 'Des fichiers gardés en mémoire pour aller plus vite. Windows la rend tout de suite dès qu''une appli en a besoin : ce n''est pas de la mémoire prise.' }
    )
}

# ---------------------------------------------------------------------------
# Le rapport (fichier texte sur le Bureau, à envoyer à quelqu'un qui aide à distance)
# ---------------------------------------------------------------------------
function Format-RapportMemoire($C) {
    $l = New-Object System.Collections.Generic.List[string]
    $l.Add('Rapport mémoire de Nevermind')
    $l.Add("PC : $($C.Pc)    Date : $($C.Date.ToString('dd/MM/yyyy à HH:mm'))")
    $l.Add('')
    $l.Add('EN BREF')
    $l.Add("  Mémoire installée : $(Format-Size $C.Installee)$(if ($C.InstalleeLue -and $C.Reservee -ge 1MB) { " (Windows en voit $(Format-Size $C.Visible))" })")
    $l.Add("  Utilisée : $(Format-Size $C.Utilisee) ($($C.Pourcent) %)")
    $l.Add("  En cache : $(Format-Size $C.Cache) (ne compte pas : Windows la rend dès qu'une appli en a besoin)")
    $l.Add("  Libre tout de suite : $(Format-Size $C.Disponible)")
    $l.Add('')
    if (@($C.Alertes).Count) {
        $l.Add('A REGARDER')
        foreach ($a in @($C.Alertes)) { $l.Add("  * $($a.Titre)"); $l.Add("    $($a.Texte)"); $l.Add("    Que faire : $($a.Quoi)") }
        $l.Add('')
    }
    $l.Add('PAR APPLI (du plus gros au plus petit)')
    foreach ($g in @($C.Applis)) {
        if ($g.Octets -lt 1MB) { continue }
        $l.Add(('  {0,10}  {1}{2}  [{3}]' -f (Format-Size $g.Octets), $g.Nom, $(if ($g.Nb -gt 1) { " ($($g.Nb) processus)" } else { '' }), $g.Etiquette))
        $l.Add("              $($g.Note)$(if (@($g.Exes).Count) { " Programmes : $((@($g.Exes) | Select-Object -First 6) -join ', ')$(if (@($g.Exes).Count -gt 6) { '...' })." })")
    }
    $l.Add('')
    $l.Add('LA PART CACHEE')
    foreach ($x in (Get-PartCacheeMemoire $C)) { $l.Add(('  {0,10}  {1}' -f (Format-Size $x.Octets), $x.Nom)); $l.Add("              $($x.Note)") }
    if (@($C.Etiquettes).Count) {
        $l.Add('')
        $l.Add('ETIQUETTES DE LA MEMOIRE DES PILOTES (les plus grosses)')
        foreach ($e in @($C.Etiquettes)) {
            $p = @(@($e.Pilotes) | Where-Object { $_ } | ForEach-Object { "$($_.Fichier)$(if ($_.Description) { " ($($_.Description)$(if ($_.Societe) { ", $($_.Societe)" }))" })" })
            $l.Add(('  {0,10}  {1}  {2}' -f (Format-Size $e.Octets), $e.Tag, $(if ($p.Count) { "pilote probable : $($p -join ' ou ')" } else { 'pilote pas retrouvé' })))
        }
    }
    $l.Add('')
    $l.Add('CE QUE NEVERMIND PEUT COUPER SANS RISQUE')
    if (@($C.Corrections).Count) { foreach ($c in @($C.Corrections)) { $l.Add("  * $($c.Nom)") } }
    else { $l.Add('  Rien : ce qui tourne est utile, ou se coupe à la main (voir chaque appli).') }
    $l.Add('')
    $l.Add('Rien n''a été changé sur le PC pour faire ce rapport.')
    $l -join "`r`n"
}

# Pose le rapport dans un dossier (le Bureau par défaut) ; rend le chemin du fichier
function Save-RapportMemoire($C, [string]$Dossier) {
    if (-not $Dossier) { $Dossier = [Environment]::GetFolderPath('Desktop') }
    $f = Join-Path $Dossier "Nevermind, rapport mémoire du $($C.Date.ToString('dd-MM-yyyy')) à $($C.Date.ToString('HH\hmm')).txt"
    [IO.File]::WriteAllText($f, (Format-RapportMemoire $C), (New-Object Text.UTF8Encoding($true)))
    $f
}

# ---------------------------------------------------------------------------
# Les gestes de la page
# ---------------------------------------------------------------------------
# La lecture se fait en tâche de fond (quelques secondes, plus 6 s environ pour retrouver un pilote) : la fenêtre reste
# fluide, la pastille « en cours » le montre. Deux étapes : les données d'abord (l'écran montre le constat, sans le nom
# du pilote), puis la recherche du pilote dans les .sys si elle est utile (le nom arrive ensuite). Le fil de fond charge
# ce fichier lui-même (que des définitions) : il n'a besoin d'aucun autre module.
$TravailMemoire = {
    param([string]$Code, $Demarrage, $Connus, [string[]]$Tags, [string]$Etape)
    . ([scriptblock]::Create($Code))
    $script:JournalFond = New-Object System.Collections.Generic.List[string]
    function Write-Log([string]$Message) { $script:JournalFond.Add($Message) }
    $r = @{ Etape = $Etape }
    try {
        if ($Etape -eq 'pilotes') { $r.Pilotes = Find-PilotesEtiquettes $Tags }
        else { $r.Donnees = Get-DonneesMemoire $Demarrage $Connus }
    } catch { $r.Erreur = $_.Exception.Message }
    $r.Journal = @($script:JournalFond)
    $r
}

# Lance la lecture (ou la recherche du pilote). $Ensuite : ce qu'il faut faire quand tout est fini (« rapport »).
function Start-ConstatMemoire([string]$Ensuite, [string]$Etape = 'donnees', [string[]]$Tags) {
    if ($Ensuite -and @($script:MemoireApres) -notcontains $Ensuite) { $script:MemoireApres = @(@($script:MemoireApres | Where-Object { $_ }) + $Ensuite) }
    if ($script:MemoireJob) { return }   # (déjà en cours : le résultat servira aussi à ce geste)
    if (-not $script:PilotesEtiquettes) { $script:PilotesEtiquettes = @{} }
    if (-not $script:MemoireCode) { $script:MemoireCode = [IO.File]::ReadAllText((Join-Path $ModulesDir 'memoire.ps1'), [Text.Encoding]::UTF8) }
    # (les programmes de la page Démarrage : pour ne conseiller d'y aller que s'ils y sont vraiment)
    $dem = @(try { @(Get-StartupItems) | Where-Object { $_.Enabled -and $_.Exe } | ForEach-Object { ([IO.Path]::GetFileName([string]$_.Exe)).ToLower(); foreach ($m in [regex]::Matches([string]$_.Commande, '[\w.-]+\.exe')) { ([IO.Path]::GetFileName($m.Value.Trim())).ToLower() } } | Select-Object -Unique } catch { Write-Log "Mémoire : page Démarrage illisible ($($_.Exception.Message))" })
    $ps = [PowerShell]::Create(); $ps.RunspacePool = $script:BgPool
    [void]$ps.AddScript($TravailMemoire.ToString()).AddArgument($script:MemoireCode).AddArgument($dem).AddArgument($script:PilotesEtiquettes.Clone()).AddArgument(@($Tags)).AddArgument($Etape)
    $script:MemoireJob = @{ PS = $ps; Handle = $ps.BeginInvoke(); Debut = Get-Date; Etape = $Etape }
    $script:Kit.MemoireEnCours = $Etape
    Set-OccupeVue $(if ($Etape -eq 'pilotes') { 'Je cherche quel pilote garde la mémoire...' } else { 'Je regarde ce qui prend la mémoire...' })
    if (-not $script:MemoireTimer) {
        $script:MemoireTimer = New-Object System.Windows.Threading.DispatcherTimer
        $script:MemoireTimer.Interval = [TimeSpan]::FromMilliseconds(300)
        $script:MemoireTimer.Add_Tick({
            try { Update-ConstatMemoireFond }
            catch {
                Write-Log "Mémoire : $($_.Exception.Message)"
                $script:MemoireJob = $null; $script:Kit.MemoireEnCours = $null; $script:MemoireApres = @()
                try { $script:MemoireTimer.Stop(); Set-OccupeVue '' } catch {}
            }
        })
    }
    $script:MemoireTimer.Start()
}

# Toutes les 300 ms pendant la lecture : relève le résultat quand il est prêt
function Update-ConstatMemoireFond {
    $j = $script:MemoireJob
    if (-not $j) { $script:MemoireTimer.Stop(); return }
    if (-not $j.Handle.IsCompleted) {
        if (((Get-Date) - $j.Debut).TotalSeconds -lt 90) { return }
        try { $j.PS.Stop(); $j.PS.Dispose() } catch {}
        $script:MemoireJob = $null
        Write-Log "Mémoire : lecture abandonnée au bout de 90 s ($($j.Etape))"
        Complete-ConstatMemoire
        return
    }
    $r = Get-JobResult $j
    $script:MemoireJob = $null
    $x = @($r)[0]
    if ($x -is [psobject]) { $x = $x.psobject.BaseObject }
    Receive-ResultatMemoire $x
}

# Le résultat d'une étape : le constat (puis, s'il le faut, la recherche du pilote), ou le nom du pilote
function Receive-ResultatMemoire($X) {
    if (-not $X) { Write-Log 'Mémoire : lecture sans résultat'; Complete-ConstatMemoire; return }
    foreach ($l in @($X.Journal)) { if ($l) { Write-Log $l } }
    if ($X.Erreur) { Write-Log "Mémoire, $($X.Etape) : $($X.Erreur)" }
    if ($X.Etape -eq 'pilotes') {
        $p = $X.Pilotes
        if ($p) { foreach ($k in @($p.Keys)) { $script:PilotesEtiquettes[[string]$k] = @($p[$k]) } }
        $d = $script:MemoireDonnees
        if ($d) {
            foreach ($e in @($d.Etiquettes)) { if ($script:PilotesEtiquettes.ContainsKey([string]$e.Tag)) { $e.Pilotes = @($script:PilotesEtiquettes[[string]$e.Tag]) } }
            $script:Kit.Memoire = Get-ConstatMemoire $d
        }
        Complete-ConstatMemoire
        return
    }
    if (-not $X.Donnees) { Complete-ConstatMemoire; return }
    $script:MemoireDonnees = $X.Donnees
    $c = Get-ConstatMemoire $X.Donnees
    $script:Kit.Memoire = $c
    Write-Log "Mémoire : $(Format-Size $c.Utilisee) utilisés sur $(Format-Size $c.Visible), $(@($c.Applis).Count) applis, $(@($c.Corrections).Count) correction(s) sûre(s), $(@($c.Alertes).Count) alerte(s)"
    # Les étiquettes dont le pilote n'est pas encore connu : leur recherche, à part
    if (-not $script:PilotesEtiquettes) { $script:PilotesEtiquettes = @{} }
    $tags = @(@($X.Donnees.Etiquettes) | Where-Object { $_ -and -not $script:PilotesEtiquettes.ContainsKey([string]$_.Tag) } | ForEach-Object { [string]$_.Tag })
    if ($tags.Count) {
        try { Update-EcranNevermind } catch { Write-Log "Écran : $_" }   # (le constat s'affiche déjà)
        Start-ConstatMemoire '' 'pilotes' $tags
        return
    }
    Complete-ConstatMemoire
}

# Tout est lu : la pastille s'éteint, l'écran est mis à jour, puis ce qui attendait (le rapport)
function Complete-ConstatMemoire {
    $script:Kit.MemoireEnCours = $null
    if (-not $script:MemoireJob -and $script:MemoireTimer) { $script:MemoireTimer.Stop() }
    Set-OccupeVue ''
    try { Update-EcranNevermind } catch { Write-Log "Écran : $_" }
    $apres = @($script:MemoireApres); $script:MemoireApres = @()
    if ($apres -contains 'rapport') {
        if ($script:Kit.Memoire) { New-RapportMemoire } else { Show-MessageVue 'Le rapport n''a pas pu être fait : la mémoire du PC n''a pas pu être lue.' }
    }
}
# « Couper ce qui est sûr » : montré avant, déclaré, noté dans l'historique (« Revenir en arrière », « Tout restaurer »)
function Invoke-CorrectionMemoire {
    $c = $script:Kit.Memoire
    $corr = @(if ($c) { $c.Corrections })
    if (-not $corr.Count) { return }
    $noms = @($corr | ForEach-Object { "•  $($_.Nom)" })
    $texte = "Nevermind va couper ce qui ne sert pas au quotidien :`n`n$($noms -join "`n")`n`nRien n'est désinstallé ni fermé de force. « Revenir en arrière », ou « Tout restaurer » dans la page Sauvegarde, remettent tout comme avant. Continuer ?"
    if (-not (Confirm-Action $texte @(Get-DetailsAvant (Get-ChangementsMemoire $c)))) { return }
    Set-Busy $true
    $faits = @(); $rates = @()
    $script:RunLog = New-Object System.Collections.ArrayList
    try {
        foreach ($x in $corr) {
            try { Set-Declarations $x.Changes; $faits += $x } catch { $rates += "$($x.Nom) : $($_.Exception.Message)"; Write-Log "Mémoire, correction $($x.Id) : $($_.Exception.Message)" }
        }
    } finally { $log = $script:RunLog; $script:RunLog = $null; Set-Busy $false }
    Update-BackupSummary
    $script:Kit.Memoire = $null; $script:MemoireDonnees = $null
    Start-ConstatMemoire   # (relu en tâche de fond : la page montre ensuite ce qui reste)
    $lines = @(if ($faits.Count) { "$($faits.Count) chose$(if ($faits.Count -gt 1) { 's' }) coupée$(if ($faits.Count -gt 1) { 's' }) :" }) + @($faits | ForEach-Object { "•  $($_.Nom)" })
    if ($rates.Count) { $lines += 'Pas pu être fait :'; $lines += @($rates | ForEach-Object { "   $_" }) }
    Set-Status 'Mémoire : ce qui est sûr est coupé.'
    Show-ResultSheet $(if ($faits.Count) { "C'est fait !" } else { 'Rien n''a pu être coupé' }) $lines $log 'La mémoire revient tout de suite pour ce qui tournait, et au prochain allumage du PC pour le reste.'
}

# « Créer un rapport » : le fichier sur le Bureau
# (lecture en cours, ou pas encore faite : le rapport est posé dès qu'elle est finie, nom du pilote compris)
function New-RapportMemoire {
    if ($script:MemoireJob -or $script:Kit.MemoireEnCours -or -not $script:Kit.Memoire) {
        Start-ConstatMemoire 'rapport'
        Show-MessageVue 'Je finis de regarder la mémoire, puis je pose le rapport sur le Bureau.'
        return
    }
    $dossier = if ($EnTest -and $env:ESSAI_DOSSIER) { $env:ESSAI_DOSSIER } else { $null }   # (essais : jamais sur le vrai Bureau)
    $f = Save-RapportMemoire $script:Kit.Memoire $dossier
    Write-Log "Mémoire : rapport posé ($f)"
    Show-MessageVue "Rapport posé sur le Bureau : « $([IO.Path]::GetFileName($f)) ». Tu peux l'envoyer à la personne qui t'aide."
}
