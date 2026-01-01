# `wezterm.git.get_head_commit_hash(path [, options])`

{{since('nightly')}}

Returns the HEAD commit hash, or `nil` if not in a git repository.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path inside the repository |
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `short` | bool | `false` | Return short (7 character) hash |

## Examples

```lua
local wezterm = require 'wezterm'

-- Get full hash
local hash = wezterm.git.get_head_commit_hash '/path/to/repo'
-- e.g., "abc123def456789..."

-- Get short hash
local short =
  wezterm.git.get_head_commit_hash('/path/to/repo', { short = true })
-- e.g., "abc123d"
```
