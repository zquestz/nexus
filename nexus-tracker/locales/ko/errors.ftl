# 트래커 오류 메시지 (v0.1.0)
#
# 모든 키는 `err-tracker-*` 접두사를 사용하여 nexus-server의 로컬라이제이션
# 네임스페이스로부터 격리됩니다. 키는 `nexus-tracker/src/errors.rs`의
# `err_tracker_*` 헬퍼와 1:1로 대응됩니다.

# 인증
err-tracker-unauthorized = 잘못되었거나 누락된 비밀번호

# 필드 검증 (error_kind: invalid)
err-tracker-fingerprint-invalid = 잘못된 인증서 지문 형식
err-tracker-name-too-long = 서버 이름이 너무 깁니다 (최대 { $max_length }자)
err-tracker-name-empty = 서버 이름은 비어 있을 수 없습니다
err-tracker-name-contains-newlines = 서버 이름에 줄 바꿈을 포함할 수 없습니다
err-tracker-name-invalid-characters = 서버 이름에 잘못된 문자가 포함되어 있습니다
err-tracker-description-too-long = 서버 설명이 너무 깁니다 (최대 { $max_length }자)
err-tracker-description-contains-newlines = 서버 설명에 줄 바꿈을 포함할 수 없습니다
err-tracker-description-invalid-characters = 서버 설명에 잘못된 문자가 포함되어 있습니다
err-tracker-password-too-long = 비밀번호가 너무 깁니다 (최대 { $max_length } 바이트)
err-tracker-address-too-long = 주소가 너무 깁니다 (최대 { $max_length } 바이트)
err-tracker-address-invalid = 잘못된 주소
err-tracker-version-too-long = 서버 버전 문자열이 너무 깁니다 (최대 { $max_length } 바이트)
err-tracker-version-invalid = 잘못된 버전 (유효한 semver여야 함)
err-tracker-locale-too-long = 로케일 코드가 너무 깁니다 (최대 { $max_length } 바이트)
err-tracker-locale-invalid = 로케일에 잘못된 문자가 포함되어 있습니다
err-tracker-port-zero = 포트는 0일 수 없습니다
err-tracker-websocket-port-zero = WebSocket 포트는 0일 수 없습니다

# 속도 / 용량
err-tracker-rate-limited = 속도 제한을 초과했습니다; 나중에 다시 시도하세요
err-tracker-capacity = 트래커가 용량에 도달했습니다; 나중에 다시 시도하세요
err-tracker-per-ip-capacity = 이 트래커에 귀하의 IP에서 너무 많은 항목이 있습니다

# 프로토콜 수준
err-tracker-malformed-message = 잘못된 형식의 메시지
err-tracker-handshake-required = 다른 메시지 이전에 핸드셰이크가 필요합니다
err-tracker-role-violation = 이 연결의 역할에 허용되지 않는 메시지
err-tracker-protocol-version-mismatch = 호환되지 않는 트래커 프로토콜 버전 (서버: { $server }, 클라이언트: { $client })
err-tracker-handshake-version-invalid = 잘못된 핸드셰이크 버전 (유효한 semver여야 함)
err-tracker-unknown-message-type = 알 수 없는 메시지 유형

# 프레임 / 전송
err-tracker-frame-error = 프레임 형식 위반
err-tracker-payload-too-large = 페이로드가 메시지 유형별 제한을 초과합니다
