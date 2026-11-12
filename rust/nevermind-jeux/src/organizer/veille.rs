//! La veille de l'Organizer, sans Windows : Update-OrgWatch (fenêtres toutes les 2 s, fenêtre devant toutes les
//! 250 ms), Register-OrgHotkeys et Unregister-OrgHotkeys (raccourcis pris seulement quand une fenêtre du jeu est
//! devant), Show-OrgChar, ce que montre la barre flottante (Show-OrgBar, Update-OrgBar, Hide-OrgBar) et les gestes de
//! la vue (Set-OrgOn, Set-OrgCapturedKey, Move-OrgChar, Remove-OrgChar...).
//!
//! Tout ce qui touche Windows passe par le trait [`Bureau`] : le fil de l'Organizer lui donne le vrai Windows
//! (`windows::organizer`), les tests un faux, et rejouent les scénarios exportés de la version PowerShell.

use super::{
    ACTIONS, Config, Fenetre, Perso, en_ligne, envoi, id_action, meme_nom, nom_touche, simulee, souris, touche_windows,
};

/// Une pastille de la barre flottante (un perso connecté).
#[derive(Debug, Clone, PartialEq)]
pub struct Pastille {
    pub nom: String,
    pub classe: String,
    pub hwnd: isize,
    pub pid: u32,
    /// Le perso dont la fenêtre est devant (mis en avant).
    pub devant: bool,
    /// L'infobulle : « Nom (Classe) : touche ».
    pub infobulle: String,
}

/// Ce que la veille demande à Windows (le vrai, ou un faux pour les tests).
pub trait Bureau {
    /// Les fenêtres du jeu ouvertes (Get-DofusWindows).
    fn fenetres(&mut self) -> Vec<Fenetre>;
    /// La fenêtre devant ([WinFocus]::Foreground).
    fn devant(&mut self) -> isize;
    /// Le programme d'une fenêtre ([WinFocus]::Pid), 0 si aucun.
    fn pid(&mut self, hwnd: isize) -> u32;
    /// Met une fenêtre au premier plan ([WinFocus]::Focus).
    fn afficher(&mut self, hwnd: isize) -> bool;
    /// Prend un raccourci à Windows ([OGNative]::AddHotKey) : `false` s'il est déjà pris par un autre programme.
    fn prendre_touche(&mut self, id: i32, mods: u32, vk: u32) -> bool;
    fn rendre_touche(&mut self, id: i32);
    /// L'écoute des boutons de la souris ([MouseHook]).
    fn souris_vider(&mut self);
    fn souris_lier(&mut self, mods: u32, bouton: u32, id: i32);
    fn souris_demarrer(&mut self) -> bool;
    fn souris_arreter(&mut self);
    /// Appuie sur une touche dans la fenêtre devant (une touche simulée), comme au clavier : modificateurs
    /// (1 Alt, 2 Ctrl, 4 Maj, 8 Win) et code de touche. `false` si Windows l'a refusé.
    fn envoyer_touche(&mut self, mods: u32, vk: u32) -> bool;
    /// La souris est-elle sur la barre flottante ?
    fn survol_barre(&mut self) -> bool;
    /// La barre flottante : son contenu, montrée, cachée, remise au-dessus de tout.
    fn barre_dessiner(&mut self, pastilles: &[Pastille]);
    fn barre_montrer(&mut self);
    fn barre_cacher(&mut self);
    fn barre_devant(&mut self);
    /// La vue est à redessiner (Build-OrgPanel).
    fn vue_redessiner(&mut self, liste: &[Perso]);
    fn enregistrer(&mut self, c: &Config);
    fn journal(&mut self, m: &str);
    fn statut(&mut self, m: &str);
}

/// L'état de la veille ($script:Org...).
#[derive(Debug, Default)]
pub struct Veille {
    pub config: Config,
    /// La minuterie tourne (Start-OrgWatch).
    pub active: bool,
    tic: u64,
    deja: bool,
    sig: Option<String>,
    hwnds: Vec<isize>,
    pids: Vec<u32>,
    titres: Vec<String>,
    pub touches_prises: bool,
    /// Les raccourcis pris : identifiant, action ($script:OrgKeyIds).
    ids: Vec<(i32, String)>,
    /// Les raccourcis refusés par Windows : action, texte ($script:OrgKeyFailed).
    pub refusees: Vec<(String, String)>,
    resume: Option<String>,
    /// La barre : `None` pas encore créée, sinon visible ou non.
    barre: Option<bool>,
    barre_sig: Option<String>,
    barre_fg: Option<isize>,
    pub vue: bool,
    pub glisse: bool,
    /// L'action dont on choisit la touche ($script:OrgCapture).
    pub capture: Option<String>,
}

