# Ошибки аутентификации и сеанса
err-not-logged-in = Не выполнен вход

# Ошибки валидации псевдонима
err-nickname-empty = Псевдоним не может быть пустым
err-nickname-in-use = Псевдоним уже используется
err-nickname-invalid = Псевдоним содержит недопустимые символы (разрешены буквы, цифры и символы - без пробелов и управляющих символов)
err-nickname-is-username = Псевдоним не может совпадать с существующим именем пользователя
err-nickname-not-found = Пользователь "{ $nickname }" не найден
err-nickname-not-online = Пользователь "{ $nickname }" не в сети
err-nickname-required = Псевдоним обязателен для общих учетных записей
err-nickname-too-long = Псевдоним слишком длинный (макс. { $max_length } символов)

# Ошибки общих учетных записей
err-shared-cannot-be-admin = Общие учетные записи не могут быть администраторами
err-shared-cannot-change-password = Невозможно изменить пароль общей учетной записи
err-shared-invalid-permissions = Общие учетные записи не могут иметь эти разрешения: { $permissions }
err-shared-message-requires-nickname = Общим учетным записям можно отправлять сообщения только по никнейму
err-shared-kick-requires-nickname = Общие учетные записи можно кикнуть только по никнейму

# Ошибки гостевой учетной записи
err-guest-disabled = Гостевой доступ не включен на этом сервере
err-cannot-rename-guest = Гостевую учетную запись нельзя переименовать
err-cannot-change-guest-password = Пароль гостевой учетной записи нельзя изменить
err-cannot-delete-guest = Гостевую учетную запись нельзя удалить

# Ошибки валидации аватара
err-avatar-invalid-format = Недопустимый формат аватара (должен быть data URI с кодировкой base64)
err-avatar-too-large = Аватар слишком большой (макс. { $max_length } символов)
err-avatar-unsupported-type = Неподдерживаемый тип аватара (только PNG, WebP или SVG)
err-authentication = Ошибка аутентификации
err-invalid-credentials = Неверное имя пользователя или пароль
err-handshake-required = Требуется рукопожатие
err-already-logged-in = Вы уже вошли в систему
err-handshake-already-completed = Рукопожатие уже выполнено
err-account-deleted = Ваша учетная запись удалена
err-account-disabled-by-admin = Учетная запись отключена администратором

# Ошибки прав доступа
err-permission-denied = Доступ запрещен

# Ошибки функций
err-chat-feature-not-enabled = Функция чата не включена

# Ошибки базы данных
err-database = Ошибка базы данных

# Ошибки формата сообщения
err-invalid-message-format = Неверный формат сообщения
err-message-not-supported = Тип сообщения не поддерживается

# Ошибки управления пользователями
err-cannot-delete-last-admin = Невозможно удалить последнего администратора
err-cannot-delete-self = Вы не можете удалить себя
err-cannot-demote-last-admin = Невозможно понизить последнего администратора
err-cannot-edit-self = Вы не можете редактировать себя
err-current-password-required = Для изменения пароля требуется текущий пароль
err-current-password-incorrect = Текущий пароль неверен
err-cannot-create-admin = Только администраторы могут создавать пользователей-администраторов
err-cannot-kick-self = Вы не можете выгнать себя
err-cannot-kick-admin = Невозможно выгнать пользователей-администраторов
err-cannot-delete-admin = Только администраторы могут удалять пользователей-администраторов
err-cannot-edit-admin = Только администраторы могут редактировать пользователей-администраторов
err-cannot-message-self = Вы не можете отправить сообщение себе
err-cannot-disable-last-admin = Невозможно отключить последнего администратора

# Ошибки темы чата
err-topic-contains-newlines = Тема не может содержать переносы строк
err-topic-invalid-characters = Тема содержит недопустимые символы

# Ошибки проверки версии
err-version-empty = Версия не может быть пустой
err-version-too-long = Версия слишком длинная (максимум { $max_length } символов)
err-version-invalid-semver = Версия должна быть в формате semver (MAJOR.MINOR.PATCH)

# Ошибки проверки пароля
err-password-empty = Пароль не может быть пустым
err-password-too-long = Пароль слишком длинный (максимум { $max_length } символов)

# Ошибки проверки локали
err-locale-too-long = Локаль слишком длинная (максимум { $max_length } символов)
err-locale-invalid-characters = Локаль содержит недопустимые символы

# Ошибки проверки функций
err-features-too-many = Слишком много функций (максимум { $max_count })
err-features-empty-feature = Название функции не может быть пустым
err-features-feature-too-long = Название функции слишком длинное (максимум { $max_length } символов)
err-features-invalid-characters = Название функции содержит недопустимые символы

# Ошибки проверки сообщений
err-message-empty = Сообщение не может быть пустым
err-message-contains-newlines = Сообщение не может содержать переносы строк
err-message-invalid-characters = Сообщение содержит недопустимые символы

# Ошибки проверки имени пользователя
err-username-empty = Имя пользователя не может быть пустым
err-username-invalid = Имя пользователя содержит недопустимые символы (разрешены буквы, цифры и символы - без пробелов и управляющих символов)

