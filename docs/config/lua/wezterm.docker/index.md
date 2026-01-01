# `wezterm.docker` module

{{since('nightly')}}

The `wezterm.docker` module provides functions for interacting with Docker
containers and images. This is useful for displaying container status in
prompts, managing development containers, or automating Docker workflows.

The module uses the Docker API internally via Unix socket connection.

## Connection

The module automatically tries multiple connection methods:
- `DOCKER_HOST` environment variable
- Default socket (`/var/run/docker.sock`)
- macOS Docker Desktop socket (`~/.docker/run/docker.sock`)

## System Functions

- [is_available](is_available.md) - Check if Docker is available and running
- [version](version.md) - Get Docker version information
- [info](info.md) - Get Docker system information

## Container Functions

- [list_containers](list_containers.md) - List Docker containers
- [get_container](get_container.md) - Get detailed container information
- [create_container](create_container.md) - Create a new container
- [start_container](start_container.md) - Start a container
- [stop_container](stop_container.md) - Stop a container
- [restart_container](restart_container.md) - Restart a container
- [remove_container](remove_container.md) - Remove a container
- [container_logs](container_logs.md) - Get container logs
- [exec](exec.md) - Execute a command in a running container

## Image Functions

- [list_images](list_images.md) - List Docker images
