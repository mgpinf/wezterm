# `wezterm.git.rebase(path, upstream)`

{{since('nightly')}}

Rebase the current branch onto upstream. Returns `true` on success, throws an
error on failure.

This performs a non-interactive rebase, automatically applying each commit.
If there are conflicts, the rebase will fail with an error.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path inside the repository |
| `upstream` | string | Branch, tag, or commit to rebase onto |

## Examples

```lua
local wezterm = require 'wezterm'

-- Rebase onto origin/main
wezterm.git.rebase('/path/to/repo', 'origin/main')

-- Rebase onto local branch
wezterm.git.rebase('/path/to/repo', 'main')

-- Rebase onto specific commit
wezterm.git.rebase('/path/to/repo', 'abc1234')
```

## Example: Fetch and rebase

```lua
local wezterm = require 'wezterm'

wezterm.git.fetch '/path/to/repo'
wezterm.git.rebase('/path/to/repo', 'origin/main')
```
