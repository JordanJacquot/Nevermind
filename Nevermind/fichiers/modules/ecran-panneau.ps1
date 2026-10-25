# Nevermind : le panneau de détail (tests des composants, partie mesurée, lag, analyse antivirus, fichiers d'un
# nettoyage, appareil du réseau, programme du trafic, audit...) montré dans l'écran (kit), en module déplié.
#
# Le panneau reste celui de toujours ($ui.TestOverlay, rempli par chaque fonctionnalité) : son contenu est lu élément
# par élément et traduit pour l'écran : textes, titres, encadrés de verdict, lignes de tableau, boutons et
# interrupteurs (qui appuient sur les vrais), les jauges en chiffres, les courbes (FPS, ping...) en courbes de l'écran,
# et les autres dessins en images nettes.
# Chargé par Nevermind.ps1 après ecran-protection.ps1.

$IconesPanneau = @{ FPS = 'jauge'; LAG = 'ondes'; AV = 'bouclier'; TMP = 'balai'; NET = 'courbe'; WIN = 'windows'; 'SÉCU' = 'bouclier'; DEL = 'poubelle'; '!' = 'aide' }

# Le texte d'un TextBlock (Text, ou ses morceaux quand il est fait de plusieurs parties)
function Get-TexteWpf($T) {
    $s = [string]$T.Text
    if (-not $s -and $T.Inlines.Count) { $s = (@($T.Inlines) | ForEach-Object { if ($_ -is [System.Windows.Documents.Run]) { $_.Text } elseif ($_ -is [System.Windows.Documents.LineBreak]) { ' ' } }) -join '' }
    $s.Trim()
}
# Le texte d'un bouton (son contenu, même quand c'est un petit assemblage d'icône et de texte)
function Get-ContenuWpf($B) {
    $c = $B.Content
    if ($c -is [string]) { return $c.Trim() }
    if ($c -is [System.Windows.Controls.TextBlock]) { return Get-TexteWpf $c }
    if ($c -is [System.Windows.Controls.Panel]) { return (@($c.Children | Where-Object { $_ -is [System.Windows.Controls.TextBlock] } | ForEach-Object { Get-TexteWpf $_ } | Where-Object { $_ -and $_.Length -gt 1 }) -join ' ') }
    [string]$c
}
# Une image (PNG intégré) d'un élément WPF : graphiques, courbes
function Get-ImageWpf($El) {
    $w = [int]$El.ActualWidth; $h = [int]$El.ActualHeight
    if ($w -lt 12 -or $h -lt 12 -or $w * $h -gt 2400000) { return $null }
    try {
        $dv = New-Object System.Windows.Media.DrawingVisual
        $ctx = $dv.RenderOpen()
        $ctx.DrawRectangle((New-Object System.Windows.Media.VisualBrush $El), $null, [System.Windows.Rect]::new(0, 0, $w, $h))
        $ctx.Close()
        $rtb = New-Object System.Windows.Media.Imaging.RenderTargetBitmap ([int]($w * 1.5)), ([int]($h * 1.5)), 144, 144, ([System.Windows.Media.PixelFormats]::Pbgra32)
        $rtb.Render($dv)
        $enc = New-Object System.Windows.Media.Imaging.PngBitmapEncoder
        $enc.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($rtb))
        $ms = New-Object IO.MemoryStream
        $enc.Save($ms)
        $b64 = [Convert]::ToBase64String($ms.ToArray()); $ms.Dispose()
        "data:image/png;base64,$b64"
    } catch { $null }
}
# Un élément est-il un dessin (courbe, radar) plutôt que du texte ?
function Test-DessinWpf($El) {
    if ($El -is [System.Windows.Controls.Canvas] -or $El -is [System.Windows.Shapes.Shape] -or $El -is [System.Windows.Controls.Image]) { return $true }
    if ($El -is [System.Windows.Controls.Panel]) {
        $formes = @($El.Children | Where-Object { $_ -is [System.Windows.Shapes.Shape] -or $_ -is [System.Windows.Controls.Canvas] }).Count
        $textes = @($El.Children | Where-Object { $_ -is [System.Windows.Controls.TextBlock] -or $_ -is [System.Windows.Controls.Control] }).Count
        return ($formes -ge 2 -and $formes -ge $textes)
    }
    $false
}