impl Veille {
    pub fn nouvelle(config: Config) -> Self {
        Veille { config, ..Default::default() }
    }

    fn enregistrer(&mut self, b: &mut dyn Bureau) {
        b.enregistrer(&self.config);
    }

    /// Get-OrgList : la liste, et la config enregistrée si elle a changé.
    pub fn liste(&mut self, b: &mut dyn Bureau) -> (Vec<Perso>, Vec<Fenetre>) {
        let f = b.fenetres();
        let (l, n) = super::liste(&mut self.config, &f);
        for _ in 0..n {
            self.enregistrer(b);
        }
        (l, f)
    }

    fn en_ligne(&mut self, b: &mut dyn Bureau) -> Vec<Perso> {
        en_ligne(&self.liste(b).0)
    }

    /// La fenêtre devant est-elle une fenêtre du jeu (sa fenêtre principale ou une autre du même programme) ?
    fn jeu_devant(&self, b: &mut dyn Bureau, fg: isize) -> bool {
        self.hwnds.contains(&fg) || {
            let p = b.pid(fg);
            p != 0 && self.pids.contains(&p)
        }
    }

    /// Un tic de la veille (Update-OrgWatch) : toutes les 250 ms, `complet` au démarrage et aux changements de réglage.
    pub fn passe(&mut self, b: &mut dyn Bureau, complet: bool) {
        self.tic += 1;
        if complet || self.tic.is_multiple_of(8) || !self.deja {
            let (list, fen) = self.liste(b);
            let sig = list
                .iter()
                .map(|x| format!("{}|{}|{}", x.nom, x.hwnd, if x.en_ligne { "True" } else { "False" }))
                .collect::<Vec<_>>()
                .join(";");
            self.hwnds.clear();
            self.pids.clear();
            for x in list.iter().filter(|x| x.en_ligne) {
                self.hwnds.push(x.hwnd);
                if x.pid != 0 {
                    self.pids.push(x.pid);
                }
            }
            if self.sig.as_deref() != Some(sig.as_str()) {
                self.sig = Some(sig);
                let vus: Vec<String> = list
                    .iter()
                    .filter(|x| x.en_ligne)
                    .map(|x| {
                        let n = if x.nom.is_empty() { "connexion" } else { x.nom.as_str() };
                        let c = if x.classe.is_empty() { String::new() } else { format!(" ({})", x.classe) };
                        format!("{n}{c} pid {}", x.pid)
                    })
                    .collect();
                let suite = if vus.is_empty() { String::new() } else { format!(" : {}", vus.join(", ")) };
                b.journal(&format!("Organizer : {} fenêtre(s) Dofus{suite}", vus.len()));
                for w in &fen {
                    if !self.titres.iter().any(|t| meme_nom(t, &w.titre)) {
                        self.titres.push(w.titre.clone());
                        b.journal(&format!("Organizer : titre vu « {} »", w.titre));
                    }
                }
                if self.vue && !self.glisse {
                    self.vue_redessiner(b);
                }
                self.barre_sig = None;
            }
            self.deja = true;
        }
        let fg = b.devant();
        let en_jeu = self.jeu_devant(b, fg);
        if en_jeu {
            self.prendre_touches(b);
        } else {
            self.rendre_touches(b);
        }
        let veut = self.config.barre
            && !self.hwnds.is_empty()
            && (en_jeu || self.config.barre_toujours || (self.barre.is_some() && b.survol_barre()));
        if veut {
            self.montrer_barre(b);
            if self.barre_fg != Some(fg) {
                self.barre_fg = Some(fg);
                self.dessiner_barre(b);
                self.barre_dessus(b);
            } else if self.tic.is_multiple_of(4) {
                self.barre_dessus(b);
            }
        } else {
            self.cacher_barre(b);
        }
    }

