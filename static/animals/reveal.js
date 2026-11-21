// Resolve only this link’s assignment on the server; never download the draw.
const revealButton = document.querySelector('#reveal');
const revealCard = document.querySelector('.reveal-card');
const recipient = document.querySelector('.recipient');
const crossing = document.querySelector('.animal-crossing');
const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
let forest = null;
let finishTimer, crossingTimer;
function alignCrossing() {
  const wasHidden = recipient.hidden;
  recipient.hidden = false;
  const name = document.querySelector('#recipient-name');
  const offset = new DOMMatrixReadOnly(getComputedStyle(recipient).transform).m42;
  const baseline = name.getBoundingClientRect().bottom + scrollY - offset;
  crossing.style.setProperty('--crossing-top', (baseline - crossing.offsetHeight) + 'px');
  recipient.hidden = wasHidden;
}
new ResizeObserver(alignCrossing).observe(document.querySelector('.reveal-stage'));
document.fonts.ready.then(alignCrossing);
addEventListener('resize', alignCrossing);
revealButton.addEventListener('click', async () => {
  revealButton.disabled = true;
  revealButton.textContent = 'Opening your Christmas secret…';
  const status = document.querySelector('#reveal-status');
  status.classList.add('sr-only');
  status.classList.remove('status');
  status.textContent = '';
  try {
    const tokens = new URLSearchParams(location.search).getAll('token');
    if (tokens.length !== 1 || !tokens[0]) throw new Error('Open the personal reveal link from your email.');
    const response = await fetch('/api/selection', {
      method:'POST', headers:{'Content-Type':'application/json'},
      body:JSON.stringify({token:tokens[0]}), cache:'no-store'
    });
    const data = await response.json();
    if (!response.ok) throw new Error(data.error || 'Could not open your reveal. Please try again.');
    document.querySelector('#recipient-name').textContent = data.name;
  } catch (error) {
    status.classList.remove('sr-only');
    status.classList.add('status');
    status.dataset.kind = 'error';
    status.textContent = error instanceof TypeError ? 'Could not connect. Please try again.' : error.message;
    revealButton.disabled = false;
    revealButton.textContent = 'Reveal my name ✧';
    return;
  }
  clearTimeout(finishTimer); clearTimeout(crossingTimer);
  revealButton.disabled = true;
  revealButton.textContent = 'A little Christmas magic…';
  document.querySelector('#reveal-status').textContent = '';
  recipient.hidden = true;
  revealCard.classList.remove('opening', 'revealed');
  crossing.classList.remove('crossing');
  void revealCard.offsetWidth;
  alignCrossing();
  forest?.reveal();
  revealCard.classList.add('opening');
  if (!reducedMotion.matches) {
    for (const animal of ['cat', 'dog']) document.querySelector('.' + animal).src = 'animals/' + animal + '-walk.webp';
    void crossing.offsetWidth;
    crossing.classList.add('crossing');
  }
  finishTimer = setTimeout(() => {
    recipient.hidden = false;
    void recipient.offsetWidth;
    revealCard.classList.add('revealed');
    document.querySelector('#reveal-status').textContent = 'You’re Secret Santa for ' + document.querySelector('#recipient-name').textContent + '.';
  }, reducedMotion.matches ? 0 : 1100);
  crossingTimer = setTimeout(() => {
    crossing.classList.remove('crossing');
    for (const animal of ['cat', 'dog']) document.querySelector('.' + animal).src = 'animals/' + animal + '-frame-0.png';
    revealButton.disabled = false;
    revealButton.textContent = 'Watch it again ↻';
  }, reducedMotion.matches ? 0 : 6222);
});
// CSS trees preserve the reveal if WebGL or module loading is unavailable.
import('../vendor/three.module.min.js').then(THREE => {
  const canvas = document.querySelector('#reveal-forest');
  const renderer = new THREE.WebGLRenderer({ canvas, alpha:true, antialias:true, powerPreference:'low-power' });
  renderer.setPixelRatio(Math.min(devicePixelRatio, 1.5));
  const scene = new THREE.Scene();
  const camera = new THREE.OrthographicCamera(-5,5,3.5,-3.5,.1,50);
  camera.position.set(0,0,15);
  scene.add(new THREE.AmbientLight(0xe3f1dc,2.4));
  const light = new THREE.DirectionalLight(0xffda9b,3.5);
  light.position.set(-3,6,9); scene.add(light);
  const halves = [-1,1].map(side => {
    const group = new THREE.Group();
    for (let i=0;i<3;i++) {
      const tree = new THREE.Group();
      tree.position.set(side*(.55+i*1.35),-2.4,-i*.4);
      tree.scale.setScalar(1-i*.08);
      const trunk = new THREE.Mesh(new THREE.CylinderGeometry(.14,.2,1,7),new THREE.MeshStandardMaterial({color:0x68482f}));
      trunk.position.y=.2; tree.add(trunk);
      for (let tier=0;tier<4;tier++) {
        const cone = new THREE.Mesh(new THREE.ConeGeometry(1.35-tier*.26,1.8,9),new THREE.MeshStandardMaterial({color:tier%2?0x2e6449:0x204e3b,roughness:.9}));
        cone.position.y=1+tier*.85; tree.add(cone);
      }
      for (let j=0;j<26;j++) {
        const y=.6+(j/26)*3.5, angle=j*2.4, radius=(4.5-y)*.31;
        const bulb=new THREE.Mesh(new THREE.SphereGeometry(.055,6,5),new THREE.MeshBasicMaterial({color:j%4?0xffd28b:0xb74939}));
        bulb.position.set(Math.cos(angle)*radius,y,Math.sin(angle)*radius); tree.add(bulb);
      }
      const star=new THREE.Mesh(new THREE.OctahedronGeometry(.18),new THREE.MeshBasicMaterial({color:0xffd28b}));
      star.position.y=4.5; tree.add(star); group.add(tree);
    }
    scene.add(group); return {group,side};
  });
  // Shared five-point star geometry; each particle fades independently.
  const starShape = new THREE.Shape();
  for (let point=0; point<10; point++) {
    const angle=Math.PI/2+point*Math.PI/5;
    const radius=point%2 ? .43 : 1;
    const x=Math.cos(angle)*radius, y=Math.sin(angle)*radius;
    if (point===0) starShape.moveTo(x,y); else starShape.lineTo(x,y);
  }
  starShape.closePath();
  const starGeometry = new THREE.ShapeGeometry(starShape);
  const sparkles = Array.from({length:64}, (_,i) => {
    const material=new THREE.MeshBasicMaterial({
      color:i%3 ? 0xffd28b : 0xfff5d6, transparent:true, opacity:0,
      depthTest:false, depthWrite:false, side:THREE.DoubleSide
    });
    const mesh=new THREE.Mesh(starGeometry,material);
    mesh.renderOrder=10;
    mesh.visible=false;
    scene.add(mesh);
    return {mesh, angle:Math.random()*Math.PI*2,
      speed:1.1+Math.random()*2.1, size:.045+Math.random()*.075,
      delay:.85+Math.random()*.65, life:1.3+Math.random()*1.6,
      spin:(Math.random()-.5)*5};
  });
  function updateSparkles(elapsed) {
    for (const p of sparkles) {
      const age=elapsed-p.delay, fraction=age/p.life;
      p.mesh.visible=!reducedMotion.matches && age>0 && fraction<1;
      if (!p.mesh.visible) continue;
      const distance=.25+p.speed*age;
      p.mesh.position.set(Math.cos(p.angle)*distance,
        Math.sin(p.angle)*distance-.4*age*age,5);
      p.mesh.rotation.z=p.angle+p.spin*age;
      p.mesh.scale.setScalar(p.size*(1-.5*fraction));
      p.mesh.material.opacity=Math.min(age*7,1)*Math.pow(1-fraction,1.4);
    }
  }
  let progress=0, frame=null, started=0;
  function paint() {
    const distance=camera.right*.95;
    for (const {group,side} of halves) { group.position.x=side*distance*progress; group.rotation.z=-side*.035*progress; }
    renderer.render(scene,camera);
  }
  function resize() {
    const {width,height}=canvas.getBoundingClientRect();
    renderer.setSize(width,height,false);
    camera.left=-3.5*width/height; camera.right=3.5*width/height;
    camera.updateProjectionMatrix(); paint();
  }
  function animate(now) {
    const elapsed=(now-started)/1000;
    const t=reducedMotion.matches?1:Math.min(elapsed/2.8,1);
    updateSparkles(elapsed);
    progress=t<.5?4*t*t*t:1-Math.pow(-2*t+2,3)/2;
    paint(); frame=!reducedMotion.matches && elapsed<4.8?requestAnimationFrame(animate):null;
  }
  forest={reveal() { if(frame!==null)cancelAnimationFrame(frame); progress=0; started=performance.now(); animate(started); }};
  new ResizeObserver(resize).observe(canvas);
  resize(); revealCard.classList.add('webgl-ready');
  if(revealCard.classList.contains('opening')) { progress=1; paint(); }
}).catch(() => {});
