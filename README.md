# Zed local cloud

Self-host Zed collaboration with sign-in through your own OpenID Connect
provider.

The repository contains a Rust service that supplies the login and cloud API
routes expected by Zed, plus NixOS packages and modules for the collaboration
server. It connects browser-based authentication to the desktop client and
provides the local user accounts used by collaboration.

The deployment brings together the Rust service, collaboration server,
database, reverse proxy, and optional calling integration. Extension and
release requests still use Zed's public services. This is an unofficial
integration that follows the interfaces of a specific Zed release.

## Development

Start the backend development VM from the repository root:

```sh
nix run
```

See [setup and development](docs/setup.md) for running the Rust service,
configuring a client, and deploying the full installation.
