# Generated Service Credentials Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the database URL ordinary NixOS configuration and generate the shared internal API key persistently during `zed-setup`.

**Architecture:** The always-present `zed-setup` oneshot owns `/var/lib/zed`, generates a TOML key file only when absent or empty, and then applies collab migrations. The setup, facade, and collab units share `StateDirectory = "zed"` and the same configured user/group; the long-running services read the generated file directly while receiving the database URL through non-secret configuration.

**Tech Stack:** NixOS modules, systemd state directories, OpenSSL, SQLx CLI, PostgreSQL, Python-based NixOS VM tests

## Global Constraints

- Do not require an operator-supplied secrets file.
- Generate the internal API key from exactly 128 random bytes with `openssl rand -base64 128`.
- Preserve an existing non-empty generated key file across service restarts.
- Store shared state in `/var/lib/zed` with directory mode `0700` and key-file mode `0600`.
- Run setup, auth, and collab as the configured service user/group.
- Keep Caddy independent: failed Zed services must not prevent Caddy from starting.
- Run collab migrations before the facade starts and applies its embedded migrations.

---

### Task 1: Specify generated state in the NixOS VM test

**Files:**
- Modify: `tests/nixos/default.nix`
- Modify: `vm.nix`

**Interfaces:**
- Consumes: the existing `services.zed-auth` module interface and NixOS VM test
- Produces: executable expectations for `database.url`, `zed-setup.service`, and `/var/lib/zed/zed-auth.toml`

- [ ] **Step 1: Remove the caller-supplied secret from test configurations**

Replace the `nodes.machine` definition with a module configuration that sets no
`secretsFile`:

```nix
nodes.machine = {
  imports = [module];

  services.zed-auth = {
    enable = true;
    package = zedAuthPackage;
    virtualHost = "http://localhost";
    collab.package = zedCollabPackage;
  };
};
```

Remove the `secrets` binding and `services.zed-auth.secretsFile` from `vm.nix`.

- [ ] **Step 2: Change the expected setup unit and add state assertions**

Import `base64` and `tomllib` in `testScript`. Replace
`zed-collab-database-migrate.service` with `zed-setup.service` in the unit and
service-user checks. Add these assertions after the units are healthy:

```python
for unit in [
    "zed-setup.service",
    "zed-auth.service",
    "zed-collab.service",
]:
    state_directory = machine.succeed(
        f"systemctl show {unit} --property=StateDirectory --value"
    ).strip()
    assert state_directory == "zed", f"{unit} uses {state_directory!r}"

assert machine.succeed(
    "stat --format='%U:%G %a' /var/lib/zed"
).strip() == "zed:zed 700"
assert machine.succeed(
    "stat --format='%U:%G %a' /var/lib/zed/zed-auth.toml"
).strip() == "zed:zed 600"

secret_toml = machine.succeed("cat /var/lib/zed/zed-auth.toml")
secret = tomllib.loads(secret_toml)
assert set(secret) == {"ZED_CLOUD_INTERNAL_API_KEY"}, secret
key = secret["ZED_CLOUD_INTERNAL_API_KEY"]
assert len(base64.b64decode(key, validate=True)) == 128

auth_environment = machine.succeed(
    "systemctl show zed-auth.service --property=Environment --value"
)
assert "DATABASE_URL=postgres://zed@%2Frun%2Fpostgresql/zed" in auth_environment
assert (
    "ZED_AUTH_SECRETS_FILE=/var/lib/zed/zed-auth.toml"
    in auth_environment
)

collab_environment = tomllib.loads(
    machine.succeed("cat /run/zed-collab/crates/collab/.env.toml")
)
assert (
    collab_environment["DATABASE_URL"]
    == "postgres://zed@%2Frun%2Fpostgresql/zed"
)
assert machine.succeed(
    "readlink /run/zed-collab/crates/collab/.env.secret.toml"
).strip() == "/var/lib/zed/zed-auth.toml"

machine.succeed("systemctl restart zed-setup.service")
assert machine.succeed("cat /var/lib/zed/zed-auth.toml") == secret_toml
```

- [ ] **Step 3: Run the NixOS test and verify RED**

Run:

```bash
nix build 'path:/home/stefan/src/misc/zed-auth#checks.x86_64-linux.nixos-basic' --print-build-logs
```

Expected: FAIL during evaluation because `secretsFile` still has no value, or
at runtime because `zed-setup.service` and generated state do not exist.

- [ ] **Step 4: Commit the failing test**

```bash
git add tests/nixos/default.nix vm.nix
git commit -m "test generated zed service credentials"
```

### Task 2: Generate and consume shared service state

**Files:**
- Modify: `modules/nixos.nix`
- Test: `tests/nixos/default.nix`

**Interfaces:**
- Consumes: `services.zed-auth.user`, `group`, `database.name`, `database.createLocally`, and the packaged collab migrations
- Produces: `services.zed-auth.database.url`, `zed-setup.service`, and `/var/lib/zed/zed-auth.toml`

- [ ] **Step 1: Make the database URL ordinary module configuration**

Remove the `secretsFile` option. Add this option under
`services.zed-auth.database`:

