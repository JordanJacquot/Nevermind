// Le kit d'écrans des applis de la suite, façon console. L'appli (PowerShell) ne dessine rien elle-même : elle envoie
// son état sous forme de JSON avec kit.etat({...}), et le kit dessine l'accueil, les pages, les questions.
// Chaque geste de l'utilisateur repart vers l'appli sous forme de message : { type: 'action', id, valeur, ecran }.
//
// L'état envoyé par l'appli (tout est facultatif sauf app et accueil) :
//   app       : { nom, version, logo, phrase }           logo = chemin d'une image dans le dossier ui ; phrase = à côté
//               du nom sur l'accueil (« Salut Jordan ») ; celle d'un thème perso passe avant
//   theme     : 'neon' | 'crepuscule' | 'terminal' | 'arcade' | 'rubis'
//   connexion : 'cable' | 'wifi' | 'aucune'              (absent : l'icône n'est pas affichée)
//   sons      : true | false
//   couleurs  : { P, S, T, fond, fond2, carte, carte2, ligne, texte, doux, pale }   un thème perso : ses couleurs à lui
//   pack      : les images d'un thème perso (adresses https://theme.local/..., servies par vue.ps1) :
//               { logo: { url, images, delai, lueur }, fond: { url, opacite, x, y }, mascotte: { url, images, delai, miroir },
//                 bas: { url, images, delai, hauteur }, police: { url, partout, pixel }, phrase }
//               images = nombre d'images de la planche (côte à côte) ; une seule : elle lévite
//   accueil   : { tuiles: [{ id, nom, icone | image | gros, petit, sous, couleur, fond }], ronds: [{ id, nom, icone, couleur }] }
//               fond = une image en fond de la tuile (la jaquette du dernier jeu...) ; beaucoup de tuiles : elles défilent
//               image = une image à la place de l'icône (fichier du dossier ui, ou https://images.local/<fichier> :
//               le dossier « images » de l'appli, servi par vue.ps1)
//               une tuile ouvre la page du même id si elle existe ; sinon elle envoie son action
//               un rond envoie toujours son action (ou ouvre la page du même id)
//   pages     : { id: page }
//     page    : { titre, sections: [{ nom, blocs: [bloc] }] }               liste à gauche, détail à droite
//            ou { titre, question: { etape, texte, choix: [{ id, texte }] } } une question à la fois
//            ou { titre, resultat: { etape, titre, badge, nom, texte, boutons: [bouton] } }
//               retour: 'appli' : la touche B est envoyée à l'appli (action id 'retour') au lieu de revenir à l'accueil
//     bloc    : { type: 'titre', texte } | { type: 'texte', texte }
//               { type: 'score', valeur, sur, titre, texte }
//               { type: 'ligne', id?, texte, note, valeur, ton: 'ok' | 'attention' | 'urgent', fait }
//               { type: 'bascule', id, texte, note, oui }
//               { type: 'bouton', id, texte, style: 'second' | 'danger' }   (les boutons qui se suivent vont sur une ligne)
//               { type: 'champ', id, texte, valeur, indice }               (envoyé quand on quitte le champ ou sur A)
//               { type: 'rdv', id, heure, titre, lieu }
//               { type: 'jauge', texte, valeur (0 à 100), note, ton }       une barre de progression (analyse, disque...)
//               { type: 'liste', elements: [texte], ton }                   une liste à puces
//               { type: 'image', url, legende, hauteur }                    une image (même adresses que image d'une tuile)
//               { type: 'courbe', titre, points: [nombres], etiquettes: [debut, fin], unite, ton }   une courbe simple
//               { type: 'mois', id, annee, mois (1 à 12), choisi: 'AAAA-MM-JJ', jours: { 'AAAA-MM-JJ': { points: [couleur], texte } } }
//                 un calendrier du mois ; un clic sur un jour envoie l'action id avec la date ('AAAA-MM-JJ')
//   Images : un fichier du dossier ui, https://images.local/<fichier> (dossier « images » de l'appli), ou une image
//   ailleurs sur le PC déclarée par l'appli (Get-ImageVue $chemin, vue.ps1 : https://fichier.local/<jeton>)
//   ecran     : id d'une page pour y aller tout de suite (ou 'accueil')
//   focus     : l'élément à choisir (« t:<id> » une tuile, « r:<id> » un rond...), par exemple au retour sur l'accueil
//
// Messages envoyés à l'appli : { type: 'pret' } au chargement, { type: 'action', id, valeur, ecran } à chaque geste,
// { type: 'menu' } quand on fait Retour depuis l'accueil (dans Mindset : retour à la console).
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
    marche: '<path d="M12 3v9M6.3 7.5a8 8 0 1 0 11.4 0"/>',
    wifi: '<path d="M2 9a15 15 0 0 1 20 0M5 13a10 10 0 0 1 14 0M8.5 16.5a5 5 0 0 1 7 0M12 20h.01"/>',
    cable: '<rect x="7" y="2" width="10" height="9" rx="1.5"/><path d="M10 2v3M12 2v3M14 2v3M12 11v4c0 3-5 2-5 7"/>',
    aucune: '<path d="M2 9a15 15 0 0 1 20 0M5 13a10 10 0 0 1 14 0M12 20h.01M3 3l18 18"/>',
    courbe: '<path d="M3 3v18h18M7 15l4-4 3 3 6-7"/>',
    loupe: '<circle cx="11" cy="11" r="7"/><path d="m20 20-4-4"/>',
    actu: '<path d="M20 12a8 8 0 1 1-2.3-5.7M20 4v5h-5"/>',
    reglages: '<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1 7 17M17 7l2.1-2.1"/>',
    aide: '<circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6V14M12 17h.01"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    agenda: '<rect x="3" y="5" width="18" height="16" rx="2"/><path d="M3 10h18M8 3v4M16 3v4"/>',
    cloche: '<path d="M6 8a6 6 0 1 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10 21h4"/>',
    coeur: '<path d="M3 12h4l2-5 4 10 2-5h6"/>',
    liste: '<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>',
    maison: '<path d="M3 11 12 3l9 8M5 9v12h14V9"/>',
    bouclier: '<path d="M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6z"/>',
    poubelle: '<path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14"/>',
    dossier: '<path d="M3 6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
    horloge: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
    euro: '<path d="M18 6a7 7 0 1 0 0 12M4 10h10M4 14h10"/>',
    etoile: '<path d="m12 3 2.8 5.7 6.2.9-4.5 4.4 1 6.2L12 17.3 6.5 20.2l1-6.2L3 9.6l6.2-.9z"/>',
    son: '<path d="M4 9v6h4l5 4V5L8 9zM16 9a4 4 0 0 1 0 6M19 6a8 8 0 0 1 0 12"/>',
    manette: '<path d="M6 9h4M8 7v4M15 10h.01M18 8h.01"/><path d="M7 4h10a5 5 0 0 1 5 5v4a4 4 0 0 1-7 2.6L13.6 14h-3.2L9 15.6A4 4 0 0 1 2 13V9a5 5 0 0 1 5-5z"/>',
    radar: '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="5"/><path d="M12 12 19 5"/>',
    jauge: '<path d="M4 18a9 9 0 1 1 16 0M12 13l4-5"/>',
  };
  const ico = (n, w = 2) => `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${I[n] || I.etoile}</svg>`;
  const esc = (t) => String(t ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
  const couleur = (c) => (c && /^[a-z-]+$/i.test(c) ? `var(--${c})` : esc(c || 'var(--P)'));   // 'P', 'vert'... ou une couleur CSS
  const heure = () => new Date().toLocaleTimeString('fr-FR', { hour: '2-digit', minute: '2-digit' });
  const CONNEXIONS = { cable: 'Câble', wifi: 'Wi-Fi', aucune: 'Pas de connexion' };

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
    else if (!d.kit) kit.etat(d);
  });

  // ---------------------------------------------------------------- affichage
  const ecran = document.getElementById('ecran'), haut = document.getElementById('haut');
  let E = { app: { nom: '' }, accueil: { tuiles: [], ronds: [] }, pages: {} };
  const nav = { ecran: 'accueil', section: 0 };
  let enAttente = false;    // un nouvel état arrivé pendant qu'on tape dans un champ

  // L'en-tête : le logo, la mascotte et le dessin du bas ne sont refaits que s'ils changent (leur animation continue)
  const dejaPose = {};
  function poser(id, cle, faire) {
    if (dejaPose[id] === cle) return;
    dejaPose[id] = cle; const el = document.getElementById(id); el.innerHTML = '';
    if (cle) faire(el);
  }
  // Une adresse d'image sûre : du thème (https://theme.local/...) ou un fichier du dossier ui, sans guillemet ni
  // parenthèse ; ou une image intégrée (data:), dont les parenthèses et apostrophes sont codées
  const urlSure = (u) => {
    if (typeof u !== 'string') return '';
    if (/^(https:\/\/(theme|images|fichier)\.local\/)?[\w%-][\w%./-]*$/.test(u) && !u.includes('..')) return u;
    if (/^data:image\/(png|svg\+xml)[;,]/.test(u) && !/["\\\s]/.test(u)) return u.replace(/\(/g, '%28').replace(/\)/g, '%29').replace(/'/g, '%27');
    return '';
  };
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
    // Le fond
    const d = document.getElementById('decor'), fu = urlSure(p.fond?.url);
    d.classList.toggle('vu', !!fu);
    if (fu) {
      d.style.setProperty('--img', `url('${fu}')`);
      d.style.setProperty('--op', String(Math.min(1, Math.max(0.05, +p.fond.opacite || 0.35))));
      d.style.setProperty('--pos', `${{ gauche: 'left', droite: 'right' }[p.fond.x] || 'center'} ${{ haut: 'top', bas: 'bottom' }[p.fond.y] || 'center'}`);
    }
    // La police
    let st = document.getElementById('policePack'); const pu = urlSure(p.police?.url);
    if (!pu) st?.remove();
    else {
      if (!st) { st = document.createElement('style'); st.id = 'policePack'; document.head.appendChild(st); }
      st.textContent = `@font-face { font-family: "PolicePack"; src: url('${pu}'); }
        ${p.police.partout ? 'body' : '.titre-app, .nom-sel, h2, h3, .tuile .gros, .question h2'} { font-family: "PolicePack", var(--police-app); }`;
    }
    poser('mascotte', p.mascotte ? JSON.stringify(p.mascotte) : '', (el) => planche(el, p.mascotte, 64, pixel));
    poser('dessinBas', p.bas ? JSON.stringify(p.bas) : '', (el) => planche(el, p.bas, Math.min(48, Math.max(16, (p.bas.hauteur | 0) || 32)), pixel));
  }
  function enTete(titre) {
    const a = E.app || {}, p = E.pack || {};
    haut.className = `haut${titre ? ' en-page' : ''}`;
    poser('logoHote', JSON.stringify([a.logo, p.logo]), (el) => {
      if (p.logo) planche(el, p.logo, 46, !!p.police?.pixel);   // le logo du thème perso, à la place de celui de l'appli
      else if (urlSure(a.logo)) el.innerHTML = `<span class="logo" id="logo" style="--logo:url('${urlSure(a.logo)}')"><i class="b"></i><i class="c c1"></i><i class="c c2"></i></span>`;
    });
    document.getElementById('titreTexte').innerHTML = titre ? `<span class="chemin">${esc(a.nom)}</span><span class="sep">›</span>${esc(titre)}`
      : `${esc(a.nom)}${p.phrase || a.phrase ? `<span class="phrase">${esc(p.phrase || a.phrase)}</span>` : ''}`;
    const cnx = E.connexion && CONNEXIONS[E.connexion] ? `<span class="cnx" title="Connexion à internet">${ico(E.connexion)}<span>${CONNEXIONS[E.connexion]}</span></span>` : '';
    document.getElementById('etatHaut').innerHTML = `${cnx}<span class="hh">${heure()}</span>`;
    document.getElementById('basG').textContent = a.version ? `${a.nom} ${a.version}` : '';
  }

  let garderDefilement = null;   // (la rangée de tuiles garde sa place quand l'accueil est redessiné)
  function placerNom() {   // le nom de la tuile choisie, au-dessus d'elle
    const el = document.activeElement, ns = document.getElementById('nomSel');
    if (!ns || el?.dataset?.tuile === undefined) return;
    ns.style.paddingLeft = `${Math.max(0, el.offsetLeft - el.parentElement.scrollLeft - 14)}px`;
  }
  function accueil(focus, anime) {
    nav.ecran = 'accueil'; enTete(null);
    const t = E.accueil?.tuiles || [], r = E.accueil?.ronds || [];
    ecran.innerHTML = `<div class="accueil${anime ? ' entree' : ''}">
      <div class="nom-sel" id="nomSel"></div>
      <div class="rangee">${t.map((x, i) => `<button type="button" class="tuile f${urlSure(x.fond) ? ' avec-fond' : ''}" data-k="t:${esc(x.id)}" data-tuile="${i}" style="--a:${couleur(x.couleur)}${urlSure(x.fond) ? `;--fond:url('${urlSure(x.fond)}')` : ''}" aria-label="${esc(x.nom)}">
        <span class="reflet"></span>${x.gros != null ? `<span class="gros">${esc(x.gros)}${x.petit ? `<small>${esc(x.petit)}</small>` : ''}</span>` : (urlSure(x.image) ? `<span class="ico"><img src="${urlSure(x.image)}" alt="" onerror="this.remove()"></span>` : `<span class="ico">${ico(x.icone, 1.8)}</span>`)}<span class="sous">${esc(x.sous)}</span></button>`).join('')}</div>
      <div class="separe"></div>
      <div class="ronds">${r.map((x, i) => `<button type="button" class="rond f" data-k="r:${esc(x.id)}" data-rond="${i}" style="--a:${couleur(x.couleur)}" aria-label="${esc(x.nom)}">${ico(x.icone)}<span>${esc(x.nom)}</span></button>`).join('')}</div></div>`;
    const rangee = ecran.querySelector('.rangee');
    if (garderDefilement != null) rangee.scrollLeft = garderDefilement;
    rangee.addEventListener('wheel', (e) => { if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) { rangee.scrollLeft += e.deltaY; e.preventDefault(); } }, { passive: false });
    rangee.addEventListener('scroll', () => { garderDefilement = rangee.scrollLeft; placerNom(); });
    const cible = (focus && ecran.querySelector(`[data-k="${CSS.escape(focus)}"]`)) || ecran.querySelector('.tuile, .rond');
    cible?.focus({ preventScroll: true }); cible?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }

  function bloc(b) {
    const note = b.note ? `<span class="n">${esc(b.note)}</span>` : '';
    switch (b.type) {
      case 'titre': return `<h2>${esc(b.texte)}</h2>`;
      case 'texte': return `<p class="texte">${esc(b.texte)}</p>`;
      case 'score': {
        const p = Math.max(0, Math.min(100, (b.valeur / (b.sur || 100)) * 100));
        return `<div class="score"><div class="anneau" style="--p:${p}"><div><b>${esc(b.valeur)}</b><small>sur ${esc(b.sur || 100)}</small></div></div><div><h3>${esc(b.titre)}</h3><p>${esc(b.texte)}</p></div></div>`;
      }
      case 'ligne': {
        const v = `<span class="v${b.ton ? ` ${esc(b.ton)}` : ''}">${esc(b.valeur)}</span>`;
        return b.id
          ? `<button type="button" class="ligne f${b.fait ? ' fait' : ''}" data-k="l:${esc(b.id)}" data-action="${esc(b.id)}"><span>${esc(b.texte)}${note}</span>${v}</button>`
          : `<div class="ligne${b.fait ? ' fait' : ''}"><span>${esc(b.texte)}${note}</span>${v}</div>`;
      }
      case 'bascule': return `<button type="button" class="ligne f" data-k="b:${esc(b.id)}" data-bascule="${esc(b.id)}" role="switch" aria-checked="${!!b.oui}"><span>${esc(b.texte)}${note}</span><span class="bascule${b.oui ? ' oui' : ''}"></span></button>`;
      case 'bouton': return `<button type="button" class="btn f${b.style ? ` ${esc(b.style)}` : ''}" data-k="o:${esc(b.id)}" data-action="${esc(b.id)}">${esc(b.texte)}</button>`;
      case 'champ': return `<label class="ligne"><span>${esc(b.texte)}${note}</span><input class="champ f" data-k="c:${esc(b.id)}" data-champ="${esc(b.id)}" value="${esc(b.valeur)}" placeholder="${esc(b.indice)}"></label>`;
      case 'jauge': {
        const p = Math.max(0, Math.min(100, +b.valeur || 0));
        return `<div class="jauge${b.ton ? ` ${esc(b.ton)}` : ''}"><div class="jauge-haut"><span>${esc(b.texte)}${note}</span><span class="v">${Math.round(p)} %</span></div><div class="barre"><i style="width:${p}%"></i></div></div>`;
      }
      case 'liste': return `<ul class="puces${b.ton ? ` ${esc(b.ton)}` : ''}">${(b.elements || []).map((t) => `<li>${esc(t)}</li>`).join('')}</ul>`;
      case 'image': {
        const u = urlSure(b.url); if (!u) return '';
        return `<figure class="image"><img src="${u}" alt="" onerror="this.parentNode.remove()" style="max-height:${Math.min(600, Math.max(60, +b.hauteur || 260))}px">${b.legende ? `<figcaption>${esc(b.legende)}</figcaption>` : ''}</figure>`;
      }
      case 'courbe': {
        const pts = (b.points || []).map(Number).filter((v) => isFinite(v));
        if (pts.length < 2) return '';
        const min = Math.min(...pts), max = Math.max(...pts), et = max - min || 1, L = 600, H = 120;
        const xy = pts.map((v, i) => `${(i / (pts.length - 1)) * L},${H - 8 - ((v - min) / et) * (H - 16)}`).join(' ');
        const e = b.etiquettes || [];
        return `<div class="courbe${b.ton ? ` ${esc(b.ton)}` : ''}"><div class="jauge-haut"><span>${esc(b.titre)}</span><span class="v">${esc(pts[pts.length - 1])}${b.unite ? ` ${esc(b.unite)}` : ''}</span></div>
          <svg viewBox="0 0 ${L} ${H}" preserveAspectRatio="none" aria-hidden="true"><polygon points="0,${H} ${xy} ${L},${H}"/><polyline points="${xy}"/></svg>
          <div class="courbe-bas"><span>${esc(e[0])}</span><span>min ${esc(min)}${b.unite ? ` ${esc(b.unite)}` : ''} · max ${esc(max)}${b.unite ? ` ${esc(b.unite)}` : ''}</span><span>${esc(e[1])}</span></div></div>`;
      }
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
      case 'rdv': return `<button type="button" class="rdv f" data-k="v:${esc(b.id)}" data-action="${esc(b.id)}"><span class="h">${esc(b.heure)}</span><b>${esc(b.titre)}</b><span>${esc(b.lieu)}</span></button>`;
      default: return '';
    }
  }
  function blocs(liste) {   // les boutons qui se suivent sont rangés sur une même ligne
    let h = '', btn = '';
    for (const b of liste || []) {
      if (b.type === 'bouton') { btn += bloc(b); continue; }
      if (btn) { h += `<div class="boutons">${btn}</div>`; btn = ''; }
      h += bloc(b);
    }
    return h + (btn ? `<div class="boutons">${btn}</div>` : '');
  }

  function page(premiere, focus) {
    const p = E.pages?.[nav.ecran];
    if (!p) { accueil(`t:${nav.ecran}`); return; }
    enTete(p.titre);
    if (p.question || p.resultat) { question(p, focus); return; }
    const s = p.sections || [];
    if (nav.section >= s.length) nav.section = 0;
    const seule = s.length < 2;
    ecran.innerHTML = `<div class="page"><div class="page-int${seule ? ' seule' : ''}${premiere ? ' entree' : ''}">${seule ? '' : `<nav class="liste plaque" aria-label="${esc(p.titre)}">${s.map((x, i) => `<button type="button" class="elt f${i === nav.section ? ' choisi' : ''}" data-k="s:${i}" data-section="${i}">${esc(x.nom)}</button>`).join('')}</nav>`}<section class="detail plaque" id="detail"></section></div></div>`;
    detail();
    const cible = (focus && ecran.querySelector(`[data-k="${CSS.escape(focus)}"]`)) || ecran.querySelector(`.elt[data-section="${nav.section}"]`) || ecran.querySelector('#detail .f');
    cible?.focus({ preventScroll: !premiere });
  }
  function detail() {
    const d = document.getElementById('detail'); if (!d) return;
    const s = E.pages[nav.ecran].sections?.[nav.section];
    d.innerHTML = s ? blocs(s.blocs) : '';
  }
  function question(p, focus) {
    if (p.question) {
      const q = p.question;
      ecran.innerHTML = `<div class="question entree">${q.etape ? `<div class="etape">${esc(q.etape)}</div>` : ''}<h2>${esc(q.texte)}</h2><div class="choix">${(q.choix || []).map((c, i) => `<button type="button" class="rep f" data-k="q:${esc(c.id)}" data-action="${esc(c.id)}"><i>${'ABCDEFGH'[i] || ''}</i>${esc(c.texte)}</button>`).join('')}</div></div>`;
    } else {
      const r = p.resultat;
      ecran.innerHTML = `<div class="question entree">${r.etape ? `<div class="etape">${esc(r.etape)}</div>` : ''}<h2>${esc(r.titre)}</h2>
        <div class="resultat plaque">${r.badge ? `<span class="badge">${esc(r.badge)}</span>` : ''}${r.nom ? `<h3>${esc(r.nom)}</h3>` : ''}<p>${esc(r.texte)}</p>${blocs((r.boutons || []).map((b) => ({ type: 'bouton', ...b })))}</div></div>`;
    }
    ((focus && ecran.querySelector(`[data-k="${CSS.escape(focus)}"]`)) || ecran.querySelector('.f'))?.focus();
  }

  function dessiner(anime) {
    const k = document.activeElement?.closest?.('#ecran [data-k]')?.dataset.k;
    if (nav.ecran === 'accueil') accueil(k, anime); else page(anime, k);
  }
  function aller(id) {
    if (id === 'accueil' || !E.pages?.[id]) { const avant = nav.ecran; accueil(avant === 'accueil' ? null : `t:${avant}`, true); return; }
    nav.ecran = id; nav.section = 0; page(true);
  }
  function retour() {
    son.retour();
    if (nav.ecran === 'accueil') { envoyer({ type: 'menu' }); return; }
    if (E.pages?.[nav.ecran]?.retour === 'appli') { envoyer({ type: 'action', id: 'retour', ecran: nav.ecran }); return; }
    const liste = ecran.querySelector('.elt.choisi');
    if (liste && document.activeElement?.closest?.('#detail')) { liste.focus(); return; }   // d'abord revenir à la liste
    aller('accueil');
  }
  const action = (id, valeur) => envoyer({ type: 'action', id, valeur, ecran: nav.ecran });

  // ---------------------------------------------------------------- valider (clic, A ou Entrée)
  function valider(el) {
    if (!el) return;
    const d = el.dataset;
    if (d.tuile !== undefined) { const t = E.accueil.tuiles[+d.tuile]; son.ok(); if (E.pages?.[t.id]) aller(t.id); else action(t.id); return; }
    if (d.rond !== undefined) { const r = E.accueil.ronds[+d.rond]; son.ok(); if (E.pages?.[r.id]) aller(r.id); else action(r.id); return; }
    if (d.section !== undefined) { son.ok(); document.querySelector('#detail .f')?.focus(); return; }
    if (d.bascule) {   // on bascule tout de suite à l'écran, l'appli confirme avec son prochain état
      const b = el.querySelector('.bascule'), oui = !b.classList.contains('oui');
      b.classList.toggle('oui', oui); el.setAttribute('aria-checked', String(oui)); son.bascule(); action(d.bascule, oui); return;
    }
    if (d.champ) { son.ok(); action(d.champ, el.value); return; }
    if (d.action) { son.ok(); action(d.action, d.valeur); }
  }

  // ---------------------------------------------------------------- le relief qui suit la souris
  addEventListener('pointermove', (e) => {
    const t = e.target.closest?.('.tuile'); if (!t) return;
    const r = t.getBoundingClientRect(), px = (e.clientX - r.left) / r.width - 0.5, py = (e.clientY - r.top) / r.height - 0.5;
    t.style.setProperty('--ry', `${px * 18}deg`); t.style.setProperty('--rx', `${-py * 18}deg`); t.style.setProperty('--bx', `${100 - (px + 0.5) * 100}%`);
  });
  ecran.addEventListener('pointerout', (e) => { const t = e.target.closest?.('.tuile'); if (t && !t.contains(e.relatedTarget)) { t.style.removeProperty('--rx'); t.style.removeProperty('--ry'); } });

  // ---------------------------------------------------------------- la sélection : souris, flèches, A (Entrée) et B (Échap)
  let dernier = null, derniereTuile = null;
  ecran.addEventListener('focusin', (e) => {
    const el = e.target.closest('.f'); if (!el) return;
    if (dernier && dernier !== el && dernier.isConnected) son.tic(); dernier = el;
    if (el.dataset.tuile !== undefined) {
      derniereTuile = el;
      const ns = document.getElementById('nomSel'); ns.textContent = E.accueil.tuiles[+el.dataset.tuile].nom;
      el.scrollIntoView({ block: 'nearest', inline: 'nearest', behavior: 'smooth' }); placerNom();
    } else if (el.dataset.rond !== undefined) document.getElementById('nomSel').textContent = '';
    if (el.dataset.section !== undefined && +el.dataset.section !== nav.section) {
      nav.section = +el.dataset.section; ecran.querySelectorAll('.elt').forEach((x) => x.classList.toggle('choisi', x === el)); detail();
    }
  });
  ecran.addEventListener('focusout', (e) => {
    const el = e.target;
    if (el.dataset?.champ && el.value !== el.defaultValue) { el.defaultValue = el.value; action(el.dataset.champ, el.value); }
    if (enAttente && el.tagName === 'INPUT') setTimeout(() => { if (document.activeElement?.tagName !== 'INPUT') { enAttente = false; dessiner(false); } });
  });
  ecran.addEventListener('pointerover', (e) => { const el = e.target.closest('.f'); if (el && el !== document.activeElement && el.tagName !== 'INPUT' && document.activeElement?.tagName !== 'INPUT') el.focus({ preventScroll: true }); });
  ecran.addEventListener('click', (e) => { const el = e.target.closest('.f'); if (el && el.tagName !== 'INPUT') valider(el); });
  document.getElementById('aideA').addEventListener('click', () => valider(document.activeElement?.closest?.('#ecran .f')));
  document.getElementById('aideB').addEventListener('click', retour);
  function voisin(dir) {
    const cur = document.activeElement?.closest?.('#ecran .f'), els = [...ecran.querySelectorAll('.f')];
    if (!cur) { els[0]?.focus(); return; }
    // Des ronds vers le haut : la tuile d'où l'on venait, comme sur une console
    if (dir === 'h' && cur.classList.contains('rond') && derniereTuile?.isConnected) { derniereTuile.focus(); return; }
    const a = cur.getBoundingClientRect(), ax = a.left + a.width / 2, ay = a.top + a.height / 2;
    let best = null, bd = Infinity;
    for (const e of els) {
      if (e === cur) continue;
      const b = e.getBoundingClientRect(), dx = b.left + b.width / 2 - ax, dy = b.top + b.height / 2 - ay;
      if ((dir === 'd' && dx <= 6) || (dir === 'g' && dx >= -6) || (dir === 'b' && dy <= 6) || (dir === 'h' && dy >= -6)) continue;
      const horiz = dir === 'd' || dir === 'g', d = (horiz ? Math.abs(dx) : Math.abs(dy)) + (horiz ? Math.abs(dy) : Math.abs(dx)) * 2.5;
      if (d < bd) { bd = d; best = e; }
    }
    if (best) { best.focus(); best.scrollIntoView({ block: 'nearest' }); }
  }
  addEventListener('keydown', (e) => {
    const dans = e.target.tagName === 'INPUT', k = { ArrowRight: 'd', ArrowLeft: 'g', ArrowDown: 'b', ArrowUp: 'h' }[e.key];
    if (k && !(dans && (k === 'd' || k === 'g'))) { e.preventDefault(); voisin(k); return; }
    if (e.key === 'Enter') { e.preventDefault(); valider(document.activeElement?.closest?.('#ecran .f')); return; }
    if (e.key === 'Escape' || (e.key === 'Backspace' && !dans)) { e.preventDefault(); retour(); }
  });

  // ---------------------------------------------------------------- horloge et logo qui « glitche » (arrêtés en pause)
  let minuteries = [];
  function demarrer() {
    if (minuteries.length) return;
    document.querySelectorAll('.hh').forEach((h) => { h.textContent = heure(); });   // (l'heure a pu changer pendant la pause)
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
      const avantTheme = E.theme;
      E = { ...E, ...nouveau };
      son.actif = E.sons !== false;
      if (E.theme !== avantTheme || 'couleurs' in nouveau) {
        document.body.dataset.theme = E.theme || 'neon';
        for (const v of ['P', 'S', 'T', 'fond', 'fond2', 'carte', 'carte2', 'ligne', 'texte', 'doux', 'pale']) {
          const c = E.couleurs?.[v];
          if (c && /^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(c)) document.body.style.setProperty(`--${v}`, c); else document.body.style.removeProperty(`--${v}`);
        }
        window.kitFond?.couleurs();
      }
      pack();
      const choisir = () => {   // (focus : l'élément à choisir une fois l'écran dessiné)
        if (!nouveau.focus) return;
        const k = nouveau.focus; delete E.focus;
        const el = ecran.querySelector(`[data-k="${CSS.escape(String(k))}"]`);
        if (el) { el.focus({ preventScroll: true }); el.scrollIntoView({ block: 'nearest', inline: 'nearest' }); }
      };
      if (nouveau.ecran) { const id = nouveau.ecran; delete E.ecran; aller(id); choisir(); return; }
      if (nav.ecran !== 'accueil' && !E.pages?.[nav.ecran]) { aller('accueil'); choisir(); return; }
      if (document.activeElement?.tagName === 'INPUT') { enAttente = true; enTete(nav.ecran === 'accueil' ? null : E.pages[nav.ecran].titre); return; }
      dessiner(false);
      choisir();
    },
    aller,
    message(texte, bravo) {
      const b = document.getElementById('bulle'); b.textContent = texte; b.classList.add('vu');
      if (bravo) son.bravo();
      clearTimeout(bulleMinuterie); bulleMinuterie = setTimeout(() => b.classList.remove('vu'), 2600);
    },
    pause() { arreter(); window.kitFond?.pause(); },
    reprendre() { demarrer(); window.kitFond?.reprendre(); },
    ecran: () => nav.ecran,
  };
  document.body.dataset.theme = 'neon';
  demarrer();
  envoyer({ type: 'pret' });
})();
