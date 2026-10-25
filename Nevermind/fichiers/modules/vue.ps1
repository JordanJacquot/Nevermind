# Nevermind : l'écran de l'appli, dessiné par le kit de la suite (dossier « ui », affiché par WebView2).
#
# L'appli ne dessine rien elle-même : elle décrit ce qu'il faut montrer (Send-Vue, voir ui\kit.js pour le détail) et
# reçoit les gestes de l'utilisateur sous forme d'actions (Register-ActionVue). La page est locale, hors connexion
# (hôte virtuel https://nevermind.local/ sur le dossier « ui », toute autre adresse refusée).
#
# Économie : cachée (réduite, autre page de Mindset), la vue est masquée et la page s'arrête (plus de 3D, plus de
# minuteries) ; au bout d'une minute cachée, WebView2 est mise en veille (TrySuspendAsync), et se réveille au retour.
# Sans WebView2 (très rare : Windows 11 l'a toujours), ou si elle ne démarre pas, l'appli en est prévenue (action
# « impossible », Register-ActionVue) pour expliquer quoi faire ; sans cette action, un message remplace l'écran vide.
# Chargé par Nevermind.ps1 avant accueil-console.ps1, qui crée l'écran (Initialize-AccueilConsole).

$EnTest      = [bool]$env:NEVERMIND_TEST
$AppNom      = 'Nevermind'
$UiDir       = Join-Path $AppDir 'ui'
$Webview2Dir = Join-Path $AppDir 'lib\webview2'
$HoteVue     = 'nevermind.local'
$script:Vue  = @{ Controle = $null; Pret = $false; Etat = $null; Actions = @{}; CacheeDepuis = $null; EnVeille = $false; Images = @{}; Hote = $null; Commun = $false }

# WebView2 est-elle possible ici ? ($null si oui, sinon la raison)
function Test-VuePossible {
    foreach ($f in 'Microsoft.Web.WebView2.Core.dll', 'Microsoft.Web.WebView2.Wpf.dll', 'WebView2Loader.dll') { if (-not (Test-Path -LiteralPath (Join-Path $Webview2Dir $f))) { return "fichier manquant : $f" } }
    if (-not (Test-Path -LiteralPath (Join-Path $UiDir 'index.html'))) { return 'fichier manquant : ui\index.html' }
    try {
        if (-not $script:WebView2Charge) {   # (une seule fois : WebView2 refuse ensuite de changer de dossier)
            Add-Type -Path (Join-Path $Webview2Dir 'Microsoft.Web.WebView2.Core.dll')
            Add-Type -Path (Join-Path $Webview2Dir 'Microsoft.Web.WebView2.Wpf.dll')
            [Microsoft.Web.WebView2.Core.CoreWebView2Environment]::SetLoaderDllFolderPath($Webview2Dir)
            $script:WebView2Charge = $true
        }
        $v = [Microsoft.Web.WebView2.Core.CoreWebView2Environment]::GetAvailableBrowserVersionString()
        if (-not $v) { return 'WebView2 absent de ce PC' }
        Write-Log "Écran : WebView2 $v"
    } catch { return "WebView2 indisponible ($($_.Exception.Message))" }
    $null
}

# Dossier des données de WebView2. Géré par Mindset (données dans <racine de la suite>\Nevermind) : le dossier commun de
# la suite, <racine de la suite>\WebView2, avec le profil « Nevermind » (stockage séparé de celui de Mindset). Mindset et
# Nevermind partagent alors les mêmes processus WebView2 : une seule ligne « Gestionnaire WebView2 » dans le Gestionnaire
# des tâches. Les deux doivent créer WebView2 avec les mêmes options (aujourd'hui : aucune), sinon WebView2 refuse.
# Sinon (copie hors de Mindset, données encore dans l'ancien dossier) ou après un refus du dossier commun ($script:VueSansCommun) :
# son dossier à lui, « ecran-webview », comme avant.
function Get-VueDossier {
    if (-not $script:VueSansCommun -and $Managed -and $DataDir -and (Split-Path $DataDir -Leaf) -eq 'Nevermind') {
        $suite = Split-Path $DataDir -Parent
        if ($suite) { return @{ Dossier = (Join-Path $suite 'WebView2'); Profil = 'Nevermind'; Commun = $true } }
    }
    @{ Dossier = (Join-Path $DataDir 'ecran-webview'); Profil = ''; Commun = $false }
}