    /// Register-OrgHotkeys : les raccourcis pris à Windows (et les boutons de souris écoutés).
    fn prendre_touches(&mut self, b: &mut dyn Bureau) {
        if self.touches_prises {
            return;
        }
        self.ids.clear();
        let avant = self.refusees.len();
        b.souris_vider();
        let mut avec_souris = false;
        for a in ACTIONS {
            let id = id_action(a);
            let texte = self.config.touche(a);
            if let Some((m, bouton)) = souris(&texte) {
                b.souris_lier(m, bouton, id);
                self.ids.push((id, a.to_string()));
                avec_souris = true;
                self.refusees.retain(|(x, _)| x != a);
                continue;
            }
            let Some((m, vk)) = touche_windows(&texte) else {
                self.refusees.retain(|(x, _)| x != a);
                continue;
            };
            // (touche déjà réservée par un autre programme : Discord, logiciel du clavier, overlay...)
            if b.prendre_touche(id, m, vk) {
                self.ids.push((id, a.to_string()));
                self.refusees.retain(|(x, _)| x != a);
            } else {
                refuser(&mut self.refusees, a, texte);
            }
        }
        if avec_souris && !b.souris_demarrer() {
            b.journal("Organizer : boutons de souris indisponibles (écoute refusée par Windows).");
            for a in ACTIONS {
                let t = self.config.touche(a);
                if souris(&t).is_some() {
                    refuser(&mut self.refusees, a, t);
                }
            }
        }
        let mut r = self.refusees.clone();
        r.sort_by(|x, y| x.0.cmp(&y.0));
        let refus = if r.is_empty() {
            String::new()
        } else {
            format!(", refusés : {}", r.iter().map(|(k, v)| format!("{k} = {v}")).collect::<Vec<_>>().join(", "))
        };
        let resume = format!("{} pris{refus}", self.ids.len());
        if self.resume.as_deref() != Some(resume.as_str()) {
            b.journal(&format!("Organizer : Dofus devant, raccourcis {resume}"));
            self.resume = Some(resume);
        }
        if self.refusees.len() != avant && self.vue {
            self.vue_redessiner(b);
        }
        self.touches_prises = true;
    }

    /// Unregister-OrgHotkeys.
    pub fn rendre_touches(&mut self, b: &mut dyn Bureau) {
        if !self.touches_prises {
            return;
        }
        for (id, _) in &self.ids {
            b.rendre_touche(*id);
        }
        b.souris_arreter();
        b.souris_vider();
        self.ids.clear();
        self.touches_prises = false;
    }

    /// Un raccourci pris (touche ou bouton de souris) vient d'être appuyé : le perso visé passe devant.
    pub fn touche(&mut self, b: &mut dyn Bureau, id: i32) {
        let Some(a) = self.ids.iter().find(|x| x.0 == id).map(|x| x.1.clone()) else { return };
        if simulee(&a) {
            // (une touche simulée : la touche choisie, dans la fenêtre du jeu devant, et rien d'autre)
            let t = self.config.envoi(&a);
            let Some((m, vk)) = touche_windows(&t) else { return };
            // (pas une touche que l'Organizer prend lui-même : elle changerait de perso, ou se renverrait sans fin)
            if ACTIONS.iter().any(|x| touche_windows(&self.config.touche(x)) == Some((m, vk))) {
                b.journal(&format!(
                    "Organizer : {} est déjà un raccourci de l'organizer, pas envoyée.",
                    nom_touche(&t)
                ));
                return;
            }
            if !b.envoyer_touche(m, vk) {
                b.journal(&format!("Organizer : Windows a refusé la touche {}.", nom_touche(&t)));
            }
            return;
        }
        let c = self.cible(b, &a);
        self.afficher_perso(b, c.as_ref().map(|p| (p.nom.as_str(), p.hwnd)));
    }

    /// Get-OrgTarget.
    pub fn cible(&mut self, b: &mut dyn Bureau, action: &str) -> Option<Perso> {
        let on = self.en_ligne(b);
        let fg = b.devant();
        let p = b.pid(fg);
        super::cible(&on, action, fg, p)
    }

