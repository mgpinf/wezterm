---
tags:
  - overlay
  - input_text
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
| **Command** | Ex commands (`:`) | `:` | `Enter`, `Escape` |

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
| `zz` | Scroll viewport to center cursor line on screen |
| `zt` | Scroll viewport to place cursor line at top of screen |
| `zb` | Scroll viewport to place cursor line at bottom of screen |

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
| `J` | Join current line with next (adds space) |
| `gJ` | Join current line with next (no space, preserves whitespace) |
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
| `>{motion}` | Indent |
| `<{motion}` | Dedent |
| `gu{motion}` | Lowercase text covered by motion |
| `gU{motion}` | Uppercase text covered by motion |
| `g~{motion}` | Toggle case of text covered by motion |

Examples:
* `dw` - Delete to next word
* `ci(` - Change inside parentheses
* `yy` - Yank current line
* `d$` - Delete to end of line
* `guw` - Lowercase next word
* `gUiw` - Uppercase inner word
* `g~$` - Toggle case to end of line
* `guu` - Lowercase entire line
* `gUU` - Uppercase entire line
* `g~~` - Toggle case of entire line
* `>ip` - Indent inner paragraph
* `<i{` - Dedent inside braces

### Text Objects

Used with operators (`d`, `c`, `y`, `>`, `<`, `gu`, `gU`, `g~`) or in Visual mode:

| Text Object | Description |
|-------------|-------------|
| `iw`, `aw` | Inner/a word |
| `iW`, `aW` | Inner/a WORD (whitespace-delimited) |
| `i(`, `a(`, `ib`, `ab` | Inner/around parentheses |
| `i[`, `a[` | Inner/around brackets |
| `i{`, `a{`, `iB`, `aB` | Inner/around braces |
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
| `>>` | Indent line |
| `<<` | Dedent line |
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
| `*` | Search word under cursor (forward, with word boundaries) |
| `#` | Search word under cursor (backward, with word boundaries) |
| `g*` | Search word under cursor (forward, no word boundaries) |
| `g#` | Search word under cursor (backward, no word boundaries) |

### Marks

| Key | Action |
|-----|--------|
| `m{a-z}` | Set mark {a-z} at current cursor position |
| `'{a-z}` | Jump to line of mark {a-z} (first non-blank) |
| `''` | Jump to position before last jump |

### Command Mode

Enter command mode by pressing `:` from Normal or Visual mode. Type a command and press `Enter` to execute.

#### Basic Commands

| Command | Action |
|---------|--------|
| `:w` | Submit (same as `ZZ`) |
| `:wq`, `:x` | Submit (same as `ZZ`) |
| `:q` | Quit (same as `ZQ`) |
| `:q!` | Force quit (same as `ZQ`) |
| `:{N}` | Go to line N (e.g., `:42` goes to line 42) |
| `:noh`, `:nohlsearch` | Clear search highlighting |

#### Substitute Command

The `:s` command performs search and replace with live preview:

| Command | Action |
|---------|--------|
| `:s/pattern/replacement/` | Replace first match on current line |
| `:s/pattern/replacement/g` | Replace all matches on current line |
| `:%s/pattern/replacement/g` | Replace all matches in entire buffer |
| `:'<,'>s/pattern/replacement/g` | Replace in visual selection (auto-filled) |
| `{count}:s/pattern/replacement/g` | Replace in next {count} lines (e.g., `5:s/foo/bar/g`) |

**Flags:**

| Flag | Description |
|------|-------------|
| `g` | Global - replace all matches on each line (not just first) |
| `c` | Confirm - prompt before each replacement |

**Confirm mode keys** (when using `c` flag):

| Key | Action |
|-----|--------|
| `y` | Replace this match and continue |
| `n` | Skip this match and continue |
| `a` | Replace all remaining matches |
| `q`, `Escape` | Quit, keeping changes made so far |
| `l` | Replace this match and quit (last) |

**Delimiter:** Any non-alphanumeric character can be used as delimiter (e.g., `:s#foo#bar#g`).

**Live preview:** As you type the pattern and replacement, matches are highlighted and replacements are shown in real-time.

**Visual mode integration:** Select text with `v`, `V`, or `Ctrl-V`, then press `:` to automatically scope the substitute to the selected lines.

### Visual Mode Operations

| Key | Action |
|-----|--------|
| `d`, `x` | Delete selection |
| `c`, `s` | Change selection (delete + insert) |
| `y` | Yank selection |
| `>` | Indent selection |
| `<` | Dedent selection |
| `v` | Toggle/switch to character-wise |
| `V` | Toggle/switch to line-wise |
| `Ctrl-V` | Toggle/switch to block-wise |
| `I` | Insert at block start (block mode) |
| `A` | Append at block end (block mode) |
| `$` | Extend block selection to end of each line (block mode) |
| `:` | Enter command mode with selection range (for `:s` substitute) |

### Insert Mode

| Key | Action |
|-----|--------|
| `Escape` | Return to Normal mode |
| `Backspace` | Delete character before cursor |
| `Ctrl-T` | Indent current line |
| `Ctrl-D` | Dedent current line |
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