# Le dossier commun a été refusé (options différentes, ancienne version de Mindset encore ouverte dessus, dossier
# illisible) : noté au journal, et l'écran est recréé tout de suite dans son dossier à lui. Il ne reste jamais vide.
function Reset-VueSansCommun([string]$Raison) {
    $script:VueSansCommun = $true
    Write-Log "Écran : dossier WebView2 commun de la suite refusé ($Raison), retour au dossier de Nevermind (ecran-webview)"
    $v = $script:Vue.Controle
    $hote = $script:Vue.Hote
    $script:Vue.Controle = $null; $script:Vue.Pret = $false
    if ($v) { try { $v.Parent.Children.Remove($v) } catch {}; try { $v.Dispose() } catch {} }
    if ($script:Vue.Image) { try { $script:Vue.Image.Parent.Children.Remove($script:Vue.Image) } catch {}; $script:Vue.Image = $null }
    if ($hote) { Initialize-Vue $hote }
}

# Crée l'écran dans $Hote (un Grid de interface.xaml)
function Initialize-Vue($Hote) {
    $raison = if ($script:VueVerifiee) { $null } else { Test-VuePossible }
    if ($raison) {
        if (Stop-VueImpossible $raison) { return }
        $t = New-Text "$AppNom ne peut pas s'afficher sur ce PC.`n`nIl lui manque un composant de Windows (WebView2). Ouvre Mindset et réinstalle $AppNom, ou fais les mises à jour de Windows." 16
        $t.TextWrapping = 'Wrap'; $t.TextAlignment = 'Center'; $t.Margin = '60'; $t.VerticalAlignment = 'Center'
        $Hote.Children.Add($t) | Out-Null
        return
    }
    $vue = New-Object Microsoft.Web.WebView2.Wpf.WebView2
    $vue.DefaultBackgroundColor = [System.Drawing.ColorTranslator]::FromHtml((ConvertTo-ThemeHex '#06050F'))   # (pas d'éclair blanc à l'allumage)
    $cp = New-Object Microsoft.Web.WebView2.Wpf.CoreWebView2CreationProperties
    $dw = Get-VueDossier
    if ($dw.Commun) {
        # Le dossier commun doit pouvoir s'écrire AVANT d'être donné à WebView2 : sinon le moteur affiche lui-même une boîte
        # d'erreur (« ne peut pas lire et écrire dans son répertoire de données ») avant que le filet ne prenne le relais.
        # Créé s'il manque, puis un petit fichier témoin écrit et effacé dans le dossier et dans son EBWebView s'il existe.
        $refus = $null
        try {
            [void][IO.Directory]::CreateDirectory($dw.Dossier)
            foreach ($d in @($dw.Dossier, (Join-Path $dw.Dossier 'EBWebView'))) {
                if (-not [IO.Directory]::Exists($d)) { continue }
                $t = Join-Path $d "ecriture-$PID.tmp"
                [IO.File]::WriteAllText($t, '1')
                [IO.File]::Delete($t)
            }
        } catch { $refus = $_.Exception.Message }
        if ($refus) { $script:VueSansCommun = $true; Write-Log "Écran : dossier WebView2 commun pas utilisable ($refus), dossier de Nevermind utilisé"; $dw = Get-VueDossier }
    }
    $cp.UserDataFolder = $dw.Dossier
    if ($dw.Profil) { $cp.ProfileName = $dw.Profil }
    $vue.CreationProperties = $cp
    $script:Vue.Hote = $Hote
    $script:Vue.Commun = $dw.Commun
    if (-not $script:VueDossierNote -or $script:VueDossierNote -ne $dw.Dossier) { $script:VueDossierNote = $dw.Dossier; Write-Log "Écran : dossier WebView2 $($dw.Dossier)$(if ($dw.Commun) { " (commun à la suite, profil $($dw.Profil))" })" }
    $vue.Add_CoreWebView2InitializationCompleted({
        param($s, $e)
        if (-not $e.IsSuccess) {
            $raisonVue = [string]$e.InitializationException.Message
            # Refus du dossier commun : on recrée l'écran dans le dossier de Nevermind (après ce message, pas pendant)
            if ($script:Vue.Commun -and $script:Vue.Controle -and $s -eq $script:Vue.Controle) {
                [void]$Window.Dispatcher.BeginInvoke([Action[string]]{ param($r) try { Reset-VueSansCommun $r } catch { [void](Stop-VueImpossible "initialisation refusée ($r), puis $($_.Exception.Message)") } }, $raisonVue)
                return
            }
            [void](Stop-VueImpossible "initialisation refusée ($raisonVue)"); return
        }
        try {
            $c = $s.CoreWebView2
            $r = $c.Settings
            $r.AreDevToolsEnabled = [bool]$env:NEVERMIND_DEV; $r.AreDefaultContextMenusEnabled = $false; $r.IsZoomControlEnabled = $false
            $r.AreBrowserAcceleratorKeysEnabled = $false; $r.IsStatusBarEnabled = $false; $r.IsPasswordAutosaveEnabled = $false; $r.IsGeneralAutofillEnabled = $false
            $c.SetVirtualHostNameToFolderMapping($HoteVue, $UiDir, [Microsoft.Web.WebView2.Core.CoreWebView2HostResourceAccessKind]::Deny)
            # Les images et la police d'un thème perso : son dossier, en lecture seule, sous https://theme.local/
            # Les images de l'appli (dossier « images » à côté de ui, s'il existe) : https://images.local/<fichier>
            $imagesDir = Join-Path $AppDir 'images'
            if (Test-Path -LiteralPath $imagesDir) { $c.SetVirtualHostNameToFolderMapping('images.local', $imagesDir, [Microsoft.Web.WebView2.Core.CoreWebView2HostResourceAccessKind]::Allow) }
            if ($ThemePack -and $ThemePack.Dir) { $c.SetVirtualHostNameToFolderMapping('theme.local', $ThemePack.Dir, [Microsoft.Web.WebView2.Core.CoreWebView2HostResourceAccessKind]::Allow) }
            # Seul l'écran de l'appli s'affiche : toute autre adresse, et toute nouvelle fenêtre, est refusée
            $c.Add_NavigationStarting({ param($s2, $e2) if (-not $e2.Uri.StartsWith("https://$HoteVue/")) { $e2.Cancel = $true; Write-Log "Écran : navigation refusée ($($e2.Uri))" } })
            $c.Add_NewWindowRequested({ param($s2, $e2) $e2.Handled = $true })
            # Une image ailleurs sur le PC, déclarée par l'appli (Get-ImageVue) : servie seulement si son jeton est connu
            # (« Access-Control-Allow-Origin » : la 3D de l'accueil peut s'en servir, une jaquette en fond de tuile)
            $c.AddWebResourceRequestedFilter('https://fichier.local/*', [Microsoft.Web.WebView2.Core.CoreWebView2WebResourceContext]::Image)
            $c.Add_WebResourceRequested({
                param($s2, $e2)
                $jeton = [IO.Path]::GetFileName(([uri]$e2.Request.Uri).AbsolutePath)
                $f = $script:Vue.Images[$jeton]
                $type = @{ '.png' = 'image/png'; '.jpg' = 'image/jpeg'; '.jpeg' = 'image/jpeg'; '.gif' = 'image/gif'; '.webp' = 'image/webp'; '.bmp' = 'image/bmp' }[[IO.Path]::GetExtension([string]$f).ToLowerInvariant()]
                if ($f -and $type -and [IO.File]::Exists($f)) {
                    try { $flux = New-Object IO.MemoryStream (, [IO.File]::ReadAllBytes($f)); $e2.Response = $s2.Environment.CreateWebResourceResponse($flux, 200, 'OK', "Content-Type: $type`r`nAccess-Control-Allow-Origin: *"); return } catch {}
                }
                $e2.Response = $s2.Environment.CreateWebResourceResponse($null, 404, 'Not Found', '')
            })
            # Le message est mis dans la file de la fenêtre au lieu d'être traité tout de suite : la WebView2 ne donne plus aucun
            # message tant que celui-ci n'est pas fini. Une question posée après un clic (Read-QuestionVue) attendrait donc sa
            # réponse pour toujours, et plus aucun bouton ne répondrait.
            $c.Add_WebMessageReceived({ param($s2, $e2)
                $json = $e2.WebMessageAsJson
                [void]$Window.Dispatcher.BeginInvoke([Action[string]]{ param($j) try { Receive-MessageVue $j } catch { Write-Log "Écran : message ignoré ($_)" } }, $json)
            })
            $c.Add_ProcessFailed({ param($s2, $e2) Write-Log "Écran : processus arrêté ($($e2.ProcessFailedKind)), rechargement"; $script:Vue.Pret = $false; try { $s2.Reload() } catch {} })
            $c.Navigate("https://$HoteVue/index.html")
        } catch { [void](Stop-VueImpossible "réglage impossible ($($_.Exception.Message))") }
    })
    # Recréée après une fermeture (Close-Vue) : l'image de l'écran prise avant reste affichée jusqu'à ce que la page
    # soit prête (pas d'écran vide pendant les 2 secondes de démarrage)
    if ($script:Vue.Instantane) {
        $img = New-Object System.Windows.Controls.Image
        $img.Source = $script:Vue.Instantane; $img.Stretch = 'Fill'
        $Hote.Children.Add($img) | Out-Null
        $script:Vue.Image = $img
        $vue.Visibility = 'Hidden'
    }
    $Hote.Children.Add($vue) | Out-Null
    $script:Vue.Controle = $vue
    # Le clavier (flèches, Entrée, Échap) va à l'écran dès que la fenêtre est active ou affichée dans Mindset
    if (-not $script:VueBranchee) { $script:VueBranchee = $true; $Window.Add_Activated({ try { [void]$script:Vue.Controle.Focus() } catch {} }) }
    $vue.Add_IsVisibleChanged({ param($s, $e) if ($e.NewValue) { try { [void]$s.Focus() } catch {} } })
    # (WebView2 ne démarre qu'une fois la boucle d'affichage lancée)
    [void]$Window.Dispatcher.BeginInvoke([action]{
        try { [void]$script:Vue.Controle.EnsureCoreWebView2Async() }
        catch {
            if ($script:Vue.Commun) { Reset-VueSansCommun $_.Exception.Message }
            else { [void](Stop-VueImpossible "démarrage impossible ($($_.Exception.Message))") }
        }
    }, [System.Windows.Threading.DispatcherPriority]::Background)

    # Regardée ou non (vérifié toutes les 0,4 s : ça ne coûte rien) : cachée, la vue est masquée (la page le sait et
    # s'arrête), puis mise en veille au bout d'une minute ; regardée à nouveau, elle revient tout de suite
    if (-not $script:VueVeille) {
        $script:VueVeille = New-Object System.Windows.Threading.DispatcherTimer
        $script:VueVeille.Interval = [TimeSpan]::FromMilliseconds(400)
        $script:VueVeille.Add_Tick({ try { if ($script:Vue.Controle) { Update-VeilleVue } } catch {} })
    }
    $script:VueVeille.Start()
}

