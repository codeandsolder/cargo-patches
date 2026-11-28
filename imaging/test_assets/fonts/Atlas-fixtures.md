# Synthetic glyph-atlas regression fixtures

These small test fonts are original geometric fixtures, provided under the same Apache-2.0 OR MIT terms as Imaging. They contain no third-party font outlines or images. Glyph ID 1 is mapped to U+E000 in each.

- AtlasCOLR: 1024 units/em, two adjacent solid red/green COLRv0 rectangles, height 768 units.
- AtlasPNG: SBIX 16 ppem RGBA PNG strike, red left half and green right half.
- AtlasMask: EBDT/EBLC 4 ppem, row-aligned 4-bit coverage strike with each row [15,8,0,15].

- AtlasBGRA: CBDT/CBLC 4 ppem, raw premultiplied BGRA, opaque red left half and half-alpha green right half.

- AtlasVariable: synthetic weight axis (400 to 900), glyph 1 ink width 512 to 896 units and corresponding variable advance.
