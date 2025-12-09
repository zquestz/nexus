# 認証とセッションのエラー
err-not-logged-in = ログインしていません

# アバター検証エラー
err-avatar-invalid-format = アバター形式が無効です（base64エンコードのデータURIである必要があります）
err-avatar-too-large = アバターが大きすぎます（最大{ $max_length }文字）
err-avatar-unsupported-type = サポートされていないアバタータイプです（PNG、WebP、SVGのみ）
err-authentication = 認証エラー
err-invalid-credentials = ユーザー名またはパスワードが無効です
err-handshake-required = ハンドシェイクが必要です
err-already-logged-in = 既にログインしています
err-handshake-already-completed = ハンドシェイクは既に完了しています
err-account-deleted = アカウントが削除されました
err-account-disabled-by-admin = 管理者によってアカウントが無効化されました

# 権限とアクセスのエラー
err-permission-denied = 権限がありません

# 機能エラー
err-chat-feature-not-enabled = チャット機能が有効になっていません

# データベースエラー
err-database = データベースエラー

# メッセージ形式のエラー
err-invalid-message-format = 無効なメッセージ形式です

# ユーザー管理のエラー
err-cannot-delete-last-admin = 最後の管理者を削除できません
err-cannot-delete-self = 自分自身を削除できません
err-cannot-demote-last-admin = 最後の管理者を降格できません
err-cannot-edit-self = 自分自身を編集できません
err-cannot-create-admin = 管理者ユーザーを作成できるのは管理者のみです
err-cannot-kick-self = 自分自身をキックできません
err-cannot-kick-admin = 管理者ユーザーをキックできません
err-cannot-message-self = 自分自身にメッセージを送ることはできません
err-cannot-disable-last-admin = 最後の管理者を無効化できません

# チャットトピックのエラー
err-topic-contains-newlines = トピックに改行を含めることはできません
err-topic-invalid-characters = トピックに無効な文字が含まれています

# バージョン検証のエラー
err-version-empty = バージョンを空にすることはできません
err-version-too-long = バージョンが長すぎます（最大{ $max_length }文字）
err-version-invalid-semver = バージョンはsemver形式（MAJOR.MINOR.PATCH）である必要があります

# パスワード検証のエラー
err-password-empty = パスワードを空にすることはできません
err-password-too-long = パスワードが長すぎます（最大{ $max_length }文字）

# ロケール検証のエラー
err-locale-too-long = ロケールが長すぎます（最大{ $max_length }文字）
err-locale-invalid-characters = ロケールに無効な文字が含まれています

# 機能検証のエラー
err-features-too-many = 機能が多すぎます（最大{ $max_count }）
err-features-empty-feature = 機能名を空にすることはできません
err-features-feature-too-long = 機能名が長すぎます（最大{ $max_length }文字）
err-features-invalid-characters = 機能名に無効な文字が含まれています

# メッセージ検証のエラー
err-message-empty = メッセージを空にすることはできません
err-message-contains-newlines = メッセージに改行を含めることはできません
err-message-invalid-characters = メッセージに無効な文字が含まれています

# ユーザー名検証のエラー
err-username-empty = ユーザー名を空にすることはできません
err-username-invalid = ユーザー名に無効な文字が含まれています（文字、数字、記号のみ使用可能 - 空白文字や制御文字は不可）

# 不明な権限エラー
err-unknown-permission = 不明な権限: '{ $permission }'

# 動的エラーメッセージ（パラメータ付き）
err-broadcast-too-long = メッセージが長すぎます（最大{ $max_length }文字）
err-chat-too-long = メッセージが長すぎます（最大{ $max_length }文字）
err-topic-too-long = トピックは{ $max_length }文字を超えることはできません
err-version-major-mismatch = 互換性のないプロトコルバージョン：サーバーはバージョン{ $server_major }.x、クライアントはバージョン{ $client_major }.x
err-version-client-too-new = クライアントバージョン{ $client_version }はサーバーバージョン{ $server_version }より新しいです。サーバーを更新するか、古いクライアントを使用してください。
err-kicked-by = { $username }によってキックされました
err-username-exists = ユーザー名「{ $username }」は既に存在します
err-user-not-found = ユーザー「{ $username }」が見つかりません
err-user-not-online = ユーザー「{ $username }」はオンラインではありません
err-failed-to-create-user = ユーザー「{ $username }」の作成に失敗しました
err-account-disabled = アカウント「{ $username }」は無効化されています
err-update-failed = ユーザー「{ $username }」の更新に失敗しました
err-username-too-long = ユーザー名が長すぎます（最大{ $max_length }文字）
# 権限バリデーションエラー
err-permissions-too-many = 権限が多すぎます（最大{ $max_count }個）
err-permissions-empty-permission = 権限名を空にすることはできません
err-permissions-permission-too-long = 権限名が長すぎます（最大{ $max_length }文字）
err-permissions-contains-newlines = 権限名に改行を含めることはできません
err-permissions-invalid-characters = 権限名に無効な文字が含まれています

# サーバー更新エラー
err-admin-required = 管理者権限が必要です
err-server-name-empty = サーバー名を空にすることはできません
err-server-name-too-long = サーバー名が長すぎます（最大{ $max_length }文字）
err-server-name-contains-newlines = サーバー名に改行を含めることはできません
err-server-name-invalid-characters = サーバー名に無効な文字が含まれています
err-server-description-too-long = サーバーの説明が長すぎます（最大{ $max_length }文字）
err-server-description-contains-newlines = サーバーの説明に改行を含めることはできません
err-server-description-invalid-characters = サーバーの説明に無効な文字が含まれています
err-max-connections-per-ip-invalid = IPあたりの最大接続数は0より大きくなければなりません
err-no-fields-to-update = 更新するフィールドがありません


