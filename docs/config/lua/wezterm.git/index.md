# `wezterm.git` module

{{since('nightly')}}

The `wezterm.git` module provides functions for querying and manipulating git
repositories. This is useful for customizing prompts, tab titles, and status
displays with git information.

The module uses libgit2 internally, which provides significantly better
performance compared to spawning git commands—important for status bar updates
that run on every prompt.

## Read Operations

These functions return `nil` on any failure (not a repo, permission denied, etc.),
making them safe to use in status bars without error handling.

- [is_repository](is_repository.md) - Check if a path is inside a git repository
- [get_repository_root](get_repository_root.md) - Get the root directory of the repository
- [get_current_branch](get_current_branch.md) - Get the current branch name
- [get_head_commit_hash](get_head_commit_hash.md) - Get the HEAD commit hash
- [is_dirty](is_dirty.md) - Check if the repository has uncommitted changes
- [get_status](get_status.md) - Get detailed status counts
- [get_remote_url](get_remote_url.md) - Get the URL of a remote
- [get_ahead_behind](get_ahead_behind.md) - Get ahead/behind counts relative to upstream

## Write Operations

These functions throw errors on failure, since they are explicit user actions
where failures should be visible.

- [checkout](checkout.md) - Checkout a branch, tag, or commit
- [add](add.md) - Stage files
- [commit](commit.md) - Create a commit
- [fetch](fetch.md) - Fetch from remote
- [push](push.md) - Push to remote
- [reset](reset.md) - Reset HEAD
- [rebase](rebase.md) - Rebase onto upstream
