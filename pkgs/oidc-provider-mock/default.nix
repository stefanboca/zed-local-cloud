{
  fetchPypi,
  lib,
  python3Packages,
}: let
  htpy = python3Packages.buildPythonPackage (finalAttrs: {
    pname = "htpy";
    version = "26.5.1";
    format = "wheel";

    src = fetchPypi {
      inherit (finalAttrs) pname version;
      format = "wheel";
      dist = "py3";
      python = "py3";
      hash = "sha256-FIVm251yDIl7qlgy4qSWlUzjF6sG70mhA7RPihUyLDs=";
    };

    dependencies = with python3Packages; [markupsafe typing-extensions];
    pythonImportsCheck = ["htpy"];
  });
in
  python3Packages.buildPythonApplication (finalAttrs: {
    pname = "oidc-provider-mock";
    version = "0.4.6";
    format = "wheel";

    src = fetchPypi {
      pname = "oidc_provider_mock";
      inherit (finalAttrs) version;
      format = "wheel";
      dist = "py3";
      python = "py3";
      hash = "sha256-Ull1c+C3tcBFzwJ0slkDJ5lR1WQcam6KxRkj00dDEAw=";
    };

    dependencies =
      (with python3Packages; [authlib flask httpx joserfc pydantic pyyaml typing-extensions uvicorn])
      ++ [htpy];
    pythonImportsCheck = ["oidc_provider_mock"];

    meta = {
      description = "Mock OpenID Provider server for development and testing";
      homepage = "https://github.com/geigerzaehler/oidc-provider-mock";
      license = lib.licenses.mit;
      mainProgram = "oidc-provider-mock";
    };
  })
