// La scène 3D derrière les écrans : des étoiles, un sol en grille qui file vers l'horizon, des lueurs aux couleurs
// du thème. 30 images par seconde suffisent pour un fond ; tout s'arrête quand l'appli est cachée ou en pause.
// Si la 3D n'est pas disponible, le fond reste simplement uni.
import * as THREE from 'three';
const hote = document.getElementById('fond3d');
let renderer;
try { renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false, powerPreference: 'high-performance' }); } catch (e) { renderer = null; }
if (renderer) {
  renderer.setPixelRatio(Math.min(devicePixelRatio, 1.5));
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  hote.appendChild(renderer.domElement);
  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(50, 1, 0.1, 100);
  camera.position.set(0, 1.2, 8);
  const tache = (taille) => {
    const cv = document.createElement('canvas'); cv.width = cv.height = 128; const x = cv.getContext('2d');
    const g = x.createRadialGradient(64, 64, 0, 64, 64, 64); g.addColorStop(0, 'rgba(255,255,255,1)'); g.addColorStop(1, 'rgba(255,255,255,0)'); x.fillStyle = g; x.fillRect(0, 0, 128, 128);
    const s = new THREE.Sprite(new THREE.SpriteMaterial({ map: new THREE.CanvasTexture(cv), transparent: true, depthWrite: false, blending: THREE.AdditiveBlending, opacity: 0.5 }));
    s.scale.setScalar(taille); scene.add(s); return s;
  };
  const lueurP = tache(12), lueurT = tache(11), lueurS = tache(16);
  lueurP.position.set(-6, 2, -6); lueurT.position.set(6.5, 0.5, -6); lueurS.position.set(0, -4.2, -3); lueurS.scale.set(22, 6, 1);
  const n = 500, pos = new Float32Array(n * 3);
  for (let i = 0; i < n; i++) { pos[i * 3] = (Math.random() - 0.5) * 40; pos[i * 3 + 1] = Math.random() * 18 - 5; pos[i * 3 + 2] = -4 - Math.random() * 20; }
  const geo = new THREE.BufferGeometry(); geo.setAttribute('position', new THREE.BufferAttribute(pos, 3));
  const etoiles = new THREE.Points(geo, new THREE.PointsMaterial({ size: 0.06, transparent: true, opacity: 0.8, depthWrite: false }));
  scene.add(etoiles);
  const grille = new THREE.GridHelper(60, 60); grille.position.y = -2.6; grille.material.transparent = true; grille.material.opacity = 0.22; scene.add(grille);
  const couleurs = () => {
    const s = getComputedStyle(document.body);
    const c = (v) => new THREE.Color(s.getPropertyValue(v).trim() || '#FFFFFF');
    scene.background = c('--fond'); scene.fog = new THREE.Fog(c('--fond'), 8, 28);
    lueurP.material.color = c('--P'); lueurT.material.color = c('--T'); lueurS.material.color = c('--S');
    etoiles.material.color = c('--P').lerp(new THREE.Color(1, 1, 1), 0.6);
    grille.material.color = c('--S');
  };
  const taille = () => { const w = hote.clientWidth || 1, h = hote.clientHeight || 1; renderer.setSize(w, h, false); camera.aspect = w / h; camera.updateProjectionMatrix(); };
  addEventListener('resize', taille); taille(); couleurs();
  let dernier = 0, enPause = false;
  const image = () => {
    const t = performance.now() / 1000; if (t - dernier < 1 / 30) return; dernier = t;
    camera.position.x += (Math.sin(t * 0.12) * 0.4 - camera.position.x) * 0.02; camera.lookAt(0, 0.4, -6);
    etoiles.rotation.y = t * 0.008;
    grille.position.z = (t * 0.6) % 1;
    lueurP.material.opacity = 0.42 + Math.sin(t * 0.7) * 0.08; lueurT.material.opacity = 0.38 + Math.sin(t * 0.6 + 2) * 0.08;
    renderer.render(scene, camera);
  };
  const marche = () => renderer.setAnimationLoop(document.hidden || enPause ? null : image);
  document.addEventListener('visibilitychange', marche); marche();
  window.kitFond = { couleurs, pause() { enPause = true; marche(); }, reprendre() { enPause = false; marche(); } };
}
