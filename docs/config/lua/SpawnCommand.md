# SpawnCommand

The `SpawnCommand` struct specifies information about a new command
to be spawned.

It is a lua object with the following fields; all of the fields
have reasonable defaults and can be omitted.

```lua
wezterm.action.SpawnCommandInNewWindow {
  -- An optional label.
  -- The label is only used for SpawnCommands that are listed in
  -- the `launch_menu` configuration section.
  -- If the label is omitted, a default will be produced based
  -- on the `args` field.
  label = 'List all the files!',

  -- The argument array specifying the command and its arguments.
  -- If omitted, the default program for the target domain will be
  -- spawned.
  args = { 'ls', '-al' },

  -- The current working directory to set for the command.
  -- If omitted, wezterm will infer a value based on the active pane
  -- at the time this action is triggered.  If the active pane
  -- matches the domain specified in this `SpawnCommand` instance
  -- then the current working directory of the active pane will be
  -- used.
  -- If the current working directory cannot be inferred then it
  -- will typically fall back to using the home directory of
  -- the current user.
  cwd = '/some/path',

  -- Sets additional environment variables in the environment for
  -- this command invocation.
  set_environment_variables = {
    SOMETHING = 'a value',
  },

  -- Specify that the multiplexer domain of the currently active pane
  -- should be used to start this process.  This is usually what you
  -- want to happen and this is the default behavior if you omit
  -- the domain.
  domain = 'CurrentPaneDomain',

  -- Specify that the default multiplexer domain be used for this
  -- command invocation.  The default domain is typically the "local"
  -- domain, which spawns processes locally.  However, if you started
  -- wezterm using `wezterm connect` or `wezterm serial` then the default
  -- domain will not be "local".
  domain = 'DefaultDomain',

  -- Specify a named multiplexer domain that should be used to spawn
  -- this new command.
  -- This is useful if you want to assign a hotkey to always start
  -- a process in a remote multiplexer rather than based on the
  -- current pane.
  -- See the Multiplexing section of the docs for more on this topic.
  domain = { DomainName = 'my.server' },

  -- Since: 20230320-124340-559cb7b0
  -- Specify the initial position for a GUI window when this command
  -- is used in a context that will create a new window, such as with
  -- wezterm.mux.spawn_window, SpawnCommandInNewWindow
  position = {
    x = 10,
    y = 300,
    -- Optional origin to use for x and y.
    -- Possible values:
    -- * "ScreenCoordinateSystem" (this is the default)
    -- * "MainScreen" (the primary or main screen)
    -- * "ActiveScreen" (whichever screen hosts the active/focused window)
    -- * {Named="HDMI-1"} - uses a screen by name. See wezterm.gui.screens()
    -- origin = "ScreenCoordinateSystem"
  },

  -- Override the exit_behavior for just this spawned command.
  -- If omitted, the global exit_behavior configuration will be used.
  exit_behavior = 'Hold', -- {{since('nightly', inline=True)}}

  -- Override the exit_behavior_messaging for just this spawned command.
  -- If omitted, the global exit_behavior_messaging configuration will be used.
  exit_behavior_messaging = 'Terse', -- {{since('nightly', inline=True)}}
}
```

{{since('nightly')}}

## exit_behavior

The `exit_behavior` field allows overriding the global
[exit_behavior](config/exit_behavior.md) configuration for this specific
spawned command. This is useful when you want certain commands to behave
differently when they exit.

Possible values:

* `"Close"` - close the corresponding pane as soon as the program exits.
* `"Hold"` - keep the pane open after the program exits.
* `"CloseOnCleanExit"` - if the program exited with a successful status, behave like `"Close"`, otherwise, behave like `"Hold"`.

If omitted, the global `exit_behavior` configuration will be used.

### Example: Hold pane open after script completes

```lua
config.keys = {
  {
    key = 'b',
    mods = 'CMD',
    action = wezterm.action.SpawnCommandInNewTab {
      args = { './build-script.sh' },
      exit_behavior = 'Hold',
    },
  },
}
```

{{since('nightly')}}

## exit_behavior_messaging

The `exit_behavior_messaging` field allows overriding the global
[exit_behavior_messaging](config/exit_behavior_messaging.md) configuration
for this specific spawned command. This controls how wezterm indicates the
exit status of the spawned process when it terminates.

Possible values:

* `"Verbose"` - Shows 2-3 lines of explanation, including the process name, its exit status and a link to the exit_behavior documentation.
* `"Brief"` - Like `"Verbose"`, but the link to the documentation is not included.
* `"Terse"` - A very short indication of the exit status is shown in square brackets.
* `"None"` - No message is shown.

If omitted, the global `exit_behavior_messaging` configuration will be used.

### Example: Minimal exit messaging for a monitoring command

```lua
config.keys = {
  {
    key = 'm',
    mods = 'CMD',
    action = wezterm.action.SpawnCommandInNewTab {
      args = { 'htop' },
      exit_behavior = 'Close',
      exit_behavior_messaging = 'None',
    },
  },
}
```

