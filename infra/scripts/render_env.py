"""Fill an env template with values from `terraform output -json` (infra/environments/poc).

Usage (from the repo root, AWS credentials for the state bucket in the shell):
    python3 -I infra/scripts/render_env.py api     > .env
    python3 -I infra/scripts/render_env.py worker  > aircloud-worker.env

Only NON-secret keys are filled. Secret keys (API key, AWS key pair, ExternalId, DB password)
always stay empty in the output, so the rendered file can be reviewed before secrets are added.
"""

import json
import subprocess
import sys
from pathlib import Path

INFRA = Path(__file__).resolve().parent.parent
ENV_DIR = INFRA / "env"
POC_DIR = INFRA / "environments" / "poc"

TEMPLATES = {
    "api": ENV_DIR / "api.env.example",
    "worker": ENV_DIR / "aircloud-worker.env.example",
}

# env key -> terraform output name
FROM_OUTPUT = {
    "api": {
        "RAW_BUCKET": "raw_bucket",
        "SAFE_BUCKET": "safe_bucket",
        "JOBS_QUEUE_URL": "jobs_queue_url",
        "RESULTS_QUEUE_URL": "jobs_results_queue_url",
        "DB_HOST": "db_address",
        "DB_NAME": "db_name",
        "DB_SECRET_ARN": "db_master_secret_arn",
        "COGNITO_USER_POOL_ID": "cognito_user_pool_id",
        "COGNITO_CLIENT_ID": "cognito_client_id",
        "COGNITO_ISSUER": "cognito_issuer",
        "AIRCLOUD_SECRET_ARN": "aircloud_secret_arn",
    },
    "worker": {
        "WORKER_ROLE_ARN": "external_gpu_worker_role_arn",
        "RAW_BUCKET": "raw_bucket",
        "SAFE_BUCKET": "safe_bucket",
        "JOBS_QUEUE_URL": "jobs_queue_url",
        "RESULTS_QUEUE_URL": "jobs_results_queue_url",
        "WORKER_IMAGE": "ecr_worker_repository_url",
    },
}

NEVER_FILLED = {
    "AIRCLOUD_API_KEY",
    "AWS_ACCESS_KEY_ID",
    "AWS_SECRET_ACCESS_KEY",
    "WORKER_EXTERNAL_ID",
    "DB_USERNAME",
    "DB_PASSWORD",
}


def terraform_outputs() -> dict[str, object]:
    raw = subprocess.run(
        ["terraform", f"-chdir={POC_DIR}", "output", "-json"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return {name: body["value"] for name, body in json.loads(raw).items()}


def render(kind: str, outputs: dict[str, object]) -> str:
    mapping = FROM_OUTPUT[kind]
    missing = [o for o in mapping.values() if outputs.get(o) in (None, "")]
    if missing:
        sys.exit(f"terraform outputs missing (apply first?): {', '.join(sorted(set(missing)))}")
    lines = []
    for line in TEMPLATES[kind].read_text().splitlines():
        key, sep, _ = line.partition("=")
        if sep and not line.startswith("#") and key in mapping and key not in NEVER_FILLED:
            line = f"{key}={outputs[mapping[key]]}"
        lines.append(line)
    return "\n".join(lines) + "\n"


def main() -> None:
    if len(sys.argv) != 2 or sys.argv[1] not in TEMPLATES:
        sys.exit(f"usage: render_env.py {{{'|'.join(TEMPLATES)}}}")
    sys.stdout.write(render(sys.argv[1], terraform_outputs()))


if __name__ == "__main__":
    main()
