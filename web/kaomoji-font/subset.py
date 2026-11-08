import glob, os, sys, io
from fontTools.ttLib import TTFont
from fontTools import subset
N=sys.argv[1]; OUT=sys.argv[2]
ios=TTFont('/home/user/halloy/fonts/iosevka-term-regular.ttf').getBestCmap()
R=[(0x02B0,0x036F),(0x0530,0x058F),(0x0600,0x06FF),(0x0C80,0x0CFF),(0x0E00,0x0E7F),(0x0F00,0x0F3F),
   (0x1400,0x167F),(0x1D00,0x1DBF),(0x2000,0x206F),(0x2190,0x21FF),(0x2200,0x22FF),(0x2300,0x23FF),
   (0x25A0,0x25FF),(0x2600,0x26FF),(0x2700,0x27BF),(0x2B00,0x2BFF),
   (0x3000,0x30FF),(0x31F0,0x31FF),(0xFE30,0xFE4F),(0xFE70,0xFEFF),(0xFF00,0xFFEF)]
want=[c for a,b in R for c in range(a,b+1) if c not in ios]
pri=['noto-sans-5','noto-sans-math','noto-sans-symbols-2','noto-sans-symbols-5','noto-sans-arabic','noto-sans-armenian','noto-sans-kannada','noto-sans-thai','tib','noto-sans-canadian','noto-sans-jp']
files=[]
for p in pri:
    files+=sorted(glob.glob(f'{N}/x/*{p}*/package/files/*-400-normal.woff2'))
files.append(f'{N}/x/dejavu/package/dejavu-1000.woff2.ttf')
cmaps={f:TTFont(f).getBestCmap() for f in files}
assign={}
for c in want:
    for f in files:
        if c in cmaps[f]: assign.setdefault(f,[]).append(c); break
os.makedirs(OUT,exist_ok=True)
total=0; missing=[c for c in want if not any(c in v for v in assign.values())]
for i,(f,cs) in enumerate(assign.items()):
    font=TTFont(f); font.flavor=None
    o=subset.Options(); o.layout_features=['*']; o.name_IDs=['*']; o.notdef_outline=True; o.hinting=False
    s=subset.Subsetter(o); s.populate(unicodes=cs); s.subset(font)
    name=os.path.basename(f).replace('-400-normal.woff2','').replace('.woff2.ttf','')
    path=f'{OUT}/{i:02d}-{name}.ttf'; font.save(path); sz=os.path.getsize(path); total+=sz
    print(f'{name:55s} {len(cs):4d} chars {sz/1024:7.1f} KB')
print('total', round(total/1024,1),'KB; not found:', len(missing))
