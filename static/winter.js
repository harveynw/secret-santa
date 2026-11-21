import * as THREE from './vendor/three.module.min.js';
const toggle = document.querySelector('#motion');
try {
 const renderer = new THREE.WebGLRenderer({canvas:document.querySelector('#winter'),alpha:true,antialias:false,powerPreference:'low-power'});
 renderer.setPixelRatio(Math.min(devicePixelRatio,1.5));
 const scene=new THREE.Scene(), camera=new THREE.PerspectiveCamera(45,1,.1,80);
 camera.position.set(0,1,18);
 scene.add(new THREE.AmbientLight(0xe3f1dc,2));
 const light=new THREE.DirectionalLight(0xffda9b,3);light.position.set(-5,10,7);scene.add(light);
 const trees=new THREE.Group();scene.add(trees);
 for(let side of [-1,1]) for(let i=0;i<5;i++){
  const tree=new THREE.Group();tree.position.set(side*(5.7+i*2),-5,-i*2);tree.rotation.y=i;
  const scale=1.2+(i%3)*.3;tree.scale.setScalar(scale);
  for(let tier=0;tier<4;tier++){
   const cone=new THREE.Mesh(new THREE.ConeGeometry(1.65-tier*.33,2.2,7),new THREE.MeshStandardMaterial({color:tier%2?0x1e4837:0x163b2d,roughness:1}));cone.position.y=tier*1.05;tree.add(cone);
  }
  for(let j=0;j<22;j++){
   const y=.2+Math.random()*4.4,angle=j*2.4,r=(4.8-y)*.32;
   const bulb=new THREE.Mesh(new THREE.SphereGeometry(.045,5,4),new THREE.MeshBasicMaterial({color:j%3?0xe9bc6e:0xb74436}));bulb.position.set(Math.cos(angle)*r,y,Math.sin(angle)*r);tree.add(bulb);
  }
  trees.add(tree);
 }
 const n=innerWidth<600?150:360, positions=new Float32Array(n*3);
 for(let i=0;i<n;i++){positions[i*3]=(Math.random()-.5)*35;positions[i*3+1]=Math.random()*22-10;positions[i*3+2]=Math.random()*14-7;}
 const geo=new THREE.BufferGeometry();geo.setAttribute('position',new THREE.BufferAttribute(positions,3));
 const textureCanvas=document.createElement('canvas');textureCanvas.width=textureCanvas.height=32;const ctx=textureCanvas.getContext('2d');const gradient=ctx.createRadialGradient(16,16,0,16,16,16);gradient.addColorStop(0,'#ffffff');gradient.addColorStop(.3,'#ffffffaa');gradient.addColorStop(1,'#ffffff00');ctx.fillStyle=gradient;ctx.fillRect(0,0,32,32);
 scene.add(new THREE.Points(geo,new THREE.PointsMaterial({size:.12,map:new THREE.CanvasTexture(textureCanvas),transparent:true,opacity:.65,depthWrite:false})));
 let paused=matchMedia('(prefers-reduced-motion: reduce)').matches, frame, last=0;
 function resize(){renderer.setSize(innerWidth,innerHeight,false);camera.aspect=innerWidth/innerHeight;camera.updateProjectionMatrix();renderer.render(scene,camera)}
 function render(t){frame=null;const dt=Math.min((t-last)/1000,.05);last=t;if(!paused){for(let i=0;i<n;i++){positions[i*3+1]-=dt*(.35+(i%5)*.12);positions[i*3]+=Math.sin(t*.0002+i)*dt*.08;if(positions[i*3+1]<-11)positions[i*3+1]=11;}geo.attributes.position.needsUpdate=true;}renderer.render(scene,camera);if(!paused&&!document.hidden)frame=requestAnimationFrame(render);}
 function update(){toggle.textContent=paused?'Let it snow':'Pause snowfall';toggle.setAttribute('aria-pressed',String(paused));if(frame)cancelAnimationFrame(frame);frame=null;last=performance.now();render(last)}
 toggle.hidden=false;toggle.onclick=()=>{paused=!paused;update()};addEventListener('resize',resize);document.addEventListener('visibilitychange',()=>{if(document.hidden){cancelAnimationFrame(frame);frame=null;}else update()});matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change',e=>{paused=e.matches;update()});resize();update();
} catch { toggle.hidden=true; }
