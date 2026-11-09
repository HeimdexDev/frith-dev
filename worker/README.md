# Frith 얼굴 블러 워커

영상의 얼굴을 검출해 블러 처리하는 파이썬 도구와 SQS 워커 골격입니다. 메시지 계약은 [`infra/docs/gpu-adapter.md`](../infra/docs/gpu-adapter.md)를 따릅니다.

## 구성

- `face_blur/detector.py`: YOLO 얼굴 검출기. 박스 형식은 `[x1, y1, x2, y2, score]`
- `face_blur/pipeline.py`: 프로브, 디코딩, 블러, 인코딩
- `face_blur/cli.py`: 로컬 실행
- `main.py`: SQS 워커
- `tools/get_models.sh`: 가중치 다운로드와 SHA-256 검증
- `tests/`: 단위 테스트

## 기본 동작

- 장치: CLI는 CUDA를 우선 사용하고 없으면 CPU로 폴백
- 워커: `USE_GPU=true`이면 CUDA 필수, CUDA가 없으면 시작 실패
- 인코더: NVENC 세션이 열리면 NVENC, 아니면 libx264 (`runtime.encoder`에 기록)
- 박스 여유: `--padding`, 기본 1.2배
- 2-pass: 전체 프레임을 먼저 검출해 박스만 메모리에 두고, 다시 디코딩하며 블러·인코딩
- 처리 시간: 디코딩을 두 번 하므로 1-pass보다 늘어남
- 박스 채우기: `--hold-frames`, 기본 5프레임씩 첫 검출 이전과 끊김 구간 양쪽을 얼굴별로 채움
- 작은 얼굴: `--min-face-px`, 기본 0이라 제외하지 않음. 워커는 잡의 값을 무시하고 항상 0
- 회전: 회전 메타데이터를 반영해 똑바로 세운 프레임으로 검출·출력. 직각 외 회전은 `UNSUPPORTED_ROTATION`
- 손상 입력: `-xerror`로 디코드 오류 시 실패. 디코드 프레임 수가 길이×프레임 레이트와 `--frame-tolerance`(기본 5%)를 넘게 다르면 `DECODE_ERROR`
- 멈춤 방지: `--probe-timeout`, `--frame-timeout`(기본 각 120초) 초과 시 ffmpeg를 종료하고 `FFMPEG_TIMEOUT`
- 홀수 해상도: 오른쪽·아래를 짝수로 패딩해 인코딩
- 가중치: 로드 전에 SHA-256을 기대값과 비교하고 다르면 시작 중단. 기대값은 `FACE_BLUR_WEIGHTS_SHA256`으로 덮어쓰기 (`infra` 템플릿에는 없는 선택 변수)
- 오디오: 항상 제거
- 출력: mp4 faststart. 완전히 인코딩되고 프레임 수가 맞을 때만 최종 파일 생성
- 미구현(TODO): VFR 정규화
- ffmpeg 경로: 기본은 `PATH`, `FFMPEG_BIN`, `FFPROBE_BIN`으로 덮어쓰기

## 로컬 실행

`worker/`에서 실행합니다. Python 3.11과 ffmpeg가 필요합니다.

- 설치
  ```sh
  python3.11 -m venv .venv
  .venv/bin/pip install -r requirements.txt
  bash tools/get_models.sh
  ```
- 실행
  ```sh
  .venv/bin/python -m face_blur.cli \
    --input input.mp4 \
    --output outputs/blurred.mp4 \
    --weights models/yolov11n-face.pt
  ```
- 산출물
  - `outputs/blurred.mp4`
  - `outputs/blurred.json`: 결과 JSON
  - `outputs/blurred.boxes.jsonl`: 프레임별 박스
- 테스트
  ```sh
  .venv/bin/python -m unittest discover -s tests
  ```

## 에어클라우드 실행 (git pull 후 실행)

GPU 인스턴스에서 Python 3.11, NVENC 지원 ffmpeg, NVIDIA 드라이버가 준비되어 있어야 합니다. 아직 실제 환경에서 검증하지 않았습니다.

- 코드 받기
  ```sh
  git clone https://github.com/HeimdexDev/frith-dev.git
  cd frith-dev
  git switch feat/face-blur
  git pull
  cd worker
  ```
- 설치
  ```sh
  python3.11 -m venv .venv
  .venv/bin/pip install -r requirements.txt
  bash tools/get_models.sh
  ```
- 영상 하나 처리
  ```sh
  .venv/bin/python -m face_blur.cli \
    --input /path/to/test.mp4 \
    --output outputs/blurred.mp4 \
    --weights models/yolov11n-face.pt \
    --device cuda
  ```
- 결과의 `runtime.device`가 `cuda`, `runtime.encoder`가 `h264_nvenc`인지 확인
- SQS 워커 실행
  - `infra/env/aircloud-worker.env.example`의 변수를 Aircloud 시크릿·환경변수로 설정
  - `.venv/bin/python main.py --weights models/yolov11n-face.pt`
- 실제 사람이 나오는 영상은 `git`에 올리지 않기

## 결과 JSON (CLI)

