---
tags:
  - float
---

# `SpawnCommandInFloatingPane`

{{since('nightly')}}

Spawn a new floating pane into the current tab. A tab can have one floating pane.
By default, a floating pane occupies the entire area of the tab, obscuring any
tiled panes or zoomed panes. Its dimensions can be configured so that the panes
beneath it remain visible, but they are not interactive until the floating pane
is hidden or closed.

The rendering priority for a tab is:

1. Tab Overlay (e.g. Launcher, InputSelector)
2. Floating Pane
3. Zoomed Pane
4. Tiled Panes

The argument accepts all fields from [SpawnCommand](../SpawnCommand.md), plus
these additional options:

* `replace_current` - If `true`, and a floating pane already exists in the tab,
  the existing floating pane will be closed and replaced with the new one.
  If `false` (the default), spawning a floating pane when one already exists
  will do nothing.
* `action` - (Optional) An event callback registered via
  `wezterm.action_callback`. The callback is invoked after the spawned floating
  pane closes, with the signature `(window, pane)`. `pane` is the pane that was
  active when the floating pane was spawned.
* `dimensions` - optional [OverlayDimensions](../OverlayDimensions.md) controlling
  the size of the floating pane. Defaults to the full tab.
* `border` - optional boolean that draws a single renderer-owned border around a
  bounded floating pane. Defaults to `false`.
* `border_color` - optional `ColorSpec` such as `{ AnsiColor = 'Blue' }` or
  `{ Color = '#7aa2f7' }`. Defaults to the floating pane foreground color.

The callback runs when the floating pane is removed. If your `exit_behavior`
keeps exited processes visible, the callback will run after you close the pane.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

config.keys = {
  -- Start `top` in a floating pane
  {
    key = 'f',
    mods = 'LEADER',
    action = act.SpawnCommandInFloatingPane {
      args = { 'top' },
    },
  },
  -- Start `htop` in a floating pane, replacing any existing floating pane
  {
    key = 'F',
    mods = 'LEADER',
    action = act.SpawnCommandInFloatingPane {
      args = { 'htop' },
      replace_current = true,
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

## Edit Text With Neovim

This example writes initial text to a temporary file, opens that file in
Neovim in a floating pane, and reads the file immediately after the floating
pane closes:

```lua
local wezterm = require 'wezterm'
local act = wezterm.action
local io = require 'io'
local os = require 'os'

config.keys = {
  {
    key = 'e',
    mods = 'LEADER',
    action = wezterm.action_callback(function(window, pane)
      local name = os.tmpname()
      local f = io.open(name, 'w+')
      f:write 'Initial text\n'
      f:flush()
      f:close()

      window:perform_action(
        act.SpawnCommandInFloatingPane {
          args = { 'nvim', name },
          action = wezterm.action_callback(function(_, source_pane)
            local edited = io.open(name, 'r')
            if not edited then
              wezterm.log_error('failed to read ' .. name)
              os.remove(name)
              return
            end

            local text = edited:read '*a'
            edited:close()
            os.remove(name)

            if text and text ~= '' then
              source_pane:send_text(text)
            end
          end),
        },
        pane
      )
    end),
  },
}
```
