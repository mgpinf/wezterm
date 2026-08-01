---
tags:
  - overlay
---

# `OverlayDimensions`

{{since('nightly')}}

`OverlayDimensions` controls the size of a dialog-like overlay. It has the
following fields:

* `width` - the overlay width, specified as `{ Cells = n }` or
  `{ Percent = n }`.
* `height` - the overlay height, specified as `{ Cells = n }` or
  `{ Percent = n }`.

Both fields default to `{ Percent = 100 }`, which preserves the traditional
full-tab overlay. Values are clamped to the available tab size, and a bounded
overlay is centered in the tab. The panes beneath a bounded overlay remain
visible but are not interactive until the overlay closes.

For example:

```lua
dimensions = {
  width = { Percent = 70 },
  height = { Cells = 20 },
}
```