# L'écran ne peut pas s'afficher : noté au journal, et l'appli prévenue ($true) si elle a son action « impossible »
#   Register-ActionVue 'impossible' { param($Raison) ... }
function Stop-VueImpossible([string]$Raison) {
    Write-Log "Écran impossible : $Raison"
    $a = $script:Vue.Actions['impossible']
    if (-not $a) { return $false }
    try { & $a $Raison $null } catch { Write-Log "Écran : $_" }
    $true
}

# Fermer l'écran tout à fait (WebView2 libérée) : une WebView2 présente dans la fenêtre fige les grosses animations
# WPF de l'appli (mesuré dans Nevermind : la carte du réseau figée 6 à 9 s). L'appli la ferme quand elle n'en a pas
# besoin et la recrée ensuite (Initialize-Vue) : environ 2 s, cachées par l'image de l'écran prise avant.
# $false tant que l'image de l'écran n'est pas finie (réessayer un peu plus tard).
function Close-Vue {
    $v = $script:Vue.Controle
    if (-not $v) { return $true }
    $cap = $script:Vue.Capture
    if ($cap -and -not $cap.Tache.IsCompleted -and ((Get-Date) - $cap.Debut).TotalSeconds -lt 1) { return $false }
    if ($cap) { Save-InstantaneVue }
    $script:Vue.Controle = $null; $script:Vue.Pret = $false; $script:Vue.CacheeDepuis = $null; $script:Vue.EnVeille = $false; $script:Vue.EnAttente = $null; $script:Vue.EtatEnAttente = $false
    try { $script:VueVeille.Stop() } catch {}
    try { $v.Parent.Children.Remove($v) } catch {}
    if ($script:Vue.Image) { try { $script:Vue.Image.Parent.Children.Remove($script:Vue.Image) } catch {}; $script:Vue.Image = $null }
    try { $v.Dispose() } catch {}
    Write-Log 'Écran fermé (recréé au retour)'
    $true
}

