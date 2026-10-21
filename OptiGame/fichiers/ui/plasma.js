// La 3D du kit (DA « console d'un futur lointain ») : les tuiles de l'accueil à bord de plasma, les anneaux de plasma
// des pages, et le système solaire du réseau. Les couleurs viennent du thème (--P, --S, --T) ; tout s'arrête quand
// l'appli est cachée ou en pause. Sans 3D (WebGL absent), le kit garde ses écrans à plat : rien ne casse.
//
// Le kit (kit.js) pose dans la page des éléments que ce fichier repère à chaque image :
//   #accueil3d               la rangée de tuiles (kit.js appelle kitPlasma.accueil(tuiles, choisie))
//   [data-anneau="0.96"]      un anneau de plasma (0 à 1), dessiné sur une toile transparente par-dessus la page ;
//                            data-ep : son épaisseur, data-alerte : orange (au-dessus de 85 % d'une mesure)
//   .reseau3d[data-reseau]   le système solaire (la box au centre, les appareils en orbite selon leur ping)
// Il prévient kit.js par des événements : « kit:tuile » (la tuile choisie change) et « kit:ouvrir » (clic sur celle du
// milieu).
import * as THREE from 'three';

let essai;
try { const c = document.createElement('canvas'); essai = c.getContext('webgl2') || c.getContext('webgl'); } catch (e) { essai = null; }
if (!essai) { window.kitPlasma = { ok: false }; }
else demarrer();