```nix
url = mkOption {
  type = str;
  default = "postgres://${cfg.user}@%2Frun%2Fpostgresql/${cfg.database.name}";
  description = "PostgreSQL URL shared by collab and the authentication facade.";
};
```

Add `DATABASE_URL = cfg.database.url;` to `collabEnvironment` and the
percent-escaped database URL to the `zed-auth.environment` attribute set:

```nix
DATABASE_URL = lib.replaceStrings ["%"] ["%%"] cfg.database.url;
```

- [ ] **Step 2: Replace migration-only dependency wiring with setup wiring**

Replace the existing migration dependency locals with:

```nix
postgresqlDependencies = optional cfg.database.createLocally "postgresql.target";
```

Make both long-running units require and start after the inline
`"zed-setup.service"` value. Keep collab's existing requirement on
`zed-auth.service`.

- [ ] **Step 3: Implement the setup unit**

Rename `zed-collab-database-migrate` to `zed-setup`, remove its enclosing
`mkIf`, and use `postgresqlDependencies` for its `requires` and `after`.
Define this script:

```nix
script = ''
  secret_file="$STATE_DIRECTORY/zed-auth.toml"
  (
    set -euo pipefail
    umask 0077
    if [ ! -s "$secret_file" ]; then
      key="$(
        ${getExe pkgs.openssl} rand -base64 128 \
          | ${lib.getExe' pkgs.coreutils "tr"} -d '\n'
      )"
      printf 'ZED_CLOUD_INTERNAL_API_KEY = "%s"\n' "$key" >"$secret_file"
    fi
  )

  ${getExe pkgs.sqlx-cli} migrate run \
    --ignore-missing \
    --source ${cfg.collab.package}/share/zed-collab/migrations \
    --database-url "$DATABASE_URL"
'';
```

Set the systemd-escaped database URL:

```nix
environment.DATABASE_URL =
  lib.replaceStrings ["%"] ["%%"] cfg.database.url;
```

Retain the oneshot, hardening, configured user/group, and `RemainAfterExit`,
and add:

```nix
StateDirectory = "zed";
StateDirectoryMode = "0700";
```

- [ ] **Step 4: Make auth and collab consume the shared state**

For `zed-auth`, remove `LoadCredential`, add
`ZED_AUTH_SECRETS_FILE = "%S/zed/zed-auth.toml"` to its environment, and add
the shared `StateDirectory` settings.

For `zed-collab`, link `.env.secret.toml` from
`"$STATE_DIRECTORY/zed-auth.toml"`, remove `LoadCredential`, add the shared
`StateDirectory` settings, and use `WorkingDirectory = "%t/zed-collab"`.

- [ ] **Step 5: Format and run the NixOS test to verify GREEN**

Run:

```bash
nix fmt
nix build 'path:/home/stefan/src/misc/zed-auth#checks.x86_64-linux.nixos-basic' --print-build-logs
```

Expected: PASS. The VM proves the generated key is valid, persistent,
restrictively stored, and shared by both consumers.

- [ ] **Step 6: Commit the module implementation**

```bash
git add modules/nixos.nix
git commit -m "generate shared zed service credentials"
```

### Task 3: Document the simplified module interface

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: the completed NixOS module interface
- Produces: operator guidance that no longer asks for a secrets file or manual internal key

- [ ] **Step 1: Update the module example and VM description**

Remove `secretsFile` from the NixOS example. State that the VM and normal module
deployment generate the internal API key in `/var/lib/zed/zed-auth.toml`.

- [ ] **Step 2: Replace the credential-file deployment instructions**

Document `services.zed-auth.database.url` as non-secret configuration. Explain
that `zed-setup` creates the key once, preserves it across restarts, and runs
collab migrations before auth and collab start.

- [ ] **Step 3: Update internal-key and validation sections**

Clarify that standalone non-NixOS runs may still set
`ZED_CLOUD_INTERNAL_API_KEY`, while the NixOS module generates it. Replace the
old migration unit name with `zed-setup` and mention that the VM test checks
key permissions and persistence.

- [ ] **Step 4: Check documentation and commit**

Run:

```bash
rg -n 'secretsFile|zed-collab-database-migrate|operator-supplied|fixed throwaway' README.md vm.nix modules tests
git diff --check
```

Expected: `rg` finds no stale module guidance or old unit names;
`git diff --check` exits successfully.

```bash
git add README.md
git commit -m "document generated zed service credentials"
```

### Task 4: Run complete regression verification

**Files:**
- Verify only

**Interfaces:**
- Consumes: all prior tasks
- Produces: final evidence that Rust and Nix behavior remain healthy

- [ ] **Step 1: Run Rust checks**

Run:

```bash
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

Expected: formatting passes, all non-ignored tests pass, and Clippy reports no
warnings.

- [ ] **Step 2: Run Nix formatting and flake checks**

Run:

```bash
nix fmt
nix flake check 'path:/home/stefan/src/misc/zed-auth' --print-build-logs
git diff --check
```

Expected: all checks pass and no whitespace errors are reported.

- [ ] **Step 3: Inspect the final change set**

Run:

```bash
git status --short
git diff HEAD^ --stat
```

Expected: only the planned module, VM test/configuration, README, design, and
plan files changed.
