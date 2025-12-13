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
button-close = 닫기
button-choose-avatar = 아바타 선택
button-clear-avatar = 지우기

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
title-edit-server-info = 서버 정보 편집
title-fingerprint-mismatch = 인증서 지문이 일치하지 않습니다!
title-server-info = 서버 정보
title-user-info = 사용자 정보
title-about = 정보

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
placeholder-server-description = 서버 설명

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
label-chat-font-size = 글꼴 크기:
label-show-connection-notifications = 연결 알림 표시
label-show-timestamps = 타임스탬프 표시
label-use-24-hour-time = 24시간 형식 사용
label-show-seconds = 초 표시
label-server-name = 이름:
label-server-description = 설명:
label-server-version = 버전:
label-chat-topic = 채팅 주제:
label-chat-topic-set-by = 주제 설정자:
label-max-connections-per-ip = IP당 최대 연결 수:
label-avatar = 아바타:
label-details = 기술 세부 정보
label-chat-options = 채팅 옵션
label-appearance = 외관
label-image = 이미지
label-general = 일반
label-limits = 제한

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
tooltip-server-info = 서버 정보
tooltip-about = 정보
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
msg-server-info-updated = 서버 설정이 업데이트되었습니다
msg-topic-display = 주제: { $topic }
msg-user-connected = { $username }님이 연결되었습니다
msg-user-disconnected = { $username }님의 연결이 해제되었습니다
msg-disconnected = 연결 해제됨: { $error }
msg-connection-cancelled = 인증서 불일치로 연결이 취소되었습니다

# =============================================================================
# Error Messages
# =============================================================================

err-connection-broken = 연결 오류
err-failed-update-server-info = 서버 정보 업데이트 실패: { $error }
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
err-message-too-long = 메시지가 너무 깁니다 ({ $length }자, 최대 { $max }자)
err-send-failed = 메시지 전송 실패
err-no-chat-permission = 메시지를 보낼 권한이 없습니다
err-broadcast-too-long = 브로드캐스트가 너무 깁니다 ({ $length }자, 최대 { $max }자)
err-broadcast-send-failed = 브로드캐스트 전송 실패
err-name-required = 북마크 이름은 필수입니다
err-address-required = 서버 주소는 필수입니다
err-port-required = 포트는 필수입니다
err-username-required = 사용자 이름은 필수입니다
err-password-required = 비밀번호는 필수입니다
err-message-required = 메시지는 필수입니다

# Validation errors
err-message-empty = 메시지는 비워둘 수 없습니다
err-message-contains-newlines = 메시지에 줄바꿈을 포함할 수 없습니다
err-message-invalid-characters = 메시지에 잘못된 문자가 포함되어 있습니다
err-username-empty = 사용자 이름은 비워둘 수 없습니다
err-username-too-long = 사용자 이름이 너무 깁니다 (최대 { $max }자)
err-username-invalid = 사용자 이름에 잘못된 문자가 포함되어 있습니다
err-password-too-long = 비밀번호가 너무 깁니다 (최대 { $max }자)
err-topic-too-long = 주제가 너무 깁니다 ({ $length }자, 최대 { $max }자)
err-avatar-unsupported-type = 지원되지 않는 파일 형식입니다. PNG, WebP, JPEG 또는 SVG를 사용하세요.
err-avatar-too-large = 아바타가 너무 큽니다. 최대 크기는 { $max_kb }KB입니다.
err-avatar-decode-failed = 아바타를 디코딩할 수 없습니다. 파일이 손상되었을 수 있습니다.
err-server-name-empty = 서버 이름은 비워둘 수 없습니다
err-server-name-too-long = 서버 이름이 너무 깁니다 (최대 { $max }자)
err-server-name-contains-newlines = 서버 이름에 줄바꿈을 포함할 수 없습니다
err-server-name-invalid-characters = 서버 이름에 잘못된 문자가 포함되어 있습니다
err-server-description-too-long = 설명이 너무 깁니다 (최대 { $max }자)
err-server-description-contains-newlines = 설명에 줄바꿈을 포함할 수 없습니다
err-server-description-invalid-characters = 설명에 잘못된 문자가 포함되어 있습니다
err-failed-send-update = 업데이트 전송 실패: { $error }

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

