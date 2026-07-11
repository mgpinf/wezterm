---
tags:
  - font
  - appearance
---
# `glyph_coverage_gamma = 1.0`

Adjusts the linear coverage of antialiased text glyphs before they are blended
with the terminal background.

The default value of `1.0` preserves the original glyph coverage. Values below
`1.0` increase partially covered edge pixels, making text appear heavier and
higher contrast. Values above `1.0` reduce edge coverage, making text appear
lighter.

```lua
-- Slightly increase the apparent weight of antialiased text.
config.glyph_coverage_gamma = 0.9
```

The accepted range is `0.1` through `10.0`. Values close to `1.0`, typically
between `0.8` and `1.2`, are recommended. This setting affects grayscale text
in both the OpenGL and WebGPU front ends and RGB coverage when OpenGL LCD
subpixel rendering is enabled. It does not affect emoji, background images,
cursors, or non-text drawing primitives.
