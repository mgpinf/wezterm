# `TypingTest`

{{since('nightly')}}

Overlays the current tab with a typing speed test, inspired by [toipe](https://github.com/Samyak2/toipe).

The typing test displays a set of random words that you must type correctly. It measures:

* **WPM (Words Per Minute)** - Your typing speed
* **Accuracy** - The percentage of characters typed correctly
* **Mistakes** - Total errors (including those corrected with backspace)

## Controls

During the test:
* Type the words as displayed
* `Backspace` - Delete the last character
* `Ctrl+W` - Delete the last word
* `Ctrl+R` - Restart with new words
* `Ctrl+C` - Exit the test

After completing the test:
* `Ctrl+R` - Try again with new words
* `Ctrl+C` - Exit

## Basic Usage

```lua
config.keys = {
  {
    key = 't',
    mods = 'CTRL|SHIFT',
    action = wezterm.action.TypingTest {},
  },
}
```

## Configuration Options

The `TypingTest` action accepts an optional table with the following fields.
These options match toipe's command line arguments:

| Field | Type | Default | toipe equivalent | Description |
|-------|------|---------|------------------|-------------|
| `wordlist` | string | `"Top250"` | `-w/--wordlist` | Built-in word list name |
| `wordlist_file` | string | `nil` | `-f/--file` | Path to custom word list file |
| `num_words` | number | `30` | `-n/--num-words` | Number of words per test (min: 5, max: 100) |
| `punctuation` | boolean | `false` | `-p/--punctuation` | Whether to include punctuation |
| `action` | action | `nil` | - | Callback action when test completes |

### Available Word Lists

| Value | Description |
|-------|-------------|
| `"Top250"` | Top 250 most common English words (default) |
| `"Top500"` | Top 500 most common English words |
| `"Top1000"` | Top 1000 most common English words |
| `"Top2500"` | Top 2500 most common English words |
| `"Top5000"` | Top 5000 most common English words |
| `"Top10000"` | Top 10000 most common English words |
| `"Top25000"` | Top 25000 most common English words |
| `"CommonlyMisspelled"` | Commonly misspelled English words |

## Examples

### Basic Test with Defaults

```lua
config.keys = {
  {
    key = 't',
    mods = 'CTRL|SHIFT',
    action = wezterm.action.TypingTest {},
  },
}
```

### Custom Word Count and Word List

```lua
config.keys = {
  {
    key = 't',
    mods = 'CTRL|SHIFT',
    action = wezterm.action.TypingTest {
      wordlist = 'Top1000',
      num_words = 50,
    },
  },
}
```

### With Punctuation

```lua
config.keys = {
  {
    key = 't',
    mods = 'CTRL|SHIFT',
    action = wezterm.action.TypingTest {
      wordlist = 'Top500',
      num_words = 30,
      punctuation = true,
    },
  },
}
```

### Custom Word List File

```lua
config.keys = {
  {
    key = 't',
    mods = 'CTRL|SHIFT',
    action = wezterm.action.TypingTest {
      wordlist_file = '/path/to/my/words.txt',
      num_words = 25,
    },
  },
}
```

### Practice Commonly Misspelled Words

```lua
config.keys = {
  {
    key = 't',
    mods = 'CTRL|SHIFT',
    action = wezterm.action.TypingTest {
      wordlist = 'CommonlyMisspelled',
      num_words = 20,
    },
  },
}
```

### With Completion Callback

```lua
wezterm.on('typing-test-complete', function(window, pane)
  window:toast_notification('Typing Test', 'Test completed!', nil, 3000)
end)

config.keys = {
  {
    key = 't',
    mods = 'CTRL|SHIFT',
    action = wezterm.action.TypingTest {
      action = wezterm.action_callback(function(window, pane)
        wezterm.emit('typing-test-complete', window, pane)
      end),
    },
  },
}
```

## See Also

* [ShowDebugOverlay](ShowDebugOverlay.md)
* [toipe](https://github.com/Samyak2/toipe) - The terminal typing tester that inspired this feature
