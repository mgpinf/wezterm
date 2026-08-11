---
tags:
  - transient
---

# `TransientMenu`

{{since('nightly')}}

This creates an overlay with keyboard driven menu similar to
Emacs transient menus. 

We can view and set switches, options, and cyclic switches, then trigger an
action with the current state of those entries passed to its callback.

Entry keys can contain multiple characters. While an incomplete key prefix is
active, entries that cannot be reached from that prefix are dimmed. For matching
entries, the consumed portion of the displayed key is dimmed while the remaining
portion retains the normal key color. The
`transient_entry_non_matching_fg` [color setting](../../appearance.md#defining-your-own-colors)
controls the dimmed foreground.

`TransientMenu` accepts the following fields:

* `description` - text to display at the top of the menu
* `title` - optional, the title that will be set for the overlay pane
* `context` - optional, accepts a [TransientContext](../TransientContext.md) object
* `cancel` - optional event callback registered via `wezterm.action_callback`. The
  callback's function signature is `(window, pane)` where `window` and
  `pane` are the [Window](../window/index.md) and [Pane](../pane/index.md).
  Called when the user cancels the overlay
* `dimensions` - optional [OverlayDimensions](../OverlayDimensions.md) controlling
  the size of the overlay. Defaults to the full tab. {{since('nightly', inline=True)}}
* `border` - optional boolean that draws a single renderer-owned border around a
  bounded overlay. Defaults to `false`. {{since('nightly', inline=True)}}
* `border_color` - optional `ColorSpec` such as `{ AnsiColor = 'Blue' }` or
  `{ Color = '#7aa2f7' }`. Defaults to the overlay foreground color.
  {{since('nightly', inline=True)}}

For specifying menu entries, use **one** of the following:

* `entries` - list of [TransientEntry](#transiententry) objects for a
  single-section menu
* `header` - optional section header text; only valid with `entries`
* `sections` - list of [TransientSection](#transientsection) objects for a
  multi-section menu

Note: You cannot combine `sections` with `entries` or `header`.

## Basic example

This complete configuration opens a menu with a switch, an option, and an
action. The action callback receives a table that maps each entry's `flag` to
its current value.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

local config = wezterm.config_builder()

config.keys = {
  {
    key = 'm',
    mods = 'CTRL|SHIFT',
    action = act.TransientMenu {
      description = 'Example menu',
      header = 'Arguments',
      entries = {
        {
          type = 'switch',
          key = 'v',
          description = 'Verbose output',
          flag = '--verbose',
        },
        {
          type = 'option',
          key = 'f',
          description = 'Output format',
          flag = '--format=',
          default = 'text',
          choices = { 'text', 'json' },
        },
        {
          type = 'action',
          key = 'r',
          description = 'Run',
          action = wezterm.action_callback(function(window, pane, result)
            wezterm.log_info('verbose: ' .. tostring(result['--verbose']))
            wezterm.log_info('format: ' .. tostring(result['--format=']))
          end),
        },
      },
    },
  },
}

return config
```

## Sections

Use `sections` to organize entries under multiple headers:

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

act.TransientMenu {
  description = 'Build',
  sections = {
    {
      header = 'Arguments',
      entries = {
        {
          type = 'switch',
          key = 'r',
          description = 'Release mode',
          flag = '--release',
        },
      },
    },
    {
      header = 'Actions',
      entries = {
        {
          type = 'action',
          key = 'b',
          description = 'Build',
          action = wezterm.action_callback(function(window, pane, result)
            wezterm.log_info('release: ' .. tostring(result['--release']))
          end),
        },
      },
    },
  },
}
```

### `TransientSection`

`TransientSection` struct is a lua object with the following fields:

* `header` - text to describe the section
* `entries` - list of [TransientEntry](#transiententry) objects

### `TransientEntry`

`TransientEntry` is a lua object with a `type` field to specify the entry type,
with all other fields at the same level.

The `type` field accepts:

* `"switch"` - boolean toggle, see [TransientSwitch](../TransientSwitch.md)
* `"option"` - value input, see [TransientOption](../TransientOption.md)
* `"cyclic"` - cycle through choices, see [TransientCyclicSwitch](../TransientCyclicSwitch.md)
* `"action"` - trigger an action, see [TransientAction](../TransientAction.md)

## Cyclic switch

A cyclic switch advances to the next choice each time its key is pressed. If
`allow_nil` is `true`, advancing past the last choice unsets the switch.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

act.TransientMenu {
  description = 'Select an ordering',
  entries = {
    {
      type = 'cyclic',
      key = 'o',
      description = 'Order',
      flag = '--order=',
      choices = { 'topological', 'date', 'author-date' },
      allow_nil = true,
    },
    {
      type = 'action',
      key = 'a',
      description = 'Apply',
      action = wezterm.action_callback(function(window, pane, result)
        wezterm.log_info('order: ' .. tostring(result['--order=']))
      end),
    },
  },
}
```