user-info-username = 사용자명:
user-info-role = 역할:
user-info-role-admin = 관리자
user-info-role-user = 사용자
user-info-connected = 연결됨:
user-info-connected-value = { $duration } 전
user-info-connected-value-sessions = { $duration } 전 ({ $count }개 세션)
user-info-features = 기능:
user-info-features-value = { $features }
user-info-features-none = 없음
user-info-locale = 언어:
user-info-address = 주소:
user-info-addresses = 주소:
user-info-created = 생성일:
user-info-end = 사용자 정보 끝
user-info-unknown = 알 수 없음
user-info-loading = 사용자 정보 로딩 중...

# =============================================================================
# Time Duration
# =============================================================================

time-days = { $count }일
time-hours = { $count }시간
time-minutes = { $count }분
time-seconds = { $count }초

# =============================================================================
# Command System
# =============================================================================

cmd-unknown = 알 수 없는 명령어: /{ $command }
cmd-help-header = 사용 가능한 명령어:
cmd-help-desc = 사용 가능한 명령어 표시
cmd-help-escape-hint = 팁: /로 시작하는 메시지를 보내려면 //를 사용하세요
cmd-message-desc = 사용자에게 메시지 보내기
cmd-message-usage = 사용법: /{ $command } <사용자명> <메시지>
cmd-userinfo-desc = 사용자 정보 표시
cmd-userinfo-usage = 사용법: /{ $command } <사용자명>
cmd-kick-desc = 서버에서 사용자 추방
cmd-kick-usage = 사용법: /{ $command } <사용자명>
cmd-topic-desc = 채팅 주제 보기 또는 관리
cmd-topic-usage = 사용법: /{ $command } [설정|지우기] [주제]
cmd-topic-arg-set = 설정
cmd-topic-arg-clear = 지우기
cmd-topic-set-usage = 사용법: /{ $command } 설정 <주제>
cmd-topic-none = 설정된 주제가 없습니다
cmd-broadcast-desc = 모든 사용자에게 공지 보내기
cmd-broadcast-usage = 사용법: /{ $command } <메시지>
cmd-clear-desc = 현재 탭의 채팅 기록 지우기
cmd-clear-usage = 사용법: /{ $command }
cmd-focus-desc = 서버 채팅 또는 사용자 메시지 창에 포커스
cmd-focus-usage = 사용법: /{ $command } [사용자명]
cmd-focus-not-found = 사용자를 찾을 수 없습니다: { $name }
cmd-list-desc = 접속 중/전체 사용자 표시
cmd-list-arg-all = 전체
cmd-list-usage = 사용법: /{ $command } [전체]
cmd-list-empty = 접속 중인 사용자가 없습니다
cmd-list-output = 온라인 사용자: { $users } ({ $count }명)
cmd-list-all-no-permission = 전체 사용자를 보려면 user_edit 또는 user_delete 권한이 필요합니다
cmd-list-all-output = 사용자: { $users } ({ $count }명)
cmd-help-usage = 사용법: /{ $command } [명령어]
cmd-topic-permission-denied = 주제를 편집할 권한이 없습니다
cmd-window-desc = 채팅 탭 관리
cmd-window-usage = 사용법: /{ $command } [다음|이전|닫기 [사용자명]]
cmd-window-arg-next = 다음
cmd-window-arg-prev = 이전
cmd-window-arg-close = 닫기
cmd-window-list = 열린 탭: { $tabs } ({ $count }개 탭)
cmd-window-close-server = 서버 탭은 닫을 수 없습니다
cmd-window-not-found = 탭을 찾을 수 없습니다: { $name }
cmd-serverinfo-desc = 서버 정보 표시
cmd-serverinfo-usage = 사용법: /{ $command }
cmd-serverinfo-header = [서버]
cmd-serverinfo-end = 서버 정보 끝

# =============================================================================
# About Panel
# =============================================================================

about-app-name = Nexus BBS
about-copyright = © 2025 Nexus BBS Project
button-choose-image = 이미지 선택
button-clear-image = 지우기
label-server-image = 서버 이미지:
err-server-image-too-large = 서버 이미지가 너무 큽니다 (최대 512KB)
err-server-image-invalid-format = 서버 이미지 형식이 잘못되었습니다 (base64 인코딩된 데이터 URI여야 합니다)
err-server-image-unsupported-type = 지원되지 않는 서버 이미지 유형입니다 (PNG, WebP, JPEG 또는 SVG만 지원)
err-server-image-decode-failed = 이미지를 디코딩할 수 없습니다. 파일이 손상되었을 수 있습니다.
err-failed-read-image = 이미지 읽기 실패: { $error }
