# 認証とセッションのエラー
err-not-logged-in = ログインしていません

# ニックネーム検証エラー
err-nickname-empty = ニックネームを空にすることはできません
err-nickname-in-use = ニックネームは既に使用されています
err-nickname-invalid = ニックネームに無効な文字が含まれています（文字、数字、記号は許可 - スペースや制御文字は不可）
err-nickname-is-username = ニックネームは既存のユーザー名にすることはできません
err-nickname-not-found = ユーザー「{ $nickname }」が見つかりません
err-nickname-not-online = ユーザー「{ $nickname }」はオンラインではありません
err-nickname-required = 共有アカウントにはニックネームが必要です
err-nickname-too-long = ニックネームが長すぎます（最大{ $max_length }文字）

# 離席メッセージエラー
err-status-too-long = 離席メッセージが長すぎます（最大{ $max_length }文字）
err-status-contains-newlines = 離席メッセージに改行を含めることはできません
err-status-invalid-characters = 離席メッセージに無効な文字が含まれています

# 共有アカウントエラー
err-shared-cannot-be-admin = 共有アカウントは管理者になれません
err-shared-cannot-change-password = 共有アカウントのパスワードは変更できません
err-shared-invalid-permissions = 共有アカウントはこれらの権限を持つことができません: { $permissions }
err-shared-message-requires-nickname = 共有アカウントにはニックネームでのみメッセージを送信できます
err-shared-kick-requires-nickname = 共有アカウントはニックネームでのみキックできます

# ゲストアカウントエラー
err-guest-disabled = このサーバーではゲストアクセスが有効になっていません
err-cannot-rename-guest = ゲストアカウントの名前は変更できません
err-cannot-change-guest-password = ゲストアカウントのパスワードは変更できません
err-cannot-delete-guest = ゲストアカウントは削除できません

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
err-message-not-supported = サポートされていないメッセージタイプです

# ユーザー管理のエラー
err-cannot-delete-last-admin = 最後の管理者を削除できません
err-cannot-delete-self = 自分自身を削除できません
err-cannot-demote-last-admin = 最後の管理者を降格できません
err-cannot-edit-self = 自分自身を編集できません
err-current-password-required = パスワードを変更するには現在のパスワードが必要です
err-current-password-incorrect = 現在のパスワードが正しくありません
err-cannot-create-admin = 管理者ユーザーを作成できるのは管理者のみです
err-cannot-kick-self = 自分自身をキックできません
err-cannot-kick-admin = 管理者ユーザーをキックできません
err-cannot-delete-admin = 管理者ユーザーを削除できるのは管理者のみです
err-cannot-edit-admin = 管理者ユーザーを編集できるのは管理者のみです
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

err-no-fields-to-update = 更新するフィールドがありません

err-server-image-too-large = サーバー画像が大きすぎます（最大512KB）
err-server-image-invalid-format = サーバー画像の形式が無効です（base64エンコードのデータURIである必要があります）
err-server-image-unsupported-type = サポートされていないサーバー画像タイプです（PNG、WebP、JPEG、SVGのみ）

# ニュースエラー
err-news-not-found = ニュース #{ $id } が見つかりません
err-news-body-too-long = ニュース本文が長すぎます（最大{ $max_length }文字）
err-news-body-invalid-characters = ニュース本文に無効な文字が含まれています
err-news-image-too-large = ニュース画像が大きすぎます（最大512KB）
err-news-image-invalid-format = ニュース画像の形式が無効です（base64エンコードのデータURIである必要があります）
err-news-image-unsupported-type = サポートされていないニュース画像タイプです（PNG、WebP、JPEG、SVGのみ）
err-news-empty-content = ニュースにはテキストまたは画像が必要です
err-cannot-edit-admin-news = 管理者が投稿したニュースを編集できるのは管理者のみです
err-cannot-delete-admin-news = 管理者が投稿したニュースを削除できるのは管理者のみです

# ファイルエリアエラー
err-file-path-too-long = ファイルパスが長すぎます（最大{ $max_length }文字）
err-file-path-invalid = ファイルパスに無効な文字が含まれています
err-file-not-found = ファイルまたはディレクトリが見つかりません
err-file-not-directory = パスはディレクトリではありません
err-dir-name-empty = ディレクトリ名を空にすることはできません
err-dir-name-too-long = ディレクトリ名が長すぎます（最大{ $max_length }文字）
err-dir-name-invalid = ディレクトリ名に無効な文字が含まれています
err-dir-already-exists = その名前のファイルまたはディレクトリは既に存在します
err-dir-create-failed = ディレクトリの作成に失敗しました

err-dir-not-empty = フォルダが空ではありません
err-delete-failed = ファイルまたはフォルダを削除できませんでした
err-rename-failed = ファイルまたはフォルダの名前を変更できませんでした
err-rename-target-exists = その名前のファイルまたはディレクトリは既に存在します
err-move-failed = ファイルまたはフォルダを移動できませんでした
err-copy-failed = ファイルまたはフォルダをコピーできませんでした
err-destination-exists = その名前のファイルまたはディレクトリは移動先に既に存在します
err-cannot-move-into-itself = フォルダを自分自身の中に移動することはできません
err-cannot-copy-into-itself = フォルダを自分自身の中にコピーすることはできません
err-destination-not-directory = 宛先パスはディレクトリではありません

# Transfer Errors
err-file-area-not-configured = ファイルエリアが設定されていません
err-file-area-not-accessible = ファイルエリアにアクセスできません
err-transfer-path-too-long = パスが長すぎます
err-transfer-path-invalid = パスに無効な文字が含まれています
err-transfer-access-denied = アクセスが拒否されました
err-transfer-read-failed = ファイルの読み取りに失敗しました
err-transfer-path-not-found = ファイルまたはディレクトリが見つかりません
err-transfer-file-failed = { $path } の転送に失敗しました: { $error }

# Upload Errors
err-upload-destination-not-allowed = 宛先フォルダはアップロードを許可していません
err-upload-write-failed = ファイルの書き込みに失敗しました
err-upload-hash-mismatch = ファイル検証に失敗しました - ハッシュが一致しません
err-upload-path-invalid = アップロードのファイルパスが無効です
err-upload-conflict = このファイル名への別のアップロードが進行中または中断されています。別のファイル名をお試しください。
err-upload-file-exists = この名前のファイルは既に存在します。別のファイル名を選択するか、管理者に既存のファイルの削除を依頼してください。
err-upload-empty = アップロードには少なくとも1つのファイルが必要です

err-upload-protocol-error = Upload protocol error
err-upload-connection-lost = Connection lost during upload

# Ban System Errors
err-ban-self = 自分自身をBANすることはできません
err-ban-admin-by-nickname = 管理者をBANすることはできません
err-ban-admin-by-ip = このIPをBANすることはできません
err-ban-invalid-target = 無効なターゲット（ニックネーム、IPアドレス、またはCIDR範囲を使用）
err-ban-invalid-duration = 無効な期間形式です（10m、4h、7d、または0で永久）
err-ban-not-found = '{ $target }' のBANが見つかりません
err-reason-too-long = BAN理由が長すぎます（最大{ $max_length }文字）
err-reason-invalid = BAN理由に無効な文字が含まれています
err-banned-permanent = このサーバーからBANされました
err-banned-with-expiry = このサーバーからBANされました（{ $remaining }後に解除）