# Les dessins d'avant qui ont leur équivalent dans l'écran, lus à chaque fois (une jauge ou une courbe en direct suit) :
# une jauge (New-Gauge) devient un chiffre, une courbe (New-LiveChart) la courbe de l'écran, et un élément qui porte ses
# blocs tout prêts (BlocKit, comme la comparaison avant / après des FPS) les donne tels quels. Une image d'un dessin
# perdait ses chiffres (trop petite, ou pas encore dessinée) et faisait un état très lourd.
function Test-JaugeWpf($El) { $El -is [System.Windows.FrameworkElement] -and $El.Tag -is [hashtable] -and $El.Tag.JaugeKit }
function Get-ChiffreJaugeWpf($Tag) {
    $s = $Tag.JaugeKit
    @{ titre = [string]$Tag.Titre; valeur = ([string]$s.Fmt -f [double]$s.To); unite = [string]$Tag.Unite; ton = (Get-TonKit $Tag.Couleur) }
}
function Add-BlocKitWpf($El, $Blocs) {
    if ($El -isnot [System.Windows.FrameworkElement] -or $El.Tag -isnot [hashtable]) { return $false }
    $t = $El.Tag
    if ($t.BlocKit) { foreach ($b in @($t.BlocKit)) { [void]$Blocs.Add($b) }; return $true }
    if ($t.JaugeKit) { [void]$Blocs.Add(@{ type = 'chiffres'; elements = @(Get-ChiffreJaugeWpf $t) }); return $true }
    if ($t.CourbeKit) {
        $c = $t.CourbeKit
        $dec = if ([string]$c.Fmt -match 'N(\d)') { [int]$Matches[1] } else { 1 }
        $pts = @($c.Values | ForEach-Object { [math]::Round([double]$_, $dec) })
        if ($pts.Count -ge 2) {
            $b = @{ type = 'courbe'; points = $pts; unite = [string]$c.Unit
                titre = $(if ($c.Titre) { [string]$c.Titre } elseif ($c.RefValue -and $c.RefText.Text) { [string]$c.RefText.Text } else { '' }) }
            if ($c.Etiquettes) { $b.etiquettes = @($c.Etiquettes) }
            [void]$Blocs.Add($b)
        }
        return $true
    }
    $false
}

