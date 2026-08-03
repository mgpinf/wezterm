# ``ShowDebugOverlay``

{{since('20210814-124438-54e29167')}}

Overlays the current tab with the debug overlay, which is a combination
of a debug log and a lua [REPL](https://en.wikipedia.org/wiki/Read%E2%80%93eval%E2%80%93print_loop).

The REPL has the following globals available:

* `wezterm` - the [wezterm](../wezterm/index.md) module is pre-imported
* `window` - the [window](../window/index.md) object for the current window

The lua context in the REPL is not connected to any global state; you cannot use it
to dynamically assign event handlers for example.  It is primarily useful for
prototyping lua snippets before you integrate them fully into your config.

`ShowDebugOverlay` accepts the following fields:

* `dimensions` - optional [OverlayDimensions](../OverlayDimensions.md) controlling
  the size of the overlay. Defaults to the full tab. {{since('nightly', inline=True)}}
* `border` - optional boolean that draws a single renderer-owned border around a
  bounded overlay. Defaults to `false`. {{since('nightly', inline=True)}}
* `border_color` - optional `ColorSpec` such as `{ AnsiColor = 'Blue' }` or
  `{ Color = '#7aa2f7' }`. Defaults to the overlay foreground color.
  {{since('nightly', inline=True)}}

```lua
config.keys = {
  -- CTRL-SHIFT-l activates a bordered debug overlay centered in the tab
  {
    key = 'L',
    mods = 'CTRL',
    action = wezterm.action.ShowDebugOverlay {
      dimensions = {
        width = { Percent = 70 },
        height = { Cells = 20 },
      },
      border = true,
      border_color = { AnsiColor = 'Blue' },
    },
  },
}
```