# Ошибка неизвестного разрешения
err-unknown-permission = Неизвестное разрешение: '{ $permission }'

# Динамические сообщения об ошибках (с параметрами)
err-broadcast-too-long = Сообщение слишком длинное (максимум { $max_length } символов)
err-chat-too-long = Сообщение слишком длинное (максимум { $max_length } символов)
err-topic-too-long = Тема не может превышать { $max_length } символов
err-version-major-mismatch = Несовместимая версия протокола: сервер версии { $server_major }.x, клиент версии { $client_major }.x
err-version-client-too-new = Версия клиента { $client_version } новее версии сервера { $server_version }. Пожалуйста, обновите сервер или используйте более старый клиент.
err-kicked-by = Вы были выгнаны пользователем { $username }
err-username-exists = Имя пользователя "{ $username }" уже существует
err-user-not-found = Пользователь "{ $username }" не найден
err-user-not-online = Пользователь "{ $username }" не в сети
err-failed-to-create-user = Не удалось создать пользователя "{ $username }"
err-account-disabled = Учетная запись "{ $username }" отключена
err-update-failed = Не удалось обновить пользователя "{ $username }"
err-username-too-long = Имя пользователя слишком длинное (максимум { $max_length } символов)
# Ошибки валидации разрешений
err-permissions-too-many = Слишком много разрешений (максимум { $max_count })
err-permissions-empty-permission = Название разрешения не может быть пустым
err-permissions-permission-too-long = Название разрешения слишком длинное (максимум { $max_length } символов)
err-permissions-contains-newlines = Название разрешения не может содержать переносы строк
err-permissions-invalid-characters = Название разрешения содержит недопустимые символы

# Ошибки обновления сервера
err-admin-required = Требуются права администратора
err-server-name-empty = Имя сервера не может быть пустым
err-server-name-too-long = Имя сервера слишком длинное (максимум { $max_length } символов)
err-server-name-contains-newlines = Имя сервера не может содержать переносы строк
err-server-name-invalid-characters = Имя сервера содержит недопустимые символы
err-server-description-too-long = Описание сервера слишком длинное (максимум { $max_length } символов)
err-server-description-contains-newlines = Описание сервера не может содержать переносы строк
err-server-description-invalid-characters = Описание сервера содержит недопустимые символы

err-no-fields-to-update = Нет полей для обновления

err-server-image-too-large = Изображение сервера слишком большое (максимум 512КБ)
err-server-image-invalid-format = Недопустимый формат изображения сервера (должен быть data URI с кодировкой base64)
err-server-image-unsupported-type = Неподдерживаемый тип изображения сервера (только PNG, WebP, JPEG или SVG)

# Ошибки новостей
err-news-not-found = Новость #{ $id } не найдена
err-news-body-too-long = Текст новости слишком длинный (максимум { $max_length } символов)
err-news-body-invalid-characters = Текст новости содержит недопустимые символы
err-news-image-too-large = Изображение новости слишком большое (максимум 512КБ)
err-news-image-invalid-format = Недопустимый формат изображения новости (должен быть data URI с кодировкой base64)
err-news-image-unsupported-type = Неподдерживаемый тип изображения новости (только PNG, WebP, JPEG или SVG)
err-news-empty-content = Новость должна содержать текст или изображение
err-cannot-edit-admin-news = Только администраторы могут редактировать новости, опубликованные администраторами
err-cannot-delete-admin-news = Только администраторы могут удалять новости, опубликованные администраторами

# Ошибки файловой области
err-file-path-too-long = Путь к файлу слишком длинный (максимум { $max_length } символов)
err-file-path-invalid = Путь к файлу содержит недопустимые символы
err-file-not-found = Файл или каталог не найден
err-file-not-directory = Путь не является каталогом
err-dir-name-empty = Имя каталога не может быть пустым
err-dir-name-too-long = Имя каталога слишком длинное (максимум { $max_length } символов)
err-dir-name-invalid = Имя каталога содержит недопустимые символы
err-dir-already-exists = Файл или каталог с таким именем уже существует
err-dir-create-failed = Не удалось создать каталог

err-dir-not-empty = Каталог не пуст
err-delete-failed = Не удалось удалить файл или каталог
err-rename-failed = Не удалось переименовать файл или каталог
err-rename-target-exists = Файл или каталог с таким именем уже существует
err-move-failed = Не удалось переместить файл или каталог
err-copy-failed = Не удалось скопировать файл или каталог
err-destination-exists = Файл или каталог с таким именем уже существует в месте назначения
err-cannot-move-into-itself = Невозможно переместить каталог внутрь самого себя
err-cannot-copy-into-itself = Невозможно скопировать каталог внутрь самого себя
err-destination-not-directory = Путь назначения не является каталогом

# Transfer Errors
err-file-area-not-configured = Файловая область не настроена
err-file-area-not-accessible = Файловая область недоступна
err-transfer-path-too-long = Путь слишком длинный
err-transfer-path-invalid = Путь содержит недопустимые символы
err-transfer-access-denied = Доступ запрещён
err-transfer-read-failed = Не удалось прочитать файлы
err-transfer-path-not-found = Файл или каталог не найден
err-transfer-file-failed = Не удалось передать { $path }: { $error }