# Traduit un élément (et ce qu'il contient) en blocs du kit
function Add-BlocsWpf($El, $Blocs, $Etat) {
    if ($null -eq $El -or $Blocs.Count -gt 160) { return }
    if ($El -is [System.Windows.UIElement] -and $El.Visibility -ne 'Visible') { return }
    if (Add-BlocKitWpf $El $Blocs) { return }
    if ($El -is [System.Windows.Controls.TextBlock]) {
        $t = Get-TexteWpf $El; if (-not $t) { return }
        $ton = Get-TonKit $El.Foreground
        $gras = $El.FontWeight -eq [System.Windows.FontWeights]::Bold -or $El.FontWeight -eq [System.Windows.FontWeights]::SemiBold
        if ($t.Length -le 3 -and $t -match '^[✓!i×›]$') { return }   # (petite icône d'un verdict)
        if ($El.FontSize -ge 30 -and $t.Length -le 12) { [void]$Blocs.Add(@{ type = 'chiffres'; elements = @(@{ titre = ''; valeur = $t; ton = $ton }) }); return }
        if ($gras -and $t.Length -le 70 -and $t -notmatch '[.:]$' -and $t -cmatch '^[A-ZÀ-Ý0-9 ''/()-]{4,}$') { [void]$Blocs.Add(@{ type = 'titre'; texte = $t }); return }
        [void]$Blocs.Add($(if ($gras -and $ton) { @{ type = 'ligne'; icone = $(if ($ton -eq 'ok') { 'ok' } else { 'alerte' }); couleur = $(switch ($ton) { 'ok' { 'vert' } 'attention' { 'orange' } 'urgent' { 'rouge' } }); texte = $t } } elseif ($gras -and $t.Length -le 80) { @{ type = 'titre'; texte = $t } } else { @{ type = 'texte'; texte = $t } }))
        return
    }
    if ($El -is [System.Windows.Controls.CheckBox]) {
        [void]$Etat.Boutons.Add($El)
        $txt = Get-ContenuWpf $El
        [void]$Blocs.Add(@{ type = 'bascule'; id = "panneau:b:$($Etat.Boutons.Count - 1)"; texte = $(if ($txt) { $txt } else { 'Activer' }); oui = [bool]$El.IsChecked }); return
    }
    if ($El -is [System.Windows.Controls.Primitives.ButtonBase]) {
        [void]$Etat.Boutons.Add($El)
        $txt = Get-ContenuWpf $El; if (-not $txt) { return }
        $prim = $El.Style -and $El.Style -eq $Window.TryFindResource('BtnPrimary')
        [void]$Blocs.Add(@{ type = 'bouton'; id = "panneau:b:$($Etat.Boutons.Count - 1)"; texte = $txt; style = $(if (-not $prim) { 'second' }) }); return
    }
    if ('SansImageKit' -eq $El.Tag) { return }   # (un dessin animé d'avant que la console ne montre pas, le radar d'une analyse)
    if (Test-DessinWpf $El) {
        if ($El -is [System.Windows.Shapes.Shape] -and $El.ActualWidth -lt 24 -and $El.ActualHeight -lt 24) { return }   # (une pastille de décor)
        $img = Get-ImageWpf $El
        if ($img) { [void]$Blocs.Add(@{ type = 'image'; url = $img; hauteur = [int][math]::Min(320, [math]::Max(60, $El.ActualHeight)) }) }
        return
    }
    if ($El -is [System.Windows.Controls.Border]) {
        # Un encadré coloré avec seulement du texte : un verdict, un conseil
        $tb = @(Get-TextesWpf $El.Child)
        $fond = $El.Background -is [System.Windows.Media.SolidColorBrush] -and $El.Background.Opacity -gt 0.05 -and $El.Background.Color.A -gt 0
        if ($fond -and $tb.Count -ge 1 -and $tb.Count -le 3 -and -not (Test-ControlesWpf $El.Child)) {
            $principal = @($tb | Where-Object { (Get-TexteWpf $_).Length -gt 3 })[0]
            if ($principal) {
                $ton = Get-TonKit $principal.Foreground
                [void]$Blocs.Add($(if ($ton) { @{ type = 'ligne'; icone = $(if ($ton -eq 'ok') { 'ok' } else { 'alerte' }); couleur = $(switch ($ton) { 'ok' { 'vert' } 'attention' { 'orange' } 'urgent' { 'rouge' } }); texte = (Get-TexteWpf $principal) } } else { @{ type = 'note'; texte = (Get-TexteWpf $principal) } }))
                return
            }
        }
        Add-BlocsWpf $El.Child $Blocs $Etat; return
    }
    if ($El -is [System.Windows.Controls.Grid] -and $El.ColumnDefinitions.Count -ge 2 -and -not (Test-ControlesWpf $El)) {
        # Une ligne de tableau (nom, détail, valeur)
        $tb = @(Get-TextesWpf $El | ForEach-Object { Get-TexteWpf $_ } | Where-Object { $_ -and -not ($_.Length -le 2 -and $_ -match '^[✓!i×›]$') })
        if ($tb.Count -ge 2 -and -not @($tb | Where-Object { $_ -cmatch '[a-zà-ÿ]' }).Count) {   # (une ligne d'en-tête en capitales)
            [void]$Blocs.Add(@{ type = 'titre'; texte = $tb[0]; droite = $tb[-1] }); return
        }
        if ($tb.Count -ge 2) {
            $l = @{ type = 'ligne'; texte = $tb[0]; valeur = $tb[-1] }
            if ($tb.Count -gt 2) { $l.note = ($tb[1..($tb.Count - 2)] -join ' · ') }
            $dot = @($El.Children | Where-Object { $_ -is [System.Windows.Shapes.Ellipse] })[0]
            if ($dot) { $ton = Get-TonKit $dot.Fill; if ($ton) { $l.etiquette = $(switch ($ton) { 'ok' { 'Bon' } 'attention' { 'À voir' } 'urgent' { 'Problème' } }); $l.ton = $ton } }
            [void]$Blocs.Add($l); return
        }
        if ($tb.Count -eq 1) { [void]$Blocs.Add(@{ type = 'texte'; texte = $tb[0] }); return }
    }
    if ($El -is [System.Windows.Controls.Panel]) {
        $enfants = @($El.Children)
        # Une rangée de jauges (New-GaugeRow) : leurs chiffres côte à côte
        $vues = @($enfants | Where-Object { $_.Visibility -eq 'Visible' })
        if ($vues.Count -ge 2 -and -not @($vues | Where-Object { -not (Test-JaugeWpf $_) }).Count) {
            [void]$Blocs.Add(@{ type = 'chiffres'; elements = @($vues | ForEach-Object { Get-ChiffreJaugeWpf $_.Tag }) }); return
        }
        if ($El -is [System.Windows.Controls.Grid]) { $enfants = @($enfants | Sort-Object { [System.Windows.Controls.Grid]::GetRow($_) }, { [System.Windows.Controls.Grid]::GetColumn($_) }) }
        foreach ($x in $enfants) { Add-BlocsWpf $x $Blocs $Etat }
        return
    }
    if ($El -is [System.Windows.Controls.ScrollViewer] -or $El -is [System.Windows.Controls.ContentControl]) { Add-BlocsWpf $El.Content $Blocs $Etat; return }
    if ($El -is [System.Windows.Controls.Decorator]) { Add-BlocsWpf $El.Child $Blocs $Etat; return }
}
function Get-TextesWpf($El) {
    if ($null -eq $El) { return }
    if ($El -is [System.Windows.Controls.TextBlock]) { if ($El.Visibility -eq 'Visible') { $El }; return }
    if ($El -is [System.Windows.Controls.Panel]) { foreach ($x in $El.Children) { Get-TextesWpf $x } }
    elseif ($El -is [System.Windows.Controls.Decorator]) { Get-TextesWpf $El.Child }
}
function Test-ControlesWpf($El) {
    if ($null -eq $El) { return $false }
    if ($El -is [System.Windows.Controls.Primitives.ButtonBase] -or (Test-DessinWpf $El)) { return $true }
    if ($El -is [System.Windows.Controls.Panel]) { foreach ($x in $El.Children) { if (Test-ControlesWpf $x) { return $true } } }
    elseif ($El -is [System.Windows.Controls.Decorator]) { return (Test-ControlesWpf $El.Child) }
    $false
}