    /// Show-OrgChar : la fenêtre du perso devant (comme un clic dans la barre des tâches).
    pub fn afficher_perso(&mut self, b: &mut dyn Bureau, perso: Option<(&str, isize)>) -> bool {
        let Some((nom, hwnd)) = perso.filter(|p| p.1 != 0) else { return false };
        let ok = b.afficher(hwnd);
        if !ok {
            b.journal(&format!("Organizer : Windows a refusé d'afficher {nom}."));
        }
        self.dessiner_barre(b);
        ok
    }

    /// Un clic sur une pastille de la barre : le perso de ce nom.
    pub fn clic_pastille(&mut self, b: &mut dyn Bureau, nom: &str) {
        let p = self.en_ligne(b).into_iter().find(|p| meme_nom(&p.nom, nom));
        self.afficher_perso(b, p.as_ref().map(|p| (p.nom.as_str(), p.hwnd)));
    }

    /// Le bouton « Afficher » d'une ligne de la vue.
    pub fn afficher_fenetre(&mut self, b: &mut dyn Bureau, hwnd: isize) {
        self.afficher_perso(b, Some(("", hwnd)));
    }

    // ------------------------------------------------------------------ la barre flottante

    fn montrer_barre(&mut self, b: &mut dyn Bureau) {
        match self.barre {
            Some(visible) => {
                if !visible {
                    self.barre = Some(true);
                    b.barre_montrer();
                }
                if self.barre_sig != self.sig {
                    self.dessiner_barre(b);
                }
            }
            None => {
                self.barre = Some(false);
                self.dessiner_barre(b);
                self.barre = Some(true);
                b.barre_montrer();
                self.barre_dessus(b);
            }
        }
    }

    pub fn cacher_barre(&mut self, b: &mut dyn Bureau) {
        if self.barre == Some(true) {
            self.barre = Some(false);
            b.barre_cacher();
        }
    }

    fn barre_dessus(&mut self, b: &mut dyn Bureau) {
        if self.barre.is_some() {
            b.barre_devant();
        }
    }

    /// Update-OrgBar : une pastille par perso connecté, celui devant mis en avant.
    pub fn dessiner_barre(&mut self, b: &mut dyn Bureau) {
        if self.barre.is_none() {
            return;
        }
        self.barre_sig = self.sig.clone();
        let fg = b.devant();
        let fg_pid = b.pid(fg);
        let on = self.en_ligne(b);
        let pastilles: Vec<Pastille> = on
            .iter()
            .enumerate()
            .map(|(i, ch)| {
                let devant = ch.hwnd == fg || (fg_pid != 0 && ch.pid == fg_pid);
                let touche = self.config.touche(&format!("P{}", i + 1));
                let classe = if ch.classe.is_empty() { String::new() } else { format!(" ({})", ch.classe) };
                let t = if touche.is_empty() { String::new() } else { format!(" : {}", nom_touche(&touche)) };
                Pastille {
                    nom: ch.nom.clone(),
                    classe: ch.classe.clone(),
                    hwnd: ch.hwnd,
                    pid: ch.pid,
                    devant,
                    infobulle: format!("{}{classe}{t}", ch.nom),
                }
            })
            .collect();
        b.barre_dessiner(&pastilles);
    }

    /// La barre est-elle à l'écran ?
    pub fn barre_visible(&self) -> bool {
        self.barre == Some(true)
    }

    // ------------------------------------------------------------------ la vue et ses gestes

    fn vue_redessiner(&mut self, b: &mut dyn Bureau) {
        // (Build-OrgPanel relit la liste : un nouveau perso y entre)
        let (l, _) = self.liste(b);
        b.vue_redessiner(&l);
    }

    /// Show-OrgView.
    pub fn montrer_vue(&mut self, b: &mut dyn Bureau, oui: bool) {
        self.vue = oui;
        if oui {
            self.vue_redessiner(b);
        }
    }

    /// Start-OrgWatch.
    pub fn demarrer(&mut self, b: &mut dyn Bureau) {
        self.active = true;
        self.tic = 0;
        self.passe(b, true);
    }

    /// Stop-OrgWatch.
    pub fn arreter(&mut self, b: &mut dyn Bureau) {
        self.active = false;
        self.rendre_touches(b);
        self.cacher_barre(b);
    }