# L'image de l'écran, prise juste avant un geste qui peut le faire quitter (une action envoyée par la page)
function Start-InstantaneVue {
    $c = $script:Vue.Controle.CoreWebView2
    if (-not $c -or -not $script:Vue.Pret -or ($script:Vue.Capture -and -not $script:Vue.Capture.Tache.IsCompleted)) { return }
    try {
        $f = New-Object IO.MemoryStream
        $script:Vue.Capture = @{ Flux = $f; Debut = Get-Date; Tache = $c.CapturePreviewAsync([Microsoft.Web.WebView2.Core.CoreWebView2CapturePreviewImageFormat]::Png, $f) }
    } catch { $script:Vue.Capture = $null }
}
function Save-InstantaneVue {
    $cap = $script:Vue.Capture; $script:Vue.Capture = $null
    if (-not $cap -or -not $cap.Tache.IsCompleted -or $cap.Tache.IsFaulted -or -not $cap.Flux.Length) { return }
    try {
        $cap.Flux.Position = 0
        $b = New-Object System.Windows.Media.Imaging.BitmapImage
        $b.BeginInit(); $b.StreamSource = $cap.Flux; $b.CacheOption = 'OnLoad'; $b.EndInit(); $b.Freeze()
        $script:Vue.Instantane = $b
    } catch {} finally { $cap.Flux.Dispose() }
}

