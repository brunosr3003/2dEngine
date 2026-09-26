"""Monta os efeitos de skills a partir de gravações CC0 já baixadas.
Uso: python3 tools/audio/preparar_skills.py /tmp/tempest-combat-src
Fontes e licenças: assets/audio/skills/README.md.
"""
from pathlib import Path
import sys, subprocess, array, math
src=Path(sys.argv[1]);root=Path(__file__).resolve().parents[2];out=root/'assets/audio/skills';out.mkdir(parents=True,exist_ok=True)
# Fonte, atraso, ganho. As camadas são gravações prontas, não osciladores.
plans={
1:('Investida',[('sword.1.ogg',0,1)],[('sword_clash.1.ogg',0,1)]),
2:('Golpe Largo',[('sword.2.ogg',0,1)],[('sword_clash.3.ogg',0,1),('blade_02.ogg',.035,.6)]),
3:('Muralha',[('metal_01.ogg',0,1)],[('sword_clash.7.ogg',0,1),('spell_01.ogg',.06,.3)]),
4:('Saque',[('blade_03.ogg',0,1)],[('sword_clash.2.ogg',0,1)]),
5:('Dança',[('sword.4.ogg',0,1),('sword.6.ogg',.15,.8),('sword.4.ogg',.3,.6)],[('blade_01.ogg',0,1),('blade_02.ogg',.12,.8),('blade_03.ogg',.24,.7)]),
6:('Vento Cortante',[('sword.5.ogg',0,1),('spell_02.ogg',.08,.3)],[('sword_clash.5.ogg',0,.7),('spell_fire_07.ogg',0,.6)]),
7:('Tiro Certeiro',[('metal_02.ogg',0,.5)],[('pistola.ogg',0,1),('sword_clash.10.ogg',.045,.15)]),
8:('Rajada',[('metal_03.ogg',0,.5)],[('pistola.ogg',0,1),('pistola.ogg',.1,.85),('pistola.ogg',.2,.7)]),
9:('Barril',[('wood_04.ogg',0,.8),('sword.6.ogg',0,.25)],[('spell_fire_03.ogg',0,1),('pistola.ogg',0,.45)]),
10:('Bênção',[('magical_1.ogg',0,.8)],[('magical_3.ogg',0,1)]),
11:('Aura',[('spell_01.ogg',0,1),('magical_3.ogg',.05,.4)],[('magical_1.ogg',0,1),('magical_3.ogg',.2,.6)]),
12:('Julgamento',[('spell_fire_05.ogg',0,.7),('spell_02.ogg',.05,.5)],[('spell_fire_04.ogg',0,1),('sword_clash.6.ogg',0,.4)]),
}
rate=44100
for id,(name,cast,hit) in plans.items():
 for phase,layers in [('cast',cast),('impact',hit)]:
  length=1.2 if phase=='impact' else .65
  mix=[0.0]*int(rate*length)
  for filename,delay,gain in layers:
   samples=array.array('f',subprocess.check_output(['ffmpeg','-v','error','-i',str(src/filename),'-t',str(length),'-ac','1','-ar',str(rate),'-f','f32le','-']))
   peak=max(abs(v) for v in samples) or 1
   start=int(delay*rate)
   for j,v in enumerate(samples[:len(mix)-start]): mix[start+j]+=v/peak*gain
  # Retira silêncio inicial e cauda longa, com fade para não estalar.
  active=[i for i,v in enumerate(mix) if abs(v)>.002]
  if not active: raise ValueError(name)
  mix=mix[max(0,active[0]-220):min(len(mix),active[-1]+1323)]
  rms=math.sqrt(sum(v*v for v in mix)/len(mix));gain=min(.7/max(abs(v) for v in mix),.12/max(rms,.001))
  data=array.array('f',(v*gain*min(1.,(len(mix)-i)/2646) for i,v in enumerate(mix)))
  subprocess.run(['ffmpeg','-v','error','-y','-f','f32le','-ar',str(rate),'-ac','1','-i','pipe:0','-c:a','libvorbis','-q:a','5',str(out/f'{id:02}_{phase}.ogg')],input=data.tobytes(),check=True)
 print(id,name)
