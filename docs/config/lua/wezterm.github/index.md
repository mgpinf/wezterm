# `wezterm.github` module

{{since('nightly')}}

The `wezterm.github` module provides functions for interacting with the GitHub
REST API. This is useful for displaying repository information, managing pull
requests, or automating GitHub workflows from your terminal.

The module uses the octocrab library internally for GitHub API access.

## Authentication

All functions accept an optional `token` parameter for authentication.
Without a token, you can access public repositories with lower rate limits.
With a token, you get higher rate limits and access to private repositories.

```lua
local wezterm = require 'wezterm'

local token = os.getenv 'GITHUB_TOKEN'
local user = wezterm.github.get_authenticated_user { token = token }
```

## User Functions

- [get_authenticated_user](get_authenticated_user.md) - Get the authenticated user's information

## Repository Functions

- [get_repo](get_repo.md) - Get repository information
- [list_repos](list_repos.md) - List repositories for the authenticated user
- [list_org_repos](list_org_repos.md) - List repositories for an organization

## Pull Request Functions

- [list_pulls](list_pulls.md) - List pull requests
- [get_pull](get_pull.md) - Get a pull request
- [create_pull](create_pull.md) - Create a pull request
- [merge_pull](merge_pull.md) - Merge a pull request

## Issue Functions

- [list_issues](list_issues.md) - List issues
- [get_issue](get_issue.md) - Get an issue
- [create_issue](create_issue.md) - Create an issue
- [update_issue](update_issue.md) - Update an issue

## Commit Functions

- [list_commits](list_commits.md) - List commits
- [get_commit](get_commit.md) - Get a commit

## Branch Functions

- [list_branches](list_branches.md) - List branches

## Release Functions

- [list_releases](list_releases.md) - List releases
- [get_release](get_release.md) - Get a release
- [get_latest_release](get_latest_release.md) - Get the latest release
- [create_release](create_release.md) - Create a release

## Workflow Functions

- [list_workflows](list_workflows.md) - List workflows
- [list_workflow_runs](list_workflow_runs.md) - List workflow runs

## Gist Functions

- [get_gist](get_gist.md) - Get a gist

## Search Functions

- [search_repos](search_repos.md) - Search repositories
- [search_issues](search_issues.md) - Search issues and pull requests
- [search_code](search_code.md) - Search code
