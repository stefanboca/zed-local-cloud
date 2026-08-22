{
  lib,
  runCommand,
}:
runCommand "zed-migrations" {
  meta = {
    description = "Project-owned database migrations for Zed local cloud";
    platforms = lib.platforms.linux;
  };
} ''
  mkdir -p $out/share/zed/migrations
  cp -r ${../../migrations}/. $out/share/zed/migrations/
''
