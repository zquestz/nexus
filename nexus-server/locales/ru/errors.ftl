# Ошибки аутентификации и сеанса
err-not-logged-in = Не выполнен вход

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
err-max-connections-per-ip-invalid = Максимальное количество подключений на IP должно быть больше 0
err-no-fields-to-update = Нет полей для обновления

err-server-image-too-large = Изображение сервера слишком большое (максимум 512КБ)
err-server-image-invalid-format = Недопустимый формат изображения сервера (должен быть data URI с кодировкой base64)
err-server-image-unsupported-type = Неподдерживаемый тип изображения сервера (только PNG, WebP, JPEG или SVG)
