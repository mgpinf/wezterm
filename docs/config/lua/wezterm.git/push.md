# `wezterm.git.push(path [, options])`

{{since('nightly')}}

Push commits to a remote. Returns `true` on success, throws an error on failure.

Uses SSH agent for authentication when connecting to SSH remotes.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path inside the repository |
| `options` | table | Optional settings |

## Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `remote` | string | `"origin"` | Remote to push to |
| `branch` | string | current branch | Branch to push |

## Examples

```lua
local wezterm = require 'wezterm'

-- Push current branch to origin
wezterm.git.push '/path/to/repo'

-- Push to specific remote
wezterm.git.push('/path/to/repo', { remote = 'upstream' })

-- Push specific branch
wezterm.git.push('/path/to/repo', {
  remote = 'origin',
  branch = 'feature-branch',
})
```

## Example: Commit and push

```lua
local wezterm = require 'wezterm'

wezterm.git.add('/path/to/repo', '.')
wezterm.git.commit('/path/to/repo', { message = 'Update' })
wezterm.git.push '/path/to/repo'
```
