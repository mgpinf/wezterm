# `wezterm.git.checkout(path, ref [, options])`

{{since('nightly')}}

Checkout a branch, tag, or commit. Returns `true` on success, throws an error
on failure.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path inside the repository |
| `ref` | string | Branch name, tag, or commit to checkout |
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `create` | bool | `false` | Create a new branch |
| `start_point` | string | `"HEAD"` | Starting point for new branch |

## Examples

```lua
local wezterm = require 'wezterm'

-- Switch to existing branch
wezterm.git.checkout('/path/to/repo', 'main')

-- Create and switch to new branch (like git checkout -b)
wezterm.git.checkout('/path/to/repo', 'feature-branch', { create = true })

-- Create branch from specific commit
wezterm.git.checkout('/path/to/repo', 'hotfix', {
  create = true,
  start_point = 'origin/main',
})

-- Checkout a tag
wezterm.git.checkout('/path/to/repo', 'v1.0.0')

-- Checkout a specific commit (detached HEAD)
wezterm.git.checkout('/path/to/repo', 'abc1234')
```