function demarrer() {
  const I = () => window.kitIcones || {};
  const lire = (n, d) => getComputedStyle(document.body).getPropertyValue(n).trim() || d;
  const police = () => lire('--police-app', '"Segoe UI Variable Display", "Segoe UI", sans-serif');
  const coul = { P: new THREE.Color(), S: new THREE.Color(), T: new THREE.Color(), O: new THREE.Color(), fond: new THREE.Color() };
  function couleurs() {
    coul.P.set(lire('--P', '#00E5FF')); coul.S.set(lire('--S', '#B04BFF')); coul.T.set(lire('--T', '#FF2EB5'));
    coul.O.set(lire('--orange', '#FFB020')); coul.fond.set(lire('--fond', '#06050F'));
    if (acc) acc.couleurs(); if (sol) sol.couleurs();
  }
  const unis = () => ({ c1: { value: coul.P }, c2: { value: coul.S }, c3: { value: coul.T }, co: { value: coul.O } });

  // ---------------------------------------------------------------- les shaders : l'anneau et le bord de plasma
  const vsUv = 'varying vec2 vUv; void main() { vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }';
  // L'anneau : le plasma coule du cyan au violet puis au magenta le long de la partie remplie ; orange en alerte
  const fsAnneau = `uniform float t; uniform float v; uniform float force; uniform float alerte; uniform vec3 c1; uniform vec3 c2; uniform vec3 c3; uniform vec3 co; varying vec2 vUv;
    void main() { float u = vUv.x; float dedans = step(u, v); float k = clamp(u / max(v, 0.001), 0.0, 1.0);
      vec3 c = k < 0.5 ? mix(c1, c2, k * 2.0) : mix(c2, c3, k * 2.0 - 1.0); c = mix(c, co, alerte);
      float n = sin(u * 70.0 - t * (6.0 + v * 5.0)) * sin(vUv.y * 6.283 + t * 2.0) * 0.5 + 0.5; float n2 = sin(u * 23.0 + t * 1.3) * sin(vUv.y * 12.566 - t * 3.0) * 0.5 + 0.5;
      vec3 vif = c * (0.55 + 0.6 * n * n2) * force + vec3(1.0) * pow(n * n2, 4.0) * 0.5;
      float bout = smoothstep(0.035, 0.0, abs(u - v)) * 1.2 * step(v, 0.999) * step(0.004, v);
      gl_FragColor = vec4(mix(vec3(0.10, 0.09, 0.22), vif, dedans) + c * bout, 1.0); }`;
  // Le bord d'une tuile : un rectangle arrondi de plasma qui tourne
  const fsBord = `uniform float t; uniform float force; uniform vec2 taille; uniform vec3 c1; uniform vec3 c2; uniform vec3 c3; uniform vec3 co; uniform float alerte; varying vec2 vUv;
    float sd(vec2 p, vec2 b, float r) { vec2 q = abs(p) - b + r; return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r; }
    void main() { vec2 p = (vUv - 0.5) * taille; float d = sd(p, taille * 0.5 - 0.06, 0.22);
      float bord = smoothstep(0.05, 0.0, abs(d + 0.03)); float halo = smoothstep(0.18, 0.0, abs(d + 0.03)) * 0.35;
      float a = atan(p.y, p.x) / 6.2832 + 0.5; float k = fract(a + t * (0.05 + force * 0.12));
      vec3 c = k < 0.33 ? mix(c1, c2, k * 3.0) : (k < 0.66 ? mix(c2, c3, k * 3.0 - 1.0) : mix(c3, c1, k * 3.0 - 2.0)); c = mix(c, co, alerte * 0.7);
      float n = sin(a * 90.0 - t * 5.0) * 0.5 + 0.5;
      gl_FragColor = vec4(c * (0.8 + n * 0.5), bord * (0.35 + 0.65 * force) * (0.7 + 0.3 * n) + halo * force); }`;

  // Une image d'icône (traits) prête à dessiner dans une toile
  const imagesIc = {};
  function imageIcone(nom, couleur) {
    const cle = nom + couleur; if (imagesIc[cle]) return imagesIc[cle];
    const im = new Image(); imagesIc[cle] = im;
    im.src = 'data:image/svg+xml,' + encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 24 24" fill="none" stroke="${couleur}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">${I()[nom] || I().etoile || ''}</svg>`);
    return im;
  }
  const quandPrete = (im, f) => { if (im.complete && im.naturalWidth) f(); else im.addEventListener('load', f, { once: true }); };
  // Une image d'ailleurs (jaquette) : chargée « sans identité » pour pouvoir la mettre dans la 3D ; si le serveur ne
  // l'autorise pas, elle est simplement oubliée
  function imageLoin(url, f) {
    if (!url) return;
    const im = new Image(); im.crossOrigin = 'anonymous';
    im.onload = () => f(im); im.onerror = () => {};
    im.src = url;
  }
  const hexa = (c) => `#${c.getHexString()}`;
  const couleurCss = (c) => {   // 'P', 'vert', '#hex'... en couleur de toile
    if (!c) return hexa(coul.P);
    if (/^[a-z-]+$/i.test(c)) return lire(`--${c}`, hexa(coul.P));
    return c;
  };

  // ---------------------------------------------------------------- l'accueil : la rangée de tuiles
  const PAS = 2.9;
  // Un bloc arrondi (comme RoundedBoxGeometry, avec les outils de base) : un rectangle arrondi extrudé
  function blocArrondi(l, h, ep, r) {
    const s = new THREE.Shape(), x = -l / 2, y = -h / 2;
    s.moveTo(x + r, y); s.lineTo(x + l - r, y); s.quadraticCurveTo(x + l, y, x + l, y + r); s.lineTo(x + l, y + h - r); s.quadraticCurveTo(x + l, y + h, x + l - r, y + h);
    s.lineTo(x + r, y + h); s.quadraticCurveTo(x, y + h, x, y + h - r); s.lineTo(x, y + r); s.quadraticCurveTo(x, y, x + r, y);
    const g = new THREE.ExtrudeGeometry(s, { depth: ep, bevelEnabled: true, bevelThickness: 0.08, bevelSize: 0.06, bevelSegments: 4, curveSegments: 10 });
    g.translate(0, 0, -ep / 2); return g;
  }
  let acc = null;
  function creerAccueil() {
    const toile = document.createElement('canvas'); toile.className = 'toile3d';
    const r = new THREE.WebGLRenderer({ canvas: toile, antialias: true, powerPreference: 'high-performance' });
    r.setPixelRatio(Math.min(devicePixelRatio, 2)); r.toneMapping = THREE.ACESFilmicToneMapping;
    const scene = new THREE.Scene(), cam = new THREE.PerspectiveCamera(38, 1, 0.1, 200);
    const etoiles = new THREE.Points(new THREE.BufferGeometry().setAttribute('position', new THREE.Float32BufferAttribute(Array.from({ length: 2400 }, (_, i) => (i % 3 === 0 ? (Math.random() - 0.5) * 60 : i % 3 === 1 ? Math.random() * 22 - 6 : -8 - Math.random() * 30)), 3)),
      new THREE.PointsMaterial({ size: 0.05, transparent: true, opacity: 0.55, depthWrite: false }));
    scene.add(etoiles);
    const solM = new THREE.Mesh(new THREE.CircleGeometry(40, 64), new THREE.MeshStandardMaterial({ color: 0x07060F, metalness: 0.3, roughness: 0.55 }));
    solM.rotation.x = -Math.PI / 2; solM.position.y = -1.9; scene.add(solM);
    scene.add(new THREE.AmbientLight(0xffffff, 0.55)); const lum = new THREE.DirectionalLight(0xffffff, 1.3); lum.position.set(-3, 6, 7); scene.add(lum);
    const rangee = new THREE.Group(); scene.add(rangee);
    const geo = { bloc: blocArrondi(2.3, 2.3, 0.3, 0.22), bord: new THREE.PlaneGeometry(2.6, 2.6), face: new THREE.PlaneGeometry(2.1, 2.1), reflet: new THREE.PlaneGeometry(2.3, 0.04), anneau: new THREE.TorusGeometry(0.42, 0.06, 24, 160) };
    const matBloc = new THREE.MeshStandardMaterial({ color: 0x0C0A1E, metalness: 0.5, roughness: 0.45 });
    const a = {
      toile, r, scene, cam, rangee, items: [], cle: '', pos: 0, cible: 0, sel: 0, tenu: null, souris: { x: 0, y: 0 },
      couleurs() {
        scene.background = coul.fond.clone(); scene.fog = new THREE.Fog(coul.fond, 14, 34);
        etoiles.material.color = coul.P.clone().lerp(new THREE.Color(1, 1, 1), 0.55);
        if (this.donnees) this.construire(this.donnees, true);
      },
      // Le visage d'une tuile, dessiné dans une texture : il bouge avec la tuile
      visage(t) {
        const W = 1024, c = document.createElement('canvas'); c.width = c.height = W; const k = c.getContext('2d');
        const tex = new THREE.CanvasTexture(c); tex.colorSpace = THREE.SRGBColorSpace; tex.anisotropy = 8;
        const couleur = couleurCss(t.couleur), pol = police();
        let icone = null, fond = null;
        const ecrire = () => {
          k.clearRect(0, 0, W, W); k.textAlign = 'center'; k.textBaseline = 'alphabetic';
          if (fond) {   // la jaquette en fond, assombrie vers le bas pour garder le texte lisible
            k.save(); k.beginPath(); k.roundRect(0, 0, W, W, 90); k.clip();
            const s = Math.max(W / fond.width, W / fond.height);
            if (fond.height > fond.width * 1.15) {
              // jaquette en hauteur : la même image floue remplit la tuile, et la jaquette entière est posée en haut (rien de coupé)
              k.filter = 'blur(18px)'; k.globalAlpha = 0.55; k.drawImage(fond, (W - fond.width * s) / 2, (W - fond.height * s) / 2, fond.width * s, fond.height * s); k.filter = 'none';
              const h = W * 0.74, l = fond.width * h / fond.height; k.globalAlpha = 0.95; k.drawImage(fond, (W - l) / 2, W * 0.05, l, h);
            } else { k.globalAlpha = 0.85; k.drawImage(fond, (W - fond.width * s) / 2, (W - fond.height * s) / 2, fond.width * s, fond.height * s); }
            k.globalAlpha = 1; const g = k.createLinearGradient(0, 0, 0, W); g.addColorStop(0.2, 'rgba(6,5,15,0.1)'); g.addColorStop(1, 'rgba(6,5,15,0.92)'); k.fillStyle = g; k.fillRect(0, 0, W, W); k.restore();
          }
          const gros = t.gros != null && t.gros !== '';
          if (gros) {
            const n = String(t.gros), pt = t.petit || '';
            k.font = `800 190px ${pol}`; const wN = k.measureText(n).width; k.font = `700 80px ${pol}`; const wS = pt ? k.measureText(pt).width : 0;
            const x0 = W / 2 - (wN + wS + 10) / 2; k.textAlign = 'left';
            k.fillStyle = t.alerte ? hexa(coul.O) : '#FFFFFF'; k.font = `800 190px ${pol}`; k.fillText(n, x0, 700);
            if (pt) { k.font = `700 80px ${pol}`; k.fillStyle = '#A9A3C9'; k.fillText(pt, x0 + wN + 10, 700); }
            k.textAlign = 'center';
          } else if (icone && !fond) {
            k.save(); k.shadowColor = couleur; k.shadowBlur = 40; k.drawImage(icone, W / 2 - 180, 160, 360, 360); k.restore(); k.drawImage(icone, W / 2 - 180, 160, 360, 360);
          }
          k.fillStyle = '#FFFFFF'; k.font = `800 96px ${pol}`; k.fillText(String(t.nom || ''), W / 2, gros ? 812 : 760, W - 80);
          k.fillStyle = '#C9C3E6'; k.font = `600 60px ${pol}`; k.fillText(String(t.sous || ''), W / 2, gros ? 900 : 860, W - 80);
          tex.needsUpdate = true;
        };
        ecrire();
        if (t.image) imageLoin(t.image, (im) => { icone = im; ecrire(); });
        else if (t.icone || !t.gros) { const im = imageIcone(t.icone || 'etoile', couleur); quandPrete(im, () => { icone = im; ecrire(); }); }
        if (t.fond) imageLoin(t.fond, (im) => { fond = im; ecrire(); });
        document.fonts?.ready?.then(ecrire);
        return tex;
      },
      construire(tuiles, forcer) {
        const cle = JSON.stringify(tuiles);
        if (cle === this.cle && !forcer) return;
        this.cle = cle; this.donnees = tuiles;
        for (const x of this.items) { rangee.remove(x.g); x.face.material.map.dispose(); x.face.material.dispose(); x.bord.material.dispose(); x.anneau?.material.dispose(); }
        this.items = tuiles.map((t, i) => {
          const g = new THREE.Group(); g.position.x = i * PAS; rangee.add(g);
          g.add(new THREE.Mesh(geo.bloc, matBloc));
          const bord = new THREE.Mesh(geo.bord, new THREE.ShaderMaterial({ vertexShader: vsUv, fragmentShader: fsBord, uniforms: { t: { value: 0 }, force: { value: 0.3 }, alerte: { value: t.alerte ? 1 : 0 }, taille: { value: new THREE.Vector2(2.6, 2.6) }, ...unis() }, transparent: true, depthWrite: false, blending: THREE.AdditiveBlending }));
          bord.position.z = 0.26; g.add(bord);
          const face = new THREE.Mesh(geo.face, new THREE.MeshBasicMaterial({ map: this.visage(t), transparent: true, depthWrite: false })); face.position.z = 0.262; g.add(face);
          let anneau = null;
          const v = parseFloat(t.gros);
          if (isFinite(v) && /\/\s*100/.test(t.petit || '')) {
            anneau = new THREE.Mesh(geo.anneau, new THREE.ShaderMaterial({ vertexShader: vsUv, fragmentShader: fsAnneau, uniforms: { t: { value: 0 }, v: { value: Math.max(0, Math.min(1, v / 100)) }, force: { value: 1 }, alerte: { value: 0 }, ...unis() } }));
            anneau.rotation.z = Math.PI / 2; anneau.position.set(0, 0.56, 0.3); g.add(anneau);
          }
          const reflet = new THREE.Mesh(geo.reflet, new THREE.MeshBasicMaterial({ color: 0xFFFFFF, transparent: true, opacity: 0.12 })); reflet.position.set(0, 1.0, 0.27); g.add(reflet);
          return { g, bord, face, anneau };
        });
        this.borner();
      },
      borner() { const n = Math.max(0, this.items.length - 1); this.cible = Math.max(0, Math.min(n, this.cible)); this.sel = Math.max(0, Math.min(n, this.sel)); },
      aller(i, tout) { this.cible = Math.max(0, Math.min(this.items.length - 1, i)); if (tout) this.pos = this.cible; },
      image(t) {
        const hote = document.getElementById('accueil3d'); if (!hote) return;
        if (toile.parentElement !== hote) hote.prepend(toile);
        const w = hote.clientWidth, h = hote.clientHeight; if (!w || !h) return;
        if (this.w !== w || this.h !== h) {
          this.w = w; this.h = h; r.setSize(w, h, false); cam.aspect = w / h;
          // la rangée tient en hauteur, et au moins 3 tuiles en largeur
          const d = Math.max(9.5, 10.5 * Math.min(1.6, 1.75 / Math.min(cam.aspect, 1.75) * (cam.aspect < 1.2 ? 1.25 : 1)));
          cam.position.set(0, 0.5, d); cam.lookAt(0, 0, 0); cam.updateProjectionMatrix();
        }
        this.pos += (this.cible - this.pos) * (this.tenu ? 0.35 : 0.12); rangee.position.x = -this.pos * PAS;
        const n = Math.max(0, Math.min(this.items.length - 1, Math.round(this.pos)));
        if (n !== this.sel) { this.sel = n; document.dispatchEvent(new CustomEvent('kit:tuile', { detail: { i: n } })); }
        this.items.forEach((x, i) => {
          const d = i - this.pos, proche = Math.max(0, 1 - Math.abs(d)), s = i === this.sel && !this.tenu;
          x.g.position.y += ((s ? 0.25 : 0) - x.g.position.y) * 0.1; x.g.position.z = proche * 0.7 - Math.min(2, Math.abs(d)) * 0.25;
          x.g.rotation.y = -Math.max(-1, Math.min(1, d)) * 0.28 + (s ? this.souris.x * 0.2 : 0); x.g.rotation.x += ((s ? this.souris.y * 0.14 : 0) - x.g.rotation.x) * 0.1;
          const u = x.bord.material.uniforms; u.t.value = t; u.force.value += ((s ? 1 : 0.2 + proche * 0.4) - u.force.value) * 0.1;
          if (x.anneau) { x.anneau.material.uniforms.t.value = t; x.anneau.material.uniforms.force.value = s ? 1.2 : 0.7; }
        });
        r.render(scene, cam);
      },
    };
    // Glisser la rangée à la souris (avec élan), un clic sur un côté avance d'une tuile, sur celle du milieu l'ouvre
    const dans = (e) => e.target.closest?.('#accueil3d');
    addEventListener('pointerdown', (e) => {
      if (!dans(e) || e.button !== 0) return;
      a.tenu = { x: e.clientX, c0: a.cible, vx: 0, tx: performance.now(), lx: e.clientX, bouge: false };
      document.getElementById('accueil3d').classList.add('tenu');
    });
    addEventListener('pointermove', (e) => {
      const hote = document.getElementById('accueil3d'); if (!hote) return;
      const b = hote.getBoundingClientRect(); a.souris.x = ((e.clientX - b.left) / b.width) * 2 - 1; a.souris.y = ((e.clientY - b.top) / b.height) * 2 - 1;
      if (!a.tenu) return;
      const px = b.width * 0.2, dx = e.clientX - a.tenu.x; if (Math.abs(dx) > 5) a.tenu.bouge = true;
      a.cible = Math.max(-0.4, Math.min(a.items.length - 0.6, a.tenu.c0 - dx / px));
      const now = performance.now(), dt = Math.max(1, now - a.tenu.tx); a.tenu.vx = (e.clientX - a.tenu.lx) / dt; a.tenu.lx = e.clientX; a.tenu.tx = now;
    });
    const lacher = (e) => {
      if (!a.tenu) return;
      document.getElementById('accueil3d')?.classList.remove('tenu');
      const n = a.items.length - 1;
      if (!a.tenu.bouge) {
        const b = document.getElementById('accueil3d')?.getBoundingClientRect(), x = b ? (e.clientX - b.left) / b.width : 0.5;
        if (Math.abs(x - 0.5) > 0.13) a.cible = Math.max(0, Math.min(n, Math.round(a.cible) + (x > 0.5 ? 1 : -1)));
        else if (dans(e)) document.dispatchEvent(new CustomEvent('kit:ouvrir', { detail: { i: a.sel } }));
      } else a.cible = Math.max(0, Math.min(n, Math.round(a.cible - a.tenu.vx * 3.5)));   // (l'élan du geste)
      a.tenu = null;
    };
    addEventListener('pointerup', lacher); addEventListener('pointercancel', lacher);
    addEventListener('wheel', (e) => { if (!dans(e)) return; e.preventDefault(); a.cible = Math.max(0, Math.min(a.items.length - 1, Math.round(a.cible) + (e.deltaY + e.deltaX > 0 ? 1 : -1))); }, { passive: false });
    a.couleurs();
    return a;
  }

  // ---------------------------------------------------------------- les anneaux de plasma des pages (une toile par-dessus)
  const toileA = document.getElementById('plasma');
  const rA = new THREE.WebGLRenderer({ canvas: toileA, antialias: true, alpha: true }); rA.setPixelRatio(Math.min(devicePixelRatio, 2)); rA.autoClear = false; rA.setClearColor(0x000000, 0);
  const sA = new THREE.Scene(), camA = new THREE.PerspectiveCamera(30, 1, 0.1, 20); camA.position.z = 4.7;
  const matA = new THREE.ShaderMaterial({ vertexShader: vsUv, fragmentShader: fsAnneau, uniforms: { t: { value: 0 }, v: { value: 0.5 }, force: { value: 1.05 }, alerte: { value: 0 }, ...unis() } });
  const geosA = {}; const anneauA = new THREE.Mesh(new THREE.TorusGeometry(1, 0.14, 32, 220), matA); sA.add(anneauA);
  const sourisP = { x: 0, y: 0 }; addEventListener('pointermove', (e) => { sourisP.x = e.clientX; sourisP.y = e.clientY; });
  let aEffacer = false;
  // La valeur affichée de chaque anneau, gardée entre deux rendus : une page redessinée (mise à jour toutes les
  // quelques secondes) recrée ses anneaux, qui repartent alors de leur valeur d'avant et non de 0. La clé : la page,
  // puis l'élément nommé le plus proche (data-id, data-k : une mesure, une ligne...), sinon la place de l'anneau
  const vus = new Map();
  const cleAnneau = (el, i) => {
    const pg = el.closest('.page, .accueil, .question'), n = el.closest('[data-id], [data-k]');
    return `${pg?.dataset.id || ''}|${n && n !== pg ? n.dataset.id || n.dataset.k : `#${i}`}`;
  };
  function anneaux(t) {
    const W = innerWidth, H = innerHeight, els = document.querySelectorAll('#ecran [data-anneau]');
    if (!els.length) { if (aEffacer) { rA.setScissorTest(false); rA.clear(); aEffacer = false; } return; }
    if (toileA.width !== Math.round(W * rA.getPixelRatio()) || toileA.height !== Math.round(H * rA.getPixelRatio())) rA.setSize(W, H, false);
    rA.setScissorTest(false); rA.clear(); rA.setScissorTest(true); aEffacer = true;
    const z = document.getElementById('ecran').getBoundingClientRect();
    els.forEach((el, i) => {
      // (un anneau neuf reprend la valeur affichée de celui qu'il remplace ; au premier affichage, il part de 0)
      if (el._cle === undefined) { el._cle = cleAnneau(el, i); el._v = vus.get(el._cle) ?? 0; }
      const b = el.getBoundingClientRect(); if (b.width < 4 || b.bottom < z.top || b.top > z.bottom || el.offsetParent === null) return;
      const cible = +el.dataset.anneau || 0; el._v += (cible - el._v) * 0.06; vus.set(el._cle, el._v);
      const ep = +(el.dataset.ep || 0.14); if (!geosA[ep]) geosA[ep] = new THREE.TorusGeometry(1, ep, 32, 220); anneauA.geometry = geosA[ep];
      const u = matA.uniforms; u.t.value = t; u.v.value = el._v; u.alerte.value = el.dataset.alerte ? 1 : 0;
      const dx = (sourisP.x - (b.left + b.width / 2)) / W, dy = (sourisP.y - (b.top + b.height / 2)) / H;
      anneauA.rotation.set(dy * 0.6, dx * 0.6, Math.PI / 2);
      const x = Math.floor(b.left), y = Math.floor(H - b.bottom), w = Math.ceil(b.width), hh = Math.ceil(b.height);
      const y0 = Math.max(y, Math.floor(H - z.bottom)), y1 = Math.min(y + hh, Math.ceil(H - z.top)); if (y1 <= y0) return;
      rA.setViewport(x, y, w, hh); rA.setScissor(x, y0, w, y1 - y0);
      camA.aspect = b.width / b.height; camA.updateProjectionMatrix(); rA.render(sA, camA);
    });
  }

  // ---------------------------------------------------------------- le réseau : le système solaire
  let sol = null;
  function creerSolaire() {
    const toile = document.createElement('canvas'); toile.className = 'toile3d';
    const r = new THREE.WebGLRenderer({ canvas: toile, antialias: true }); r.setPixelRatio(Math.min(devicePixelRatio, 2));
    const scene = new THREE.Scene(), cam = new THREE.PerspectiveCamera(40, 2, 0.1, 100);
    const groupe = new THREE.Group(); scene.add(groupe);
    scene.add(new THREE.Points(new THREE.BufferGeometry().setAttribute('position', new THREE.Float32BufferAttribute(Array.from({ length: 900 }, () => (Math.random() - 0.5) * 60), 3)), new THREE.PointsMaterial({ color: 0x8FF4FF, size: 0.05 })));
    scene.add(new THREE.AmbientLight(0xffffff, 0.3)); const lumiere = new THREE.PointLight(0xffffff, 40, 30); scene.add(lumiere);
    const tex = {};
    function texIcone(nom, c) {
      const cle = nom + c; if (tex[cle]) return tex[cle];
      const cv = document.createElement('canvas'); cv.width = cv.height = 256; const k = cv.getContext('2d'); const t = new THREE.CanvasTexture(cv); t.colorSpace = THREE.SRGBColorSpace; tex[cle] = t;
      const fond = () => { k.clearRect(0, 0, 256, 256); k.fillStyle = '#14102A'; k.beginPath(); k.roundRect(8, 8, 240, 240, 60); k.fill(); k.lineWidth = 8; k.strokeStyle = c; k.stroke(); };
      fond(); const im = imageIcone(nom, c); quandPrete(im, () => { fond(); k.drawImage(im, 52, 52, 152, 152); t.needsUpdate = true; });
      return t;
    }
    const s = {
      toile, r, cle: '', ets: [], anims: [], hote: null,
      couleurs() { scene.background = coul.fond.clone().lerp(new THREE.Color(0x0A0820), 0.5); lumiere.color = coul.P.clone(); if (this.donnees) this.construire(this.donnees, true); },
      construire(d, forcer) {
        const cle = JSON.stringify(d); if (cle === this.cle && !forcer) return; this.cle = cle; this.donnees = d;
        while (groupe.children.length) { const o = groupe.children.pop(); o.traverse?.((x) => { x.geometry?.dispose?.(); x.material?.dispose?.(); }); }
        this.ets.forEach((e) => e.d.remove()); this.ets = []; this.anims = [];
        const etq = this.hote?.querySelector('.etq');
        const etiq = (txt, cls, obj, dy) => { if (!etq) return; const el = document.createElement('div'); el.className = `et ${cls}`; el.textContent = txt; etq.appendChild(el); this.ets.push({ d: el, obj, dy }); };
        const sprite = (nom, c, taille) => { const sp = new THREE.Sprite(new THREE.SpriteMaterial({ map: texIcone(nom, c), depthWrite: false })); sp.scale.setScalar(taille); return sp; };
        const soleil = new THREE.Mesh(new THREE.SphereGeometry(0.9, 48, 32), new THREE.MeshStandardMaterial({ color: 0x0A2A3A, emissive: coul.P, emissiveIntensity: 0.9, roughness: 0.3 })); groupe.add(soleil);
        const couronne = new THREE.Mesh(new THREE.SphereGeometry(1.25, 32, 24), new THREE.MeshBasicMaterial({ color: coul.P, transparent: true, opacity: 0.12 })); groupe.add(couronne);
        this.anims.push((t) => couronne.scale.setScalar(1 + Math.sin(t * 2) * 0.04));
        const sb = sprite('box', hexa(coul.P), 0.9); sb.position.set(0, 2.1, 0); groupe.add(sb); etiq(d.centre || 'Box', 'nom centre', sb, 0.8);
        const fam = [coul.P, coul.T, coul.S, new THREE.Color(lire('--vert', '#3DDC84'))];
        const tri = [...(d.appareils || [])].sort((a, b) => a.ping - b.ping), rayons = [2.4, 3.6, 4.8, 6], parAnneau = Math.max(4, Math.ceil(tri.length / 4));
        tri.forEach((a, i) => {
          const n = Math.min(3, Math.floor(i / parAnneau)), rr = rayons[n], ang = (i % parAnneau) / parAnneau * Math.PI * 2 + n * 0.6;
          const c = a.attention || a.nouveau ? coul.O.clone() : (a.couleur ? new THREE.Color(couleurCss(a.couleur)) : fam[i % 4].clone());
          const g = new THREE.Group(); groupe.add(g);
          g.add(new THREE.Mesh(new THREE.SphereGeometry(0.22, 24, 16), new THREE.MeshStandardMaterial({ color: c, emissive: c, emissiveIntensity: 0.5, roughness: 0.4 })));
          const sp = sprite(a.icone || 'inconnu', hexa(c), 0.7); sp.position.y = 0.7; g.add(sp); etiq(a.nom, `nom${a.attention || a.nouveau ? ' attention' : ''}`, sp, 0.55);
          const v = 0.12 / Math.sqrt(rr); this.anims.push((t) => { const u = ang + t * v; g.position.set(Math.cos(u) * rr, 0, Math.sin(u) * rr); });
        });
        rayons.forEach((rr, k) => { const o = new THREE.Mesh(new THREE.TorusGeometry(rr, 0.012, 8, 160), new THREE.MeshBasicMaterial({ color: [coul.P, coul.S, coul.T, fam[3]][k], transparent: true, opacity: 0.45 })); o.rotation.x = Math.PI / 2; groupe.add(o); });
      },
      image(t) {
        const hote = document.querySelector('#ecran .reseau3d'); if (!hote) return;
        if (hote !== this.hote) { this.hote = hote; this.cle = ''; hote.prepend(toile); }
        try { this.construire(JSON.parse(hote.dataset.reseau || '{}')); } catch (e) { return; }
        const b = hote.getBoundingClientRect(), z = document.getElementById('ecran').getBoundingClientRect();
        if (b.bottom < z.top || b.top > z.bottom) return;
        const w = hote.clientWidth, h = hote.clientHeight;
        if (this.w !== w || this.h !== h) { this.w = w; this.h = h; r.setSize(w, h, false); cam.aspect = w / h; cam.updateProjectionMatrix(); }
        this.anims.forEach((f) => f(t));
        // (vue plongeante : les orbites restent bien rondes et les noms ne se chevauchent pas)
        cam.position.set(Math.sin(t * 0.05) * 9.5, 9.5, Math.cos(t * 0.05) * 9.5); cam.lookAt(0, -0.6, 0); r.render(scene, cam);
        const v = new THREE.Vector3(), dir = cam.position.clone().setY(0).normalize();
        for (const e of this.ets) {
          e.obj.getWorldPosition(v);
          // un appareil qui passe derrière la box : son nom s'efface un peu
          const derriere = e.d.classList.contains('centre') ? 0 : -(v.x * dir.x + v.z * dir.z);
          e.d.style.opacity = derriere > 1.2 ? '0.45' : '1';
          v.y += e.dy; v.project(cam); e.d.style.left = `${(v.x + 1) / 2 * w}px`; e.d.style.top = `${(1 - v.y) / 2 * h}px`;
        }
      },
    };
    s.couleurs();
    return s;
  }

  // ---------------------------------------------------------------- la boucle : rien quand l'appli est cachée ou en pause
  let enPause = false, boucle = 0;
  const t0 = performance.now();
  function image() {
    boucle = 0;
    if (document.hidden || enPause) return;
    boucle = requestAnimationFrame(image);
    const t = (performance.now() - t0) / 1000;
    if (document.getElementById('accueil3d')) { acc ??= creerAccueil(); acc.image(t); }
    if (document.querySelector('#ecran .reseau3d')) { sol ??= creerSolaire(); sol.image(t); }
    anneaux(t);
  }
  const relancer = () => { if (!boucle && !document.hidden && !enPause) boucle = requestAnimationFrame(image); };
  document.addEventListener('visibilitychange', relancer);
  couleurs();

  window.kitPlasma = {
    ok: true,
    accueil(tuiles, sel, tout) { acc ??= creerAccueil(); acc.construire(tuiles); acc.aller(sel, tout || acc.items.length !== tuiles.length); acc.sel = Math.round(acc.pos); relancer(); },
    deplacer(d) { if (acc) acc.cible = Math.max(0, Math.min(acc.items.length - 1, Math.round(acc.cible) + d)); },
    aller(i) { acc?.aller(i); },
    sel: () => (acc ? acc.sel : 0),
    couleurs,
    pause() { enPause = true; },
    reprendre() { enPause = false; relancer(); },
  };
  document.body.classList.add('avec-3d');
  window.kit?.redessiner?.();
  relancer();
}
