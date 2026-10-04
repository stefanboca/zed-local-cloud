{
  lib,
  pkgs,
}: let
  inherit (lib.meta) getExe;
in
  pkgs.writeShellApplication {
    name = "zed-sqlx-prepare";
    runtimeInputs = with pkgs; [
      gcc
      pkg-config
      postgresql
      sqlx-cli
    ];
    text = ''
      if [[ ! -f Cargo.toml || ! -d migrations || ! -d queries ]]; then
        echo "run this app from the zed-local-cloud repository root" >&2
        exit 1
      fi

      temporary_directory="$(mktemp -d /tmp/zed-sqlx-prepare.XXXXXXXX)"
      data_directory="$temporary_directory/data"
      socket_directory="$temporary_directory/socket"
      postgres_started=false

      cleanup() {
        if "$postgres_started"; then
          pg_ctl --pgdata "$data_directory" --mode fast stop >/dev/null
        fi
        rm -rf -- "$temporary_directory"
      }
      trap cleanup EXIT

      mkdir "$socket_directory"
      initdb \
        --auth trust \
        --encoding UTF8 \
        --no-instructions \
        --no-locale \
        --pgdata "$data_directory" \
        --username postgres
      pg_ctl \
        --pgdata "$data_directory" \
        --log "$temporary_directory/postgres.log" \
        --options "-k $socket_directory -c listen_addresses=" \
        --wait \
        start
      postgres_started=true

      createdb --host "$socket_directory" --username postgres zed
      encoded_socket_directory="''${socket_directory//\//%2F}"
      database_url="postgres://postgres@$encoded_socket_directory/zed"

      sqlx migrate run \
        --source "$PWD/migrations" \
        --database-url "$database_url"
      psql \
        --dbname "$database_url" \
        --set ON_ERROR_STOP=1 \
        --file "$PWD/queries/database/update_pg_trgm.sql"

      unset SQLX_OFFLINE
      export TAILWIND_CLI=${getExe pkgs.tailwindcss_4}
      cargo sqlx prepare --no-dotenv --database-url "$database_url"
    '';
  }
