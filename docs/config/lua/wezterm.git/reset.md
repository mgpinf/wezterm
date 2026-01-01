# `wezterm.git.reset(path [, options])`

{{since('nightly')}}

Reset the current HEAD. Returns `true` on success, throws an error on failure.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path inside the repository |
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `mode` | string | `"mixed"` | Reset mode: `"soft"`, `"mixed"`, or `"hard"` |
| `target` | string | `"HEAD"` | Target commit to reset to |

## Reset Modes

| Mode | Index | Working Tree | Description |
|------|-------|--------------|-------------|
| `soft` | Unchanged | Unchanged | Only moves HEAD |
| `mixed` | Reset | Unchanged | Unstages changes |
| `hard` | Reset | Reset | Discards all changes |

## Examples

```lua
local wezterm = require 'wezterm'

-- Unstage all changes (git reset)
wezterm.git.reset '/path/to/repo'

-- Soft reset to previous commit
wezterm.git.reset('/path/to/repo', {
  mode = 'soft',
  target = 'HEAD~1',
})

-- Hard reset to specific commit (discards changes!)
wezterm.git.reset('/path/to/repo', {
  mode = 'hard',
  target = 'origin/main',
})
```
