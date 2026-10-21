// Le kit d'écrans des applis de la suite, DA « console d'un futur lointain » (validée par Jordan le 2026-10-21) :
// l'accueil en tuiles 3D à bord de plasma, les pages en « module déplié » (un module à gauche avec son anneau de plasma,
// le contenu à droite), des sous-onglets en pastilles. L'appli (PowerShell) ne dessine rien elle-même : elle envoie son
// état en JSON avec kit.etat({...}) ; chaque geste repart vers elle : { type: 'action', id, valeur, ecran }.
// La 3D (tuiles, anneaux, système solaire) est dans plasma.js ; sans 3D, le kit montre les mêmes écrans à plat.
//
// L'état envoyé par l'appli (tout est facultatif sauf app et accueil) :
//   app       : { nom, version, logo, phrase }       logo = image du dossier ui ; phrase = à côté du nom sur l'accueil
//   theme     : 'neon' | 'crepuscule' | 'terminal' | 'arcade' | 'rubis'      couleurs : { P, S, T, fond, ... } (thème perso)
//   connexion : 'cable' | 'wifi' | 'aucune'          sons : true | false
//   pack      : les images d'un thème perso (logo, fond, mascotte, bas, police, phrase), voir pack()
//   accueil   : { tuiles: [{ id, nom, icone | image | gros, petit, sous, couleur, fond, alerte }],
//                 ronds: [{ id, nom, icone, couleur }] }
//               Les tuiles sont aussi les onglets du haut ; les ronds, les petites icônes à droite du haut.
//               Une tuile ou un rond ouvre la page du même id si elle existe ; sinon il envoie son action.
//   pages     : { id: page }
//     page    : { titre, sous, icone, couleur, parent, actions: [bouton], module, onglets | sections | blocs }
//               parent : la page d'où l'on vient (le retour y ramène, son onglet reste allumé, « Parent › Titre »)
//               actions : les boutons en haut à droite de la page ({ id, texte, icone, style })
//               module : le module de gauche { anneau: { valeur, sur, texte, petit, unite, ton } | icone | image,
//                        titre, texte, boutons: [bouton], case: { id, texte, oui }, resume: [{ nom, valeur, ton }] }
//                        sans module : le contenu prend toute la largeur ; un bloc « score » en premier devient le module
//               onglets : [{ id, nom, module?, blocs, onglets: [...] }] des sous-onglets en pastilles (deux niveaux au plus) ;
//                         un onglet qui a son module remplace celui de la page tant qu'il est choisi
//               sections : l'ancien format ({ nom, blocs }), montré en sous-onglets
//               retour: 'appli' : B est envoyé à l'appli (action 'retour') au lieu de revenir en arrière
//            ou { titre, question: { etape, texte, choix: [{ id, texte }] } } une question à la fois
//            ou { titre, resultat: { etape, titre, badge, nom, texte, boutons: [bouton] } }
//     bloc    : { type: 'titre', texte, droite, direct }      un titre de bloc (direct : la pastille « En direct »)
//               { type: 'texte', texte } | { type: 'note', texte }   un paragraphe | un encadré « bon à savoir »
//               { type: 'score', valeur, sur, titre, texte }
//               { type: 'ligne', id?, texte, note, valeur, ton, fait, icone, couleur, image, etiquette, quand: [haut, bas],
//                 barre (0 à 100), bascule: { id, oui }, bascules: [{ id, oui, texte }], bouton: { id, texte, style },
//                 case: { id, oui, valeur } }   image : une petite image à la place de l'icône (icône d'un programme)
//                 ton : 'ok' | 'attention' | 'urgent' (couleur de la valeur et de l'étiquette)
//               { type: 'bascule', id, texte, note, oui } | { type: 'case', id, texte, note, valeur, oui }
//               { type: 'bouton', id, texte, style: 'second' | 'danger', icone }   (les boutons qui se suivent : une ligne)
//               { type: 'champ', id, texte, valeur, indice, recherche }   (envoyé en quittant le champ, sur A ou en tapant
//                 pour une recherche)
//               { type: 'choix', id, valeur, options: [{ id, texte }] }          un choix parmi plusieurs (pastilles)
//               { type: 'cartes', id, valeur, options: [{ id, titre, texte }] }  un choix en cartes (style du compteur)
//               { type: 'outils', elements: [{ id, nom, note, icone, couleur, etiquette, ton }] }   des cartes d'outils
//               { type: 'raccourcis', elements: [{ id, nom, icone, couleur }] }  des petits liens vers un outil
//               { type: 'mesures', elements: [{ nom, note, valeur (0 à 100), texte, ton }] }   des petits anneaux
//               { type: 'pastilles', elements: [{ texte, ton }] }                un résumé en pastilles
//               { type: 'reglages', rangs: [{ texte, note, bascule: { id, oui } | bouton: { id, texte } | lien: { id, texte }
//                 | touches: ['Ctrl', 'F'] }] }                                    une carte de réglages
//               { type: 'faq', elements: [{ question, reponse, icone }] }        des questions qui s'ouvrent
//               { type: 'jauge', texte, valeur (0 à 100), note, ton } | { type: 'liste', elements, ton }
//               { type: 'image', url, legende, hauteur } | { type: 'courbe', titre, points, etiquettes, unite, ton }
//               { type: 'mois', ... } | { type: 'rdv', id, heure, titre, lieu }
//               { type: 'grille', elements: [{ id, titre, sous, image, couleur, marque }] }   des jaquettes
//               { type: 'chiffres', elements: [{ titre, valeur, unite, ton }] } | { type: 'banniere', image, titre, sous }
//               { type: 'reseau', centre, appareils: [{ nom, icone, couleur, ping, attention, nouveau }] }   le système
//                 solaire : la box au centre, les appareils en orbite d'autant plus proche qu'ils répondent vite
//               { type: 'apercu', coin, style, fps, id }   l'aperçu de l'overlay ; un clic sur un coin envoie id avec le coin
//               { type: 'etapes', elements: [{ texte, fait }] }
//   Images : un fichier du dossier ui, https://images.local/<fichier>, ou https://fichier.local/<jeton> (Get-ImageVue)
//   ecran     : id d'une page pour y aller tout de suite (ou 'accueil')
//   focus     : l'élément à choisir (« t:<id> » une tuile, « l:<id> » une ligne...), par exemple au retour
//
// Messages envoyés à l'appli : { type: 'pret' } au chargement, { type: 'action', id, valeur, ecran } à chaque geste,
// { type: 'ecran', id } quand on change d'écran, { type: 'menu' } quand on fait Retour depuis l'accueil (dans Mindset :
// retour à la console).
// Ordres de l'appli en plus de l'état : { kit: 'message', texte, bravo }, { kit: 'aller', ecran }, { kit: 'pause' },
// { kit: 'reprendre' }, { kit: 'mesures', valeurs: { id: { valeur, texte, note, ton } } } (les mesures en direct d'un bloc
// « mesures » dont les éléments ont un id, sans redessiner la page), { kit: 'occupe', texte } (la pastille « en cours » ;
// texte vide : fini), { kit: 'question', id, titre, texte, boutons: [{ id, texte, style }] } (une question par-dessus
// l'écran ; la réponse revient en { type: 'reponse', id, valeur: id du bouton } ; Échap : le dernier bouton).
(() => {
  'use strict';
  // ---------------------------------------------------------------- petits sons de console (après le premier geste)
  const son = {
    actif: true, ctx: null,
    bip(f, d, v, type = 'sine', f2) {
      if (!this.actif || !this.ctx) return;
      try {
        const t = this.ctx.currentTime, o = this.ctx.createOscillator(), g = this.ctx.createGain();
        o.type = type; o.frequency.setValueAtTime(f, t); if (f2) o.frequency.exponentialRampToValueAtTime(f2, t + d);
        g.gain.setValueAtTime(0, t); g.gain.linearRampToValueAtTime(v, t + 0.005); g.gain.exponentialRampToValueAtTime(0.0001, t + d);
        o.connect(g).connect(this.ctx.destination); o.start(t); o.stop(t + d + 0.02);
      } catch (e) {}
    },
    tic() { this.bip(2100, 0.025, 0.02, 'square'); },
    ok() { this.bip(880, 0.05, 0.04, 'triangle'); setTimeout(() => this.bip(1320, 0.07, 0.035, 'triangle'), 45); },
    retour() { this.bip(990, 0.05, 0.035, 'triangle'); setTimeout(() => this.bip(660, 0.07, 0.03, 'triangle'), 45); },
    bascule() { this.bip(1500, 0.03, 0.03, 'square'); },
    bravo() { [784, 988, 1175, 1568].forEach((f, i) => setTimeout(() => this.bip(f, 0.12, 0.035, 'triangle'), i * 70)); },
  };
  const reveil = () => { try { son.ctx ??= new AudioContext(); son.ctx.resume?.(); } catch (e) {} };
  addEventListener('pointerdown', reveil, true); addEventListener('keydown', reveil, true);

  // ---------------------------------------------------------------- icônes (traits simples, 24 x 24)
  const I = {
    pc: '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>',
    tel: '<rect x="7" y="2" width="10" height="20" rx="2"/><path d="M11 18h2"/>',
    eclair: '<path d="M13 2 4 14h7l-1 8 9-12h-7z"/>',
    marche: '<path d="M12 3v8M6.3 6.3a8 8 0 1 0 11.4 0"/>',
    wifi: '<path d="M2 9a15 15 0 0 1 20 0M5 13a10 10 0 0 1 14 0M8.5 16.5a5 5 0 0 1 7 0M12 20h.01"/>',
    cable: '<rect x="7" y="2" width="10" height="9" rx="1.5"/><path d="M10 2v3M12 2v3M14 2v3M12 11v4c0 3-5 2-5 7"/>',
    aucune: '<path d="M2 9a15 15 0 0 1 20 0M5 13a10 10 0 0 1 14 0M12 20h.01M3 3l18 18"/>',
    courbe: '<path d="M3 3v18h18M7 15l4-4 3 3 6-7"/>',
    loupe: '<circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/>',
    actu: '<path d="M20 12a8 8 0 1 1-2.3-5.7M20 4v5h-5"/>',
    reglages: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>',
    aide: '<circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6V14M12 17h.01"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    agenda: '<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M3 10h18M8 3v4M16 3v4"/>',
    cloche: '<path d="M6 8a6 6 0 1 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10 21h4"/>',
    coeur: '<path d="M3 12h4l2-5 4 10 2-5h6"/>',
    liste: '<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>',
    maison: '<path d="M3 11 12 3l9 8M5 9v12h14V9"/>',
    bouclier: '<path d="M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6z"/>',
    poubelle: '<path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14"/>',
    balai: '<path d="M3 21h18M6 21l1-6h10l1 6M12 3v12"/><path d="M8 15c0-2 1.8-3 4-3s4 1 4 3"/>',
    dossier: '<path d="M3 6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
    horloge: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
    sauve: '<path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5M12 7v5l3 2"/>',
    euro: '<path d="M18 6a7 7 0 1 0 0 12M4 10h10M4 14h10"/>',
    etoile: '<path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1 6.2L12 17.3 6.5 20.2l1-6.2L3 9.6l6.2-.9z"/>',
    son: '<path d="M4 9v6h4l5 4V5L8 9zM16 9a4 4 0 0 1 0 6M19 6a8 8 0 0 1 0 12"/>',
    manette: '<path d="M6 9h4M8 7v4M15 10h.01M18 8h.01"/><path d="M7 4h10a5 5 0 0 1 5 5v4a4 4 0 0 1-7 2.6L13.6 14h-3.2L9 15.6A4 4 0 0 1 2 13V9a5 5 0 0 1 5-5z"/>',
    radar: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="5"/><path d="M12 12 19 5"/>',
    jauge: '<path d="M4 18a9 9 0 1 1 16 0M12 13l4-5"/>',
    puce: '<rect x="5" y="5" width="14" height="14" rx="2"/><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"/>',
    memoire: '<rect x="2" y="7" width="20" height="10" rx="1.5"/><path d="M6 11v2M10 11v2M14 11v2M18 11v2M5 17v3M19 17v3"/>',
    disque: '<rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="12" cy="11" r="4"/><path d="M7 17h2"/>',
    test: '<path d="M9 3h6M10 3v6l-5 9a2 2 0 0 0 1.7 3h10.6a2 2 0 0 0 1.7-3l-5-9V3"/><path d="M7.5 15h9"/>',
    rapport: '<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6M8 13h8M8 17h5"/>',
    alerte: '<path d="M12 3 2 20h20zM12 10v4M12 17h.01"/>',
    ok: '<circle cx="12" cy="12" r="9"/><path d="M8 12l3 3 5-6"/>',
    app: '<rect x="4" y="4" width="16" height="16" rx="4"/><path d="M9 12h6"/>',
    windows: '<path d="M3 5.5l7.5-1v7H3zM12.5 4.2 21 3v8.5h-8.5zM3 13h7.5v7L3 19zM12.5 13H21v8l-8.5-1.2z"/>',
    ondes: '<path d="M2 9a15 15 0 0 1 20 0M5 12.5a10 10 0 0 1 14 0M8.5 16a5 5 0 0 1 7 0"/><circle cx="12" cy="19.5" r="1"/>',
    ecran: '<rect x="2" y="6" width="20" height="12" rx="2"/><circle cx="15" cy="12" r="3"/><path d="M6 10h3M6 14h3"/>',
    box: '<rect x="3" y="12" width="18" height="7" rx="2"/><path d="M7 12 5 5M17 12l2-7M7 16h.01M11 16h.01"/>',
    portable: '<rect x="5" y="5" width="14" height="10" rx="1.5"/><path d="M2 19h20l-2-4H4z"/>',
    tablette: '<rect x="4" y="3" width="16" height="18" rx="2.5"/><path d="M11 18h2"/>',
    console: '<path d="M6 9h4M8 7v4M15 10h.01M18 8h.01"/><path d="M7 4h10a5 5 0 0 1 5 5v4a4 4 0 0 1-7 2.6L13.6 14h-3.2L9 15.6A4 4 0 0 1 2 13V9a5 5 0 0 1 5-5z"/>',
    tv: '<rect x="2" y="4" width="20" height="13" rx="2"/><path d="M7 21l2-4M17 21l-2-4"/>',
    camera: '<path d="M3 7h12l3 3v4l-3 3H3z"/><circle cx="9" cy="12" r="2.5"/><path d="M18 11l3-2v6l-3-2"/>',
    imprimante: '<path d="M6 9V3h12v6"/><rect x="3" y="9" width="18" height="8" rx="2"/><path d="M6 14h12v7H6z"/>',
    enceinte: '<rect x="6" y="2" width="12" height="20" rx="3"/><circle cx="12" cy="14" r="3.5"/><path d="M12 6h.01"/>',
    ampoule: '<path d="M9 18h6M10 21h4M12 3a6 6 0 0 0-4 10.5c.8.8 1 1.6 1 2.5h6c0-.9.2-1.7 1-2.5A6 6 0 0 0 12 3z"/>',
    repeteur: '<path d="M5 12a10 10 0 0 1 14 0M8 15a6 6 0 0 1 8 0"/><rect x="8" y="17" width="8" height="5" rx="1.5"/>',
    inconnu: '<circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6V14M12 17h.01"/>',
    internet: '<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3c3 3.5 3 14.5 0 18M12 3c-3 3.5-3 14.5 0 18"/>',
    fleche: '<path d="M9 5l7 7-7 7"/>',
    retour: '<path d="M15 5l-7 7 7 7"/>',
    croix: '<path d="M6 6l12 12M18 6 6 18"/>',
    coche: '<path d="M5 12l5 5 9-10"/>',
  };
  window.kitIcones = I;   // (plasma.js dessine les icônes des tuiles avec)
  const ico = (n, w = 2) => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${I[n] || I.etoile}</svg>`;
  const esc = (t) => String(t ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const couleur = (c) => (c && /^[a-z-]+$/i.test(c) ? `var(--${c})` : esc(c || 'var(--P)'));   // 'P', 'vert'... ou une couleur CSS
  const tons = { ok: 'vert', attention: 'orange', urgent: 'rouge' };
  const heure = () => new Date().toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' });
  const CONNEXIONS = { cable: 'Câble', wifi: 'Wi-Fi', aucune: 'Pas de connexion' };
  const borne = (v, a, b) => Math.max(a, Math.min(b, v));
  // Une liste d'un seul élément envoyée par PowerShell arrive souvent sans ses crochets (un objet seul) : tout champ qui
  // doit être une liste est remis en liste, à tous les niveaux, dès l'arrivée de l'état
  const LISTES = new Set(['tuiles', 'ronds', 'blocs', 'onglets', 'sections', 'elements', 'rangs', 'boutons', 'resume', 'options', 'appareils', 'actions', 'choix', 'points', 'bascules', 'touches', 'quand']);
  function normaliser(o) {
    if (Array.isArray(o)) { o.forEach(normaliser); return o; }
    if (!o || typeof o !== 'object') return o;
    for (const [k, v] of Object.entries(o)) {
      if (LISTES.has(k) && v != null && !Array.isArray(v)) o[k] = [v];
      normaliser(o[k]);
    }
    return o;
  }

  // ---------------------------------------------------------------- liaison avec l'appli
  const webview = window.chrome?.webview;
  function envoyer(msg) {
    if (webview) webview.postMessage(msg);
    kit.surMessage?.(msg);      // pour les essais dans un navigateur
  }
  // L'appli envoie soit un état (voir plus haut), soit un ordre : { kit: 'message', texte, bravo } | { kit: 'aller', ecran }
  // | { kit: 'pause' } | { kit: 'reprendre' }
  // Une erreur de l'écran est notée dans le journal de l'appli (les tests échouent s'il y en a une)
  const signaler = (texte) => { try { webview?.postMessage({ type: 'erreur', texte: String(texte).slice(0, 300) }); } catch (e) {} };
  addEventListener('error', (e) => signaler(`${e.message} (${(e.filename || '').split('/').pop()}:${e.lineno})`));
  addEventListener('unhandledrejection', (e) => signaler(e.reason?.message || e.reason));
  webview?.addEventListener('message', (e) => {
    const d = e.data; if (!d || typeof d !== 'object') return;
    if (d.kit === 'message') kit.message(d.texte, d.bravo);
    else if (d.kit === 'aller') kit.aller(d.ecran);
    else if (d.kit === 'pause') kit.pause();
    else if (d.kit === 'reprendre') kit.reprendre();
    else if (d.kit === 'mesures') kit.mesures(d.valeurs);
    else if (d.kit === 'occupe') kit.occupe(d.texte);
    else if (d.kit === 'question') kit.question(d);
    else if (!d.kit) kit.etat(d);
  });

  // ---------------------------------------------------------------- l'état
  const ecran = document.getElementById('ecran'), haut = document.getElementById('haut');
  let E = { app: { nom: '' }, accueil: { tuiles: [], ronds: [] }, pages: {} };
  const nav = { ecran: 'accueil', onglets: {}, sel: 0 };   // onglets : les pastilles choisies, par page
  let enAttente = false;    // un nouvel état arrivé pendant qu'on tape dans un champ
  const plasma = () => (window.kitPlasma?.ok ? window.kitPlasma : null);

  // ---------------------------------------------------------------- le thème perso (images, police)
  const dejaPose = {};
  function poser(id, cle, faire) {
    if (dejaPose[id] === cle) return;
    dejaPose[id] = cle; const el = document.getElementById(id); el.innerHTML = '';
    if (cle) faire(el);
  }
  // Une adresse d'image sûre : du thème (https://theme.local/...), des images de l'appli ou un fichier du dossier ui, sans
  // guillemet ni parenthèse ; ou une image intégrée (data:), dont les parenthèses et apostrophes sont codées
  const urlSure = (u) => {
    if (typeof u !== 'string') return '';
    if (/^(https:\/\/(theme|images|fichier)\.local\/)?[\w%-][\w%./-]*$/.test(u) && !u.includes('..')) return u;
    if (/^data:image\/(png|svg\+xml)[;,]/.test(u) && !/["\\\s]/.test(u)) return u.replace(/\(/g, '%28').replace(/\)/g, '%29').replace(/'/g, '%27');
    return '';
  };
  window.kitUrlSure = urlSure;
  function planche(hote, p, hauteur, pixel) {   // une image animée : planche d'images côte à côte, ou une image qui lévite
    const url = urlSure(p?.url); if (!url) return;
    const img = new Image();
    img.onload = () => {
      const n = Math.max(1, Math.min(64, (p.images | 0) || 1)), h = hauteur, w = Math.round((img.naturalWidth / n) * (h / img.naturalHeight));
      const el = document.createElement('span');
      el.className = `planche${pixel ? ' pixel' : ''}${p.miroir ? ' miroir' : ''}${n === 1 ? ' levite' : ''}`;
      Object.assign(el.style, { width: `${w}px`, height: `${h}px`, backgroundImage: `url('${url}')`, backgroundSize: `${w * n}px ${h}px` });
      if (p.lueur && /^#[0-9a-f]{6}$/i.test(p.lueur)) el.style.filter = `drop-shadow(0 0 8px ${p.lueur})`;
      hote.appendChild(el);
      if (n > 1) el.animate([{ backgroundPositionX: '0px' }, { backgroundPositionX: `-${w * n}px` }], { duration: n * Math.max(30, (p.delai | 0) || 100), iterations: Infinity, easing: `steps(${n})` });
    };
    img.src = url;
  }
  function pack() {
    const p = E.pack || {}, pixel = !!p.police?.pixel;
    const d = document.getElementById('decor'), fu = urlSure(p.fond?.url);
    d.classList.toggle('vu', !!fu);
    if (fu) {
      d.style.setProperty('--img', `url('${fu}')`);
      d.style.setProperty('--op', String(Math.min(1, Math.max(0.05, +p.fond.opacite || 0.35))));
      d.style.setProperty('--pos', `${{ gauche: 'left', droite: 'right' }[p.fond.x] || 'center'} ${{ haut: 'top', bas: 'bottom' }[p.fond.y] || 'center'}`);
    }
    let st = document.getElementById('policePack'); const pu = urlSure(p.police?.url);
    if (!pu) st?.remove();
    else {
      if (!st) { st = document.createElement('style'); st.id = 'policePack'; document.head.appendChild(st); }
      st.textContent = `@font-face { font-family: "PolicePack"; src: url('${pu}'); }
        ${p.police.partout ? 'body' : '.marque, .nom-sel, h1, h3, .module .grand, .question h2'} { font-family: "PolicePack", var(--police-app); }`;
    }
    poser('mascotte', p.mascotte ? JSON.stringify(p.mascotte) : '', (el) => planche(el, p.mascotte, 52, pixel));
    poser('dessinBas', p.bas ? JSON.stringify(p.bas) : '', (el) => planche(el, p.bas, Math.min(48, Math.max(16, (p.bas.hauteur | 0) || 32)), pixel));
  }

  // ---------------------------------------------------------------- le haut : logo et nom (retour à l'accueil),
  // les onglets (les tuiles de l'accueil), les petites icônes (les ronds), la connexion et l'heure
  // La page « racine » d'une page : celle de sa tuile (en suivant les parents)
  function racine(id) {
    for (let k = 0; k < 8 && E.pages?.[id]?.parent; k++) id = E.pages[id].parent;
    return id;
  }
  function enTete() {
    const a = E.app || {}, p = E.pack || {}, surAccueil = nav.ecran === 'accueil';
    poser('logoHote', JSON.stringify([a.logo, p.logo]), (el) => {
      if (p.logo) planche(el, p.logo, 40, !!p.police?.pixel);   // le logo du thème perso, à la place de celui de l'appli
      else if (urlSure(a.logo)) el.innerHTML = `<span class="logo" id="logo" style="--logo:url('${urlSure(a.logo)}')"><i class="b"></i><i class="c c1"></i><i class="c c2"></i></span>`;
    });
    const phrase = surAccueil && (p.phrase || a.phrase);
    document.getElementById('titreTexte').innerHTML = `${esc(a.nom)}${phrase ? `<span class="phrase">${esc(phrase)}</span>` : ''}`;
    const r = racine(nav.ecran), t = E.accueil?.tuiles || [], ronds = E.accueil?.ronds || [];
    const cle = JSON.stringify([t.map((x) => [x.id, x.nom, x.alerte]), ronds.map((x) => [x.id, x.nom, x.icone]), r]);
    if (haut.dataset.cle !== cle) {
      haut.dataset.cle = cle;
      document.getElementById('onglets').innerHTML = t.map((x, i) => `<button type="button" class="onglet f${x.id === r ? ' actif' : ''}" data-k="h:${esc(x.id)}" data-tuile="${i}">${esc(x.nom)}${x.alerte ? '<i class="point"></i>' : ''}</button>`).join('');
      document.getElementById('rondsHaut').innerHTML = ronds.map((x, i) => `<button type="button" class="rond f${x.id === r ? ' actif' : ''}" data-k="r:${esc(x.id)}" data-rond="${i}" title="${esc(x.nom)}" aria-label="${esc(x.nom)}" style="--a:${couleur(x.couleur)}">${ico(x.icone, 1.8)}</button>`).join('');
    }
    const cnx = E.connexion && CONNEXIONS[E.connexion] ? `<span class="cnx" title="${CONNEXIONS[E.connexion]}">${ico(E.connexion)}</span>` : '';
    document.getElementById('etatHaut').innerHTML = `${cnx}<span class="hh">${heure()}</span>`;
    document.getElementById('basG').textContent = a.version && surAccueil ? `${a.nom} ${a.version}` : '';   // (sur l'accueil seulement : sur une page, il passerait sous le module)
  }

  // ---------------------------------------------------------------- l'accueil : la rangée de tuiles
  function accueil(focus, anime) {
    nav.ecran = 'accueil'; enTete(); direEcran();
    const t = E.accueil?.tuiles || [];
    let sel = nav.sel;
    if (focus && focus.startsWith('t:')) { const k = t.findIndex((x) => `t:${x.id}` === focus); if (k >= 0) sel = k; }
    nav.sel = sel = borne(sel, 0, Math.max(0, t.length - 1));
    const p = plasma();
    if (p) {
      // La 3D : plasma.js dessine les tuiles dans la toile ; le kit garde le nom, le clavier et les sons
      const donnees = t.map((x) => ({ id: x.id, nom: x.nom, sous: x.sous, icone: x.icone, gros: x.gros, petit: x.petit, couleur: x.couleur, image: urlSure(x.image), fond: urlSure(x.fond), alerte: x.alerte }));
      if (!document.getElementById('accueil3d')) {
        ecran.innerHTML = `<div class="accueil3d${anime ? ' entree' : ''}" id="accueil3d"><div class="nom-sel" id="nomSel"></div><div class="aide-accueil">Fais glisser la rangée ou utilise les flèches ; clic (ou A) pour ouvrir</div></div>`;
      }
      p.accueil(donnees, sel, anime);
      nomSel(sel);
      return;
    }
    // Sans 3D : les tuiles à plat
    ecran.innerHTML = `<div class="accueil${anime ? ' entree' : ''}"><div class="nom-sel" id="nomSel"></div>
      <div class="rangee">${t.map((x, i) => `<button type="button" class="tuile f${urlSure(x.fond) ? ' avec-fond' : ''}" data-k="t:${esc(x.id)}" data-tuile="${i}" style="--a:${couleur(x.couleur)}${urlSure(x.fond) ? `;--fond:url('${urlSure(x.fond)}')` : ''}" aria-label="${esc(x.nom)}">
        ${x.gros != null ? `<span class="gros">${esc(x.gros)}${x.petit ? `<small>${esc(x.petit)}</small>` : ''}</span>` : (urlSure(x.image) ? `<span class="ico"><img src="${urlSure(x.image)}" alt="" onerror="this.remove()"></span>` : `<span class="ico">${ico(x.icone, 1.8)}</span>`)}<b>${esc(x.nom)}</b><span class="sous">${esc(x.sous)}</span></button>`).join('')}</div></div>`;
    const cible = ecran.querySelector(`[data-tuile="${sel}"]`) || ecran.querySelector('.tuile');
    cible?.focus({ preventScroll: true }); cible?.scrollIntoView({ block: 'nearest', inline: 'center' });
    nomSel(sel);
  }
  function nomSel(i) {
    const ns = document.getElementById('nomSel'), x = E.accueil?.tuiles?.[i]; if (!ns || !x) return;
    ns.textContent = x.nom; ns.style.color = couleur(x.couleur);
  }
  // plasma.js prévient quand la tuile choisie change, ou quand on ouvre celle du milieu
  document.addEventListener('kit:tuile', (e) => { if (nav.ecran !== 'accueil') return; if (e.detail.i !== nav.sel) son.tic(); nav.sel = e.detail.i; nomSel(nav.sel); });
  document.addEventListener('kit:ouvrir', (e) => { if (nav.ecran === 'accueil') ouvrirTuile(e.detail.i); });
  function ouvrirTuile(i) {
    const t = E.accueil?.tuiles?.[i]; if (!t) return;
    nav.sel = i; son.ok();
    if (E.pages?.[t.id]) aller(t.id); else action(t.id);
  }

  // ---------------------------------------------------------------- les blocs du contenu
  const fleche = () => `<span class="chev">${ico('fleche', 2)}</span>`;
  const icone = (n, c) => (n ? `<span class="ic" style="--a:${couleur(c)}">${ico(n, 1.7)}</span>` : '');
  const etiq = (t, ton) => (t ? `<span class="etiquette${ton ? ` ${esc(ton)}` : ''}">${esc(t)}</span>` : '');
  const btn = (b, cls = '') => `<button type="button" class="btn f${b.style ? ` ${esc(b.style)}` : ''}${cls}" data-k="o:${esc(b.id)}" data-action="${esc(b.id)}"${b.valeur != null ? ` data-valeur="${esc(b.valeur)}"` : ''}>${b.icone ? ico(b.icone) : ''}${esc(b.texte)}</button>`;
  const bascule = (id, oui) => `<button type="button" class="bascule f${oui ? ' oui' : ''}" data-k="b:${esc(id)}" data-bascule="${esc(id)}" role="switch" aria-checked="${!!oui}"></button>`;
  const caseC = (id, oui, valeur) => `<button type="button" class="case f${oui ? ' oui' : ''}" data-k="x:${esc(id)}:${esc(valeur ?? '')}" data-case="${esc(id)}"${valeur != null ? ` data-valeur="${esc(valeur)}"` : ''} role="checkbox" aria-checked="${!!oui}">${ico('coche', 3)}</button>`;
  const barre = (v, ton) => `<span class="flux${ton ? ` ${esc(ton)}` : ''}"><i style="width:${borne(+v || 0, 0, 100)}%"></i></span>`;
  const anneau = (v, extra = '', ton, ep) => `<span class="anneau" data-anneau="${borne(+v || 0, 0, 100) / 100}"${ep ? ` data-ep="${ep}"` : ''}${ton === 'attention' || ton === 'urgent' ? ' data-alerte="1"' : ''} style="--p:${borne(+v || 0, 0, 100)}">${extra}</span>`;

  function bloc(b) {
    const note = b.note ? `<span class="n">${esc(b.note)}</span>` : '';
    switch (b.type) {
      case 'titre': return `<h2 class="titre-bloc">${esc(b.texte)}${b.direct ? '<span class="direct">En direct</span>' : ''}${b.droite ? `<span class="droite">${esc(b.droite)}</span>` : ''}</h2>`;
      case 'texte': return `<p class="texte">${esc(b.texte)}</p>`;
      case 'note': return `<p class="note">${esc(b.texte)}</p>`;
      case 'score': {
        const p = borne((b.valeur / (b.sur || 100)) * 100, 0, 100);
        return `<div class="score">${anneau(p, `<span class="centre"><b>${esc(b.valeur)}</b><small>/${esc(b.sur || 100)}</small></span>`)}<div><h3>${esc(b.titre)}</h3><p>${esc(b.texte)}</p></div></div>`;
      }
      case 'ligne': {
        const interne = b.bascule || b.bascules?.length || b.bouton || b.case || b.boutons?.length;
        const img = urlSure(b.image) ? `<span class="ic img"><img src="${urlSure(b.image)}" alt="" onerror="this.remove()"></span>` : icone(b.icone, b.couleur);
        const debut = `${b.case ? caseC(b.case.id, b.case.oui, b.case.valeur) : ''}${b.quand ? `<span class="quand"><b>${esc(b.quand[0])}</b>${esc(b.quand[1] || '')}</span>` : ''}${img}`;
        const milieu = `<span class="txt"><b>${esc(b.texte)}</b>${note}</span>`;
        const fin = `${b.barre != null ? barre(b.barre, b.ton) : ''}${etiq(b.etiquette, b.ton)}${b.valeur != null && b.valeur !== '' ? `<span class="v${b.ton ? ` ${esc(b.ton)}` : ''}">${esc(b.valeur)}</span>` : ''}${b.bascule ? bascule(b.bascule.id, b.bascule.oui) : ''}${(b.bascules || []).map((x) => `<span class="bascule-l">${esc(x.texte)}${bascule(x.id, x.oui)}</span>`).join('')}${b.bouton ? btn(b.bouton, ' petit') : ''}`;
        const cls = `ligne plaque${b.fait ? ' fait' : ''}${b.boutons?.length ? ' avec-btns' : ''}`;
        if (b.id && !interne) return `<button type="button" class="${cls} f" data-k="l:${esc(b.id)}" data-action="${esc(b.id)}">${debut}${milieu}<span class="fin">${fin}${fleche()}</span></button>`;
        // (boutons : une rangée de petits boutons sous le texte, dans la même carte)
        const btns = b.boutons?.length ? `<span class="btns">${b.boutons.map((x) => btn(x, ' petit')).join('')}</span>` : '';
        return `<div class="${cls}">${debut}${milieu}<span class="fin">${fin}</span>${btns}</div>`;
      }
      case 'bascule': return `<button type="button" class="ligne plaque f" data-k="b:${esc(b.id)}" data-bascule="${esc(b.id)}" role="switch" aria-checked="${!!b.oui}"><span class="txt"><b>${esc(b.texte)}</b>${note}</span><span class="fin"><span class="bascule${b.oui ? ' oui' : ''}"></span></span></button>`;
      case 'case': return `<button type="button" class="ligne plaque f" data-k="x:${esc(b.id)}:${esc(b.valeur ?? '')}" data-case="${esc(b.id)}"${b.valeur != null ? ` data-valeur="${esc(b.valeur)}"` : ''} role="checkbox" aria-checked="${!!b.oui}"><span class="case${b.oui ? ' oui' : ''}">${ico('coche', 3)}</span><span class="txt"><b>${esc(b.texte)}</b>${note}</span>${b.droite ? `<span class="fin"><span class="v">${esc(b.droite)}</span></span>` : ''}</button>`;
      case 'bouton': return btn(b);
      case 'champ': return b.recherche
        ? `<label class="champ-grand plaque">${ico('loupe')}<input class="champ f" data-k="c:${esc(b.id)}" data-champ="${esc(b.id)}" data-direct="1" value="${esc(b.valeur)}" placeholder="${esc(b.indice || b.texte)}"></label>`
        : `<label class="ligne plaque"><span class="txt"><b>${esc(b.texte)}</b>${note}</span><input class="champ f" data-k="c:${esc(b.id)}" data-champ="${esc(b.id)}" value="${esc(b.valeur)}" placeholder="${esc(b.indice)}"></label>`;
      case 'choix': return `<div class="choix-p" role="radiogroup">${(b.options || []).map((o) => `<button type="button" class="f${o.id === b.valeur ? ' oui' : ''}" data-k="p:${esc(b.id)}:${esc(o.id)}" data-action="${esc(b.id)}" data-valeur="${esc(o.id)}" data-choix="1" role="radio" aria-checked="${o.id === b.valeur}">${esc(o.texte)}</button>`).join('')}</div>`;
      case 'cartes': return `<div class="cartes-choix">${(b.options || []).map((o) => `<button type="button" class="carte-choix plaque f${o.id === b.valeur ? ' oui' : ''}" data-k="p:${esc(b.id)}:${esc(o.id)}" data-action="${esc(b.id)}" data-valeur="${esc(o.id)}" data-choix="1"><b>${esc(o.titre)}</b><span>${esc(o.texte)}</span></button>`).join('')}</div>`;
      case 'outils': return `<div class="outils">${(b.elements || []).map((x) => `<button type="button" class="outil plaque f" data-k="l:${esc(x.id)}" data-action="${esc(x.id)}"><span class="haut-o">${icone(x.icone || 'etoile', x.couleur)}${etiq(x.etiquette, x.ton)}</span><b>${esc(x.nom)}</b><span class="n">${esc(x.note)}</span></button>`).join('')}</div>`;
      case 'raccourcis': return `<div class="raccourcis">${(b.elements || []).map((x) => `<button type="button" class="raccourci plaque f" data-k="l:${esc(x.id)}" data-action="${esc(x.id)}">${icone(x.icone || 'etoile', x.couleur)}<b>${esc(x.nom)}</b>${fleche()}</button>`).join('')}</div>`;
      case 'mesures': return `<div class="mesures">${(b.elements || []).map((x) => `<div class="mesure plaque"${x.id ? ` data-id="${esc(x.id)}"` : ''}>${anneau(x.valeur, `<span class="centre"><b class="${x.ton ? esc(x.ton) : ''}">${esc(x.texte ?? `${Math.round(+x.valeur || 0)} %`)}</b></span>`, x.ton)}<b>${esc(x.nom)}</b><span class="n">${esc(x.note)}</span></div>`).join('')}</div>`;
      case 'pastilles': return `<div class="pastilles">${(b.elements || []).map((x) => `<span class="pastille ${esc(x.ton || 'ok')}">${ico(x.ton === 'attention' || x.ton === 'urgent' ? 'alerte' : 'ok', 2.2)}${esc(x.texte)}</span>`).join('')}</div>`;
      case 'reglages': return `<div class="reglages plaque">${(b.rangs || []).map((r) => `<div class="rang"><span class="txt"><b>${esc(r.texte)}</b>${r.note ? `<span class="n">${esc(r.note)}</span>` : ''}</span>${r.bascule ? bascule(r.bascule.id, r.bascule.oui) : ''}${r.bouton ? btn(r.bouton, ' petit') : ''}${r.lien ? `<button type="button" class="raccourci plaque f" data-k="l:${esc(r.lien.id)}" data-action="${esc(r.lien.id)}"><b>${esc(r.lien.texte)}</b>${fleche()}</button>` : ''}${r.touches ? `<span class="touches">${r.touches.map((k) => `<kbd>${esc(k)}</kbd>`).join('+')}</span>` : ''}${r.valeur ? `<span class="v">${esc(r.valeur)}</span>` : ''}</div>`).join('')}</div>`;
      case 'faq': return `<div class="faq">${(b.elements || []).map((x, i) => `<div class="question-r plaque"><button type="button" class="q f" data-k="q:${i}" data-ouvrir="1">${icone(x.icone || 'aide', x.couleur)}<b>${esc(x.question)}</b>${fleche()}</button><p>${esc(x.reponse)}</p></div>`).join('')}</div>`;
      case 'jauge': {
        const p = borne(+b.valeur || 0, 0, 100);
        return `<div class="jauge"><div class="jauge-haut"><span>${esc(b.texte)}${note}</span><span class="v${b.ton ? ` ${esc(b.ton)}` : ''}">${Math.round(p)} %</span></div>${barre(p, b.ton)}</div>`;
      }
      case 'liste': return `<ul class="puces${b.ton ? ` ${esc(b.ton)}` : ''}">${(b.elements || []).map((t) => `<li>${esc(t)}</li>`).join('')}</ul>`;
      case 'image': {
        const u = urlSure(b.url); if (!u) return '';
        return `<figure class="image"><img src="${u}" alt="" onerror="this.parentNode.remove()" style="max-height:${borne(+b.hauteur || 260, 60, 600)}px">${b.legende ? `<figcaption>${esc(b.legende)}</figcaption>` : ''}</figure>`;
      }
      case 'courbe': {
        const pts = (b.points || []).map(Number).filter((v) => isFinite(v));
        if (pts.length < 2) return '';
        const min = Math.min(...pts), max = Math.max(...pts), et = max - min || 1, L = 600, H = 130;
        const xy = pts.map((v, i) => `${((i / (pts.length - 1)) * L).toFixed(1)},${(H - 10 - ((v - min) / et) * (H - 24)).toFixed(1)}`).join(' ');
        const e = b.etiquettes || [], u = b.unite ? ` ${esc(b.unite)}` : '', g = `g${Math.random().toString(36).slice(2, 8)}`;
        return `<div class="courbe plaque${b.ton ? ` ${esc(b.ton)}` : ''}"><div class="jauge-haut"><span>${esc(b.titre)}</span><span class="v">${esc(pts[pts.length - 1])}${u}</span></div>
          <svg viewBox="0 0 ${L} ${H}" preserveAspectRatio="none" aria-hidden="true"><defs><linearGradient id="${g}c" x1="0" x2="1"><stop offset="0" stop-color="var(--P)"/><stop offset=".5" stop-color="var(--S)"/><stop offset="1" stop-color="var(--T)"/></linearGradient>
          <linearGradient id="${g}f" x1="0" x2="0" y1="0" y2="1"><stop offset="0" stop-color="var(--S)" stop-opacity=".35"/><stop offset="1" stop-color="var(--S)" stop-opacity="0"/></linearGradient></defs>
          <polygon points="0,${H} ${xy} ${L},${H}" fill="url(#${g}f)"/><polyline points="${xy}" stroke="url(#${g}c)"/></svg>
          <div class="courbe-bas"><span>${esc(e[0])}</span><span>min ${esc(min)}${u} · max ${esc(max)}${u}</span><span>${esc(e[1])}</span></div></div>`;
      }
      case 'grille': return `<div class="grille">${(b.elements || []).map((x) => {
        const u = urlSure(x.image), lettre = esc(String(x.titre || '?').trim().charAt(0).toUpperCase());
        return `<button type="button" class="carte-g plaque f" data-k="g:${esc(x.id)}" data-action="${esc(x.id)}" style="--a:${couleur(x.couleur)}">
          <span class="jaquette">${u ? `<img src="${u}" alt="" loading="lazy" onerror="this.remove()">` : ''}<i>${lettre}</i>${x.marque ? `<em>${esc(x.marque)}</em>` : ''}</span>
          <b>${esc(x.titre)}</b><span>${esc(x.sous)}</span></button>`;
      }).join('')}</div>`;
      case 'chiffres': return `<div class="chiffres">${(b.elements || []).map((x) => `<div class="chiffre plaque"><span>${esc(x.titre)}</span><b class="${x.ton ? esc(x.ton) : ''}">${esc(x.valeur)}${x.unite ? `<small>${esc(x.unite)}</small>` : ''}</b></div>`).join('')}</div>`;
      case 'banniere': {
        const u = urlSure(b.image);
        return `<div class="banniere"${u ? ` style="--img:url('${u}')"` : ''}><div><h3>${esc(b.titre)}</h3><p>${esc(b.sous)}</p></div></div>`;
      }
      case 'reseau': {
        const ap = (b.appareils || []).map((x) => ({ nom: x.nom, icone: I[x.icone] ? x.icone : 'inconnu', couleur: x.couleur, ping: +x.ping || 0, attention: !!x.attention, nouveau: !!x.nouveau }));
        if (plasma()) return `<div class="reseau3d plaque" data-reseau="${esc(JSON.stringify({ centre: b.centre || 'Box', appareils: ap }))}"><div class="etq"></div><p class="legende">${esc(b.legende || 'Plus un appareil tourne près de la box, plus il répond vite. Orange : à vérifier.')}</p></div>`;
        const n = ap.length, L = 640, H = 300, cx = L / 2, cy = H / 2;
        let liens = '', noeuds = '';
        ap.forEach((x, i) => {
          const ang = (i / Math.max(1, n)) * Math.PI * 2 - Math.PI / 2, r = i % 2 ? 125 : 95;
          const x1 = cx + Math.cos(ang) * r * 1.9, y1 = cy + Math.sin(ang) * r;
          liens += `<line x1="${cx}" y1="${cy}" x2="${x1.toFixed(1)}" y2="${y1.toFixed(1)}"/>`;
          noeuds += `<g class="noeud${x.attention || x.nouveau ? ' attention' : ''}" style="--a:${couleur(x.couleur)}"><circle cx="${x1.toFixed(1)}" cy="${y1.toFixed(1)}" r="9"/><text x="${x1.toFixed(1)}" y="${(y1 + 24).toFixed(1)}">${esc(x.nom)}</text></g>`;
        });
        return `<div class="reseau plaque"><svg viewBox="0 0 ${L} ${H}" aria-hidden="true">${liens}<g class="centre"><circle cx="${cx}" cy="${cy}" r="18"/><text x="${cx}" y="${cy + 38}">${esc(b.centre || 'Box')}</text></g>${noeuds}</svg></div>`;
      }
      case 'apercu': {
        const coins = ['haut-gauche', 'haut-droite', 'bas-gauche', 'bas-droite'];
        return `<div class="apercu"><div class="scene-jeu">${coins.map((c) => `<button type="button" class="coin f ${c}${c === b.coin ? ' choisi' : ''}" data-k="c:${c}" data-action="${esc(b.id)}" data-valeur="${c}" aria-label="Mettre le compteur ${c.replace('-', ' à ')}">${c === b.coin ? `<span class="compteur ${esc(b.style || 'complet')}"><b>${esc(b.fps ?? 144)}<small>FPS</small></b>${b.style === 'discret' || !b.detail ? '' : `<i>${esc(b.detail)}</i>`}</span>` : '<span class="placer">Placer ici</span>'}</button>`).join('')}</div>${b.legende ? `<p class="legende">${esc(b.legende)}</p>` : ''}</div>`;
      }
      case 'etapes': return `<ol class="etapes">${(b.elements || []).map((x) => `<li class="${x.fait ? 'fait' : ''}">${esc(x.texte)}</li>`).join('')}</ol>`;
      case 'mois': {
        const a = b.annee | 0, m = b.mois | 0; if (!a || m < 1 || m > 12) return '';
        const premier = (new Date(a, m - 1, 1).getDay() + 6) % 7, n = new Date(a, m, 0).getDate(), j0 = new Date();
        const auj = `${j0.getFullYear()}-${String(j0.getMonth() + 1).padStart(2, '0')}-${String(j0.getDate()).padStart(2, '0')}`;
        let h = ['L', 'M', 'M', 'J', 'V', 'S', 'D'].map((x) => `<span class="jsem">${x}</span>`).join('') + '<span></span>'.repeat(premier);
        for (let d = 1; d <= n; d++) {
          const cle = `${a}-${String(m).padStart(2, '0')}-${String(d).padStart(2, '0')}`, info = b.jours?.[cle] || {};
          const points = (info.points || []).slice(0, 4).map((c) => `<i style="background:${couleur(c)}"></i>`).join('');
          h += `<button type="button" class="jour f${cle === b.choisi ? ' choisi' : ''}${cle === auj ? ' auj' : ''}" data-k="j:${cle}" data-action="${esc(b.id)}" data-valeur="${cle}" title="${esc(info.texte)}"><b>${d}</b><span class="pts">${points}</span></button>`;
        }
        return `<div class="mois">${h}</div>`;
      }
      case 'rdv': return `<button type="button" class="rdv plaque f" data-k="v:${esc(b.id)}" data-action="${esc(b.id)}"><span class="h">${esc(b.heure)}</span><b>${esc(b.titre)}</b><span>${esc(b.lieu)}</span></button>`;
      default: return '';
    }
  }
  function blocs(liste) {   // les boutons qui se suivent sont rangés sur une même ligne
    let h = '', bt = '';
    for (const b of liste || []) {
      if (!b) continue;
      if (b.type === 'bouton') { bt += bloc(b); continue; }
      if (bt) { h += `<div class="boutons">${bt}</div>`; bt = ''; }
      h += bloc(b);
    }
    return h + (bt ? `<div class="boutons">${bt}</div>` : '');
  }

  // ---------------------------------------------------------------- une page : en-tête, pastilles, module déplié
  function ongletsDe(p) {
    if (p.onglets?.length) return p.onglets;
    if (p.sections?.length > 1) return p.sections.map((s, i) => ({ id: `s${i}`, nom: s.nom, blocs: s.blocs }));
    return null;
  }
  function moduleDe(p, contenu) {
    if (p.module) return p.module;
    const b0 = contenu[0];   // (ancien format : un score en premier devient le module)
    if (b0?.type === 'score') { contenu.shift(); return { anneau: { valeur: b0.valeur, sur: b0.sur }, titre: b0.titre, texte: b0.texte }; }
    return null;
  }
  function module(m) {
    let visuel = '';
    if (m.anneau) {
      const a = m.anneau, p = borne(((+a.valeur || 0) / (+a.sur || 100)) * 100, 0, 100);
      const centre = `<span class="centre"><b class="grand${a.ton ? ` ${esc(a.ton)}` : ''}">${esc(a.texte ?? a.valeur)}</b>${a.petit ?? (a.unite ? '' : `/${a.sur || 100}`) ? `<small>${esc(a.petit ?? `/${a.sur || 100}`)}</small>` : ''}${a.unite ? `<small>${esc(a.unite)}</small>` : ''}</span>`;
      visuel = `<span class="anneau grand-anneau" data-anneau="${p / 100}" data-ep="0.11" style="--p:${p}">${centre}</span>`;
    } else if (urlSure(m.image)) visuel = `<span class="jaquette-m" style="--img:url('${urlSure(m.image)}')"><b>${esc(m.legende || '')}</b></span>`;
    else visuel = `<span class="grande-ic" style="--a:${couleur(m.couleur)}">${ico(m.icone || 'etoile', 1.2)}</span>`;
    const boutons = (m.boutons || []).map((b, i) => btn({ ...b, style: b.style || (i ? 'second' : '') })).join('');
    return `<aside class="module plasma-bord fort"><span class="reflet"></span>${visuel}
      ${m.titre ? `<h3>${esc(m.titre)}</h3>` : ''}${m.texte ? `<p>${esc(m.texte)}</p>` : ''}
      ${boutons ? `<div class="boutons-v">${boutons}</div>` : ''}
      ${m.case ? `<button type="button" class="case-m f" data-k="x:${esc(m.case.id)}:" data-case="${esc(m.case.id)}" role="checkbox" aria-checked="${!!m.case.oui}"><span class="case${m.case.oui ? ' oui' : ''}">${ico('coche', 3)}</span>${esc(m.case.texte)}</button>` : ''}
      ${m.resume?.length ? `<div class="resume">${m.resume.map((r) => `<div><span>${esc(r.nom)}</span><b class="${r.ton ? esc(r.ton) : ''}">${esc(r.valeur)}</b></div>`).join('')}</div>` : ''}</aside>`;
  }
  function pastilles(liste, choisi, niveau) {
    return `<div class="pastilles-o${niveau ? ' petites' : ''}" role="tablist">${liste.map((o, i) => `<button type="button" class="f${i === choisi ? ' oui' : ''}" data-k="o${niveau}:${i}" data-onglet="${niveau}:${i}" role="tab" aria-selected="${i === choisi}">${esc(o.nom)}</button>`).join('')}</div>`;
  }
  function page(premiere, focus) {
    const p = E.pages?.[nav.ecran];
    if (!p) { accueil(`t:${nav.ecran}`); return; }
    enTete();
    if (p.question || p.resultat) { question(p, focus); return; }
    const garde = !premiere && ecran.querySelector('.page')?.dataset.id === nav.ecran ? ecran.querySelector('.defile')?.scrollTop : 0;
    const parent = p.parent && E.pages?.[p.parent];
    // (la même page redessinée : ses barres glissent de leur largeur d'avant vers la nouvelle, au lieu de sauter)
    const largeurs = !premiere && ecran.querySelector('.page')?.dataset.id === nav.ecran ? [...ecran.querySelectorAll('.flux i')].map((x) => x.style.width) : null;
    ecran.innerHTML = `<div class="page${premiere ? ' entree' : ''}" data-id="${esc(nav.ecran)}"><div class="defile">
      <div class="tete"><button type="button" class="retour f" data-k="retour" data-retour="1" title="Retour" aria-label="Retour">${ico('retour', 2.2)}</button>
        ${p.icone ? `<span class="ic-page plasma-bord fort" style="--a:${couleur(p.couleur)}">${ico(p.icone, 1.6)}</span>` : ''}
        <div class="titres">${parent ? `<div class="chemin">${esc(parent.titre)} › <b>${esc(p.titre)}</b></div>` : ''}<h1>${esc(p.titre)}</h1>${p.sous ? `<p>${esc(p.sous)}</p>` : ''}</div>
        <div class="actions">${(p.actions || []).map((b) => btn({ ...b, style: b.style || 'second' })).join('')}</div></div>
      <div id="corps"></div></div></div>`;
    corps();
    if (largeurs) ecran.querySelectorAll('.flux i').forEach((x, k) => {
      const fin = x.style.width; if (largeurs[k] == null || largeurs[k] === fin) return;
      x.style.transition = 'none'; x.style.width = largeurs[k]; void x.offsetWidth; x.style.transition = ''; x.style.width = fin;
    });
    if (garde) { const c = ecran.querySelector('.defile'); if (c) c.scrollTop = garde; }
    const cible = (focus && ecran.querySelector(`[data-k="${CSS.escape(focus)}"]`)) || (premiere ? ecran.querySelector('.pastilles-o .oui, .module .btn, #corps .f') : null);
    cible?.focus({ preventScroll: true });
  }
  function corps() {
    const el = document.getElementById('corps'); if (!el) return;
    const p = E.pages[nav.ecran], o1 = ongletsDe(p);
    const ch = nav.onglets[nav.ecran] || (nav.onglets[nav.ecran] = [0, 0]);
    let tete = '', tete2 = '', contenu, propre = null;
    if (o1) {
      ch[0] = borne(ch[0], 0, o1.length - 1);
      const o = o1[ch[0]]; tete = pastilles(o1, ch[0], 0);
      if (o.onglets?.length) {   // (deuxième niveau : des pastilles plus petites, en haut du contenu)
        ch[1] = borne(ch[1], 0, o.onglets.length - 1);
        tete2 = `${o.texte ? `<p class="texte">${esc(o.texte)}</p>` : ''}${pastilles(o.onglets, ch[1], 1)}`;
        contenu = [...(o.onglets[ch[1]].blocs || [])]; propre = o.onglets[ch[1]].module || o.module;
      } else { contenu = [...(o.blocs || [])]; propre = o.module; }
    } else contenu = [...(p.blocs || p.sections?.[0]?.blocs || [])];
    const m = propre || moduleDe(p, contenu);
    el.innerHTML = `${tete}<div class="deplie${m ? '' : ' sans-module'}">${m ? module(m) : ''}<section class="contenu">${tete2}${blocs(contenu)}</section></div>`;
  }
  function question(p, focus) {
    if (p.question) {
      const q = p.question;
      ecran.innerHTML = `<div class="question entree">${q.etape ? `<div class="etape">${esc(q.etape)}</div>` : ''}<h2>${esc(q.texte)}</h2><div class="choix">${(q.choix || []).map((c, i) => `<button type="button" class="rep plaque f" data-k="q:${esc(c.id)}" data-action="${esc(c.id)}"><i>${'ABCDEFGH'[i] || ''}</i>${esc(c.texte)}</button>`).join('')}</div></div>`;
    } else {
      const r = p.resultat;
      ecran.innerHTML = `<div class="question entree">${r.etape ? `<div class="etape">${esc(r.etape)}</div>` : ''}<h2>${esc(r.titre)}</h2>
        <div class="resultat plasma-bord fort">${r.badge ? `<span class="badge">${esc(r.badge)}</span>` : ''}${r.nom ? `<h3>${esc(r.nom)}</h3>` : ''}<p>${esc(r.texte)}</p>${blocs((r.boutons || []).map((b) => ({ type: 'bouton', ...b })))}</div></div>`;
    }
    ((focus && ecran.querySelector(`[data-k="${CSS.escape(focus)}"]`)) || ecran.querySelector('#ecran .f'))?.focus();
  }

  // L'appli sait sur quel écran on est (pour y revenir après une fiche, par exemple)
  let ecranDit = null;
  function direEcran() { if (nav.ecran !== ecranDit) { ecranDit = nav.ecran; envoyer({ type: 'ecran', id: nav.ecran }); } }
  function dessiner(anime) {
    const k = document.activeElement?.closest?.('#ecran [data-k]')?.dataset.k;
    if (nav.ecran === 'accueil') accueil(k, anime); else page(anime, k);
  }
  function aller(id, focus) {
    if (id === 'accueil' || !E.pages?.[id]) {
      const avant = racine(nav.ecran), k = (E.accueil?.tuiles || []).findIndex((x) => x.id === avant);
      if (k >= 0) nav.sel = k;
      if (nav.ecran !== 'accueil') ecran.innerHTML = '';
      accueil(null, true); return;
    }
    const avant = nav.ecran; nav.ecran = id;
    if (E.pages[id].parent !== avant && avant !== id) nav.onglets[id] = nav.onglets[id] || [0, 0];
    page(true, focus); direEcran();
  }
  function retour() {
    son.retour();
    if (nav.ecran === 'accueil') { envoyer({ type: 'menu' }); return; }
    const p = E.pages?.[nav.ecran];
    if (p?.retour === 'appli') { envoyer({ type: 'action', id: 'retour', ecran: nav.ecran }); return; }
    if (p?.parent && E.pages?.[p.parent]) { const de = nav.ecran; aller(p.parent, `l:${de}`); return; }
    aller('accueil');
  }
  const action = (id, valeur) => envoyer({ type: 'action', id, valeur, ecran: nav.ecran });

  // ---------------------------------------------------------------- valider (clic, A ou Entrée)
  function valider(el) {
    if (!el) return;
    const d = el.dataset;
    if (d.retour) { retour(); return; }
    if (d.tuile !== undefined) { ouvrirTuile(+d.tuile); return; }
    if (d.rond !== undefined) { const r = E.accueil.ronds[+d.rond]; son.ok(); if (E.pages?.[r.id]) aller(r.id); else action(r.id); return; }
    if (d.onglet) {
      const [n, i] = d.onglet.split(':').map(Number), ch = nav.onglets[nav.ecran] || (nav.onglets[nav.ecran] = [0, 0]);
      if (ch[n] === i) return;
      son.tic(); ch[n] = i; if (n === 0) ch[1] = 0;
      corps(); document.querySelector(`[data-onglet="${n}:${i}"]`)?.focus({ preventScroll: true }); return;
    }
    if (d.ouvrir) { el.parentElement.classList.toggle('ouvert'); son.tic(); return; }
    if (d.bascule) {   // on bascule tout de suite à l'écran, l'appli confirme avec son prochain état
      const b = el.classList.contains('bascule') ? el : el.querySelector('.bascule'), oui = !b.classList.contains('oui');
      b.classList.toggle('oui', oui); el.setAttribute('aria-checked', String(oui)); son.bascule(); action(d.bascule, oui); return;
    }
    if (d.case) {
      const c = el.classList.contains('case') ? el : el.querySelector('.case'), oui = !c.classList.contains('oui');
      c.classList.toggle('oui', oui); el.setAttribute('aria-checked', String(oui)); son.bascule();
      envoyer({ type: 'action', id: d.case, valeur: d.valeur != null ? { valeur: d.valeur, oui } : oui, ecran: nav.ecran }); return;
    }
    if (d.champ) { son.ok(); action(d.champ, el.value); return; }
    if (d.choix) { el.parentElement.querySelectorAll('[data-choix]').forEach((x) => { x.classList.toggle('oui', x === el); x.setAttribute('aria-checked', String(x === el)); }); }
    if (d.action) {
      son.ok();
      if (E.pages?.[d.action] && d.valeur == null) { aller(d.action); return; }   // une ligne ou un outil qui mène à une page
      action(d.action, d.valeur);
    }
  }

  // ---------------------------------------------------------------- la sélection : souris, flèches, A (Entrée) et B (Échap)
  let dernier = null;
  const zoneF = () => [...document.querySelectorAll('#haut .f, #ecran .f')].filter((e) => e.offsetParent !== null);
  document.addEventListener('focusin', (e) => {
    const el = e.target.closest?.('.f'); if (!el) return;
    if (dernier && dernier !== el && dernier.isConnected) son.tic(); dernier = el;
    if (el.dataset.tuile !== undefined && el.classList.contains('tuile')) { nav.sel = +el.dataset.tuile; nomSel(nav.sel); el.scrollIntoView({ block: 'nearest', inline: 'center', behavior: 'smooth' }); }
  });
  ecran.addEventListener('focusout', (e) => {
    const el = e.target;
    if (el.dataset?.champ && !el.dataset.direct && el.value !== el.defaultValue) { el.defaultValue = el.value; action(el.dataset.champ, el.value); }
    if (enAttente && el.tagName === 'INPUT') setTimeout(() => { if (document.activeElement?.tagName !== 'INPUT') { enAttente = false; dessiner(false); } });
  });
  // Une recherche part pendant qu'on tape (un court instant après la dernière touche)
  let tape = 0;
  ecran.addEventListener('input', (e) => { const el = e.target; if (!el.dataset?.direct) return; clearTimeout(tape); tape = setTimeout(() => { el.defaultValue = el.value; action(el.dataset.champ, el.value); }, 280); });
  document.addEventListener('pointerover', (e) => { const el = e.target.closest?.('#ecran .f, #haut .f'); if (el && el !== document.activeElement && el.tagName !== 'INPUT' && document.activeElement?.tagName !== 'INPUT') el.focus({ preventScroll: true }); });
  document.addEventListener('click', (e) => { const el = e.target.closest?.('#ecran .f, #haut .f'); if (el && el.tagName !== 'INPUT') valider(el); });
  document.getElementById('aideA').addEventListener('click', () => { if (nav.ecran === 'accueil' && plasma()) ouvrirTuile(nav.sel); else valider(document.activeElement?.closest?.('.f')); });
  document.getElementById('aideB').addEventListener('click', retour);
  // Le logo et le nom ramènent à l'accueil, de n'importe quelle page
  document.getElementById('marque').addEventListener('click', () => { if (nav.ecran !== 'accueil') { son.retour(); aller('accueil'); } });
  function voisin(dir) {
    const cur = document.activeElement?.closest?.('.f'), els = zoneF();
    if (!cur || !els.includes(cur)) { (ecran.querySelector('.f') || els[0])?.focus(); return; }
    const a = cur.getBoundingClientRect(), ax = a.left + a.width / 2, ay = a.top + a.height / 2;
    let best = null, bd = Infinity;
    for (const e of els) {
      if (e === cur) continue;
      const b = e.getBoundingClientRect(), dx = b.left + b.width / 2 - ax, dy = b.top + b.height / 2 - ay;
      if ((dir === 'd' && dx <= 6) || (dir === 'g' && dx >= -6) || (dir === 'b' && dy <= 6) || (dir === 'h' && dy >= -6)) continue;
      const horiz = dir === 'd' || dir === 'g', d = (horiz ? Math.abs(dx) : Math.abs(dy)) + (horiz ? Math.abs(dy) : Math.abs(dx)) * 2.5;
      if (d < bd) { bd = d; best = e; }
    }
    if (best) { best.focus({ preventScroll: true }); best.scrollIntoView({ block: 'nearest', inline: 'nearest' }); }
  }
  addEventListener('keydown', (e) => {
    const dans = e.target.tagName === 'INPUT', k = { ArrowRight: 'd', ArrowLeft: 'g', ArrowDown: 'b', ArrowUp: 'h' }[e.key];
    const enHaut = !!document.activeElement?.closest?.('#haut');
    // L'accueil en 3D : gauche et droite font défiler la rangée, A ouvre la tuile du milieu, haut va aux onglets
    if (nav.ecran === 'accueil' && plasma() && !enHaut && !dans) {
      if (k === 'd' || k === 'g') { e.preventDefault(); plasma().deplacer(k === 'd' ? 1 : -1); return; }
      if (k === 'h') { e.preventDefault(); document.querySelector('#onglets .f')?.focus(); return; }
      if (e.key === 'Enter') { e.preventDefault(); ouvrirTuile(nav.sel); return; }
    }
    if (nav.ecran === 'accueil' && plasma() && enHaut && k === 'b') { e.preventDefault(); document.activeElement.blur(); return; }
    if (k && !(dans && (k === 'd' || k === 'g'))) { e.preventDefault(); voisin(k); return; }
    if (e.key === 'Enter') { e.preventDefault(); valider(document.activeElement?.closest?.('.f')); return; }
    if (e.key === 'Escape' || (e.key === 'Backspace' && !dans)) { e.preventDefault(); retour(); }
  });

  // ---------------------------------------------------------------- horloge et logo qui « glitche » (arrêtés en pause)
  let minuteries = [];
  function demarrer() {
    if (minuteries.length) return;
    document.querySelectorAll('.hh').forEach((h) => { h.textContent = heure(); });
    minuteries = [
      setInterval(() => document.querySelectorAll('.hh').forEach((h) => { h.textContent = heure(); }), 15000),
      setInterval(() => { const l = document.getElementById('logo'); if (!l) return; l.classList.remove('glitch'); void l.offsetWidth; l.classList.add('glitch'); }, 2600),
    ];
  }
  function arreter() { minuteries.forEach(clearInterval); minuteries = []; }
  document.addEventListener('visibilitychange', () => (document.hidden ? arreter() : demarrer()));

  let bulleMinuterie = 0;
  // ---------------------------------------------------------------- ce que l'appli peut appeler
  const kit = window.kit = {
    surMessage: null,
    etat(nouveau) {
      normaliser(nouveau);
      const avantTheme = E.theme;
      E = { ...E, ...nouveau };
      son.actif = E.sons !== false;
      if (E.theme !== avantTheme || 'couleurs' in nouveau) {
        document.body.dataset.theme = E.theme || 'neon';
        for (const v of ['P', 'S', 'T', 'fond', 'fond2', 'carte', 'carte2', 'ligne', 'texte', 'doux', 'pale']) {
          const c = E.couleurs?.[v];
          if (c && /^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(c)) document.body.style.setProperty(`--${v}`, c); else document.body.style.removeProperty(`--${v}`);
        }
        window.kitPlasma?.couleurs?.();
      }
      pack();
      const choisir = () => {   // (focus : l'élément à choisir une fois l'écran dessiné)
        if (!nouveau.focus) return;
        const k = String(nouveau.focus); delete E.focus;
        if (nav.ecran === 'accueil' && k.startsWith('t:')) { const i = (E.accueil?.tuiles || []).findIndex((x) => `t:${x.id}` === k); if (i >= 0) { nav.sel = i; plasma()?.aller(i); nomSel(i); } }
        const el = document.querySelector(`[data-k="${CSS.escape(k)}"]`);
        if (el) { el.focus({ preventScroll: true }); el.scrollIntoView({ block: 'nearest', inline: 'nearest' }); }
      };
      if (nouveau.ecran) { const id = nouveau.ecran; delete E.ecran; aller(id); choisir(); return; }
      if (nav.ecran !== 'accueil' && !E.pages?.[nav.ecran]) { aller('accueil'); choisir(); return; }
      if (document.activeElement?.tagName === 'INPUT') { enAttente = true; enTete(); return; }
      dessiner(false);
      choisir();
    },
    aller,
    redessiner() { if (nav.ecran === 'accueil') { ecran.innerHTML = ''; accueil(null, false); } else dessiner(false); },
    message(texte, bravo) {
      const b = document.getElementById('bulle'); b.textContent = texte; b.classList.add('vu');
      if (bravo) son.bravo();
      clearTimeout(bulleMinuterie); bulleMinuterie = setTimeout(() => b.classList.remove('vu'), 2600);
    },
    // Les mesures en direct : l'anneau, le chiffre et la note changent sur place
    mesures(valeurs) {
      for (const [id, v] of Object.entries(valeurs || {})) {
        const el = ecran.querySelector(`.mesure[data-id="${CSS.escape(id)}"]`); if (!el || !v) continue;
        const a = el.querySelector('.anneau'), p = borne(+v.valeur || 0, 0, 100);
        a.dataset.anneau = p / 100; a.style.setProperty('--p', p);
        if (v.ton === 'attention' || v.ton === 'urgent') a.dataset.alerte = '1'; else delete a.dataset.alerte;
        const b = a.querySelector('.centre b'); if (b) { b.textContent = v.texte ?? `${Math.round(p)} %`; b.className = v.ton || ''; }
        if (v.note != null) { const n = el.querySelector(':scope > .n'); if (n) n.textContent = v.note; }
      }
    },
    // Le travail en cours : une pastille en bas de l'écran, avec ce que fait l'appli (« Nettoyage... »)
    occupe(texte) {
      const o = document.getElementById('occupe'); if (!o) return;
      if (texte) { o.querySelector('span').textContent = texte; o.classList.add('vu'); } else o.classList.remove('vu');
    },
    // Une question par-dessus l'écran (« Continuer ? ») : la réponse part à l'appli, l'écran reprend
    question(q) {
      const fond = document.getElementById('questionM'); if (!fond) return;
      const boutons = (q.boutons?.length ? q.boutons : [{ id: 'ok', texte: 'OK' }]);
      fond.innerHTML = `<div class="boite plasma-bord fort" role="dialog" aria-modal="true">${q.titre ? `<h3>${esc(q.titre)}</h3>` : ''}<p>${esc(q.texte).replace(/\n/g, '<br>')}</p>
        <div class="boutons">${boutons.map((b) => `<button type="button" class="btn${b.style ? ` ${esc(b.style)}` : ''}" data-rep="${esc(b.id)}">${esc(b.texte)}</button>`).join('')}</div></div>`;
      fond.classList.add('vu'); son.ok();
      const repondre = (v) => { fond.classList.remove('vu'); fond.innerHTML = ''; removeEventListener('keydown', clavier, true); envoyer({ type: 'reponse', id: q.id, valeur: v }); };
      const clavier = (e) => {
        if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); repondre(boutons[boutons.length - 1].id); }
        else if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); const f = document.activeElement?.dataset?.rep; repondre(f || boutons[0].id); }
        else if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') { e.preventDefault(); e.stopPropagation(); const l = [...fond.querySelectorAll('[data-rep]')], i = l.indexOf(document.activeElement); l[(i + (e.key === 'ArrowRight' ? 1 : l.length - 1)) % l.length]?.focus(); }
        else if (e.key.startsWith('Arrow') || e.key === 'Backspace') { e.preventDefault(); e.stopPropagation(); }
      };
      addEventListener('keydown', clavier, true);
      fond.querySelectorAll('[data-rep]').forEach((b) => b.addEventListener('click', () => repondre(b.dataset.rep)));
      fond.querySelector('[data-rep]')?.focus();
    },
    pause() { arreter(); window.kitPlasma?.pause?.(); },
    reprendre() { demarrer(); window.kitPlasma?.reprendre?.(); },
    ecran: () => nav.ecran,
  };
  document.body.dataset.theme = 'neon';
  demarrer();
  envoyer({ type: 'pret' });
})();
