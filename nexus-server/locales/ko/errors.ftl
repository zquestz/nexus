# 인증 및 세션 오류
err-not-logged-in = 로그인되지 않음

# 닉네임 유효성 검사 오류
err-nickname-empty = 닉네임은 비워둘 수 없습니다
err-nickname-in-use = 닉네임이 이미 사용 중입니다
err-nickname-invalid = 닉네임에 잘못된 문자가 포함되어 있습니다 (문자, 숫자, 기호 허용 - 공백 또는 제어 문자 불가)
err-nickname-is-username = 닉네임은 기존 사용자 이름이 될 수 없습니다
err-nickname-not-found = 사용자 "{ $nickname }"을(를) 찾을 수 없습니다
err-nickname-not-online = 사용자 "{ $nickname }"이(가) 온라인 상태가 아닙니다
err-nickname-required = 공유 계정에는 닉네임이 필요합니다
err-nickname-too-long = 닉네임이 너무 깁니다 (최대 { $max_length }자)

# 공유 계정 오류
err-shared-cannot-be-admin = 공유 계정은 관리자가 될 수 없습니다
err-shared-cannot-change-password = 공유 계정의 비밀번호는 변경할 수 없습니다
err-shared-invalid-permissions = 공유 계정은 다음 권한을 가질 수 없습니다: { $permissions }
err-shared-message-requires-nickname = 공유 계정은 닉네임으로만 메시지를 받을 수 있습니다
err-shared-kick-requires-nickname = 공유 계정은 닉네임으로만 추방할 수 있습니다

# 게스트 계정 오류
err-guest-disabled = 이 서버에서는 게스트 접속이 활성화되지 않았습니다
err-cannot-rename-guest = 게스트 계정의 이름은 변경할 수 없습니다
err-cannot-change-guest-password = 게스트 계정의 비밀번호는 변경할 수 없습니다
err-cannot-delete-guest = 게스트 계정은 삭제할 수 없습니다

# 아바타 유효성 검사 오류
err-avatar-invalid-format = 아바타 형식이 잘못되었습니다 (base64 인코딩된 데이터 URI여야 합니다)
err-avatar-too-large = 아바타가 너무 큽니다 (최대 { $max_length }자)
err-avatar-unsupported-type = 지원되지 않는 아바타 유형입니다 (PNG, WebP 또는 SVG만 가능)
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
err-current-password-required = 비밀번호를 변경하려면 현재 비밀번호가 필요합니다
err-current-password-incorrect = 현재 비밀번호가 올바르지 않습니다
err-cannot-create-admin = 관리자만 관리자 사용자를 만들 수 있습니다
err-cannot-kick-self = 자기 자신을 추방할 수 없습니다
err-cannot-kick-admin = 관리자 사용자를 추방할 수 없습니다
err-cannot-delete-admin = 관리자만 관리자 사용자를 삭제할 수 있습니다
err-cannot-edit-admin = 관리자만 관리자 사용자를 편집할 수 있습니다
err-cannot-message-self = 자기 자신에게 메시지를 보낼 수 없습니다
err-cannot-disable-last-admin = 마지막 관리자를 비활성화할 수 없습니다

# 채팅 주제 오류
err-topic-contains-newlines = 주제에 줄 바꿈을 포함할 수 없습니다
err-topic-invalid-characters = 주제에 잘못된 문자가 포함되어 있습니다

# 버전 검증 오류
err-version-empty = 버전은 비어 있을 수 없습니다
err-version-too-long = 버전이 너무 깁니다 (최대 { $max_length }자)
err-version-invalid-semver = 버전은 semver 형식이어야 합니다 (MAJOR.MINOR.PATCH)

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
err-version-major-mismatch = 호환되지 않는 프로토콜 버전: 서버는 버전 { $server_major }.x, 클라이언트는 버전 { $client_major }.x입니다
err-version-client-too-new = 클라이언트 버전 { $client_version }이(가) 서버 버전 { $server_version }보다 최신입니다. 서버를 업데이트하거나 이전 클라이언트를 사용하세요.
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

# 서버 업데이트 오류
err-admin-required = 관리자 권한이 필요합니다
err-server-name-empty = 서버 이름은 비어 있을 수 없습니다
err-server-name-too-long = 서버 이름이 너무 깁니다 (최대 { $max_length }자)
err-server-name-contains-newlines = 서버 이름에 줄 바꿈을 포함할 수 없습니다
err-server-name-invalid-characters = 서버 이름에 잘못된 문자가 포함되어 있습니다
err-server-description-too-long = 서버 설명이 너무 깁니다 (최대 { $max_length }자)
err-server-description-contains-newlines = 서버 설명에 줄 바꿈을 포함할 수 없습니다
err-server-description-invalid-characters = 서버 설명에 잘못된 문자가 포함되어 있습니다
err-max-connections-per-ip-invalid = IP당 최대 연결 수는 0보다 커야 합니다
err-no-fields-to-update = 업데이트할 필드가 없습니다

err-server-image-too-large = 서버 이미지가 너무 큽니다 (최대 512KB)
err-server-image-invalid-format = 서버 이미지 형식이 잘못되었습니다 (base64 인코딩된 데이터 URI여야 합니다)
err-server-image-unsupported-type = 지원되지 않는 서버 이미지 유형입니다 (PNG, WebP, JPEG 또는 SVG만 지원)

# 뉴스 오류
err-news-not-found = 뉴스 #{ $id }을(를) 찾을 수 없습니다
err-news-body-too-long = 뉴스 내용이 너무 깁니다 (최대 { $max_length }자)
err-news-body-invalid-characters = 뉴스 내용에 잘못된 문자가 포함되어 있습니다
err-news-image-too-large = 뉴스 이미지가 너무 큽니다 (최대 512KB)
err-news-image-invalid-format = 뉴스 이미지 형식이 잘못되었습니다 (base64 인코딩된 데이터 URI여야 합니다)
err-news-image-unsupported-type = 지원되지 않는 뉴스 이미지 유형입니다 (PNG, WebP, JPEG 또는 SVG만 지원)
err-news-empty-content = 뉴스에는 텍스트 또는 이미지가 있어야 합니다
err-cannot-edit-admin-news = 관리자가 게시한 뉴스는 관리자만 수정할 수 있습니다
err-cannot-delete-admin-news = 관리자가 게시한 뉴스는 관리자만 삭제할 수 있습니다

# 파일 영역 오류
err-file-path-too-long = 파일 경로가 너무 깁니다 (최대 { $max_length }자)
err-file-path-invalid = 파일 경로에 잘못된 문자가 포함되어 있습니다
err-file-not-found = 파일 또는 디렉토리를 찾을 수 없습니다
err-file-not-directory = 경로가 디렉토리가 아닙니다
err-dir-name-empty = 디렉토리 이름은 비워둘 수 없습니다
err-dir-name-too-long = 디렉토리 이름이 너무 깁니다 (최대 { $max_length }자)
err-dir-name-invalid = 디렉토리 이름에 잘못된 문자가 포함되어 있습니다
err-dir-already-exists = 해당 이름의 파일 또는 디렉토리가 이미 존재합니다
err-dir-create-failed = 디렉토리 생성에 실패했습니다

err-dir-not-empty = 폴더가 비어 있지 않습니다
err-delete-failed = 파일 또는 폴더를 삭제할 수 없습니다
err-rename-failed = 파일 또는 폴더의 이름을 변경할 수 없습니다
err-rename-target-exists = 해당 이름의 파일 또는 디렉토리가 이미 존재합니다
