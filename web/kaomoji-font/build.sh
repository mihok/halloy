#!/usr/bin/env bash
# Rebuild fonts/slashcheer-kaomoji.ttf: the web build's fallback for kaomoji
# characters Iosevka lacks (kana, halfwidth forms, Arabic, Kannada, Thai,
# Tibetan brackets, Canadian syllabics, Armenian, symbol blocks). Browsers
# have no system fonts for iced to fall back on, so these must be bundled.
#
# Sources: Noto Sans fonts from npm (@fontsource, SIL OFL 1.1) and DejaVu
# Sans (dejavu-fonts-ttf, Bitstream Vera license), subset to the blocks in
# subset.py and merged into one font. Needs python3 (fonttools, brotli).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
cd "$work"
python3 -m venv venv && venv/bin/pip -q install fonttools brotli
for f in noto-sans noto-sans-arabic noto-sans-kannada noto-sans-thai noto-sans-canadian-aboriginal \
	noto-sans-armenian noto-sans-symbols-2 noto-sans-math noto-sans-jp noto-sans-symbols noto-serif-tibetan; do
	npm pack -q "@fontsource/$f" >/dev/null
done
npm pack -q dejavu-fonts-ttf >/dev/null
mkdir -p x
for t in fontsource-*.tgz; do mkdir -p "x/${t%.tgz}" && tar -xzf "$t" -C "x/${t%.tgz}"; done
mv x/fontsource-noto-serif-tibetan-* x/tib
mkdir -p x/dejavu && tar -xzf dejavu-fonts-ttf-*.tgz -C x/dejavu
venv/bin/python -c "
from fontTools.ttLib import TTFont
from fontTools.ttLib.scaleUpem import scale_upem
t = TTFont('x/dejavu/package/ttf/DejaVuSans.ttf'); scale_upem(t, 1000); t.save('x/dejavu/package/dejavu-1000.woff2.ttf')"
venv/bin/python "$here/subset.py" "$work" "$work/fb"
venv/bin/python "$here/merge.py" "$work/fb" "$here/../../fonts/slashcheer-kaomoji.ttf"
venv/bin/python "$here/coverage.py" "$here/../../fonts/slashcheer-kaomoji.ttf" >/dev/null
rm -rf "$work"
