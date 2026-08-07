#!/usr/bin/env python3
import base64
import hashlib
import json
import os
import secrets
import subprocess
import sys
import tempfile
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlencode, urlsplit, urlunsplit


ISSUER = "http://127.0.0.1:18080"
CLIENT_ID = "zed-local"
CLIENT_SECRET = "test-client-secret"
OPENSSL = sys.argv[1]
CODES = {}
LOGIN_COUNT = 0


def base64url(value):
    return base64.urlsafe_b64encode(value).rstrip(b"=").decode()


key_fd, KEY_PATH = tempfile.mkstemp(prefix="fake-oidc-", suffix=".pem")
os.close(key_fd)
subprocess.run(
    [
        OPENSSL,
        "genpkey",
        "-algorithm",
        "RSA",
        "-pkeyopt",
        "rsa_keygen_bits:2048",
        "-out",
        KEY_PATH,
    ],
    check=True,
    stderr=subprocess.DEVNULL,
)
modulus_output = subprocess.check_output(
    [OPENSSL, "rsa", "-in", KEY_PATH, "-noout", "-modulus"],
    text=True,
).strip()
MODULUS = bytes.fromhex(modulus_output.removeprefix("Modulus="))


def signed_id_token(nonce):
    now = int(time.time())
    header = base64url(
        json.dumps(
            {"alg": "RS256", "kid": "test-key", "typ": "JWT"},
            separators=(",", ":"),
        ).encode()
    )
    payload = base64url(
        json.dumps(
            {
                "iss": ISSUER,
                "sub": "alice-subject",
                "aud": CLIENT_ID,
                "exp": now + 300,
                "iat": now,
                "nonce": nonce,
            },
            separators=(",", ":"),
        ).encode()
    )
    signing_input = f"{header}.{payload}".encode()
    signature = subprocess.run(
        [OPENSSL, "dgst", "-sha256", "-sign", KEY_PATH],
        input=signing_input,
        check=True,
        capture_output=True,
    ).stdout
    return f"{header}.{payload}.{base64url(signature)}"


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        global LOGIN_COUNT

        request = urlsplit(self.path)
        if request.path == "/.well-known/openid-configuration":
            self.reply_json(
                {
                    "issuer": ISSUER,
                    "authorization_endpoint": f"{ISSUER}/authorize",
                    "token_endpoint": f"{ISSUER}/token",
                    "jwks_uri": f"{ISSUER}/jwks",
                    "userinfo_endpoint": f"{ISSUER}/userinfo",
                    "response_types_supported": ["code"],
                    "subject_types_supported": ["public"],
                    "id_token_signing_alg_values_supported": ["RS256"],
                    "token_endpoint_auth_methods_supported": ["client_secret_basic"],
                    "code_challenge_methods_supported": ["S256"],
                }
            )
        elif request.path == "/jwks":
            self.reply_json(
                {
                    "keys": [
                        {
                            "kty": "RSA",
                            "use": "sig",
                            "kid": "test-key",
                            "alg": "RS256",
                            "n": base64url(MODULUS),
                            "e": "AQAB",
                        }
                    ]
                }
            )
        elif request.path == "/authorize":
            query = parse_qs(request.query)
            required = [
                "client_id",
                "redirect_uri",
                "state",
                "nonce",
                "code_challenge",
            ]
            if any(len(query.get(name, [])) != 1 for name in required):
                self.send_error(400)
                return
            if query["client_id"][0] != CLIENT_ID:
                self.send_error(400)
                return
            if query.get("code_challenge_method") != ["S256"]:
                self.send_error(400)
                return

            LOGIN_COUNT += 1
            code = secrets.token_urlsafe(24)
            CODES[code] = {
                "attempt": LOGIN_COUNT,
                "challenge": query["code_challenge"][0],
                "nonce": query["nonce"][0],
            }
            callback = urlsplit(query["redirect_uri"][0])
            location = urlunsplit(
                callback._replace(
                    query=urlencode(
                        {"code": code, "state": query["state"][0]}
                    )
                )
            )
            self.redirect(location)
        elif request.path == "/userinfo":
            authorization = self.headers.get("Authorization", "")
            if not authorization.startswith("Bearer access-"):
                self.send_error(401)
                return
            code = authorization.removeprefix("Bearer access-")
            login = CODES.get(code)
            if login is None:
                self.send_error(401)
                return
            attempt = login["attempt"]
            groups = ["users", "zed-admin"] if attempt == 1 else ["users"]
            self.reply_json(
                {
                    "sub": "alice-subject",
                    "preferred_username": "alice",
                    "email": "alice@example.com",
                    "name": "Alice Example",
                    "picture": f"https://images.example.com/alice-{attempt}.png",
                    "groups": groups,
                }
            )
        else:
            self.send_error(404)

    def do_POST(self):
        request = urlsplit(self.path)
        if request.path != "/token":
            self.send_error(404)
            return

        expected_authorization = "Basic " + base64.b64encode(
            f"{CLIENT_ID}:{CLIENT_SECRET}".encode()
        ).decode()
        if self.headers.get("Authorization") != expected_authorization:
            self.send_error(401)
            return
        length = int(self.headers.get("Content-Length", "0"))
        form = parse_qs(self.rfile.read(length).decode())
        if form.get("grant_type") != ["authorization_code"]:
            self.send_error(400)
            return
        code = form.get("code", [None])[0]
        verifier = form.get("code_verifier", [None])[0]
        login = CODES.get(code)
        if login is None or verifier is None:
            self.send_error(400)
            return
        actual_challenge = base64url(hashlib.sha256(verifier.encode()).digest())
        if actual_challenge != login["challenge"]:
            self.send_error(400)
            return

        self.reply_json(
            {
                "access_token": f"access-{code}",
                "token_type": "Bearer",
                "id_token": signed_id_token(login["nonce"]),
            }
        )

    def log_message(self, _format, *_args):
        pass

    def redirect(self, location):
        self.send_response(302)
        self.send_header("Location", location)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def reply_json(self, value):
        body = json.dumps(value).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


try:
    ThreadingHTTPServer(("127.0.0.1", 18080), Handler).serve_forever()
finally:
    os.unlink(KEY_PATH)
