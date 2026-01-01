# `wezterm.git.commit(path, options)`

{{since('nightly')}}

Create a commit with the staged changes. Returns the commit hash on success,
throws an error on failure.

## Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `path` | string | Path inside the repository |
| `options` | table | Commit options |

## Options

| Option | Type | Required | Description |
|--------|------|----------|-------------|
| `message` | string | Yes | Commit message |
| `author_name` | string | No | Author name (defaults to git config) |
| `author_email` | string | No | Author email (defaults to git config) |

## Examples

```lua
local wezterm = require 'wezterm'

-- Simple commit
local hash = wezterm.git.commit('/path/to/repo', {
  message = 'Add new feature'
})
wezterm.log_info('Created commit: ' .. hash)

-- Commit with custom author
wezterm.git.commit('/path/to/repo', {
  message = 'Fix bug',
  author_name = 'John Doe',
  author_email = 'john@example.com'
})
```

## Example: Stage and commit

```lua
local wezterm = require 'wezterm'

-- Stage all changes and commit
wezterm.git.add('/path/to/repo', '.')
local hash = wezterm.git.commit('/path/to/repo', {
  message = 'Update files'
})
```