function Update-VeilleVue {
    $v = $script:Vue.Controle
    $h = (New-Object System.Windows.Interop.WindowInteropHelper $Window).Handle
    $regardee = [VueNatif]::EstRegardee($h)
    if ($regardee) {
        if (-not $script:Vue.CacheeDepuis) { return }
        $script:Vue.CacheeDepuis = $null
        if ($script:Vue.EnVeille) { try { $v.CoreWebView2.Resume() } catch {}; $script:Vue.EnVeille = $false }
        $v.Visibility = 'Visible'
        [void]$v.Focus()
        Write-Log 'Écran : de nouveau à l''écran'
        # Ce qui a attendu pendant la veille (voir Send-OrdreVue) : l'écran d'abord, puis les autres ordres
        $attente = @($script:Vue.EnAttente); $script:Vue.EnAttente = $null
        if ($script:Vue.EtatEnAttente) { $script:Vue.EtatEnAttente = $false; if ($script:Vue.Etat) { Send-OrdreVue $script:Vue.Etat } }
        foreach ($o in $attente) { if ($null -ne $o) { Send-OrdreVue $o } }
        return
    }
    if (-not $script:Vue.CacheeDepuis) {
        $script:Vue.CacheeDepuis = Get-Date
        $v.Visibility = 'Hidden'   # (WebView2 le dit à la page : plus de 3D, plus de minuteries)
        Write-Log 'Écran : caché, en pause'
        return
    }
    if (-not $script:Vue.EnVeille -and $v.CoreWebView2 -and ((Get-Date) - $script:Vue.CacheeDepuis).TotalSeconds -ge 60) {
        try { [void]$v.CoreWebView2.TrySuspendAsync(); $script:Vue.EnVeille = $true; Write-Log 'Écran : en veille' } catch {}
    }
}

# ---------------------------------------------------------------------------
# Parler à l'écran
# ---------------------------------------------------------------------------
# En veille (écran caché depuis une minute) : WebView2 n'est pas réveillée pour un envoi (elle se rendormait 0,4 s après, et
# recommençait à chaque envoi, même en pleine partie). L'ordre attend le retour à l'écran (Update-VeilleVue) : l'écran
# complet (Send-Vue) est gardé dans $script:Vue.Etat, les autres ordres dans une petite file (les 20 derniers).
function Send-OrdreVue($Objet) {
    $c = $script:Vue.Controle.CoreWebView2
    if (-not $c -or -not $script:Vue.Pret) { return }
    if ($script:Vue.EnVeille) {
        if ([object]::ReferenceEquals($Objet, $script:Vue.Etat)) { $script:Vue.EtatEnAttente = $true }
        else {
            if ($null -eq $script:Vue.EnAttente) { $script:Vue.EnAttente = New-Object System.Collections.ArrayList }
            [void]$script:Vue.EnAttente.Add($Objet)
            while ($script:Vue.EnAttente.Count -gt 20) { $script:Vue.EnAttente.RemoveAt(0) }
        }
        return
    }
    # (« $(if (...) { ... }) » sans « else » qui ne donne rien devient {} en JSON avec PowerShell 5.1 : remis à null)
    try { $c.PostWebMessageAsJson((($Objet | ConvertTo-Json -Depth 16 -Compress) -replace '":\{\}', '":null')) } catch { Write-Log "Écran : envoi impossible ($_)" }
}