function Get-PanneauKit {
    $etat = @{ Boutons = New-Object System.Collections.ArrayList }
    $blocs = New-Object System.Collections.ArrayList
    Add-BlocsWpf $ui.TestBody $blocs $etat
    $script:Kit.PanneauBoutons = $etat.Boutons
    $tag = [string]$ui.TestTag.Text
    $ic = if ($IconesPanneau.ContainsKey($tag)) { $IconesPanneau[$tag] } else { 'test' }
    $etatTexte = if ($ui.TestStateText.Visibility -eq 'Visible') { [string]$ui.TestStateText.Text } else { '' }
    $ton = Get-TonKit $ui.TestStateText.Foreground
    $prog = [double]$ui.TestProgress.Value
    $enCours = $ui.BtnTestStop.Visibility -eq 'Visible'
    $boutons = @(
        if ($enCours) { @{ id = 'panneau:stop'; texte = [string]$ui.BtnTestStop.Content; style = 'danger' } }
        if ($ui.BtnTestAgain.Visibility -eq 'Visible') { @{ id = 'panneau:again'; texte = (Get-ContenuWpf $ui.BtnTestAgain) } }
        if ($ui.BtnTestClose.IsEnabled -and -not $enCours) { @{ id = 'panneau:close'; texte = 'Fermer'; style = 'second' } }
    )
    $module = if ($enCours -and $prog -gt 0 -and $prog -lt 100 -and $ui.TestPct.Text) {
        @{ anneau = @{ valeur = [int]$prog; sur = 100; petit = '%' }; titre = $(if ($etatTexte) { $etatTexte } else { 'En cours' }); texte = [string]$ui.TestSub.Text; boutons = $boutons }
    } else {
        @{ icone = $(if ($ton -eq 'ok') { 'ok' } elseif ($ton -in 'attention', 'urgent') { 'alerte' } else { $ic }); couleur = $(switch ($ton) { 'ok' { 'vert' } 'attention' { 'orange' } 'urgent' { 'rouge' } default { 'P' } })
            titre = $(if ($etatTexte) { $etatTexte } else { [string]$ui.TestTitle.Text }); texte = [string]$ui.TestSub.Text; boutons = $boutons }
    }
    @{ titre = [string]$ui.TestTitle.Text; sous = [string]$ui.TestSub.Text; icone = $ic; couleur = 'P'; retour = 'appli'
        parent = $(if ($script:Kit.PanneauDe -and $script:Kit.PanneauDe -notin 'accueil', 'panneau', 'fiche') { $script:Kit.PanneauDe })
        module = $module; blocs = @($blocs) }
}

