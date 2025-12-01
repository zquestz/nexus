# Nexus BBS Client - Korean Translations

# =============================================================================
# Buttons
# =============================================================================

button-cancel = 취소
button-send = 보내기
button-delete = 삭제
button-connect = 연결
button-save = 저장
button-create = 생성
button-edit = 편집
button-update = 업데이트
button-accept-new-certificate = 새 인증서 수락

# =============================================================================
# Titles
# =============================================================================

title-nexus-bbs = Nexus BBS
title-connect-to-server = 서버에 연결
title-add-bookmark = 북마크 추가
title-edit-server = 서버 편집
title-broadcast-message = 브로드캐스트 메시지
title-user-create = 사용자 생성
title-user-edit = 사용자 편집
title-update-user = 사용자 업데이트
title-connected = 연결됨
title-settings = 설정
title-bookmarks = 북마크
title-users = 사용자
title-fingerprint-mismatch = 인증서 지문이 일치하지 않습니다!

# =============================================================================
# Placeholders
# =============================================================================

placeholder-username = 사용자 이름
placeholder-password = 비밀번호
placeholder-port = 포트
placeholder-server-address = 서버 주소
placeholder-server-name = 서버 이름
placeholder-username-optional = 사용자 이름 (선택)
placeholder-password-optional = 비밀번호 (선택)
placeholder-password-keep-current = 비밀번호 (현재 유지하려면 비워두세요)
placeholder-message = 메시지를 입력하세요...
placeholder-no-permission = 권한 없음
placeholder-broadcast-message = 브로드캐스트 메시지를 입력하세요...

# =============================================================================
# Labels
# =============================================================================

label-auto-connect = 자동 연결
label-add-bookmark = 북마크 추가
label-admin = 관리자
label-enabled = 활성화
label-permissions = 권한:
label-expected-fingerprint = 예상 지문:
label-received-fingerprint = 수신된 지문:
label-theme = 테마
label-show-connection-notifications = 연결 알림 표시

# =============================================================================
# Permission Display Names
# =============================================================================

permission-user_list = 사용자 목록
permission-user_info = 사용자 정보
permission-chat_send = 채팅 전송
permission-chat_receive = 채팅 수신
permission-chat_topic = 채팅 주제
permission-chat_topic_edit = 채팅 주제 편집
permission-user_broadcast = 사용자 브로드캐스트
permission-user_create = 사용자 생성
permission-user_delete = 사용자 삭제
permission-user_edit = 사용자 편집
permission-user_kick = 사용자 추방
permission-user_message = 사용자 메시지

# =============================================================================
# Tooltips
# =============================================================================

tooltip-chat = 채팅
tooltip-broadcast = 브로드캐스트
tooltip-user-create = 사용자 생성
tooltip-user-edit = 사용자 편집
tooltip-settings = 설정
tooltip-hide-bookmarks = 북마크 숨기기
tooltip-show-bookmarks = 북마크 표시
tooltip-hide-user-list = 사용자 목록 숨기기
tooltip-show-user-list = 사용자 목록 표시
tooltip-disconnect = 연결 해제
tooltip-edit = 편집
tooltip-info = 정보
tooltip-message = 메시지
tooltip-kick = 추방
tooltip-close = 닫기
tooltip-add-bookmark = 북마크 추가

# =============================================================================
# Empty States
# =============================================================================

empty-select-server = 목록에서 서버를 선택하세요
empty-no-connections = 연결 없음
empty-no-bookmarks = 북마크 없음
empty-no-users = 온라인 사용자 없음

# =============================================================================
# Chat Tab Labels
# =============================================================================

chat-tab-server = #서버

# =============================================================================
# System Message Usernames
# =============================================================================


# =============================================================================
# Chat Message Prefixes
# =============================================================================

chat-prefix-system = [시스템]
chat-prefix-error = [오류]
chat-prefix-info = [정보]
chat-prefix-broadcast = [BROADCAST]

# =============================================================================
# Success Messages
# =============================================================================

msg-user-kicked-success = 사용자가 성공적으로 추방되었습니다
msg-broadcast-sent = 브로드캐스트가 성공적으로 전송되었습니다
msg-user-created = 사용자가 성공적으로 생성되었습니다
msg-user-deleted = 사용자가 성공적으로 삭제되었습니다
msg-user-updated = 사용자가 성공적으로 업데이트되었습니다
msg-permissions-updated = 권한이 업데이트되었습니다
msg-topic-updated = 주제가 성공적으로 업데이트되었습니다

# =============================================================================
# Dynamic Messages (with parameters)
# =============================================================================

