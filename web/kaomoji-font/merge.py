import glob, os, sys
from fontTools.merge import Merger, Options
from fontTools.ttLib import TTFont
fs=sorted(glob.glob(sys.argv[1]+'/*.ttf')); out=sys.argv[2]
f=Merger(options=Options(drop_tables=['vhea','vmtx','VORG','DSIG','meta','STAT','MATH','FFTM'])).merge(fs)
n=f['name']; n.names=[]
for i,v in [(0,'Glyphs from Noto Sans (Copyright The Noto Project Authors) and DejaVu Sans (Copyright Bitstream, DejaVu changes are in public domain); SIL Open Font License 1.1 / Bitstream Vera license.'),
            (1,'slashcheer kaomoji'),(2,'Regular'),(3,'slashcheer-kaomoji'),(4,'slashcheer kaomoji'),(5,'Version 1.0'),(6,'slashcheer-kaomoji'),
            (13,'SIL Open Font License 1.1 (Noto) and Bitstream Vera Fonts license (DejaVu)')]:
    n.setName(v,i,3,1,0x409)
f.save(out); print(len(fs),'files ->',round(os.path.getsize(out)/1024,1),'KB', len(f.getGlyphOrder()),'glyphs')
