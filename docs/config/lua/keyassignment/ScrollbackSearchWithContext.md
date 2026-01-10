# `ScrollbackSearchWithContext`

{{since('nightly')}}

Activates an alternative search overlay that provides a grep-like interface for
searching the scrollback buffer. Unlike the standard search overlay, this view
lists all matches with configurable surrounding context lines, similar to `grep -C`.

This action accepts no arguments.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

return {
  keys = {
    {
      key = 'f',
      mods = 'CMD',
      action = act.ScrollbackSearchWithContext,
    },
  },
}
```

## Search Modes

The overlay supports three search modes:
* **Case-Sensitive**: Matches exact case.
* **Case-Insensitive**: Matches text ignoring case.
* **Regex**: Treats the pattern as a regular expression.

## View Modes

* **List**: Shows matches with full context lines in a card-like layout.
* **Compact**: Shows matches in a denser list, with context lines hidden.

## Key Assignments

The overlay operates in two modes: **Input Mode** (for typing the search query)
and **Navigation Mode** (for browsing results).

### Input Mode

| Key | Action |
|-----|--------|
| <kbd>Enter</kbd> | Switch to Navigation Mode |
| <kbd>Escape</kbd> | Cancel/Close the overlay |
| <kbd>Ctrl</kbd>+<kbd>R</kbd> | Cycle Search Mode (Sensitive -> Insensitive -> Regex) |
| <kbd>UpArrow</kbd> | Previous search history |
| <kbd>DownArrow</kbd> | Next search history |

### Navigation Mode

| Key | Action |
|-----|--------|
| <kbd>/</kbd> | Switch back to Input Mode |
| <kbd>q</kbd> | Cancel/Close the overlay |
| <kbd>Enter</kbd> | Yank (copy) the selected match line to clipboard and close |
| <kbd>j</kbd> / <kbd>DownArrow</kbd> | Move selection down |
| <kbd>k</kbd> / <kbd>UpArrow</kbd> | Move selection up |
| <kbd>n</kbd> | Next page/wrap |
| <kbd>N</kbd> | Previous page/wrap |
| <kbd>g</kbd> | Go to first match |
| <kbd>G</kbd> | Go to last match |
| <kbd>y</kbd> | Yank (copy) the selected match line to clipboard |
| <kbd>Y</kbd> | Yank (copy) the selected match with context lines |
| <kbd>v</kbd> | Toggle View Mode (List/Compact) |
| <kbd>c</kbd> | Set number of context lines (requires count prefix, e.g. `5c`) |
| <kbd>Tab</kbd> | Increase context lines |
| <kbd>Shift</kbd>+<kbd>Tab</kbd> | Decrease context lines |
| <kbd>Ctrl</kbd>+<kbd>R</kbd> | Cycle Search Mode |

Most navigation keys accept a numeric prefix (count). For example, `5j` moves down 5 matches.