msg-topic-cleared = { $username }님이 주제를 삭제했습니다
msg-topic-set = { $username }님이 주제를 설정했습니다: { $topic }
msg-topic-display = 주제: { $topic }
msg-user-connected = { $username }님이 연결되었습니다
msg-user-disconnected = { $username }님의 연결이 해제되었습니다
msg-disconnected = 연결 해제됨: { $error }
msg-connection-cancelled = 인증서 불일치로 연결이 취소되었습니다

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = 연결 오류
err-user-kick-failed = 사용자 추방 실패
err-no-shutdown-handle = 연결 오류: 종료 핸들 없음
err-userlist-failed = 사용자 목록 새로고침 실패
err-port-invalid = 포트는 유효한 숫자여야 합니다 (1-65535)

# Network connection errors
err-no-peer-certificates = 서버 인증서를 찾을 수 없습니다
err-no-certificates-in-chain = 체인에 인증서가 없습니다
err-unexpected-handshake-response = 예기치 않은 핸드셰이크 응답
err-no-session-id = 세션 ID를 받지 못했습니다
err-login-failed = 로그인 실패
err-unexpected-login-response = 예기치 않은 로그인 응답
err-connection-closed = 연결이 종료되었습니다
err-could-not-determine-config-dir = 설정 디렉토리를 확인할 수 없습니다
err-message-too-long = 메시지가 너무 깁니다
err-send-failed = 메시지 전송 실패
err-broadcast-too-long = 브로드캐스트 메시지가 너무 깁니다
err-broadcast-send-failed = 브로드캐스트 전송 실패
err-name-required = 북마크 이름은 필수입니다
err-address-required = 서버 주소는 필수입니다
err-port-required = 포트는 필수입니다
err-username-required = 사용자 이름은 필수입니다
err-password-required = 비밀번호는 필수입니다
err-message-required = 메시지는 필수입니다

# =============================================================================
# Dynamic Error Messages (with parameters)
# =============================================================================

err-failed-save-config = 설정 저장 실패: { $error }
err-failed-save-settings = 설정 저장 실패: { $error }
err-invalid-port-bookmark = 북마크의 포트가 잘못되었습니다: { $name }
err-failed-send-broadcast = 브로드캐스트 전송 실패: { $error }
err-failed-send-message = 메시지 전송 실패: { $error }
err-failed-create-user = 사용자 생성 실패: { $error }
err-failed-delete-user = 사용자 삭제 실패: { $error }
err-failed-update-user = 사용자 업데이트 실패: { $error }
err-failed-update-topic = 주제 업데이트 실패: { $error }
err-message-too-long-details = { $error } ({ $length }자, 최대 { $max })

# Network connection errors (with parameters)
err-invalid-address = 잘못된 주소 '{ $address }': { $error }
err-could-not-resolve = 주소 '{ $address }'를 확인할 수 없습니다
err-connection-timeout = { $seconds }초 후 연결 시간 초과
err-connection-failed = 연결 실패: { $error }
err-tls-handshake-failed = TLS 핸드셰이크 실패: { $error }
err-failed-send-handshake = 핸드셰이크 전송 실패: { $error }
err-failed-read-handshake = 핸드셰이크 응답 읽기 실패: { $error }
err-handshake-failed = 핸드셰이크 실패: { $error }
err-failed-parse-handshake = 핸드셰이크 응답 구문 분석 실패: { $error }
err-failed-send-login = 로그인 전송 실패: { $error }
err-failed-read-login = 로그인 응답 읽기 실패: { $error }
err-failed-parse-login = 로그인 응답 구문 분석 실패: { $error }
err-failed-create-server-name = 서버 이름 생성 실패: { $error }
err-failed-create-config-dir = 설정 디렉토리 생성 실패: { $error }
err-failed-serialize-config = 설정 직렬화 실패: { $error }
err-failed-write-config = 설정 파일 쓰기 실패: { $error }
err-failed-read-config-metadata = 설정 파일 메타데이터 읽기 실패: { $error }
err-failed-set-config-permissions = 설정 파일 권한 설정 실패: { $error }

# =============================================================================
# Fingerprint Warning
# =============================================================================

fingerprint-warning = 이는 보안 문제(MITM 공격)를 나타내거나 서버 인증서가 재생성되었을 수 있습니다. 서버 관리자를 신뢰하는 경우에만 수락하세요.

# =============================================================================
# User Info Display
# =============================================================================

user-info-header = [{ $username }]
user-info-is-admin = 관리자입니다
user-info-connected-ago = 연결됨: { $duration } 전
user-info-connected-sessions = 연결됨: { $duration } 전 ({ $count }개 세션)
user-info-features = 기능: { $features }
user-info-locale = 언어: { $locale }
user-info-address = 주소: { $address }
user-info-addresses = 주소:
user-info-address-item = - { $address }
user-info-created = 생성일: { $created }
user-info-end = 사용자 정보 끝
user-info-unknown = 알 수 없음
user-info-error = 오류: { $error }

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count }일
time-hours = { $count }시간
time-minutes = { $count }분
time-seconds = { $count }초