# 認証とセッションのエラー
err-not-logged-in = ログインしていません
err-authentication = 認証エラー
err-invalid-credentials = ユーザー名またはパスワードが無効です
err-handshake-required = ハンドシェイクが必要です
err-already-logged-in = 既にログインしています
err-handshake-already-completed = ハンドシェイクは既に完了しています
err-account-deleted = アカウントが削除されました
err-account-disabled-by-admin = 管理者によってアカウントが無効化されました

# 権限とアクセスのエラー
err-permission-denied = 権限がありません

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

# メッセージ検証のエラー
err-message-empty = メッセージを空にすることはできません

# ユーザー名検証のエラー
err-username-empty = ユーザー名を空にすることはできません
err-username-invalid = ユーザー名に無効な文字が含まれています（文字、数字、記号のみ使用可能 - 空白文字や制御文字は不可）

# 動的エラーメッセージ（パラメータ付き）
err-broadcast-too-long = メッセージが長すぎます（最大{ $max_length }文字）
err-chat-too-long = メッセージが長すぎます（最大{ $max_length }文字）
err-topic-too-long = トピックは{ $max_length }文字を超えることはできません
err-version-mismatch = バージョンの不一致：サーバーは{ $server_version }を使用、クライアントは{ $client_version }を使用
err-kicked-by = { $username }によってキックされました
err-username-exists = ユーザー名「{ $username }」は既に存在します
err-user-not-found = ユーザー「{ $username }」が見つかりません
err-user-not-online = ユーザー「{ $username }」はオンラインではありません
err-failed-to-create-user = ユーザー「{ $username }」の作成に失敗しました
err-account-disabled = アカウント「{ $username }」は無効化されています
err-update-failed = ユーザー「{ $username }」の更新に失敗しました
err-username-too-long = ユーザー名が長すぎます（最大{ $max_length }文字）