    /// Set-OrgOn : l'interrupteur « Activer l'organizer ».
    pub fn allumer(&mut self, b: &mut dyn Bureau, on: bool) {
        self.config.on = on;
        self.enregistrer(b);
        if on {
            self.demarrer(b);
        } else {
            self.arreter(b);
        }
        if self.vue {
            self.vue_redessiner(b);
        }
        b.statut(if on { "Organizer Dofus activé." } else { "Organizer Dofus coupé." });
    }

    /// Les interrupteurs de la barre (« Afficher la barre », « Toujours visible »). La version PowerShell refaisait
    /// une passe de veille même Organizer coupé (la barre restait alors affichée) : seulement s'il est allumé ici
    /// (DIVERGENCES.md).
    pub fn regler_barre(&mut self, b: &mut dyn Bureau, toujours: bool, oui: bool) {
        if toujours {
            self.config.barre_toujours = oui;
        } else {
            self.config.barre = oui;
        }
        self.enregistrer(b);
        if self.active {
            self.passe(b, true);
        }
    }

    /// Set-OrgCapturedKey : la touche choisie pour l'action en cours de choix.
    pub fn choisir_touche(&mut self, b: &mut dyn Bureau, texte: &str) {
        let Some(action) = self.capture.take() else { return };
        if envoi(&action) {
            // (la touche faite dans le jeu par la touche simulée de même numéro : « E1 » pour « S1 »)
            let s = format!("S{}", &action[1..]);
            super::mettre(&mut self.config.envois, &s, texte.to_string());
            self.enregistrer(b);
            self.vue_redessiner(b);
            b.statut(&format!("Organizer : {}.", nom_touche(texte)));
            return;
        }
        // (une même touche ne sert qu'à une action)
        if !texte.is_empty() {
            for (k, v) in self.config.touches.iter_mut() {
                if !meme_nom(k, &action) && meme_nom(v, texte) {
                    v.clear();
                }
            }
        }
        super::mettre(&mut self.config.touches, &action, texte.to_string());
        self.enregistrer(b);
        if self.touches_prises {
            self.rendre_touches(b);
        }
        self.vue_redessiner(b);
        b.statut(&format!("Organizer : {}.", nom_touche(&self.config.touche(&action))));
    }

    /// Un bouton de souris compte-t-il pendant le choix en cours ? Pas pour la touche faite dans le jeu par une
    /// touche simulée : celle-là se choisit au clavier.
    pub fn capture_souris(&self) -> bool {
        self.capture.as_deref().is_some_and(|a| !envoi(a))
    }

    /// Échap pendant le choix d'une touche.
    pub fn annuler_choix(&mut self, b: &mut dyn Bureau) {
        self.capture = None;
        self.vue_redessiner(b);
    }

    /// Un perso lâché sur un autre (glisser une ligne de la vue).
    pub fn deplacer(&mut self, b: &mut dyn Bureau, nom: &str, vers: &str) {
        if super::deplacer(&mut self.config, nom, vers) {
            self.enregistrer(b);
        }
        self.vue_redessiner(b);
        self.dessiner_barre(b);
    }

    /// Le bouton « Retirer » d'un perso pas connecté.
    pub fn retirer(&mut self, b: &mut dyn Bureau, nom: &str) {
        super::retirer(&mut self.config, nom);
        self.enregistrer(b);
        self.vue_redessiner(b);
    }

    /// « Remettre la barre en haut au centre » (Reset-OrgBarPosition, côté réglages).
    pub fn oublier_position(&mut self, b: &mut dyn Bureau) {
        self.config.barre_x = None;
        self.config.barre_y = None;
        self.enregistrer(b);
    }

    /// La barre glissée à une nouvelle place (en pixels indépendants de l'écran, comme WPF).
    pub fn placer_barre(&mut self, b: &mut dyn Bureau, x: f64, y: f64) {
        self.config.barre_x = Some(x);
        self.config.barre_y = Some(y);
        self.enregistrer(b);
    }

    /// La touche refusée pour une action (encore la même que dans les réglages) : la vue le dit.
    pub fn refusee(&self, action: &str) -> bool {
        self.refusees.iter().any(|(a, t)| a == action && meme_nom(t, &self.config.touche(action)))
    }
}

fn refuser(l: &mut Vec<(String, String)>, a: &str, t: String) {
    match l.iter_mut().find(|(x, _)| x == a) {
        Some(e) => e.1 = t,
        None => l.push((a.to_string(), t)),
    }
}
