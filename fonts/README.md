# fonts

`texgyreheros-regular.otf` is **TeX Gyre Heros Regular, version 2.004** (30 October 2009), by Bogusław Jackowski
and Janusz M. Nowacki for the TeX Users Groups, based on URW Nimbus Sans L. It is distributed under the
**GUST Font License** (`GUST-FONT-LICENSE.txt`), an instance of the LaTeX Project Public License 1.3c.
`README-TeX-Gyre-Heros.txt` and `MANIFEST-TeX-Gyre-Heros.txt` are the package's own readme and manifest.

## What this folder is, against the original

This is part of the TeX Gyre Heros package, not all of it. The file `texgyreheros-regular.otf` is byte-for-byte
the one in GUST's `qhv2.004otf.zip` and in CTAN's `fonts/tex-gyre` (SHA-256
`6ae1a09d5a940367b7aaaa91ee8bd8a2c333bfe193e7096e23f931357d62081f`). The only change is that the other files
the manifest lists (the other weights and styles, the condensed family, the Type 1, metric and TeX support files)
are left out. The font has not been modified or renamed.

The complete, unmodified package is available from GUST, <https://www.gust.org.pl/projects/e-foundry/tex-gyre/heros>,
and from CTAN, <https://ctan.org/pkg/tex-gyre-heros>.

The authors of TeX Gyre Heros do not support this repository or its use of the font. Report problems here, not to them.

## How CrystalOSD uses it

`crates/glyph_enhance` embeds the file and, with `--text sharp`, draws the outlines of its glyphs at run time
in place of the console font's Latin glyphs, placed where the console placed them. The placements are listed in
`crates/glyph_enhance/data/heros-fit.txt` and made by `crystalosd-heros-fit`. No modified font file is made or
distributed. The binary embeds the font, so a binary distribution should carry this folder with it.
