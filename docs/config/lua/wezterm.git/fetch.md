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
| `remote_branch` | string | nil | Remote branch to fetch |
| `local_branch` | string | nil | Local branch to update (defaults to `remote_branch` if not specified) |
| `refspec` | string | nil | Custom refspec (takes precedence over `remote_branch`/`local_branch`) |

## Examples

```lua
local wezterm = require 'wezterm'

-- Fetch from origin
wezterm.git.fetch '/path/to/repo'

-- Fetch from specific remote
wezterm.git.fetch('/path/to/repo', { remote = 'upstream' })

-- Fetch remote branch to same-named local branch
-- Equivalent to: git fetch origin main:main
wezterm.git.fetch('/path/to/repo', { remote_branch = 'main' })

-- Fetch remote branch to different local branch
-- Equivalent to: git fetch origin main:my-local-main
wezterm.git.fetch('/path/to/repo', {
  remote_branch = 'main',
  local_branch = 'my-local-main',
})

-- Fetch from upstream remote to local branch
-- Equivalent to: git fetch upstream develop:upstream-develop
wezterm.git.fetch('/path/to/repo', {
  remote = 'upstream',
  remote_branch = 'develop',
  local_branch = 'upstream-develop',
})

-- Using custom refspec directly
wezterm.git.fetch('/path/to/repo', {
  refspec = 'refs/heads/feature:refs/heads/local-feature',
})
```
