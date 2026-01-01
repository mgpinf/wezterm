# `wezterm.git.fetch(path [, options])`

{{since('nightly')}}

Fetch from a remote. Returns `true` on success, throws an error on failure.

Uses SSH agent for authentication when connecting to SSH remotes.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path inside the repository |
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `remote` | string | `"origin"` | Remote to fetch from |

## Examples

```lua
local wezterm = require 'wezterm'

-- Fetch from origin
wezterm.git.fetch '/path/to/repo'

-- Fetch from specific remote
wezterm.git.fetch('/path/to/repo', { remote = 'upstream' })
```
