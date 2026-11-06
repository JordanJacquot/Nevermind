//! Le score et les gains (`Measure-Score`, `Get-ScoreLook`, tableau-de-bord.ps1) et les points à régler
//! (`Get-HubFindings`, navigation.ps1).

use crate::modele::{Analyse, Constat, couleurs};
use crate::net;

/// `Measure-Score` : un point vert compte entièrement, un orange à 40 %, un rouge pas du tout. Le gain de chaque
/// constat qui compte (pas ignoré) est écrit dans le constat. Renvoie (score, potentiel en un clic).
pub fn mesurer(constats: &mut [Constat], ignores: &[String]) -> (i64, i64) {
    let ignore = |c: &Constat| ignores.iter().any(|x| x.eq_ignore_ascii_case(&c.id));
    let (mut total, mut obtenu, mut auto) = (0.0f64, 0.0f64, 0.0f64);
    let mut brut: Vec<Option<f64>> = vec![None; constats.len()];
    for (i, c) in constats.iter_mut().enumerate() {
        if ignore(c) || c.poids <= 0 || c.statut == "info" {
            continue;
        }
        let credit = match c.statut.as_str() {
            "ok" => 1.0,
            "warn" => 0.4,
            _ => 0.0,
        };
        total += c.poids as f64;
        obtenu += c.poids as f64 * credit;
        let g = c.poids as f64 * (1.0 - credit);
        c.gain = g;
        brut[i] = Some(g);
        if c.auto() && c.statut != "ok" {
            auto += g;
        }
    }
    let score = if total != 0.0 { net::entier(100.0 * obtenu / total) } else { 100 };
    let potentiel = if total != 0.0 { net::entier(100.0 * (obtenu + auto) / total) } else { 100 };
    for (i, c) in constats.iter_mut().enumerate() {
        if ignore(c) {
            continue;
        }
        let r = if total != 0.0 { brut[i].unwrap_or(0.0) } else { 0.0 };
        let mut g = if total != 0.0 { net::entier(100.0 * r / total) } else { 0 };
        if g > 0 {
            g = 1.max(net::entier(100.0 * (obtenu + r) / total) - score);
        }
        c.gain = g as f64;
    }
    (score, potentiel)
}

/// `Get-ScoreLook` : l'appréciation et sa couleur
pub fn apparence(score: i64) -> (&'static str, &'static str) {
    if score >= 85 {
        ("Excellent", couleurs::OK)
    } else if score >= 65 {
        ("Bien", "#9BE15D")
    } else if score >= 45 {
        ("À améliorer", couleurs::WARN)
    } else {
        ("Mal optimisé", couleurs::BAD)
    }
}

/// Ce que `Write-EtatMindset` (navigation.ps1:64-78) prend de l'analyse pour la carte de Nevermind dans Mindset : le
/// score, le premier point à régler (un important d'abord, sinon un à faire soi-même), s'il est urgent (rouge), et le
/// nombre de points à régler.
pub fn etat_mindset(a: &Analyse) -> serde_json::Value {
    let (importants, manuels) = a_regler(Some(a));
    let top = importants.first().or(manuels.first());
    serde_json::json!({
        "Score": a.score,
        "AFaire": top.map_or(String::new(), |c| c.titre.clone()),
        "Urgent": top.is_some_and(|c| c.statut == "bad"),
        "NbAFaire": importants.len() + manuels.len(),
    })
}

/// `Get-HubFindings` : les points importants (un vrai problème, ou ce que Nevermind fait en un clic), puis ceux à faire
/// soi-même ; chacun trié par gain.
pub fn a_regler(a: Option<&Analyse>) -> (Vec<Constat>, Vec<Constat>) {
    let Some(a) = a else { return (Vec::new(), Vec::new()) };
    let tous: Vec<&Constat> = a.actifs.iter().filter(|c| c.statut != "ok" && c.gain > 0.0).collect();
    let mut importants: Vec<Constat> =
        tous.iter().filter(|c| c.statut == "bad" || c.auto()).map(|c| (*c).clone()).collect();
    net::trier(&mut importants, |x, y| (x.statut != "bad").cmp(&(y.statut != "bad")).then(y.gain.total_cmp(&x.gain)));
    let mut manuels: Vec<Constat> =
        tous.iter().filter(|c| !(c.statut == "bad" || c.auto())).map(|c| (*c).clone()).collect();
    net::trier(&mut manuels, |x, y| y.gain.total_cmp(&x.gain));
    (importants, manuels)
}