| 필드 | 형식 | 설명 |
|---|---|---|
| `output.path` | 문자열 | 블러 영상의 로컬 경로 |
| `output.sha256` | 64자 소문자 hex | 블러 영상의 SHA-256 |
| `output.bytes` | 정수 | 블러 영상 크기 |
| `model_digest` | 64자 소문자 hex | 가중치 파일의 SHA-256 |
| `pipeline_version` | 문자열 | 파이프라인 버전 |
| `width`, `height` | 정수 | 출력 해상도 |
| `fps_num`, `fps_den` | 정수 | 출력 프레임 레이트의 분자·분모 |
| `duration_ms` | 정수 | 출력 길이(ms) |
| `frame_count` | 정수 | 출력 프레임 수 |
| `processing_seconds` | 실수 | 전체 처리 시간 |
| `detection_params` | 객체 | `conf`, `iou`, `input_long`, `padding`, `hold_frames`, `min_face_px` |
| `frames_total` | 정수 | 검출을 수행한 전체 프레임 수 |
| `frames_with_faces` | 정수 | 검출 박스가 1개 이상인 프레임 수 (채운 박스 제외) |
| `frames_filled` | 정수 | 채운 박스가 1개 이상인 프레임 수 |
| `longest_zero_run_frames` | 정수 | 검출 0건이 연속된 최장 프레임 수 (채운 박스 무관) |
| `no_faces_detected` | 불리언 | 전체 영상에서 검출이 0건인지 여부. 검수 판정이 아님 |
| `runtime` | 객체 | 실제 사용한 `device`, `encoder` |
| `boxes_path` | 문자열 | 프레임별 박스 파일 경로 |

- 검수 판정(`passed`, `approved` 등)은 넣지 않습니다.
- `boxes.jsonl`은 한 줄에 한 프레임입니다: `{"frame": 0, "boxes": [[x1,y1,x2,y2,score]], "filled": [[...]]}`
  - `boxes`: 해당 프레임의 검출 결과
  - `filled`: 앞뒤 채우기로 추가한 박스
  - 좌표는 여유 영역을 적용하기 전의 원본 프레임 픽셀
  - 얼굴 위치 정보이므로 외부 공유 금지

## 결과 메시지 (SQS)

- 계약 필드: `schema_version`, `job_id`, `status`, `retryable`, `output.{bucket,key,sha256,bytes}`, `error`, `worker`, `started_at`, `finished_at`
- 추가 필드: `output.version_id`, `model_digest`, `pipeline_version`
- 검출 통계(`succeeded`일 때만): `frames_total`, `frames_with_faces`, `frames_filled`, `longest_zero_run_frames`, `no_faces_detected`
- 얼굴이 검출되지 않아도 `status`는 `succeeded`로 유지
- 처리 순서: raw 다운로드, SHA-256 검증, 블러, safe 업로드, 결과 전송, 잡 삭제

## 사수 확인 필요 (TODO)

- `original_proxy` 저장 위치 없음: 비교 재생용 원본 프록시를 둘 버킷·권한이 없어 현재 블러 결과만 출력
- 결과 메시지 계약 변경 필요: `output.version_id`, `model_digest`, `pipeline_version`을 추가했으며 API 소비자 합의가 필요
- 워커 role에 `s3:GetObjectVersion` 필요: 현재 `raw.version_id`로 고정하지 못해 최신 객체를 받고 SHA-256으로 검증
- 이미지 배포 불가 상태: OIDC가 feature 브랜치를 허용하지 않고, `deploy-worker.yml`의 Trivy 버전이 자리표시자
- 용어 충돌: `face_status: ready`(검수 완료)와 pair manifest의 `status: ready`(처리 완료로 읽힘)
- Ultralytics AGPL 확인: 레포 라이선스 `Proprietary`와의 호환성, 가중치 라이선스 포함
- 결과 고정 (리뷰 9번): API가 `version_id`로 미리보기·검수를 묶는 방안, `s3:GetObjectVersion` 권한
- 키 규칙 (리뷰 11번): `key_prefix` 형식과 조직 일치 검증, job_id를 UUID로 제한하고 재사용 금지
- 검수 게이트 (리뷰 2, 14번): 검출 0건 영상의 처리 정책, 결과 메시지의 통계·파라미터 필드 합의
- `min_face_px` 의미 (리뷰 4번): 계약 예시값 24의 유지 여부. 현재 워커는 잡 값을 무시
- 정확도 기준 (리뷰 6번, 낮음): 재현율 목표, 4K 처리 방식, `conf`, 블러 강도 평가
- 운영 정책 (리뷰 7, 8, 10번): 작업 시간·재전달 상한, heartbeat 실패 시 동작, 비정상 종료 시 raw 삭제 보장
- 타임라인 (리뷰 12번): VFR 처리 방식

## 구현상 남은 항목

- `worker.region`은 현재 `null`: 환경변수 템플릿에 Aircloud 리전 값이 없음
- `boxes.jsonl`은 워커 모드에서 업로드하지 않고 스크래치와 함께 삭제: 저장 위치 계약 없음
- 5 GiB 초과 출력은 `OUTPUT_TOO_LARGE`로 실패: 멀티파트 업로드 미구현
- 재전달로 재처리되면 같은 키를 덮어쓰지만 API는 첫 결과만 반영: 프로세스가 재시작되면 `sha256`이 실제 최신 객체와 달라질 수 있어 `version_id` 기준 조회 필요
- 프로세스 비정상 종료 시 스크래치가 남을 수 있음: 시작 시 정리 미구현
- 실제 AWS(SQS, S3, AssumeRole) 연동, CUDA, NVENC, Docker 빌드 미검증