# Ce qu'il faut montrer (accueil, pages...) : voir ui\kit.js. Gardé, et renvoyé si l'écran se recharge.
# Le thème de la suite et la connexion sont ajoutés ici, l'appli n'a pas à s'en occuper.
function Send-Vue($Etat) {
    if (-not $Etat.Contains('theme')) { $Etat.theme = Get-ThemeVue }
    if (-not $Etat.Contains('couleurs')) { $c = Get-CouleursVue; if ($c) { $Etat.couleurs = $c } }
    if (-not $Etat.Contains('connexion')) { $Etat.connexion = Get-ConnexionVue }
    if (-not $Etat.Contains('pack')) { $p = Get-PackVue; if ($p) { $Etat.pack = $p } }
    $script:Vue.Etat = $Etat
    Send-OrdreVue $Etat
}
# Une image n'importe où sur le PC (jaquette d'un jeu, photo) : son adresse pour l'écran (https://fichier.local/<jeton>).
# Seuls les fichiers déclarés ainsi sont servis, et seulement des images.
function Get-ImageVue([string]$Chemin) {
    if (-not $Chemin) { return $null }
    $cle = $Chemin.ToLowerInvariant()
    foreach ($p in $script:Vue.Images.GetEnumerator()) { if ($p.Value.ToLowerInvariant() -eq $cle) { return "https://fichier.local/$($p.Key)" } }
    $jeton = ([guid]::NewGuid().ToString('N')) + [IO.Path]::GetExtension($Chemin).ToLowerInvariant()
    $script:Vue.Images[$jeton] = $Chemin
    "https://fichier.local/$jeton"
}
function Show-MessageVue([string]$Texte, [switch]$Bravo) { Send-OrdreVue @{ kit = 'message'; texte = $Texte; bravo = [bool]$Bravo } }
# La pastille « en cours » en bas de l'écran (texte vide : fini)
function Set-OccupeVue([string]$Texte) { if ($script:Vue.Pret) { Send-OrdreVue @{ kit = 'occupe'; texte = $Texte } } }
# Une question dans l'écran, dans le style de l'appli ; attend la réponse (la fenêtre reste vivante) et renvoie l'id du
# bouton choisi ($null si l'écran n'est pas là : à l'appli de demander autrement).
#   Read-QuestionVue 'Continuer ?' -Boutons @(@{ id = 'oui'; texte = 'Oui' }, @{ id = 'non'; texte = 'Non'; style = 'second' })
#   $Details : lignes montrées sous la question dans un bloc repliable « Détails techniques », fermé au départ
function Read-QuestionVue([string]$Texte, [string]$Titre = '', [array]$Boutons = @(@{ id = 'oui'; texte = 'Oui' }, @{ id = 'non'; texte = 'Non'; style = 'second' }), [string[]]$Details) {
    if (-not $script:Vue.Pret -or -not $script:Vue.Controle -or $script:Vue.Controle.Visibility -ne 'Visible' -or $script:Vue.Cadre) {
        Write-Log "Question : pas posée à l'écran ($(if ($script:Vue.Cadre) { 'une autre question attend déjà' } elseif (-not $script:Vue.Pret) { 'écran pas prêt' } else { 'écran caché' }))"
        return $null
    }
    Write-Log 'Question : posée à l''écran'
    $id = [guid]::NewGuid().ToString('N').Substring(0, 10)
    $script:Vue.Reponse = $null
    $q = @{ kit = 'question'; id = $id; titre = $Titre; texte = $Texte; boutons = @($Boutons) }
    if (@($Details | Where-Object { $_ }).Count) { $q.details = @{ titre = 'Détails techniques'; lignes = @($Details | Where-Object { $_ }) } }
    Send-OrdreVue $q
    $cadre = New-Object System.Windows.Threading.DispatcherFrame
    $script:Vue.Cadre = $cadre
    # (si l'écran disparaît entre-temps, la question est abandonnée : la fenêtre ne reste jamais bloquée)
    $garde = New-Object System.Windows.Threading.DispatcherTimer
    $garde.Interval = [TimeSpan]::FromMilliseconds(500)
    $garde.Add_Tick({ if (-not $script:Vue.Pret -or -not $script:Vue.Controle -or -not $Window.IsLoaded) { if ($script:Vue.Cadre) { $script:Vue.Cadre.Continue = $false } } })
    $garde.Start()
    try { [System.Windows.Threading.Dispatcher]::PushFrame($cadre) } finally { $garde.Stop(); $script:Vue.Cadre = $null }
    if ($script:Vue.Reponse -and $script:Vue.Reponse.Id -eq $id) { Write-Log "Question : réponse « $($script:Vue.Reponse.Valeur) »"; return $script:Vue.Reponse.Valeur }
    Write-Log 'Question : abandonnée (l''écran a disparu)'
    [string]$Boutons[-1].id
}
function Open-EcranVue([string]$Ecran) { Send-OrdreVue @{ kit = 'aller'; ecran = $Ecran } }

# Le thème de la suite pour l'écran : un des 5 thèmes ; un thème perso garde le nom de sa base et donne ses couleurs
function Get-ThemeVue { if ($AppThemes.Keys -contains $ThemeId -and $ThemeId -notlike 'pack-*') { $ThemeId } else { 'neon' } }
function Get-CouleursVue {
    if ($ThemeId -notlike 'pack-*') { return $null }
    $t = [ordered]@{}
    foreach ($p in @{ P = '00E5FF'; S = 'B04BFF'; T = 'FF2EB5'; fond = '06050F'; fond2 = '0C0920' }.GetEnumerator()) { $t[$p.Key] = ConvertTo-ThemeHex "#$($p.Value)" }
    $t
}

