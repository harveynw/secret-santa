"""Generate the code-native Christmas link-preview artwork; requires librsvg (rsvg-convert)."""
from pathlib import Path
import random
import subprocess

root = Path(__file__).resolve().parents[1]
rng = random.Random(2026)
snow = ''.join(f'<circle cx="{rng.randrange(1200)}" cy="{rng.randrange(630)}" r="{rng.choice([1,1,1.5,2])}" fill="#f6eedb" opacity="{rng.uniform(.12,.45):.2f}"/>' for _ in range(110))
trees = ''
for x, y, scale in [(-25,440,1.3),(95,535,1),(1110,535,1),(1225,440,1.3)]:
 trees += f'<g transform="translate({x} {y}) scale({scale})">'
 for tier in range(4):
  top = -260+tier*54
  width = 37+tier*25
  trees += f'<path d="M0 {top} L{-width} {top+125} H{width} Z" fill="{["#214937","#204633","#1b3f30","#183a2c"][tier]}"/>'
 for i in range(15):
  yy=-190+i*15
  xx=rng.uniform(-1,1)*(yy+280)*.32
  trees += f'<circle cx="{xx}" cy="{yy}" r="2.4" fill="#dbb878" opacity=".8"/>'
 trees += '</g>'
svg=f'''<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630" viewBox="0 0 1200 630">
<defs><radialGradient id="bg"><stop stop-color="#234c3c"/><stop offset="1" stop-color="#0b231b"/></radialGradient></defs>
<rect width="1200" height="630" fill="url(#bg)"/>{snow}{trees}
<rect x="25" y="25" width="1150" height="580" rx="4" fill="none" stroke="#dbb878" stroke-opacity=".4"/>
<path d="M600 74 L606 94 L626 100 L606 106 L600 126 L594 106 L574 100 L594 94 Z" fill="#dbb878"/>
<text x="600" y="175" text-anchor="middle" fill="#dbb878" font-family="Arial" font-size="15" letter-spacing="5">A LITTLE CHRISTMAS MAGIC</text>
<text x="600" y="290" text-anchor="middle" fill="#f6eedb" font-family="Georgia" font-size="76">You’re invited</text>
<text x="600" y="398" text-anchor="middle" fill="#dbb878" font-family="Georgia" font-style="italic" font-size="102">Secret Santa</text>
<path d="M452 448 H556 M644 448 H748" stroke="#dbb878" stroke-opacity=".55"/>
<path d="M600 439 L609 448 L600 457 L591 448 Z" fill="#dbb878"/>
<text x="600" y="505" text-anchor="middle" fill="#c8d2c2" font-family="Arial" font-size="21">Join the Christmas gift exchange.</text>
<text x="600" y="555" text-anchor="middle" fill="#dbb878" font-family="Arial" font-size="13" letter-spacing="4">CHRISTMAS GIFT EXCHANGE</text>
</svg>'''
p=root/'static/christmas-preview.svg'
p.write_text(svg)
subprocess.run(['rsvg-convert',str(p),'-o',str(root/'static/christmas-preview.png')],check=True)
