---
tags:
  - overlay
---

# `InputText`

{{since('nightly')}}

The `InputText` key assignment presents a Vim-like multi-line text
editor overlay.
This is useful for editing commit messages, configuration snippets,
quick notes, or any text that benefits from modal editing with full
Vim keybindings.

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

config.keys = {
  {
    key = 'e',
    mods = 'LEADER',
    action = act.InputText {
      title = 'Edit and Send to Terminal',
      action = wezterm.action_callback(function(window, pane, result)
        -- result is nil if cancelled with ZQ or Ctrl-C
        if result and result.text ~= '' then
          pane:send_text(result.text)
        end
      end),
    },
  },
}
```

The `InputText` struct takes the following fields:

* `title` - (Optional) The title displayed at the top of the editor.
* `initial_value` - (Optional) The starting content of the editor.
* `action` - an event callback registered via `wezterm.action_callback`. The
  callback's function signature is `(window, pane, result)` where `window` and
  `pane` are the [Window](../window/index.md) and [Pane](../pane/index.md)
  objects from the current pane and window, and `result` is a table containing
  the `text` field with the full text content as a string (lines joined with `\n`).
  `result` will be `nil` if the user cancels with `ZQ` or `Ctrl-C`.

## Modes

The editor supports Vim-style modal editing:

| Mode | Description | Enter | Exit |
|------|-------------|-------|------|
| **Normal** | Navigation and commands | Default, `Escape` | - |
| **Insert** | Text insertion | `i`, `a`, `o`, `O`, `I`, `A` | `Escape` |
| **Replace** | Overwrite characters | `R` | `Escape` |
| **Visual** | Character selection | `v` | `Escape`, `d`, `c`, `y` |
| **Visual Line** | Line selection | `V` | `Escape`, `d`, `c`, `y` |
| **Visual Block** | Column selection | `Ctrl-V` | `Escape`, `d`, `c`, `y` |
| **Search** | Incremental search | `/`, `?` | `Enter`, `Escape` |

## Key Assignments

### Submitting and Canceling

| Key | Action |
|-----|--------|
| `ZZ` | Submit the text and trigger callback with `result` (Vim-style :wq) |
| `ZQ` | Cancel and trigger callback with `nil` (Vim-style :q!) |
| `Ctrl-C` | Cancel and trigger callback with `nil` |
| `Escape` | Exit current mode (in Normal mode, does nothing) |

### Navigation (Normal/Visual modes)

| Key | Action |
|-----|--------|
| `h`, `j`, `k`, `l` | Move left, down, up, right |
| `w`, `W` | Move to next word/WORD start |
| `b`, `B` | Move to previous word/WORD start |
| `e`, `E` | Move to next word/WORD end |
| `ge`, `gE` | Move to previous word/WORD end |
| `0` | Move to start of line |
| `^` | Move to first non-blank character |
| `$` | Move to end of line |
| `gg` | Move to first line |
| `G` | Move to last line |
| `{count}G` | Move to line {count} |
| `H` | Move to top of screen |
| `M` | Move to middle of screen |
| `L` | Move to bottom of screen |
| `{`, `}` | Move to previous/next paragraph |
| `(`, `)` | Move to previous/next sentence |
| `f{char}` | Find character forward (inclusive) |
| `F{char}` | Find character backward (inclusive) |
| `t{char}` | Find character forward (exclusive) |
| `T{char}` | Find character backward (exclusive) |
| `;` | Repeat last character search |
| `,` | Repeat last character search (reverse) |
| `%` | Jump to matching bracket |
| `Ctrl-D` | Scroll half page down |
| `Ctrl-U` | Scroll half page up |
| `Ctrl-E` | Scroll one line down |
| `Ctrl-Y` | Scroll one line up |

### Editing (Normal mode)

| Key | Action |
|-----|--------|
| `i` | Insert before cursor |
| `a` | Insert after cursor |
| `I` | Insert at first non-blank of line |
| `A` | Insert at end of line |
| `o` | Open new line below |
| `O` | Open new line above |
| `R` | Enter Replace mode |
| `x` | Delete character at cursor |
| `X` | Delete character before cursor |
| `s` | Substitute character (delete + insert) |
| `S` | Substitute line (delete line + insert) |
| `r{char}` | Replace character with {char} |
| `J` | Join current line with next |
| `~` | Toggle case of character |
| `Ctrl-A` | Increment number under cursor |
| `Ctrl-X` | Decrement number under cursor |
| `u` | Undo |
| `Ctrl-R` | Redo |
| `.` | Repeat last change |

### Operators (Normal mode)

Operators can be combined with motions or text objects:

| Operator | Action |
|----------|--------|
| `d{motion}` | Delete |
| `c{motion}` | Change (delete + insert) |
| `y{motion}` | Yank (copy) |

Examples:
* `dw` - Delete to next word
* `ci(` - Change inside parentheses
* `yy` - Yank current line
* `d$` - Delete to end of line

### Text Objects

Used with operators (`d`, `c`, `y`) or in Visual mode:

| Text Object | Description |
|-------------|-------------|
| `iw`, `aw` | Inner/a word |
| `iW`, `aW` | Inner/a WORD (whitespace-delimited) |
| `i(`, `a(` | Inner/around parentheses |
| `i[`, `a[` | Inner/around brackets |
| `i{`, `a{` | Inner/around braces |
| `i<`, `a<` | Inner/around angle brackets |
| `i"`, `a"` | Inner/around double quotes |
| `i'`, `a'` | Inner/around single quotes |
| `` i` ``, `` a` `` | Inner/around backticks |
| `ip`, `ap` | Inner/a paragraph |
| `is`, `as` | Inner/a sentence |

### Line Operations

| Key | Action |
|-----|--------|
| `dd` | Delete line |
| `cc` | Change line |
| `yy` | Yank line |
| `D` | Delete to end of line |
| `C` | Change to end of line |
| `Y` | Yank to end of line |

### Paste

| Key | Action |
|-----|--------|
| `p` | Paste after cursor |
| `P` | Paste before cursor |

### Search

| Key | Action |
|-----|--------|
| `/` | Search forward |
| `?` | Search backward |
| `n` | Next match |
| `N` | Previous match |
| `*` | Search word under cursor (forward) |
| `#` | Search word under cursor (backward) |

### Visual Mode Operations

| Key | Action |
|-----|--------|
| `d`, `x` | Delete selection |
| `c`, `s` | Change selection (delete + insert) |
| `y` | Yank selection |
| `v` | Toggle/switch to character-wise |
| `V` | Toggle/switch to line-wise |
| `Ctrl-V` | Toggle/switch to block-wise |
| `I` | Insert at block start (block mode) |
| `A` | Append at block end (block mode) |

### Insert Mode

| Key | Action |
|-----|--------|
| `Escape` | Return to Normal mode |
| `Backspace` | Delete character before cursor |
| Arrow keys | Move cursor |
| Any character | Insert at cursor |

## Examples

### Compose and Execute Shell Commands

Use the editor to compose multi-line shell commands, then execute them:

```lua
local wezterm = require 'wezterm'
local act = wezterm.action

config.keys = {
  {
    key = 'e',
    mods = 'LEADER',
    action = act.InputText {
      title = 'Compose Command',
      action = wezterm.action_callback(function(window, pane, result)
        if result and result.text ~= '' then
          -- Send each line as a separate command
          for line in result.text:gmatch '[^\n]+' do
            pane:send_text(line .. '\n')
          end
        end
      end),
    },
  },
}
```

### Quick Scratch Pad

Edit text and copy it to the clipboard:

```lua
config.keys = {
  {
    key = 's',
    mods = 'LEADER',
    action = act.InputText {
      title = 'Scratch Pad',
      action = wezterm.action_callback(function(window, pane, result)
        if result and result.text ~= '' then
          window:copy_to_clipboard(result.text)
          window:toast_notification(
            'Copied',
            'Text copied to clipboard',
            nil,
            3000
          )
        end
      end),
    },
  },
}
```

### Edit with Initial Content from Clipboard

Pre-populate the editor with clipboard content for editing:

```lua
config.keys = {
  {
    key = 'v',
    mods = 'LEADER',
    action = wezterm.action_callback(function(window, pane)
      window:perform_action(
        act.InputText {
          title = 'Edit Clipboard',
          initial_value = window:get_clipboard(),
          action = wezterm.action_callback(function(win, p, result)
            if result and result.text ~= '' then
              win:copy_to_clipboard(result.text)
            end
          end),
        },
        pane
      )
    end),
  },
}
```
