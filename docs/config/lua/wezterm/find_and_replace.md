---
title: wezterm.find_and_replace
tags:
 - utility
 - filesystem
---

# `wezterm.find_and_replace(options)`

{{since('nightly')}}

Performs find and replace operations on a single file. Supports both literal
string matching and regular expressions.

Returns a table containing information about the operation including whether
the file was modified, the number of replacements made, and details about
each match.

## Parameters

The `options` parameter is a table that must contain:

| Option | Type | Required | Default | Description |
|--------|------|----------|---------|-------------|
| `path` | string | yes | - | Path to the file to modify |
| `find` | string | yes | - | Pattern to search for |
| `replace` | string | yes | - | Replacement text |
| `regex` | bool | no | `false` | Treat `find` as a regular expression |
| `ignore_case` | bool | no | `false` | Case-insensitive matching |
| `global` | bool | no | `true` | Replace all occurrences (not just first) |
| `dry_run` | bool | no | `false` | Preview changes without modifying file |
| `backup` | string | no | `nil` | File extension for backup (e.g., `".bak"`) |
| `multiline` | bool | no | `true` | Enable multiline mode for regex (`^` and `$` match line boundaries) |
| `dot_matches_newline` | bool | no | `false` | Allow `.` to match newline characters in regex |

## Return Value

Returns a table with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `modified` | bool | Whether the file was changed |
| `count` | number | Number of replacements made |
| `content` | string | The resulting file content |
| `matches` | table | Array of match information |

Each entry in `matches` contains:

| Field | Type | Description |
|-------|------|-------------|
| `line` | number | Line number (1-based) |
| `column` | number | Column number (1-based) |
| `text` | string | The matched text |

## Examples

### Simple literal replacement

```lua
local wezterm = require 'wezterm'

local result = wezterm.find_and_replace {
  path = '/path/to/file.txt',
  find = 'old_value',
  replace = 'new_value',
}

wezterm.log_info('Modified: ' .. tostring(result.modified))
wezterm.log_info('Replacements: ' .. result.count)
```

### Case-insensitive replacement

```lua
local wezterm = require 'wezterm'

local result = wezterm.find_and_replace {
  path = '/path/to/file.txt',
  find = 'TODO',
  replace = 'DONE',
  ignore_case = true,
}
```

### Regex replacement

```lua
local wezterm = require 'wezterm'

-- Replace all date formats YYYY-MM-DD with MM/DD/YYYY
local result = wezterm.find_and_replace {
  path = '/path/to/file.txt',
  find = '(\\d{4})-(\\d{2})-(\\d{2})',
  replace = '$2/$3/$1',
  regex = true,
}
```

### Dry run to preview changes

```lua
local wezterm = require 'wezterm'

local result = wezterm.find_and_replace {
  path = '/path/to/config.json',
  find = '"debug": true',
  replace = '"debug": false',
  dry_run = true,
}

-- Preview what would change
for _, match in ipairs(result.matches) do
  wezterm.log_info(
    string.format(
      'Line %d, Column %d: %s',
      match.line,
      match.column,
      match.text
    )
  )
end
```

### Create backup before modifying

```lua
local wezterm = require 'wezterm'

local result = wezterm.find_and_replace {
  path = '/path/to/important.conf',
  find = 'old_setting',
  replace = 'new_setting',
  backup = '.bak', -- Creates important.conf.bak
}
```

### Replace only first occurrence

```lua
local wezterm = require 'wezterm'

local result = wezterm.find_and_replace {
  path = '/path/to/file.txt',
  find = 'placeholder',
  replace = 'actual_value',
  global = false, -- Only replace first match
}
```
