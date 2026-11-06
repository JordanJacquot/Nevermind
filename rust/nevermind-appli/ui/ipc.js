// Le seul fichier des pages qui parle à Rust (docs\regles-tauri.md du dépôt Mindset, règle 1) : withGlobalTauri est
// coupé, rien d'autre ne touche à l'objet interne de Tauri. Seules les trois commandes de l'appli sont appelées, et la
// fenêtre n'a la permission que de celles-là (capabilities\principale.json).
//
// C'est le pont du kit d'écrans (kit.js) : le kit attend le « webview » de WebView2 (postMessage vers l'appli, des
// messages « message » venus d'elle). Ce fichier, chargé AVANT kit.js (script classique), lui en donne un pareil,
// window.kitPont, qui passe par Tauri :
// - l'état et les ordres de l'appli arrivent par un canal (commande suivre_page), dans l'ordre où Rust les envoie ;
// - les messages du kit (pret, action, ecran, menu, erreur, reponse) partent par la commande action, qui les vérifie ;
// - la page dit à Rust quand elle devient visible ou cachée (commande noter_visibilite, pour le journal).
(() => {
  'use strict';
  const tauri = window.__TAURI_INTERNALS__;
  const ecouteurs = [];
  // L'état et les ordres arrivés avant que le kit n'écoute (il écoute dès son chargement, juste après ce fichier)
  const enAttente = [];

  // Un message du kit vers Rust : une copie en JSON simple (rien d'autre ne passe)
  function envoyer(message) {
    let copie;
    try { copie = JSON.parse(JSON.stringify(message)); } catch (e) { return; }
    if (!copie || typeof copie !== 'object' || Array.isArray(copie)) return;
    tauri.invoke('action', { message: copie }).catch(() => {});
  }

  function donner(d) {
    if (!ecouteurs.length) { enAttente.push(d); return; }
    for (const f of ecouteurs) {
      try { f({ data: d }); } catch (e) { envoyer({ type: 'erreur', texte: String((e && e.message) || e).slice(0, 300) }); }
    }
  }

  // Le canal de l'état : chaque envoi arrive avec son numéro ({ message, index }, puis { end, index } à la fermeture
  // du canal) ; les messages sont rendus dans l'ordre où Rust les a envoyés, comme le fait la bibliothèque JavaScript
  // de Tauri.
  function suivre() {
    let attendu = 0;
    const recus = new Map();
    const id = tauri.transformCallback((envoi) => {
      if (!envoi || typeof envoi.index !== 'number') return;
      recus.set(envoi.index, envoi);
      while (recus.has(attendu)) {
        const suivant = recus.get(attendu);
        recus.delete(attendu);
        attendu += 1;
        if (!suivant.end && suivant.message && typeof suivant.message === 'object') donner(suivant.message);
      }
    });
    const canal = { toJSON: () => '__CHANNEL__:' + id };
    return tauri.invoke('suivre_page', { canal });
  }

  window.kitPont = Object.freeze({
    postMessage: envoyer,
    addEventListener(type, f) {
      if (type !== 'message' || typeof f !== 'function') return;
      ecouteurs.push(f);
      enAttente.splice(0).forEach(donner);
    },
  });

  // Visible ou cachée : noté au journal par Rust (cachée, WebView2 met la page en pause, et le kit arrête sa 3D)
  function signalerVisibilite() {
    tauri.invoke('noter_visibilite', { visible: document.visibilityState === 'visible' }).catch(() => {});
  }
  document.addEventListener('visibilitychange', signalerVisibilite);
  suivre().catch(() => {});
  signalerVisibilite();
})();
