# 인증 및 세션 오류
err-not-logged-in = 로그인되지 않음
err-authentication = 인증 오류
err-invalid-credentials = 잘못된 사용자 이름 또는 비밀번호
err-handshake-required = 핸드셰이크 필요
err-already-logged-in = 이미 로그인됨
err-handshake-already-completed = 핸드셰이크가 이미 완료됨
err-account-deleted = 계정이 삭제되었습니다
err-account-disabled-by-admin = 관리자가 계정을 비활성화했습니다

# 권한 및 액세스 오류
err-permission-denied = 권한이 거부됨

# 기능 오류
err-chat-feature-not-enabled = 채팅 기능이 활성화되지 않았습니다

# 데이터베이스 오류
err-database = 데이터베이스 오류

# 메시지 형식 오류
err-invalid-message-format = 잘못된 메시지 형식

# 사용자 관리 오류
err-cannot-delete-last-admin = 마지막 관리자를 삭제할 수 없습니다
err-cannot-delete-self = 자신을 삭제할 수 없습니다
err-cannot-demote-last-admin = 마지막 관리자를 강등할 수 없습니다
err-cannot-edit-self = 자신을 편집할 수 없습니다
err-cannot-create-admin = 관리자만 관리자 사용자를 만들 수 있습니다
err-cannot-kick-self = 자기 자신을 추방할 수 없습니다
err-cannot-kick-admin = 관리자 사용자를 추방할 수 없습니다
err-cannot-message-self = 자기 자신에게 메시지를 보낼 수 없습니다
err-cannot-disable-last-admin = 마지막 관리자를 비활성화할 수 없습니다

# 채팅 주제 오류
err-topic-contains-newlines = 주제에 줄 바꿈을 포함할 수 없습니다
err-topic-invalid-characters = 주제에 잘못된 문자가 포함되어 있습니다

# 버전 검증 오류
err-version-empty = 버전은 비어 있을 수 없습니다
err-version-too-long = 버전이 너무 깁니다 (최대 { $max_length }자)
err-version-invalid-characters = 버전에 잘못된 문자가 포함되어 있습니다

# 비밀번호 검증 오류
err-password-empty = 비밀번호는 비어 있을 수 없습니다
err-password-too-long = 비밀번호가 너무 깁니다 (최대 { $max_length }자)

# 로케일 검증 오류
err-locale-too-long = 로케일이 너무 깁니다 (최대 { $max_length }자)
err-locale-invalid-characters = 로케일에 잘못된 문자가 포함되어 있습니다

# 기능 검증 오류
err-features-too-many = 기능이 너무 많습니다 (최대 { $max_count })
err-features-empty-feature = 기능 이름은 비어 있을 수 없습니다
err-features-feature-too-long = 기능 이름이 너무 깁니다 (최대 { $max_length }자)
err-features-invalid-characters = 기능 이름에 잘못된 문자가 포함되어 있습니다

# 메시지 검증 오류
err-message-empty = 메시지는 비어 있을 수 없습니다
err-message-contains-newlines = 메시지에 줄 바꿈을 포함할 수 없습니다
err-message-invalid-characters = 메시지에 잘못된 문자가 포함되어 있습니다

# 사용자 이름 검증 오류
err-username-empty = 사용자 이름은 비어 있을 수 없습니다
err-username-invalid = 사용자 이름에 잘못된 문자가 포함되어 있습니다 (문자, 숫자 및 기호 허용 - 공백 또는 제어 문자 불가)

# 알 수 없는 권한 오류
err-unknown-permission = 알 수 없는 권한: '{ $permission }'

# 동적 오류 메시지 (매개변수 포함)
err-broadcast-too-long = 메시지가 너무 깁니다 (최대 { $max_length }자)
err-chat-too-long = 메시지가 너무 깁니다 (최대 { $max_length }자)
err-topic-too-long = 주제는 { $max_length }자를 초과할 수 없습니다
err-version-mismatch = 버전 불일치: 서버는 { $server_version }을 사용하고 클라이언트는 { $client_version }을 사용합니다
err-kicked-by = { $username }에게 추방당했습니다
err-username-exists = 사용자 이름 "{ $username }"이(가) 이미 존재합니다
err-user-not-found = 사용자 "{ $username }"을(를) 찾을 수 없습니다
err-user-not-online = 사용자 "{ $username }"이(가) 온라인 상태가 아닙니다
err-failed-to-create-user = 사용자 "{ $username }"을(를) 생성하지 못했습니다
err-account-disabled = 계정 "{ $username }"이(가) 비활성화되었습니다
err-update-failed = 사용자 "{ $username }"을(를) 업데이트하지 못했습니다
err-username-too-long = 사용자 이름이 너무 깁니다 (최대 { $max_length }자)
# 권한 유효성 검사 오류
err-permissions-too-many = 권한이 너무 많습니다 (최대 { $max_count }개)
err-permissions-empty-permission = 권한 이름은 비워둘 수 없습니다
err-permissions-permission-too-long = 권한 이름이 너무 깁니다 (최대 { $max_length }자)
err-permissions-contains-newlines = 권한 이름에 줄바꿈을 포함할 수 없습니다
err-permissions-invalid-characters = 권한 이름에 잘못된 문자가 포함되어 있습니다