# Les images d'un thème perso pour l'écran (themes.ps1, Get-ThemePack) : fond, logo, mascotte, dessin du bas, police,
# phrase d'accueil, et le bloc Console (images des tuiles, mascotte de l'accueil, chargement ; avec lui, son fond passe
# avant Background et il n'y a pas de dessin en bas). Servis depuis le dossier du thème (https://theme.local/, voir Initialize-Vue).
function Get-PackVue {
    $t = $ThemePack
    if (-not $t) { return $null }
    $u = { param($f) 'https://theme.local/' + [uri]::EscapeDataString([IO.Path]::GetFileName([string]$f)) }
    $p = [ordered]@{}
    $c = $t.Console
    # Console.Logo (une seule image, animée possible) passe avant Logo, qui reste l'image fixe des fenêtres
    if ($c -and $c.Logo) { $p.logo = @{ url = (& $u $c.Logo); images = 1; lueur = $t.LogoGlow } }
    elseif ($t.Logo) { $p.logo = @{ url = (& $u $t.Logo); images = $t.LogoFrames; delai = $t.LogoDelay; lueur = $t.LogoGlow } }
    $fond = if ($c -and $c.Fond) { $c.Fond } else { $t.Background }
    if ($fond) {
        $op = if ($c -and $c.Fond -and $c.FondOpacite) { $c.FondOpacite } else { $t.BackgroundOpacity }
        $p.fond = @{ url = (& $u $fond); opacite = $op
            x = @{ Left = 'gauche'; Right = 'droite' }[[string]$t.BackgroundAlignX]; y = @{ Top = 'haut'; Bottom = 'bas' }[[string]$t.BackgroundAlignY] }
    }
    if ($t.Mascotte) { $p.mascotte = @{ url = (& $u $t.Mascotte.File); images = $t.Mascotte.Frames; delai = $t.Mascotte.Delay; miroir = $t.Mascotte.Flip } }
    if ($c) {
        $tu = [ordered]@{}; foreach ($k in $c.Tuiles.Keys) { $tu[$k] = & $u $c.Tuiles[$k] }
        $p.console = [ordered]@{ tuiles = $tu }
        if ($c.Mascotte) { $p.console.mascotte = @{ url = (& $u $c.Mascotte) } }
        if ($c.Chargement) { $p.console.chargement = @{ url = (& $u $c.Chargement.File); effet = $c.Chargement.Effet } }
    }
    elseif ($t.Footer -and $t.Footer.File) { $p.bas = @{ url = (& $u $t.Footer.File); images = $t.Footer.Frames; delai = $t.Footer.Delay; hauteur = $t.Footer.Height } }
    elseif ($t.Footer -and $t.Footer.Files) { $p.bas = @{ url = (& $u @($t.Footer.Files)[0]); images = 1; hauteur = $t.Footer.Height } }
    if ($t.Font) { $p.police = @{ url = (& $u $t.Font); partout = ($t.FontScope -eq 'tout'); pixel = [bool]$t.FontPixel } }
    $p
}