# Appelé toutes les 200 ms (Update-EcranKitVisible) : le panneau ouvert est montré et suivi (test en cours, ping...)
function Update-PanneauKit {
    $ouvert = $ui.TestOverlay.Visibility -eq 'Visible'
    if ($ouvert) {
        # (les images sont refaites au plus une fois par seconde : le reste est relu à chaque fois)
        $maintenant = Get-Date
        if ($script:Kit.PanneauOuvert -and $script:Kit.PanneauVu -and ($maintenant - $script:Kit.PanneauVu).TotalMilliseconds -lt 900) { return }
        $script:Kit.PanneauVu = $maintenant
        $p = Get-PanneauKit
        $cle = $p | ConvertTo-Json -Depth 8 -Compress
        if ($cle -eq $script:Kit.PanneauCle) { return }
        $script:Kit.PanneauCle = $cle; $script:Kit.Panneau = $p
        $premiere = -not $script:Kit.PanneauOuvert
        if ($premiere) { $script:Kit.PanneauOuvert = $true; $script:Kit.PanneauDe = $(if ($script:Vue.Ecran) { $script:Vue.Ecran } else { 'accueil' }); $p.parent = $(if ($script:Kit.PanneauDe -notin 'accueil', 'panneau', 'fiche') { $script:Kit.PanneauDe }) }
        Update-EcranNevermind
        if ($premiere) { Open-EcranVue 'panneau' }
    } elseif ($script:Kit.PanneauOuvert) {
        $script:Kit.PanneauOuvert = $false; $script:Kit.Panneau = $null; $script:Kit.PanneauCle = ''; $script:Kit.PanneauVu = $null
        $de = $script:Kit.PanneauDe
        if ($script:Vue.Ecran -eq 'panneau') { Open-EcranVue $(if ($de -and $de -notin 'panneau', 'fiche') { $de } else { 'accueil' }) }
        Update-EcranNevermind
    }
}

function Register-ActionsPanneau {
    Register-ActionVue 'panneau:stop' { param($Valeur, $Ecran) Invoke-BoutonKit 'BtnTestStop' }
    Register-ActionVue 'panneau:again' { param($Valeur, $Ecran) Invoke-BoutonKit 'BtnTestAgain' }
    Register-ActionVue 'panneau:close' { param($Valeur, $Ecran) if ($ui.BtnTestClose.IsEnabled) { Invoke-BoutonKit 'BtnTestClose' } }
}
# Un bouton ou un interrupteur du panneau : le vrai est actionné
function Invoke-ActionPanneau([string]$Id, $Valeur) {
    if ($Id -notmatch '^panneau:b:(\d+)$') { return $false }
    $b = @($script:Kit.PanneauBoutons)[[int]$Matches[1]]
    if (-not $b -or -not $b.IsEnabled -or -not $b.IsVisible) { return $true }
    if ($b -is [System.Windows.Controls.CheckBox]) { $b.IsChecked = [bool]$Valeur }
    $b.RaiseEvent((New-Object System.Windows.RoutedEventArgs ([System.Windows.Controls.Primitives.ButtonBase]::ClickEvent)))
    $script:Kit.PanneauVu = $null   # (le panneau a pu changer : relu tout de suite)
    $true
}
