---
title: wezterm.find_and_replace_in_files
tags:
 - utility
 - filesystem
---

# `wezterm.find_and_replace_in_files(options)`

{{since('nightly')}}

Performs find and replace operations across multiple files in a directory.
Recursively searches for files matching the specified criteria and applies
replacements. Supports both literal string matching and regular expressions.

This function processes files in parallel for improved performance on large
codebases.

## Parameters

The `options` parameter is a table that must contain:

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `directory` | string | yes | - | Directory to search in |
| `find` | string | yes | - | Pattern to search for |
| `replace` | string | yes | - | Replacement text |
| `regex` | bool | no | `false` | Treat `find` as a regular expression |
| `ignore_case` | bool | no | `false` | Case-insensitive matching |
| `global` | bool | no | `true` | Replace all occurrences per file |
| `dry_run` | bool | no | `false` | Preview changes without modifying files |
| `backup` | string | no | `nil` | File extension for backups (e.g., `".bak"`) |
| `extensions` | table | no | `{}` | Array of file extensions to include (e.g., `{"lua", "rs"}`) |
| `exclude` | table | no | `{}` | Array of glob patterns to exclude |
| `max_depth` | number | no | `nil` | Maximum directory depth to traverse |
| `hidden` | bool | no | `false` | Include hidden files and directories |
| `multiline` | bool | no | `true` | Enable multiline mode for regex |
| `dot_matches_newline` | bool | no | `false` | Allow `.` to match newline in regex |

## Return Value

Returns a table with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `files` | table | Array of per-file results |
| `total_files` | number | Total files processed |
| `modified_files` | number | Number of files that were modified |
| `total_replacements` | number | Total replacements made across all files |

Each entry in `files` contains:

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | Absolute path to the file |
| `modified` | bool | Whether the file was changed |
| `count` | number | Number of replacements in this file |
| `matches` | table | Array of match details |

Each entry in `matches` contains:

| Field | Type | Description |
|-------|------|-------------|
| `line` | number | Line number (1-based) |
| `column` | number | Column number (1-based) |
| `text` | string | The matched text |

## Examples

### Replace across all Lua files

```lua
local wezterm = require 'wezterm'

local result = wezterm.find_and_replace_in_files {
  directory = wezterm.config_dir,
  find = 'old_function',
  replace = 'new_function',
  extensions = { 'lua' },
}

wezterm.log_info(
  string.format(
    'Modified %d of %d files, %d total replacements',
    result.modified_files,
    result.total_files,
    result.total_replacements
  )
)
```

### Dry run to preview changes

```lua
local wezterm = require 'wezterm'

local result = wezterm.find_and_replace_in_files {
  directory = '/path/to/project',
  find = 'deprecated_api',
  replace = 'new_api',
  extensions = { 'py', 'pyi' },
  dry_run = true,
}

-- Show what would change
for _, file in ipairs(result.files) do
  if file.modified then
    wezterm.log_info('Would modify: ' .. file.path)
    for _, match in ipairs(file.matches) do
      wezterm.log_info(string.format('  Line %d: %s', match.line, match.text))
    end
  end
end
```

### Regex replacement with exclusions

```lua
local wezterm = require 'wezterm'

-- Update version numbers in source files
local result = wezterm.find_and_replace_in_files {
  directory = '/path/to/project',
  find = 'version\\s*=\\s*"1\\.0\\.\\d+"',
  replace = 'version = "2.0.0"',
  regex = true,
  extensions = { 'toml', 'json' },
  exclude = { '**/target/**', '**/node_modules/**', '**/.git/**' },
}
```

### Create backups before modifying

```lua
local wezterm = require 'wezterm'

local result = wezterm.find_and_replace_in_files {
  directory = '/etc/myapp',
  find = 'localhost',
  replace = 'production.example.com',
  extensions = { 'conf', 'ini' },
  backup = '.backup',
}
```

### Include hidden files with depth limit

```lua
local wezterm = require 'wezterm'

local result = wezterm.find_and_replace_in_files {
  directory = wezterm.home_dir,
  find = 'old@email.com',
  replace = 'new@email.com',
  hidden = true,
  max_depth = 2,
  extensions = { 'conf', 'rc' },
}
```

### Case-insensitive replacement in all files

```lua
local wezterm = require 'wezterm'

local result = wezterm.find_and_replace_in_files {
  directory = '/path/to/docs',
  find = 'wezterm',
  replace = 'WezTerm',
  ignore_case = true,
  extensions = { 'md', 'txt' },
}
```
