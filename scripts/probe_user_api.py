#!/usr/bin/env python3
"""Probe NovelAI user API endpoints without printing secrets.

The script reads a bearer token from NOVELAI_USR_TOKEN in the process
environment or from a repository-local .env file. It prints response shape and
redacted previews only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any


DEFAULT_BASE_URL = "https://api.novelai.net"
DEFAULT_ENDPOINTS = ["/user/subscription", "/user/data"]
DEFAULT_TOKEN_VAR = "NOVELAI_USR_TOKEN"

SENSITIVE_KEY_RE = re.compile(
    r"(token|secret|password|email|username|user_name|customer|payment|"
    r"stripe|paypal|card|address|phone|session|jwt|bearer|auth|license|"
    r"api[_-]?key|(^|[_-])id$)",
    re.IGNORECASE,
)
EMAIL_RE = re.compile(r"[\w.+-]+@[\w.-]+\.[A-Za-z]{2,}")
JWT_RE = re.compile(r"^[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{10,}$")
LONG_SECRET_RE = re.compile(r"^[A-Za-z0-9_./+=-]{40,}$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Probe NovelAI user API endpoints and print redacted JSON summaries."
    )
    parser.add_argument(
        "--base-url",
        default=DEFAULT_BASE_URL,
        help=f"API base URL. Default: {DEFAULT_BASE_URL}",
    )
    parser.add_argument(
        "--endpoint",
        action="append",
        help="Endpoint path to probe. Repeatable. Defaults to subscription and user data.",
    )
    parser.add_argument(
        "--method",
        choices=["GET", "POST"],
        default="GET",
        help="HTTP method to use for all endpoints. Default: GET",
    )
    parser.add_argument(
        "--json-body",
        help="JSON body for POST requests. Omit for an empty JSON object.",
    )
    parser.add_argument(
        "--env-file",
        default=".env",
        help="Repository-local env file to read if the token is not in the process env.",
    )
    parser.add_argument(
        "--token-var",
        default=DEFAULT_TOKEN_VAR,
        help=f"Token variable name. Default: {DEFAULT_TOKEN_VAR}",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=20.0,
        help="Request timeout in seconds. Default: 20",
    )
    parser.add_argument(
        "--max-preview-chars",
        type=int,
        default=12000,
        help="Maximum redacted preview characters per endpoint. Default: 12000",
    )
    parser.add_argument(
        "--write-redacted",
        action="store_true",
        help="Write redacted response previews to scripts/out/*.redacted.json.",
    )
    return parser.parse_args()


def read_dotenv(path: Path) -> dict[str, str]:
    if not path.exists():
        return {}

    values: dict[str, str] = {}
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line[len("export ") :].lstrip()
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if not key:
            continue
        if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
            value = value[1:-1]
        values[key] = value
    return values


def resolve_token(token_var: str, env_file: Path) -> str:
    env_value = os.environ.get(token_var)
    if env_value and env_value.strip():
        return env_value.strip()

    dotenv_value = read_dotenv(env_file).get(token_var)
    if dotenv_value and dotenv_value.strip():
        return dotenv_value.strip()

    raise SystemExit(
        f"Missing token. Set {token_var} in the process environment or in {env_file}."
    )


def endpoint_url(base_url: str, endpoint: str) -> str:
    if not endpoint.startswith("/"):
        raise SystemExit(f"Endpoint must be a path starting with '/': {endpoint}")
    return urllib.parse.urljoin(base_url.rstrip("/") + "/", endpoint.lstrip("/"))


def is_sensitive_key(key: str) -> bool:
    return bool(SENSITIVE_KEY_RE.search(key))


def is_sensitive_value(value: str) -> bool:
    return bool(EMAIL_RE.search(value) or JWT_RE.match(value) or LONG_SECRET_RE.match(value))


def redact(value: Any, key: str | None = None) -> Any:
    if key is not None and is_sensitive_key(key):
        return "<redacted>"

    if isinstance(value, dict):
        return {item_key: redact(item_value, item_key) for item_key, item_value in value.items()}
    if isinstance(value, list):
        return [redact(item) for item in value]
    if isinstance(value, str):
        if is_sensitive_value(value):
            return "<redacted>"
        return EMAIL_RE.sub("<redacted-email>", value)
    return value


def redact_text(value: str) -> str:
    value = EMAIL_RE.sub("<redacted-email>", value)
    value = re.sub(r"Bearer\s+[A-Za-z0-9_./+=-]+", "Bearer <redacted>", value)
    value = re.sub(r'"([^"]*(?:token|secret|password|email|key|auth|id)[^"]*)"\s*:\s*"[^"]*"',
                   r'"\1": "<redacted>"', value, flags=re.IGNORECASE)
    return value


def type_name(value: Any) -> str:
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "bool"
    if isinstance(value, int):
        return "int"
    if isinstance(value, float):
        return "float"
    if isinstance(value, str):
        return "string"
    if isinstance(value, list):
        return f"array[{len(value)}]"
    if isinstance(value, dict):
        return "object"
    return type(value).__name__


def collect_shape(value: Any, path: str = "$", rows: list[dict[str, str]] | None = None) -> list[dict[str, str]]:
    if rows is None:
        rows = []

    rows.append({"path": path, "type": type_name(value)})
    if isinstance(value, dict):
        for key in sorted(value.keys()):
            collect_shape(value[key], f"{path}.{key}", rows)
    elif isinstance(value, list) and value:
        collect_shape(value[0], f"{path}[0]", rows)
    return rows


def parse_json_body(raw: str | None) -> bytes | None:
    if raw is None:
        return None
    try:
        parsed = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise SystemExit(f"--json-body is not valid JSON: {exc}") from exc
    return json.dumps(parsed, separators=(",", ":")).encode("utf-8")


def request_endpoint(
    url: str,
    token: str,
    method: str,
    body: bytes | None,
    timeout: float,
) -> tuple[int, str | None, bytes]:
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/json",
        "User-Agent": "novelai-bridge-user-api-probe/0.1",
    }
    data = None
    if method == "POST":
        data = body if body is not None else b"{}"
        headers["Content-Type"] = "application/json"

    request = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, response.headers.get("Content-Type"), response.read()
    except urllib.error.HTTPError as exc:
        return exc.code, exc.headers.get("Content-Type"), exc.read()


def summarize_response(endpoint: str, status: int, content_type: str | None, body: bytes) -> dict[str, Any]:
    decoded = body.decode("utf-8", errors="replace")
    result: dict[str, Any] = {
        "endpoint": endpoint,
        "status": status,
        "content_type": content_type,
        "body_bytes": len(body),
        "body_sha256": hashlib.sha256(body).hexdigest(),
    }

    try:
        parsed = json.loads(decoded)
    except json.JSONDecodeError:
        result["json"] = False
        result["redacted_text_preview"] = redact_text(decoded)
        return result

    redacted = redact(parsed)
    result["json"] = True
    result["shape"] = collect_shape(parsed)
    result["redacted_preview"] = redacted
    return result


def safe_output_name(endpoint: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9_.-]+", "_", endpoint.strip("/")) or "root"
    return f"{cleaned}.redacted.json"


def main() -> int:
    args = parse_args()
    root = Path(__file__).resolve().parents[1]
    env_file = Path(args.env_file)
    if not env_file.is_absolute():
        env_file = root / env_file

    token = resolve_token(args.token_var, env_file)
    endpoints = args.endpoint or DEFAULT_ENDPOINTS
    body = parse_json_body(args.json_body)
    out_dir = root / "scripts" / "out"

    for endpoint in endpoints:
        url = endpoint_url(args.base_url, endpoint)
        status, content_type, raw_body = request_endpoint(
            url=url,
            token=token,
            method=args.method,
            body=body,
            timeout=args.timeout,
        )
        summary = summarize_response(endpoint, status, content_type, raw_body)
        rendered = json.dumps(summary, ensure_ascii=False, indent=2)
        if len(rendered) > args.max_preview_chars:
            rendered = rendered[: args.max_preview_chars] + "\n... <truncated>"
        print(rendered)

        if args.write_redacted:
            out_dir.mkdir(parents=True, exist_ok=True)
            output_path = out_dir / safe_output_name(endpoint)
            output_path.write_text(
                json.dumps(summary, ensure_ascii=False, indent=2) + "\n",
                encoding="utf-8",
            )
            print(f"wrote redacted response: {output_path}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
