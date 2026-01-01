# `wezterm.git.add(path, files)`

{{since('nightly')}}

Stage files for commit. Returns `true` on success, throws an error on failure.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path inside the repository |
| `files` | string or table | File(s) to stage |

## Examples

```lua
local wezterm = require 'wezterm'

-- Stage a single file
wezterm.git.add('/path/to/repo', 'file.txt')

-- Stage multiple files
wezterm.git.add('/path/to/repo', { 'file1.txt', 'file2.txt', 'src/main.rs' })

-- Stage all files (like git add .)
wezterm.git.add('/path/to/repo', '.')

-- Stage all files (alternative)
wezterm.git.add('/path/to/repo', '*')
```