# Câble, Wi-Fi ou rien : une carte réseau active avec une passerelle (relu au plus toutes les 30 secondes)
$script:ConnexionVue = @{ Valeur = 'aucune'; Lue = [datetime]::MinValue }
function Get-ConnexionVue {
    if (((Get-Date) - $script:ConnexionVue.Lue).TotalSeconds -lt 30) { return $script:ConnexionVue.Valeur }
    $v = 'aucune'
    try {
        foreach ($n in [System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces()) {
            if ($n.OperationalStatus -ne 'Up' -or $n.NetworkInterfaceType -in 'Loopback', 'Tunnel') { continue }
            if (-not @($n.GetIPProperties().GatewayAddresses | Where-Object { $_.Address.ToString() -notin '0.0.0.0', '::' }).Count) { continue }
            if ($n.NetworkInterfaceType -eq 'Wireless80211') { $v = 'wifi' } else { $v = 'cable'; break }   # (le câble l'emporte)
        }
    } catch {}
    $script:ConnexionVue = @{ Valeur = $v; Lue = Get-Date }
    $v
}

# ---------------------------------------------------------------------------
# Les gestes de l'utilisateur
# ---------------------------------------------------------------------------
# Register-ActionVue 'analyser' { param($Valeur, $Ecran) ... }   l'id d'une tuile, d'un bouton, d'une bascule, d'un champ...
# Register-ActionVue '*' { param($Id, $Valeur, $Ecran) ... }      toutes les autres
function Register-ActionVue([string]$Id, [scriptblock]$Action) { $script:Vue.Actions[$Id] = $Action }

function Receive-MessageVue([string]$Json) {
    $m = $Json | ConvertFrom-Json
    switch ([string]$m.type) {
        'pret' {
            $script:Vue.Pret = $true
            Write-Log 'Écran prêt'
            if ($script:Vue.Image) {   # (recréée : la vraie page remplace l'image, le temps qu'elle se dessine)
                $script:VueDevoiler = New-Object System.Windows.Threading.DispatcherTimer
                $script:VueDevoiler.Interval = [TimeSpan]::FromMilliseconds(250)
                $script:VueDevoiler.Add_Tick({
                    param($s) $s.Stop()
                    if ($script:Vue.Controle) { $script:Vue.Controle.Visibility = 'Visible'; [void]$script:Vue.Controle.Focus() }
                    if ($script:Vue.Image) { try { $script:Vue.Image.Parent.Children.Remove($script:Vue.Image) } catch {}; $script:Vue.Image = $null }
                })
                $script:VueDevoiler.Start()
            }
            if (-not $script:Vue.CacheeDepuis) { [void]$script:Vue.Controle.Focus() }   # (le clavier, si elle est déjà à l'écran)
            if ($script:Vue.Etat) { Send-OrdreVue $script:Vue.Etat }
            if ($EnTest -and $env:NEVERMIND_ECRAN) { Open-EcranVue $env:NEVERMIND_ECRAN }   # (tests : capture d'une page)
            if ($script:Vue.Actions['pret']) { & $script:Vue.Actions['pret'] $null $null }
        }
        'erreur' { Write-Log "Écran : erreur ($([string]$m.texte))" }
        'reponse' {   # La réponse à une question posée par Read-QuestionVue
            $script:Vue.Reponse = @{ Id = [string]$m.id; Valeur = [string]$m.valeur }
            if ($script:Vue.Cadre) { $script:Vue.Cadre.Continue = $false }
        }
        'ecran' {   # L'écran montré (accueil ou id d'une page) : $script:Vue.Ecran
            $script:Vue.Ecran = [string]$m.id
            if ($script:Vue.Actions['ecran']) { & $script:Vue.Actions['ecran'] $script:Vue.Ecran $null }
        }
        'menu' {   # Retour depuis l'accueil : dans Mindset, retour à la console
            if ($script:Vue.Actions['menu']) { & $script:Vue.Actions['menu'] $null $null }
            elseif ($script:Embedded) { Request-MenuMindset }
        }
        'action' {
            $id = [string]$m.id
            if ($id -notmatch '^[\w:.-]{1,60}$') { Write-Log "Écran : action refusée ($id)"; return }
            # Journal des gestes : chaque bouton appuyé (son nom seulement, jamais ce qui est tapé), le temps pris, l'erreur s'il y en a une
            $note = $id -notmatch '^(recherche|jeux-cherche|jeux-filtre|pret)$'
            $chrono = [Diagnostics.Stopwatch]::StartNew()
            if ($note) { Write-Log "Geste : $id (écran $([string]$m.ecran))" }
            try {
                Start-InstantaneVue   # (si ce geste quitte l'écran, son image servira au retour)
                if ($script:Vue.Actions[$id]) { & $script:Vue.Actions[$id] $m.valeur ([string]$m.ecran) }
                elseif ($script:Vue.Actions['*']) { & $script:Vue.Actions['*'] $id $m.valeur ([string]$m.ecran) }
                else { Write-Log "Écran : action sans suite ($id)" }
            } catch { Write-Log "Geste : $id en erreur ($($_.Exception.Message) $($_.InvocationInfo.PositionMessage))" }
            if ($note) { Write-Log "Geste : $id traité en $($chrono.ElapsedMilliseconds) ms" }
            if ($script:Vue.Actions['apres']) { & $script:Vue.Actions['apres'] $id ([string]$m.ecran) }   # (l'écran à jour après le geste)
        }
    }
}

# Test automatique : image de l'écran (WebView2 se dessine à part : la capture de la fenêtre WPF ne la voit pas)
function Save-CaptureVue([string]$Fichier, [scriptblock]$Ensuite) {
    $c = $script:Vue.Controle.CoreWebView2
    if (-not $c) { if ($Ensuite) { & $Ensuite $false }; return }
    $flux = [IO.File]::Create($Fichier)
    $tache = $c.CapturePreviewAsync([Microsoft.Web.WebView2.Core.CoreWebView2CapturePreviewImageFormat]::Png, $flux)
    $t = New-Object System.Windows.Threading.DispatcherTimer
    $t.Interval = [TimeSpan]::FromMilliseconds(150)
    $t.Tag = @{ Flux = $flux; Tache = $tache; Ensuite = $Ensuite }
    $t.Add_Tick({
        param($s)
        if (-not $s.Tag.Tache.IsCompleted) { return }
        $s.Stop(); $s.Tag.Flux.Dispose()
        if ($s.Tag.Ensuite) { & $s.Tag.Ensuite (-not $s.Tag.Tache.IsFaulted) }
    })
    $t.Start()
}
